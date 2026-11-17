//! Issue #1013 — a multipart session that has left `Open` ends: the custodian walks its owned
//! `sidx:` residue (marks every position, then deletes the entry), retires an `Aborting` session's
//! untrusted records **by chunk id** on passes that see the whole fleet, and then deletes the
//! session in one batch that releases its `mpuctl` slot exactly once (proposal 0016's
//! *Owned-`sidx:` drain step* and *Terminal session delete* rows, `0016:672-674`; decision 5's
//! backstop, `0016:1654-1668`; the terminal-delete rule, `0016:582-602`; the tombstone rule,
//! `0016:574`).
//!
//! Driven **only** through `reconcile_step` with the contexts the deployed loop passes
//! (`crates/server/src/custodian.rs`, `run_reconstruction_until`): a reconstruction pass, every
//! other input `None`, its `now_millis` and its context's clock one source, its fleet the live
//! one, `operator_fleet_size` the configured count and the tombstone input `Retain`; GC a pass of
//! its own over the whole fleet. Every pass builds its leadership fence and context afresh. The
//! metadata is a real backend (`RedbMetadataStore::in_memory()`) behind a recording wrapper; the
//! D servers a placement-aware double whose listings are counted. Sessions are made by the
//! production Create, fenced by the production Abort, published by the production Complete.
//!
//! * **(D)** An `Aborting` session with committed parts, crashed in-flight residue and a slot it
//!   never released drains to: every residue position marked with the walk's `<upload-id>:<epoch>`
//!   event in a batch before the one deleting its entry, no `sidx:` or `slot:` record left, the
//!   `seggrp:` marker gone, the session gone in one committed batch that comes after the last
//!   entry's and the obligation's deletes, and `mpuctl.count` one lower — and no lower after more
//!   passes. Past the grace every fragment is gone. A second `Aborting` session whose nonce a
//!   `seg:` record names keeps its marker.
//! * **(D, `Completed`)** A `Completed` session with in-flight residue, drained with the deployed
//!   contexts: the residue is walked away, its obligations drain, and the session record (the
//!   tombstone) stays with its slot counted; an identical Complete retry answers
//!   `AlreadyCompleted`. A pass whose tombstone input says the window has passed deletes it once.
//! * **(D, the gate)** A `Completed` session past its window whose residue the walk holds (a mark
//!   of none of the three shapes over it) is not deleted while that entry stands; once the mark is
//!   repaired it is. An `Aborting` session whose obligation is held (the undecodable `part:`
//!   control below) is not deleted either.
//! * **(D, race)** A wrapper runs a second drainer to completion between the first's read of the
//!   session and its read of `mpuctl`: the first's terminal batch is refused, and the count goes
//!   down by exactly one.
//! * **(E1, E2, E3)** For an `Aborting` session, on a whole-fleet pass: a `part:` record whose
//!   RS(4,2) chunk is placed on five servers, an owned entry with that placement, and an owned value
//!   that will not decode (an unsupported scheme), its chunk taken from its key. For each, every
//!   fragment bearing the chunk id that the fleet's listings find is marked — for E2 every position
//!   the entry names as well — in a batch before the one deleting the record; past the grace those
//!   fragments are gone, a committed object's fragments on the same servers stay, and the terminal
//!   delete has fired. **Late landing:** a wrapper publishes a fragment right after the record's
//!   deletion, after the listing. For E2, at a position the entry names: it finds the walk's mark
//!   and is gone after the grace. For E3: it stays on disk across GC passes, none failing — the
//!   stated residual.
//! * **(E, controls)** An `Open` session's records of the same kinds, and every fragment of their
//!   chunks, stay. A server left out at startup and a server unreachable this pass each leave the
//!   records and fragments in place with a `partial-fleet` hold; once the fleet is whole they
//!   retire, and that server's fragments are gone after the grace too. A `part:` value that will
//!   not decode is held as `record-unreadable`, audited as needing a human, and the session stays.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::field::{Field, Visit};
use tracing::instrument::WithSubscriber;
use tracing::Dispatch;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self as md, decode_orphan_mark, orphan_key, seg_key, seggrp_key, ChunkRef, EcScheme, InodeId,
    OrphanMark, SegmentGroup, SegmentNonce, SegmentRecord,
};
use wyrd_core::multipart::abort::abort_session;
use wyrd_core::multipart::complete::{
    complete_upload, CompleteRequest, NamedParts, PublishRequest,
};
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::{
    self as mp, decode_admission_record, decode_owned_entry, decode_part_record,
    decode_part_summary, decode_session_record, decode_slot_record, mpu_key, parse_sidx_key,
    part_key, psum_key, retire_key, sidx_key, AbortOutcome, Budget, CompleteOutcome, CreateOutcome,
    Digest, OwnedEntry, PartNumber, PartSummary, RetireMode, RetireToken, StagedPlacement,
    UploadId, MPUCTL_KEY,
};
use wyrd_core::placement::Topology;
use wyrd_custodian::gc::W_WRITE_MILLIS;
use wyrd_custodian::{
    reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone, GcContext, ReconcileError,
    Reconciled, ReconstructionContext, Tombstone,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    Result as StoreResult, WriteBatch,
};

// ─────────────────────────────── fixtures ───────────────────────────────

/// Where every rig's clock starts.
const T0: u64 = 10_000_000;
/// The orphan grace every GC pass here honours.
const GRACE: u64 = 60_000;
/// The late-write deadline GC's sweep of fragment-less marks waits out, stated here.
const LATE_WRITE: u64 = 41_000;
/// The audit target the retirement leg names a hold on.
const AUDIT_TARGET: &str = "wyrd.custodian.retirement.audit";
/// The target it reports a wait on.
const WAIT_TARGET: &str = "wyrd.custodian.retirement";
const BUCKET: &str = "photos";
const ROOT: InodeId = 0;
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
const COMMITTED_AT: u64 = 1_500;
/// Bytes per chunk of a planted part.
const CHUNK_LEN: u64 = 64;
/// Two data fragments and one parity: three positions a chunk.
const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };
/// Four data fragments and two parity: six positions a chunk.
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };

fn upload(n: u64) -> UploadId {
    UploadId::new(format!("{n:032x}")).unwrap()
}

fn nonce(n: u64) -> SegmentNonce {
    SegmentNonce::new(format!("{:032x}", 0x5e60_0000 + n)).unwrap()
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).unwrap()
}

fn name(key: &[u8]) -> String {
    String::from_utf8(key.to_vec()).unwrap()
}

fn digest_of(n: u32) -> Digest {
    let mut bytes = [0x3c; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    Digest::from_bytes(bytes)
}

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

/// A chunk of `scheme`, `CHUNK_LEN` bytes, at `placement`.
fn chunk(id: ChunkId, scheme: EcScheme, placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id,
        scheme,
        len: CHUNK_LEN,
        placement,
    }
}

/// Every fragment position `chunks` names, in order.
fn positions(chunks: &[ChunkRef]) -> Vec<(DServerId, FragmentId)> {
    chunks
        .iter()
        .flat_map(|c| {
            c.fragments()
                .map(|(index, dserver)| (dserver, frag(c.id, index)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Fragment `i` of `chunk` on server `i`, for every `i` of `servers`.
fn diagonal(chunk: ChunkId, servers: &[u16]) -> Vec<(DServerId, FragmentId)> {
    servers
        .iter()
        .map(|&i| (DServerId::from(i), frag(chunk, i)))
        .collect()
}

/// The `part:` value of a part over `chunks` with `digest`, in the codec's own spelling — not
/// checked here, so a value the decoder refuses can be planted too.
fn raw_part_value(chunks: &[ChunkRef], digest: Digest) -> Bytes {
    let len: u64 = chunks.iter().map(|c| c.len).sum();
    Bytes::from(format!(
        "{{\"chunks\":{},\"len\":{len},\"digest\":\"{digest}\",\
         \"committed_at_millis\":{COMMITTED_AT},\"session_epoch\":0}}",
        String::from_utf8(md::encode(&chunks).to_vec()).unwrap()
    ))
}

/// The puts planting part `n` of `id` over `chunks`: its `part:` record — checked to decode —
/// and its `psum:` summary.
fn plant_part(id: &UploadId, n: u32, chunks: &[ChunkRef]) -> Vec<(Vec<u8>, Bytes)> {
    let digest = digest_of(n);
    let value = raw_part_value(chunks, digest);
    let decoded = decode_part_record(&value).expect("a canonical part");
    assert_eq!(md::encode(&decoded), value);
    let summary = PartSummary {
        chunks: chunks.len() as u32,
        len: chunks.iter().map(|c| c.len).sum(),
        digest,
        committed_at_millis: COMMITTED_AT,
    };
    let psum = md::encode(&summary);
    assert_eq!(decode_part_summary(&psum).unwrap(), summary);
    vec![
        (part_key(id, part(n)), value),
        (psum_key(id, part(n)), psum),
    ]
}

/// The owned entry of `chunk`, staged by part `n` of `id` at `placement` under `scheme`: its key
/// and its value, checked to decode.
fn owned(
    id: &UploadId,
    n: u32,
    chunk: ChunkId,
    scheme: EcScheme,
    placement: Vec<DServerId>,
) -> (Vec<u8>, Bytes) {
    let staged = StagedPlacement::new(scheme, placement).unwrap();
    let entry = OwnedEntry::new(id.clone(), T0 + 10_000, staged).to_pending();
    let key = sidx_key(id, part(n), chunk);
    let value = md::encode(&entry);
    decode_owned_entry(&key, &value).expect("an owned entry");
    (key, value)
}

/// The slot record a crashed attempt of part `n` left at index 0 of `id`, checked to decode.
fn slot(id: &UploadId, n: u32) -> (Vec<u8>, Bytes) {
    let value = format!(
        "{{\"part_number\":{n},\"attempt_id\":\"{:032x}\",\"reserved_at_millis\":{T0},\
         \"lease_expiry_millis\":{}}}",
        0xa77e_0000_u64 + u64::from(n),
        T0 + 10_000
    );
    decode_slot_record(value.as_bytes()).expect("a slot record");
    let key = format!("slot:{id}:000000").into_bytes();
    (key, Bytes::from(value))
}

// ─────────────────────────── the recording store ───────────────────────────

/// A second drainer, run to completion over the inner store the first time the wrapped store is
/// asked for `mpuctl`.
struct Rival {
    fleet: Fleet,
    clock: ManualClock,
    ran: Arc<AtomicBool>,
}

/// A fragment published right after the batch deleting `deleted` commits.
struct Late {
    deleted: Vec<u8>,
    fleet: Fleet,
    at: (DServerId, FragmentId),
    /// The fleet's listing count when it was published.
    listings_before: Arc<AtomicU64>,
}

/// `RedbMetadataStore` behind a wrapper that records every batch sent, with what the store
/// answered, and can run one [`Rival`] and one [`Late`].
struct Store {
    inner: RedbMetadataStore,
    sent: Mutex<Vec<(WriteBatch, Option<CommitOutcome>)>>,
    rival: Mutex<Option<Rival>>,
    late: Mutex<Option<Late>>,
}

impl Store {
    fn new() -> Self {
        Self {
            inner: RedbMetadataStore::in_memory().unwrap(),
            sent: Mutex::new(Vec::new()),
            rival: Mutex::new(None),
            late: Mutex::new(None),
        }
    }

    /// Fixture writes, straight to the inner store.
    async fn plant(&self, puts: Vec<(Vec<u8>, Bytes)>) {
        let mut batch = WriteBatch::new();
        batch.puts.extend(puts);
        assert_eq!(
            self.inner.commit(batch).await.unwrap(),
            CommitOutcome::Committed
        );
    }

    /// A fixture delete, straight to the inner store.
    async fn remove(&self, key: &[u8]) {
        let batch = WriteBatch::new().delete(key.to_vec());
        assert_eq!(
            self.inner.commit(batch).await.unwrap(),
            CommitOutcome::Committed
        );
    }

    async fn value(&self, key: &[u8]) -> Option<Bytes> {
        self.inner.get(key).await.unwrap()
    }

    async fn under(&self, prefix: &[u8]) -> BTreeMap<Vec<u8>, Bytes> {
        let (page, next) = self.inner.scan_page(prefix, None, 10_000).await.unwrap();
        assert!(next.is_none(), "a fixture range fits one page");
        page.into_iter().collect()
    }

    /// Every batch sent since the last call, with what the store answered (`None`: an error).
    fn take_sent(&self) -> Vec<(WriteBatch, Option<CommitOutcome>)> {
        std::mem::take(&mut *self.sent.lock().unwrap())
    }
}

#[async_trait]
impl MetadataStore for Store {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        if key == MPUCTL_KEY {
            let rival = self.rival.lock().unwrap().take();
            if let Some(rival) = rival {
                let view = View::deployed();
                let outcome = pass(&self.inner, &rival.fleet, &rival.clock, &view, None).await;
                assert!(outcome.is_ok(), "the rival's pass: {outcome:?}");
                rival.ran.store(true, Ordering::SeqCst);
            }
        }
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
    ) -> StoreResult<wyrd_traits::ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        let outcome = self.inner.commit(batch.clone()).await;
        let answered = outcome.as_ref().ok().copied();
        let deletes = batch.deletes.clone();
        self.sent.lock().unwrap().push((batch, answered));
        if answered == Some(CommitOutcome::Committed) {
            let mut late = self.late.lock().unwrap();
            if late.as_ref().is_some_and(|l| deletes.contains(&l.deleted)) {
                let late = late.take().unwrap();
                late.listings_before
                    .store(late.fleet.listings(), Ordering::SeqCst);
                let (dserver, at) = late.at;
                late.fleet.put(dserver, at, Bytes::from_static(b"late"));
            }
        }
        outcome
    }
}

// ─────────────────────────────── the D servers ───────────────────────────────

/// In-process D servers addressed by id, and how many listings were taken of them.
#[derive(Clone, Default)]
struct Fleet {
    servers: Arc<Mutex<BTreeMap<DServerId, HashMap<FragmentId, Bytes>>>>,
    listings: Arc<AtomicU64>,
}

impl Fleet {
    fn serving(ids: impl IntoIterator<Item = DServerId>) -> Self {
        let servers = ids.into_iter().map(|id| (id, HashMap::new())).collect();
        Self {
            servers: Arc::new(Mutex::new(servers)),
            listings: Arc::default(),
        }
    }

    fn put(&self, dserver: DServerId, frag: FragmentId, bytes: Bytes) {
        let mut servers = self.servers.lock().unwrap();
        servers
            .get_mut(&dserver)
            .expect("a server")
            .insert(frag, bytes);
    }

    fn holds(&self, dserver: DServerId, frag: FragmentId) -> bool {
        let servers = self.servers.lock().unwrap();
        servers
            .get(&dserver)
            .is_some_and(|held| held.contains_key(&frag))
    }

    fn store_at(&self, at: &[(DServerId, FragmentId)]) {
        for &(dserver, frag) in at {
            self.put(dserver, frag, Bytes::from(format!("{frag:?}")));
        }
    }

    fn ids(&self) -> Vec<DServerId> {
        self.servers.lock().unwrap().keys().copied().collect()
    }

    fn listings(&self) -> u64 {
        self.listings.load(Ordering::SeqCst)
    }
}

/// One D server of a [`Fleet`], as a pass's fleet lists it.
struct Server {
    id: DServerId,
    fleet: Fleet,
}

#[async_trait]
impl ChunkStore for Server {
    async fn put_fragment(&self, id: FragmentId, bytes: Bytes, _: Option<u64>) -> StoreResult<()> {
        self.fleet.put(self.id, id, bytes);
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> StoreResult<Option<Bytes>> {
        let servers = self.fleet.servers.lock().unwrap();
        Ok(servers
            .get(&self.id)
            .and_then(|held| held.get(&id).cloned()))
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        self.fleet.listings.fetch_add(1, Ordering::SeqCst);
        let servers = self.fleet.servers.lock().unwrap();
        Ok(servers
            .get(&self.id)
            .map(|held| held.keys().copied().collect())
            .unwrap_or_default())
    }

    async fn delete_fragment(&self, id: FragmentId) -> StoreResult<()> {
        let mut servers = self.fleet.servers.lock().unwrap();
        if let Some(held) = servers.get_mut(&self.id) {
            held.remove(&id);
        }
        Ok(())
    }

    async fn health(&self) -> StoreResult<Health> {
        Ok(Health::Healthy)
    }
}

// ─────────────────────────────── the audit seam ───────────────────────────────

/// The retirement leg's audit events and waits, captured by a subscriber attached to each pass's
/// future.
#[derive(Clone, Default)]
struct Audit {
    events: Arc<Mutex<Vec<HashMap<String, String>>>>,
    waits: Arc<Mutex<Vec<(String, String)>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Audit {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = FieldText(HashMap::new());
        event.record(&mut fields);
        let target = event.metadata().target();
        if target == AUDIT_TARGET {
            self.events.lock().unwrap().push(fields.0);
        } else if target == WAIT_TARGET {
            if let (Some(key), Some(wait)) = (fields.0.get("key"), fields.0.get("wait")) {
                self.waits.lock().unwrap().push((key.clone(), wait.clone()));
            }
        }
    }
}

struct FieldText(HashMap<String, String>);

impl Visit for FieldText {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

impl Audit {
    fn dispatch(&self) -> Dispatch {
        Dispatch::new(tracing_subscriber::registry().with(self.clone()))
    }

    /// The audit events of `action` naming `key`; empties the capture.
    fn take(&self, action: &str, key: &[u8]) -> Vec<HashMap<String, String>> {
        let key = name(key);
        let mut events = self.events.lock().unwrap();
        let named = events
            .iter()
            .filter(|e| e.get("action").map(String::as_str) == Some(action) && e["key"] == key)
            .cloned()
            .collect();
        events.clear();
        named
    }

    /// The holds naming the obligation at `key`; empties the capture.
    fn obligation_holds(&self, key: &[u8]) -> Vec<HashMap<String, String>> {
        self.take("retire-obligation-held", key)
    }

    /// The holds naming the session at `key`; empties the capture.
    fn session_holds(&self, key: &[u8]) -> Vec<HashMap<String, String>> {
        self.take("retire-session-held", key)
    }

    fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }

    fn clear(&self) {
        self.events.lock().unwrap().clear();
        self.waits.lock().unwrap().clear();
    }
}

// ─────────────────────────────── the passes ───────────────────────────────

/// What one reconstruction pass sees: which configured servers are missing from its fleet, which
/// of those it dropped as unreachable, and the tombstone input.
struct View {
    missing: Vec<DServerId>,
    unreachable: Vec<DServerId>,
    tombstone: Tombstone,
}

impl View {
    /// What the deployed loop passes with every configured server reachable: the whole fleet,
    /// nothing unreachable, the tombstone retained (#625 not landed).
    fn deployed() -> Self {
        Self {
            missing: Vec::new(),
            unreachable: Vec::new(),
            tombstone: Tombstone::Retain,
        }
    }

    /// The deployed view, with the tombstone window passed.
    fn window_passed() -> Self {
        Self {
            tombstone: Tombstone::Elapsed,
            ..Self::deployed()
        }
    }

    /// A pass whose fleet lacks `missing`: left out at startup (never connected, so not
    /// unreachable either) or, when `dropped`, unreachable this pass.
    fn without(missing: DServerId, dropped: bool) -> Self {
        Self {
            missing: vec![missing],
            unreachable: if dropped { vec![missing] } else { Vec::new() },
            tombstone: Tombstone::Retain,
        }
    }
}

/// One fenced reconstruction pass as the deployed loop runs it, over `meta`: a fresh leadership
/// term and context, its fleet the configured servers `view` presents, `operator_fleet_size` the
/// configured count, `now_millis` and the context's clock read from the one clock.
async fn pass(
    meta: &dyn MetadataStore,
    fleet: &Fleet,
    clock: &ManualClock,
    view: &View,
    audit: Option<&Audit>,
) -> Result<Reconciled, ReconcileError> {
    let (zone, custodian) = elect().await;
    let configured = fleet.ids();
    let servers: Vec<Server> = configured
        .iter()
        .copied()
        .filter(|id| !view.missing.contains(id))
        .map(|id| Server {
            id,
            fleet: fleet.clone(),
        })
        .collect();
    let live: Vec<(DServerId, &dyn ChunkStore)> = servers
        .iter()
        .map(|s| (s.id, s as &dyn ChunkStore))
        .collect();
    let topology = Topology::default();
    let ctx = ReconstructionContext {
        meta,
        fleet: &live,
        topology: &topology,
        unreachable: &view.unreachable,
        clock,
        staged_write_window_millis: W_WRITE_MILLIS,
        operator_fleet_size: configured.len(),
        tombstone: view.tombstone,
    };
    let now = clock.now_millis();
    let step = reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, now);
    match audit {
        Some(audit) => step.with_subscriber(audit.dispatch()).await,
        None => step.await,
    }
}

async fn elect() -> (FencedZone, Custodian) {
    let coord = MemCoordination::new();
    let custodian = Custodian::elect(&coord, "zone-session-terminal")
        .await
        .expect("leader election over the in-memory coordination seam");
    let mut zone = FencedZone::new();
    zone.install(custodian.leadership());
    (zone, custodian)
}

// ─────────────────────────────── the rig ───────────────────────────────

/// One store, one fleet, one clock — the deployed loop's one time source — and the audit seam.
struct Rig {
    store: Store,
    fleet: Fleet,
    clock: ManualClock,
    audit: Audit,
}

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

/// A profile admitting eight sessions; Complete reads no other cap of it.
fn profile() -> Budget {
    let json = "{\"w_ref\":16,\"max_part_chunks\":1,\"max_parts_per_session\":1,\
                \"max_inflight_parts\":1,\"max_staged_chunks\":1}";
    md::decode(json.as_bytes()).expect("a legal profile")
}

impl Rig {
    /// A rig over D servers `0..servers`, the bucket in place.
    async fn new(servers: u64) -> Self {
        let rig = Self {
            store: Store::new(),
            fleet: Fleet::serving(0..servers),
            clock: ManualClock::new(T0),
            audit: Audit::default(),
        };
        let bucket = vec![(md::bucket_key(BUCKET), Bytes::from_static(MARKER))];
        rig.store.plant(bucket).await;
        rig
    }

    /// One reconstruction pass with the contexts the deployed loop passes.
    async fn drain(&self) -> Result<Reconciled, ReconcileError> {
        self.drain_seeing(&View::deployed()).await
    }

    async fn drain_seeing(&self, view: &View) -> Result<Reconciled, ReconcileError> {
        pass(
            &self.store,
            &self.fleet,
            &self.clock,
            view,
            Some(&self.audit),
        )
        .await
    }

    /// Deployed passes until the session `id` is gone, at most `max` of them; none fails or holds.
    async fn drain_until_ended(&self, id: &UploadId, max: usize) {
        let mut passes = 0;
        while self.store.value(&mpu_key(id)).await.is_some() {
            assert!(passes < max, "{id} ends within {max} passes");
            let outcome = self.drain().await.expect("the pass never fails");
            assert_ne!(outcome, Reconciled::Blocked, "pass {passes}");
            passes += 1;
        }
        assert!(self.audit.is_empty(), "nothing held");
    }

    /// One fenced GC pass over the whole fleet, as the deployed loop runs it.
    async fn gc(&self) -> Reconciled {
        let (zone, custodian) = elect().await;
        let servers: Vec<Server> = self
            .fleet
            .ids()
            .into_iter()
            .map(|id| Server {
                id,
                fleet: self.fleet.clone(),
            })
            .collect();
        let fleet: Vec<(DServerId, &dyn ChunkStore)> = servers
            .iter()
            .map(|s| (s.id, s as &dyn ChunkStore))
            .collect();
        let ctx = GcContext {
            meta: &self.store,
            fleet: &fleet,
            grace_window_millis: GRACE,
            expired_pending: ExpiredPendingPolicy::Defer,
        };
        let now = self.clock.now_millis();
        reconcile_step(&zone, &custodian, Some(&ctx), None, None, None, now)
            .await
            .expect("the GC pass never fails")
    }

    /// Move past the grace of every mark written so far, and run GC passes until one reclaims
    /// nothing more.
    async fn reclaim_past_grace(&self) {
        self.clock.advance(GRACE + 1);
        for _ in 0..4 {
            if self.gc().await != Reconciled::Changed {
                return;
            }
        }
    }

    /// A session made by Create, numbered `n`, targeting `object`.
    async fn session(&self, n: u64, object: &str) -> UploadId {
        let request = CreateRequest {
            bucket: BUCKET,
            parent: ROOT,
            object,
            content_type: None,
            created_at_millis: 1_000,
            clock_source: "wall",
        };
        let created = create_session(
            &self.store.inner,
            &profile(),
            &request,
            &mut Fixed(Some((upload(n), nonce(n)))),
        )
        .await
        .unwrap();
        let CreateOutcome::Created { upload_id } = created else {
            panic!("expected Created, got {created:?}");
        };
        upload_id
    }

    /// The client Abort of `id`; it must fence.
    async fn abort(&self, id: &UploadId) {
        let aborted = abort_session(&self.store.inner, id).await;
        assert!(matches!(aborted, Ok(AbortOutcome::Fenced)), "{aborted:?}");
    }

    /// Plant `parts` of `id` (each a part number and its chunks), their fragments on the fleet.
    async fn plant_parts(&self, id: &UploadId, parts: &[(u32, Vec<ChunkRef>)]) {
        let mut puts = Vec::new();
        for (n, chunks) in parts {
            puts.extend(plant_part(id, *n, chunks));
            self.fleet.store_at(&positions(chunks));
        }
        self.store.plant(puts).await;
    }

    /// The production Complete of `id` at `object`, naming `named`, publishing under `inode`.
    async fn complete(
        &self,
        id: &UploadId,
        object: &str,
        named: &[u32],
        inode: InodeId,
    ) -> CompleteOutcome {
        let list = NamedParts::new(named.iter().map(|&n| (part(n), digest_of(n))).collect());
        let request = CompleteRequest {
            bucket: BUCKET,
            parent: ROOT,
            object,
            parts: &list.unwrap(),
            fenced_at_millis: 2_000,
            clock_source: "wall",
        };
        let publish = PublishRequest {
            fresh_inode: inode,
            completed_at_millis: 3_000,
        };
        complete_upload(&self.store.inner, id, &request, &publish)
            .await
            .expect("the Complete answers")
    }

    /// A committed object published by session `n` at `inode`: one RS(2,1) chunk on servers 0–2
    /// with its fragments. Its positions.
    async fn committed_object(&self, n: u64, inode: InodeId) -> Vec<(DServerId, FragmentId)> {
        let object = format!("{BUCKET}/committed-{n}");
        let id = self.session(n, &object).await;
        let chunks = vec![chunk(ChunkId::from(n) * 1_000, RS21, vec![0, 1, 2])];
        self.plant_parts(&id, &[(1, chunks.clone())]).await;
        let outcome = self.complete(&id, &object, &[1], inode).await;
        assert!(
            matches!(outcome, CompleteOutcome::Published(_)),
            "{outcome:?}"
        );
        positions(&chunks)
    }

    /// The `mpuctl` count.
    async fn sessions_counted(&self) -> u64 {
        let ledger = self.store.value(MPUCTL_KEY).await.expect("the ledger");
        decode_admission_record(&ledger).unwrap().count()
    }

    /// The epoch of `id`'s session record.
    async fn epoch_of(&self, id: &UploadId) -> u64 {
        let record = self.store.value(&mpu_key(id)).await.expect("the session");
        decode_session_record(&record).unwrap().epoch()
    }

    /// The decoded `orphan:` mark at `position`, if any.
    async fn mark_at(&self, position: (DServerId, FragmentId)) -> Option<OrphanMark> {
        let stored = self
            .store
            .value(&orphan_key(position.0, position.1))
            .await?;
        Some(decode_orphan_mark(&stored).expect("a mark of one of the three shapes"))
    }

    /// Every position of `positions` carries a structured mark of `event`.
    async fn assert_marked(&self, positions: &[(DServerId, FragmentId)], event: &str) {
        for &position in positions {
            let mark = self.mark_at(position).await;
            let mark = mark.unwrap_or_else(|| panic!("{position:?} is unmarked"));
            assert_eq!(mark.event(), Some(event), "{position:?}");
        }
    }

    /// No position bearing `chunk` carries a mark.
    async fn assert_unmarked(&self, chunk: ChunkId) {
        for key in self.store.under(b"orphan:").await.keys() {
            let (_, frag) = md::parse_orphan_key(key).expect("a mark key");
            assert_ne!(frag.chunk, chunk, "{} marks the held chunk", name(key));
        }
    }

    fn assert_gone(&self, positions: &[(DServerId, FragmentId)]) {
        for &(dserver, frag) in positions {
            assert!(
                !self.fleet.holds(dserver, frag),
                "{frag:?} on {dserver} survived"
            );
        }
    }

    fn assert_kept(&self, positions: &[(DServerId, FragmentId)]) {
        for &(dserver, frag) in positions {
            assert!(
                self.fleet.holds(dserver, frag),
                "{frag:?} on {dserver} was reclaimed"
            );
        }
    }
}

/// The bytes-mode key of the suffix-free session token of `id` at `epoch`, and its event.
fn teardown(id: &UploadId, epoch: u64) -> (Vec<u8>, String) {
    let token = RetireToken::Session {
        upload_id: id.clone(),
        epoch,
        part: None,
    };
    (retire_key(RetireMode::Bytes, &token), token.to_string())
}

/// The index in `sent` of the first **committed** batch that puts `key`, and of the first that
/// deletes it.
fn put_at(sent: &[(WriteBatch, Option<CommitOutcome>)], key: &[u8]) -> Option<usize> {
    sent.iter().position(|(batch, outcome)| {
        *outcome == Some(CommitOutcome::Committed) && batch.puts.iter().any(|(k, _)| k == key)
    })
}

fn deleted_at(sent: &[(WriteBatch, Option<CommitOutcome>)], key: &[u8]) -> Option<usize> {
    sent.iter().position(|(batch, outcome)| {
        *outcome == Some(CommitOutcome::Committed) && batch.deletes.iter().any(|k| k == key)
    })
}

/// How many committed batches of `sent` delete `key`.
fn committed_deletes(sent: &[(WriteBatch, Option<CommitOutcome>)], key: &[u8]) -> usize {
    sent.iter()
        .filter(|(batch, outcome)| {
            *outcome == Some(CommitOutcome::Committed) && batch.deletes.iter().any(|k| k == key)
        })
        .count()
}

/// Every position of `positions` was marked by a committed batch strictly before the committed
/// batch deleting `record`.
fn assert_marked_before_deleted(
    sent: &[(WriteBatch, Option<CommitOutcome>)],
    positions: &[(DServerId, FragmentId)],
    record: &[u8],
) {
    let deleted = deleted_at(sent, record).unwrap_or_else(|| panic!("{} deleted", name(record)));
    for &(dserver, frag) in positions {
        let key = orphan_key(dserver, frag);
        let marked = put_at(sent, &key).unwrap_or_else(|| panic!("{} marked", name(&key)));
        assert!(
            marked < deleted,
            "{} marked in batch {marked}, {} deleted in batch {deleted}",
            name(&key),
            name(record)
        );
    }
}

// ─────────────────────────────── (D) ───────────────────────────────

/// **(D)** An `Aborting` session — two committed parts, the residue of a crashed third part (two
/// owned entries, one of whose fragments all landed and one of which landed only its first), and
/// the slot that attempt never released — drains through deployed passes to its end: every residue
/// position, landed or not, marked from its entry's placement with the walk's event in a batch
/// before the one deleting that entry; no `sidx:` or `slot:` record left; the `seggrp:` marker
/// gone; the session deleted by one committed batch, after the last entry's delete and the
/// obligation's; `mpuctl.count` one lower, and no lower after more passes. Past the grace every
/// fragment of the session is gone.
#[tokio::test]
async fn d_an_aborted_session_walks_its_residue_and_ends_once() {
    let rig = Rig::new(3).await;
    let id = rig.session(1, &format!("{BUCKET}/d")).await;
    let parts: Vec<(u32, Vec<ChunkRef>)> = vec![
        (1, vec![chunk(1_100, RS21, vec![0, 1, 2])]),
        (2, vec![chunk(1_200, RS21, vec![0, 1, 2])]),
    ];
    rig.plant_parts(&id, &parts).await;
    let whole = owned(&id, 3, 1_300, RS21, vec![0, 1, 2]);
    let torn = owned(&id, 3, 1_301, RS21, vec![0, 1, 2]);
    let whole_at = diagonal(1_300, &[0, 1, 2]);
    let torn_at = diagonal(1_301, &[0, 1, 2]);
    rig.fleet.store_at(&whole_at);
    rig.fleet.store_at(&torn_at[..1]);
    rig.store
        .plant(vec![whole.clone(), torn.clone(), slot(&id, 3)])
        .await;
    rig.abort(&id).await;
    let epoch = rig.epoch_of(&id).await;
    let event = format!("{id}:{epoch}");
    let (obligation, _) = teardown(&id, 0);
    assert!(rig.store.value(&obligation).await.is_some());
    let counted = rig.sessions_counted().await;
    rig.store.take_sent();

    rig.drain_until_ended(&id, 6).await;

    let sent = rig.store.take_sent();
    for ((key, _), at) in [(&whole, &whole_at), (&torn, &torn_at)] {
        rig.assert_marked(at, &event).await;
        assert_marked_before_deleted(&sent, at, key);
    }
    let session = mpu_key(&id);
    assert_eq!(committed_deletes(&sent, &session), 1, "one terminal delete");
    let ended = deleted_at(&sent, &session).unwrap();
    for key in [&whole.0, &torn.0, &obligation] {
        assert!(deleted_at(&sent, key).unwrap() < ended, "{}", name(key));
    }
    assert!(rig.store.under(&mp::sidx_range(&id)).await.is_empty());
    assert!(rig.store.under(&mp::slot_range(&id)).await.is_empty());
    assert!(rig.store.under(&mp::part_range(&id)).await.is_empty());
    assert_eq!(rig.store.value(&seggrp_key(&nonce(1))).await, None);
    assert_eq!(rig.sessions_counted().await, counted - 1);

    for _ in 0..2 {
        rig.drain().await.unwrap();
    }
    assert_eq!(rig.sessions_counted().await, counted - 1, "released once");

    rig.reclaim_past_grace().await;
    rig.assert_gone(&whole_at);
    rig.assert_gone(&torn_at[..1]);
    for (_, chunks) in &parts {
        rig.assert_gone(&positions(chunks));
    }
}

/// **(D, the marker)** An `Aborting` session whose segment-group nonce a `seg:` record names — a
/// group adopted elsewhere — ends with its marker kept, and the record with it; the count still
/// goes down by one.
#[tokio::test]
async fn d_an_adopted_groups_marker_outlives_the_session() {
    let rig = Rig::new(3).await;
    let id = rig.session(2, &format!("{BUCKET}/d-adopted")).await;
    let group = SegmentGroup::from_nonce(nonce(2), 1);
    let record = SegmentRecord::new(vec![chunk(2_000, RS21, vec![0, 1, 2])], 0).unwrap();
    let seg = seg_key(&group, 0).unwrap();
    rig.store
        .plant(vec![(seg.clone(), md::encode(&record))])
        .await;
    rig.abort(&id).await;
    let counted = rig.sessions_counted().await;

    rig.drain_until_ended(&id, 4).await;

    assert!(
        rig.store.value(&seggrp_key(&nonce(2))).await.is_some(),
        "kept"
    );
    assert!(rig.store.value(&seg).await.is_some());
    assert_eq!(rig.sessions_counted().await, counted - 1);
}

/// **(D, `Completed`)** A session with two parts and the residue of a crashed third (one owned
/// entry, its fragments landed) is completed naming both parts. Deployed passes walk the residue
/// away and drain the Complete's obligation, and the session record — the tombstone — stays with
/// its slot counted; an identical Complete retry answers `AlreadyCompleted`. A pass whose tombstone
/// input says the window has passed deletes it, once: the count goes down by one, the marker of the
/// unadopted group goes, and more such passes change nothing. Past the grace the residue's
/// fragments are gone and the published ones stay.
#[tokio::test]
async fn d_a_completed_session_keeps_its_tombstone_until_the_window_passes() {
    let rig = Rig::new(3).await;
    let object = format!("{BUCKET}/d-completed");
    let id = rig.session(3, &object).await;
    let parts: Vec<(u32, Vec<ChunkRef>)> = vec![
        (1, vec![chunk(3_100, RS21, vec![0, 1, 2])]),
        (2, vec![chunk(3_200, RS21, vec![0, 1, 2])]),
    ];
    rig.plant_parts(&id, &parts).await;
    let residue = owned(&id, 3, 3_300, RS21, vec![0, 1, 2]);
    let residue_at = diagonal(3_300, &[0, 1, 2]);
    rig.fleet.store_at(&residue_at);
    rig.store.plant(vec![residue.clone()]).await;
    let first = rig.complete(&id, &object, &[1, 2], 42).await;
    let CompleteOutcome::Published(publication) = first else {
        panic!("{first:?}");
    };
    let epoch = rig.epoch_of(&id).await;
    let counted = rig.sessions_counted().await;
    rig.store.take_sent();

    for round in 0..4 {
        let outcome = rig.drain().await.expect("no pass fails");
        assert_ne!(outcome, Reconciled::Blocked, "round {round}");
    }

    let sent = rig.store.take_sent();
    rig.assert_marked(&residue_at, &format!("{id}:{epoch}"))
        .await;
    assert_marked_before_deleted(&sent, &residue_at, &residue.0);
    assert!(rig.store.under(&mp::sidx_range(&id)).await.is_empty());
    assert!(rig
        .store
        .under(&mp::retire_session_range(RetireMode::Records, &id))
        .await
        .is_empty());
    assert!(rig
        .store
        .under(&mp::retire_session_range(RetireMode::Bytes, &id))
        .await
        .is_empty());
    assert!(
        rig.store.value(&mpu_key(&id)).await.is_some(),
        "the tombstone stays"
    );
    assert_eq!(committed_deletes(&sent, &mpu_key(&id)), 0);
    assert_eq!(
        rig.sessions_counted().await,
        counted,
        "its slot still counted"
    );
    let retry = rig.complete(&id, &object, &[1, 2], 43).await;
    assert_eq!(retry, CompleteOutcome::AlreadyCompleted(publication));

    let outcome = rig.drain_seeing(&View::window_passed()).await.unwrap();
    assert_eq!(outcome, Reconciled::Changed);
    assert_eq!(rig.store.value(&mpu_key(&id)).await, None);
    assert_eq!(rig.sessions_counted().await, counted - 1);
    assert_eq!(rig.store.value(&seggrp_key(&nonce(3))).await, None);
    for _ in 0..2 {
        rig.drain_seeing(&View::window_passed()).await.unwrap();
    }
    assert_eq!(rig.sessions_counted().await, counted - 1, "released once");
    assert_eq!(committed_deletes(&rig.store.take_sent(), &mpu_key(&id)), 1);

    rig.reclaim_past_grace().await;
    rig.assert_gone(&residue_at);
    for (_, chunks) in &parts {
        rig.assert_kept(&positions(chunks));
    }
}

/// **(D, the gate)** A `Completed` session past its tombstone window, every obligation of it
/// drained, owns one residue entry whose first position carries a mark of none of the three
/// shapes: the walk holds it, so its `sidx:` range is never observed empty and the session is not
/// deleted, pass after pass, its slot still counted. Once the mark is repaired the walk empties the
/// range and the session is deleted, once.
#[tokio::test]
async fn d_the_terminal_delete_waits_for_an_empty_owned_range() {
    let rig = Rig::new(3).await;
    let object = format!("{BUCKET}/d-gate");
    let id = rig.session(4, &object).await;
    rig.plant_parts(&id, &[(1, vec![chunk(4_100, RS21, vec![0, 1, 2])])])
        .await;
    let residue = owned(&id, 2, 4_200, RS21, vec![0, 1, 2]);
    let damaged = orphan_key(0, frag(4_200, 0));
    rig.store
        .plant(vec![
            residue.clone(),
            (damaged.clone(), Bytes::from_static(b"not-a-mark")),
        ])
        .await;
    let published = rig.complete(&id, &object, &[1], 44).await;
    assert!(matches!(published, CompleteOutcome::Published(_)));
    let counted = rig.sessions_counted().await;

    for round in 0..3 {
        let outcome = rig.drain_seeing(&View::window_passed()).await.unwrap();
        assert_eq!(outcome, Reconciled::Blocked, "round {round}");
        let holds = rig.audit.session_holds(&mpu_key(&id));
        assert_eq!(holds.len(), 1, "round {round}: {holds:?}");
        assert_eq!(holds[0]["hold"], "unreadable-mark");
        assert_eq!(holds[0]["record"], name(&residue.0));
        assert!(
            rig.store.value(&mpu_key(&id)).await.is_some(),
            "round {round}"
        );
        assert_eq!(rig.store.value(&residue.0).await, Some(residue.1.clone()));
        assert_eq!(rig.sessions_counted().await, counted, "round {round}");
    }
    assert!(
        rig.store
            .under(&mp::retire_session_range(RetireMode::Records, &id))
            .await
            .is_empty(),
        "no obligation is what held it"
    );

    rig.store.remove(&damaged).await;
    rig.store.take_sent();
    for _ in 0..3 {
        rig.drain_seeing(&View::window_passed()).await.unwrap();
    }
    assert_eq!(rig.store.value(&mpu_key(&id)).await, None);
    assert_eq!(rig.sessions_counted().await, counted - 1);
    assert_eq!(committed_deletes(&rig.store.take_sent(), &mpu_key(&id)), 1);
}

/// An `Aborting` session with nothing left but its record — beside an `Open` one, when `beside`,
/// so the ledger counts two. A wrapper runs a second drainer's whole pass between the first
/// drainer's read of the session and its read of `mpuctl`: the second deletes the session and
/// lowers the count. The count goes down by exactly one and stays there, and the first holds
/// nothing. Its sent batches deleting the session record are returned.
async fn race(n: u64, beside: bool) -> Vec<(WriteBatch, Option<CommitOutcome>)> {
    let rig = Rig::new(3).await;
    if beside {
        rig.session(n + 100, &format!("{BUCKET}/d-race-open")).await;
    }
    let id = rig.session(n, &format!("{BUCKET}/d-race")).await;
    rig.abort(&id).await;
    let (obligation, _) = teardown(&id, 0);
    rig.drain().await.unwrap();
    assert_eq!(rig.store.value(&obligation).await, None, "drained first");
    assert!(rig.store.value(&mpu_key(&id)).await.is_some());
    let counted = rig.sessions_counted().await;
    let ran = Arc::new(AtomicBool::new(false));
    *rig.store.rival.lock().unwrap() = Some(Rival {
        fleet: rig.fleet.clone(),
        clock: rig.clock.clone(),
        ran: Arc::clone(&ran),
    });
    rig.store.take_sent();

    let outcome = rig.drain().await.expect("the pass never fails");

    assert!(ran.load(Ordering::SeqCst), "the rival ran");
    assert_ne!(
        outcome,
        Reconciled::Blocked,
        "the first drainer holds nothing"
    );
    assert!(rig.audit.is_empty(), "nothing audited");
    assert_eq!(rig.store.value(&mpu_key(&id)).await, None);
    assert_eq!(rig.sessions_counted().await, counted - 1, "released once");
    let sent: Vec<_> = rig
        .store
        .take_sent()
        .into_iter()
        .filter(|(batch, _)| batch.deletes.contains(&mpu_key(&id)))
        .collect();
    for _ in 0..2 {
        rig.drain().await.unwrap();
    }
    assert_eq!(rig.sessions_counted().await, counted - 1);
    sent
}

/// **(D, race)** With another session counted, the first drainer reads the lowered ledger and
/// sends its terminal batch — preconditioned on the session record's exact bytes — and the store
/// refuses it: the count goes down by one, not two.
#[tokio::test]
async fn d_two_drainers_racing_the_terminal_delete_release_the_slot_once() {
    let sent = race(5, true).await;
    assert_eq!(sent.len(), 1, "the first drainer sent one terminal batch");
    assert_eq!(
        sent[0].1,
        Some(CommitOutcome::Conflict),
        "and it was refused"
    );
}

/// **(D, race, the last session)** With no other session counted, the rival leaves the ledger at
/// zero. The first drainer reads that, finds the session gone, and stops: no batch, and no hold
/// claiming the ledger needs a human.
#[tokio::test]
async fn d_the_drainer_that_loses_the_last_sessions_race_holds_nothing() {
    let sent = race(6, false).await;
    assert!(sent.is_empty(), "{sent:?}");
}

// ─────────────────────────────── (E) ───────────────────────────────

/// Which untrusted record an `Aborting` session holds.
#[derive(Clone, Copy, Debug)]
enum Untrusted {
    /// E1: a `part:` record whose RS(4,2) chunk is placed on servers 0–4.
    Part,
    /// E2: an owned entry with that placement.
    Entry,
    /// E3: an owned value that will not decode — an unsupported scheme.
    Undecodable,
}

/// The chunk every untrusted record of session `n` names.
fn untrusted_chunk(n: u64) -> ChunkId {
    ChunkId::from(n) * 1_000 + 7
}

/// Plant, for session `id` numbered `n`, the record `kind` names; its key.
async fn plant_untrusted(rig: &Rig, id: &UploadId, n: u64, kind: Untrusted) -> Vec<u8> {
    let bad = untrusted_chunk(n);
    match kind {
        Untrusted::Part => {
            let puts = plant_part(id, 1, &[chunk(bad, RS42, vec![0, 1, 2, 3, 4])]);
            let key = puts[0].0.clone();
            rig.store.plant(puts).await;
            key
        }
        Untrusted::Entry => {
            let (key, value) = owned(id, 1, bad, RS42, vec![0, 1, 2, 3, 4]);
            rig.store.plant(vec![(key.clone(), value)]).await;
            key
        }
        Untrusted::Undecodable => {
            let (key, value) = owned(id, 1, bad, RS42, vec![0, 1, 2, 3, 4, 5]);
            let spelled = name(&value).replace("\"k\":4,\"m\":2", "\"k\":0,\"m\":0");
            assert_ne!(spelled, name(&value), "the scheme was rewritten");
            assert!(decode_owned_entry(&key, spelled.as_bytes()).is_err());
            let (owner, _, chunk) = parse_sidx_key(&key).unwrap();
            assert_eq!((owner, chunk), (id.clone(), bad), "its key names the chunk");
            rig.store
                .plant(vec![(key.clone(), Bytes::from(spelled))])
                .await;
            key
        }
    }
}

/// The event an untrusted record of session `id` is retired under: the Abort's obligation's for
/// a `part:` record, the owned walk's `<upload-id>:<epoch>` for an entry.
async fn retired_under(rig: &Rig, id: &UploadId, kind: Untrusted) -> String {
    match kind {
        Untrusted::Part => teardown(id, 0).1,
        Untrusted::Entry | Untrusted::Undecodable => format!("{id}:{}", rig.epoch_of(id).await),
    }
}

/// **(E)** For an `Aborting` session holding the record `kind` names, with a committed object's
/// fragments on the same servers: the untrusted chunk's fragments sit at `landed` — a position per
/// server — and one more lands, at `late`, right after the record's deletion (after the listing).
/// Deployed passes, whole fleet: every landed fragment, and for an entry that decodes every
/// position it names, is marked in a batch before the one deleting the record; the session ends.
/// Past the grace every landed fragment is gone, and the committed object's stay. What becomes of
/// the late fragment, when there is one, is the caller's to check on the rig returned.
async fn retire_by_chunk_id(
    kind: Untrusted,
    n: u64,
    landed: &[u16],
    late: Option<(DServerId, FragmentId)>,
) -> Rig {
    let rig = Rig::new(6).await;
    let committed = rig.committed_object(90 + n, 77).await;
    let id = rig.session(n, &format!("{BUCKET}/e-{n}")).await;
    let key = plant_untrusted(&rig, &id, n, kind).await;
    let bad = untrusted_chunk(n);
    let landed_at = diagonal(bad, landed);
    rig.fleet.store_at(&landed_at);
    rig.abort(&id).await;
    let event = retired_under(&rig, &id, kind).await;
    let counted = rig.sessions_counted().await;
    let listings_before = Arc::new(AtomicU64::new(0));
    if let Some(at) = late {
        assert!(!rig.fleet.holds(at.0, at.1), "nothing there to list yet");
        *rig.store.late.lock().unwrap() = Some(Late {
            deleted: key.clone(),
            fleet: rig.fleet.clone(),
            at,
            listings_before: Arc::clone(&listings_before),
        });
    }
    rig.store.take_sent();

    rig.drain_until_ended(&id, 8).await;

    if let Some(at) = late {
        assert!(
            rig.fleet.holds(at.0, at.1),
            "the late fragment landed after the record's deletion"
        );
        assert!(
            listings_before.load(Ordering::SeqCst) > 0,
            "and after the listing"
        );
    }
    let sent = rig.store.take_sent();
    let mut evidenced = landed_at.clone();
    if matches!(kind, Untrusted::Entry) {
        evidenced.extend(diagonal(bad, &[0, 1, 2, 3, 4]));
    }
    rig.assert_marked(&evidenced, &event).await;
    assert_marked_before_deleted(&sent, &evidenced, &key);
    assert_eq!(rig.store.value(&key).await, None, "the record is gone");
    assert_eq!(
        rig.sessions_counted().await,
        counted - 1,
        "the terminal delete fired"
    );

    rig.reclaim_past_grace().await;
    rig.assert_gone(&landed_at);
    rig.assert_kept(&committed);
    rig
}

/// **(E1)** A `part:` record whose RS(4,2) chunk is placed on five servers, its six fragments on
/// servers 0–5 — the one on server 5 at a position the record does not name — is retired by chunk
/// id: all six are marked, then the record goes, and past the grace all six are gone.
#[tokio::test]
async fn e1_an_aborted_sessions_untrusted_part_is_retired_by_chunk_id() {
    // A part's chunk has no write in flight (its commit follows every write's `Ok`): no late leg.
    retire_by_chunk_id(Untrusted::Part, 21, &[0, 1, 2, 3, 4, 5], None).await;
}

/// **(E2)** An owned entry with that placement, its chunk's fragments landed on servers 0, 1, 2
/// and 5: the four landed and the five positions it names (3 and 4 with nothing under them yet)
/// are marked, then the entry goes. **Late landing:** a fragment published at position 3 right
/// after the entry's deletion finds the walk's mark, and is gone after the grace with the rest.
#[tokio::test]
async fn e2_an_aborted_sessions_wrong_length_entry_is_retired_by_chunk_id() {
    let late = (3, frag(untrusted_chunk(22), 3));
    let rig = retire_by_chunk_id(Untrusted::Entry, 22, &[0, 1, 2, 5], Some(late)).await;
    rig.assert_gone(&[late]);
}

/// **(E3)** An owned value that will not decode (`RS(0,0)`, which the staged placement refuses),
/// its chunk taken from its key, fragments landed on servers 0–3: all four marked, then the entry
/// goes, and past the grace they are gone. **Late landing:** a fragment published at position 4
/// right after the deletion has no mark and no record — the stated residual — so it stays on disk
/// across GC passes past the late-write deadline and the grace, none of which fails.
#[tokio::test]
async fn e3_an_aborted_sessions_undecodable_entry_is_retired_by_its_keys_chunk_id() {
    let late = (4, frag(untrusted_chunk(23), 4));
    let rig = retire_by_chunk_id(Untrusted::Undecodable, 23, &[0, 1, 2, 3], Some(late)).await;
    for _ in 0..3 {
        rig.clock.advance(LATE_WRITE + GRACE);
        rig.gc().await;
    }
    rig.assert_kept(&[late]);
    assert!(
        rig.mark_at(late).await.is_none(),
        "no mark: kept, never reclaimed"
    );
}

// ─────────────────────────────── (E, controls) ───────────────────────────────

/// **(E, `Open`)** An `Open` session holding all three kinds of record — a wrong-length `part:`
/// record, a wrong-length owned entry and an undecodable one, each over its own chunk with
/// fragments on servers 0–5 — keeps every record and every fragment across deployed passes and GC
/// past the grace. Nothing marks any of them.
#[tokio::test]
async fn e_an_open_sessions_untrusted_records_are_kept_whole() {
    let rig = Rig::new(6).await;
    let mut kept = Vec::new();
    let mut records = Vec::new();
    for (n, kind) in [
        (31, Untrusted::Part),
        (32, Untrusted::Entry),
        (33, Untrusted::Undecodable),
    ] {
        let id = rig.session(n, &format!("{BUCKET}/open-{n}")).await;
        let key = plant_untrusted(&rig, &id, n, kind).await;
        records.push((key.clone(), rig.store.value(&key).await.unwrap()));
        let at = diagonal(untrusted_chunk(n), &[0, 1, 2, 3, 4, 5]);
        rig.fleet.store_at(&at);
        kept.extend(at);
    }

    for _ in 0..3 {
        rig.drain().await.expect("no pass fails");
    }
    rig.reclaim_past_grace().await;

    for (key, value) in &records {
        assert_eq!(
            rig.store.value(key).await.as_ref(),
            Some(value),
            "{}",
            name(key)
        );
    }
    rig.assert_kept(&kept);
    for n in [31, 32, 33] {
        rig.assert_unmarked(untrusted_chunk(n)).await;
    }
}

/// An `Aborting` session holding `kinds`, each over its own chunk with fragments on servers 0–5,
/// drained while server 5 is missing — left out at startup, or, when `dropped`, unreachable this
/// pass. Every pass holds each record as `partial-fleet`: the records stay, nothing of their
/// chunks is marked, the session stays, and GC past the grace keeps every fragment. Once the fleet
/// is whole the records retire and the session ends; past the grace every fragment is gone, server
/// 5's included.
async fn short_fleet(kinds: &[(u64, Untrusted)], dropped: bool) {
    let rig = Rig::new(6).await;
    let n = kinds[0].0;
    let id = rig.session(n, &format!("{BUCKET}/short-{n}")).await;
    let mut records = Vec::new();
    let mut fragments = Vec::new();
    for &(m, kind) in kinds {
        let key = plant_untrusted(&rig, &id, m, kind).await;
        records.push((key.clone(), rig.store.value(&key).await.unwrap()));
        let at = diagonal(untrusted_chunk(m), &[0, 1, 2, 3, 4, 5]);
        rig.fleet.store_at(&at);
        fragments.extend(at);
    }
    rig.abort(&id).await;
    let (obligation, _) = teardown(&id, 0);
    let counted = rig.sessions_counted().await;
    let view = View::without(5, dropped);

    for round in 0..3 {
        let outcome = rig.drain_seeing(&view).await.expect("no pass fails");
        assert_eq!(outcome, Reconciled::Blocked, "round {round}");
        let holds = if matches!(kinds[0].1, Untrusted::Part) {
            rig.audit.obligation_holds(&obligation)
        } else {
            rig.audit.session_holds(&mpu_key(&id))
        };
        assert_eq!(holds.len(), records.len(), "round {round}: {holds:?}");
        for (hold, (key, _)) in holds.iter().zip(&records) {
            assert_eq!(hold["hold"], "partial-fleet", "round {round}");
            assert_eq!(hold["record"], name(key), "round {round}");
        }
        for (key, value) in &records {
            assert_eq!(
                rig.store.value(key).await.as_ref(),
                Some(value),
                "{}",
                name(key)
            );
        }
        for &(m, _) in kinds {
            rig.assert_unmarked(untrusted_chunk(m)).await;
        }
        assert!(rig.store.value(&mpu_key(&id)).await.is_some());
        assert_eq!(rig.sessions_counted().await, counted);
    }
    rig.reclaim_past_grace().await;
    rig.assert_kept(&fragments);
    rig.audit.clear();

    rig.drain_until_ended(&id, 8).await;
    for (key, _) in &records {
        assert_eq!(rig.store.value(key).await, None, "{}", name(key));
    }
    assert_eq!(rig.sessions_counted().await, counted - 1);
    rig.reclaim_past_grace().await;
    rig.assert_gone(&fragments);
}

/// **(E, fleet, a `part:` record)** Server 5 left out at startup.
#[tokio::test]
async fn e_a_part_record_waits_for_a_server_left_out_at_startup() {
    short_fleet(&[(41, Untrusted::Part)], false).await;
}

/// **(E, fleet, a `part:` record)** Server 5 unreachable this pass.
#[tokio::test]
async fn e_a_part_record_waits_for_a_server_unreachable_this_pass() {
    short_fleet(&[(42, Untrusted::Part)], true).await;
}

/// **(E, fleet, owned entries)** A wrong-length entry and an undecodable one; server 5 left out at
/// startup.
#[tokio::test]
async fn e_owned_entries_wait_for_a_server_left_out_at_startup() {
    short_fleet(
        &[(43, Untrusted::Entry), (44, Untrusted::Undecodable)],
        false,
    )
    .await;
}

/// **(E, fleet, owned entries)** The same two; server 5 unreachable this pass.
#[tokio::test]
async fn e_owned_entries_wait_for_a_server_unreachable_this_pass() {
    short_fleet(
        &[(45, Untrusted::Entry), (46, Untrusted::Undecodable)],
        true,
    )
    .await;
}

/// **(E, undecodable `part:`)** An `Aborting` session whose one part value will not decode — its
/// chunk's scheme `RS(0,0)`, which `decode_part_record` refuses. Its key names no chunk, so nothing
/// can find its bytes: every pass holds the obligation as `record-unreadable`, the audit event says
/// it needs a human, the record and the obligation stay byte-identical, nothing of the chunk is
/// marked, and the session's terminal delete does not fire — its record stays, its slot counted.
#[tokio::test]
async fn e_an_undecodable_part_record_needs_a_human_and_the_session_stays() {
    let rig = Rig::new(3).await;
    let id = rig.session(51, &format!("{BUCKET}/e-undecodable")).await;
    let zero = EcScheme::ReedSolomon { k: 0, m: 0 };
    let value = raw_part_value(&[chunk(51_000, zero, vec![0])], digest_of(1));
    assert!(
        decode_part_record(&value).is_err(),
        "the decoder refuses it"
    );
    let key = part_key(&id, part(1));
    rig.store.plant(vec![(key.clone(), value.clone())]).await;
    rig.fleet.store_at(&[(0, frag(51_000, 0))]);
    rig.abort(&id).await;
    let (obligation, _) = teardown(&id, 0);
    let owed = rig.store.value(&obligation).await.unwrap();
    let counted = rig.sessions_counted().await;

    for round in 0..3 {
        let outcome = rig.drain().await.expect("no pass fails");
        assert_eq!(outcome, Reconciled::Blocked, "round {round}");
        let holds = rig.audit.obligation_holds(&obligation);
        assert_eq!(holds.len(), 1, "round {round}: {holds:?}");
        assert_eq!(holds[0]["hold"], "record-unreadable");
        assert_eq!(holds[0]["record"], name(&key));
        assert!(
            holds[0]["detail"].contains("needs a human"),
            "{:?}",
            holds[0]
        );
        assert_eq!(rig.store.value(&key).await, Some(value.clone()));
        assert_eq!(rig.store.value(&obligation).await, Some(owed.clone()));
        assert!(
            rig.store.value(&mpu_key(&id)).await.is_some(),
            "round {round}"
        );
        assert_eq!(rig.sessions_counted().await, counted, "round {round}");
        rig.assert_unmarked(51_000).await;
    }
}
