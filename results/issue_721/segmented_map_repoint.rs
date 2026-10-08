//! Issue #721 (0016 decision 7(f), `0016:669`): **a chunk whose `ChunkRef` lives in a `seg:`
//! record is REPAIRED — the maintenance write path for a segmented map exists and the repair pass
//! completes through it.** On the base #697's containment left the obligation refused every pass,
//! forever, never drainable (that would be data loss) and with no code path able to move the
//! placement: redundancy decaying with no actor exiting the state, which C-1 forbids
//! (`docs/principles.md` §5).
//!
//! Every leg drives the real fenced control point [`reconcile_step`] and observes the **store**
//! (and, for containment, the durability seam). The first two are RED on the base, as is the last;
//! the rest are not independently red (the base refuses, which also writes nothing) and carry a
//! named negation instead. **No assertion names a symbol this patch introduces**, so the file also
//! compiles with the production change reverted; `MAX_ROOT_VALUE_BYTES` is a base constant.
//!
//! **Reaching the read→prepare window:** the resolver reads the group's range with `scan_page` and
//! never `get`, while the move's own read is the only `get` anyone performs on a `seg:` key — so
//! [`MemMeta`] races by landing the competing batch **after answering the `seg:` page**.

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
    SegmentRecord, SegmentRef, SegmentedMap, MAX_ROOT_VALUE_BYTES,
};
use wyrd_core::placement::Topology;
use wyrd_core::write::encode_ec_fragment;
use wyrd_core::{erasure, repair};
use wyrd_custodian::{reconcile_step, Custodian, FencedZone, Reconciled, ReconstructionContext};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait doubles: the pass is proven over the seams, backend-agnostic ----
/// When the racing writer's batch lands, relative to the pass's own reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Race {
    /// **After the resolver's bounded `seg:` page is answered** — the one instant after the read
    /// the plan is built from and before the move's own `get`.
    AfterSegmentPage,
    /// **On the way into a commit**, so the resolve has completed and the pass does not restart
    /// onto the new generation the way a root moving *during* it would.
    IntoCommit,
}

/// What the racing writer lands: raw key/value pairs with no preconditions of its own — it got
/// there first, it is not competing for a compare-and-swap.
type Racing = Vec<(Vec<u8>, Bytes)>;

/// A `BTreeMap`-backed metadata store that also plays a **racing writer** at either instant.
#[derive(Default)]
struct MemMeta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    /// The racing writer's pending puts and when they land — taken when it fires: it races once.
    racing: Mutex<Option<(Race, Racing)>>,
}

impl MemMeta {
    /// Arm the racing writer: `puts` land the first time the pass reaches `when`.
    fn arm(&self, when: Race, puts: Racing) {
        *self.racing.lock().unwrap() = Some((when, puts));
    }

    /// Whether the armed batch landed — the self-check that the leg's window was reached, rather
    /// than the leg passing because the race never happened.
    fn raced(&self) -> bool {
        self.racing.lock().unwrap().is_none()
    }

    /// Land the armed batch if it is due at `when`. Takes the `kv` lock itself.
    fn fire(&self, when: Race) {
        let due = matches!(&*self.racing.lock().unwrap(), Some((at, _)) if *at == when);
        if !due {
            return;
        }
        let (_, puts) = self.racing.lock().unwrap().take().expect("armed");
        let mut kv = self.kv.lock().unwrap();
        for (key, value) in puts {
            kv.insert(key, value);
        }
    }
}

#[async_trait]
impl MetadataStore for MemMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        let kv = self.kv.lock().unwrap();
        let rows = kv.iter().filter(|(key, _)| key.starts_with(prefix));
        Ok(rows
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }

    // The required paginated read (#634): the dev-only testkit helper pages over `scan`.
    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<wyrd_traits::ScanPage> {
        let page = wyrd_testkit::test_double_scan_page(self, prefix, after, limit).await;
        // AFTER the page is materialised, so the resolver answers from the pre-race bytes the
        // plan is built from — and any later reader of this record sees the race.
        if prefix.starts_with(b"seg:") {
            self.fire(Race::AfterSegmentPage);
        }
        page
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.fire(Race::IntoCommit);
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

#[derive(Default)]
struct MemDServer {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
}

#[async_trait]
impl ChunkStore for MemDServer {
    async fn put_fragment(
        &self,
        id: FragmentId,
        fragment: Bytes,
        _deadline_millis: Option<u64>,
    ) -> Result<()> {
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

/// A `MakeWriter` collecting what the pass emits, so the containment leg asserts on the record
/// the durability seam actually carried.
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

/// How many audit rows carried `action`, and how many times `counter` ticked beside them — the
/// containment leg is stated over both, so a counter that ticks per obligation where the rule is
/// per object fails rather than passes quietly.
fn rows(logged: &str, action: &str) -> usize {
    logged.matches(&format!(r#""action":"{action}""#)).count()
}

fn ticks(logged: &str, counter: &str) -> usize {
    logged
        .matches(&format!(r#""monotonic_counter.{counter}":1"#))
        .count()
}

/// Install a permissive global `tracing` default **once**, so the audit callsites never latch
/// `Interest::never` under the parallel test harness (issue #214).
fn enable_audit_callsites() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}

const NOW: u64 = 10_000;
const CHUNK_LEN: u64 = 8;
const K: u8 = 1;
const M: u8 = 1;
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const EPOCH: u64 = 7;
const INODE: InodeId = 1;

/// The chunk each leg is owed a repair on: in the **second** segment record at a non-zero object
/// offset, so addressing crosses a boundary the resolved chunk list hides. [`DECOY`] sits in the
/// first record, owed nothing; [`TRAILER`] is what a racing writer appends.
const CHUNK: ChunkId = 0xA2_00;
/// Its neighbour **inside the same `seg:` record** — the sibling a repair must MERGE with.
const SIBLING: ChunkId = 0xA3_00;
const DECOY: ChunkId = 0xA1_00;
const TRAILER: ChunkId = 0xA4_00;

const SURVIVOR: DServerId = 0;
/// Where the lost fragment was placed — in no leg's fleet or topology: it is the loss.
const LOST: DServerId = 1;
const FREE: DServerId = 2;
const RACER: DServerId = 7;
/// The largest [`DServerId`] there is: re-encoding a one-digit placement entry to this
/// twenty-digit one is the ~19 bytes the ceiling leg's repoint outgrows its budget by, exactly as
/// `crates/custodian/tests/placement_ceiling.rs:66` sizes the flat arm's.
const HUGE: DServerId = u64::MAX;

fn group() -> SegmentGroup {
    SegmentGroup::new(NONCE, EPOCH).unwrap()
}

fn chunk_ref(id: ChunkId, placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id,
        scheme: EcScheme::ReedSolomon { k: K, m: M },
        len: CHUNK_LEN,
        placement,
    }
}

/// The records every leg but the ceiling one seeds: a decoy in segment 0, the chunk under
/// repair and its sibling in segment 1.
fn records() -> Vec<SegmentRecord> {
    vec![
        SegmentRecord::new(vec![chunk_ref(DECOY, vec![SURVIVOR, LOST])], 0).unwrap(),
        SegmentRecord::new(
            vec![
                chunk_ref(CHUNK, vec![SURVIVOR, LOST]),
                chunk_ref(SIBLING, vec![SURVIVOR, LOST]),
            ],
            CHUNK_LEN,
        )
        .unwrap(),
    ]
}

async fn put(meta: &MemMeta, key: Vec<u8>, value: Bytes) {
    let outcome = meta
        .commit(WriteBatch::new().put(key, value))
        .await
        .unwrap();
    assert_eq!(outcome, CommitOutcome::Committed, "fixture: seeding failed");
}

/// Seed a committed **segmented** object from raw records — each at its own `seg:` key, under a
/// root whose table is derived from the records themselves, so the fixture cannot drift from the
/// shape the resolver checks. Never a committer: this build ships no producer of segmented maps
/// (`segmented_map_restore.rs:387-431` does the same).
async fn seed(meta: &MemMeta, records: &[SegmentRecord]) {
    let group = group();
    let mut segments = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let index = index as u32;
        segments.push(SegmentRef {
            index,
            byte_offset: record.byte_offset(),
            byte_len: record.byte_len(),
        });
        let key = metadata::seg_key(&group, index).unwrap();
        put(meta, key, metadata::encode(record)).await;
    }
    let map = SegmentedMap::new(group, segments).unwrap();
    let root = InodeRecord {
        size: map.span(),
        chunk_map: ChunkMap::Segmented(map),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    let key = metadata::inode_key(INODE);
    put(meta, key.clone(), metadata::encode(&root)).await;
    let resolved = metadata::resolve_chunk_map(meta, &key, &root).await;
    assert!(
        matches!(resolved, Ok(Some(_))),
        "fixture: the seeded segmented object must resolve"
    );
}

/// Store the surviving fragment 0 of `chunk` on `d0` — real shards through the production
/// encoder, so the loop's verify passes. Fragment 1, on [`LOST`], is the loss every leg repairs.
async fn survivor(d0: &MemDServer, chunk: ChunkId) {
    let data = vec![b'w'; CHUNK_LEN as usize];
    let shards = erasure::encode(K.into(), M.into(), &data).expect("shards encode");
    let frag = FragmentId { chunk, index: 0 };
    let bytes = encode_ec_fragment(chunk, 0, K, M, &shards[0]);
    d0.put_fragment(frag, bytes, None).await.unwrap();
}

async fn enqueue(meta: &MemMeta, chunk: ChunkId) {
    repair::enqueue_repair(meta, chunk, "scrub").await.unwrap();
}

async fn queued(meta: &MemMeta) -> Vec<ChunkId> {
    let mut chunks = repair::queued_repairs(meta).await.unwrap();
    chunks.sort_unstable();
    chunks
}

async fn segment(meta: &MemMeta, index: u32) -> Bytes {
    let key = metadata::seg_key(&group(), index).unwrap();
    meta.get(&key).await.unwrap().expect("fixture: seeded")
}

async fn placements(meta: &MemMeta, index: u32) -> Vec<Vec<DServerId>> {
    let record: SegmentRecord = metadata::decode(&segment(meta, index).await).unwrap();
    record
        .chunks()
        .iter()
        .map(|chunk| chunk.placement.clone())
        .collect()
}

async fn root(meta: &MemMeta) -> Bytes {
    let key = metadata::inode_key(INODE);
    meta.get(&key).await.unwrap().expect("fixture: seeded")
}

async fn orphans(meta: &MemMeta) -> Vec<Vec<u8>> {
    let rows = meta.scan(b"orphan:").await.unwrap();
    rows.into_iter().map(|(key, _)| key).collect()
}

/// The zone every leg runs over: the survivor in domain "a", ONE free domain "c" held by `free`.
fn zone(free: DServerId) -> Topology {
    let mut topology = Topology::default();
    topology.register(SURVIVOR, "a").register(free, "c");
    topology
}

/// One pass through the real fenced control point, beside what it emitted on the durability seam.
async fn run(
    meta: &MemMeta,
    d0: &MemDServer,
    free: (DServerId, &MemDServer),
    topology: &Topology,
) -> (Reconciled, String) {
    enable_audit_callsites();
    let coord = MemCoordination::new();
    let leader = Custodian::elect(&coord, "zone-repoint").await.unwrap();
    let mut fenced = FencedZone::new();
    fenced.install(leader.leadership());
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(SURVIVOR, d0), (free.0, free.1)];
    let ctx = ReconstructionContext {
        meta,
        fleet: &fleet,
        topology,
        unreachable: &[],
    };
    let capture = Capture::default();
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(capture.clone());
    let outcome = reconcile_step(&fenced, &leader, None, None, Some(&ctx), None, NOW)
        .with_subscriber(tracing::Dispatch::new(
            tracing_subscriber::registry().with(layer),
        ))
        .await
        .expect("one multipart object must not stop repair for the whole store");
    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    (outcome, logged)
}

/// The shared fixture, owed a repair on each of `owed` — each a chunk whose `ChunkRef` lives in
/// segment record 1, so a leg can put two obligations inside ONE record.
async fn fixture_owing(owed: &[ChunkId]) -> (MemMeta, MemDServer, MemDServer) {
    let (meta, d0, free) = (
        MemMeta::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    seed(&meta, &records()).await;
    for &chunk in owed {
        survivor(&d0, chunk).await;
        enqueue(&meta, chunk).await;
    }
    (meta, d0, free)
}

async fn fixture() -> (MemMeta, MemDServer, MemDServer) {
    fixture_owing(&[CHUNK]).await
}

/// A `seg:` record's bytes as a **racing writer** would leave them — a real record, not a marker.
fn rewritten(index: u32, chunks: Vec<ChunkRef>, byte_offset: u64) -> (Vec<u8>, Bytes) {
    let record = SegmentRecord::new(chunks, byte_offset).unwrap();
    let key = metadata::seg_key(&group(), index).unwrap();
    (key, metadata::encode(&record))
}

fn torn(index: u32, record: &SegmentRecord) -> (Vec<u8>, Bytes) {
    let key = metadata::seg_key(&group(), index).unwrap();
    let bytes = metadata::encode(record);
    (key, bytes.slice(..bytes.len() / 2))
}

/// Land `racing` in the read→prepare window of one pass owed a repair on each of `owed`, and
/// assert the repair LOST: the race really landed, the root was never rewritten, segment 1 still
/// holds exactly `expect`.
async fn raced_and_lost(owed: &[ChunkId], racing: Racing, expect: &Bytes) -> (Reconciled, String) {
    let (meta, d0, free) = fixture_owing(owed).await;
    let topology = zone(FREE);
    let root_before = root(&meta).await;
    meta.arm(Race::AfterSegmentPage, racing);

    let (outcome, logged) = run(&meta, &d0, (FREE, &free), &topology).await;

    assert!(
        meta.raced(),
        "fixture: the racing write never landed, so this leg proved nothing: {logged}"
    );
    assert_eq!(
        root(&meta).await,
        root_before,
        "the root was rewritten: {logged}"
    );
    assert_no_repair_metadata(&meta, outcome, expect, owed).await;
    (outcome, logged)
}

/// What a repair that LOST must leave behind: segment 1 holding exactly `record`, every
/// obligation still queued, no orphan mark, and a pass that certifies nothing — **repair-owned**
/// metadata, since the superseded-root leg's fixture writes a competing root.
async fn assert_no_repair_metadata(
    meta: &MemMeta,
    outcome: Reconciled,
    record: &Bytes,
    owed: &[ChunkId],
) {
    assert_eq!(
        &segment(meta, 1).await,
        record,
        "the segment record still holds EXACTLY the bytes it held before: a stale plan never \
         lands its own placement over newer ones"
    );
    assert_eq!(
        queued(meta).await,
        owed.to_vec(),
        "every obligation is kept, never discarded — the queue is the last record saying live \
         data is under-replicated"
    );
    assert!(
        orphans(meta).await.is_empty(),
        "no orphan mark was published for a move that did not happen"
    );
    assert_ne!(
        outcome,
        Reconciled::Changed,
        "the pass must not certify a repair it did not make"
    );
}

/// A committed **segmented** object whose chunk has lost a fragment is repaired: the rebuilt
/// fragment lands in a domain distinct from the survivor's, the **`seg:` record's** own
/// `ChunkRef` names it, the obligation is discharged in the same mutation. RED on the base.
#[tokio::test]
async fn a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record() {
    let (meta, d0, free) = fixture().await;
    let topology = zone(FREE);
    let root_before = root(&meta).await;
    let decoy_before = segment(&meta, 0).await;

    let (outcome, logged) = run(&meta, &d0, (FREE, &free), &topology).await;

    assert_eq!(
        outcome,
        Reconciled::Changed,
        "a repair that moved a placement is a change the pass must report: {logged}"
    );
    assert_eq!(
        placements(&meta, 1).await,
        vec![vec![SURVIVOR, FREE], vec![SURVIVOR, LOST]],
        "the `seg:` record's own ChunkRef names the rebuilt fragment's new D server, and only \
         the chunk owed a repair moved: {logged}"
    );
    assert_ne!(
        topology.domain_of(SURVIVOR),
        topology.domain_of(FREE),
        "fixture: the rebuild's target is in a domain distinct from the survivor's"
    );
    let rebuilt = FragmentId {
        chunk: CHUNK,
        index: 1,
    };
    assert!(
        free.get_fragment(rebuilt).await.unwrap().is_some(),
        "the rebuilt fragment is on the D server the record now names"
    );
    assert!(
        queued(&meta).await.is_empty(),
        "the repair discharged its obligation in the same mutation — what exits the \
         refused-forever state"
    );
    assert_eq!(root(&meta).await, root_before, "the root was rewritten");
    assert_eq!(
        segment(&meta, 0).await,
        decoy_before,
        "the other segment record was rewritten: the move addresses ONE"
    );
}

/// A competing writer moves a **different** chunk of the same `seg:` record between the
/// resolver's page and the move's own read. Both survive — the design decision, since the move
/// pins the record's **freshly-read** bytes plus **the reference it planned from**; pinning the
/// record as the resolve saw it would stall concurrent repairs and redden this leg. RED on base.
#[tokio::test]
async fn a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged() {
    let (meta, d0, free) = fixture().await;
    let topology = zone(FREE);
    meta.arm(
        Race::AfterSegmentPage,
        vec![rewritten(
            1,
            vec![
                chunk_ref(CHUNK, vec![SURVIVOR, LOST]),
                chunk_ref(SIBLING, vec![SURVIVOR, RACER]),
            ],
            CHUNK_LEN,
        )],
    );

    let (outcome, logged) = run(&meta, &d0, (FREE, &free), &topology).await;

    assert!(
        meta.raced(),
        "fixture: the racing write never landed, so this leg proved nothing: {logged}"
    );
    assert_eq!(
        outcome,
        Reconciled::Changed,
        "a repair inside a record another writer touched must still land: {logged}"
    );
    assert_eq!(
        placements(&meta, 1).await,
        vec![vec![SURVIVOR, FREE], vec![SURVIVOR, RACER]],
        "BOTH survive: the repair repointed its own chunk and the competing writer's SIBLING \
         move was merged, neither overwritten nor conflicted"
    );
    assert!(
        queued(&meta).await.is_empty(),
        "the obligation is discharged: a sibling's edit is not a reason to stall a repair"
    );
}

/// The same window, but the competing writer moves **the planned chunk's own** placement. The
/// repair must lose: repointing anyway would silently revert a newer placement onto bytes the
/// move had just re-read. NOT independently red; its named negation is deleting the
/// `chunk == prior` equality. The rebuilt destination **fragment** is deliberately not asserted
/// about — retracting a published write is what #638 rejected four times; what matters is that
/// no METADATA moved.
#[tokio::test]
async fn a_racing_move_of_the_planned_chunk_itself_is_a_conflict_that_writes_nothing() {
    let (key, competing) = rewritten(
        1,
        vec![
            chunk_ref(CHUNK, vec![SURVIVOR, RACER]),
            chunk_ref(SIBLING, vec![SURVIVOR, LOST]),
        ],
        CHUNK_LEN,
    );

    raced_and_lost(&[CHUNK], vec![(key, competing.clone())], &competing).await;
}

/// The root is flipped to a different generation after the resolve and before the commit. A
/// supersede always moves the root first (`0016:2452-2462`), so a repoint racing one must lose
/// rather than repoint a generation nothing references. NOT independently red; its named
/// negation is deleting the root precondition.
#[tokio::test]
async fn a_superseded_root_generation_makes_the_repair_lose_without_writing_its_own_metadata() {
    let (meta, d0, free) = fixture().await;
    let topology = zone(FREE);
    let segment_before = segment(&meta, 1).await;
    let seeded: InodeRecord = metadata::decode(&root(&meta).await).unwrap();
    let superseding = InodeRecord {
        version: seeded.version + 1,
        ..seeded.clone()
    };
    meta.arm(
        Race::IntoCommit,
        vec![(metadata::inode_key(INODE), metadata::encode(&superseding))],
    );

    let (outcome, logged) = run(&meta, &d0, (FREE, &free), &topology).await;

    // A repoint conditioned on a retired root generation loses its compare-and-swap.
    assert_no_repair_metadata(&meta, outcome, &segment_before, &[CHUNK]).await;
    let after: InodeRecord = metadata::decode(&root(&meta).await).unwrap();
    assert_eq!(
        after.chunk_map, seeded.chunk_map,
        "the only root the store holds is one this fixture wrote: a move never rewrites a root: \
         {logged}"
    );
}

/// A filler chunk no leg is owed a repair on, `extra` bytes over the narrowest it can be: each
/// placement entry encodes as two bytes (`,0`), and a two-digit id pays the odd byte.
fn filler(extra: usize) -> ChunkRef {
    let (id, entries) = match extra {
        0 => (7, 0),
        odd if odd % 2 == 1 => (7, odd / 2 + 1),
        even => (17, even / 2),
    };
    ChunkRef {
        id,
        scheme: EcScheme::None,
        len: 1,
        placement: vec![0; entries],
    }
}

/// A segment record over `chunks` at `byte_offset`, padded until it encodes to EXACTLY `target`
/// bytes — measured, so a codec change moves the padding, not the property under test. The last
/// filler is widened by what is missing; its `len` is untouched, so the record's span holds.
fn padded(chunks: Vec<ChunkRef>, byte_offset: u64, target: usize) -> SegmentRecord {
    let build = |chunks: &[ChunkRef]| SegmentRecord::new(chunks.to_vec(), byte_offset).unwrap();
    let width = |chunks: &[ChunkRef]| metadata::encode(&build(chunks)).len();
    let mut chunks = chunks;
    loop {
        let mut candidate = chunks.clone();
        candidate.push(filler(0));
        if width(&candidate) > target {
            break;
        }
        chunks = candidate;
    }
    let last = chunks.len() - 1;
    assert!(last > 0, "fixture: no room to pad this record");
    chunks[last] = filler(target - width(&chunks));
    let record = build(&chunks);
    assert_eq!(
        metadata::encode(&record).len(),
        target,
        "fixture: the padding missed its target"
    );
    record
}

/// #710 established that a placement write weighs the record it would leave behind BEFORE it
/// writes anything, and refuses rather than commit one no later repair could overwrite — for the
/// flat arm only. This pins it for the segmented one: a `seg:` record seeded exactly ON the
/// budget a publication writes one under (`MAX_ROOT_VALUE_BYTES`, half the value ceiling,
/// `0016:1462-1467`), whose repoint onto a twenty-digit id would land ~19 bytes past it. NOT
/// independently red; its named negation is widening the budget to the value ceiling.
#[tokio::test]
async fn a_repoint_that_would_outgrow_a_segment_records_budget_is_refused() {
    let (meta, d0, free) = (
        MemMeta::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let topology = zone(HUGE);
    seed(
        &meta,
        &[
            SegmentRecord::new(vec![chunk_ref(DECOY, vec![SURVIVOR, LOST])], 0).unwrap(),
            padded(
                vec![chunk_ref(CHUNK, vec![SURVIVOR, LOST])],
                CHUNK_LEN,
                MAX_ROOT_VALUE_BYTES,
            ),
        ],
    )
    .await;
    survivor(&d0, CHUNK).await;
    enqueue(&meta, CHUNK).await;
    let before = segment(&meta, 1).await;

    let (outcome, logged) = run(&meta, &d0, (HUGE, &free), &topology).await;

    let after = segment(&meta, 1).await;
    assert!(
        after.len() <= MAX_ROOT_VALUE_BYTES,
        "{} bytes stored, past the budget a publication writes a segment record under",
        after.len()
    );
    assert_eq!(
        after, before,
        "a refusal writes NOTHING: the segment record is byte-identical: {logged}"
    );
    assert_eq!(
        queued(&meta).await,
        vec![CHUNK],
        "the obligation stays queued — a refusal is 'this record must shrink first'"
    );
    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "a pass that refused a repair has a hole in its picture, so it certifies nothing"
    );
}

/// The same window, but the competing writer leaves a record the **root's own segment table no
/// longer describes**: it APPENDS a chunk, so the record covers more of the object than the
/// root's `SegmentRef` says, while the planned chunk stays where the plan left it — addressing
/// alone would rewrite it. The repair must lose: the read path refuses such a record outright
/// (`resolve_chunk_map`'s extent check, `crates/core/src/metadata.rs:2582`), so rewriting it
/// would mint a FRESH generation of a row no consumer can resolve — another writer's
/// inconsistency adopted as this pass's durable write (ADR-0045 decision 3). NOT independently
/// red; its named negation is deleting the extent comparison.
#[tokio::test]
async fn a_racing_rewrite_that_leaves_the_roots_segment_table_behind_is_a_conflict() {
    let (key, competing) = rewritten(
        1,
        vec![
            chunk_ref(CHUNK, vec![SURVIVOR, LOST]),
            chunk_ref(SIBLING, vec![SURVIVOR, LOST]),
            chunk_ref(TRAILER, vec![SURVIVOR, RACER]),
        ],
        CHUNK_LEN,
    );

    // Span and all: a repair does not re-publish a record whose span contradicts its root.
    raced_and_lost(&[CHUNK], vec![(key, competing.clone())], &competing).await;
}

/// The same window again, but the record the move re-reads is **torn** — present, and not a
/// record any more. That is not the retirement race (retired records are *deleted*, never
/// scribbled over), so it is this object's own corruption: answered as a conflict it would
/// quietly re-plan the same damaged record every pass forever, with no actor exiting the state —
/// the shape #721 exists to remove. So the pass names the object on the durability seam, keeps
/// the obligations, drains nothing and certifies nothing, as it already does for an object the
/// *reading* could not resolve.
///
/// **TWO** obligations sit in that one record, because the name and the count are per OBJECT: a
/// damaged record is repaired one at a time, so a second name would inflate the counter beside it
/// and hand an operator two repair obligations for one stored row (core states the same rule for
/// the same namespace — "at most once per row", `crates/core/src/metadata.rs:2124-2130`), and the
/// refusal this slice removes already held it. RED on the base, which refuses such an object
/// under a different action and never meets this record at all; the named negations are answering
/// the decode failure with a conflict, and dropping the contained-key set.
#[tokio::test]
async fn a_torn_segment_record_is_contained_once_per_object_never_re_planned() {
    let (key, half) = torn(1, &records()[1]);
    assert!(
        metadata::decode::<SegmentRecord>(&half).is_err(),
        "fixture: the torn bytes must not decode, or this leg tests nothing"
    );

    let (outcome, logged) =
        raced_and_lost(&[CHUNK, SIBLING], vec![(key, half.clone())], &half).await;

    assert_eq!(
        (
            rows(&logged, "unresolvable-chunk-map"),
            ticks(&logged, "reconstruction_unresolvable_records")
        ),
        (1, 1),
        "one damaged record is ONE name and ONE count, however many obligations meet it: {logged}"
    );
    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "a pass that met a record it could not read certifies nothing: {logged}"
    );
}
