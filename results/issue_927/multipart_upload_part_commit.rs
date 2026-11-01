//! Issue #927 (the second of #657's two slices) — `UploadPart`'s **commit half**,
//! `wyrd_core::multipart::commit::{commit_part, upload_part}`, driven over a real store
//! (`RedbMetadataStore::in_memory()`), a placement-aware chunk-store double, and a store wrapper
//! that logs every batch and read and can answer a batch `Conflict` or `CommitUnknownResult`
//! (applied or not, either flag), hold one and land it later, or run an action before or after a
//! batch or between two reads (proposal 0016's *Part commit* and *Live-session compensation*
//! rows, `0016:660`, `:673`; decision 5, `0016:1584-1653`, `:1720-1733`). Every session is made
//! by Create (#894), every part staged by `stage_part` (#932/#933), and every Abort is a real one
//! (#895).
//!
//! * **(A)** parts committed out of order, the pair `part:`/`psum:` agreeing, the attempt's
//!   staging records gone, and the last renewal before the commit's reads.
//! * **(B)** a lost commit is classified from a re-read, never compensated blind: fenced,
//!   rolled back, a same-part rival, an unknown landed commit, an unknown unlanded re-upload, a
//!   held commit three ways, a rival after an unapplied unknown commit, and a held commit beside
//!   an unknown empty compensation.
//! * **(C)** the counterexamples of #657's first attempt (C1 empty parts, C2 torn reads, C3 and
//!   C3′ a landed commit then a rival), C4 empty obligations, the compensation's own unknown
//!   outcomes, and every loop's bound.
//! * **(D)** re-upload, the per-part bound, and the cumulative ceiling's bounded overshoot.
//! * **(F)** every commit round is dispatched with the margin left, and a late landing is safe.
//! * **(G)** a settled stop under a live session compensates, and the others do not.
//! * **(H)** a held commit under a rollback, a held compensation under a teardown, the obligation
//!   re-read, the epoch chase's real bound, and the class of a composed error.
//! * **The sign-off's legs** (#927, attempt 2): a damaged `part:` record is never a rival; a rival's
//!   record over an empty prior settles nothing; the ceiling leaves out the part's own prior; a
//!   taken obligation key stops a re-upload; the kept doubt is the one that may still land; the
//!   epoch chase re-reads every obligation it sent; and a slot gone under a fenced or newer session.
//! * **The sign-off's legs** (#927, attempt 3): a damaged session settles nothing, under a held
//!   commit and a held compensation, including damage repaired mid-round; a prior part gone under
//!   the same session is damage; rounds all out of flight compensate; a slot rewritten under the
//!   attempt's own id during a held commit; and the session-gone row.
//! * **The sign-off's legs** (#927, attempt 5): a changed prior settles nothing whether it was
//!   empty or not, under a same-number rival and under a repair that moves one of its fragments
//!   away and back; a store fault while a held compensation is settled; a re-upload's obligation
//!   under the live epoch; and a damaged `part:` record under a session that moved.
//! * **The sign-off's legs** (#927, attempt 6): a non-`Open` session record at an epoch that has
//!   not risen is damage, never a fence this attempt lost to, under a held commit, a held
//!   compensation, damage repaired mid-round, and with nothing in doubt.
//! * **The sign-off's legs** (#927, attempt 7): a missing `mpu:` record is a terminal delete only
//!   with the slot gone too, else damage that keeps the doubt and lands a held batch once restored;
//!   the quiesce renewal's adoption is rejected when the epoch has not risen; held damage at a
//!   re-read keeps the doubt; and the slot-bytes guard reads the owner it recorded, not the batch's
//!   own bytes.
//!
//! On **every** batch of every leg the wrapper checks that a batch naming an attempt's `slot:`
//! key requires exactly the bytes that attempt last wrote there, recorded from that attempt's own
//! batches as they landed; that a part-commit batch is sent with the dispatch margin left on the
//! lease it requires; and that every value a batch puts decodes back unchanged.
//!
//! The wrapper keeps its state in struct fields, never in a `static` (ADR-0035).

#![forbid(unsafe_code)]

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{self, Stream};
use sha2::{Digest as _, Sha256};
use tokio::sync::Semaphore;
use wyrd_core::metadata::{self, EcScheme, InodeId, SegmentNonce};
use wyrd_core::multipart::abort::abort_session;
use wyrd_core::multipart::commit::{
    commit_part, upload_part, CommitError, DoubtfulBatch, InDoubt, Stop, UploadPartError,
};
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::stage::{
    reserve_slot, stage_part, StageClock, StageError, StageOutcome, StageRequest, StageRuntime,
    StagedAttempt, Stop as StageStop,
};
use wyrd_core::multipart::{
    decode_owned_entry, decode_part_record, decode_part_summary, decode_retire_obligation,
    decode_session_record, decode_slot_record, mpu_key, parse_sidx_key, part_key, psum_key,
    retire_key, sidx_range, slot_key, slot_range, AbortOutcome, AttemptId, Budget, CreateOutcome,
    Digest, PartNumber, PartRecord, PartSummary, RecordError, Refusal, ReserveOutcome, RetireMode,
    RetireToken, SessionState, UploadId, UploadPartOutcome, MAX_INFLIGHT_PARTS, MAX_PART_CHUNKS,
    MAX_STAGED_CHUNKS, PART_PREFIX, PSUM_PREFIX, RETIRE_BYTES_PREFIX, SIDX_PREFIX, SLOT_PREFIX,
};
use wyrd_core::placement::Topology;
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    classify, BoxError, ChunkId, ChunkStore, CommitOutcome, CommitUnknownResult, DServerId,
    ErrorClass, FragmentId, Health, MetadataStore, PlacementChunkStore, Result as StoreResult,
    ScanPage, WriteBatch, WriteDeadlineExpired,
};

// ─────────────────────────────── fixtures ───────────────────────────────

const BUCKET: &str = "photos";
/// The root inode every Alpha object dirent lives under, as `<bucket>/<key>` (ADR-0046).
const ROOT: InodeId = 0;
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
/// No leg may hang the suite: a leg runs in milliseconds.
const LEG_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a driver waits for a commit to reach a point it is waiting for.
const WAIT_FOR: Duration = Duration::from_secs(10);
/// More commits than any one leg may make. A loop past it never stops committing.
const RUNAWAY_COMMITS: usize = 200;
/// The test clock's first reading. A leg that does not move it stamps everything the same
/// millisecond.
const T0: u64 = 1_000_000;
/// The lease TTL every attempt is staged with: above the dispatch margin, which it must exceed.
const TTL: u64 = 10_000;
/// The lease a part-commit batch must have left when it is sent, written here rather than imported
/// so that changing the production value fails this file.
const MARGIN: u64 = 5_000;
const _: () = assert!(MARGIN < TTL);
/// The stated bound on part-commit batches, written here so that changing it fails this file.
const COMMIT_ROUNDS: u32 = 4;
/// The stated bound on compensation batches, written here so that changing it fails this file.
const COMPENSATION_ROUNDS: u32 = 4;
/// `W_write`.
const W: u64 = 3_000;
/// The body-idle bound: never reached, since no leg moves the clock while staging.
const IDLE: u64 = 100 * TTL;
/// Bytes per chunk.
const CHUNK: usize = 64;
/// Six fragments per chunk.
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };
/// Twelve D servers, each its own failure domain.
const SERVERS: u64 = 12;

/// Run `leg` under [`LEG_TIMEOUT`].
async fn within<T>(leg: impl Future<Output = T>) -> T {
    tokio::time::timeout(LEG_TIMEOUT, leg)
        .await
        .expect("the leg settled in time")
}

/// Wait until `ready` holds, letting the other futures of the leg run in between.
async fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let start = tokio::time::Instant::now();
    while !ready() {
        assert!(start.elapsed() < WAIT_FOR, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// A profile admitting 8 sessions. Create stores it; the commit never reads it.
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

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).unwrap()
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

fn topology() -> Topology {
    let mut topology = Topology::default();
    for id in 0..SERVERS {
        topology.register(id, format!("d{id:02}"));
    }
    topology
}

/// `len` deterministic body bytes, distinct per `seed`.
fn body(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn sha256(bytes: &[u8]) -> Digest {
    Digest::from_bytes(Sha256::digest(bytes).into())
}

type Body = Pin<Box<dyn Stream<Item = StoreResult<Bytes>> + Send>>;

fn stream_of(body: &[u8]) -> Body {
    let pieces = if body.is_empty() {
        Vec::new()
    } else {
        vec![Ok(Bytes::copy_from_slice(body))]
    };
    Box::pin(stream::iter(pieces))
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

/// Which batch a commit is, told apart by what it writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    /// `require_absent(slot:..)` and a slot put.
    Reserve,
    /// `require_absent(sidx:..)`.
    Intent,
    /// Puts the slot and owned entries, no `require_absent`: a renewal.
    Renewal,
    /// Puts a `part:` record.
    Commit,
    /// Deletes a slot and puts no `part:`: the live-session compensation.
    Compensation,
    Other,
}

fn kind_of(batch: &WriteBatch) -> Kind {
    let absent = |prefix: &[u8]| {
        batch
            .preconditions
            .iter()
            .any(|p| p.expected.is_none() && p.key.starts_with(prefix))
    };
    let puts = |prefix: &[u8]| batch.puts.iter().any(|(key, _)| key.starts_with(prefix));
    let deletes = |prefix: &[u8]| batch.deletes.iter().any(|key| key.starts_with(prefix));
    if puts(PART_PREFIX) {
        Kind::Commit
    } else if deletes(SLOT_PREFIX) {
        Kind::Compensation
    } else if absent(SIDX_PREFIX) {
        Kind::Intent
    } else if absent(SLOT_PREFIX) {
        Kind::Reserve
    } else if puts(SLOT_PREFIX) || puts(SIDX_PREFIX) {
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
    /// A batch through the wrapper, logged when it is answered.
    Commit {
        kind: Kind,
        batch: WriteBatch,
        answer: Answer,
    },
    /// A read through the wrapper: a `get`'s key or a `scan`'s prefix.
    Read { key: Vec<u8> },
    /// A batch of `kind` reached its gate.
    Gated { kind: Kind },
}

#[derive(Debug, Clone, Default)]
struct Log(Arc<Mutex<Vec<Event>>>);

impl Log {
    fn push(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }

    fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    /// Every batch of `of`, in order, with its answer.
    fn commits(&self, of: Kind) -> Vec<(WriteBatch, Answer)> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Commit {
                    kind,
                    batch,
                    answer,
                } if kind == of => Some((batch, answer)),
                _ => None,
            })
            .collect()
    }

    /// How many reads came between each two consecutive batches of `of`.
    fn reads_between(&self, of: Kind) -> Vec<usize> {
        let mut gaps = Vec::new();
        let mut reads = None;
        for event in self.events() {
            match event {
                Event::Commit { kind, .. } if kind == of => {
                    gaps.extend(reads.take());
                    reads = Some(0);
                }
                Event::Read { .. } => {
                    if let Some(reads) = reads.as_mut() {
                        *reads += 1;
                    }
                }
                _ => {}
            }
        }
        gaps
    }

    /// How many reads came after the last batch of `of`.
    fn reads_after_last(&self, of: Kind) -> usize {
        self.events()
            .iter()
            .rev()
            .take_while(|event| !matches!(event, Event::Commit { kind, .. } if *kind == of))
            .filter(|event| matches!(event, Event::Read { .. }))
            .count()
    }

    fn gated(&self, of: Kind) -> usize {
        self.events()
            .iter()
            .filter(|event| matches!(event, Event::Gated { kind } if *kind == of))
            .count()
    }
}

// ──────────────────────── the scripted store wrapper ────────────────────────

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
/// Something the wrapper runs on the inner store beside a batch or a read.
type Action = Arc<dyn Fn() -> BoxFuture + Send + Sync>;

/// An action that runs `future` the one time it is called.
fn once(future: impl Future<Output = ()> + Send + 'static) -> Action {
    let slot: Mutex<Option<BoxFuture>> = Mutex::new(Some(Box::pin(future)));
    Arc::new(move || slot.lock().unwrap().take().expect("an action runs once"))
}

/// One thing the wrapper does beside a batch or a read.
#[derive(Clone)]
enum Step {
    /// Run an action on the inner store.
    Run(Action),
    /// Apply the oldest held batch, which must land.
    ReleaseHeld,
    /// Move the test clock forward.
    Advance(u64),
}

/// How the wrapper answers one batch.
#[derive(Clone)]
enum How {
    /// Delegate unchanged.
    Pass,
    /// Answer `Conflict`, applying nothing (TiKV's lock conflict).
    Conflict,
    /// Apply the batch or not, then answer `CommitUnknownResult { may_still_commit }`.
    Unknown {
        applied: bool,
        may_still_commit: bool,
    },
    /// Keep the batch unapplied, answer `CommitUnknownResult { may_still_commit: true }`, and
    /// land it when the test releases it.
    Hold,
    /// Wait for a permit on the gate, then delegate.
    Gate(Arc<Semaphore>),
    /// Fail definitely, applying nothing.
    Fail,
}

#[derive(Clone)]
struct Act {
    before: Vec<Step>,
    how: How,
    after: Vec<Step>,
}

impl Act {
    fn new(how: How) -> Self {
        Self {
            before: Vec::new(),
            how,
            after: Vec::new(),
        }
    }

    fn pass() -> Self {
        Self::new(How::Pass)
    }

    fn conflict() -> Self {
        Self::new(How::Conflict)
    }

    fn unknown(applied: bool, may_still_commit: bool) -> Self {
        Self::new(How::Unknown {
            applied,
            may_still_commit,
        })
    }

    fn hold() -> Self {
        Self::new(How::Hold)
    }

    fn fail() -> Self {
        Self::new(How::Fail)
    }

    fn before(mut self, step: Step) -> Self {
        self.before.push(step);
        self
    }

    fn after(mut self, step: Step) -> Self {
        self.after.push(step);
        self
    }
}

#[derive(Default)]
struct ScriptState {
    plans: HashMap<Kind, (VecDeque<Act>, Option<Act>)>,
    held: VecDeque<WriteBatch>,
    made: usize,
    /// Reads answered since the first `Commit` batch was answered; `None` before it.
    reads_after_commit: Option<usize>,
    /// Steps to run after the read whose count since the first commit reaches the number.
    read_hooks: Vec<(usize, Vec<Step>)>,
    /// Counts of reads since the first commit that answer a store fault.
    read_faults: Vec<usize>,
    /// How far each read through the wrapper moves the test clock.
    read_tick: u64,
    /// Steps to run after the next `scan`: the commit's cumulative read.
    scan_steps: Vec<Step>,
    /// The slot bytes each attempt last wrote at each slot key, from its batches that landed.
    last_slot: HashMap<(Vec<u8>, AttemptId), Bytes>,
    /// The attempt that owns each slot key, recorded from each `slot:` put that landed —
    /// independently of the batch [`Scripted::check_slot_bytes`] is checking. A reserve or a
    /// renewal puts its own attempt's id, and a re-reserve of a freed index overwrites it, so this
    /// is always the attempt a conforming batch naming the key must require.
    slot_owner: HashMap<Vec<u8>, AttemptId>,
    /// Every batch that landed through the wrapper, held ones released included.
    landed: Vec<WriteBatch>,
}

struct Scripted {
    inner: Arc<RedbMetadataStore>,
    clock: ManualClock,
    log: Log,
    state: Mutex<ScriptState>,
}

/// Every value a batch puts decodes back unchanged: a slot, an owned entry, a part, a summary,
/// a retirement obligation (decoded against its own key, so an empty `{chunks}` is refused here).
fn assert_values_decode(batch: &WriteBatch) {
    for (key, value) in &batch.puts {
        if key.starts_with(SLOT_PREFIX) {
            let record = decode_slot_record(value).expect("every slot value decodes");
            assert_eq!(metadata::encode(&record), *value);
        } else if key.starts_with(SIDX_PREFIX) {
            decode_owned_entry(key, value).expect("every owned entry decodes against its key");
        } else if key.starts_with(PART_PREFIX) {
            let record = decode_part_record(value).expect("every part record decodes");
            assert_eq!(
                metadata::encode(&record),
                *value,
                "a part re-encodes unchanged"
            );
        } else if key.starts_with(PSUM_PREFIX) {
            let summary = decode_part_summary(value).expect("every summary decodes");
            assert_eq!(metadata::encode(&summary), *value);
        } else if key.starts_with(RETIRE_BYTES_PREFIX) {
            let (mode, _, _) = decode_retire_obligation(key, value)
                .expect("every obligation decodes against its own key");
            assert_eq!(mode, RetireMode::Bytes);
        }
    }
}

impl Scripted {
    /// `kind`'s batches follow `plan`, then `then` (`Pass` when unset).
    fn script(&self, kind: Kind, plan: Vec<Act>, then: Option<Act>) {
        self.state
            .lock()
            .unwrap()
            .plans
            .insert(kind, (plan.into(), then));
    }

    /// After the `n`-th read that follows the first `Commit` batch's answer, run `steps`.
    fn after_read(&self, n: usize, steps: Vec<Step>) {
        self.state.lock().unwrap().read_hooks.push((n, steps));
    }

    /// The `n`-th read that follows the first `Commit` batch's answer faults.
    fn fail_read(&self, n: usize) {
        self.state.lock().unwrap().read_faults.push(n);
    }

    /// Every read through the wrapper from now on moves the clock by `millis`.
    fn tick_reads(&self, millis: u64) {
        self.state.lock().unwrap().read_tick = millis;
    }

    /// After the next `scan`, run `steps`.
    fn after_scan(&self, steps: Vec<Step>) {
        self.state.lock().unwrap().scan_steps = steps;
    }

    /// Whether the read about to happen is one [`Self::fail_read`] named.
    fn read_faults(&self) -> bool {
        let state = self.state.lock().unwrap();
        let next = state.reads_after_commit.map(|count| count + 1);
        next.is_some_and(|next| state.read_faults.contains(&next))
    }

    fn held_count(&self) -> usize {
        self.state.lock().unwrap().held.len()
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
        let outcome = self.inner.commit(held.clone()).await.unwrap();
        if outcome == CommitOutcome::Committed {
            self.landed(&held);
        }
        outcome
    }

    /// Record a batch that landed: the slot bytes each attempt wrote, and the attempt that owns
    /// each slot key.
    fn landed(&self, batch: &WriteBatch) {
        let mut state = self.state.lock().unwrap();
        for (key, value) in &batch.puts {
            if key.starts_with(SLOT_PREFIX) {
                let attempt = decode_slot_record(value).unwrap().attempt_id().clone();
                state.slot_owner.insert(key.clone(), attempt.clone());
                state
                    .last_slot
                    .insert((key.clone(), attempt), value.clone());
            }
        }
        state.landed.push(batch.clone());
    }

    /// **The slot-bytes invariant**, on every batch: a batch that names an attempt's slot key
    /// requires exactly the bytes **that attempt** last wrote there, and deletes a slot only under
    /// such a requirement. The expected identity is the owner the wrapper recorded for the key
    /// (from the reserve/renewal puts that landed), **not** the id read back out of the batch's own
    /// `expected` bytes: a batch carrying another recorded attempt's bytes is caught by comparing
    /// the claimed id to that independently-recorded owner before the by-attempt lookup.
    fn check_slot_bytes(&self, batch: &WriteBatch) -> Result<(), String> {
        let state = self.state.lock().unwrap();
        for p in &batch.preconditions {
            let (true, Some(required)) = (p.key.starts_with(SLOT_PREFIX), &p.expected) else {
                continue;
            };
            let claimed = decode_slot_record(required).unwrap().attempt_id().clone();
            let owner = state.slot_owner.get(&p.key);
            if owner != Some(&claimed) {
                return Err(format!(
                    "a batch requires slot {} carrying attempt {claimed}'s bytes, but {owner:?} \
                     reserved it",
                    String::from_utf8_lossy(&p.key)
                ));
            }
            if state.last_slot.get(&(p.key.clone(), claimed.clone())) != Some(required) {
                return Err(format!(
                    "a batch of attempt {claimed} requires {} at bytes it did not last write",
                    String::from_utf8_lossy(&p.key)
                ));
            }
        }
        for key in &batch.deletes {
            if key.starts_with(SLOT_PREFIX)
                && !batch
                    .preconditions
                    .iter()
                    .any(|p| p.key == *key && p.expected.is_some())
            {
                return Err("a slot is deleted only under a requirement on its bytes".into());
            }
        }
        Ok(())
    }

    /// **The dispatch margin**, on every part-commit batch of every leg: the lease it requires, the
    /// one that attempt last wrote, has at least [`MARGIN`] left at the moment it is sent.
    fn check_margin(&self, batch: &WriteBatch) {
        let now = self.clock.now_millis();
        for p in &batch.preconditions {
            let (true, Some(required)) = (p.key.starts_with(SLOT_PREFIX), &p.expected) else {
                continue;
            };
            let lease = decode_slot_record(required).unwrap().lease_expiry_millis();
            assert!(
                lease.saturating_sub(now) >= MARGIN,
                "a part commit was sent with {} ms left on its lease",
                lease.saturating_sub(now)
            );
        }
    }

    /// How many landed batches deleted `key`.
    fn releases(&self, key: &[u8]) -> usize {
        self.state
            .lock()
            .unwrap()
            .landed
            .iter()
            .filter(|batch| batch.deletes.iter().any(|deleted| deleted == key))
            .count()
    }

    async fn run(&self, steps: &[Step]) {
        for step in steps {
            match step {
                Step::Run(action) => action().await,
                Step::ReleaseHeld => {
                    assert_eq!(self.release_held().await, CommitOutcome::Committed);
                }
                Step::Advance(millis) => self.clock.advance(*millis),
            }
        }
    }

    /// A read through the wrapper returned: move the clock, and fire the hook it is due for.
    async fn on_read(&self) {
        let due = {
            let mut state = self.state.lock().unwrap();
            self.clock.advance(state.read_tick);
            let Some(count) = state.reads_after_commit.as_mut() else {
                return;
            };
            *count += 1;
            let count = *count;
            let (due, keep): (Vec<_>, Vec<_>) =
                state.read_hooks.drain(..).partition(|(n, _)| *n == count);
            state.read_hooks = keep;
            due
        };
        for (_, steps) in due {
            self.run(&steps).await;
        }
    }
}

#[async_trait]
impl MetadataStore for Scripted {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        let found = if self.read_faults() {
            Err("scripted read fault".into())
        } else {
            self.inner.get(key).await
        };
        self.log.push(Event::Read { key: key.to_vec() });
        self.on_read().await;
        found
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        let found = self.inner.scan(prefix).await;
        self.log.push(Event::Read {
            key: prefix.to_vec(),
        });
        self.on_read().await;
        let steps = std::mem::take(&mut self.state.lock().unwrap().scan_steps);
        self.run(&steps).await;
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
        self.check_slot_bytes(&batch)
            .expect("the slot-bytes invariant");
        if kind == Kind::Commit {
            self.check_margin(&batch);
        }
        let act = {
            let mut state = self.state.lock().unwrap();
            state.made += 1;
            assert!(
                state.made <= RUNAWAY_COMMITS,
                "the commit never stopped committing ({} batches)",
                state.made
            );
            match state.plans.get_mut(&kind) {
                Some((plan, then)) => plan.pop_front().or_else(|| then.clone()),
                None => None,
            }
            .unwrap_or_else(Act::pass)
        };
        self.run(&act.before).await;
        let result = match act.how {
            How::Pass => {
                let result = self.inner.commit(batch.clone()).await;
                if matches!(result, Ok(CommitOutcome::Committed)) {
                    self.landed(&batch);
                }
                result
            }
            How::Conflict => {
                tokio::task::yield_now().await;
                Ok(CommitOutcome::Conflict)
            }
            How::Unknown {
                applied,
                may_still_commit,
            } => {
                if applied {
                    let landed = self.inner.commit(batch.clone()).await?;
                    assert_eq!(landed, CommitOutcome::Committed, "the scripted batch lands");
                    self.landed(&batch);
                }
                Err(unknown(may_still_commit))
            }
            How::Hold => {
                self.state.lock().unwrap().held.push_back(batch.clone());
                Err(unknown(true))
            }
            How::Fail => Err("scripted store fault".into()),
            How::Gate(gate) => {
                self.log.push(Event::Gated { kind });
                gate.acquire().await.expect("the gate stays open").forget();
                let result = self.inner.commit(batch.clone()).await;
                if matches!(result, Ok(CommitOutcome::Committed)) {
                    self.landed(&batch);
                }
                result
            }
        };
        self.run(&act.after).await;
        self.log.push(Event::Commit {
            kind,
            batch,
            answer: answer_of(&result),
        });
        if kind == Kind::Commit {
            self.state
                .lock()
                .unwrap()
                .reads_after_commit
                .get_or_insert(0);
        }
        result
    }
}

// ──────────────────────── the chunk-store double ────────────────────────

/// In-process D servers addressed by stable id, enforcing a put's deadline at its publication
/// point as `ChunkStore::put_fragment` requires.
#[derive(Clone)]
struct Fleet(Arc<FleetState>);

struct FleetState {
    servers: Mutex<HashMap<DServerId, HashMap<FragmentId, Bytes>>>,
    clock: ManualClock,
}

impl Fleet {
    fn new(clock: &ManualClock) -> Self {
        Self(Arc::new(FleetState {
            servers: Mutex::new((0..SERVERS).map(|id| (id, HashMap::new())).collect()),
            clock: clock.clone(),
        }))
    }

    /// Every chunk id a fragment of which is stored anywhere.
    fn chunks(&self) -> BTreeSet<ChunkId> {
        self.0
            .servers
            .lock()
            .unwrap()
            .values()
            .flat_map(|held| held.keys().map(|id| id.chunk).collect::<Vec<_>>())
            .collect()
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
        Err("Fleet: a staged write must address a D server".into())
    }

    async fn get_fragment(&self, _id: FragmentId) -> StoreResult<Option<Bytes>> {
        Ok(None)
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        Ok(self
            .0
            .servers
            .lock()
            .unwrap()
            .values()
            .flat_map(|held| held.keys().copied().collect::<Vec<_>>())
            .collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> StoreResult<()> {
        for held in self.0.servers.lock().unwrap().values_mut() {
            held.remove(&id);
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
            .0
            .servers
            .lock()
            .unwrap()
            .get(&dserver)
            .and_then(|held| held.get(&id).cloned()))
    }

    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        fragment: Bytes,
        deadline_millis: Option<u64>,
    ) -> StoreResult<()> {
        if let Some(deadline) = deadline_millis {
            if let Some(refused) =
                WriteDeadlineExpired::if_elapsed(id, deadline, self.0.clock.now_millis())
            {
                return Err(Box::new(refused));
            }
        }
        self.0
            .servers
            .lock()
            .unwrap()
            .get_mut(&dserver)
            .ok_or_else(|| format!("Fleet: unknown D server {dserver}"))?
            .insert(id, fragment);
        Ok(())
    }
}

// ─────────────────────────────── the runtime ───────────────────────────────

/// The clock every attempt is staged and committed on, and a timer that never fires: no leg moves
/// the clock while staging, so no renewal falls due and no staging bound is reached.
struct Runtime {
    clock: ManualClock,
    next_chunk: Arc<AtomicU64>,
}

impl StageClock for Runtime {
    fn now_millis(&self) -> u64 {
        self.clock.now_millis()
    }
}

impl StageRuntime for Runtime {
    fn next_chunk_id(&mut self) -> ChunkId {
        ChunkId::from(self.next_chunk.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn wait(&self, _millis: u64) -> impl Future<Output = ()> + Send {
        std::future::pending()
    }
}

/// The same clock, for a commit run by an action.
struct At(ManualClock);

impl StageClock for At {
    fn now_millis(&self) -> u64 {
        self.0.now_millis()
    }
}

// ─────────────────────────────── the rig ───────────────────────────────

/// Which store an attempt's batches go through: the wrapper, which logs and checks them, or the
/// inner store directly (a rival the wrapper does not script).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Through {
    Wrapper,
    Inner,
}

struct Rig {
    clock: ManualClock,
    log: Log,
    inner: Arc<RedbMetadataStore>,
    store: Scripted,
    fleet: Fleet,
    runtime: Runtime,
    topology: Topology,
    upload: UploadId,
    next_attempt: u128,
}

impl Rig {
    async fn new() -> Self {
        let clock = ManualClock::new(T0);
        let log = Log::default();
        let inner = Arc::new(RedbMetadataStore::in_memory().unwrap());
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
        let outcome = create_session(&*inner, &profile(), &create, &mut Fixed(Some(identity(1))))
            .await
            .expect("the create ran");
        let CreateOutcome::Created { upload_id } = outcome else {
            panic!("expected Created, got {outcome:?}");
        };
        Self {
            store: Scripted {
                inner: Arc::clone(&inner),
                clock: clock.clone(),
                log: log.clone(),
                state: Mutex::new(ScriptState::default()),
            },
            fleet: Fleet::new(&clock),
            runtime: Runtime {
                clock: clock.clone(),
                next_chunk: Arc::new(AtomicU64::new(0)),
            },
            topology: topology(),
            upload: upload_id,
            next_attempt: 0,
            inner,
            clock,
            log,
        }
    }

    fn attempt_id(&mut self) -> AttemptId {
        self.next_attempt += 1;
        AttemptId::new(format!("{:032x}", 0xa77e_0000_u128 + self.next_attempt)).unwrap()
    }

    /// Stage `bytes` as part `n`, through `through`.
    async fn stage(&mut self, through: Through, n: u32, bytes: &[u8]) -> StagedAttempt {
        self.stage_with(through, n, bytes, |_| {}).await
    }

    async fn stage_with(
        &mut self,
        through: Through,
        n: u32,
        bytes: &[u8],
        tweak: impl FnOnce(&mut StageRequest<'_>),
    ) -> StagedAttempt {
        let attempt = self.attempt_id();
        let mut request = StageRequest {
            upload_id: &self.upload,
            part_number: part(n),
            attempt_id: &attempt,
            max_inflight_parts: MAX_INFLIGHT_PARTS,
            lease_ttl_millis: TTL,
            chunk_size: CHUNK,
            scheme: RS42,
            topology: &self.topology,
            write_window_millis: W,
            max_part_chunks: MAX_PART_CHUNKS,
            body_idle_millis: IDLE,
        };
        tweak(&mut request);
        let body = stream_of(bytes);
        let outcome = match through {
            Through::Wrapper => {
                stage_part(&self.store, &self.fleet, &mut self.runtime, &request, body).await
            }
            Through::Inner => {
                stage_part(&*self.inner, &self.fleet, &mut self.runtime, &request, body).await
            }
        };
        match outcome {
            Ok(StageOutcome::Staged(attempt)) => *attempt,
            other => panic!("expected Staged, got {other:?}"),
        }
    }

    /// Commit `attempt` through the wrapper, at the shipped cumulative ceiling.
    async fn commit(&self, attempt: StagedAttempt) -> Result<UploadPartOutcome, CommitError> {
        commit_part(&self.store, &self.runtime, attempt, MAX_STAGED_CHUNKS).await
    }

    /// Stage and commit part `n` on the inner store: a part some earlier attempt committed.
    async fn committed(&mut self, n: u32, bytes: &[u8]) -> PartRecord {
        let attempt = self.stage(Through::Inner, n, bytes).await;
        let outcome = commit_part(&*self.inner, &self.runtime, attempt, MAX_STAGED_CHUNKS)
            .await
            .expect("the prior commit ran");
        assert!(matches!(outcome, UploadPartOutcome::Committed { .. }));
        self.part(n).await.expect("the prior part is committed").0
    }

    /// A rival's commit of `attempt` (staged [`Through::Inner`]) on the inner store, which must
    /// land.
    fn rival(&self, attempt: StagedAttempt) -> Step {
        let inner = Arc::clone(&self.inner);
        let clock = At(self.clock.clone());
        Step::Run(once(async move {
            let outcome = commit_part(&*inner, &clock, attempt, MAX_STAGED_CHUNKS)
                .await
                .expect("the rival's commit ran");
            assert!(
                matches!(outcome, UploadPartOutcome::Committed { .. }),
                "the rival committed: {outcome:?}"
            );
        }))
    }

    /// A Complete that fenced the session and rolled it back: `Open@E` → `Open@E+2`, written by
    /// the wrapper on the inner store. Runs every time it is called.
    fn rollback(&self) -> Step {
        let inner = Arc::clone(&self.inner);
        let upload = self.upload.clone();
        Step::Run(Arc::new(move || {
            let inner = Arc::clone(&inner);
            let upload = upload.clone();
            Box::pin(async move {
                roll_back(&inner, &upload).await;
            })
        }))
    }

    /// A real Abort (#895) of the session, which must fence it.
    fn abort(&self) -> Step {
        let inner = Arc::clone(&self.inner);
        let upload = self.upload.clone();
        Step::Run(Arc::new(move || {
            let inner = Arc::clone(&inner);
            let upload = upload.clone();
            Box::pin(async move {
                let fenced = abort_session(&*inner, &upload)
                    .await
                    .expect("the abort ran");
                assert_eq!(fenced, AbortOutcome::Fenced);
            })
        }))
    }

    /// The session's `sidx:` entries and `slot:` records deleted, standing in for #659's
    /// teardown.
    fn teardown(&self) -> Step {
        let inner = Arc::clone(&self.inner);
        let upload = self.upload.clone();
        Step::Run(Arc::new(move || {
            let inner = Arc::clone(&inner);
            let upload = upload.clone();
            Box::pin(async move {
                let mut batch = WriteBatch::new();
                for range in [sidx_range(&upload), slot_range(&upload)] {
                    for (key, _) in inner.scan(&range).await.unwrap() {
                        batch = batch.delete(key);
                    }
                }
                assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
            })
        }))
    }

    /// The whole `UploadPart` through the wrapper, at the shipped cumulative ceiling.
    async fn upload(
        &mut self,
        n: u32,
        bytes: &[u8],
        max_part_chunks: u32,
    ) -> Result<UploadPartOutcome, UploadPartError> {
        let attempt = self.attempt_id();
        let request = StageRequest {
            upload_id: &self.upload,
            part_number: part(n),
            attempt_id: &attempt,
            max_inflight_parts: MAX_INFLIGHT_PARTS,
            lease_ttl_millis: TTL,
            chunk_size: CHUNK,
            scheme: RS42,
            topology: &self.topology,
            write_window_millis: W,
            max_part_chunks,
            body_idle_millis: IDLE,
        };
        upload_part(
            &self.store,
            &self.fleet,
            &mut self.runtime,
            &request,
            MAX_STAGED_CHUNKS,
            stream_of(bytes),
        )
        .await
    }

    /// Run the steps now, on the wrapper.
    async fn run(&self, steps: Vec<Step>) {
        self.store.run(&steps).await;
    }

    /// Writes `bytes` at `key` on the inner store: damage, or another writer's value.
    fn plant(&self, key: Vec<u8>, bytes: Bytes) -> Step {
        let inner = Arc::clone(&self.inner);
        Step::Run(once(async move {
            let batch = WriteBatch::new().put(key, bytes);
            assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
        }))
    }

    /// A test-local batch in the idle fence's shape (`0016:1859-1863`; #625 owns the real one):
    /// `require(mpu == Open@E)`, `require(slot == slot_bytes)`, the session moved to `Aborting`.
    /// It must answer `expected`.
    fn fence(
        &self,
        attempt: &StagedAttempt,
        slot_bytes: Arc<Mutex<Option<Bytes>>>,
        expected: CommitOutcome,
    ) -> Step {
        let inner = Arc::clone(&self.inner);
        let upload = self.upload.clone();
        let slot = attempt.slot_key().to_vec();
        Step::Run(once(async move {
            let key = mpu_key(&upload);
            let session = inner.get(&key).await.unwrap().unwrap();
            let record = decode_session_record(&session).unwrap();
            let teardown = record.open_teardown(&upload).unwrap();
            let read = slot_bytes
                .lock()
                .unwrap()
                .clone()
                .expect("the slot was read");
            let batch = WriteBatch::new()
                .require(key.clone(), session)
                .require(slot, read)
                .put(key, metadata::encode(teardown.session()));
            assert_eq!(inner.commit(batch).await.unwrap(), expected, "the fence");
        }))
    }

    /// Reads `attempt`'s slot into `into`.
    fn read_slot(&self, attempt: &StagedAttempt, into: Arc<Mutex<Option<Bytes>>>) -> Step {
        let inner = Arc::clone(&self.inner);
        let slot = attempt.slot_key().to_vec();
        Step::Run(once(async move {
            *into.lock().unwrap() = inner.get(&slot).await.unwrap();
        }))
    }

    /// Asserts that `attempt` compensated once under `epoch`: one obligation for its own chunks
    /// (none for an empty part), no slot, no owned entry, and no part record of its own.
    async fn assert_compensated(&self, attempt: &StagedAttempt, epoch: u64) {
        let expected = if attempt.chunks().is_empty() {
            Vec::new()
        } else {
            vec![(token_of(attempt, epoch), chunk_ids(attempt.chunks()))]
        };
        assert_eq!(self.part_obligations().await, expected);
        assert_eq!(self.slots_of(attempt).await, 0, "no slot of the attempt");
        assert_eq!(self.owned_of(attempt).await, 0, "no owned entry");
        assert_eq!(self.releases(attempt), 1, "released once");
        if let Some((record, _)) = self.part(attempt.part_number().get()).await {
            assert!(
                attempt.chunks().is_empty() || !is_own(&record, attempt),
                "its own part is not committed"
            );
        }
        self.assert_every_fragment_named().await;
    }

    /// Whether the slot read before the first compensation came after the last read of the part
    /// key and of the session, as the rule for settling a held commit requires.
    fn compensation_follows_a_slot_read(&self, attempt: &StagedAttempt) -> bool {
        let events = self.log.events();
        let Some(at) = events.iter().position(|event| {
            matches!(
                event,
                Event::Commit {
                    kind: Kind::Compensation,
                    ..
                }
            )
        }) else {
            return false;
        };
        let read_of = |wanted: &[u8]| {
            events[..at]
                .iter()
                .rposition(|event| matches!(event, Event::Read { key } if key == wanted))
        };
        // Which of the two session reads around it found the change does not matter: only that
        // a slot read follows the part read and the first of them.
        let slot = read_of(attempt.slot_key());
        let part = read_of(&part_key(&self.upload, attempt.part_number()));
        slot.is_some() && slot > part
    }

    async fn session(&self) -> Bytes {
        self.inner
            .get(&mpu_key(&self.upload))
            .await
            .unwrap()
            .expect("the session is stored")
    }

    /// Part `n`'s record and summary, each decoded, if committed; they must agree.
    async fn part(&self, n: u32) -> Option<(PartRecord, PartSummary)> {
        let record = self
            .inner
            .get(&part_key(&self.upload, part(n)))
            .await
            .unwrap();
        let summary = self
            .inner
            .get(&psum_key(&self.upload, part(n)))
            .await
            .unwrap();
        match (record, summary) {
            (None, None) => None,
            (Some(record), Some(summary)) => {
                let record = decode_part_record(&record).unwrap();
                let summary = decode_part_summary(&summary).unwrap();
                assert_eq!(
                    summary.chunks as usize,
                    record.chunks().len(),
                    "chunk count"
                );
                assert_eq!(summary.len, record.len(), "length");
                assert_eq!(summary.digest, *record.digest(), "digest");
                assert_eq!(summary.committed_at_millis, record.committed_at_millis());
                Some((record, summary))
            }
            (record, summary) => panic!("part and summary disagree: {record:?} / {summary:?}"),
        }
    }

    /// Every `retire:bytes:` obligation: its token and the chunk ids it names.
    async fn obligations(&self) -> Vec<(RetireToken, Vec<ChunkId>)> {
        self.inner
            .scan(RETIRE_BYTES_PREFIX)
            .await
            .unwrap()
            .into_iter()
            .map(|(key, value)| {
                let (_, token, payload) = decode_retire_obligation(&key, &value).unwrap();
                (token, payload.chunks().iter().map(|c| c.id).collect())
            })
            .collect()
    }

    /// The `retire:bytes:` obligations a part attempt installed (session-wide ones excluded).
    async fn part_obligations(&self) -> Vec<(RetireToken, Vec<ChunkId>)> {
        self.obligations()
            .await
            .into_iter()
            .filter(|(token, _)| matches!(token, RetireToken::Session { part: Some(_), .. }))
            .collect()
    }

    /// How many slots `attempt` holds.
    async fn slots_of(&self, attempt: &StagedAttempt) -> usize {
        self.inner
            .scan(&slot_range(&self.upload))
            .await
            .unwrap()
            .iter()
            .filter(|(_, value)| {
                decode_slot_record(value).unwrap().attempt_id() == attempt.attempt_id()
            })
            .count()
    }

    /// How many of `attempt`'s owned entries remain.
    async fn owned_of(&self, attempt: &StagedAttempt) -> usize {
        let mut held = 0;
        for owned in attempt.owned() {
            if self.inner.get(owned.key()).await.unwrap().is_some() {
                held += 1;
            }
        }
        held
    }

    /// How many landed batches released `attempt`'s slot.
    fn releases(&self, attempt: &StagedAttempt) -> usize {
        self.store.releases(attempt.slot_key())
    }

    /// **C-1, whole-store**: every chunk a fragment of which the fleet stores is named by an
    /// owned `sidx:` entry, a `part:` record or a `retire:bytes:` obligation.
    async fn assert_every_fragment_named(&self) {
        let mut named = BTreeSet::new();
        for (key, _) in self.inner.scan(&sidx_range(&self.upload)).await.unwrap() {
            named.insert(parse_sidx_key(&key).unwrap().2);
        }
        for (_, value) in self.inner.scan(PART_PREFIX).await.unwrap() {
            for chunk in decode_part_record(&value).unwrap().chunks() {
                named.insert(chunk.id);
            }
        }
        for (_, chunks) in self.obligations().await {
            named.extend(chunks);
        }
        let stored = self.fleet.chunks();
        let unnamed: Vec<_> = stored.difference(&named).collect();
        assert!(
            unnamed.is_empty(),
            "stored chunks no record names: {unnamed:?}"
        );
    }
}

/// `Open@E` → `Open@E+2` (a fence to `Completing@E+1` and its rollback, decision 3), committed
/// under `require` on the bytes read. The record's other fields are kept, so it is the same
/// session at a later epoch.
async fn roll_back(inner: &RedbMetadataStore, upload: &UploadId) -> Bytes {
    let key = mpu_key(upload);
    let bytes = inner.get(&key).await.unwrap().expect("the session exists");
    let record = decode_session_record(&bytes).unwrap();
    assert!(
        matches!(record.state(), SessionState::Open {}),
        "rolled back from Open"
    );
    let text = std::str::from_utf8(&bytes).unwrap();
    let from = format!(
        "\"epoch\":{},\"attempts\":{},",
        record.epoch(),
        record.attempts()
    );
    let to = format!(
        "\"epoch\":{},\"attempts\":{},",
        record.epoch() + 2,
        record.attempts() + 1
    );
    assert_eq!(text.matches(&from).count(), 1);
    let next = Bytes::from(text.replace(&from, &to));
    let rolled = decode_session_record(&next).expect("the rolled-back session decodes");
    assert_eq!(rolled.epoch(), record.epoch() + 2);
    assert!(matches!(rolled.state(), SessionState::Open {}));
    let batch = WriteBatch::new()
        .require(key.clone(), bytes)
        .put(key, next.clone());
    assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
    next
}

fn committed_as(outcome: &Result<UploadPartOutcome, CommitError>, attempt: &StagedAttempt) {
    assert_eq!(
        outcome.as_ref().ok(),
        Some(&UploadPartOutcome::Committed {
            part_number: attempt.part_number(),
            digest: *attempt.digest(),
        }),
        "{outcome:?}"
    );
}

fn refused(outcome: &Result<UploadPartOutcome, CommitError>, refusal: Refusal) {
    assert_eq!(
        outcome.as_ref().ok(),
        Some(&UploadPartOutcome::Refused(refusal)),
        "{outcome:?}"
    );
}

/// The doubt a commit ended in, once it is shown to read `Indeterminate`.
///
/// Its `Display` names the upload id, the batch in doubt, whether it may still be applied, and
/// what stopped the commit, as the peer errors' do (`multipart_abort.rs`, `multipart_create.rs`).
fn in_doubt(outcome: Result<UploadPartOutcome, CommitError>) -> Box<InDoubt> {
    match outcome {
        Err(err @ CommitError::InDoubt(_)) => {
            assert_eq!(classify(&err), ErrorClass::Indeterminate, "{err}");
            let text = err.to_string();
            let CommitError::InDoubt(doubt) = err else {
                unreachable!()
            };
            let batch = match doubt.batch {
                DoubtfulBatch::Commit => "part commit",
                DoubtfulBatch::Compensation => "compensation",
            };
            let may = if doubt.may_still_commit {
                "may still be applied"
            } else {
                "may have been applied"
            };
            for wanted in [
                doubt.upload_id.to_string(),
                "is in doubt".to_owned(),
                batch.to_owned(),
                may.to_owned(),
                doubt.cause.to_string(),
            ] {
                assert!(text.contains(&wanted), "{text:?} names {wanted:?}");
            }
            doubt
        }
        other => panic!("expected InDoubt, got {other:?}"),
    }
}

/// What a stop's message says it left in the store.
fn residue(released: bool) -> &'static str {
    if released {
        "the attempt's slot is released and its chunks are retired"
    } else {
        "this attempt released nothing, and what it staged stays in the store"
    }
}

/// The cause of a commit that stopped with nothing in doubt. Its `Display` names the upload id,
/// the cause, that nothing is in doubt, and what it left; it is never classed `Indeterminate`.
fn stopped(outcome: Result<UploadPartOutcome, CommitError>, released: bool) -> Stop {
    match outcome {
        Err(err @ CommitError::Stopped { .. }) => {
            assert_ne!(classify(&err), ErrorClass::Indeterminate, "{err}");
            assert_eq!(err.slot_released(), released, "{err}");
            let text = err.to_string();
            let CommitError::Stopped {
                upload_id, cause, ..
            } = err
            else {
                unreachable!()
            };
            for wanted in [
                upload_id.to_string(),
                cause.to_string(),
                "no batch of this part attempt is in doubt".to_owned(),
                residue(released).to_owned(),
            ] {
                assert!(text.contains(&wanted), "{text:?} names {wanted:?}");
            }
            cause
        }
        other => panic!("expected Stopped, got {other:?}"),
    }
}

/// What stopped a quiesce renewal that returned an error. Its `Display` names the renewal's error
/// and what it left, and its class is the renewal error's.
fn renewal_stopped(outcome: Result<UploadPartOutcome, CommitError>, released: bool) -> StageError {
    match outcome {
        Err(err @ CommitError::Renewal { .. }) => {
            assert_eq!(err.slot_released(), released, "{err}");
            let text = err.to_string();
            assert!(text.contains(residue(released)), "{text:?}");
            let class = classify(&err);
            let CommitError::Renewal { error, .. } = err else {
                unreachable!()
            };
            assert!(text.contains(&error.to_string()), "{text:?}");
            assert_eq!(class, classify(&error), "the renewal error's class");
            error
        }
        other => panic!("expected a renewal error, got {other:?}"),
    }
}

/// The part record `attempt`'s commit puts: its chunks, under its digest.
fn is_own(record: &PartRecord, attempt: &StagedAttempt) -> bool {
    record.chunks() == attempt.chunks() && record.digest() == attempt.digest()
}

fn chunk_ids(chunks: &[metadata::ChunkRef]) -> Vec<ChunkId> {
    chunks.iter().map(|chunk| chunk.id).collect()
}

/// What the quiesce renewal will leave on `attempt`'s lease if the clock does not move first: a
/// renewal stamps one millisecond later than the lease it replaces when a TTL from now is no later
/// than that (`next_expiry`), and every intent while staging did the same.
fn left_after_renewal(rig: &Rig, attempt: &StagedAttempt) -> u64 {
    attempt.slot().lease_expiry_millis() + 1 - rig.clock.now_millis()
}

/// The token of `attempt`'s per-part obligation under `epoch`.
fn token_of(attempt: &StagedAttempt, epoch: u64) -> RetireToken {
    RetireToken::Session {
        upload_id: attempt.upload_id().clone(),
        epoch,
        part: Some((attempt.part_number(), attempt.attempt_id().clone())),
    }
}

/// The epoch a batch requires the session at.
fn required_epoch(batch: &WriteBatch, upload: &UploadId) -> u64 {
    let session = batch
        .preconditions
        .iter()
        .find(|p| p.key == mpu_key(upload))
        .and_then(|p| p.expected.clone())
        .expect("the batch requires the session");
    decode_session_record(&session).unwrap().epoch()
}

// ─────────────── (A) the commit ───────────────

/// **(A)** Parts 3, 1 and 2, in that order, through the composed `upload_part` (reserve → stage
/// → commit), part 3 three chunks long and part 1 two: each answers `Committed` with its
/// number and the body's digest; its `part:` and `psum:` records decode and agree; the attempt
/// holds no slot and no owned entry afterwards. The log shows each attempt's last renewal before
/// the commit's reads, and no renewal between those reads and the commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_parts_staged_out_of_order_each_commit_with_records_that_agree() {
    within(async {
        let mut rig = Rig::new().await;
        for (n, len, seed) in [(3u32, 3 * CHUNK - 5, 3u8), (1, 2 * CHUNK, 1), (2, 1, 2)] {
            rig.log.clear();
            let bytes = body(len, seed);
            let attempt = rig.attempt_id();
            let request = StageRequest {
                upload_id: &rig.upload,
                part_number: part(n),
                attempt_id: &attempt,
                max_inflight_parts: MAX_INFLIGHT_PARTS,
                lease_ttl_millis: TTL,
                chunk_size: CHUNK,
                scheme: RS42,
                topology: &rig.topology,
                write_window_millis: W,
                max_part_chunks: MAX_PART_CHUNKS,
                body_idle_millis: IDLE,
            };
            let outcome = upload_part(
                &rig.store,
                &rig.fleet,
                &mut rig.runtime,
                &request,
                MAX_STAGED_CHUNKS,
                stream_of(&bytes),
            )
            .await
            .expect("the upload ran");
            assert_eq!(
                outcome,
                UploadPartOutcome::Committed {
                    part_number: part(n),
                    digest: sha256(&bytes),
                }
            );
            let (record, summary) = rig.part(n).await.expect("the part is committed");
            assert_eq!(record.len(), len as u64);
            assert_eq!(record.chunks().len(), len.div_ceil(CHUNK));
            assert_eq!(summary.chunks as usize, len.div_ceil(CHUNK));
            assert_eq!(record.session_epoch(), 0);
            assert_eq!(record.committed_at_millis(), T0);
            // The attempt's staging records are gone, and nothing else of the session's is.
            assert!(rig
                .inner
                .scan(&slot_range(&rig.upload))
                .await
                .unwrap()
                .is_empty());
            assert!(rig
                .inner
                .scan(&sidx_range(&rig.upload))
                .await
                .unwrap()
                .is_empty());
            // Last renewal → the commit's reads → the commit, no renewal in between.
            let events = rig.log.events();
            let commit_at = events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        Event::Commit {
                            kind: Kind::Commit,
                            ..
                        }
                    )
                })
                .expect("a commit was sent");
            let renewal_at = events[..commit_at]
                .iter()
                .rposition(|e| {
                    matches!(
                        e,
                        Event::Commit {
                            kind: Kind::Renewal,
                            ..
                        }
                    )
                })
                .expect("a renewal preceded the commit");
            let first_read = events[renewal_at..commit_at]
                .iter()
                .position(|e| matches!(e, Event::Read { .. }))
                .map(|at| at + renewal_at)
                .expect("the commit read before it committed");
            assert!(
                matches!(&events[first_read], Event::Read { key } if key.starts_with(PSUM_PREFIX))
            );
            assert!(renewal_at < first_read && first_read < commit_at);
            assert!(
                !events[first_read..commit_at].iter().any(|e| matches!(
                    e,
                    Event::Commit {
                        kind: Kind::Renewal,
                        ..
                    }
                )),
                "no renewal between the commit's reads and the commit"
            );
            let (commit, answer) = &rig.log.commits(Kind::Commit)[0];
            assert_eq!(*answer, Answer::Committed);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
            // One batch: the part, its summary, the slot release and every owned delete.
            assert_eq!(commit.puts.len(), 2);
            assert_eq!(
                commit.deletes.len(),
                1 + record.chunks().len(),
                "the slot and one owned entry per chunk"
            );
        }
        rig.assert_every_fragment_named().await;
    })
    .await;
}

// ─────────────── (B) a lost commit is classified ───────────────

/// **(B)** The session fenced to `Aborting` by a real Abort between the last renewal and the
/// commit: `NoSuchUpload`, and nothing more written — no compensation; the slot and the owned
/// entries stay for the reaper.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_fenced_session_answers_no_such_upload_and_writes_nothing() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 1, 9)).await;
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(rig.abort())], None);
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::NoSuchUpload);
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation"
        );
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot is the reaper's");
        assert_eq!(rig.owned_of(&attempt).await, attempt.owned().len());
        assert!(rig.part(1).await.is_none());
        assert!(rig.part_obligations().await.is_empty());
        rig.assert_every_fragment_named().await;

        // The same Abort, landing before the quiesce renewal: the renewal is refused, and no
        // commit is sent.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 1, 9)).await;
        rig.store
            .script(Kind::Renewal, vec![Act::pass().before(rig.abort())], None);
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot is the reaper's");
    })
    .await;
}

/// **(B)** The session rolled back to `Open@E+2` between the last renewal and the commit (a
/// Complete fenced and rolled back): the attempt compensates under `require(mpu == Open@E+2)` —
/// one obligation for its own chunks under the newer epoch, its slot released, its owned entries
/// deleted, in one batch — and answers exactly `OperationAborted`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_rolled_back_session_is_compensated_under_its_newer_epoch() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(2 * CHUNK, 4)).await;
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(rig.rollback())], None);
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        let compensations = rig.log.commits(Kind::Compensation);
        assert_eq!(compensations.len(), 1);
        let (batch, answer) = &compensations[0];
        assert_eq!(*answer, Answer::Committed);
        assert_eq!(required_epoch(batch, &rig.upload), 2, "under Open@E'");
        let session = decode_session_record(&rig.session().await).unwrap();
        assert_eq!(session.epoch(), 2);
        assert!(
            matches!(session.state(), SessionState::Open {}),
            "still Open"
        );
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 2), chunk_ids(attempt.chunks()))]
        );
        assert_eq!(batch.deletes.len(), 1 + attempt.owned().len(), "one batch");
        assert_eq!(rig.slots_of(&attempt).await, 0);
        assert_eq!(rig.releases(&attempt), 1);
        assert_eq!(rig.owned_of(&attempt).await, 0);
        assert!(rig.part(1).await.is_none());
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(B), decided at sign-off:** a rollback **before** the quiesce renewal is adopted by that renewal
/// (`stage.rs`, its session re-read), so the commit runs under the live epoch and may answer
/// `Committed`. The rule above covers a rollback between the last renewal and the commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 221)).await;
        rig.store.script(
            Kind::Renewal,
            vec![Act::conflict().before(rig.rollback())],
            None,
        );
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        let commits = rig.log.commits(Kind::Commit);
        assert_eq!(commits.len(), 1);
        assert_eq!(
            required_epoch(&commits[0].0, &rig.upload),
            2,
            "the live epoch"
        );
        assert_eq!(rig.part(1).await.unwrap().0.session_epoch(), 2);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 0);
    })
    .await;
}

/// **(B)** A same-part rival committed between the loser's reads and its commit: the loser
/// commits one obligation for its **own** chunks (decoding under `decode_retire_obligation`),
/// releases its slot and deletes its owned entries in one batch, and answers exactly
/// `OperationAborted`. The rival's part stands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_same_part_loser_compensates_its_own_chunks() {
    within(async {
        let mut rig = Rig::new().await;
        let loser = rig.stage(Through::Wrapper, 1, &body(CHUNK, 1)).await;
        let winner = rig.stage(Through::Inner, 1, &body(2 * CHUNK, 2)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(winner.clone()))],
            None,
        );
        let outcome = rig.commit(loser.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        let (record, _) = rig.part(1).await.unwrap();
        assert!(is_own(&record, &winner), "the rival's part stands");
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&loser, 0), chunk_ids(loser.chunks()))],
            "one obligation, for the loser's own chunks"
        );
        let compensations = rig.log.commits(Kind::Compensation);
        assert_eq!(compensations.len(), 1);
        assert_eq!(compensations[0].0.deletes.len(), 1 + loser.owned().len());
        assert_eq!(compensations[0].0.puts.len(), 1, "the obligation");
        assert_eq!(rig.slots_of(&loser).await, 0);
        assert_eq!(rig.releases(&loser), 1);
        assert_eq!(rig.owned_of(&loser).await, 0);
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(B)** A commit whose batch lands but answers `CommitUnknownResult` (either flag): the
/// re-read finds its own commit, the answer is `Committed`, nothing is compensated, and the slot
/// is released exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_landed_commit_answered_unknown_is_committed() {
    for may_still_commit in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 5)).await;
            rig.store.script(
                Kind::Commit,
                vec![Act::unknown(true, may_still_commit)],
                None,
            );
            let outcome = rig.commit(attempt.clone()).await;
            committed_as(&outcome, &attempt);
            assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
            assert_eq!(
                rig.log.commits(Kind::Compensation).len(),
                0,
                "no compensation"
            );
            assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
            assert_eq!(rig.releases(&attempt), 1);
            assert_eq!(rig.slots_of(&attempt).await, 0);
            assert!(rig.part_obligations().await.is_empty());
        })
        .await;
    }
}

/// **(B)** A re-upload whose batch did not land (`may_still_commit: false`): the re-read finds
/// the **prior** part, which must not be answered `Committed`; the attempt carries on as for a
/// commit that did not land, and ends with its own part committed and the prior's chunks retired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_an_unlanded_reupload_is_not_committed_on_the_prior_part() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(2 * CHUNK, 7)).await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 8)).await;
        rig.store
            .script(Kind::Commit, vec![Act::unknown(false, false)], None);
        let outcome = rig.commit(attempt.clone()).await;
        committed_as(&outcome, &attempt);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 2, "it committed again");
        assert!(
            is_own(&rig.part(1).await.unwrap().0, &attempt),
            "its own part"
        );
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 0), chunk_ids(prior.chunks()))]
        );
        assert_eq!(rig.releases(&attempt), 1);
        assert_eq!(rig.slots_of(&attempt).await, 0);
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(B)** A held commit (`may_still_commit: true`, kept unapplied by the wrapper): the re-read
/// finds no part, and the attempt neither answers `Committed` nor compensates on it. Three runs:
/// (i) the held batch released before the attempt's next batch; (ii) released after the attempt
/// returned; (iii) every later batch answered `Conflict` until the rounds run out, then released.
/// In every run exactly one of {its own part, its compensation} is durable at the end, the slot
/// is released exactly once, and the answer is `Committed` only when its own part landed; in (iii)
/// it is an in-doubt error keeping `may_still_commit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_held_commit_is_never_compensated_and_at_most_one_lands() {
    for run in ["i", "ii", "iii"] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 3, 6)).await;
            let (plan, then) = match run {
                "i" => (
                    vec![Act::hold(), Act::pass().before(Step::ReleaseHeld)],
                    None,
                ),
                "ii" => (vec![Act::hold()], None),
                _ => (vec![Act::hold()], Some(Act::conflict())),
            };
            rig.store.script(Kind::Commit, plan, then);
            let outcome = rig.commit(attempt.clone()).await;
            assert_eq!(
                rig.log.commits(Kind::Compensation).len(),
                0,
                "run {run}: no compensation while the commit may land"
            );
            match run {
                "i" | "ii" => committed_as(&outcome, &attempt),
                _ => {
                    let doubt = in_doubt(outcome);
                    assert_eq!(doubt.batch, DoubtfulBatch::Commit);
                    assert!(doubt.may_still_commit, "the doubt is kept");
                    assert!(
                        matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
                        "{:?}",
                        doubt.cause
                    );
                    assert_eq!(rig.log.commits(Kind::Commit).len(), COMMIT_ROUNDS as usize);
                    assert_eq!(
                        rig.log.reads_between(Kind::Commit),
                        vec![4, 4, 4],
                        "a retry follows the four reads of one lap, with no lap of reads only"
                    );
                    assert!(rig.part(1).await.is_none(), "nothing landed yet");
                    assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
                }
            }
            // Every commit it sent requires its slot at the bytes the held batch requires, so at
            // most one of them lands.
            let commits = rig.log.commits(Kind::Commit);
            let slot_required = |batch: &WriteBatch| {
                batch
                    .preconditions
                    .iter()
                    .find(|p| p.key == attempt.slot_key())
                    .and_then(|p| p.expected.clone())
                    .expect("the commit requires its slot")
            };
            assert!(commits.len() >= 2, "run {run}: the held commit was retried");
            assert!(
                commits
                    .iter()
                    .all(|(batch, _)| slot_required(batch) == slot_required(&commits[0].0)),
                "run {run}: every commit requires the held batch's slot bytes"
            );
            if rig.store.held_count() > 0 {
                let landed = rig.store.release_held().await;
                let expected = if run == "ii" {
                    CommitOutcome::Conflict
                } else {
                    CommitOutcome::Committed
                };
                assert_eq!(landed, expected, "run {run}");
            }
            assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "run {run}");
            assert!(rig.part_obligations().await.is_empty(), "run {run}");
            assert_eq!(rig.releases(&attempt), 1, "run {run}: released once");
            assert_eq!(rig.slots_of(&attempt).await, 0);
            assert_eq!(rig.owned_of(&attempt).await, 0);
            rig.assert_every_fragment_named().await;
        })
        .await;
    }
}

/// **(B)** A same-number rival plus an unknown outcome, with this attempt's commit **not**
/// applied (one run per flag; for `may_still_commit: true` the wrapper holds the batch): a
/// non-empty rival part then lands, and the re-read finds the rival's record and this attempt's
/// slot still at the bytes it last wrote. The commit required the key absent, which never comes
/// back, so it can never land: never `Committed`; the attempt compensates under its own slot and
/// answers exactly `OperationAborted`. The held batch, released afterwards, conflicts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_rival_after_an_unapplied_unknown_commit_is_compensated() {
    for may_still_commit in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 11)).await;
            let rival = rig.stage(Through::Inner, 1, &body(2 * CHUNK, 12)).await;
            let lost = if may_still_commit {
                Act::hold()
            } else {
                Act::unknown(false, false)
            };
            rig.store.script(
                Kind::Commit,
                vec![lost.after(rig.rival(rival.clone()))],
                None,
            );
            let outcome = rig.commit(attempt.clone()).await;
            refused(&outcome, Refusal::OperationAborted);
            assert!(
                rig.compensation_follows_a_slot_read(&attempt),
                "the slot read that proves it did not land follows the read that found the rival"
            );
            assert!(is_own(&rig.part(1).await.unwrap().0, &rival));
            assert_eq!(
                rig.part_obligations().await,
                vec![(token_of(&attempt, 0), chunk_ids(attempt.chunks()))]
            );
            if may_still_commit {
                assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
            }
            assert!(is_own(&rig.part(1).await.unwrap().0, &rival));
            assert_eq!(rig.releases(&attempt), 1);
            assert_eq!(rig.slots_of(&attempt).await, 0);
            rig.assert_every_fragment_named().await;
        })
        .await;
    }
}

/// **(B)** A held commit, then an unknown **empty** compensation (C1's shape). An empty re-upload
/// of part 1 over an empty part 1, the clock fixed. The wrapper holds the commit, then lands an
/// empty same-number rival in the same millisecond, whose bytes equal the prior's, so the held
/// commit's `require(part == prior)` can still hold. Any compensation batch is met by landing the
/// held commit first and answering it `CommitUnknownResult { may_still_commit: true }` unapplied;
/// any other batch by `Conflict`. The log shows no compensation sent while the commit was held;
/// the answer is never `OperationAborted` (here: the in-doubt error keeping `may_still_commit`);
/// once the held commit is released, the slot is released exactly once and no `retire:` key
/// exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_held_commit_beside_an_unknown_empty_compensation_never_answers_operation_aborted() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &[]).await;
        assert!(prior.is_empty());
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        let rival = rig.stage(Through::Inner, 1, &[]).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::hold().after(rig.rival(rival))],
            Some(Act::conflict()),
        );
        rig.store.script(
            Kind::Compensation,
            vec![],
            Some(Act::unknown(false, true).before(Step::ReleaseHeld)),
        );
        let outcome = rig.commit(attempt.clone()).await;
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation while the commit was held"
        );
        if matches!(outcome, Err(CommitError::InDoubt(_))) {
            let doubt = in_doubt(outcome);
            assert_eq!(doubt.batch, DoubtfulBatch::Commit);
            assert!(doubt.may_still_commit, "the doubt is kept");
        } else {
            committed_as(&outcome, &attempt);
        }
        if rig.store.held_count() > 0 {
            assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        }
        assert_eq!(rig.releases(&attempt), 1);
        assert_eq!(rig.slots_of(&attempt).await, 0);
        assert!(rig.obligations().await.is_empty(), "no retire: key");
    })
    .await;
}

// ─────────────── (C) the counterexamples ───────────────

/// **C1, an empty part looks the same as somebody else's.** The clock fixed, an empty part 1 is
/// committed. An empty re-upload in the same millisecond and epoch commits with
/// `CommitUnknownResult`, **not applied** (one run per flag; the true run held): its bytes equal
/// the prior's, and must not be taken for `Committed`. When the call returns its slot is not held
/// (or covered by the doubt it reports), and the session holds exactly as many slots as live
/// attempts: none. Then the same for a first upload against a same-number rival whose empty part
/// landed in the same millisecond: compensated, `OperationAborted`, no obligation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_an_empty_part_is_never_committed_on_its_bytes() {
    for may_still_commit in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            let prior = rig.committed(1, &[]).await;
            let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
            let lost = if may_still_commit {
                Act::hold()
            } else {
                Act::unknown(false, false)
            };
            rig.store.script(Kind::Commit, vec![lost], None);
            let before = rig.part(1).await.unwrap().0;
            assert_eq!(before, prior);
            let outcome = rig.commit(attempt.clone()).await;
            match &outcome {
                Ok(UploadPartOutcome::Committed { .. }) => {
                    assert_eq!(
                        rig.slots_of(&attempt).await,
                        0,
                        "a Committed attempt holds no slot"
                    );
                }
                Err(CommitError::InDoubt(_)) => {}
                other => panic!("expected Committed or in doubt, got {other:?}"),
            }
            if rig.store.held_count() > 0 {
                rig.store.release_held().await;
            }
            assert_eq!(rig.slots_of(&attempt).await, 0);
            assert!(
                rig.inner
                    .scan(&slot_range(&rig.upload))
                    .await
                    .unwrap()
                    .is_empty(),
                "held slots equal live attempts: none"
            );
            assert_eq!(rig.releases(&attempt), 1);
            assert!(rig.obligations().await.is_empty());
        })
        .await;

        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
            let rival = rig.stage(Through::Inner, 1, &[]).await;
            let lost = if may_still_commit {
                Act::hold()
            } else {
                Act::unknown(false, false)
            };
            rig.store
                .script(Kind::Commit, vec![lost.after(rig.rival(rival))], None);
            let outcome = rig.commit(attempt.clone()).await;
            refused(&outcome, Refusal::OperationAborted);
            assert_eq!(rig.slots_of(&attempt).await, 0);
            if rig.store.held_count() > 0 {
                assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
            }
            assert_eq!(rig.releases(&attempt), 1);
            assert!(rig
                .inner
                .scan(&slot_range(&rig.upload))
                .await
                .unwrap()
                .is_empty());
            assert!(
                rig.obligations().await.is_empty(),
                "an empty part owes nothing"
            );
        })
        .await;
    }
}

/// **C2, a torn read.** The commit is held and never applied. At each gap between the reads the
/// attempt makes to classify it — the commit's answer and the part read, the part read and the
/// first session read, that and the slot read, the slot read and the last session read, and after
/// that — the wrapper runs a real Abort and then deletes the session's `sidx:` entries and `slot:`
/// records (standing in for #659's teardown). No part exists, so the answer is never `Committed`:
/// `NoSuchUpload` or an in-doubt error. The held batch, released afterwards, conflicts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_torn_read_never_answers_committed() {
    for gap in 0..=4usize {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 21)).await;
            let mut held = Act::hold();
            if gap == 0 {
                held = held.after(rig.abort()).after(rig.teardown());
            } else {
                rig.store.after_read(gap, vec![rig.abort(), rig.teardown()]);
            }
            rig.store.script(Kind::Commit, vec![held], None);
            let outcome = rig.commit(attempt.clone()).await;
            if matches!(outcome, Err(CommitError::InDoubt(_))) {
                let doubt = in_doubt(outcome);
                assert_eq!(doubt.batch, DoubtfulBatch::Commit, "gap {gap}");
                // The held commit requires `Open@E`, which the Abort ended for good.
                assert!(!doubt.may_still_commit, "gap {gap}: it can never land now");
            } else {
                refused(&outcome, Refusal::NoSuchUpload);
            }
            assert!(
                !matches!(rig.state_of().await, SessionState::Open {}),
                "gap {gap}: the abort ran"
            );
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "gap {gap}");
            assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
            assert!(rig.part(1).await.is_none(), "gap {gap}: no part exists");
        })
        .await;
    }
}

impl Rig {
    async fn state_of(&self) -> SessionState {
        decode_session_record(&self.session().await)
            .unwrap()
            .state()
            .clone()
    }
}

/// **C3, a landed commit, then a rival before the re-read.** The commit lands but answers
/// `CommitUnknownResult { may_still_commit: false }`; a rival re-upload of the same part number
/// then replaces the part. The answer is `Committed` — the slot vanished while the session stayed
/// `Open@E`, which only this attempt's commit can do — and no compensation is sent.
///
/// **C3′**: as C3, and then a third attempt reserves the index the commit freed and holds it, all
/// before the re-read. The re-read finds the rival's part and a slot at that index carrying
/// another attempt id: still `Committed`, and the third attempt's slot is byte-for-byte unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_a_landed_commit_then_a_rival_is_committed() {
    for reuse in [false, true] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 31)).await;
            let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 32)).await;
            let third = rig.attempt_id();
            let freed = attempt.slot_index();
            let third_slot: Arc<Mutex<Option<Bytes>>> = Arc::default();
            let mut act = Act::unknown(true, false).after(rig.rival(rival.clone()));
            if reuse {
                let inner = Arc::clone(&rig.inner);
                let upload = rig.upload.clone();
                let topology = rig.topology.clone();
                let seen = Arc::clone(&third_slot);
                let third = third.clone();
                act = act.after(Step::Run(once(async move {
                    let request = StageRequest {
                        upload_id: &upload,
                        part_number: part(4),
                        attempt_id: &third,
                        max_inflight_parts: MAX_INFLIGHT_PARTS,
                        lease_ttl_millis: TTL,
                        chunk_size: CHUNK,
                        scheme: RS42,
                        topology: &topology,
                        write_window_millis: W,
                        max_part_chunks: MAX_PART_CHUNKS,
                        body_idle_millis: IDLE,
                    };
                    let reserved = reserve_slot(&*inner, &request, T0).await.unwrap();
                    assert_eq!(
                        reserved,
                        ReserveOutcome::Reserved {
                            index: freed,
                            attempt_id: third.clone()
                        },
                        "the third attempt holds the freed index"
                    );
                    *seen.lock().unwrap() = inner.get(&slot_key(&upload, freed)).await.unwrap();
                })));
            }
            rig.store.script(Kind::Commit, vec![act], None);
            let outcome = rig.commit(attempt.clone()).await;
            committed_as(&outcome, &attempt);
            assert_eq!(rig.log.commits(Kind::Commit).len(), 1);
            assert_eq!(
                rig.log.commits(Kind::Compensation).len(),
                0,
                "no compensation"
            );
            assert!(
                is_own(&rig.part(1).await.unwrap().0, &rival),
                "the rival replaced it"
            );
            assert_eq!(rig.releases(&attempt), 1);
            if reuse {
                let held = third_slot
                    .lock()
                    .unwrap()
                    .clone()
                    .expect("the third reserved");
                assert_eq!(
                    rig.inner.get(&slot_key(&rig.upload, freed)).await.unwrap(),
                    Some(held),
                    "the third attempt's slot is unchanged"
                );
            }
            rig.assert_every_fragment_named().await;
        })
        .await;
    }
}

/// **C4, empty parts and retirement.** An obligation must name a chunk, so none is installed for
/// an empty list, and the slot release and owned deletes ride in the same batch either way:
/// (a) an empty re-upload over an empty prior commits with no `retire:` key; (b) an empty
/// re-upload over a non-empty prior writes one obligation, for the prior's chunks; (c) an empty
/// part losing to a same-number rival compensates with a batch that releases its slot and writes
/// no obligation, answering `OperationAborted` — and that compensation's unknown outcome
/// (landed; not landed with `may_still_commit: false`; held) is settled by its slot and the
/// session alone, each run ending with the slot released once and no `retire:` key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_empty_parts_never_install_an_empty_obligation() {
    within(async {
        // (a)
        let mut rig = Rig::new().await;
        rig.committed(1, &[]).await;
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        assert!(rig.obligations().await.is_empty(), "(a) no retire: key");
        assert_eq!(rig.slots_of(&attempt).await, 0);

        // (b)
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(2 * CHUNK, 41)).await;
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 0), chunk_ids(prior.chunks()))],
            "(b) one obligation, for the prior's chunks"
        );
        rig.assert_every_fragment_named().await;
    })
    .await;
    // (c), and its compensation's three unknown outcomes.
    for comp in [
        None,
        Some((true, false)),
        Some((false, false)),
        Some((false, true)),
    ] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
            let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 42)).await;
            rig.store.script(
                Kind::Commit,
                vec![Act::pass().before(rig.rival(rival))],
                None,
            );
            let plan = match comp {
                None => vec![],
                Some((applied, true)) => {
                    assert!(!applied);
                    vec![Act::hold()]
                }
                Some((applied, false)) => vec![Act::unknown(applied, false)],
            };
            rig.store.script(Kind::Compensation, plan, None);
            let outcome = rig.commit(attempt.clone()).await;
            refused(&outcome, Refusal::OperationAborted);
            let compensations = rig.log.commits(Kind::Compensation);
            assert!(
                compensations.iter().all(|(batch, _)| batch.puts.is_empty()),
                "{comp:?}: an empty compensation puts no obligation"
            );
            if rig.store.held_count() > 0 {
                assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
            }
            assert_eq!(rig.releases(&attempt), 1, "{comp:?}: released once");
            assert_eq!(rig.slots_of(&attempt).await, 0);
            assert!(
                rig.obligations().await.is_empty(),
                "{comp:?}: no retire: key"
            );
        })
        .await;
    }
}

/// **(C) The compensation's own unknown outcome**, for a non-empty part that lost to a rival:
/// landed; not landed with `may_still_commit: false`; held while the retry lands; and held while
/// every later compensation is answered `Conflict`. Each is settled by re-reading its obligation,
/// its slot and the session: every run ends with exactly one obligation and the slot released
/// once, the last only once the held batch is released after an in-doubt error that keeps
/// `may_still_commit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_unknown_compensation_outcome_is_settled_by_a_re_read() {
    for run in ["landed", "lost", "held", "held-then-conflicts"] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(2 * CHUNK, 51)).await;
            let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 52)).await;
            rig.store
                .script(Kind::Commit, vec![Act::pass().before(rig.rival(rival))], None);
            let (plan, then) = match run {
                "landed" => (vec![Act::unknown(true, false)], None),
                "lost" => (vec![Act::unknown(false, false)], None),
                "held" => (vec![Act::hold()], None),
                _ => (vec![Act::hold()], Some(Act::conflict())),
            };
            rig.store.script(Kind::Compensation, plan, then);
            let outcome = rig.commit(attempt.clone()).await;
            if run == "held-then-conflicts" {
                let doubt = in_doubt(outcome);
                assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
                assert!(doubt.may_still_commit, "the doubt is kept");
                assert!(matches!(doubt.cause, Stop::CompensationRoundsSpent { rounds } if rounds == COMPENSATION_ROUNDS));
                assert!(rig.part_obligations().await.is_empty(), "nothing landed yet");
                assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
            } else {
                refused(&outcome, Refusal::OperationAborted);
                if run == "landed" {
                    assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no retry");
                }
                if rig.store.held_count() > 0 {
                    assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
                }
            }
            assert_eq!(
                rig.part_obligations().await,
                vec![(token_of(&attempt, 0), chunk_ids(attempt.chunks()))],
                "{run}: exactly one obligation"
            );
            assert_eq!(rig.releases(&attempt), 1, "{run}: released once");
            assert_eq!(rig.slots_of(&attempt).await, 0);
            assert_eq!(rig.owned_of(&attempt).await, 0);
            rig.assert_every_fragment_named().await;
        })
        .await;
    }
}

/// **(C) Every loop is bounded.** With every batch answered `Conflict` unapplied, each loop ends
/// in a typed error after exactly its stated number of commits, counted against numbers written in
/// this file: the commit rounds then the compensation rounds (G); the commit rounds alone when the
/// compensation lands; the compensation after a rival won; and the epoch chase, every compensation
/// losing to one more rollback, each under a newer epoch than the last.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_every_loop_ends_after_exactly_its_bound() {
    within(async {
        // The commit rounds, then the compensation rounds, each counted.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 61)).await;
        rig.store.script(Kind::Commit, vec![], Some(Act::conflict()));
        rig.store
            .script(Kind::Compensation, vec![], Some(Act::conflict()));
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(cause, Stop::CompensationRoundsSpent { rounds } if rounds == COMPENSATION_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(
            cause.to_string(),
            "all 4 compensations were spent without one landing"
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), COMMIT_ROUNDS as usize);
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            COMPENSATION_ROUNDS as usize
        );
        assert_eq!(rig.slots_of(&attempt).await, 1, "nothing was released");

        // The commit rounds alone: the settled stop's compensation lands.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 61)).await;
        rig.store.script(Kind::Commit, vec![], Some(Act::conflict()));
        let cause = stopped(rig.commit(attempt.clone()).await, true);
        assert!(
            matches!(cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(
            cause.to_string(),
            "all 4 part commits were spent without one landing"
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), COMMIT_ROUNDS as usize);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1);
        rig.assert_compensated(&attempt, 0).await;

        // The compensation.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 62)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 63)).await;
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(rig.rival(rival))], None);
        rig.store
            .script(Kind::Compensation, vec![], Some(Act::conflict()));
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(cause, Stop::CompensationRoundsSpent { rounds } if rounds == COMPENSATION_ROUNDS),
            "{cause:?}"
        );
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            COMPENSATION_ROUNDS as usize
        );

        // The epoch chase.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 64)).await;
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(rig.rollback())], None);
        rig.store.script(
            Kind::Compensation,
            vec![],
            Some(Act::conflict().before(rig.rollback())),
        );
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(cause, Stop::CompensationRoundsSpent { rounds } if rounds == COMPENSATION_ROUNDS),
            "{cause:?}"
        );
        let epochs: Vec<u64> = rig
            .log
            .commits(Kind::Compensation)
            .iter()
            .map(|(batch, _)| required_epoch(batch, &rig.upload))
            .collect();
        assert_eq!(epochs.len(), COMPENSATION_ROUNDS as usize);
        assert!(
            epochs.windows(2).all(|w| w[0] < w[1]),
            "each lap chases a newer epoch: {epochs:?}"
        );
        assert_eq!(
            epochs[0], 2,
            "the first compensation is under the first rollback"
        );
    })
    .await;
}

// ─────────────── (D) re-upload and ceilings ───────────────

/// **(D)** Re-uploading a committed part replaces it under `require(part == prior)` and puts the
/// prior's chunks in one `retire:bytes:` obligation, all in the commit's one batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_reupload_retires_the_prior_chunks_in_the_same_batch() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(2 * CHUNK, 71)).await;
        let prior_bytes = rig
            .inner
            .get(&part_key(&rig.upload, part(1)))
            .await
            .unwrap()
            .unwrap();
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 9, 72)).await;
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        let commits = rig.log.commits(Kind::Commit);
        assert_eq!(commits.len(), 1);
        let batch = &commits[0].0;
        assert!(
            batch
                .preconditions
                .iter()
                .any(|p| p.key == part_key(&rig.upload, part(1))
                    && p.expected.as_ref() == Some(&prior_bytes)),
            "require(part == prior)"
        );
        assert!(
            batch
                .puts
                .iter()
                .any(|(key, _)| key.starts_with(RETIRE_BYTES_PREFIX)),
            "in the same batch"
        );
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 0), chunk_ids(prior.chunks()))]
        );
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(D)** A part whose staging hit the per-part bound (two chunks, a three-chunk body) answers
/// `EntityTooLarge` and compensates what it staged in one batch; no commit is sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_part_past_the_per_part_bound_is_refused_and_compensated() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig
            .stage_with(Through::Wrapper, 1, &body(3 * CHUNK, 81), |r| {
                r.max_part_chunks = 2
            })
            .await;
        assert!(attempt.chunk_bound_exceeded());
        let outcome = rig.commit(attempt.clone()).await;
        refused(
            &outcome,
            Refusal::EntityTooLarge {
                chunks: 3,
                limit: 2,
            },
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0, "no commit");
        let compensations = rig.log.commits(Kind::Compensation);
        assert_eq!(compensations.len(), 1, "one batch");
        assert_eq!(compensations[0].0.deletes.len(), 1 + 2);
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 0), chunk_ids(attempt.chunks()))]
        );
        assert_eq!(rig.slots_of(&attempt).await, 0);
        assert_eq!(rig.owned_of(&attempt).await, 0);
        assert!(rig.part(1).await.is_none());
        rig.assert_every_fragment_named().await;

        // The same body through the composed `UploadPart`.
        let mut rig = Rig::new().await;
        let outcome = rig
            .upload(1, &body(3 * CHUNK, 82), 2)
            .await
            .expect("the upload ran");
        assert_eq!(
            outcome,
            UploadPartOutcome::Refused(Refusal::EntityTooLarge {
                chunks: 3,
                limit: 2,
            })
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0, "no commit");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1);
        assert!(rig.part(1).await.is_none());
        assert_eq!(rig.part_obligations().await.len(), 1);
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(D) The cumulative ceiling is a read-time check, and its race is bounded.** With `psum:`
/// summaries planted one chunk below `MAX_STAGED_CHUNKS`, two attempts on parts 1 and 2, one
/// chunk each, both read that one chunk of headroom: the wrapper gates the first commit until the
/// second has committed, then lets it through. Both commit, and the total is
/// `MAX_STAGED_CHUNKS + 1`, inside the bound `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` allows. A third
/// commit after them is refused `EntityTooLarge` and compensated.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_the_cumulative_ceiling_refuses_and_its_overshoot_is_bounded() {
    within(async {
        let mut rig = Rig::new().await;
        let planted = PartSummary {
            chunks: MAX_STAGED_CHUNKS - 1,
            len: 0,
            digest: sha256(b""),
            committed_at_millis: T0,
        };
        rig.inner
            .commit(WriteBatch::new().put(
                psum_key(&rig.upload, part(9000)),
                metadata::encode(&planted),
            ))
            .await
            .unwrap();
        let first = rig.stage(Through::Wrapper, 1, &body(CHUNK, 91)).await;
        let second = rig.stage(Through::Wrapper, 2, &body(CHUNK, 92)).await;
        let gate = Arc::new(Semaphore::new(0));
        rig.store.script(
            Kind::Commit,
            vec![Act::new(How::Gate(Arc::clone(&gate)))],
            None,
        );
        let (a, b) = tokio::join!(rig.commit(first.clone()), async {
            until("the first commit is gated", || {
                rig.log.gated(Kind::Commit) == 1
            })
            .await;
            let outcome = rig.commit(second.clone()).await;
            gate.add_permits(1);
            outcome
        });
        committed_as(&a, &first);
        committed_as(&b, &second);
        let mut total = 0u64;
        for (_, value) in rig.inner.scan(PSUM_PREFIX).await.unwrap() {
            total += u64::from(decode_part_summary(&value).unwrap().chunks);
        }
        assert_eq!(total, u64::from(MAX_STAGED_CHUNKS) + 1, "the overshoot");
        assert!(
            total
                <= u64::from(MAX_STAGED_CHUNKS)
                    + u64::from(MAX_INFLIGHT_PARTS) * u64::from(MAX_PART_CHUNKS)
        );

        let third = rig.stage(Through::Wrapper, 3, &body(CHUNK, 93)).await;
        let outcome = rig.commit(third.clone()).await;
        refused(
            &outcome,
            Refusal::EntityTooLarge {
                chunks: u64::from(MAX_STAGED_CHUNKS) + 2,
                limit: u64::from(MAX_STAGED_CHUNKS),
            },
        );
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&third, 0), chunk_ids(third.chunks()))]
        );
        assert_eq!(rig.slots_of(&third).await, 0);
        assert!(rig.part(3).await.is_none());
        rig.assert_every_fragment_named().await;
    })
    .await;
}

// ─────────────── (B) the read-gap leg ───────────────

/// What the wrapper does at a read gap while a commit is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum History {
    /// The held commit lands, then a same-number rival replaces it.
    LandedThenRival,
    /// The held commit lands, then a Complete rolls the session back.
    LandedThenRollback,
    /// A same-number rival lands; the held commit does not.
    NotLandedRival,
    /// The session rolls back; the held commit does not land.
    NotLandedRollback,
}

/// **(B) The read-gap leg: a held commit, and a precondition that dies between the reads.** The
/// commit of a non-empty part is held. At **each** gap between the reads the attempt makes to
/// classify it (0 is before the part read, then after it, after the first session read, after the
/// slot read, and after the last session read), the wrapper runs one of four histories.
///
/// Landed, then a rival: no compensation, `Committed`, the slot released once. Landed, then a
/// rollback: no compensation, the in-doubt error, never `OperationAborted`. Not landed, either
/// way: the attempt reaches a slot read made after it saw the change, and only then compensates,
/// answering `OperationAborted`; the held batch, released after it returns, answers `Conflict` and
/// writes nothing. Those last two are there so an attempt cannot pass by never compensating.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read() {
    for history in [
        History::LandedThenRival,
        History::LandedThenRollback,
        History::NotLandedRival,
        History::NotLandedRollback,
    ] {
        for gap in 0..=4usize {
            within(async {
                let at = format!("{history:?} at gap {gap}");
                let mut rig = Rig::new().await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 2, 111)).await;
                let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 112)).await;
                let steps = match history {
                    History::LandedThenRival => vec![Step::ReleaseHeld, rig.rival(rival.clone())],
                    History::LandedThenRollback => vec![Step::ReleaseHeld, rig.rollback()],
                    History::NotLandedRival => vec![rig.rival(rival.clone())],
                    History::NotLandedRollback => vec![rig.rollback()],
                };
                let mut held = Act::hold();
                if gap == 0 {
                    held.after = steps;
                } else {
                    rig.store.after_read(gap, steps);
                }
                rig.store.script(Kind::Commit, vec![held], None);
                let outcome = rig.commit(attempt.clone()).await;
                let compensations = rig.log.commits(Kind::Compensation).len();
                if matches!(
                    history,
                    History::LandedThenRollback | History::NotLandedRollback
                ) {
                    // A session seen moved sends no further commit, since none could land. Only a
                    // rollback after the lap's last read goes unseen until the retry conflicts.
                    let expected = if gap == 4 { 2 } else { 1 };
                    assert_eq!(rig.log.commits(Kind::Commit).len(), expected, "{at}");
                }
                match history {
                    History::LandedThenRival => {
                        assert_eq!(compensations, 0, "{at}");
                        committed_as(&outcome, &attempt);
                        assert_eq!(rig.releases(&attempt), 1, "{at}");
                        assert_eq!(rig.slots_of(&attempt).await, 0, "{at}");
                        assert!(is_own(&rig.part(1).await.unwrap().0, &rival), "{at}");
                    }
                    History::LandedThenRollback => {
                        assert_eq!(compensations, 0, "{at}");
                        let doubt = in_doubt(outcome);
                        assert_eq!(doubt.batch, DoubtfulBatch::Commit, "{at}");
                        assert!(
                            matches!(doubt.cause, Stop::SessionMoved),
                            "{at}: {:?}",
                            doubt.cause
                        );
                        assert_eq!(rig.releases(&attempt), 1, "{at}");
                        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "{at}");
                    }
                    History::NotLandedRival | History::NotLandedRollback => {
                        refused(&outcome, Refusal::OperationAborted);
                        assert_eq!(compensations, 1, "{at}");
                        assert!(rig.compensation_follows_a_slot_read(&attempt), "{at}");
                        assert_eq!(
                            rig.store.release_held().await,
                            CommitOutcome::Conflict,
                            "{at}: the held batch writes nothing"
                        );
                        let epoch = if history == History::NotLandedRollback {
                            2
                        } else {
                            0
                        };
                        rig.assert_compensated(&attempt, epoch).await;
                    }
                }
            })
            .await;
        }
    }
}

// ─────────────── (F) the dispatch margin ───────────────

/// **(F) Boundary.** With exactly the margin left when the round comes to be sent it is sent; with
/// one millisecond less it is not, and the attempt compensates and returns the typed stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 121)).await;
        let spent = left_after_renewal(&rig, &attempt) - MARGIN;
        rig.store.after_scan(vec![Step::Advance(spent)]);
        let outcome = rig.commit(attempt.clone()).await;
        committed_as(&outcome, &attempt);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1);

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 122)).await;
        let spent = left_after_renewal(&rig, &attempt) - MARGIN + 1;
        rig.store.after_scan(vec![Step::Advance(spent)]);
        let expiry = attempt.slot().lease_expiry_millis() + 1;
        let now = T0 + spent;
        let cause = stopped(rig.commit(attempt.clone()).await, true);
        assert!(
            matches!(
                cause,
                Stop::LeaseShort { lease_expiry_millis, now_millis }
                    if lease_expiry_millis == expiry && now_millis == now
            ),
            "{cause:?}"
        );
        assert_eq!(
            cause.to_string(),
            format!(
                "the lease expiring at {expiry} had less than 5000 ms left at {now}, so no commit \
                 round was sent"
            )
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0, "not sent");
        rig.assert_compensated(&attempt, 0).await;
    })
    .await;
}

/// **(F) Used up by the reads.** The summary scan and the part read take the lease below the mark,
/// so no commit is sent at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_lease_used_up_by_the_reads_sends_no_commit() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 123)).await;
        rig.store
            .tick_reads((left_after_renewal(&rig, &attempt) - MARGIN) / 2 + 1);
        let cause = stopped(rig.commit(attempt.clone()).await, true);
        assert!(matches!(cause, Stop::LeaseShort { .. }), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0);
        rig.assert_compensated(&attempt, 0).await;
    })
    .await;
}

/// **(F) Used up by retries.** `Conflict` rounds separated by slow reads take the lease below the
/// mark before a later round: with 10002 ms left and 500 ms a read, the scan and the part read leave
/// 9002 ms, and each round's four reads spend 2000, so rounds go at 9002, 7002 and 5002 and not at
/// 3002. With nothing in doubt no further commit is sent and the attempt compensates. Never `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 124)).await;
        assert_eq!(left_after_renewal(&rig, &attempt), 10_002);
        rig.store.tick_reads(500);
        rig.store
            .script(Kind::Commit, vec![], Some(Act::conflict()));
        let cause = stopped(rig.commit(attempt.clone()).await, true);
        assert!(matches!(cause, Stop::LeaseShort { .. }), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Commit).len(), 3);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1);
        rig.assert_compensated(&attempt, 0).await;
    })
    .await;
}

/// **(F) With a commit in doubt.** A commit answered `may_still_commit: true` is outstanding when
/// the lease runs short. No further commit and no compensation are sent, and the attempt returns
/// the in-doubt error keeping the flag.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 125)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::hold().after(Step::Advance(
                left_after_renewal(&rig, &attempt) - MARGIN + 1,
            ))],
            None,
        );
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(
            matches!(doubt.cause, Stop::LeaseShort { .. }),
            "{:?}",
            doubt.cause
        );
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no further commit");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
    })
    .await;
}

/// **(F) Delayed after dispatch.** A commit is sent with the margin left, and the wrapper holds it
/// and moves the clock past the lease's expiry, so the batch passed the dispatch check and has not
/// applied when the lease runs out. (i) It lands before the re-read: `Committed`. (ii) It lands
/// after the attempt returned: the in-doubt error keeping the flag, then the part durable and the
/// slot released once. (iii) A batch in the idle fence's shape lands first, and the held batch then
/// answers `Conflict`: no part, and `NoSuchUpload`. (iv) The held batch lands, then that fence
/// batch, built from slot bytes read before the landing: the fence answers `Conflict`, the session
/// stays `Open@E`, and the part is durable. In all four, no compensation: an expired lease is never
/// evidence that a commit did not land.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock() {
    for run in ["i", "ii", "iii", "iv"] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK + 4, 126)).await;
            let before = rig.session().await;
            let read: Arc<Mutex<Option<Bytes>>> = Arc::default();
            let past = Step::Advance(left_after_renewal(&rig, &attempt) + 1);
            let held = match run {
                "i" => Act::hold().after(past).after(Step::ReleaseHeld),
                "ii" => Act::hold().after(past),
                "iii" => Act::hold()
                    .after(past)
                    .after(rig.read_slot(&attempt, Arc::clone(&read)))
                    .after(rig.fence(&attempt, Arc::clone(&read), CommitOutcome::Committed)),
                _ => Act::hold()
                    .after(past)
                    .after(rig.read_slot(&attempt, Arc::clone(&read)))
                    .after(Step::ReleaseHeld)
                    .after(rig.fence(&attempt, Arc::clone(&read), CommitOutcome::Conflict)),
            };
            rig.store.script(Kind::Commit, vec![held], None);
            let outcome = rig.commit(attempt.clone()).await;
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "run {run}");
            assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "run {run}");
            match run {
                "i" | "iv" => {
                    committed_as(&outcome, &attempt);
                    assert_eq!(rig.session().await, before, "run {run}: still Open@E");
                }
                "ii" => {
                    let doubt = in_doubt(outcome);
                    assert!(doubt.may_still_commit, "the doubt is kept");
                    assert!(rig.part(1).await.is_none(), "not yet");
                    assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
                }
                _ => {
                    refused(&outcome, Refusal::NoSuchUpload);
                    assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
                    assert!(rig.part(1).await.is_none(), "no part record");
                    assert_eq!(rig.releases(&attempt), 0);
                    return;
                }
            }
            assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "run {run}");
            assert_eq!(rig.releases(&attempt), 1, "run {run}: released once");
            assert_eq!(rig.slots_of(&attempt).await, 0, "run {run}");
            assert_eq!(rig.owned_of(&attempt).await, 0, "run {run}");
        })
        .await;
    }
}

// ─────────────── (G) a settled stop compensates ───────────────

/// **(G)** The quiesce renewal stopping with `CoverShort`, `RenewalRoundsSpent`, `LeaseLapsed` or
/// `InvalidRequest`, with nothing in doubt, sends the live-session compensation: one obligation for
/// the attempt's own chunks (none for an empty part), no slot, no owned entry. The answer is the
/// renewal's typed error, in its class, reporting that the slot was released.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_settled_renewal_stop_compensates_and_keeps_its_error() {
    for bytes in [body(CHUNK + 1, 131), Vec::new()] {
        for stop in ["cover-short", "rounds", "lapsed", "invalid"] {
            within(async {
                let at = format!("{stop}, {} bytes", bytes.len());
                let mut rig = Rig::new().await;
                let ttl = if stop == "invalid" { MARGIN } else { TTL };
                let attempt = rig
                    .stage_with(Through::Wrapper, 1, &bytes, |r| r.lease_ttl_millis = ttl)
                    .await;
                match stop {
                    "cover-short" => rig.store.script(
                        Kind::Renewal,
                        vec![],
                        // Below the margin at each round's judging, after any bump a renewal's
                        // stamp gets from the lease it replaces.
                        Some(Act::pass().after(Step::Advance(TTL - MARGIN + 10))),
                    ),
                    "rounds" => rig
                        .store
                        .script(Kind::Renewal, vec![], Some(Act::conflict())),
                    "lapsed" => rig.clock.set(attempt.slot().lease_expiry_millis()),
                    _ => {}
                }
                let error = renewal_stopped(rig.commit(attempt.clone()).await, true);
                let StageError::Stopped { cause, .. } = &error else {
                    panic!("{at}: expected a settled stop, got {error:?}");
                };
                let matched = match stop {
                    "cover-short" => matches!(cause, StageStop::CoverShort { .. }),
                    "rounds" => matches!(cause, StageStop::RenewalRoundsSpent { .. }),
                    "lapsed" => matches!(cause, StageStop::LeaseLapsed { .. }),
                    _ => matches!(cause, StageStop::InvalidRequest { .. }),
                };
                assert!(matched, "{at}: {cause:?}");
                assert_eq!(rig.log.commits(Kind::Commit).len(), 0, "{at}: no commit");
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "{at}");
                rig.assert_compensated(&attempt, 0).await;
            })
            .await;
        }
    }
}

/// **(G) The cap witness.** `MAX_INFLIGHT_PARTS` + 1 attempts on one session each end in a settled
/// stop (spent commit rounds, a stopped renewal, a short lease, in turn). The session's held-slot
/// count is 0 afterwards, and the next reserve succeeds instead of answering `SlowDown`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_settled_stops_never_fill_the_slot_cap() {
    within(async {
        let mut rig = Rig::new().await;
        for at in 0..=MAX_INFLIGHT_PARTS {
            let attempt = rig.stage(Through::Wrapper, at + 1, &body(CHUNK, 132)).await;
            match at % 3 {
                0 => {
                    rig.store
                        .script(Kind::Commit, vec![], Some(Act::conflict()));
                    stopped(rig.commit(attempt).await, true);
                }
                1 => {
                    rig.store
                        .script(Kind::Renewal, vec![], Some(Act::conflict()));
                    renewal_stopped(rig.commit(attempt).await, true);
                    rig.store.script(Kind::Renewal, vec![], None);
                }
                _ => {
                    let spent = left_after_renewal(&rig, &attempt) - MARGIN + 1;
                    rig.store.after_scan(vec![Step::Advance(spent)]);
                    stopped(rig.commit(attempt).await, true);
                }
            }
        }
        assert!(
            rig.inner
                .scan(&slot_range(&rig.upload))
                .await
                .unwrap()
                .is_empty(),
            "no slot is held"
        );
        let next = rig.attempt_id();
        let request = StageRequest {
            upload_id: &rig.upload,
            part_number: part(100),
            attempt_id: &next,
            max_inflight_parts: MAX_INFLIGHT_PARTS,
            lease_ttl_millis: TTL,
            chunk_size: CHUNK,
            scheme: RS42,
            topology: &rig.topology,
            write_window_millis: W,
            max_part_chunks: MAX_PART_CHUNKS,
            body_idle_millis: IDLE,
        };
        let reserved = reserve_slot(&*rig.inner, &request, rig.clock.now_millis())
            .await
            .unwrap();
        assert!(
            matches!(reserved, ReserveOutcome::Reserved { .. }),
            "{reserved:?}"
        );
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **(G) Not compensated.** No compensation batch, and an answer that does not claim a release, for
/// a renewal in doubt, `SlotLost`, `Record`, `Store`, and a read that faults during classification.
/// `EntryLost` cannot come out of a renewal: the unit test in `commit.rs` holds that row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate() {
    within(async {
        // A renewal in doubt.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 133)).await;
        rig.store.script(
            Kind::Renewal,
            vec![Act::unknown(false, true)],
            Some(Act::conflict()),
        );
        let error = renewal_stopped(rig.commit(attempt.clone()).await, false);
        assert!(matches!(error, StageError::InDoubt(_)), "{error:?}");
        assert_eq!(classify(&error), ErrorClass::Indeterminate);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1);

        // `SlotLost` from the renewal.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 134)).await;
        let gone = WriteBatch::new().delete(attempt.slot_key().to_vec());
        rig.inner.commit(gone).await.unwrap();
        let error = renewal_stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(
                &error,
                StageError::Stopped {
                    cause: StageStop::SlotLost { .. },
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // `Record` from the renewal: a damaged session.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 135)).await;
        let damage = rig.plant(mpu_key(&rig.upload), Bytes::from_static(b"torn"));
        rig.store
            .script(Kind::Renewal, vec![Act::conflict().before(damage)], None);
        let error = renewal_stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(
                &error,
                StageError::Stopped {
                    cause: StageStop::Record(_),
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // `Store` from the renewal.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 136)).await;
        rig.store.script(Kind::Renewal, vec![Act::fail()], None);
        let error = renewal_stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(
                &error,
                StageError::Stopped {
                    cause: StageStop::Store(_),
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // `Store` from the commit: alone, and keeping an earlier commit's doubt.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 137)).await;
        rig.store.script(Kind::Commit, vec![Act::fail()], None);
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(matches!(cause, Stop::Store(_)), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 138)).await;
        rig.store
            .script(Kind::Commit, vec![Act::hold(), Act::fail()], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(matches!(doubt.cause, Stop::Store(_)), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
    })
    .await;
    // A read that faults during classification, at each of the four reads, with nothing in doubt
    // and with a commit held.
    for read in 1..=4usize {
        for held in [false, true] {
            within(async {
                let at = format!("read {read}, held {held}");
                let mut rig = Rig::new().await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 139)).await;
                let first = if held { Act::hold() } else { Act::conflict() };
                rig.store.script(Kind::Commit, vec![first], None);
                rig.store.fail_read(read);
                let outcome = rig.commit(attempt.clone()).await;
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
                if held {
                    let doubt = in_doubt(outcome);
                    assert!(doubt.may_still_commit, "{at}: the doubt is kept");
                    assert!(matches!(doubt.cause, Stop::Store(_)), "{at}");
                } else {
                    let cause = stopped(outcome, false);
                    assert!(matches!(cause, Stop::Store(_)), "{at}: {cause:?}");
                }
                assert_eq!(rig.slots_of(&attempt).await, 1, "{at}");
            })
            .await;
        }
    }
}

/// **(G)** A slot found gone, or rewritten under this attempt's id, with no batch of this attempt
/// explaining it: `SlotLost`, naming the key, at the commit's re-read and at the compensation's. No
/// compensation at the commit's, and no release claimed at either. A damaged slot or session at a
/// re-read is `Record`, never another attempt's slot or a moved session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_slot_lost_or_a_damaged_record_at_a_re_read_stops_without_a_release() {
    for how in ["gone", "rewritten"] {
        within(async {
            // At the commit's re-read, with nothing in doubt.
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 141)).await;
            let step = other_slot_bytes(&rig, &attempt, how);
            rig.store
                .script(Kind::Commit, vec![Act::conflict().before(step)], None);
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert_slot_lost(&cause, &attempt);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

            // At the compensation's re-read, with nothing in doubt.
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
            let rival = rig.stage(Through::Inner, 1, &[]).await;
            let step = other_slot_bytes(&rig, &attempt, how);
            rig.store.script(
                Kind::Commit,
                vec![Act::pass().before(rig.rival(rival))],
                None,
            );
            rig.store
                .script(Kind::Compensation, vec![Act::conflict().before(step)], None);
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert_slot_lost(&cause, &attempt);
        })
        .await;
    }
    for (phase, damage) in [
        ("commit", "session"),
        ("commit", "slot"),
        ("compensation", "session"),
        ("compensation", "slot"),
    ] {
        within(async {
            let at = format!("{damage} damaged at the {phase}'s re-read");
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
            let key = if damage == "session" {
                mpu_key(&rig.upload)
            } else {
                attempt.slot_key().to_vec()
            };
            let step = rig.plant(key, Bytes::from_static(b"torn"));
            if phase == "commit" {
                rig.store
                    .script(Kind::Commit, vec![Act::conflict().before(step)], None);
            } else {
                let rival = rig.stage(Through::Inner, 1, &[]).await;
                rig.store.script(
                    Kind::Commit,
                    vec![Act::pass().before(rig.rival(rival))],
                    None,
                );
                rig.store
                    .script(Kind::Compensation, vec![Act::conflict().before(step)], None);
            }
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert!(matches!(cause, Stop::Record(_)), "{at}: {cause:?}");
        })
        .await;
    }
    // The commit's own reads: a damaged summary, and a damaged prior.
    for damaged in ["summary", "prior"] {
        within(async {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 142)).await;
            let key = if damaged == "summary" {
                psum_key(&rig.upload, part(2))
            } else {
                part_key(&rig.upload, part(1))
            };
            rig.run(vec![rig.plant(key, Bytes::from_static(b"torn"))])
                .await;
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert!(matches!(cause, Stop::Record(_)), "{damaged}: {cause:?}");
            assert_eq!(rig.log.commits(Kind::Commit).len(), 0, "{damaged}");
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{damaged}");
        })
        .await;
    }
}

/// A step that moves `attempt`'s slot under it: deletes it, or rewrites it under its own id with a
/// later lease.
fn other_slot_bytes(rig: &Rig, attempt: &StagedAttempt, how: &str) -> Step {
    let key = attempt.slot_key().to_vec();
    if how == "gone" {
        let inner = Arc::clone(&rig.inner);
        return Step::Run(once(async move {
            let batch = WriteBatch::new().delete(key);
            assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
        }));
    }
    let bytes = attempt.slot_bytes();
    let record = decode_slot_record(bytes).unwrap();
    let text = std::str::from_utf8(bytes).unwrap();
    let from = format!("\"lease_expiry_millis\":{}", record.lease_expiry_millis());
    // Far enough on that no renewal's stamp can equal it.
    let to = format!(
        "\"lease_expiry_millis\":{}",
        record.lease_expiry_millis() + 1_000
    );
    assert_eq!(text.matches(&from).count(), 1);
    rig.plant(key, Bytes::from(text.replace(&from, &to)))
}

fn assert_slot_lost(cause: &Stop, attempt: &StagedAttempt) {
    let expected = String::from_utf8_lossy(attempt.slot_key()).into_owned();
    assert!(
        matches!(cause, Stop::SlotLost { key } if *key == expected),
        "{cause:?}"
    );
    assert!(cause.to_string().contains(&expected), "{cause}");
}

/// **(G) During that compensation.** A rollback to `Open@E'` is chased as in (B). A session found
/// fenced answers `NoSuchUpload` with no further write. A compensation that spends its rounds or
/// stays in doubt returns its own typed error, which does not claim a release.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_settled_stops_compensation_chases_stops_or_reports_itself() {
    within(async {
        // Chased.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 151)).await;
        rig.store
            .script(Kind::Commit, vec![], Some(Act::conflict()));
        rig.store.script(
            Kind::Compensation,
            vec![Act::conflict().before(rig.rollback())],
            None,
        );
        let cause = stopped(rig.commit(attempt.clone()).await, true);
        assert!(matches!(cause, Stop::CommitRoundsSpent { .. }), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 2);
        rig.assert_compensated(&attempt, 2).await;

        // Fenced, at the stopped renewal's compensation too.
        for renewal in [false, true] {
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 152)).await;
            let kind = if renewal { Kind::Renewal } else { Kind::Commit };
            rig.store.script(kind, vec![], Some(Act::conflict()));
            rig.store.script(
                Kind::Compensation,
                vec![Act::conflict().before(rig.abort())],
                None,
            );
            let outcome = rig.commit(attempt.clone()).await;
            refused(&outcome, Refusal::NoSuchUpload);
            assert_eq!(
                rig.log.commits(Kind::Compensation).len(),
                1,
                "no further write"
            );
            assert_eq!(
                rig.slots_of(&attempt).await,
                1,
                "the residue is the reaper's"
            );
        }

        // Fenced after an unknown outcome: the slot read comes before the session read that found
        // the move, so the slot is read again before the doubt goes.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 153)).await;
        rig.store
            .script(Kind::Commit, vec![], Some(Act::conflict()));
        rig.store.script(
            Kind::Compensation,
            vec![Act::unknown(false, true).after(rig.abort())],
            None,
        );
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::NoSuchUpload);
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            1,
            "no further write"
        );

        // Spent.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 154)).await;
        rig.store
            .script(Kind::Commit, vec![], Some(Act::conflict()));
        rig.store
            .script(Kind::Compensation, vec![], Some(Act::conflict()));
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(cause, Stop::CompensationRoundsSpent { .. }),
            "{cause:?}"
        );

        // In doubt.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 155)).await;
        rig.store
            .script(Kind::Commit, vec![], Some(Act::conflict()));
        rig.store
            .script(Kind::Compensation, vec![Act::hold()], Some(Act::conflict()));
        let outcome = rig.commit(attempt.clone()).await;
        assert!(!outcome.as_ref().is_err_and(CommitError::slot_released));
        let doubt = in_doubt(outcome);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(doubt.may_still_commit);
    })
    .await;
}

// ─────────────── (H) the five gaps ───────────────

/// **(H) A commit held while a rollback lands.** The commit is held. Right after the attempt's slot
/// read, the held commit lands and a Complete rolls the session back to `Open@E'`. The answer is
/// the in-doubt error; no compensation is sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_a_commit_held_while_a_rollback_lands_is_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 161)).await;
        rig.store.script(Kind::Commit, vec![Act::hold()], None);
        // The part read, the session read, then the slot read.
        rig.store
            .after_read(3, vec![Step::ReleaseHeld, rig.rollback()]);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation"
        );
        assert_eq!(rig.releases(&attempt), 1);
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
    })
    .await;
}

/// **(H) A held compensation under a teardown.** An empty part loses to a rival, its compensation
/// is held, then an Abort and a teardown run. The answer is the in-doubt error, never
/// `OperationAborted`, because nothing of this attempt landed and the session is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_a_held_compensation_under_a_teardown_is_in_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        let rival = rig.stage(Through::Inner, 1, &[]).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::hold().after(rig.abort()).after(rig.teardown())],
            None,
        );
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(!doubt.may_still_commit, "the session it required is gone");
        assert!(
            matches!(doubt.cause, Stop::SessionMoved),
            "{:?}",
            doubt.cause
        );
        assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
        assert_eq!(rig.releases(&attempt), 0, "nothing of this attempt landed");
    })
    .await;
}

/// **(H) The obligation re-read.** A compensation lands with an unknown outcome, then a client
/// Abort runs before the re-read. Only the obligation re-read can settle this: the session moved,
/// so the slot says nothing. The answer is not an in-doubt error. A second run plants other bytes
/// at the obligation's key: `ObligationKeyTaken`, naming the key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 171)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 172)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::unknown(true, false).after(rig.abort())],
            None,
        );
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1);
        assert_eq!(rig.releases(&attempt), 1);

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 173)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 174)).await;
        let key = retire_key(RetireMode::Bytes, &token_of(&attempt, 0));
        let planted = rig.plant(key.clone(), Bytes::from_static(b"other"));
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::conflict().before(planted)],
            None,
        );
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        let expected = String::from_utf8_lossy(&key).into_owned();
        assert!(
            matches!(&cause, Stop::ObligationKeyTaken { key } if *key == expected),
            "{cause:?}"
        );
        assert!(cause.to_string().contains(&expected), "{cause}");
    })
    .await;
}

/// **(H) The epoch chase's real bound.** One rollback beats the commit, then two more beat
/// compensations: three rollbacks, `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The next
/// compensation lands, and the slot is released under the live session. The worst case takes one
/// more: a rival beats the commit and **all three** rollbacks beat compensations, so the fourth
/// compensation is the one that lands. That second run is what ties the bound to its value.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 181)).await;
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(rig.rollback())], None);
        rig.store.script(
            Kind::Compensation,
            vec![
                Act::conflict().before(rig.rollback()),
                Act::conflict().before(rig.rollback()),
            ],
            None,
        );
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 3);
        let session = decode_session_record(&rig.session().await).unwrap();
        assert_eq!(session.epoch(), 6);
        assert!(matches!(session.state(), SessionState::Open {}), "live");
        rig.assert_compensated(&attempt, 6).await;

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 182)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 183)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![
                Act::conflict().before(rig.rollback()),
                Act::conflict().before(rig.rollback()),
                Act::conflict().before(rig.rollback()),
            ],
            None,
        );
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            COMPENSATION_ROUNDS as usize
        );
        rig.assert_compensated(&attempt, 6).await;
    })
    .await;
}

/// **(H) The class of a composed error.** An in-doubt error from the composed `upload_part`, read
/// through `classify`, is `Indeterminate`, and it says what the commit's says. A staging error and
/// a staging refusal come through the composition as themselves, with no commit sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_a_composed_error_keeps_its_class_and_its_message() {
    within(async {
        let mut rig = Rig::new().await;
        rig.store
            .script(Kind::Commit, vec![Act::hold()], Some(Act::conflict()));
        let err = rig
            .upload(1, &body(CHUNK, 191), MAX_PART_CHUNKS)
            .await
            .expect_err("the commit is in doubt");
        assert_eq!(classify(&err), ErrorClass::Indeterminate, "{err}");
        let text = err.to_string();
        let UploadPartError::Commit(inner) = err else {
            panic!("expected the commit's error, got {err:?}");
        };
        assert_eq!(text, inner.to_string());
        assert!(text.contains("is in doubt"), "{text}");
        assert!(matches!(inner, CommitError::InDoubt(_)));

        let mut rig = Rig::new().await;
        rig.store.script(Kind::Reserve, vec![Act::fail()], None);
        let err = rig
            .upload(1, &body(CHUNK, 192), MAX_PART_CHUNKS)
            .await
            .expect_err("staging stopped");
        assert_eq!(classify(&err), ErrorClass::Terminal, "{err}");
        let text = err.to_string();
        let UploadPartError::Stage(inner) = err else {
            panic!("expected staging's error, got {err:?}");
        };
        assert_eq!(text, inner.to_string());
        assert!(text.contains("multipart staging"), "{text}");
        assert_eq!(rig.log.commits(Kind::Commit).len(), 0);

        let mut rig = Rig::new().await;
        rig.run(vec![rig.abort()]).await;
        let outcome = rig
            .upload(1, &body(CHUNK, 193), MAX_PART_CHUNKS)
            .await
            .expect("the upload ran");
        assert_eq!(outcome, UploadPartOutcome::Refused(Refusal::NoSuchUpload));
    })
    .await;
}

// ─────────────── the sign-off's legs (attempt 2) ───────────────

const DAMAGE: &[u8] = b"not a part record";

/// **A damaged `part:` record is not a rival** (ADR-0045). A commit answered `Conflict` because the
/// part key holds bytes that do not decode stops with `Stop::Record`: no `OperationAborted`, no
/// compensation, and the slot and owned entries stay. Run for a first upload, a re-upload over a
/// non-empty prior, and a re-upload over an empty prior.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_damaged_part_record_at_the_re_read_is_a_record_stop_not_a_rival() {
    for prior in [None, Some(body(CHUNK, 201)), Some(Vec::new())] {
        within(async {
            let at = format!("{:?} prior bytes", prior.as_ref().map(Vec::len));
            let mut rig = Rig::new().await;
            if let Some(bytes) = &prior {
                rig.committed(1, bytes).await;
            }
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 202)).await;
            let damage = rig.plant(part_key(&rig.upload, part(1)), Bytes::from_static(DAMAGE));
            rig.store
                .script(Kind::Commit, vec![Act::pass().before(damage)], None);
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert!(matches!(cause, Stop::Record(_)), "{at}: {cause:?}");
            assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "{at}: no retry");
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
            assert_eq!(rig.slots_of(&attempt).await, 1, "{at}: the slot stays");
            assert_eq!(rig.owned_of(&attempt).await, 1, "{at}: the entry stays");
            assert_eq!(rig.releases(&attempt), 0, "{at}");
        })
        .await;
    }
}

/// **A damaged `part:` record under a held commit.** The damage settles nothing, because only a
/// rival's record shows the commit can no longer land: the answer is the in-doubt error with the
/// cause `Record` and the flag kept, and no compensation is sent. Once the damage is repaired the
/// held commit can still land. With the commit landed before the damage, the own-commit proof comes
/// first, whatever the part key holds, and the answer is `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 203)).await;
        let key = part_key(&rig.upload, part(1));
        let damage = rig.plant(key.clone(), Bytes::from_static(DAMAGE));
        rig.store
            .script(Kind::Commit, vec![Act::hold().after(damage)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "damage settles nothing");
        assert!(matches!(doubt.cause, Stop::Record(_)), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
        rig.inner
            .commit(WriteBatch::new().delete(key))
            .await
            .unwrap();
        assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        assert_eq!(rig.releases(&attempt), 1);
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 204)).await;
        let damage = rig.plant(part_key(&rig.upload, part(1)), Bytes::from_static(DAMAGE));
        rig.store.script(
            Kind::Commit,
            vec![Act::unknown(true, false).after(damage)],
            None,
        );
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
    })
    .await;
}

/// **An empty prior's bytes can come back.** Over an empty prior, a commit is held and a
/// **non-empty** rival lands. That rival's record does not settle the held commit: a later empty
/// rival in the same millisecond writes the prior's bytes again, and the held commit's
/// `require(part == prior)` holds once more. If a compensation is ever sent, the wrapper makes that
/// history real: it restores the bytes, lands the held commit, and answers the compensation
/// unknown. Required: no compensation, and the in-doubt error keeping the flag. (A non-empty
/// prior's bytes can come back too, by a repair; the attempt-5 legs below hold that.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &[]).await;
        assert!(prior.is_empty());
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 205)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 206)).await;
        let restorer = rig.stage(Through::Inner, 1, &[]).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::hold().after(rig.rival(rival.clone()))],
            Some(Act::conflict()),
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::unknown(false, true)
                .before(rig.rival(restorer))
                .before(Step::ReleaseHeld)],
            None,
        );
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(
            matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
            "{:?}",
            doubt.cause
        );
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation while the commit can still land"
        );
        assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
        assert!(is_own(&rig.part(1).await.unwrap().0, &rival));
    })
    .await;
}

/// **The ceiling leaves out the part's own prior** (a re-upload replaces it). With `psum:` planted
/// at `MAX_STAGED_CHUNKS - 1` and a committed one-chunk part 1, the session is at the ceiling
/// exactly, and a one-chunk re-upload of part 1 keeps it there: `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_the_cumulative_ceiling_leaves_out_the_parts_own_prior() {
    within(async {
        let mut rig = Rig::new().await;
        let planted = PartSummary {
            chunks: MAX_STAGED_CHUNKS - 1,
            len: 0,
            digest: sha256(b""),
            committed_at_millis: T0,
        };
        rig.inner
            .commit(WriteBatch::new().put(
                psum_key(&rig.upload, part(9000)),
                metadata::encode(&planted),
            ))
            .await
            .unwrap();
        let prior = rig.committed(1, &body(CHUNK, 207)).await;
        assert_eq!(prior.chunks().len(), 1);
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 208)).await;
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 0), chunk_ids(prior.chunks()))]
        );
    })
    .await;
}

/// **A re-upload's obligation key is required absent.** Other bytes planted at the key the re-upload
/// would install its obligation under: no commit can land, so the rounds are spent, the compensation
/// finds the same key taken, and the answer is `ObligationKeyTaken`, naming the key. Those bytes and
/// the prior part are both kept, and nothing is released.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(CHUNK, 209)).await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 210)).await;
        let key = retire_key(RetireMode::Bytes, &token_of(&attempt, 0));
        let planted = Bytes::from_static(b"other");
        rig.run(vec![rig.plant(key.clone(), planted.clone())]).await;
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        let expected = String::from_utf8_lossy(&key).into_owned();
        assert!(
            matches!(&cause, Stop::ObligationKeyTaken { key } if *key == expected),
            "{cause:?}"
        );
        assert_eq!(rig.inner.get(&key).await.unwrap(), Some(planted));
        assert_eq!(rig.part(1).await.unwrap().0, prior, "the prior part stands");
        assert_eq!(rig.log.commits(Kind::Commit).len(), COMMIT_ROUNDS as usize);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1);
        assert_eq!(rig.slots_of(&attempt).await, 1, "nothing was released");
        assert_eq!(rig.owned_of(&attempt).await, 1);
    })
    .await;
}

/// **The kept doubt is the round that may still land, not the latest.** Round 1 is held, round 2
/// answers `may_still_commit: false`, and the next read faults. Round 1 can still land, so the error
/// keeps `may_still_commit: true` and reports round 1's outcome.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_the_kept_doubt_is_the_round_that_may_still_land() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 211)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::hold(), Act::unknown(false, false)],
            None,
        );
        // Round 1's four reads, then the first read after round 2.
        rig.store.fail_read(5);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 2);
        assert!(matches!(doubt.cause, Stop::Store(_)), "{:?}", doubt.cause);
        assert!(doubt.may_still_commit, "round 1 may still land");
        assert!(doubt.outcome.may_still_commit, "round 1's own report");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
        assert_eq!(rig.releases(&attempt), 1);
    })
    .await;
}

/// **The epoch chase re-reads every obligation it sent.** A loser's first compensation, under `E`,
/// is held. Right after its slot read that batch lands and the session rolls back, so the attempt
/// chases to `E+2`, and the second round conflicts on the slot the first released. Only the first
/// round's obligation can say what happened: its key is under the old epoch, and the session has
/// moved off the bytes that round required. The answer is `OperationAborted`, never in doubt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_the_epoch_chase_re_reads_every_obligation_it_sent() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 212)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 213)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold()], None);
        // The commit's four reads, then the first compensation's obligation and slot reads.
        rig.store
            .after_read(6, vec![Step::ReleaseHeld, rig.rollback()]);
        let outcome = rig.commit(attempt.clone()).await;
        refused(&outcome, Refusal::OperationAborted);
        let compensations = rig.log.commits(Kind::Compensation);
        assert_eq!(compensations.len(), 2, "it chased the rollback");
        assert_eq!(required_epoch(&compensations[0].0, &rig.upload), 0);
        assert_eq!(required_epoch(&compensations[1].0, &rig.upload), 2);
        assert_eq!(compensations[1].1, Answer::Conflict);
        rig.assert_compensated(&attempt, 0).await;
    })
    .await;
}

/// A step that deletes `attempt`'s slot on the inner store, with no batch of the attempt doing it.
fn slot_deleted(rig: &Rig, attempt: &StagedAttempt) -> Step {
    let inner = Arc::clone(&rig.inner);
    let key = attempt.slot_key().to_vec();
    Step::Run(once(async move {
        let batch = WriteBatch::new().delete(key);
        assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
    }))
}

/// **A slot gone under a session that moved, at the commit's re-read, with nothing in doubt.** A
/// teardown took it under a fenced session: `NoSuchUpload`, since nothing of this attempt landed.
/// Under a newer `Open` epoch nothing explains it: `SlotLost`. Neither compensates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_slot_gone_under_a_moved_session_at_the_commits_re_read() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 214)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.abort()).before(rig.teardown())],
            None,
        );
        refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert!(rig.part(1).await.is_none());

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 215)).await;
        let gone = slot_deleted(&rig, &attempt);
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rollback()).before(gone)],
            None,
        );
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert_slot_lost(&cause, &attempt);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
    })
    .await;
}

/// **A slot gone under a session that moved, at the compensation's re-read, with nothing in doubt.**
/// A refusal whose compensation finds the session fenced and torn down answers the session's
/// refusal, not its own, and writes nothing more: for a part that lost to a rival, and for one past
/// the per-part bound. Under a newer `Open` epoch nothing explains the slot: `SlotLost`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 216)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 217)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::pass().before(rig.abort()).before(rig.teardown())],
            None,
        );
        refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
        assert!(rig.part_obligations().await.is_empty());

        let mut rig = Rig::new().await;
        let attempt = rig
            .stage_with(Through::Wrapper, 1, &body(3 * CHUNK, 218), |r| {
                r.max_part_chunks = 2
            })
            .await;
        rig.store.script(
            Kind::Compensation,
            vec![Act::pass().before(rig.abort()).before(rig.teardown())],
            None,
        );
        refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
        assert!(rig.part(1).await.is_none());

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 219)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 220)).await;
        let gone = slot_deleted(&rig, &attempt);
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::conflict().before(rig.rollback()).before(gone)],
            None,
        );
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert_slot_lost(&cause, &attempt);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
    })
    .await;
}

// ─────────────── the sign-off's legs (attempt 3) ───────────────

const TORN: &[u8] = b"not a session record";

/// A step that removes `keys` on the inner store.
fn erased(rig: &Rig, keys: Vec<Vec<u8>>) -> Step {
    let inner = Arc::clone(&rig.inner);
    Step::Run(once(async move {
        let mut batch = WriteBatch::new();
        for key in keys {
            batch = batch.delete(key);
        }
        assert_eq!(inner.commit(batch).await.unwrap(), CommitOutcome::Committed);
    }))
}

/// A record that decodes as `Open` at `bytes`'s own epoch and is not `bytes`: a value no legal
/// history writes, since a session returns to `Open` only at a higher epoch.
fn same_epoch_other_bytes(bytes: &Bytes) -> Bytes {
    let record = decode_session_record(bytes).unwrap();
    let text = std::str::from_utf8(bytes).unwrap();
    let from = format!("\"attempts\":{},", record.attempts());
    let to = format!("\"attempts\":{},", record.attempts() + 1);
    assert_eq!(text.matches(&from).count(), 1);
    let other = Bytes::from(text.replace(&from, &to));
    let decoded = decode_session_record(&other).expect("the planted session decodes");
    assert_eq!(decoded.epoch(), record.epoch());
    assert!(matches!(decoded.state(), SessionState::Open {}));
    other
}

/// The session at `bytes` fenced to `Aborting` as a real fence writes it (`open_teardown`), but at
/// `epoch` in place of the next one. With `epoch` at or below the record's own this is a value no
/// legal history writes, since every fence bumps the epoch (`0016:704-708`): the shape of a stale or
/// misapplied fence, which an attempt must read as damage, never as a fence it lost to.
fn aborting_at(bytes: &Bytes, upload: &UploadId, epoch: u64) -> Bytes {
    let record = decode_session_record(bytes).unwrap();
    let fenced = metadata::encode(
        record
            .open_teardown(upload)
            .expect("fenced from Open")
            .session(),
    );
    let text = std::str::from_utf8(&fenced).unwrap();
    let from = format!("\"epoch\":{},", record.epoch() + 1);
    let to = format!("\"epoch\":{epoch},");
    assert_eq!(text.matches(&from).count(), 1);
    let other = Bytes::from(text.replace(&from, &to));
    let decoded = decode_session_record(&other).expect("the planted session decodes");
    assert_eq!(decoded.epoch(), epoch);
    assert!(matches!(decoded.state(), SessionState::Aborting {}));
    other
}

/// How many kinds of damage [`session_damage`] builds.
const DAMAGE_KINDS: usize = 4;

/// The kinds of damage a session read can find, each `(label, bytes, the record error)`: bytes that
/// do not decode; an `Open` record under other bytes at the same epoch; and a non-`Open` record at
/// the same epoch and at a lower one, the shape of a fence whose epoch has not risen, which no legal
/// history writes. `session` must be at epoch 1 or above, so that a lower epoch exists.
fn session_damage(rig: &Rig, session: &Bytes) -> Vec<(&'static str, Bytes, Option<RecordError>)> {
    let epoch = decode_session_record(session).unwrap().epoch();
    assert!(epoch >= 1, "a lower epoch exists");
    let not_risen = |found| {
        Some(RecordError::SessionEpochNotRisen {
            found,
            required: epoch,
        })
    };
    let kinds = vec![
        ("torn", Bytes::from_static(TORN), None),
        (
            "Open at the same epoch",
            same_epoch_other_bytes(session),
            not_risen(epoch),
        ),
        (
            "Aborting at the same epoch",
            aborting_at(session, &rig.upload, epoch),
            not_risen(epoch),
        ),
        (
            "Aborting at a lower epoch",
            aborting_at(session, &rig.upload, epoch - 1),
            not_risen(epoch - 1),
        ),
    ];
    assert_eq!(kinds.len(), DAMAGE_KINDS);
    kinds
}

/// `cause` is the record error `expected`, or, with none named, any record error.
fn assert_record_stop(cause: &Stop, expected: Option<&RecordError>, at: &str) {
    match (cause, expected) {
        (Stop::Record(found), Some(expected)) => assert_eq!(found, expected, "{at}"),
        (Stop::Record(_), None) => {}
        _ => panic!("{at}: expected a record stop, got {cause:?}"),
    }
}

/// **A damaged session under a held commit keeps the doubt.** The commit is held, and the session
/// is damaged at each read gap in turn and stays damaged, with each kind of damage: torn bytes, an
/// `Open` record at the same epoch, and an `Aborting` record at the same and at a lower epoch (a
/// fence raises the epoch, so those are no fence). Damage is not a moved session, so nothing
/// settles: the answer is the in-doubt error keeping `may_still_commit`, with the cause `Record`,
/// never `NoSuchUpload`, and no compensation. Repairing the session lets the held batch land, which
/// is why claiming it could not would be wrong.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_damaged_session_under_a_held_commit_keeps_the_doubt() {
    // The damage lands after this many of the round's reads: part, session, slot, session.
    for after in 0..=3usize {
        for kind in 0..DAMAGE_KINDS {
            within(async {
                let mut rig = Rig::new().await;
                // One rollback first, so that a lower epoch exists for the damage to carry.
                rig.run(vec![rig.rollback()]).await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 221)).await;
                let session = rig.session().await;
                let (label, bytes, expected) = session_damage(&rig, &session).swap_remove(kind);
                let at = format!("{label} after read {after}");
                let damage = rig.plant(mpu_key(&rig.upload), bytes);
                if after == 0 {
                    rig.store
                        .script(Kind::Commit, vec![Act::hold().after(damage)], None);
                } else {
                    rig.store.script(Kind::Commit, vec![Act::hold()], None);
                    rig.store.after_read(after, vec![damage]);
                }
                let doubt = in_doubt(rig.commit(attempt.clone()).await);
                assert_eq!(doubt.batch, DoubtfulBatch::Commit, "{at}");
                assert!(doubt.may_still_commit, "{at}: damage settles nothing");
                assert!(doubt.outcome.may_still_commit, "{at}");
                assert_record_stop(&doubt.cause, expected.as_ref(), &at);
                assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "{at}: no retry");
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
                assert_eq!(
                    rig.slots_of(&attempt).await,
                    1,
                    "{at}: covered by the doubt"
                );

                let repair = rig.plant(mpu_key(&rig.upload), session.clone());
                rig.run(vec![repair]).await;
                assert_eq!(
                    rig.store.release_held().await,
                    CommitOutcome::Committed,
                    "{at}: the repaired session lets the held batch land"
                );
                assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "{at}");
                assert_eq!(rig.releases(&attempt), 1, "{at}");
                assert_eq!(rig.slots_of(&attempt).await, 0, "{at}");
            })
            .await;
        }
    }
}

/// **Damage repaired mid-round is no evidence of anything.** The round's first session read finds
/// damage and its last finds the session repaired, so the classification sees `Open@E` and a slot
/// still held. If that first read had cleared the doubt, the attempt would read a rival's record over
/// an empty prior as a lost commit, or spend its rounds with nothing in doubt, and compensate while
/// the held commit can still land. Required: never `Refused(OperationAborted)` and no compensation,
/// the in-doubt error keeping the flag, and the held batch landing once the bytes it requires are
/// back. Run with the repair on either side of the slot read, and with each kind of damage: an
/// `Aborting` record at an epoch that has not risen read as a fence would clear the doubt the same
/// way torn bytes read as a moved session would. A held commit that lands during the damage is still
/// proved by the slot and the session read after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_session_damaged_and_repaired_mid_round_settles_nothing() {
    for repair_after in [2usize, 3] {
        for kind in 0..DAMAGE_KINDS {
            // A rival's record over an empty prior, which settles nothing on its own.
            within(async {
                let mut rig = Rig::new().await;
                rig.run(vec![rig.rollback()]).await;
                rig.committed(1, &[]).await;
                let required = rig
                    .inner
                    .get(&part_key(&rig.upload, part(1)))
                    .await
                    .unwrap()
                    .unwrap();
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 222)).await;
                let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 223)).await;
                let session = rig.session().await;
                let (label, damage, _) = session_damage(&rig, &session).swap_remove(kind);
                let at = format!("rival, {label}, repaired after read {repair_after}");
                rig.store.script(
                    Kind::Commit,
                    vec![Act::hold().after(rig.rival(rival))],
                    Some(Act::conflict()),
                );
                rig.store
                    .after_read(1, vec![rig.plant(mpu_key(&rig.upload), damage)]);
                rig.store.after_read(
                    repair_after,
                    vec![rig.plant(mpu_key(&rig.upload), session.clone())],
                );
                let doubt = in_doubt(rig.commit(attempt.clone()).await);
                assert_eq!(doubt.batch, DoubtfulBatch::Commit, "{at}");
                assert!(doubt.may_still_commit, "{at}");
                assert!(
                    matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
                    "{at}: {:?}",
                    doubt.cause
                );
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
                assert_eq!(rig.slots_of(&attempt).await, 1, "{at}");
                let back = rig.plant(part_key(&rig.upload, part(1)), required);
                rig.run(vec![back]).await;
                assert_eq!(
                    rig.store.release_held().await,
                    CommitOutcome::Committed,
                    "{at}: the held commit could still land"
                );
                assert_eq!(rig.releases(&attempt), 1, "{at}");
            })
            .await;

            // A first upload, with every later round answered `Conflict`.
            within(async {
                let mut rig = Rig::new().await;
                rig.run(vec![rig.rollback()]).await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 224)).await;
                let session = rig.session().await;
                let (label, damage, _) = session_damage(&rig, &session).swap_remove(kind);
                let at = format!("no rival, {label}, repaired after read {repair_after}");
                rig.store
                    .script(Kind::Commit, vec![Act::hold()], Some(Act::conflict()));
                rig.store
                    .after_read(1, vec![rig.plant(mpu_key(&rig.upload), damage)]);
                rig.store
                    .after_read(repair_after, vec![rig.plant(mpu_key(&rig.upload), session)]);
                let doubt = in_doubt(rig.commit(attempt.clone()).await);
                assert!(doubt.may_still_commit, "{at}");
                assert!(
                    matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
                    "{at}: {:?}",
                    doubt.cause
                );
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
                assert_eq!(rig.store.release_held().await, CommitOutcome::Committed, "{at}");
                assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "{at}");
                assert_eq!(rig.releases(&attempt), 1, "{at}");
            })
            .await;
        }
    }

    // The held commit lands while the session is damaged, and the repair comes before the slot read.
    for kind in 0..DAMAGE_KINDS {
        within(async {
            let mut rig = Rig::new().await;
            rig.run(vec![rig.rollback()]).await;
            let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 225)).await;
            let session = rig.session().await;
            let (label, damage, _) = session_damage(&rig, &session).swap_remove(kind);
            let key = mpu_key(&rig.upload);
            rig.store.script(Kind::Commit, vec![Act::hold()], None);
            rig.store
                .after_read(1, vec![rig.plant(key.clone(), damage)]);
            rig.store
                .after_read(2, vec![rig.plant(key, session), Step::ReleaseHeld]);
            committed_as(&rig.commit(attempt.clone()).await, &attempt);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{label}");
            assert_eq!(rig.releases(&attempt), 1, "{label}");
        })
        .await;
    }
}

/// **A damaged session under a held compensation keeps the doubt.** A loser's compensation is held
/// and the session is damaged, with each kind of damage (an `Aborting` record at an epoch that has
/// not risen included: read as a fence it would answer `NoSuchUpload` for a compensation that can
/// still land): the in-doubt error names the compensation and keeps the flag, and no further round
/// is sent. Repairing the session lets that batch land, and it is the one release. Run for a
/// non-empty part, whose obligation key is also read, and for an empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_a_damaged_session_under_a_held_compensation_keeps_the_doubt() {
    for bytes in [body(CHUNK, 226), Vec::new()] {
        for kind in 0..DAMAGE_KINDS {
            within(async {
                let mut rig = Rig::new().await;
                // One rollback first, so that a lower epoch exists for the damage to carry.
                rig.run(vec![rig.rollback()]).await;
                let attempt = rig.stage(Through::Wrapper, 1, &bytes).await;
                let rival = rig.stage(Through::Inner, 1, &bytes).await;
                let session = rig.session().await;
                let epoch = decode_session_record(&session).unwrap().epoch();
                let (label, damage, expected) = session_damage(&rig, &session).swap_remove(kind);
                let at = format!("{label}, {} bytes", bytes.len());
                rig.store.script(
                    Kind::Commit,
                    vec![Act::pass().before(rig.rival(rival))],
                    None,
                );
                rig.store.script(
                    Kind::Compensation,
                    vec![Act::hold().after(rig.plant(mpu_key(&rig.upload), damage))],
                    None,
                );
                let doubt = in_doubt(rig.commit(attempt.clone()).await);
                assert_eq!(doubt.batch, DoubtfulBatch::Compensation, "{at}");
                assert!(doubt.may_still_commit, "{at}: damage settles nothing");
                assert_record_stop(&doubt.cause, expected.as_ref(), &at);
                assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "{at}");
                assert_eq!(
                    rig.slots_of(&attempt).await,
                    1,
                    "{at}: covered by the doubt"
                );

                let repair = rig.plant(mpu_key(&rig.upload), session.clone());
                rig.run(vec![repair]).await;
                assert_eq!(
                    rig.store.release_held().await,
                    CommitOutcome::Committed,
                    "{at}: the repaired session lets the held batch land"
                );
                rig.assert_compensated(&attempt, epoch).await;
            })
            .await;
        }
    }
}

/// **A record whose epoch has not risen is damage, never a rollback and never a fence.** With
/// nothing in doubt, at the commit's re-read and at the compensation's, for an `Open` record under
/// other bytes at the same epoch, an `Open` record at a lower epoch, and an `Aborting` record at the
/// same and at a lower epoch: `Stop::Record` naming both epochs, never `NoSuchUpload`, no chase, and
/// nothing sent under the planted record. A fence raises the epoch, so a non-`Open` record at one
/// that has not risen is no fence this attempt lost to.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence() {
    for phase in ["commit", "compensation"] {
        for kind in [
            "Open, same",
            "Open, lower",
            "Aborting, same",
            "Aborting, lower",
        ] {
            within(async {
                let at = format!("{kind} epoch at the {phase}'s re-read");
                let mut rig = Rig::new().await;
                let older = rig.session().await;
                rig.run(vec![rig.rollback()]).await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 227)).await;
                let session = rig.session().await;
                let required = decode_session_record(&session).unwrap().epoch();
                assert_eq!(required, 2, "one rollback: Open@0 to Open@2");
                let (planted, found) = match kind {
                    "Open, same" => (same_epoch_other_bytes(&session), required),
                    "Open, lower" => (older, 0),
                    "Aborting, same" => (aborting_at(&session, &rig.upload, required), required),
                    _ => (aborting_at(&session, &rig.upload, 1), 1),
                };
                let expected = RecordError::SessionEpochNotRisen { found, required };
                let damage = rig.plant(mpu_key(&rig.upload), planted);
                if phase == "commit" {
                    rig.store
                        .script(Kind::Commit, vec![Act::pass().before(damage)], None);
                } else {
                    let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 228)).await;
                    rig.store.script(
                        Kind::Commit,
                        vec![Act::pass().before(rig.rival(rival))],
                        None,
                    );
                    rig.store.script(
                        Kind::Compensation,
                        vec![Act::conflict().before(damage)],
                        None,
                    );
                }
                let cause = stopped(rig.commit(attempt.clone()).await, false);
                assert_record_stop(&cause, Some(&expected), &at);
                let text = cause.to_string();
                assert!(text.contains("only a higher epoch"), "{at}: {text}");
                let sent = usize::from(phase == "compensation");
                assert_eq!(rig.log.commits(Kind::Compensation).len(), sent, "{at}");
                assert_eq!(rig.slots_of(&attempt).await, 1, "{at}: the slot stays");
            })
            .await;
        }
    }
}

/// **A prior part record gone under the same session is damage, not a rival.** A `part:` record is
/// removed only after its session leaves `Open`, so nothing explains this but damage: an explicit
/// record stop, never `OperationAborted`, with no compensation, and the slot and owned entries left.
/// It settles nothing either, so a held commit keeps the doubt and lands once the record is back. The
/// own-commit proof still comes first, and a session that moved answers by its own row. Run over a
/// non-empty prior and an empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_prior_part_record_gone_under_the_same_session_is_damage() {
    let missing = RecordError::PartRecordMissing { part_number: 1 };
    for (seed, prior) in [(229u8, body(CHUNK, 230)), (231, Vec::new())] {
        let at = format!("{} prior bytes", prior.len());
        let setup = |seed: u8| {
            let prior = prior.clone();
            async move {
                let mut rig = Rig::new().await;
                rig.committed(1, &prior).await;
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, seed)).await;
                let keys = vec![
                    part_key(&rig.upload, part(1)),
                    psum_key(&rig.upload, part(1)),
                ];
                (rig, attempt, keys)
            }
        };
        within(async {
            // Nothing in doubt: the commit answers `Conflict`.
            let (rig, attempt, keys) = setup(seed).await;
            let gone = erased(&rig, keys);
            rig.store
                .script(Kind::Commit, vec![Act::pass().before(gone)], None);
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert_record_stop(&cause, Some(&missing), &at);
            assert!(
                cause.to_string().contains("part 1's record is gone"),
                "{cause}"
            );
            assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "{at}: no retry");
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
            assert_eq!(rig.slots_of(&attempt).await, 1, "{at}: the slot stays");
            assert_eq!(rig.owned_of(&attempt).await, 1, "{at}: the entry stays");

            // A commit out of flight is settled by the held slot: the same stop.
            let (rig, attempt, keys) = setup(seed).await;
            let gone = erased(&rig, keys);
            rig.store.script(
                Kind::Commit,
                vec![Act::unknown(false, false).after(gone)],
                None,
            );
            let cause = stopped(rig.commit(attempt.clone()).await, false);
            assert_record_stop(&cause, Some(&missing), &at);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");

            // A held commit: the doubt stands, and restoring the record lets the batch land.
            let (rig, attempt, keys) = setup(seed).await;
            let part_bytes = rig.inner.get(&keys[0]).await.unwrap().unwrap();
            let psum_bytes = rig.inner.get(&keys[1]).await.unwrap().unwrap();
            let gone = erased(&rig, keys.clone());
            rig.store
                .script(Kind::Commit, vec![Act::hold().after(gone)], None);
            let doubt = in_doubt(rig.commit(attempt.clone()).await);
            assert_eq!(doubt.batch, DoubtfulBatch::Commit, "{at}");
            assert!(doubt.may_still_commit, "{at}: damage settles nothing");
            assert_record_stop(&doubt.cause, Some(&missing), &at);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
            let batch = WriteBatch::new()
                .put(keys[0].clone(), part_bytes)
                .put(keys[1].clone(), psum_bytes);
            assert_eq!(
                rig.inner.commit(batch).await.unwrap(),
                CommitOutcome::Committed
            );
            assert_eq!(rig.store.release_held().await, CommitOutcome::Committed);
            assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "{at}");

            // The commit landed first: the own-commit proof, whatever the part key holds.
            let (rig, attempt, keys) = setup(seed).await;
            let gone = erased(&rig, keys);
            rig.store.script(
                Kind::Commit,
                vec![Act::unknown(true, false).after(gone)],
                None,
            );
            committed_as(&rig.commit(attempt.clone()).await, &attempt);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");

            // Gone under a rollback: the newer epoch's row, not damage.
            let (rig, attempt, keys) = setup(seed).await;
            let gone = erased(&rig, keys);
            rig.store.script(
                Kind::Commit,
                vec![Act::pass().before(gone).before(rig.rollback())],
                None,
            );
            refused(
                &rig.commit(attempt.clone()).await,
                Refusal::OperationAborted,
            );
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "{at}");
            // Not `assert_compensated`: the erased prior left its own chunks named by nothing,
            // which is the damage this leg plants.
            assert_eq!(
                rig.part_obligations().await,
                vec![(token_of(&attempt, 2), chunk_ids(attempt.chunks()))],
                "{at}"
            );
            assert_eq!(rig.slots_of(&attempt).await, 0, "{at}");
            assert_eq!(rig.owned_of(&attempt).await, 0, "{at}");
            assert_eq!(rig.releases(&attempt), 1, "{at}");

            // Gone under a fence: the fence's row, with no compensation.
            let (rig, attempt, keys) = setup(seed).await;
            let gone = erased(&rig, keys);
            rig.store.script(
                Kind::Commit,
                vec![Act::pass().before(gone).before(rig.abort())],
                None,
            );
            refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 0, "{at}");
        })
        .await;
    }
}

/// **Every commit round out of flight.** Every round answers `CommitUnknownResult` with
/// `may_still_commit: false` and applies nothing. Each is settled by the slot read after it, so when
/// the rounds run out nothing is in doubt: the attempt compensates and returns
/// `Stopped(CommitRoundsSpent)` with the slot released. Run for a non-empty part and an empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_every_commit_round_out_of_flight_compensates_and_reports_the_release() {
    for bytes in [body(CHUNK, 232), Vec::new()] {
        within(async {
            let at = format!("{} bytes", bytes.len());
            let mut rig = Rig::new().await;
            let attempt = rig.stage(Through::Wrapper, 1, &bytes).await;
            rig.store
                .script(Kind::Commit, Vec::new(), Some(Act::unknown(false, false)));
            let cause = stopped(rig.commit(attempt.clone()).await, true);
            assert!(
                matches!(cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
                "{at}: {cause:?}"
            );
            assert_eq!(rig.log.commits(Kind::Commit).len(), 4, "{at}");
            assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "{at}");
            rig.assert_compensated(&attempt, 0).await;
            assert!(rig.part(1).await.is_none(), "{at}");
        })
        .await;
    }
}

/// **A slot rewritten under the attempt's own id during a held batch.** The bytes are not the ones
/// the attempt last wrote and the id is its own, so this is neither "held" nor "another attempt's":
/// the in-doubt error with the cause `SlotLost`, never `Committed` and never a compensation. Run
/// under a held commit and under a held compensation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 233)).await;
        let rewrite = other_slot_bytes(&rig, &attempt, "rewritten");
        rig.store
            .script(Kind::Commit, vec![Act::hold().after(rewrite)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit);
        assert_slot_lost(&doubt.cause, &attempt);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert!(rig.part(1).await.is_none());

        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 234)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 235)).await;
        let rewrite = other_slot_bytes(&rig, &attempt, "rewritten");
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold().after(rewrite)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(doubt.may_still_commit);
        assert_slot_lost(&doubt.cause, &attempt);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no retry");
    })
    .await;
}

/// **A missing `mpu:` record is a terminal delete only with the slot gone too.** A real terminal
/// delete removes the session record **and** the session's surviving `slot:` records in one batch
/// (`0016:674`), so an absent record with this attempt's slot still held is damage, not the session
/// moved for good: the attempt stops with `Stop::Record` (`SessionRecordMissing`), keeping any
/// doubt and `may_still_commit`, and restoring the record lets a held batch land. With the slot
/// gone too it is the terminal delete: `NoSuchUpload`, the residue the delete already took. Run at
/// the commit's re-read and at the compensation's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone() {
    let missing = |cause: &Stop| {
        matches!(
            cause,
            Stop::Record(RecordError::SessionRecordMissing { .. })
        )
    };
    within(async {
        // The commit's re-read, nothing in doubt, only the record erased: the slot is held, so this
        // is damage, not the session moved for good.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 236)).await;
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(gone)], None);
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(missing(&cause), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot stays");

        // The commit's re-read, nothing in doubt, the record and the slot both gone: the terminal
        // delete, answered `NoSuchUpload`.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 236)).await;
        let gone = erased(
            &rig,
            vec![mpu_key(&rig.upload), attempt.slot_key().to_vec()],
        );
        rig.store
            .script(Kind::Commit, vec![Act::pass().before(gone)], None);
        refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // A held commit, only the record erased: the slot held is damage, so the doubt and the
        // flag are kept, no retry and no compensation; restoring the record lets the held batch
        // land.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 237)).await;
        let session = rig.session().await;
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store
            .script(Kind::Commit, vec![Act::hold().after(gone)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "damage settles nothing");
        assert!(missing(&doubt.cause), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
        rig.run(vec![rig.plant(mpu_key(&rig.upload), session)])
            .await;
        assert_eq!(
            rig.store.release_held().await,
            CommitOutcome::Committed,
            "the restored record lets the held batch land"
        );
        assert!(is_own(&rig.part(1).await.unwrap().0, &attempt));
        assert_eq!(rig.releases(&attempt), 1);

        // A held commit that landed before the record and the slot went: no read can say so.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 238)).await;
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store.script(
            Kind::Commit,
            vec![Act::hold().after(Step::ReleaseHeld).after(gone)],
            None,
        );
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(!doubt.may_still_commit, "that session is gone for good");
        assert!(
            matches!(doubt.cause, Stop::SessionMoved),
            "{:?}",
            doubt.cause
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // The compensation's re-read, nothing in doubt, only the record erased: the slot held, so
        // damage.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 239)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 240)).await;
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::conflict().before(gone)], None);
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(missing(&cause), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
        assert!(rig.part_obligations().await.is_empty());
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot stays");
        // Its obligation, its slot, the session: nothing was in doubt, so no second lap of reads.
        assert_eq!(rig.log.reads_after_last(Kind::Compensation), 3);

        // The compensation's re-read, nothing in doubt, the record and the slot both gone:
        // `NoSuchUpload`.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 239)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 240)).await;
        let gone = erased(
            &rig,
            vec![mpu_key(&rig.upload), attempt.slot_key().to_vec()],
        );
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::conflict().before(gone)], None);
        refused(&rig.commit(attempt.clone()).await, Refusal::NoSuchUpload);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
        assert!(rig.part_obligations().await.is_empty());

        // A held empty compensation, only the record erased: the slot held is damage, so the doubt
        // and the flag are kept; restoring the record lets the held batch land, and it is the one
        // release.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        let rival = rig.stage(Through::Inner, 1, &[]).await;
        let session = rig.session().await;
        let epoch = decode_session_record(&session).unwrap().epoch();
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold().after(gone)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(doubt.may_still_commit, "damage settles nothing");
        assert!(missing(&doubt.cause), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
        assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
        rig.run(vec![rig.plant(mpu_key(&rig.upload), session)])
            .await;
        assert_eq!(
            rig.store.release_held().await,
            CommitOutcome::Committed,
            "the restored record lets the held batch land"
        );
        rig.assert_compensated(&attempt, epoch).await;

        // A held empty compensation that lands right after its slot read, and then the record and
        // the slot go: the one read that says so is the slot read of the next lap.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &[]).await;
        let rival = rig.stage(Through::Inner, 1, &[]).await;
        let gone = erased(&rig, vec![mpu_key(&rig.upload)]);
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold()], None);
        // The commit's four reads, then the compensation's slot read.
        rig.store.after_read(5, vec![Step::ReleaseHeld, gone]);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(!doubt.may_still_commit, "that session is gone for good");
        assert!(
            matches!(doubt.cause, Stop::SessionMoved),
            "{:?}",
            doubt.cause
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 1, "no more");
    })
    .await;
}

// ─────────────── the sign-off's legs (attempt 5) ───────────────

/// The stored bytes of part `n`.
async fn part_bytes(rig: &Rig, n: u32) -> Bytes {
    rig.inner
        .get(&part_key(&rig.upload, part(n)))
        .await
        .unwrap()
        .expect("the part is committed")
}

/// A step in the shape of the custodian's staged re-place adoption
/// (`crates/custodian/src/reconstruction/staged.rs`, `0016:671`): `require(mpu == Open@E)`,
/// `require(part == the bytes read)`, and the part record put back with chunk `chunk`'s first
/// fragment placed on `to` and every other byte kept, derived from the stored bytes exactly as
/// `repointed_part` derives it. A fragment moved from one server to another and, on a later pass,
/// back again writes the prior's exact bytes a second time.
fn repoint(rig: &Rig, n: u32, chunk: usize, to: DServerId) -> Step {
    let inner = Arc::clone(&rig.inner);
    let upload = rig.upload.clone();
    Step::Run(once(async move {
        let session_key = mpu_key(&upload);
        let session = inner.get(&session_key).await.unwrap().unwrap();
        let key = part_key(&upload, part(n));
        let stored = inner.get(&key).await.unwrap().expect("the part exists");
        let record = decode_part_record(&stored).unwrap();
        let mut chunks = record.chunks().to_vec();
        chunks[chunk].placement[0] = to;
        let list = metadata::encode(&record.chunks());
        let at = stored
            .windows(list.len())
            .position(|window| window == &list[..])
            .expect("the stored bytes spell the chunk list canonically");
        let mut next = Vec::with_capacity(stored.len());
        next.extend_from_slice(&stored[..at]);
        next.extend_from_slice(&metadata::encode(&chunks));
        next.extend_from_slice(&stored[at + list.len()..]);
        let read_back = decode_part_record(&next).expect("the repointed record decodes");
        assert_eq!(read_back.chunks(), chunks.as_slice());
        assert_eq!(read_back.digest(), record.digest());
        let batch = WriteBatch::new()
            .require(session_key, session)
            .require(key.clone(), stored)
            .put(key, next);
        assert_eq!(
            inner.commit(batch).await.unwrap(),
            CommitOutcome::Committed,
            "the re-place"
        );
    }))
}

/// **A changed non-empty prior settles nothing** (the sign-off of attempt 4). A re-upload over a
/// non-empty prior is held, and a same-number rival lands. The rival's record does not prove the
/// held commit can never land, because a repair can write the prior's exact bytes back (the next
/// leg), so the attempt keeps the doubt as it does over an empty prior: no compensation, and the
/// in-doubt error with the flag kept, never `OperationAborted`. If a compensation is ever sent, the
/// wrapper makes that history real: it restores the prior's bytes, lands the held commit, and
/// answers the compensation unknown, so an attempt that treats a non-empty prior as settled answers
/// `OperationAborted` for a part that committed. The held batch, released over the rival's record,
/// conflicts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(2 * CHUNK, 241)).await;
        assert!(!prior.is_empty());
        let restore = rig.plant(part_key(&rig.upload, part(1)), part_bytes(&rig, 1).await);
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 242)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 243)).await;
        rig.store.script(
            Kind::Commit,
            vec![Act::hold().after(rig.rival(rival.clone()))],
            Some(Act::conflict()),
        );
        rig.store.script(
            Kind::Compensation,
            vec![Act::unknown(false, true)
                .before(restore)
                .before(Step::ReleaseHeld)],
            None,
        );
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(
            matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
            "{:?}",
            doubt.cause
        );
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation while the commit can still land"
        );
        assert_eq!(rig.slots_of(&attempt).await, 1, "covered by the doubt");
        assert!(is_own(&rig.part(1).await.unwrap().0, &rival));
        assert_eq!(rig.store.release_held().await, CommitOutcome::Conflict);
        assert_eq!(rig.releases(&attempt), 0, "nothing of this attempt landed");
        assert!(is_own(&rig.part(1).await.unwrap().0, &rival));
    })
    .await;
}

/// When the prior's bytes come back in [`b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restored {
    /// At the end of the first lap of reads, before the retry, which then lands itself.
    BeforeRetry,
    /// Right after the retry conflicted, and the held batch is released with it.
    WithRelease,
    /// After the attempt returned in doubt, and the held batch is released with it.
    AfterReturn,
}

/// **A held re-upload while repair moves the prior away and back** (the sign-off of attempt 4; the
/// custodian's staged reconstruction, `0016:671`). A re-upload over a non-empty prior is held. A
/// batch in the re-place adoption's exact shape then moves one of the prior's fragments to another
/// server, so the part key no longer holds what the held commit requires, at each gap of the first
/// lap of reads in turn; a later re-place moves it back, so the prior's exact bytes return and the
/// held commit can land. Required, in every run: no compensation while the commit may land, never
/// `OperationAborted`, the answer `Committed` or the in-doubt error keeping the flag, and at the end
/// the attempt's own part durable, the prior's chunks owed, and the slot released exactly once. If
/// a compensation is ever sent, the wrapper makes the race real: it moves the prior back, lands the
/// held commit, and answers the compensation unknown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back() {
    for restored in [
        Restored::BeforeRetry,
        Restored::WithRelease,
        Restored::AfterReturn,
    ] {
        for away_at in 0..=4usize {
            within(async {
                let at = format!("{restored:?}, moved away at gap {away_at}");
                let mut rig = Rig::new().await;
                let prior = rig.committed(1, &body(2 * CHUNK, 244)).await;
                let placement = &prior.chunks()[0].placement;
                let first = placement[0];
                let spare = (0..SERVERS)
                    .find(|id| !placement.contains(id))
                    .expect("a server outside the chunk's placement");
                let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 245)).await;
                let away = repoint(&rig, 1, 0, spare);
                let back = repoint(&rig, 1, 0, first);
                let mut held = Act::hold();
                if away_at == 0 {
                    held = held.after(away);
                } else {
                    rig.store.after_read(away_at, vec![away]);
                }
                // `back` runs once: by a hook, in the retry's wake, or after the return.
                let mut later = None;
                let (plan, then) = match restored {
                    Restored::BeforeRetry => {
                        rig.store.after_read(4, vec![back]);
                        (vec![held], Some(Act::pass()))
                    }
                    Restored::WithRelease => (
                        vec![held, Act::conflict().after(back).after(Step::ReleaseHeld)],
                        Some(Act::conflict()),
                    ),
                    Restored::AfterReturn => {
                        later = Some(back);
                        (vec![held], Some(Act::conflict()))
                    }
                };
                rig.store.script(Kind::Commit, plan, then);
                rig.store.script(
                    Kind::Compensation,
                    vec![Act::unknown(false, true)
                        .before(repoint(&rig, 1, 0, first))
                        .before(Step::ReleaseHeld)],
                    None,
                );
                let outcome = rig.commit(attempt.clone()).await;
                assert_eq!(
                    rig.log.commits(Kind::Compensation).len(),
                    0,
                    "{at}: no compensation while the commit may land"
                );
                match restored {
                    Restored::BeforeRetry | Restored::WithRelease => {
                        committed_as(&outcome, &attempt);
                        assert_eq!(rig.log.commits(Kind::Commit).len(), 2, "{at}");
                    }
                    Restored::AfterReturn => {
                        let doubt = in_doubt(outcome);
                        assert_eq!(doubt.batch, DoubtfulBatch::Commit, "{at}");
                        assert!(doubt.may_still_commit, "{at}: the doubt is kept");
                        assert!(
                            matches!(doubt.cause, Stop::CommitRoundsSpent { rounds } if rounds == COMMIT_ROUNDS),
                            "{at}: {:?}",
                            doubt.cause
                        );
                        assert_eq!(rig.slots_of(&attempt).await, 1, "{at}: covered by the doubt");
                        rig.run(later.take().into_iter().collect()).await;
                    }
                }
                if rig.store.held_count() > 0 {
                    let expected = if restored == Restored::BeforeRetry {
                        // The retry landed, so the held batch finds the slot gone.
                        CommitOutcome::Conflict
                    } else {
                        CommitOutcome::Committed
                    };
                    assert_eq!(rig.store.release_held().await, expected, "{at}");
                }
                assert!(is_own(&rig.part(1).await.unwrap().0, &attempt), "{at}");
                assert_eq!(
                    rig.part_obligations().await,
                    vec![(token_of(&attempt, 0), chunk_ids(prior.chunks()))],
                    "{at}: the prior's chunks are owed"
                );
                assert_eq!(rig.releases(&attempt), 1, "{at}: released once");
                assert_eq!(rig.slots_of(&attempt).await, 0, "{at}");
                assert_eq!(rig.owned_of(&attempt).await, 0, "{at}");
                rig.assert_every_fragment_named().await;
            })
            .await;
        }
    }
}

/// **A store fault while a held compensation is being settled keeps the doubt** (the sign-off of
/// attempt 4). A loser's compensation is held, and then its next round fails outright, or the read
/// of its obligation, of its slot, or of the session faults. Each answers the in-doubt error naming
/// the compensation, with `may_still_commit` kept and the cause `Store`, never a plain stop: the
/// held batch can still land, and does once released, which is the one release. Run for a non-empty
/// part, whose obligation is read first, and for an empty one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt() {
    for (bytes, faults) in [
        (
            body(CHUNK, 246),
            vec![
                ("round", 0usize),
                ("obligation", 5),
                ("slot", 6),
                ("session", 7),
            ],
        ),
        (Vec::new(), vec![("round", 0), ("slot", 5), ("session", 6)]),
    ] {
        for (fault, read) in faults {
            within(async {
                let at = format!("{fault} faults, {} bytes", bytes.len());
                let mut rig = Rig::new().await;
                let attempt = rig.stage(Through::Wrapper, 1, &bytes).await;
                let rival = rig.stage(Through::Inner, 1, &bytes).await;
                rig.store.script(
                    Kind::Commit,
                    vec![Act::pass().before(rig.rival(rival))],
                    None,
                );
                if fault == "round" {
                    rig.store
                        .script(Kind::Compensation, vec![Act::hold(), Act::fail()], None);
                } else {
                    rig.store
                        .script(Kind::Compensation, vec![Act::hold()], None);
                    // The commit's four reads, then the compensation's.
                    rig.store.fail_read(read);
                }
                let doubt = in_doubt(rig.commit(attempt.clone()).await);
                assert_eq!(doubt.batch, DoubtfulBatch::Compensation, "{at}");
                assert!(
                    doubt.may_still_commit,
                    "{at}: the held round may still land"
                );
                assert!(
                    doubt.outcome.may_still_commit,
                    "{at}: the held round's report"
                );
                assert!(
                    matches!(doubt.cause, Stop::Store(_)),
                    "{at}: {:?}",
                    doubt.cause
                );
                let sent = if fault == "round" { 2 } else { 1 };
                assert_eq!(rig.log.commits(Kind::Compensation).len(), sent, "{at}");
                assert_eq!(
                    rig.slots_of(&attempt).await,
                    1,
                    "{at}: covered by the doubt"
                );
                assert_eq!(
                    rig.store.release_held().await,
                    CommitOutcome::Committed,
                    "{at}"
                );
                rig.assert_compensated(&attempt, 0).await;
            })
            .await;
        }
    }
}

/// **A re-upload above epoch 0** (the sign-off of attempt 4). Part 1 is committed under `Open@0`; a
/// re-upload is staged, and a Complete rolls the session back to `Open@2` before the quiesce renewal,
/// which adopts it. The commit lands under the live epoch, and the obligation owing the prior's chunks
/// is keyed under that epoch too, the epoch of the session the batch required, never a fixed 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_reupload_under_a_newer_epoch_owes_the_prior_under_that_epoch() {
    within(async {
        let mut rig = Rig::new().await;
        let prior = rig.committed(1, &body(CHUNK, 247)).await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(2 * CHUNK, 248)).await;
        rig.store.script(
            Kind::Renewal,
            vec![Act::conflict().before(rig.rollback())],
            None,
        );
        committed_as(&rig.commit(attempt.clone()).await, &attempt);
        let commits = rig.log.commits(Kind::Commit);
        assert_eq!(commits.len(), 1);
        assert_eq!(
            required_epoch(&commits[0].0, &rig.upload),
            2,
            "the live epoch"
        );
        let (record, _) = rig.part(1).await.unwrap();
        assert!(is_own(&record, &attempt));
        assert_eq!(record.session_epoch(), 2);
        assert_eq!(
            rig.part_obligations().await,
            vec![(token_of(&attempt, 2), chunk_ids(prior.chunks()))],
            "the prior's chunks, owed under the live epoch"
        );
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 0);
        rig.assert_every_fragment_named().await;
    })
    .await;
}

/// **A damaged `part:` record under a session that moved** (the sign-off of attempt 4). The commit
/// answers `Conflict`; the part key holds bytes that do not decode, and the session is back at
/// `Open@E'`. Damage is judged before the session's row: `Stop::Record`, no compensation under
/// either epoch, and the slot and the owned entry stay.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_a_damaged_part_record_under_a_moved_session_is_a_record_stop() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 249)).await;
        let damage = rig.plant(part_key(&rig.upload, part(1)), Bytes::from_static(DAMAGE));
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(damage).before(rig.rollback())],
            None,
        );
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(matches!(cause, Stop::Record(_)), "{cause:?}");
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot stays");
        assert_eq!(rig.owned_of(&attempt).await, 1, "the entry stays");
        let session = decode_session_record(&rig.session().await).unwrap();
        assert_eq!(session.epoch(), 2, "the session moved");
    })
    .await;
}

// ─────────────── the sign-off's legs (attempt 6) ───────────────

/// **The quiesce renewal's adoption is rejected when the epoch has not risen** (the sign-off of
/// attempt 6). The renewal (`stage.rs`) re-reads the session when its round conflicts and adopts
/// whatever `Open` bytes it finds, without judging the epoch. Here a round conflicts and an `Open`
/// record at the **same** epoch under other bytes is planted, so the renewal adopts bytes no legal
/// history wrote. The commit judges the adoption — the one session read `judge_session` does not
/// cover — and stops with `Stop::Record` (`SessionEpochNotRisen`), the slot held, no commit sent.
/// Without the check the commit would land under the adopted bytes and answer `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_the_quiesce_renewal_adoption_is_rejected_when_the_epoch_has_not_risen() {
    within(async {
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 252)).await;
        let session = rig.session().await;
        let damaged = same_epoch_other_bytes(&session);
        let plant = rig.plant(mpu_key(&rig.upload), damaged);
        rig.store
            .script(Kind::Renewal, vec![Act::conflict().before(plant)], None);
        let cause = stopped(rig.commit(attempt.clone()).await, false);
        assert!(
            matches!(
                cause,
                Stop::Record(RecordError::SessionEpochNotRisen { found, required })
                    if found == required
            ),
            "{cause:?}"
        );
        assert_eq!(
            rig.log.commits(Kind::Commit).len(),
            0,
            "no commit under the adopted bytes"
        );
        assert_eq!(
            rig.log.commits(Kind::Compensation).len(),
            0,
            "no compensation"
        );
        assert_eq!(rig.slots_of(&attempt).await, 1, "the slot stays");
    })
    .await;
}

/// **Damage found at a re-read while a batch is held keeps the doubt** (the sign-off of attempt 6).
/// Each of three doubt-keeping re-reads returns the in-doubt error with `may_still_commit` kept and
/// the cause named, never a plain stop that would drop the doubt: an undecodable slot at the
/// commit's re-read, a taken obligation key at the compensation's, and an undecodable slot at the
/// compensation's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_damage_at_a_re_read_keeps_the_doubt() {
    within(async {
        // Undecodable slot under a held commit.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 253)).await;
        let torn = rig.plant(
            attempt.slot_key().to_vec(),
            Bytes::from_static(b"torn slot"),
        );
        rig.store
            .script(Kind::Commit, vec![Act::hold().after(torn)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Commit);
        assert!(doubt.may_still_commit, "the held commit may still land");
        assert!(matches!(doubt.cause, Stop::Record(_)), "{:?}", doubt.cause);
        assert_eq!(rig.log.commits(Kind::Commit).len(), 1, "no retry");
        assert_eq!(rig.log.commits(Kind::Compensation).len(), 0);

        // A taken obligation key under a held compensation (a non-empty part owes an obligation).
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 254)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 255)).await;
        let key = retire_key(RetireMode::Bytes, &token_of(&attempt, 0));
        let taken = rig.plant(key, Bytes::from_static(b"other"));
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold().after(taken)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(
            doubt.may_still_commit,
            "the held compensation may still land"
        );
        assert!(
            matches!(doubt.cause, Stop::ObligationKeyTaken { .. }),
            "{:?}",
            doubt.cause
        );

        // Undecodable slot under a held compensation.
        let mut rig = Rig::new().await;
        let attempt = rig.stage(Through::Wrapper, 1, &body(CHUNK, 156)).await;
        let rival = rig.stage(Through::Inner, 1, &body(CHUNK, 157)).await;
        let torn = rig.plant(
            attempt.slot_key().to_vec(),
            Bytes::from_static(b"torn slot"),
        );
        rig.store.script(
            Kind::Commit,
            vec![Act::pass().before(rig.rival(rival))],
            None,
        );
        rig.store
            .script(Kind::Compensation, vec![Act::hold().after(torn)], None);
        let doubt = in_doubt(rig.commit(attempt.clone()).await);
        assert_eq!(doubt.batch, DoubtfulBatch::Compensation);
        assert!(
            doubt.may_still_commit,
            "the held compensation may still land"
        );
        assert!(matches!(doubt.cause, Stop::Record(_)), "{:?}", doubt.cause);
    })
    .await;
}

/// **The slot-bytes guard reads the owner it recorded, not the batch's own bytes** (the sign-off of
/// attempt 6). Attempt A reserves a slot; A's index is freed and attempt B reserves it, so the
/// wrapper has recorded B as the slot's owner independently of any later batch. A batch that
/// requires that slot carrying A's recorded bytes — which no conforming attempt builds, since each
/// requires the bytes its own state holds — is rejected by comparing the claimed id to the recorded
/// owner, where reading the id out of the batch's own `expected` bytes would have let it pass. B's
/// own batch, requiring the slot it owns at the bytes it last wrote, is accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_slot_bytes_guard_reads_the_recorded_owner_not_the_batchs_own_bytes() {
    within(async {
        let mut rig = Rig::new().await;
        let a = rig.stage(Through::Wrapper, 1, &body(CHUNK, 158)).await;
        rig.run(vec![slot_deleted(&rig, &a)]).await;
        let b = rig.stage(Through::Wrapper, 1, &body(CHUNK, 159)).await;
        assert_eq!(a.slot_key(), b.slot_key(), "B reserved the freed index");
        assert_ne!(
            a.slot_bytes(),
            b.slot_bytes(),
            "the two reserves wrote different bytes"
        );
        let bad = WriteBatch::new()
            .require(a.slot_key().to_vec(), a.slot_bytes().clone())
            .delete(a.slot_key().to_vec());
        let err = rig
            .store
            .check_slot_bytes(&bad)
            .expect_err("the guard rejects a batch carrying another recorded attempt's bytes");
        assert!(err.contains("reserved it"), "{err}");
        let good = WriteBatch::new()
            .require(b.slot_key().to_vec(), b.slot_bytes().clone())
            .delete(b.slot_key().to_vec());
        rig.store
            .check_slot_bytes(&good)
            .expect("B's own batch, requiring the slot it owns, passes");
    })
    .await;
}
