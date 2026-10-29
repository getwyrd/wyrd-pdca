//! Issue #657 (slice 1 of 2, #926) — `UploadPart`'s **staging half**,
//! `wyrd_core::multipart::stage::{stage_part, reserve_slot}`, driven over a real store
//! (`RedbMetadataStore::in_memory()`), a placement-aware chunk-store double (the shape of `Fleet`
//! in `domain_placement.rs`) and a store wrapper that logs every batch and can act on any of
//! them (proposal 0016 decision 5, `0016:1676-1847`; the *Part slot reserve* and *Part intent*
//! rows, `0016:658-659`). Every session here is made by Create (#894).
//!
//! * **(A)** staging, and the order that makes it safe; the hand-off's chunk references.
//! * **(A′)** every fragment write's deadline is `t_auth + W`, however late it is dispatched, and
//!   the wait for a write still pending at that deadline is abandoned there.
//! * **(B)** the drain fence: a drain recorded just before an intent commits re-plans (B1); a
//!   drain recorded before staging is planned around (B2); four of twelve domains draining (B3).
//! * **(C)** slots: the cap, a non-`Open` session, an adopted ambiguous reserve, the same-index
//!   rule, a held reserve that lands after its attempt refused, and one that lands just before
//!   the session moves.
//! * **(D)** renewal: while the body is still arriving, while a body read or a fragment write is
//!   pending, refused on a lapsed lease — by every intent and renewal round, at its own fresh
//!   reading — and refused once the session left `Open`.
//! * **(E)** unknown outcomes on the intent and the renewal, settled by a re-read, including a
//!   held intent that lands between two of the reads.
//! * **(F)** every loop ends after exactly its stated number of commits.
//! * **(G)** the per-part chunk bound.
//!
//! The wrappers keep their state in struct fields, never in a `static` (ADR-0035).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::io::Write as _;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{self, Stream, StreamExt as _};
use sha2::{Digest as _, Sha256};
use tokio::sync::Semaphore;
use wyrd_core::metadata::{self, ChunkRef, EcScheme, InodeId, SegmentNonce};
use wyrd_core::multipart::abort::abort_session;
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::stage::{
    reserve_slot, stage_part, RenewOutcome, StageClock, StageError, StageOutcome, StageRequest,
    StageRuntime, StagedAttempt, Stop, INTENT_ROUNDS, RENEWAL_ROUNDS, RESERVE_AMBIGUOUS_ROUNDS,
};
use wyrd_core::multipart::{
    decode_owned_entry, decode_session_record, decode_slot_record, mpu_key, parse_sidx_key,
    parse_slot_key, sidx_key, sidx_range, slot_key, slot_range, AbortOutcome, AttemptId,
    Backpressure, Budget, CreateOutcome, Digest, OwnedEntry, PartNumber, Refusal, ReserveOutcome,
    SessionState, SlotIndex, SlotRecord, UploadId, MAX_INFLIGHT_PARTS, PART_PREFIX, PSUM_PREFIX,
    SIDX_PREFIX, SLOT_PREFIX,
};
use wyrd_core::placement::{select_distinct_domains, SelectorError, Topology};
use wyrd_core::read::read_chunk_verified;
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    classify, BoxError, ChunkId, ChunkStore, CommitOutcome, CommitUnknownResult, DServerId,
    ErrorClass, FragmentId, Health, MetadataStore, PlacementChunkStore, Result as StoreResult,
    ScanPage, WriteBatch,
};

// ─────────────────────────────── fixtures ───────────────────────────────

const BUCKET: &str = "photos";
/// The root inode every Alpha object dirent lives under, as `<bucket>/<key>` (ADR-0046).
const ROOT: InodeId = 0;
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
/// No leg may hang the suite: a leg runs in milliseconds, so one still running after this has
/// hung (the repo's 30 s watchdog, `chunkstore-grpc/tests/write_deadline.rs`).
const LEG_TIMEOUT: Duration = Duration::from_secs(30);
/// How long past [`LEG_TIMEOUT`] the watchdog thread waits for a leg that never yields, which no
/// async timeout can interrupt, before it ends the test process.
const WATCHDOG_GRACE: Duration = Duration::from_secs(5);
/// How long a driver waits for the staging future to reach a point it is waiting for.
const WAIT_FOR: Duration = Duration::from_secs(10);
/// More commits than any staging may make. A loop past it never stops committing.
const RUNAWAY_COMMITS: usize = 400;
/// The test clock's first reading.
const T0: u64 = 1_000_000;
/// The lease TTL every request here asks for; renewal falls due half of it after the last one.
const TTL: u64 = 10_000;
const HALF: u64 = TTL / 2;
/// `W_write`.
const W: u64 = 3_000;
/// Bytes per chunk.
const CHUNK: usize = 64;
/// Six fragments per chunk.
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };
const FRAGMENTS: u16 = 6;
/// Twelve D servers, each its own failure domain.
const SERVERS: u64 = 12;
const PART: u32 = 7;

/// Run `leg` under [`LEG_TIMEOUT`]. A leg stuck in a loop that never yields cannot be timed out
/// from its own task, so a watchdog thread ends the whole test process (exit 101) if the leg has
/// neither finished nor panicked [`WATCHDOG_GRACE`] after the timeout.
async fn within<T>(leg: impl Future<Output = T>) -> T {
    let (disarm, armed) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        // A finished or panicking leg drops `disarm`, which ends the wait early.
        if armed.recv_timeout(LEG_TIMEOUT + WATCHDOG_GRACE) == Err(RecvTimeoutError::Timeout) {
            // Straight to stderr: the test harness captures `eprintln!` from this thread, and
            // the process ends before that capture is printed.
            let _ = writeln!(
                std::io::stderr(),
                "a leg ran past {LEG_TIMEOUT:?} without yielding; ending the test process"
            );
            std::process::exit(101);
        }
    });
    let output = tokio::time::timeout(LEG_TIMEOUT, leg)
        .await
        .expect("the leg settled in time");
    drop(disarm);
    output
}

/// Wait until `ready` holds, letting the staging future run in between; panics after
/// [`WAIT_FOR`]. A schedule whose renewal never comes fails here, not at the leg timeout.
async fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let start = tokio::time::Instant::now();
    while !ready() {
        assert!(start.elapsed() < WAIT_FOR, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// A profile admitting 8 sessions. Create stores it; staging never reads it.
fn profile() -> Budget {
    let json = r#"{"w_ref":16,"max_part_chunks":1,"max_parts_per_session":1,"max_inflight_parts":1,"max_staged_chunks":1}"#;
    metadata::decode(json.as_bytes()).expect("a legal profile")
}

fn identity(n: u128) -> (UploadId, SegmentNonce) {
    (
        UploadId::new(format!("{n:032x}")).unwrap(),
        SegmentNonce::new(format!("{:032x}", n ^ 0x5a5a_5a5a)).unwrap(),
    )
}

fn attempt(n: u128) -> AttemptId {
    AttemptId::new(format!("{:032x}", 0xa77e_0000_u128 + n)).unwrap()
}

fn part() -> PartNumber {
    PartNumber::new(PART).unwrap()
}

/// The runtime Create needs: one fixed identity, no jitter, no wait.
struct Fixed(Option<(UploadId, SegmentNonce)>);

impl CreateRuntime for Fixed {
    fn mint(&mut self) -> (UploadId, SegmentNonce) {
        self.0.take().expect("one identity per create")
    }

    fn jitter(&mut self) -> u64 {
        0
    }

    fn wait(&mut self, _millis: u64) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }
}

/// Twelve servers `0..12` in domains `d00..d11`, equally loaded: the selector takes domains in
/// label order, so an RS(4,2) chunk lands on servers `0..6`.
fn topology() -> Topology {
    let mut topology = Topology::default();
    for id in 0..SERVERS {
        topology.register(id, format!("d{id:02}"));
    }
    topology
}

/// [`topology`], with every server loaded but `preferred`: the selector's first pick.
fn topology_preferring(preferred: DServerId) -> Topology {
    let mut topology = topology();
    for id in 0..SERVERS {
        topology.set_utilization(id, if id == preferred { 0 } else { 10 });
    }
    topology
}

/// The request every leg starts from.
fn request<'a>(
    topology: &'a Topology,
    upload: &'a UploadId,
    attempt: &'a AttemptId,
) -> StageRequest<'a> {
    StageRequest {
        upload_id: upload,
        part_number: part(),
        attempt_id: attempt,
        max_inflight_parts: MAX_INFLIGHT_PARTS,
        lease_ttl_millis: TTL,
        chunk_size: CHUNK,
        scheme: RS42,
        topology,
        write_window_millis: W,
        max_part_chunks: 1_000,
    }
}

/// `len` deterministic body bytes.
fn body_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 + 7) as u8).collect()
}

/// `body` cut into pieces of `sizes`, which must cover it exactly.
fn pieces(body: &[u8], sizes: &[usize]) -> Vec<Bytes> {
    assert_eq!(
        sizes.iter().sum::<usize>(),
        body.len(),
        "the pieces cover the body"
    );
    let mut at = 0;
    sizes
        .iter()
        .map(|size| {
            let piece = Bytes::copy_from_slice(&body[at..at + size]);
            at += size;
            piece
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> Digest {
    Digest::from_bytes(Sha256::digest(bytes).into())
}

type Body = Pin<Box<dyn Stream<Item = StoreResult<Bytes>> + Send>>;

fn stream_of(pieces: Vec<Bytes>) -> Body {
    Box::pin(stream::iter(pieces.into_iter().map(Ok)))
}

/// A body the test feeds piece by piece; it ends when the sender is dropped.
fn channel() -> (tokio::sync::mpsc::UnboundedSender<Bytes>, Body) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
    let body = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|piece| (Ok(piece), rx))
    });
    (tx, Box::pin(body))
}

/// A body whose `i`-th piece advances the clock by its `u64` the moment it is read, and is
/// logged as read ([`Event::Read`]).
fn ticking(clock: &ManualClock, log: &Log, pieces: Vec<(u64, Bytes)>) -> Body {
    let (clock, log) = (clock.clone(), log.clone());
    Box::pin(
        stream::iter(pieces.into_iter().enumerate()).map(move |(index, (advance, piece))| {
            clock.advance(advance);
            log.push(Event::Read { index });
            Ok(piece)
        }),
    )
}

fn unknown(may_still_commit: bool) -> BoxError {
    Box::new(CommitUnknownResult {
        backend: "scripted",
        code: None,
        detail: "scripted unknown outcome".into(),
        may_still_commit,
    })
}

// ─────────────────────────────── the log ───────────────────────────────

/// Which batch of staging a commit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    /// `require_absent(slot:..)`.
    Reserve,
    /// `require_absent(sidx:..)`.
    Intent,
    /// `require(slot:.. == prior)` and no new owned entry.
    Renewal,
    Other,
}

fn kind_of(batch: &WriteBatch) -> Kind {
    let absent = |prefix: &[u8]| {
        batch
            .preconditions
            .iter()
            .any(|p| p.expected.is_none() && p.key.starts_with(prefix))
    };
    let present = |prefix: &[u8]| {
        batch
            .preconditions
            .iter()
            .any(|p| p.expected.is_some() && p.key.starts_with(prefix))
    };
    if absent(SIDX_PREFIX) {
        Kind::Intent
    } else if absent(SLOT_PREFIX) {
        Kind::Reserve
    } else if present(SLOT_PREFIX) {
        Kind::Renewal
    } else {
        Kind::Other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Committed,
    Conflict,
    Unknown { may_still_commit: bool },
    Failed,
}

fn answer_of(result: &StoreResult<CommitOutcome>) -> Answer {
    match result {
        Ok(CommitOutcome::Committed) => Answer::Committed,
        Ok(CommitOutcome::Conflict) => Answer::Conflict,
        Err(fault) => match fault.downcast_ref::<CommitUnknownResult>() {
            Some(unknown) => Answer::Unknown {
                may_still_commit: unknown.may_still_commit,
            },
            None => Answer::Failed,
        },
    }
}

#[derive(Debug, Clone)]
enum Event {
    /// A batch through the wrapper: what it was, the clock when it arrived, and the answer.
    Commit {
        kind: Kind,
        batch: WriteBatch,
        clock: u64,
        answer: Answer,
    },
    /// A ticking body yielded its `index`-th piece (0-based).
    Read { index: usize },
    /// A fragment put the chunk store was asked for, logged on arrival, before any hold.
    Put {
        dserver: DServerId,
        id: FragmentId,
        deadline: Option<u64>,
        clock: u64,
    },
}

/// One ordered log shared by the store wrapper and the chunk-store double.
#[derive(Debug, Clone, Default)]
struct Log(Arc<Mutex<Vec<Event>>>);

struct Commit {
    batch: WriteBatch,
    clock: u64,
    answer: Answer,
}

struct Put {
    dserver: DServerId,
    id: FragmentId,
    deadline: Option<u64>,
    clock: u64,
}

impl Log {
    fn push(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }

    fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }

    fn commits(&self, of: Kind) -> Vec<Commit> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Commit {
                    kind,
                    batch,
                    clock,
                    answer,
                } if kind == of => Some(Commit {
                    batch,
                    clock,
                    answer,
                }),
                _ => None,
            })
            .collect()
    }

    fn landed(&self, of: Kind) -> usize {
        self.commits(of)
            .iter()
            .filter(|commit| commit.answer == Answer::Committed)
            .count()
    }

    fn puts(&self) -> Vec<Put> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Put {
                    dserver,
                    id,
                    deadline,
                    clock,
                } => Some(Put {
                    dserver,
                    id,
                    deadline,
                    clock,
                }),
                Event::Commit { .. } | Event::Read { .. } => None,
            })
            .collect()
    }

    fn put_count(&self, id: FragmentId) -> usize {
        self.puts().iter().filter(|put| put.id == id).count()
    }

    /// The intent commits for `chunk`.
    fn intents_of(&self, chunk: ChunkId) -> Vec<Commit> {
        self.commits(Kind::Intent)
            .into_iter()
            .filter(|commit| intent_chunk(&commit.batch) == chunk)
            .collect()
    }
}

/// The owned entry an intent batch puts: its key and value.
fn intent_put(batch: &WriteBatch) -> (Vec<u8>, Bytes) {
    batch
        .puts
        .iter()
        .find(|(key, _)| key.starts_with(SIDX_PREFIX))
        .cloned()
        .expect("an intent puts its owned entry")
}

fn intent_chunk(batch: &WriteBatch) -> ChunkId {
    parse_sidx_key(&intent_put(batch).0).unwrap().2
}

fn intent_entry(batch: &WriteBatch) -> OwnedEntry {
    let (key, value) = intent_put(batch);
    decode_owned_entry(&key, &value).unwrap().2
}

/// The slot value a batch puts.
fn slot_put(batch: &WriteBatch) -> SlotRecord {
    let (_, value) = batch
        .puts
        .iter()
        .find(|(key, _)| key.starts_with(SLOT_PREFIX))
        .expect("the batch puts the slot");
    decode_slot_record(value).unwrap()
}

/// The slot value a batch requires.
fn slot_required(batch: &WriteBatch) -> Bytes {
    batch
        .preconditions
        .iter()
        .find(|p| p.key.starts_with(SLOT_PREFIX))
        .and_then(|p| p.expected.clone())
        .expect("the batch requires its slot's bytes")
}

/// Every value a batch puts under `slot:` or `sidx:` is one its decoder accepts back unchanged.
fn assert_values_decode(batch: &WriteBatch) {
    for (key, value) in &batch.puts {
        if key.starts_with(SLOT_PREFIX) {
            let record =
                decode_slot_record(value).expect("every slot value staging writes decodes");
            assert_eq!(
                metadata::encode(&record),
                *value,
                "a slot value re-encodes byte for byte"
            );
        } else if key.starts_with(SIDX_PREFIX) {
            decode_owned_entry(key, value).expect("every owned entry decodes against its key");
        }
    }
}

// ──────────────────────── the scripted store wrapper ────────────────────────

/// A batch to commit on the inner store, built from the batch under test.
type Hook = Arc<dyn Fn(&WriteBatch) -> WriteBatch + Send + Sync>;

/// What the wrapper does with one commit of a [`Kind`].
#[derive(Clone)]
enum Act {
    /// Delegate unchanged.
    Pass,
    /// Answer `Conflict` without applying anything, as TiKV does when a locking read meets
    /// another writer's lock.
    Conflict,
    /// As `Conflict`, and advance the clock by the given milliseconds before answering: a round
    /// lost slowly.
    ConflictThenAdvance(u64),
    /// Apply the batch or not, then answer `CommitUnknownResult { may_still_commit }`.
    Unknown {
        applied: bool,
        may_still_commit: bool,
    },
    /// Keep the batch unapplied, advance the clock by `advance`, and answer
    /// `CommitUnknownResult { may_still_commit: true }`. It lands when the test releases it.
    Hold { advance: u64 },
    /// As `Hold { advance: 0 }`, after committing `hook(batch)` on the inner store.
    HoldThen(Hook),
    /// Commit `hook(batch)` on the inner store first, then delegate.
    First(Hook),
    /// Delegate, then advance the clock: a dispatch delayed after the commit returned.
    PassThenAdvance(u64),
    /// Apply the oldest held batch, then delegate.
    ReleaseHeldFirst,
}

/// What the wrapper does right after a read returns, before handing its answer back: the
/// interleaving "between this read and the next one".
#[derive(Clone)]
enum Then {
    /// Advance the clock.
    Advance(u64),
    /// Apply the oldest held batch, which must land.
    ReleaseHeld,
    /// Commit this batch on the inner store, which must land.
    Commit(WriteBatch),
}

/// Fire `then` after the read of `key` (a `get`'s key, or a `scan`'s prefix) that follows `skip`
/// earlier reads of it.
struct ReadHook {
    key: Vec<u8>,
    skip: usize,
    then: Vec<Then>,
}

#[derive(Default)]
struct ScriptState {
    plans: HashMap<Kind, (VecDeque<Act>, Option<Act>)>,
    held: VecDeque<WriteBatch>,
    made: usize,
    reads: Vec<ReadHook>,
}

struct Scripted {
    inner: RedbMetadataStore,
    clock: ManualClock,
    log: Log,
    state: Mutex<ScriptState>,
}

impl Scripted {
    /// `kind`'s commits follow `plan`, then `then` (`Pass` when unset).
    fn script(&self, kind: Kind, plan: Vec<Act>, then: Option<Act>) {
        self.state
            .lock()
            .unwrap()
            .plans
            .insert(kind, (plan.into(), then));
    }

    /// Apply the oldest held batch now; what it met.
    async fn release_held(&self) -> CommitOutcome {
        let held = self
            .state
            .lock()
            .unwrap()
            .held
            .pop_front()
            .expect("a batch is held");
        self.inner.commit(held).await.unwrap()
    }

    fn hold(&self, batch: WriteBatch) {
        self.state.lock().unwrap().held.push_back(batch);
    }

    /// After the read of `key` that follows `skip` earlier ones, do `then`.
    fn after_read(&self, key: &[u8], skip: usize, then: Vec<Then>) {
        self.state.lock().unwrap().reads.push(ReadHook {
            key: key.to_vec(),
            skip,
            then,
        });
    }

    /// A read of `key` has returned: fire the hook it is due for, if any.
    async fn on_read(&self, key: &[u8]) {
        let due = {
            let mut state = self.state.lock().unwrap();
            match state.reads.iter().position(|hook| hook.key == key) {
                Some(at) if state.reads[at].skip > 0 => {
                    state.reads[at].skip -= 1;
                    Vec::new()
                }
                Some(at) => state.reads.remove(at).then,
                None => Vec::new(),
            }
        };
        for then in due {
            match then {
                Then::Advance(millis) => self.clock.advance(millis),
                Then::ReleaseHeld => {
                    assert_eq!(self.release_held().await, CommitOutcome::Committed);
                }
                Then::Commit(batch) => {
                    assert_eq!(
                        self.inner.commit(batch).await.unwrap(),
                        CommitOutcome::Committed
                    );
                }
            }
        }
    }
}

#[async_trait]
impl MetadataStore for Scripted {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        let found = self.inner.get(key).await;
        self.on_read(key).await;
        found
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        let found = self.inner.scan(prefix).await;
        self.on_read(prefix).await;
        found
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> StoreResult<ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        let kind = kind_of(&batch);
        assert_values_decode(&batch);
        let clock = self.clock.now_millis();
        let act = {
            let mut state = self.state.lock().unwrap();
            state.made += 1;
            assert!(
                state.made <= RUNAWAY_COMMITS,
                "staging never stopped committing ({} commits)",
                state.made
            );
            match state.plans.get_mut(&kind) {
                Some((plan, then)) => plan.pop_front().or_else(|| then.clone()),
                None => None,
            }
            .unwrap_or(Act::Pass)
        };
        let result = match act {
            Act::Pass => self.inner.commit(batch.clone()).await,
            Act::Conflict => {
                // A real store yields here; so must this one, or no timeout could fire.
                tokio::task::yield_now().await;
                Ok(CommitOutcome::Conflict)
            }
            Act::ConflictThenAdvance(advance) => {
                tokio::task::yield_now().await;
                self.clock.advance(advance);
                Ok(CommitOutcome::Conflict)
            }
            Act::Unknown {
                applied,
                may_still_commit,
            } => {
                if applied {
                    let landed = self.inner.commit(batch.clone()).await?;
                    assert_eq!(landed, CommitOutcome::Committed, "the scripted batch lands");
                }
                Err(unknown(may_still_commit))
            }
            Act::Hold { advance } => {
                self.hold(batch.clone());
                self.clock.advance(advance);
                Err(unknown(true))
            }
            Act::HoldThen(hook) => {
                self.hold(batch.clone());
                let first = hook(&batch);
                assert_eq!(self.inner.commit(first).await?, CommitOutcome::Committed);
                Err(unknown(true))
            }
            Act::First(hook) => {
                let first = hook(&batch);
                assert_eq!(self.inner.commit(first).await?, CommitOutcome::Committed);
                self.inner.commit(batch.clone()).await
            }
            Act::PassThenAdvance(advance) => {
                let result = self.inner.commit(batch.clone()).await;
                self.clock.advance(advance);
                result
            }
            Act::ReleaseHeldFirst => {
                assert_eq!(self.release_held().await, CommitOutcome::Committed);
                self.inner.commit(batch.clone()).await
            }
        };
        self.log.push(Event::Commit {
            kind,
            batch,
            clock,
            answer: answer_of(&result),
        });
        result
    }
}

// ──────────────────────── the chunk-store double ────────────────────────

/// In-process D servers addressed by stable id (`domain_placement.rs`'s `Fleet`), logging every
/// put with its deadline, and able to hold one put until the test releases it.
struct Fleet {
    servers: BTreeMap<DServerId, Mutex<HashMap<FragmentId, Bytes>>>,
    clock: ManualClock,
    log: Log,
    hold: Mutex<Option<(FragmentId, Arc<Semaphore>)>>,
}

impl Fleet {
    /// Hold the put of `id` until a permit is added to the returned gate.
    fn hold(&self, id: FragmentId) -> Arc<Semaphore> {
        let gate = Arc::new(Semaphore::new(0));
        *self.hold.lock().unwrap() = Some((id, Arc::clone(&gate)));
        gate
    }
}

#[async_trait]
impl ChunkStore for Fleet {
    async fn put_fragment(
        &self,
        _id: FragmentId,
        _fragment: Bytes,
        _deadline_millis: Option<u64>,
    ) -> StoreResult<()> {
        Err("Fleet: a staged write must address a D server (use put_fragment_at)".into())
    }

    async fn get_fragment(&self, _id: FragmentId) -> StoreResult<Option<Bytes>> {
        Ok(None)
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        Ok(self
            .servers
            .values()
            .flat_map(|server| server.lock().unwrap().keys().copied().collect::<Vec<_>>())
            .collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> StoreResult<()> {
        for server in self.servers.values() {
            server.lock().unwrap().remove(&id);
        }
        Ok(())
    }

    async fn health(&self) -> StoreResult<Health> {
        Ok(Health::Healthy)
    }
}

#[async_trait]
impl PlacementChunkStore for Fleet {
    async fn get_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
    ) -> StoreResult<Option<Bytes>> {
        Ok(self
            .servers
            .get(&dserver)
            .and_then(|server| server.lock().unwrap().get(&id).cloned()))
    }

    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        fragment: Bytes,
        deadline_millis: Option<u64>,
    ) -> StoreResult<()> {
        self.log.push(Event::Put {
            dserver,
            id,
            deadline: deadline_millis,
            clock: self.clock.now_millis(),
        });
        let gate = self
            .hold
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(held, _)| *held == id)
            .map(|(_, gate)| Arc::clone(gate));
        if let Some(gate) = gate {
            gate.acquire().await.expect("the gate stays open").forget();
        }
        self.servers
            .get(&dserver)
            .ok_or_else(|| format!("Fleet: unknown D server {dserver}"))?
            .lock()
            .unwrap()
            .insert(id, fragment);
        Ok(())
    }
}

// ─────────────────────────────── the runtime ───────────────────────────────

/// The staging runtime: the manual clock, chunk ids `1, 2, 3, …`, and a renewal timer the test
/// fires by hand ([`Runtime::timer`]'s permits).
struct Runtime {
    clock: ManualClock,
    next_chunk: ChunkId,
    timer: Arc<Semaphore>,
}

impl StageClock for Runtime {
    fn now_millis(&self) -> u64 {
        self.clock.now_millis()
    }
}

impl StageRuntime for Runtime {
    fn next_chunk_id(&mut self) -> ChunkId {
        self.next_chunk += 1;
        self.next_chunk
    }

    fn wait(&mut self, _millis: u64) -> impl Future<Output = ()> + Send {
        let timer = Arc::clone(&self.timer);
        async move {
            timer
                .acquire()
                .await
                .expect("the timer stays open")
                .forget();
        }
    }
}

// ─────────────────────────────── the rig ───────────────────────────────

struct Rig {
    clock: ManualClock,
    log: Log,
    store: Scripted,
    fleet: Fleet,
    runtime: Runtime,
    topology: Topology,
    upload: UploadId,
    /// The session bytes Create stored: `Open@0`.
    session: Bytes,
    attempt: AttemptId,
}

impl Rig {
    async fn new() -> Self {
        let clock = ManualClock::new(T0);
        let log = Log::default();
        let inner = RedbMetadataStore::in_memory().unwrap();
        inner
            .commit(WriteBatch::new().put(metadata::bucket_key(BUCKET), MARKER))
            .await
            .unwrap();
        let object = format!("{BUCKET}/object");
        let create = CreateRequest {
            bucket: BUCKET,
            parent: ROOT,
            object: &object,
            content_type: None,
            created_at_millis: T0,
            clock_source: "wall",
        };
        let outcome = create_session(&inner, &profile(), &create, &mut Fixed(Some(identity(1))))
            .await
            .expect("the create ran");
        let CreateOutcome::Created { upload_id } = outcome else {
            panic!("expected Created, got {outcome:?}");
        };
        let session = inner.get(&mpu_key(&upload_id)).await.unwrap().unwrap();
        Self {
            store: Scripted {
                inner,
                clock: clock.clone(),
                log: log.clone(),
                state: Mutex::new(ScriptState::default()),
            },
            fleet: Fleet {
                servers: (0..SERVERS)
                    .map(|id| (id, Mutex::new(HashMap::new())))
                    .collect(),
                clock: clock.clone(),
                log: log.clone(),
                hold: Mutex::new(None),
            },
            runtime: Runtime {
                clock: clock.clone(),
                next_chunk: 0,
                timer: Arc::new(Semaphore::new(0)),
            },
            topology: topology(),
            upload: upload_id,
            session,
            attempt: attempt(0),
            clock,
            log,
        }
    }

    /// Stage `body` under `request`, with nothing running beside it.
    async fn stage(
        &mut self,
        request: impl FnOnce(&mut StageRequest<'_>),
        body: Body,
    ) -> Result<StageOutcome, StageError> {
        let mut req = self::request(&self.topology, &self.upload, &self.attempt);
        request(&mut req);
        stage_part(&self.store, &self.fleet, &mut self.runtime, &req, body).await
    }
}

fn staged(outcome: Result<StageOutcome, StageError>) -> Box<StagedAttempt> {
    match outcome {
        Ok(StageOutcome::Staged(attempt)) => attempt,
        other => panic!("expected Staged, got {other:?}"),
    }
}

/// The cause of a staging that stopped with nothing in doubt.
fn stopped(outcome: Result<StageOutcome, StageError>) -> Stop {
    match outcome {
        Err(StageError::Stopped { cause, .. }) => cause,
        other => panic!("expected Stopped, got {other:?}"),
    }
}

/// Every slot of `upload`, by index.
async fn slots(store: &impl MetadataStore, upload: &UploadId) -> Vec<(SlotIndex, SlotRecord)> {
    let mut found: Vec<_> = store
        .scan(&slot_range(upload))
        .await
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            let (owner, index) = parse_slot_key(&key).unwrap();
            assert_eq!(&owner, upload);
            (index, decode_slot_record(&value).unwrap())
        })
        .collect();
    found.sort_by_key(|(index, _)| *index);
    found
}

/// The slots `attempt` holds.
async fn slots_of(store: &impl MetadataStore, upload: &UploadId, attempt: &AttemptId) -> usize {
    slots(store, upload)
        .await
        .iter()
        .filter(|(_, slot)| slot.attempt_id() == attempt)
        .count()
}

/// Every owned entry of `upload`, by chunk id.
async fn owned(store: &impl MetadataStore, upload: &UploadId) -> BTreeMap<ChunkId, OwnedEntry> {
    store
        .scan(&sidx_range(upload))
        .await
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            let (part_number, chunk, entry) = decode_owned_entry(&key, &value).unwrap();
            assert_eq!(part_number, part());
            (chunk, entry)
        })
        .collect()
}

async fn count(store: &impl MetadataStore, prefix: &[u8]) -> usize {
    store.scan(prefix).await.unwrap().len()
}

async fn snapshot(store: &impl MetadataStore) -> Vec<(Vec<u8>, Bytes)> {
    let mut all = store.scan(b"").await.unwrap();
    all.sort();
    all
}

/// **(A)** Every chunk's intent batches all precede the first put of that chunk's fragments
/// (`0016:1788`: assert the order, not just the end state).
fn assert_intents_precede_puts(log: &Log) {
    let events = log.events();
    let mut chunks = BTreeSet::new();
    for event in &events {
        if let Event::Put { id, .. } = event {
            chunks.insert(id.chunk);
        }
    }
    for chunk in chunks {
        let first_put = events
            .iter()
            .position(|e| matches!(e, Event::Put { id, .. } if id.chunk == chunk))
            .unwrap();
        let intents: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                matches!(e, Event::Commit { kind: Kind::Intent, batch, .. }
                    if intent_chunk(batch) == chunk)
            })
            .map(|(at, _)| at)
            .collect();
        assert!(!intents.is_empty(), "chunk {chunk} has an intent");
        assert!(
            intents.iter().all(|at| *at < first_put),
            "chunk {chunk}: every intent commit precedes its first fragment put"
        );
    }
}

/// Every fragment the fleet was asked to write is named, now, by a durable owned entry whose
/// placement puts that fragment on the server it was written to.
async fn assert_fragments_evidenced(rig: &Rig) {
    let entries = owned(&rig.store.inner, &rig.upload).await;
    for put in rig.log.puts() {
        let entry = entries
            .get(&put.id.chunk)
            .unwrap_or_else(|| panic!("fragment {:?} has no durable owned entry", put.id));
        assert_eq!(
            entry.staged().placement()[usize::from(put.id.index)],
            put.dserver,
            "fragment {:?} was written where its entry says",
            put.id
        );
    }
}

/// Nothing staged names `server`: no owned entry, no fragment put, no hand-off chunk.
async fn assert_nothing_names(rig: &Rig, attempt: &StagedAttempt, server: DServerId) {
    for (chunk, entry) in owned(&rig.store.inner, &rig.upload).await {
        assert!(
            !entry.staged().placement().contains(&server),
            "chunk {chunk}'s entry names draining server {server}"
        );
    }
    assert!(
        rig.log.puts().iter().all(|put| put.dserver != server),
        "no fragment is written to draining server {server}"
    );
    assert!(
        attempt
            .chunks()
            .iter()
            .all(|c| !c.placement.contains(&server)),
        "no hand-off chunk names draining server {server}"
    );
}

/// No lease a batch of staging required was lapsed when that batch arrived: neither the slot
/// nor any owned entry ever lapsed while the client was supplying data (`0016:1824-1837`).
fn assert_no_lapse_observed(log: &Log) {
    for event in log.events() {
        let Event::Commit {
            kind: Kind::Intent | Kind::Renewal,
            batch,
            clock,
            ..
        } = event
        else {
            continue;
        };
        for p in &batch.preconditions {
            let Some(value) = &p.expected else { continue };
            let lease = if p.key.starts_with(SLOT_PREFIX) {
                decode_slot_record(value).unwrap().lease_expiry_millis()
            } else if p.key.starts_with(SIDX_PREFIX) {
                decode_owned_entry(&p.key, value)
                    .unwrap()
                    .2
                    .lease_expiry_millis()
            } else {
                continue;
            };
            assert!(
                lease > clock,
                "a lease required at {clock} had lapsed at {lease}"
            );
        }
    }
}

/// `open`'s session at `Open@epoch`, in the codec's canonical spelling.
fn open_at(open: &Bytes, epoch: u64) -> Bytes {
    let record = decode_session_record(open).unwrap();
    let json = |text: &str| String::from_utf8(metadata::encode(&text).to_vec()).unwrap();
    let bytes = format!(
        "{{\"parent\":{},\"object\":{},\"created_at_millis\":{},\"clock_source\":{},\
         \"segment_nonce\":\"{}\",\"epoch\":{epoch},\"attempts\":0,\"state\":{{\"kind\":\"Open\"}}}}",
        record.parent(),
        json(record.object()),
        record.created_at_millis(),
        json(record.clock_source()),
        record.segment_nonce(),
    );
    let decoded = decode_session_record(bytes.as_bytes()).expect("a canonical session");
    assert!(matches!(decoded.state(), SessionState::Open {}));
    assert_eq!(metadata::encode(&decoded), bytes.as_bytes());
    Bytes::from(bytes)
}

/// A hook that moves `upload`'s session to `Open@epoch` (a Complete fence and its release).
fn move_session(upload: &UploadId, to: Bytes) -> Hook {
    let key = mpu_key(upload);
    Arc::new(move |_| WriteBatch::new().put(key.clone(), to.clone()))
}

/// A canonical slot value for attempt `n` of part `part_number`, as residue or a live part would
/// leave it.
fn other_slot(n: u128, part_number: u32) -> Bytes {
    let bytes = format!(
        "{{\"part_number\":{part_number},\"attempt_id\":\"{}\",\"reserved_at_millis\":{T0},\
         \"lease_expiry_millis\":{}}}",
        attempt(n),
        T0 + TTL
    );
    let decoded = decode_slot_record(bytes.as_bytes()).expect("a canonical slot");
    assert_eq!(metadata::encode(&decoded), bytes.as_bytes());
    Bytes::from(bytes)
}

// ═══════════════════════ (A) staging and its order ═══════════════════════

/// **(A)** A body of three full chunks and a short fourth, fed as uneven pieces (10 bytes, then
/// 100 spanning the first boundary, then 37, then 65 spanning the third; no boundary aligned to
/// the chunk size), stages exactly four chunks. Read back through the fragments they equal the
/// body; the length and digest are the body's. Mid-stage the store holds an owned entry and no
/// `pending:` entry. After: one slot (this attempt's), one owned entry per chunk at the
/// placement its fragments went to, nothing under `part:` or `psum:`; every intent precedes its
/// chunk's first fragment put; the hand-off carries one complete `ChunkRef` per chunk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs() {
    within(async {
        let mut rig = Rig::new().await;
        let body = body_bytes(3 * CHUNK + 20);
        let body_pieces = pieces(&body, &[10, 100, 37, 65]);
        let gate = rig.fleet.hold(FragmentId { chunk: 1, index: 0 });
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(
            &rig.store,
            &rig.fleet,
            &mut rig.runtime,
            &req,
            stream_of(body_pieces),
        );
        let (log, inner) = (rig.log.clone(), &rig.store.inner);
        let driver = async {
            let first = FragmentId { chunk: 1, index: 0 };
            until("chunk 1's first fragment put", || log.put_count(first) == 1).await;
            assert!(
                !inner.scan(SIDX_PREFIX).await.unwrap().is_empty(),
                "while staging, scan(\"sidx:\") is non-empty"
            );
            assert!(
                inner.scan(b"pending:").await.unwrap().is_empty(),
                "while staging, scan(\"pending:\") is empty"
            );
            gate.add_permits(1);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let attempt = staged(outcome);
        let inner = &rig.store.inner;

        assert_eq!(attempt.chunks().len(), (body.len()).div_ceil(CHUNK));
        let mut read_back = Vec::new();
        for chunk in attempt.chunks() {
            read_back
                .extend_from_slice(&read_chunk_verified(inner, &rig.fleet, chunk).await.unwrap());
        }
        assert_eq!(
            read_back, body,
            "the chunks read back through their fragments are the body"
        );
        assert_eq!(attempt.len(), body.len() as u64);
        assert_eq!(
            *attempt.digest(),
            sha256(&body),
            "the digest is the body's SHA-256"
        );

        let held = slots(inner, &rig.upload).await;
        assert_eq!(held.len(), 1, "one slot");
        assert_eq!(held[0].1.attempt_id(), &rig.attempt);
        assert_eq!(held[0].1.part_number(), part());
        let entries = owned(inner, &rig.upload).await;
        assert_eq!(
            entries.len(),
            attempt.chunks().len(),
            "one owned entry per chunk"
        );
        for (chunk, entry) in &entries {
            let written: BTreeMap<u16, DServerId> = rig
                .log
                .puts()
                .iter()
                .filter(|put| put.id.chunk == *chunk)
                .map(|put| (put.id.index, put.dserver))
                .collect();
            assert_eq!(written.len(), usize::from(FRAGMENTS));
            for (index, dserver) in written {
                assert_eq!(entry.staged().placement()[usize::from(index)], dserver);
            }
        }
        assert_eq!(count(inner, PART_PREFIX).await, 0, "nothing under part:");
        assert_eq!(count(inner, PSUM_PREFIX).await, 0, "nothing under psum:");
        assert!(inner.scan(b"pending:").await.unwrap().is_empty());
        assert_intents_precede_puts(&rig.log);
        assert_fragments_evidenced(&rig).await;

        // The hand-off: complete chunk references, in chunk order.
        let chunks = attempt.chunks();
        assert_eq!(chunks.iter().map(|c| c.len).sum::<u64>(), body.len() as u64);
        let (last, full) = chunks.split_last().unwrap();
        assert!(
            full.iter().all(|c| c.len == CHUNK as u64),
            "every chunk but the last is full"
        );
        assert_eq!(last.len, 20, "the last chunk carries the short remainder");
        assert_eq!(attempt.owned().len(), chunks.len());
        for (chunk, held) in chunks.iter().zip(attempt.owned()) {
            let (_, id, entry) = decode_owned_entry(held.key(), held.bytes()).unwrap();
            assert_eq!(&entry, held.entry());
            assert_eq!(chunk.id, id, "the ChunkRef's id is its owned entry's chunk");
            assert_eq!(chunk.scheme, entry.staged().scheme());
            assert_eq!(chunk.placement, entry.staged().placement());
            assert_eq!(
                entries[&id], entry,
                "the hand-off's entry is the stored one"
            );
            assert_eq!(
                inner.get(held.key()).await.unwrap().as_ref(),
                Some(held.bytes())
            );
        }
        let ids: Vec<ChunkId> = chunks.iter().map(|c| c.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "chunk order");
        assert_eq!(attempt.part_number(), part());
        assert_eq!(attempt.attempt_id(), &rig.attempt);
        assert_eq!(attempt.upload_id(), &rig.upload);
        assert_eq!(attempt.session_bytes(), &rig.session);
        assert_eq!(attempt.slot_index(), held[0].0);
        assert_eq!(
            attempt.slot_key(),
            slot_key(&rig.upload, held[0].0).as_slice()
        );
        assert_eq!(attempt.slot(), &held[0].1);
        assert_eq!(
            inner.get(attempt.slot_key()).await.unwrap().as_ref(),
            Some(attempt.slot_bytes())
        );
        assert!(!attempt.chunk_bound_exceeded());
        assert!(!attempt.is_empty());
    })
    .await;
}

/// **(A)** An empty body stages no chunk: the attempt holds its slot and nothing else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_an_empty_body_stages_no_chunk() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = staged(rig.stage(|_| {}, stream_of(Vec::new())).await);
        assert!(attempt.is_empty());
        assert!(attempt.chunks().is_empty());
        assert_eq!(*attempt.digest(), sha256(&[]));
        assert_eq!(slots(&rig.store.inner, &rig.upload).await.len(), 1);
        assert!(owned(&rig.store.inner, &rig.upload).await.is_empty());
    })
    .await;
}

// ═══════════════════════ (A′) the write deadline ═══════════════════════

/// **(A′)** Every staged fragment put carries `t_auth + W`, where `t_auth` is the clock reading
/// when its chunk's intent was committed. The wrapper advances the clock right after each intent
/// commit returns, so the puts are dispatched later — and the deadline is still `t_auth + W`:
/// never `t_dispatch + W`, never `None`, never unbounded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prime_every_fragment_deadline_is_t_auth_plus_w() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store
            .script(Kind::Intent, vec![], Some(Act::PassThenAdvance(777)));
        let body = body_bytes(3 * CHUNK);
        staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK])))
                .await,
        );
        let intents = rig.log.commits(Kind::Intent);
        assert_eq!(intents.len(), 3);
        let puts = rig.log.puts();
        assert_eq!(puts.len(), 3 * usize::from(FRAGMENTS));
        for intent in intents {
            assert_eq!(intent.answer, Answer::Committed);
            let chunk = intent_chunk(&intent.batch);
            let t_auth = intent.clock;
            for put in puts.iter().filter(|put| put.id.chunk == chunk) {
                assert_eq!(put.clock, t_auth + 777, "the dispatch was delayed");
                assert_eq!(put.deadline, Some(t_auth + W), "deadline = t_auth + W");
                assert_ne!(put.deadline, Some(put.clock + W), "never t_dispatch + W");
                assert_ne!(put.deadline, None);
                assert_ne!(put.deadline, Some(u64::MAX));
            }
        }
    })
    .await;
}

/// **(A′)** The wait for a chunk's fragment writes is bounded by their deadline, fail-closed
/// (`0016:1555-1557`): a write still pending once the clock has passed `t_auth + W` (with no
/// renewal due) is abandoned, never re-sent, and staging stops with a typed error naming the
/// chunk and the deadline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prime_a_write_pending_past_its_deadline_is_abandoned() {
    within(async {
        let mut rig = Rig::new().await;
        let held = FragmentId { chunk: 1, index: 0 };
        let _gate = rig.fleet.hold(held);
        let body = body_bytes(2 * CHUNK);
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(
            &rig.store,
            &rig.fleet,
            &mut rig.runtime,
            &req,
            stream_of(pieces(&body, &[2 * CHUNK])),
        );
        let driver = async move {
            until("chunk 1's held put", || log.put_count(held) == 1).await;
            // The fixture binds: no renewal is due at the deadline.
            const { assert!(W + 1 < HALF) };
            clock.advance(W);
            timer.add_permits(1);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let cause = stopped(outcome);
        assert!(
            matches!(
                cause,
                Stop::WriteDeadlineElapsed {
                    chunk: 1,
                    deadline_millis
                } if deadline_millis == T0 + W
            ),
            "{cause:?}"
        );
        assert_eq!(
            rig.log.put_count(held),
            1,
            "the abandoned write is not re-sent"
        );
        assert!(rig.log.commits(Kind::Renewal).is_empty());
        assert_eq!(
            rig.log.commits(Kind::Intent).len(),
            1,
            "no later chunk was started"
        );
    })
    .await;
}

// ═══════════════════════ (B) the drain fence ═══════════════════════

/// The first server the owned entry an intent batch puts is placed on.
fn first_server(batch: &WriteBatch) -> DServerId {
    intent_entry(batch).staged().placement()[0]
}

/// **B1.** The wrapper records `desired:dserver:<S>`, for a server `S` the plan selected, just
/// before the first intent commits. That intent fails on its own drain fence and re-plans;
/// staging completes, and no owned entry, fragment or hand-off chunk names `S`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b1_a_drain_recorded_just_before_the_intent_commits_makes_it_replan() {
    within(async {
        let mut rig = Rig::new().await;
        let drained = Arc::new(Mutex::new(None));
        let seen = Arc::clone(&drained);
        let drain: Hook = Arc::new(move |batch| {
            let server = first_server(batch);
            *seen.lock().unwrap() = Some(server);
            WriteBatch::new().put(metadata::desired_dserver_key(server), b"draining".to_vec())
        });
        rig.store
            .script(Kind::Intent, vec![Act::First(drain)], None);
        let body = body_bytes(3 * CHUNK);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK])))
                .await,
        );
        let server = drained.lock().unwrap().expect("the hook ran");

        let intents = rig.log.commits(Kind::Intent);
        assert_eq!(
            intents[0].answer,
            Answer::Conflict,
            "the drain fence failed the intent"
        );
        assert_eq!(first_server(&intents[0].batch), server);
        // The fence is one `require_absent` per server the entry names, in the custodian's bytes.
        let fenced: BTreeSet<Vec<u8>> = intents[0]
            .batch
            .preconditions
            .iter()
            .filter(|p| p.expected.is_none() && p.key.starts_with(b"desired:dserver:"))
            .map(|p| p.key.clone())
            .collect();
        let named: BTreeSet<Vec<u8>> = intent_entry(&intents[0].batch)
            .staged()
            .placement()
            .iter()
            .map(|s| format!("desired:dserver:{s}").into_bytes())
            .collect();
        assert_eq!(fenced, named);
        assert_eq!(intents[1].answer, Answer::Committed, "the re-plan landed");
        assert_nothing_names(&rig, &attempt, server).await;
        assert_eq!(attempt.chunks().len(), 3);
        assert_fragments_evidenced(&rig).await;
    })
    .await;
}

/// **B2.** `desired:dserver:<S>` is recorded before staging and `S` is the selector's first
/// pick (lowest utilization) on the full topology. Staging completes, nothing names `S`, and no
/// intent conflicts: the plan left `S` out, rather than learning of it from the fence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_a_drain_recorded_before_staging_is_planned_around() {
    within(async {
        let mut rig = Rig::new().await;
        let preferred = 9;
        rig.topology = topology_preferring(preferred);
        assert_eq!(
            select_distinct_domains(&rig.topology, FRAGMENTS).unwrap()[0],
            preferred,
            "the fixture binds: S is the full topology's first pick"
        );
        let drain = WriteBatch::new().put(
            metadata::desired_dserver_key(preferred),
            b"draining".to_vec(),
        );
        rig.store.inner.commit(drain).await.unwrap();
        let body = body_bytes(3 * CHUNK);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK])))
                .await,
        );
        assert_nothing_names(&rig, &attempt, preferred).await;
        assert!(
            rig.log
                .commits(Kind::Intent)
                .iter()
                .all(|c| c.answer == Answer::Committed),
            "no intent conflicted"
        );
        assert_eq!(rig.log.commits(Kind::Intent).len(), 3);
    })
    .await;
}

/// **B3.** Four of twelve single-server domains draining and RS(4,2), which fits in the other
/// eight: the part stages, and nothing names a draining server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b3_four_of_twelve_draining_still_stages() {
    within(async {
        let mut rig = Rig::new().await;
        for server in 0..4 {
            let drain =
                WriteBatch::new().put(metadata::desired_dserver_key(server), b"draining".to_vec());
            rig.store.inner.commit(drain).await.unwrap();
        }
        let body = body_bytes(2 * CHUNK + 5);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK + 5])))
                .await,
        );
        for server in 0..4 {
            assert_nothing_names(&rig, &attempt, server).await;
        }
        assert_eq!(attempt.chunks().len(), 3);
    })
    .await;
}

/// **B3.** Too few non-draining domains for the scheme (RS(6,3) needs nine, eight remain): a
/// typed placement error, never `NoSuchUpload` for an `Open` session, and nothing written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b3_too_few_domains_is_a_typed_placement_error_writing_nothing() {
    within(async {
        let mut rig = Rig::new().await;
        for server in 0..4 {
            let drain =
                WriteBatch::new().put(metadata::desired_dserver_key(server), b"draining".to_vec());
            rig.store.inner.commit(drain).await.unwrap();
        }
        let before = snapshot(&rig.store.inner).await;
        let rs63 = EcScheme::ReedSolomon { k: 6, m: 3 };
        let outcome = rig
            .stage(
                |r| r.scheme = rs63,
                stream_of(vec![Bytes::from(body_bytes(CHUNK))]),
            )
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(
                cause,
                Stop::Placement(SelectorError::InsufficientDomains { have: 8, need: 9 })
            ),
            "{cause:?}"
        );
        assert_eq!(snapshot(&rig.store.inner).await, before, "nothing written");
        assert!(rig.log.events().is_empty(), "no commit, no put");
        let session = rig
            .store
            .inner
            .get(&mpu_key(&rig.upload))
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            decode_session_record(&session).unwrap().state(),
            SessionState::Open {}
        ));
    })
    .await;
}

/// **B3.** Drains recorded after the slot is reserved, leaving too few domains: the intent's own
/// plan refuses with the same typed placement error, not `NoSuchUpload`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b3_drains_after_the_reserve_leave_a_typed_placement_error() {
    within(async {
        let mut rig = Rig::new().await;
        let drains: Hook = Arc::new(|_| {
            (0..7).fold(WriteBatch::new(), |batch, server| {
                batch.put(metadata::desired_dserver_key(server), b"draining".to_vec())
            })
        });
        rig.store
            .script(Kind::Reserve, vec![Act::First(drains)], None);
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(
                cause,
                Stop::Placement(SelectorError::InsufficientDomains { have: 5, need: 6 })
            ),
            "{cause:?}"
        );
        assert!(
            rig.log.commits(Kind::Intent).is_empty(),
            "no intent was sent"
        );
        assert!(rig.log.puts().is_empty());
    })
    .await;
}

/// **(B)** The drain key keeps the custodian's exact bytes.
#[test]
fn b_the_drain_key_keeps_its_bytes() {
    assert_eq!(
        metadata::desired_dserver_key(42),
        b"desired:dserver:42".to_vec()
    );
    assert_eq!(metadata::DESIRED_DSERVER_PREFIX, b"desired:dserver:");
    assert_eq!(
        metadata::parse_desired_dserver_key(b"desired:dserver:42"),
        Some(42)
    );
    assert_eq!(
        metadata::parse_desired_dserver_key(b"desired:dserver:x"),
        None
    );
    assert_eq!(metadata::parse_desired_dserver_key(b"orphan:42"), None);
}

// ═══════════════════════ (C) slots ═══════════════════════

/// A request for attempt `n` on the rig's session.
fn reserve_request<'a>(rig: &'a Rig, attempt: &'a AttemptId) -> StageRequest<'a> {
    request(&rig.topology, &rig.upload, attempt)
}

fn inflight_full() -> ReserveOutcome {
    ReserveOutcome::Refused(Refusal::SlowDown {
        pressure: Backpressure::InflightParts {
            max_inflight_parts: MAX_INFLIGHT_PARTS,
        },
    })
}

/// **(C)** `MAX_INFLIGHT_PARTS` reserves succeed on one session, at indices `0..16`; the next is
/// `SlowDown { InflightParts }`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_the_cap_is_the_key_space() {
    within(async {
        let rig = Rig::new().await;
        for n in 0..u128::from(MAX_INFLIGHT_PARTS) {
            let id = attempt(n);
            let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &id), T0)
                .await
                .unwrap();
            assert_eq!(
                outcome,
                ReserveOutcome::Reserved {
                    index: SlotIndex::new(n as u32).unwrap(),
                    attempt_id: id
                }
            );
        }
        let id = attempt(99);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &id), T0)
            .await
            .unwrap();
        assert_eq!(outcome, inflight_full());
        assert_eq!(
            slots(&rig.store.inner, &rig.upload).await.len(),
            MAX_INFLIGHT_PARTS as usize
        );
    })
    .await;
}

/// **(C)** A reserve against a session that is not `Open` — fenced by Abort, or absent — answers
/// `NoSuchUpload` and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_non_open_session_is_no_such_upload_writing_nothing() {
    within(async {
        let mut rig = Rig::new().await;
        let fenced = abort_session(&rig.store.inner, &rig.upload).await.unwrap();
        assert_eq!(fenced, AbortOutcome::Fenced);
        let before = snapshot(&rig.store.inner).await;
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(outcome, ReserveOutcome::Refused(Refusal::NoSuchUpload));
        let staging = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(10))]))
            .await;
        assert!(
            matches!(staging, Ok(StageOutcome::Refused(Refusal::NoSuchUpload))),
            "{staging:?}"
        );
        let (absent, _) = identity(77);
        let mut req = reserve_request(&rig, &rig.attempt);
        req.upload_id = &absent;
        let outcome = reserve_slot(&rig.store, &req, T0).await.unwrap();
        assert_eq!(outcome, ReserveOutcome::Refused(Refusal::NoSuchUpload));
        assert_eq!(snapshot(&rig.store.inner).await, before, "nothing written");
        assert!(rig.log.events().is_empty(), "no commit");
    })
    .await;
}

/// **(C)** A reserve whose batch lands but answers `CommitUnknownResult` — either
/// `may_still_commit` — adopts its own slot on re-read, with one commit. With a second live
/// attempt beside it and no residue, the held slots equal the live attempts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_landed_unknown_reserve_adopts_its_own_slot() {
    for may_still_commit in [false, true] {
        within(async {
            let rig = Rig::new().await;
            rig.store.script(
                Kind::Reserve,
                vec![Act::Unknown {
                    applied: true,
                    may_still_commit,
                }],
                None,
            );
            let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
                .await
                .unwrap();
            assert_eq!(
                outcome,
                ReserveOutcome::Reserved {
                    index: SlotIndex::new(0).unwrap(),
                    attempt_id: rig.attempt.clone()
                },
                "may_still_commit {may_still_commit}"
            );
            assert_eq!(
                rig.log.commits(Kind::Reserve).len(),
                1,
                "adopted, not re-reserved"
            );
            let other = attempt(5);
            let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &other), T0)
                .await
                .unwrap();
            assert!(matches!(outcome, ReserveOutcome::Reserved { .. }));
            assert_eq!(
                slots(&rig.store.inner, &rig.upload).await.len(),
                2,
                "two live attempts"
            );
            assert_eq!(
                slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
                1
            );
        })
        .await;
    }
}

/// **(C) The same-index rule.** Attempt X holds index 0. This reserve takes index 1 and its batch
/// is held (`may_still_commit: true`, absent on re-read); X releases index 0 before the retry.
/// The retry goes to index **1** again with the same attempt id, and once the held batch is
/// released this attempt holds exactly one slot. A reserve that took the lowest free index after
/// the ambiguous outcome would take 0 here and end with two.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_ambiguous_reserve_retries_the_same_index() {
    within(async {
        let rig = Rig::new().await;
        let x = attempt(1);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &x), T0)
            .await
            .unwrap();
        assert!(matches!(outcome, ReserveOutcome::Reserved { index, .. } if index.get() == 0));
        let x_slot = rig
            .store
            .inner
            .get(&slot_key(&rig.upload, SlotIndex::new(0).unwrap()))
            .await
            .unwrap()
            .unwrap();
        let key = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        let release: Hook = Arc::new(move |_| {
            WriteBatch::new()
                .require(key.clone(), x_slot.clone())
                .delete(key.clone())
        });
        rig.store
            .script(Kind::Reserve, vec![Act::HoldThen(release)], None);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            ReserveOutcome::Reserved {
                index: SlotIndex::new(1).unwrap(),
                attempt_id: rig.attempt.clone()
            },
            "the retry pinned index 1, though index 0 was free"
        );
        assert_eq!(
            rig.log.commits(Kind::Reserve).len(),
            3,
            "X's, the held one, the retry"
        );
        assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
        assert_eq!(
            slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
            1,
            "exactly one slot of this attempt"
        );
    })
    .await;
}

/// **(C)** After [`RESERVE_AMBIGUOUS_ROUNDS`] ambiguous rounds the reserve refuses `SlowDown`,
/// after exactly that many commits. Then the held batch lands after the refusal, with
/// `MAX_INFLIGHT_PARTS − 1` other slots held: exactly one slot of the refused attempt exists, no
/// live attempt owns it, and it counts toward the cap — the next reserve is refused too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot() {
    within(async {
        let rig = Rig::new().await;
        let x = attempt(1);
        reserve_slot(&rig.store, &reserve_request(&rig, &x), T0)
            .await
            .unwrap();
        let mut plan = vec![Act::Hold { advance: 0 }];
        plan.extend((1..RESERVE_AMBIGUOUS_ROUNDS).map(|_| Act::Unknown {
            applied: false,
            may_still_commit: true,
        }));
        rig.store.script(Kind::Reserve, plan, None);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            inflight_full(),
            "refused after its ambiguous rounds"
        );
        let ours: Vec<_> = rig.log.commits(Kind::Reserve).into_iter().skip(1).collect();
        assert_eq!(
            ours.len(),
            RESERVE_AMBIGUOUS_ROUNDS as usize,
            "exactly the bound"
        );
        let pinned = slot_key(&rig.upload, SlotIndex::new(1).unwrap());
        assert!(
            ours.iter()
                .all(|c| c.batch.puts.iter().all(|(key, _)| *key == pinned)),
            "every round went to the one index"
        );
        assert_eq!(
            slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
            0
        );

        // Fill every other index, then the held batch lands.
        for index in 2..MAX_INFLIGHT_PARTS {
            let key = slot_key(&rig.upload, SlotIndex::new(index).unwrap());
            let put = WriteBatch::new().put(key, other_slot(u128::from(index), PART + 1));
            rig.store.inner.commit(put).await.unwrap();
        }
        assert_eq!(
            slots(&rig.store.inner, &rig.upload).await.len(),
            MAX_INFLIGHT_PARTS as usize - 1
        );
        assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        assert_eq!(
            slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
            1,
            "exactly one slot of the refused attempt, never two"
        );
        assert_eq!(
            slots(&rig.store.inner, &rig.upload).await.len(),
            MAX_INFLIGHT_PARTS as usize
        );
        let next = attempt(50);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &next), T0)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            inflight_full(),
            "the residue counts toward the cap"
        );
    })
    .await;
}

/// **(C)** A reserve whose held index is then taken by another attempt refuses rather than
/// probing elsewhere: a second slot for this attempt could land once that attempt released it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_held_index_taken_by_another_refuses() {
    within(async {
        let rig = Rig::new().await;
        let key = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        // Another attempt at the same part: only the attempt id tells it from this one's slot.
        let squat: Hook =
            Arc::new(move |_| WriteBatch::new().put(key.clone(), other_slot(9, PART)));
        rig.store
            .script(Kind::Reserve, vec![Act::HoldThen(squat)], None);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(outcome, inflight_full());
        assert_eq!(
            rig.log.commits(Kind::Reserve).len(),
            1,
            "no probe of another index"
        );
        assert_eq!(
            slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
            0
        );
    })
    .await;
}

/// **(C)** A session moved to a later `Open` epoch under the reserve (a Complete fence and its
/// release): the reserve is answered from the new state and reserves under it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_session_moved_to_a_later_open_epoch_is_reserved_under() {
    within(async {
        let rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        let hook = move_session(&rig.upload, later.clone());
        rig.store
            .script(Kind::Reserve, vec![Act::First(hook)], None);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert!(
            matches!(outcome, ReserveOutcome::Reserved { .. }),
            "{outcome:?}"
        );
        let reserves = rig.log.commits(Kind::Reserve);
        assert_eq!(reserves.len(), 2);
        assert_eq!(reserves[0].answer, Answer::Conflict);
        let session = mpu_key(&rig.upload);
        assert!(reserves[1]
            .batch
            .preconditions
            .iter()
            .any(|p| p.key == session && p.expected.as_ref() == Some(&later)));
    })
    .await;
}

/// **(C)** A held reserve at index 0 lands **after** its slot read found the index empty and
/// **before** the session moves to a later `Open` epoch, so the session read that follows finds
/// the session moved. The reserve reads the index once more before letting it go, finds its own
/// slot, and keeps it under the new session: one reserve commit, one slot of this attempt, and
/// the intent fenced on the new session bytes lands first time. A reserve that unpinned on
/// seeing the session move would probe index 1 and hold two slots.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot() {
    within(async {
        let mut rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        rig.store
            .script(Kind::Reserve, vec![Act::Hold { advance: 0 }], None);
        let index0 = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        let move_session = WriteBatch::new().put(mpu_key(&rig.upload), later.clone());
        rig.store.after_read(
            &index0,
            0,
            vec![Then::ReleaseHeld, Then::Commit(move_session)],
        );
        let body = body_bytes(CHUNK);
        let attempt = staged(rig.stage(|_| {}, stream_of(vec![Bytes::from(body)])).await);
        assert_eq!(
            slots_of(&rig.store.inner, &rig.upload, &rig.attempt).await,
            1,
            "exactly one slot of this attempt"
        );
        assert_eq!(
            attempt.slot_index().get(),
            0,
            "the index its held batch took"
        );
        assert_eq!(rig.log.commits(Kind::Reserve).len(), 1, "no second probe");
        assert_eq!(attempt.session_bytes(), &later, "fenced on the new session");
        let intents = rig.log.commits(Kind::Intent);
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].answer, Answer::Committed);
    })
    .await;
}

/// **(C)** The index a held reserve may still install is found holding bytes that are not a
/// slot record: a typed record error (ADR-0045) that keeps the held batch's doubt, never read as
/// "another attempt's slot".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_undecodable_slot_at_the_reserved_index_is_a_typed_error() {
    within(async {
        let rig = Rig::new().await;
        let key = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        let garbage: Hook =
            Arc::new(move |_| WriteBatch::new().put(key.clone(), b"not a slot".to_vec()));
        rig.store
            .script(Kind::Reserve, vec![Act::HoldThen(garbage)], None);
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0).await;
        let Err(StageError::InDoubt(doubt)) = outcome else {
            panic!("expected InDoubt, got {outcome:?}");
        };
        assert!(doubt.may_still_commit, "the held batch's doubt is kept");
        assert!(matches!(doubt.cause, Stop::Record(_)), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Reserve).len(), 1);
    })
    .await;
}

// ═══════════════════════ (D) renewal ═══════════════════════

/// The renewal batches in the log that landed, each checked to rewrite the attempt's slot in the
/// same batch as every owned entry it requires (`0016:349`, `:659`).
fn renewals(log: &Log) -> Vec<Commit> {
    let renewals = log.commits(Kind::Renewal);
    for renewal in &renewals {
        let batch = &renewal.batch;
        let required: BTreeSet<&Vec<u8>> = batch
            .preconditions
            .iter()
            .filter(|p| p.expected.is_some() && p.key.starts_with(SIDX_PREFIX))
            .map(|p| &p.key)
            .collect();
        let put: BTreeSet<&Vec<u8>> = batch
            .puts
            .iter()
            .filter(|(key, _)| key.starts_with(SIDX_PREFIX))
            .map(|(key, _)| key)
            .collect();
        assert_eq!(
            required, put,
            "a renewal rewrites exactly the owned entries it requires"
        );
        let slot_required = batch
            .preconditions
            .iter()
            .find(|p| p.key.starts_with(SLOT_PREFIX))
            .expect("a renewal requires its slot");
        assert!(
            batch.puts.iter().any(|(key, _)| *key == slot_required.key),
            "every renewal rewrites the slot"
        );
    }
    renewals
}

/// **(D) A slow client.** The body arrives in small pieces, the clock moving past half a TTL
/// between them and past a whole TTL before the first chunk boundary, with no timer fired.
/// Renewal runs between the pieces: the first renewal lands before the third piece is even read,
/// the slot lease is moved forward before the first intent, and no lease is ever lapsed. A writer
/// that renewed only at chunk boundaries would find its slot lapsed at the first one
/// (`0016:1824-1837`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_renewal_runs_while_the_body_arrives_before_the_first_chunk() {
    within(async {
        let mut rig = Rig::new().await;
        let body = body_bytes(CHUNK + 10);
        let sized = pieces(&body, &[10, 10, 10, CHUNK - 30, 10]);
        let ticks = vec![0, HALF + 1, HALF + 1, HALF + 1, 0];
        // The fixture binds: the first chunk boundary falls after the reserved lease expired.
        const { assert!(T0 + 3 * (HALF + 1) > T0 + TTL) };
        let body_stream = ticking(&rig.clock, &rig.log, ticks.into_iter().zip(sized).collect());
        let attempt = staged(rig.stage(|_| {}, body_stream).await);
        assert_eq!(attempt.len(), body.len() as u64);
        let events = rig.log.events();
        let at = |found: &dyn Fn(&Event) -> bool| events.iter().position(found);
        let first_intent = at(&|e| {
            matches!(
                e,
                Event::Commit {
                    kind: Kind::Intent,
                    ..
                }
            )
        })
        .unwrap();
        let first_renewal = at(&|e| {
            matches!(
                e,
                Event::Commit {
                    kind: Kind::Renewal,
                    ..
                }
            )
        })
        .expect("a renewal ran");
        let third_read = at(&|e| matches!(e, Event::Read { index: 2 })).unwrap();
        assert!(
            first_renewal < third_read,
            "renewed while the body was still arriving"
        );
        assert!(
            first_renewal < first_intent,
            "renewed before the first intent"
        );
        let renewals = renewals(&rig.log);
        assert!(
            renewals.len() >= 2,
            "renewed again before the first chunk boundary"
        );
        assert_eq!(renewals[0].answer, Answer::Committed);
        assert_eq!(
            slot_put(&renewals[0].batch).lease_expiry_millis(),
            T0 + HALF + 1 + TTL,
            "the slot lease moved forward from the renewal's own reading"
        );
        assert_no_lapse_observed(&rig.log);
    })
    .await;
}

/// **(D-i) Body withheld.** After the slot is reserved the test withholds the next piece, moves
/// the clock past half a TTL and fires the timer: a renewal rewriting the slot commits while the
/// read is still pending. Then the piece is delivered and staging finishes normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_i_renewal_commits_while_a_body_read_is_pending() {
    within(async {
        let mut rig = Rig::new().await;
        let (tx, body_stream) = channel();
        let body = body_bytes(2 * CHUNK);
        tx.send(Bytes::copy_from_slice(&body[..10])).unwrap();
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(&rig.store, &rig.fleet, &mut rig.runtime, &req, body_stream);
        let driver = async move {
            until("the reserve", || log.landed(Kind::Reserve) == 1).await;
            clock.advance(HALF + 1);
            timer.add_permits(1);
            until("a renewal", || log.landed(Kind::Renewal) == 1).await;
            assert!(
                log.commits(Kind::Intent).is_empty(),
                "the body is still withheld"
            );
            tx.send(Bytes::copy_from_slice(&body[10..])).unwrap();
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let attempt = staged(outcome);
        assert_eq!(attempt.len(), 2 * CHUNK as u64);
        let renewal = &renewals(&rig.log)[0];
        assert_eq!(
            slot_put(&renewal.batch).lease_expiry_millis(),
            T0 + HALF + 1 + TTL
        );
        assert_no_lapse_observed(&rig.log);
    })
    .await;
}

/// **(D)** Renewal falls due at exactly half a TTL after the reservation, not one tick later: with
/// the clock at that instant and the timer fired while a body read is pending, the renewal
/// commits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_renewal_falls_due_at_exactly_half_a_ttl() {
    within(async {
        let mut rig = Rig::new().await;
        let (tx, body_stream) = channel();
        let body = body_bytes(CHUNK);
        tx.send(Bytes::copy_from_slice(&body[..10])).unwrap();
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(&rig.store, &rig.fleet, &mut rig.runtime, &req, body_stream);
        let driver = async move {
            until("the reserve", || log.landed(Kind::Reserve) == 1).await;
            clock.advance(HALF);
            timer.add_permits(1);
            until("a renewal at the due instant", || {
                log.landed(Kind::Renewal) == 1
            })
            .await;
            tx.send(Bytes::copy_from_slice(&body[10..])).unwrap();
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        staged(outcome);
        let renewal = &renewals(&rig.log)[0];
        assert_eq!(renewal.clock, T0 + HALF);
        assert_eq!(
            slot_put(&renewal.batch).lease_expiry_millis(),
            T0 + HALF + TTL
        );
    })
    .await;
}

/// **(D-ii) Fragment write delayed.** The chunk store holds one fragment put of chunk 2, after
/// chunk 1's and chunk 2's owned entries are durable. The test moves the clock and fires the
/// timer: a renewal rewriting the slot and both chunks' owned leases commits while the put is
/// pending. Then the put is released, staging finishes, and that put was sent once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_ii_renewal_commits_while_a_fragment_write_is_pending() {
    within(async {
        let mut rig = Rig::new().await;
        let held = FragmentId { chunk: 2, index: 0 };
        let gate = rig.fleet.hold(held);
        let body = body_bytes(3 * CHUNK);
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let mut req = request(&rig.topology, &rig.upload, &rig.attempt);
        // A write window wider than half a TTL, so the held write is still inside its deadline
        // when the renewal falls due.
        req.write_window_millis = 3 * TTL;
        let staging = stage_part(
            &rig.store,
            &rig.fleet,
            &mut rig.runtime,
            &req,
            stream_of(pieces(&body, &[3 * CHUNK])),
        );
        let upload = rig.upload.clone();
        let driver = async move {
            until("chunk 2's held put", || log.put_count(held) == 1).await;
            clock.advance(HALF + 1);
            timer.add_permits(1);
            until("a renewal", || log.landed(Kind::Renewal) == 1).await;
            let renewal = &renewals(&log)[0];
            let rewritten: BTreeSet<Vec<u8>> = renewal
                .batch
                .puts
                .iter()
                .map(|(key, _)| key.clone())
                .collect();
            for chunk in [1, 2] {
                assert!(
                    rewritten.contains(&sidx_key(&upload, part(), chunk)),
                    "the renewal moves chunk {chunk}'s owned lease"
                );
            }
            assert_eq!(
                log.commits(Kind::Intent).len(),
                2,
                "chunk 3 has not started"
            );
            gate.add_permits(1);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let attempt = staged(outcome);
        assert_eq!(attempt.chunks().len(), 3);
        assert_eq!(rig.log.put_count(held), 1, "the held put was not re-sent");
        assert_no_lapse_observed(&rig.log);
        // Every renewed owned lease moved with the slot's, from the renewal's own reading.
        let renewal = &renewals(&rig.log)[0];
        let leases: BTreeSet<u64> = renewal
            .batch
            .puts
            .iter()
            .map(|(key, value)| {
                if key.starts_with(SLOT_PREFIX) {
                    decode_slot_record(value).unwrap().lease_expiry_millis()
                } else {
                    decode_owned_entry(key, value)
                        .unwrap()
                        .2
                        .lease_expiry_millis()
                }
            })
            .collect();
        assert_eq!(leases, BTreeSet::from([T0 + HALF + 1 + TTL]));
    })
    .await;
}

/// **(D)** The slot's lease lapsed before the next renewal: the renewal refuses, writes nothing,
/// and staging stops with a typed error. A lapsed lease is never moved forward.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_lapsed_slot_lease_is_refused_never_revived() {
    within(async {
        let mut rig = Rig::new().await;
        let (tx, body_stream) = channel();
        tx.send(Bytes::from(body_bytes(10))).unwrap();
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(&rig.store, &rig.fleet, &mut rig.runtime, &req, body_stream);
        let driver = async move {
            until("the reserve", || log.landed(Kind::Reserve) == 1).await;
            clock.advance(TTL + 1);
            timer.add_permits(1);
            // The timer woke staging while the read was pending; then end the body, so a staging
            // that went on past the lapsed lease finishes (and fails the leg) instead of waiting.
            until("the timer to fire", || timer.available_permits() == 0).await;
            drop(tx);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let reserved = rig.log.commits(Kind::Reserve).remove(0).batch.puts[0].clone();
        let err = outcome.expect_err("staging stopped");
        assert_eq!(classify(&err), ErrorClass::Terminal);
        // The message names the upload, the record and the rule, and says nothing is in doubt.
        let text = err.to_string();
        let slot = String::from_utf8(reserved.0.clone()).unwrap();
        for part in [
            rig.upload.as_str(),
            slot.as_str(),
            "is not revived",
            "no batch of this part attempt is in doubt",
        ] {
            assert!(text.contains(part), "{part:?} in {text:?}");
        }
        let cause = stopped(Err(err));
        assert!(
            matches!(&cause, Stop::LeaseLapsed { key, lease_expiry_millis, now_millis }
                if key.as_bytes() == reserved.0.as_slice()
                    && *lease_expiry_millis == T0 + TTL
                    && *now_millis == T0 + TTL + 1),
            "{cause:?}"
        );
        assert!(rig.log.commits(Kind::Renewal).is_empty(), "nothing written");
        assert_eq!(
            rig.store.inner.get(&reserved.0).await.unwrap(),
            Some(reserved.1),
            "the slot is exactly as reserved"
        );
    })
    .await;
}

/// **(D)** An owned entry's lease lapsed (the slot's has not) before the next renewal: refused,
/// nothing written, a typed error naming that entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_lapsed_owned_lease_is_refused_never_revived() {
    within(async {
        let mut rig = Rig::new().await;
        // Chunk 1 is registered at T0, chunk 2 at T0 + 1000: the slot's lease outlives chunk 1's.
        rig.store
            .script(Kind::Intent, vec![], Some(Act::PassThenAdvance(1_000)));
        let held = FragmentId { chunk: 2, index: 0 };
        let _gate = rig.fleet.hold(held);
        let body = body_bytes(3 * CHUNK);
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(
            &rig.store,
            &rig.fleet,
            &mut rig.runtime,
            &req,
            stream_of(pieces(&body, &[3 * CHUNK])),
        );
        let driver = async move {
            until("chunk 2's held put", || log.put_count(held) == 1).await;
            clock.set(T0 + TTL);
            timer.add_permits(1);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let cause = stopped(outcome);
        let chunk1 = String::from_utf8(sidx_key(&rig.upload, part(), 1)).unwrap();
        assert!(
            matches!(&cause, Stop::LeaseLapsed { key, lease_expiry_millis, .. }
                if *key == chunk1 && *lease_expiry_millis == T0 + TTL),
            "{cause:?}"
        );
        assert!(rig.log.commits(Kind::Renewal).is_empty(), "nothing written");
        let entry = &owned(&rig.store.inner, &rig.upload).await[&1];
        assert_eq!(
            entry.lease_expiry_millis(),
            T0 + TTL,
            "chunk 1's lease was not revived"
        );
    })
    .await;
}

/// **(D)** A renewal after the session left `Open` (an Abort fenced it) refuses and resurrects
/// nothing: its batch conflicts, nothing is written, and staging answers `NoSuchUpload`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_renewal_after_the_session_left_open_resurrects_nothing() {
    within(async {
        let mut rig = Rig::new().await;
        let (tx, body_stream) = channel();
        tx.send(Bytes::from(body_bytes(10))).unwrap();
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(&rig.store, &rig.fleet, &mut rig.runtime, &req, body_stream);
        let (inner, upload) = (&rig.store.inner, rig.upload.clone());
        let driver = async move {
            until("the reserve", || log.landed(Kind::Reserve) == 1).await;
            let fenced = abort_session(inner, &upload).await.unwrap();
            assert_eq!(fenced, AbortOutcome::Fenced);
            clock.advance(HALF + 1);
            timer.add_permits(1);
            until("the timer to fire", || timer.available_permits() == 0).await;
            drop(tx);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        assert!(
            matches!(outcome, Ok(StageOutcome::Refused(Refusal::NoSuchUpload))),
            "{outcome:?}"
        );
        let renewal_answers: Vec<Answer> = rig
            .log
            .commits(Kind::Renewal)
            .iter()
            .map(|c| c.answer)
            .collect();
        assert_eq!(
            renewal_answers,
            vec![Answer::Conflict],
            "one renewal, and it wrote nothing"
        );
        let reserved = rig.log.commits(Kind::Reserve).remove(0).batch.puts[0].clone();
        assert_eq!(
            rig.store.inner.get(&reserved.0).await.unwrap(),
            Some(reserved.1)
        );
    })
    .await;
}

/// **(D)** The commit's quiesce step ([`StagedAttempt::renew`]) moves every lease forward, and
/// never backwards: a TTL too short to reach past the current slot lease still moves it one
/// millisecond on. Once the session left `Open` it answers `NoSuchUpload` and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_the_quiesce_renewal_never_moves_a_lease_backwards() {
    within(async {
        let mut rig = Rig::new().await;
        let body = body_bytes(2 * CHUNK);
        let mut attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK])))
                .await,
        );
        // Reserved at T0 and both intents committed at T0: each batch stamped T0 + TTL, moved on
        // one millisecond past the lease it replaced.
        let before = attempt.slot().lease_expiry_millis();
        assert_eq!(before, T0 + TTL + 2);
        let now = T0 + 100;
        rig.clock.set(now);
        let outcome = attempt.renew(&rig.store, &rig.runtime, 1).await.unwrap();
        assert_eq!(outcome, RenewOutcome::Renewed);
        assert_eq!(
            attempt.slot().lease_expiry_millis(),
            before + 1,
            "one millisecond on"
        );
        for held in attempt.owned() {
            assert_eq!(held.entry().lease_expiry_millis(), before + 1);
            assert_eq!(
                rig.store.inner.get(held.key()).await.unwrap().as_ref(),
                Some(held.bytes())
            );
        }
        let outcome = attempt
            .renew(&rig.store, &rig.runtime, 3 * TTL)
            .await
            .unwrap();
        assert_eq!(outcome, RenewOutcome::Renewed);
        assert_eq!(attempt.slot().lease_expiry_millis(), now + 3 * TTL);
        assert_eq!(
            rig.store
                .inner
                .get(attempt.slot_key())
                .await
                .unwrap()
                .as_ref(),
            Some(attempt.slot_bytes())
        );

        let fenced = abort_session(&rig.store.inner, &rig.upload).await.unwrap();
        assert_eq!(fenced, AbortOutcome::Fenced);
        let unchanged = snapshot(&rig.store.inner).await;
        rig.clock.set(now + 1);
        let outcome = attempt.renew(&rig.store, &rig.runtime, TTL).await.unwrap();
        assert_eq!(outcome, RenewOutcome::Refused(Refusal::NoSuchUpload));
        assert_eq!(
            snapshot(&rig.store.inner).await,
            unchanged,
            "nothing resurrected"
        );
    })
    .await;
}

/// **(D)** The slot was taken from under a live attempt (deleted, with the session unchanged):
/// the renewal conflicts and staging stops with a typed error naming the slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_slot_lost_under_an_unchanged_session_is_a_typed_error() {
    within(async {
        let mut rig = Rig::new().await;
        let (tx, body_stream) = channel();
        tx.send(Bytes::from(body_bytes(10))).unwrap();
        let (log, clock, timer) = (
            rig.log.clone(),
            rig.clock.clone(),
            Arc::clone(&rig.runtime.timer),
        );
        let req = request(&rig.topology, &rig.upload, &rig.attempt);
        let staging = stage_part(&rig.store, &rig.fleet, &mut rig.runtime, &req, body_stream);
        let (inner, slot) = (
            &rig.store.inner,
            slot_key(&rig.upload, SlotIndex::new(0).unwrap()),
        );
        let driver = async move {
            until("the reserve", || log.landed(Kind::Reserve) == 1).await;
            inner.commit(WriteBatch::new().delete(slot)).await.unwrap();
            clock.advance(HALF + 1);
            timer.add_permits(1);
            until("the timer to fire", || timer.available_permits() == 0).await;
            drop(tx);
        };
        let (outcome, ()) = tokio::join!(staging, driver);
        let cause = stopped(outcome);
        assert!(matches!(cause, Stop::SlotLost { .. }), "{cause:?}");
    })
    .await;
}

/// The cause of a staging that stopped on a lapsed lease: the key, its expiry, the reading.
fn lapsed(cause: &Stop) -> (Vec<u8>, u64, u64) {
    match cause {
        Stop::LeaseLapsed {
            key,
            lease_expiry_millis,
            now_millis,
        } => (key.clone().into_bytes(), *lease_expiry_millis, *now_millis),
        other => panic!("expected LeaseLapsed, got {other:?}"),
    }
}

/// **(D)** The slot's lease lapses while the first intent reads the drain records (no renewal
/// is due when the intent starts). The intent checks every lease at its own reading, taken after
/// that read, and refuses: no intent is sent, so the lapsed slot lease is never moved forward,
/// and no fragment is written. An intent that stamped a fresh lease without the check would
/// revive the slot and stage the part.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read() {
    within(async {
        let mut rig = Rig::new().await;
        // The first drain read is staging's pre-check; the second is the first intent's.
        rig.store.after_read(
            metadata::DESIRED_DSERVER_PREFIX,
            1,
            vec![Then::Advance(TTL)],
        );
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        let reserved = rig.log.commits(Kind::Reserve).remove(0).batch.puts[0].clone();
        assert_eq!(lapsed(&cause), (reserved.0.clone(), T0 + TTL, T0 + TTL));
        assert!(
            rig.log.commits(Kind::Intent).is_empty(),
            "no intent was sent"
        );
        assert!(rig.log.commits(Kind::Renewal).is_empty());
        assert!(rig.log.puts().is_empty());
        assert_eq!(
            rig.store.inner.get(&reserved.0).await.unwrap(),
            Some(reserved.1),
            "the slot is exactly as reserved"
        );
    })
    .await;
}

/// **(D)** Chunk 1's owned lease (the earliest lease the attempt holds; the slot's is a
/// millisecond later) lapses while chunk 2's intent reads the drain records. Chunk 2's intent
/// checks chunk 1's lease too, and refuses before it is sent. An intent that checked only the
/// slot's lease would land chunk 2 under a dead entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read() {
    within(async {
        let mut rig = Rig::new().await;
        // Pre-check, chunk 1's intent, then chunk 2's.
        rig.store.after_read(
            metadata::DESIRED_DSERVER_PREFIX,
            2,
            vec![Then::Advance(TTL)],
        );
        let body = body_bytes(2 * CHUNK);
        let outcome = rig
            .stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK])))
            .await;
        let cause = stopped(outcome);
        let chunk1 = sidx_key(&rig.upload, part(), 1);
        assert_eq!(lapsed(&cause), (chunk1, T0 + TTL, T0 + TTL));
        assert!(
            rig.log.intents_of(2).is_empty(),
            "chunk 2's intent was never sent"
        );
        assert!(rig.log.puts().iter().all(|put| put.id.chunk == 1));
        assert!(rig.log.commits(Kind::Renewal).is_empty());
        assert_eq!(
            owned(&rig.store.inner, &rig.upload).await[&1].lease_expiry_millis(),
            T0 + TTL,
            "chunk 1's lease was not revived"
        );
    })
    .await;
}

/// **(D)** An intent round is lost — answered `Conflict` unapplied, or held — slowly enough that
/// the slot's lease lapses before the retry. The retry checks the leases at its own fresh reading
/// and refuses before it is sent: one intent commit. Lost to a conflict, nothing is in doubt; held,
/// the error keeps the held batch's `may_still_commit` doubt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds() {
    for held in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            let lost = if held {
                Act::Hold { advance: TTL }
            } else {
                Act::ConflictThenAdvance(TTL)
            };
            rig.store.script(Kind::Intent, vec![lost], None);
            let outcome = rig
                .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
                .await;
            let slot = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
            let cause = match (held, outcome) {
                (false, outcome) => stopped(outcome),
                (true, Err(StageError::InDoubt(doubt))) => {
                    assert!(doubt.may_still_commit, "the held batch's doubt is kept");
                    doubt.cause
                }
                (true, other) => panic!("expected InDoubt, got {other:?}"),
            };
            assert_eq!(lapsed(&cause), (slot, T0 + TTL, T0 + TTL), "held {held}");
            assert_eq!(rig.log.commits(Kind::Intent).len(), 1, "no retry was sent");
            assert!(rig.log.puts().is_empty());
        })
        .await;
    }
}

/// **(D)** A renewal round is lost — answered `Conflict` unapplied, or held — slowly enough that
/// the slot's lease lapses before the retry. The retry reads the clock afresh, checks the leases
/// at that reading and refuses before it is sent: one renewal commit, and the slot is not revived.
/// Held, the error keeps the doubt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds() {
    for held in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            // Renewal falls due at T0 + HALF + 1; the lost round moves the clock past T0 + TTL.
            let lost = if held {
                Act::Hold { advance: HALF }
            } else {
                Act::ConflictThenAdvance(HALF)
            };
            rig.store.script(Kind::Renewal, vec![lost], None);
            let body_stream = renewal_body(&rig.clock, &rig.log);
            let outcome = rig.stage(|_| {}, body_stream).await;
            let slot = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
            let cause = match (held, outcome) {
                (false, outcome) => stopped(outcome),
                (true, Err(StageError::InDoubt(doubt))) => {
                    assert!(doubt.may_still_commit, "the held batch's doubt is kept");
                    doubt.cause
                }
                (true, other) => panic!("expected InDoubt, got {other:?}"),
            };
            assert_eq!(
                lapsed(&cause),
                (slot.clone(), T0 + TTL, T0 + TTL + 1),
                "held {held}"
            );
            let renewals = rig.log.commits(Kind::Renewal);
            assert_eq!(renewals.len(), 1, "no retry was sent");
            assert_eq!(renewals[0].clock, T0 + HALF + 1);
            assert!(rig.log.commits(Kind::Intent).is_empty());
            let reserved = rig.log.commits(Kind::Reserve).remove(0).batch.puts[0].clone();
            assert_eq!(
                rig.store.inner.get(&slot).await.unwrap(),
                Some(reserved.1),
                "the slot was not revived"
            );
        })
        .await;
    }
}

/// **(D)** The commit's quiesce step ([`StagedAttempt::renew`]) runs on the attempt's clock, not
/// a fixed reading: its first round is lost slowly enough for the leases to lapse, and the retry,
/// at a fresh reading, refuses with nothing more sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_the_quiesce_renewal_reads_the_clock_every_round() {
    within(async {
        let mut rig = Rig::new().await;
        let mut attempt = staged(
            rig.stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
                .await,
        );
        let unchanged = snapshot(&rig.store.inner).await;
        rig.store
            .script(Kind::Renewal, vec![Act::ConflictThenAdvance(2 * TTL)], None);
        let err = attempt
            .renew(&rig.store, &rig.runtime, TTL)
            .await
            .expect_err("the retry refused");
        let cause = stopped(Err(err));
        let (key, lease, now) = lapsed(&cause);
        assert_eq!(key, attempt.slot_key());
        assert_eq!(lease, attempt.slot().lease_expiry_millis());
        assert_eq!(now, T0 + 2 * TTL);
        assert_eq!(rig.log.commits(Kind::Renewal).len(), 1, "no retry was sent");
        assert_eq!(
            snapshot(&rig.store.inner).await,
            unchanged,
            "nothing revived"
        );
    })
    .await;
}

// ═══════════════════════ (E) unknown outcomes ═══════════════════════

/// **(E)** An intent whose batch lands but answers `CommitUnknownResult` (either
/// `may_still_commit`) is recognised by re-reading the owned entry's exact bytes: one commit, no
/// second entry for the chunk, and its fragments go to the placement that landed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_landed_unknown_intent_is_adopted_from_its_exact_bytes() {
    for may_still_commit in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            rig.store.script(
                Kind::Intent,
                vec![Act::Unknown {
                    applied: true,
                    may_still_commit,
                }],
                None,
            );
            let body = body_bytes(3 * CHUNK);
            let attempt = staged(
                rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK])))
                    .await,
            );
            assert_eq!(rig.log.intents_of(1).len(), 1, "adopted, not re-sent");
            let entries = owned(&rig.store.inner, &rig.upload).await;
            assert_eq!(entries.len(), 3, "one owned entry per chunk");
            let landed = intent_entry(&rig.log.intents_of(1)[0].batch);
            assert_eq!(entries[&1], landed);
            assert_eq!(attempt.chunks()[0].placement, landed.staged().placement());
            assert_fragments_evidenced(&rig).await;
        })
        .await;
    }
}

/// **(E)** An intent answered `CommitUnknownResult { may_still_commit: false }` and not applied is
/// retried; staging ends with exactly one owned entry per chunk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_an_out_of_flight_unknown_intent_that_did_not_land_is_retried() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(
            Kind::Intent,
            vec![Act::Unknown {
                applied: false,
                may_still_commit: false,
            }],
            None,
        );
        let body = body_bytes(3 * CHUNK);
        staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK])))
                .await,
        );
        assert_eq!(rig.log.intents_of(1).len(), 2, "retried once");
        assert_eq!(owned(&rig.store.inner, &rig.upload).await.len(), 3);
        assert_fragments_evidenced(&rig).await;
    })
    .await;
}

/// **(E) (i)** An intent is held (`may_still_commit: true`) and the clock moves on; the held
/// batch is released just before the attempt's next batch, which therefore conflicts. The
/// re-read finds the held batch's exact entry: it is adopted — its placement, and its own
/// `t_auth` for the deadline — and no fragment was written before that.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_i_a_held_intent_released_before_the_next_batch_is_adopted() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(
            Kind::Intent,
            vec![Act::Hold { advance: 500 }, Act::ReleaseHeldFirst],
            None,
        );
        let body = body_bytes(2 * CHUNK);
        staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK])))
                .await,
        );
        let intents = rig.log.intents_of(1);
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[1].answer, Answer::Conflict);
        let held = intent_put(&intents[0].batch);
        assert_eq!(
            rig.store.inner.get(&held.0).await.unwrap(),
            Some(held.1),
            "the held batch's entry is the durable one"
        );
        for put in rig.log.puts().iter().filter(|p| p.id.chunk == 1) {
            assert_eq!(
                put.deadline,
                Some(intents[0].clock + W),
                "the held batch's t_auth"
            );
        }
        assert_intents_precede_puts(&rig.log);
        assert_fragments_evidenced(&rig).await;
        assert_eq!(owned(&rig.store.inner, &rig.upload).await.len(), 2);
    })
    .await;
}

/// **(E) (ii)** Chunk 2's intent is held, and every later batch is answered `Conflict` unapplied
/// until the rounds run out: no fragment of chunk 2 is written on the strength of an absent read,
/// and staging returns an in-doubt error that keeps `may_still_commit`. The held batch then lands,
/// and every fragment written is named by a durable owned entry with its placement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_ii_a_held_intent_that_never_settles_ends_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(
            Kind::Intent,
            vec![Act::Pass, Act::Hold { advance: 0 }],
            Some(Act::Conflict),
        );
        let body = body_bytes(3 * CHUNK);
        let outcome = rig.stage(|_| {}, stream_of(pieces(&body, &[3 * CHUNK]))).await;
        let err = match outcome {
            Err(err @ StageError::InDoubt(_)) => err,
            other => panic!("expected InDoubt, got {other:?}"),
        };
        assert_eq!(classify(&err), ErrorClass::Indeterminate, "{err}");
        let text = err.to_string();
        let spent = format!("chunk 2's intent spent all {INTENT_ROUNDS} rounds");
        for part in [
            rig.upload.as_str(),
            "is in doubt",
            "may still be applied",
            spent.as_str(),
        ] {
            assert!(text.contains(part), "{part:?} in {text:?}");
        }
        let StageError::InDoubt(doubt) = err else { unreachable!() };
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(
            matches!(doubt.cause, Stop::IntentRoundsSpent { chunk: 2, rounds } if rounds == INTENT_ROUNDS),
            "{:?}",
            doubt.cause
        );
        assert_eq!(rig.log.intents_of(2).len(), INTENT_ROUNDS as usize);
        // Every later batch required the slot at the bytes the held batch required, so at most one
        // of them could land.
        let required: BTreeSet<Bytes> = rig
            .log
            .intents_of(2)
            .iter()
            .map(|intent| slot_required(&intent.batch))
            .collect();
        assert_eq!(required.len(), 1, "one required slot value across every round");
        assert!(
            rig.log.puts().iter().all(|p| p.id.chunk != 2),
            "no fragment of chunk 2 was written"
        );
        assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        assert_fragments_evidenced(&rig).await;
        assert_eq!(owned(&rig.store.inner, &rig.upload).await.len(), 2);
    })
    .await;
}

/// **(E)** An intent answered `CommitUnknownResult { may_still_commit: false }`, not applied, and
/// then answered `Conflict` until its rounds run out: the read after the unknown outcome settled
/// it (the batch is out of flight and its entry absent), so the error says nothing is in doubt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_an_out_of_flight_unknown_a_read_settled_is_not_reported_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(
            Kind::Intent,
            vec![Act::Unknown {
                applied: false,
                may_still_commit: false,
            }],
            Some(Act::Conflict),
        );
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(cause, Stop::IntentRoundsSpent { chunk: 1, rounds } if rounds == INTENT_ROUNDS),
            "{cause:?}"
        );
        assert!(owned(&rig.store.inner, &rig.upload).await.is_empty());
    })
    .await;
}

/// **(E)** An intent held (`may_still_commit: true`) while the session moves on to a later `Open`
/// epoch: the held batch required the old session bytes, so it can never land, and once the
/// rounds run out the error says nothing is in doubt. Released afterwards, the held batch indeed
/// conflicts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_intent_whose_session_moved_on_is_no_longer_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        let hook = move_session(&rig.upload, later);
        rig.store.script(
            Kind::Intent,
            vec![Act::HoldThen(hook)],
            Some(Act::Conflict),
        );
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(cause, Stop::IntentRoundsSpent { chunk: 1, rounds } if rounds == INTENT_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
        assert!(owned(&rig.store.inner, &rig.upload).await.is_empty());
        assert!(rig.log.puts().is_empty());
    })
    .await;
}

/// The body every renewal-outcome leg uses: renewal falls due before the third piece is read.
fn renewal_body(clock: &ManualClock, log: &Log) -> Body {
    let body = body_bytes(2 * CHUNK);
    let sized = pieces(&body, &[10, 10, 2 * CHUNK - 20]);
    ticking(
        clock,
        log,
        vec![0, HALF + 1, 0].into_iter().zip(sized).collect(),
    )
}

/// **(E)** A renewal answered `CommitUnknownResult` is settled by re-reading its own slot's bytes:
/// one that landed is adopted (one commit, and the next batch requires its slot), and one that
/// did not land, with `may_still_commit: false`, is sent again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_an_unknown_renewal_is_settled_by_its_own_slot_bytes() {
    for (applied, may_still_commit, commits) in
        [(true, false, 1), (true, true, 1), (false, false, 2)]
    {
        within(async {
            let mut rig = Rig::new().await;
            rig.store.script(
                Kind::Renewal,
                vec![Act::Unknown {
                    applied,
                    may_still_commit,
                }],
                None,
            );
            let body_stream = renewal_body(&rig.clock, &rig.log);
            staged(rig.stage(|_| {}, body_stream).await);
            let renewals = renewals(&rig.log);
            assert_eq!(
                renewals.len(),
                commits,
                "applied {applied}, may {may_still_commit}"
            );
            let landed = renewals.last().unwrap();
            let slot = landed
                .batch
                .puts
                .iter()
                .find(|(key, _)| key.starts_with(SLOT_PREFIX))
                .unwrap()
                .clone();
            let intent = &rig.log.commits(Kind::Intent)[0];
            assert!(
                intent
                    .batch
                    .preconditions
                    .iter()
                    .any(|p| p.key == slot.0 && p.expected.as_ref() == Some(&slot.1)),
                "the next batch requires the renewed slot"
            );
            assert_eq!(intent.answer, Answer::Committed);
        })
        .await;
    }
}

/// **(E)** A session moved to a later `Open` epoch under an intent: the intent is answered from
/// the new state and lands under it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_an_intent_under_a_session_moved_to_a_later_open_epoch_lands_under_it() {
    within(async {
        let mut rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        let hook = move_session(&rig.upload, later.clone());
        rig.store.script(Kind::Intent, vec![Act::First(hook)], None);
        let body = body_bytes(2 * CHUNK);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK])))
                .await,
        );
        assert_eq!(attempt.session_bytes(), &later);
        let intents = rig.log.intents_of(1);
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].answer, Answer::Conflict);
        assert_eq!(intents[1].answer, Answer::Committed);
    })
    .await;
}

/// **(E)** A fresh chunk's `sidx:` key already holding bytes no batch of this attempt put is a
/// typed error, never adopted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_chunk_id_already_taken_is_a_typed_error() {
    within(async {
        let mut rig = Rig::new().await;
        let key = sidx_key(&rig.upload, part(), 1);
        rig.store
            .inner
            .commit(WriteBatch::new().put(key.clone(), b"someone else".to_vec()))
            .await
            .unwrap();
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(&cause, Stop::ChunkIdTaken { key: named } if named.as_bytes() == key.as_slice()),
            "{cause:?}"
        );
        assert!(rig.log.puts().is_empty());
    })
    .await;
}

/// **(E)** Chunk 1's intent is held, and lands **after** its entry read found nothing and
/// **before** its slot read. The slot then holds the value only that batch puts, so the intent
/// reads its entry again and adopts the batch: no retry, no `SlotLost`, its fragments go to the
/// placement that landed under its own `t_auth`, and chunk 2's intent requires the slot that
/// batch put.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store
            .script(Kind::Intent, vec![Act::Hold { advance: 0 }], None);
        let entry = sidx_key(&rig.upload, part(), 1);
        rig.store.after_read(&entry, 0, vec![Then::ReleaseHeld]);
        let body = body_bytes(2 * CHUNK);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(pieces(&body, &[2 * CHUNK])))
                .await,
        );
        let intents = rig.log.intents_of(1);
        assert_eq!(intents.len(), 1, "adopted, not re-sent");
        let held = intent_put(&intents[0].batch);
        assert_eq!(rig.store.inner.get(&held.0).await.unwrap(), Some(held.1));
        assert_eq!(
            attempt.chunks()[0].placement,
            intent_entry(&intents[0].batch).staged().placement()
        );
        for put in rig.log.puts().iter().filter(|p| p.id.chunk == 1) {
            assert_eq!(
                put.deadline,
                Some(intents[0].clock + W),
                "the held batch's t_auth"
            );
        }
        let held_slot = intents[0]
            .batch
            .puts
            .iter()
            .find(|(key, _)| key.starts_with(SLOT_PREFIX))
            .unwrap()
            .1
            .clone();
        assert_eq!(
            slot_required(&rig.log.intents_of(2)[0].batch),
            held_slot,
            "chunk 2 is fenced on the slot the adopted batch put"
        );
        assert_intents_precede_puts(&rig.log);
        assert_fragments_evidenced(&rig).await;
        assert_eq!(owned(&rig.store.inner, &rig.upload).await.len(), 2);
    })
    .await;
}

/// The script for an intent or renewal loop whose rounds all conflict but the last, which is held.
fn held_last_round() -> Vec<Act> {
    const { assert!(INTENT_ROUNDS == RENEWAL_ROUNDS) };
    let mut plan = vec![Act::Conflict; INTENT_ROUNDS as usize - 1];
    plan.push(Act::Hold { advance: 0 });
    plan
}

/// **(E)** Chunk 1's intent loses its first rounds and its **last** round is held. That batch
/// lands after its slot read and just before the session moves to a later `Open` epoch, so the
/// session read finds the session moved. The intent reads its entry again before letting the
/// doubt go, adopts the batch that landed, and writes the chunk under the new session — never
/// "rounds spent, nothing in doubt" for a chunk whose entry is durable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_intent_landing_before_the_session_moves_is_adopted() {
    within(async {
        let mut rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        rig.store.script(Kind::Intent, held_last_round(), None);
        let slot = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        let move_session = WriteBatch::new().put(mpu_key(&rig.upload), later.clone());
        // Each lost round reads the slot once; fire after the last round's slot read.
        rig.store.after_read(
            &slot,
            INTENT_ROUNDS as usize - 1,
            vec![Then::ReleaseHeld, Then::Commit(move_session)],
        );
        let attempt = staged(
            rig.stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
                .await,
        );
        let intents = rig.log.intents_of(1);
        assert_eq!(
            intents.len(),
            INTENT_ROUNDS as usize,
            "no round past the bound"
        );
        let landed = intent_put(&intents[INTENT_ROUNDS as usize - 1].batch);
        assert_eq!(
            rig.store.inner.get(&landed.0).await.unwrap(),
            Some(landed.1)
        );
        assert_eq!(attempt.session_bytes(), &later, "fenced on the new session");
        assert_eq!(attempt.chunks().len(), 1);
        assert_intents_precede_puts(&rig.log);
        assert_fragments_evidenced(&rig).await;
    })
    .await;
}

/// **(E)** The renewal analogue: a renewal's last round is held and lands just before the session
/// moves to a later `Open` epoch. The renewal reads its slot again before letting the doubt go,
/// adopts that round, and staging goes on under the new session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_renewal_landing_before_the_session_moves_is_adopted() {
    within(async {
        let mut rig = Rig::new().await;
        let later = open_at(&rig.session, 2);
        rig.store.script(Kind::Renewal, held_last_round(), None);
        let slot = slot_key(&rig.upload, SlotIndex::new(0).unwrap());
        let move_session = WriteBatch::new().put(mpu_key(&rig.upload), later.clone());
        rig.store.after_read(
            &slot,
            RENEWAL_ROUNDS as usize - 1,
            vec![Then::ReleaseHeld, Then::Commit(move_session)],
        );
        let body_stream = renewal_body(&rig.clock, &rig.log);
        let attempt = staged(rig.stage(|_| {}, body_stream).await);
        let renewals = rig.log.commits(Kind::Renewal);
        assert_eq!(
            renewals.len(),
            RENEWAL_ROUNDS as usize,
            "no round past the bound"
        );
        let landed = slot_put(&renewals[RENEWAL_ROUNDS as usize - 1].batch);
        let intent = &rig.log.commits(Kind::Intent)[0];
        assert_eq!(intent.answer, Answer::Committed);
        assert_eq!(
            decode_slot_record(&slot_required(&intent.batch)).unwrap(),
            landed,
            "the next batch requires the adopted round's slot"
        );
        assert_eq!(attempt.session_bytes(), &later, "fenced on the new session");
    })
    .await;
}

/// **(E)** As in `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted`, but chunk
/// 1's entry is deleted right after its held intent lands, before the intent reads it again. The slot proves the batch landed and the entry is gone: a typed error
/// naming the entry, nothing in doubt, and no fragment of the chunk written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_an_entry_removed_after_its_intent_landed_is_a_typed_error() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store
            .script(Kind::Intent, vec![Act::Hold { advance: 0 }], None);
        let entry = sidx_key(&rig.upload, part(), 1);
        let remove = WriteBatch::new().delete(entry.clone());
        rig.store
            .after_read(&entry, 0, vec![Then::ReleaseHeld, Then::Commit(remove)]);
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let err = match outcome {
            Err(err @ StageError::Stopped { .. }) => err,
            other => panic!("expected Stopped, got {other:?}"),
        };
        let text = err.to_string();
        let named = String::from_utf8(entry.clone()).unwrap();
        for part in [
            named.as_str(),
            "removed or rewritten after its intent landed",
            "no batch of this part attempt is in doubt",
        ] {
            assert!(text.contains(part), "{part:?} in {text:?}");
        }
        let cause = stopped(Err(err));
        assert!(
            matches!(&cause, Stop::EntryLost { key } if key.as_bytes() == entry.as_slice()),
            "{cause:?}"
        );
        assert_eq!(rig.log.commits(Kind::Intent).len(), 1);
        assert!(
            rig.log.puts().is_empty(),
            "no fragment without a durable entry"
        );
    })
    .await;
}

// ═══════════════════════ (F) every loop is bounded ═══════════════════════

/// **(F)** Every reserve answered `Conflict` unapplied: the reserve probes each index once —
/// exactly `MAX_INFLIGHT_PARTS` commits — and refuses `SlowDown`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_the_reserve_probe_is_bounded() {
    within(async {
        let rig = Rig::new().await;
        rig.store.script(Kind::Reserve, vec![], Some(Act::Conflict));
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(outcome, inflight_full());
        let reserves = rig.log.commits(Kind::Reserve);
        assert_eq!(reserves.len(), MAX_INFLIGHT_PARTS as usize);
        let probed: BTreeSet<Vec<u8>> =
            reserves.iter().map(|c| c.batch.puts[0].0.clone()).collect();
        assert_eq!(
            probed.len(),
            MAX_INFLIGHT_PARTS as usize,
            "each probe a different index"
        );
    })
    .await;
}

/// **(F)** A session that moves to a later `Open` epoch under every reserve commit: the probe
/// count alone bounds the loop, at exactly `MAX_INFLIGHT_PARTS` commits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_the_reserve_probe_is_bounded_while_the_session_keeps_moving() {
    within(async {
        let rig = Rig::new().await;
        let epoch = Arc::new(AtomicU64::new(0));
        let (open, key) = (rig.session.clone(), mpu_key(&rig.upload));
        let moving: Hook = Arc::new(move |_| {
            let next = epoch.fetch_add(2, Ordering::SeqCst) + 2;
            WriteBatch::new().put(key.clone(), open_at(&open, next))
        });
        rig.store
            .script(Kind::Reserve, vec![], Some(Act::First(moving)));
        let outcome = reserve_slot(&rig.store, &reserve_request(&rig, &rig.attempt), T0)
            .await
            .unwrap();
        assert_eq!(outcome, inflight_full());
        assert_eq!(
            rig.log.commits(Kind::Reserve).len(),
            MAX_INFLIGHT_PARTS as usize
        );
    })
    .await;
}

/// **(F)** Every intent answered `Conflict` unapplied: exactly [`INTENT_ROUNDS`] commits, then a
/// typed error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_the_intent_loop_is_bounded() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(Kind::Intent, vec![], Some(Act::Conflict));
        let outcome = rig
            .stage(|_| {}, stream_of(vec![Bytes::from(body_bytes(CHUNK))]))
            .await;
        let cause = stopped(outcome);
        assert!(
            matches!(cause, Stop::IntentRoundsSpent { chunk: 1, rounds } if rounds == INTENT_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(rig.log.commits(Kind::Intent).len(), INTENT_ROUNDS as usize);
        assert!(rig.log.puts().is_empty());
    })
    .await;
}

/// **(F)** Every renewal answered `Conflict` unapplied: exactly [`RENEWAL_ROUNDS`] commits, then a
/// typed error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_the_renewal_loop_is_bounded() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(Kind::Renewal, vec![], Some(Act::Conflict));
        let body_stream = renewal_body(&rig.clock, &rig.log);
        let outcome = rig.stage(|_| {}, body_stream).await;
        let cause = stopped(outcome);
        assert!(
            matches!(cause, Stop::RenewalRoundsSpent { rounds } if rounds == RENEWAL_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(
            rig.log.commits(Kind::Renewal).len(),
            RENEWAL_ROUNDS as usize
        );
    })
    .await;
}

/// **(F)** A renewal held (`may_still_commit: true`) and then answered `Conflict` until its
/// rounds run out ends in doubt, keeping `may_still_commit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_held_renewal_that_never_settles_ends_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store.script(
            Kind::Renewal,
            vec![Act::Hold { advance: 0 }],
            Some(Act::Conflict),
        );
        let body_stream = renewal_body(&rig.clock, &rig.log);
        let outcome = rig.stage(|_| {}, body_stream).await;
        let Err(StageError::InDoubt(doubt)) = outcome else {
            panic!("expected InDoubt, got {outcome:?}");
        };
        assert!(doubt.may_still_commit);
        assert!(matches!(doubt.cause, Stop::RenewalRoundsSpent { .. }));
        assert_eq!(
            rig.log.commits(Kind::Renewal).len(),
            RENEWAL_ROUNDS as usize
        );
    })
    .await;
}

// ═══════════════════════ (G) the per-part chunk bound ═══════════════════════

/// **(G)** A body longer than the per-part bound (3 here, a parameter) never registers more than
/// 3 owned entries. Staging stops and reports the overrun, handing the staged attempt back
/// intact for the commit to refuse and compensate: its chunks, entries, length and digest cover
/// exactly the staged bytes. A body of exactly 3 chunks is not an overrun.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_the_per_part_chunk_bound_stops_staging_and_reports_it() {
    within(async {
        let mut rig = Rig::new().await;
        let body = body_bytes(5 * CHUNK);
        let outcome = rig
            .stage(
                |r| r.max_part_chunks = 3,
                stream_of(pieces(&body, &[CHUNK + 1, 4 * CHUNK - 1])),
            )
            .await;
        let attempt = staged(outcome);
        assert!(attempt.chunk_bound_exceeded(), "the overrun is reported");
        assert_eq!(attempt.chunks().len(), 3);
        assert_eq!(attempt.owned().len(), 3);
        assert_eq!(
            owned(&rig.store.inner, &rig.upload).await.len(),
            3,
            "never more than 3"
        );
        assert_eq!(attempt.len(), 3 * CHUNK as u64);
        assert_eq!(*attempt.digest(), sha256(&body[..3 * CHUNK]));
        assert_eq!(rig.log.commits(Kind::Intent).len(), 3);
        assert_fragments_evidenced(&rig).await;

        let mut rig = Rig::new().await;
        let body = body_bytes(3 * CHUNK);
        let attempt = staged(
            rig.stage(
                |r| r.max_part_chunks = 3,
                stream_of(pieces(&body, &[3 * CHUNK])),
            )
            .await,
        );
        assert!(
            !attempt.chunk_bound_exceeded(),
            "exactly the bound is not an overrun"
        );
        assert_eq!(attempt.chunks().len(), 3);
        assert_eq!(*attempt.digest(), sha256(&body));
    })
    .await;
}

// ═══════════════════════ request validation ═══════════════════════

/// A zero chunk size, or a TTL with no renewal instant inside it, is refused before anything is
/// read or written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invalid_request_writes_nothing() {
    within(async {
        let mut rig = Rig::new().await;
        let before = snapshot(&rig.store.inner).await;
        for outcome in [
            rig.stage(
                |r| r.chunk_size = 0,
                stream_of(vec![Bytes::from_static(b"x")]),
            )
            .await,
            rig.stage(
                |r| r.lease_ttl_millis = 1,
                stream_of(vec![Bytes::from_static(b"x")]),
            )
            .await,
        ] {
            let cause = stopped(outcome);
            assert!(matches!(cause, Stop::InvalidRequest { .. }), "{cause:?}");
        }
        let staged_two = rig
            .stage(|r| r.lease_ttl_millis = 2, stream_of(Vec::new()))
            .await;
        assert!(
            matches!(staged_two, Ok(StageOutcome::Staged(_))),
            "a TTL of 2 is valid"
        );
        let after: Vec<_> = snapshot(&rig.store.inner)
            .await
            .into_iter()
            .filter(|(key, _)| !key.starts_with(SLOT_PREFIX))
            .collect();
        assert_eq!(after, before, "the invalid requests wrote nothing");
        assert_eq!(
            rig.log.commits(Kind::Reserve).len(),
            1,
            "only the valid one reserved"
        );
    })
    .await;
}

/// The rig's chunk references point at real fragments: a sanity check that the double stores
/// what it is given at the server it is given.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fleet_double_resolves_fragments_by_placement() {
    within(async {
        let mut rig = Rig::new().await;
        let body = body_bytes(CHUNK);
        let attempt = staged(
            rig.stage(|_| {}, stream_of(vec![Bytes::from(body.clone())]))
                .await,
        );
        let chunk: &ChunkRef = &attempt.chunks()[0];
        let moved = ChunkRef {
            placement: chunk.placement.iter().map(|s| (s + 1) % SERVERS).collect(),
            ..chunk.clone()
        };
        assert!(
            read_chunk_verified(&rig.store.inner, &rig.fleet, &moved)
                .await
                .is_err(),
            "a wrong placement finds nothing"
        );
        assert_eq!(
            read_chunk_verified(&rig.store.inner, &rig.fleet, chunk)
                .await
                .unwrap(),
            body
        );
    })
    .await;
}
