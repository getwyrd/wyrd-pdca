//! Issue #650 (slice 3 of 6 of the #635 re-slicing, 0016 decision 7(e)): **GC and
//! scrub route the reference build through the shared resolver, and both certify
//! honestly over an incomplete result.**
//!
//! The fixture (in-memory trait stores, raw-record `seg:`/root seeding) is adapted
//! from `results/issue_650/sources/salvage.diff`'s GC/scrub legs, trimmed to what
//! this slice's production code touches — `crates/custodian/src/{gc,scrub,
//! reconciliation}.rs`. Restore/reconstruction/rebalance/backfill and their
//! containment legs are #651's; they are not pulled forward here, and this file is
//! kept free of their symbols so #651 can add its own binding legs without touching
//! this one.
//!
//! Every leg is driven through the real fenced control point
//! [`reconcile_step`](wyrd_custodian::reconcile_step), never a parallel entry —
//! exactly the shape #508's fourth attempt got wrong (a resolver wired into the read
//! path alone while `gc.rs` kept walking the inline field, so a later GC pass deleted
//! a live segmented object's fragments). The observables are deliberately
//! **positive** — a fragment still on disk, a drain answering `Pending`, a repair
//! obligation actually queued — never "no error was raised" (a pass that did nothing
//! also produces no error).
//!
//! 1. `segmented_objects_fragments_survive_gc_and_scrub_and_a_drain_answers_pending`
//!    — criterion (1): past the grace window, every fragment a segmented object owns
//!    is still on its D server, and a drain of a server holding one answers
//!    [`ReconciliationStatus::Pending`]. An unrelated, genuinely-collectable lease is
//!    reclaimed in the SAME pass, so "GC deleted nothing" cannot pass this test by
//!    having done nothing at all.
//! 2. `one_unresolvable_committed_inode_blocks_certification_and_reclaims_nothing` —
//!    criterion (2): a committed object whose second `seg:` record is genuinely
//!    absent. GC alone and scrub alone (`reconcile_step` supplying only one context
//!    at a time) both return `Ok(_)` and **not** `Reconciled::Satisfied`; nothing —
//!    not the damaged object's own readable fragment, not an unrelated,
//!    otherwise-collectable one — is reclaimed anywhere in the fleet, because an
//!    incomplete set authorizes no reclamation at all (not merely one scoped around
//!    the object it cannot read). `Reconciled::Blocked` is added by this same patch,
//!    so it is never named here — naming it would compile-fail on the reverted tree
//!    and score a RED leg as a pass; `!= Satisfied` is the assertion the reverted
//!    tree can still run and fail.
//! 3. `one_damaged_object_does_not_end_the_walk_the_rest_of_the_store_is_still_handled`
//!    — criterion (3): the damaged object from (2) sits in the SAME store as a
//!    healthy segmented object. The pass still completes (`Ok`, never propagated as
//!    an `Err` for the whole store), the healthy object's fragments are still
//!    present, and scrub still verified them (queued their dummy, checksum-failing
//!    content for repair) — proof the reference build iterated past the damaged
//!    object rather than aborting the whole scan the moment it was met.
//! 4. `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed` —
//!    the other half of criterion (3)'s prose: a store fault that is NOT the
//!    resolver's own typed verdict about one object's map (a plain, non-
//!    `ChunkMapError` failure injected into the `seg:` range read) still propagates
//!    as `Err`, proving the new downcast branch in `gc::referenced_fragments` does
//!    not fold every failure into "unresolvable".

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, ChunkMap, ChunkRef, EcScheme, InodeId, InodeRecord, InodeState, SegmentGroup,
    SegmentRecord, SegmentRef, SegmentedMap,
};
use wyrd_core::repair;
use wyrd_custodian::desired_state::*;
use wyrd_custodian::{
    reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone, GcContext, Reconciled,
    ScrubContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait stores (the loops are proven over the seams, backend-agnostic) ----

/// A trivial in-memory metadata store.
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

    // The required paginated read (#634): a test double needs *a* body, not a
    // backend's — the dev-only testkit helper pages over this store's own `scan`
    // (and therefore inherits `SCAN_CAP`, which a backend may not). Mirrors
    // `crates/custodian/tests/gc.rs:73-80`.
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

/// One D server's fragment bytes — a deliberately dumb `ChunkStore`. Content is
/// never real erasure-coded payload: presence-on-disk (never `delete_fragment`d) is
/// what every leg here asserts, not checksum validity — scrub's own corruption path
/// is proven elsewhere (`crates/custodian/tests/scrub.rs`), and dummy content
/// failing `fragment_intact` is exactly what makes "scrub reached and enqueued this
/// fragment" a usable positive observable in leg 3.
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

/// A [`MetadataStore`] that fails one configured `seg:` group's own range with a
/// plain, non-[`metadata::ChunkMapError`] fault — the shape of a genuine backend
/// outage while resolving a generation's segments, as opposed to a structural
/// anomaly the resolver itself describes and recovers by downcast
/// (`crates/core/src/metadata.rs:2639`). Everything else delegates to `inner`
/// unchanged.
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
            return Err(Box::new(std::io::Error::other(
                "simulated store fault: segment range unreachable",
            )));
        }
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.inner.commit(batch).await
    }
}

// ---- fixture: raw-record `seg:` + root seeding (no producer of segmented maps —
// this slice lands none; #653 owns the real publisher) ----

const GRACE: u64 = 1_000;

const SEGMENTED_INODE: InodeId = 1;
const DAMAGED_INODE: InodeId = 2;

/// The healthy object's segment-group nonce (32 lowercase hex characters, `0016:354`)
/// and the `Completing` fence epoch its segments are scoped by.
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const EPOCH: u64 = 7;
/// The damaged object's group — its own, so nothing about it is inside the healthy
/// object's bounded `seg:` range.
const DAMAGED_NONCE: &str = "fedcba9876543210fedcba9876543210";
const DAMAGED_EPOCH: u64 = 11;

/// Each fixture chunk is one fragment (`EcScheme::None`), placed on exactly one
/// D server — the smallest shape that still puts a segmented object's fragments on
/// more than one server (so a drain of ONE of them is a meaningful `Pending`).
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

/// Seed a committed **segmented** root at `inode`, naming `chunks.len()` segments (one
/// chunk each, `(chunk id, placed dserver)`), but WRITE only the first `written` of
/// their `seg:` records. `written == chunks.len()` is the healthy shape; a smaller
/// `written` is the real gap this slice's containment rule exists for — a segment the
/// root's own table names, on a generation it still names, that genuinely never got
/// written (`metadata::ChunkMapError::SegmentAbsent`, surfaced through
/// [`metadata::resolve_chunk_map`]).
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

// ---- criterion (1): a segmented object's fragments survive GC + scrub, and a
// drain of a server holding one answers Pending ----

#[tokio::test]
async fn segmented_objects_fragments_survive_gc_and_scrub_and_a_drain_answers_pending() {
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let chunk_a: ChunkId = 0xA1_00;
    let chunk_b: ChunkId = 0xA2_00;
    seed_segmented(
        &meta,
        SEGMENTED_INODE,
        &group,
        &[(chunk_a, 0), (chunk_b, 1)],
        2,
    )
    .await;
    d0.put(frag(chunk_a, 0)).await;
    d1.put(frag(chunk_b, 0)).await;

    // Input (1) — an expired pending lease, unrelated to the segmented object: real,
    // genuinely-collectable garbage. This MUST be gone after the pass for the
    // fragments-survive assertion below to mean anything — otherwise a GC pass that
    // reclaimed nothing at all (because it did nothing at all) would pass this test
    // by omission, exactly the false positive the brief's criterion (1) warns of.
    let lease_chunk: ChunkId = 0xE1_00;
    d0.put(frag(lease_chunk, 0)).await;
    metadata::put_pending(
        &meta,
        lease_chunk,
        &metadata::PendingEntry {
            lease_expiry_millis: 10,
        },
    )
    .await
    .unwrap();

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

    // Past the grace window.
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

    // The pass ran for real: the unrelated collectable garbage is gone.
    assert!(
        d0.get_fragment(frag(lease_chunk, 0))
            .await
            .unwrap()
            .is_none(),
        "the pass must have reclaimed the unrelated expired-lease fragment for real"
    );

    // The positive observable (1): every fragment the segmented object owns is still
    // on the D server that holds it.
    assert!(
        d0.get_fragment(frag(chunk_a, 0)).await.unwrap().is_some(),
        "segment 0's chunk must survive GC"
    );
    assert!(
        d1.get_fragment(frag(chunk_b, 0)).await.unwrap().is_some(),
        "segment 1's chunk must survive GC"
    );

    // The positive observable (2): a drain of a server holding a segmented fragment
    // answers Pending, not (wrongly) Satisfied — the leg that catches a resolver
    // that decodes the segmented shape but never reads the `seg:` range.
    set_lifecycle(&meta, 0, DServerLifecycle::Draining)
        .await
        .unwrap();
    assert_eq!(
        reconciliation_status(&meta, 0).await.unwrap(),
        ReconciliationStatus::Pending,
        "d0 genuinely holds a referenced segmented fragment; the drain must not \
         certify satisfied"
    );
}

// ---- criterion (2): one unresolvable committed inode blocks certification and
// reclaims nothing ----

#[tokio::test]
async fn one_unresolvable_committed_inode_blocks_certification_and_reclaims_nothing() {
    let meta = MemMeta::default();
    let d0 = MemDServer::default();

    let group = SegmentGroup::new(DAMAGED_NONCE, DAMAGED_EPOCH).unwrap();
    let chunk_a: ChunkId = 0xD1_00;
    let chunk_b: ChunkId = 0xD2_00;
    // Two segments named; only the first was ever written. The second `seg:` record
    // is a real gap — genuinely absent under a generation the root still names.
    seed_segmented(
        &meta,
        DAMAGED_INODE,
        &group,
        &[(chunk_a, 0), (chunk_b, 0)],
        1,
    )
    .await;
    d0.put(frag(chunk_a, 0)).await;

    // Fixture check: the seeded root really does fail to resolve.
    let root_key = metadata::inode_key(DAMAGED_INODE);
    let root: InodeRecord = metadata::decode(&meta.get(&root_key).await.unwrap().unwrap()).unwrap();
    assert!(
        metadata::resolve_chunk_map(&meta, &root_key, &root)
            .await
            .is_err(),
        "fixture: the seeded root's map must genuinely fail to resolve"
    );

    // An unrelated, genuinely collectable fragment: the only thing that can keep it
    // is the fail-safe blanket containment rule — an incomplete reference set
    // authorizes NO reclamation anywhere in the fleet, not merely around the object
    // it cannot read (`ReferenceSet::protects`); no fragment can be shown not to be
    // one of the unresolvable object's own, unknown chunks.
    let lease_chunk: ChunkId = 0xD3_00;
    d0.put(frag(lease_chunk, 0)).await;
    metadata::put_pending(
        &meta,
        lease_chunk,
        &metadata::PendingEntry {
            lease_expiry_millis: 10,
        },
    )
    .await
    .unwrap();

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

    // GC alone: `Ok(_)`, and NOT `Satisfied` — `Reconciled::Blocked` is added by this
    // same patch, so it is deliberately not named here (it would compile-fail on the
    // reverted tree and score a RED leg as a pass).
    let gc_outcome = reconcile_step(
        &zone,
        &custodian,
        Some(&gc_ctx),
        None,
        None,
        None,
        GRACE + 1_000,
    )
    .await
    .unwrap();
    assert_ne!(
        gc_outcome,
        Reconciled::Satisfied,
        "GC must not certify convergence over an incomplete reference set"
    );

    // Scrub alone: the identical condition, the identical answer.
    let scrub_outcome = reconcile_step(&zone, &custodian, None, Some(&scrub_ctx), None, None, 0)
        .await
        .unwrap();
    assert_ne!(
        scrub_outcome,
        Reconciled::Satisfied,
        "scrub must not certify the store over an incomplete reference set"
    );

    // Nothing reclaimed: neither the damaged object's own readable fragment nor the
    // unrelated, genuinely-collectable garbage.
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
         reference set is incomplete — the containment is fleet-wide, not scoped to \
         the object that has it"
    );
}

// ---- criterion (3): one damaged object does not end the walk ----

#[tokio::test]
async fn one_damaged_object_does_not_end_the_walk_the_rest_of_the_store_is_still_handled() {
    let meta = MemMeta::default();
    let d0 = MemDServer::default();
    let d1 = MemDServer::default();

    // The healthy object, in its own group.
    let healthy_group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let healthy_a: ChunkId = 0xC1_00;
    let healthy_b: ChunkId = 0xC2_00;
    seed_segmented(
        &meta,
        SEGMENTED_INODE,
        &healthy_group,
        &[(healthy_a, 0), (healthy_b, 1)],
        2,
    )
    .await;
    d0.put(frag(healthy_a, 0)).await;
    d1.put(frag(healthy_b, 0)).await;

    // The damaged object — same shape as criterion (2)'s fixture — coexisting in the
    // SAME store, scanned alongside the healthy one.
    let damaged_group = SegmentGroup::new(DAMAGED_NONCE, DAMAGED_EPOCH).unwrap();
    let damaged_a: ChunkId = 0xD1_01;
    let damaged_b: ChunkId = 0xD2_01;
    seed_segmented(
        &meta,
        DAMAGED_INODE,
        &damaged_group,
        &[(damaged_a, 0), (damaged_b, 0)],
        1,
    )
    .await;
    d0.put(frag(damaged_a, 0)).await;

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

    // The walk must COMPLETE — `Ok`, never propagated as an `Err` for the whole
    // store because ONE object's map could not be resolved.
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
    assert_ne!(
        outcome,
        Reconciled::Satisfied,
        "the incomplete set must not certify"
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

    // VERIFIED: scrub reached the healthy object and enqueued its (dummy-content,
    // checksum-failing) fragments for repair — proof the reference build iterated
    // PAST the damaged object to reach it, rather than aborting the whole scan the
    // moment it was met.
    let queued = repair::queued_repairs(&meta).await.unwrap();
    assert!(
        queued.contains(&healthy_a),
        "the healthy object's fragment must still be verified: {queued:?}"
    );
    assert!(
        queued.contains(&healthy_b),
        "the healthy object's fragment must still be verified: {queued:?}"
    );
}

// ---- the other half of criterion (3): a genuine store fault still propagates ----

#[tokio::test]
async fn a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed() {
    let inner = MemMeta::default();
    let group = SegmentGroup::new(NONCE, EPOCH).unwrap();
    let chunk: ChunkId = 0xF1_00;
    seed_segmented(&inner, SEGMENTED_INODE, &group, &[(chunk, 0)], 1).await;

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
    assert!(
        outcome.is_err(),
        "a store fault that is not the resolver's own typed verdict about this \
         object's map must propagate, not be folded into 'unresolvable': {outcome:?}"
    );
}
