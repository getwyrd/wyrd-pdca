//! Issue #651 (slice 4 of 6 of the #635 re-slicing, 0016 decision 7(e)/7(f)): **the passes
//! that repair, move or rewrite an object's chunks go through the resolver, under the same
//! per-object containment — and a repoint is safe to persist.**
//!
//! The fixture (in-memory trait stores, raw-record `seg:` + root seeding) is adapted from
//! #650's; this slice ships no producer of segmented maps, so every segmented object here is
//! written by hand exactly as a committer would leave it. The GC / scrub / read-path legs are
//! #649's and #650's and are not pulled forward.
//!
//! Every leg drives a **real** entry — [`reconcile_step`](wyrd_custodian::reconcile_step), the
//! fenced control point, or [`reconcile_after_restore`] / `backfill::reconcile`, the pass's own
//! public entry — never a parallel test-only path. The observables are deliberately
//! **positive**: a fragment rebuilt and verifying, a placement that moved, a record whose bytes
//! did not change, an obligation still queued, a *counted* number of resolutions — never "no
//! error was raised", which a pass that did nothing at all also produces.
//!
//! The four binding criteria:
//!
//! 1. `restore_over_segmented_objects_marks_nothing_and_keeps_every_fragment` —
//!    [`RestoreReport::stranded_marked`] is 0 over segmented objects and every fragment they
//!    own survives the GC pass that follows, while the same pass still marks a genuine stray
//!    (so the zero is a decision, not inaction).
//! 2. `reconstruction_repairs_a_chunk_whose_ref_lives_in_a_seg_record`,
//!    `rebalance_evacuates_a_seg_record_chunk_off_a_draining_server`, and
//!    `a_repoint_that_would_cross_the_value_ceiling_is_refused_and_nothing_is_written` — a
//!    repair and an evacuation both reach a `ChunkRef` that lives in a `seg:` record, and a
//!    repoint that would push a record past the value ceiling is refused rather than
//!    persisted (observed through the rebalance pass, never by calling the helper).
//! 3. `backfill_leaves_a_segmented_record_byte_identical_and_still_fills_a_flat_one`.
//! 4. `reconstruction_resolves_each_object_once_per_pass_not_once_per_queued_chunk` — with
//!    Q = 9 obligations over N = 3 objects, the resolutions are **counted** on an instrumented
//!    store and must be N.
//!
//! Plus the containment leg the issue's What names:
//! `a_damaged_object_does_not_starve_the_healthy_ones_and_its_obligation_stays_queued`.

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
    SegmentRecord, SegmentRef, SegmentedMap, MAX_VALUE_BYTES,
};
use wyrd_core::placement::Topology;
use wyrd_core::write::encode_ec_fragment;
use wyrd_core::{erasure, repair};
use wyrd_custodian::backfill::{self, BackfillContext};
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, Custodian, DServerLifecycle, ExpiredPendingPolicy,
    FencedZone, GcContext, RebalanceContext, Reconciled, ReconstructionContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait stores (the loops are proven over the seams, backend-agnostic) ----

/// A trivial in-memory metadata store. `BTreeMap`, not a `HashMap`: several legs below need
/// the store to answer `scan(b"inode:")` in key order, so the DAMAGED object is met before
/// the healthy one and "the healthy object was still handled" cannot pass on an
/// implementation that abandons the walk at the first blocker.
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
    // the dev-only testkit helper pages over this store's own `scan`.
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

/// A [`MetadataStore`] that **counts segment-range resolutions**: each `seg:` range read
/// starts with a `scan_page` carrying no cursor, so counting exactly those counts how many
/// times a pass resolved a segmented object's chunk map — the number criterion (4) is about.
///
/// Continuation pages (a cursor present) are counted separately, so the leg can state that
/// its fixture pages in one go and the two numbers are not being confused.
struct CountingMeta {
    inner: MemMeta,
    resolves: Mutex<usize>,
    continuations: Mutex<usize>,
}

impl CountingMeta {
    fn new(inner: MemMeta) -> Self {
        Self {
            inner,
            resolves: Mutex::new(0),
            continuations: Mutex::new(0),
        }
    }

    fn resolves(&self) -> usize {
        *self.resolves.lock().unwrap()
    }

    fn continuations(&self) -> usize {
        *self.continuations.lock().unwrap()
    }
}

#[async_trait]
impl MetadataStore for CountingMeta {
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
        if prefix.starts_with(b"seg:") {
            let counter = if after.is_none() {
                &self.resolves
            } else {
                &self.continuations
            };
            *counter.lock().unwrap() += 1;
        }
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.inner.commit(batch).await
    }
}

/// One D server's fragment bytes, holding the **real** stored fragment bytes so their
/// checksums verify and a rebuilt shard round-trips.
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

// ---- audit capture (the operator-facing half of the contract) ----

/// A `MakeWriter` collecting what the subscriber emits, so a leg asserts on the record the
/// pass actually produced rather than assuming one exists — the proven in-tree pattern
/// (`crates/custodian/tests/segmented_map_consumers.rs`).
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
/// `Interest::never` under the parallel test harness (issue #214): a sibling test in this
/// binary that hits one with no subscriber installed would otherwise disable the callsite for
/// the whole process and leave the capture empty. Called at the top of EVERY leg, since all of
/// them drive passes that fire those callsites.
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

// ---- fixture ----

const GRACE: u64 = 1_000;
/// RS(2,1): three fragments, so one can be lost and rebuilt from the survivors.
const K: u8 = 2;
const M: u8 = 1;
const SCHEME: EcScheme = EcScheme::ReedSolomon { k: K, m: M };
/// The payload every fixture chunk erasure-codes, so its fragments are genuine: a rebuilt
/// shard has to decode and match the committed identity, not merely exist.
const PAYLOAD: &[u8] = b"a segmented object's chunk, erasure coded for real";
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const DAMAGED_NONCE: &str = "fedcba9876543210fedcba9876543210";
const EPOCH: u64 = 7;

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

fn group(nonce: &str) -> SegmentGroup {
    SegmentGroup::new(nonce, EPOCH).expect("a well-formed segment group")
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

/// A four-domain topology over servers 0..3 (A..D), the shape the placement selector places
/// an RS(2,1) chunk across.
fn four_domains() -> Topology {
    let mut t = Topology::default();
    t.register(0, "A").register(1, "B").register(2, "C");
    t.register(3, "D");
    t
}

/// One RS(2,1) chunk placed on `placement`, over the shared [`PAYLOAD`].
fn rs_chunk(id: ChunkId, placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id,
        scheme: SCHEME,
        len: PAYLOAD.len() as u64,
        placement,
    }
}

/// The real, checksum-carrying fragment bytes of `chunk` — the same encoding the write path
/// stores and the same one `repair::intact_shard` verifies.
fn ec_fragments(chunk: &ChunkRef) -> Vec<Bytes> {
    let (k, m) = match chunk.scheme {
        EcScheme::ReedSolomon { k, m } => (k, m),
        EcScheme::None => panic!("the fixture's chunks are erasure coded"),
    };
    let mut payload = PAYLOAD.to_vec();
    payload.resize(chunk.len as usize, b'.');
    erasure::encode(k as usize, m as usize, &payload)
        .expect("the fixture payload encodes")
        .into_iter()
        .enumerate()
        .map(|(index, shard)| encode_ec_fragment(chunk.id, index as u16, k, m, &shard))
        .collect()
}

/// Place every one of `chunk`'s fragments on the D server its placement names, skipping the
/// indices in `lost` — a genuine, scrub-detectable loss rather than a fabricated one.
async fn place_fragments(fleet: &[(DServerId, &MemDServer)], chunk: &ChunkRef, lost: &[u16]) {
    for (index, bytes) in ec_fragments(chunk).into_iter().enumerate() {
        let index = index as u16;
        if lost.contains(&index) {
            continue;
        }
        let dserver = chunk.placement[index as usize];
        let store = fleet
            .iter()
            .find(|(id, _)| *id == dserver)
            .map(|(_, s)| *s)
            .expect("the fixture places only onto the fleet it built");
        store
            .put_fragment(frag(chunk.id, index), bytes)
            .await
            .unwrap();
    }
}

/// Seed a committed **segmented** object at `inode`: one `seg:` record per entry of
/// `segments`, plus the segmented root that names them — raw `WriteBatch` puts, built with the
/// real validating constructors, never a publish path (this slice ships no producer of
/// segmented maps).
///
/// `written` is how many of the named segments actually get a record: `segments.len()` is the
/// healthy shape, and anything less is the damaged one the containment legs need — a segment
/// the root's own table names, on a generation it still names, that genuinely never landed
/// (`metadata::ChunkMapError::SegmentAbsent`).
async fn seed_segmented(
    meta: &MemMeta,
    inode: InodeId,
    group: &SegmentGroup,
    segments: &[Vec<ChunkRef>],
    written: usize,
) {
    let mut table = Vec::new();
    let mut offset = 0u64;
    for (index, chunks) in segments.iter().enumerate() {
        let record = SegmentRecord::new(chunks.clone(), offset).expect("a well-formed segment");
        table.push(SegmentRef {
            index: index as u32,
            byte_offset: offset,
            byte_len: record.byte_len(),
        });
        if index < written {
            let key = metadata::seg_key(group, index as u32).unwrap();
            commit(meta, WriteBatch::new().put(key, metadata::encode(&record))).await;
        }
        offset += record.byte_len();
    }
    let map = SegmentedMap::new(group.clone(), table).expect("a well-formed segment table");
    let root = InodeRecord {
        size: map.span(),
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

/// The chunks the store resolves for `inode` **right now**, through the production resolver —
/// the store's own answer, never a restatement of the fixture.
async fn live_chunks(meta: &impl MetadataStore, inode: InodeId) -> Vec<ChunkRef> {
    metadata::resolve_current_chunk_map(meta, &metadata::inode_key(inode))
        .await
        .expect("the object resolves")
        .expect("the object is live")
        .chunks
        .into_owned()
}

async fn raw(meta: &impl MetadataStore, key: &[u8]) -> Bytes {
    meta.get(key).await.unwrap().expect("the record is present")
}

/// **A pass that could not account for part of the store may not certify it.** Neither of
/// the two certifying answers — "reality already matched" (`Satisfied`) and "reality
/// diverged and this step converged it" (`Changed`) — is an honest verdict over a walk with
/// a hole in it, because both are read as *the backlog is being worked*.
///
/// Spelled as "neither certifying answer" rather than by naming a verdict: the property is
/// the refusal to certify, and stating it this way keeps the assertion one that a tree
/// *without* the fix compiles and **fails on the assertion** — a red that measures the
/// behaviour, never the presence of a new enum variant.
#[track_caller]
fn assert_not_certified(outcome: Reconciled, why: &str) {
    assert!(
        !matches!(outcome, Reconciled::Satisfied | Reconciled::Changed),
        "{why} — got {outcome:?}"
    );
}

// ---- criterion (1): post-restore reconciliation strands nothing a segmented object owns ----

#[tokio::test]
async fn restore_over_segmented_objects_marks_nothing_and_keeps_every_fragment() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    // Two segmented objects, every fragment present: after a restore their maps are intact and
    // nothing they own is stranded.
    let a = rs_chunk(0xA1, vec![0, 1, 2]);
    let b = rs_chunk(0xB1, vec![1, 2, 3]);
    seed_segmented(
        &meta,
        1,
        &group(NONCE),
        &[vec![a.clone()], vec![b.clone()]],
        2,
    )
    .await;
    place_fragments(&owned, &a, &[]).await;
    place_fragments(&owned, &b, &[]).await;

    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let gc_ctx = GcContext {
        meta: &meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Reclaim,
    };

    let report = reconcile_after_restore(&gc_ctx, 0)
        .await
        .expect("post-restore reconciliation reads a segmented object's map");
    assert_eq!(
        report.stranded_marked, 0,
        "every fragment on disk belongs to a committed segmented object: marking one hands \
         live bytes to GC. got {report:?}"
    );
    assert!(
        report.dangling.is_empty() && report.misplaced.is_empty(),
        "a segmented object whose fragments are all present is neither dangling nor \
         misplaced: {report:?}"
    );
    assert!(
        report.is_clean(),
        "nothing here needs an operator: a store of readable segmented objects with every \
         fragment present is a clean post-restore verdict. got {report:?}"
    );

    // The teeth: a mark is a durable `orphan:` record, and GC reclaims on exactly that
    // evidence — so "nothing was marked" is checked where the evidence would be, in the
    // ledger itself, not inferred from a report field.
    assert!(
        orphan_ledger(&meta).await.is_empty(),
        "the pass wrote no `orphan:` evidence at all: a fragment of a committed segmented \
         object must never become collectable"
    );

    // ...and the whole fenced control point runs over this store: the repair and evacuation
    // loops walk the same segmented objects, certify (nothing outstanding), and every
    // fragment the objects own is still where the committed placement says.
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-restore").await;
    let topology = four_domains();
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let rebalance = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };
    assert_eq!(
        reconcile_step(
            &zone,
            &custodian,
            None,
            None,
            Some(&reconstruction),
            Some(&rebalance),
            GRACE + 1_000,
        )
        .await
        .expect("the reconcile step runs over segmented objects"),
        Reconciled::Satisfied,
        "a store of readable segmented objects with nothing outstanding is certified — the \
         passes resolved every object rather than failing closed or reporting a hole"
    );
    for chunk in [&a, &b] {
        for (index, &dserver) in chunk.placement.iter().enumerate() {
            let store = owned.iter().find(|(id, _)| *id == dserver).unwrap().1;
            assert!(
                store
                    .get_fragment(frag(chunk.id, index as u16))
                    .await
                    .unwrap()
                    .is_some(),
                "fragment {index} of chunk {:#x} must survive: it is referenced by a committed \
                 segmented map",
                chunk.id
            );
        }
    }

    // ...and the zero above is a DECISION, not inaction: the same pass over the same store
    // marks a genuine stray, and the evidence lands in the ledger GC reclaims from — so a
    // wrongly-marked segmented fragment would have arrived there in exactly this way.
    let stray = frag(0xDEAD, 0);
    d0.put_fragment(stray, Bytes::from_static(b"unreferenced"))
        .await
        .unwrap();
    let second = reconcile_after_restore(&gc_ctx, GRACE + 1_000)
        .await
        .expect("the pass runs again");
    assert_eq!(
        second.stranded_marked, 1,
        "the pass must still mark a fragment no committed map references — otherwise the zero \
         above says nothing. got {second:?}"
    );
    assert_eq!(
        orphan_ledger(&meta).await,
        vec![(0, stray)],
        "exactly the stray is now collectable — the ledger names it, and names nothing the \
         segmented objects own"
    );

    // ...and an object the pass CANNOT read changes the verdict without ending the pass. Its
    // root names two segments and only the first was written, so it is contained: named in
    // the report, `is_clean` false, nothing marked on its account — while every object the
    // pass could read is still reported, which is the whole reason it does not stop.
    let hidden = rs_chunk(0xF0, vec![0, 1, 2]);
    let unwritten = rs_chunk(0xF1, vec![0, 1, 2]);
    seed_segmented(
        &meta,
        3,
        &group(DAMAGED_NONCE),
        &[vec![hidden.clone()], vec![unwritten]],
        1,
    )
    .await;
    place_fragments(&owned, &hidden, &[]).await;
    // A genuine loss on a READABLE object, so "reporting continues" has an observable of its
    // own rather than being the absence of one.
    d3.delete_fragment(frag(b.id, 2)).await.unwrap();
    let audit = Capture::default();
    let contained = reconcile_after_restore(&gc_ctx, 2 * GRACE + 2_000)
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("one unreadable object is contained, not an error that ends the pass");
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"unresolvable-chunk-map""#)
            && logged.contains(r#""inode":"inode:3""#),
        "the object the pass could not read must reach the operator BY NAME — a report drawn \
         over part of the store, with nothing said about the part it could not read, is \
         indistinguishable from a clean one: {logged}"
    );
    assert!(
        !contained.is_clean(),
        "a report drawn over part of the store is not a clean bill for it: {contained:?}"
    );
    assert_eq!(
        contained.under_replicated,
        vec![b.id],
        "every object the pass COULD read is still reported — containment is per object, and \
         the walk goes on. got {contained:?}"
    );
    assert_eq!(
        contained.stranded_marked, 0,
        "nothing is marked while the picture has a hole in it: an unreadable map hides which \
         chunks its object owns. got {contained:?}"
    );
    assert_eq!(
        orphan_ledger(&meta).await,
        vec![(0, stray)],
        "the ledger is unchanged — only the stray marked while the walk was complete"
    );
}

/// Every `(dserver, fragment)` the store currently holds `orphan:` evidence for — read
/// through the production key grammar (`metadata::parse_orphan_key`), which is the same
/// evidence GC reclaims on.
async fn orphan_ledger(meta: &impl MetadataStore) -> Vec<(DServerId, FragmentId)> {
    let mut out: Vec<(DServerId, FragmentId)> = meta
        .scan(b"orphan:")
        .await
        .unwrap()
        .into_iter()
        .map(|(key, _)| metadata::parse_orphan_key(&key).expect("a well-formed orphan key"))
        .collect();
    out.sort_by_key(|(dserver, frag)| (*dserver, frag.chunk, frag.index));
    out
}

// ---- criterion (2a): a repair reaches a chunk whose `ChunkRef` lives in a `seg:` record ----

#[tokio::test]
async fn reconstruction_repairs_a_chunk_whose_ref_lives_in_a_seg_record() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    // One segmented object whose fragment 2 — placed on server 3, domain D — is genuinely
    // gone, and a real obligation on the shared repair queue. The survivors hold domains A and
    // B, so the selector's only lower-labelled free distinct domain is C: the rebuild lands on
    // server 2, which makes "the placement moved" a deterministic observable.
    let chunk = rs_chunk(0xC1, vec![0, 1, 3]);
    seed_segmented(&meta, 1, &group(NONCE), &[vec![chunk.clone()]], 1).await;
    place_fragments(&owned, &chunk, &[2]).await;
    repair::enqueue_repair(&meta, chunk.id, "scrub")
        .await
        .unwrap();

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-reconstruction").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let outcome = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .await
    .expect("the repair loop reads a segmented object's map");
    assert_eq!(
        outcome,
        Reconciled::Changed,
        "the obligation is for a chunk that IS under-replicated and IS repairable"
    );

    // Positive observable: the rebuilt fragment is on a new server, it verifies against the
    // committed identity, and the `seg:` record — not the root — is what moved.
    let live = live_chunks(&meta, 1).await;
    let repaired = &live[0];
    assert_eq!(
        repaired.placement,
        vec![0, 1, 2],
        "the repair must repoint index 2 off the server that lost it, onto the one free \
         distinct domain: the `ChunkRef` it rewrote lives in a `seg:` record"
    );
    let bytes = d2
        .get_fragment(frag(chunk.id, 2))
        .await
        .unwrap()
        .expect("the rebuilt fragment is on the server the repointed placement names");
    assert!(
        repair::fragment_intact(&bytes, frag(chunk.id, 2), SCHEME),
        "the rebuilt fragment must carry the committed identity, not merely exist"
    );
    assert!(
        !repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&chunk.id),
        "a completed repair drains its obligation in the same version-conditional commit"
    );
    let root: InodeRecord = metadata::decode(&raw(&meta, &metadata::inode_key(1)).await).unwrap();
    assert_eq!(
        root.version, 1,
        "a segmented repoint compare-and-swaps the `seg:` record; the root is pinned, not \
         rewritten (0016 decision 7(f))"
    );
}

// ---- criterion (2b): an evacuation reaches one too ----

#[tokio::test]
async fn rebalance_evacuates_a_seg_record_chunk_off_a_draining_server() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    let chunk = rs_chunk(0xE1, vec![0, 1, 2]);
    seed_segmented(&meta, 1, &group(NONCE), &[vec![chunk.clone()]], 1).await;
    place_fragments(&owned, &chunk, &[]).await;
    wyrd_custodian::set_lifecycle(&meta, 0, DServerLifecycle::Decommissioning)
        .await
        .unwrap();

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-rebalance").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let rebalance = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };
    let outcome = reconcile_step(&zone, &custodian, None, None, None, Some(&rebalance), 5_000)
        .await
        .expect("the drain loop reads a segmented object's map");
    assert_eq!(
        outcome,
        Reconciled::Changed,
        "a fragment of a segmented object sits on a decommissioning server; the drain must \
         move it"
    );

    let live = live_chunks(&meta, 1).await;
    let moved = &live[0];
    assert_eq!(
        moved.placement[0], 3,
        "index 0 evacuates onto the one free distinct domain (server 3): {:?}",
        moved.placement
    );
    let bytes = d3
        .get_fragment(frag(chunk.id, 0))
        .await
        .unwrap()
        .expect("the evacuated copy is on the server the repointed placement names");
    assert!(
        repair::fragment_intact(&bytes, frag(chunk.id, 0), SCHEME),
        "an evacuation copies the intact fragment, it does not fabricate one"
    );
    assert!(
        meta.get(&metadata::orphan_key(0, frag(chunk.id, 0)))
            .await
            .unwrap()
            .is_some(),
        "the displaced copy on the draining server is orphaned in the same commit, so GC can \
         reclaim it on its own grace window"
    );
}

// ---- criterion (2c): a repoint that would cross the value ceiling is refused ----

/// A `seg:` record that is **legal today** (inside [`MAX_VALUE_BYTES`]) but has less headroom
/// left than `growth`, the bytes the move under test will add — so every refusal below is
/// observed on a legal→over-ceiling transition, never on a record that was already oversize.
///
/// Padded in two stages. Whole filler chunks first, which lands anywhere within one filler's
/// width of the target; then **one byte at a time** — a single filler's `len` widened from
/// one decimal digit to two — until the record is inside `growth` of the ceiling. The second
/// stage is what makes the fixture exact: a chunk is far wider than the growth some of these
/// moves cost, so padding in whole chunks alone would leave headroom the move never needed
/// and prove the refusal on a record that had room to spare.
fn near_ceiling_segment(moved: ChunkRef, growth: usize) -> (SegmentRecord, Vec<ChunkRef>) {
    // Padding ids live above every chunk id these legs name: a filler that collided with the
    // chunk under test would make the object reference one id twice, which is the ambiguity
    // the repair pass now refuses to arbitrate — the fixture would be testing that instead.
    const FILLER_BASE: ChunkId = 0x1000;
    assert!(
        moved.id < FILLER_BASE,
        "fixture: the moved chunk's id must not collide with the padding"
    );
    let filler = |id: ChunkId| ChunkRef {
        id: FILLER_BASE + id,
        scheme: EcScheme::None,
        len: 1,
        placement: vec![0],
    };
    let encoded = |chunks: &[ChunkRef]| {
        metadata::encode(&SegmentRecord::new(chunks.to_vec(), 0).expect("a well-formed segment"))
            .len()
    };
    let mut chunks = vec![moved, filler(1)];
    let width = metadata::encode(&filler(1)).len() + 1;
    while encoded(&chunks) + width + growth <= MAX_VALUE_BYTES {
        chunks.push(filler(chunks.len() as ChunkId));
    }
    let mut widened = 1;
    while encoded(&chunks) + growth <= MAX_VALUE_BYTES {
        // `1` -> `10`: one more digit in one filler's `len`, so the record grows by exactly
        // one byte and the loop cannot overshoot. There are always more fillers left than
        // bytes to add — the coarse stage overshot by less than one filler's width.
        chunks[widened].len = 10;
        widened += 1;
        assert!(
            widened < chunks.len(),
            "fixture: ran out of fillers to widen"
        );
    }
    let record = SegmentRecord::new(chunks.clone(), 0).expect("a well-formed segment");
    (record, chunks)
}

/// Assert the fixture is the transition the refusal is about: legal now, and with less room
/// left than `growth` — so a pass that persisted the move would take it over the ceiling.
fn assert_legal_but_within(stored: &Bytes, growth: usize) {
    assert!(
        stored.len() <= MAX_VALUE_BYTES,
        "fixture: the record must start LEGAL — the refusal under test is the transition to \
         oversize, not a record that was already over ({} bytes)",
        stored.len()
    );
    assert!(
        MAX_VALUE_BYTES - stored.len() < growth,
        "fixture: the record must have LESS headroom ({} bytes) than the move needs ({growth} \
         bytes), or the move would legitimately succeed",
        MAX_VALUE_BYTES - stored.len()
    );
}

/// The bytes `entries` placement entries cost to widen from a one-digit D-server id to a
/// twenty-digit one — the growth every ceiling fixture here is built around.
fn widening_growth(entries: usize) -> usize {
    entries * ("18446744073709551615".len() - "0".len())
}

#[tokio::test]
async fn a_repoint_that_would_cross_the_value_ceiling_is_refused_and_nothing_is_written() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    // The draining server holds EVERY fragment of the chunk, and every destination the
    // selector can reach is a 20-digit D-server id — so the move rewrites six placement
    // entries from one digit to twenty, and the record grows by 114 bytes.
    let wide = EcScheme::ReedSolomon { k: 4, m: 2 };
    let targets: Vec<DServerId> = (0..6).map(|i| u64::MAX - i).collect();
    let moved = ChunkRef {
        id: 0xF1,
        scheme: wide,
        len: PAYLOAD.len() as u64,
        placement: vec![0; 6],
    };
    let growth = widening_growth(6);
    let (record, chunks) = near_ceiling_segment(moved.clone(), growth);
    assert_legal_but_within(&metadata::encode(&record), growth);

    let group = group(NONCE);
    seed_segmented(&meta, 1, &group, &[chunks], 1).await;
    let seg_key = metadata::seg_key(&group, 0).unwrap();
    let before = raw(&meta, &seg_key).await;

    // A real fleet: the draining server holds the six genuine fragments, and each destination
    // is a live D server (so nothing aborts before the repoint is even attempted).
    let source = MemDServer::default();
    let destinations: Vec<MemDServer> = (0..6).map(|_| MemDServer::default()).collect();
    for (index, bytes) in ec_fragments(&moved).into_iter().enumerate() {
        source
            .put_fragment(frag(moved.id, index as u16), bytes)
            .await
            .unwrap();
    }
    let mut fleet: Vec<(DServerId, &dyn ChunkStore)> = vec![(0, &source)];
    let mut topology = Topology::default();
    topology.register(0, "A");
    for (slot, (id, store)) in targets.iter().zip(&destinations).enumerate() {
        topology.register(*id, ["B", "C", "D", "E", "F", "G"][slot]);
        fleet.push((*id, store));
    }
    wyrd_custodian::set_lifecycle(&meta, 0, DServerLifecycle::Decommissioning)
        .await
        .unwrap();

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-ceiling").await;
    let rebalance = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };
    let audit = Capture::default();
    let outcome = reconcile_step(&zone, &custodian, None, None, None, Some(&rebalance), 9_000)
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("a refused repoint is one chunk's refusal, not a failed pass");

    // (a) The record is untouched — byte-identical, so the object still resolves. A record
    //     grown past the ceiling is one no compare-and-swap could ever rewrite again.
    assert_eq!(
        raw(&meta, &seg_key).await,
        before,
        "a repoint the record cannot carry must leave it byte-identical"
    );
    assert_eq!(
        live_chunks(&meta, 1).await[0].placement,
        vec![0; 6],
        "the placement still names the draining server: nothing was persisted"
    );
    // (b) Nothing was written anywhere — not one destination copy. A pass that copied first
    //     and refused afterwards would leave six abandoned fragments behind, every pass, for
    //     as long as the record stays unrepairable.
    for (slot, store) in destinations.iter().enumerate() {
        assert!(
            store.list_fragments().await.unwrap().is_empty(),
            "destination {slot} must hold nothing: the repoint was refused before any byte moved"
        );
    }
    // (c) ...and the pass says so, on both surfaces an operator reads: a `repoint-refused`
    //     audit line naming the CEILING as the cause (so the refusal is attributable to the
    //     record, not to a shortage of capacity that will clear on its own), and a
    //     non-certifying outcome. `Satisfied` here would tell an operator the decommission
    //     converged while the fragments are still on the server they asked to empty.
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"repoint-refused""#) && logged.contains("value ceiling"),
        "the refusal must be stated, and attributed to the ceiling. got: {logged}"
    );
    assert_not_certified(
        outcome,
        "a drain that could not move a fragment off the draining server must not certify",
    );
}

/// The same refusal on the **repair** pass, which is the other half of criterion (2): a
/// rebuild whose repoint would carry the `seg:` record past the ceiling is refused before a
/// shard is written, and the pass says so on its own answer.
///
/// Its own leg rather than the drain's, because the two reach the refusal by different
/// routes and report it on different surfaces — and because a repair pass that certified
/// `Satisfied` over a refusal would tell an operator the repair backlog is being worked
/// while a chunk sits below its redundancy floor with nothing that will ever restore it.
#[tokio::test]
async fn a_repair_whose_repoint_would_cross_the_value_ceiling_is_refused_and_stays_queued() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    // RS(4,2) on six 1-digit servers, with fragments 4 and 5 genuinely gone: four survivors,
    // exactly `k`, so the chunk IS repairable. The only free distinct domains left carry
    // 20-digit D-server ids, so the rebuild's repoint widens two placement entries.
    let wide: Vec<DServerId> = vec![u64::MAX, u64::MAX - 1];
    let chunk = ChunkRef {
        id: 0xC7,
        scheme: EcScheme::ReedSolomon { k: 4, m: 2 },
        len: PAYLOAD.len() as u64,
        placement: (0..6).collect(),
    };
    let growth = widening_growth(2);
    let (record, chunks) = near_ceiling_segment(chunk.clone(), growth);
    assert_legal_but_within(&metadata::encode(&record), growth);

    let group = group(NONCE);
    seed_segmented(&meta, 1, &group, &[chunks], 1).await;
    let seg_key = metadata::seg_key(&group, 0).unwrap();
    let before = raw(&meta, &seg_key).await;

    let survivors: Vec<MemDServer> = (0..4).map(|_| MemDServer::default()).collect();
    let owned: Vec<(DServerId, &MemDServer)> = survivors
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u64, s))
        .collect();
    place_fragments(&owned, &chunk, &[4, 5]).await;
    repair::enqueue_repair(&meta, chunk.id, "scrub")
        .await
        .unwrap();

    // The survivors' four domains are excluded from the rebuild, and the servers that held
    // the lost fragments are not in the topology at all — so the only placements the selector
    // can reach are the two wide ones.
    let destinations: Vec<MemDServer> = (0..2).map(|_| MemDServer::default()).collect();
    let mut topology = Topology::default();
    let mut fleet: Vec<(DServerId, &dyn ChunkStore)> = Vec::new();
    for (id, store) in &owned {
        topology.register(*id, ["A", "B", "C", "D"][*id as usize]);
        fleet.push((*id, *store));
    }
    for (slot, (id, store)) in wide.iter().zip(&destinations).enumerate() {
        topology.register(*id, ["E", "F"][slot]);
        fleet.push((*id, store));
    }

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-repair-ceiling").await;
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let audit = Capture::default();
    let outcome = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        9_000,
    )
    .with_subscriber(capturing_dispatch(audit.clone()))
    .await
    .expect("a refused repoint is one chunk's refusal, not a failed pass");

    assert_eq!(
        raw(&meta, &seg_key).await,
        before,
        "a repair the record cannot carry must leave it byte-identical"
    );
    for (slot, store) in destinations.iter().enumerate() {
        assert!(
            store.list_fragments().await.unwrap().is_empty(),
            "destination {slot} must hold nothing: a shard rebuilt for a repoint that could \
             never commit is garbage that comes back every pass"
        );
    }
    assert!(
        repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&chunk.id),
        "the obligation stays queued: the chunk is still below its redundancy floor"
    );
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"repoint-refused""#) && logged.contains("value ceiling"),
        "the refusal must be stated, and attributed to the ceiling. got: {logged}"
    );
    assert_not_certified(
        outcome,
        "a repair pass that could not persist a rebuild may not report the backlog worked",
    );
}

// ---- criterion (3): backfill declines a segmented record, and still fills a flat one ----

#[tokio::test]
async fn backfill_leaves_a_segmented_record_byte_identical_and_still_fills_a_flat_one() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    // `inode:1` sorts first, so the flat record is reached only if the pass walks PAST the
    // segmented one it declines. Its chunk carries an EMPTY placement — the one thing this
    // pass exists to fill, seeded here precisely because a `seg:` record's chunk is the one
    // it must decline: the gauges below then report a population that is genuinely
    // outstanding, not a store that happens to be drained.
    let group = group(NONCE);
    let segmented = rs_chunk(0xA1, vec![]);
    seed_segmented(&meta, 1, &group, &[vec![segmented]], 1).await;
    let seg_key = metadata::seg_key(&group, 0).unwrap();
    let seg_before = raw(&meta, &seg_key).await;
    let root_before = raw(&meta, &metadata::inode_key(1)).await;

    // A pre-M3 / mixed-era flat record: an EMPTY placement, which is exactly what this pass
    // exists to materialize.
    let flat = InodeRecord {
        size: PAYLOAD.len() as u64,
        chunk_map: vec![rs_chunk(0xB2, vec![])].into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    commit(
        &meta,
        WriteBatch::new().put(metadata::inode_key(2), metadata::encode(&flat)),
    )
    .await;

    let audit = Capture::default();
    let outcome = backfill::reconcile(&BackfillContext { meta: &meta })
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("a segmented record is declined, not a failed pass");

    assert_eq!(
        raw(&meta, &seg_key).await,
        seg_before,
        "backfill must leave a segmented record BYTE-IDENTICAL: its chunks live in `seg:` \
         records, which no inode compare-and-swap may rewrite"
    );
    assert_eq!(
        raw(&meta, &metadata::inode_key(1)).await,
        root_before,
        "nor may it rewrite the segmented root it did not fill"
    );
    let filled: InodeRecord = metadata::decode(&raw(&meta, &metadata::inode_key(2)).await).unwrap();
    assert_eq!(
        filled.chunk_map.as_flat().unwrap()[0].placement,
        vec![0, 1, 2],
        "the fillable FLAT record in the same store is still filled in the same pass — one \
         declined record does not stop the drain"
    );
    // The skip is STATED — the reason, the record, and how many chunks it declined to fill —
    // and the pass does not certify. A silent skip under `Changed`/`Satisfied` is what lets an
    // unfilled population sit unnoticed.
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"declined-segmented""#)
            && logged.contains(r#""inode":1"#)
            && logged.contains(r#""unfilled":1"#),
        "the decline must be stated on the audit seam, NAME the record, and say how many \
         chunks it declined to fill: {logged}"
    );
    // ...and the population gauge counts that record's chunk HONESTLY (resolved, not walked
    // off the root) while the companion level says the record went unassessed — so neither
    // number can be read as a drained store.
    assert!(
        logged.contains(r#""gauge.backfill_placement_remaining":1"#)
            && logged.contains(r#""gauge.backfill_records_unassessed":1"#),
        "the outstanding chunk must be counted, beside the count of records excluded: {logged}"
    );
    assert_not_certified(
        outcome,
        "a pass that declined a record may not certify the population drained",
    );
}

/// The other half of criterion (3), and the one a decline-everything implementation passes
/// by accident: over a store whose segmented records are **fully placed** — which is every
/// record the segment writer produces, since a repoint's replacement vector must be exactly
/// `fragment_count()` long — backfill has nothing to decline, so it must **certify**. A pass
/// that charged every segmented record to "unassessed" on sight would leave the ordinary
/// segmented store permanently `Blocked`, and the drain-to-zero signal ADR-0040 decision 6
/// gates the fallback removal on could never be read at all.
///
/// The same leg counts the pass's resolutions: one per object, for the population gauge as
/// well as the fill. A second walk to publish a number the pass already holds doubles every
/// object's reads for nothing.
#[tokio::test]
async fn backfill_certifies_a_fully_placed_segmented_store_and_resolves_each_object_once() {
    enable_audit_callsites();
    let inner = MemMeta::default();
    const OBJECTS: u64 = 2;
    let nonces = [NONCE, DAMAGED_NONCE];
    for object in 0..OBJECTS {
        let chunk = rs_chunk((0x2000 + object) as ChunkId, vec![0, 1, 2]);
        seed_segmented(
            &inner,
            object + 1,
            &group(nonces[object as usize]),
            &[vec![chunk]],
            1,
        )
        .await;
    }
    let meta = CountingMeta::new(inner);

    let audit = Capture::default();
    let outcome = backfill::reconcile(&BackfillContext { meta: &meta })
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("a fully placed segmented store is an ordinary store");

    assert_eq!(
        outcome,
        Reconciled::Satisfied,
        "a segmented record with nothing to fill is not a record this pass failed to \
         assess — it resolved and classified every chunk in it"
    );
    let logged = audit.contents();
    assert!(
        logged.contains(r#""gauge.backfill_placement_remaining":0"#)
            && logged.contains(r#""gauge.backfill_records_unassessed":0"#),
        "both levels must return to zero over a store with nothing outstanding — a level \
         that cannot reach zero is a signal an operator can never read: {logged}"
    );
    assert!(
        !logged.contains(r#""action":"declined-segmented""#),
        "nothing was declined, so nothing may be reported as declined: {logged}"
    );
    assert_eq!(
        meta.resolves(),
        OBJECTS as usize,
        "one resolution per OBJECT per pass, gauge included: publishing the remaining \
         population from a SECOND walk would cost {} range reads for a number the pass \
         already held",
        2 * OBJECTS
    );
}

// ---- criterion (4): one resolution per object per pass, counted ----

#[tokio::test]
async fn reconstruction_resolves_each_object_once_per_pass_not_once_per_queued_chunk() {
    enable_audit_callsites();
    let inner = MemMeta::default();
    let (d0, d1, d2) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 3] = [(0, &d0), (1, &d1), (2, &d2)];

    // N = 3 segmented objects, Q = 9 obligations: three chunks each, every fragment present,
    // so each obligation is assessed (and drained) rather than short-circuited.
    const OBJECTS: u64 = 3;
    const CHUNKS_PER_OBJECT: u64 = 3;
    let nonces = [
        "0123456789abcdef0123456789abcdef",
        "fedcba9876543210fedcba9876543210",
        "00112233445566778899aabbccddeeff",
    ];
    let mut queued = Vec::new();
    for object in 0..OBJECTS {
        let chunks: Vec<ChunkRef> = (0..CHUNKS_PER_OBJECT)
            .map(|i| rs_chunk((0x1000 + object * 0x100 + i) as ChunkId, vec![0, 1, 2]))
            .collect();
        for chunk in &chunks {
            place_fragments(&owned, chunk, &[]).await;
            queued.push(chunk.id);
        }
        seed_segmented(
            &inner,
            object + 1,
            &group(nonces[object as usize]),
            &[chunks],
            1,
        )
        .await;
    }
    for chunk in &queued {
        repair::enqueue_repair(&inner, *chunk, "scrub")
            .await
            .unwrap();
    }
    assert_eq!(queued.len() as u64, OBJECTS * CHUNKS_PER_OBJECT);
    // One more obligation, for a chunk that lives in NO object — deleted out from under it.
    // Over a walk that read every committed object, "not found" really does mean absent, so
    // this one is drained rather than kept: the containment rule keeps an obligation queued
    // only while the picture has a hole in it.
    let deleted: ChunkId = 0xDEAD;
    repair::enqueue_repair(&inner, deleted, "read")
        .await
        .unwrap();

    let meta = CountingMeta::new(inner);
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-index").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 3] = [(0, &d0), (1, &d1), (2, &d2)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .await
    .expect("the repair loop reads segmented objects' maps");

    // The pass ran for real: every obligation was assessed and drained — the nine because
    // their chunks are at full redundancy, the tenth because its chunk is genuinely
    // referenced by nothing and the walk that says so was complete.
    assert!(
        repair::queued_repairs(&meta).await.unwrap().is_empty(),
        "each of the {} obligations must have been assessed against the object that owns it, \
         and the obligation for a deleted chunk drained",
        queued.len() + 1
    );
    assert_eq!(
        meta.continuations(),
        0,
        "fixture: each object's segment range fits in one page, so a range read is one call"
    );
    assert_eq!(
        meta.resolves(),
        OBJECTS as usize,
        "one resolution per OBJECT per pass. Resolving per queued chunk would cost {} — the \
         Q x N repair loop that is correct and unusable at fleet scale",
        queued.len() * OBJECTS as usize
    );
}

// ---- containment: a damaged object does not starve the healthy ones ----

#[tokio::test]
async fn a_damaged_object_does_not_starve_the_healthy_ones_and_its_obligation_stays_queued() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    // `inode:1` is DAMAGED — its root names two segments and only the first was ever written —
    // and sorts before the healthy object, so the healthy one is reached only past it.
    let hidden = rs_chunk(0xD1, vec![0, 1, 2]);
    let unwritten = rs_chunk(0xD2, vec![0, 1, 2]);
    seed_segmented(
        &meta,
        1,
        &group(DAMAGED_NONCE),
        &[vec![hidden.clone()], vec![unwritten.clone()]],
        1,
    )
    .await;
    place_fragments(&owned, &hidden, &[]).await;

    // ...and `inode:2` is healthy, with a genuinely under-replicated chunk (fragment 2, on
    // server 3, is gone) AND a fragment on the server the operator is draining (server 1).
    let healthy = rs_chunk(0x1401, vec![0, 1, 3]);
    seed_segmented(&meta, 2, &group(NONCE), &[vec![healthy.clone()]], 1).await;
    place_fragments(&owned, &healthy, &[2]).await;

    // One obligation for the healthy object's chunk, one for a chunk that may well live in the
    // map the pass cannot read.
    repair::enqueue_repair(&meta, healthy.id, "scrub")
        .await
        .unwrap();
    repair::enqueue_repair(&meta, unwritten.id, "read")
        .await
        .unwrap();

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-containment").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let rebalance = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };
    wyrd_custodian::set_lifecycle(&meta, 1, DServerLifecycle::Draining)
        .await
        .unwrap();
    // The two passes run in SEPARATE steps, so each one's own answer is observable: a step
    // reports the least certified of its loops, which would let one loop's refusal stand in
    // for the other's.
    let audit = Capture::default();
    let repaired = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .with_subscriber(capturing_dispatch(audit.clone()))
    .await
    .expect("one damaged object is contained, not an error that ends the step");
    let outcome = reconcile_step(&zone, &custodian, None, None, None, Some(&rebalance), 2_000)
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("the drain walks past the damaged object too");
    assert_not_certified(
        repaired,
        "the repair pass left an obligation unassessed, so it may not certify — on its own \
         answer, not on the drain's",
    );

    // (a) The healthy object's repair happened anyway — index 2 rebuilt onto the free distinct
    //     domain C (server 2) — and (c) its evacuation off the draining server ran too: index 1
    //     moved to domain D (server 3).
    let live = live_chunks(&meta, 2).await;
    assert_eq!(
        live[0].placement,
        vec![0, 3, 2],
        "the healthy object must be repaired AND drained even though another object is \
         unreadable: one damaged object may not starve the store"
    );
    assert!(
        !repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&healthy.id),
        "its obligation is drained by the repair commit"
    );
    // (b) The obligation whose chunk may live in the unreadable map is STILL QUEUED. Draining
    //     it would retire a repair on an incomplete reading — redundancy decaying with nothing
    //     left that will ever restore it.
    assert!(
        repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&unwritten.id),
        "an obligation the pass could not assess must never be drained as unreferenced"
    );
    // (c) ...and nothing of the damaged object was moved or reclaimed on the way past — its
    //     own fragment on the draining server included, since a map the pass cannot read is a
    //     map it may not act on.
    for (index, &dserver) in hidden.placement.iter().enumerate() {
        let store = owned.iter().find(|(id, _)| *id == dserver).unwrap().1;
        assert!(
            store
                .get_fragment(frag(hidden.id, index as u16))
                .await
                .unwrap()
                .is_some(),
            "nothing of the unreadable object is moved or reclaimed on the way past"
        );
    }
    // (d) ...and the step names the blocker and refuses to certify while any of it is
    //     unreadable — an operator needs the record to repair, not an unexplained stall.
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"unresolvable-chunk-map""#)
            && logged.contains(r#""inode":"inode:1""#),
        "the damaged object must be attributed by NAME on the repair pass's audit seam: {logged}"
    );
    assert!(
        logged.contains(r#""gauge.reconstruction_unassessable":1"#),
        "the obligation the pass could not judge must be counted on its own level — off the \
         repairable backlog, off the data-loss counter: {logged}"
    );
    assert_not_certified(
        outcome,
        "a pass that walked past an object it could not read may not report the store converged",
    );

    // (e) ...and when the operator cancels the drain, the level the blocked pass raised comes
    //     back DOWN. A gauge whose zero sample is skipped on the idle pass never returns: the
    //     last value ever published — the non-zero one — stands forever, so an operator
    //     reading it is told a store still holds damaged records long after it does not.
    wyrd_custodian::clear_lifecycle(&meta, 1).await.unwrap();
    let idle = Capture::default();
    reconcile_step(&zone, &custodian, None, None, None, Some(&rebalance), 3_000)
        .with_subscriber(capturing_dispatch(idle.clone()))
        .await
        .expect("a pass with nothing to drain is still a pass");
    let idle = idle.contents();
    assert!(
        idle.contains(r#""gauge.rebalance_unresolvable_records":0"#),
        "the level must be sampled at zero on the pass that has no drain to work: {idle}"
    );
}

// ---- containment: an EMPTY queue is not a certificate ----

/// **A repair pass with nothing queued still walks, and still refuses to certify a store it
/// could not read.** "No obligation is outstanding" and "this store is converged" are
/// different claims, and only the walk tells them apart: an unreadable object produces no
/// obligation of its own, so a pass that short-circuits on an empty queue reports the
/// healthiest possible answer about exactly the records it never looked at — and publishes
/// its four durability levels at zero on their behalf. Certification is what a decommission
/// and a reclamation are then taken on (`docs/principles.md` §5 C-1).
#[tokio::test]
async fn an_idle_repair_pass_over_an_unreadable_object_still_refuses_to_certify() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let d0 = MemDServer::default();

    // One damaged object: its root names two segments, only the first was ever written. No
    // repair obligation exists anywhere in the store — this pass is idle by construction.
    let hidden = rs_chunk(0x1D1, vec![0, 1, 2]);
    let unwritten = rs_chunk(0x1D2, vec![0, 1, 2]);
    seed_segmented(
        &meta,
        1,
        &group(DAMAGED_NONCE),
        &[vec![hidden], vec![unwritten]],
        1,
    )
    .await;
    assert!(
        repair::queued_repairs(&meta).await.unwrap().is_empty(),
        "the fixture's whole point is an EMPTY repair queue"
    );

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-idle").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 1] = [(0, &d0)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let audit = Capture::default();
    let outcome = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .with_subscriber(capturing_dispatch(audit.clone()))
    .await
    .expect("an unreadable object is contained, not an error that ends the step");
    assert_not_certified(
        outcome,
        "an empty queue over a store with an unreadable record is not a converged store",
    );
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"unresolvable-chunk-map""#)
            && logged.contains(r#""inode":"inode:1""#),
        "and the blocker is named on the audit seam, on the idle pass too: {logged}"
    );
}

// ---- containment: a record no mutation could name is contained, never skipped ----

/// **A committed record under a key this store's own grammar does not produce is contained,
/// not passed over.** `inode:007` parses to 7 through `u64::from_str` and re-renders as
/// `inode:7`, so every compare-and-swap a pass would build for it names a *different*,
/// absent key: the record can be neither repaired nor rewritten. Dropping it from the walk
/// is the silent-corruption direction — the object owns chunks nothing else references, so
/// reconstruction reads its queued repair as "referenced by no committed chunk map" and
/// **drains** it, retiring redundancy with nothing left that will ever restore it, while
/// every pass reports a store it never fully read as converged.
///
/// So it is contained exactly as an undecodable record is: named, counted, walked past —
/// and both the pass that repairs and the pass that rewrites say their picture has a hole
/// in it.
#[tokio::test]
async fn a_committed_record_under_a_key_no_mutation_can_name_is_contained_not_skipped() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    // The unaddressable record, sorting FIRST (`inode:007` < `inode:2`), so the healthy
    // object is reached only past it. Its chunk carries an empty placement — the one thing
    // backfill exists to fill — so the pass that rewrites records has a genuine reason to
    // touch it and must still decline.
    const STRAY_KEY: &[u8] = b"inode:007";
    let orphaned = rs_chunk(0xF1, vec![]);
    let stray = InodeRecord {
        size: PAYLOAD.len() as u64,
        chunk_map: vec![orphaned.clone()].into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    commit(
        &meta,
        WriteBatch::new().put(STRAY_KEY.to_vec(), metadata::encode(&stray)),
    )
    .await;
    let stray_before = raw(&meta, STRAY_KEY).await;
    repair::enqueue_repair(&meta, orphaned.id, "scrub")
        .await
        .unwrap();

    // ...and a healthy segmented object whose fragment 2 (server 3) is genuinely gone.
    let healthy = rs_chunk(0xF2, vec![0, 1, 3]);
    seed_segmented(&meta, 2, &group(NONCE), &[vec![healthy.clone()]], 1).await;
    place_fragments(&owned, &healthy, &[2]).await;
    repair::enqueue_repair(&meta, healthy.id, "scrub")
        .await
        .unwrap();

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-unaddressable").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let audit = Capture::default();
    let repaired = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .with_subscriber(capturing_dispatch(audit.clone()))
    .await
    .expect("an unaddressable record is contained, not an error that ends the step");
    let filled = backfill::reconcile(&BackfillContext { meta: &meta })
        .with_subscriber(capturing_dispatch(audit.clone()))
        .await
        .expect("...and the pass that rewrites records contains it too");

    // (a) The obligation for the chunk only that record references is STILL QUEUED. This is
    //     the assertion the finding turns on: a walk that skipped the record would find the
    //     chunk in no map it could read and drain the obligation as unreferenced.
    assert!(
        repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&orphaned.id),
        "an obligation whose chunk is referenced only by a record the pass could not \
         address must never be drained as unreferenced"
    );
    // (b) Nothing rewrote it — not the repair pass, and not the pass whose whole job is
    //     filling exactly the empty placement it carries.
    assert_eq!(
        raw(&meta, STRAY_KEY).await,
        stray_before,
        "a record no mutation can name is left BYTE-IDENTICAL: the two legal answers are \
         resolve it or decline it, never overwrite it"
    );
    // (c) The walk went on: the healthy object was repaired anyway.
    assert_eq!(
        live_chunks(&meta, 2).await[0].placement,
        vec![0, 1, 2],
        "one unaddressable record may not starve every healthy object's repair"
    );
    // (d) Both passes name it and neither certifies.
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"unresolvable-chunk-map""#)
            && logged.contains(r#""inode":"inode:007""#),
        "the blocking record must reach the operator BY NAME, on both passes' audit seams — \
         a skip nobody is told about is indistinguishable from a clean store: {logged}"
    );
    assert_not_certified(
        repaired,
        "the repair pass may not certify over a record it could not account for",
    );
    assert_not_certified(filled, "and neither may the fill pass");
}

// ---- containment: one chunk id, two committed references ----

/// **A chunk id two committed objects reference is arbitrated by nobody.** Last-writer-wins
/// on the pass's index — a bare `insert`, which is what the previous scan effectively did —
/// picks one reference and then acts on it: the repair repoints *that* object's placement,
/// its commit drains the obligation both references share, and the reference nobody chose
/// is left under-replicated with nothing queued that would ever restore it. So the pass
/// repairs neither, keeps the obligation queued, names both objects, and does not certify.
#[tokio::test]
async fn a_chunk_id_referenced_by_two_objects_is_repaired_by_neither_and_stays_queued() {
    enable_audit_callsites();
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let owned: [(DServerId, &MemDServer); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];

    // One chunk id, two committed objects, two DIFFERENT placement records over the same
    // fragments — and a genuine loss, so the chunk really is repairable and the pass really
    // does have to decide whose placement to rewrite.
    let shared = rs_chunk(0xDD, vec![0, 1, 3]);
    seed_segmented(&meta, 1, &group(NONCE), &[vec![shared.clone()]], 1).await;
    seed_segmented(
        &meta,
        2,
        &group(DAMAGED_NONCE),
        &[vec![rs_chunk(0xDD, vec![0, 1, 3])]],
        1,
    )
    .await;
    place_fragments(&owned, &shared, &[2]).await;
    repair::enqueue_repair(&meta, shared.id, "scrub")
        .await
        .unwrap();
    let (first, second) = (
        raw(&meta, &metadata::seg_key(&group(NONCE), 0).unwrap()).await,
        raw(&meta, &metadata::seg_key(&group(DAMAGED_NONCE), 0).unwrap()).await,
    );

    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord, "zone-ambiguous").await;
    let topology = four_domains();
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let reconstruction = ReconstructionContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
        unreachable: &[],
    };
    let audit = Capture::default();
    let outcome = reconcile_step(
        &zone,
        &custodian,
        None,
        None,
        Some(&reconstruction),
        None,
        1_000,
    )
    .with_subscriber(capturing_dispatch(audit.clone()))
    .await
    .expect("an ambiguous chunk id is contained, not an error that ends the step");

    assert!(
        repair::queued_repairs(&meta)
            .await
            .unwrap()
            .contains(&shared.id),
        "the obligation both references share must stay queued: draining it retires the \
         repair of whichever reference the pass did not touch"
    );
    assert_eq!(
        (
            raw(&meta, &metadata::seg_key(&group(NONCE), 0).unwrap()).await,
            raw(&meta, &metadata::seg_key(&group(DAMAGED_NONCE), 0).unwrap()).await,
        ),
        (first, second),
        "neither placement record is rewritten — repointing either moves the fragments the \
         other one names"
    );
    assert!(
        d2.get_fragment(frag(shared.id, 2)).await.unwrap().is_none(),
        "and no rebuilt fragment is placed for a repair that was never attempted"
    );
    let logged = audit.contents();
    assert!(
        logged.contains(r#""action":"ambiguous-chunk""#) && logged.contains(r#""objects":"1,2""#),
        "both objects must reach the operator by name — a corruption only a human can \
         arbitrate is useless as a bare count: {logged}"
    );
    assert!(
        logged.contains(r#""gauge.reconstruction_unassessable":1"#),
        "the unjudged obligation is counted on its own level: {logged}"
    );
    assert_not_certified(
        outcome,
        "the walk read every record, and the pass STILL may not certify: an obligation it \
         could not judge is a backlog it is not working through",
    );
}
