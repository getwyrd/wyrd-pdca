//! Issue #1014 (#659 child 3) — the **orphan-identity migration gate** and the routing of `unlink`
//! and supersede through one `retire:bytes:g:<inode>:<version>` obligation behind it (proposal
//! 0016 decision 4 rules 1–2, `0016:1079-1290`).
//!
//! Driven through `reconcile_step` with the contexts the deployed loop passes
//! (`crates/server/src/custodian.rs`, `run_reconstruction_until`): a **repair** pass — the
//! reconstruction input, every other input `None`, its `now_millis` and its context's clock one
//! source — which runs the retirement drain and then the cleanup walk; and **GC** as a pass of its
//! own. Every pass builds its leadership fence and context afresh. The metadata store is
//! `RedbMetadataStore::in_memory()` behind a thin wrapper (a forced race, a frozen key, a failing
//! page, steady deletes landing between the cleanup's reads, refused commits); the D servers are
//! a placement-aware in-process double. Deletes and overwrites go through the production
//! `metadata::unlink` and `write::commit_overwrite`; multipart objects through the production
//! Complete.
//!
//! * **(F)** before a valid marker: `unlink` / supersede behave as on the base (a 100-chunk delete
//!   writes its 900 marks inline, a segmented object is refused) and GC treats every mark as
//!   protecting. Then the cleanup judges marks: **(F1)** on a committed fragment: dropped; on a
//!   fragment only a pending obligation names (a Complete's superseded generation, or a routed
//!   delete's segmented retirement after a restore): dropped; on a segmented object's fragment
//!   its published parts also name: dropped — but never on the word of a placement or a staging
//!   entry it cannot trust, which leaves the mark and blocks the marker until repaired. **(F2)**
//!   On a staged one: dropped, and the publish → records drain → routed delete sequence reclaims
//!   nothing before the drain marks, nothing inside its fresh grace, everything after. **(F3)** The
//!   2026-09-12 trap: kept byte-identical, its bytes go after its grace — and with the session's
//!   record damaged, kept, the marker blocked until the record is repaired. **(F4)** A
//!   `reclaiming` mark and a draining one: byte-identical — and a `reclaiming` mark a restore
//!   brings back onto a live fragment blocks the marker, so the object's delete stays inline and
//!   nothing goes before its grace; one only a retirement names does not block, so drain, GC and
//!   the walk all finish; a draining mark whose retirement names other fragments, on a live
//!   object's, withholds the marker until that retirement drains, and the object's routed delete
//!   after it reclaims nothing early. **(F5)** A drop racing `unlink` (and an overwrite) fails, the
//!   writer's mark survives, the bytes go after its grace; steady deletes landing between the
//!   cleanup's reads — at the ledger's head, or inside its reading — never stop the marker being
//!   written within a few passes; two lost drops at the read budget's edge never run it out.
//!   **(F6)** An unreadable inode, `seg:` and `part:` record each block the marker until
//!   repaired, an interrupted walk writes none, and a damaged walk cursor past the ledger starts a
//!   fresh lap rather than ending one unjudged. **(F7)** A marker from before a restore reads
//!   as not complete once the production post-restore pass has run, and a restore in the middle
//!   of a lap that spans passes discards that lap.
//! * **(A)** with the marker present: **(A1)** the historical 30-chunk RS(6,3) delete reclaims all
//!   270 fragments; **(A2)** a 100-chunk RS(6,3) delete installs exactly one obligation and no
//!   mark, and all 900 fragments go — but none before the drain's marks and none inside their
//!   grace, under GC's expired-lease input too for an object whose writer never released its
//!   leases; the 83-chunk RS(4,2) boundary (502 operations) takes the obligation path; **(A3)**
//!   the same for an overwrite's prior generation and a segmented object's delete.
//!
//! Every count is a literal stated here; fragments are asserted present or absent on the fleet.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self as md, decode_orphan_mark, encode_orphan_mark, orphan_key, seg_key, ChunkMap,
    ChunkMapError, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, ObjectMeta, OrphanMark,
    SegmentNonce,
};
use wyrd_core::multipart::complete::{
    complete_upload, CompleteRequest, NamedParts, PublishRequest,
};
use wyrd_core::multipart::create::{create_session, CreateRequest, CreateRuntime};
use wyrd_core::multipart::{
    decode_part_record, decode_part_summary, mpu_key, part_key, psum_key, retire_key, sidx_key,
    Budget, CompleteOutcome, CreateOutcome, Digest, OwnedEntry, PartNumber, PartSummary,
    RetireMode, RetireToken, StagedPlacement, UploadId, MPUFENCE_KEY, RETIRE_BYTES_PREFIX,
};
use wyrd_core::orphan_identity::{CLEANUP_WALK_KEY, ORPHAN_IDENTITY_KEY};
use wyrd_core::placement::Topology;
use wyrd_core::write::{self, PlannedChunk, WritePlan};
use wyrd_custodian::gc::W_WRITE_MILLIS;
use wyrd_custodian::orphan_identity::{status, CleanupStatus};
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone,
    GcContext, ReconcileError, Reconciled, ReconstructionContext,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_testkit::{Clock, ManualClock};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    PlacementChunkStore, Result as StoreResult, WriteBatch,
};

// ─────────────────────────────── fixtures ───────────────────────────────

/// Where every rig's clock starts.
const T0: u64 = 10_000_000;
/// The orphan grace every GC pass here honours.
const GRACE: u64 = 60_000;
/// A stamp far in the past: a mark an older custodian wrote long ago, past every grace.
const ANCIENT: u64 = 1_000;
const BUCKET: &str = "photos";
const ROOT: InodeId = 0;
const BUCKET_MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
const COMMITTED_AT: u64 = 1_500;
const CHUNK_LEN: u64 = 64;
/// Six data fragments and three parity: nine positions a chunk, on servers `0..9`.
const RS63: EcScheme = EcScheme::ReedSolomon { k: 6, m: 3 };
/// Four data fragments and two parity: six positions a chunk, on servers `0..6`.
const RS42: EcScheme = EcScheme::ReedSolomon { k: 4, m: 2 };
/// Two data fragments and one parity: three positions a chunk, on servers `0..3`.
const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };
/// The D servers every rig serves.
const SERVERS: u64 = 9;

fn width(scheme: EcScheme) -> u64 {
    match scheme {
        EcScheme::None => 1,
        EcScheme::ReedSolomon { k, m } => u64::from(k) + u64::from(m),
    }
}

/// `count` chunks of `scheme` from id `first`, each placed on servers `0..k+m`.
fn chunks(first: ChunkId, count: u64, scheme: EcScheme) -> Vec<ChunkRef> {
    (0..count)
        .map(|i| ChunkRef {
            id: first + ChunkId::from(i),
            scheme,
            len: CHUNK_LEN,
            placement: (0..width(scheme)).collect(),
        })
        .collect()
}

/// Every fragment position `chunks` places, in order.
fn positions(chunks: &[ChunkRef]) -> Vec<(DServerId, FragmentId)> {
    chunks
        .iter()
        .flat_map(|c| {
            c.fragments()
                .map(|(index, dserver)| (dserver, FragmentId { chunk: c.id, index }))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn upload(n: u64) -> UploadId {
    UploadId::new(format!("{n:032x}")).unwrap()
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).unwrap()
}

fn digest_of(n: u32) -> Digest {
    let mut bytes = [0x3c; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    Digest::from_bytes(bytes)
}

fn legacy(at: u64) -> Bytes {
    encode_orphan_mark(&OrphanMark::legacy(at))
}

fn structured(at: u64, event: &str) -> Bytes {
    encode_orphan_mark(&OrphanMark::structured(at, event).unwrap())
}

/// The `part:` and `psum:` puts of part `n` of `id` over `chunks`, in the codecs' own spelling.
fn plant_part(id: &UploadId, n: u32, chunks: &[ChunkRef]) -> Vec<(Vec<u8>, Bytes)> {
    let digest = digest_of(n);
    let len: u64 = chunks.iter().map(|c| c.len).sum();
    let record = format!(
        "{{\"chunks\":{},\"len\":{len},\"digest\":\"{digest}\",\
         \"committed_at_millis\":{COMMITTED_AT},\"session_epoch\":0}}",
        String::from_utf8(md::encode(&chunks).to_vec()).unwrap()
    );
    let decoded = decode_part_record(record.as_bytes()).expect("a canonical part");
    assert_eq!(md::encode(&decoded), record.as_bytes());
    let summary = PartSummary {
        chunks: chunks.len() as u32,
        len,
        digest,
        committed_at_millis: COMMITTED_AT,
    };
    let psum = md::encode(&summary);
    assert_eq!(decode_part_summary(&psum).unwrap(), summary);
    vec![
        (part_key(id, part(n)), Bytes::from(record)),
        (psum_key(id, part(n)), psum),
    ]
}

/// The obligation key of generation `(inode, version)`.
fn generation_key(inode: InodeId, version: u64) -> Vec<u8> {
    retire_key(
        RetireMode::Bytes,
        &RetireToken::Generation { inode, version },
    )
}

/// A flat generation obligation of `(inode, version)` over `chunks`, in the codec's own spelling:
/// its key and value.
fn generation_obligation(inode: InodeId, version: u64, chunks: &[ChunkRef]) -> (Vec<u8>, Bytes) {
    let key = generation_key(inode, version);
    let value = format!(
        "{{\"generation\":{{\"inode\":{inode},\"version\":{version},\"chunks\":{}}}}}",
        String::from_utf8(md::encode(&chunks).to_vec()).unwrap()
    );
    wyrd_core::multipart::decode_retire_obligation(&key, value.as_bytes()).unwrap();
    (key, Bytes::from(value))
}

// ─────────────────────────── the metadata store ───────────────────────────

/// A writer that runs against the store at the instant the cleanup commits one drop (F5).
enum Race {
    /// The production inline `unlink` of `name`, at `at`.
    Unlink { name: String, at: u64 },
    /// The production `write::commit_overwrite` of `inode` over `prior` with `plan`, at `at`.
    Overwrite {
        inode: InodeId,
        prior: Box<InodeRecord>,
        plan: Box<WritePlan>,
        at: u64,
    },
    /// The production Complete of `upload` at `object`, naming `named`, publishing under `inode`.
    Complete {
        upload: UploadId,
        object: String,
        named: Vec<u32>,
        inode: InodeId,
    },
}

/// A production Complete of `id` at `object`, naming `named`, publishing under `inode`; it must
/// publish.
async fn complete_on(
    store: &RedbMetadataStore,
    id: &UploadId,
    object: &str,
    named: &[u32],
    inode: InodeId,
) {
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
    let outcome = complete_upload(store, id, &request, &publish).await;
    assert!(
        matches!(outcome, Ok(CompleteOutcome::Published(_))),
        "{outcome:?}"
    );
}

/// `RedbMetadataStore` behind a wrapper that can (a) run a [`Race`] just before the commit that
/// deletes one `orphan:` key goes through, (b) answer `Conflict` to every commit writing or
/// deleting a **frozen** key — a drain that has not reached that obligation yet — (c) fail a
/// page of the `orphan:` ledger after its head, once, (d) run one queued production `unlink` each
/// time the `orphan:` ledger is read from its head — before that read — or the first time the
/// `inode:` namespace is scanned after any read of the ledger — after that scan — and (e) answer
/// `Conflict` to the next commits deleting a **refused** key, a set number of times.
struct Store {
    inner: RedbMetadataStore,
    race: Mutex<Option<(Vec<u8>, Race)>>,
    raced: AtomicBool,
    frozen: Mutex<Vec<Vec<u8>>>,
    fail_ledger_page: AtomicBool,
    at_ledger_head: Mutex<VecDeque<(String, u64)>>,
    after_inode_scan: Mutex<VecDeque<(String, u64)>>,
    inode_scan_armed: AtomicBool,
    unlinks_ran: AtomicUsize,
    refused: Mutex<HashMap<Vec<u8>, usize>>,
}

impl Store {
    fn new() -> Self {
        Self {
            inner: RedbMetadataStore::in_memory().unwrap(),
            race: Mutex::new(None),
            raced: AtomicBool::new(false),
            frozen: Mutex::new(Vec::new()),
            fail_ledger_page: AtomicBool::new(false),
            at_ledger_head: Mutex::new(VecDeque::new()),
            after_inode_scan: Mutex::new(VecDeque::new()),
            inode_scan_armed: AtomicBool::new(false),
            unlinks_ran: AtomicUsize::new(0),
            refused: Mutex::new(HashMap::new()),
        }
    }

    /// The production inline `unlink` of the next queued object, straight on the inner store.
    async fn unlink_next(&self, queue: &Mutex<VecDeque<(String, u64)>>) -> StoreResult<()> {
        let next = queue.lock().unwrap().pop_front();
        if let Some((name, at)) = next {
            let unlinked = md::unlink(&self.inner, ROOT, &name, at).await?;
            let outcome = unlinked.expect("the object is bound").outcome;
            assert_eq!(outcome, CommitOutcome::Committed, "the steady delete lands");
            self.unlinks_ran.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Whether `batch` deletes a refused key with refusals left — and spend one.
    fn refuses(&self, batch: &WriteBatch) -> bool {
        let mut refused = self.refused.lock().unwrap();
        for key in &batch.deletes {
            if let Some(left) = refused.get_mut(key) {
                if *left > 0 {
                    *left -= 1;
                    return true;
                }
            }
        }
        false
    }

    /// Fixture writes, straight to the inner store.
    async fn plant(&self, puts: Vec<(Vec<u8>, Bytes)>) {
        for chunk in puts.chunks(400) {
            let mut batch = WriteBatch::new();
            batch.puts.extend(chunk.iter().cloned());
            assert_eq!(
                self.inner.commit(batch).await.unwrap(),
                CommitOutcome::Committed
            );
        }
    }

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
        let mut records = BTreeMap::new();
        let mut after: Option<Vec<u8>> = None;
        loop {
            let (page, next) = self
                .inner
                .scan_page(prefix, after.as_deref(), 10_000)
                .await
                .unwrap();
            let last = page.last().map(|(key, _)| key.clone());
            records.extend(page);
            match (next, last) {
                (Some(_), Some(last)) => after = Some(last),
                _ => return records,
            }
        }
    }

    fn freeze(&self, key: &[u8]) {
        self.frozen.lock().unwrap().push(key.to_vec());
    }

    fn thaw(&self) {
        self.frozen.lock().unwrap().clear();
    }

    /// Whether `batch` writes or deletes a frozen key — what a drain step does to the obligation
    /// it advances or finishes. A batch that only requires one (a drop judged by it) goes through.
    fn touches_frozen(&self, batch: &WriteBatch) -> bool {
        let frozen = self.frozen.lock().unwrap();
        let mut keys = batch
            .puts
            .iter()
            .map(|(k, _)| k)
            .chain(batch.deletes.iter());
        keys.any(|key| frozen.contains(key))
    }
}

#[async_trait]
impl MetadataStore for Store {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        let scanned = self.inner.scan(prefix).await?;
        if prefix == b"inode:" && self.inode_scan_armed.swap(false, Ordering::SeqCst) {
            // The reader holds the objects as they were; one of them goes now.
            self.unlink_next(&self.after_inode_scan).await?;
        }
        Ok(scanned)
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> StoreResult<wyrd_traits::ScanPage> {
        if prefix == md::ORPHAN_PREFIX
            && after.is_some()
            && self.fail_ledger_page.swap(false, Ordering::SeqCst)
        {
            return Err("injected: the orphan ledger page after the head failed".into());
        }
        if prefix == md::ORPHAN_PREFIX {
            if after.is_none() {
                self.unlink_next(&self.at_ledger_head).await?;
            }
            if !self.after_inode_scan.lock().unwrap().is_empty() {
                self.inode_scan_armed.store(true, Ordering::SeqCst);
            }
        }
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        if self.touches_frozen(&batch) || self.refuses(&batch) {
            return Ok(CommitOutcome::Conflict);
        }
        let race = {
            let mut race = self.race.lock().unwrap();
            match &*race {
                Some((target, _)) if batch.deletes.contains(target) => race.take(),
                _ => None,
            }
        };
        if let Some((_, race)) = race {
            // The cleanup has read the mark and the records it judged by, and its drop is on its
            // way to the store: the writer lands first.
            match race {
                Race::Unlink { name, at } => {
                    let unlinked = md::unlink(&self.inner, ROOT, &name, at).await?;
                    let outcome = unlinked.expect("the object is bound").outcome;
                    assert_eq!(outcome, CommitOutcome::Committed, "the unlink lands");
                }
                Race::Overwrite {
                    inode,
                    prior,
                    plan,
                    at,
                } => {
                    let outcome =
                        write::commit_overwrite(&self.inner, inode, &prior, &plan, at).await?;
                    assert_eq!(outcome, CommitOutcome::Committed, "the overwrite lands");
                    write::release(&self.inner, &plan).await?;
                }
                Race::Complete {
                    upload,
                    object,
                    named,
                    inode,
                } => complete_on(&self.inner, &upload, &object, &named, inode).await,
            }
            self.raced.store(true, Ordering::SeqCst);
        }
        self.inner.commit(batch).await
    }
}

// ─────────────────────────────── the D servers ───────────────────────────────

/// In-process D servers addressed by id.
#[derive(Clone, Default)]
struct Fleet(Arc<Mutex<BTreeMap<DServerId, HashMap<FragmentId, Bytes>>>>);

impl Fleet {
    fn serving(ids: impl IntoIterator<Item = DServerId>) -> Self {
        let servers = ids.into_iter().map(|id| (id, HashMap::new())).collect();
        Self(Arc::new(Mutex::new(servers)))
    }

    fn put(&self, dserver: DServerId, frag: FragmentId, bytes: Bytes) {
        let mut servers = self.0.lock().unwrap();
        servers
            .get_mut(&dserver)
            .expect("a server")
            .insert(frag, bytes);
    }

    fn holds(&self, dserver: DServerId, frag: FragmentId) -> bool {
        let servers = self.0.lock().unwrap();
        servers
            .get(&dserver)
            .is_some_and(|held| held.contains_key(&frag))
    }

    fn store_all(&self, chunks: &[ChunkRef]) {
        for (dserver, frag) in positions(chunks) {
            self.put(dserver, frag, Bytes::from(format!("{frag:?}")));
        }
    }

    fn ids(&self) -> Vec<DServerId> {
        self.0.lock().unwrap().keys().copied().collect()
    }
}

#[async_trait]
impl ChunkStore for Fleet {
    async fn put_fragment(&self, _: FragmentId, _: Bytes, _: Option<u64>) -> StoreResult<()> {
        Err("Fleet: a write must address a D server".into())
    }

    async fn get_fragment(&self, id: FragmentId) -> StoreResult<Option<Bytes>> {
        let servers = self.0.lock().unwrap();
        Ok(servers.values().find_map(|held| held.get(&id).cloned()))
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        let servers = self.0.lock().unwrap();
        Ok(servers
            .values()
            .flat_map(|held| held.keys().copied())
            .collect())
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
        _deadline_millis: Option<u64>,
    ) -> StoreResult<()> {
        self.put(dserver, id, fragment);
        Ok(())
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
        let servers = self.fleet.0.lock().unwrap();
        Ok(servers
            .get(&self.id)
            .and_then(|held| held.get(&id).cloned()))
    }

    async fn list_fragments(&self) -> StoreResult<Vec<FragmentId>> {
        let servers = self.fleet.0.lock().unwrap();
        Ok(servers
            .get(&self.id)
            .map(|held| held.keys().copied().collect())
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

// ─────────────────────────────── the rig ───────────────────────────────

/// One store, one fleet, one clock — the deployed loop's one time source.
struct Rig {
    store: Store,
    fleet: Fleet,
    clock: ManualClock,
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

async fn elect() -> (FencedZone, Custodian) {
    let coord = MemCoordination::new();
    let custodian = Custodian::elect(&coord, "zone-retirement-routing")
        .await
        .expect("leader election over the in-memory coordination seam");
    let mut zone = FencedZone::new();
    zone.install(custodian.leadership());
    (zone, custodian)
}

impl Rig {
    async fn new() -> Self {
        let rig = Self {
            store: Store::new(),
            fleet: Fleet::serving(0..SERVERS),
            clock: ManualClock::new(T0),
        };
        let bucket = vec![(md::bucket_key(BUCKET), Bytes::from_static(BUCKET_MARKER))];
        rig.store.plant(bucket).await;
        rig
    }

    fn servers(&self) -> Vec<Server> {
        self.fleet
            .ids()
            .into_iter()
            .map(|id| Server {
                id,
                fleet: self.fleet.clone(),
            })
            .collect()
    }

    /// One fenced **repair** pass as the deployed loop runs it: the reconstruction input alone
    /// (`run_reconstruction_until`), a fresh term and context, `now_millis` and the context's
    /// clock read from the one clock. Its retirement leg drains; its cleanup walks.
    async fn repair(&self) -> Result<Reconciled, ReconcileError> {
        let (zone, custodian) = elect().await;
        let servers = self.servers();
        let fleet: Vec<(DServerId, &dyn ChunkStore)> = servers
            .iter()
            .map(|s| (s.id, s as &dyn ChunkStore))
            .collect();
        let topology = Topology::default();
        let ctx = ReconstructionContext {
            meta: &self.store,
            fleet: &fleet,
            topology: &topology,
            unreachable: &[],
            clock: &self.clock,
            staged_write_window_millis: W_WRITE_MILLIS,
            operator_fleet_size: fleet.len(),
            tombstone: wyrd_custodian::Tombstone::Retain,
        };
        let now = self.clock.now_millis();
        reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, now).await
    }

    /// One fenced GC pass over the whole fleet at the clock's reading, as the deployed loop runs
    /// it once every configured server is visible.
    async fn gc(&self) -> Reconciled {
        self.gc_with(ExpiredPendingPolicy::Defer).await
    }

    /// One fenced GC pass under `expired_pending`.
    async fn gc_with(&self, expired_pending: ExpiredPendingPolicy) -> Reconciled {
        let (zone, custodian) = elect().await;
        let servers = self.servers();
        let fleet: Vec<(DServerId, &dyn ChunkStore)> = servers
            .iter()
            .map(|s| (s.id, s as &dyn ChunkStore))
            .collect();
        let ctx = GcContext {
            meta: &self.store,
            fleet: &fleet,
            grace_window_millis: GRACE,
            expired_pending,
        };
        let now = self.clock.now_millis();
        reconcile_step(&zone, &custodian, Some(&ctx), None, None, None, now)
            .await
            .expect("the GC pass never fails")
    }

    /// GC passes until one reclaims nothing more (at most eight).
    async fn gc_settled(&self) {
        for _ in 0..8 {
            if self.gc().await != Reconciled::Changed {
                return;
            }
        }
        panic!("GC kept changing for eight passes");
    }

    /// The production post-restore pass over the whole fleet.
    async fn restore_pass(&self) {
        let servers = self.servers();
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
        reconcile_after_restore(&ctx, self.clock.now_millis())
            .await
            .expect("the post-restore pass runs");
    }

    async fn status(&self) -> CleanupStatus {
        status(&self.store).await.expect("the status reads")
    }

    /// Repair passes until the cleanup's marker is valid, at most `max`; none fails.
    async fn repair_until_complete(&self, max: usize) {
        for _ in 0..max {
            if self.status().await == CleanupStatus::Complete {
                return;
            }
            self.repair().await.expect("the repair pass never fails");
        }
        assert_eq!(self.status().await, CleanupStatus::Complete, "within {max}");
    }

    /// Repair passes until the obligation at `key` is gone, at most `max`; none fails.
    async fn drain_until_gone(&self, key: &[u8], max: usize) {
        for _ in 0..max {
            if self.store.value(key).await.is_none() {
                return;
            }
            self.repair().await.expect("the repair pass never fails");
        }
        assert!(
            self.store.value(key).await.is_none(),
            "drained within {max}"
        );
    }

    /// A committed flat object `name` under inode `id` over `chunks`, its fragments on the fleet.
    async fn object(&self, name: &str, id: InodeId, chunks: &[ChunkRef]) -> InodeRecord {
        let record = InodeRecord {
            size: chunks.iter().map(|c| c.len).sum(),
            chunk_map: ChunkMap::Flat(chunks.to_vec()),
            state: InodeState::Committed,
            version: 1,
            etag: None,
            content_type: None,
            modified: None,
        };
        let created = md::create(&self.store.inner, ROOT, name, id, &record).await;
        assert_eq!(created.unwrap(), CommitOutcome::Committed);
        self.fleet.store_all(chunks);
        record
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
        let nonce = SegmentNonce::new(format!("{:032x}", 0x5e60_0000 + n)).unwrap();
        let created = create_session(
            &self.store.inner,
            &profile(),
            &request,
            &mut Fixed(Some((upload(n), nonce))),
        )
        .await
        .unwrap();
        let CreateOutcome::Created { upload_id } = created else {
            panic!("expected Created, got {created:?}");
        };
        upload_id
    }

    /// Plant `parts` of `id` (each a part number and its chunks), their fragments on the fleet.
    async fn plant_parts(&self, id: &UploadId, parts: &[(u32, Vec<ChunkRef>)]) {
        let mut puts = Vec::new();
        for (n, chunks) in parts {
            puts.extend(plant_part(id, *n, chunks));
            self.fleet.store_all(chunks);
        }
        self.store.plant(puts).await;
    }

    /// A production Complete of `id` at `object`, naming `named`, publishing under `inode`.
    async fn complete(&self, id: &UploadId, object: &str, named: &[u32], inode: InodeId) {
        complete_on(&self.store.inner, id, object, named, inode).await;
    }

    /// The inode record under `id`, decoded.
    async fn inode(&self, id: InodeId) -> InodeRecord {
        let bytes = self.store.value(&md::inode_key(id)).await.expect("bound");
        md::decode(&bytes).unwrap()
    }

    /// The production `unlink` of `name` at the clock's reading; it must commit.
    async fn unlink(&self, name: &str) {
        let now = self.clock.now_millis();
        let unlinked = md::unlink(&self.store, ROOT, name, now).await.unwrap();
        assert_eq!(unlinked.expect("bound").outcome, CommitOutcome::Committed);
    }

    /// Plant `value` as the mark of every position of `at`.
    async fn mark(&self, at: &[(DServerId, FragmentId)], value: Bytes) {
        let puts = at
            .iter()
            .map(|&(d, f)| (orphan_key(d, f), value.clone()))
            .collect();
        self.store.plant(puts).await;
    }

    async fn mark_at(&self, position: (DServerId, FragmentId)) -> Option<Bytes> {
        self.store.value(&orphan_key(position.0, position.1)).await
    }

    async fn assert_unmarked(&self, at: &[(DServerId, FragmentId)], why: &str) {
        for &position in at {
            assert_eq!(self.mark_at(position).await, None, "{why}: {position:?}");
        }
    }

    async fn assert_marked_with(&self, at: &[(DServerId, FragmentId)], value: &Bytes, why: &str) {
        for &position in at {
            assert_eq!(
                self.mark_at(position).await.as_ref(),
                Some(value),
                "{why}: {position:?}"
            );
        }
    }

    fn assert_gone(&self, at: &[(DServerId, FragmentId)], why: &str) {
        for &(dserver, frag) in at {
            assert!(
                !self.fleet.holds(dserver, frag),
                "{why}: {frag:?} on {dserver} survived"
            );
        }
    }

    fn assert_kept(&self, at: &[(DServerId, FragmentId)], why: &str) {
        for &(dserver, frag) in at {
            assert!(
                self.fleet.holds(dserver, frag),
                "{why}: {frag:?} on {dserver} was reclaimed"
            );
        }
    }

    /// **The retirement's presence, then its absence** (A2, A3): with an obligation at `key`
    /// installed and no mark on `at` yet, GC-only passes with the clock past every older mark's
    /// grace reclaim none of `at`; the drain then marks every position under the obligation's
    /// token, stamped at the drain; GC passes inside that grace reclaim none; passes past it
    /// leave every one gone, and neither the obligation nor any mark of them remains.
    async fn assert_retired_through_the_drain(
        &self,
        key: &[u8],
        token: &str,
        at: &[(DServerId, FragmentId)],
    ) {
        assert!(self.store.value(key).await.is_some(), "installed");
        self.assert_unmarked(at, "no inline mark").await;

        self.clock.advance(10 * GRACE);
        for _ in 0..3 {
            self.gc().await;
        }
        self.assert_kept(at, "GC before the drain marks");

        let drained_at = self.clock.now_millis();
        self.drain_until_gone(key, 6).await;
        for &position in at {
            let mark = self.mark_at(position).await.expect("marked by the drain");
            let mark = decode_orphan_mark(&mark).unwrap();
            assert_eq!(mark.event(), Some(token), "{position:?}");
            assert_eq!(mark.orphaned_at_millis(), drained_at, "{position:?}");
        }

        self.clock.advance(GRACE - 1);
        for _ in 0..3 {
            self.gc().await;
        }
        self.assert_kept(at, "GC inside the drain's grace");

        self.clock.advance(2);
        self.gc_settled().await;
        self.assert_gone(at, "GC past the drain's grace");
        self.assert_unmarked(at, "no mark left").await;
        assert!(self.store.value(key).await.is_none(), "no obligation left");
    }
}

/// A flat 100-chunk RS(6,3) object, `name` at inode `id`: 900 fragments.
async fn hundred_chunk_object(rig: &Rig, name: &str, id: InodeId, first: ChunkId) -> Vec<ChunkRef> {
    let chunks = chunks(first, 100, RS63);
    rig.object(name, id, &chunks).await;
    assert_eq!(positions(&chunks).len(), 900);
    chunks
}

// ───────────────────────────── (F) the migration gate ─────────────────────────────

/// (F) Before a valid marker: a 100-chunk RS(6,3) delete writes its 900 marks inline in the one
/// batch and installs no obligation, a segmented object's delete and overwrite are refused, and
/// GC treats every mark as protecting — a fragment no record names, marked long ago, is kept.
/// After the cleanup completes, that fragment is reclaimed.
#[tokio::test]
async fn f_before_the_marker_the_paths_behave_as_on_the_base_and_gc_holds_every_mark() {
    let rig = Rig::new().await;
    // A fragment no record names, marked long ago: GC's ordinary input once the gate is open.
    let stray = chunks(900_000, 1, RS21);
    rig.fleet.store_all(&stray);
    let stray = positions(&stray);
    rig.mark(&stray, legacy(ANCIENT)).await;

    let big = hundred_chunk_object(&rig, "photos/big", 41, 100_000).await;
    rig.unlink("photos/big").await;
    rig.assert_marked_with(&positions(&big), &legacy(T0), "inline marks")
        .await;
    assert!(
        rig.store.under(RETIRE_BYTES_PREFIX).await.is_empty(),
        "no obligation before the marker"
    );

    // A segmented object, published by the production Complete: refused, as on the base.
    let id = rig.session(1, "photos/seg").await;
    let parts = vec![
        (1, chunks(200_000, 90, RS21)),
        (2, chunks(201_000, 90, RS21)),
    ];
    rig.plant_parts(&id, &parts).await;
    rig.complete(&id, "photos/seg", &[1, 2], 42).await;
    let seg = rig.inode(42).await;
    assert!(seg.chunk_map.is_segmented(), "180 chunks segment");
    let refused = md::unlink(&rig.store, ROOT, "photos/seg", T0).await;
    let err = refused.expect_err("a segmented delete is refused before the marker");
    assert!(
        matches!(
            err.downcast_ref::<ChunkMapError>(),
            Some(ChunkMapError::SegmentedMapUnsupported { .. })
        ),
        "{err}"
    );
    let overwrite = md::commit_chunk_map_superseding(
        &rig.store,
        42,
        &seg,
        chunks(202_000, 1, RS21),
        CHUNK_LEN,
        T0,
        &ObjectMeta::default(),
    )
    .await;
    assert!(overwrite.is_err(), "a segmented overwrite is refused too");

    // GC: every mark protects while no marker exists.
    rig.clock.advance(10 * GRACE);
    for _ in 0..3 {
        rig.gc().await;
    }
    rig.assert_kept(&stray, "a long-expired mark while the gate is closed");
    rig.assert_kept(
        &positions(&big),
        "the inline delete's marks while the gate is closed",
    );

    rig.repair_until_complete(4).await;
    let marker = rig.store.value(ORPHAN_IDENTITY_KEY).await.expect("marker");
    assert_eq!(
        &marker[..],
        b"{}",
        "named after the absent restore-fence record"
    );
    rig.gc_settled().await;
    rig.assert_gone(&stray, "reclaimed once the gate is open");
    rig.assert_gone(
        &positions(&big),
        "the inline delete's bytes, past their grace",
    );
}

/// (F1) A mark on a fragment a committed map names is dropped; one on a fragment nothing names is
/// left. A committed object's own bytes are never touched.
#[tokio::test]
async fn f1_a_mark_on_a_committed_fragment_is_dropped() {
    let rig = Rig::new().await;
    let live = chunks(1_000, 4, RS63);
    rig.object("photos/live", 7, &live).await;
    let live = positions(&live);
    rig.mark(&live[..20], legacy(ANCIENT)).await;
    rig.mark(&live[20..30], structured(ANCIENT, "g:7:0")).await;
    let stray = chunks(1_500, 1, RS21);
    rig.fleet.store_all(&stray);
    let stray = positions(&stray);
    rig.mark(&stray, legacy(ANCIENT)).await;

    assert_eq!(
        rig.repair().await.expect("the pass"),
        Reconciled::Changed,
        "it dropped marks and wrote the marker"
    );
    assert_eq!(rig.status().await, CleanupStatus::Complete);
    rig.assert_unmarked(&live, "a committed fragment's stale mark")
        .await;
    rig.assert_marked_with(&stray, &legacy(ANCIENT), "named by nothing: GC's input")
        .await;
    rig.gc_settled().await;
    rig.assert_kept(&live, "committed");
    rig.assert_gone(&stray, "GC's ordinary input");
    assert_eq!(
        rig.repair().await.expect("the pass"),
        Reconciled::Satisfied,
        "a valid marker: nothing to walk"
    );
}

/// (F2) A stale legacy mark on a staged fragment — an `Open` session's owned `sidx:` entry, and its
/// unpublished part — is dropped. Then the part publishes (production Complete), the records drain
/// retires its `part:` records, and the object is deleted with the routed `unlink`: GC-only passes
/// past the stale stamp's grace, run before the drain marks, reclaim none of its fragments; passes
/// inside the drain's fresh grace reclaim none; passes past it reclaim all.
#[tokio::test]
async fn f2_a_stale_mark_on_a_staged_fragment_is_dropped_and_never_judged() {
    let rig = Rig::new().await;
    let id = rig.session(2, "photos/staged").await;
    let staged = chunks(2_000, 60, RS63);
    rig.plant_parts(&id, &[(1, staged.clone())]).await;
    let staged_at = positions(&staged);
    assert_eq!(
        staged_at.len(),
        540,
        "past one batch: the delete will route"
    );
    // An owned entry of the same session, in flight.
    let owned_chunk = 2_900;
    let placement = StagedPlacement::new(RS21, vec![0, 1, 2]).unwrap();
    let entry = OwnedEntry::new(id.clone(), T0 + 600_000, placement).to_pending();
    let sidx = sidx_key(&id, part(2), owned_chunk);
    rig.store
        .plant(vec![(sidx.clone(), md::encode(&entry))])
        .await;
    let owned_at = positions(&chunks(owned_chunk, 1, RS21));
    rig.mark(&staged_at, legacy(ANCIENT)).await;
    rig.mark(&owned_at, legacy(ANCIENT)).await;

    rig.repair_until_complete(4).await;
    rig.assert_unmarked(&staged_at, "a staged part's stale mark")
        .await;
    rig.assert_unmarked(&owned_at, "an owned entry's stale mark")
        .await;
    rig.store.remove(&sidx).await;

    // Publish, retire the published part's records, then delete through the routed path.
    rig.complete(&id, "photos/staged", &[1], 8).await;
    let records = records_obligation(&rig, &id).await;
    rig.drain_until_gone(&records, 4).await;
    assert!(rig.store.value(&part_key(&id, part(1))).await.is_none());
    let version = rig.inode(8).await.version;
    rig.unlink("photos/staged").await;
    rig.assert_retired_through_the_drain(
        &generation_key(8, version),
        &format!("g:8:{version}"),
        &staged_at,
    )
    .await;
}

/// The one `retire:records:` obligation a Complete of `id` installed for its published parts, read
/// from the session's own bounded range.
async fn records_obligation(rig: &Rig, id: &UploadId) -> Vec<u8> {
    let range = wyrd_core::multipart::retire_session_range(RetireMode::Records, id);
    let found = rig.store.under(&range).await;
    assert_eq!(found.len(), 1, "one records obligation: {found:?}");
    found.into_keys().next().unwrap()
}

/// (F3) The 2026-09-12 trap: the published part's records still name the fragments (its number in
/// a pending `retire:records:` `{parts}` obligation — the records drain has not reached it), no
/// committed map does (the inline `unlink` deleted the object and wrote the mark). The cleanup
/// keeps that mark byte-identical; after the records drain retires the part records, GC reclaims
/// the fragments after that mark's grace. No fragment is left with no mark and no reference.
#[tokio::test]
async fn f3_the_trap_mark_on_a_published_part_is_kept_byte_identical() {
    let rig = Rig::new().await;
    let id = rig.session(3, "photos/trap").await;
    let published = chunks(3_000, 2, RS21);
    rig.plant_parts(&id, &[(1, published.clone())]).await;
    rig.complete(&id, "photos/trap", &[1], 9).await;
    let records = records_obligation(&rig, &id).await;
    assert!(
        rig.store.value(&records).await.is_some(),
        "the flip's obligation"
    );
    // The records drain has not reached it yet.
    rig.store.freeze(&records);

    // The inline delete (gate closed, a small object) unreferences the object and marks it.
    rig.unlink("photos/trap").await;
    let at = positions(&published);
    let evidence = legacy(T0);
    rig.assert_marked_with(&at, &evidence, "the delete's inline marks")
        .await;

    rig.repair_until_complete(4).await;
    rig.assert_marked_with(&at, &evidence, "the trap's mark, byte-identical")
        .await;
    assert!(rig.store.value(&part_key(&id, part(1))).await.is_some());

    // The records drain retires the part records, without marking.
    rig.store.thaw();
    rig.drain_until_gone(&records, 4).await;
    assert!(rig.store.value(&part_key(&id, part(1))).await.is_none());
    rig.assert_marked_with(&at, &evidence, "still the delete's evidence")
        .await;

    rig.clock.advance(GRACE - 1);
    rig.gc_settled().await;
    rig.assert_kept(&at, "inside the delete's grace");
    rig.clock.advance(2);
    rig.gc_settled().await;
    rig.assert_gone(
        &at,
        "after the delete's grace: no fragment left unnamed and unmarked",
    );
}

/// (F4) A `reclaiming` mark on a fragment no record names, and a structured mark whose event's
/// `retire:bytes:` obligation is present and names its fragment (a drain mid-retirement: it marked
/// that position and has not reached its end), are byte-identical after the walk, which completes;
/// a stale mark on a live object beside them is dropped.
#[tokio::test]
async fn f4_reclaiming_and_draining_marks_are_left_byte_identical() {
    let rig = Rig::new().await;
    let live = chunks(4_000, 1, RS21);
    rig.object("photos/f4", 10, &live).await;
    let at = positions(&live);
    // A pending generation obligation whose drain marked its first position and stopped there.
    let retired = chunks(4_500, 1, RS21);
    rig.fleet.store_all(&retired);
    let retired_at = positions(&retired);
    let (draining, value) = generation_obligation(500, 1, &retired);
    rig.store.plant(vec![(draining.clone(), value)]).await;
    rig.store.freeze(&draining);
    // GC's decision on a fragment nothing names any more, not yet finished.
    let unnamed = positions(&chunks(4_700, 1, RS21));

    let reclaiming = encode_orphan_mark(
        &OrphanMark::structured(ANCIENT, "g:77:1")
            .unwrap()
            .into_reclaiming(),
    );
    let mid_drain = structured(T0, "g:500:1");
    rig.mark(&unnamed, reclaiming.clone()).await;
    rig.mark(&retired_at[..1], mid_drain.clone()).await;
    rig.mark(&at, legacy(ANCIENT)).await;

    rig.repair_until_complete(4).await;
    rig.assert_marked_with(&unnamed, &reclaiming, "reclaiming")
        .await;
    rig.assert_marked_with(&retired_at[..1], &mid_drain, "draining")
        .await;
    rig.assert_unmarked(&at, "the stale one beside them").await;
}

/// (F4, a draining mark under a live reference) A stale mark on a fragment of a live 100-chunk
/// object names, by its event, a retirement that is still pending but names **other** fragments —
/// a state no drain writes, which a restore splicing two moments, or damage, can leave. The walk
/// leaves the mark byte-identical but writes no marker while that retirement is pending: once it
/// drains, GC judges the mark by its own old stamp, so a routed delete of the object would expose
/// the fragment before that delete's drain marks it. After the unrelated retirement drains, a lap
/// drops the mark and completes; the routed delete then reclaims nothing before its drain marks,
/// nothing inside their grace, and every fragment after.
#[tokio::test]
async fn f4_a_draining_mark_on_a_live_fragment_withholds_the_marker_until_its_retirement_drains() {
    let rig = Rig::new().await;
    let object = hundred_chunk_object(&rig, "photos/f4-spliced", 702, 70_200).await;
    let at = positions(&object);
    let unrelated = chunks(70_900, 1, RS21);
    rig.fleet.store_all(&unrelated);
    let (pending, value) = generation_obligation(703, 1, &unrelated);
    rig.store.plant(vec![(pending.clone(), value)]).await;
    rig.store.freeze(&pending);
    let spliced = at[0];
    let stale = structured(ANCIENT, "g:703:1");
    rig.mark(&[spliced], stale.clone()).await;
    let named = String::from_utf8(orphan_key(spliced.0, spliced.1)).unwrap();

    for _ in 0..2 {
        assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Blocked);
    }
    assert_eq!(rig.status().await, CleanupStatus::Blocked { record: named });
    assert!(rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none());
    rig.assert_marked_with(
        &[spliced],
        &stale,
        "byte-identical while its retirement drains",
    )
    .await;

    rig.store.thaw();
    rig.drain_until_gone(&pending, 6).await;
    rig.repair_until_complete(4).await;
    rig.assert_unmarked(&[spliced], "dropped once its retirement is gone")
        .await;

    rig.unlink("photos/f4-spliced").await;
    rig.assert_retired_through_the_drain(&generation_key(702, 1), "g:702:1", &at)
        .await;
}

/// (F4, the closed-gate cycle) A production Complete over an existing object installs the
/// superseded generation's obligation; a restored image then carries a `reclaiming` mark — GC's
/// decision, not finished — on one of that generation's fragments, which only the obligation
/// names, and the production post-restore pass runs. The drain waits for GC to finish that reclaim
/// before it marks the position, and GC finishes no reclaim while the gate is closed, so the walk
/// must not block on it. Repair and GC passes alternate: the walk completes, GC finishes the
/// reclaim, the drain marks the rest and goes, and the rest of the generation's fragments go only
/// past the drain's grace.
#[tokio::test]
async fn f4_a_reclaiming_mark_only_a_retirement_names_lets_drain_gc_and_walk_finish() {
    let rig = Rig::new().await;
    let prior = chunks(6_800, 2, RS21);
    rig.object("photos/cycle", 68, &prior).await;
    let at = positions(&prior);
    let id = rig.session(68, "photos/cycle").await;
    rig.plant_parts(&id, &[(1, chunks(6_900, 1, RS21))]).await;
    rig.complete(&id, "photos/cycle", &[1], 168).await;
    let superseded = generation_key(68, 1);
    assert!(
        rig.store.value(&superseded).await.is_some(),
        "the flip's obligation"
    );
    // The second chunk's first fragment: the drain marks the first chunk before it meets it.
    let doomed = at[3];
    let reclaiming = encode_orphan_mark(&OrphanMark::legacy(ANCIENT).into_reclaiming());
    rig.mark(&[doomed], reclaiming).await;
    rig.restore_pass().await;
    assert_ne!(rig.status().await, CleanupStatus::Complete);

    let mut cycle = Vec::new();
    for _ in 0..6 {
        if rig.store.value(&superseded).await.is_none() {
            break;
        }
        cycle.push(rig.repair().await.expect("the pass"));
        cycle.push(rig.gc().await);
    }
    assert!(
        rig.store.value(&superseded).await.is_none(),
        "the obligation drained: {cycle:?}"
    );
    assert_eq!(rig.status().await, CleanupStatus::Complete);
    assert!(
        !rig.fleet.holds(doomed.0, doomed.1),
        "GC finished the reclaim it had decided"
    );
    let rest: Vec<_> = at.iter().copied().filter(|&p| p != doomed).collect();
    rig.assert_kept(&rest, "inside the drain's grace");
    rig.clock.advance(GRACE + 1);
    rig.gc_settled().await;
    rig.assert_gone(&rest, "past the drain's grace");
    rig.assert_unmarked(&at, "no mark left").await;
}

/// (F4, through a restore) A restored image carries a `reclaiming` mark on a fragment of a live
/// 100-chunk object. The post-restore pass runs, then the cleanup, then the object's delete, then
/// GC passes with the clock past the old mark's stamp and no drain run: none of the object's
/// fragments goes. The cleanup **blocked**, naming the mark, which it left byte-identical — no
/// writer may replace a `reclaiming` mark, so were the routing on, the delete would leave GC to
/// finish that reclaim at once, with no grace at all. So the delete stayed inline, its fresh
/// marks covering that fragment too, and the bytes go only after their grace. Second leg: an
/// operator repairs the mark instead; the lap completes, the delete routes, and nothing goes
/// before the drain's marks or inside their grace.
#[tokio::test]
async fn f4_a_reclaiming_mark_on_a_live_fragment_never_opens_the_routing() {
    for repaired in [false, true] {
        let rig = Rig::new().await;
        rig.repair_until_complete(2).await;
        let object = hundred_chunk_object(&rig, "photos/f4-live", 40, 40_000).await;
        let at = positions(&object);

        rig.restore_pass().await;
        let reclaiming = encode_orphan_mark(
            &OrphanMark::structured(ANCIENT, "g:40:0")
                .unwrap()
                .into_reclaiming(),
        );
        let doomed = at[0];
        rig.mark(&[doomed], reclaiming.clone()).await;
        let named = String::from_utf8(orphan_key(doomed.0, doomed.1)).unwrap();

        let mut answers = Vec::new();
        for _ in 0..2 {
            answers.push(rig.repair().await.expect("the pass"));
        }

        if !repaired {
            let fresh = legacy(rig.clock.now_millis());
            rig.unlink("photos/f4-live").await;
            rig.clock.advance(GRACE - 1);
            for _ in 0..3 {
                rig.gc().await;
            }
            rig.assert_kept(&at, "past the old mark's stamp, before any drain");
            assert_eq!(answers, [Reconciled::Blocked, Reconciled::Blocked]);
            assert_eq!(rig.status().await, CleanupStatus::Blocked { record: named });
            assert!(
                rig.store.under(RETIRE_BYTES_PREFIX).await.is_empty(),
                "no obligation while the lap is blocked"
            );
            rig.assert_marked_with(&at, &fresh, "inline marks, over the reclaiming one too")
                .await;
            rig.repair_until_complete(4).await;
            rig.clock.advance(2);
            rig.gc_settled().await;
            rig.assert_gone(&at, "after the delete's grace");
        } else {
            assert_eq!(answers, [Reconciled::Blocked, Reconciled::Blocked]);
            assert_eq!(rig.status().await, CleanupStatus::Blocked { record: named });
            rig.assert_marked_with(&[doomed], &reclaiming, "byte-identical")
                .await;
            rig.store.remove(&orphan_key(doomed.0, doomed.1)).await;
            rig.repair_until_complete(4).await;
            let marker = rig.store.value(ORPHAN_IDENTITY_KEY).await.unwrap();
            assert_eq!(&marker[..], b"{\"fence_generation\":1}");
            rig.unlink("photos/f4-live").await;
            rig.assert_retired_through_the_drain(&generation_key(40, 1), "g:40:1", &at)
                .await;
        }
    }
}

/// (F3, the session damaged) The trap of F3, then the published session's `mpu:` record is
/// damaged: whether its parts were published can no longer be read. The cleanup keeps the trap's
/// marks byte-identical and **blocks**, naming the session, so no marker is written; once the
/// record is repaired a lap completes with the marks still kept, the records drain retires the
/// part records, and GC reclaims the fragments only after the delete's grace — never left with no
/// mark and no reference.
#[tokio::test]
async fn f3_a_damaged_session_blocks_the_marker_and_keeps_the_trap() {
    let rig = Rig::new().await;
    let id = rig.session(33, "photos/trap-damaged").await;
    let published = chunks(33_000, 2, RS21);
    rig.plant_parts(&id, &[(1, published.clone())]).await;
    rig.complete(&id, "photos/trap-damaged", &[1], 33).await;
    let records = records_obligation(&rig, &id).await;
    rig.store.freeze(&records);
    rig.unlink("photos/trap-damaged").await;
    let at = positions(&published);
    let evidence = legacy(T0);
    rig.assert_marked_with(&at, &evidence, "the delete's inline marks")
        .await;

    let session = mpu_key(&id);
    let original = rig.store.value(&session).await.expect("the session");
    rig.store
        .plant(vec![(session.clone(), Bytes::from_static(b"{\"torn\":"))])
        .await;
    for _ in 0..3 {
        assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Blocked);
    }
    let named = String::from_utf8(session.clone()).unwrap();
    assert_eq!(rig.status().await, CleanupStatus::Blocked { record: named });
    assert!(rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none());
    rig.assert_marked_with(&at, &evidence, "the trap, with the session damaged")
        .await;

    rig.store.plant(vec![(session, original)]).await;
    rig.repair_until_complete(4).await;
    rig.assert_marked_with(&at, &evidence, "the trap, once the session reads")
        .await;
    rig.store.thaw();
    rig.drain_until_gone(&records, 4).await;
    assert!(rig.store.value(&part_key(&id, part(1))).await.is_none());
    rig.assert_marked_with(&at, &evidence, "still the delete's evidence")
        .await;
    rig.clock.advance(GRACE - 1);
    rig.gc_settled().await;
    rig.assert_kept(&at, "inside the delete's grace");
    rig.clock.advance(2);
    rig.gc_settled().await;
    rig.assert_gone(&at, "after the delete's grace");
}

/// (F1, segmented and named twice) Right after a production Complete publishes a segmented object
/// — its records drain not yet run — the committed inode (through its `seg:` records) and the
/// published parts' records both name its fragments. A stale mark there is dropped: the committed
/// map outranks the published part, and the drop is pinned on the inode and the `seg:` record.
/// Then the routed delete retires the object through the drain.
#[tokio::test]
async fn f1_a_stale_mark_on_a_segmented_object_its_parts_also_name_is_dropped() {
    let rig = Rig::new().await;
    let id = rig.session(34, "photos/f1-seg").await;
    let one = chunks(34_000, 90, RS21);
    let two = chunks(34_200, 90, RS21);
    rig.plant_parts(&id, &[(1, one.clone()), (2, two.clone())])
        .await;
    rig.complete(&id, "photos/f1-seg", &[1, 2], 34).await;
    let root = rig.inode(34).await;
    assert!(root.chunk_map.is_segmented());
    let records = records_obligation(&rig, &id).await;
    rig.store.freeze(&records);
    let stale_at: Vec<_> = positions(&one[..3])
        .into_iter()
        .chain(positions(&two[87..]))
        .collect();
    rig.mark(&stale_at, legacy(ANCIENT)).await;

    rig.repair_until_complete(4).await;
    rig.assert_unmarked(&stale_at, "committed outranks published, segmented too")
        .await;
    assert!(rig.store.value(&part_key(&id, part(1))).await.is_some());

    rig.store.thaw();
    rig.drain_until_gone(&records, 4).await;
    rig.unlink("photos/f1-seg").await;
    let at: Vec<_> = positions(&one).into_iter().chain(positions(&two)).collect();
    rig.assert_retired_through_the_drain(
        &generation_key(34, root.version),
        &format!("g:34:{}", root.version),
        &at,
    )
    .await;
}

/// (Bounded reads) Two lost drops at the read budget's edge. `n` structured marks whose event names
/// an obligation that is gone — one keyed read each, all of them before it in the ledger — leave
/// the pass `4096 - n` reads when it meets a stale mark on an `Open` session's part, whose drop the
/// store refuses twice. Every pass completes without fault, and the mark goes on a later pass:
/// with 7 reads left the window stops before it, with 8 it spends exactly those.
#[tokio::test]
async fn f_two_lost_drops_at_the_read_budget_never_run_it_out() {
    for n in [4_089_u64, 4_088] {
        let rig = Rig::new().await;
        let id = rig.session(35, "photos/budget").await;
        let staged = chunks(2_000_000, 1, RS63);
        rig.plant_parts(&id, &[(1, staged.clone())]).await;
        let target = positions(&staged)[8];
        assert_eq!(target.0, 8, "after every mark on server 0 in key order");
        rig.mark(&[target], structured(ANCIENT, "g:999:1")).await;
        let ahead: Vec<_> = (0..n)
            .map(|i| {
                let frag = FragmentId {
                    chunk: 1_000_000 + ChunkId::from(i),
                    index: 0,
                };
                (
                    orphan_key(0, frag),
                    structured(ANCIENT, &format!("g:{i}:1")),
                )
            })
            .collect();
        rig.store.plant(ahead).await;
        rig.store
            .refused
            .lock()
            .unwrap()
            .insert(orphan_key(target.0, target.1), 2);

        rig.repair_until_complete(4).await;
        rig.assert_unmarked(
            &[target],
            "the staged part's stale mark, dropped on a later pass",
        )
        .await;
        assert_eq!(
            rig.store.refused.lock().unwrap().values().sum::<usize>(),
            0,
            "both drops were refused first"
        );
    }
}

/// (Steady deletes, between the cleanup's reads) Every time the cleanup reads the ledger from its
/// head, a production inline `unlink` of another small object lands first, and a stale mark sits
/// on a live object. The marker is still written within three passes, with deletes still landing,
/// and the deleted objects' bytes go only after their own marks' grace.
#[tokio::test]
async fn f_steady_deletes_at_the_ledger_head_do_not_stall_the_marker() {
    let rig = Rig::new().await;
    let live = chunks(36_000, 2, RS21);
    rig.object("photos/steady-live", 36, &live).await;
    rig.mark(&positions(&live), legacy(ANCIENT)).await;
    let mut deleted = Vec::new();
    for i in 0..10_u64 {
        let object = chunks(36_100 + ChunkId::from(i), 1, RS21);
        let name = format!("photos/steady-{i}");
        rig.object(&name, 100 + i, &object).await;
        deleted.extend(positions(&object));
        rig.store
            .at_ledger_head
            .lock()
            .unwrap()
            .push_back((name, rig.clock.now_millis()));
    }

    rig.repair_until_complete(3).await;
    let landed = rig.store.unlinks_ran.load(Ordering::SeqCst);
    assert!(
        (1..10).contains(&landed),
        "deletes landed and were still landing: {landed}"
    );
    rig.assert_unmarked(&positions(&live), "the live object's stale mark")
        .await;

    rig.store.at_ledger_head.lock().unwrap().clear();
    for i in landed as u64..10 {
        rig.unlink(&format!("photos/steady-{i}")).await;
    }
    rig.clock.advance(GRACE - 1);
    rig.gc_settled().await;
    rig.assert_kept(&deleted, "inside the deletes' grace");
    rig.clock.advance(2);
    rig.gc_settled().await;
    rig.assert_gone(&deleted, "after the deletes' grace");
    rig.assert_kept(&positions(&live), "live");
}

/// (Steady deletes, inside the cleanup's reading) Four live objects carry stale marks, and every
/// pass one of them is deleted inline right after the cleanup has read the committed objects —
/// the last first. Each delete makes one drop's reading stale: the window stops before that mark,
/// never past it, and the lap goes on from there. The marker is written within four passes, one
/// delete landing in each; every deleted object's own inline marks survive the walk, and its bytes
/// go only after their grace.
#[tokio::test]
async fn f_a_stale_judgement_stops_the_window_and_keeps_the_lap() {
    let rig = Rig::new().await;
    let mut objects = Vec::new();
    for i in 0..4_u64 {
        let object = chunks(37_000 + ChunkId::from(i), 1, RS21);
        let name = format!("photos/stale-{i}");
        rig.object(&name, 200 + i, &object).await;
        rig.mark(&positions(&object), legacy(ANCIENT)).await;
        objects.push(positions(&object));
    }
    for i in (0..4_u64).rev() {
        rig.store
            .after_inode_scan
            .lock()
            .unwrap()
            .push_back((format!("photos/stale-{i}"), rig.clock.now_millis()));
    }

    let mut passes = 0;
    while rig.status().await != CleanupStatus::Complete {
        assert!(passes < 4, "the marker within four passes");
        rig.repair().await.expect("the pass");
        passes += 1;
        assert_eq!(
            rig.store.unlinks_ran.load(Ordering::SeqCst),
            passes,
            "one delete landed in every pass"
        );
    }
    let fresh = legacy(T0);
    for at in &objects {
        rig.assert_marked_with(at, &fresh, "each delete's own inline marks")
            .await;
    }
    rig.clock.advance(GRACE - 1);
    rig.gc_settled().await;
    for at in &objects {
        rig.assert_kept(at, "inside the deletes' grace");
    }
    rig.clock.advance(2);
    rig.gc_settled().await;
    for at in &objects {
        rig.assert_gone(at, "after the deletes' grace");
    }
}

/// (A lap across passes) More marks than one window holds: the lap spans two passes and writes no
/// marker after the first. A restore lands between them (the production post-restore pass opens a
/// new generation) and brings back a stale mark on a live object **behind** where the lap stood:
/// the lap begun under the old generation is discarded, a fresh one walks from the head, and the
/// marker it writes names the new generation only after that mark is dropped.
#[tokio::test]
async fn f7_a_restore_mid_lap_discards_the_lap() {
    let rig = Rig::new().await;
    let live = chunks(38_000, 1, RS21);
    rig.object("photos/mid-lap", 38, &live).await;
    let strays: Vec<_> = (0..16_484_u64)
        .map(|i| {
            let frag = FragmentId {
                chunk: 3_000_000 + ChunkId::from(i),
                index: 0,
            };
            (orphan_key(1, frag), legacy(ANCIENT))
        })
        .collect();
    rig.store.plant(strays).await;

    assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Partial);
    assert_eq!(rig.status().await, CleanupStatus::Walking);
    assert!(rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none());

    rig.restore_pass().await;
    let behind: Vec<_> = positions(&live)
        .into_iter()
        .filter(|&(dserver, _)| dserver == 0)
        .collect();
    assert_eq!(behind.len(), 1);
    rig.mark(&behind, legacy(ANCIENT)).await;

    rig.repair_until_complete(4).await;
    let marker = rig.store.value(ORPHAN_IDENTITY_KEY).await.unwrap();
    assert_eq!(&marker[..], b"{\"fence_generation\":1}");
    rig.assert_unmarked(
        &behind,
        "a mark the restore brought back behind the old lap",
    )
    .await;
}

/// (The expired-lease input) A 100-chunk object written through the production write path whose
/// writer never released its leases — a crash between its commit and its release — is deleted
/// through the routed `unlink`: unreferenced, unmarked, its chunks' leases expired. GC passes
/// under `ExpiredPendingPolicy::Reclaim`, before the drain marks, reclaim none of its fragments;
/// passes inside the drain's grace reclaim none; passes past it reclaim all.
#[tokio::test]
async fn a2_a_routed_delete_of_unreleased_leases_waits_for_the_drain() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let object = chunks(39_000, 100, RS63);
    let plan = WritePlan {
        chunks: object
            .iter()
            .map(|c| PlannedChunk {
                id: c.id,
                scheme: c.scheme,
                len: c.len,
                fragments: (0..9).map(|i| (i, Bytes::from_static(b"l"))).collect(),
                placement: c.placement.clone(),
            })
            .collect(),
        size: 100 * CHUNK_LEN,
        object_meta: ObjectMeta::default(),
    };
    let now = rig.clock.now_millis();
    write::intent(&rig.store, &plan, now + 1_000).await.unwrap();
    write::write_fragments(&rig.fleet, &plan).await.unwrap();
    let created = write::commit_create(&rig.store, ROOT, "photos/leased", 39, &plan, now).await;
    assert_eq!(created.unwrap(), CommitOutcome::Committed);
    // No `write::release`: the writer died here, its `pending:` entries left behind.
    assert!(rig
        .store
        .value(&md::pending_key(object[0].id))
        .await
        .is_some());
    let at = positions(&object);

    rig.unlink("photos/leased").await;
    let key = generation_key(39, 1);
    assert!(rig.store.value(&key).await.is_some(), "routed");
    rig.assert_unmarked(&at, "no inline mark").await;

    rig.clock.advance(10 * GRACE);
    for _ in 0..3 {
        rig.gc_with(ExpiredPendingPolicy::Reclaim).await;
    }
    rig.assert_kept(&at, "expired leases, before the drain marks");

    rig.drain_until_gone(&key, 6).await;
    rig.clock.advance(GRACE - 1);
    for _ in 0..3 {
        rig.gc_with(ExpiredPendingPolicy::Reclaim).await;
    }
    rig.assert_kept(&at, "expired leases, inside the drain's grace");
    rig.clock.advance(2);
    for _ in 0..8 {
        if rig.gc_with(ExpiredPendingPolicy::Reclaim).await != Reconciled::Changed {
            break;
        }
    }
    rig.assert_gone(&at, "past the drain's grace");
}

/// (F, the pending-obligation row) A stale mark on a fragment that only a pending `retire:bytes:`
/// obligation names — here a production Complete over an existing object, before any marker,
/// installing the superseded generation's obligation — is dropped: left, GC's keyed lookup would
/// judge the fragment by that old stamp before the drain marks it. Then nothing is reclaimed
/// before the drain marks, nothing inside its grace, and everything after it.
#[tokio::test]
async fn f_a_stale_mark_on_a_fragment_only_a_pending_obligation_names_is_dropped() {
    let rig = Rig::new().await;
    let prior = chunks(5_800, 2, RS21);
    rig.object("photos/superseded", 18, &prior).await;
    let at = positions(&prior);
    rig.mark(&at, legacy(ANCIENT)).await;
    let id = rig.session(18, "photos/superseded").await;
    rig.plant_parts(&id, &[(1, chunks(5_900, 1, RS21))]).await;
    rig.complete(&id, "photos/superseded", &[1], 99).await;
    let superseded = generation_key(18, 1);
    assert!(
        rig.store.value(&superseded).await.is_some(),
        "the flip's obligation"
    );
    // The drain has not reached it yet.
    rig.store.freeze(&superseded);

    rig.repair_until_complete(4).await;
    rig.assert_unmarked(&at, "a stale mark on a fragment a pending obligation names")
        .await;
    rig.store.thaw();
    rig.assert_retired_through_the_drain(&superseded, "g:18:1", &at)
        .await;
}

/// (F1, a segmented retirement) A segmented object (production Complete, 180 chunks) is deleted
/// through the routed `unlink` once the gate is open, its drain not yet started. A restore then
/// closes the gate — the production post-restore pass opens a new generation, and marks every
/// fragment no committed map names, the retired object's among them — and brings back a stale mark
/// on one of them. Only the segmented obligation names those fragments, through the retired
/// group's `seg:` records. The walk drops every one of those marks, each pinned on the obligation
/// and its `seg:` record, and completes; the drain then retires every fragment, none before its
/// marks or inside their grace.
#[tokio::test]
async fn f1_a_stale_mark_only_a_segmented_retirement_names_is_dropped() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let id = rig.session(82, "photos/x3").await;
    let one = chunks(82_000, 90, RS21);
    let two = chunks(82_200, 90, RS21);
    rig.plant_parts(&id, &[(1, one.clone()), (2, two.clone())])
        .await;
    rig.complete(&id, "photos/x3", &[1, 2], 82).await;
    let root = rig.inode(82).await;
    assert!(root.chunk_map.is_segmented());
    let records = records_obligation(&rig, &id).await;
    rig.drain_until_gone(&records, 4).await;
    rig.unlink("photos/x3").await;
    let retired = generation_key(82, root.version);
    assert!(rig.store.value(&retired).await.is_some(), "routed");
    rig.store.freeze(&retired);

    rig.restore_pass().await;
    assert_ne!(rig.status().await, CleanupStatus::Complete, "closed again");
    let at: Vec<_> = positions(&one).into_iter().chain(positions(&two)).collect();
    let stale = at[400];
    rig.mark(&[stale], legacy(ANCIENT)).await;

    rig.repair_until_complete(6).await;
    rig.assert_unmarked(&at, "marks only a segmented retirement names")
        .await;
    rig.store.thaw();
    rig.assert_retired_through_the_drain(&retired, &format!("g:82:{}", root.version), &at)
        .await;
}

/// (An untrusted placement never drops a mark) A committed RS(2,1) chunk whose placement lists two
/// servers for three fragments — GC protects every fragment bearing its id — and an old copy of its
/// fragment 0 on server 5 with its legacy mark, as a completed move leaves one. Whether the map
/// names that copy cannot be read from such a placement, so the walk leaves the mark and blocks,
/// naming the inode: dropped, the copy would keep no mark and no record once the placement is
/// repaired, for good. After an operator repairs the placement, a lap finds nothing naming the
/// copy and completes, and GC reclaims the copy past its mark's grace; the object's own fragments
/// stay.
#[tokio::test]
async fn f_an_untrusted_committed_placement_never_drops_a_mark() {
    let rig = Rig::new().await;
    let mut short = chunks(80_000, 1, RS21);
    short[0].placement = vec![0, 1];
    let record = rig.object("photos/untrusted", 80, &short).await;
    let live = positions(&chunks(80_000, 1, RS21));
    let copy = (
        5,
        FragmentId {
            chunk: 80_000,
            index: 0,
        },
    );
    rig.fleet
        .put(copy.0, copy.1, Bytes::from_static(b"old copy"));
    rig.mark(&[copy], legacy(ANCIENT)).await;
    let inode = String::from_utf8(md::inode_key(80)).unwrap();

    for _ in 0..2 {
        assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Blocked);
    }
    assert_eq!(rig.status().await, CleanupStatus::Blocked { record: inode });
    rig.assert_marked_with(
        &[copy],
        &legacy(ANCIENT),
        "left: the placement proves nothing",
    )
    .await;

    let repaired = InodeRecord {
        chunk_map: ChunkMap::Flat(chunks(80_000, 1, RS21)),
        ..record
    };
    rig.store
        .plant(vec![(md::inode_key(80), md::encode(&repaired))])
        .await;
    rig.repair_until_complete(4).await;
    rig.assert_marked_with(&[copy], &legacy(ANCIENT), "named by nothing now")
        .await;
    rig.gc_settled().await;
    rig.assert_gone(&[copy], "the old copy, reclaimed by its mark");
    rig.assert_kept(&live, "the object's own fragments");
}

/// (An undecodable owned entry never drops a mark) An `Open` session's `sidx:` entry whose value
/// will not decode, under a key that names its chunk — GC's staged class protects that chunk
/// whole. A stale mark on one of the chunk's fragments is left and the walk blocks, naming the
/// entry; once the entry decodes, placing the fragment, the mark is dropped as any staged one.
#[tokio::test]
async fn f_an_undecodable_owned_entry_never_drops_a_mark() {
    let rig = Rig::new().await;
    let id = rig.session(81, "photos/untrusted-staged").await;
    let owned = chunks(81_000, 1, RS21);
    rig.fleet.store_all(&owned);
    let at = positions(&owned);
    let sidx = sidx_key(&id, part(1), 81_000);
    rig.store
        .plant(vec![(sidx.clone(), Bytes::from_static(b"{\"torn\":"))])
        .await;
    rig.mark(&at[..1], legacy(ANCIENT)).await;
    for _ in 0..2 {
        assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Blocked);
    }
    let entry_name = String::from_utf8(sidx.clone()).unwrap();
    assert_eq!(
        rig.status().await,
        CleanupStatus::Blocked { record: entry_name }
    );
    rig.assert_marked_with(&at[..1], &legacy(ANCIENT), "left: the entry proves nothing")
        .await;
    let placement = StagedPlacement::new(RS21, vec![0, 1, 2]).unwrap();
    let entry = OwnedEntry::new(id.clone(), T0 + 600_000, placement).to_pending();
    rig.store.plant(vec![(sidx, md::encode(&entry))]).await;
    rig.repair_until_complete(4).await;
    rig.assert_unmarked(&at[..1], "a staged entry's stale mark, once it decodes")
        .await;
}

/// (A damaged walk cursor) The cleanup's own progress record is damaged to `{"after":"ff"}` —
/// canonical JSON and lowercase hex, but a key past every `orphan:` key — while a stale mark sits on
/// a live 100-chunk object. Resumed from, it would end the lap at once, every mark unjudged, and
/// write the marker over that stale mark. It is refused as damage: the pass begins a fresh lap from
/// the head, drops the mark, and only then writes the marker. The object's routed delete then
/// reclaims nothing before its drain marks, nothing inside their grace, and every fragment after.
#[tokio::test]
async fn f6_a_damaged_walk_cursor_starts_a_fresh_lap() {
    let rig = Rig::new().await;
    let object = hundred_chunk_object(&rig, "photos/f6-cursor", 71, 71_000).await;
    let at = positions(&object);
    rig.mark(&at[..1], legacy(ANCIENT)).await;
    rig.store
        .plant(vec![(
            CLEANUP_WALK_KEY.to_vec(),
            Bytes::from_static(br#"{"after":"ff"}"#),
        )])
        .await;

    assert_eq!(rig.repair().await.expect("the pass"), Reconciled::Changed);
    assert_eq!(rig.status().await, CleanupStatus::Complete);
    rig.assert_unmarked(&at[..1], "judged on a fresh lap").await;
    rig.unlink("photos/f6-cursor").await;
    rig.assert_retired_through_the_drain(&generation_key(71, 1), "g:71:1", &at)
        .await;
}

/// (F5) A wrapper pauses the cleanup between its reads and its commit of an F1 drop and runs the
/// production inline `unlink` of that object — at the very stamp the stale mark carries, so only
/// the inode's bytes can tell the two marks apart. The drop fails, the unlink's mark survives, and
/// the fragments go only after its grace. Then the same with an overwrite.
#[tokio::test]
async fn f5_a_drop_racing_unlink_or_overwrite_fails_and_the_writers_mark_survives() {
    for overwrite in [false, true] {
        let rig = Rig::new().await;
        let live = chunks(5_000, 2, RS21);
        let prior = rig.object("photos/f5", 11, &live).await;
        let at = positions(&live);
        let stamp = rig.clock.now_millis();
        rig.mark(&at, legacy(stamp)).await;

        let race = if overwrite {
            let next = chunks(5_500, 1, RS21);
            let plan = WritePlan {
                chunks: next
                    .iter()
                    .map(|c| PlannedChunk {
                        id: c.id,
                        scheme: c.scheme,
                        len: c.len,
                        fragments: (0..3).map(|i| (i, Bytes::from_static(b"f"))).collect(),
                        placement: c.placement.clone(),
                    })
                    .collect(),
                size: CHUNK_LEN,
                object_meta: ObjectMeta::default(),
            };
            write::intent(&rig.store.inner, &plan, T0 + 600_000)
                .await
                .unwrap();
            write::write_fragments(&rig.fleet, &plan).await.unwrap();
            Race::Overwrite {
                inode: 11,
                prior: Box::new(prior),
                plan: Box::new(plan),
                at: stamp,
            }
        } else {
            Race::Unlink {
                name: "photos/f5".into(),
                at: stamp,
            }
        };
        let target = orphan_key(at[0].0, at[0].1);
        *rig.store.race.lock().unwrap() = Some((target, race));

        rig.repair_until_complete(4).await;
        assert!(rig.store.raced.load(Ordering::SeqCst), "the race ran");
        rig.assert_marked_with(&at, &legacy(stamp), "the writer's marks survive")
            .await;

        rig.clock.advance(GRACE - 1);
        rig.gc_settled().await;
        rig.assert_kept(&at, "inside the writer's grace");
        rig.clock.advance(2);
        rig.gc_settled().await;
        rig.assert_gone(&at, "after the writer's grace");
    }
}

/// (F5, racing a Complete) A stale mark on an `Open` session's part is judged a drop; a wrapper
/// runs the production Complete of that session between the cleanup's reads and its commit. The
/// drop fails on the session record it required, and its reading is stale — the part is
/// published now — so the window stops **before** the mark rather than passing it: the next pass
/// judges it again, the committed object now naming the fragment, and drops it. Skipped instead,
/// the mark would survive into the published object, to be judged by a later routed delete.
#[tokio::test]
async fn f5_a_drop_racing_a_complete_is_judged_again_never_skipped() {
    let rig = Rig::new().await;
    let id = rig.session(41, "photos/f5-complete").await;
    let staged = chunks(41_000, 2, RS21);
    rig.plant_parts(&id, &[(1, staged.clone())]).await;
    let at = positions(&staged);
    rig.mark(&at, legacy(ANCIENT)).await;
    let race = Race::Complete {
        upload: id.clone(),
        object: "photos/f5-complete".into(),
        named: vec![1],
        inode: 41,
    };
    *rig.store.race.lock().unwrap() = Some((orphan_key(at[0].0, at[0].1), race));

    rig.repair_until_complete(4).await;
    assert!(rig.store.raced.load(Ordering::SeqCst), "the race ran");
    assert!(
        rig.inode(41).await.chunk_map.as_flat().is_some(),
        "published"
    );
    rig.assert_unmarked(&at, "judged again once published, and dropped")
        .await;
}

/// The damage (F6) one fixture plants, and how to repair it.
enum Damage {
    Inode,
    Segment,
    Part,
}

/// (F6) A walk that meets an unreadable inode, `seg:` record or `part:` record reaches the end of
/// the ledger and writes no marker: the pass answers `Blocked`, the cleanup's status names the
/// record, and a 100-chunk delete still writes its marks inline. After the record is repaired, a
/// later walk completes and writes the marker, and only then does the delete route.
#[tokio::test]
async fn f6_an_unreadable_record_blocks_the_marker_until_it_is_repaired() {
    for damage in [Damage::Inode, Damage::Segment, Damage::Part] {
        let rig = Rig::new().await;
        let stale = chunks(6_000, 1, RS21);
        rig.object("photos/f6-live", 12, &stale).await;
        rig.mark(&positions(&stale), legacy(ANCIENT)).await;

        let (damaged, original) = match damage {
            Damage::Inode => {
                let key = md::inode_key(12);
                (key.clone(), rig.store.value(&key).await.unwrap())
            }
            Damage::Segment => {
                let id = rig.session(6, "photos/f6-seg").await;
                let parts = vec![(1, chunks(6_100, 90, RS21)), (2, chunks(6_300, 90, RS21))];
                rig.plant_parts(&id, &parts).await;
                rig.complete(&id, "photos/f6-seg", &[1, 2], 13).await;
                let ChunkMap::Segmented(map) = rig.inode(13).await.chunk_map else {
                    panic!("segmented");
                };
                let key = seg_key(map.group(), 1).unwrap();
                (key.clone(), rig.store.value(&key).await.unwrap())
            }
            Damage::Part => {
                let id = rig.session(7, "photos/f6-part").await;
                rig.plant_parts(&id, &[(1, chunks(6_500, 1, RS21))]).await;
                let key = part_key(&id, part(1));
                (key.clone(), rig.store.value(&key).await.unwrap())
            }
        };
        rig.store
            .plant(vec![(damaged.clone(), Bytes::from_static(b"{\"torn\":"))])
            .await;

        let answer = rig.repair().await.expect("the pass");
        assert_eq!(answer, Reconciled::Blocked);
        let named = String::from_utf8(damaged.clone()).unwrap();
        assert_eq!(rig.status().await, CleanupStatus::Blocked { record: named });
        assert!(rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none());
        for _ in 0..2 {
            rig.repair().await.expect("the pass");
        }
        assert!(
            rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none(),
            "still none"
        );
        let inline = hundred_chunk_object(&rig, "photos/f6-a", 14, 7_000).await;
        rig.unlink("photos/f6-a").await;
        rig.assert_marked_with(&positions(&inline), &legacy(T0), "the routing stays off")
            .await;

        rig.store.plant(vec![(damaged, original)]).await;
        rig.repair_until_complete(4).await;
        rig.assert_unmarked(&positions(&stale), "the stale mark, once a lap completes")
            .await;
        let routed = hundred_chunk_object(&rig, "photos/f6-b", 15, 8_000).await;
        rig.unlink("photos/f6-b").await;
        rig.assert_unmarked(&positions(&routed), "routed").await;
        assert!(rig.store.value(&generation_key(15, 1)).await.is_some());
    }
}

/// (F6) A walk interrupted mid-way — a ledger page after the head fails — writes no marker; a
/// later pass completes it.
#[tokio::test]
async fn f6_an_interrupted_walk_writes_no_marker() {
    let rig = Rig::new().await;
    // More marks than one page of the walk (1,024), on fragments no record names.
    let strays = chunks(10_000, 400, RS21);
    rig.mark(&positions(&strays), legacy(ANCIENT)).await;
    rig.store.fail_ledger_page.store(true, Ordering::SeqCst);
    assert!(rig.repair().await.is_err(), "the pass fails mid-walk");
    assert!(rig.store.value(ORPHAN_IDENTITY_KEY).await.is_none());
    assert_ne!(rig.status().await, CleanupStatus::Complete);
    rig.repair_until_complete(4).await;
}

/// (F7) A marker written under one restore-fence generation reads as not complete once the
/// production post-restore pass has opened a new one. With stale marks planted as a restored image
/// carries them — on a live object, and on a fragment no record names — the routing and GC's keyed
/// lookup stay off and GC treats every mark as protecting, until the cleanup walks again and
/// writes a marker for the new generation.
#[tokio::test]
async fn f7_a_restore_turns_the_routing_off_until_the_cleanup_walks_again() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let before = rig.store.value(ORPHAN_IDENTITY_KEY).await.expect("marker");
    rig.restore_pass().await;
    assert!(
        rig.store.value(MPUFENCE_KEY).await.is_some(),
        "a generation opened"
    );
    assert_eq!(
        rig.store.value(ORPHAN_IDENTITY_KEY).await,
        Some(before),
        "the old marker is still there, and reads as not complete"
    );
    assert_ne!(rig.status().await, CleanupStatus::Complete);

    // The image's stale marks: a drained retirement's on a live 100-chunk object, and a long
    // expired one on a fragment no record names.
    let live = hundred_chunk_object(&rig, "photos/f7", 16, 9_000).await;
    let stale = structured(ANCIENT, "g:16:0");
    rig.mark(&positions(&live), stale.clone()).await;
    let stray = chunks(9_900, 1, RS21);
    rig.fleet.store_all(&stray);
    let stray = positions(&stray);
    rig.mark(&stray, legacy(ANCIENT)).await;

    rig.clock.advance(10 * GRACE);
    for _ in 0..3 {
        rig.gc().await;
    }
    rig.assert_kept(&stray, "every mark protects until the cleanup walks again");

    // The routing is off: this delete writes fresh marks inline over the stale ones.
    let other = hundred_chunk_object(&rig, "photos/f7-b", 17, 9_500).await;
    rig.mark(&positions(&other), stale.clone()).await;
    rig.unlink("photos/f7-b").await;
    let fresh = legacy(rig.clock.now_millis());
    rig.assert_marked_with(&positions(&other), &fresh, "inline")
        .await;
    assert!(rig.store.value(&generation_key(17, 1)).await.is_none());
    for _ in 0..3 {
        rig.gc().await;
    }
    rig.assert_kept(&positions(&other), "inside the inline marks' grace");

    rig.repair_until_complete(4).await;
    let marker = rig.store.value(ORPHAN_IDENTITY_KEY).await.unwrap();
    assert_eq!(&marker[..], b"{\"fence_generation\":1}");
    rig.assert_unmarked(
        &positions(&live),
        "the image's stale marks on a live object",
    )
    .await;
    rig.gc_settled().await;
    rig.assert_gone(
        &stray,
        "reclaimed once the cleanup has walked the restored ledger",
    );
    rig.assert_kept(&positions(&live), "live");

    // Now the routing is on, and the live object's delete keeps its bytes until the drain.
    rig.unlink("photos/f7").await;
    rig.assert_retired_through_the_drain(&generation_key(16, 1), "g:16:1", &positions(&live))
        .await;
}

// ───────────────────── (A) positive reclamation, with the marker ─────────────────────

/// (A1) The tracker's historical case: a committed 30-chunk RS(6,3) object — 270 fragments, 274
/// operations, inside one batch — deleted with `metadata::unlink`; after reconcile steps past the
/// orphan grace every one of its 270 fragments is gone.
#[tokio::test]
async fn a1_the_historical_30_chunk_delete_is_reclaimed_whole() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let object = chunks(20_000, 30, RS63);
    rig.object("photos/a1", 20, &object).await;
    let at = positions(&object);
    assert_eq!(at.len(), 270);
    rig.unlink("photos/a1").await;
    rig.repair().await.expect("the pass");
    rig.clock.advance(GRACE + 1);
    rig.repair().await.expect("the pass");
    rig.gc_settled().await;
    rig.assert_gone(&at, "all 270");
}

/// (A2) A committed flat 100-chunk RS(6,3) object (900 fragments, 904 operations inline) deleted
/// with `metadata::unlink`: before any reconcile step the unlink's own batch installed exactly one
/// `retire:bytes:g:<inode>:<version>` and wrote no mark; the bytes stay through GC passes before the
/// drain and inside its grace, and after it every one of the 900 is gone with no obligation or
/// mark of it left.
#[tokio::test]
async fn a2_a_delete_past_the_batch_budget_retires_through_one_obligation() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let object = hundred_chunk_object(&rig, "photos/a2", 21, 21_000).await;
    rig.unlink("photos/a2").await;
    let obligations = rig.store.under(RETIRE_BYTES_PREFIX).await;
    assert_eq!(
        obligations.keys().cloned().collect::<Vec<_>>(),
        vec![generation_key(21, 1)],
        "exactly one obligation"
    );
    assert!(
        rig.store.under(md::ORPHAN_PREFIX).await.is_empty(),
        "no mark"
    );
    rig.assert_retired_through_the_drain(&generation_key(21, 1), "g:21:1", &positions(&object))
        .await;
}

/// (A2) The boundary, preconditions counted: an 83-chunk RS(4,2) delete is 498 marks, two deletes
/// and two preconditions — 502 operations, past 500 (500 with the preconditions ignored) — so it
/// takes the obligation path with no inline mark; 82 chunks (496) stays inline.
#[tokio::test]
async fn a2_the_boundary_counts_preconditions() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let routed = chunks(22_000, 83, RS42);
    rig.object("photos/a2-83", 22, &routed).await;
    rig.unlink("photos/a2-83").await;
    rig.assert_unmarked(&positions(&routed), "502 operations: no inline mark")
        .await;
    assert!(rig.store.value(&generation_key(22, 1)).await.is_some());

    let inline = chunks(23_000, 82, RS42);
    rig.object("photos/a2-82", 23, &inline).await;
    rig.unlink("photos/a2-82").await;
    rig.assert_marked_with(&positions(&inline), &legacy(T0), "496 operations: inline")
        .await;
    assert!(rig.store.value(&generation_key(23, 1)).await.is_none());

    rig.assert_retired_through_the_drain(&generation_key(22, 1), "g:22:1", &positions(&routed))
        .await;
}

/// (A3) An ordinary overwrite (`write::commit_overwrite`) of a 100-chunk RS(6,3) object: the prior
/// generation's 900 fragments retire through one obligation, as A2's; the new generation is kept.
#[tokio::test]
async fn a3_an_overwrite_past_the_batch_budget_retires_its_prior_generation() {
    let rig = Rig::new().await;
    rig.repair_until_complete(2).await;
    let prior_chunks = hundred_chunk_object(&rig, "photos/a3", 24, 24_000).await;
    let prior = rig.inode(24).await;
    let next = chunks(24_500, 1, RS21);
    let plan = WritePlan {
        chunks: next
            .iter()
            .map(|c| PlannedChunk {
                id: c.id,
                scheme: c.scheme,
                len: c.len,
                fragments: (0..3).map(|i| (i, Bytes::from_static(b"n"))).collect(),
                placement: c.placement.clone(),
            })
            .collect(),
        size: CHUNK_LEN,
        object_meta: ObjectMeta::default(),
    };
    write::intent(&rig.store, &plan, T0 + 600_000)
        .await
        .unwrap();
    write::write_fragments(&rig.fleet, &plan).await.unwrap();
    let now = rig.clock.now_millis();
    let outcome = write::commit_overwrite(&rig.store, 24, &prior, &plan, now).await;
    assert_eq!(outcome.unwrap(), CommitOutcome::Committed);
    write::release(&rig.store, &plan).await.unwrap();
    assert_eq!(
        rig.store
            .under(RETIRE_BYTES_PREFIX)
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![generation_key(24, 1)],
        "exactly one obligation, for the superseded version"
    );
    assert!(
        rig.store.under(md::ORPHAN_PREFIX).await.is_empty(),
        "no mark"
    );
    rig.assert_retired_through_the_drain(
        &generation_key(24, 1),
        "g:24:1",
        &positions(&prior_chunks),
    )
    .await;
    rig.assert_kept(&positions(&next), "the new generation");
}

/// (A3) The `unlink` of a **segmented** object published by the production Complete — no inline
/// path exists for one: one obligation naming its segment group, and its 540 fragments go only
/// after the drain's marks and their grace.
#[tokio::test]
async fn a3_a_segmented_delete_retires_through_one_obligation() {
    let rig = Rig::new().await;
    let id = rig.session(30, "photos/a3-seg").await;
    let one = chunks(30_000, 90, RS21);
    let two = chunks(31_000, 90, RS21);
    rig.plant_parts(&id, &[(1, one.clone()), (2, two.clone())])
        .await;
    rig.complete(&id, "photos/a3-seg", &[1, 2], 31).await;
    let records = records_obligation(&rig, &id).await;
    rig.drain_until_gone(&records, 4).await;
    rig.repair_until_complete(4).await;
    let root = rig.inode(31).await;
    assert!(root.chunk_map.is_segmented());
    rig.unlink("photos/a3-seg").await;
    let at: Vec<_> = positions(&one).into_iter().chain(positions(&two)).collect();
    assert_eq!(at.len(), 540);
    assert_eq!(
        rig.store
            .under(RETIRE_BYTES_PREFIX)
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![generation_key(31, root.version)],
        "exactly one obligation"
    );
    rig.assert_retired_through_the_drain(
        &generation_key(31, root.version),
        &format!("g:31:{}", root.version),
        &at,
    )
    .await;
}
