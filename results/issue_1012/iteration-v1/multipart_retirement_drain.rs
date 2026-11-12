//! Issue #659 child-1 (#1012): the **retirement drain step** — proposal 0016 decision 4 rules 1–2
//! (`0016:1079-1290`), the *Retirement drain step* row of the batch inventory (`0016:668`) and the
//! "no cursor record class" rule (`0016:613-619`) — dispatched from the deployed reconcile step.
//!
//! Every leg runs over the real embedded metadata store, `RedbMetadataStore::in_memory()`, behind
//! a recorder that logs each batch that lands, and over a placement-aware fleet of in-memory D
//! servers. The drain is driven **only through `reconcile_step`**, with the inputs the deployed
//! loop hands it (`crates/server/src/custodian.rs`, `run_reconstruction_until`): a fresh
//! `ReconstructionContext` per pass for the reconstruction pass the retirement leg rides, and a
//! fresh `GcContext` per pass for GC, which reclaims what the drain evidenced. No test-only entry.
//!
//! * **(B) Complete, then drain.** A subset Complete (#658) publishes parts 1 and 3 and leaves
//!   part 2 unnamed. Reconcile passes drain both obligations the flip installed: no `part:` or
//!   `psum:` record remains, no `orphan:` mark ever names a chunk of the published map
//!   (`0016:1503`), every fragment of part 2 is marked in a commit no later than the one deleting
//!   its record (its marks span several commits, so the order is a real one), and once GC runs
//!   past the orphan grace part 2's fragments are gone while the published ones stay.
//! * **(C) Convergence at scale.** `{parts}` obligations over 4,001 planted parts, one of each
//!   mode, and one part whose 1,422 marks need six commits, drain to empty. Every drain commit
//!   is within `MAX_BATCH_BYTES` and `MAX_BATCH_OPS` (preconditions counted); each obligation's
//!   commits form one chain, each requiring the bytes the previous one left with a cursor
//!   strictly past the previous one's, and only the last deletes the obligation. A commit that
//!   fails mid-pass ends the pass, and a fresh custodian resumes from the stored cursor. A
//!   second drain over the empty ledger commits nothing.
//! * **(C′) The mark guard, `reclaiming` first.** A `reclaiming` mark carrying another event is
//!   left byte-identical, with the obligation and its cursor kept, until GC's key delete lands;
//!   then it is written fresh under `require_absent`. Beside it: the same event is skipped,
//!   another event and a legacy mark are re-stamped under `require(== prior)`, an absent mark is
//!   written under `require_absent`.
//! * **(E-hold)** A `part:` record whose placement is not one D server per fragment is never
//!   deleted unmarked: the step marks nothing from it, deletes nothing, answers `Blocked` with a
//!   `hold` audit event, keeps the obligation and its cursor, and GC keeps every fragment.
//! * **(G)** A `Completing@3` session fenced by the post-restore pass, with segments written and
//!   parts staged, is drained one step at a time; after every drain commit the production
//!   `reconcile_after_restore` runs and never names the session, until both obligations are gone.
//!
//! The recorder keeps its state in struct fields, never in a `static` (ADR-0035); the audit
//! capture is a thread-local default subscriber over a shared buffer.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Debug;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, decode_orphan_mark, encode_orphan_mark, orphan_key, parse_orphan_key, ChunkRef, EcScheme,
    InodeId, InodeRecord, OrphanMark, SegmentGroup, SegmentNonce, SegmentRecord,
};
use wyrd_core::multipart::commit::upload_part;
use wyrd_core::multipart::complete::{
    complete_upload, CompleteRequest, NamedParts, PublishRequest,
};
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::stage::{StageClock, StageRequest, StageRuntime};
use wyrd_core::multipart::{
    self as mp, decode_part_record, decode_retire_obligation, part_key, psum_key, retire_key,
    AttemptId, Budget, CompleteOutcome, CreateOutcome, Digest, PartNumber, PartSummary,
    RetireCursor, RetireMode, RetireToken, RetireWork, UploadId, UploadPartOutcome,
    MAX_BATCH_BYTES, MAX_BATCH_OPS, MAX_INFLIGHT_PARTS, MAX_PART_CHUNKS,
};
use wyrd_core::placement::Topology;
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone,
    GcContext, ReconcileError, Reconciled, ReconstructionContext, RestoreReport,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    BoxError, ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    PlacementChunkStore, Result as StoreResult, ScanPage, WriteBatch,
};

/// Every D server of the fleet, each its own failure domain.
const SERVERS: u64 = 12;
/// Six fragments a chunk: what staging writes.
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };
/// Nine fragments a chunk: the widest scheme the sizing assumes.
const RS63: EcScheme = EcScheme::ReedSolomon { k: 6, m: 3 };
/// The orphan grace GC is run with.
const GRACE: u64 = 10_000;
/// A runaway guard on any leg's drive loop.
const MAX_PASSES: usize = 64;
/// The upload's bucket and the marker Create checks.
const BUCKET: &str = "photos";
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
const ROOT: InodeId = 0;
const CLOCK: &str = "wall";
const CREATED_AT: u64 = 1_000;
const FENCED_AT: u64 = 2_000;
const COMPLETED_AT: u64 = 2_500;
/// Bytes per staged chunk.
const CHUNK: usize = 64;

// ──────────────────────────────── the store ────────────────────────────────

/// `RedbMetadataStore::in_memory()` behind a recorder: every batch that lands is logged, in
/// order. It can fail one **drain** commit (a batch whose first precondition requires a
/// `retire:` key's exact value — only the drain step builds one) without applying it, and it can
/// run the production post-restore pass after every drain commit that lands.
struct Store {
    inner: RedbMetadataStore,
    log: Mutex<Vec<WriteBatch>>,
    drain_commits: AtomicUsize,
    fail_drain_commit: Mutex<Option<usize>>,
    /// How many batches the log held when the injected fault struck.
    failed_at: Mutex<Option<usize>>,
    restore_over: Option<Fleet>,
    reports: Mutex<Vec<RestoreReport>>,
}

impl Store {
    fn new() -> Self {
        Self {
            inner: RedbMetadataStore::in_memory().expect("an in-memory redb store"),
            log: Mutex::new(Vec::new()),
            drain_commits: AtomicUsize::new(0),
            fail_drain_commit: Mutex::new(None),
            failed_at: Mutex::new(None),
            restore_over: None,
            reports: Mutex::new(Vec::new()),
        }
    }

    /// A store that runs `reconcile_after_restore` over `fleet` after every drain commit.
    fn restoring(fleet: Fleet) -> Self {
        Self {
            restore_over: Some(fleet),
            ..Self::new()
        }
    }

    /// Plant `batch` blind, outside the log.
    async fn seed(&self, batch: WriteBatch) {
        let outcome = self.inner.commit(batch).await.expect("the seed commits");
        assert_eq!(outcome, CommitOutcome::Committed);
    }

    async fn value(&self, key: &[u8]) -> Option<Bytes> {
        self.inner.get(key).await.expect("a read")
    }

    async fn keys(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
        let rows = self.inner.scan(prefix).await.expect("a scan");
        let mut keys: Vec<Vec<u8>> = rows.into_iter().map(|(key, _)| key).collect();
        keys.sort();
        keys
    }

    fn take_log(&self) -> Vec<WriteBatch> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    fn log_len(&self) -> usize {
        self.log.lock().unwrap().len()
    }
}

/// Whether `batch` is a drain step's commit: it opens with `require(retire:… == prior)`.
fn is_drain(batch: &WriteBatch) -> bool {
    batch
        .preconditions
        .first()
        .is_some_and(|p| p.key.starts_with(b"retire:") && p.expected.is_some())
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
        let drain = is_drain(&batch);
        if drain {
            let n = self.drain_commits.fetch_add(1, Ordering::SeqCst);
            let mut fail = self.fail_drain_commit.lock().unwrap();
            if *fail == Some(n) {
                *fail = None;
                *self.failed_at.lock().unwrap() = Some(self.log_len());
                return Err(BoxError::from("the recorder failed this drain commit"));
            }
        }
        let outcome = self.inner.commit(batch.clone()).await?;
        if outcome == CommitOutcome::Committed {
            self.log.lock().unwrap().push(batch);
            if let (true, Some(fleet)) = (drain, &self.restore_over) {
                let disks = fleet.disks();
                let report = restore_pass(&self.inner, &disks).await;
                self.reports.lock().unwrap().push(report);
            }
        }
        Ok(outcome)
    }
}

/// The production post-restore pass over `meta` and `disks`.
async fn restore_pass(meta: &dyn MetadataStore, disks: &[Disk]) -> RestoreReport {
    let fleet = fleet_of(disks);
    let ctx = GcContext {
        meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_after_restore(&ctx, 1_000_000)
        .await
        .expect("the post-restore pass runs")
}

/// The bytes a batch carries — every precondition's key and value, every put's key and value,
/// every delete's key — and its operations, preconditions counted (`0016:630-648`).
fn batch_size(batch: &WriteBatch) -> (u64, u64) {
    let pre: usize = batch
        .preconditions
        .iter()
        .map(|p| p.key.len() + p.expected.as_ref().map_or(0, Bytes::len))
        .sum();
    let puts: usize = batch.puts.iter().map(|(k, v)| k.len() + v.len()).sum();
    let deletes: usize = batch.deletes.iter().map(Vec::len).sum();
    let ops = batch.preconditions.len() + batch.puts.len() + batch.deletes.len();
    ((pre + puts + deletes) as u64, ops as u64)
}

fn puts<'b>(batch: &'b WriteBatch, key: &[u8]) -> Option<&'b Bytes> {
    batch
        .puts
        .iter()
        .find(|(put, _)| put.as_slice() == key)
        .map(|(_, value)| value)
}

fn deletes(batch: &WriteBatch, key: &[u8]) -> bool {
    batch
        .deletes
        .iter()
        .any(|deleted| deleted.as_slice() == key)
}

fn required<'b>(batch: &'b WriteBatch, key: &[u8]) -> Option<&'b Option<Bytes>> {
    batch
        .preconditions
        .iter()
        .find(|p| p.key.as_slice() == key)
        .map(|p| &p.expected)
}

// ──────────────────────────────── the fleet ────────────────────────────────

/// In-memory D servers addressed by id: what staging writes each fragment to, at the server its
/// placement names, and what GC lists and deletes through one view per server.
#[derive(Clone)]
struct Fleet(Arc<Mutex<BTreeMap<DServerId, HashMap<FragmentId, Bytes>>>>);

impl Fleet {
    fn new() -> Self {
        let servers = (0..SERVERS).map(|id| (id, HashMap::new())).collect();
        Self(Arc::new(Mutex::new(servers)))
    }

    fn place(&self, dserver: DServerId, frag: FragmentId) {
        let mut servers = self.0.lock().unwrap();
        let held = servers.get_mut(&dserver).expect("a known D server");
        held.insert(frag, Bytes::from_static(b"fragment"));
    }

    fn holds(&self, dserver: DServerId, frag: FragmentId) -> bool {
        let servers = self.0.lock().unwrap();
        servers
            .get(&dserver)
            .is_some_and(|held| held.contains_key(&frag))
    }

    fn disks(&self) -> Vec<Disk> {
        (0..SERVERS)
            .map(|id| Disk {
                fleet: self.clone(),
                id,
            })
            .collect()
    }
}

/// One D server of the fleet, as GC and the reconstruction pass address it.
struct Disk {
    fleet: Fleet,
    id: DServerId,
}

fn fleet_of(disks: &[Disk]) -> Vec<(DServerId, &dyn ChunkStore)> {
    disks
        .iter()
        .map(|disk| (disk.id, disk as &dyn ChunkStore))
        .collect()
}

#[async_trait]
impl ChunkStore for Disk {
    async fn put_fragment(&self, id: FragmentId, bytes: Bytes, _: Option<u64>) -> StoreResult<()> {
        let mut servers = self.fleet.0.lock().unwrap();
        servers.entry(self.id).or_default().insert(id, bytes);
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> StoreResult<Option<Bytes>> {
        let servers = self.fleet.0.lock().unwrap();
        Ok(servers
            .get(&self.id)
            .and_then(|held| held.get(&id).cloned()))
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        let servers = self.fleet.0.lock().unwrap();
        let held = servers.get(&self.id);
        Ok(held
            .map(|h| h.keys().copied().collect())
            .unwrap_or_default())
    }

    async fn delete_fragment(&self, id: FragmentId) -> StoreResult<()> {
        let mut servers = self.fleet.0.lock().unwrap();
        if let Some(held) = servers.get_mut(&self.id) {
            held.remove(&id);
        }
        Ok(())
    }

    async fn health(&self) -> StoreResult<Health> {
        Ok(Health::Healthy)
    }
}

#[async_trait]
impl ChunkStore for Fleet {
    async fn put_fragment(&self, _: FragmentId, _: Bytes, _: Option<u64>) -> StoreResult<()> {
        Err("Fleet: a write must address a D server".into())
    }

    async fn get_fragment(&self, _: FragmentId) -> StoreResult<Option<Bytes>> {
        Ok(None)
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        let servers = self.0.lock().unwrap();
        Ok(servers.values().flat_map(|h| h.keys().copied()).collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> StoreResult<()> {
        for held in self.0.lock().unwrap().values_mut() {
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
        let servers = self.0.lock().unwrap();
        Ok(servers
            .get(&dserver)
            .and_then(|held| held.get(&id).cloned()))
    }

    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        fragment: Bytes,
        _deadline: Option<u64>,
    ) -> StoreResult<()> {
        let mut servers = self.0.lock().unwrap();
        let held = servers.get_mut(&dserver).ok_or("Fleet: unknown D server")?;
        held.insert(id, fragment);
        Ok(())
    }
}

fn topology() -> Topology {
    let mut topology = Topology::default();
    for id in 0..SERVERS {
        topology.register(id, format!("d{id:02}"));
    }
    topology
}

// ──────────────────────────────── the custodian ────────────────────────────────

/// One elected custodian: the fenced zone it reconciles under.
struct Custody {
    zone: FencedZone,
    custodian: Custodian,
}

impl Custody {
    /// A freshly elected custodian, as a fresh process elects one: nothing carried over.
    async fn elect() -> Self {
        let coord = MemCoordination::new();
        let custodian = Custodian::elect(&coord, "zone-retirement-drain")
            .await
            .expect("elected");
        let mut zone = FencedZone::new();
        zone.install(custodian.leadership());
        Self { zone, custodian }
    }

    /// The deployed loop's **reconstruction pass** at `now`, with a fresh context: the
    /// reconstruction loop over an empty repair queue, then the retirement leg.
    async fn drain(
        &self,
        meta: &dyn MetadataStore,
        fleet: &Fleet,
        now: u64,
    ) -> Result<Reconciled, ReconcileError> {
        let disks = fleet.disks();
        let fleet = fleet_of(&disks);
        let topology = topology();
        let clock = ManualClock::new(now);
        let ctx = ReconstructionContext {
            meta,
            fleet: &fleet,
            topology: &topology,
            unreachable: &[],
            clock: &clock,
            staged_write_window_millis: wyrd_custodian::gc::W_WRITE_MILLIS,
        };
        let step = reconcile_step(
            &self.zone,
            &self.custodian,
            None,
            None,
            Some(&ctx),
            None,
            now,
        );
        step.await
    }

    /// The deployed loop's **GC pass** at `now`, with a fresh context.
    async fn gc(
        &self,
        meta: &dyn MetadataStore,
        fleet: &Fleet,
        now: u64,
    ) -> Result<Reconciled, ReconcileError> {
        let disks = fleet.disks();
        let fleet = fleet_of(&disks);
        let ctx = GcContext {
            meta,
            fleet: &fleet,
            grace_window_millis: GRACE,
            expired_pending: ExpiredPendingPolicy::Defer,
        };
        reconcile_step(
            &self.zone,
            &self.custodian,
            Some(&ctx),
            None,
            None,
            None,
            now,
        )
        .await
    }
}

// ──────────────────────────────── records ────────────────────────────────

fn upload(n: u128) -> UploadId {
    UploadId::new(format!("{n:032x}")).unwrap()
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).unwrap()
}

fn session_token(id: &UploadId, epoch: u64) -> RetireToken {
    RetireToken::Session {
        upload_id: id.clone(),
        epoch,
        part: None,
    }
}

/// A chunk of `scheme` placed on `placement`.
fn chunk(id: ChunkId, scheme: EcScheme, placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id,
        scheme,
        len: 5,
        placement,
    }
}

/// Every fragment position `chunk` names, by its placement.
fn positions(chunk: &ChunkRef) -> Vec<(DServerId, FragmentId)> {
    chunk
        .placement
        .iter()
        .enumerate()
        .map(|(index, &dserver)| {
            let index = u16::try_from(index).unwrap();
            let frag = FragmentId {
                chunk: chunk.id,
                index,
            };
            (dserver, frag)
        })
        .collect()
}

/// The blind batch that plants part `n` of `id` naming `chunks`, with its summary, each in its
/// codec's canonical spelling — checked by decoding and re-encoding.
fn plant_part(batch: WriteBatch, id: &UploadId, n: u32, chunks: &[ChunkRef]) -> WriteBatch {
    let listed: Vec<String> = chunks
        .iter()
        .map(|c| String::from_utf8(metadata::encode(c).to_vec()).unwrap())
        .collect();
    let len: u64 = chunks.iter().map(|c| c.len).sum();
    let digest = Digest::from_bytes([0xef; 32]);
    let record = format!(
        "{{\"chunks\":[{}],\"len\":{len},\"digest\":\"{digest}\",\"committed_at_millis\":800,\
         \"session_epoch\":3}}",
        listed.join(",")
    );
    let decoded = decode_part_record(record.as_bytes()).expect("a canonical part");
    assert_eq!(metadata::encode(&decoded), record.as_bytes());
    let summary = PartSummary {
        chunks: u32::try_from(chunks.len()).unwrap(),
        len,
        digest,
        committed_at_millis: 800,
    };
    batch
        .put(part_key(id, part(n)), Bytes::from(record))
        .put(psum_key(id, part(n)), metadata::encode(&summary))
}

/// `value` as the obligation at `key`, checked: an installer's own spelling.
fn obligation(key: &[u8], value: &str) -> Bytes {
    decode_retire_obligation(key, value.as_bytes()).expect("an obligation a writer installs");
    Bytes::from(value.to_owned())
}

/// The cursor the obligation at `key` holds, as stored in `value`.
fn cursor_of(key: &[u8], value: &[u8]) -> Option<RetireCursor> {
    let (_, _, payload) = decode_retire_obligation(key, value).expect("a decodable obligation");
    payload.cursor().copied()
}

/// A session record at `epoch` in `state` with segment nonce `nonce`, in the codec's own spelling.
fn session_bytes(state: &str, epoch: u64, nonce: &str) -> Bytes {
    let bytes = format!(
        "{{\"parent\":42,\"object\":\"o\",\"created_at_millis\":100,\"clock_source\":\"wall\",\
         \"segment_nonce\":\"{nonce}\",\"epoch\":{epoch},\"attempts\":1,\"state\":{state}}}"
    );
    let record = mp::decode_session_record(bytes.as_bytes()).expect(state);
    assert_eq!(metadata::encode(&record), bytes.as_bytes());
    Bytes::from(bytes)
}

/// Every `orphan:` mark in `meta`: its position and its decoded value.
async fn marks(meta: &Store) -> BTreeMap<(DServerId, ChunkId, u16), OrphanMark> {
    let rows = meta.inner.scan(b"orphan:").await.unwrap();
    rows.into_iter()
        .map(|(key, value)| {
            let (dserver, frag) = parse_orphan_key(&key).expect("an orphan key");
            let mark = decode_orphan_mark(&value).expect("a decodable mark");
            ((dserver, frag.chunk, frag.index), mark)
        })
        .collect()
}

fn structured(at: u64, event: &str) -> Bytes {
    encode_orphan_mark(&OrphanMark::structured(at, event).unwrap())
}

// ──────────────────────────────── the audit seam ────────────────────────────────

/// The retirement leg's audit events, captured by a thread-local default subscriber.
#[derive(Clone, Default)]
struct Audit(Arc<Mutex<Vec<HashMap<String, String>>>>);

impl Audit {
    /// Capture this thread's audit events until the guard drops.
    fn capture(&self) -> tracing::subscriber::DefaultGuard {
        let subscriber = tracing_subscriber::registry().with(self.clone());
        tracing::subscriber::set_default(subscriber)
    }

    fn events(&self, action: &str) -> Vec<HashMap<String, String>> {
        let events = self.0.lock().unwrap();
        events
            .iter()
            .filter(|fields| fields.get("action").is_some_and(|a| a == action))
            .cloned()
            .collect()
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Audit {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() == "wyrd.custodian.retirement.audit" {
            let mut fields = FieldText(HashMap::new());
            event.record(&mut fields);
            self.0.lock().unwrap().push(fields.0);
        }
    }
}

struct FieldText(HashMap<String, String>);

impl Visit for FieldText {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

// ───────────────────────── (B) Complete, then drain ─────────────────────────

/// Create's runtime: one fixed identity, no jitter, no wait.
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

/// Staging's runtime: a clock no leg moves and a timer that never fires.
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

/// A session made by Create, three parts staged by `UploadPart` onto the fleet, and a Complete
/// naming parts 1 and 3. Part 2's 90 chunks of six fragments need several drain commits.
struct Completed {
    id: UploadId,
    inode: InodeId,
    published: BTreeSet<ChunkId>,
    unnamed: Vec<(DServerId, FragmentId)>,
    unnamed_part: Vec<u8>,
}

async fn complete_a_subset(meta: &Store, fleet: &Fleet) -> Completed {
    meta.seed(WriteBatch::new().put(metadata::bucket_key(BUCKET), MARKER))
        .await;
    let object = format!("{BUCKET}/object-1");
    let profile: Budget = metadata::decode(
        br#"{"w_ref":16,"max_part_chunks":1,"max_parts_per_session":1,"max_inflight_parts":1,"max_staged_chunks":1}"#,
    )
    .expect("a legal profile");
    let nonce = SegmentNonce::new(format!("{:032x}", 0x5a5a_5a5a_u128 ^ 1)).unwrap();
    let request = CreateRequest {
        bucket: BUCKET,
        parent: ROOT,
        object: &object,
        content_type: None,
        created_at_millis: CREATED_AT,
        clock_source: CLOCK,
    };
    let created = create_session(
        meta,
        &profile,
        &request,
        &mut Fixed(Some((upload(1), nonce))),
    )
    .await
    .expect("the create ran");
    let CreateOutcome::Created { upload_id: id } = created else {
        panic!("expected Created, got {created:?}");
    };
    let topology = topology();
    let mut runtime = Runtime {
        clock: ManualClock::new(CREATED_AT),
        next_chunk: Arc::new(AtomicU64::new(0)),
    };
    let mut digests = BTreeMap::new();
    for (n, chunks) in [(1_u32, 3_usize), (2, 90), (3, 2)] {
        let body: Vec<u8> = (0..chunks * CHUNK).map(|i| (i as u8) ^ n as u8).collect();
        let attempt = AttemptId::new(format!("{:032x}", 0xa77e_0000_u128 + u128::from(n))).unwrap();
        let stage = StageRequest {
            upload_id: &id,
            part_number: part(n),
            attempt_id: &attempt,
            max_inflight_parts: MAX_INFLIGHT_PARTS,
            lease_ttl_millis: 10_000,
            chunk_size: CHUNK,
            scheme: RS42,
            topology: &topology,
            write_window_millis: 3_000,
            max_part_chunks: MAX_PART_CHUNKS,
            body_idle_millis: 1_000_000,
        };
        let body = stream::iter(vec![Ok(Bytes::from(body))]);
        match upload_part(meta, fleet, &mut runtime, &stage, body).await {
            Ok(UploadPartOutcome::Committed { digest, .. }) => {
                digests.insert(n, digest);
            }
            other => panic!("expected part {n} committed, got {other:?}"),
        }
    }
    let named = NamedParts::new(vec![(part(1), digests[&1]), (part(3), digests[&3])]).unwrap();
    let request = CompleteRequest {
        bucket: BUCKET,
        parent: ROOT,
        object: &object,
        parts: &named,
        fenced_at_millis: FENCED_AT,
        clock_source: CLOCK,
    };
    let publish = PublishRequest {
        fresh_inode: 500,
        completed_at_millis: COMPLETED_AT,
    };
    let outcome = complete_upload(meta, &id, &request, &publish).await;
    let Ok(CompleteOutcome::Published(publication)) = outcome else {
        panic!("expected the subset to publish, got {outcome:?}");
    };
    let inode: InodeRecord = metadata::decode(
        &meta
            .value(&metadata::inode_key(publication.inode))
            .await
            .expect("the published inode"),
    )
    .unwrap();
    let published = inode
        .chunk_map
        .as_flat()
        .expect("a flat publication")
        .iter()
        .map(|c| c.id)
        .collect();
    let unnamed_part = part_key(&id, part(2));
    let record = meta.value(&unnamed_part).await.expect("part 2's record");
    let unnamed = decode_part_record(&record)
        .unwrap()
        .chunks()
        .iter()
        .flat_map(positions)
        .collect();
    Completed {
        id,
        inode: publication.inode,
        published,
        unnamed,
        unnamed_part,
    }
}

/// **(B)** A subset Complete (#658), then reconcile passes as the deployed loop runs them. The
/// drain is reached only through the reconstruction pass's `reconcile_step`.
///
/// Red on any build whose `reconcile_step` does not dispatch the drain: both obligations stay,
/// the part records stay, and part 2's fragments are never marked or reclaimed.
#[tokio::test]
async fn b_a_subset_complete_drains_and_its_unnamed_part_is_reclaimed_after_grace() {
    let (meta, fleet) = (Store::new(), Fleet::new());
    let done = complete_a_subset(&meta, &fleet).await;
    let token = session_token(&done.id, 1).to_string();
    let obligations = meta.keys(b"retire:").await;
    assert_eq!(
        obligations,
        vec![
            retire_key(RetireMode::Bytes, &session_token(&done.id, 1)),
            retire_key(RetireMode::Records, &session_token(&done.id, 1)),
        ],
        "the flip installed the unnamed part's bytes and the published parts' records"
    );
    assert!(
        done.unnamed.len() > MAX_BATCH_OPS as usize / 2,
        "part 2 spans commits"
    );
    for &(dserver, frag) in &done.unnamed {
        assert!(
            fleet.holds(dserver, frag),
            "staging wrote part 2's {frag:?}"
        );
    }
    meta.take_log();
    let custody = Custody::elect().await;

    let drain_at = 10_000;
    let mut passes = 0;
    while !meta.keys(b"retire:").await.is_empty() {
        passes += 1;
        assert!(passes <= MAX_PASSES, "the obligations never drained");
        custody
            .drain(&meta, &fleet, drain_at)
            .await
            .expect("a drain pass");
        custody
            .gc(&meta, &fleet, drain_at)
            .await
            .expect("a gc pass");
    }

    let range = |prefix: &str| format!("{prefix}{}:", done.id).into_bytes();
    assert!(
        meta.keys(&range("part:")).await.is_empty(),
        "no part: record remains"
    );
    assert!(
        meta.keys(&range("psum:")).await.is_empty(),
        "no psum: record remains"
    );
    let log = meta.take_log();
    for batch in &log {
        for (key, _) in &batch.puts {
            if let Some((_, frag)) = parse_orphan_key(key) {
                assert!(
                    !done.published.contains(&frag.chunk),
                    "a commit marked {frag:?}, a chunk of the published map (0016:1503)"
                );
            }
        }
    }
    let found = marks(&meta).await;
    assert!(
        found
            .keys()
            .all(|(_, chunk, _)| !done.published.contains(chunk)),
        "no mark names a published chunk"
    );
    let deleted_at = log
        .iter()
        .position(|batch| deletes(batch, &done.unnamed_part))
        .expect("part 2's record was deleted");
    let mut marked_before = 0;
    for &(dserver, frag) in &done.unnamed {
        let key = orphan_key(dserver, frag);
        let at = log
            .iter()
            .position(|batch| puts(batch, &key).is_some())
            .unwrap_or_else(|| panic!("{frag:?} on {dserver} was never marked"));
        assert!(
            at <= deleted_at,
            "{frag:?} was marked in commit {at}, after part 2's record went in commit {deleted_at}"
        );
        marked_before += usize::from(at < deleted_at);
        let mark = decode_orphan_mark(puts(&log[at], &key).unwrap()).unwrap();
        assert_eq!(
            mark.event(),
            Some(token.as_str()),
            "the mark names the obligation"
        );
        assert_eq!(
            mark.orphaned_at_millis(),
            drain_at,
            "grace starts at the drain"
        );
    }
    assert!(
        marked_before > 0,
        "some of part 2's marks landed in an earlier commit"
    );

    custody
        .gc(&meta, &fleet, drain_at + GRACE - 1)
        .await
        .unwrap();
    for &(dserver, frag) in &done.unnamed {
        assert!(
            fleet.holds(dserver, frag),
            "{frag:?} reclaimed inside its grace"
        );
    }
    let after_grace = drain_at + GRACE;
    custody.gc(&meta, &fleet, after_grace).await.unwrap();
    for &(dserver, frag) in &done.unnamed {
        assert!(!fleet.holds(dserver, frag), "{frag:?} outlived its grace");
    }
    let inode: InodeRecord =
        metadata::decode(&meta.value(&metadata::inode_key(done.inode)).await.unwrap()).unwrap();
    for chunk in inode.chunk_map.as_flat().unwrap() {
        for (dserver, frag) in positions(chunk) {
            assert!(
                fleet.holds(dserver, frag),
                "a published fragment was reclaimed"
            );
        }
    }
}

// ───────────────────────── (C) convergence at scale ─────────────────────────

/// The parts each `{parts}` obligation of (C) names.
const MANY_PARTS: u32 = 4_001;

/// **(C)** Three obligations planted as canonical records:
///
/// * `retire:bytes:s:<X>:1` owing `{parts: [[1, 4001]]}` — one one-fragment chunk a part;
/// * `retire:records:s:<Y>:1` owing the same, over another session's 4,001 parts;
/// * `retire:bytes:s:<Z>:1` owing `{parts: [[1, 1]]}` — one part of 158 RS(6,3) chunks, 1,422
///   positions, more than one commit's operations can mark.
///
/// The 21st drain commit fails, ending its pass; a freshly elected custodian carries on.
///
/// Red on a drain with no stored cursor (each step re-derives from the start and never
/// finishes) and on one that deletes an obligation after its first batch.
#[tokio::test]
async fn c_obligations_past_one_batch_drain_to_empty_in_bounded_commits() {
    let (meta, fleet) = (Store::new(), Fleet::new());
    let (x, y, z) = (upload(0x0c1), upload(0x0c2), upload(0x0c3));
    let mut batch = WriteBatch::new();
    for n in 1..=MANY_PARTS {
        let dserver = u64::from(n) % SERVERS;
        let bytes_chunk = chunk(0x10_0000 + ChunkId::from(n), EcScheme::None, vec![dserver]);
        let records_chunk = chunk(0x20_0000 + ChunkId::from(n), EcScheme::None, vec![dserver]);
        batch = plant_part(batch, &x, n, &[bytes_chunk]);
        batch = plant_part(batch, &y, n, &[records_chunk]);
        if batch.puts.len() >= 1_000 {
            meta.seed(std::mem::take(&mut batch)).await;
        }
    }
    let wide: Vec<ChunkRef> = (0..u128::from(MAX_PART_CHUNKS))
        .map(|i| chunk(0x30_0000 + i, RS63, (0..9).collect()))
        .collect();
    batch = plant_part(batch, &z, 1, &wide);
    let keys = [
        retire_key(RetireMode::Bytes, &session_token(&x, 1)),
        retire_key(RetireMode::Records, &session_token(&y, 1)),
        retire_key(RetireMode::Bytes, &session_token(&z, 1)),
    ];
    let installed = [
        obligation(&keys[0], r#"{"parts":[[1,4001]]}"#),
        obligation(&keys[1], r#"{"parts":[[1,4001]]}"#),
        obligation(&keys[2], r#"{"parts":[[1,1]]}"#),
    ];
    for (key, value) in keys.iter().zip(&installed) {
        batch = batch.put(key.clone(), value.clone());
    }
    meta.seed(batch).await;
    *meta.fail_drain_commit.lock().unwrap() = Some(20);

    let mut custody = Custody::elect().await;
    let (mut passes, mut failed) = (0, false);
    while !meta.keys(b"retire:").await.is_empty() {
        passes += 1;
        assert!(passes <= MAX_PASSES, "the obligations never drained");
        match custody.drain(&meta, &fleet, 5_000).await {
            Ok(_) => {}
            Err(ReconcileError::Store(fault)) if !failed => {
                assert!(fault.to_string().contains("recorder failed"), "{fault}");
                failed = true;
                // A fresh process: a new election, new contexts, nothing in memory.
                custody = Custody::elect().await;
            }
            Err(other) => panic!("pass {passes}: {other}"),
        }
    }
    // The ledger is empty: so is everything it owed. An obligation deleted before its work was
    // done leaves its records behind.
    for id in [&x, &y, &z] {
        let parts = format!("part:{id}:").into_bytes();
        let left = meta.keys(&parts).await.len();
        assert_eq!(
            left, 0,
            "{id}'s obligation is gone with {left} of its parts left"
        );
        let sums = format!("psum:{id}:").into_bytes();
        assert!(
            meta.keys(&sums).await.is_empty(),
            "{id}'s summaries are gone"
        );
    }

    let log = meta.take_log();
    let drains: Vec<&WriteBatch> = log.iter().filter(|b| is_drain(b)).collect();
    for batch in &drains {
        let (bytes, ops) = batch_size(batch);
        assert!(
            ops <= u64::from(MAX_BATCH_OPS),
            "a commit of {ops} operations"
        );
        assert!(bytes <= MAX_BATCH_BYTES, "a commit of {bytes} bytes");
    }
    let failed_at = *meta.failed_at.lock().unwrap();
    let mut resumed_mid_obligation = false;
    for (key, installed) in keys.iter().zip(&installed) {
        let chain: Vec<&WriteBatch> = drains
            .iter()
            .copied()
            .filter(|b| b.preconditions[0].key == *key)
            .collect();
        assert!(
            chain.len() > 1,
            "{}: drained in one commit",
            String::from_utf8_lossy(key)
        );
        let mut prior = installed.clone();
        let mut last: Option<RetireCursor> = None;
        for (i, batch) in chain.iter().enumerate() {
            assert_eq!(
                batch.preconditions[0].expected.as_ref(),
                Some(&prior),
                "step {i} requires the bytes the step before it left"
            );
            if i + 1 == chain.len() {
                assert!(deletes(batch, key), "the last step deletes the obligation");
                assert!(puts(batch, key).is_none());
                break;
            }
            assert!(
                !deletes(batch, key),
                "step {i} deleted the obligation before the end"
            );
            let value = puts(batch, key).expect("a step that does not finish moves the cursor");
            let cursor = cursor_of(key, value).expect("a stored cursor");
            assert!(
                last.is_none_or(|last| cursor > last),
                "step {i}: {cursor:?} after {last:?}"
            );
            last = Some(cursor);
            prior = value.clone();
        }
        // The fault fell between two steps of one of these chains: the first commit after it
        // required a cursor the failed pass had stored.
        let after_fault = chain.iter().position(|b| {
            let index = log.iter().position(|l| std::ptr::eq(l, *b)).unwrap();
            failed_at.is_some_and(|failed_at| index >= failed_at)
        });
        if let Some(i) = after_fault.filter(|i| *i > 0) {
            let stored = chain[i].preconditions[0].expected.as_ref().unwrap();
            resumed_mid_obligation |= cursor_of(key, stored).is_some();
        }
    }
    assert!(failed, "the injected fault ended a pass");
    assert!(
        resumed_mid_obligation,
        "the restart resumed from a stored cursor"
    );

    let z_chain = drains
        .iter()
        .filter(|b| b.preconditions[0].key == keys[2])
        .count();
    assert!(z_chain >= 6, "the wide part took {z_chain} commits");
    let found = marks(&meta).await;
    let x_token = session_token(&x, 1).to_string();
    let z_token = session_token(&z, 1).to_string();
    for n in 1..=MANY_PARTS {
        let at = (u64::from(n) % SERVERS, 0x10_0000 + ChunkId::from(n), 0);
        let mark = found
            .get(&at)
            .unwrap_or_else(|| panic!("part {n}'s fragment is unmarked"));
        assert_eq!(mark.event(), Some(x_token.as_str()));
        let records = (u64::from(n) % SERVERS, 0x20_0000 + ChunkId::from(n), 0);
        assert!(
            !found.contains_key(&records),
            "a records obligation marked part {n}"
        );
    }
    for chunk in &wide {
        for (dserver, frag) in positions(chunk) {
            let mark = &found[&(dserver, frag.chunk, frag.index)];
            assert_eq!(mark.event(), Some(z_token.as_str()));
        }
    }

    // Over an empty ledger the drain commits nothing and certifies the ledger drained.
    let before = meta.log_len();
    for _ in 0..2 {
        let outcome = custody.drain(&meta, &fleet, 6_000).await.unwrap();
        assert_eq!(outcome, Reconciled::Satisfied);
    }
    assert_eq!(meta.log_len(), before, "a second drain committed something");
}

// ───────────────────────── (C′) the mark guard ─────────────────────────

/// **(C′)** A flat `{generation}` obligation over five one-fragment chunks whose positions hold,
/// in order: this event's own mark (a), another event's mark and a legacy mark (b), a
/// `reclaiming` mark carrying another event (0), and nothing (c).
///
/// Red on a guard that treats a `reclaiming` mark as "another event" and re-stamps it.
#[tokio::test]
async fn c_prime_a_reclaiming_mark_is_never_replaced_and_the_three_arms_hold() {
    let (meta, fleet) = (Store::new(), Fleet::new());
    let token = RetireToken::Generation {
        inode: 9,
        version: 2,
    };
    let event = token.to_string();
    let key = retire_key(RetireMode::Bytes, &token);
    let chunks: Vec<ChunkRef> = (0..5)
        .map(|i| chunk(0x901 + i, EcScheme::None, vec![i as u64]))
        .collect();
    let listed: Vec<String> = chunks
        .iter()
        .map(|c| String::from_utf8(metadata::encode(c).to_vec()).unwrap())
        .collect();
    let installed = obligation(
        &key,
        &format!(
            r#"{{"generation":{{"inode":9,"version":2,"chunks":[{}]}}}}"#,
            listed.join(",")
        ),
    );
    let at: Vec<Vec<u8>> = chunks
        .iter()
        .map(|c| orphan_key(positions(c)[0].0, positions(c)[0].1))
        .collect();
    let same = structured(100, &event);
    let other = structured(100, "g:9:1");
    let legacy = encode_orphan_mark(&OrphanMark::legacy(100));
    let reclaiming = encode_orphan_mark(
        &OrphanMark::structured(100, "g:9:1")
            .unwrap()
            .into_reclaiming(),
    );
    meta.seed(
        WriteBatch::new()
            .put(key.clone(), installed.clone())
            .put(at[0].clone(), same.clone())
            .put(at[1].clone(), other.clone())
            .put(at[2].clone(), legacy.clone())
            .put(at[3].clone(), reclaiming.clone()),
    )
    .await;
    let custody = Custody::elect().await;

    // Step 1: (a) skipped, (b) twice, then (0) stops the step before (c).
    let outcome = custody.drain(&meta, &fleet, 5_000).await.unwrap();
    assert_eq!(outcome, Reconciled::Changed, "the (b) arms landed");
    let log = meta.take_log();
    assert_eq!(log.len(), 1, "one commit");
    let step = &log[0];
    assert_eq!(required(step, &key), Some(&Some(installed.clone())));
    assert_eq!(
        required(step, &at[1]),
        Some(&Some(other.clone())),
        "(b): require(== prior)"
    );
    assert_eq!(
        required(step, &at[2]),
        Some(&Some(legacy.clone())),
        "(b): a legacy mark too"
    );
    assert_eq!(required(step, &at[0]), None, "(a) carries no precondition");
    assert_eq!(required(step, &at[3]), None, "(0) is never written");
    let fresh = structured(5_000, &event);
    assert_eq!(
        meta.value(&at[0]).await,
        Some(same.clone()),
        "(a): skipped, stamp intact"
    );
    assert_eq!(
        meta.value(&at[1]).await,
        Some(fresh.clone()),
        "(b): re-stamped"
    );
    assert_eq!(
        meta.value(&at[2]).await,
        Some(fresh.clone()),
        "(b): legacy re-stamped"
    );
    assert_eq!(
        meta.value(&at[3]).await,
        Some(reclaiming.clone()),
        "(0): byte-identical"
    );
    assert_eq!(meta.value(&at[4]).await, None, "the step stopped at (0)");
    let held = meta.value(&key).await.expect("the obligation stays");
    let cursor = cursor_of(&key, &held).expect("a cursor");
    assert!(
        cursor <= RetireCursor::new(RetireWork::Generation, 3, 0),
        "the cursor is at or before the reclaiming position: {cursor:?}"
    );

    // Step 2: GC has not deleted the key yet. Nothing moves.
    let outcome = custody.drain(&meta, &fleet, 6_000).await.unwrap();
    assert_eq!(outcome, Reconciled::Partial, "held on GC");
    assert!(
        meta.take_log().is_empty(),
        "no commit while the mark is reclaiming"
    );
    assert_eq!(
        meta.value(&at[3]).await,
        Some(reclaiming.clone()),
        "still byte-identical"
    );
    assert_eq!(
        meta.value(&key).await,
        Some(held.clone()),
        "obligation and cursor kept"
    );

    // GC destroys the fragment and deletes the key blind; the next step takes arm (c).
    meta.seed(WriteBatch::new().delete(at[3].clone())).await;
    let outcome = custody.drain(&meta, &fleet, 7_000).await.unwrap();
    assert_eq!(outcome, Reconciled::Changed);
    let log = meta.take_log();
    assert_eq!(log.len(), 1, "one commit finishes it");
    assert_eq!(required(&log[0], &key), Some(&Some(held)));
    assert_eq!(
        required(&log[0], &at[3]),
        Some(&None),
        "(c): require_absent"
    );
    assert_eq!(
        required(&log[0], &at[4]),
        Some(&None),
        "(c): require_absent"
    );
    assert!(
        deletes(&log[0], &key),
        "the finishing step deletes the obligation"
    );
    let fresher = structured(7_000, &event);
    assert_eq!(
        meta.value(&at[3]).await,
        Some(fresher.clone()),
        "a fresh mark of this event"
    );
    assert_eq!(meta.value(&at[4]).await, Some(fresher));
    assert_eq!(meta.value(&at[0]).await, Some(same), "never re-stamped");
    assert_eq!(meta.value(&at[1]).await, Some(fresh));
    assert_eq!(meta.value(&key).await, None, "the obligation is gone");
}

// ───────────────────────── (E-hold) validated placements ─────────────────────────

/// **(E-hold)** An `Aborting` session's `{session, all}` obligation whose first part's chunk is
/// placed on five servers for a scheme of six fragments.
///
/// Red on a drain that deletes an untrusted `part:` record without marking it.
#[tokio::test]
async fn e_hold_an_untrusted_part_record_is_held_never_deleted_unmarked() {
    let audit = Audit::default();
    let _capture = audit.capture();
    let (meta, fleet) = (Store::new(), Fleet::new());
    let id = upload(0x0e1);
    let nonce = format!("{:032x}", 0x0e1e_u128);
    let untrusted = chunk(0xE01, RS42, vec![0, 1, 2, 3, 4]);
    let trusted = chunk(0xE02, RS42, (0..6).collect());
    for index in 0..6 {
        fleet.place(
            u64::from(index),
            FragmentId {
                chunk: 0xE01,
                index,
            },
        );
    }
    for (dserver, frag) in positions(&trusted) {
        fleet.place(dserver, frag);
    }
    let key = retire_key(RetireMode::Bytes, &session_token(&id, 3));
    let installed = obligation(&key, r#"{"session":true,"parts":"all"}"#);
    let batch = WriteBatch::new()
        .put(
            mp::mpu_key(&id),
            session_bytes(r#"{"kind":"Aborting"}"#, 4, &nonce),
        )
        .put(key.clone(), installed.clone());
    let batch = plant_part(batch, &id, 1, &[untrusted]);
    meta.seed(plant_part(batch, &id, 2, std::slice::from_ref(&trusted)))
        .await;
    let held_part = meta.value(&part_key(&id, part(1))).await.unwrap();
    let custody = Custody::elect().await;

    let mut outcomes = Vec::new();
    for now in [5_000, 6_000] {
        outcomes.push(custody.drain(&meta, &fleet, now).await.unwrap());
    }

    assert_eq!(
        meta.value(&part_key(&id, part(1))).await,
        Some(held_part),
        "never deleted"
    );
    assert!(meta.value(&psum_key(&id, part(1))).await.is_some());
    let found = marks(&meta).await;
    assert!(
        found.is_empty(),
        "nothing was marked from an untrusted placement: {found:?}"
    );
    assert_eq!(
        meta.value(&key).await,
        Some(installed),
        "obligation and cursor in place"
    );
    assert!(meta.take_log().is_empty(), "nothing committed");
    assert_eq!(
        outcomes,
        [Reconciled::Blocked, Reconciled::Blocked],
        "a record it cannot trust blocks the leg, pass after pass"
    );
    let holds = audit.events("hold");
    let part_name = String::from_utf8(part_key(&id, part(1))).unwrap();
    assert!(
        holds
            .iter()
            .any(|e| e["reason"] == "untrusted-record" && e["record"] == part_name),
        "the hold is named on the audit seam: {holds:?}"
    );

    let far = 10 * GRACE;
    custody.gc(&meta, &fleet, far).await.unwrap();
    custody.gc(&meta, &fleet, far + 1).await.unwrap();
    for index in 0..6 {
        let frag = FragmentId {
            chunk: 0xE01,
            index,
        };
        assert!(
            fleet.holds(u64::from(index), frag),
            "GC kept the untrusted chunk's {frag:?}"
        );
    }
    for (dserver, frag) in positions(&trusted) {
        assert!(
            fleet.holds(dserver, frag),
            "the next part's {frag:?} is still staged"
        );
    }
}

// ─────────────────── (G) restore stays accurate between drain batches ───────────────────

/// The parts and segments of the (G) attempt: more segments than one records commit deletes.
const ATTEMPT: u32 = 600;

/// **(G)** A `Completing@3` session whose attempt wrote 600 segments over 600 staged parts is
/// fenced by the post-restore pass (`Aborting@4`, `retire:bytes:{session, all}` and
/// `retire:records:{seg}`, as `restore_completing_fence.rs` seeds them). It is then drained one
/// commit at a time, and after every drain commit the production `reconcile_after_restore` runs.
///
/// Red against independent draining of the two obligations: the bytes drain deletes `part:`
/// records the surviving segments still need, and the next restore run reports `ChunkInNoPart`.
#[tokio::test]
async fn g_restore_never_names_a_session_whose_obligations_are_half_drained() {
    let fleet = Fleet::new();
    let meta = Store::restoring(fleet.clone());
    let id = upload(0x0a7);
    let nonce: String = id.as_str().chars().rev().collect();
    let group = SegmentGroup::new(nonce.clone(), 3).unwrap();
    let state = "{\"kind\":\"Completing\",\"fenced_at_millis\":900,\"segments_written\":600,\
                 \"publish_target\":{\"parent\":42,\"name\":\"o\",\"epoch\":3}}";
    let mut batch = WriteBatch::new().put(mp::mpu_key(&id), session_bytes(state, 3, &nonce));
    for n in 1..=ATTEMPT {
        let held = chunk(0xA000 + ChunkId::from(n), EcScheme::None, vec![1]);
        batch = plant_part(batch, &id, n, std::slice::from_ref(&held));
        let segment = SegmentRecord::new(vec![held], u64::from(n - 1) * 5).unwrap();
        let key = metadata::seg_key(&group, n - 1).unwrap();
        batch = batch.put(key, metadata::encode(&segment));
    }
    meta.seed(batch).await;
    let fenced = restore_pass(&meta.inner, &fleet.disks()).await;
    assert_eq!(fenced.sessions_fenced, 1, "{fenced:?}");
    let bytes_key = retire_key(RetireMode::Bytes, &session_token(&id, 3));
    let records_key = retire_key(RetireMode::Records, &session_token(&id, 3));
    assert_eq!(
        meta.keys(b"retire:").await,
        vec![bytes_key.clone(), records_key.clone()]
    );
    let custody = Custody::elect().await;

    let mut passes = 0;
    while !meta.keys(b"retire:").await.is_empty() {
        passes += 1;
        assert!(passes <= MAX_PASSES, "the obligations never drained");
        custody
            .drain(&meta, &fleet, 5_000)
            .await
            .expect("a drain pass");
    }

    let reports = std::mem::take(&mut *meta.reports.lock().unwrap());
    assert!(
        reports.len() >= 4,
        "the drain took {} commits",
        reports.len()
    );
    let session = String::from_utf8(mp::mpu_key(&id)).unwrap();
    for (step, report) in reports.iter().enumerate() {
        assert!(
            report
                .segments_unaccounted
                .iter()
                .all(|u| u.session != session),
            "after drain commit {step} restore named the session: {report:?}"
        );
        assert!(report.sessions_unsettled.is_empty(), "{report:?}");
    }
    let log = meta.take_log();
    let records_gone = log
        .iter()
        .position(|b| deletes(b, &records_key))
        .expect("the records obligation was deleted");
    let first_part_delete = log
        .iter()
        .position(|b| b.deletes.iter().any(|k| k.starts_with(b"part:")))
        .expect("parts were deleted");
    assert!(
        first_part_delete > records_gone,
        "a part record went (commit {first_part_delete}) before the segments' obligation \
         (commit {records_gone})"
    );
    assert!(
        log[..records_gone]
            .iter()
            .any(|b| b.puts.iter().any(|(k, _)| k.starts_with(b"orphan:"))),
        "the bytes obligation marked while the segments drained"
    );
    assert!(meta.keys(b"seg:").await.is_empty());
    assert!(meta.keys(format!("part:{id}:").as_bytes()).await.is_empty());
}

// ───────────────────────── (F) a segmented generation ─────────────────────────

/// **(F)** A `{generation}` obligation over a **segmented** map names its segments by key, never
/// by frozen placements, so the drain marks each `seg:` record's **current** placement, read at
/// drain time (`0016:2417-2425`): a segment repointed after the obligation was installed is
/// marked where its fragments are now, never where they were. Each record goes after its marks.
#[tokio::test]
async fn f_a_segmented_generation_is_marked_at_each_segments_current_placement() {
    let (meta, fleet) = (Store::new(), Fleet::new());
    let token = RetireToken::Generation {
        inode: 77,
        version: 3,
    };
    let event = token.to_string();
    let group = SegmentGroup::new(format!("{:032x}", 0x5e9_u128), 2).unwrap();
    let key = retire_key(RetireMode::Bytes, &token);
    let installed = obligation(
        &key,
        &format!(
            r#"{{"generation":{{"inode":77,"version":3,"segments":{{"nonce":"{}","epoch":2}}}}}}"#,
            group.nonce()
        ),
    );
    let segment = |index: u32, placement: Vec<DServerId>| {
        let chunks = vec![chunk(0xF00 + ChunkId::from(index), RS42, placement)];
        let record = SegmentRecord::new(chunks, u64::from(index) * 5).unwrap();
        (
            metadata::seg_key(&group, index).unwrap(),
            metadata::encode(&record),
        )
    };
    let mut batch = WriteBatch::new().put(key.clone(), installed);
    for index in 0..3 {
        let (seg, value) = segment(index, (0..6).collect());
        batch = batch.put(seg, value);
    }
    meta.seed(batch).await;
    // A repoint of segment 1 that won before the supersede: its chunk now lives on 6..12.
    let (moved, value) = segment(1, (6..12).collect());
    meta.seed(WriteBatch::new().put(moved.clone(), value)).await;
    let custody = Custody::elect().await;

    let outcome = custody.drain(&meta, &fleet, 5_000).await.unwrap();

    assert_eq!(outcome, Reconciled::Changed);
    assert_eq!(meta.value(&key).await, None, "the obligation drained");
    assert!(
        meta.keys(b"seg:").await.is_empty(),
        "every segment record went"
    );
    let found = marks(&meta).await;
    for index in 0..3_u32 {
        let current: Vec<DServerId> = if index == 1 {
            (6..12).collect()
        } else {
            (0..6).collect()
        };
        let frags = positions(&chunk(0xF00 + ChunkId::from(index), RS42, current));
        for (dserver, frag) in frags {
            let mark = found.get(&(dserver, frag.chunk, frag.index));
            let mark = mark.unwrap_or_else(|| panic!("segment {index}'s {frag:?} on {dserver}"));
            assert_eq!(mark.event(), Some(event.as_str()));
        }
    }
    for index in 0..6_u16 {
        let stale = (u64::from(index), 0xF01, index);
        assert!(
            !found.contains_key(&stale),
            "the repointed segment marked at its old place"
        );
    }
    let log = meta.take_log();
    let deleted = log.iter().position(|b| deletes(b, &moved)).unwrap();
    let marked = log
        .iter()
        .position(|b| {
            b.puts
                .iter()
                .any(|(k, _)| parse_orphan_key(k).is_some_and(|(_, f)| f.chunk == 0xF01))
        })
        .unwrap();
    assert!(marked <= deleted, "segment 1 went before its marks");
}
