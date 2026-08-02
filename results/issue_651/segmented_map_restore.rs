//! Issue #651 (slice 4a of 7 of the #635 re-slicing, 0016 decision 7(e)/(f)): **the two
//! surfaces that report whether a reconciliation is COMPLETE answer contained and attributed
//! over a reference set with a hole in it** — post-restore reconciliation
//! (`wyrd_custodian::reconcile_after_restore`) and the drain status
//! (`wyrd_custodian::desired_state::reconciliation_status`).
//!
//! #650 built the containment vocabulary these two consume (`gc::ReferenceSet`, its
//! `unresolvable` map, `protects`) and deferred both of these surfaces here **by name, in its
//! own code**. What was left:
//!
//!   * restore's *report* half re-read every committed record itself and `?`d out on a
//!     segmented map, so ONE segmented object — or one committed record that would not decode
//!     — turned the whole pass into an `Err` and the operator command produced **no report at
//!     all**: not a stranded count, not the dangling/misplaced chunks of the objects it COULD
//!     read; and
//!   * the drain surface answered a bare `Pending` over an incomplete set — the same word it
//!     uses for a server that genuinely still holds referenced fragments — so an operator
//!     watching a decommission stall could not learn WHICH record was blocking it, and the
//!     stall is a state nothing exits.
//!
//! The fixture (in-memory trait stores, raw-record `seg:`/root seeding) is the pruned #650
//! fixture (`crates/custodian/tests/segmented_map_consumers.rs`) — integration-test crates
//! cannot import across files — carrying only what these three criteria need: no loop
//! dispatch, no coordination, no scrub context. The post-restore pass is an operator one-shot,
//! never a loop step, so it is driven directly exactly as `tests/restore_reconcile.rs` drives
//! it.
//!
//! **Why no assertion here names the answer's new shape:** the production files are reverted
//! under the per-fix red leg, so naming a symbol this patch introduces (`RestoreReport`'s new
//! field, a new `ReconciliationStatus` variant) would make that leg fail to COMPILE and the
//! red would degrade to "a symbol is missing" instead of "the behaviour was wrong". Every leg
//! below is therefore expressed in base-visible entries only, and the positive matches on the
//! new shapes ship in `tests/restore_reconcile.rs` and `tests/segmented_map_consumers.rs`,
//! which the whole-tree gate runs.
//!
//! 1. `a_segmented_object_no_longer_stops_the_post_restore_pass` — criterion (1).
//! 2. `an_unreadable_object_is_contained_and_the_run_is_not_certified` — criterion (2a):
//!    non-certification with the incomplete reading as the SOLE cause.
//! 3. `an_unreadable_object_does_not_starve_the_objects_the_pass_could_read` — criterion (2b):
//!    containment — the damaged object costs the readable one's loss nothing.
//! 4. `a_drain_over_an_incomplete_reference_set_names_the_blocking_record` — criterion (3).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_core::metadata::{
    self, ChunkMap, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, SegmentGroup,
    SegmentRecord, SegmentRef, SegmentedMap,
};
use wyrd_custodian::desired_state::{reconciliation_status, set_lifecycle, DServerLifecycle};
use wyrd_custodian::{reconcile_after_restore, ExpiredPendingPolicy, GcContext};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait stores (the pass is proven over the seams, backend-agnostic) ----

/// A trivial in-memory metadata store.
///
/// `BTreeMap`, so `scan` answers in key order and the DAMAGED record (`inode:1`) is always the
/// one the reference build meets FIRST — otherwise "the readable object was still reported"
/// could pass on an implementation that abandons the walk at the first blocker, simply because
/// the readable object had already been handled by then.
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

    // The required paginated read (#634): a test double needs *a* body, not a backend's — the
    // dev-only testkit helper pages over this store's own `scan`.
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
/// erasure-coded payload: what these legs assert is presence on disk and the absence of an
/// `orphan:` record, never checksum validity.
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

// ---- audit capture (the proven in-tree pattern, `crates/core/tests/read_repair.rs`) ----

/// A `MakeWriter` collecting what the subscriber emits, so a leg asserts on the record the
/// surface actually produced rather than assuming one exists.
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
/// `Interest::never` under the parallel test harness: `tracing` caches each callsite's interest
/// in process-global state the first time it is hit, so a sibling test in this binary that hits
/// an audit callsite with no subscriber installed could otherwise disable it for the whole
/// process and leave the capture below empty (issue #214).
///
/// Called at the top of EVERY test here — all of them drive passes that fire those callsites,
/// so whichever the harness schedules first has to be the one that installs the default.
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

/// Every DISTINCT record name an audit trail attributed, read back out of the JSON the
/// subscriber wrote (`"inode":"<name>"`) — a set, and read rather than assumed, because what
/// makes an attribution usable is that two damaged records arrive under two names.
fn attributed_objects(logged: &str) -> BTreeSet<String> {
    logged
        .split(r#""inode":""#)
        .skip(1)
        .filter_map(|rest| rest.split('"').next().map(str::to_owned))
        .collect()
}

/// Assert that a surface's audit trail attributes the blocker: the right seam, the right
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

const NOW: u64 = 10_000;
const GRACE: u64 = 1_000;

/// `inode:1` sorts BEFORE `inode:2`, so in the mixed-store legs the DAMAGED object is always
/// the one the reference build meets first.
const DAMAGED_INODE: InodeId = 1;
const READABLE_INODE: InodeId = 2;
/// `DAMAGED_INODE`'s key, as the store spells it — the name the blocker must reach the
/// operator under.
const DAMAGED_OBJECT: &str = "inode:1";

/// The readable object's segment-group nonce (32 lowercase hex characters, `0016:354`) and the
/// fence epoch its segments are scoped by; the damaged object gets its own group, so nothing
/// about it is inside the readable object's bounded `seg:` range.
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const EPOCH: u64 = 7;
const DAMAGED_NONCE: &str = "fedcba9876543210fedcba9876543210";
const DAMAGED_EPOCH: u64 = 11;

/// Each fixture chunk is one fragment (`EcScheme::None`) placed on exactly one D server — the
/// smallest shape that still puts a segmented object's fragments on more than one server.
const CHUNK_LEN: u64 = 5;

/// A D server holding nothing any valid placement names, so a drain of it is decided by the
/// completeness of the reference set alone rather than by a genuine reference.
const EMPTY_DSERVER: DServerId = 9;

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

async fn commit(meta: &MemMeta, batch: WriteBatch) {
    assert_eq!(meta.commit(batch).await.unwrap(), CommitOutcome::Committed);
}

/// Seed a committed **segmented** root at `inode` naming `chunks.len()` segments (one chunk
/// each, `(chunk id, placed dserver)`), but WRITE only the first `written` of their `seg:`
/// records. `written == chunks.len()` is the readable shape; a smaller `written` is the real
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

/// Seed the object the containment legs are built around: a committed root naming two
/// segments, only the first of which was ever written, plus that first segment's fragment on
/// `d0`. Asserts the fixture really is unreadable, so a leg can never pass because the fault it
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

/// Whether `frag` on `dserver` was handed to GC — the front half of a deletion, and the only
/// durable trace this pass leaves. "Still on disk" alone proves nothing (this pass deletes
/// nothing); an `orphan:` record is the pass saying *these bytes may be reclaimed*.
async fn is_marked_collectable(meta: &MemMeta, dserver: DServerId, frag: FragmentId) -> bool {
    meta.get(&metadata::orphan_key(dserver, frag))
        .await
        .unwrap()
        .is_some()
}

// ---- criterion (1): a segmented object no longer stops the pass ----

/// The pass re-read every committed record through its own scan and refused a segmented map
/// outright, so a store holding ONE segmented object returned `Err` and the operator command
/// produced no report at all.
///
/// The positive observable is the data-loss trap underneath it: a build whose reference set
/// cannot see a segmented object's chunks calls every fragment it owns unreferenced and MARKS
/// them — handing live bytes to GC on its next grace window. So "returned `Ok`" is asserted
/// together with "and marked nothing of it".
#[tokio::test]
async fn a_segmented_object_no_longer_stops_the_post_restore_pass() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let chunk_a: ChunkId = 0xA1_00;
    let chunk_b: ChunkId = 0xA2_00;
    seed_segmented(
        &meta,
        READABLE_INODE,
        &group,
        &[(chunk_a, 0), (chunk_b, 1)],
        2,
    )
    .await;
    d0.put(frag(chunk_a, 0)).await;
    d1.put(frag(chunk_b, 0)).await;

    let fleet: Vec<(DServerId, &dyn ChunkStore)> = vec![(0, &d0), (1, &d1)];
    let ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };

    let report = reconcile_after_restore(&ctx, NOW)
        .await
        .expect("one segmented object must not turn the whole post-restore pass into an error");

    assert_eq!(
        report.stranded_marked, 0,
        "every fragment on disk belongs to the segmented object's committed map: marking one \
         hands live bytes to GC, which is the data-loss trap this pass exists to avoid: \
         {report:?}"
    );
    for (dserver, fragment) in [(0, frag(chunk_a, 0)), (1, frag(chunk_b, 0))] {
        assert!(
            !is_marked_collectable(&meta, dserver, fragment).await,
            "no `orphan:` record may exist for a fragment a committed segmented map still \
             references — GC would reclaim it after the grace window and the data would be GONE"
        );
        assert!(
            fleet[dserver as usize]
                .1
                .get_fragment(fragment)
                .await
                .unwrap()
                .is_some(),
            "the segmented object's fragment must still be on its D server"
        );
    }
    assert!(
        report.dangling.is_empty() && report.misplaced.is_empty(),
        "every fragment of this object sits exactly where its placement names, so a pass that \
         can read the map must report neither loss nor a stale placement: {report:?}"
    );
}

// ---- criterion (2a): the run is not certified, with the incomplete reading the SOLE cause ----

/// An otherwise **fully healthy** store — nothing dangling, nothing misplaced, nothing
/// under-replicated, nothing to mark — plus one committed object whose chunk map cannot be
/// read. The pass must complete, mark nothing, and refuse to call the run clean.
///
/// The "otherwise healthy" part is what makes this leg binding: `is_clean()` is already false
/// whenever any loss is reported, so a scenario carrying one would satisfy it without the fix.
/// Here the incomplete reading is the only thing that can make it false.
#[tokio::test]
async fn an_unreadable_object_is_contained_and_the_run_is_not_certified() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    let damaged_a: ChunkId = 0xD1_00;
    let damaged_b: ChunkId = 0xD2_00;
    seed_damaged(&meta, &d0, damaged_a, damaged_b).await;

    // A wholly healthy object beside it: its every fragment present, at the D server its own
    // placement names.
    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let healthy_a: ChunkId = 0xC1_00;
    let healthy_b: ChunkId = 0xC2_00;
    seed_segmented(
        &meta,
        READABLE_INODE,
        &group,
        &[(healthy_a, 0), (healthy_b, 1)],
        2,
    )
    .await;
    d0.put(frag(healthy_a, 0)).await;
    d1.put(frag(healthy_b, 0)).await;

    let fleet: Vec<(DServerId, &dyn ChunkStore)> = vec![(0, &d0), (1, &d1)];
    let ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };

    let audit = Capture::default();
    let report = reconcile_after_restore(&ctx, NOW)
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("one unreadable object is contained, not an error that blanks the whole report");

    assert_eq!(
        report.stranded_marked, 0,
        "an unreadable map hides WHICH chunks its object owns, so no fragment in the fleet can \
         be shown not to be one of them — nothing may be marked while the set is incomplete: \
         {report:?}"
    );
    assert!(
        !is_marked_collectable(&meta, 0, frag(damaged_a, 0)).await,
        "the damaged object's own readable fragment must not be handed to GC"
    );
    assert!(
        report.dangling.is_empty()
            && report.misplaced.is_empty()
            && report.under_replicated.is_empty(),
        "this store is healthy apart from the record that could not be read — reporting a loss \
         here would make the non-certification below pass for the wrong reason: {report:?}"
    );
    assert!(
        !report.is_clean(),
        "the ONLY thing wrong with this store is that the pass could not read one committed \
         object — and `is_clean` is what the operator command exits on. Certifying a reading \
         that did not finish tells a restore script the run was healthy: {report:?}"
    );
    assert_attributes_blocker(
        &audit.contents(),
        "wyrd.custodian.restore.audit",
        DAMAGED_OBJECT,
    );
}

// ---- criterion (2b): containment — the damaged object does not starve the readable ones ----

/// The same unreadable object, seeded beside a readable object that has a **genuine loss**.
/// Refusing to certify is only half the rule: the pass must still report what it *could* read,
/// or one damaged record blanks the post-restore picture for the whole store.
#[tokio::test]
async fn an_unreadable_object_does_not_starve_the_objects_the_pass_could_read() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    let damaged_a: ChunkId = 0xD1_01;
    let damaged_b: ChunkId = 0xD2_01;
    seed_damaged(&meta, &d0, damaged_a, damaged_b).await;

    // The readable object: chunk `present` still has its fragment, chunk `lost` does not —
    // `EcScheme::None`, so k = 1 and a chunk with no fragment anywhere is unreadable AND
    // unreconstructible. That verdict is exactly what an `Err` over the damaged record used to
    // cost the operator.
    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let present: ChunkId = 0xC1_01;
    let lost: ChunkId = 0xC2_01;
    seed_segmented(&meta, READABLE_INODE, &group, &[(present, 0), (lost, 1)], 2).await;
    d0.put(frag(present, 0)).await;

    let fleet: Vec<(DServerId, &dyn ChunkStore)> = vec![(0, &d0), (1, &d1)];
    let ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };

    let report = reconcile_after_restore(&ctx, NOW)
        .await
        .expect("one unreadable object must not end the walk over the ones that are readable");

    assert!(
        report.dangling.contains(&lost) || report.misplaced.contains(&lost),
        "the readable object's genuine loss must still be reported — withholding it because \
         ANOTHER object could not be read is the failure this containment rule exists to \
         prevent, and it lands at exactly the moment an operator needs the report: {report:?}"
    );
    assert_eq!(
        report.stranded_marked, 0,
        "the containment is fleet-wide: while any committed object is unreadable, nothing may \
         be marked anywhere — not the damaged object's own fragment, not the readable \
         object's: {report:?}"
    );
    assert!(
        !is_marked_collectable(&meta, 0, frag(damaged_a, 0)).await,
        "the damaged object's own readable fragment must not be handed to GC"
    );
    assert!(
        d0.get_fragment(frag(damaged_a, 0)).await.unwrap().is_some(),
        "and its bytes must still be on disk"
    );
}

// ---- criterion (3): the drain surface tells its two "not yet" answers apart ----

/// `reconciliation_status` answered a bare `Pending` over an incomplete reference set — the
/// same word it uses for a server that genuinely still holds referenced fragments. An operator
/// watching a decommission stall could not learn WHICH record was blocking it, so the stall was
/// a state nothing exits: rebalance cannot evacuate fragments of a map it cannot read, so the
/// drain never converges on its own.
///
/// Asserted on the audit seam, which is where an unattributed refusal is observable without
/// naming the answer's new shape (see the module note). The status must still be *answerable* —
/// one damaged object may not turn a per-server operator query into an `Err` fleet-wide.
#[tokio::test]
async fn a_drain_over_an_incomplete_reference_set_names_the_blocking_record() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();

    let damaged_a: ChunkId = 0xD1_02;
    let damaged_b: ChunkId = 0xD2_02;
    seed_damaged(&meta, &d0, damaged_a, damaged_b).await;

    // Drained server: one no VALID committed placement names, so the answer turns on the
    // completeness of the reference set rather than on a genuine reference. This is the moment
    // the surface is most dangerous — it is where it would otherwise say `Satisfied`, i.e.
    // "you may decommission this box".
    set_lifecycle(&meta, EMPTY_DSERVER, DServerLifecycle::Draining)
        .await
        .unwrap();

    let audit = Capture::default();
    let status = reconciliation_status(&meta, EMPTY_DSERVER)
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("one unreadable object must not blank the drain-status surface fleet-wide");
    let logged = audit.contents();

    assert_attributes_blocker(&logged, "wyrd.custodian.drain.audit", DAMAGED_OBJECT);
    assert_eq!(
        attributed_objects(&logged),
        BTreeSet::from([DAMAGED_OBJECT.to_owned()]),
        "the blocker — and only the blocker — must be named: a status that reports records it \
         is not actually blocked by sends the operator to repair the wrong one. \
         got: {status:?} / {logged}"
    );
}
