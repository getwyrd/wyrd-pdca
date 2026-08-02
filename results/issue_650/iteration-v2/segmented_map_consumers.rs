//! Issue #650 (slice 3 of 6 of the #635 re-slicing, 0016 decision 7(e)): **GC and scrub
//! build their reference set through the shared resolver, and both answer honestly over an
//! incomplete one.**
//!
//! The fixture (in-memory trait stores, raw-record `seg:`/root seeding) is adapted from the
//! closed PR's GC/scrub legs, trimmed to what this slice's production code touches —
//! `crates/custodian/src/{gc,scrub,reconciliation,desired_state}.rs`. Restore /
//! reconstruction / rebalance / backfill and their containment legs are #651's; they are not
//! pulled forward, and this file names none of their symbols, so #651 can add its own
//! binding legs without touching it.
//!
//! Every leg drives the real fenced control point
//! [`reconcile_step`](wyrd_custodian::reconcile_step), never a parallel entry — exactly the
//! shape an earlier attempt got wrong (a resolver wired into the read path alone while
//! `gc.rs` kept walking the inline chunk list, so a later GC pass deleted a live segmented
//! object's fragments). The observables are deliberately **positive** — a fragment still on
//! disk, an unrelated fragment genuinely reclaimed, a drain answering `Pending`, a repair
//! obligation actually queued, an audit line naming the record — never "no error was
//! raised", which a pass that did nothing at all also produces.
//!
//! **Why the outcome assertions spell out `!= Satisfied && != Changed` instead of naming the
//! answer:** the third outcome (`Reconciled::Blocked`) is added by the same patch as the fix,
//! so naming it here would make the pre-fix leg a COMPILE error and score a red as a pass.
//! The two base-visible variants are exhaustive before the fix, so excluding both is exactly
//! "the answer this tree cannot give" — assertion-red pre-fix, and no weaker than naming the
//! variant post-fix. The positive matches on `Reconciled::Blocked` and
//! `ReconciliationStatus::PendingUnresolvable` ship in `tests/{gc,scrub,rebalance}.rs`, which
//! the whole-tree gate runs.
//!
//! 1. `segmented_objects_fragments_survive_gc_and_scrub_and_a_drain_answers_pending` —
//!    criterion (1).
//! 2. `one_unreadable_committed_inode_blocks_every_certifying_answer_and_reclaims_nothing` —
//!    criterion (2), including the operator attribution on both audit seams and the
//!    drain-status surface.
//! 3. `one_damaged_object_does_not_end_the_walk_the_rest_of_the_store_is_still_handled` and
//!    `a_structurally_unreadable_committed_root_is_contained_not_propagated` — criterion (3),
//!    over both ways a committed object can be unreadable (its `seg:` records, and its own
//!    root bytes).
//! 4. `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed` — the
//!    other half of criterion (3): a fault that is NOT one object's map still ends the pass.
//! 5. `a_blocked_loop_beside_a_converging_one_still_reports_the_blocked_answer` — the
//!    cross-loop rule: a step never claims more than its weakest loop.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, ChunkMap, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, SegmentGroup,
    SegmentRecord, SegmentRef, SegmentedMap,
};
use wyrd_core::repair;
use wyrd_custodian::desired_state::*;
use wyrd_custodian::{
    reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone, GcContext, ReconcileError,
    Reconciled, ScrubContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait stores (the loops are proven over the seams, backend-agnostic) ----

/// A trivial in-memory metadata store.
///
/// `BTreeMap`, not the `HashMap` the sibling test doubles use: the reference build walks
/// `scan(b"inode:")` in whatever order the store answers in, and the continuation legs below
/// need the DAMAGED record to be met FIRST — otherwise "the healthy object was still
/// handled" could pass on an implementation that abandons the walk at the first blocker,
/// simply because the healthy object had already been handled by then.
#[derive(Default)]
struct MemMeta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
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

    // The required paginated read (#634): a test double needs *a* body, not a backend's —
    // the dev-only testkit helper pages over this store's own `scan` (and therefore inherits
    // `SCAN_CAP`, which a backend may not). Mirrors `crates/custodian/tests/gc.rs:73-80`.
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

/// One D server's fragment bytes — a deliberately dumb `ChunkStore`. Content is never real
/// erasure-coded payload: presence-on-disk (never `delete_fragment`d) is what these legs
/// assert, not checksum validity — scrub's own corruption path is proven in
/// `crates/custodian/tests/scrub.rs`, and dummy content failing `fragment_intact` is exactly
/// what makes "scrub reached and enqueued this fragment" a usable positive observable below.
#[derive(Default)]
struct MemDServer {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
}

impl MemDServer {
    async fn put(&self, frag: FragmentId) {
        self.frags
            .lock()
            .unwrap()
            .insert(frag, Bytes::from_static(b"bytes"));
    }
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

/// The injected fault's exact text, so the propagation leg can prove THIS error came back
/// rather than accepting any error at all (a pre-fix tree fails that read differently).
const STORE_FAULT: &str = "simulated store fault: segment range unreachable";

/// A [`MetadataStore`] that fails one configured `seg:` group's own range with a plain,
/// non-[`metadata::ChunkMapError`] fault — a genuine backend outage while a generation's
/// segments are being read, as opposed to a structural anomaly the resolver itself describes
/// and a maintenance pass recovers by downcast. Everything else delegates unchanged.
struct PoisonedMeta {
    inner: MemMeta,
    poison_prefix: Vec<u8>,
}

#[async_trait]
impl MetadataStore for PoisonedMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        self.inner.scan(prefix).await
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<wyrd_traits::ScanPage> {
        if prefix.starts_with(self.poison_prefix.as_slice()) {
            return Err(Box::new(std::io::Error::other(STORE_FAULT)));
        }
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.inner.commit(batch).await
    }
}

// ---- audit capture (the operator-facing half of the contract) ----

/// A `MakeWriter` collecting what the subscriber emits, so a leg asserts on the record the
/// pass actually produced rather than assuming one exists. The proven in-tree pattern
/// (`crates/core/tests/read_repair.rs:643-697`, `crates/server/tests/custodian_day_one.rs`).
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

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

/// Install a permissive global `tracing` default **once**, so the audit callsites never latch
/// `Interest::never` under the parallel test harness. `tracing` caches each callsite's
/// interest in process-global state the first time it is hit, so a sibling test in this
/// binary that hits `emit_unresolvable` with no subscriber installed could otherwise disable
/// the callsite for the whole process and leave the capture below empty (issue #214; the
/// same guard as `crates/custodian/tests/rebalance.rs`'s `enable_metric_callsites`).
///
/// Called at the top of EVERY test here — all of them drive passes that fire those
/// callsites, so whichever the harness schedules first has to be the one that installs the
/// default, not merely the one leg that reads the capture back.
fn enable_audit_callsites() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}

fn capturing_dispatch(capture: Capture) -> tracing::Dispatch {
    tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json().with_writer(capture)),
    )
}

/// Assert that one pass's audit trail attributes the blocker: the right seam, the right
/// action, and — the part an operator actually needs — the NAME of the record to repair.
fn assert_attributes_blocker(logged: &str, seam: &str, object: &str) {
    assert!(
        logged.contains(&format!(r#""target":"{seam}""#)),
        "the blocker must be reported on {seam} so a collector can select on it. got: {logged}"
    );
    assert!(
        logged.contains(r#""action":"unresolvable-chunk-map""#),
        "the audit line must classify the blocker as an unreadable chunk map. got: {logged}"
    );
    assert!(
        logged.contains(&format!(r#""inode":"{object}""#)),
        "the audit line must NAME the record to repair — a refusal an operator cannot \
         attribute is a stall with no way out. got: {logged}"
    );
}

// ---- fixture: raw-record `seg:` + root seeding (this slice lands no producer of segmented
// maps; #653 owns the real staged-publication committer) ----

const GRACE: u64 = 1_000;

/// `inode:1` sorts BEFORE `inode:2`, so in the mixed-store legs the DAMAGED object is always
/// the one the reference build meets first.
const DAMAGED_INODE: InodeId = 1;
const HEALTHY_INODE: InodeId = 2;

/// The healthy object's segment-group nonce (32 lowercase hex characters, `0016:354`) and
/// the fence epoch its segments are scoped by.
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const EPOCH: u64 = 7;
/// The damaged object's group — its own, so nothing about it is inside the healthy object's
/// bounded `seg:` range.
const DAMAGED_NONCE: &str = "fedcba9876543210fedcba9876543210";
const DAMAGED_EPOCH: u64 = 11;

/// Each fixture chunk is one fragment (`EcScheme::None`) placed on exactly one D server — the
/// smallest shape that still puts a segmented object's fragments on more than one server, so
/// a drain of ONE of them is a meaningful `Pending`.
const CHUNK_LEN: u64 = 5;

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

async fn commit(meta: &MemMeta, batch: WriteBatch) {
    assert_eq!(meta.commit(batch).await.unwrap(), CommitOutcome::Committed);
}

async fn elect(coord: &MemCoordination, zone_name: &str) -> (FencedZone, Custodian) {
    let leader = Custodian::elect(coord, zone_name).await.unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    (zone, leader)
}

/// Seed a committed **segmented** root at `inode` naming `chunks.len()` segments (one chunk
/// each, `(chunk id, placed dserver)`), but WRITE only the first `written` of their `seg:`
/// records. `written == chunks.len()` is the healthy shape; a smaller `written` is the real
/// gap this slice's containment rule exists for — a segment the root's own table names, on a
/// generation it still names, that genuinely never got written
/// (`metadata::ChunkMapError::SegmentAbsent`, as surfaced by `metadata::resolve_chunk_map`).
///
/// Built with the real validating constructors rather than hand-typed JSON, so a fixture typo
/// cannot silently change WHICH rule the leg exercises; the bytes that land are the same ones
/// `metadata::encode` / `metadata::seg_key` put in a store, and the seeding is a raw
/// `WriteBatch` put — never a publish path, since no producer of segmented maps exists yet.
async fn seed_segmented(
    meta: &MemMeta,
    inode: InodeId,
    group: &SegmentGroup,
    chunks: &[(ChunkId, DServerId)],
    written: usize,
) {
    let mut segments = Vec::new();
    let mut offset = 0u64;
    for (index, &(chunk_id, dserver)) in chunks.iter().enumerate() {
        segments.push(SegmentRef {
            index: index as u32,
            byte_offset: offset,
            byte_len: CHUNK_LEN,
        });
        if index < written {
            let chunk_ref = ChunkRef {
                id: chunk_id,
                scheme: EcScheme::None,
                len: CHUNK_LEN,
                placement: vec![dserver],
            };
            let record = SegmentRecord::new(vec![chunk_ref], offset).unwrap();
            let key = metadata::seg_key(group, index as u32).unwrap();
            commit(meta, WriteBatch::new().put(key, metadata::encode(&record))).await;
        }
        offset += CHUNK_LEN;
    }
    let map = SegmentedMap::new(group.clone(), segments).unwrap();
    let root = InodeRecord {
        size: offset,
        chunk_map: ChunkMap::Segmented(map),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    commit(
        meta,
        WriteBatch::new().put(metadata::inode_key(inode), metadata::encode(&root)),
    )
    .await;
}

/// Seed the damaged object used by the containment legs: two segments named, only the first
/// ever written, plus its readable fragment on `d0`. Returns the chunk that fragment belongs
/// to. Asserts the fixture really is unreadable, so a leg can never pass because the fault it
/// was built around silently stopped being one.
async fn seed_damaged(meta: &MemMeta, d0: &MemDServer, a: ChunkId, b: ChunkId) {
    let group = SegmentGroup::new(DAMAGED_NONCE, DAMAGED_EPOCH).unwrap();
    seed_segmented(meta, DAMAGED_INODE, &group, &[(a, 0), (b, 0)], 1).await;
    d0.put(frag(a, 0)).await;

    let root_key = metadata::inode_key(DAMAGED_INODE);
    let root: InodeRecord = metadata::decode(&meta.get(&root_key).await.unwrap().unwrap()).unwrap();
    assert!(
        metadata::resolve_chunk_map(meta, &root_key, &root)
            .await
            .is_err(),
        "fixture: the seeded root's map must genuinely fail to resolve"
    );
}

/// An expired pending lease over `chunk`: real, genuinely collectable garbage, so "nothing
/// was reclaimed" can be told apart from "the pass did nothing at all".
async fn seed_expired_lease(meta: &MemMeta, d0: &MemDServer, chunk: ChunkId) {
    d0.put(frag(chunk, 0)).await;
    metadata::put_pending(
        meta,
        chunk,
        &metadata::PendingEntry {
            lease_expiry_millis: 10,
        },
    )
    .await
    .unwrap();
}

// ---- criterion (1): a segmented object's fragments survive GC + scrub, and a drain of a
// server holding one answers Pending ----

#[tokio::test]
async fn segmented_objects_fragments_survive_gc_and_scrub_and_a_drain_answers_pending() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let chunk_a: ChunkId = 0xA1_00;
    let chunk_b: ChunkId = 0xA2_00;
    seed_segmented(
        &meta,
        HEALTHY_INODE,
        &group,
        &[(chunk_a, 0), (chunk_b, 1)],
        2,
    )
    .await;
    d0.put(frag(chunk_a, 0)).await;
    d1.put(frag(chunk_b, 0)).await;

    // Genuinely collectable garbage, unrelated to the segmented object: it MUST be gone
    // after the pass for the fragments-survive assertions to mean anything — otherwise a GC
    // pass that reclaimed nothing because it did nothing would pass this leg by omission.
    let lease_chunk: ChunkId = 0xE1_00;
    seed_expired_lease(&meta, &d0, lease_chunk).await;

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(0, &d0), (1, &d1)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_ctx = ScrubContext {
        meta: &meta,
        fleet: &fleet,
    };

    // Past the grace window: nothing here is being saved by a window that has not elapsed.
    reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        Some(&scrub_ctx),
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .unwrap();

    // The pass ran for real.
    assert!(
        d0.get_fragment(frag(lease_chunk, 0))
            .await
            .unwrap()
            .is_none(),
        "the pass must have reclaimed the unrelated expired-lease fragment for real"
    );

    // Positive observable (1): every fragment the segmented object owns is still on the D
    // server that holds it.
    assert!(
        d0.get_fragment(frag(chunk_a, 0)).await.unwrap().is_some(),
        "segment 0's chunk must survive GC"
    );
    assert!(
        d1.get_fragment(frag(chunk_b, 0)).await.unwrap().is_some(),
        "segment 1's chunk must survive GC"
    );

    // Positive observable (2): a drain of a server holding a segmented fragment answers
    // Pending, not (wrongly) Satisfied — the leg that catches a build which decodes the
    // segmented shape but never reads the `seg:` range, for which the object owns nothing.
    set_lifecycle(&meta, 0, DServerLifecycle::Draining)
        .await
        .unwrap();
    assert_eq!(
        reconciliation_status(&meta, 0).await.unwrap(),
        ReconciliationStatus::Pending,
        "d0 genuinely holds a referenced segmented fragment; the drain must not certify \
         satisfied"
    );

    // ...and scrub verified those fragments rather than skipping them: dummy content fails
    // `fragment_intact`, so a fetched fragment becomes a durable repair obligation. A
    // reference set that never saw them queues nothing.
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        queued.contains(&chunk_a) && queued.contains(&chunk_b),
        "scrub must have fetched and verified the segmented object's fragments: {queued:?}"
    );
}

// ---- criterion (2): one unreadable committed inode blocks every certifying answer and
// reclaims nothing ----

#[tokio::test]
async fn one_unreadable_committed_inode_blocks_every_certifying_answer_and_reclaims_nothing() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();

    let chunk_a: ChunkId = 0xD1_00;
    let chunk_b: ChunkId = 0xD2_00;
    seed_damaged(&meta, &d0, chunk_a, chunk_b).await;

    // Unrelated, genuinely collectable garbage. The only thing that can keep it is the
    // fail-safe containment rule — an incomplete reference set authorizes NO reclamation
    // anywhere in the fleet, not merely around the object it cannot read: no fragment can be
    // shown not to be one of that object's own, unknown chunks.
    let lease_chunk: ChunkId = 0xD3_00;
    seed_expired_lease(&meta, &d0, lease_chunk).await;

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &d0)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_ctx = ScrubContext {
        meta: &meta,
        fleet: &fleet,
    };

    // GC alone: `Ok(_)`, and neither of the two answers that CERTIFY something. (`Satisfied`
    // claims the store converged; `Changed` claims the pass converged it. Both are claims
    // about a picture with a known hole in it.)
    let gc_audit = Capture::default();
    let gc_outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        None,
        None,
        None,
        GRACE + 1_000,
    )
    .with_subscriber(capturing_dispatch(gc_audit.clone()))
    .await
    .expect("one unreadable object is contained, not an error that ends the pass");
    assert!(
        gc_outcome != Reconciled::Satisfied && gc_outcome != Reconciled::Changed,
        "GC must not report convergence over an incomplete reference set; got {gc_outcome:?}"
    );
    assert_attributes_blocker(
        &gc_audit.contents(),
        "wyrd.custodian.gc.audit",
        "inode:1", // DAMAGED_INODE, as the store spells its key
    );

    // Scrub alone: the identical condition, the identical answer, the same attribution.
    let scrub_audit = Capture::default();
    let scrub_outcome = reconcile_step(&zone, &custodian, None, Some(&scrub_ctx), None, None, 0)
        .with_subscriber(capturing_dispatch(scrub_audit.clone()))
        .await
        .expect("one unreadable object is contained, not an error that ends the pass");
    assert!(
        scrub_outcome != Reconciled::Satisfied && scrub_outcome != Reconciled::Changed,
        "scrub must not certify a store it could only partly read; got {scrub_outcome:?}"
    );
    assert_eq!(
        gc_outcome, scrub_outcome,
        "two passes reading ONE set must give the SAME answer about it — a disagreement is a \
         state an operator cannot resolve from outside"
    );
    assert_attributes_blocker(
        &scrub_audit.contents(),
        "wyrd.custodian.scrub.audit",
        "inode:1",
    );

    // Nothing reclaimed: neither the damaged object's own readable fragment nor the
    // unrelated, otherwise-collectable garbage.
    assert!(
        d0.get_fragment(frag(chunk_a, 0)).await.unwrap().is_some(),
        "the damaged object's own readable fragment must not be reclaimed"
    );
    assert!(
        d0.get_fragment(frag(lease_chunk, 0))
            .await
            .unwrap()
            .is_some(),
        "an unrelated, otherwise-collectable fragment must not be reclaimed while the \
         reference set is incomplete — the containment is fleet-wide, not scoped to the \
         object that has the fault"
    );

    // The third pass that reads this set: the drain-status surface. It must not certify
    // either — "this server holds nothing referenced" over an incomplete set is the same
    // claim as the reclaim, in the form an operator acts on by decommissioning the box —
    // and, like `PendingMalformed`, it must NAME the blocker rather than stall unexplained.
    set_lifecycle(&meta, 0, DServerLifecycle::Draining)
        .await
        .unwrap();
    let status = reconciliation_status(&meta, 0).await.unwrap();
    assert!(
        status != ReconciliationStatus::Satisfied
            && status != ReconciliationStatus::NotRequested
            && status != ReconciliationStatus::Pending,
        "a drain over an incomplete reference set must answer a distinct, attributed \
         blocked state — never `Satisfied`, and never an unexplained `Pending`; got {status:?}"
    );
    assert!(
        format!("{status:?}").contains("inode:1"),
        "the blocked drain answer must name the record to repair; got {status:?}"
    );
}

// ---- criterion (3): one damaged object does not end the walk ----

#[tokio::test]
async fn one_damaged_object_does_not_end_the_walk_the_rest_of_the_store_is_still_handled() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    // The damaged object FIRST (`inode:1`), so the healthy one below is reached only if the
    // build walks past it — the store answers `scan` in key order.
    let damaged_a: ChunkId = 0xD1_01;
    let damaged_b: ChunkId = 0xD2_01;
    seed_damaged(&meta, &d0, damaged_a, damaged_b).await;

    // The healthy object, in its own group, in the SAME store and the same pass.
    let healthy_group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let healthy_a: ChunkId = 0xC1_00;
    let healthy_b: ChunkId = 0xC2_00;
    seed_segmented(
        &meta,
        HEALTHY_INODE,
        &healthy_group,
        &[(healthy_a, 0), (healthy_b, 1)],
        2,
    )
    .await;
    d0.put(frag(healthy_a, 0)).await;
    d1.put(frag(healthy_b, 0)).await;

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(0, &d0), (1, &d1)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_ctx = ScrubContext {
        meta: &meta,
        fleet: &fleet,
    };

    // The walk must COMPLETE — `Ok`, never propagated as an `Err` for the whole store
    // because ONE object's map could not be read.
    let outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        Some(&scrub_ctx),
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .expect("one damaged object must not end the walk for the whole store");
    assert!(
        outcome != Reconciled::Satisfied && outcome != Reconciled::Changed,
        "the incomplete set must not certify; got {outcome:?}"
    );

    // PROTECTED: the healthy object's fragments are still on disk.
    assert!(
        d0.get_fragment(frag(healthy_a, 0)).await.unwrap().is_some(),
        "the healthy object's fragment must still be protected"
    );
    assert!(
        d1.get_fragment(frag(healthy_b, 0)).await.unwrap().is_some(),
        "the healthy object's fragment must still be protected"
    );

    // VERIFIED: scrub reached the healthy object — met SECOND, after the damaged one — and
    // enqueued its (dummy-content, checksum-failing) fragments for repair. That is the proof
    // the reference build iterated PAST the blocker rather than abandoning the scan at it.
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        queued.contains(&healthy_a) && queued.contains(&healthy_b),
        "the healthy object must still be verified past the damaged one: {queued:?}"
    );
}

/// The other shape of "unreadable": the root's OWN bytes will not decode. A `segment_count`
/// that disagrees with the table it carries is rejected at decode (structural invariants
/// surface as errors, never as values — ADR-0045), so this record never reaches the resolver
/// at all. It must be contained exactly the same way: an undecodable record is one object's
/// fault, and ending the walk over it costs every healthy object in the store its protection
/// and its verification.
#[tokio::test]
async fn a_structurally_unreadable_committed_root_is_contained_not_propagated() {
    enable_audit_callsites();
    // Raw stored bytes, necessarily hand-written: the validating constructors exist
    // precisely to make this value unconstructible in process.
    const MISMATCHED_ROOT: &[u8] = br#"{"size":12,"chunk_map":{"group":{"nonce":"fedcba9876543210fedcba9876543210","epoch":11},"segment_count":3,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;

    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    commit(
        &meta,
        WriteBatch::new().put(metadata::inode_key(DAMAGED_INODE), MISMATCHED_ROOT),
    )
    .await;
    assert!(
        metadata::decode::<InodeRecord>(MISMATCHED_ROOT).is_err(),
        "fixture: the seeded root must genuinely fail to decode"
    );

    // A healthy flat object beside it, met SECOND, whose fragment is genuinely collectable
    // garbage on any pass that certifies: it stays put, because an incomplete set authorizes
    // nothing.
    let healthy_chunk: ChunkId = 0xB1_00;
    let record = InodeRecord {
        size: CHUNK_LEN,
        chunk_map: vec![ChunkRef {
            id: healthy_chunk,
            scheme: EcScheme::None,
            len: CHUNK_LEN,
            placement: vec![0],
        }]
        .into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    commit(
        &meta,
        WriteBatch::new().put(
            metadata::inode_key(HEALTHY_INODE),
            metadata::encode(&record),
        ),
    )
    .await;
    d0.put(frag(healthy_chunk, 0)).await;
    let lease_chunk: ChunkId = 0xB2_00;
    seed_expired_lease(&meta, &d0, lease_chunk).await;

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &d0)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_ctx = ScrubContext {
        meta: &meta,
        fleet: &fleet,
    };

    let outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        Some(&scrub_ctx),
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .expect("an undecodable record is one object's fault; it must not end the whole pass");
    assert!(
        outcome != Reconciled::Satisfied && outcome != Reconciled::Changed,
        "a record the pass could not read must block certification; got {outcome:?}"
    );
    assert!(
        d0.get_fragment(frag(lease_chunk, 0))
            .await
            .unwrap()
            .is_some(),
        "nothing is reclaimed while the reference set is incomplete"
    );
    // The walk continued past the undecodable record: the healthy object beside it was still
    // fetched and verified (its dummy content fails `fragment_intact`).
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        queued.contains(&healthy_chunk),
        "the healthy object must still be verified past the undecodable record: {queued:?}"
    );
}

// ---- the other half of criterion (3): a genuine store fault still propagates ----

#[tokio::test]
async fn a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed() {
    enable_audit_callsites();
    let inner = MemMeta::default();
    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let chunk: ChunkId = 0xF1_00;
    seed_segmented(&inner, HEALTHY_INODE, &group, &[(chunk, 0)], 1).await;

    let poison_prefix = metadata::seg_range_prefix(&group);
    let d0 = MemDServer::default();
    d0.put(frag(chunk, 0)).await;
    let meta = PoisonedMeta {
        inner,
        poison_prefix,
    };

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &d0)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };

    let outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        None,
        None,
        None,
        GRACE + 1_000,
    )
    .await;
    // Not merely "an error": THE injected one. A pass that refused the segmented shape
    // outright would also fail this leg with an error of its own, and that must not read as
    // this property holding.
    let Err(ReconcileError::Store(err)) = outcome else {
        panic!("a store fault under the resolve must propagate as a store error: {outcome:?}")
    };
    let reported = err.to_string();
    assert!(
        reported.contains(STORE_FAULT),
        "the fault that comes back must be the injected store fault — a fault that is not \
         one object's map must NOT be folded into 'this object is unreadable'; got {reported}"
    );
}

// ---- the cross-loop rule: a step never claims more than its weakest loop ----

/// Two loops, two stores, two different answers: GC over a store it cannot fully read, scrub
/// over a store where it genuinely converges something (a corrupt fragment enqueued for
/// repair). The step must report the BLOCKED answer, in both loop orders — the converging
/// loop's work is durable in the store either way, while the refusal is the only thing that
/// tells the caller its picture has a hole in it.
#[tokio::test]
async fn a_blocked_loop_beside_a_converging_one_still_reports_the_blocked_answer() {
    enable_audit_callsites();
    let blocked_meta = MemMeta::default();
    let blocked_d0 = MemDServer::default();
    seed_damaged(&blocked_meta, &blocked_d0, 0x91_00, 0x92_00).await;

    // The converging store: a committed reference whose placed fragment is dummy content,
    // so scrub fetches it, fails `fragment_intact`, and enqueues a repair — `Changed`.
    let converging_meta = MemMeta::default();
    let converging_d0 = MemDServer::default();
    let live: ChunkId = 0x93_00;
    let record = InodeRecord {
        size: CHUNK_LEN,
        chunk_map: vec![ChunkRef {
            id: live,
            scheme: EcScheme::None,
            len: CHUNK_LEN,
            placement: vec![0],
        }]
        .into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    commit(
        &converging_meta,
        WriteBatch::new().put(
            metadata::inode_key(HEALTHY_INODE),
            metadata::encode(&record),
        ),
    )
    .await;
    converging_d0.put(frag(live, 0)).await;

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-1").await;
    let blocked_fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &blocked_d0)];
    let converging_fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &converging_d0)];

    // Order A — the blocked loop first: a later converging loop must not overwrite it.
    let gc_blocked = GcContext {
        meta: &blocked_meta,
        fleet: &blocked_fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_converging = ScrubContext {
        meta: &converging_meta,
        fleet: &converging_fleet,
    };
    let outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_blocked),
        Some(&scrub_converging),
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .unwrap();
    assert!(
        outcome != Reconciled::Satisfied && outcome != Reconciled::Changed,
        "a step with one blocked loop cannot report convergence, whatever its other loops \
         did; got {outcome:?}"
    );
    assert_eq!(
        repair::queued_repairs(&converging_meta).await.unwrap(),
        vec![live],
        "the converging loop's work is durable either way — the blocked answer reports the \
         hole, it does not undo the other loop"
    );

    // Order B — the converging loop first: the blocked loop must still lower the answer.
    let gc_converging = GcContext {
        meta: &converging_meta,
        fleet: &converging_fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };
    let scrub_blocked = ScrubContext {
        meta: &blocked_meta,
        fleet: &blocked_fleet,
    };
    let outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_converging),
        Some(&scrub_blocked),
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .unwrap();
    assert!(
        outcome != Reconciled::Satisfied && outcome != Reconciled::Changed,
        "a blocked loop lowers the step's answer whichever order the loops ran in; got \
         {outcome:?}"
    );
}
