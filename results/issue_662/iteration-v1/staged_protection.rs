//! Issue #662 (child 2 of #637): **staged bytes are a protection class, and GC records a
//! reclamation before it destroys anything** (proposal 0016 decision 2, `0016:765-893`; the
//! ledger rules `0016:1189-1247`, `:1312-1336`).
//!
//! The invariant (`0016:869-871`): every durable byte is, at every instant, committed-referenced,
//! staged with a named exit, or garbage with a sound reclamation path — and no pass destroys a
//! byte before that destruction is durable in metadata. On `main` a multipart session's bytes are
//! in no protected class: a committed part's fragments and an in-flight part's owned `sidx:`
//! fragments are reclaimed by GC the moment they carry an `orphan:` mark past grace, and the
//! post-restore pass marks them stranded on the same predicate. GC deletes a fragment before it
//! records that it is doing so, and it reads only the bare-decimal mark.
//!
//! Every leg runs the production entry points — `reconcile_step` with a `GcContext`, and
//! `reconcile_after_restore` — over in-memory doubles ([`Meta`], [`DServer`]). No client can
//! create a multipart session until the S3 verbs land (#508), so every staged record here is
//! seeded, as the bytes the base decoders accept: session and part values as hand-written JSON in
//! the shape `crates/core/tests/multipart_session_records.rs:81-145` builds (neither record has a
//! writer-side constructor, `crates/core/src/multipart.rs:2127-2130`, `:2492`), owned entries
//! through `OwnedEntry`'s checked path, and every one round-tripped through its decoder
//! (`decode_session_record`, `decode_part_record`, `decode_owned_entry`) before it is seeded. The
//! structured `orphan:` values are raw JSON bytes too: this file names nothing the slice adds, so
//! it compiles, and fails by assertion, on the base.
//!
//! The legs:
//!
//! * **A** — GC protects both staged classes, with the evidence present (a mark past grace).
//! * **B** — restore protects them through the same predicate, and GC then keeps them.
//! * **C** — source before destination, for both handoffs: a part commit (`sidx:` → `part:`) and
//!   a publication (`part:` → committed inode, X67), each performed atomically between the
//!   builder's two reads.
//! * **D** — the staged build reads per-session ranges, never a global scan.
//! * **E** — a pending byte retirement protects its fragment, by one keyed read (X97).
//! * **F** — reclamation is recorded before destruction: (i) an intent commit that errors
//!   destroys nothing; (ii) a mark changed under the pass loses only its own intent; (iii) an
//!   adoption CAS at the instant of the delete loses; (iv) a `reclaiming` mark is finished on the
//!   next pass without a second grace test.
//! * **G** — all three value shapes decode and none is rejected; a fourth fails closed.
//!
//! Two containment tests cover the staged records a pass cannot read (ADR-0045 decision 3): one
//! whose chunks cannot be known at all withholds every reclaim and mark until it is repaired, and
//! one whose placement cannot be trusted has its whole chunk held.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{self, orphan_key, EcScheme, InodeRecord};
use wyrd_core::multipart::{
    decode_owned_entry, decode_part_record, decode_session_record, mpu_key, part_key, part_range,
    retire_key, sidx_key, sidx_range, OwnedEntry, PartNumber, RetireMode, RetireToken,
    StagedPlacement, UploadId,
};
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, Custodian, ExpiredPendingPolicy, FencedZone,
    GcContext, Reconciled, RestoreReport,
};
use wyrd_traits::{
    page_cursor, page_limit, page_start, ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId,
    Health, MetadataStore, PageStart, Result, ScanCapExceeded, ScanPage, WriteBatch, SCAN_CAP,
};

/// The grace window every GC pass runs with.
const GRACE: u64 = 1_000;
/// An instant far from zero: a stamp at it is inside the grace window, a stamp at zero long past.
const NOW: u64 = 1_000_000;
/// Long after every stamp in this file, `NOW` included.
const LATER: u64 = NOW + 10 * GRACE;
/// The session epoch every staged record is written under.
const EPOCH: u64 = 3;
/// The owned entries' lease: live for the whole run.
const LEASE: u64 = NOW + 100 * GRACE;
/// Reed-Solomon 2+1 as a `part:` value spells its scheme — three fragments a chunk.
const RS21_JSON: &str = r#"{"ReedSolomon":{"k":2,"m":1}}"#;
const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };
/// `EcScheme::None` as a stored value spells it — one fragment a chunk, at index 0.
const NONE_JSON: &str = r#""None""#;

// ---- what the doubles saw, in one order across the metadata store and the D servers ----

/// One step at a store seam.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    /// A commit landed, deleting these keys.
    Committed { deletes: Vec<Vec<u8>> },
    /// `delete_fragment` on one D server.
    Deleted(DServerId, FragmentId),
}

type Log = Arc<Mutex<Vec<Event>>>;

/// One read the metadata double served.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Read {
    Get(Vec<u8>),
    Scan(Vec<u8>),
    ScanPage(Vec<u8>),
}

/// One commit the metadata double was handed, and what became of it (`None`: it errored).
#[derive(Clone, Debug)]
struct Commit {
    required: Vec<Vec<u8>>,
    puts: Vec<Vec<u8>>,
    outcome: Option<CommitOutcome>,
}

/// A handoff the metadata double performs **atomically, once**, the instant the first read of
/// either of its two ranges completes — so it lands between the builder's two reads of it,
/// whichever of them the builder makes first (leg C).
struct Handoff {
    ranges: [Vec<u8>; 2],
    puts: Vec<(Vec<u8>, Bytes)>,
    deletes: Vec<Vec<u8>>,
    /// The read that fired it.
    fired_after: Option<Vec<u8>>,
}

/// Whether a read of `prefix` reads any of `range`: a prefix of it (`""`, `"par"`) or a
/// narrowing of it.
fn reaches(prefix: &[u8], range: &[u8]) -> bool {
    range.starts_with(prefix) || prefix.starts_with(range)
}

// ---- the metadata double ----

/// An in-memory metadata store over an ORDERED map, recording every read and commit, with a scan
/// cap (on a `scan`'s answer and on a `scan_page` page, the one knob the production backends apply
/// to both) and the injections the legs need.
struct Meta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    reads: Mutex<Vec<Read>>,
    commits: Mutex<Vec<Commit>>,
    log: Log,
    cap: usize,
    handoff: Mutex<Option<Handoff>>,
    /// F(i): refuse, with an error, any commit carrying a precondition on an `orphan:` key.
    fail_intents: bool,
    /// F(ii): the moment the first `orphan:` page has been served, rewrite this key's value — a
    /// change landing after GC read the mark and before anything GC commits.
    restamp: Mutex<Option<(Vec<u8>, Bytes)>>,
}

impl Meta {
    fn new(log: &Log) -> Self {
        Self {
            kv: Mutex::default(),
            reads: Mutex::default(),
            commits: Mutex::default(),
            log: Arc::clone(log),
            cap: SCAN_CAP,
            handoff: Mutex::default(),
            fail_intents: false,
            restamp: Mutex::default(),
        }
    }

    fn seed(&self, key: Vec<u8>, value: impl Into<Bytes>) {
        self.kv.lock().unwrap().insert(key, value.into());
    }

    fn value(&self, key: &[u8]) -> Option<Bytes> {
        self.kv.lock().unwrap().get(key).cloned()
    }

    fn reads(&self) -> Vec<Read> {
        self.reads.lock().unwrap().clone()
    }

    fn commits(&self) -> Vec<Commit> {
        self.commits.lock().unwrap().clone()
    }

    fn note(&self, read: Read) {
        self.reads.lock().unwrap().push(read);
    }

    /// A read of `prefix` has completed: fire the handoff if it is the first read of either of
    /// its ranges.
    fn read_done(&self, prefix: &[u8]) {
        let mut handoff = self.handoff.lock().unwrap();
        let Some(handoff) = handoff.as_mut() else {
            return;
        };
        if handoff.fired_after.is_some() || !handoff.ranges.iter().any(|r| reaches(prefix, r)) {
            return;
        }
        let mut kv = self.kv.lock().unwrap();
        for (key, value) in &handoff.puts {
            kv.insert(key.clone(), value.clone());
        }
        for key in &handoff.deletes {
            kv.remove(key);
        }
        handoff.fired_after = Some(prefix.to_vec());
    }

    fn handoff_fired_after(&self) -> Option<Vec<u8>> {
        self.handoff
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|handoff| handoff.fired_after.clone())
    }
}

#[async_trait]
impl MetadataStore for Meta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.note(Read::Get(key.to_vec()));
        Ok(self.value(key))
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        self.note(Read::Scan(prefix.to_vec()));
        let hits: Vec<(Vec<u8>, Bytes)> = self
            .kv
            .lock()
            .unwrap()
            .range(prefix.to_vec()..)
            .take_while(|(key, _)| key.starts_with(prefix))
            .take(self.cap.saturating_add(1))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        // `>` not `>=`: exactly `cap` keys is a complete answer, as on every backend.
        if hits.len() > self.cap {
            return Err(Box::new(ScanCapExceeded {
                cap: self.cap,
                prefix: prefix.to_vec(),
            }));
        }
        self.read_done(prefix);
        Ok(hits)
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<ScanPage> {
        self.note(Read::ScanPage(prefix.to_vec()));
        let bound = page_limit(limit, self.cap, prefix)?;
        let lower = match page_start(prefix, after) {
            PageStart::Prefix => Bound::Included(prefix.to_vec()),
            PageStart::After(cursor) => Bound::Excluded(cursor.to_vec()),
            PageStart::PastPrefix => return Ok((Vec::new(), None)),
        };
        let items: Vec<(Vec<u8>, Bytes)> = self
            .kv
            .lock()
            .unwrap()
            .range((lower, Bound::Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .take(bound)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let next = page_cursor(&items, bound);
        if prefix == metadata::ORPHAN_PREFIX {
            if let Some((key, value)) = self.restamp.lock().unwrap().take() {
                self.kv.lock().unwrap().insert(key, value);
            }
        }
        self.read_done(prefix);
        Ok((items, next))
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let mut record = Commit {
            required: batch
                .preconditions
                .iter()
                .map(|pre| pre.key.clone())
                .collect(),
            puts: batch.puts.iter().map(|(key, _)| key.clone()).collect(),
            outcome: None,
        };
        let intent = record
            .required
            .iter()
            .any(|key| key.starts_with(metadata::ORPHAN_PREFIX));
        if self.fail_intents && intent {
            self.commits.lock().unwrap().push(record);
            return Err("injected: the commit recording reclaim intent failed".into());
        }
        let outcome = {
            let mut kv = self.kv.lock().unwrap();
            if batch
                .preconditions
                .iter()
                .any(|pre| kv.get(&pre.key).cloned() != pre.expected)
            {
                CommitOutcome::Conflict
            } else {
                for (key, value) in &batch.puts {
                    kv.insert(key.clone(), value.clone());
                }
                for key in &batch.deletes {
                    kv.remove(key);
                }
                CommitOutcome::Committed
            }
        };
        if outcome == CommitOutcome::Committed {
            self.log.lock().unwrap().push(Event::Committed {
                deletes: batch.deletes.clone(),
            });
        }
        record.outcome = Some(outcome);
        self.commits.lock().unwrap().push(record);
        Ok(outcome)
    }
}

// ---- the D-server double ----

/// F(iii): an adoption CAS the D server attempts at the instant GC deletes `frag` — a move
/// committing a placement that names the fragment, preconditioned on its pre-mark's bytes.
struct Adoption {
    meta: Arc<Meta>,
    frag: FragmentId,
    mark: (Vec<u8>, Bytes),
    outcome: Option<CommitOutcome>,
}

/// One D server's fragments — a deliberately dumb `ChunkStore` that logs every delete.
struct DServer {
    id: DServerId,
    frags: Mutex<HashMap<FragmentId, Bytes>>,
    deleted: Mutex<Vec<FragmentId>>,
    log: Log,
    adoption: Mutex<Option<Adoption>>,
}

impl DServer {
    fn new(id: DServerId, log: &Log) -> Self {
        Self {
            id,
            frags: Mutex::default(),
            deleted: Mutex::default(),
            log: Arc::clone(log),
            adoption: Mutex::default(),
        }
    }

    fn put(&self, frag: FragmentId) {
        self.frags
            .lock()
            .unwrap()
            .insert(frag, Bytes::from_static(b"bytes"));
    }

    fn holds(&self, frag: FragmentId) -> bool {
        self.frags.lock().unwrap().contains_key(&frag)
    }

    fn deletes_of(&self, frag: FragmentId) -> usize {
        self.deleted
            .lock()
            .unwrap()
            .iter()
            .filter(|&&f| f == frag)
            .count()
    }
}

#[async_trait]
impl ChunkStore for DServer {
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
        self.log.lock().unwrap().push(Event::Deleted(self.id, id));
        self.deleted.lock().unwrap().push(id);
        // The adoption races the delete: it commits at the instant GC destroys the bytes.
        let pending = match self.adoption.lock().unwrap().as_ref() {
            Some(adoption) if adoption.frag == id && adoption.outcome.is_none() => {
                Some((Arc::clone(&adoption.meta), adoption.mark.clone()))
            }
            _ => None,
        };
        if let Some((meta, (key, prior))) = pending {
            let adopted = [b"adopted:".as_slice(), &key].concat();
            let outcome = meta
                .commit(
                    WriteBatch::new()
                        .require(key, prior)
                        .put(adopted, "placement"),
                )
                .await?;
            self.adoption.lock().unwrap().as_mut().unwrap().outcome = Some(outcome);
        }
        self.frags.lock().unwrap().remove(&id);
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

// ---- fixtures: the staged records, as the bytes the base decoders accept ----

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

fn upload(tag: u8) -> UploadId {
    UploadId::new(format!("{tag:02x}").repeat(16)).unwrap()
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).unwrap()
}

/// A placement vector as the encoder spells one: `[1,2,3]`, no spaces.
fn placement_json(placement: &[DServerId]) -> String {
    let servers: Vec<String> = placement.iter().map(|d| d.to_string()).collect();
    format!("[{}]", servers.join(","))
}

/// One chunk of a `part:` record or a flat chunk map, five bytes long.
fn chunk_json(id: ChunkId, scheme: &str, placement: &[DServerId]) -> String {
    format!(
        r#"{{"id":{id},"scheme":{scheme},"len":5,"placement":{}}}"#,
        placement_json(placement)
    )
}

/// The `mpu:` value of an `Open` session — `session_with(None, EPOCH, OPEN_JSON)` of
/// `multipart_session_records.rs:88-99`, with no Complete fence attempted yet.
fn open_session_bytes() -> Vec<u8> {
    let bytes = format!(
        r#"{{"parent":42,"object":"key/one","created_at_millis":1000,"clock_source":"wall","epoch":{EPOCH},"attempts":0,"state":{{"kind":"Open"}}}}"#
    )
    .into_bytes();
    decode_session_record(&bytes).expect("fixture: an Open session the base decoder accepts");
    bytes
}

/// A committed `part:` value holding `chunks` — `part(..)` of `multipart_session_records.rs:135`.
fn part_bytes(chunks: &[String]) -> Vec<u8> {
    let bytes = format!(
        r#"{{"chunks":[{}],"len":{},"digest":"{}","committed_at_millis":1000,"session_epoch":{EPOCH}}}"#,
        chunks.join(","),
        5 * chunks.len(),
        "ab".repeat(32),
    )
    .into_bytes();
    decode_part_record(&bytes).expect("fixture: a part record the base decoder accepts");
    bytes
}

/// A committed inode whose flat chunk map holds `chunks`.
fn committed_inode_bytes(chunks: &[String]) -> Vec<u8> {
    let bytes = format!(
        r#"{{"size":{},"chunk_map":[{}],"state":"Committed","version":1}}"#,
        5 * chunks.len(),
        chunks.join(","),
    )
    .into_bytes();
    let record: InodeRecord =
        metadata::decode(&bytes).expect("fixture: a committed inode the base decoder accepts");
    assert_eq!(
        metadata::encode(&record).as_ref(),
        bytes.as_slice(),
        "fixture: the inode is spelled as its encoder spells it"
    );
    bytes
}

/// An owned `sidx:` entry for `chunk`, planned onto `placement`: its key and its value, minted
/// through `OwnedEntry`'s checked path and read back through `decode_owned_entry`.
fn owned_entry(
    owner: &UploadId,
    part_number: PartNumber,
    chunk: ChunkId,
    scheme: EcScheme,
    placement: Vec<DServerId>,
) -> (Vec<u8>, Bytes) {
    let key = sidx_key(owner, part_number, chunk);
    let staged = StagedPlacement::new(scheme, placement).unwrap();
    let value = metadata::encode(&OwnedEntry::new(owner.clone(), LEASE, staged).to_pending());
    decode_owned_entry(&key, &value).expect("fixture: an owned entry the base decoder accepts");
    (key, value)
}

/// A legacy mark: the bare decimal `mark_orphaned` writes (`gc.rs:181-193` on the base).
fn legacy(at: u64) -> Bytes {
    Bytes::from(at.to_string())
}

/// A structured mark, `{orphaned_at_millis, event}` (`0016:1195-1197`).
fn structured(at: u64, event: &str) -> Bytes {
    Bytes::from(format!(
        r#"{{"orphaned_at_millis":{at},"event":"{event}"}}"#
    ))
}

/// A `reclaiming` mark, `{orphaned_at_millis, event?, reclaiming: true}` (`0016:1325`).
fn reclaiming(at: u64, event: Option<&str>) -> Bytes {
    Bytes::from(match event {
        Some(event) => {
            format!(r#"{{"orphaned_at_millis":{at},"event":"{event}","reclaiming":true}}"#)
        }
        None => format!(r#"{{"orphaned_at_millis":{at},"reclaiming":true}}"#),
    })
}

fn servers(n: usize, log: &Log) -> Vec<DServer> {
    (0..n).map(|i| DServer::new(i as DServerId, log)).collect()
}

fn fleet_of(servers: &[DServer]) -> Vec<(DServerId, &dyn ChunkStore)> {
    servers
        .iter()
        .map(|server| (server.id, server as &dyn ChunkStore))
        .collect()
}

/// One GC pass at `now`, through the fenced `reconcile_step`, with a fresh `GcContext` as the
/// deployed loop builds one — under `Defer`, a deployment's default.
async fn gc(
    meta: &Meta,
    fleet: &[(DServerId, &dyn ChunkStore)],
    now: u64,
) -> std::result::Result<Reconciled, String> {
    let coord = MemCoordination::new();
    let leader = Custodian::elect(&coord, "zone-staged-protection")
        .await
        .unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    let ctx = GcContext {
        meta,
        fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_step(&zone, &leader, Some(&ctx), None, None, None, now)
        .await
        .map_err(|err| err.to_string())
}

async fn restore(meta: &Meta, fleet: &[(DServerId, &dyn ChunkStore)], now: u64) -> RestoreReport {
    let ctx = GcContext {
        meta,
        fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_after_restore(&ctx, now)
        .await
        .expect("the post-restore pass")
}

/// A permissive global default, installed before any callsite a pass fires is first hit, so none
/// can latch `Interest::never` under the parallel harness and leave leg G's capture empty (#214;
/// `crates/custodian/tests/gc.rs:1143-1146`).
fn permissive_tracing() {
    let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
}

/// The staged fixture legs A and B share: one `Open` session holding a committed part 1 — chunk
/// 100, RS(2,1) on D servers 1, 2, 3 — and an in-flight part 2 whose owned entry plans chunk 200
/// onto D servers 2, 3, 1; every fragment of both on disk. Plus chunk 300 on D server 1, which no
/// record names: the control, so a pass that reclaimed nothing could not pass for one that
/// protected something. Returns the staged fragments with their D servers.
fn seed_one_session(meta: &Meta, servers: &[DServer]) -> Vec<(DServerId, FragmentId)> {
    let session = upload(1);
    meta.seed(mpu_key(&session), open_session_bytes());
    let committed = [1, 2, 3];
    meta.seed(
        part_key(&session, part(1)),
        part_bytes(&[chunk_json(100, RS21_JSON, &committed)]),
    );
    let planned = vec![2, 3, 1];
    let (key, value) = owned_entry(&session, part(2), 200, RS21, planned.clone());
    meta.seed(key, value);
    let mut staged = Vec::new();
    for (index, (&c, &p)) in committed.iter().zip(&planned).enumerate() {
        staged.push((c, frag(100, index as u16)));
        staged.push((p, frag(200, index as u16)));
    }
    for &(dserver, f) in &staged {
        servers[dserver as usize].put(f);
    }
    servers[1].put(CONTROL);
    staged
}

/// The unreferenced fragment every leg's control is.
const CONTROL: FragmentId = FragmentId {
    chunk: 300,
    index: 0,
};

// ---- leg A: GC protects both staged classes, with the evidence present ----

/// Every staged fragment — a committed part's (`part:`) and an in-flight part's (`sidx:`) —
/// carries an `orphan:` mark long past grace, so nothing but the staged class stands between it
/// and a reclaim: without the mark GC's conservative arm would keep it anyway (`gc.rs`'s "no
/// evidence the grace window elapsed" arm), and the leg would pass on an absence. Every one
/// survives, its mark untouched; the control is reclaimed. On the base every one is reclaimed.
#[tokio::test]
async fn a_gc_protects_both_staged_classes_with_the_evidence_present() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(4, &log);
    let staged = seed_one_session(&meta, &servers);
    for &(dserver, f) in &staged {
        meta.seed(orphan_key(dserver, f), legacy(0));
    }
    meta.seed(orphan_key(1, CONTROL), legacy(0));

    let fleet = fleet_of(&servers);
    let outcome = gc(&meta, &fleet, NOW).await;
    for &(dserver, f) in &staged {
        assert!(
            servers[dserver as usize].holds(f),
            "leg A: staged fragment {f:?} on D server {dserver} was reclaimed on a mark past \
             grace — a {} fragment of a live upload",
            if f.chunk == 100 {
                "committed part's"
            } else {
                "in-flight part's"
            }
        );
        assert_eq!(
            meta.value(&orphan_key(dserver, f)),
            Some(legacy(0)),
            "leg A: the mark on staged fragment {f:?} was consumed or rewritten"
        );
    }
    assert!(
        outcome.is_ok(),
        "leg A: the GC pass failed: {:?}",
        outcome.err()
    );
    assert!(
        !servers[1].holds(CONTROL),
        "fixture: the unreferenced control, marked past grace, is reclaimed"
    );
}

// ---- leg B: restore protects them through the same predicate ----

/// The same session, no marks. The post-restore pass writes no `orphan:` key for any staged
/// fragment and does not count one as stranded (it marks only the control); then, long past
/// grace, GC keeps every one of them and reclaims the control. A protection built inside GC
/// alone passes leg A and fails here: restore strands the parts and the next GC pass deletes
/// them. On the base that is exactly what happens.
#[tokio::test]
async fn b_restore_protects_them_through_the_same_predicate() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(4, &log);
    let staged = seed_one_session(&meta, &servers);
    let fleet = fleet_of(&servers);

    let report = restore(&meta, &fleet, NOW).await;
    for &(dserver, f) in &staged {
        assert_eq!(
            meta.value(&orphan_key(dserver, f)),
            None,
            "leg B: the post-restore pass marked staged fragment {f:?} on D server {dserver} \
             stranded ({report:?})"
        );
    }
    assert_eq!(
        report.stranded_marked, 1,
        "leg B: only the control is stranded — the staged fragments are not unreferenced: \
         {report:?}"
    );
    assert_eq!(
        meta.value(&orphan_key(1, CONTROL)),
        Some(legacy(NOW)),
        "fixture: the control is marked, at the pass's clock"
    );

    let outcome = gc(&meta, &fleet, LATER).await;
    for &(dserver, f) in &staged {
        assert!(
            servers[dserver as usize].holds(f),
            "leg B: staged fragment {f:?} on D server {dserver} was reclaimed after the restore"
        );
    }
    assert!(
        outcome.is_ok(),
        "leg B: the GC pass failed: {:?}",
        outcome.err()
    );
    assert!(
        !servers[1].holds(CONTROL),
        "fixture: the control, marked by the restore and past grace, is reclaimed"
    );
}

// ---- leg C: source before destination, for both handoffs ----

/// **(i) The part commit.** Chunk 400 is in flight: an owned entry plans it onto D server 1,
/// where its one fragment sits, marked past grace. The double performs the part commit — one
/// batch deleting the owned entry and writing the `part:` record that holds the chunk — the
/// instant the builder's first read of either range completes. A build that read `part:` first
/// would see the chunk in neither class. On the base neither range is read and the fragment is
/// reclaimed.
#[tokio::test]
async fn c1_a_part_commit_between_the_builders_reads_leaves_no_gap() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let session = upload(2);
    let f = frag(400, 0);
    meta.seed(mpu_key(&session), open_session_bytes());
    let (sidx, owned) = owned_entry(&session, part(1), 400, EcScheme::None, vec![1]);
    meta.seed(sidx.clone(), owned);
    servers[1].put(f);
    meta.seed(orphan_key(1, f), legacy(0));
    *meta.handoff.lock().unwrap() = Some(Handoff {
        ranges: [sidx_range(&session), part_range(&session)],
        puts: vec![(
            part_key(&session, part(1)),
            Bytes::from(part_bytes(&[chunk_json(400, NONE_JSON, &[1])])),
        )],
        deletes: vec![sidx.clone()],
        fired_after: None,
    });

    let fleet = fleet_of(&servers);
    let outcome = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(f),
        "leg C(i): the part-commit handoff landed between the builder's two reads and the \
         chunk was reclaimed — it was in neither class the build read"
    );
    assert!(
        outcome.is_ok(),
        "leg C(i): the GC pass failed: {:?}",
        outcome.err()
    );
    assert!(
        meta.handoff_fired_after().is_some() && meta.value(&sidx).is_none(),
        "fixture: the handoff landed during the pass"
    );
}

/// **(ii) The publication — X67.** Chunk 500 is committed in part 1 of a session, one fragment
/// on D server 1, marked past grace. The double performs the publication — the committed inode
/// naming the chunk written, and the `part:` record removed as the records drain removes it —
/// the instant the builder's first read of `part:<id>:` or of `inode:` completes. A build that
/// read the inodes first could miss the flip and then miss the record the drain had removed. On
/// the base the one read is `inode:`, the handoff lands after it, and the fragment is reclaimed.
#[tokio::test]
async fn c2_a_publication_between_the_builders_reads_leaves_no_gap() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let session = upload(3);
    let f = frag(500, 0);
    let chunk = chunk_json(500, NONE_JSON, &[1]);
    meta.seed(mpu_key(&session), open_session_bytes());
    let record = part_key(&session, part(1));
    meta.seed(record.clone(), part_bytes(std::slice::from_ref(&chunk)));
    servers[1].put(f);
    meta.seed(orphan_key(1, f), legacy(0));
    let inode = metadata::inode_key(77);
    *meta.handoff.lock().unwrap() = Some(Handoff {
        ranges: [part_range(&session), b"inode:".to_vec()],
        puts: vec![(inode.clone(), Bytes::from(committed_inode_bytes(&[chunk])))],
        deletes: vec![record.clone()],
        fired_after: None,
    });

    let fleet = fleet_of(&servers);
    let outcome = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(f),
        "leg C(ii): the publication landed between the builder's two reads and the published \
         object's chunk was reclaimed — it was in neither class the build read"
    );
    assert!(
        outcome.is_ok(),
        "leg C(ii): the GC pass failed: {:?}",
        outcome.err()
    );
    assert!(
        meta.handoff_fired_after().is_some()
            && meta.value(&record).is_none()
            && meta.value(&inode).is_some(),
        "fixture: the publication landed during the pass"
    );
}

// ---- leg D: the staged build is bounded per session, never a global scan ----

/// With the double's cap lowered to [`D_CAP`], more committed parts are seeded across sessions
/// than one `scan("part:")` could return — and more owned entries than one `scan("sidx:")` could
/// — while every session's own ranges, and the session list, stay below it (0016's
/// `SCAN_CAP / MAX_PARTS_PER_SESSION` row, `0016:890`). `reconcile_step` succeeds, no `scan` or
/// `scan_page` reads the `part:` or `sidx:` namespace as a whole, and every staged fragment —
/// each marked past grace — survives while the control is reclaimed.
#[tokio::test]
async fn d_the_staged_build_reads_per_session_ranges_never_a_global_scan() {
    const D_CAP: usize = 24;
    const SESSIONS: u8 = 6;
    const PARTS: u32 = 5;
    const OWNED: u32 = 5;
    permissive_tracing();
    let log = Log::default();
    let meta = Meta {
        cap: D_CAP,
        ..Meta::new(&log)
    };
    let servers = servers(2, &log);
    let mut staged = Vec::new();
    for s in 0..SESSIONS {
        let session = upload(0x10 + s);
        meta.seed(mpu_key(&session), open_session_bytes());
        for p in 1..=PARTS {
            let chunk = 10_000 + u128::from(s) * 100 + u128::from(p);
            meta.seed(
                part_key(&session, part(p)),
                part_bytes(&[chunk_json(chunk, NONE_JSON, &[1])]),
            );
            staged.push(frag(chunk, 0));
        }
        for o in 1..=OWNED {
            let chunk = 20_000 + u128::from(s) * 100 + u128::from(o);
            let (key, value) =
                owned_entry(&session, part(PARTS + o), chunk, EcScheme::None, vec![1]);
            meta.seed(key, value);
            staged.push(frag(chunk, 0));
        }
    }
    assert!(
        usize::from(SESSIONS) * PARTS as usize > D_CAP
            && usize::from(SESSIONS) * OWNED as usize > D_CAP
            && (PARTS as usize) < D_CAP
            && (OWNED as usize) < D_CAP
            && usize::from(SESSIONS) < D_CAP,
        "fixture: each namespace as a whole is past the cap, every session's ranges below it"
    );
    for &f in &staged {
        servers[1].put(f);
        meta.seed(orphan_key(1, f), legacy(0));
    }
    servers[1].put(CONTROL);
    meta.seed(orphan_key(1, CONTROL), legacy(0));

    let fleet = fleet_of(&servers);
    let outcome = gc(&meta, &fleet, NOW).await;
    assert!(
        outcome.is_ok(),
        "leg D: the GC pass failed: {:?}",
        outcome.err()
    );
    let whole: Vec<Read> = meta
        .reads()
        .into_iter()
        .filter(|read| match read {
            Read::Scan(prefix) | Read::ScanPage(prefix) => {
                b"part:".starts_with(prefix) || b"sidx:".starts_with(prefix)
            }
            Read::Get(_) => false,
        })
        .collect();
    assert!(
        whole.is_empty(),
        "leg D: the pass read a staged namespace as a whole — past the cap that read fails \
         every pass: {whole:?}"
    );
    for &f in &staged {
        assert!(
            servers[1].holds(f),
            "leg D: staged fragment {f:?} was reclaimed"
        );
    }
    assert!(
        !servers[1].holds(CONTROL),
        "fixture: the control is reclaimed"
    );
}

// ---- leg E: a pending byte retirement protects its fragment, by keyed lookup (X97) ----

/// An unreferenced fragment whose structured mark, long past grace, names a byte retirement by
/// its token. While `retire:bytes:<token>` exists the fragment survives; once it is gone the next
/// pass reclaims the fragment and consumes the mark. Each pass looks the obligation up with one
/// `get` of that key, and neither ever reads the `retire:` namespace as a range. On the base the
/// structured value does not decode, so the fragment is never reclaimed — the second pass is
/// the red.
#[tokio::test]
async fn e_a_pending_byte_retirement_protects_its_fragment_by_one_keyed_read() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let token = RetireToken::Session {
        upload_id: upload(4),
        epoch: 4,
        part: None,
    };
    let obligation = retire_key(RetireMode::Bytes, &token);
    let f = frag(600, 0);
    let mark = orphan_key(1, f);
    servers[1].put(f);
    meta.seed(mark.clone(), structured(0, &token.to_string()));
    // Presence is the whole of what the lookup asks (`0016:1244-1245`).
    meta.seed(obligation.clone(), "pending obligation");
    let fleet = fleet_of(&servers);

    let first = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(f),
        "leg E: the fragment was reclaimed while the byte retirement naming it is still draining"
    );
    assert!(first.is_ok(), "leg E: pass 1 failed: {:?}", first.err());
    let pass_one = meta.reads();

    meta.kv.lock().unwrap().remove(&obligation);
    let second = gc(&meta, &fleet, NOW).await;
    assert!(second.is_ok(), "leg E: pass 2 failed: {:?}", second.err());
    assert!(
        !servers[1].holds(f),
        "leg E: the retirement has drained, the mark is past its grace, and the fragment \
         survived — a structured mark must decode"
    );
    assert_eq!(
        meta.value(&mark),
        None,
        "leg E: the consumed mark is removed"
    );

    let reads = meta.reads();
    let pass_two = &reads[pass_one.len()..];
    for (pass, reads) in [(1, pass_one.as_slice()), (2, pass_two)] {
        let lookups = reads
            .iter()
            .filter(|read| **read == Read::Get(obligation.clone()))
            .count();
        assert_eq!(
            lookups, 1,
            "leg E, pass {pass}: the obligation is looked up with exactly one keyed read"
        );
        let ranged: Vec<&Read> = reads
            .iter()
            .filter(|read| match read {
                Read::Scan(prefix) | Read::ScanPage(prefix) => reaches(prefix, b"retire:"),
                Read::Get(_) => false,
            })
            .collect();
        assert!(
            ranged.is_empty(),
            "leg E, pass {pass}: the `retire:` namespace was read as a range: {ranged:?}"
        );
    }
}

// ---- leg F: reclamation is recorded before destruction (`0016:1312-1336`) ----

/// **(i)** The double errors on the commit that records reclaim intent. The fragment — unreferenced,
/// marked past grace — is still on disk after the pass, its mark unchanged, and the pass reports
/// the failure. On the base no such commit exists: the fragment is deleted before anything is
/// recorded.
#[tokio::test]
async fn f1_an_intent_commit_that_errors_destroys_nothing() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta {
        fail_intents: true,
        ..Meta::new(&log)
    };
    let servers = servers(2, &log);
    let f = frag(700, 0);
    servers[1].put(f);
    meta.seed(orphan_key(1, f), legacy(0));
    let fleet = fleet_of(&servers);

    let outcome = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(f),
        "leg F(i): the fragment was destroyed although its reclamation never became durable"
    );
    assert_eq!(
        (servers[1].deletes_of(f), meta.value(&orphan_key(1, f))),
        (0, Some(legacy(0))),
        "leg F(i): no delete may be issued, and the mark stays as it was"
    );
    assert!(
        outcome.is_err(),
        "leg F(i): a pass whose intent commit failed must say so, not report {outcome:?}"
    );
}

/// **(ii)** Five unreferenced fragments on one D server, each marked past grace. The instant GC
/// has read the ledger, one mark is re-stamped — a later unreference event, its new stamp inside
/// grace. The intent for that one is a `Conflict`: its fragment survives with the new mark
/// intact, while the four others in the same pass are still reclaimed. On the base the fragment
/// is deleted on the stale reading.
#[tokio::test]
async fn f2_a_mark_changed_under_the_pass_loses_only_its_own_intent() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let moved = frag(800, 0);
    let others: Vec<FragmentId> = (801..805).map(|chunk| frag(chunk, 0)).collect();
    for &f in others.iter().chain([&moved]) {
        servers[1].put(f);
        meta.seed(orphan_key(1, f), legacy(0));
    }
    *meta.restamp.lock().unwrap() = Some((orphan_key(1, moved), legacy(NOW)));
    let fleet = fleet_of(&servers);

    let outcome = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(moved),
        "leg F(ii): the fragment was reclaimed on a mark that changed after GC read it"
    );
    assert_eq!(
        meta.value(&orphan_key(1, moved)),
        Some(legacy(NOW)),
        "leg F(ii): the new mark must stand — it is the evidence for a later pass"
    );
    for &f in &others {
        assert!(
            !servers[1].holds(f) && meta.value(&orphan_key(1, f)).is_none(),
            "leg F(ii): {f:?} was not reclaimed — a lost intent must cost only its own fragment"
        );
    }
    assert!(
        outcome.is_ok(),
        "leg F(ii): the GC pass failed: {:?}",
        outcome.err()
    );
    let key = orphan_key(1, moved);
    assert!(
        meta.commits()
            .iter()
            .any(|commit| commit.required.contains(&key)
                && commit.outcome == Some(CommitOutcome::Conflict)),
        "leg F(ii): the intent on the changed mark must have lost its compare-and-set"
    );

    // A pass whose ONLY verdict loses that way reclaimed nothing, and must not certify a ledger it
    // did not finish judging: here the mark moves to another stamp still past grace, which the
    // next pass does act on.
    let lone = frag(806, 0);
    servers[1].put(lone);
    meta.seed(orphan_key(1, lone), legacy(0));
    *meta.restamp.lock().unwrap() = Some((orphan_key(1, lone), legacy(1)));
    let superseded = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(lone),
        "leg F(ii): the fragment was reclaimed on a mark that changed after GC read it"
    );
    assert_eq!(
        superseded,
        Ok(Reconciled::Partial),
        "leg F(ii): a pass whose one verdict was superseded has not judged the ledger as it now \
         stands — `Satisfied` would certify it"
    );
    let next = gc(&meta, &fleet, NOW).await;
    assert!(
        next.is_ok(),
        "leg F(ii): the next pass failed: {:?}",
        next.err()
    );
    assert!(
        !servers[1].holds(lone),
        "leg F(ii): the next pass judges the new mark, past grace, and reclaims the fragment"
    );
}

/// **(iii)** At the instant GC deletes the fragment, a move tries to adopt it: a CAS
/// `require(orphan:<pos> == <the bytes GC read>)` committing a placement that names it
/// (`0016:1291-1301`). It gets `Conflict` — reclamation began, durably, before the bytes went. On
/// the base the key still holds exactly those bytes, and the move publishes a placement over
/// deleted bytes.
#[tokio::test]
async fn f3_an_adoption_cas_at_the_instant_of_the_delete_loses() {
    permissive_tracing();
    let log = Log::default();
    let meta = Arc::new(Meta::new(&log));
    let servers = servers(2, &log);
    let f = frag(900, 0);
    servers[1].put(f);
    let key = orphan_key(1, f);
    meta.seed(key.clone(), legacy(0));
    *servers[1].adoption.lock().unwrap() = Some(Adoption {
        meta: Arc::clone(&meta),
        frag: f,
        mark: (key.clone(), legacy(0)),
        outcome: None,
    });
    let fleet = fleet_of(&servers);

    let outcome = gc(&meta, &fleet, NOW).await;
    let adopted = servers[1]
        .adoption
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|adoption| adoption.outcome);
    assert_eq!(
        adopted,
        Some(CommitOutcome::Conflict),
        "leg F(iii): the adoption CAS on the mark's original bytes must lose at the instant GC \
         deletes the fragment"
    );
    assert!(
        outcome.is_ok(),
        "leg F(iii): the GC pass failed: {:?}",
        outcome.err()
    );
    assert!(
        !servers[1].holds(f) && meta.value(&key).is_none(),
        "fixture: GC did reclaim the fragment and consume its mark"
    );
}

/// **(iv) Restart.** Two fragments whose marks are already `reclaiming` — a pass decided, recorded
/// and died before deleting — one stamped just now, one naming an event. The next pass, at a time
/// still inside both stamps' grace windows, finishes both: each fragment deleted exactly once,
/// then its key, and no fresh intent written. A second pass deletes nothing more. On the base the
/// value does not decode, and the fragments are kept forever.
#[tokio::test]
async fn f4_a_reclaiming_mark_is_finished_without_a_second_grace_test() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let cases = [
        (frag(1_000, 0), reclaiming(NOW, None)),
        (frag(1_001, 0), reclaiming(NOW, Some("g:12:3"))),
    ];
    for (f, value) in &cases {
        servers[1].put(*f);
        meta.seed(orphan_key(1, *f), value.clone());
    }
    let fleet = fleet_of(&servers);

    let outcome = gc(&meta, &fleet, NOW).await;
    for (f, _) in &cases {
        assert_eq!(
            servers[1].deletes_of(*f),
            1,
            "leg F(iv): a `reclaiming` mark over a present fragment is finished — the fragment \
             deleted exactly once — with no second grace test"
        );
    }
    assert!(
        outcome.is_ok(),
        "leg F(iv): the GC pass failed: {:?}",
        outcome.err()
    );
    let events = log.lock().unwrap().clone();
    for (f, _) in &cases {
        let key = orphan_key(1, *f);
        let deleted = events
            .iter()
            .position(|event| *event == Event::Deleted(1, *f));
        let consumed = events.iter().position(
            |event| matches!(event, Event::Committed { deletes } if deletes.contains(&key)),
        );
        assert!(
            matches!((deleted, consumed), (Some(d), Some(c)) if d < c),
            "leg F(iv): the fragment is deleted first, then its key: {events:?}"
        );
        assert!(
            !meta
                .commits()
                .iter()
                .any(|commit| commit.required.contains(&key) || commit.puts.contains(&key)),
            "leg F(iv): a decided reclamation writes no fresh intent"
        );
    }

    let again = gc(&meta, &fleet, NOW).await;
    assert!(again.is_ok(), "leg F(iv): pass 2 failed: {:?}", again.err());
    for (f, _) in &cases {
        assert_eq!(
            servers[1].deletes_of(*f),
            1,
            "leg F(iv): the reclamation happens once"
        );
    }
}

// ---- leg G: all three value shapes decode, and none is rejected ----

/// Collects what a `tracing` subscriber writes, so leg G reads back the audit lines the passes
/// actually emitted (`crates/custodian/tests/gc_ledger_walk.rs:1204-1222`).
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

impl Capture {
    fn dispatch(&self) -> tracing::Dispatch {
        tracing::Dispatch::new(
            tracing_subscriber::registry().with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(self.clone()),
            ),
        )
    }

    /// Whether a line on `target` named `key` as an unreadable mark.
    fn named_unreadable(&self, target: &str, key: &[u8]) -> bool {
        let logged = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        let named = format!(r#""mark":"{}""#, String::from_utf8_lossy(key));
        logged.lines().any(|line| {
            line.contains(r#""action":"unreadable-orphan-mark""#)
                && line.contains(&format!(r#""target":"{target}""#))
                && line.contains(&named)
        })
    }
}

/// One unreferenced fragment per shape, on D server 1: a legacy mark past grace and one inside
/// it, a structured mark past grace and one inside it, two `reclaiming` marks stamped just now
/// (one naming an event, one not), and a value that is **none** of the three — an object claiming
/// `"reclaiming":false`, a spelling no writer produces.
///
/// The post-restore pass counts all seven as already marked and leaves every value byte for
/// byte as it was — a legacy value is never rewritten on read — marking only the one fragment
/// that had no mark. GC then honours each shape's meaning: past grace reclaimed, inside it kept,
/// `reclaiming` finished whatever its stamp. The value that is none of the three fails closed:
/// both passes leave it byte-identical, its fragment is never reclaimed, and each pass names it on
/// its own audit seam. On the base the structured and `reclaiming` values do not decode.
#[tokio::test]
async fn g_all_three_value_shapes_decode_and_a_fourth_fails_closed() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let legacy_old = frag(1_100, 0);
    let legacy_new = frag(1_101, 0);
    let structured_old = frag(1_102, 0);
    let structured_new = frag(1_103, 0);
    let reclaiming_event = frag(1_104, 0);
    let reclaiming_bare = frag(1_105, 0);
    let unreadable = frag(1_106, 0);
    let unmarked = frag(1_107, 0);
    let unreadable_value = Bytes::from_static(br#"{"orphaned_at_millis":0,"reclaiming":false}"#);
    let marks = [
        (legacy_old, legacy(0)),
        (legacy_new, legacy(NOW)),
        (structured_old, structured(0, "g:5:1")),
        (structured_new, structured(NOW, "g:5:2")),
        (reclaiming_event, reclaiming(NOW, Some("g:5:3"))),
        (reclaiming_bare, reclaiming(NOW, None)),
        (unreadable, unreadable_value.clone()),
    ];
    for (f, value) in &marks {
        servers[1].put(*f);
        meta.seed(orphan_key(1, *f), value.clone());
    }
    servers[1].put(unmarked);
    let fleet = fleet_of(&servers);
    let audit = Capture::default();
    let unreadable_key = orphan_key(1, unreadable);

    // The post-restore pass: every shape is a mark, and none is touched.
    let report = restore(&meta, &fleet, NOW)
        .with_subscriber(audit.dispatch())
        .await;
    for (f, value) in &marks {
        assert_eq!(
            meta.value(&orphan_key(1, *f)).as_ref(),
            Some(value),
            "leg G: the post-restore pass rewrote the mark on {f:?}"
        );
    }
    assert_eq!(
        (report.already_marked, report.stranded_marked),
        (marks.len(), 1),
        "leg G: every value, of whichever shape, is already a mark; only the unmarked fragment \
         is marked: {report:?}"
    );

    // GC at `NOW`: each shape's meaning.
    let outcome = gc(&meta, &fleet, NOW)
        .with_subscriber(audit.dispatch())
        .await;
    for (f, shape) in [
        (legacy_old, "legacy, past grace"),
        (structured_old, "structured, past grace"),
        (reclaiming_event, "reclaiming, with an event"),
        (reclaiming_bare, "reclaiming, without one"),
    ] {
        assert!(
            !servers[1].holds(f) && meta.value(&orphan_key(1, f)).is_none(),
            "leg G: the {shape} mark on {f:?} did not license its reclaim"
        );
    }
    for (f, shape) in [
        (legacy_new, "legacy"),
        (structured_new, "structured"),
        (unmarked, "restore's fresh"),
    ] {
        assert!(
            servers[1].holds(f),
            "leg G: the {shape} mark on {f:?} is inside its grace window, and it was reclaimed"
        );
    }
    assert!(
        outcome.is_ok(),
        "leg G: the GC pass failed: {:?}",
        outcome.err()
    );

    // Long past every stamp: what remains is reclaimed, except the value no writer spells.
    let later = gc(&meta, &fleet, LATER).await;
    assert!(
        later.is_ok(),
        "leg G: the later pass failed: {:?}",
        later.err()
    );
    for f in [legacy_new, structured_new, unmarked] {
        assert!(!servers[1].holds(f), "leg G: {f:?} survived past its grace");
    }
    assert!(
        servers[1].holds(unreadable),
        "leg G: a fragment whose mark is none of the three shapes was reclaimed"
    );
    assert_eq!(
        meta.value(&unreadable_key),
        Some(unreadable_value),
        "leg G: the value no writer spells was rewritten or deleted"
    );
    assert!(
        audit.named_unreadable("wyrd.custodian.gc.audit", &unreadable_key),
        "leg G: GC must name the value no writer spells on its audit seam"
    );
    assert!(
        audit.named_unreadable("wyrd.custodian.restore.audit", &unreadable_key),
        "leg G: the post-restore pass must name the value no writer spells on its audit seam"
    );
}

// ---- containment: a staged record that cannot be read is never skipped (ADR-0045 decision 3) ----

/// A session whose committed part record will not decode: the chunks it holds are unknown, so it
/// may be holding any fragment in the fleet. While it is unread GC reclaims nothing — not even an
/// unrelated fragment marked past grace — and answers `Blocked`, and the post-restore pass marks
/// nothing and names the record for a human. Once the record is repaired the same fragment is
/// reclaimed: the containment waits for the repair, it does not outlast it. On the base the
/// record is never read, and the unrelated fragment is reclaimed at once.
#[tokio::test]
async fn containment_an_unreadable_staged_record_withholds_every_reclaim_and_mark() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(2, &log);
    let session = upload(5);
    meta.seed(mpu_key(&session), open_session_bytes());
    let record = part_key(&session, part(1));
    meta.seed(record.clone(), r#"{"chunks":"torn"}"#);
    servers[1].put(CONTROL);
    meta.seed(orphan_key(1, CONTROL), legacy(0));
    let stray = frag(1_200, 0);
    servers[1].put(stray);
    let fleet = fleet_of(&servers);

    let blocked = gc(&meta, &fleet, NOW).await;
    assert!(
        servers[1].holds(CONTROL),
        "containment: a fragment was reclaimed while a staged record whose chunks are unknown \
         is unread — it may be one of them"
    );
    assert_eq!(
        blocked,
        Ok(Reconciled::Blocked),
        "containment: a pass over an incomplete reference set certifies nothing"
    );

    let report = restore(&meta, &fleet, NOW).await;
    assert_eq!(
        (report.stranded_marked, meta.value(&orphan_key(1, stray))),
        (0, None),
        "containment: the post-restore pass marked a fragment while a staged record is unread: \
         {report:?}"
    );
    let name = String::from_utf8(record.clone()).unwrap();
    assert!(
        report.unresolvable.contains(&name) && report.needs_human(),
        "containment: the unread record must be named for a human: {report:?}"
    );

    meta.seed(record, part_bytes(&[chunk_json(1_201, NONE_JSON, &[1])]));
    let repaired = gc(&meta, &fleet, NOW).await;
    assert!(
        repaired.is_ok() && !servers[1].holds(CONTROL),
        "containment: once the record is repaired the unrelated fragment is reclaimed \
         ({repaired:?})"
    );
}

/// Staged records whose placement cannot be trusted but whose chunk is known: a committed part
/// whose Reed-Solomon chunk names one D server for three fragments (the decoder reads it —
/// placement length is liberal on read — so the maintenance pass must judge it), and an owned
/// entry whose value is an ordinary lease rather than an owned one (its decoder refuses the value,
/// but the key still names the chunk). Every fragment of both chunks, on every server, is held,
/// each marked past grace; an unrelated fragment marked past grace is still reclaimed, since
/// nothing here hides which chunks the records own. On the base both chunks are reclaimed.
#[tokio::test]
async fn containment_a_staged_chunk_with_an_untrusted_placement_is_held_whole() {
    permissive_tracing();
    let log = Log::default();
    let meta = Meta::new(&log);
    let servers = servers(4, &log);
    let session = upload(6);
    meta.seed(mpu_key(&session), open_session_bytes());
    meta.seed(
        part_key(&session, part(1)),
        part_bytes(&[chunk_json(1_300, RS21_JSON, &[1])]),
    );
    let torn_entry = sidx_key(&session, part(2), 1_301);
    meta.seed(
        torn_entry.clone(),
        metadata::encode(&metadata::PendingEntry {
            lease_expiry_millis: LEASE,
            owner: None,
            staged: None,
        }),
    );
    assert!(
        decode_owned_entry(&torn_entry, &meta.value(&torn_entry).unwrap()).is_err(),
        "fixture: the owned-entry decoder refuses an ordinary lease under `sidx:`"
    );
    let held: Vec<(DServerId, FragmentId)> = (0..3u16)
        .flat_map(|index| {
            [
                (DServerId::from(index) + 1, frag(1_300, index)),
                (3 - DServerId::from(index), frag(1_301, index)),
            ]
        })
        .collect();
    for &(dserver, f) in &held {
        servers[dserver as usize].put(f);
        meta.seed(orphan_key(dserver, f), legacy(0));
    }
    servers[1].put(CONTROL);
    meta.seed(orphan_key(1, CONTROL), legacy(0));
    let fleet = fleet_of(&servers);

    let outcome = gc(&meta, &fleet, NOW).await;
    for &(dserver, f) in &held {
        assert!(
            servers[dserver as usize].holds(f),
            "containment: {f:?} on D server {dserver} belongs to a staged chunk whose placement \
             cannot be trusted, and was reclaimed"
        );
    }
    assert!(
        !servers[1].holds(CONTROL),
        "containment: a known chunk held whole blocks nothing else — the unrelated fragment is \
         reclaimed"
    );
    assert_eq!(
        outcome,
        Ok(Reconciled::Changed),
        "containment: the set is complete, so the pass is not blocked"
    );
}
