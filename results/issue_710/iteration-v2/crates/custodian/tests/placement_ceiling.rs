//! **Issue #710 — a placement write that would not survive is refused, and a move that did
//! not persist does not certify.**
//!
//! Two write-path defects on the **flat** repair/evacuation path, one rule:
//!
//! 1. Every placement mutation is `require(key, encode(prior))` + `put(key, encode(next))`,
//!    and nothing on the write path weighed `encode(next)` against the value ceiling every
//!    backend inherits ([`MAX_VALUE_BYTES`]). A repair that moves a fragment onto a
//!    twenty-digit [`DServerId`] re-encodes the record longer than it was — so a repair could
//!    commit a record the tightest backend would refuse, after which **every** later repair
//!    of that object fails: an object whose placement can never be maintained again.
//! 2. A move that did **not** persist still certified: the pass answered
//!    [`Reconciled::Satisfied`] over a drain that had not moved a byte, which tells an
//!    operator the box is safe to remove.
//!
//! Every leg drives the real fenced control point [`reconcile_step`] and observes the
//! **store**. The ceiling helper and the outcome variants the fix introduces are never named
//! here, so reverting the production change leaves this file compiling and RED (the base's
//! own behaviour) rather than failing to build.
//!
//! Legs:
//! 1. [`a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted`]
//! 2. [`an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain`], with
//!    [`an_evacuation_landing_exactly_on_the_value_ceiling_still_commits`] pinning the
//!    ceiling's admissible side (the refusal is `>`, never `>=`)
//! 3. [`a_ceiling_refused_repair_is_subtracted_from_the_reported_successes`]
//!
//! Roughly half of what follows is harness rather than decision: the in-memory
//! [`MetadataStore`] double every `custodian` integration test rolls for itself (there is no
//! shared one), the fixture builders, and the two pass runners. The D servers are the **real**
//! `chunkstore-fs` backend precisely so this file needs no second double, and every fixture
//! is shared by more than one leg for the same reason.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use tempfile::TempDir;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, ChunkMap, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, MAX_VALUE_BYTES,
};
use wyrd_core::placement::Topology;
use wyrd_core::{erasure, repair, write};
use wyrd_custodian::desired_state::{
    reconciliation_status, set_lifecycle, DServerLifecycle, ReconciliationStatus,
};
use wyrd_custodian::{
    reconcile_step, Custodian, DurabilityTelemetry, ExporterConfig, FencedZone, RebalanceContext,
    Reconciled, ReconstructionContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, MetadataStore, Result, WriteBatch,
};

/// The largest [`DServerId`] there is — the growth vector `crates/core/src/metadata.rs`'s
/// ceiling note names: one moved placement entry re-encoded from a one-digit id to a
/// twenty-digit one is ~19 bytes the record did not carry before.
const HUGE: DServerId = u64::MAX;
/// Every fixture's chunk is RS(2,1): two survivors are enough to rebuild, and one lost
/// fragment is one placement entry to move.
const SCHEME: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };
const K: usize = 2;
const M: usize = 1;
/// The chunk payload. Deliberately tiny — what this file sizes is the **record**, not the
/// data it points at.
const DATA: &[u8] = b"the payload every placement-ceiling fixture stores";
/// A record length comfortably clear of the ceiling, for the roots whose own size is
/// irrelevant to the outcome their plan takes.
const CLEAR_OF_CEILING: usize = 400;

// ---- the doubles: an in-memory metadata store; the D servers are the REAL fs backend ----

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
        let kv = self.kv.lock().unwrap();
        Ok(kv
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
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
        if batch
            .preconditions
            .iter()
            .any(|pre| kv.get(&pre.key).cloned() != pre.expected)
        {
            return Ok(CommitOutcome::Conflict);
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

/// `count` D servers, each the real on-disk [`FsChunkStore`] rooted in its own temp dir —
/// which verifies a fragment's identity and checksum on both `put` and `get`, so a fixture
/// that seeded a fragment the loops would not accept fails loudly here instead of quietly
/// proving nothing. The temp-dir guards are returned so they outlive the pass.
fn dservers(count: usize) -> (Vec<TempDir>, Vec<FsChunkStore>) {
    let dirs: Vec<TempDir> = (0..count).map(|_| tempfile::tempdir().unwrap()).collect();
    let stores = dirs
        .iter()
        .map(|dir| FsChunkStore::open(dir.path()).unwrap())
        .collect();
    (dirs, stores)
}

/// One RS(2,1) `chunk` placed on `placement`.
fn flat(chunk: ChunkId, placement: Vec<DServerId>) -> ChunkMap {
    ChunkMap::Flat(vec![ChunkRef {
        id: chunk,
        scheme: SCHEME,
        len: DATA.len() as u64,
        placement,
    }])
}

/// A committed **flat** root for `chunk`, its ADR-0047 `content_type` — object metadata a
/// placement-maintenance commit preserves verbatim (`..prior.clone()`), so it sizes the
/// record without touching anything the loops read — padded so the whole record encodes to
/// EXACTLY `len` bytes.
fn root(chunk: ChunkId, placement: Vec<DServerId>, version: u64, len: usize) -> InodeRecord {
    let mut record = InodeRecord {
        size: DATA.len() as u64,
        chunk_map: flat(chunk, placement),
        state: InodeState::Committed,
        version,
        etag: None,
        content_type: Some(String::new()),
        modified: None,
    };
    record.content_type = Some("x".repeat(len - metadata::encode(&record).len()));
    record
}

/// Commit `record` at `inode` and store its chunk's real RS(2,1) fragments on the servers its
/// placement names — all but the one at `missing`, this pass's loss. Real shards through the
/// production encoder, so the loops' identity + checksum verify passes on every survivor.
async fn seed(
    meta: &MemMeta,
    fleet: &[(DServerId, &dyn ChunkStore)],
    inode: InodeId,
    record: &InodeRecord,
    missing: Option<usize>,
) {
    meta.commit(WriteBatch::new().put(metadata::inode_key(inode), metadata::encode(record)))
        .await
        .unwrap();
    let chunk = &record.chunk_map.as_flat().unwrap()[0];
    let shards = erasure::encode(K, M, DATA).unwrap();
    for (index, dserver) in chunk.placement.iter().enumerate() {
        if Some(index) == missing {
            continue;
        }
        let frag = FragmentId {
            chunk: chunk.id,
            index: index as u16,
        };
        let bytes =
            write::encode_ec_fragment(frag.chunk, frag.index, K as u8, M as u8, &shards[index]);
        let store = fleet.iter().find(|(id, _)| id == dserver).unwrap().1;
        store.put_fragment(frag, bytes).await.unwrap();
    }
}

async fn elect(coord: &MemCoordination) -> (FencedZone, Custodian) {
    let leader = Custodian::elect(coord, "zone-placement-ceiling")
        .await
        .unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    (zone, leader)
}

/// Install a permissive global `tracing` default once, so the durability metric callsites
/// never latch `Interest::never` under the parallel harness (the loops' own tests do the
/// same).
fn enable_metric_callsites() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}

/// A durability counter's value on the Prometheus surface a deployment scrapes (the exporter
/// suffixes a monotonic counter `_total` and hangs the scope labels off the name; a `#
/// HELP`/`# TYPE` line never starts with the name itself).
fn counter(exposed: &str, name: &str) -> u64 {
    exposed
        .lines()
        .filter(|line| line.starts_with(name))
        .filter_map(|line| line.rsplit_once(' ')?.1.parse::<f64>().ok())
        .map(|value| value as u64)
        .sum()
}

/// The fleet view a context takes: each id paired with its D server's store.
fn fleet<'a>(
    ids: &[DServerId],
    stores: &'a [FsChunkStore],
) -> Vec<(DServerId, &'a dyn ChunkStore)> {
    ids.iter()
        .zip(stores)
        .map(|(id, store)| (*id, store as &dyn ChunkStore))
        .collect()
}

/// One fenced reconstruction pass over `fleet`/`topology`: what it answered, and the
/// durability surface it emitted.
async fn repair_pass(
    meta: &MemMeta,
    fleet: &[(DServerId, &dyn ChunkStore)],
    topology: &Topology,
) -> (Reconciled, String) {
    let ctx = ReconstructionContext {
        meta,
        fleet,
        topology,
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
    telemetry.flush().unwrap();
    (
        outcome,
        telemetry
            .gather_prometheus()
            .expect("Prometheus configured"),
    )
}

// ---- leg 1: a repoint that would cross the value ceiling is refused, not persisted ----

/// A committed flat root sits exactly ON the ceiling (a legal record). Its chunk has lost the
/// fragment on server 1, and the only free distinct domain left holds the twenty-digit
/// [`HUGE`] id — so the repointed record would grow ~19 bytes PAST the ceiling.
///
/// RED on the base: `repair_chunk`'s CAS weighs nothing, so the oversized record COMMITS —
/// `get(inode_key)` then returns bytes longer than the tightest backend would ever have
/// stored, the obligation is drained, and every later repair of that object would fail on the
/// `put`.
#[tokio::test]
async fn a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted() {
    enable_metric_callsites();
    const CHUNK: ChunkId = 0x00C0_FFEE;
    const INODE: InodeId = 1;
    let lost = FragmentId {
        chunk: CHUNK,
        index: 1,
    };

    let meta = MemMeta::default();
    let (_dirs, stores) = dservers(3);
    // Survivors on servers 0 ("a") and 2 ("c"); the lost fragment's server 1 is off the fleet,
    // so the one free distinct domain is "h" — held by the HUGE id.
    let fleet = fleet(&[0, 2, HUGE], &stores);
    let mut topology = Topology::default();
    topology
        .register(0, "a")
        .register(2, "c")
        .register(HUGE, "h");

    let seeded = root(CHUNK, vec![0, 1, 2], 1, MAX_VALUE_BYTES);
    seed(&meta, &fleet, INODE, &seeded, Some(1)).await;
    repair::enqueue_repair(&meta, CHUNK, "health")
        .await
        .unwrap();

    let (outcome, exposed) = repair_pass(&meta, &fleet, &topology).await;
    let stored = meta
        .get(&metadata::inode_key(INODE))
        .await
        .unwrap()
        .unwrap();

    // The binding assertion: an in-memory store has no ceiling of its own, so what proves the
    // refusal is the STORED LENGTH — bytes the tightest backend would have refused.
    let bytes = stored.len();
    assert!(
        bytes <= MAX_VALUE_BYTES,
        "{bytes} bytes stored, past the backend value ceiling"
    );
    assert_eq!(
        stored,
        metadata::encode(&seeded),
        "record not byte-identical"
    );
    // Nothing at all was written: not the record, and not the rebuilt fragment — which GC
    // would otherwise have to hold forever with no grace evidence for it.
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(queued.contains(&CHUNK), "obligation dropped: {queued:?}");
    let on_target = stores[2].get_fragment(lost).await.unwrap();
    assert!(on_target.is_none(), "a refused repair stranded a fragment");
    // A pass that refused a repair has a hole in its redundancy picture, so it certifies
    // nothing — an operator reading `Satisfied` is told redundancy is restored.
    assert_eq!(outcome, Reconciled::Blocked, "the pass certified a refusal");
    assert!(
        exposed.contains("reconstruction_ceiling_refused"),
        "the refusal is not named on the durability audit seam:\n{exposed}"
    );
}

// ---- leg 2: an evacuation that did not persist does not certify the drain ----

/// One drain pass over a committed flat root whose fragment on the draining server can only
/// move to the [`HUGE`]-id server, seeded so the record the move WOULD commit encodes to
/// exactly `moved_len` bytes.
///
/// Returns what the pass answered, the record as stored afterwards, whether the target D
/// server holds the fragment, the drain's own status, and the durability surface.
async fn drain_pass(moved_len: usize) -> (Reconciled, Bytes, bool, ReconciliationStatus, String) {
    enable_metric_callsites();
    const CHUNK: ChunkId = 0x0000_BEEF;
    const INODE: InodeId = 1;
    const DRAIN: DServerId = 1;
    let evacuated = FragmentId {
        chunk: CHUNK,
        index: 1,
    };

    let meta = MemMeta::default();
    let (_dirs, stores) = dservers(4);
    let fleet = fleet(&[0, DRAIN, 2, HUGE], &stores);
    // The fragments that stay hold "a" and "c", and the draining server's own "b" is out of
    // the pool — so the one free distinct domain is "h", held by the HUGE id.
    let mut topology = Topology::default();
    topology
        .register(0, "a")
        .register(DRAIN, "b")
        .register(2, "c")
        .register(HUGE, "h");

    // Pad the record the evacuation WOULD write (the fragment repointed onto the HUGE id,
    // version 2) to exactly `moved_len`, then seed that same record with the placement it has
    // BEFORE the move — so the loop's own re-encode lands exactly on `moved_len`, whatever the
    // id widths cost, rather than on a delta this fixture hard-codes.
    let moved = root(CHUNK, vec![0, HUGE, 2], 2, moved_len);
    let seeded = InodeRecord {
        chunk_map: flat(CHUNK, vec![0, DRAIN, 2]),
        version: 1,
        ..moved
    };
    seed(&meta, &fleet, INODE, &seeded, None).await;
    set_lifecycle(&meta, DRAIN, DServerLifecycle::Draining)
        .await
        .unwrap();

    let ctx = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let telemetry = DurabilityTelemetry::new(ExporterConfig::Prometheus).unwrap();
    let subscriber = tracing_subscriber::registry().with(telemetry.metrics_layer());
    let outcome = reconcile_step(&zone, &custodian, None, None, None, Some(&ctx), 500)
        .with_subscriber(subscriber)
        .await
        .unwrap();
    telemetry.flush().unwrap();

    (
        outcome,
        meta.get(&metadata::inode_key(INODE))
            .await
            .unwrap()
            .unwrap(),
        stores[3].get_fragment(evacuated).await.unwrap().is_some(),
        reconciliation_status(&meta, DRAIN).await.unwrap(),
        telemetry
            .gather_prometheus()
            .expect("Prometheus configured"),
    )
}

/// RED on the base twice over: the oversized repoint COMMITS (so the stored record crosses
/// the ceiling and the fragment is copied to its new home), and even where a move does not
/// persist the pass answers `Satisfied` — certifying a drain that moved nothing.
#[tokio::test]
async fn an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain() {
    let (outcome, stored, on_target, status, exposed) = drain_pass(MAX_VALUE_BYTES + 1).await;
    let record: InodeRecord = metadata::decode(&stored).unwrap();

    // A drain that did not move a byte is not a converging drain: an operator reading
    // `Satisfied` here is being told the box is safe to pull.
    assert_eq!(outcome, Reconciled::Blocked, "a stalled drain certified");
    assert_eq!(status, ReconciliationStatus::Pending, "drain reported done");
    let bytes = stored.len();
    assert!(
        bytes <= MAX_VALUE_BYTES,
        "{bytes} bytes stored, past the backend value ceiling"
    );
    assert_eq!(record.version, 1, "the refused move persisted something");
    assert_eq!(
        record.chunk_map.as_flat().unwrap()[0].placement,
        vec![0, 1, 2],
        "the record was repointed off the draining server anyway"
    );
    // Nothing at all was written — not even the fragment copy, which GC would otherwise have
    // to hold forever with no grace evidence for it.
    assert!(!on_target, "a refused move left a stranded fragment copy");
    assert!(
        exposed.contains("rebalance_ceiling_refused"),
        "the refusal is not named on the durability audit seam:\n{exposed}"
    );
}

/// **The ceiling's admissible side.** A repoint landing EXACTLY on [`MAX_VALUE_BYTES`] is a
/// value every backend stores, so it must still be written: the refusal is `>`, never `>=`.
/// The same fixture, one byte under — the move persists and the drain converges.
#[tokio::test]
async fn an_evacuation_landing_exactly_on_the_value_ceiling_still_commits() {
    let (outcome, stored, on_target, status, _) = drain_pass(MAX_VALUE_BYTES).await;
    let record: InodeRecord = metadata::decode(&stored).unwrap();

    assert_eq!(outcome, Reconciled::Changed, "a legal repoint was withheld");
    assert_eq!(stored.len(), MAX_VALUE_BYTES, "not the record under test");
    assert_eq!(
        record.chunk_map.as_flat().unwrap()[0].placement,
        vec![0, HUGE, 2],
        "the fragment was not repointed off the draining server"
    );
    assert!(on_target, "the evacuated fragment is not at its new home");
    assert_eq!(
        status,
        ReconciliationStatus::Satisfied,
        "drain not converged"
    );
}

// ---- leg 3: a refused move is subtracted, never counted as a success ----

/// One pass, three chunks, three outcomes: one repaired, one refused for crossing the
/// ceiling, one aborted (its only free domain is a ghost the fleet does not hold). The
/// documented durability identity — `reconstruction_repaired − conflict − aborted −
/// ceiling_refused` — must equal the obligations the pass actually drained with a commit, so
/// a refused repair can never inflate the reported successes.
#[tokio::test]
async fn a_ceiling_refused_repair_is_subtracted_from_the_reported_successes() {
    enable_metric_callsites();
    const REPAIRED: ChunkId = 0xC0;
    const REFUSED: ChunkId = 0xEF;
    const ABORTED: ChunkId = 0xAB;
    const GHOST: DServerId = 7;

    let meta = MemMeta::default();
    let (_dirs, stores) = dservers(4);
    let fleet = fleet(&[0, 1, 2, HUGE], &stores);
    // Utilization orders the free domains (cheapest first, ties by label), so three plans
    // select three different targets out of ONE topology. "g" is registered but its server is
    // deliberately NOT in the fleet view.
    let mut topology = Topology::default();
    topology
        .register(0, "a")
        .register(1, "b")
        .register(2, "c")
        .register(GHOST, "g")
        .register(HUGE, "h")
        .set_utilization(2, 0)
        .set_utilization(HUGE, 1)
        .set_utilization(GHOST, 2)
        .set_utilization(0, 3)
        .set_utilization(1, 4);

    // Survivors in "a","b" → the cheapest free domain is "c" (server 2, in the fleet): commits.
    let repaired = root(REPAIRED, vec![0, 1, 2], 1, CLEAR_OF_CEILING);
    seed(&meta, &fleet, 1, &repaired, Some(2)).await;
    // Survivors in "a","c" → the cheapest free domain is "h" (the HUGE id), and this root is
    // already AT the ceiling, so that twenty-digit id is what crosses it: refused.
    let refused = root(REFUSED, vec![0, 1, 2], 1, MAX_VALUE_BYTES);
    seed(&meta, &fleet, 2, &refused, Some(1)).await;
    // Survivors in "c","h" → the cheapest free domain is "g", whose server the fleet does not
    // hold, so the rebuilt shard cannot be placed: aborted.
    let aborted = root(ABORTED, vec![2, HUGE, 1], 1, CLEAR_OF_CEILING);
    seed(&meta, &fleet, 3, &aborted, Some(2)).await;
    for chunk in [REPAIRED, REFUSED, ABORTED] {
        repair::enqueue_repair(&meta, chunk, "health")
            .await
            .unwrap();
    }

    let (outcome, exposed) = repair_pass(&meta, &fleet, &topology).await;
    assert_eq!(outcome, Reconciled::Blocked, "the pass certified a refusal");

    // The committed count, observed independently of the metric: an obligation leaves the
    // queue only in the same atomic batch that repointed its record.
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        queued.len() == 2 && queued.contains(&REFUSED) && queued.contains(&ABORTED),
        "exactly the refused and the aborted obligations stay queued: {queued:?}"
    );
    let committed = 3 - queued.len() as u64;
    // No fragment was displaced by any of the three: the one repair that landed rebuilt onto
    // the domain it had lost, and the other two wrote nothing. So an orphan grace record here
    // would be a GC delete queued against a fragment the committed record still references.
    let orphans = meta.scan(b"orphan:").await.unwrap();
    assert!(orphans.is_empty(), "a referenced fragment was orphaned");

    let successes = counter(&exposed, "reconstruction_repaired")
        .saturating_sub(counter(&exposed, "reconstruction_conflict"))
        .saturating_sub(counter(&exposed, "reconstruction_aborted"))
        .saturating_sub(counter(&exposed, "reconstruction_ceiling_refused"));
    assert_eq!(
        successes, committed,
        "reported successes (repaired − conflict − aborted − ceiling_refused) must equal the \
         commits\n{exposed}"
    );
}
