//! Issue #721 (0016 decision 7(f), `0016:669`): **a chunk whose `ChunkRef` lives in a `seg:`
//! record is REPAIRED — the maintenance write path for a segmented map exists and the repair
//! pass completes through it.**
//!
//! On the base #697's containment left the obligation in a state nothing exits: the repair is
//! *refused*, every pass, forever — it may not be drained (that would be data loss) and no code
//! path could move the placement, because the only placement writer in the tree rebuilds an
//! **inode** record. A published multipart object's redundancy therefore decayed untended,
//! permanently — a state with no actor that exits it in bounded time, which is what C-1 forbids
//! (`docs/principles.md` §5).
//!
//! Five legs, each driving the real fenced control point [`reconcile_step`] and observing the
//! **store**. Legs 1 and 2 are RED on the base; legs 3, 4 and 5 are not independently red — the
//! base refuses, so it writes nothing for its own reason — and each carries a named negation
//! instead (delete the pin, watch the leg go red).
//!
//! **No assertion names a symbol this patch introduces**, since the per-fix red leg reverts the
//! production files and keeps this one: everything is driven through `reconcile_step` and read
//! back out of the store. `MAX_ROOT_VALUE_BYTES` is a base constant
//! (`crates/core/src/metadata.rs:352`) and may be named.
//!
//! **Reaching the read→prepare window.** The two reads a repair makes on a `seg:` record are on
//! DIFFERENT store methods: the resolver reads the group's range with `scan_page`
//! (`metadata::read_group_range`) and never `get`, while the move's own read is the only `get`
//! anyone performs on a `seg:` key. So the window legs 2 and 3 need is *between the resolver's
//! page and that `get`*, and [`MemMeta`] reaches it by applying the racing writer's batch
//! **after answering the `seg:` page**. Counting `get`s would not work: there is only one, and
//! it is already the move's.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
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

// ---- in-memory trait doubles (the pass is proven over the seams, backend-agnostic) ----

/// When the racing writer's batch lands, relative to the pass's own reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Race {
    /// **After the resolver's bounded `seg:` page is answered** — the one instant after the
    /// read the plan is built from and before the move's own `get` of the record it rewrites.
    /// Legs 2 and 3 live here.
    AfterSegmentPage,
    /// **On the way into a commit**, so the resolve has already completed and the pass does not
    /// simply restart onto the new generation the way a root moving *during* it would make it.
    /// Leg 4's superseding root generation lands here.
    IntoCommit,
}

/// What the racing writer lands: raw key/value pairs, with no preconditions of its own — it is
/// the writer that got there first, not one competing for a compare-and-swap.
type Racing = Vec<(Vec<u8>, Bytes)>;

/// A `BTreeMap`-backed metadata store — key order, so what a leg reads back is the store's own
/// ordering — that can also play a **racing writer** at one of the two instants above.
#[derive(Default)]
struct MemMeta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    /// The racing writer's pending puts and when they land. Taken when it fires, so it races
    /// exactly once.
    racing: Mutex<Option<(Race, Racing)>>,
}

impl MemMeta {
    /// Arm the racing writer: `puts` land the first time the pass reaches `when`.
    fn arm(&self, when: Race, puts: Racing) {
        *self.racing.lock().unwrap() = Some((when, puts));
    }

    /// Whether the armed batch has landed — the fixture self-check that the leg's window was
    /// actually reached, rather than the leg passing because the race never happened.
    fn raced(&self) -> bool {
        self.racing.lock().unwrap().is_none()
    }

    /// Land the armed batch if it is due at `when`. The `kv` lock is taken and released inside,
    /// so a caller may hold neither.
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

    // The required paginated read (#634): a test double needs *a* body, not a backend's — the
    // dev-only testkit helper pages over this store's own `scan`.
    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<wyrd_traits::ScanPage> {
        let page = wyrd_testkit::test_double_scan_page(self, prefix, after, limit).await;
        // AFTER the page is materialised, so the resolver answers from the pre-race bytes and
        // the plan is built from them — and any later reader of this record sees the race.
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

/// One D server's fragments, holding the **real** stored bytes so checksums verify.
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

// ---- fixture ----

const NOW: u64 = 10_000;
const CHUNK_LEN: u64 = 8;
/// RS(1,1): one data shard plus one parity shard — the smallest scheme carrying redundancy, so
/// the ONE surviving fragment (k = 1) is enough to rebuild the other.
const K: u8 = 1;
const M: u8 = 1;
/// A segment-group nonce (32 lowercase hex characters, `0016:354`) and the fence epoch its
/// segments are scoped by.
const NONCE: &str = "0123456789abcdef0123456789abcdef";
const EPOCH: u64 = 7;
/// The object every leg seeds.
const INODE: InodeId = 1;

/// The chunk each leg is owed a repair on. It lives in the **second** segment record, at a
/// non-zero object offset, so the addressing genuinely crosses a segment boundary the resolved
/// chunk list hides.
const CHUNK: ChunkId = 0xA2_00;
/// Its neighbour **inside the same `seg:` record** — the sibling a racing writer moves in leg 2,
/// which the repair must MERGE rather than serialise on.
const SIBLING: ChunkId = 0xA3_00;
/// A chunk in the FIRST segment record, owed nothing: the offset arithmetic must find the
/// second record, not this one.
const DECOY: ChunkId = 0xA1_00;

/// The survivor's D server, and the failure domain it occupies.
const SURVIVOR: DServerId = 0;
/// Where the lost fragment was placed. In neither the fleet nor the topology of any leg: it is
/// the loss.
const LOST: DServerId = 1;
/// The free failure domain a repair re-places the rebuilt fragment on.
const FREE: DServerId = 2;
/// Where a racing writer moves a placement to — a server no pass of these legs re-places onto,
/// so a placement naming it can only be the racing writer's.
const RACER: DServerId = 7;
/// The largest [`DServerId`] there is: re-encoding a one-digit placement entry to this
/// twenty-digit one is the ~19 bytes leg 5's repoint outgrows its record's budget by, exactly
/// as `crates/custodian/tests/placement_ceiling.rs:66` sizes the flat arm's.
const HUGE: DServerId = u64::MAX;

fn group() -> SegmentGroup {
    SegmentGroup::new(NONCE, EPOCH).unwrap()
}

/// A committed chunk reference: RS(1,1) over [`CHUNK_LEN`] bytes on `placement`.
fn chunk_ref(id: ChunkId, placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id,
        scheme: EcScheme::ReedSolomon { k: K, m: M },
        len: CHUNK_LEN,
        placement,
    }
}

/// The two segment records every leg but the ceiling one seeds: segment 0 holds the decoy,
/// segment 1 holds the repaired chunk followed by its sibling.
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

/// Seed a committed **segmented** object from raw records — each written at its own `seg:` key,
/// under a root whose segment table is derived from the records themselves so the fixture
/// cannot drift from the shape the resolver checks. Raw records throughout, never a committer:
/// this build ships no producer of segmented maps, which is exactly why the fixture writes them
/// by hand (`crates/custodian/tests/segmented_map_restore.rs:387-431` does the same).
///
/// Then **prove the fixture is what the leg thinks it is**: it really resolves, so no leg can
/// pass because the object silently stopped being a readable segmented one.
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
/// encoder, so the loop's identity + checksum verify passes on it and the rebuild is genuine.
/// Fragment 1, on [`LOST`], is never written: the loss every leg repairs.
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

/// The obligations still on the shared repair queue, in a stable order.
async fn queued(meta: &MemMeta) -> Vec<ChunkId> {
    let mut chunks = repair::queued_repairs(meta).await.unwrap();
    chunks.sort_unstable();
    chunks
}

/// One segment record's stored bytes — what a refusal or a conflict must leave byte-identical,
/// and where a landed repoint must show up.
async fn segment(meta: &MemMeta, index: u32) -> Bytes {
    let key = metadata::seg_key(&group(), index).unwrap();
    meta.get(&key).await.unwrap().expect("fixture: seeded")
}

/// The placement vectors inside a segment record, in its own chunk order.
async fn placements(meta: &MemMeta, index: u32) -> Vec<Vec<DServerId>> {
    let record: SegmentRecord = metadata::decode(&segment(meta, index).await).unwrap();
    record
        .chunks()
        .iter()
        .map(|chunk| chunk.placement.clone())
        .collect()
}

/// The root record's stored bytes — a placement move rewrites the SEGMENT, never the root.
async fn root(meta: &MemMeta) -> Bytes {
    let key = metadata::inode_key(INODE);
    meta.get(&key).await.unwrap().expect("fixture: seeded")
}

/// Every orphan-ledger key the store holds: the evidence a repair publishes for the fragment it
/// displaced, so a repair that did not happen must have written none.
async fn orphans(meta: &MemMeta) -> Vec<Vec<u8>> {
    let rows = meta.scan(b"orphan:").await.unwrap();
    rows.into_iter().map(|(key, _)| key).collect()
}

/// The zone every leg runs over: the survivor's server in failure domain "a", and ONE free
/// domain "c" held by `free`. [`LOST`] is in neither the fleet nor the topology.
fn zone(free: DServerId) -> Topology {
    let mut topology = Topology::default();
    topology.register(SURVIVOR, "a").register(free, "c");
    topology
}

/// One reconstruction pass through the **real fenced control point**. Reconstruction is the
/// only loop wired: the others walk `inode:` themselves and would answer over the same store.
async fn run(
    meta: &MemMeta,
    d0: &MemDServer,
    free: (DServerId, &MemDServer),
    topology: &Topology,
) -> Reconciled {
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
    reconcile_step(&fenced, &leader, None, None, Some(&ctx), None, NOW)
        .await
        .expect("one multipart object must not stop repair for the whole store")
}

/// The store, the survivor's D server and the free domain's, seeded with the two-segment object
/// and one obligation on [`CHUNK`] — the fixture legs 1 to 4 share.
async fn fixture() -> (MemMeta, MemDServer, MemDServer) {
    let (meta, d0, free) = (
        MemMeta::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    seed(&meta, &records()).await;
    survivor(&d0, CHUNK).await;
    enqueue(&meta, CHUNK).await;
    (meta, d0, free)
}

/// A `seg:` record's bytes as a **racing writer** would leave them: `chunks`, at `byte_offset`,
/// keyed at `index`. The competing generation is written through the same encoder the pass
/// reads, so what a leg asserts afterwards is a real record and not a marker.
fn rewritten(index: u32, chunks: Vec<ChunkRef>, byte_offset: u64) -> (Vec<u8>, Bytes) {
    let record = SegmentRecord::new(chunks, byte_offset).unwrap();
    let key = metadata::seg_key(&group(), index).unwrap();
    (key, metadata::encode(&record))
}

// ---- leg 1: the repair lands, in the `seg:` record that holds the chunk ----

/// A committed **segmented** object whose chunk has lost a fragment is repaired: the rebuilt
/// fragment is placed on a healthy D server in a failure domain distinct from the survivor's,
/// the **`seg:` record's** own `ChunkRef` names it, and the obligation is discharged in the
/// same mutation. RED on the base, which refuses: the `seg:` bytes come back byte-identical,
/// the obligation is still queued, and the pass answers `Blocked`.
#[tokio::test]
async fn a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record() {
    let (meta, d0, free) = fixture().await;
    let topology = zone(FREE);
    let root_before = root(&meta).await;
    let decoy_before = segment(&meta, 0).await;

    let outcome = run(&meta, &d0, (FREE, &free), &topology).await;

    assert_eq!(
        outcome,
        Reconciled::Changed,
        "a repair that moved a placement is a change the pass must report"
    );
    // The binding assertion: the placement lives in the SEGMENT record, so that is where the
    // repoint has to show up.
    assert_eq!(
        placements(&meta, 1).await,
        vec![vec![SURVIVOR, FREE], vec![SURVIVOR, LOST]],
        "the `seg:` record's own ChunkRef must name the rebuilt fragment's new D server, and \
         only the chunk owed a repair may move"
    );
    assert_ne!(
        topology.domain_of(SURVIVOR),
        topology.domain_of(FREE),
        "fixture: the rebuild's target must be in a failure domain distinct from the survivor's"
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
        "the repair discharged its obligation in the same mutation — the queue is how a \
         refused-forever repair stayed visible, and completing the write is what exits it"
    );
    // A repoint rewrites the SEGMENT, never the root: two moves inside one multipart object
    // must not serialise on the root, and the root's segment-table budget is not maintenance's.
    assert_eq!(root(&meta).await, root_before, "the root was rewritten");
    assert_eq!(
        segment(&meta, 0).await,
        decoy_before,
        "the other segment record was rewritten; the move addresses ONE record"
    );
}

// ---- leg 2: a sibling's racing move inside the same record is MERGED ----

/// A competing writer moves a **different** chunk of the same `seg:` record between the
/// resolver's page and the move's own read. Both survive: the repair lands AND the sibling's
/// new placement is still there afterwards.
///
/// This pins the design decision. The move re-reads the one record it rewrites and pins that
/// record's **freshly-read** bytes plus **the reference it planned from** — so an edit to
/// another chunk merges, while an edit to the planned chunk itself (leg 3) conflicts. Pinning
/// the whole record as the resolve saw it would turn every concurrent repair inside one
/// multipart object into a stall, and this leg would go red.
///
/// RED on the base: the pass refuses, so the repair never lands.
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

    let outcome = run(&meta, &d0, (FREE, &free), &topology).await;

    assert!(
        meta.raced(),
        "fixture: the racing write never landed, so this leg proved nothing"
    );
    assert_eq!(
        outcome,
        Reconciled::Changed,
        "a repair inside a record another writer touched must still land"
    );
    assert_eq!(
        placements(&meta, 1).await,
        vec![vec![SURVIVOR, FREE], vec![SURVIVOR, RACER]],
        "BOTH survive: the repair repointed its own chunk, and the competing writer's move of \
         a SIBLING chunk in the same record was merged, not overwritten and not conflicted"
    );
    assert!(
        queued(&meta).await.is_empty(),
        "the obligation is discharged: a sibling's edit is not a reason to stall a repair"
    );
}

// ---- leg 3: a racing move of the planned chunk itself is a CONFLICT ----

/// The same window, but the competing writer moves **the planned chunk's own** placement. The
/// repair must lose: its plan was built from a reference that is no longer there, and
/// repointing anyway would silently revert a newer placement onto bytes the move had just
/// re-read.
///
/// NOT independently red — pre-fix the pass refuses, and a refusal writes nothing either. Its
/// named negation: delete the `chunk == prior` equality from the primitive's addressing rule
/// and this leg goes red, the chunk having been matched on its byte offset alone.
///
/// The rebuilt destination **fragment** is deliberately not asserted about: production writes
/// fragments before the commit, and retracting an already-published write is the rule #638
/// rejected four times. What matters here is that no METADATA moved.
#[tokio::test]
async fn a_racing_move_of_the_planned_chunk_itself_is_a_conflict_that_writes_nothing() {
    let (meta, d0, free) = fixture().await;
    let topology = zone(FREE);
    let root_before = root(&meta).await;
    let (key, competing) = rewritten(
        1,
        vec![
            chunk_ref(CHUNK, vec![SURVIVOR, RACER]),
            chunk_ref(SIBLING, vec![SURVIVOR, LOST]),
        ],
        CHUNK_LEN,
    );
    meta.arm(Race::AfterSegmentPage, vec![(key, competing.clone())]);

    let outcome = run(&meta, &d0, (FREE, &free), &topology).await;

    assert!(
        meta.raced(),
        "fixture: the racing write never landed, so this leg proved nothing"
    );
    assert_eq!(
        segment(&meta, 1).await,
        competing,
        "the record still holds EXACTLY the competing writer's placement, byte for byte: a \
         stale plan never lands its own placement over a newer one"
    );
    assert_eq!(root(&meta).await, root_before, "the root was rewritten");
    assert_eq!(
        queued(&meta).await,
        vec![CHUNK],
        "the obligation is kept, never discarded: nothing repaired the chunk, and the queue is \
         the last record saying live data is under-replicated"
    );
    assert!(
        orphans(&meta).await.is_empty(),
        "no orphan mark was published for a move that did not happen"
    );
    assert_ne!(
        outcome,
        Reconciled::Changed,
        "the pass must not certify a repair it did not make (pre-fix it refuses and answers \
         `Blocked`; post-fix a lost race is transient and re-assessed next pass, exactly as \
         the flat path has always answered it)"
    );
}

// ---- leg 4: a superseded root generation makes the move lose ----

/// The root is flipped to a different generation after the resolve completed and before the
/// commit. A supersede always moves the root first (`0016:2452-2462`), so a repoint racing one
/// must lose rather than repoint a generation nothing references any more.
///
/// Asserted as **repair-owned** metadata rather than "nothing was written": this leg's own
/// fixture necessarily writes the competing root generation, so a blanket no-write assertion
/// would contradict it.
///
/// NOT independently red — pre-fix the pass refuses before it ever reaches a commit, so the
/// racing writer never even fires. Its named negation: delete the root precondition from the
/// primitive and this leg goes red, the repoint having landed on a retired generation.
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

    let outcome = run(&meta, &d0, (FREE, &free), &topology).await;

    assert_eq!(
        segment(&meta, 1).await,
        segment_before,
        "the placement did not move: a repoint conditioned on a retired root generation loses \
         its compare-and-swap instead of writing into it"
    );
    assert_eq!(
        queued(&meta).await,
        vec![CHUNK],
        "the obligation is kept for the next pass, which will re-plan against the live root"
    );
    assert!(
        orphans(&meta).await.is_empty(),
        "no orphan mark was published for a move that did not happen"
    );
    let after: InodeRecord = metadata::decode(&root(&meta).await).unwrap();
    assert_eq!(
        after.chunk_map, seeded.chunk_map,
        "the only root the store holds is a generation this fixture wrote — a placement move \
         never rewrites a root at all"
    );
    assert_ne!(
        outcome,
        Reconciled::Changed,
        "the pass must not certify a repair it did not make"
    );
}

// ---- leg 5: the ceiling refusal holds over a segment record ----

/// A filler chunk no leg is owed a repair on, carrying `extra` bytes over the narrowest one it
/// can be: each placement entry past the first is two encoded bytes (`,0`), the first is one
/// (`0`), and a two-digit id pays the odd byte. Every filler shares one id — the padding's only
/// property is its encoded width.
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

/// A segment record over `chunks` at `byte_offset`, padded with fillers until it encodes to
/// EXACTLY `target` bytes.
///
/// Measured, never a hard-coded chunk count: the fixture states the ceiling it means, so a
/// codec change moves the padding instead of silently moving the property under test.
fn padded(chunks: Vec<ChunkRef>, byte_offset: u64, target: usize) -> SegmentRecord {
    let build = |chunks: &[ChunkRef]| SegmentRecord::new(chunks.to_vec(), byte_offset).unwrap();
    let width = |chunks: &[ChunkRef]| metadata::encode(&build(chunks)).len();
    let mut chunks = chunks;
    let mut fillers = 0usize;
    loop {
        let mut candidate = chunks.clone();
        candidate.push(filler(0));
        if width(&candidate) > target {
            break;
        }
        chunks = candidate;
        fillers += 1;
    }
    assert!(fillers > 0, "fixture: no room to pad this record");
    // Widen the LAST filler by what is still missing. Its own `len` is untouched, so the span
    // the record claims — and the root's table with it — does not move.
    let gap = target - width(&chunks);
    let last = chunks.len() - 1;
    chunks[last] = filler(gap);
    let record = build(&chunks);
    assert_eq!(
        metadata::encode(&record).len(),
        target,
        "fixture: the padding missed its target"
    );
    record
}

/// #710 established that a placement write weighs the record it would leave behind BEFORE it
/// writes anything, and refuses rather than commit one no later repair could overwrite. It
/// could only state that for the flat arm; this pins it for the segmented one.
///
/// A `seg:` record seeded exactly ON the budget a conforming publication writes one under
/// (`MAX_ROOT_VALUE_BYTES` — half the value ceiling, `0016:1462-1467`) whose repoint would move
/// a fragment onto a twenty-digit id, growing it ~19 bytes past that: refused, record
/// byte-identical, obligation queued, nothing certified.
///
/// NOT independently red — pre-fix the pass refuses this object for the other reason. Its named
/// negation: widen the budget to the full value ceiling and this leg goes red.
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

    let outcome = run(&meta, &d0, (HUGE, &free), &topology).await;

    let after = segment(&meta, 1).await;
    assert!(
        after.len() <= MAX_ROOT_VALUE_BYTES,
        "{} bytes stored, past the budget a publication writes a segment record under",
        after.len()
    );
    assert_eq!(
        after, before,
        "a refusal writes NOTHING: the segment record is byte-identical"
    );
    assert_eq!(
        queued(&meta).await,
        vec![CHUNK],
        "the obligation stays queued — a refusal is 'this record must shrink first', never a \
         repair that happened"
    );
    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "a pass that refused a repair has a hole in its redundancy picture, so it certifies \
         nothing: an operator reading `Satisfied` is told redundancy is restored"
    );
}
