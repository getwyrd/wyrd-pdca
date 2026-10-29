//! Issue #657 — `UploadPart`'s **staging**, `wyrd_core::multipart::upload_part`: the slot
//! reserve, the owned intent per chunk, the renewal, and the part commit or the live-session
//! compensation (proposal 0016's batch rows `0016:658-660`, `:673`, and decision 5,
//! `0016:1507-1793`), driven over a real store (`RedbMetadataStore::in_memory()`) and a
//! placement-aware chunk-store double on a multi-thread tokio runtime.
//!
//! Every session here is made by Create (`create_session`, #894). A store wrapper logs every
//! batch the attempt under test sends, and can act before or instead of a chosen commit: write a
//! drain record, fence the session, hold the batch, answer an unknown outcome, or wait for the test
//! to run a rival. The chunk-store double logs every fragment write with its deadline, into the
//! same log, so the order of intents and bytes is observable. The clock is a `ManualClock` the
//! wrapper can advance right after an intent lands.
//!
//! * **(A)** parts staged out of order commit, with `part:` and `psum:` agreeing; mid-stage the
//!   store holds `sidx:` entries and no `pending:` one; each chunk's intent lands before its first
//!   fragment write; every fragment carries exactly `t_auth + W`, also when dispatched late.
//! * **(B)** the drain fence: a drain recorded just before an intent makes it re-plan (B1); a
//!   drained preferred server is never planned (B2); four of twelve domains draining still stage,
//!   and too few domains is a typed error, never `NoSuchUpload` (B3).
//! * **(C)** the slot key space is the cap; a reserve against a session that is not `Open` writes
//!   nothing; an unknown reserve that landed is adopted; an ambiguous one retries the same index.
//! * **(D)** every renewal rewrites the slot with the owned leases; a renewal after the fence
//!   resurrects nothing.
//! * **(E)** a lost commit is classified from what the store holds, never compensated blind.
//! * **(F)** re-upload, and the per-part and cumulative ceilings.
//!
//! The wrapper keeps its state in struct fields, never in a `static` (ADR-0035).

#![forbid(unsafe_code)]

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest as _, Sha256};
use tokio::sync::Notify;
use wyrd_core::metadata::{
    self, desired_dserver_key, ChunkRef, EcScheme, InodeId, PendingEntry, SegmentNonce,
    DESIRED_DSERVER_PREFIX,
};
use wyrd_core::multipart::abort::abort_session;
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::upload_part::{
    reserve_slot, upload_part, Residue, Stop, UploadPartError, UploadPartRequest,
    UPLOAD_PART_ROUNDS,
};
use wyrd_core::multipart::{
    decode_owned_entry, decode_part_record, decode_part_summary, decode_retire_obligation,
    decode_session_record, decode_slot_record, mpu_key, parse_sidx_key, parse_slot_key, part_key,
    psum_key, psum_range, retire_key, sidx_range, slot_range, AbortOutcome, AttemptId,
    Backpressure, Budget, CreateOutcome, Digest, PartNumber, PartRecord, PartSummary, Refusal,
    ReserveOutcome, RetireMode, RetireToken, SessionState, UploadId, UploadPartOutcome,
    MAX_INFLIGHT_PARTS, MAX_PART_CHUNKS, MAX_STAGED_CHUNKS, PART_PREFIX, SIDX_PREFIX, SLOT_PREFIX,
};
use wyrd_core::placement::{select_distinct_domains, SelectorError, Topology};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    BoxError, ChunkId, ChunkStore, CommitOutcome, CommitUnknownResult, DServerId, FragmentId,
    Health, MetadataStore, PlacementChunkStore, Result as StoreResult, ScanPage, WriteBatch,
};

// ─────────────────────────────── fixtures ───────────────────────────────

const BUCKET: &str = "photos";
/// The root inode every Alpha object dirent lives under, as `<bucket>/<key>` (ADR-0046).
const ROOT: InodeId = 0;
const CLOCK: &str = "wall";
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
/// No leg may hang the suite: a schedule that never settles fails here instead.
const LEG_TIMEOUT: Duration = Duration::from_secs(120);
/// More batches than any leg sends. An attempt past it never stops sending.
const RUNAWAY: usize = 2_000;
/// The clock's first reading.
const START: u64 = 1_000_000;
/// The lease TTL every attempt here stamps.
const TTL: u64 = 10_000;
/// `W_write`, the staged-write window every attempt here is given.
const WINDOW: u64 = 7_000;
/// Twelve D servers, each its own failure domain `d00`..`d11`.
const SERVERS: u64 = 12;
const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };

async fn within<T>(leg: impl Future<Output = T>) -> T {
    tokio::time::timeout(LEG_TIMEOUT, leg)
        .await
        .expect("the leg settled in time")
}

fn nz(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("non-zero")
}

fn part(number: u32) -> PartNumber {
    PartNumber::new(number).expect("a part number")
}

fn attempt(n: u128) -> AttemptId {
    AttemptId::new(format!("{:032x}", 0xa77e_0000_0000_u128 + n)).expect("an attempt id")
}

/// A profile admitting a few sessions. Admission is not under test here: the staging enforces
/// the protocol's own constants (`MAX_INFLIGHT_PARTS`, `MAX_PART_CHUNKS`, `MAX_STAGED_CHUNKS`).
fn profile() -> Budget {
    let json = "{\"w_ref\":16,\"max_part_chunks\":1,\"max_parts_per_session\":1,\
                \"max_inflight_parts\":1,\"max_staged_chunks\":1}";
    metadata::decode(json.as_bytes()).expect("a legal profile")
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

/// `n` bytes counting up from `seed`.
fn body(seed: u8, n: usize) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add(i as u8)).collect()
}

fn sha(bytes: &[u8]) -> Digest {
    Digest::from_bytes(Sha256::digest(bytes).into())
}

/// Twelve single-server domains, with the utilization given.
fn topology(utilization: &[(DServerId, u64)]) -> Topology {
    let mut topology = Topology::default();
    for id in 0..SERVERS {
        topology.register(id, format!("d{id:02}"));
    }
    for (id, used) in utilization {
        topology.set_utilization(*id, *used);
    }
    topology
}

// ───────────────────────────── the shared log ─────────────────────────────

/// Which batch of the protocol a batch is, by its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Reserve,
    Intent,
    Renewal,
    Commit,
    Compensation,
    Other,
}

fn kind(batch: &WriteBatch) -> Kind {
    let puts = |prefix: &[u8]| batch.puts.iter().any(|(key, _)| key.starts_with(prefix));
    let absent = |prefix: &[u8]| {
        batch
            .preconditions
            .iter()
            .any(|pre| pre.expected.is_none() && pre.key.starts_with(prefix))
    };
    let required = |prefix: &[u8]| {
        batch
            .preconditions
            .iter()
            .any(|pre| pre.expected.is_some() && pre.key.starts_with(prefix))
    };
    let deletes = |prefix: &[u8]| batch.deletes.iter().any(|key| key.starts_with(prefix));
    if puts(PART_PREFIX) {
        Kind::Commit
    } else if deletes(SLOT_PREFIX) {
        Kind::Compensation
    } else if absent(SIDX_PREFIX) {
        Kind::Intent
    } else if absent(SLOT_PREFIX) {
        Kind::Reserve
    } else if required(SLOT_PREFIX) {
        Kind::Renewal
    } else {
        Kind::Other
    }
}

/// What the wrapper answered a commit with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answered {
    Committed,
    Conflict,
    Unknown,
}

#[derive(Debug, Clone)]
enum Event {
    /// A batch the attempt under test sent: the clock reading when it arrived, what the wrapper
    /// answered, and whether it was applied to the store then.
    Commit {
        kind: Kind,
        batch: WriteBatch,
        at: u64,
        answered: Answered,
        landed: bool,
    },
    /// A held batch the test released, and whether it landed.
    Released { batch: WriteBatch, landed: bool },
    /// A fragment write, with its deadline and the clock reading when it was dispatched.
    Put {
        dserver: DServerId,
        id: FragmentId,
        deadline: Option<u64>,
        at: u64,
    },
}

type Log = Arc<Mutex<Vec<Event>>>;

// ───────────────────────────── the store wrapper ─────────────────────────────

/// What the wrapper does before a matched commit.
#[derive(Clone)]
enum Pre {
    /// Write `desired:dserver:<S>` on the inner store for the first server the batch's owned
    /// entry plans, recording `S`.
    DrainPlanned,
    /// Fence the session with a real Abort on the inner store.
    Abort(UploadId),
    /// Tell the test the batch arrived, and wait for its word.
    Gate {
        reached: Arc<Notify>,
        release: Arc<Notify>,
    },
    /// Apply the held batch to the inner store first.
    ReleaseHeld,
    /// Count what the store holds under `sidx:` and `pending:`.
    Snapshot,
}

/// What the wrapper answers a matched commit with.
#[derive(Clone, Copy)]
enum Then {
    /// Delegate.
    Pass,
    /// Delegate, then advance the clock by this much: a dispatch delayed after the commit.
    PassThenAdvance(u64),
    /// Answer `Conflict` without applying anything, as TiKV does when a locking read meets
    /// another writer's lock.
    Conflict,
    /// Apply the batch (whatever it meets) or not, then answer
    /// `CommitUnknownResult { may_still_commit }`.
    Unknown { apply: bool, may_still_commit: bool },
    /// Keep the batch unapplied and answer `CommitUnknownResult { may_still_commit: true }`; it
    /// lands when the test releases it.
    Hold,
}

/// A rule for the `nth` commits of one kind (1-based, inclusive range), counted across every
/// attempt that commits through the wrapper.
#[derive(Clone)]
struct Rule {
    kind: Kind,
    from: usize,
    to: usize,
    pre: Vec<Pre>,
    then: Then,
}

impl Rule {
    fn at(kind: Kind, nth: usize, pre: Vec<Pre>, then: Then) -> Self {
        Self {
            kind,
            from: nth,
            to: nth,
            pre,
            then,
        }
    }

    fn starting(kind: Kind, nth: usize, pre: Vec<Pre>, then: Then) -> Self {
        Self {
            kind,
            from: nth,
            to: usize::MAX,
            pre,
            then,
        }
    }
}

#[derive(Default)]
struct WrapperState {
    rules: Vec<Rule>,
    counts: HashMap<Kind, usize>,
    sent: usize,
    held: Option<WriteBatch>,
    drained: Vec<DServerId>,
    /// `(sidx: entries, pending: entries)` at each `Pre::Snapshot`.
    snapshots: Vec<(usize, usize)>,
}

struct Store {
    inner: RedbMetadataStore,
    clock: ManualClock,
    log: Log,
    state: Mutex<WrapperState>,
}

fn unknown(may_still_commit: bool) -> BoxError {
    BoxError::from(CommitUnknownResult {
        backend: "test wrapper",
        code: None,
        detail: "the wrapper withheld the outcome".to_owned(),
        may_still_commit,
    })
}

impl Store {
    fn state(&self) -> std::sync::MutexGuard<'_, WrapperState> {
        self.state.lock().unwrap()
    }

    fn script(&self, rules: Vec<Rule>) {
        self.state().rules = rules;
    }

    fn drained(&self) -> Vec<DServerId> {
        self.state().drained.clone()
    }

    fn snapshots(&self) -> Vec<(usize, usize)> {
        self.state().snapshots.clone()
    }

    /// Apply the held batch now; whether it landed.
    async fn release_held(&self) -> bool {
        let held = self.state().held.take().expect("a batch is held");
        let landed = self.inner.commit(held.clone()).await.unwrap() == CommitOutcome::Committed;
        self.log.lock().unwrap().push(Event::Released {
            batch: held,
            landed,
        });
        landed
    }

    async fn pre(&self, step: &Pre, batch: &WriteBatch) {
        match step {
            Pre::DrainPlanned => {
                let (_, value) = batch
                    .puts
                    .iter()
                    .find(|(key, _)| key.starts_with(SIDX_PREFIX))
                    .expect("an intent puts its owned entry");
                let entry: PendingEntry = metadata::decode(value).unwrap();
                let server = entry.staged.expect("an owned entry").placement()[0];
                let drain = WriteBatch::new().put(desired_dserver_key(server), &b"draining"[..]);
                self.inner.commit(drain).await.unwrap();
                self.state().drained.push(server);
            }
            Pre::Abort(upload_id) => {
                let fenced = abort_session(&self.inner, upload_id).await;
                assert!(matches!(fenced, Ok(AbortOutcome::Fenced)), "{fenced:?}");
            }
            Pre::Gate { reached, release } => {
                reached.notify_one();
                release.notified().await;
            }
            Pre::ReleaseHeld => {
                assert!(self.release_held().await, "the held batch lands");
            }
            Pre::Snapshot => {
                let sidx = self.inner.scan(SIDX_PREFIX).await.unwrap().len();
                let pending = self.inner.scan(b"pending:").await.unwrap().len();
                self.state().snapshots.push((sidx, pending));
            }
        }
    }
}

#[async_trait]
impl MetadataStore for Store {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        self.inner.scan(prefix).await
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
        let kind = kind(&batch);
        let at = self.clock.now_millis();
        let rule = {
            let mut state = self.state();
            state.sent += 1;
            assert!(state.sent <= RUNAWAY, "the attempt never stopped sending");
            let count = state.counts.entry(kind).or_default();
            *count += 1;
            let nth = *count;
            state
                .rules
                .iter()
                .find(|rule| rule.kind == kind && rule.from <= nth && nth <= rule.to)
                .cloned()
        };
        let (pre, then) = rule.map_or((Vec::new(), Then::Pass), |rule| (rule.pre, rule.then));
        for step in &pre {
            self.pre(step, &batch).await;
        }
        let (answered, landed, result) = match then {
            Then::Pass | Then::PassThenAdvance(_) => {
                let outcome = self.inner.commit(batch.clone()).await?;
                let landed = outcome == CommitOutcome::Committed;
                let answered = if landed {
                    Answered::Committed
                } else {
                    Answered::Conflict
                };
                (answered, landed, Ok(outcome))
            }
            Then::Conflict => {
                // A real store yields here; so must this one.
                tokio::task::yield_now().await;
                (Answered::Conflict, false, Ok(CommitOutcome::Conflict))
            }
            Then::Unknown {
                apply,
                may_still_commit,
            } => {
                let landed =
                    apply && self.inner.commit(batch.clone()).await? == CommitOutcome::Committed;
                (Answered::Unknown, landed, Err(unknown(may_still_commit)))
            }
            Then::Hold => {
                self.state().held = Some(batch.clone());
                (Answered::Unknown, false, Err(unknown(true)))
            }
        };
        self.log.lock().unwrap().push(Event::Commit {
            kind,
            batch,
            at,
            answered,
            landed,
        });
        if let Then::PassThenAdvance(millis) = then {
            if landed {
                self.clock.advance(millis);
            }
        }
        result
    }
}

// ───────────────────────────── the chunk store double ─────────────────────────────

/// In-process D servers addressed by stable id (the shape of `domain_placement.rs`'s `Fleet`),
/// logging every write with its deadline.
struct Fleet {
    servers: HashMap<DServerId, Mutex<HashMap<FragmentId, Bytes>>>,
    clock: ManualClock,
    log: Log,
}

#[async_trait]
impl ChunkStore for Fleet {
    async fn put_fragment(
        &self,
        _id: FragmentId,
        _fragment: Bytes,
        _deadline_millis: Option<u64>,
    ) -> StoreResult<()> {
        Err("Fleet: write must address a D server by id (use put_fragment_at)".into())
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
        self.log.lock().unwrap().push(Event::Put {
            dserver,
            id,
            deadline: deadline_millis,
            at: self.clock.now_millis(),
        });
        self.servers
            .get(&dserver)
            .ok_or_else(|| format!("Fleet: unknown D server {dserver}"))?
            .lock()
            .unwrap()
            .insert(id, fragment);
        Ok(())
    }
}

// ───────────────────────────── the fixture ─────────────────────────────

struct Fx {
    store: Store,
    fleet: Fleet,
    clock: ManualClock,
    log: Log,
    topology: Topology,
    upload: UploadId,
}

/// A store holding one `Open@0` session made by Create, a fleet of twelve D servers, and a
/// clock at [`START`].
async fn fixture_with(topology: Topology) -> Fx {
    let inner = RedbMetadataStore::in_memory().unwrap();
    inner
        .commit(WriteBatch::new().put(metadata::bucket_key(BUCKET), MARKER))
        .await
        .unwrap();
    let upload = UploadId::new(format!("{:032x}", 0x5e55_1011_u128)).unwrap();
    let nonce = SegmentNonce::new(format!("{:032x}", 0x0dd5_eed5_u128)).unwrap();
    let request = CreateRequest {
        bucket: BUCKET,
        parent: ROOT,
        object: "photos/staged",
        content_type: None,
        created_at_millis: START,
        clock_source: CLOCK,
    };
    let created = create_session(
        &inner,
        &profile(),
        &request,
        &mut Fixed(Some((upload, nonce))),
    )
    .await
    .expect("the create ran");
    let CreateOutcome::Created { upload_id } = created else {
        panic!("expected Created, got {created:?}");
    };
    let clock = ManualClock::new(START);
    let log: Log = Arc::default();
    Fx {
        store: Store {
            inner,
            clock: clock.clone(),
            log: log.clone(),
            state: Mutex::default(),
        },
        fleet: Fleet {
            servers: (0..SERVERS)
                .map(|id| (id, Mutex::new(HashMap::new())))
                .collect(),
            clock: clock.clone(),
            log: log.clone(),
        },
        clock,
        log,
        topology,
        upload: upload_id,
    }
}

async fn fixture() -> Fx {
    fixture_with(topology(&[])).await
}

/// One `UploadPart` of `bytes` as part `number` by `attempt_id`, over `meta` (the wrapper, or its
/// inner store for a rival the wrapper must not see), minting chunk ids above `ids`.
#[allow(clippy::too_many_arguments)]
async fn upload_over(
    fx: &Fx,
    meta: &impl MetadataStore,
    number: u32,
    attempt_id: &AttemptId,
    scheme: EcScheme,
    chunk_size: usize,
    bytes: &[u8],
    ids: u128,
) -> Result<UploadPartOutcome, UploadPartError> {
    let request = UploadPartRequest {
        upload_id: &fx.upload,
        part_number: part(number),
        attempt_id,
        scheme,
        chunk_size,
        topology: &fx.topology,
        lease_ttl_millis: nz(TTL),
        write_window_millis: nz(WINDOW),
    };
    let source =
        futures_util::stream::iter(vec![Ok::<Bytes, BoxError>(Bytes::copy_from_slice(bytes))]);
    let clock = fx.clock.clone();
    let mut next = ids;
    upload_part(
        meta,
        &fx.fleet,
        &request,
        source,
        move || clock.now_millis(),
        move || {
            next += 1;
            next
        },
    )
    .await
}

/// [`upload_over`] through the wrapper.
async fn upload(
    fx: &Fx,
    number: u32,
    attempt_id: &AttemptId,
    scheme: EcScheme,
    chunk_size: usize,
    bytes: &[u8],
    ids: u128,
) -> Result<UploadPartOutcome, UploadPartError> {
    upload_over(
        fx, &fx.store, number, attempt_id, scheme, chunk_size, bytes, ids,
    )
    .await
}

fn committed(number: u32, bytes: &[u8]) -> UploadPartOutcome {
    UploadPartOutcome::Committed {
        part_number: part(number),
        digest: sha(bytes),
    }
}

/// Part `number`'s `part:` and `psum:` records, decoded and checked to agree.
async fn committed_part(fx: &Fx, number: u32) -> (PartRecord, Bytes) {
    let bytes = fx
        .store
        .inner
        .get(&part_key(&fx.upload, part(number)))
        .await
        .unwrap()
        .expect("the part record");
    let record = decode_part_record(&bytes).expect("a valid part record");
    let summary = fx
        .store
        .inner
        .get(&psum_key(&fx.upload, part(number)))
        .await
        .unwrap()
        .expect("the part summary");
    let summary: PartSummary = decode_part_summary(&summary).expect("a valid summary");
    assert_eq!(
        summary.chunks as usize,
        record.chunks().len(),
        "psum chunks"
    );
    assert_eq!(summary.len, record.len(), "psum len");
    assert_eq!(summary.digest, *record.digest(), "psum digest");
    assert_eq!(summary.committed_at_millis, record.committed_at_millis());
    (record, bytes)
}

/// The owned entries of part `number`, as `(chunk, value)`.
async fn owned_of(fx: &Fx, number: u32) -> Vec<(ChunkId, Bytes)> {
    fx.store
        .inner
        .scan(&sidx_range(&fx.upload))
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(key, value)| {
            let (_, owner_part, chunk) = parse_sidx_key(&key).unwrap();
            (owner_part == part(number)).then_some((chunk, value))
        })
        .collect()
}

/// The slots `attempt_id` holds.
async fn slots_of(fx: &Fx, attempt_id: &AttemptId) -> Vec<Vec<u8>> {
    fx.store
        .inner
        .scan(&slot_range(&fx.upload))
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, value)| decode_slot_record(value).unwrap().attempt_id() == attempt_id)
        .map(|(key, _)| key)
        .collect()
}

/// The per-part obligation key of `attempt_id` on part `number` at epoch `epoch`.
fn per_part_key(fx: &Fx, epoch: u64, number: u32, attempt_id: &AttemptId) -> Vec<u8> {
    retire_key(
        RetireMode::Bytes,
        &RetireToken::Session {
            upload_id: fx.upload.clone(),
            epoch,
            part: Some((part(number), attempt_id.clone())),
        },
    )
}

/// Every commit event of `kind`, in order.
fn commits_of(log: &Log, wanted: Kind) -> Vec<(WriteBatch, u64, Answered, bool)> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            Event::Commit {
                kind,
                batch,
                at,
                answered,
                landed,
            } if *kind == wanted => Some((batch.clone(), *at, *answered, *landed)),
            _ => None,
        })
        .collect()
}

/// Every fragment write, in order.
fn puts(log: &Log) -> Vec<(DServerId, FragmentId, Option<u64>, u64)> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            Event::Put {
                dserver,
                id,
                deadline,
                at,
            } => Some((*dserver, *id, *deadline, *at)),
            _ => None,
        })
        .collect()
}

/// The kinds of every batch sent, in order.
fn kinds(log: &Log) -> Vec<Kind> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            Event::Commit { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect()
}

/// How many landed batches (sent or released) deleted `key`.
fn landed_deletes(log: &Log, key: &[u8]) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|event| match event {
            Event::Commit { batch, landed, .. } | Event::Released { batch, landed } => {
                *landed && batch.deletes.iter().any(|deleted| deleted == key)
            }
            Event::Put { .. } => false,
        })
        .count()
}

/// The owned entry each landed intent put: `(chunk, placement)`, in order.
fn landed_intents(log: &Log) -> Vec<(ChunkId, Vec<DServerId>)> {
    commits_of(log, Kind::Intent)
        .into_iter()
        .filter(|(_, _, _, landed)| *landed)
        .map(|(batch, _, _, _)| {
            let (key, value) = batch
                .puts
                .iter()
                .find(|(key, _)| key.starts_with(SIDX_PREFIX))
                .expect("an intent puts its owned entry")
                .clone();
            let (_, _, entry) = decode_owned_entry(&key, &value).expect("a valid owned entry");
            let (_, _, chunk) = parse_sidx_key(&key).unwrap();
            (chunk, entry.staged().placement().to_vec())
        })
        .collect()
}

/// The slot key `batch` requires (every batch after the reserve requires exactly one).
fn required_slot(batch: &WriteBatch) -> Vec<u8> {
    batch
        .preconditions
        .iter()
        .find(|pre| pre.key.starts_with(SLOT_PREFIX) && pre.expected.is_some())
        .expect("the batch requires its slot")
        .key
        .clone()
}

/// **(A)** Every chunk's owned entry lands before the first of its fragments is written: each
/// write's chunk has a landed intent earlier in the log.
fn intent_precedes_bytes(log: &Log, chunks: &BTreeSet<ChunkId>) {
    let mut landed: HashMap<ChunkId, usize> = HashMap::new();
    let mut checked = 0;
    for (at, event) in log.lock().unwrap().iter().enumerate() {
        match event {
            Event::Commit {
                kind: Kind::Intent,
                batch,
                landed: true,
                ..
            } => {
                for (key, _) in &batch.puts {
                    if key.starts_with(SIDX_PREFIX) {
                        let (_, _, chunk) = parse_sidx_key(key).unwrap();
                        landed.entry(chunk).or_insert(at);
                    }
                }
            }
            Event::Put { id, .. } if chunks.contains(&id.chunk) => {
                let intent = landed.get(&id.chunk).unwrap_or_else(|| {
                    panic!(
                        "chunk {} was written before its owned entry landed",
                        id.chunk
                    )
                });
                assert!(*intent < at);
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked > 0, "some fragment was checked");
}

/// An attempt that ended stopped, with nothing in doubt: its cause and residue.
fn stopped(outcome: Result<UploadPartOutcome, UploadPartError>) -> (Stop, Residue) {
    match outcome {
        Err(UploadPartError::Stopped(stopped)) => (stopped.cause, stopped.residue),
        other => panic!("expected Stopped, got {other:?}"),
    }
}

// ─────────────────────────── (A) staging and commit ───────────────────────────

/// **(A)** Parts 3, 1 and 2, staged in that order (three, one and two chunks), each commit to
/// `Committed { part_number, digest }` with the body's SHA-256. After each commit its `part:`
/// and `psum:` records decode and agree, and no owned entry or slot of that attempt is left.
/// Mid-stage — when each commit arrives, after its intents — the store holds owned `sidx:`
/// entries and no `pending:` one. Every chunk's owned entry lands before its first fragment.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_parts_staged_out_of_order_commit_and_their_records_agree() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![Rule::starting(
            Kind::Commit,
            1,
            vec![Pre::Snapshot],
            Then::Pass,
        )]);
        let mut chunk_ids = BTreeSet::new();
        for (i, (number, len)) in [(3_u32, 10_usize), (1, 4), (2, 7)].into_iter().enumerate() {
            let bytes = body(number as u8, len);
            let who = attempt(i as u128);
            let outcome = upload(&fx, number, &who, RS21, 4, &bytes, 1_000 * (i as u128 + 1))
                .await
                .expect("the attempt ran");
            assert_eq!(outcome, committed(number, &bytes), "part {number}");
            let (record, _) = committed_part(&fx, number).await;
            assert_eq!(
                record.chunks().len(),
                len.div_ceil(4),
                "part {number} chunks"
            );
            assert_eq!(record.len(), len as u64);
            assert_eq!(*record.digest(), sha(&bytes));
            chunk_ids.extend(record.chunks().iter().map(|chunk| chunk.id));
            assert!(
                owned_of(&fx, number).await.is_empty(),
                "no owned entry left"
            );
            assert!(slots_of(&fx, &who).await.is_empty(), "no slot left");
        }
        let snapshots = fx.store.snapshots();
        assert_eq!(snapshots.len(), 3, "one look per commit");
        for (sidx, pending) in snapshots {
            assert!(sidx > 0, "mid-stage the owned entries are under sidx:");
            assert_eq!(pending, 0, "and nothing is under pending:");
        }
        intent_precedes_bytes(&fx.log, &chunk_ids);
    })
    .await;
}

/// **(A)** The write deadline is exact. The wrapper advances the clock right after each intent
/// lands, so every fragment is dispatched late. Each fragment's deadline is still `t_auth + W`,
/// `t_auth` the clock reading when its chunk's landed intent was sent: never the dispatch time
/// plus `W`, never `None`, never unbounded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_every_staged_write_carries_t_auth_plus_w_however_late_it_goes_out() {
    within(async {
        let fx = fixture().await;
        const DELAY: u64 = 2_500;
        fx.store.script(vec![Rule::starting(
            Kind::Intent,
            1,
            vec![],
            Then::PassThenAdvance(DELAY),
        )]);
        let bytes = body(9, 12);
        let outcome = upload(&fx, 1, &attempt(1), RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        let mut t_auth: HashMap<ChunkId, u64> = HashMap::new();
        for (batch, at, _, landed) in commits_of(&fx.log, Kind::Intent) {
            if landed {
                let (key, _) = batch
                    .puts
                    .iter()
                    .find(|(key, _)| key.starts_with(SIDX_PREFIX))
                    .unwrap();
                t_auth.insert(parse_sidx_key(key).unwrap().2, at);
            }
        }
        assert_eq!(t_auth.len(), 3, "three chunks");
        let writes = puts(&fx.log);
        assert_eq!(writes.len(), 9, "three fragments per chunk");
        for (_, id, deadline, dispatched) in writes {
            let authorized = t_auth[&id.chunk];
            assert!(
                dispatched >= authorized + DELAY,
                "dispatched after the delay"
            );
            assert_eq!(
                deadline,
                Some(authorized + WINDOW),
                "chunk {} fragment {}",
                id.chunk,
                id.index
            );
            assert_ne!(deadline, Some(dispatched + WINDOW));
            assert_ne!(deadline, Some(u64::MAX));
        }
    })
    .await;
}

// ─────────────────────────── (B) the drain fence ───────────────────────────

/// **B1.** The wrapper records a drain for a server the plan selected **just before** that
/// chunk's intent commits. The intent carries `require_absent(desired:dserver:<S>)` and fails;
/// the chunk plans again without `S`, and the part commits. No landed owned entry, no `part:`
/// chunk and no fragment of it names `S`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b1_a_drain_recorded_just_before_the_intent_makes_it_plan_again() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![Rule::at(
            Kind::Intent,
            1,
            vec![Pre::DrainPlanned],
            Then::Pass,
        )]);
        let bytes = body(1, 8);
        let outcome = upload(&fx, 1, &attempt(1), RS42, 8, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(
            outcome,
            committed(1, &bytes),
            "a drain re-plans, never refuses"
        );
        let drained = fx.store.drained();
        assert_eq!(drained.len(), 1);
        let s = drained[0];
        let intents = commits_of(&fx.log, Kind::Intent);
        assert_eq!(
            intents.len(),
            2,
            "the fenced intent, then the re-planned one"
        );
        assert!(
            intents[0]
                .0
                .preconditions
                .iter()
                .any(|pre| pre.key == desired_dserver_key(s) && pre.expected.is_none()),
            "the intent is fenced on the drain record of every planned server"
        );
        assert_eq!(intents[0].2, Answered::Conflict, "the fenced intent failed");
        assert!(!intents[0].3);
        assert!(intents[1].3, "the second plan landed");
        for (_, placement) in landed_intents(&fx.log) {
            assert!(!placement.contains(&s), "no landed owned entry names {s}");
        }
        let (record, _) = committed_part(&fx, 1).await;
        let chunk_ids: BTreeSet<ChunkId> = record.chunks().iter().map(|chunk| chunk.id).collect();
        for chunk in record.chunks() {
            assert!(!chunk.placement.contains(&s), "the part names no {s}");
        }
        for (dserver, id, _, _) in puts(&fx.log) {
            if chunk_ids.contains(&id.chunk) {
                assert_ne!(dserver, s, "no fragment lands on {s}");
            }
        }
    })
    .await;
}

/// **B2.** With `desired:dserver:<S>` present **before** staging, and `S` the server the
/// selector prefers (least utilized), the part commits, nothing names `S`, and no intent ever
/// conflicts: placement reads the topology minus the draining servers.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_a_server_drained_before_staging_is_never_planned() {
    within(async {
        let s: DServerId = 0;
        let busy: Vec<(DServerId, u64)> = (1..SERVERS).map(|id| (id, 100)).collect();
        let topology = topology(&busy);
        assert!(
            select_distinct_domains(&topology, 6).unwrap().contains(&s),
            "the selector prefers {s} on the full topology"
        );
        let fx = fixture_with(topology).await;
        fx.store
            .inner
            .commit(WriteBatch::new().put(desired_dserver_key(s), &b"draining"[..]))
            .await
            .unwrap();
        let bytes = body(2, 16);
        let outcome = upload(&fx, 1, &attempt(1), RS42, 8, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        for (_, _, answered, landed) in commits_of(&fx.log, Kind::Intent) {
            assert_eq!(answered, Answered::Committed, "no intent conflicts");
            assert!(landed);
        }
        let (record, _) = committed_part(&fx, 1).await;
        for chunk in record.chunks() {
            assert!(!chunk.placement.contains(&s), "the part names no {s}");
        }
        for (dserver, _, _, _) in puts(&fx.log) {
            assert_ne!(dserver, s, "no fragment lands on {s}");
        }
    })
    .await;
}

/// **B3.** Four of twelve single-server domains draining and RS(4,2) needing six: a part stages
/// and commits on the other eight. Seven draining leaves five: the answer is the typed placement
/// error, never `NoSuchUpload` for an `Open` session, and the attempt's slot is released.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b3_four_of_twelve_draining_still_stages_and_too_few_is_a_typed_error() {
    within(async {
        let fx = fixture().await;
        let drain = |ids: std::ops::Range<DServerId>| {
            let mut batch = WriteBatch::new();
            for id in ids {
                batch = batch.put(desired_dserver_key(id), &b"draining"[..]);
            }
            batch
        };
        fx.store.inner.commit(drain(0..4)).await.unwrap();
        let bytes = body(3, 16);
        let outcome = upload(&fx, 1, &attempt(1), RS42, 8, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes), "eight domains hold RS(4,2)");
        let (record, _) = committed_part(&fx, 1).await;
        for chunk in record.chunks() {
            assert!(
                chunk.placement.iter().all(|id| *id >= 4),
                "{:?}",
                chunk.placement
            );
        }

        fx.store.inner.commit(drain(4..7)).await.unwrap();
        let who = attempt(2);
        let outcome = upload(&fx, 2, &who, RS42, 8, &body(4, 8), 200).await;
        let (cause, residue) = stopped(outcome);
        assert!(
            matches!(
                cause,
                Stop::Placement(SelectorError::InsufficientDomains { have: 5, need: 6 })
            ),
            "{cause:?}"
        );
        assert_eq!(residue, Residue::Released, "the slot was released");
        assert!(slots_of(&fx, &who).await.is_empty());
        assert!(owned_of(&fx, 2).await.is_empty());
        let session = fx
            .store
            .inner
            .get(&mpu_key(&fx.upload))
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

// ─────────────────────────────── (C) slots ───────────────────────────────

/// **(C)** `MAX_INFLIGHT_PARTS` reserves on one session each take an index; the next answers
/// `SlowDown { InflightParts }`: the key space is the cap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_the_slot_key_space_is_the_cap() {
    within(async {
        let fx = fixture().await;
        let mut indices = BTreeSet::new();
        for i in 0..MAX_INFLIGHT_PARTS {
            let who = attempt(u128::from(i));
            let outcome = reserve_slot(&fx.store, &fx.upload, part(i + 1), &who, START, nz(TTL))
                .await
                .expect("the reserve ran");
            let ReserveOutcome::Reserved { index, attempt_id } = outcome else {
                panic!("reserve {i}: {outcome:?}");
            };
            assert_eq!(attempt_id, who);
            indices.insert(index.get());
        }
        assert_eq!(indices.len(), MAX_INFLIGHT_PARTS as usize);
        let outcome = reserve_slot(
            &fx.store,
            &fx.upload,
            part(99),
            &attempt(99),
            START,
            nz(TTL),
        )
        .await
        .expect("the reserve ran");
        assert_eq!(
            outcome,
            ReserveOutcome::Refused(Refusal::SlowDown {
                pressure: Backpressure::InflightParts {
                    max_inflight_parts: MAX_INFLIGHT_PARTS
                }
            })
        );
        let slots = fx.store.inner.scan(&slot_range(&fx.upload)).await.unwrap();
        assert_eq!(slots.len(), MAX_INFLIGHT_PARTS as usize);
    })
    .await;
}

/// **(C)** A reserve against a session that is no longer `Open` (an Abort fenced it), or against
/// an unknown upload id, answers `NoSuchUpload` and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_reserve_against_a_session_that_is_not_open_writes_nothing() {
    within(async {
        let fx = fixture().await;
        let fenced = abort_session(&fx.store.inner, &fx.upload).await;
        assert!(matches!(fenced, Ok(AbortOutcome::Fenced)));
        let before = fx.store.inner.scan(b"").await.unwrap();
        let outcome = reserve_slot(&fx.store, &fx.upload, part(1), &attempt(1), START, nz(TTL))
            .await
            .expect("the reserve ran");
        assert_eq!(outcome, ReserveOutcome::Refused(Refusal::NoSuchUpload));
        let stranger = UploadId::new(format!("{:032x}", 0xdead_u128)).unwrap();
        let outcome = reserve_slot(&fx.store, &stranger, part(1), &attempt(2), START, nz(TTL))
            .await
            .expect("the reserve ran");
        assert_eq!(outcome, ReserveOutcome::Refused(Refusal::NoSuchUpload));
        assert_eq!(
            fx.store.inner.scan(b"").await.unwrap(),
            before,
            "nothing written"
        );
        assert!(
            commits_of(&fx.log, Kind::Reserve).is_empty(),
            "no batch sent"
        );
    })
    .await;
}

/// **(C)** A reserve whose batch lands but whose outcome is `CommitUnknownResult` adopts its own
/// slot on the re-read (its own attempt id), with one send: the session's held slots equal its
/// live attempts. Both `may_still_commit` values.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_reserve_that_landed_with_an_unknown_outcome_adopts_its_own_slot() {
    within(async {
        for may_still_commit in [false, true] {
            let fx = fixture().await;
            fx.store.script(vec![Rule::at(
                Kind::Reserve,
                1,
                vec![],
                Then::Unknown {
                    apply: true,
                    may_still_commit,
                },
            )]);
            let first = attempt(1);
            let outcome = reserve_slot(&fx.store, &fx.upload, part(1), &first, START, nz(TTL))
                .await
                .expect("the reserve ran");
            assert!(
                matches!(&outcome, ReserveOutcome::Reserved { attempt_id, .. } if *attempt_id == first),
                "{outcome:?}"
            );
            let second = attempt(2);
            let outcome = reserve_slot(&fx.store, &fx.upload, part(2), &second, START, nz(TTL))
                .await
                .expect("the reserve ran");
            assert!(matches!(outcome, ReserveOutcome::Reserved { .. }));
            assert_eq!(slots_of(&fx, &first).await.len(), 1, "one slot per attempt");
            assert_eq!(slots_of(&fx, &second).await.len(), 1);
            let slots = fx.store.inner.scan(&slot_range(&fx.upload)).await.unwrap();
            assert_eq!(slots.len(), 2, "held slots == live attempts");
            assert_eq!(
                commits_of(&fx.log, Kind::Reserve).len(),
                2,
                "the adopted reserve was not sent again"
            );
        }
    })
    .await;
}

/// **(C)** A reserve answered `may_still_commit: true` and read absent retries the **same**
/// index with the **same** attempt id and bytes: the two sends are exclusive through
/// `require_absent`, so when the held one is released later it fails, and the session holds one
/// slot for the attempt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_ambiguous_reserve_retries_the_same_index_with_the_same_attempt() {
    within(async {
        let fx = fixture().await;
        fx.store
            .script(vec![Rule::at(Kind::Reserve, 1, vec![], Then::Hold)]);
        let who = attempt(1);
        let outcome = reserve_slot(&fx.store, &fx.upload, part(1), &who, START, nz(TTL))
            .await
            .expect("the reserve ran");
        let ReserveOutcome::Reserved { index, .. } = outcome else {
            panic!("{outcome:?}");
        };
        let sends = commits_of(&fx.log, Kind::Reserve);
        assert_eq!(sends.len(), 2);
        assert_eq!(sends[0].2, Answered::Unknown);
        assert_eq!(
            sends[0].0.puts, sends[1].0.puts,
            "same index, same attempt, same bytes"
        );
        let (key, _) = &sends[1].0.puts[0];
        assert_eq!(parse_slot_key(key).unwrap().1, index);
        assert!(sends[1].3, "the retry landed");
        assert!(
            !fx.store.release_held().await,
            "the held send can no longer land"
        );
        assert_eq!(
            slots_of(&fx, &who).await.len(),
            1,
            "one slot for the attempt"
        );
    })
    .await;
}

// ─────────────────────────────── (D) renewal ───────────────────────────────

/// **(D)** The clock moves past half the lease between chunks, so the attempt renews before
/// chunks 2 and 3 and once more before the commit. Every renewal batch requires the session and
/// the slot, rewrites the attempt's **own slot** with a later lease, and in the same batch puts
/// every owned entry staged before it, with that same lease.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_every_renewal_rewrites_the_slot_with_the_owned_leases() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![Rule::starting(
            Kind::Intent,
            1,
            vec![],
            Then::PassThenAdvance(6_000),
        )]);
        let bytes = body(5, 12);
        let outcome = upload(&fx, 1, &attempt(1), RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        let session = fx
            .store
            .inner
            .get(&mpu_key(&fx.upload))
            .await
            .unwrap()
            .unwrap();
        let mut staged: BTreeSet<Vec<u8>> = BTreeSet::new();
        let mut slot_lease = 0;
        let mut renewals = 0;
        let events = fx.log.lock().unwrap().clone();
        for event in events {
            let Event::Commit {
                kind,
                batch,
                landed: true,
                ..
            } = event
            else {
                continue;
            };
            let slot_put = |batch: &WriteBatch| {
                batch
                    .puts
                    .iter()
                    .find(|(key, _)| key.starts_with(SLOT_PREFIX))
                    .map(|(_, value)| decode_slot_record(value).unwrap())
            };
            match kind {
                Kind::Reserve | Kind::Intent => {
                    let slot = slot_put(&batch).expect("a slot put");
                    assert!(slot.lease_expiry_millis() > slot_lease);
                    slot_lease = slot.lease_expiry_millis();
                    for (key, _) in &batch.puts {
                        if key.starts_with(SIDX_PREFIX) {
                            staged.insert(key.clone());
                        }
                    }
                }
                Kind::Renewal => {
                    renewals += 1;
                    let slot_key = required_slot(&batch);
                    assert!(batch
                        .preconditions
                        .iter()
                        .any(|pre| pre.key == mpu_key(&fx.upload)
                            && pre.expected.as_ref() == Some(&session)));
                    let slot = slot_put(&batch).expect("the renewal rewrites its own slot");
                    assert!(batch.puts.iter().any(|(key, _)| *key == slot_key));
                    assert!(slot.lease_expiry_millis() > slot_lease, "a later lease");
                    slot_lease = slot.lease_expiry_millis();
                    let owned: BTreeSet<Vec<u8>> = batch
                        .puts
                        .iter()
                        .filter(|(key, _)| key.starts_with(SIDX_PREFIX))
                        .map(|(key, _)| key.clone())
                        .collect();
                    assert_eq!(owned, staged, "every owned lease, in the same batch");
                    for (key, value) in &batch.puts {
                        if key.starts_with(SIDX_PREFIX) {
                            let (_, _, entry) = decode_owned_entry(key, value).unwrap();
                            assert_eq!(entry.lease_expiry_millis(), slot_lease);
                        }
                    }
                }
                Kind::Commit | Kind::Compensation | Kind::Other => {}
            }
        }
        assert_eq!(renewals, 3, "before chunks 2 and 3, and before the commit");
    })
    .await;
}

/// **(D)** The session is fenced (a real Abort) just before the first renewal commits. The
/// renewal fails, the attempt answers `NoSuchUpload`, and it resurrects nothing: the owned entry
/// and the slot hold exactly what the last intent wrote, no part or obligation of the attempt
/// exists, and no batch follows the renewal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_renewal_after_the_session_left_open_resurrects_nothing() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![
            Rule::starting(Kind::Intent, 1, vec![], Then::PassThenAdvance(6_000)),
            Rule::at(
                Kind::Renewal,
                1,
                vec![Pre::Abort(fx.upload.clone())],
                Then::Pass,
            ),
        ]);
        let who = attempt(1);
        let outcome = upload(&fx, 1, &who, RS21, 4, &body(6, 12), 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, UploadPartOutcome::Refused(Refusal::NoSuchUpload));
        assert_eq!(
            kinds(&fx.log),
            [Kind::Reserve, Kind::Intent, Kind::Renewal],
            "nothing is sent after the refused renewal"
        );
        let intent = &commits_of(&fx.log, Kind::Intent)[0].0;
        let (sidx_key, sidx_value) = intent
            .puts
            .iter()
            .find(|(key, _)| key.starts_with(SIDX_PREFIX))
            .unwrap();
        let (slot_key, slot_value) = intent
            .puts
            .iter()
            .find(|(key, _)| key.starts_with(SLOT_PREFIX))
            .unwrap();
        let owned = owned_of(&fx, 1).await;
        assert_eq!(owned.len(), 1);
        assert_eq!(
            fx.store.inner.get(sidx_key).await.unwrap().as_ref(),
            Some(sidx_value)
        );
        assert_eq!(
            fx.store.inner.get(slot_key).await.unwrap().as_ref(),
            Some(slot_value)
        );
        assert!(fx
            .store
            .inner
            .get(&part_key(&fx.upload, part(1)))
            .await
            .unwrap()
            .is_none());
        assert!(fx
            .store
            .inner
            .get(&per_part_key(&fx, 0, 1, &who))
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

// ───────────────────── (E) a lost commit is classified ─────────────────────

/// **(E)** The session fenced to `Aborting` between the last intent and the commit: the commit
/// answers `NoSuchUpload` and writes nothing more — no compensation; the owned entries and the
/// slot are the drain's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_session_fenced_before_the_commit_answers_no_such_upload() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![Rule::at(
            Kind::Commit,
            1,
            vec![Pre::Abort(fx.upload.clone())],
            Then::Pass,
        )]);
        let who = attempt(1);
        let outcome = upload(&fx, 1, &who, RS21, 4, &body(7, 8), 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, UploadPartOutcome::Refused(Refusal::NoSuchUpload));
        assert_eq!(
            kinds(&fx.log).last(),
            Some(&Kind::Commit),
            "the commit is the last batch"
        );
        assert!(commits_of(&fx.log, Kind::Compensation).is_empty());
        assert!(fx
            .store
            .inner
            .get(&part_key(&fx.upload, part(1)))
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            owned_of(&fx, 1).await.len(),
            2,
            "the residue is left for the drain"
        );
        assert_eq!(slots_of(&fx, &who).await.len(), 1);
        assert!(fx
            .store
            .inner
            .get(&per_part_key(&fx, 0, 1, &who))
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

/// Assert the per-part obligation at `key` decodes against its key and owes exactly `owed`.
async fn assert_owes(fx: &Fx, key: &[u8], owed: &[(ChunkId, Vec<DServerId>)]) {
    let value = fx
        .store
        .inner
        .get(key)
        .await
        .unwrap()
        .expect("the obligation");
    let (mode, _, payload) = decode_retire_obligation(key, &value).expect("a valid obligation");
    assert_eq!(mode, RetireMode::Bytes);
    let chunks: Vec<(ChunkId, Vec<DServerId>)> = payload
        .chunks()
        .iter()
        .map(|chunk: &ChunkRef| (chunk.id, chunk.placement.clone()))
        .collect();
    assert_eq!(chunks, owed);
}

/// **(E)** A rival attempt commits the same part number while this attempt's commit waits. The
/// loser compensates in **one** batch — one `retire:bytes:` obligation for its own chunks (which
/// decodes under its key), its slot release and its owned entries' deletes — and answers a
/// refusal; the rival's part stands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_same_part_rival_makes_the_loser_compensate_in_one_batch() {
    within(async {
        let fx = fixture().await;
        let reached = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fx.store.script(vec![Rule::at(
            Kind::Commit,
            1,
            vec![Pre::Gate {
                reached: reached.clone(),
                release: release.clone(),
            }],
            Then::Pass,
        )]);
        let who = attempt(1);
        let rival = attempt(2);
        let rival_bytes = body(40, 8);
        let our_bytes = body(8, 8);
        let ours = upload(&fx, 1, &who, RS21, 4, &our_bytes, 100);
        let theirs = async {
            reached.notified().await;
            let outcome =
                upload_over(&fx, &fx.store.inner, 1, &rival, RS21, 4, &rival_bytes, 900).await;
            release.notify_one();
            outcome
        };
        let (ours, theirs) = tokio::join!(ours, theirs);
        assert_eq!(theirs.expect("the rival ran"), committed(1, &rival_bytes));
        assert_eq!(
            ours.expect("the attempt ran"),
            UploadPartOutcome::Refused(Refusal::OperationAborted)
        );
        let (record, _) = committed_part(&fx, 1).await;
        assert_eq!(
            *record.digest(),
            sha(&rival_bytes),
            "the rival's part stands"
        );
        let compensations = commits_of(&fx.log, Kind::Compensation);
        assert_eq!(compensations.len(), 1, "one compensation batch");
        let (batch, _, _, landed) = &compensations[0];
        assert!(landed);
        let owed = landed_intents(&fx.log);
        assert_eq!(owed.len(), 2);
        let key = per_part_key(&fx, 0, 1, &who);
        assert_eq!(batch.puts.len(), 1, "one obligation, nothing else put");
        assert_eq!(batch.puts[0].0, key);
        let slot = required_slot(batch);
        assert!(
            batch.deletes.contains(&slot),
            "the slot is released in the same batch"
        );
        assert_eq!(
            batch.deletes.len(),
            1 + owed.len(),
            "and every owned entry deleted"
        );
        assert_owes(&fx, &key, &owed).await;
        assert!(slots_of(&fx, &who).await.is_empty());
        assert!(owned_of(&fx, 1).await.is_empty());
    })
    .await;
}

/// **(E)** A commit whose batch lands but answers `CommitUnknownResult`: the attempt re-reads,
/// finds the exact `part:` bytes it built, answers `Committed`, and sends nothing more — no
/// compensation, no second release. Both `may_still_commit` values.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_commit_that_landed_with_an_unknown_outcome_answers_committed_once() {
    within(async {
        for may_still_commit in [false, true] {
            let fx = fixture().await;
            fx.store.script(vec![Rule::at(
                Kind::Commit,
                1,
                vec![],
                Then::Unknown {
                    apply: true,
                    may_still_commit,
                },
            )]);
            let bytes = body(10, 8);
            let outcome = upload(&fx, 1, &attempt(1), RS21, 4, &bytes, 100)
                .await
                .expect("the attempt ran");
            assert_eq!(outcome, committed(1, &bytes));
            let (record, stored) = committed_part(&fx, 1).await;
            assert_eq!(*record.digest(), sha(&bytes));
            let commits = commits_of(&fx.log, Kind::Commit);
            assert_eq!(commits.len(), 1, "never sent again");
            assert_eq!(
                commits[0]
                    .0
                    .puts
                    .iter()
                    .find(|(key, _)| key.starts_with(PART_PREFIX))
                    .map(|(_, v)| v),
                Some(&stored),
                "the part found is the exact bytes the attempt built"
            );
            assert_eq!(
                kinds(&fx.log).last(),
                Some(&Kind::Commit),
                "nothing after it"
            );
            let slot = required_slot(&commits[0].0);
            assert_eq!(landed_deletes(&fx.log, &slot), 1, "the slot released once");
        }
    })
    .await;
}

/// Prior part 1, committed by attempt `P` on the inner store: its record.
async fn prior_part(fx: &Fx) -> (PartRecord, Bytes) {
    let bytes = body(50, 8);
    let outcome = upload_over(fx, &fx.store.inner, 1, &attempt(70), RS21, 4, &bytes, 7_000)
        .await
        .expect("the prior upload ran");
    assert_eq!(outcome, committed(1, &bytes));
    committed_part(fx, 1).await
}

/// **(E)** A re-upload whose commit did not land answers `CommitUnknownResult {
/// may_still_commit: false }`. The re-read finds the **prior** part — not this attempt's bytes —
/// so the attempt does not answer `Committed` for it: the outcome is settled as not landed, the
/// commit is sent again, and the attempt ends with its **own** part committed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_re_upload_whose_commit_did_not_land_ends_with_its_own_part() {
    within(async {
        let fx = fixture().await;
        let (prior, _) = prior_part(&fx).await;
        fx.store.script(vec![Rule::at(
            Kind::Commit,
            1,
            vec![],
            Then::Unknown {
                apply: false,
                may_still_commit: false,
            },
        )]);
        let bytes = body(11, 8);
        let outcome = upload(&fx, 1, &attempt(1), RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        let (record, _) = committed_part(&fx, 1).await;
        assert_eq!(*record.digest(), sha(&bytes), "its own part, not the prior");
        assert_ne!(record.chunks(), prior.chunks());
        let commits = commits_of(&fx.log, Kind::Commit);
        assert_eq!(commits.len(), 2, "sent again once the read settled it");
        assert!(!commits[0].3 && commits[1].3);
    })
    .await;
}

/// Exactly one of {the attempt's own part, its compensation} is durable, and the slot was
/// released exactly once.
async fn exactly_one_durable(fx: &Fx, who: &AttemptId, own: &[u8], slot: &[u8]) -> bool {
    let stored = fx
        .store
        .inner
        .get(&part_key(&fx.upload, part(1)))
        .await
        .unwrap();
    let own_part = stored
        .as_ref()
        .is_some_and(|stored| *decode_part_record(stored).unwrap().digest() == sha(own));
    let compensated = fx
        .store
        .inner
        .get(&per_part_key(fx, 0, 1, who))
        .await
        .unwrap()
        .is_some();
    assert!(
        own_part != compensated,
        "exactly one of the part and the compensation"
    );
    assert_eq!(
        landed_deletes(&fx.log, slot),
        1,
        "the slot released exactly once"
    );
    assert!(slots_of(fx, who).await.is_empty());
    own_part
}

/// **(E) held, run (i).** The first commit is held (`may_still_commit: true`, unapplied); the
/// attempt neither answers nor compensates on the read that finds no part, and sends the commit
/// again against the same slot bytes. The held batch is released just before that second send,
/// which then fails; the read finds the attempt's own bytes: `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_commit_released_before_the_next_send() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![
            Rule::at(Kind::Commit, 1, vec![], Then::Hold),
            Rule::at(Kind::Commit, 2, vec![Pre::ReleaseHeld], Then::Pass),
        ]);
        let who = attempt(1);
        let bytes = body(12, 8);
        let outcome = upload(&fx, 1, &who, RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        let commits = commits_of(&fx.log, Kind::Commit);
        assert_eq!(commits.len(), 2);
        assert_eq!(required_slot(&commits[0].0), required_slot(&commits[1].0));
        assert_eq!(
            commits[0].0.preconditions, commits[1].0.preconditions,
            "the second send requires what the held one requires"
        );
        assert!(
            !commits[1].3,
            "the second send failed: the held one had landed"
        );
        assert!(commits_of(&fx.log, Kind::Compensation).is_empty());
        let slot = required_slot(&commits[0].0);
        assert!(exactly_one_durable(&fx, &who, &bytes, &slot).await);
    })
    .await;
}

/// **(E) held, run (ii).** The first commit is held; the second send lands and the attempt
/// answers `Committed`. Released after the attempt returned, the held batch fails: one part,
/// one release.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_commit_released_after_the_attempt_returned() {
    within(async {
        let fx = fixture().await;
        fx.store
            .script(vec![Rule::at(Kind::Commit, 1, vec![], Then::Hold)]);
        let who = attempt(1);
        let bytes = body(13, 8);
        let outcome = upload(&fx, 1, &who, RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        assert!(
            !fx.store.release_held().await,
            "the held commit can no longer land"
        );
        assert!(commits_of(&fx.log, Kind::Compensation).is_empty());
        let slot = required_slot(&commits_of(&fx.log, Kind::Commit)[0].0);
        assert!(exactly_one_durable(&fx, &who, &bytes, &slot).await);
    })
    .await;
}

/// **(E) held, run (iii).** The first commit is held, and every later send is answered
/// `Conflict` without being applied, until the attempt's rounds run out: it returns an in-doubt
/// error that keeps `may_still_commit`, having sent no compensation. Released afterwards, the held
/// batch lands: the attempt's own part, one release, and the answer was never `Committed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_held_commit_and_conflicts_end_in_doubt_and_the_held_part_lands() {
    within(async {
        let fx = fixture().await;
        fx.store.script(vec![
            Rule::at(Kind::Commit, 1, vec![], Then::Hold),
            Rule::starting(Kind::Commit, 2, vec![], Then::Conflict),
        ]);
        let who = attempt(1);
        let bytes = body(14, 8);
        let outcome = upload(&fx, 1, &who, RS21, 4, &bytes, 100).await;
        let Err(UploadPartError::InDoubt(doubt)) = outcome else {
            panic!("expected InDoubt, got {outcome:?}");
        };
        assert!(doubt.may_still_commit, "the doubt is kept");
        assert!(
            matches!(doubt.cause, Stop::RoundsSpent { rounds } if rounds == UPLOAD_PART_ROUNDS),
            "{:?}",
            doubt.cause
        );
        assert_eq!(
            wyrd_traits::classify(&UploadPartError::InDoubt(doubt)),
            wyrd_traits::ErrorClass::Indeterminate
        );
        assert_eq!(
            commits_of(&fx.log, Kind::Commit).len(),
            UPLOAD_PART_ROUNDS as usize
        );
        assert!(
            commits_of(&fx.log, Kind::Compensation).is_empty(),
            "never compensated"
        );
        assert!(fx.store.release_held().await, "the held commit lands");
        let slot = required_slot(&commits_of(&fx.log, Kind::Commit)[0].0);
        assert!(exactly_one_durable(&fx, &who, &bytes, &slot).await);
    })
    .await;
}

/// **(E)** A same-number rival commits while this attempt's commit waits, and the commit then
/// answers an unknown outcome. The re-read finds the rival's record, not this attempt's bytes:
/// never `Committed`; the attempt compensates under its own slot. Both `may_still_commit` values.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_a_rival_and_an_unknown_outcome_is_never_committed() {
    within(async {
        for may_still_commit in [false, true] {
            let fx = fixture().await;
            let reached = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            fx.store.script(vec![Rule::at(
                Kind::Commit,
                1,
                vec![Pre::Gate {
                    reached: reached.clone(),
                    release: release.clone(),
                }],
                Then::Unknown {
                    apply: true,
                    may_still_commit,
                },
            )]);
            let who = attempt(1);
            let rival_bytes = body(41, 8);
            let our_bytes = body(15, 8);
            let ours = upload(&fx, 1, &who, RS21, 4, &our_bytes, 100);
            let theirs = async {
                reached.notified().await;
                let rival = attempt(2);
                let outcome =
                    upload_over(&fx, &fx.store.inner, 1, &rival, RS21, 4, &rival_bytes, 900).await;
                release.notify_one();
                outcome
            };
            let (ours, theirs) = tokio::join!(ours, theirs);
            assert_eq!(theirs.expect("the rival ran"), committed(1, &rival_bytes));
            assert_eq!(
                ours.expect("the attempt ran"),
                UploadPartOutcome::Refused(Refusal::OperationAborted),
                "never Committed for the rival's record"
            );
            let compensations = commits_of(&fx.log, Kind::Compensation);
            assert_eq!(compensations.len(), 1);
            assert!(compensations[0].3, "compensated under its own slot");
            let commits = commits_of(&fx.log, Kind::Commit);
            assert_eq!(
                required_slot(&compensations[0].0),
                required_slot(&commits[0].0)
            );
            assert_owes(
                &fx,
                &per_part_key(&fx, 0, 1, &who),
                &landed_intents(&fx.log),
            )
            .await;
            let (record, _) = committed_part(&fx, 1).await;
            assert_eq!(*record.digest(), sha(&rival_bytes));
        }
    })
    .await;
}

// ──────────────────────── (F) re-upload and ceilings ────────────────────────

/// **(F)** Re-uploading a committed part number replaces it under `require(part == prior)` and
/// owes the prior part's chunks in a `retire:bytes:` obligation **in the same batch**.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_re_upload_replaces_the_part_and_retires_the_prior_chunks_in_one_batch() {
    within(async {
        let fx = fixture().await;
        let (prior, prior_bytes) = prior_part(&fx).await;
        let who = attempt(1);
        let bytes = body(16, 8);
        let outcome = upload(&fx, 1, &who, RS21, 4, &bytes, 100)
            .await
            .expect("the attempt ran");
        assert_eq!(outcome, committed(1, &bytes));
        let commits = commits_of(&fx.log, Kind::Commit);
        assert_eq!(commits.len(), 1);
        let batch = &commits[0].0;
        assert!(batch
            .preconditions
            .iter()
            .any(|pre| pre.key == part_key(&fx.upload, part(1))
                && pre.expected.as_ref() == Some(&prior_bytes)));
        let key = per_part_key(&fx, 0, 1, &who);
        assert!(batch
            .preconditions
            .iter()
            .any(|pre| pre.key == key && pre.expected.is_none()));
        assert!(batch.puts.iter().any(|(put, _)| *put == key), "same batch");
        let owed: Vec<(ChunkId, Vec<DServerId>)> = prior
            .chunks()
            .iter()
            .map(|chunk| (chunk.id, chunk.placement.clone()))
            .collect();
        assert_owes(&fx, &key, &owed).await;
        let (record, _) = committed_part(&fx, 1).await;
        assert_eq!(*record.digest(), sha(&bytes));
    })
    .await;
}

/// **(F)** The staged-chunk ceiling is a read-time check over `psum:`, and 0016 allows the race it
/// leaves. With summaries planted one chunk below `MAX_STAGED_CHUNKS`, two attempts on different
/// part numbers each stage one chunk; the wrapper holds the first commit until the second has
/// committed. Both read the one chunk of headroom and both commit: the total is
/// `MAX_STAGED_CHUNKS + 1`, inside the bound `MAX_STAGED_CHUNKS + MAX_INFLIGHT_PARTS ×
/// MAX_PART_CHUNKS`. A third commit after them is refused `EntityTooLarge` and compensates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_the_staged_ceiling_is_read_time_and_its_overshoot_is_bounded() {
    within(async {
        let fx = fixture().await;
        let ceiling = u64::from(MAX_STAGED_CHUNKS);
        let full = u64::from(MAX_PART_CHUNKS);
        let mut planted = WriteBatch::new();
        let mut remaining = ceiling - 1;
        let mut number = 1_000;
        while remaining > 0 {
            let chunks = remaining.min(full);
            let summary = PartSummary {
                chunks: chunks as u32,
                len: chunks,
                digest: sha(&[]),
                committed_at_millis: START,
            };
            planted = planted.put(
                psum_key(&fx.upload, part(number)),
                metadata::encode(&summary),
            );
            remaining -= chunks;
            number += 1;
        }
        fx.store.inner.commit(planted).await.unwrap();
        let reached = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fx.store.script(vec![Rule::at(
            Kind::Commit,
            1,
            vec![Pre::Gate {
                reached: reached.clone(),
                release: release.clone(),
            }],
            Then::Pass,
        )]);
        let (one, two) = (body(17, 4), body(18, 4));
        let (first_attempt, second_attempt) = (attempt(1), attempt(2));
        let first = upload(&fx, 1, &first_attempt, RS21, 4, &one, 100);
        let second = async {
            reached.notified().await;
            let outcome = upload(&fx, 2, &second_attempt, RS21, 4, &two, 200).await;
            release.notify_one();
            outcome
        };
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.expect("the first ran"), committed(1, &one));
        assert_eq!(second.expect("the second ran"), committed(2, &two));
        let total: u64 = fx
            .store
            .inner
            .scan(&psum_range(&fx.upload))
            .await
            .unwrap()
            .iter()
            .map(|(_, value)| u64::from(decode_part_summary(value).unwrap().chunks))
            .sum();
        assert_eq!(total, ceiling + 1, "both read the one chunk of headroom");
        assert!(total <= ceiling + u64::from(MAX_INFLIGHT_PARTS) * full);

        let third = attempt(3);
        let outcome = upload(&fx, 3, &third, RS21, 4, &body(19, 4), 300)
            .await
            .expect("the third ran");
        assert_eq!(
            outcome,
            UploadPartOutcome::Refused(Refusal::EntityTooLarge {
                chunks: ceiling + 2,
                limit: ceiling,
            })
        );
        assert!(
            slots_of(&fx, &third).await.is_empty(),
            "compensated: slot released"
        );
        assert!(owned_of(&fx, 3).await.is_empty());
        assert!(fx
            .store
            .inner
            .get(&per_part_key(&fx, 0, 3, &third))
            .await
            .unwrap()
            .is_some());
    })
    .await;
}

/// **(F)** A part that would stage more than `MAX_PART_CHUNKS` chunks is refused
/// `EntityTooLarge` before the chunk past the cap is staged, and compensates what it staged: no
/// part, no owned entry, no slot, and one obligation owing every staged chunk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_part_past_max_part_chunks_is_refused_and_compensated() {
    within(async {
        let fx = fixture().await;
        let who = attempt(1);
        let cap = MAX_PART_CHUNKS as usize;
        let outcome = upload(&fx, 1, &who, EcScheme::None, 1, &body(20, cap + 1), 100)
            .await
            .expect("the attempt ran");
        assert_eq!(
            outcome,
            UploadPartOutcome::Refused(Refusal::EntityTooLarge {
                chunks: cap as u64 + 1,
                limit: u64::from(MAX_PART_CHUNKS),
            })
        );
        assert_eq!(
            landed_intents(&fx.log).len(),
            cap,
            "the chunk past the cap was not staged"
        );
        assert!(fx
            .store
            .inner
            .get(&part_key(&fx.upload, part(1)))
            .await
            .unwrap()
            .is_none());
        assert!(owned_of(&fx, 1).await.is_empty());
        assert!(slots_of(&fx, &who).await.is_empty());
        assert_owes(
            &fx,
            &per_part_key(&fx, 0, 1, &who),
            &landed_intents(&fx.log),
        )
        .await;
    })
    .await;
}

/// The drain-fence key has one home in core, and its bytes are the custodian's:
/// `desired:dserver:<id>`.
#[test]
fn the_drain_fence_key_keeps_the_custodians_bytes() {
    assert_eq!(DESIRED_DSERVER_PREFIX, b"desired:dserver:");
    assert_eq!(desired_dserver_key(7), b"desired:dserver:7".to_vec());
    assert_eq!(
        metadata::parse_desired_dserver_key(&desired_dserver_key(u64::MAX)),
        Some(u64::MAX)
    );
}
