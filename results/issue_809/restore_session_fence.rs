//! Issue #809 (child 2 of #664's split) — the post-restore pass **fences every upload session a
//! restore resurrected**, and says what it skipped or could not fence (proposal 0016 decision 1.4
//! and D-B, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:717-728`; the restore row
//! of its decision-2 table, `:823`; the X57 row of its failure table, `:880`; the fence rows of its
//! batch table, `:660`, `:664-665`).
//!
//! A metadata restore rewinds the store to an image whose upload sessions may still be `Open` or
//! `Completing` while the bytes they staged are gone. Left alone, such a session can still be
//! completed, and publish an object over reclaimed bytes. And a `Completing` session that had
//! already written its segment records needs those records retired in the **same** batch as its
//! fence, or nothing anywhere in the design ever deletes them (X57).
//!
//! Every leg drives the production `reconcile_after_restore` over in-memory doubles. No client
//! creates a session before the S3 verbs (#508), so every multipart record is seeded as raw JSON.
//! A `Completing` fixture carries the segment-group nonce this slice adds to the session record
//! (`publish_target.segment_nonce`); the base decoder refuses that field
//! (`#[serde(deny_unknown_fields)]`), which is harmless there because base restore never reads
//! `mpu:`. The file names only symbols the base already has: what the fix adds to
//! `RestoreReport` is asserted through the report's `Debug` rendering.
//!
//! The legs:
//! - **E** staged skips are counted, apart from `pending_skipped` (`staged_skipped: 2`).
//! - **F** a resurrected `Open@E` session ends `Aborting@E+1`, its byte-retirement obligation
//!   installed in the same commit (and never without it, when that commit fails), and a Complete
//!   retried against it cannot fence it.
//! - **G** a resurrected `Completing@E` session that wrote segments ends `Aborting@E+1`, and the
//!   one batch installs `retire:bytes` naming the session and its parts **and** `retire:records`
//!   naming exactly its `seg:<nonce>:<E>` group.
//! - **H(i)** a `Completing` record with no nonce fails decode: left byte-identical, and named as
//!   needing a human. **H(ii)** a `Completing` session whose segment records name a chunk none of
//!   its parts holds is fenced, and still named as needing a human. The fence's other two ways of
//!   not landing are named too, having written nothing: a fence that loses its compare-and-set to
//!   a concurrent writer (and lands on the next run), and a session whose epoch has no successor.
//! - **H(iii)** an untrusted staged record is named in the report, needs no human, and is not a
//!   clean result — on the pass that fences its session and on the one after.
//! - **K** a second pass installs nothing and fences nothing, and still names the case-H sessions;
//!   an already-fenced session whose obligation will not decode is named as well.
//!
//! Base: every leg fails by assertion. The base pass reads no `mpu:` value and writes no fence,
//! and its report has no `staged_skipped`, `sessions_fenced` or `staged_untrusted`.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::ops::Bound;
use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;

use async_trait::async_trait;
use bytes::Bytes;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use wyrd_core::metadata::{
    self, orphan_key, pending_key, seg_key, ChunkRef, EcScheme, SegmentGroup, SegmentRecord,
};
use wyrd_core::multipart::{
    decode_part_record, decode_retire_obligation, decode_session_record, mpu_key, part_key,
    retire_key, retire_session_range, sidx_key, OwnedEntry, PartNumber, PartScope, RetireMode,
    RetirePayload, RetireToken, SessionState, StagedPlacement, UploadId,
};
use wyrd_custodian::{reconcile_after_restore, ExpiredPendingPolicy, GcContext, RestoreReport};
use wyrd_traits::{
    page_cursor, page_limit, page_start, BoxError, ChunkId, ChunkStore, CommitOutcome, DServerId,
    FragmentId, Health, MetadataStore, PageStart, Result, ScanCapExceeded, ScanPage, WriteBatch,
    SCAN_CAP,
};

/// The reader-safe grace window every pass here runs with.
const GRACE: u64 = 50;
/// The instant every pass runs at.
const NOW: u64 = 10_000;
/// The bucket and object every seeded session targets.
const PARENT: u64 = 42;
const OBJECT: &str = "fenced/object";
/// Every seeded session's epoch, `E`.
const EPOCH: u64 = 3;
/// The session fields a fence must carry over unchanged. None is a default: a fence that rebuilt
/// the record from defaults would fail every one of them.
const CREATED_AT: u64 = 100;
const CLOCK: &str = "wall";
const ATTEMPTS: u32 = 2;
/// When a `Completing` fixture's Complete fence landed.
const FENCED_AT: u64 = 900;
/// A lease far past every pass's clock, so no leg turns on one expiring.
const LEASE: u64 = NOW * 1_000;
/// What the metadata double answers a commit it was armed to fail.
const INJECTED_COMMIT_FAULT: &str = "injected metadata-store commit fault";

const RS_2_1: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };

// ---- the metadata double ------------------------------------------------------------------------

/// An in-memory `MetadataStore` over an ordered map: preconditions checked atomically, a scan cap
/// (`scan` refuses past it and `scan_page` clamps a page to it, through the seam's own
/// `page_limit` / `page_start` / `page_cursor`), a log of every applied commit's puts, and one
/// armable commit fault.
struct Meta {
    kv: Mutex<BTreeMap<Vec<u8>, Bytes>>,
    cap: usize,
    /// The keys each APPLIED commit put, in commit order.
    applied: Mutex<Vec<Vec<Vec<u8>>>>,
    /// Fail — with an `Err`, writing nothing — the first commit that puts this key.
    fail_put_of: Mutex<Option<Vec<u8>>>,
    /// The keys each commit the double failed would have put.
    refused: Mutex<Vec<Vec<Vec<u8>>>>,
    /// A concurrent writer: its batch lands, on its own preconditions, right before the first
    /// commit that puts this key — between the pass's read and the pass's own commit.
    race: Mutex<Option<(Vec<u8>, WriteBatch)>>,
    /// How the concurrent writer's batch came out, once it has landed.
    raced: Mutex<Option<CommitOutcome>>,
}

impl Meta {
    fn new() -> Self {
        Self::with_cap(SCAN_CAP)
    }

    fn with_cap(cap: usize) -> Self {
        Self {
            kv: Mutex::new(BTreeMap::new()),
            cap,
            applied: Mutex::new(Vec::new()),
            fail_put_of: Mutex::new(None),
            refused: Mutex::new(Vec::new()),
            race: Mutex::new(None),
            raced: Mutex::new(None),
        }
    }

    /// Put a fixture record in place — not a pass's write.
    fn seed(&self, key: impl Into<Vec<u8>>, value: impl Into<Bytes>) {
        self.kv.lock().unwrap().insert(key.into(), value.into());
    }

    fn value(&self, key: &[u8]) -> Option<Bytes> {
        self.kv.lock().unwrap().get(key).cloned()
    }

    fn holds(&self, key: &[u8]) -> bool {
        self.kv.lock().unwrap().contains_key(key)
    }

    fn keys_under(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
        self.kv
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Every record the store holds, keys and values — to compare one pass's store with the next.
    fn snapshot(&self) -> BTreeMap<Vec<u8>, Bytes> {
        self.kv.lock().unwrap().clone()
    }

    fn fail_commit_putting(&self, key: Vec<u8>) {
        *self.fail_put_of.lock().unwrap() = Some(key);
    }

    /// Land `batch` right before the first commit that puts `key`, as a concurrent writer would.
    fn race_before_commit_putting(&self, key: Vec<u8>, batch: WriteBatch) {
        *self.race.lock().unwrap() = Some((key, batch));
    }

    /// How the concurrent writer's batch came out — `None` if no commit ever triggered it.
    fn race_outcome(&self) -> Option<CommitOutcome> {
        *self.raced.lock().unwrap()
    }

    /// Apply `batch` atomically: every precondition holds, or nothing changes.
    fn apply(&self, batch: WriteBatch) -> CommitOutcome {
        let puts: Vec<Vec<u8>> = batch.puts.iter().map(|(key, _)| key.clone()).collect();
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
        self.applied.lock().unwrap().push(puts);
        CommitOutcome::Committed
    }

    /// The keys the applied commit that put `key` put, if one did.
    fn applied_commit_putting(&self, key: &[u8]) -> Option<Vec<Vec<u8>>> {
        self.applied
            .lock()
            .unwrap()
            .iter()
            .find(|puts| puts.iter().any(|put| put.as_slice() == key))
            .cloned()
    }

    /// Whether the double failed a commit that would have put `key`.
    fn refused_a_commit_putting(&self, key: &[u8]) -> bool {
        self.refused
            .lock()
            .unwrap()
            .iter()
            .any(|puts| puts.iter().any(|put| put.as_slice() == key))
    }
}

#[async_trait]
impl MetadataStore for Meta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        let hits: Vec<(Vec<u8>, Bytes)> = self
            .kv
            .lock()
            .unwrap()
            .range::<[u8], _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if hits.len() > self.cap {
            return Err(BoxError::from(ScanCapExceeded {
                cap: self.cap,
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
        let limit = page_limit(limit, self.cap, prefix)?;
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
        let puts: Vec<Vec<u8>> = batch.puts.iter().map(|(key, _)| key.clone()).collect();
        {
            let mut armed = self.fail_put_of.lock().unwrap();
            if armed.as_ref().is_some_and(|key| puts.contains(key)) {
                armed.take();
                self.refused.lock().unwrap().push(puts);
                return Err(BoxError::from(INJECTED_COMMIT_FAULT));
            }
        }
        let racer = {
            let mut race = self.race.lock().unwrap();
            if race.as_ref().is_some_and(|(key, _)| puts.contains(key)) {
                race.take().map(|(_, batch)| batch)
            } else {
                None
            }
        };
        if let Some(racer) = racer {
            let outcome = self.apply(racer);
            *self.raced.lock().unwrap() = Some(outcome);
        }
        Ok(self.apply(batch))
    }
}

// ---- the D-server double and the fleet ----------------------------------------------------------

/// One D server's fragments. The post-restore pass lists them and never reads their bytes.
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

fn frag(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

fn place(d: &[Disk; 4], dserver: DServerId, frag: FragmentId) {
    d[dserver as usize]
        .frags
        .lock()
        .unwrap()
        .insert(frag, Bytes::from_static(b"staged"));
}

fn on_disk(d: &[Disk; 4], dserver: DServerId, frag: FragmentId) -> bool {
    d[dserver as usize]
        .frags
        .lock()
        .unwrap()
        .contains_key(&frag)
}

/// One post-restore pass at [`NOW`] over the whole four-server fleet.
async fn restore_pass(meta: &Meta, d: &[Disk; 4]) -> Result<RestoreReport> {
    let fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d[0]), (1, &d[1]), (2, &d[2]), (3, &d[3])];
    let ctx = GcContext {
        meta,
        fleet: &fleet,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_after_restore(&ctx, NOW).await
}

// ---- the records --------------------------------------------------------------------------------

/// An upload id: 32 lowercase-hex characters from a 2-character hex pair. Every leg uses its own.
fn upload(pair: &str) -> UploadId {
    UploadId::new(pair.repeat(16)).expect("32 lowercase-hex characters")
}

/// A segment-group nonce: 32 lowercase-hex characters from a 2-character hex pair.
fn nonce(pair: &str) -> String {
    pair.repeat(16)
}

fn part_no(n: u32) -> PartNumber {
    PartNumber::new(n).expect("a part number in range")
}

/// A key as the report names it: `gc::object_name` leaves an ASCII key unchanged.
fn name(key: &[u8]) -> String {
    String::from_utf8(key.to_vec()).expect("every key here is ASCII")
}

/// A session record at `epoch` in the state `state_json` spells, in the codec's own field order.
fn session_at(epoch: u64, state_json: &str) -> Bytes {
    Bytes::from(format!(
        "{{\"parent\":{PARENT},\"object\":\"{OBJECT}\",\"created_at_millis\":{CREATED_AT},\
         \"clock_source\":\"{CLOCK}\",\"epoch\":{epoch},\"attempts\":{ATTEMPTS},\"state\":{state_json}}}"
    ))
}

/// An `Open@E` session — a shape the base decoder accepts, checked here so a leg can never be red
/// over its own fixture.
fn open_session() -> Bytes {
    let bytes = session_at(EPOCH, "{\"kind\":\"Open\"}");
    let record = decode_session_record(&bytes)
        .unwrap_or_else(|fault| panic!("the seeded Open session must decode: {fault}"));
    assert_eq!(
        metadata::encode(&record),
        bytes,
        "the seeded Open session must be the decoder's own spelling"
    );
    bytes
}

/// A `Completing@epoch` session that has written `segments_written` segment records, carrying
/// `segment_nonce` in its publish target — or, for `None`, the pre-decision shape with no nonce.
///
/// Not decoded here: the base decoder refuses the nonce field, and this leg's red must come from
/// the pass, not from its fixture. The fence decodes it with the fix applied — and a fixture that
/// were not the codec's own spelling would fail decode there, and land in H(i)'s path instead.
fn completing_session_at(epoch: u64, segments_written: u32, segment_nonce: Option<&str>) -> Bytes {
    let nonce_field = match segment_nonce {
        Some(nonce) => format!(",\"segment_nonce\":\"{nonce}\""),
        None => String::new(),
    };
    session_at(
        epoch,
        &format!(
            "{{\"kind\":\"Completing\",\"fenced_at_millis\":{FENCED_AT},\
             \"segments_written\":{segments_written},\"publish_target\":{{\"parent\":{PARENT},\
             \"name\":\"{OBJECT}\",\"epoch\":{epoch}{nonce_field}}}}}"
        ),
    )
}

fn chunk_ref(id: ChunkId, scheme: EcScheme, placement: &[DServerId]) -> ChunkRef {
    ChunkRef {
        id,
        scheme,
        len: 5,
        placement: placement.to_vec(),
    }
}

fn chunk_json(chunk: &ChunkRef) -> String {
    String::from_utf8(metadata::encode(chunk).to_vec()).expect("a chunk ref encodes as JSON")
}

/// A committed part record naming `chunks`, round-tripped through the base decoder. A placement's
/// length is not a decode-time rule, so a wrong-length one decodes too.
fn part_record(chunks: &[ChunkRef]) -> Bytes {
    let refs: Vec<String> = chunks.iter().map(chunk_json).collect();
    let len: u64 = chunks.iter().map(|chunk| chunk.len).sum();
    let bytes = Bytes::from(format!(
        "{{\"chunks\":[{}],\"len\":{len},\"digest\":\"{}\",\"committed_at_millis\":800,\
         \"session_epoch\":{EPOCH}}}",
        refs.join(","),
        "ef".repeat(32)
    ));
    let record = decode_part_record(&bytes)
        .unwrap_or_else(|fault| panic!("the seeded part record must decode: {fault}"));
    assert_eq!(metadata::encode(&record), bytes);
    bytes
}

/// One segment record naming `chunks` from `byte_offset` on, round-tripped through the base
/// decoder.
fn segment_record(chunks: &[ChunkRef], byte_offset: u64) -> Bytes {
    let refs: Vec<String> = chunks.iter().map(chunk_json).collect();
    let len: u64 = chunks.iter().map(|chunk| chunk.len).sum();
    let bytes = Bytes::from(format!(
        "{{\"chunks\":[{}],\"byte_offset\":{byte_offset},\"byte_len\":{len}}}",
        refs.join(",")
    ));
    let record: SegmentRecord = metadata::decode(&bytes)
        .unwrap_or_else(|fault| panic!("the seeded segment record must decode: {fault}"));
    assert_eq!(metadata::encode(&record), bytes);
    bytes
}

/// An owned staging entry of `owner`, its chunk planned as `placement` under `scheme`. The chunk
/// is the one the `sidx:` key it is filed under names.
fn owned_entry(owner: &UploadId, scheme: EcScheme, placement: &[DServerId]) -> Bytes {
    let staged = StagedPlacement::new(scheme, placement.to_vec()).expect("a supported scheme");
    metadata::encode(&OwnedEntry::new(owner.clone(), LEASE, staged).to_pending())
}

/// An ordinary streaming write's lease, far from expiry.
fn lease() -> Bytes {
    Bytes::from(format!("{{\"lease_expiry_millis\":{LEASE}}}"))
}

/// The session-wide retirement key of `id`'s fence from `epoch`, in `mode`.
fn fence_key(mode: RetireMode, id: &UploadId, epoch: u64) -> Vec<u8> {
    retire_key(
        mode,
        &RetireToken::Session {
            upload_id: id.clone(),
            epoch,
            part: None,
        },
    )
}

/// Every `retire:` key under `id`'s own session-scoped ranges, both modes.
fn session_obligations(meta: &Meta, id: &UploadId) -> Vec<Vec<u8>> {
    RetireMode::ALL
        .into_iter()
        .flat_map(|mode| meta.keys_under(&retire_session_range(mode, id)))
        .collect()
}

/// Decode the obligation under `key` against that key, failing the leg if it is absent or refused.
fn obligation(meta: &Meta, key: &[u8]) -> (RetireMode, RetireToken, RetirePayload) {
    let value = meta
        .value(key)
        .unwrap_or_else(|| panic!("no obligation under {}", name(key)));
    decode_retire_obligation(key, &value).unwrap_or_else(|fault| {
        panic!(
            "the obligation under {} does not decode against its own key: {fault}",
            name(key)
        )
    })
}

/// Whether `payload` owes part `n`: named in its explicit set, or covered by the `all` wildcard.
fn owes_part(payload: &RetirePayload, n: u32) -> bool {
    match payload.parts() {
        Some(PartScope::All) => true,
        Some(PartScope::Set(set)) => set.runs().iter().any(|&(lo, hi)| lo <= n && n <= hi),
        None => false,
    }
}

/// Assert `id`'s session record is `Aborting@E+1` with every other field carried over.
fn assert_fenced(meta: &Meta, id: &UploadId) {
    let key = mpu_key(id);
    let value = meta.value(&key).expect("the session record is still there");
    let record = decode_session_record(&value).unwrap_or_else(|fault| {
        panic!(
            "the session {} does not decode after the pass — it was not fenced: {fault}",
            name(&key)
        )
    });
    assert_eq!(
        (record.state(), record.epoch()),
        (&SessionState::Aborting {}, EPOCH + 1),
        "the resurrected session {} was not fenced to Aborting@E+1",
        name(&key)
    );
    assert_eq!(
        (
            record.parent(),
            record.object(),
            record.content_type(),
            record.created_at_millis(),
            record.clock_source(),
            record.attempts(),
        ),
        (PARENT, OBJECT, None, CREATED_AT, CLOCK, ATTEMPTS),
        "the fence must change the state and the epoch, nothing else"
    );
}

/// The `[...]` list the report's `Debug` rendering gives `field`, or `None` when it has no such
/// field — how this file reads a field the fix adds without naming it.
fn debug_list<'r>(rendered: &'r str, field: &str) -> Option<&'r str> {
    let open = format!("{field}: [");
    let start = rendered.find(&open)? + open.len();
    let len = rendered[start..].find(']')?;
    Some(&rendered[start..start + len])
}

// ---- the audit seam -----------------------------------------------------------------------------

/// One custodian audit event, as the thread that emitted it saw it.
struct AuditEvent {
    thread: ThreadId,
    target: String,
    fields: Vec<String>,
}

fn audit_log() -> &'static Mutex<Vec<AuditEvent>> {
    static LOG: OnceLock<Mutex<Vec<AuditEvent>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::new()))
}

/// Install the capture once for the whole test binary, before any pass on any thread runs, so no
/// audit callsite is ever first met with no subscriber in place. Every leg calls it first.
fn capture_audit() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        tracing_subscriber::registry()
            .with(AuditCapture)
            .try_init()
            .expect("this test binary installs the only global subscriber");
    });
}

struct AuditCapture;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for AuditCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let target = event.metadata().target();
        if !target.starts_with("wyrd.custodian.") {
            return;
        }
        let mut fields = FieldText(Vec::new());
        event.record(&mut fields);
        audit_log().lock().unwrap().push(AuditEvent {
            thread: std::thread::current().id(),
            target: target.to_owned(),
            fields: fields.0,
        });
    }
}

struct FieldText(Vec<String>);

impl Visit for FieldText {
    fn record_str(&mut self, _field: &Field, value: &str) {
        self.0.push(value.to_owned());
    }

    fn record_debug(&mut self, _field: &Field, value: &dyn Debug) {
        self.0.push(format!("{value:?}"));
    }
}

const RESTORE_AUDIT: &str = "wyrd.custodian.restore.audit";

/// Whether a pass on this thread emitted, on the restore audit seam, an event carrying every one
/// of `needles` among its fields.
fn on_restore_audit_seam(needles: &[&str]) -> bool {
    let thread = std::thread::current().id();
    audit_log().lock().unwrap().iter().any(|event| {
        event.thread == thread
            && event.target == RESTORE_AUDIT
            && needles
                .iter()
                .all(|needle| event.fields.iter().any(|field| field.contains(needle)))
    })
}

// ---- (E) staged skips are counted, apart from pending ones --------------------------------------

/// **(E)** Two staged fragments — one a committed part names, one an in-flight owned staging entry
/// names — and a third held by an ordinary write's `pending:` lease. The report counts the first
/// two as staged-skipped and the third as pending-skipped: two different reasons a fragment was
/// left unmarked, never folded into one count (`0016:823`).
///
/// Base: the report has no staged counter, so its rendering has no `staged_skipped: 2`.
#[tokio::test]
async fn e_restore_counts_staged_skips_apart_from_pending_skips() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("e1");
    meta.seed(mpu_key(&id), open_session());
    let part_chunk: ChunkId = 0xE01;
    meta.seed(
        part_key(&id, part_no(1)),
        part_record(&[chunk_ref(part_chunk, EcScheme::None, &[0])]),
    );
    let owned_chunk: ChunkId = 0xE02;
    meta.seed(
        sidx_key(&id, part_no(2), owned_chunk),
        owned_entry(&id, EcScheme::None, &[1]),
    );
    let leased: ChunkId = 0xE03;
    meta.seed(pending_key(leased), lease());
    let fragments = [
        (0, frag(part_chunk, 0)),
        (1, frag(owned_chunk, 0)),
        (2, frag(leased, 0)),
    ];
    for (dserver, fragment) in fragments {
        place(&d, dserver, fragment);
    }

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert!(
        rendered.contains("staged_skipped: 2"),
        "the two staged fragments were not reported as staged-skipped: {rendered}"
    );
    assert!(
        rendered.contains("pending_skipped: 1"),
        "the leased fragment must be counted as pending-skipped, and only it: {rendered}"
    );
    assert_eq!(report.stranded_marked, 0, "{rendered}");
    for (dserver, fragment) in fragments {
        assert!(
            !meta.holds(&orphan_key(dserver, fragment)),
            "{fragment:?} on server {dserver} was marked: {rendered}"
        );
    }
}

// ---- (F) a resurrected Open session is fenced, in one batch -------------------------------------

/// The keys an `Open@E` session with one part places, for leg F's two tests.
fn seed_open_session_with_a_part(meta: &Meta, d: &[Disk; 4], id: &UploadId, chunk: ChunkId) {
    meta.seed(mpu_key(id), open_session());
    meta.seed(
        part_key(id, part_no(1)),
        part_record(&[chunk_ref(chunk, EcScheme::None, &[0])]),
    );
    place(d, 0, frag(chunk, 0));
}

/// **(F)** An `Open@E` session with one committed part ends `Aborting@E+1`, and the commit that
/// fenced it also installed its byte-retirement obligation `retire:bytes:s:<id>:<E>` — the
/// session's staged residue and its parts, the part included, so nothing the session wrote is
/// left without a deleter. Every obligation the pass wrote decodes through
/// `decode_retire_obligation` against the key it sits under, the report counts one fenced
/// session, and the pass deleted nothing itself.
///
/// A Complete retried against the session afterwards cannot fence it: the Complete fence requires
/// `Open@E` (`0016:660`), and the session record no longer holds those bytes. (The client-visible
/// `4xx` is #658's.)
///
/// Base: the session is still `Open@E`, no `retire:` key exists, and the retried Complete fences
/// it.
#[tokio::test]
async fn f_restore_fences_a_resurrected_open_session() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("f1");
    let chunk: ChunkId = 0xF01;
    seed_open_session_with_a_part(&meta, &d, &id, chunk);
    let open = meta.value(&mpu_key(&id)).expect("seeded");

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert_fenced(&meta, &id);
    let bytes_key = fence_key(RetireMode::Bytes, &id, EPOCH);
    let (mode, token, payload) = obligation(&meta, &bytes_key);
    assert_eq!(mode, RetireMode::Bytes);
    assert_eq!(
        token,
        RetireToken::Session {
            upload_id: id.clone(),
            epoch: EPOCH,
            part: None,
        }
    );
    assert!(
        payload.session() && owes_part(&payload, 1),
        "the Open fence's obligation must owe the session's staged residue and its part: \
         {payload:?}"
    );
    let written = meta.keys_under(b"retire:");
    assert_eq!(
        written,
        vec![bytes_key.clone()],
        "the Open fence installs exactly one obligation"
    );
    for key in &written {
        obligation(&meta, key);
    }
    let fence = meta
        .applied_commit_putting(&mpu_key(&id))
        .expect("a commit wrote the session record");
    assert!(
        fence.contains(&bytes_key),
        "the session was fenced in one commit and its obligation installed in another: {:?}",
        fence.iter().map(|key| name(key)).collect::<Vec<_>>()
    );
    assert!(
        rendered.contains("sessions_fenced: 1"),
        "the report does not count the fenced session: {rendered}"
    );
    assert!(
        !report.needs_human(),
        "a clean fence needs no human: {rendered}"
    );
    // The fence retires through the obligation; it deletes and marks nothing itself.
    assert!(meta.holds(&part_key(&id, part_no(1))));
    assert!(on_disk(&d, 0, frag(chunk, 0)));
    assert!(!meta.holds(&orphan_key(0, frag(chunk, 0))));

    // The Complete fence as the protocol writes it: `require(mpu == Open@E)`, then `Completing@E+1`.
    let retried = meta
        .commit(WriteBatch::new().require(mpu_key(&id), open).put(
            mpu_key(&id),
            completing_session_at(EPOCH + 1, 0, Some(&nonce("5e"))),
        ))
        .await
        .expect("the store answers the retried Complete fence");
    assert_eq!(
        retried,
        CommitOutcome::Conflict,
        "a Complete retried against the restored session fenced it — it could now publish over \
         bytes the restore rewound past"
    );
}

/// **(F)** The fence is **one** batch: a double that fails the commit writing the session record —
/// or, in the second run, the commit writing its obligation — leaves **none** of the fence's
/// writes behind. Either run alone would let one split shape through (obligation first, or
/// session first); together they catch both. The failure is a store fault, so the pass fails
/// rather than report a fence that did not land.
///
/// Base: no fence commit is ever attempted.
#[tokio::test]
async fn f_the_open_fence_lands_whole_or_not_at_all() {
    capture_audit();
    for (pair, fail_session_put) in [("f2", true), ("f3", false)] {
        let meta = Meta::new();
        let d = disks();
        let id = upload(pair);
        seed_open_session_with_a_part(&meta, &d, &id, 0xF02);
        let open = meta.value(&mpu_key(&id)).expect("seeded");
        let failed = if fail_session_put {
            mpu_key(&id)
        } else {
            fence_key(RetireMode::Bytes, &id, EPOCH)
        };
        meta.fail_commit_putting(failed.clone());

        let result = restore_pass(&meta, &d).await;

        assert!(
            meta.refused_a_commit_putting(&failed),
            "no commit ever tried to write {} — the pass did not fence the session",
            name(&failed)
        );
        assert!(
            result.is_err(),
            "a store fault under the fence's commit must fail the pass: {result:?}"
        );
        assert_eq!(
            meta.value(&mpu_key(&id)),
            Some(open),
            "the fence's commit failed, yet the session record changed (the {} put was failed)",
            name(&failed)
        );
        assert!(
            meta.keys_under(b"retire:").is_empty(),
            "the fence's commit failed, yet an obligation was installed (the {} put was failed)",
            name(&failed)
        );
    }
}

// ---- (G) a resurrected Completing session, and its segments' deleter ----------------------------

/// A `Completing@E` session that has written one segment record per part: part `n` names chunk
/// `chunks[n - 1]` and segment `n - 1` names the same chunk. Seeded under the segment group
/// `(segment_nonce, E)`, which the session record names.
fn seed_completing_session(
    meta: &Meta,
    d: &[Disk; 4],
    id: &UploadId,
    segment_nonce: &str,
    chunks: &[ChunkId],
) -> SegmentGroup {
    let segments_written = u32::try_from(chunks.len()).expect("a handful of segments");
    meta.seed(
        mpu_key(id),
        completing_session_at(EPOCH, segments_written, Some(segment_nonce)),
    );
    let group = SegmentGroup::new(segment_nonce, EPOCH).expect("a 32-hex nonce");
    for (n, &chunk) in (1..).zip(chunks) {
        let dserver = (n % 3) as DServerId;
        let chunk = chunk_ref(chunk, EcScheme::None, &[dserver]);
        meta.seed(
            part_key(id, part_no(n)),
            part_record(std::slice::from_ref(&chunk)),
        );
        place(d, dserver, frag(chunk.id, 0));
        meta.seed(
            seg_key(&group, n - 1).expect("an addressable index"),
            segment_record(&[chunk], u64::from(n - 1) * 5),
        );
    }
    group
}

/// **(G)** A `Completing@E` session that had already written its segment records, its nonce on
/// its record, ends `Aborting@E+1`. The one batch that fences it installs `retire:bytes` naming the
/// session and exactly its parts **and** `retire:records` naming exactly `seg:<nonce>:<E>`
/// (`0016:665`), both decoding against their keys; the records obligation's `segments()` is that
/// group, and it owes no part record — those are the bytes obligation's to delete after marking.
/// Fencing the session as if it were `Open` would leave those segment records with no deleter
/// anywhere (X57, `0016:880`).
///
/// The store's scan cap is 2, below the session's three parts and three segments, so the fence
/// must read both ranges in pages.
///
/// Base: the session is untouched and no obligation exists.
#[tokio::test]
async fn g_restore_fences_a_resurrected_completing_session_with_its_segments_deleter() {
    capture_audit();
    let meta = Meta::with_cap(2);
    let d = disks();
    let id = upload("c1");
    let group = seed_completing_session(&meta, &d, &id, &nonce("5e"), &[0xC01, 0xC02, 0xC03]);

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert_fenced(&meta, &id);
    let bytes_key = fence_key(RetireMode::Bytes, &id, EPOCH);
    let records_key = fence_key(RetireMode::Records, &id, EPOCH);
    let (mode, _, bytes) = obligation(&meta, &bytes_key);
    assert_eq!(mode, RetireMode::Bytes);
    assert!(bytes.session(), "{bytes:?}");
    match bytes.parts() {
        Some(PartScope::Set(set)) => assert_eq!(
            set.runs(),
            [(1, 3)],
            "the bytes obligation must name exactly the session's three parts"
        ),
        other => panic!("the Completing fence names its parts explicitly, got {other:?}"),
    }
    assert!(bytes.segments().is_none(), "{bytes:?}");

    let (mode, _, records) = obligation(&meta, &records_key);
    assert_eq!(mode, RetireMode::Records);
    assert_eq!(
        records.segments(),
        Some(&group),
        "the records obligation must name exactly seg:<nonce>:<E>"
    );
    assert!(
        records.parts().is_none() && !records.session(),
        "a records obligation owing parts would delete them unmarked: {records:?}"
    );

    let mut expected = vec![bytes_key.clone(), records_key.clone()];
    expected.sort();
    assert_eq!(session_obligations(&meta, &id), expected);
    let fence = meta
        .applied_commit_putting(&mpu_key(&id))
        .expect("a commit wrote the session record");
    assert!(
        fence.contains(&bytes_key) && fence.contains(&records_key),
        "the fence and its two obligations did not land in one commit: {:?}",
        fence.iter().map(|key| name(key)).collect::<Vec<_>>()
    );
    assert!(rendered.contains("sessions_fenced: 1"), "{rendered}");
    assert!(!report.needs_human(), "{rendered}");
    // Retired through the obligation, not deleted here.
    for index in 0..3 {
        assert!(meta.holds(&seg_key(&group, index).unwrap()));
    }
}

// ---- (H) what cannot be fenced cleanly is never passed off as done ------------------------------

/// **(H)(i)** A `Completing` record with **no** nonce — the pre-decision shape — fails decode. The
/// pass cannot tell which segment group it would retire, so it writes nothing for it: the record
/// stays byte-identical (ADR-0045 — structural damage is an error, never a value to act on), no
/// obligation is installed, and the session is named as needing a human.
///
/// Base: nothing names it, and `needs_human()` is false.
#[tokio::test]
async fn h_i_a_completing_record_without_its_nonce_is_left_untouched_and_named() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("d1");
    let pre_decision = completing_session_at(EPOCH, 1, None);
    meta.seed(mpu_key(&id), pre_decision.clone());
    meta.seed(
        part_key(&id, part_no(1)),
        part_record(&[chunk_ref(0xD01, EcScheme::None, &[0])]),
    );
    place(&d, 0, frag(0xD01, 0));

    let report = restore_pass(&meta, &d)
        .await
        .expect("an undecodable session is named, never an Err that blanks the report");
    let rendered = format!("{report:?}");

    assert_eq!(
        meta.value(&mpu_key(&id)),
        Some(pre_decision),
        "a session record the pass cannot decode must be left byte-identical"
    );
    assert!(
        session_obligations(&meta, &id).is_empty(),
        "an obligation was installed for a session the pass could not decode"
    );
    assert!(
        report.needs_human(),
        "a resurrected session the pass could not fence needs a human: {rendered}"
    );
    assert!(
        rendered.contains(&name(&mpu_key(&id))),
        "the session the pass could not fence is not named: {rendered}"
    );
}

/// **(H)(ii)** A `Completing` session whose segment records name a chunk none of its `part:`
/// records holds. It is still fenced — nothing may let it publish — with both of its obligations;
/// but those obligations do not account for that chunk (the records obligation deletes segment
/// records without marking, and the bytes obligation marks the parts'), so the session is named as
/// needing a human.
///
/// Base: the session is untouched, and nothing names it.
#[tokio::test]
async fn h_ii_a_segment_naming_a_chunk_no_part_holds_is_fenced_and_still_named() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("d2");
    seed_stray_segment_session(&meta, &d, &id, &nonce("6f"), 0xD21, 0xD2F);

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert_fenced(&meta, &id);
    let (_, _, bytes) = obligation(&meta, &fence_key(RetireMode::Bytes, &id, EPOCH));
    assert!(bytes.session() && owes_part(&bytes, 1), "{bytes:?}");
    let (_, _, records) = obligation(&meta, &fence_key(RetireMode::Records, &id, EPOCH));
    assert_eq!(
        records.segments(),
        Some(&SegmentGroup::new(nonce("6f"), EPOCH).unwrap())
    );
    assert!(
        report.needs_human(),
        "segment records naming a chunk no part holds need a human: {rendered}"
    );
    assert!(
        rendered.contains(&name(&mpu_key(&id))),
        "the session is not named: {rendered}"
    );
    assert!(rendered.contains("sessions_fenced: 1"), "{rendered}");
}

/// A `Completing@E` session with one part naming `part_chunk`, and one segment record naming both
/// `part_chunk` and `stray` — a chunk no part record of the session holds.
fn seed_stray_segment_session(
    meta: &Meta,
    d: &[Disk; 4],
    id: &UploadId,
    segment_nonce: &str,
    part_chunk: ChunkId,
    stray: ChunkId,
) {
    meta.seed(
        mpu_key(id),
        completing_session_at(EPOCH, 1, Some(segment_nonce)),
    );
    let owned = chunk_ref(part_chunk, EcScheme::None, &[1]);
    meta.seed(
        part_key(id, part_no(1)),
        part_record(std::slice::from_ref(&owned)),
    );
    place(d, 1, frag(part_chunk, 0));
    let group = SegmentGroup::new(segment_nonce, EPOCH).expect("a 32-hex nonce");
    meta.seed(
        seg_key(&group, 0).unwrap(),
        segment_record(&[owned, chunk_ref(stray, EcScheme::None, &[2])], 0),
    );
}

/// **(H)** A fence the store **refuses** writes nothing and is named. A concurrent Complete fence
/// lands between the pass's read of an `Open@E` session and the pass's own fence commit (the
/// runbook's writers-stopped rule broken), so the fence's `require` on the bytes it read fails.
/// Nothing of the fence is written — the session is the racer's `Completing@E+1` and no obligation
/// exists — and the session is named as needing a human rather than counted as fenced: a
/// `Conflict` is never read as a fence that landed. The next run reads the session again and
/// fences it, `Completing@E+1` to `Aborting@E+2`.
///
/// Base: the pass commits no fence, so the concurrent writer is never even triggered.
#[tokio::test]
async fn h_a_fence_that_loses_its_compare_and_set_writes_nothing_and_is_named() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("d3");
    seed_open_session_with_a_part(&meta, &d, &id, 0xD31);
    let open = meta.value(&mpu_key(&id)).expect("seeded");
    let completing = completing_session_at(EPOCH + 1, 0, Some(&nonce("9c")));
    meta.race_before_commit_putting(
        mpu_key(&id),
        WriteBatch::new()
            .require(mpu_key(&id), open)
            .put(mpu_key(&id), completing.clone()),
    );

    let report = restore_pass(&meta, &d)
        .await
        .expect("a refused fence is named, never an Err that blanks the report");
    let rendered = format!("{report:?}");

    assert_eq!(
        meta.race_outcome(),
        Some(CommitOutcome::Committed),
        "the concurrent Complete fence never landed, so the pass's fence was never refused"
    );
    assert_eq!(
        meta.value(&mpu_key(&id)),
        Some(completing),
        "the refused fence must have written nothing: the session is the concurrent writer's"
    );
    assert!(
        session_obligations(&meta, &id).is_empty(),
        "the refused fence installed an obligation"
    );
    assert!(
        report.needs_human(),
        "a session the fence could not fence needs a human: {rendered}"
    );
    assert!(
        rendered.contains(&name(&mpu_key(&id))) && rendered.contains("sessions_fenced: 0"),
        "the refused fence must be named, and not counted as a fence: {rendered}"
    );

    let rerun = restore_pass(&meta, &d)
        .await
        .expect("the next post-restore pass runs");
    let rendered = format!("{rerun:?}");
    assert!(rendered.contains("sessions_fenced: 1"), "{rendered}");
    assert!(!rerun.needs_human(), "{rendered}");
    let record = decode_session_record(&meta.value(&mpu_key(&id)).expect("present"))
        .expect("the fenced session decodes");
    assert_eq!(
        (record.state(), record.epoch()),
        (&SessionState::Aborting {}, EPOCH + 2)
    );
}

/// **(H)** A session whose epoch has **no successor** — a value no writer reaches, so only damage
/// produces it — cannot be fenced without wrapping to an epoch its retirement tokens were each
/// minted once for. It is left byte-identical, no obligation is installed, and it is named.
///
/// Base: nothing names it.
#[tokio::test]
async fn h_a_session_whose_epoch_has_no_successor_is_left_untouched_and_named() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("d4");
    let at_the_last_epoch = session_at(u64::MAX, "{\"kind\":\"Open\"}");
    decode_session_record(&at_the_last_epoch).expect("an Open session at the last epoch decodes");
    meta.seed(mpu_key(&id), at_the_last_epoch.clone());

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert_eq!(
        meta.value(&mpu_key(&id)),
        Some(at_the_last_epoch),
        "a session that cannot be fenced must be left byte-identical"
    );
    assert!(session_obligations(&meta, &id).is_empty());
    assert!(report.needs_human(), "{rendered}");
    assert!(rendered.contains(&name(&mpu_key(&id))), "{rendered}");
}

/// **(H)(iii)** An untrusted staged record is **reported, and needs no human** — the question #803
/// left at `restore.rs`'s `deferred: #664` marker, decided at #664's plan revision (2026-09-18).
///
/// Two sessions, both fenced by the first pass: session 1 holds one `part:` record whose chunk
/// placement names one D server for a three-fragment scheme; session 2 holds only trusted records.
/// On that pass and on the next (which fences nothing, so nothing but this record is left to keep
/// the run from being clean):
///
/// - (a) the report names session 1's record by key under `staged_untrusted`, and none of session
///   2's keys there — the discriminating arm;
/// - (b) `needs_human()` is false and `is_clean()` is false: once its session is fenced the
///   record's bytes are garbage whatever it says, so what is left is cleanup, not a judgement —
///   but a damaged record is not a clean result either;
/// - (c) no fragment of its chunk is marked, and the audit line still fires.
///
/// Base: the report has no `staged_untrusted`, and reads `is_clean()` true over the held record.
#[tokio::test]
async fn h_iii_an_untrusted_staged_record_is_named_and_needs_no_human() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();

    let one = upload("a1");
    meta.seed(mpu_key(&one), open_session());
    let held: ChunkId = 0xA01;
    let untrusted = part_key(&one, part_no(3));
    meta.seed(
        untrusted.clone(),
        part_record(&[chunk_ref(held, RS_2_1, &[0])]),
    );
    let held_frags = [(0, frag(held, 0)), (1, frag(held, 1)), (3, frag(held, 2))];
    for (dserver, fragment) in held_frags {
        place(&d, dserver, fragment);
    }

    let two = upload("a2");
    meta.seed(mpu_key(&two), open_session());
    let trusted_part = part_key(&two, part_no(1));
    meta.seed(
        trusted_part.clone(),
        part_record(&[chunk_ref(0xA02, EcScheme::None, &[1])]),
    );
    let trusted_owned = sidx_key(&two, part_no(2), 0xA03);
    meta.seed(
        trusted_owned.clone(),
        owned_entry(&two, EcScheme::None, &[2]),
    );
    place(&d, 1, frag(0xA02, 0));
    place(&d, 2, frag(0xA03, 0));

    for (pass, fenced) in [(1, 2), (2, 0)] {
        let report = restore_pass(&meta, &d)
            .await
            .expect("the post-restore pass runs");
        let rendered = format!("{report:?}");

        let named = debug_list(&rendered, "staged_untrusted").unwrap_or_else(|| {
            panic!("pass {pass}: the report names no untrusted staged record: {rendered}")
        });
        assert!(
            named.contains(&name(&untrusted)),
            "pass {pass}: the untrusted record {} is not named: {rendered}",
            name(&untrusted)
        );
        for key in [mpu_key(&two), trusted_part.clone(), trusted_owned.clone()] {
            assert!(
                !named.contains(&name(&key)),
                "pass {pass}: the trusted record {} is named as untrusted: {rendered}",
                name(&key)
            );
        }
        assert!(
            rendered.contains(&format!("sessions_fenced: {fenced}")),
            "pass {pass}: {rendered}"
        );
        assert!(
            !report.needs_human(),
            "pass {pass}: an untrusted staged record needs no human: {rendered}"
        );
        assert!(
            !report.is_clean(),
            "pass {pass}: a run that met an untrusted staged record is not a clean bill: {rendered}"
        );
        for (dserver, fragment) in held_frags {
            assert!(
                !meta.holds(&orphan_key(dserver, fragment)),
                "pass {pass}: {fragment:?} on server {dserver} was marked although an untrusted \
                 record names its chunk: {rendered}"
            );
        }
    }
    assert!(
        on_restore_audit_seam(&["untrusted-staged-record", &name(&untrusted)]),
        "the pass held the chunk without naming the record on its audit seam"
    );
}

// ---- (K) a second pass is idempotent, and still says what needs a human -------------------------

/// **(K)** Four resurrected sessions — a clean `Open` one, a clean `Completing` one, and the two
/// case-H sessions — through two passes. The first fences the three it can. The second installs no
/// second obligation and fences nothing (the store is exactly as the first pass left it), and it
/// **still** names both case-H sessions — the one it could not decode, and the one it fenced on the
/// first pass whose segment records name a chunk no part holds, which is `Aborting` by now:
/// "already `Aborting`" never means "nothing to report". The two clean sessions are named on
/// neither pass.
///
/// The store's scan cap is 2, so the session listing takes two pages.
///
/// Base: the first pass fences nothing.
#[tokio::test]
async fn k_a_second_pass_fences_nothing_again_and_still_names_what_needs_a_human() {
    capture_audit();
    let meta = Meta::with_cap(2);
    let d = disks();

    let open = upload("b1");
    seed_open_session_with_a_part(&meta, &d, &open, 0xB11);
    let completing = upload("b2");
    seed_completing_session(&meta, &d, &completing, &nonce("7a"), &[0xB21, 0xB22, 0xB23]);
    let undecodable = upload("b3");
    meta.seed(mpu_key(&undecodable), completing_session_at(EPOCH, 0, None));
    let stray = upload("b4");
    seed_stray_segment_session(&meta, &d, &stray, &nonce("8b"), 0xB41, 0xB4F);
    let named_on_every_pass = [mpu_key(&undecodable), mpu_key(&stray)];
    let never_named = [mpu_key(&open), mpu_key(&completing)];

    let first = restore_pass(&meta, &d)
        .await
        .expect("the first post-restore pass runs");
    let first_rendered = format!("{first:?}");
    assert!(
        first_rendered.contains("sessions_fenced: 3"),
        "the first pass must fence the three sessions it can: {first_rendered}"
    );
    for id in [&open, &completing, &stray] {
        assert_fenced(&meta, id);
    }
    let after_first = meta.snapshot();

    let second = restore_pass(&meta, &d)
        .await
        .expect("the second post-restore pass runs");
    let second_rendered = format!("{second:?}");
    assert!(
        second_rendered.contains("sessions_fenced: 0"),
        "the second pass fenced again: {second_rendered}"
    );
    assert!(
        meta.snapshot() == after_first,
        "the second pass wrote to the store — a second obligation, or a second fence"
    );

    for (pass, report, rendered) in [(1, &first, &first_rendered), (2, &second, &second_rendered)] {
        assert!(
            report.needs_human(),
            "pass {pass}: the case-H sessions need a human: {rendered}"
        );
        for key in &named_on_every_pass {
            assert!(
                rendered.contains(&name(key)),
                "pass {pass}: the session {} is no longer named: {rendered}",
                name(key)
            );
        }
        for key in &never_named {
            assert!(
                !rendered.contains(&name(key)),
                "pass {pass}: the cleanly fenced session {} is named: {rendered}",
                name(key)
            );
        }
    }
}

/// **(K)** An already-fenced session whose ended attempt **cannot be checked** is named too: an
/// `Aborting@E+1` session whose `retire:records:s:<id>:<E>` obligation will not decode against
/// its key. Being `Aborting` already is no reason to stop looking, and a record that cannot be read
/// is a human's (ADR-0045) — so it is named, and nothing is written: not a second fence, and not
/// a replacement for the obligation.
///
/// Base: nothing names it.
#[tokio::test]
async fn k_an_already_fenced_session_whose_obligation_will_not_decode_is_named() {
    capture_audit();
    let meta = Meta::new();
    let d = disks();
    let id = upload("b5");
    meta.seed(
        mpu_key(&id),
        session_at(EPOCH + 1, "{\"kind\":\"Aborting\"}"),
    );
    let records = fence_key(RetireMode::Records, &id, EPOCH);
    let damaged = Bytes::from_static(b"{\"seg\":\"not a segment group\"}");
    assert!(
        decode_retire_obligation(&records, &damaged).is_err(),
        "the damage must be one the obligation's decoder refuses"
    );
    meta.seed(records.clone(), damaged);
    let before = meta.snapshot();

    let report = restore_pass(&meta, &d)
        .await
        .expect("the post-restore pass runs");
    let rendered = format!("{report:?}");

    assert!(
        report.needs_human(),
        "an ended attempt that cannot be checked needs a human: {rendered}"
    );
    assert!(
        rendered.contains(&name(&mpu_key(&id))) && rendered.contains("sessions_fenced: 0"),
        "the session must be named, and not fenced again: {rendered}"
    );
    assert!(
        meta.snapshot() == before,
        "the pass wrote to the store over a session it could only name"
    );
}
