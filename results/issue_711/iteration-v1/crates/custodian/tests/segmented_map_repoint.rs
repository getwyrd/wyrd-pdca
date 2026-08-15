//! Issue #711 (0016 decision 7(f), `0016:669`): **a chunk that lives in a `seg:` record is
//! repaired and evacuated like any other — its placement is moved in the record that holds
//! it.**
//!
//! #695/#696/#697 stopped the three maintenance passes aborting on a segmented object, but
//! they deliberately wrote nothing: a repair obligation or a drain evacuation for a
//! `seg:`-resident chunk was REFUSED and stayed queued, every pass, forever. Nothing exited
//! that state — the obligation may not be drained (that is data loss) and no code path could
//! move the placement, because the only placement writers in the tree rebuilt an **inode**
//! record. So a published multipart object's redundancy decayed untended and a D-server
//! decommission holding one of its fragments never converged. Both are permanent states,
//! which C-1 rules out as costs (`docs/principles.md` §5).
//!
//! Every leg drives the REAL fenced control point
//! [`reconcile_step`](wyrd_custodian::reconcile_step) over in-memory trait doubles and then
//! reads the **store** — the `seg:` record's own bytes, the shared repair queue, the orphan
//! ledger, the operator's drain status. **No assertion names a symbol this patch introduces**
//! (the per-fix red leg reverts the production files and keeps this one, so such a reference
//! would degrade the red to a compile error).
//!
//! Legs 1, 2 and 4 are red on the base — refused, nothing written, obligation queued. Leg 3
//! is **not** independently red: pre-fix the move is refused for the other reason. It ships
//! because it pins the ceiling rule for the segmented arm, which #710's flat-only fixture
//! cannot.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::erasure;
use wyrd_core::metadata::{
    self, decode, encode, inode_key, orphan_key, seg_key, ChunkMap, ChunkRef, EcScheme, InodeId,
    InodeRecord, InodeState, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap,
    MAX_VALUE_BYTES,
};
use wyrd_core::placement::Topology;
use wyrd_core::repair::{enqueue_repair, fragment_intact, queued_repairs};
use wyrd_core::write::encode_ec_fragment;
use wyrd_custodian::desired_state::{
    reconciliation_status, set_lifecycle, DServerLifecycle, ReconciliationStatus,
};
use wyrd_custodian::{
    reconcile_step, Custodian, FencedZone, RebalanceContext, Reconciled, ReconstructionContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result,
    WriteBatch,
};

// ---- in-memory trait doubles (the passes are proven over the seams, backend-agnostic) ----

/// A `BTreeMap`-backed metadata store, so `scan` answers in key order: "the object met FIRST
/// claims the shared chunk" (leg 4) is then a fixture property rather than luck.
#[derive(Default)]
struct MemMeta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
}

impl MemMeta {
    /// Every row the store holds, byte for byte — what a refusal, which writes nothing at
    /// all, must leave untouched.
    fn rows(&self) -> BTreeMap<Vec<u8>, Bytes> {
        self.kv.lock().unwrap().clone()
    }

    /// The rows under `prefix`, in key order.
    fn under(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Bytes)> {
        let kv = self.kv.lock().unwrap();
        kv.iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    async fn put(&self, key: Vec<u8>, value: impl Into<Bytes>) {
        let landed = self.commit(WriteBatch::new().put(key, value)).await;
        assert_eq!(
            landed.unwrap(),
            CommitOutcome::Committed,
            "fixture: seed put"
        );
    }
}

#[async_trait]
impl MetadataStore for MemMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        Ok(self.under(prefix))
    }

    /// The required paginated read (#634): a test double needs *a* body, not a backend's —
    /// the dev-only testkit helper pages over this store's own `scan`.
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

impl MemDServer {
    /// How many fragments this server holds — a rebuilt fragment written twice, or written
    /// somewhere the placement does not name, shows up here.
    fn count(&self) -> usize {
        self.frags.lock().unwrap().len()
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

// ---- fixture ----

const NOW: u64 = 10_000;
const CHUNK_LEN: u64 = 8;
/// RS(1,1): one data shard plus one parity shard — the smallest scheme carrying redundancy,
/// so the ONE surviving fragment (k = 1) is enough to rebuild the other.
const K: u8 = 1;
const M: u8 = 1;
/// The segment-group nonce every fixture object shares (32 lowercase hex characters,
/// `0016:354`). The group's **epoch is the inode id**, so two seeded objects never share a
/// `seg:` range.
const NONCE: &str = "0123456789abcdef0123456789abcdef";
/// The object every leg seeds first, and (leg 4) the one met first in key order.
const OBJECT: InodeId = 1;
/// Leg 4's second committed object, naming the SAME chunk.
const OTHER: InodeId = 2;
/// The chunk under maintenance in every leg.
const CHUNK: ChunkId = 0x5E_6A;
/// A twenty-digit D-server id: repointing a one-digit id onto it grows the record that names
/// it by nineteen bytes, which is what carries leg 3's segment record over the ceiling.
const HUGE: DServerId = u64::MAX;
/// Leg 3's filler chunk ids, all of the same decimal width so one filler costs a constant
/// number of bytes (the padding arithmetic below depends on it).
const FILLER_ID: ChunkId = 10_000_000;

/// The chunk under maintenance, as every leg seeds it: fragment 0 on `placement[0]`,
/// fragment 1 on `placement[1]`.
fn under_maintenance(placement: Vec<DServerId>) -> ChunkRef {
    ChunkRef {
        id: CHUNK,
        scheme: EcScheme::ReedSolomon { k: K, m: M },
        len: CHUNK_LEN,
        placement,
    }
}

/// Seed one committed **segmented** object at `inode`: raw `seg:` records plus a segmented
/// root, written by hand and never through a committer (this build ships no producer of
/// segmented maps — #653 owns one). One segment per entry of `segments`, holding its chunks
/// in order.
///
/// The seeding then **proves its own shape** through the production resolver, so no leg
/// passes because the object it was built around silently stopped being resolvable.
async fn seed_segmented(
    meta: &MemMeta,
    inode: InodeId,
    segments: &[Vec<ChunkRef>],
) -> SegmentGroup {
    let group = SegmentGroup::new(NONCE, inode).unwrap();
    let mut table = Vec::new();
    let mut byte_offset = 0u64;
    for (index, chunks) in segments.iter().enumerate() {
        let index = index as u32;
        let record = SegmentRecord::new(chunks.clone(), byte_offset).unwrap();
        table.push(SegmentRef {
            index,
            byte_offset,
            byte_len: record.byte_len(),
        });
        byte_offset += record.byte_len();
        meta.put(seg_key(&group, index).unwrap(), encode(&record))
            .await;
    }
    let map = SegmentedMap::new(group.clone(), table).unwrap();
    let root = InodeRecord {
        size: map.span(),
        chunk_map: ChunkMap::Segmented(map),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    meta.put(inode_key(inode), encode(&root)).await;
    let live = metadata::resolve_chunk_map(meta, &inode_key(inode), &root).await;
    assert!(
        live.is_ok_and(|resolved| resolved.is_some()),
        "fixture: the seeded segmented object must resolve through the shared resolver"
    );
    group
}

/// Write the REAL v1 fragment `index` of [`CHUNK`] onto `server` — a real shard through the
/// production encoder, so the loops' full identity + checksum verify passes on it.
async fn place(server: &MemDServer, index: u16) {
    let data = vec![b'w'; CHUNK_LEN as usize];
    let shards = erasure::encode(K.into(), M.into(), &data).expect("shards encode");
    let bytes = encode_ec_fragment(CHUNK, index, K, M, &shards[index as usize]);
    let frag = FragmentId {
        chunk: CHUNK,
        index,
    };
    server.put_fragment(frag, bytes).await.unwrap();
}

fn frag(index: u16) -> FragmentId {
    FragmentId {
        chunk: CHUNK,
        index,
    }
}

/// The placement the `seg:` record itself names for the chunk at `at` in segment `segment` —
/// read back out of the stored record, never out of the root.
async fn seg_placement(
    meta: &MemMeta,
    group: &SegmentGroup,
    segment: u32,
    at: usize,
) -> Vec<DServerId> {
    let key = seg_key(group, segment).unwrap();
    let bytes = meta
        .get(&key)
        .await
        .unwrap()
        .expect("the segment record is still there");
    let record: SegmentRecord = decode(&bytes).expect("the segment record still decodes");
    record.chunks()[at].placement.clone()
}

/// A topology over `servers`, each in its own failure domain (one letter per id).
fn topology(servers: &[DServerId]) -> Topology {
    let mut topology = Topology::default();
    for (letter, &server) in ["a", "b", "c", "d"].iter().zip(servers) {
        topology.register(server, *letter);
    }
    topology
}

/// Whether every server in `placement` sits in a **distinct** failure domain — the
/// durability invariant a repair or an evacuation must preserve (`0005:298`).
fn spread_over_distinct_domains(topology: &Topology, placement: &[DServerId]) -> bool {
    let domains: Vec<_> = placement
        .iter()
        .map(|server| topology.domain_of(*server).cloned())
        .collect();
    domains.iter().all(Option::is_some)
        && (0..domains.len()).all(|i| (i + 1..domains.len()).all(|j| domains[i] != domains[j]))
}

/// One fenced pass through the **real** control point, with exactly one loop wired.
async fn pass(
    repair: Option<&ReconstructionContext<'_>>,
    drain: Option<&RebalanceContext<'_>>,
) -> Reconciled {
    let coord = MemCoordination::new();
    let leader = Custodian::elect(&coord, "zone-repoint").await.unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    reconcile_step(&zone, &leader, None, None, repair, drain, NOW)
        .await
        .unwrap_or_else(|err| panic!("the pass must COMPLETE and answer: {err}"))
}

/// One fenced **reconstruction** pass over the whole repair queue.
async fn repair_pass(
    meta: &MemMeta,
    fleet: &[(DServerId, &dyn ChunkStore)],
    topology: &Topology,
) -> Reconciled {
    let ctx = ReconstructionContext {
        meta,
        fleet,
        topology,
        unreachable: &[],
    };
    pass(Some(&ctx), None).await
}

// ---- leg 1: a `seg:`-resident under-replicated chunk is REPAIRED ----

/// A committed multipart object has lost one of a chunk's fragments. On the base the repair
/// is refused and the obligation stays queued for ever: the object's redundancy decays with
/// nothing able to restore it. Here the rebuilt fragment lands on a healthy D server in a
/// distinct failure domain, the **`seg:` record** names it, and the obligation is discharged
/// by the same commit that moved the placement.
#[tokio::test]
async fn a_seg_resident_under_replicated_chunk_is_repaired() {
    let meta = MemMeta::default();
    let (d0, d2) = (MemDServer::default(), MemDServer::default());
    // Fragment 1's server (1) is in neither the fleet nor the topology: it is the loss.
    let group = seed_segmented(&meta, OBJECT, &[vec![under_maintenance(vec![0, 1])]]).await;
    place(&d0, 0).await;
    enqueue_repair(&meta, CHUNK, "scrub").await.unwrap();
    let root_before = meta.get(&inode_key(OBJECT)).await.unwrap().unwrap();
    let topology = topology(&[0, 2]);
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(0, &d0), (2, &d2)];

    let outcome = repair_pass(&meta, &fleet, &topology).await;

    assert_eq!(
        outcome,
        Reconciled::Changed,
        "the repair landed, so the pass says so"
    );
    let placement = seg_placement(&meta, &group, 0, 0).await;
    assert_eq!(
        placement,
        vec![0, 2],
        "the `seg:` record still names the lost fragment's old server — the placement was \
         never moved in the record that actually holds it"
    );
    assert!(
        spread_over_distinct_domains(&topology, &placement),
        "the rebuilt fragment did not land in a failure domain distinct from the survivor's"
    );
    // The rebuilt bytes are really there and are really this fragment: a placement pointing
    // at a server holding nothing is a repair that made the object WORSE.
    let rebuilt = d2
        .get_fragment(frag(1))
        .await
        .unwrap()
        .expect("the rebuilt fragment is on its new D server");
    assert!(
        fragment_intact(&rebuilt, frag(1), EcScheme::ReedSolomon { k: K, m: M }),
        "the rebuilt fragment does not verify its checksum and full identity"
    );
    assert!(
        !queued_repairs(&meta).await.unwrap().contains(&CHUNK),
        "the obligation is still queued: it is discharged by the SAME commit that moved the \
         placement, or the repair is repeated for ever"
    );
    assert_eq!(
        meta.get(&inode_key(OBJECT)).await.unwrap().unwrap(),
        root_before,
        "the root record's bytes changed: a chunk that lives in a `seg:` record is repointed \
         there, and the root is only the generation the move is pinned to"
    );
}

// ---- leg 2: a `seg:`-resident fragment is EVACUATED off a draining server ----

/// The operator marks a D server draining while it holds a fragment of a multipart object.
/// On the base the evacuation is refused, so the decommission never converges — the box can
/// never be pulled. Here the fragment is copied to a non-draining server in a distinct
/// domain, the `seg:` record names it, the vacated position is orphan-marked in the same
/// commit, and the operator's own drain query reports the server free.
#[tokio::test]
async fn a_seg_resident_fragment_is_evacuated_off_a_draining_server() {
    let meta = MemMeta::default();
    let (d0, d1, d2) = (
        MemDServer::default(),
        MemDServer::default(),
        MemDServer::default(),
    );
    let group = seed_segmented(&meta, OBJECT, &[vec![under_maintenance(vec![0, 1])]]).await;
    place(&d0, 0).await;
    place(&d1, 1).await;
    set_lifecycle(&meta, 0, DServerLifecycle::Draining)
        .await
        .unwrap();
    let root_before = meta.get(&inode_key(OBJECT)).await.unwrap().unwrap();
    let topology = topology(&[0, 1, 2]);
    let fleet: [(DServerId, &dyn ChunkStore); 3] = [(0, &d0), (1, &d1), (2, &d2)];
    let ctx = RebalanceContext {
        meta: &meta,
        fleet: &fleet,
        topology: &topology,
    };

    let outcome = pass(None, Some(&ctx)).await;

    assert_eq!(
        outcome,
        Reconciled::Changed,
        "the evacuation landed, so the pass says so"
    );
    let placement = seg_placement(&meta, &group, 0, 0).await;
    assert_eq!(
        placement,
        vec![2, 1],
        "the `seg:` record still names the draining server for fragment 0 — the drain cannot \
         converge while it does"
    );
    assert!(
        spread_over_distinct_domains(&topology, &placement),
        "the evacuated fragment collapsed the chunk's failure-domain spread"
    );
    assert!(
        d2.get_fragment(frag(0)).await.unwrap().is_some(),
        "the fragment's bytes are not at the new home its placement names"
    );
    // The vacated position carries its grace record, written by the SAME commit: without it
    // GC holds those bytes for ever with no evidence they are collectable (#364).
    assert_eq!(
        meta.under(b"orphan:")
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        vec![orphan_key(0, frag(0))],
        "exactly the vacated position is orphan-marked"
    );
    assert_eq!(
        meta.get(&inode_key(OBJECT)).await.unwrap().unwrap(),
        root_before,
        "the root record's bytes changed: the placement lives in the `seg:` record"
    );
    // The end the drain exists for, read from the operator's own surface.
    assert_eq!(
        reconciliation_status(&meta, 0).await.unwrap(),
        ReconciliationStatus::Satisfied,
        "the decommission still does not converge over a multipart object"
    );
}

// ---- leg 3: the ceiling refusal holds over a SEGMENTED record ----

/// A segment record holding `head` plus filler chunks, padded to **exactly** `len` encoded
/// bytes: whole fillers of a constant width for the coarse fit, then the decimal width of
/// one filler's id for the remainder (one digit is one byte). It asserts it landed, so a
/// fixture that stopped measuring what it measures fails loudly instead of passing on a
/// record that never approached the ceiling.
fn padded_segment(head: Vec<ChunkRef>, len: usize) -> SegmentRecord {
    let filler = |id: ChunkId| ChunkRef {
        id,
        scheme: EcScheme::None,
        len: 1,
        placement: vec![0],
    };
    let build = |chunks: &[ChunkRef]| SegmentRecord::new(chunks.to_vec(), 0).expect("a segment");
    let width = |chunks: &[ChunkRef]| encode(&build(chunks)).len();

    let base = width(&head);
    let with_one: Vec<ChunkRef> = head.iter().cloned().chain([filler(FILLER_ID)]).collect();
    let unit = width(&with_one) - base;
    // As many whole fillers as fit, from ONE measurement rather than by re-encoding a
    // hundred-kilobyte record per step; then backed off until it really is under `len`
    // (the record's own `byte_len` widens as the fillers accumulate).
    let mut count = (len - base) / unit;
    let fillers = |count: usize| -> Vec<ChunkRef> {
        head.iter()
            .cloned()
            .chain((0..count).map(|at| filler(FILLER_ID + at as ChunkId)))
            .collect()
    };
    while count > 0 && width(&fillers(count)) > len {
        count -= 1;
    }
    let mut chunks = fillers(count);
    // One decimal digit at a time, walking back over the fillers so a remainder wider than
    // one id can hold is still absorbed exactly.
    let mut at = chunks.len();
    while width(&chunks) < len {
        at -= 1;
        assert!(at >= head.len(), "fixture: not enough filler to pad with");
        while width(&chunks) < len {
            let Some(wider) = chunks[at].id.checked_mul(10) else {
                break;
            };
            chunks[at].id = wider;
        }
    }
    let record = build(&chunks);
    assert_eq!(
        encode(&record).len(),
        len,
        "fixture: the seeded segment record must land exactly on its target length"
    );
    record
}

/// A `seg:` record seeded just under the value ceiling every backend inherits, whose repoint
/// — the lost fragment rebuilt onto the only free domain, held by a twenty-digit D-server id
/// — would carry it over. The move is REFUSED and writes nothing at all: not the record, and
/// not the rebuilt fragment, which GC would otherwise hold for ever with no grace evidence
/// for it.
///
/// This leg is **not** independently red (pre-fix the move is refused for the other reason).
/// It ships because #710's flat-only fixture cannot pin the rule for a segment record, and
/// the arm that must never regress is the one that writes a record no later repair can
/// overwrite (`crates/core/src/metadata.rs:333-341`).
#[tokio::test]
async fn a_repoint_that_would_cross_the_ceiling_over_a_segment_record_is_refused() {
    let meta = MemMeta::default();
    let (d0, dh) = (MemDServer::default(), MemDServer::default());
    // Nineteen bytes of headroom short of what repointing `1` → `HUGE` costs.
    let record = padded_segment(vec![under_maintenance(vec![0, 1])], MAX_VALUE_BYTES - 18);
    let group = SegmentGroup::new(NONCE, OBJECT).unwrap();
    let table = vec![SegmentRef {
        index: 0,
        byte_offset: 0,
        byte_len: record.byte_len(),
    }];
    let map = SegmentedMap::new(group.clone(), table).unwrap();
    let root = InodeRecord {
        size: map.span(),
        chunk_map: ChunkMap::Segmented(map),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    meta.put(seg_key(&group, 0).unwrap(), encode(&record)).await;
    meta.put(inode_key(OBJECT), encode(&root)).await;
    place(&d0, 0).await;
    enqueue_repair(&meta, CHUNK, "scrub").await.unwrap();
    let before = meta.rows();
    let topology = topology(&[0, HUGE]);
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(0, &d0), (HUGE, &dh)];

    let outcome = repair_pass(&meta, &fleet, &topology).await;

    assert_eq!(
        meta.rows(),
        before,
        "a refusal writes NOTHING: the `seg:` record and the root are byte-identical"
    );
    assert!(
        queued_repairs(&meta).await.unwrap().contains(&CHUNK),
        "the obligation was discarded for a repair nothing performed — it is the last record \
         saying live data is under-replicated"
    );
    assert_eq!(
        dh.count(),
        0,
        "a refused repair stranded a rebuilt fragment: nothing at all may be written"
    );
    assert_eq!(
        outcome,
        Reconciled::Blocked,
        "a pass that refused a repair certified over it"
    );
}

// ---- leg 4: two committed references to one chunk get ONE plan ----

/// Two committed objects name the SAME chunk (a chunk shared by two multipart objects), with
/// one repair queued for it. The pass must act on **one** committed reference — the first in
/// key order, exactly the one the base's own scan chose — not on each independently: a
/// second, independent plan would rebuild and repoint over the first's answer and orphan
/// positions the other object still references.
#[tokio::test]
async fn two_committed_references_to_one_chunk_get_one_plan() {
    let meta = MemMeta::default();
    let (d0, d2) = (MemDServer::default(), MemDServer::default());
    let first = seed_segmented(&meta, OBJECT, &[vec![under_maintenance(vec![0, 1])]]).await;
    let second = seed_segmented(&meta, OTHER, &[vec![under_maintenance(vec![0, 1])]]).await;
    place(&d0, 0).await;
    enqueue_repair(&meta, CHUNK, "scrub").await.unwrap();
    let other_before = meta.rows();
    let other_seg = seg_key(&second, 0).unwrap();
    let topology = topology(&[0, 2]);
    let fleet: [(DServerId, &dyn ChunkStore); 2] = [(0, &d0), (2, &d2)];

    let outcome = repair_pass(&meta, &fleet, &topology).await;

    assert_eq!(outcome, Reconciled::Changed, "the repair landed");
    assert_eq!(
        seg_placement(&meta, &first, 0, 0).await,
        vec![0, 2],
        "the first committed reference in key order is the one repaired"
    );
    assert_eq!(
        meta.rows().get(&other_seg),
        other_before.get(&other_seg),
        "the SECOND object's record was rewritten too: one queued chunk gets one plan, \
         however many committed maps name it"
    );
    assert!(
        !queued_repairs(&meta).await.unwrap().contains(&CHUNK),
        "the obligation is still queued after a repair that landed"
    );
    // The rebuilt fragment was written ONCE, at the one position the one plan moved.
    assert_eq!(
        (d0.count(), d2.count()),
        (1, 1),
        "a fragment was rebuilt somewhere no committed placement names"
    );
    // Nothing either object still names AND still holds was marked collectable: an orphan
    // record is the front half of a GC deletion, so a mark over the shared survivor would
    // hand GC a live object's only copy.
    let marked: Vec<Vec<u8>> = meta
        .under(b"orphan:")
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert!(
        !marked.contains(&orphan_key(0, frag(0))),
        "the survivor both objects still name was marked collectable: {marked:?}"
    );
    assert_eq!(
        marked,
        vec![orphan_key(1, frag(1))],
        "exactly the position the move vacated — which holds no bytes — is marked"
    );
}
