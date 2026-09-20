//! Issue #664 (637.4) — what the custodian may **claim** about a server or a restored image once
//! multipart uploads exist (proposal 0016 decision 2's drain / rebalance / restore rows,
//! `docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`; the failure table
//! `:874-890`; decision 1.4 / D-B `:717-728`; X57 `:880`).
//!
//! One invariant, four surfaces: **no answer the custodian gives claims more than is true.**
//!
//! * a drain is `Satisfied` only when no byte that can still become referenced — committed,
//!   committed-part or in-flight — lives on that server (legs **A**, **B**, **C**, **D**);
//! * the post-restore report says what it skipped for a live upload, rather than skipping it
//!   silently (leg **E**);
//! * a restored image is declared fenced only when every session it resurrected can no longer
//!   publish **and** every record that session wrote has a named deleter (legs **F**, **G**,
//!   **H**); and
//! * "fenced" is a durable, per-generation fact a gateway can gate on, not a claim that outlives
//!   the pass that earned it (leg **I**).
//!
//! Every leg drives the production entry points — `reconciliation_status`, the rebalance loop
//! through `reconcile_step`, and `reconcile_after_restore` — over in-memory doubles. No client can
//! create a session until the S3 verbs land (#508), so every upload record is **seeded**, as raw
//! JSON the codecs accept (the shapes of `crates/core/tests/multipart_session_records.rs:81-149`).
//!
//! **This file names no symbol the fix adds.** The counters the fix puts on `RestoreReport` are
//! asserted through the report's `Debug` rendering and the restore-fence generation record through
//! its raw key, because naming either directly would stop the file compiling on the base — and a
//! leg that cannot compile on the base is a leg that never proved it was red there.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_chunk_format::FragmentHeader;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{
    self, dirent_key, inode_key, orphan_key, seg_key, ChunkRef, DirentRecord, EcScheme, InodeId,
    InodeRecord, InodeState, SegmentGroup, SegmentRecord,
};
use wyrd_core::multipart::{
    complete_answer, decode_owned_entry, decode_part_record, decode_retire_obligation,
    decode_session_record, mpu_key, part_key, retire_key, sidx_key, CompleteAnswer, OwnedEntry,
    PartNumber, PartScope, RetireMode, RetireToken, SessionState, StagedPlacement, UploadId,
};
use wyrd_core::placement::Topology;
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, reconciliation_status, set_lifecycle, Custodian,
    DServerLifecycle, ExpiredPendingPolicy, FencedZone, GcContext, RebalanceContext, Reconciled,
    ReconciliationStatus, RestoreReport,
};
use wyrd_traits::{
    page_cursor, page_limit, page_start, BoxError, ChunkId, ChunkStore, CommitOutcome, DServerId,
    FragmentId, Health, MetadataStore, PageStart, Result, ScanCapExceeded, ScanPage, WriteBatch,
    SCAN_CAP,
};

/// The reader-safe grace window every pass here runs with.
const GRACE: u64 = 50;
/// The instant every pass runs at.
const NOW: u64 = 10_000;
/// An owned entry's lease: far past every pass's clock, so no leg turns on it.
const LEASE: u64 = NOW * 1_000;
/// The bucket and object every seeded session targets.
const PARENT: InodeId = 42;
const OBJECT: &str = "staged/object";
/// Every seeded session's epoch, and therefore the epoch its fence's obligations are keyed by
/// (`0016:663-665`: the fence that ENDS attempt `E` installs under `s:<id>:<E>`).
const EPOCH: u64 = 3;
/// Every seeded `Completing` session's segment-group nonce — the half of `seg:<nonce>:<epoch>:`
/// that only the session record can supply (the design call this slice settles).
const NONCE: &str = "0123456789abcdef0123456789abcdef";
/// The D server every drain leg marks.
const DRAINING: DServerId = 3;
/// What the metadata double answers a commit it was armed to fail with.
const INJECTED_COMMIT_FAULT: &str = "injected metadata-store commit fault";

// ---- the metadata double ------------------------------------------------------------------------

/// A batch the double refuses, and the store snapshot it took as that batch arrived.
struct CommitProbe {
    /// The key prefix a batch must name — in a put, a delete or a precondition — to be probed.
    touching: Vec<u8>,
    /// Refuse the probed batch entirely, so nothing of it is applied.
    refuse: bool,
    /// The value of [`Self::watch`] the first time a probed batch arrived, before it applied.
    seen: Option<Option<Bytes>>,
    /// The key to snapshot when that happens.
    watch: Vec<u8>,
    fired: bool,
}

/// An in-memory `MetadataStore` over an ordered map.
///
/// `scan` refuses a result past the cap and `scan_page` clamps a page to it through the seam's own
/// `page_limit` / `page_start` / `page_cursor` — the shape of every backend's `with_scan_cap` knob.
/// It also carries one probe, which is how the atomicity legs (F, G) and the mid-pass generation
/// read (I) observe a commit **as it happens** rather than inferring it afterwards.
struct Meta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    reads: Mutex<Vec<Vec<u8>>>,
    probe: Mutex<Option<CommitProbe>>,
}

impl Meta {
    fn new() -> Self {
        Self {
            kv: Mutex::new(BTreeMap::new()),
            reads: Mutex::new(Vec::new()),
            probe: Mutex::new(None),
        }
    }

    /// Put a fixture record in place — not a read, and not a pass's write.
    fn seed(&self, key: impl Into<Vec<u8>>, value: impl Into<Bytes>) {
        self.kv.lock().unwrap().insert(key.into(), value.into());
    }

    fn get_now(&self, key: &[u8]) -> Option<Bytes> {
        self.kv.lock().unwrap().get(key).cloned()
    }

    fn holds(&self, key: &[u8]) -> bool {
        self.kv.lock().unwrap().contains_key(key)
    }

    /// Snapshot `watch` the first time a batch naming a key under `touching` arrives, and — when
    /// `refuse` — fail that batch so **nothing** of it applies.
    fn probe(&self, touching: &[u8], watch: &[u8], refuse: bool) {
        *self.probe.lock().unwrap() = Some(CommitProbe {
            touching: touching.to_vec(),
            refuse,
            seen: None,
            watch: watch.to_vec(),
            fired: false,
        });
    }

    /// What the probe saw under its watched key, if it fired.
    fn probed(&self) -> Option<Option<Bytes>> {
        self.probe
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|p| p.seen.clone())
    }

    fn reads(&self) -> Vec<Vec<u8>> {
        self.reads.lock().unwrap().clone()
    }

    fn log(&self, subject: &[u8]) {
        self.reads.lock().unwrap().push(subject.to_vec());
    }

    /// Whether any read logged so far named exactly `subject`.
    fn read_of(&self, subject: &[u8]) -> bool {
        self.reads().iter().any(|read| read == subject)
    }

    /// Apply `batch` atomically: every precondition holds, or nothing changes.
    fn apply(&self, batch: WriteBatch) -> CommitOutcome {
        let mut kv = self.kv.lock().unwrap();
        for pre in &batch.preconditions {
            if kv.get(&pre.key) != pre.expected.as_ref() {
                return CommitOutcome::Conflict;
            }
        }
        for key in batch.deletes {
            kv.remove(&key);
        }
        for (key, value) in batch.puts {
            kv.insert(key, value);
        }
        CommitOutcome::Committed
    }
}

#[async_trait]
impl MetadataStore for Meta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.log(key);
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        self.log(prefix);
        let hits: Vec<(Vec<u8>, Bytes)> = self
            .kv
            .lock()
            .unwrap()
            .range::<[u8], _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if hits.len() > SCAN_CAP {
            return Err(BoxError::from(ScanCapExceeded {
                cap: SCAN_CAP,
                prefix: prefix.to_vec(),
            }));
        }
        Ok(hits)
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<ScanPage> {
        self.log(prefix);
        let limit = page_limit(limit, SCAN_CAP, prefix)?;
        let lower = match page_start(prefix, after) {
            PageStart::After(cursor) => Bound::Excluded(cursor),
            PageStart::Prefix => Bound::Included(prefix),
            PageStart::PastPrefix => return Ok((Vec::new(), None)),
        };
        let items: Vec<(Vec<u8>, Bytes)> = self
            .kv
            .lock()
            .unwrap()
            .range::<[u8], _>((lower, Bound::Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .take(limit)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let next = page_cursor(&items, limit);
        Ok((items, next))
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let refuse = {
            let mut probe = self.probe.lock().unwrap();
            match probe.as_mut() {
                Some(probe) if !probe.fired && batch_touches(&batch, &probe.touching) => {
                    probe.fired = true;
                    probe.seen = Some(self.kv.lock().unwrap().get(&probe.watch).cloned());
                    probe.refuse
                }
                _ => false,
            }
        };
        if refuse {
            return Err(BoxError::from(INJECTED_COMMIT_FAULT));
        }
        Ok(self.apply(batch))
    }
}

/// Whether `batch` names any key under `prefix` — in a put, a delete or a precondition. A
/// precondition counts because a compare-and-set on a record is exactly how a fence "touches" it.
fn batch_touches(batch: &WriteBatch, prefix: &[u8]) -> bool {
    batch
        .puts
        .iter()
        .map(|(key, _)| key)
        .chain(batch.deletes.iter())
        .chain(batch.preconditions.iter().map(|pre| &pre.key))
        .any(|key| key.starts_with(prefix))
}

// ---- the D-server double and the fleet ----------------------------------------------------------

#[derive(Default)]
struct Disk {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
}

#[async_trait]
impl ChunkStore for Disk {
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

fn disks() -> [Disk; 4] {
    Default::default()
}

fn fleet(d: &[Disk; 4]) -> [(DServerId, &dyn ChunkStore); 4] {
    [(0, &d[0]), (1, &d[1]), (2, &d[2]), (3, &d[3])]
}

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

/// Put a real v1 single-copy fragment of `frag`'s chunk on `dserver`.
fn place(d: &[Disk; 4], dserver: DServerId, frag: FragmentId) {
    let payload = b"staged";
    let bytes = wyrd_chunk_format::encode(
        &FragmentHeader::new_v1(frag.chunk, payload.len() as u64),
        payload,
    );
    d[dserver as usize]
        .frags
        .lock()
        .unwrap()
        .insert(frag, Bytes::from(bytes));
}

/// Every fragment on every disk, so a leg can assert a pass moved none of them.
fn fleet_contents(d: &[Disk; 4]) -> Vec<(DServerId, Vec<FragmentId>)> {
    d.iter()
        .enumerate()
        .map(|(dserver, disk)| {
            let mut frags: Vec<FragmentId> = disk.frags.lock().unwrap().keys().copied().collect();
            frags.sort_unstable_by_key(|f| (f.chunk, f.index));
            (dserver as DServerId, frags)
        })
        .collect()
}

// ---- the passes ---------------------------------------------------------------------------------

async fn elect() -> (FencedZone, Custodian) {
    let coord = MemCoordination::new();
    let custodian = Custodian::elect(&coord, "zone-staged-drain-restore")
        .await
        .expect("leader election over the in-memory coordination seam");
    let mut zone = FencedZone::new();
    zone.install(custodian.leadership());
    (zone, custodian)
}

/// One post-restore pass at [`NOW`].
async fn restore_pass(meta: &Meta, d: &[Disk; 4]) -> Result<RestoreReport> {
    let fleet = fleet(d);
    let ctx = GcContext {
        meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_after_restore(&ctx, NOW).await
}

/// One rebalance pass through the fenced control point.
async fn rebalance_pass(meta: &Meta, d: &[Disk; 4], topology: &Topology) -> Reconciled {
    let (zone, custodian) = elect().await;
    let fleet = fleet(d);
    let ctx = RebalanceContext {
        meta,
        fleet: &fleet,
        topology,
    };
    reconcile_step(&zone, &custodian, None, None, None, Some(&ctx), NOW)
        .await
        .expect("the rebalance pass runs")
}

fn four_domains() -> Topology {
    let mut t = Topology::default();
    t.register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(3, "D");
    t
}

// ---- the records --------------------------------------------------------------------------------

/// An upload id: 32 lowercase-hex characters from a 2-character **hex** pair, so every leg has its
/// own and no two share a record name on the audit seam.
fn upload(pair: &str) -> UploadId {
    UploadId::new(pair.repeat(16)).expect("32 lowercase-hex characters")
}

fn part_no(n: u32) -> PartNumber {
    PartNumber::new(n).expect("a part number in range")
}

/// An `Open` session record, spelled as the codec's own encoding and round-tripped through it.
fn open_session() -> Bytes {
    let bytes = session_bytes("{\"kind\":\"Open\"}");
    decode_session_record(&bytes).expect("the seeded Open session must decode");
    Bytes::from(bytes)
}

/// A `Completing` session record at [`EPOCH`], carrying the segment-group `nonce` this slice adds.
///
/// **Deliberately not round-tripped through `decode_session_record` here.** On the base that
/// decode refuses the value (`#[serde(deny_unknown_fields)]` on the wire shape), and a fixture
/// that panics at seed time would make a leg red for the wrong reason — the legs below have to
/// earn their red from the pass's own answer. It is harmless on the base for the same reason the
/// brief gives: the base's post-restore pass never reads an `mpu:` VALUE at all.
fn completing_session(nonce: Option<&str>, segments_written: u32) -> Bytes {
    let nonce = match nonce {
        Some(nonce) => format!(",\"nonce\":\"{nonce}\""),
        None => String::new(),
    };
    Bytes::from(session_bytes(&format!(
        "{{\"kind\":\"Completing\",\"fenced_at_millis\":900,\
         \"segments_written\":{segments_written},\"publish_target\":{{\"parent\":{PARENT},\
         \"name\":\"{OBJECT}\",\"epoch\":{EPOCH}{nonce}}}}}"
    )))
}

fn session_bytes(state_json: &str) -> Vec<u8> {
    format!(
        "{{\"parent\":{PARENT},\"object\":\"{OBJECT}\",\"created_at_millis\":100,\
         \"clock_source\":\"wall\",\"epoch\":{EPOCH},\"attempts\":1,\"state\":{state_json}}}"
    )
    .into_bytes()
}

fn chunk_ref(id: ChunkId, placement: &[DServerId]) -> ChunkRef {
    ChunkRef {
        id,
        scheme: EcScheme::None,
        len: 5,
        placement: placement.to_vec(),
    }
}

/// A committed part record naming `chunks`, round-tripped through the codec.
fn part(chunks: &[ChunkRef]) -> Bytes {
    let refs: Vec<String> = chunks
        .iter()
        .map(|chunk| String::from_utf8(metadata::encode(chunk).to_vec()).unwrap())
        .collect();
    let len: u64 = chunks.iter().map(|chunk| chunk.len).sum();
    let bytes = format!(
        "{{\"chunks\":[{}],\"len\":{len},\"digest\":\"{}\",\"committed_at_millis\":800,\
         \"session_epoch\":{EPOCH}}}",
        refs.join(","),
        "ef".repeat(32)
    )
    .into_bytes();
    decode_part_record(&bytes).expect("the seeded part record must decode");
    Bytes::from(bytes)
}

/// An owned staging entry of `owner`, planned onto `placement`, round-tripped through the codec.
fn owned(owner: &UploadId, key: &[u8], placement: &[DServerId]) -> Bytes {
    let staged =
        StagedPlacement::new(EcScheme::None, placement.to_vec()).expect("a supported scheme");
    let value = metadata::encode(&OwnedEntry::new(owner.clone(), LEASE, staged).to_pending());
    decode_owned_entry(key, &value).expect("the seeded owned entry must decode");
    value
}

/// A committed object under `name`, its single-fragment chunk placed on `dserver`.
async fn commit_object(
    meta: &Meta,
    d: &[Disk; 4],
    name: &str,
    id: InodeId,
    chunk: ChunkId,
    dserver: DServerId,
) {
    let record = InodeRecord {
        size: 5,
        chunk_map: vec![chunk_ref(chunk, &[dserver])].into(),
        state: InodeState::Committed,
        version: 1,
        ..Default::default()
    };
    meta.seed(inode_key(id), metadata::encode(&record));
    meta.seed(
        dirent_key(PARENT, name),
        metadata::encode(&DirentRecord { inode: id }),
    );
    place(d, dserver, frag(chunk, 0));
}

/// The restore-fence generation record's key, spelled by hand: naming the production constant
/// would not compile on the base (see this file's header).
const FENCE_KEY: &[u8] = b"mpufence";

/// What the generation record says, as text — asserted by substring for the same reason.
fn fence_record(meta: &Meta) -> String {
    String::from_utf8(
        meta.get_now(FENCE_KEY)
            .expect("the restore-fence generation record is present")
            .to_vec(),
    )
    .expect("the record is JSON text")
}

// =================================================================================================
// (A)(B)(C) — the drain's answer over the staged class
// =================================================================================================

/// **(A)** A server holding **only** an in-flight part's fragment — an owned `sidx:` entry's
/// planned placement, with no `part:` record for it yet — is still `Pending`.
///
/// This is `0016:827`'s sharper form: the upload has staged bytes and has not committed the part,
/// so no record outside the `sidx:` entry names them. On the base `reconciliation_status` reads
/// committed placements alone and answers `Satisfied` — telling an operator a box holding a live
/// upload's only copy may be wiped.
#[tokio::test]
async fn a_drain_counts_an_in_flight_part_as_held() {
    let meta = Meta::new();
    let d = disks();
    let id = upload("a1");
    let chunk: ChunkId = 0x0A01;
    meta.seed(mpu_key(&id), open_session());
    let key = sidx_key(&id, part_no(1), chunk);
    meta.seed(key.clone(), owned(&id, &key, &[DRAINING]));
    place(&d, DRAINING, frag(chunk, 0));
    set_lifecycle(&meta, DRAINING, DServerLifecycle::Draining)
        .await
        .unwrap();

    assert_eq!(
        reconciliation_status(&meta, DRAINING).await.unwrap(),
        ReconciliationStatus::Pending,
        "a server holding an in-flight part's staged fragment is not drained: the upload can \
         still commit that part and then publish it"
    );
}

/// **(B)** A server holding **only** a committed part's fragment — a `part:` record naming it, no
/// committed chunk map anywhere — is `Pending`.
///
/// Its own leg, not a variation of (A): an implementation that counted one staged class and not
/// the other would pass exactly one of these two (`0016:883`).
#[tokio::test]
async fn b_drain_counts_a_committed_part_as_held() {
    let meta = Meta::new();
    let d = disks();
    let id = upload("b1");
    let chunk: ChunkId = 0x0B01;
    meta.seed(mpu_key(&id), open_session());
    meta.seed(
        part_key(&id, part_no(1)),
        part(&[chunk_ref(chunk, &[DRAINING])]),
    );
    place(&d, DRAINING, frag(chunk, 0));
    set_lifecycle(&meta, DRAINING, DServerLifecycle::Draining)
        .await
        .unwrap();

    assert_eq!(
        reconciliation_status(&meta, DRAINING).await.unwrap(),
        ReconciliationStatus::Pending,
        "a server holding a committed part's fragment is not drained: one Complete publishes it"
    );
}

/// **(C)** A drain still finishes. The uploads' staged fragments sit on servers 0–2 and the
/// draining server holds none of them and no committed reference, so the answer is `Satisfied`.
///
/// A guard, green on the base as well: it is what kills the mutant that answers `Pending` for
/// **any** staged fragment anywhere — which would wedge every drain in a cluster with one live
/// upload on it. (#637 v1's `*server != dserver` mutant survived every other leg.)
#[tokio::test]
async fn c_drain_finishes_when_the_uploads_live_elsewhere() {
    let meta = Meta::new();
    let d = disks();
    let id = upload("c1");
    meta.seed(mpu_key(&id), open_session());
    for (index, dserver) in [0, 1, 2].into_iter().enumerate() {
        let chunk: ChunkId = 0x0C01 + index as ChunkId;
        meta.seed(
            part_key(&id, part_no(index as u32 + 1)),
            part(&[chunk_ref(chunk, &[dserver])]),
        );
        place(&d, dserver, frag(chunk, 0));
        let owned_chunk: ChunkId = 0x0C11 + index as ChunkId;
        let key = sidx_key(&id, part_no(index as u32 + 4), owned_chunk);
        meta.seed(key.clone(), owned(&id, &key, &[dserver]));
        place(&d, dserver, frag(owned_chunk, 0));
    }
    set_lifecycle(&meta, DRAINING, DServerLifecycle::Draining)
        .await
        .unwrap();

    assert_eq!(
        reconciliation_status(&meta, DRAINING).await.unwrap(),
        ReconciliationStatus::Satisfied,
        "the draining server holds nothing — neither committed nor staged — so the drain IS \
         satisfied; blocking it on staged bytes that live elsewhere would wedge every drain in a \
         cluster with one live upload"
    );
}

// =================================================================================================
// (D) — rebalance and drain agree, and rebalance leaves staged bytes alone
// =================================================================================================

/// **(D)** For a draining server holding **only** staged fragments, a rebalance pass writes no
/// fragment anywhere and rewrites no `part:` record (`0016:881`) — and the drain is `Pending`.
///
/// The two halves are one claim. Rebalance's own answer is about its pass, not about a server:
/// with nothing committed on the draining box it plans no evacuation and reports `Satisfied`,
/// which is true of the pass and says nothing about whether the box may be pulled. The surface
/// that answers *that* is `reconciliation_status`, and it must say `Pending` — otherwise the two
/// together tell an operator the drain is done over a live upload's bytes.
#[tokio::test]
async fn d_rebalance_leaves_staged_bytes_alone_and_the_drain_stays_pending() {
    let meta = Meta::new();
    let d = disks();
    let topology = four_domains();
    // A committed object, well away from the draining server, so the rebalance scan has a real
    // record to read and a plan it could have made.
    commit_object(&meta, &d, "committed", 1, 0x0D01, 0).await;

    let id = upload("d1");
    meta.seed(mpu_key(&id), open_session());
    let part_chunk: ChunkId = 0x0D11;
    let part_record = part(&[chunk_ref(part_chunk, &[DRAINING])]);
    meta.seed(part_key(&id, part_no(1)), part_record.clone());
    place(&d, DRAINING, frag(part_chunk, 0));
    let owned_chunk: ChunkId = 0x0D12;
    let key = sidx_key(&id, part_no(2), owned_chunk);
    meta.seed(key.clone(), owned(&id, &key, &[DRAINING]));
    place(&d, DRAINING, frag(owned_chunk, 0));
    set_lifecycle(&meta, DRAINING, DServerLifecycle::Draining)
        .await
        .unwrap();

    let before = fleet_contents(&d);
    let inode_before = meta.get_now(&inode_key(1));
    let outcome = rebalance_pass(&meta, &d, &topology).await;

    assert_eq!(
        fleet_contents(&d),
        before,
        "the rebalance pass moved a fragment: staged bytes are disjoint from the committed \
         namespace it evacuates (`0016:881`)"
    );
    assert_eq!(
        meta.get_now(&part_key(&id, part_no(1))),
        Some(part_record),
        "the rebalance pass rewrote a `part:` record"
    );
    assert_eq!(
        meta.get_now(&inode_key(1)),
        inode_before,
        "the rebalance pass repointed a placement it had no reason to"
    );
    assert_eq!(
        outcome,
        Reconciled::Satisfied,
        "with nothing committed on the draining server the pass plans no evacuation — that is a \
         statement about the PASS"
    );
    assert_eq!(
        reconciliation_status(&meta, DRAINING).await.unwrap(),
        ReconciliationStatus::Pending,
        "...and the per-server query is the one that answers whether the box may be pulled: it \
         still holds a live upload's staged bytes"
    );
}

// =================================================================================================
// (E) — restore reports what it skipped for a live upload
// =================================================================================================

/// **(E)** The post-restore pass reports the fragments it left alone for a live upload
/// **separately** from the ones it left to a pending lease (`0016:823`).
///
/// Asserted through the report's `Debug` rendering, because naming the counter directly would stop
/// this file compiling on the base — where the rendering carries no such counter at all.
#[tokio::test]
async fn e_restore_reports_staged_skips_separately() {
    let meta = Meta::new();
    let d = disks();
    let id = upload("e1");
    meta.seed(mpu_key(&id), open_session());
    let part_chunk: ChunkId = 0x0E01;
    meta.seed(
        part_key(&id, part_no(1)),
        part(&[chunk_ref(part_chunk, &[0])]),
    );
    place(&d, 0, frag(part_chunk, 0));
    let owned_chunk: ChunkId = 0x0E02;
    let key = sidx_key(&id, part_no(2), owned_chunk);
    meta.seed(key.clone(), owned(&id, &key, &[1]));
    place(&d, 1, frag(owned_chunk, 0));
    // The control: a fragment nothing names, which the same pass DOES mark. Without it a pass
    // that marked nothing at all would satisfy the staged claim vacuously.
    let stray: ChunkId = 0x0E03;
    place(&d, 2, frag(stray, 0));

    let report = restore_pass(&meta, &d).await.expect("the pass runs");
    let printed = format!("{report:?}");
    assert!(
        printed.contains("staged_skipped: 2"),
        "the report does not say how many fragments it kept for a live upload: {printed}"
    );
    assert_eq!(
        report.pending_skipped, 0,
        "staged bytes are not a pending lease's: they have a different owner and a different \
         retirement path ({printed})"
    );
    assert_eq!(report.stranded_marked, 1, "{printed}");
    assert!(
        meta.holds(&orphan_key(2, frag(stray, 0))),
        "the unreferenced control was not marked, so leaving the staged fragments unmarked \
         proves nothing: {printed}"
    );
    for (dserver, chunk) in [(0, part_chunk), (1, owned_chunk)] {
        assert!(
            !meta.holds(&orphan_key(dserver, frag(chunk, 0))),
            "the pass marked a staged fragment: {printed}"
        );
    }
}

// =================================================================================================
// (F)(G)(H) — the restore fence
// =================================================================================================

/// Seed one `Open` session with a committed part, and return its id and `mpu:` key.
fn seed_open_session(
    meta: &Meta,
    d: &[Disk; 4],
    pair: &str,
    chunk: ChunkId,
) -> (UploadId, Vec<u8>) {
    let id = upload(pair);
    meta.seed(mpu_key(&id), open_session());
    meta.seed(part_key(&id, part_no(1)), part(&[chunk_ref(chunk, &[0])]));
    place(d, 0, frag(chunk, 0));
    let key = mpu_key(&id);
    (id, key)
}

/// The session-scoped retirement token a fence of epoch `E` installs under (`0016:663-665`).
fn session_token(id: &UploadId) -> RetireToken {
    RetireToken::Session {
        upload_id: id.clone(),
        epoch: EPOCH,
        part: None,
    }
}

/// **(F)** A resurrected `Open@E` session ends as `Aborting@E+1`, with its byte-retirement
/// obligation installed **in the same batch** (D-B, `0016:717-728`).
///
/// Three things in one leg, because they are one guarantee:
///
/// * the session is fenced, and the counter says so;
/// * the obligation is real — it decodes through `decode_retire_obligation` against the key it
///   sits under, so the retirement drain (#659) can act on it; and
/// * a Complete retried against the fenced session **cannot fence it**, because that fence
///   requires `Open@E` (`0016:660`). The client-visible status is #658's.
#[tokio::test]
async fn f_restore_fences_a_resurrected_open_session() {
    let meta = Meta::new();
    let d = disks();
    let (id, key) = seed_open_session(&meta, &d, "f1", 0x0F01);
    let before = meta.get_now(&key).expect("the seeded session");

    // The positive control, first: on the record as the restore left it, a Complete WOULD fence.
    let resurrected = decode_session_record(&before).expect("the seeded Open session decodes");
    assert!(
        matches!(
            complete_answer(Some(resurrected.state()), None),
            CompleteAnswer::Fences
        ),
        "the resurrected session must be one a Complete could fence, or fencing it proves nothing"
    );

    let report = restore_pass(&meta, &d).await.expect("the pass runs");
    let printed = format!("{report:?}");
    assert!(
        printed.contains("sessions_fenced: 1"),
        "the report does not say it fenced the resurrected session: {printed}"
    );

    let fenced = decode_session_record(&meta.get_now(&key).expect("the session record"))
        .expect("the fenced session record decodes");
    assert_eq!(fenced.state(), &SessionState::Aborting {});
    assert_eq!(
        fenced.epoch(),
        EPOCH + 1,
        "a fence bumps the epoch, so the epoch it fenced can never be fenced twice"
    );
    assert!(
        !matches!(
            complete_answer(Some(fenced.state()), None),
            CompleteAnswer::Fences
        ),
        "a Complete can still fence the session the restore resurrected"
    );

    // The obligation, decoded against its own key — the only thing that makes it a deleter.
    let token = session_token(&id);
    let obligation_key = retire_key(RetireMode::Bytes, &token);
    let value = meta
        .get_now(&obligation_key)
        .expect("the fence installs the session's byte-retirement obligation");
    let (mode, decoded, payload) = decode_retire_obligation(&obligation_key, &value)
        .expect("the installed obligation decodes against the key it sits under");
    assert_eq!((mode, decoded), (RetireMode::Bytes, token));
    assert!(
        payload.session(),
        "the teardown must owe the session's own staged residue: {payload:?}"
    );
    assert!(
        matches!(payload.parts(), Some(PartScope::All)),
        "the `Open` teardown enumerates the session's part range at drain time, because this \
         fence is what freezes it (`0016:2187`): {payload:?}"
    );
}

/// **(F, atomicity)** The fence and its obligation are **one** batch: a double that fails the
/// commit naming the session record leaves **none** of the writes present.
#[tokio::test]
async fn f_the_fence_and_its_obligation_land_in_one_batch() {
    let meta = Meta::new();
    let d = disks();
    let (id, key) = seed_open_session(&meta, &d, "f2", 0x0F02);
    let before = meta.get_now(&key).expect("the seeded session");
    meta.probe(b"mpu:", FENCE_KEY, true);

    let fault = restore_pass(&meta, &d)
        .await
        .expect_err("the refused commit fails the pass");
    assert!(
        fault.to_string().contains(INJECTED_COMMIT_FAULT),
        "the pass must fail on the store's own error, not replace it: {fault}"
    );
    assert_eq!(
        meta.get_now(&key),
        Some(before),
        "the session record changed although its batch was refused"
    );
    assert!(
        !meta.holds(&retire_key(RetireMode::Bytes, &session_token(&id))),
        "the retirement obligation is durable although the fence that installs it was refused — \
         it is not in the same batch, so a failure can leave an obligation naming a range a live \
         session can still add to"
    );
}

/// Seed a `Completing@E` session: `part_chunks` in its `part:` records (one part each, numbered
/// from 1) and `seg_chunks` in one `seg:<nonce>:<E>:000000` record.
fn seed_completing_session(
    meta: &Meta,
    d: &[Disk; 4],
    pair: &str,
    nonce: Option<&str>,
    part_chunks: &[ChunkId],
    seg_chunks: &[ChunkId],
) -> (UploadId, Vec<u8>) {
    let id = upload(pair);
    meta.seed(
        mpu_key(&id),
        completing_session(nonce, seg_chunks.len() as u32),
    );
    for (index, &chunk) in part_chunks.iter().enumerate() {
        meta.seed(
            part_key(&id, part_no(index as u32 + 1)),
            part(&[chunk_ref(chunk, &[0])]),
        );
        place(d, 0, frag(chunk, 0));
    }
    if !seg_chunks.is_empty() {
        let group = SegmentGroup::new(nonce.expect("a segment group needs a nonce"), EPOCH)
            .expect("a 32-hex nonce");
        let refs: Vec<ChunkRef> = seg_chunks.iter().map(|&c| chunk_ref(c, &[0])).collect();
        let record = SegmentRecord::new(refs, 0).expect("a non-empty segment record");
        meta.seed(
            seg_key(&group, 0).expect("segment index 0 is addressable"),
            metadata::encode(&record),
        );
    }
    let key = mpu_key(&id);
    (id, key)
}

/// **(G)** A resurrected `Completing@E` session that had already written segments ends as
/// `Aborting@E+1`, and **one batch** installs both obligations it owes (X57, `0016:665`, `:880`):
///
/// * `retire:bytes:s:<id>:<E>` naming the session and its parts; and
/// * `retire:records:s:<id>:<E>` naming **exactly** that attempt's segment group.
///
/// The second is the one #637 v1 could not install, because nothing on the session record named
/// the group — so its `seg:` records had no deleter anywhere in the design and were reported as
/// residue instead. That draining the range actually empties it is #659's to prove.
#[tokio::test]
async fn g_restore_fences_a_completing_session_with_its_segments_deleter() {
    let meta = Meta::new();
    let d = disks();
    let chunks: [ChunkId; 2] = [0x0A01, 0x0A02];
    let (id, key) = seed_completing_session(&meta, &d, "1a", Some(NONCE), &chunks, &chunks);

    let report = restore_pass(&meta, &d).await.expect("the pass runs");
    let printed = format!("{report:?}");
    assert!(printed.contains("sessions_fenced: 1"), "{printed}");
    assert!(
        !report.needs_human(),
        "a `Completing` session whose parts cover its segments is fenced CLEANLY: {printed}"
    );

    let fenced = decode_session_record(&meta.get_now(&key).expect("the session record"))
        .expect("the fenced session record decodes");
    assert_eq!(fenced.state(), &SessionState::Aborting {});
    assert_eq!(fenced.epoch(), EPOCH + 1);

    let token = session_token(&id);
    let bytes_key = retire_key(RetireMode::Bytes, &token);
    let (mode, decoded, payload) = decode_retire_obligation(
        &bytes_key,
        &meta
            .get_now(&bytes_key)
            .expect("the byte-retirement obligation"),
    )
    .expect("it decodes against the key it sits under");
    assert_eq!((mode, decoded), (RetireMode::Bytes, token.clone()));
    assert!(payload.session(), "{payload:?}");
    assert!(
        matches!(payload.parts(), Some(PartScope::Set(set)) if set.runs() == [(1, 2)]),
        "the `Completing` teardown names its parts EXPLICITLY — the wildcard is not legal for a \
         published-part set (`0016:919-921`): {payload:?}"
    );

    let records_key = retire_key(RetireMode::Records, &token);
    let (mode, decoded, payload) = decode_retire_obligation(
        &records_key,
        &meta
            .get_now(&records_key)
            .expect("the segment records have a named deleter"),
    )
    .expect("it decodes against the key it sits under");
    assert_eq!((mode, decoded), (RetireMode::Records, token));
    assert_eq!(
        payload.segments(),
        Some(&SegmentGroup::new(NONCE, EPOCH).expect("a 32-hex nonce")),
        "the records obligation must name EXACTLY this attempt's `seg:<nonce>:<E>` group — a \
         group with any other epoch would give one obligation several legal keys: {payload:?}"
    );
}

/// **(G, atomicity)** Both obligations and the fence are one batch: the refused commit leaves
/// none of the three.
#[tokio::test]
async fn g_both_obligations_and_the_fence_land_in_one_batch() {
    let meta = Meta::new();
    let d = disks();
    let chunks: [ChunkId; 1] = [0x0A03];
    let (id, key) = seed_completing_session(&meta, &d, "1b", Some(NONCE), &chunks, &chunks);
    let before = meta.get_now(&key).expect("the seeded session");
    meta.probe(b"mpu:", FENCE_KEY, true);

    restore_pass(&meta, &d)
        .await
        .expect_err("the refused commit fails the pass");
    assert_eq!(
        meta.get_now(&key),
        Some(before),
        "the session record changed"
    );
    let token = session_token(&id);
    for (label, mode) in [
        ("bytes", RetireMode::Bytes),
        ("records", RetireMode::Records),
    ] {
        assert!(
            !meta.holds(&retire_key(mode, &token)),
            "the {label} obligation is durable although the fence that installs it was refused"
        );
    }
}

/// **(H)(i)** A `Completing` record with **no** nonce — the pre-decision shape — fails decode.
/// The pass leaves it byte-identical (ADR-0045), names it as needing a human, and does **not**
/// mark the fence generation complete.
#[tokio::test]
async fn h_a_completing_session_without_a_nonce_is_named_not_rewritten() {
    let meta = Meta::new();
    let d = disks();
    let chunks: [ChunkId; 1] = [0x0A04];
    let (_id, key) = seed_completing_session(&meta, &d, "2a", None, &chunks, &[]);
    let before = meta.get_now(&key).expect("the seeded session");

    let report = restore_pass(&meta, &d).await.expect("the pass runs");
    let printed = format!("{report:?}");
    assert_eq!(
        meta.get_now(&key),
        Some(before),
        "a session record the codec refuses is left EXACTLY as it was found, never rewritten \
         from a guess about what it meant (ADR-0045)"
    );
    assert!(
        report.needs_human(),
        "a session that could not be fenced can still publish over bytes the restore did not \
         bring back: {printed}"
    );
    assert!(
        printed.contains(&String::from_utf8(key).unwrap()),
        "the session is not NAMED in the report, so an operator has nothing to repair: {printed}"
    );
    assert!(
        !fence_record(&meta).contains("\"complete\":true"),
        "the restore-fence generation was marked complete over a session that was never fenced"
    );
}

/// **(H)(ii)** A `Completing` session whose `seg:` records name a chunk that none of its `part:`
/// records holds — a part record the restored image is missing — is **still fenced**, and still
/// named as needing a human.
///
/// Both halves matter. Leaving it unfenced would leave a session a client can complete over; and
/// building the teardown from whatever `part:` keys happened to be present, and calling the run
/// clean, is exactly what #637 v1 did.
#[tokio::test]
async fn h_a_completing_session_whose_segments_outrun_its_parts_is_fenced_and_named() {
    let meta = Meta::new();
    let d = disks();
    let held: ChunkId = 0x0A05;
    let orphaned: ChunkId = 0x0A06;
    let (_id, key) =
        seed_completing_session(&meta, &d, "2b", Some(NONCE), &[held], &[held, orphaned]);

    let report = restore_pass(&meta, &d).await.expect("the pass runs");
    let printed = format!("{report:?}");
    assert!(printed.contains("sessions_fenced: 1"), "{printed}");
    let fenced = decode_session_record(&meta.get_now(&key).expect("the session record"))
        .expect("the fenced session record decodes");
    assert_eq!(fenced.state(), &SessionState::Aborting {});
    assert!(
        report.needs_human(),
        "the bytes behind the chunk no surviving `part:` record holds have no named deleter: \
         {printed}"
    );
    assert!(
        printed.contains(&String::from_utf8(key).unwrap()),
        "the session is not NAMED in the report: {printed}"
    );
    assert!(
        !fence_record(&meta).contains("\"complete\":true"),
        "the restore-fence generation was marked complete over a teardown that cannot be shown \
         to name every record the attempt wrote"
    );
}

// =================================================================================================
// (I) — the restore-fence generation record
// =================================================================================================

/// **(I)** The generation record, on durable state, in three arms:
///
/// * **(i)** before any post-restore pass it is **absent** — "never fenced" has exactly one
///   spelling;
/// * **(ii)** *during* a pass, read at its first fence commit, it names that pass's generation
///   and reads **not complete**; and
/// * **(iii)** after the pass it reads complete for that generation — and a **second** pass
///   advances the generation and reads not-complete until it finishes, so a later restore
///   invalidates the earlier completion instead of being masked by it.
#[tokio::test]
async fn i_the_restore_fence_generation_advances_and_completes_last() {
    let meta = Meta::new();
    let d = disks();
    seed_open_session(&meta, &d, "3a", 0x0A07);
    // A stray, so the pass has mark batches to write after the fence — "complete" has to be the
    // LAST write it makes, not merely the last fence.
    place(&d, 2, frag(0x0A08, 0));

    // (i)
    assert!(
        !meta.holds(FENCE_KEY),
        "a store no post-restore pass has run over must carry no generation record: absence is \
         the one spelling of \"never fenced\""
    );

    // (ii)
    meta.probe(b"mpu:", FENCE_KEY, false);
    let report = restore_pass(&meta, &d).await.expect("the first pass runs");
    assert!(!report.needs_human(), "{report:?}");
    let during = meta
        .probed()
        .expect("the probe fired at the first fence commit")
        .expect("the generation record is already durable when the first fence commits");
    let during = String::from_utf8(during.to_vec()).expect("the record is JSON text");
    assert!(
        during.contains("\"generation\":1") && during.contains("\"complete\":false"),
        "at the first fence commit the record must name THIS pass's generation and read \
         not-complete, or a gateway could read a completion covering less than the whole pass: \
         {during}"
    );
    assert!(
        meta.read_of(FENCE_KEY),
        "the pass never read the prior generation: {:?}",
        meta.reads()
    );

    // (iii)
    let after = fence_record(&meta);
    assert!(
        after.contains("\"generation\":1") && after.contains("\"complete\":true"),
        "after a clean pass the record must read complete for that pass's generation: {after}"
    );
    assert!(
        meta.holds(&orphan_key(2, frag(0x0A08, 0))),
        "the pass marked nothing, so \"complete after every write\" proves nothing"
    );

    // A SECOND restore, bringing back a different session. The completion above must not mask it.
    seed_open_session(&meta, &d, "3b", 0x0A09);
    meta.probe(b"mpu:", FENCE_KEY, false);
    let report = restore_pass(&meta, &d).await.expect("the second pass runs");
    assert!(!report.needs_human(), "{report:?}");
    let during = meta
        .probed()
        .expect("the probe fired at the second pass's first fence commit")
        .expect("the record is durable");
    let during = String::from_utf8(during.to_vec()).expect("the record is JSON text");
    assert!(
        during.contains("\"generation\":2") && during.contains("\"complete\":false"),
        "a second pass must ADVANCE the generation and read not-complete while it runs, or a \
         later restore inherits an earlier pass's certification: {during}"
    );
    let after = fence_record(&meta);
    assert!(
        after.contains("\"generation\":2") && after.contains("\"complete\":true"),
        "the second pass must certify its OWN generation: {after}"
    );
}
