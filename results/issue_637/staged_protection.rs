//! Issue #637 — **staged-byte protection** (proposal 0016 decision 2,
//! `docs/design/proposals/draft/0016-multipart-commit-protocol.md`, "A protection class for
//! durable-but-unpublished bytes, per consumer").
//!
//! A multipart upload's durable-but-unpublished bytes — the fragments its committed `part:`
//! records place, and the ones its in-flight owned `sidx:` entries plan — are a protection class
//! every maintenance pass reads, and each pass makes its own stated decision from it. Every leg
//! below drives a **production** pass (`reconcile_step`, `reconcile_after_restore`,
//! `reconciliation_status`) over in-memory trait doubles; only the metadata store and the D-server
//! fleet are doubles.
//!
//! The staging records are seeded as the raw bytes a store holds, and every seed is first put
//! through the tree's own decoder for its namespace, so no leg can pass because its fixture
//! quietly stopped being the record it names.
//!
//! This file names only symbols the tree carried **before** this slice: it must compile — and
//! fail by assertion — on that tree. New durable state is therefore read as raw bytes under its
//! key, and the new `RestoreReport` counters through the report's `Debug` rendering.
//!
//! | Leg | Claim |
//! |---|---|
//! | A | GC keeps a staged fragment even with lapsed `orphan:` evidence on it |
//! | B | a drain holding only an in-flight (`sidx:`) or only a committed-part fragment is `Pending` |
//! | C | restore marks no staged fragment, counts them as staged, fences every open session, and a later GC keeps the bytes |
//! | C2 | the restore-fence generation is durable, advances per restore, and says complete only after the fence |
//! | D | scrub verifies a committed part's fragment and queues a repair for a corrupt one |
//! | E | reconstruction rebuilds a lost staged fragment under the pre-mark + fenced CAS rule, win and loss |
//! | F | a staged-only draining server gets an empty evacuation plan while its drain is `Pending` |
//! | G | the `orphan:` ledger is walked in bounded pages, with a continuation that survives a restart |
//! | H | all three `orphan:` value variants decode; an undecodable one is left untouched and named |
//! | H2 | keyed pending-retirement protection, reclaim restart, the fragment-less mark sweep, the identity gate |
//! | I | GC records reclamation intent before it destroys the bytes |
//! | I2 | 0016's classification sweep runs after every scenario: no fragment is in no safe class |

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, Once};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_chunk_format::CORE_HEADER_LEN;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, ChunkMap, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, SegmentGroup,
    SegmentRecord, SegmentRef, SegmentedMap,
};
use wyrd_core::multipart::{
    self, OwnedEntry, PartNumber, PartScope, RetireMode, RetireToken, SessionState,
    StagedPlacement, UploadId,
};
use wyrd_core::placement::Topology;
use wyrd_core::{erasure, repair, write::encode_ec_fragment};
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, reconciliation_status, set_lifecycle, Custodian,
    DServerLifecycle, ExpiredPendingPolicy, FencedZone, GcContext, RebalanceContext,
    ReconcileError, Reconciled, ReconciliationStatus, ReconstructionContext, RestoreReport,
    ScrubContext,
};
use wyrd_traits::{
    page_limit, ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    Result, ScanCapExceeded, ScanPage, WriteBatch, SCAN_CAP,
};

// ---------------------------------------------------------------------------------------------
// The metadata-store double
// ---------------------------------------------------------------------------------------------

/// A `BTreeMap`-backed metadata store: raw byte-lexicographic key order, so a paged walk is
/// answered in the order the `scan_page` contract states (`crates/traits/src/lib.rs`, clause 1).
///
/// It can be told to **lower its per-listing cap** (leg G, the `crates/metadata-redb/tests/scan.rs`
/// idiom): `scan` then fails loud past the cap exactly as a backend does, and `scan_page` clamps
/// its page to it. It records every **range read** it answers (H2(a) fails the test on a wholesale
/// read of the `retire:` namespace), and it can refuse every commit that touches a key under a
/// given prefix (legs I and C2).
#[derive(Default)]
struct Store {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    cap: Option<usize>,
    reads: Mutex<Vec<RangeRead>>,
    refuse_commits_touching: Mutex<Option<Vec<u8>>>,
}

/// One range read the store answered: its prefix, whether it was a paged read, and how many
/// keys it handed back.
#[derive(Clone, Debug)]
struct RangeRead {
    prefix: Vec<u8>,
    paged: bool,
    keys: usize,
}

impl Store {
    fn capped(cap: usize) -> Self {
        Self {
            cap: Some(cap),
            ..Self::default()
        }
    }

    fn refuse_commits_touching(&self, prefix: Option<&[u8]>) {
        *self.refuse_commits_touching.lock().unwrap() = prefix.map(<[u8]>::to_vec);
    }

    fn take_reads(&self) -> Vec<RangeRead> {
        std::mem::take(&mut *self.reads.lock().unwrap())
    }

    fn len_under(&self, prefix: &[u8]) -> usize {
        let kv = self.kv.lock().unwrap();
        kv.keys().filter(|key| key.starts_with(prefix)).count()
    }
}

#[async_trait]
impl MetadataStore for Store {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        let rows: Vec<(Vec<u8>, Bytes)> = {
            let kv = self.kv.lock().unwrap();
            kv.iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        };
        self.reads.lock().unwrap().push(RangeRead {
            prefix: prefix.to_vec(),
            paged: false,
            keys: rows.len(),
        });
        let cap = self.cap.unwrap_or(SCAN_CAP);
        if rows.len() > cap {
            return Err(ScanCapExceeded {
                cap,
                prefix: prefix.to_vec(),
            }
            .into());
        }
        Ok(rows)
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<ScanPage> {
        let limit = page_limit(limit, self.cap.unwrap_or(SCAN_CAP), prefix)?;
        let mut rows: Vec<(Vec<u8>, Bytes)> = {
            let kv = self.kv.lock().unwrap();
            kv.iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .filter(|(key, _)| after.is_none_or(|after| key.as_slice() > after))
                .take(limit + 1)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        };
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last().map(|(key, _)| key.clone())
        } else {
            None
        };
        self.reads.lock().unwrap().push(RangeRead {
            prefix: prefix.to_vec(),
            paged: true,
            keys: rows.len(),
        });
        Ok((rows, next))
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        if let Some(refused) = self.refuse_commits_touching.lock().unwrap().as_deref() {
            let touches = batch.puts.iter().map(|(key, _)| key).chain(&batch.deletes);
            if touches.into_iter().any(|key| key.starts_with(refused)) {
                return Err("injected: the metadata store refused this commit".into());
            }
        }
        let mut kv = self.kv.lock().unwrap();
        for pre in &batch.preconditions {
            if kv.get(&pre.key).cloned() != pre.expected {
                return Ok(CommitOutcome::Conflict);
            }
        }
        for (key, value) in batch.puts {
            kv.insert(key, value);
        }
        for key in batch.deletes {
            kv.remove(&key);
        }
        Ok(CommitOutcome::Committed)
    }
}

// ---------------------------------------------------------------------------------------------
// The D-server double
// ---------------------------------------------------------------------------------------------

/// A hook a D server runs once, the moment a chosen fragment lands on it (leg E's lost CAS: the
/// session is fenced in the window between the rebuilt fragment's write and the re-place's CAS).
type LandingHook = Box<dyn FnOnce() + Send>;

/// One D server's fragment bytes.
#[derive(Default)]
struct DServer {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
    /// Successful `delete_fragment` calls (H2(b): a resumed reclaim deletes exactly once).
    deletes: Mutex<usize>,
    /// Fail the next `delete_fragment` (H2(b): the crash between the reclaim-intent CAS and the
    /// deletion).
    fail_next_delete: Mutex<bool>,
    on_landing: Mutex<Option<(FragmentId, LandingHook)>>,
}

impl DServer {
    fn hold(&self, frag: FragmentId, bytes: Bytes) {
        self.frags.lock().unwrap().insert(frag, bytes);
    }

    fn holds(&self, frag: FragmentId) -> bool {
        self.frags.lock().unwrap().contains_key(&frag)
    }

    fn bytes(&self, frag: FragmentId) -> Option<Bytes> {
        self.frags.lock().unwrap().get(&frag).cloned()
    }

    fn is_empty(&self) -> bool {
        self.frags.lock().unwrap().is_empty()
    }
}

#[async_trait]
impl ChunkStore for DServer {
    async fn put_fragment(
        &self,
        id: FragmentId,
        fragment: Bytes,
        _deadline_millis: Option<u64>,
    ) -> Result<()> {
        self.frags.lock().unwrap().insert(id, fragment);
        let hook = {
            let mut armed = self.on_landing.lock().unwrap();
            match armed.take() {
                Some((frag, hook)) if frag == id => Some(hook),
                other => {
                    *armed = other;
                    None
                }
            }
        };
        if let Some(hook) = hook {
            hook();
        }
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        Ok(self.bytes(id))
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        Ok(self.frags.lock().unwrap().keys().copied().collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        if std::mem::take(&mut *self.fail_next_delete.lock().unwrap()) {
            return Err("injected: the D server died before the delete landed".into());
        }
        if self.frags.lock().unwrap().remove(&id).is_some() {
            *self.deletes.lock().unwrap() += 1;
        }
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

// ---------------------------------------------------------------------------------------------
// The rig: a store, a fleet, a topology, and one call per production pass
// ---------------------------------------------------------------------------------------------

/// The logical time every pass runs at unless a leg says otherwise.
const NOW: u64 = 10_000;
/// The reader-safe grace window GC and restore are handed.
const GRACE: u64 = 1_000;
/// RS(2,1): three fragments per chunk, any two rebuild it.
const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };

struct Rig {
    meta: Arc<Store>,
    d: Vec<Arc<DServer>>,
    topology: Topology,
}

impl Rig {
    /// `servers` D servers, server `i` alone in failure domain `D<i>`.
    fn new(servers: usize) -> Self {
        Self::over(Store::default(), servers)
    }

    fn over(store: Store, servers: usize) -> Self {
        permissive_tracing();
        let mut topology = Topology::default();
        for id in 0..servers {
            topology.register(id as DServerId, format!("D{id}"));
        }
        Self {
            meta: Arc::new(store),
            d: (0..servers).map(|_| Arc::new(DServer::default())).collect(),
            topology,
        }
    }

    fn fleet(&self) -> Vec<(DServerId, &dyn ChunkStore)> {
        self.d
            .iter()
            .enumerate()
            .map(|(id, d)| (id as DServerId, &**d as &dyn ChunkStore))
            .collect()
    }

    fn fleet_of(&self, ids: &[DServerId]) -> Vec<(DServerId, &dyn ChunkStore)> {
        ids.iter()
            .map(|&id| (id, &*self.d[id as usize] as &dyn ChunkStore))
            .collect()
    }

    async fn put(&self, key: Vec<u8>, value: impl Into<Bytes>) {
        let landed = self.meta.commit(WriteBatch::new().put(key, value)).await;
        assert_eq!(landed.unwrap(), CommitOutcome::Committed, "fixture put");
    }

    async fn get(&self, key: &[u8]) -> Option<Bytes> {
        self.meta.get(key).await.unwrap()
    }

    /// One GC pass through the fenced control point, as a freshly elected custodian — so a
    /// continuation the pass carries forward can only have been carried in the store.
    async fn gc(&self, now: u64, grace: u64) -> std::result::Result<Reconciled, ReconcileError> {
        let fleet = self.fleet();
        let ctx = GcContext {
            meta: &*self.meta,
            fleet: &fleet,
            grace_window_millis: grace,
            expired_pending: ExpiredPendingPolicy::Defer,
        };
        let (zone, custodian, _coord) = elect().await;
        reconcile_step(&zone, &custodian, Some(&ctx), None, None, None, now).await
    }

    async fn scrub(&self) -> std::result::Result<Reconciled, ReconcileError> {
        let fleet = self.fleet();
        let ctx = ScrubContext {
            meta: &*self.meta,
            fleet: &fleet,
        };
        let (zone, custodian, _coord) = elect().await;
        reconcile_step(&zone, &custodian, None, Some(&ctx), None, None, NOW).await
    }

    /// One reconstruction pass over the servers named — a lost server is simply absent from the
    /// fleet and the topology the pass is handed, as the deployed role hands it the live view.
    async fn reconstruct(
        &self,
        live: &[DServerId],
    ) -> std::result::Result<Reconciled, ReconcileError> {
        let fleet = self.fleet_of(live);
        let mut topology = Topology::default();
        for &id in live {
            topology.register(id, format!("D{id}"));
        }
        let ctx = ReconstructionContext {
            meta: &*self.meta,
            fleet: &fleet,
            topology: &topology,
            unreachable: &[],
        };
        let (zone, custodian, _coord) = elect().await;
        reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, NOW).await
    }

    async fn rebalance(&self) -> std::result::Result<Reconciled, ReconcileError> {
        let fleet = self.fleet();
        let ctx = RebalanceContext {
            meta: &*self.meta,
            fleet: &fleet,
            topology: &self.topology,
        };
        let (zone, custodian, _coord) = elect().await;
        reconcile_step(&zone, &custodian, None, None, None, Some(&ctx), NOW).await
    }

    async fn restore(&self, now: u64) -> Result<RestoreReport> {
        let fleet = self.fleet();
        let ctx = GcContext {
            meta: &*self.meta,
            fleet: &fleet,
            grace_window_millis: GRACE,
            expired_pending: ExpiredPendingPolicy::Defer,
        };
        reconcile_after_restore(&ctx, now).await
    }

    /// Place the three fragments of `chunk` on the D servers `placement` names — real v1
    /// fragments of `data`, so a verify or a rebuild over them is a genuine one.
    fn place(&self, chunk: ChunkId, placement: [DServerId; 3], data: &[u8]) {
        for (index, bytes) in stripe(chunk, data).into_iter().enumerate() {
            let frag = frag(chunk, index as u16);
            self.d[placement[index] as usize].hold(frag, bytes);
        }
    }

    fn holds(&self, dserver: DServerId, frag: FragmentId) -> bool {
        self.d[dserver as usize].holds(frag)
    }

    /// Every fragment on disk, fleet-wide.
    async fn on_disk(&self) -> Vec<(DServerId, FragmentId)> {
        let mut all = Vec::new();
        for (id, d) in self.d.iter().enumerate() {
            for frag in d.list_fragments().await.unwrap() {
                all.push((id as DServerId, frag));
            }
        }
        all
    }

    /// 0016's **classification sweep** (`0016:2906-2921`, the at-least-one-safe-class helper
    /// this protocol earns): every fragment on disk must be in **at least one** safe class —
    /// committed-referenced, staged with a session that still exists, or evidenced for
    /// reclamation by an `orphan:` mark this sweep can read — and no committed-referenced
    /// fragment may carry evidence whose grace has already lapsed. **No gaps, not a partition**:
    /// the protocol deliberately overlaps protection across its handoffs, so a fragment in two
    /// classes is correct and is never flagged.
    ///
    /// Returns the fragments in no class. Each leg asserts it is exactly the set that leg
    /// quarantines on purpose — empty for every leg but H.
    async fn unclassified(&self, now: u64, grace: u64) -> Vec<(DServerId, FragmentId)> {
        let meta = &*self.meta;
        let mut committed: HashSet<(DServerId, FragmentId)> = HashSet::new();
        for (key, value) in meta.scan(b"inode:").await.unwrap() {
            let Ok(record) = metadata::decode::<InodeRecord>(&value) else {
                continue;
            };
            if record.state != InodeState::Committed {
                continue;
            }
            let Ok(Some(resolved)) = metadata::resolve_chunk_map(meta, &key, &record).await else {
                continue;
            };
            for chunk in resolved.chunks.iter() {
                for (index, dserver) in chunk.fragments() {
                    committed.insert((dserver, frag(chunk.id, index)));
                }
            }
        }
        let mut staged: HashSet<(DServerId, FragmentId)> = HashSet::new();
        for (key, _) in meta.scan(multipart::MPU_PREFIX).await.unwrap() {
            let Ok(upload) = multipart::parse_mpu_key(&key) else {
                continue;
            };
            for (key, value) in meta.scan(&multipart::sidx_range(&upload)).await.unwrap() {
                if let Ok((_, chunk, entry)) = multipart::decode_owned_entry(&key, &value) {
                    for (index, &dserver) in entry.staged().placement().iter().enumerate() {
                        staged.insert((dserver, frag(chunk, index as u16)));
                    }
                }
            }
            for (_, value) in meta.scan(&multipart::part_range(&upload)).await.unwrap() {
                if let Ok(part) = multipart::decode_part_record(&value) {
                    for chunk in part.chunks() {
                        for (index, dserver) in chunk.fragments() {
                            staged.insert((dserver, frag(chunk.id, index)));
                        }
                    }
                }
            }
        }
        let mut gaps = Vec::new();
        for (dserver, fragment) in self.on_disk().await {
            let at = (dserver, fragment);
            let mark = self
                .get(&metadata::orphan_key(dserver, fragment))
                .await
                .and_then(|value| readable_mark(&value));
            if committed.contains(&at) {
                if let Some((stamp, reclaiming)) = mark {
                    assert!(
                        reclaiming || now < stamp.saturating_add(grace),
                        "classification sweep: {at:?} is committed-referenced AND carries \
                         lapsed reclamation evidence — the pair GC would act on wrongly"
                    );
                }
                continue;
            }
            if staged.contains(&at) || mark.is_some() {
                continue;
            }
            gaps.push(at);
        }
        gaps
    }

    /// The sweep, asserted: no fragment on disk is in no safe class.
    async fn assert_no_gaps(&self, now: u64, grace: u64, leg: &str) {
        let gaps = self.unclassified(now, grace).await;
        assert!(
            gaps.is_empty(),
            "{leg}: the classification sweep found fragment(s) in NO safe class — neither \
             committed-referenced, nor staged with a live session, nor evidenced for \
             reclamation: {gaps:?}"
        );
    }
}

/// A fresh leader over a fresh coordination service — a new custodian every pass. The service
/// is handed back so it outlives the pass it fenced.
async fn elect() -> (FencedZone, Custodian, MemCoordination) {
    let coord = MemCoordination::new();
    let custodian = Custodian::elect(&coord, "zone-637").await.unwrap();
    let mut zone = FencedZone::new();
    zone.install(custodian.leadership());
    (zone, custodian, coord)
}

/// `tracing` latches a callsite's interest process-globally on its first hit, so a sibling test
/// hitting an audit callsite with no subscriber installed would leave a later capture empty
/// (wyrd #214). One permissive global default, before any pass runs.
fn permissive_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}

// ---------------------------------------------------------------------------------------------
// Record seeds — raw bytes, each checked against the tree's own decoder for its namespace
// ---------------------------------------------------------------------------------------------

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

fn upload(fill: &str) -> UploadId {
    UploadId::new(fill.repeat(16)).unwrap()
}

fn rs21(chunk: ChunkId, placement: [DServerId; 3], len: u64) -> ChunkRef {
    ChunkRef {
        id: chunk,
        scheme: RS21,
        len,
        placement: placement.to_vec(),
    }
}

/// The three real v1 fragments of `data` under RS(2,1), indexed by fragment.
fn stripe(chunk: ChunkId, data: &[u8]) -> Vec<Bytes> {
    let shards = erasure::encode(2, 1, data).unwrap();
    shards
        .iter()
        .enumerate()
        .map(|(index, shard)| encode_ec_fragment(chunk, index as u16, 2, 1, shard))
        .collect()
}

/// A session record: `Open`, or `Completing` at `epoch` with one segment already written.
fn session_value(epoch: u64, completing: bool) -> String {
    let state = if completing {
        format!(
            r#"{{"kind":"Completing","fenced_at_millis":2000,"segments_written":1,"publish_target":{{"parent":1,"name":"big.bin","epoch":{epoch}}}}}"#
        )
    } else {
        r#"{"kind":"Open"}"#.to_owned()
    };
    format!(
        r#"{{"parent":1,"object":"big.bin","created_at_millis":1000,"clock_source":"wall","epoch":{epoch},"attempts":{},"state":{state}}}"#,
        u32::from(completing)
    )
}

async fn seed_session(rig: &Rig, id: &UploadId, epoch: u64, completing: bool) -> Bytes {
    let value = Bytes::from(session_value(epoch, completing));
    let decoded = multipart::decode_session_record(&value).expect("fixture: a canonical session");
    assert_eq!(decoded.epoch(), epoch, "fixture: session epoch");
    rig.put(multipart::mpu_key(id), value.clone()).await;
    value
}

async fn seed_part(rig: &Rig, id: &UploadId, part: u32, epoch: u64, chunks: &[ChunkRef]) -> Bytes {
    let len: u64 = chunks.iter().map(|chunk| chunk.len).sum();
    let listed: Vec<String> = chunks
        .iter()
        .map(|chunk| String::from_utf8(metadata::encode(chunk).to_vec()).unwrap())
        .collect();
    let value = Bytes::from(format!(
        r#"{{"chunks":[{}],"len":{len},"digest":"{}","committed_at_millis":1500,"session_epoch":{epoch}}}"#,
        listed.join(","),
        "ab".repeat(32)
    ));
    multipart::decode_part_record(&value).expect("fixture: a canonical part record");
    let key = multipart::part_key(id, PartNumber::new(part).unwrap());
    rig.put(key, value.clone()).await;
    value
}

async fn seed_owned(
    rig: &Rig,
    id: &UploadId,
    part: u32,
    chunk: ChunkId,
    placement: [DServerId; 3],
) {
    let staged = StagedPlacement::new(RS21, placement.to_vec()).unwrap();
    let entry = OwnedEntry::new(id.clone(), NOW + 60_000, staged);
    let key = multipart::sidx_key(id, PartNumber::new(part).unwrap(), chunk);
    let value = metadata::encode(&entry.to_pending());
    multipart::decode_owned_entry(&key, &value).expect("fixture: a canonical owned entry");
    rig.put(key, value).await;
}

async fn seed_committed(rig: &Rig, inode: InodeId, chunks: Vec<ChunkRef>) {
    let record = InodeRecord {
        size: chunks.iter().map(|chunk| chunk.len).sum(),
        chunk_map: chunks.into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    rig.put(metadata::inode_key(inode), metadata::encode(&record))
        .await;
}

async fn mark(rig: &Rig, dserver: DServerId, frag: FragmentId, value: &str) {
    rig.put(
        metadata::orphan_key(dserver, frag),
        value.to_owned().into_bytes(),
    )
    .await;
}

/// Read an `orphan:` value the way this file's oracle reads one — the stamp, and whether
/// reclamation was already decided — or `None` for a value none of the three shapes spells: a
/// bare canonical decimal, `{"orphaned_at_millis":N,...}`, or that with `"reclaiming":true`.
///
/// A deliberately small, independent oracle rather than a call into the production decoder,
/// which the tree this file must compile against does not have.
fn readable_mark(value: &[u8]) -> Option<(u64, bool)> {
    let text = std::str::from_utf8(value).ok()?;
    let digits = |s: &str| -> Option<u64> {
        let canonical = !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_digit())
            && (s == "0" || !s.starts_with('0'));
        canonical.then(|| s.parse().ok()).flatten()
    };
    if let Some(stamp) = digits(text) {
        return Some((stamp, false));
    }
    let body = text
        .strip_prefix(r#"{"orphaned_at_millis":"#)?
        .strip_suffix('}')?;
    let stamp = body.split(',').next()?;
    Some((digits(stamp)?, body.contains(r#""reclaiming":true"#)))
}

// ---------------------------------------------------------------------------------------------
// Leg A — GC protects staged fragments, WITH the evidence that makes the pass act
// ---------------------------------------------------------------------------------------------

/// An `Open` session holding a committed `part:` record and an in-flight owned `sidx:` entry,
/// and a **lapsed** `orphan:` mark on one fragment of each — the evidence that makes GC act.
/// (Without it GC's conservative arm would keep an unevidenced fragment on any tree, and this
/// leg would prove nothing.) Every fragment of both chunks survives the pass.
///
/// 0016's own oracle for "leave the reference set committed-only" (`0016:878`).
#[tokio::test]
async fn a_gc_keeps_staged_fragments_that_carry_lapsed_orphan_evidence() {
    let rig = Rig::new(3);
    let u = upload("a1");
    let (committed, in_flight): (ChunkId, ChunkId) = (0xA0_01, 0xA0_02);
    seed_session(&rig, &u, 3, false).await;
    seed_part(&rig, &u, 1, 3, &[rs21(committed, [0, 1, 2], 40)]).await;
    seed_owned(&rig, &u, 2, in_flight, [0, 1, 2]).await;
    rig.place(committed, [0, 1, 2], &[7u8; 40]);
    rig.place(in_flight, [0, 1, 2], &[9u8; 40]);
    // Lapsed evidence: stamped at 0, the grace window long gone by NOW.
    mark(&rig, 0, frag(committed, 0), "0").await;
    mark(&rig, 1, frag(in_flight, 1), "0").await;

    rig.gc(NOW, GRACE).await.expect("the GC pass completes");

    for chunk in [committed, in_flight] {
        for (index, dserver) in [(0u16, 0), (1, 1), (2, 2)] {
            assert!(
                rig.holds(dserver, frag(chunk, index)),
                "GC reclaimed fragment {index} of staged chunk {chunk:#x} on D server \
                 {dserver} — a live upload's bytes, deleted on the strength of an orphan \
                 mark the staged reference set should have overridden"
            );
        }
    }
    rig.assert_no_gaps(NOW, GRACE, "leg A").await;
}

// ---------------------------------------------------------------------------------------------
// Leg B — the drain counts both staged classes as held
// ---------------------------------------------------------------------------------------------

/// A server holding **only** an in-flight owned (`sidx:`) fragment, with its drain recorded:
/// the drain is `Pending`. Answering `Satisfied` is the F6 wipe trace — the operator wipes the
/// box, the part commits, and a Complete publishes a map naming wiped bytes.
#[tokio::test]
async fn b_a_drain_holding_only_an_in_flight_owned_fragment_is_pending() {
    let rig = Rig::new(4);
    let u = upload("b1");
    let chunk: ChunkId = 0xB0_01;
    seed_session(&rig, &u, 1, false).await;
    // Fragment 0 of the in-flight chunk is planned on server 3, and nothing else is there.
    seed_owned(&rig, &u, 1, chunk, [3, 1, 2]).await;
    rig.place(chunk, [3, 1, 2], &[1u8; 40]);
    set_lifecycle(&*rig.meta, 3, DServerLifecycle::Draining)
        .await
        .unwrap();

    let status = reconciliation_status(&*rig.meta, 3).await.unwrap();
    assert_eq!(
        status,
        ReconciliationStatus::Pending,
        "server 3 holds an in-flight part's fragment: the drain must stay Pending"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg B (sidx)").await;
}

/// The same, for a **committed** part's fragment — separately, because a drain that counts only
/// one of the two classes passes one of these and fails the other (0016's iteration-3
/// finding-4 hole).
#[tokio::test]
async fn b_a_drain_holding_only_a_committed_part_fragment_is_pending() {
    let rig = Rig::new(4);
    let u = upload("b2");
    let chunk: ChunkId = 0xB0_02;
    seed_session(&rig, &u, 1, false).await;
    seed_part(&rig, &u, 1, 1, &[rs21(chunk, [3, 1, 2], 40)]).await;
    rig.place(chunk, [3, 1, 2], &[2u8; 40]);
    set_lifecycle(&*rig.meta, 3, DServerLifecycle::Draining)
        .await
        .unwrap();

    let status = reconciliation_status(&*rig.meta, 3).await.unwrap();
    assert_eq!(
        status,
        ReconciliationStatus::Pending,
        "server 3 holds a committed part's fragment: the drain must stay Pending"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg B (part)").await;
}

// ---------------------------------------------------------------------------------------------
// Leg C — restore protects staged bytes, fences every open session, and the data survives
// ---------------------------------------------------------------------------------------------

const RESTORE_FENCE_KEY: &[u8] = b"restore:fence";

/// The restored image: session U1 `Open@2` with a committed part and an in-flight entry, and
/// session U2 `Completing@5` that had already written one `seg:` record of its attempt — nine
/// staged fragments on disk, none referenced by any committed chunk map.
struct RestoredImage {
    rig: Rig,
    open: UploadId,
    completing: UploadId,
    open_bytes: Bytes,
    staged: Vec<(DServerId, FragmentId)>,
}

async fn restored_image() -> RestoredImage {
    let rig = Rig::new(3);
    let open = upload("c1");
    let completing = upload("c2");
    let (part_chunk, owned_chunk, completing_chunk): (ChunkId, ChunkId, ChunkId) =
        (0xC0_01, 0xC0_02, 0xC0_03);
    let open_bytes = seed_session(&rig, &open, 2, false).await;
    seed_part(&rig, &open, 1, 2, &[rs21(part_chunk, [0, 1, 2], 40)]).await;
    seed_owned(&rig, &open, 2, owned_chunk, [0, 1, 2]).await;
    seed_session(&rig, &completing, 5, true).await;
    let completing_ref = rs21(completing_chunk, [0, 1, 2], 40);
    seed_part(
        &rig,
        &completing,
        1,
        5,
        std::slice::from_ref(&completing_ref),
    )
    .await;
    // The attempt's one segment, written before the snapshot and before its root flip.
    let group = SegmentGroup::new("0f".repeat(16), 5).unwrap();
    let segment = SegmentRecord::new(vec![completing_ref], 0).unwrap();
    rig.put(
        metadata::seg_key(&group, 0).unwrap(),
        metadata::encode(&segment),
    )
    .await;
    let mut staged = Vec::new();
    for chunk in [part_chunk, owned_chunk, completing_chunk] {
        rig.place(chunk, [0, 1, 2], &[chunk as u8; 40]);
        for index in 0..3u16 {
            staged.push((DServerId::from(index), frag(chunk, index)));
        }
    }
    RestoredImage {
        rig,
        open,
        completing,
        open_bytes,
        staged,
    }
}

/// The post-restore pass over an image holding live uploads marks **none** of their fragments
/// stranded, and has skipped them **as staged** rather than merely not reached them — and a GC
/// pass after the grace window then finds every staged byte still on disk. On a pass that does
/// not know the class, every staged fragment is marked and that GC pass deletes them: data
/// loss, not a tidier counter.
#[tokio::test]
async fn c_restore_marks_no_staged_fragment_and_a_later_gc_keeps_every_byte() {
    let image = restored_image().await;
    let rig = &image.rig;

    let report = rig.restore(NOW).await.expect("the restore pass completes");
    assert_eq!(
        report.stranded_marked, 0,
        "a live upload's fragments are not strays: {report:?}"
    );
    // `staged_skipped` is this slice's counter, beside `pending_skipped`: read through the
    // report's `Debug` so this file still compiles on the tree that lacks it.
    let rendered = format!("{report:?}");
    assert!(
        rendered.contains(&format!("staged_skipped: {}", image.staged.len())),
        "every staged fragment must be counted as skipped AS STAGED: {rendered}"
    );
    for &(dserver, fragment) in &image.staged {
        assert!(
            rig.get(&metadata::orphan_key(dserver, fragment))
                .await
                .is_none(),
            "restore marked staged fragment {fragment:?} on D server {dserver} stranded"
        );
    }

    // ...and the consequence, which is the point: past the grace window, GC runs.
    let later = NOW + 10 * GRACE;
    rig.gc(later, GRACE).await.expect("the GC pass completes");
    for &(dserver, fragment) in &image.staged {
        assert!(
            rig.holds(dserver, fragment),
            "staged fragment {fragment:?} on D server {dserver} was deleted by the GC pass \
             after the restore — the restore handed a live upload's bytes to GC"
        );
    }
    rig.assert_no_gaps(later, GRACE, "leg C").await;
}

/// "Then fence" is an observable, not prose (D-B, `0016:823`): a records-only image cannot prove
/// the staged bytes still exist, so every resurrected `Open` and `Completing` session is fenced
/// to `Aborting` — by the ordinary teardown fence for `Open`, by the dedicated restore-fence
/// transition for `Completing` — each installing its retirement obligation in the fencing batch,
/// and the counter says so. A Complete retried against the restored session is then refused: its
/// fence preconditions on the session bytes the image carried, and those are gone.
#[tokio::test]
async fn c_restore_fences_every_resurrected_session_and_a_retried_complete_is_refused() {
    let image = restored_image().await;
    let rig = &image.rig;

    let report = rig.restore(NOW).await.expect("the restore pass completes");
    let rendered = format!("{report:?}");
    assert!(
        rendered.contains("sessions_fenced: 2"),
        "both resurrected sessions must be fenced, and counted: {rendered}"
    );
    // The one half of the restore-fence batch this pass cannot write: the Completing attempt's
    // `retire:records:{seg:<g>:<E>}`, since the session record does not name its segment group.
    // The session is fenced all the same, and it is NAMED for a human rather than passed over.
    let unretired = format!(
        "segments_unretired: [{:?}]",
        String::from_utf8(multipart::mpu_key(&image.completing)).unwrap()
    );
    assert!(
        rendered.contains(&unretired),
        "the fenced Completing session whose segment records the fence cannot name must be \
         reported for a human: {rendered}"
    );

    // Open@2 -> Aborting@3, with its teardown obligation installed at the fenced epoch.
    let open = rig.get(&multipart::mpu_key(&image.open)).await.unwrap();
    let open = multipart::decode_session_record(&open).unwrap();
    assert!(
        matches!(open.state(), SessionState::Aborting {}) && open.epoch() == 3,
        "the Open session must be fenced to Aborting@3: {open:?}"
    );
    let token = RetireToken::Session {
        upload_id: image.open.clone(),
        epoch: 2,
        part: None,
    };
    let key = multipart::retire_key(RetireMode::Bytes, &token);
    let value = rig
        .get(&key)
        .await
        .expect("the Open fence installs its teardown obligation");
    let (_, _, owed) = multipart::decode_retire_obligation(&key, &value).unwrap();
    assert!(
        owed.session() && matches!(owed.parts(), Some(PartScope::All)),
        "the Open fence retires the session's residue and every part: {owed:?}"
    );

    // Completing@5 -> Aborting@6, retiring the session and the exact part set it froze.
    let completing = rig
        .get(&multipart::mpu_key(&image.completing))
        .await
        .unwrap();
    let completing = multipart::decode_session_record(&completing).unwrap();
    assert!(
        matches!(completing.state(), SessionState::Aborting {}) && completing.epoch() == 6,
        "the Completing session must take the restore-fence transition to Aborting@6: \
         {completing:?}"
    );
    let token = RetireToken::Session {
        upload_id: image.completing.clone(),
        epoch: 5,
        part: None,
    };
    let key = multipart::retire_key(RetireMode::Bytes, &token);
    let value = rig
        .get(&key)
        .await
        .expect("the restore fence installs retire:bytes:{session, parts}");
    let (_, _, owed) = multipart::decode_retire_obligation(&key, &value).unwrap();
    let frozen = matches!(owed.parts(), Some(PartScope::Set(set)) if set.runs() == [(1, 1)]);
    assert!(
        owed.session() && frozen,
        "the restore fence retires the session and exactly the part set it froze: {owed:?}"
    );

    // A client retrying Complete against the restored image: its fence preconditions on the
    // session bytes it read from the image, `Open@2`.
    let retried = WriteBatch::new()
        .require(multipart::mpu_key(&image.open), image.open_bytes.clone())
        .put(
            multipart::mpu_key(&image.open),
            session_value(3, true).into_bytes(),
        );
    assert_eq!(
        rig.meta.commit(retried).await.unwrap(),
        CommitOutcome::Conflict,
        "a Complete retried against a restored session must be refused, never publish"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg C (fence)").await;
}

/// **C2** — the restore-fence generation record 0016 requires before any gateway serves
/// multipart verbs on a restored image (`0016:723-728`): durable, authoritative, and never
/// complete before the fence is. Absent before any restore; written as **in progress** the
/// moment a restore pass starts, and still in progress if the pass dies before its fence lands;
/// **complete** once the fence has landed; and advanced by every later restore, so a later
/// restore invalidates an earlier completion instead of being masked by it.
#[tokio::test]
async fn c2_the_restore_fence_generation_is_durable_and_says_complete_only_after_the_fence() {
    let image = restored_image().await;
    let rig = &image.rig;
    let fence = || async { rig.get(RESTORE_FENCE_KEY).await };

    assert_eq!(
        fence().await,
        None,
        "no restore has run: no fence generation"
    );

    // A restore pass that dies before its session fence can land.
    rig.meta
        .refuse_commits_touching(Some(multipart::MPU_PREFIX));
    let died = rig.restore(NOW).await;
    assert_eq!(
        fence().await.as_deref(),
        Some(br#"{"generation":1,"complete":false}"#.as_slice()),
        "a restore whose fence has not landed is an IN-PROGRESS generation, readable as such"
    );
    assert!(died.is_err(), "the refused fence ends the pass: {died:?}");

    // Re-run: the fence lands, and only then is the generation complete.
    rig.meta.refuse_commits_touching(None);
    rig.restore(NOW).await.expect("the re-run completes");
    assert_eq!(
        fence().await.as_deref(),
        Some(br#"{"generation":2,"complete":true}"#.as_slice()),
        "the fence landed: the generation that fenced it is complete"
    );

    // A later restore — its image resurrects an open session — advances the generation: the
    // earlier completion is invalidated the moment the new pass starts, not masked by it.
    seed_session(rig, &image.open, 9, false).await;
    rig.meta
        .refuse_commits_touching(Some(multipart::MPU_PREFIX));
    assert!(rig.restore(NOW).await.is_err(), "fixture: fence refused");
    assert_eq!(
        fence().await.as_deref(),
        Some(br#"{"generation":3,"complete":false}"#.as_slice()),
        "a later restore invalidates the earlier completion"
    );
    rig.meta.refuse_commits_touching(None);
    rig.restore(NOW).await.expect("the later restore completes");
    assert_eq!(
        fence().await.as_deref(),
        Some(br#"{"generation":4,"complete":true}"#.as_slice()),
        "and completes under its own generation"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg C2").await;
}

// ---------------------------------------------------------------------------------------------
// Leg D — scrub verifies a committed part's fragments and queues the repair
// ---------------------------------------------------------------------------------------------

/// A bit-flip in a committed part's fragment (the idiom `tests/scrub.rs` uses): scrub must
/// verify it and leave a **durable repair obligation naming that chunk**. "Walks it" is not an
/// answer — only the queued obligation is.
#[tokio::test]
async fn d_scrub_queues_a_repair_for_a_corrupt_committed_part_fragment() {
    let rig = Rig::new(3);
    let u = upload("d1");
    let chunk: ChunkId = 0xD0_01;
    seed_session(&rig, &u, 1, false).await;
    seed_part(&rig, &u, 1, 1, &[rs21(chunk, [0, 1, 2], 40)]).await;
    rig.place(chunk, [0, 1, 2], &[3u8; 40]);
    // Flip one payload bit of fragment 1: its self-describing checksum no longer verifies.
    let mut rotten = rig.d[1].bytes(frag(chunk, 1)).unwrap().to_vec();
    rotten[usize::from(CORE_HEADER_LEN) + 3] ^= 0x01;
    rig.d[1].hold(frag(chunk, 1), Bytes::from(rotten));

    rig.scrub().await.expect("the scrub pass completes");

    assert!(
        rig.get(&repair::repair_key(chunk)).await.is_some(),
        "scrub must queue a durable repair obligation for the corrupt staged chunk {chunk:#x}"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg D").await;
}

// ---------------------------------------------------------------------------------------------
// Leg E — reconstruction resolves the staged chunk and re-places it under the session fence
// ---------------------------------------------------------------------------------------------

struct LostStagedFragment {
    rig: Rig,
    upload: UploadId,
    chunk: ChunkId,
    part_key: Vec<u8>,
    part_bytes: Bytes,
    session_bytes: Bytes,
}

/// An `Open@4` session whose committed part's RS(2,1) chunk sits on servers 0, 1, 2 — and server
/// 2 is **lost**. A repair obligation is queued for the chunk. The live fleet is 0, 1, 3, so the
/// rebuild's only home in a distinct failure domain is server 3.
async fn lost_staged_fragment() -> LostStagedFragment {
    let rig = Rig::new(4);
    let upload = upload("e1");
    let chunk: ChunkId = 0xE0_01;
    let session_bytes = seed_session(&rig, &upload, 4, false).await;
    let part_bytes = seed_part(&rig, &upload, 1, 4, &[rs21(chunk, [0, 1, 2], 40)]).await;
    rig.place(chunk, [0, 1, 2], &[5u8; 40]);
    // Server 2 is gone: its fragment with it.
    rig.d[2].frags.lock().unwrap().clear();
    repair::enqueue_repair(&*rig.meta, chunk, "scrub")
        .await
        .unwrap();
    LostStagedFragment {
        rig,
        part_key: multipart::part_key(&upload, PartNumber::new(1).unwrap()),
        upload,
        chunk,
        part_bytes,
        session_bytes,
    }
}

/// The **win**: the rebuilt fragment lands on server 3 intact and scheme-correct, the `part:`
/// record names that holder, the destination pre-mark is gone (adopted), the vacated source is
/// newly orphan-evidenced, and the repair obligation drains. A changed placement alone would prove
/// only that metadata moved — every leg of the protocol is asserted.
#[tokio::test]
async fn e_reconstruction_rebuilds_a_lost_staged_fragment_and_repoints_its_part() {
    let lost = lost_staged_fragment().await;
    let rig = &lost.rig;
    let moved = frag(lost.chunk, 2);

    rig.reconstruct(&[0, 1, 3])
        .await
        .expect("the reconstruction pass completes");

    let rebuilt = rig.d[3]
        .bytes(moved)
        .expect("the rebuilt fragment must be written to its new holder, server 3");
    assert!(
        repair::fragment_intact(&rebuilt, moved, RS21),
        "the fragment on server 3 must be intact and scheme-correct for chunk {:#x}",
        lost.chunk
    );
    let part = rig.get(&lost.part_key).await.unwrap();
    let part = multipart::decode_part_record(&part).unwrap();
    assert_eq!(
        part.chunks()[0].placement,
        vec![0, 1, 3],
        "the part record must name the new holder"
    );
    assert!(
        rig.get(&metadata::orphan_key(3, moved)).await.is_none(),
        "the destination pre-mark must be removed when the move is adopted"
    );
    let vacated = rig.get(&metadata::orphan_key(2, moved)).await;
    assert!(
        vacated.as_deref().and_then(readable_mark).is_some(),
        "the vacated source position must be newly orphan-evidenced: {vacated:?}"
    );
    assert!(
        rig.get(&repair::repair_key(lost.chunk)).await.is_none(),
        "the repair obligation drains on the win"
    );
    assert_eq!(
        rig.get(&multipart::mpu_key(&lost.upload)).await,
        Some(lost.session_bytes.clone()),
        "a re-place never writes the session record"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg E (win)").await;
}

/// The **loss**: the session is fenced in the window between the rebuilt fragment's write and the
/// re-place's CAS (X29, `0016:888`). The CAS must lose — the part record untouched — and the
/// destination's pre-mark must **stand**, covering the fragment already written so GC can reclaim
/// it; the obligation stays queued for after the fence resolves.
#[tokio::test]
async fn e_a_staged_re_place_that_loses_to_a_session_fence_leaves_its_pre_mark_standing() {
    let lost = lost_staged_fragment().await;
    let rig = &lost.rig;
    let moved = frag(lost.chunk, 2);
    // The moment the rebuilt fragment lands on server 3, the session is aborted (Open@4 ->
    // Aborting@5) — a client Abort racing the repair.
    let meta = rig.meta.clone();
    let session_key = multipart::mpu_key(&lost.upload);
    let fenced = Bytes::from(
        session_value(5, false).replace(r#"{"kind":"Open"}"#, r#"{"kind":"Aborting"}"#),
    );
    multipart::decode_session_record(&fenced).expect("fixture: a canonical Aborting session");
    let (key, prior) = (session_key.clone(), lost.session_bytes.clone());
    let fence = move || {
        let mut kv = meta.kv.lock().unwrap();
        assert_eq!(
            kv.get(&key),
            Some(&prior),
            "fixture: the fence races an Open session"
        );
        kv.insert(key, fenced);
    };
    *rig.d[3].on_landing.lock().unwrap() = Some((moved, Box::new(fence)));

    rig.reconstruct(&[0, 1, 3])
        .await
        .expect("the reconstruction pass completes");

    let premark = rig.get(&metadata::orphan_key(3, moved)).await;
    assert!(
        premark.as_deref().and_then(readable_mark).is_some(),
        "the destination pre-mark must stand on a lost CAS, so GC reclaims the pre-written \
         fragment: {premark:?}"
    );
    assert!(
        rig.holds(3, moved),
        "the rebuilt fragment was written before the fence landed — the window this leg is about"
    );
    assert_eq!(
        rig.get(&lost.part_key).await,
        Some(lost.part_bytes.clone()),
        "the re-place CAS must LOSE to the session fence: the part record is untouched"
    );
    assert!(
        rig.get(&repair::repair_key(lost.chunk)).await.is_some(),
        "the repair obligation stays queued on a lost CAS"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg E (loss)").await;
}

// ---------------------------------------------------------------------------------------------
// Leg F — rebalance's answer is disjoint from the staged set, and consistent with leg B
// ---------------------------------------------------------------------------------------------

/// A draining server holding **only** staged fragments: the evacuation plan is empty — nothing is
/// copied anywhere, no record moves — **while** the drain is `Pending`. The pair is the assertion
/// (`0016:881`): a staged set merged into the committed placements would make these two answers
/// contradict each other.
#[tokio::test]
async fn f_a_staged_only_draining_server_gets_no_evacuation_while_its_drain_is_pending() {
    let rig = Rig::new(4);
    let u = upload("f1");
    let (committed, in_flight): (ChunkId, ChunkId) = (0xF0_01, 0xF0_02);
    seed_session(&rig, &u, 1, false).await;
    let part = seed_part(&rig, &u, 1, 1, &[rs21(committed, [3, 1, 2], 40)]).await;
    seed_owned(&rig, &u, 2, in_flight, [3, 1, 2]).await;
    rig.place(committed, [3, 1, 2], &[6u8; 40]);
    rig.place(in_flight, [3, 1, 2], &[8u8; 40]);
    set_lifecycle(&*rig.meta, 3, DServerLifecycle::Draining)
        .await
        .unwrap();
    let before = rig.meta.kv.lock().unwrap().clone();

    let outcome = rig.rebalance().await.expect("the rebalance pass completes");

    assert_eq!(
        outcome,
        Reconciled::Satisfied,
        "rebalance has nothing of its own to move off a server holding only staged bytes"
    );
    assert!(
        rig.d[0].is_empty(),
        "no staged fragment may be copied off the draining server"
    );
    for chunk in [committed, in_flight] {
        assert!(
            rig.holds(3, frag(chunk, 0)),
            "the staged fragment stays put"
        );
    }
    assert_eq!(
        *rig.meta.kv.lock().unwrap(),
        before,
        "an empty evacuation plan writes nothing — no part, no orphan mark"
    );
    assert_eq!(
        rig.get(&multipart::part_key(&u, PartNumber::new(1).unwrap()))
            .await,
        Some(part)
    );
    assert_eq!(
        reconciliation_status(&*rig.meta, 3).await.unwrap(),
        ReconciliationStatus::Pending,
        "...while the drain of that server is Pending: its staged bytes are held there"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg F").await;
}

/// A committed **segmented** object's fragment on a draining server is committed content, not
/// staged: the drain is `Pending` on it, and the evacuation that `0016:826` gives it — a `seg:`
/// repoint under the destination-pre-mark rule — is `repoint_chunk`'s (#682, open), so this
/// pass REFUSES it rather than silently dropping it: nothing moves and the pass does not certify.
#[tokio::test]
async fn f_a_committed_segmented_fragment_on_a_draining_server_is_held_and_never_dropped() {
    let rig = Rig::new(4);
    let chunk: ChunkId = 0xF0_03;
    let group = SegmentGroup::new("5e".repeat(16), 1).unwrap();
    let segment = SegmentRecord::new(vec![rs21(chunk, [3, 1, 2], 40)], 0).unwrap();
    rig.put(
        metadata::seg_key(&group, 0).unwrap(),
        metadata::encode(&segment),
    )
    .await;
    let map = SegmentedMap::new(
        group,
        vec![SegmentRef {
            index: 0,
            byte_offset: 0,
            byte_len: 40,
        }],
    )
    .unwrap();
    let root = InodeRecord {
        size: 40,
        chunk_map: ChunkMap::Segmented(map),
        state: InodeState::Committed,
        version: 1,
        ..InodeRecord::new_empty()
    };
    rig.put(metadata::inode_key(7), metadata::encode(&root))
        .await;
    rig.place(chunk, [3, 1, 2], &[4u8; 40]);
    set_lifecycle(&*rig.meta, 3, DServerLifecycle::Draining)
        .await
        .unwrap();

    let outcome = rig.rebalance().await.expect("the rebalance pass completes");

    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "an evacuation this pass may not perform is refused and withholds certification"
    );
    assert!(
        rig.holds(3, frag(chunk, 0)),
        "nothing is moved by a refusal"
    );
    assert_eq!(
        reconciliation_status(&*rig.meta, 3).await.unwrap(),
        ReconciliationStatus::Pending,
        "the draining server still holds committed content"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg F (segmented)").await;
}

// ---------------------------------------------------------------------------------------------
// Leg G — the ledger walk is bounded per pass, and it converges
// ---------------------------------------------------------------------------------------------

/// The store's lowered per-listing cap (the `crates/metadata-redb/tests/scan.rs` idiom).
const LOWERED_CAP: usize = 64;
/// The retention-safe head: fresh marks, well inside their grace window, with no fragment under
/// them yet — and more of them than any bounded pass reads at this cap.
const HEAD: u64 = 3_000;
/// The actionable tail: lapsed marks over real fragments, sorting after the whole head.
const TAIL: u64 = 20;

/// An `orphan:` population past the store's cap: `scan` of it fails loud, so a GC that reads the
/// ledger with one `scan` aborts the whole reconcile step — before scrub, reconstruction and
/// rebalance ever run. Walked in bounded pages instead, the pass succeeds; no pass reads the whole
/// ledger; and repeated passes **converge**. The fixture is shaped so a walk that restarts at the
/// first page every pass never converges: a head of in-grace marks longer than a pass, then the
/// tail. Each pass is a freshly elected custodian, so the continuation can only live in the store.
#[tokio::test]
async fn g_the_orphan_ledger_is_walked_in_bounded_pages_and_the_tail_converges() {
    let rig = Rig::over(Store::capped(LOWERED_CAP), 2);
    for n in 0..HEAD {
        // Server 0, stamped just before NOW: within grace, and within any late-write deadline.
        mark(
            &rig,
            0,
            frag(0x6_0000 + ChunkId::from(n), 0),
            &(NOW - 10).to_string(),
        )
        .await;
    }
    let tail: Vec<FragmentId> = (0..TAIL)
        .map(|n| frag(0x7_0000 + ChunkId::from(n), 0))
        .collect();
    for &fragment in &tail {
        // Server 1 sorts after server 0: the tail is behind the whole head.
        rig.d[1].hold(fragment, Bytes::from_static(b"reclaimable"));
        mark(&rig, 1, fragment, "0").await;
    }
    let ledger = (HEAD + TAIL) as usize;
    assert_eq!(rig.meta.len_under(metadata::ORPHAN_PREFIX), ledger);

    let mut per_pass = Vec::new();
    let mut passes = 0;
    while tail.iter().any(|&f| rig.holds(1, f)) {
        passes += 1;
        assert!(passes <= 16, "the tail never converged: {per_pass:?}");
        rig.meta.take_reads();
        rig.gc(NOW, GRACE)
            .await
            .expect("a ledger past the store's cap must not abort the reconcile step");
        let read: usize = rig
            .meta
            .take_reads()
            .iter()
            .filter(|read| read.prefix.starts_with(metadata::ORPHAN_PREFIX))
            .map(|read| {
                assert!(
                    read.paged,
                    "the orphan ledger is never read with one `scan`: {read:?}"
                );
                read.keys
            })
            .sum();
        assert!(
            read < ledger,
            "pass {passes} materialised the whole ledger ({read} of {ledger} marks)"
        );
        per_pass.push(read);
    }

    let budget = per_pass[0];
    assert!(
        (budget as u64) < HEAD,
        "fixture: the head must outlast one pass ({budget} marks read in the first)"
    );
    let bound = ledger.div_ceil(budget) + 1;
    assert!(
        passes <= bound,
        "the tail must drain within ceil(ledger / per-pass budget) + 1 = {bound} passes, \
         took {passes}: {per_pass:?}"
    );
    for n in 0..HEAD {
        let fragment = frag(0x6_0000 + ChunkId::from(n), 0);
        assert!(
            rig.get(&metadata::orphan_key(0, fragment)).await.is_some(),
            "an in-grace mark of the head was consumed"
        );
    }
    rig.assert_no_gaps(NOW, GRACE, "leg G").await;
}

// ---------------------------------------------------------------------------------------------
// Leg H — the three orphan: value variants, and the one that is none of them
// ---------------------------------------------------------------------------------------------

/// A `tracing` writer the leg reads back, so "surfaced" is what the pass emitted, not assumed.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'w> tracing_subscriber::fmt::MakeWriter<'w> for Capture {
    type Writer = Self;
    fn make_writer(&'w self) -> Self::Writer {
        self.clone()
    }
}

/// The three shapes an `orphan:` value takes (`0016:1190-1216`, `:1321-1333`), each over an
/// unreferenced fragment whose grace has lapsed: the legacy bare decimal every writer in the tree
/// stamps today, `{ orphaned_at_millis, event }`, and that with `reclaiming: true`. And two values
/// that are none of them.
const LEGACY: &str = "0";
const STRUCTURED: &str = r#"{"orphaned_at_millis":0,"event":"g:9:1"}"#;
const RECLAIMING: &str = r#"{"orphaned_at_millis":0,"event":"g:9:1","reclaiming":true}"#;
const UNDECODABLE: [&str; 2] = [r#"{"orphaned_at_millis":"soon"}"#, "not a mark"];

/// GC decodes all three and reclaims under each — the reclaiming variant by resuming the
/// reclamation already decided. A value that decodes as none of them fails **closed**: the pass
/// leaves its fragment and its key exactly as they were, classifies it, and names it on the
/// audit seam (ADR-0045: rewriting corrupt metadata is the one thing a maintenance loop may never
/// do).
#[tokio::test]
async fn h_gc_decodes_all_three_orphan_value_variants_and_leaves_an_undecodable_one_named() {
    let rig = Rig::new(1);
    let decodable: Vec<FragmentId> = (0..3).map(|n| frag(0x80_00 + n, 0)).collect();
    let quarantined: Vec<FragmentId> = (0..2).map(|n| frag(0x81_00 + n, 0)).collect();
    for (&fragment, value) in decodable.iter().zip([LEGACY, STRUCTURED, RECLAIMING]) {
        rig.d[0].hold(fragment, Bytes::from_static(b"reclaimable"));
        mark(&rig, 0, fragment, value).await;
    }
    for (&fragment, value) in quarantined.iter().zip(UNDECODABLE) {
        rig.d[0].hold(fragment, Bytes::from_static(b"quarantined"));
        mark(&rig, 0, fragment, value).await;
    }

    let audit = Capture::default();
    let logging = tracing::Dispatch::new(
        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(audit.clone()),
        ),
    );
    rig.gc(NOW, GRACE)
        .with_subscriber(logging)
        .await
        .expect("an undecodable mark must not fail the pass");

    for (fragment, variant) in decodable.iter().zip(["legacy", "structured", "reclaiming"]) {
        assert!(
            !rig.holds(0, *fragment),
            "the {variant} orphan value must decode and reclaim its lapsed fragment"
        );
        assert!(
            rig.get(&metadata::orphan_key(0, *fragment)).await.is_none(),
            "the {variant} mark is consumed with its fragment"
        );
    }
    let logged = String::from_utf8(audit.0.lock().unwrap().clone()).unwrap();
    for (fragment, value) in quarantined.iter().zip(UNDECODABLE) {
        assert!(
            rig.holds(0, *fragment),
            "an undecodable mark reclaims nothing"
        );
        assert_eq!(
            rig.get(&metadata::orphan_key(0, *fragment))
                .await
                .as_deref(),
            Some(value.as_bytes()),
            "an undecodable mark is left byte-for-byte as it was"
        );
        let entry = format!(
            r#""entry":"{}""#,
            String::from_utf8(metadata::orphan_key(0, *fragment)).unwrap()
        );
        assert!(
            logged.lines().any(
                |line| line.contains(r#""action":"undecodable-orphan-mark""#)
                    && line.contains(&entry)
            ),
            "the undecodable mark must be named on the audit seam: {logged}"
        );
    }
    let mut left = rig.unclassified(NOW, GRACE).await;
    left.sort_by_key(|(_, f)| f.chunk);
    let expected: Vec<(DServerId, FragmentId)> = quarantined.iter().map(|&f| (0, f)).collect();
    assert_eq!(
        left, expected,
        "leg H: the only fragments in no safe class are the two quarantined ones"
    );
}

/// The post-restore pass, the other consumer of `orphan:` values: a fragment carrying any of the
/// three is already evidenced — never re-marked, its stamp untouched — and one carrying an
/// undecodable value is neither re-marked nor overwritten, but named for a human.
#[tokio::test]
async fn h_restore_reads_all_three_variants_as_evidence_and_never_rewrites_an_undecodable_one() {
    let rig = Rig::new(1);
    let fragments: Vec<FragmentId> = (0..5).map(|n| frag(0x82_00 + n, 0)).collect();
    let values = [
        LEGACY,
        STRUCTURED,
        RECLAIMING,
        UNDECODABLE[0],
        UNDECODABLE[1],
    ];
    for (&fragment, value) in fragments.iter().zip(values) {
        rig.d[0].hold(fragment, Bytes::from_static(b"stray"));
        mark(&rig, 0, fragment, value).await;
    }

    let report = rig.restore(NOW).await.expect("the restore pass completes");

    assert_eq!(report.stranded_marked, 0, "{report:?}");
    assert_eq!(
        report.already_marked, 3,
        "all three decodable variants are evidence already: {report:?}"
    );
    for (&fragment, value) in fragments.iter().zip(values) {
        assert_eq!(
            rig.get(&metadata::orphan_key(0, fragment)).await.as_deref(),
            Some(value.as_bytes()),
            "restore must leave every existing orphan value exactly as it was"
        );
    }
    let rendered = format!("{report:?}");
    for &fragment in &fragments[3..] {
        let key = String::from_utf8(metadata::orphan_key(0, fragment)).unwrap();
        assert!(
            rendered.contains(&key) && report.needs_human(),
            "an undecodable orphan value is a human's, and is named: {rendered}"
        );
    }
    let mut left = rig.unclassified(NOW, GRACE).await;
    left.sort_by_key(|(_, f)| f.chunk);
    let expected: Vec<(DServerId, FragmentId)> = fragments[3..].iter().map(|&f| (0, f)).collect();
    assert_eq!(
        left, expected,
        "leg H (restore): the only fragments in no safe class are the two whose marks no pass can \
         read — left for a human, never rewritten"
    );
}

// ---------------------------------------------------------------------------------------------
// Leg H2 — the GC protocols decision 2 names, each with its own observable
// ---------------------------------------------------------------------------------------------

/// **(a) Keyed pending-retirement protection (X97, `0016:1226-1247`).** A mark names the
/// unreference event that wrote it, and an event's token *is* its obligation's key — so while
/// `retire:bytes:<token>` is pending, the fragments it names are protected by one keyed `get`,
/// never by expanding the obligation. Fifty lapsed marks, each under its own still-pending
/// retirement (a drain that has fallen behind): none is reclaimed, and the pass never reads the
/// `retire:` namespace as a range. Once the obligations drain, the marks' own grace governs.
#[tokio::test]
async fn h2a_a_pending_retirement_protects_its_fragments_by_one_keyed_lookup() {
    let rig = Rig::new(1);
    let marked: Vec<(FragmentId, Vec<u8>)> = (0..50u64)
        .map(|n| {
            let fragment = frag(0x90_00 + ChunkId::from(n), 0);
            let token = RetireToken::Generation {
                inode: 100 + n,
                version: 1,
            };
            (fragment, multipart::retire_key(RetireMode::Bytes, &token))
        })
        .collect();
    for (n, (fragment, obligation)) in marked.iter().enumerate() {
        rig.d[0].hold(*fragment, Bytes::from_static(b"retiring"));
        let event = format!(r#"{{"orphaned_at_millis":0,"event":"g:{}:1"}}"#, 100 + n);
        mark(&rig, 0, *fragment, &event).await;
        let chunk =
            String::from_utf8(metadata::encode(&rs21(fragment.chunk, [0, 1, 2], 40)).to_vec())
                .unwrap();
        let owed = format!(
            r#"{{"generation":{{"inode":{},"version":1,"chunks":[{chunk}]}}}}"#,
            100 + n
        );
        multipart::decode_retire_obligation(obligation, owed.as_bytes())
            .expect("fixture: a canonical generation obligation");
        rig.put(obligation.clone(), owed.into_bytes()).await;
    }
    rig.meta.take_reads();

    rig.gc(NOW, GRACE).await.expect("the GC pass completes");

    for (fragment, _) in &marked {
        assert!(
            rig.holds(0, *fragment),
            "a fragment named by a still-pending retirement must be protected"
        );
    }
    for read in rig.meta.take_reads() {
        assert!(
            !read.prefix.starts_with(b"retire:"),
            "GC expanded the retirement namespace wholesale instead of one keyed lookup per \
             mark: {read:?}"
        );
    }

    // The drain catches up: each obligation is gone, and each mark's own grace now governs.
    for (_, obligation) in &marked {
        rig.meta
            .commit(WriteBatch::new().delete(obligation.clone()))
            .await
            .unwrap();
    }
    rig.gc(NOW, GRACE).await.expect("the GC pass completes");
    for (fragment, _) in &marked {
        assert!(
            !rig.holds(0, *fragment),
            "once its retirement drained, a lapsed structured mark reclaims its fragment"
        );
    }
    rig.assert_no_gaps(NOW, GRACE, "leg H2(a)").await;
}

/// **(b) Reclaim restart (X86, `0016:1321-1333`).** GC records its decision first — the mark
/// CASed to `reclaiming` and committed — and only then deletes the bytes. A crash between the
/// two leaves a durable `reclaiming` value over a fragment still present; the next pass resumes
/// the reclamation and completes it exactly once: one successful delete, and no stranded
/// `reclaiming` key.
#[tokio::test]
async fn h2b_a_reclaim_interrupted_after_its_intent_commits_resumes_exactly_once() {
    let rig = Rig::new(1);
    let fragment = frag(0x91_00, 0);
    rig.d[0].hold(fragment, Bytes::from_static(b"reclaimable"));
    mark(&rig, 0, fragment, LEGACY).await;
    *rig.d[0].fail_next_delete.lock().unwrap() = true;

    assert!(
        rig.gc(NOW, GRACE).await.is_err(),
        "fixture: the D server died under the delete"
    );
    let decided = rig.get(&metadata::orphan_key(0, fragment)).await;
    assert_eq!(
        decided.as_deref().and_then(readable_mark),
        Some((0, true)),
        "the reclamation decision is durable before the bytes go, stamp intact: {decided:?}"
    );
    assert!(
        rig.holds(0, fragment),
        "fixture: the crash left the fragment in place"
    );

    rig.gc(NOW, GRACE)
        .await
        .expect("the resumed pass completes");
    assert!(
        !rig.holds(0, fragment),
        "the resumed pass completes the reclamation"
    );
    assert!(
        rig.get(&metadata::orphan_key(0, fragment)).await.is_none(),
        "no stranded reclaiming key"
    );
    assert_eq!(
        *rig.d[0].deletes.lock().unwrap(),
        1,
        "exactly one delete landed"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg H2(b)").await;
}

/// **(c) The fragment-less mark sweep (X87/X96, `0016:1359-1391`).** A mark whose position no
/// `list_fragments()` ever reports is visited by nothing else, so GC deletes the mark itself —
/// but only once it is older than the late-write deadline **and** the absence was observed after
/// that deadline. A mark still inside the deadline stays; so does one whose fragment landed late
/// but in window (a listing from before the deadline must not sweep it); and so does one on a
/// server the pass cannot see.
#[tokio::test]
async fn h2c_fragment_less_marks_are_swept_only_on_an_absence_observed_past_the_deadline() {
    let rig = Rig::new(2);
    // A teardown's mark over a planned position that never received a fragment.
    let never = frag(0x92_00, 0);
    mark(&rig, 0, never, "0").await;
    // A mark on a server outside the fleet: its absence can never be observed.
    let unseen = frag(0x92_02, 0);
    mark(&rig, 9, unseen, "0").await;
    let grace = 100_000;

    rig.gc(10_000, grace).await.expect("pass 1");
    assert!(
        rig.get(&metadata::orphan_key(0, never)).await.is_some(),
        "inside the late-write deadline a fragment-less mark is kept — a write may still land"
    );

    // A pre-mark, stamped at 40 000, whose destination write will land late — but inside its
    // window.
    let late = frag(0x92_01, 0);
    mark(&rig, 0, late, "40000").await;
    rig.gc(50_000, grace).await.expect("pass 2");
    assert!(
        rig.get(&metadata::orphan_key(0, never)).await.is_none(),
        "past the deadline, with its absence observed after it, the fragment-less mark goes"
    );
    assert!(
        rig.get(&metadata::orphan_key(0, late)).await.is_some(),
        "a mark not yet past its own deadline stays"
    );

    // The late write lands inside its window, after pass 2 listed server 0 empty.
    rig.d[0].hold(late, Bytes::from_static(b"landed late"));
    rig.gc(90_000, grace).await.expect("pass 3");
    assert!(
        rig.get(&metadata::orphan_key(0, late)).await.is_some() && rig.holds(0, late),
        "a stale listing must not sweep the mark of a fragment that has since landed"
    );
    assert!(
        rig.get(&metadata::orphan_key(9, unseen)).await.is_some(),
        "a mark on a server outside the fleet is never swept: its absence was never observed"
    );
    rig.assert_no_gaps(90_000, grace, "leg H2(c)").await;
}

/// The durable marker a completed orphan-identity cleanup leaves (`0016:1259-1273`).
const IDENTITY_CLEANUP_KEY: &[u8] = b"custodian:gc:orphan-identity";

/// **(d) The orphan-identity migration gate (X92, `0016:1222-1224`, `:1259-1273`).** Until a
/// bounded cleanup has cleared the stale marks on still-referenced fragments and left a durable
/// marker saying so, identity-keyed retirement is **disabled** and GC falls back to the safe
/// direction: every mark protects. Here a still-referenced fragment carries a stale mark that is
/// inside its grace window, so the first cleanup sweep cannot finish: no marker, and nothing is
/// reclaimed — not even two lapsed marks on unreferenced fragments. Once the stale mark lapses the
/// sweep drops it and completes; the marker appears and the lapsed marks reclaim.
#[tokio::test]
async fn h2d_identity_keyed_retirement_waits_for_a_durable_cleanup_marker() {
    let rig = Rig::new(2);
    // A live object whose fragment carries a stale mark, stamped before this pass, in grace.
    let live = 0x93_00;
    seed_committed(
        &rig,
        1,
        vec![ChunkRef {
            id: live,
            scheme: EcScheme::None,
            len: 5,
            placement: vec![0],
        }],
    )
    .await;
    rig.d[0].hold(frag(live, 0), Bytes::from_static(b"live"));
    mark(&rig, 0, frag(live, 0), &(NOW - 100).to_string()).await;
    // Two lapsed marks over unreferenced fragments: one legacy, one carrying an event whose
    // retirement has already drained.
    let legacy = frag(0x93_01, 0);
    let evented = frag(0x93_02, 0);
    for (fragment, value) in [
        (legacy, LEGACY),
        (evented, r#"{"orphaned_at_millis":0,"event":"g:77:1"}"#),
    ] {
        rig.d[1].hold(fragment, Bytes::from_static(b"reclaimable"));
        mark(&rig, 1, fragment, value).await;
    }

    rig.gc(NOW, GRACE).await.expect("pass 1");
    assert_eq!(
        rig.get(IDENTITY_CLEANUP_KEY).await,
        None,
        "a cleanup that met a stale mark it could not yet clear has not completed"
    );
    for fragment in [legacy, evented] {
        assert!(
            rig.holds(1, fragment),
            "before the cleanup marker, every mark protects: nothing is reclaimed on one"
        );
    }

    // Past the stale mark's grace: the cleanup drops it and completes.
    let later = NOW + GRACE;
    rig.gc(later, GRACE).await.expect("pass 2");
    assert!(
        rig.get(IDENTITY_CLEANUP_KEY).await.is_some(),
        "a completed cleanup leaves its durable marker"
    );
    assert!(
        rig.get(&metadata::orphan_key(0, frag(live, 0)))
            .await
            .is_none(),
        "the cleanup drops the stale mark on the still-referenced fragment"
    );
    assert!(
        rig.holds(0, frag(live, 0)),
        "and never touches the live bytes"
    );
    for fragment in [legacy, evented] {
        assert!(
            !rig.holds(1, fragment),
            "after the marker, identity-keyed retirement is enabled: lapsed marks reclaim"
        );
    }
    rig.assert_no_gaps(later, GRACE, "leg H2(d)").await;
}

/// **(d), the other arm.** With identity-keyed retirement enabled, a mark carrying a **different**
/// or **legacy** unreference-event identity is re-stamped with the new event's own identity and a
/// fresh grace (`0016:1222-1224`): a staged re-place that vacates a position already carrying a
/// legacy mark leaves that position evidenced under the move's identity, stamped now — never
/// under the ancient stamp.
#[tokio::test]
async fn h2d_a_legacy_mark_meeting_a_new_unreference_event_is_restamped_with_its_identity() {
    let lost = lost_staged_fragment().await;
    let rig = &lost.rig;
    let moved = frag(lost.chunk, 2);
    // The identity gate is open: a clean GC sweep has left its marker.
    rig.gc(NOW, GRACE).await.expect("the cleanup sweep");
    assert!(
        rig.get(IDENTITY_CLEANUP_KEY).await.is_some(),
        "a clean cleanup sweep leaves its durable marker: identity-keyed retirement is enabled"
    );
    // Then the position the repair will vacate is found carrying an ancient legacy mark.
    mark(rig, 2, moved, "7").await;

    rig.reconstruct(&[0, 1, 3])
        .await
        .expect("the reconstruction pass completes");

    let vacated = rig.get(&metadata::orphan_key(2, moved)).await.unwrap();
    let text = String::from_utf8(vacated.to_vec()).unwrap();
    assert!(
        text.starts_with(&format!(r#"{{"orphaned_at_millis":{NOW},"event":"m:"#)),
        "the legacy mark must be re-stamped with the move's own identity and a fresh grace: \
         {text}"
    );
    rig.assert_no_gaps(NOW, GRACE, "leg H2(d) re-stamp").await;
}

// ---------------------------------------------------------------------------------------------
// Leg I — reclamation intent precedes destruction
// ---------------------------------------------------------------------------------------------

/// GC CASes `orphan:<pos>` to `reclaiming` and **commits** before it calls `delete_fragment`
/// (`0016:666`, `:1312-1319`) — the ordering an adoption CAS preconditioned on a pre-mark's bytes
/// depends on. With a store that refuses the commit, the fragment must still exist: a pass that
/// deletes first and records afterwards has already destroyed it.
#[tokio::test]
async fn i_gc_commits_its_reclamation_intent_before_it_deletes_the_bytes() {
    let rig = Rig::new(1);
    let fragment = frag(0xA1_00, 0);
    rig.d[0].hold(fragment, Bytes::from_static(b"reclaimable"));
    mark(&rig, 0, fragment, LEGACY).await;
    rig.meta
        .refuse_commits_touching(Some(metadata::ORPHAN_PREFIX));

    let _ = rig.gc(NOW, GRACE).await;

    assert!(
        rig.holds(0, fragment),
        "the fragment was destroyed although the commit recording its reclamation failed"
    );
    rig.meta.refuse_commits_touching(None);
    rig.assert_no_gaps(NOW, GRACE, "leg I").await;
}
