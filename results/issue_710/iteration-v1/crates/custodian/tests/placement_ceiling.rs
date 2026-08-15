//! Issue #710: two write-path defects on the **flat** repair/evacuation path.
//!
//! 1. A repair or evacuation may repoint a chunk's placement onto a record whose
//!    re-encoded value crosses the backend value ceiling ([`MAX_VALUE_BYTES`]) — the
//!    tightest backend refuses the `put`, and thereafter every repair of that object
//!    fails; a store without native enforcement commits the oversized record, which is
//!    thereafter un-overwritable on a tighter backend. Neither case is a *refusal*.
//! 2. A move that did not persist (aborted, or now refused for crossing the ceiling)
//!    must neither certify the pass `Satisfied` nor inflate the reported success count.
//!
//! Driven only through symbols visible on the base — the ceiling helpers and outcome
//! variants this patch introduces are NEVER named here, so reverting the production
//! change makes the crossing repair/evacuation actually commit (the base's own
//! behaviour) rather than fail to compile.
//!
//! Three legs:
//! 1. [`repair_refuses_a_placement_move_that_would_cross_the_value_ceiling`] — a repair
//!    whose repointed placement would cross the ceiling is refused, not persisted.
//! 2. [`evacuation_that_would_cross_the_value_ceiling_does_not_certify`] — an evacuation
//!    that did not persist does not certify the drain.
//! 3. [`a_ceiling_refused_repair_is_subtracted_from_reported_successes`] — a refused
//!    repair is subtracted from the durability-plane success identity, never counted.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{self, EcScheme, InodeId, InodeRecord, MAX_VALUE_BYTES};
use wyrd_core::placement::Topology;
use wyrd_core::repair;
use wyrd_core::write::write_new_object_placed;
use wyrd_custodian::desired_state::{
    reconciliation_status, set_lifecycle, DServerLifecycle, ReconciliationStatus,
};
use wyrd_custodian::{
    reconcile_step, Custodian, DurabilityTelemetry, ExporterConfig, FencedZone, RebalanceContext,
    Reconciled, ReconstructionContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    PlacementChunkStore, Result, WriteBatch,
};

// ---- in-memory trait stores (backend-agnostic; the loops are proven over the seams,
// the same shape `reconstruction.rs` / `rebalance.rs`'s own tests use) ----

#[derive(Default)]
struct MemMeta {
    kv: Mutex<HashMap<Vec<u8>, Bytes>>,
}

#[async_trait]
impl MetadataStore for MemMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        Ok(self
            .kv
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<wyrd_traits::ScanPage> {
        wyrd_testkit::test_double_scan_page(self, prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let mut kv = self.kv.lock().unwrap();
        for pre in &batch.preconditions {
            if kv.get(&pre.key).cloned() != pre.expected {
                return Ok(CommitOutcome::Conflict);
            }
        }
        for (k, v) in batch.puts {
            kv.insert(k, v);
        }
        for k in batch.deletes {
            kv.remove(&k);
        }
        Ok(CommitOutcome::Committed)
    }
}

/// One D server's fragment bytes — a deliberately dumb `ChunkStore` holding the **real**
/// stored fragment bytes, so their checksums verify.
#[derive(Default)]
struct MemDServer {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
}

#[async_trait]
impl ChunkStore for MemDServer {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        self.frags.lock().unwrap().insert(id, fragment);
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        Ok(self.frags.lock().unwrap().get(&id).cloned())
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        Ok(self.frags.lock().unwrap().keys().copied().collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        self.frags.lock().unwrap().remove(&id);
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

/// A **placement-aware** fleet over several [`MemDServer`]s, for the real write path
/// ([`write_new_object_placed`]) to fan fragments out over.
struct Fleet<'a> {
    servers: Vec<(DServerId, &'a MemDServer)>,
}

impl<'a> Fleet<'a> {
    fn store(&self, dserver: DServerId) -> Option<&'a MemDServer> {
        self.servers
            .iter()
            .find(|(id, _)| *id == dserver)
            .map(|(_, s)| *s)
    }
}

#[async_trait]
impl ChunkStore for Fleet<'_> {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        if let Some(store) = self.store(DServerId::from(id.index)) {
            store.put_fragment(id, fragment).await?;
        }
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        for (_, store) in &self.servers {
            if let Some(bytes) = store.get_fragment(id).await? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        let mut all = Vec::new();
        for (_, store) in &self.servers {
            all.extend(store.list_fragments().await?);
        }
        Ok(all)
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        for (_, store) in &self.servers {
            store.delete_fragment(id).await?;
        }
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

#[async_trait]
impl PlacementChunkStore for Fleet<'_> {
    async fn get_fragment_at(&self, dserver: DServerId, id: FragmentId) -> Result<Option<Bytes>> {
        match self.store(dserver) {
            Some(store) => store.get_fragment(id).await,
            None => Ok(None),
        }
    }

    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        fragment: Bytes,
    ) -> Result<()> {
        if let Some(store) = self.store(dserver) {
            store.put_fragment(id, fragment).await?;
        }
        Ok(())
    }
}

// ---- helpers ----

const ROOT: InodeId = 0;
/// The brief's own example (`crates/core/src/metadata.rs:334-352` growth vector): a
/// small id replaced by the largest `u64` — one moved placement entry grows by ~19
/// bytes.
const HUGE: DServerId = u64::MAX;
/// How far under [`MAX_VALUE_BYTES`] a seeded root is padded: comfortably less than the
/// ~19-byte growth one moved placement entry produces, so the growth reliably crosses
/// the ceiling the record was seeded just under.
const CEILING_MARGIN: usize = 10;

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

async fn elect(coord: &MemCoordination) -> (FencedZone, Custodian) {
    let leader = Custodian::elect(coord, "zone-placement-ceiling")
        .await
        .unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    (zone, leader)
}

async fn read_inode(meta: &MemMeta, id: InodeId) -> InodeRecord {
    let bytes = meta
        .get(&metadata::inode_key(id))
        .await
        .unwrap()
        .expect("inode present");
    metadata::decode(&bytes).unwrap()
}

/// Install a permissive global `tracing` default once, so the durability metric
/// callsites never latch `Interest::never` under the parallel test harness (mirrors
/// `reconstruction.rs` / `rebalance.rs`'s own tests, iteration-4 flake fix).
fn enable_metric_callsites() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}

/// The value of a **counter** metric read back off the Prometheus surface.
fn counter_total(exposed: &str, name: &str) -> u64 {
    let with_suffix = format!("{name}_total");
    exposed
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let key = fields.next()?;
            let value = fields.next()?;
            let metric = key.split('{').next().unwrap_or(key);
            if metric == name || metric == with_suffix {
                value.parse::<f64>().ok().map(|v| v as u64)
            } else {
                None
            }
        })
        .sum()
}

/// Pad `record` via its `content_type` field — ADR-0047 object metadata a
/// placement-maintenance commit PRESERVES verbatim (`..prior.clone()`), so it grows the
/// encoded record without touching the chunk list either loop under test resolves or
/// validates — until the record's own re-encoded length lands EXACTLY `margin` bytes
/// under [`MAX_VALUE_BYTES`]: comfortably less than the ~19-byte growth one moved
/// placement entry (a single-digit id replaced by `u64::MAX`) produces, so that growth
/// reliably crosses the ceiling the record was seeded just under.
fn pad_to_just_under_ceiling(mut record: InodeRecord, margin: usize) -> InodeRecord {
    record.content_type = Some(String::new());
    let base = metadata::encode(&record).len();
    let target = MAX_VALUE_BYTES.saturating_sub(margin);
    let fill = target.saturating_sub(base);
    record.content_type = Some("x".repeat(fill));
    record
}

/// CAS the committed record at `id` to `padded` — the padding step is itself a
/// version-preserving mutation (the test's own seeding, not the loop under test).
async fn commit_padded(meta: &MemMeta, id: InodeId, padded: InodeRecord) {
    let key = metadata::inode_key(id);
    let prior_bytes = meta.get(&key).await.unwrap().unwrap();
    let outcome = meta
        .commit(
            WriteBatch::new()
                .require(key.clone(), prior_bytes)
                .put(key, metadata::encode(&padded)),
        )
        .await
        .unwrap();
    assert_eq!(outcome, CommitOutcome::Committed, "padding CAS must land");
}

fn four_domains() -> Topology {
    let mut t = Topology::default();
    t.register(0, "A").register(1, "B").register(2, "C");
    t
}

/// Write one RS(2,1) chunk via the REAL write path (servers 0,1,2 / domains A,B,C).
async fn write_rs_2_1(
    meta: &MemMeta,
    fleet: &Fleet<'_>,
    id: InodeId,
    name: &str,
    chunk: ChunkId,
) -> Vec<u8> {
    let data = format!("repair chunk {chunk:#x}, every byte of it").into_bytes();
    let topo = four_domains();
    let outcome = write_new_object_placed(
        meta,
        fleet,
        ROOT,
        name,
        id,
        &data,
        data.len(),
        EcScheme::ReedSolomon { k: 2, m: 1 },
        &topo,
        || 0,
        1_000,
        || chunk,
    )
    .await
    .unwrap();
    assert_eq!(outcome, CommitOutcome::Committed);
    assert_eq!(
        read_inode(meta, id).await.chunk_map.as_flat().unwrap()[0].placement,
        vec![0, 1, 2],
        "RS(2,1) placed across distinct domains A,B,C (servers 0,1,2)"
    );
    data
}

/// [`write_rs_2_1`], then pad the committed record just under [`MAX_VALUE_BYTES`].
async fn write_rs_2_1_padded(
    meta: &MemMeta,
    fleet: &Fleet<'_>,
    id: InodeId,
    name: &str,
    chunk: ChunkId,
) -> Vec<u8> {
    let data = write_rs_2_1(meta, fleet, id, name, chunk).await;
    let prior = read_inode(meta, id).await;
    let padded = pad_to_just_under_ceiling(prior, CEILING_MARGIN);
    commit_padded(meta, id, padded).await;
    data
}

// ---- leg 1: a repair that would cross the value ceiling is refused, not persisted ----

/// Base behaviour: `commit_chunk_map`'s CAS has no ceiling check
/// (`crates/core/src/metadata.rs:1741-1768`), so the oversized repointed record commits
/// — `get(inode_key)` then returns bytes whose length **exceeds** [`MAX_VALUE_BYTES`].
/// Reverting the ceiling check (`metadata::flat_value_ceiling_crossed`, introduced by
/// this patch) reproduces exactly that: RED.
#[tokio::test]
async fn repair_refuses_a_placement_move_that_would_cross_the_value_ceiling() {
    enable_metric_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let fleet = Fleet {
        servers: vec![(0, &d0), (1, &d1), (2, &d2)],
    };

    const INODE: InodeId = 1;
    const CHUNK: ChunkId = 0xC0FFEE;

    // Hand-seed a committed FLAT root just under MAX_VALUE_BYTES: a real RS(2,1) chunk
    // on small-id D servers 0,1,2 (domains A,B,C).
    write_rs_2_1_padded(&meta, &fleet, INODE, "obj", CHUNK).await;
    let seeded_len = metadata::encode(&read_inode(&meta, INODE).await).len();
    assert!(
        seeded_len < MAX_VALUE_BYTES,
        "the seeded root must start UNDER the ceiling; got {seeded_len} bytes"
    );

    // Lose domain B (server 1, id "1"): survivors on A (server 0), C (server 2).
    d1.delete_fragment(frag(CHUNK, 1)).await.unwrap();
    repair::enqueue_repair(&meta, CHUNK, "health")
        .await
        .unwrap();

    // The only free domain (distinct from the survivors' A, C) is a brand-new one
    // holding the HUGE DServerId — so the repointed placement entry grows from the
    // 1-digit id "1" to `u64::MAX` (20 digits), ~19 bytes, crossing the ceiling this
    // record was seeded just under.
    let d_huge = MemDServer::default();
    let mut healthy_topo = Topology::default();
    healthy_topo
        .register(0, "A")
        .register(2, "C")
        .register(HUGE, "HUGE_DOMAIN");
    let healthy_fleet: [(DServerId, &dyn ChunkStore); 3] = [(0, &d0), (2, &d2), (HUGE, &d_huge)];
    let ctx = ReconstructionContext {
        meta: &meta,
        fleet: &healthy_fleet,
        topology: &healthy_topo,
        unreachable: &[],
    };

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let telemetry = DurabilityTelemetry::new(ExporterConfig::Prometheus).unwrap();
    let subscriber = tracing_subscriber::registry().with(telemetry.metrics_layer());

    let prior_bytes = meta
        .get(&metadata::inode_key(INODE))
        .await
        .unwrap()
        .unwrap();
    let outcome = reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, 500)
        .with_subscriber(subscriber)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "a repair refused for crossing the value ceiling must not certify the pass"
    );

    let post_bytes = meta
        .get(&metadata::inode_key(INODE))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        post_bytes, prior_bytes,
        "the refused repoint writes nothing: the record is byte-identical"
    );
    assert!(
        post_bytes.len() <= MAX_VALUE_BYTES,
        "the stored record must never cross the backend value ceiling; got {} bytes",
        post_bytes.len()
    );

    let remaining = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        remaining.contains(&CHUNK),
        "the obligation stays queued; got {remaining:?}"
    );

    telemetry.flush().unwrap();
    let exposed = telemetry
        .gather_prometheus()
        .expect("Prometheus surface configured");
    assert!(
        exposed.contains("reconstruction_ceiling_refused"),
        "the refusal is named on the durability audit seam; got:\n{exposed}"
    );
}

// ---- leg 2: an evacuation that would cross the value ceiling does not certify ----

/// Base behaviour: `rebalance::evacuate_chunk`'s CAS has no ceiling check either, so the
/// oversized repointed record commits and the drain answers `Satisfied` over a move that
/// actually happened — RED (this assertion would pass, wrongly, on base). Reverting the
/// ceiling check reproduces exactly that.
#[tokio::test]
async fn evacuation_that_would_cross_the_value_ceiling_does_not_certify() {
    enable_metric_callsites();
    let meta = MemMeta::default();
    let d_drain = MemDServer::default();

    const INODE: InodeId = 1;
    const CHUNK: ChunkId = 0xBEEF;
    const DRAIN: DServerId = 5;

    let fleet = Fleet {
        servers: vec![(DRAIN, &d_drain)],
    };
    let mut write_topo = Topology::default();
    write_topo.register(DRAIN, "A");
    let data = b"evacuate this single-fragment chunk, every byte of it".to_vec();
    let outcome = write_new_object_placed(
        &meta,
        &fleet,
        ROOT,
        "obj",
        INODE,
        &data,
        data.len(),
        EcScheme::None,
        &write_topo,
        || 0,
        1_000,
        || CHUNK,
    )
    .await
    .unwrap();
    assert_eq!(outcome, CommitOutcome::Committed);
    assert_eq!(
        read_inode(&meta, INODE).await.chunk_map.as_flat().unwrap()[0].placement,
        vec![DRAIN],
        "the single fragment placed on the (soon-draining) server"
    );

    // Hand-seed the record just under MAX_VALUE_BYTES via its `content_type` field.
    let prior = read_inode(&meta, INODE).await;
    let padded = pad_to_just_under_ceiling(prior, CEILING_MARGIN);
    commit_padded(&meta, INODE, padded).await;

    set_lifecycle(&meta, DRAIN, DServerLifecycle::Draining)
        .await
        .unwrap();

    // The only non-draining candidate holds the HUGE DServerId, so the repointed
    // placement entry grows from the 1-digit id "5" to `u64::MAX` (20 digits), ~19
    // bytes, crossing the ceiling this record was seeded just under.
    let d_huge = MemDServer::default();
    let mut topo = Topology::default();
    topo.register(DRAIN, "A").register(HUGE, "HUGE_DOMAIN");
    let dyn_fleet: [(DServerId, &dyn ChunkStore); 2] = [(DRAIN, &d_drain), (HUGE, &d_huge)];
    let ctx = RebalanceContext {
        meta: &meta,
        fleet: &dyn_fleet,
        topology: &topo,
    };

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let telemetry = DurabilityTelemetry::new(ExporterConfig::Prometheus).unwrap();
    let subscriber = tracing_subscriber::registry().with(telemetry.metrics_layer());

    let key = metadata::inode_key(INODE);
    let prior_bytes = meta.get(&key).await.unwrap().unwrap();
    let outcome = reconcile_step(&zone, &custodian, None, None, None, Some(&ctx), 500)
        .with_subscriber(subscriber)
        .await
        .unwrap();

    assert!(
        !matches!(outcome, Reconciled::Satisfied),
        "a move refused for crossing the value ceiling must not certify the drain; got {outcome:?}"
    );

    let post_bytes = meta.get(&key).await.unwrap().unwrap();
    assert_eq!(
        post_bytes, prior_bytes,
        "the refused move writes nothing: the record is byte-identical"
    );
    assert!(
        post_bytes.len() <= MAX_VALUE_BYTES,
        "the stored record must never cross the backend value ceiling; got {} bytes",
        post_bytes.len()
    );

    assert!(
        d_drain
            .get_fragment(frag(CHUNK, 0))
            .await
            .unwrap()
            .is_some(),
        "the fragment is still on the draining server — nothing moved"
    );
    assert_eq!(
        reconciliation_status(&meta, DRAIN).await.unwrap(),
        ReconciliationStatus::Pending,
        "the drain remains unsatisfied — the server is still genuinely referenced"
    );

    telemetry.flush().unwrap();
    let exposed = telemetry
        .gather_prometheus()
        .expect("Prometheus surface configured");
    assert!(
        exposed.contains("rebalance_ceiling_refused"),
        "the refusal is named on the durability audit seam; got:\n{exposed}"
    );
}

// ---- leg 3: a refused move is subtracted, never counted as a success ----

/// The documented `reconstruction_repaired − conflict − aborted` identity, extended by
/// this patch to `− ceiling_refused`, must still hold over a pass mixing one repaired,
/// one aborted, and one ceiling-refused chunk. Not independently discriminating (on
/// base the would-be-refused repair simply commits its oversized record and *correctly*
/// counts as a success, so the identity holds there too) — it is red only as a
/// derivative of leg 1's `remaining`/version assertions below.
#[tokio::test]
async fn a_ceiling_refused_repair_is_subtracted_from_reported_successes() {
    enable_metric_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d_huge) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );

    const INODE_COMMIT: InodeId = 1;
    const INODE_ABORT: InodeId = 2;
    const INODE_REFUSED: InodeId = 3;
    const CHUNK_COMMIT: ChunkId = 0xC0;
    const CHUNK_ABORT: ChunkId = 0xAB;
    const CHUNK_REFUSED: ChunkId = 0xEF;
    const GHOST: DServerId = 7;

    // COMMIT and REFUSED: plain RS(2,1) on domains A,B,C (servers 0,1,2).
    let fleet3 = Fleet {
        servers: vec![(0, &d0), (1, &d1), (2, &d2)],
    };
    write_rs_2_1(&meta, &fleet3, INODE_COMMIT, "commit", CHUNK_COMMIT).await;
    write_rs_2_1_padded(&meta, &fleet3, INODE_REFUSED, "refused", CHUNK_REFUSED).await;

    // ABORT: RS(3,1) on domains A,B,C,HUGE_DOMAIN (servers 0,1,2,HUGE) — the HUGE
    // fragment is a genuine, never-lost survivor from the start, so ABORT's OWN
    // free-domain search never even considers the huge-id domain: it excludes REFUSED's
    // growth target from ABORT's abort (and vice versa), so one shared reconstruction
    // topology can drive all three outcomes deterministically in one pass.
    let fleet4 = Fleet {
        servers: vec![(0, &d0), (1, &d1), (2, &d2), (HUGE, &d_huge)],
    };
    let mut abort_write_topo = Topology::default();
    abort_write_topo
        .register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(HUGE, "HUGE_DOMAIN");
    let abort_data = b"abort chunk, every byte of it".to_vec();
    let outcome = write_new_object_placed(
        &meta,
        &fleet4,
        ROOT,
        "abort",
        INODE_ABORT,
        &abort_data,
        abort_data.len(),
        EcScheme::ReedSolomon { k: 3, m: 1 },
        &abort_write_topo,
        || 0,
        1_000,
        || CHUNK_ABORT,
    )
    .await
    .unwrap();
    assert_eq!(outcome, CommitOutcome::Committed);
    assert_eq!(
        read_inode(&meta, INODE_ABORT)
            .await
            .chunk_map
            .as_flat()
            .unwrap()[0]
            .placement,
        vec![0, 1, 2, HUGE],
        "RS(3,1) placed across all four registered domains, HUGE included as a real survivor"
    );

    // Differential loss, so the three plans take DIFFERENT outcomes in one pass:
    //   * COMMIT loses domain C (server 2) -> survivors A,B -> the cheapest free domain
    //     is C itself (utilization 0) -> Committed.
    //   * ABORT loses domain B (server 1) -> survivors A,C,HUGE_DOMAIN (HUGE_DOMAIN
    //     already its own survivor, excluded from its OWN free-domain search) -> its
    //     only remaining free domain is the ghost (util 500, NOT in the fleet) ->
    //     Aborted.
    //   * REFUSED loses domain A (server 0) -> survivors B,C (C is REFUSED's own
    //     survivor, excluded from ITS free-domain search; A is loaded, utilization
    //     1000) -> the selector's only cheap candidate left is HUGE_DOMAIN (utilization
    //     0) -> the commit is attempted, but the padded record crosses the ceiling ->
    //     Refused.
    d2.delete_fragment(frag(CHUNK_COMMIT, 2)).await.unwrap();
    d1.delete_fragment(frag(CHUNK_ABORT, 1)).await.unwrap();
    d0.delete_fragment(frag(CHUNK_REFUSED, 0)).await.unwrap();
    repair::enqueue_repair(&meta, CHUNK_COMMIT, "health")
        .await
        .unwrap();
    repair::enqueue_repair(&meta, CHUNK_ABORT, "health")
        .await
        .unwrap();
    repair::enqueue_repair(&meta, CHUNK_REFUSED, "health")
        .await
        .unwrap();

    let mut topo = Topology::default();
    topo.register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(GHOST, "GHOST")
        .register(HUGE, "HUGE_DOMAIN")
        .set_utilization(0, 1000)
        .set_utilization(1, 1000)
        .set_utilization(GHOST, 500);
    let recon_fleet: [(DServerId, &dyn ChunkStore); 4] =
        [(0, &d0), (1, &d1), (2, &d2), (HUGE, &d_huge)];
    let ctx = ReconstructionContext {
        meta: &meta,
        fleet: &recon_fleet,
        topology: &topo,
        unreachable: &[],
    };

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let telemetry = DurabilityTelemetry::new(ExporterConfig::Prometheus).unwrap();
    let subscriber = tracing_subscriber::registry().with(telemetry.metrics_layer());

    let outcome = reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, 500)
        .with_subscriber(subscriber)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "the ceiling-refused plan withholds certification even though the commit plan changed something"
    );

    // Observe the committed count independently of the metric.
    let remaining = repair::queued_repairs(&meta).await.unwrap();
    assert_eq!(
        remaining.len(),
        2,
        "the aborted and refused obligations both stay queued; got {remaining:?}"
    );
    assert!(remaining.contains(&CHUNK_ABORT), "got {remaining:?}");
    assert!(remaining.contains(&CHUNK_REFUSED), "got {remaining:?}");
    assert_eq!(
        read_inode(&meta, INODE_COMMIT).await.version,
        2,
        "the committed plan bumped its inode with one version-conditional commit"
    );
    assert_eq!(
        read_inode(&meta, INODE_ABORT).await.version,
        1,
        "the aborted plan committed nothing (its inode is unchanged)"
    );
    assert_eq!(
        read_inode(&meta, INODE_REFUSED).await.version,
        1,
        "the refused plan committed nothing — its record is byte-identical"
    );
    let committed_count = 3 - remaining.len() as u64; // 3 obligations enqueued
    assert_eq!(committed_count, 1, "exactly one plan committed");

    // BINDING: the telemetry's derived successes equal the committed count.
    telemetry.flush().unwrap();
    let exposed = telemetry
        .gather_prometheus()
        .expect("Prometheus surface configured");
    let repaired = counter_total(&exposed, "reconstruction_repaired");
    let conflict = counter_total(&exposed, "reconstruction_conflict");
    let aborted = counter_total(&exposed, "reconstruction_aborted");
    let ceiling_refused = counter_total(&exposed, "reconstruction_ceiling_refused");
    let derived_successes = repaired
        .saturating_sub(conflict)
        .saturating_sub(aborted)
        .saturating_sub(ceiling_refused);
    assert_eq!(
        derived_successes, committed_count,
        "successful repairs (reconstruction_repaired − conflict − aborted − ceiling_refused) \
         must equal the committed count: got repaired={repaired} conflict={conflict} \
         aborted={aborted} ceiling_refused={ceiling_refused} (derived {derived_successes}) \
         vs committed {committed_count}\n{exposed}"
    );
}
