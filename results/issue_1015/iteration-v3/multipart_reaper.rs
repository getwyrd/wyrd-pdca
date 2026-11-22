//! The multipart **reaper**, driven through its operator one-shot (#625, slice 625.1; proposal
//! 0016 decision 6, `0016:1796-2277`).
//!
//! Every leg seeds a real redb store with raw session records, drops it (redb holds an exclusive
//! lock), runs the built binary as a subprocess —
//! `wyrd custodian --data-dir <dir> --reap-abandoned-uploads`, the flag last — waits for it with a
//! bound, reopens the store and asserts on what it durably holds. The command is stringly typed,
//! so this file names no symbol the reaper adds and compiles the same on a tree without it.
//!
//! * **A** — idle abandonment is reclaimed, every byte evidenced: an `Open` session that only the
//!   idle arm can reap (older than `W_open`, inside `W_session`, every lease lapsed) is fenced and
//!   drained, and an `orphan:` mark stands on every fragment position of its committed part and of
//!   its crashed in-flight residue.
//! * **B** — the idle arm never reaps a progressing upload: a live owned `sidx:` lease, a reserved
//!   slot with a renewed lease and no chunk yet, a recent part commit, and a recent reservation
//!   whose lease lapsed each keep the session.
//! * **C** — the residency ceiling still bites a session with fresh progress.
//! * **E-open** — the clock guard suppresses judgments, not the pass: foreign-clocked `Open`
//!   sessions past either window survive; a foreign-clocked `Aborting` session is still drained
//!   and deleted.
//! * **H** — a budget-profile mismatch stops the pass before it acts: the operator signal, and
//!   not one byte of the store changed, an already-`Aborting` upload's drain included.
//! * A session the pass cannot judge is held: the one-shot exits non-zero, and the pass still
//!   tears down the upload beside it.
//!
//! Each runs plain `#[test]`s with short-lived runtimes of their own: the role `block_on`s its own.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use wyrd_core::metadata::{self, decode_orphan_mark, orphan_key, ChunkRef, EcScheme};
use wyrd_core::multipart::{
    decode_admission_record, mpu_key, part_key, part_range, psum_key, psum_range, retire_key,
    retire_session_range, sidx_key, sidx_range, slot_key, slot_range, AdmissionRecord, OwnedEntry,
    PartNumber, PartRecord, PartSummary, RetireMode, SessionRecord, SlotIndex, SlotRecord,
    StagedPlacement, UploadId, MAX_INFLIGHT_PARTS, MAX_PARTS_PER_SESSION, MAX_PART_CHUNKS,
    MAX_STAGED_CHUNKS, MPUCTL_KEY, W_REF,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_traits::{ChunkId, CommitOutcome, DServerId, FragmentId, MetadataStore, WriteBatch};

const WYRD: &str = env!("CARGO_BIN_EXE_wyrd");

/// The longest one one-shot may take. It runs one pass over a handful of records; a minute is
/// generous on a loaded CI host, and a hang past it fails the leg rather than the suite.
const ONE_SHOT_BOUND: Duration = Duration::from_secs(120);

/// How many one-shots leg A, C and E-open allow the store to converge in. A fence and the drain's
/// marks land in the first; the terminal delete waits for the `retire:` obligation to drain, so
/// it lands in the second or third. Six leaves room without hiding a pass that never converges.
const CONVERGE_RUNS: usize = 6;

const DAY: u64 = 24 * 60 * 60 * 1_000;
const HOUR: u64 = 60 * 60 * 1_000;

/// The deployment wall clock, read once per leg at seeding. The reaper judges every session
/// against the same source (`clock_source: "wall"`), so the fixture's stamps are written relative
/// to it.
// wall-clock exempt: the reaper under test reads the deployment wall clock, so its fixture's
// stamps must come from that same clock (#619).
#[allow(clippy::disallowed_methods)]
fn wall_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after the epoch")
        .as_millis() as u64
}

// ───────────────────────────── the store, outside the process ─────────────────────────────

/// A redb store under a temp dir the one-shot is pointed at.
struct Store {
    dir: tempfile::TempDir,
}

impl Store {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temp dir"),
        }
    }

    fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    /// Run `f` over the store, opened for the call and closed before it returns, in a runtime of
    /// its own.
    fn with<T>(&self, f: impl AsyncFnOnce(&RedbMetadataStore) -> T) -> T {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let meta = RedbMetadataStore::open(self.data_dir().join("meta.redb")).expect("redb");
            f(&meta).await
        })
    }

    /// Put every record, in one batch.
    fn seed(&self, records: Vec<(Vec<u8>, Bytes)>) {
        self.with(async |meta| {
            let batch = records
                .into_iter()
                .fold(WriteBatch::new(), |batch, (key, value)| {
                    batch.put(key, value)
                });
            let outcome = meta.commit(batch).await.expect("seeded");
            assert_eq!(outcome, CommitOutcome::Committed);
        });
    }

    /// Every key and value the store holds.
    fn snapshot(&self) -> BTreeMap<Vec<u8>, Bytes> {
        self.with(async |meta| {
            meta.scan(b"")
                .await
                .expect("scan")
                .into_iter()
                .collect::<BTreeMap<_, _>>()
        })
    }

    fn value(&self, key: &[u8]) -> Option<Bytes> {
        let key = key.to_vec();
        self.with(async move |meta| meta.get(&key).await.expect("get"))
    }

    fn range(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Bytes)> {
        let prefix = prefix.to_vec();
        self.with(async move |meta| meta.scan(&prefix).await.expect("scan"))
    }

    /// The `mpuctl` count.
    fn counted(&self) -> u64 {
        let ledger = self.value(MPUCTL_KEY).expect("the admission ledger");
        decode_admission_record(&ledger)
            .expect("the ledger decodes")
            .count()
    }

    /// One one-shot over this store: its exit status and what it wrote to stderr.
    fn reap_once(&self) -> (ExitStatus, String) {
        let mut child = Reaped(
            Command::new(WYRD)
                .arg("custodian")
                .arg("--data-dir")
                .arg(self.data_dir())
                // Last in argv: on a parser that does not know it as valueless, the command fails
                // at parse and reaps nothing.
                .arg("--reap-abandoned-uploads")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn wyrd"),
        );
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("wait") {
                break status;
            }
            assert!(
                started.elapsed() < ONE_SHOT_BOUND,
                "the one-shot must return after one pass, within {ONE_SHOT_BOUND:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut stderr = String::new();
        child
            .0
            .stderr
            .take()
            .expect("stderr")
            .read_to_string(&mut stderr)
            .expect("stderr is utf-8");
        (status, stderr)
    }

    /// One-shots until `done` holds, at most [`CONVERGE_RUNS`]. The store's convergence is
    /// asserted first, then that every one-shot exited successfully: a leg fails on what the store
    /// holds before it fails on a status.
    fn reap_until(&self, done: impl Fn(&Self) -> bool) {
        let mut failed = Vec::new();
        for run in 0..CONVERGE_RUNS {
            if done(self) {
                break;
            }
            let (status, stderr) = self.reap_once();
            if !status.success() {
                failed.push(format!("one-shot {run} exited {status}:\n{stderr}"));
            }
        }
        assert!(
            done(self),
            "the store did not converge within {CONVERGE_RUNS} one-shots\n{}",
            failed.join("\n")
        );
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}

/// The one-shot's child, killed on every exit path so a hang never outlives the test.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ─────────────────────────────── the records ───────────────────────────────

/// Decode `json` as `T` and re-encode it: the canonical bytes the store's decoders require.
fn canonical<T: serde::Serialize + serde::de::DeserializeOwned>(json: &str) -> Bytes {
    let value: T = metadata::decode(json.as_bytes()).expect("a legal record");
    metadata::encode(&value)
}

fn upload(n: u8) -> UploadId {
    UploadId::new(format!("{n:02x}").repeat(16)).expect("an upload id")
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).expect("a part number")
}

const RS21: EcScheme = EcScheme::ReedSolomon { k: 2, m: 1 };

/// One RS(2,1) chunk placed on `servers`.
fn chunk(id: ChunkId, servers: [DServerId; 3]) -> ChunkRef {
    ChunkRef {
        id,
        scheme: RS21,
        len: 4_096,
        placement: servers.to_vec(),
    }
}

/// Every fragment position of `chunk`: fragment `i` on `placement[i]`.
fn positions(chunk: &ChunkRef) -> Vec<(DServerId, FragmentId)> {
    chunk
        .placement
        .iter()
        .enumerate()
        .map(|(i, &dserver)| {
            (
                dserver,
                FragmentId {
                    chunk: chunk.id,
                    index: i as u16,
                },
            )
        })
        .collect()
}

/// The shipped budget profile, spelled from its public constants: the one the custodian under
/// test holds as its own.
fn shipped_profile_json(w_ref: u64) -> String {
    format!(
        "{{\"w_ref\":{w_ref},\"max_part_chunks\":{MAX_PART_CHUNKS},\
         \"max_parts_per_session\":{MAX_PARTS_PER_SESSION},\
         \"max_inflight_parts\":{MAX_INFLIGHT_PARTS},\"max_staged_chunks\":{MAX_STAGED_CHUNKS}}}"
    )
}

/// `mpuctl` counting `count` sessions under the profile with `W_ref = w_ref` (and every other
/// number shipped), its `max_sessions` the profile's own derivation.
fn ledger(count: u64, w_ref: u64) -> (Vec<u8>, Bytes) {
    let profile = shipped_profile_json(w_ref);
    let profile_budget: wyrd_core::multipart::Budget =
        metadata::decode(profile.as_bytes()).expect("a legal profile");
    let json = format!(
        "{{\"count\":{count},\"max_sessions\":{},\"profile\":{profile}}}",
        profile_budget.max_sessions()
    );
    (MPUCTL_KEY.to_vec(), canonical::<AdmissionRecord>(&json))
}

/// The session record of `id` in `state` (its JSON), created at `created`, stamped under `clock`.
fn session_json(id: &UploadId, created: u64, clock: &str, epoch: u64, state: &str) -> String {
    format!(
        "{{\"parent\":42,\"object\":\"bucket/object-{id}\",\"created_at_millis\":{created},\
         \"clock_source\":\"{clock}\",\"segment_nonce\":\"{}\",\"epoch\":{epoch},\"attempts\":0,\
         \"state\":{{\"kind\":\"{state}\"}}}}",
        id.as_str()
    )
}

/// An `Open@0` session record.
fn open_session(id: &UploadId, created: u64, clock: &str) -> (Vec<u8>, Bytes) {
    let json = session_json(id, created, clock, 0, "Open");
    (mpu_key(id), canonical::<SessionRecord>(&json))
}

/// A committed part `n` of `id` over `chunks`, committed at `at`: its `part:` and `psum:` records.
fn committed_part(id: &UploadId, n: u32, chunks: &[ChunkRef], at: u64) -> Vec<(Vec<u8>, Bytes)> {
    let digest = format!("{:064x}", n);
    let len: u64 = chunks.iter().map(|c| c.len).sum();
    let chunks_json = String::from_utf8(metadata::encode(&chunks).to_vec()).unwrap();
    let part_json = format!(
        "{{\"chunks\":{chunks_json},\"len\":{len},\"digest\":\"{digest}\",\
         \"committed_at_millis\":{at},\"session_epoch\":0}}"
    );
    let summary_json = format!(
        "{{\"chunks\":{},\"len\":{len},\"digest\":\"{digest}\",\"committed_at_millis\":{at}}}",
        chunks.len()
    );
    vec![
        (part_key(id, part(n)), canonical::<PartRecord>(&part_json)),
        (
            psum_key(id, part(n)),
            canonical::<PartSummary>(&summary_json),
        ),
    ]
}

/// Slot `index` of `id`, reserved for part `n` at `reserved`, leased to `lease`.
fn slot(id: &UploadId, index: u32, n: u32, reserved: u64, lease: u64) -> (Vec<u8>, Bytes) {
    let json = format!(
        "{{\"part_number\":{n},\"attempt_id\":\"{}\",\"reserved_at_millis\":{reserved},\
         \"lease_expiry_millis\":{lease}}}",
        "ab".repeat(16)
    );
    (
        slot_key(id, SlotIndex::new(index).expect("a slot index")),
        canonical::<SlotRecord>(&json),
    )
}

/// The owned staging entry of `chunk` for part `n` of `id`, leased to `lease`.
fn owned(id: &UploadId, n: u32, chunk: &ChunkRef, lease: u64) -> (Vec<u8>, Bytes) {
    let staged = StagedPlacement::new(chunk.scheme, chunk.placement.clone()).expect("staged");
    let entry = OwnedEntry::new(id.clone(), lease, staged).to_pending();
    (sidx_key(id, part(n), chunk.id), metadata::encode(&entry))
}

/// An upload already `Aborting` under `clock`: fenced from `Open@0` by the one teardown the client
/// Abort and the reaper share, its `{session, all}` obligation beside it, holding one committed
/// part over `committed` and the residue of a crashed in-flight part over `residue`.
fn aborting_session(
    id: &UploadId,
    clock: &str,
    now: u64,
    committed: &ChunkRef,
    residue: &ChunkRef,
) -> Vec<(Vec<u8>, Bytes)> {
    let open: SessionRecord =
        metadata::decode(session_json(id, now - 14 * DAY, clock, 0, "Open").as_bytes())
            .expect("a session");
    let teardown = open.open_teardown(id).expect("Open fences");
    let obligation = teardown.obligation();
    let mut records = vec![
        (mpu_key(id), metadata::encode(teardown.session())),
        (obligation.key(), metadata::encode(obligation.payload())),
        slot(id, 0, 2, now - 10 * DAY, now - 9 * DAY),
        owned(id, 2, residue, now - 9 * DAY),
    ];
    records.extend(committed_part(
        id,
        1,
        std::slice::from_ref(committed),
        now - 10 * DAY,
    ));
    records
}

/// Whether every staged and session record of `id` is gone.
fn session_gone(store: &Store, id: &UploadId) -> bool {
    store.value(&mpu_key(id)).is_none()
        && [
            part_range(id),
            psum_range(id),
            sidx_range(id),
            slot_range(id),
        ]
        .iter()
        .all(|prefix| store.range(prefix).is_empty())
}

/// Assert the whole teardown of `id`: every record gone, both of its `retire:` ranges empty.
fn assert_torn_down(store: &Store, id: &UploadId) {
    assert!(store.value(&mpu_key(id)).is_none(), "mpu:{id} deleted");
    for (name, prefix) in [
        ("part:", part_range(id)),
        ("psum:", psum_range(id)),
        ("sidx:", sidx_range(id)),
        ("slot:", slot_range(id)),
        ("retire:bytes:", retire_session_range(RetireMode::Bytes, id)),
        (
            "retire:records:",
            retire_session_range(RetireMode::Records, id),
        ),
    ] {
        assert!(store.range(&prefix).is_empty(), "{name}{id} is empty");
    }
}

/// Assert GC's licence for every position of `expected`: an `orphan:` mark stands there, it
/// decodes stamped (not `reclaiming`), the `retire:bytes:` obligation its event names (if any) is
/// gone, and no `part:` or `sidx:` record in the store still names the position.
fn assert_licensed(store: &Store, expected: &[(DServerId, FragmentId)]) {
    let naming: Vec<(DServerId, FragmentId)> = store
        .range(b"part:")
        .iter()
        .flat_map(|(_, value)| {
            let part: PartRecord = metadata::decode(value).expect("a part");
            part.chunks().iter().flat_map(positions).collect::<Vec<_>>()
        })
        .chain(store.range(b"sidx:").iter().flat_map(|(key, value)| {
            let (_, chunk_id, entry) =
                wyrd_core::multipart::decode_owned_entry(key, value).expect("an owned entry");
            let staged = entry.staged();
            positions(&ChunkRef {
                id: chunk_id,
                scheme: staged.scheme(),
                len: 0,
                placement: staged.placement().to_vec(),
            })
        }))
        .collect();
    for &(dserver, frag) in expected {
        let key = orphan_key(dserver, frag);
        let value = store
            .value(&key)
            .unwrap_or_else(|| panic!("an orphan: mark on server {dserver}, {frag:?}"));
        let mark = decode_orphan_mark(&value).expect("the mark decodes");
        assert!(!mark.is_reclaiming(), "{frag:?} is stamped, not reclaiming");
        if let Some(token) = mark.retire_token() {
            assert!(
                store
                    .value(&retire_key(RetireMode::Bytes, &token))
                    .is_none(),
                "the obligation {token} the mark names has drained"
            );
        }
        assert!(
            !naming.contains(&(dserver, frag)),
            "no record still names {dserver}/{frag:?}"
        );
    }
}

// ─────────────────────────────── the legs ───────────────────────────────

/// **A — idle abandonment is reclaimed, with every byte evidenced.** Only the idle arm can reap
/// this session: it was created 14 days ago (past `W_open`, inside `W_session`), its newest
/// progress stamp is 10 days old, and every lease lapsed 9 days ago. It holds one committed part
/// and the residue of a crashed in-flight part (a slot and an owned entry, no part record).
#[test]
fn idle_abandoned_session_is_fenced_and_drained_with_every_byte_evidenced() {
    let store = Store::new();
    let now = wall_now();
    let id = upload(0xa1);
    let committed = chunk(0xa1_0001, [1, 2, 3]);
    let in_flight = chunk(0xa1_0002, [4, 5, 6]);
    let mut records = vec![
        ledger(1, W_REF),
        open_session(&id, now - 14 * DAY, "wall"),
        slot(&id, 0, 2, now - 10 * DAY, now - 9 * DAY),
        owned(&id, 2, &in_flight, now - 9 * DAY),
    ];
    records.extend(committed_part(
        &id,
        1,
        std::slice::from_ref(&committed),
        now - 10 * DAY,
    ));
    store.seed(records);
    assert_eq!(store.counted(), 1);

    store.reap_until(|store| session_gone(store, &id));

    assert_torn_down(&store, &id);
    assert_eq!(store.counted(), 0, "the admission slot is released");
    // Both byte sets: the committed part's positions, named explicitly, and the residue's.
    let committed_positions = positions(&committed);
    assert_eq!(
        committed_positions,
        vec![
            (
                1,
                FragmentId {
                    chunk: 0xa1_0001,
                    index: 0
                }
            ),
            (
                2,
                FragmentId {
                    chunk: 0xa1_0001,
                    index: 1
                }
            ),
            (
                3,
                FragmentId {
                    chunk: 0xa1_0001,
                    index: 2
                }
            ),
        ]
    );
    assert_licensed(&store, &committed_positions);
    assert_licensed(&store, &positions(&in_flight));
}

/// **B — the idle arm never reaps a progressing upload.** Four sessions, each created 14 days
/// ago (inside `W_session`) with a part committed 10 days ago, each with one piece of evidence
/// of progress: (i) an owned `sidx:` lease still unexpired (one long `UploadPart` streaming),
/// (ii) a reserved slot with no chunk written yet and a renewed lease, (iii) a part committed an
/// hour ago, (iv) a slot reserved an hour ago whose lease has already lapsed — the reservation
/// stamp alone is progress. Two one-shots leave every byte of the store as it was, the admission
/// count with it.
#[test]
fn idle_arm_never_reaps_a_progressing_upload() {
    let store = Store::new();
    let now = wall_now();
    let created = now - 14 * DAY;
    let old = now - 10 * DAY;
    let (streaming, pre_first_chunk, recent, reserved) =
        (upload(0xb1), upload(0xb2), upload(0xb3), upload(0xb4));
    let mut records = vec![
        ledger(4, W_REF),
        open_session(&streaming, created, "wall"),
        open_session(&pre_first_chunk, created, "wall"),
        open_session(&recent, created, "wall"),
        open_session(&reserved, created, "wall"),
        // (i) a live owned lease, renewed an hour out.
        owned(&streaming, 2, &chunk(0xb1_0002, [4, 5, 6]), now + HOUR),
        // (ii) reserved 10 days ago, no chunk yet, its lease renewed an hour out.
        slot(&pre_first_chunk, 0, 2, old, now + HOUR),
        // (iv) reserved an hour ago, its lease lapsed half an hour ago.
        slot(&reserved, 0, 2, now - HOUR, now - HOUR / 2),
    ];
    for (n, id) in [(1, &streaming), (2, &pre_first_chunk), (4, &reserved)] {
        records.extend(committed_part(
            id,
            1,
            &[chunk(0xb0_0000 + n, [1, 2, 3])],
            old,
        ));
    }
    // (iii) the newest part committed an hour ago.
    records.extend(committed_part(
        &recent,
        1,
        &[chunk(0xb3_0001, [1, 2, 3])],
        old,
    ));
    records.extend(committed_part(
        &recent,
        2,
        &[chunk(0xb3_0002, [1, 2, 3])],
        now - HOUR,
    ));
    store.seed(records);
    let before = store.snapshot();

    let runs: Vec<(ExitStatus, String)> = (0..2).map(|_| store.reap_once()).collect();

    assert_eq!(
        store.snapshot(),
        before,
        "no record of a progressing upload moved"
    );
    assert_eq!(store.counted(), 4, "the admission count is unchanged");
    // And the one-shot ran its pass: a store left alone by a command that never ran proves
    // nothing about the reaper.
    for (run, (status, stderr)) in runs.iter().enumerate() {
        assert!(
            status.success(),
            "one-shot {run} failed: {status}\n{stderr}"
        );
    }
}

/// **C — the residency ceiling still bites.** A session created 40 days ago (past `W_session`)
/// with fresh progress everywhere — a part committed an hour ago, a live slot, a live owned lease
/// — is fenced and drained all the same, and its admission slot released.
#[test]
fn over_age_session_is_fenced_and_drained_however_live() {
    let store = Store::new();
    let now = wall_now();
    let id = upload(0xc1);
    let committed = chunk(0xc1_0001, [1, 2, 3]);
    let in_flight = chunk(0xc1_0002, [4, 5, 6]);
    let mut records = vec![
        ledger(1, W_REF),
        open_session(&id, now - 40 * DAY, "wall"),
        slot(&id, 0, 2, now - HOUR, now + HOUR),
        owned(&id, 2, &in_flight, now + HOUR),
    ];
    records.extend(committed_part(
        &id,
        1,
        std::slice::from_ref(&committed),
        now - HOUR,
    ));
    store.seed(records);

    store.reap_until(|store| session_gone(store, &id));

    assert_torn_down(&store, &id);
    assert_eq!(store.counted(), 0, "the admission slot is released");
    assert_licensed(&store, &positions(&committed));
    assert_licensed(&store, &positions(&in_flight));
}

/// **E-open — the clock guard suppresses judgments, not the pass.** Two `Open` sessions stamped
/// under a clock the reaper does not read — one past `W_open` only, one past `W_session` — are
/// never judged and survive byte for byte. Only an operator can end them, so every one-shot
/// names them and exits non-zero: it never reports such a store as having nothing to reap. A
/// third, already `Aborting` under the same foreign clock with its obligation installed, is
/// walked, drained and deleted all the same, which reads no session timestamp at all.
#[test]
fn foreign_clocked_open_sessions_survive_and_a_foreign_aborting_one_drains() {
    let store = Store::new();
    let now = wall_now();
    let (idle, over_age, aborting) = (upload(0xe1), upload(0xe2), upload(0xe3));
    let mut records = vec![
        ledger(3, W_REF),
        open_session(&idle, now - 14 * DAY, "logical"),
        slot(&idle, 0, 2, now - 10 * DAY, now - 9 * DAY),
        open_session(&over_age, now - 40 * DAY, "logical"),
    ];
    records.extend(committed_part(
        &idle,
        1,
        &[chunk(0xe1_0001, [1, 2, 3])],
        now - 10 * DAY,
    ));
    records.extend(committed_part(
        &over_age,
        1,
        &[chunk(0xe2_0001, [1, 2, 3])],
        now - 35 * DAY,
    ));
    let aborted_part = chunk(0xe3_0001, [1, 2, 3]);
    let aborted_residue = chunk(0xe3_0002, [4, 5, 6]);
    records.extend(aborting_session(
        &aborting,
        "logical",
        now,
        &aborted_part,
        &aborted_residue,
    ));
    store.seed(records);
    let survivors = |snapshot: &BTreeMap<Vec<u8>, Bytes>| -> BTreeMap<Vec<u8>, Bytes> {
        snapshot
            .iter()
            .filter(|(key, _)| {
                [&idle, &over_age]
                    .iter()
                    .any(|id| key.windows(32).any(|w| w == id.as_str().as_bytes()))
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    let before = survivors(&store.snapshot());
    assert!(before.len() >= 6, "the foreign Open sessions are seeded");

    let mut runs = Vec::new();
    for _ in 0..CONVERGE_RUNS {
        if session_gone(&store, &aborting) {
            break;
        }
        runs.push(store.reap_once());
    }

    assert!(
        session_gone(&store, &aborting),
        "the foreign Aborting session is torn down within {CONVERGE_RUNS} one-shots"
    );
    assert_eq!(
        survivors(&store.snapshot()),
        before,
        "neither foreign-clocked Open session was judged"
    );
    assert_torn_down(&store, &aborting);
    assert_eq!(
        store.counted(),
        2,
        "the Aborting session's slot is released"
    );
    assert_licensed(&store, &positions(&aborted_part));
    assert_licensed(&store, &positions(&aborted_residue));
    for (run, (status, stderr)) in runs.iter().enumerate() {
        for id in [&idle, &over_age] {
            assert!(
                stderr.contains(&format!("mpu:{id}")) && stderr.contains("foreign-clock"),
                "one-shot {run} names {id} as held for its clock:\n{stderr}"
            );
        }
        assert!(
            !status.success(),
            "one-shot {run} exits non-zero while a foreign-clocked Open session waits for an \
             operator: {status}"
        );
    }
}

/// **H — a profile mismatch stops the reaper before it acts.** The ledger stores a profile with
/// twice the shipped `W_ref`, so the custodian is not sized for the sessions it would fence. A
/// session the idle arm would reap (leg A's) and one past the ceiling (leg C's) are both present,
/// and so is one already `Aborting` with its `retire:bytes:` obligation, a committed part and
/// in-flight residue — work the retirement drain would otherwise do this same pass. The one-shot
/// emits the operator signal and exits non-zero, and the store is byte-identical: no fence, no
/// mark, no deletion, no counter change.
#[test]
fn profile_mismatch_signals_and_mutates_nothing() {
    let store = Store::new();
    let now = wall_now();
    let (idle, over_age, aborting) = (upload(0x71), upload(0x72), upload(0x73));
    let mut records = vec![
        ledger(3, 2 * W_REF),
        open_session(&idle, now - 14 * DAY, "wall"),
        slot(&idle, 0, 2, now - 10 * DAY, now - 9 * DAY),
        owned(&idle, 2, &chunk(0x71_0002, [4, 5, 6]), now - 9 * DAY),
        open_session(&over_age, now - 40 * DAY, "wall"),
    ];
    records.extend(committed_part(
        &idle,
        1,
        &[chunk(0x71_0001, [1, 2, 3])],
        now - 10 * DAY,
    ));
    records.extend(aborting_session(
        &aborting,
        "wall",
        now,
        &chunk(0x73_0001, [1, 2, 3]),
        &chunk(0x73_0002, [4, 5, 6]),
    ));
    store.seed(records);
    let before = store.snapshot();

    let (status, stderr) = store.reap_once();

    assert!(
        stderr.contains("reaper-profile-mismatch"),
        "the operator signal names the mismatch:\n{stderr}"
    );
    assert!(!status.success(), "a refused pass exits non-zero: {status}");
    assert_eq!(store.snapshot(), before, "the store is untouched");
    assert_eq!(store.counted(), 3);
}

/// **A held session fails the one-shot, and the pass goes on.** Beside leg A's idle session sits
/// a session record that does not decode. Every one-shot names it (`reaper-session-held`) and
/// exits non-zero, so a script running the one-shot sees that something needs a human — yet each
/// pass still fences, drains and deletes the idle session around it, and the undecodable record
/// is left byte for byte.
#[test]
fn a_held_session_fails_the_one_shot_and_the_pass_goes_on() {
    let store = Store::new();
    let now = wall_now();
    let (idle, damaged) = (upload(0x81), upload(0x82));
    let committed = chunk(0x81_0001, [1, 2, 3]);
    let mut records = vec![
        ledger(2, W_REF),
        open_session(&idle, now - 14 * DAY, "wall"),
        (
            mpu_key(&damaged),
            Bytes::from_static(b"not a session record"),
        ),
    ];
    records.extend(committed_part(
        &idle,
        1,
        std::slice::from_ref(&committed),
        now - 10 * DAY,
    ));
    store.seed(records);

    let mut runs = Vec::new();
    for _ in 0..CONVERGE_RUNS {
        if session_gone(&store, &idle) {
            break;
        }
        runs.push(store.reap_once());
    }

    assert!(
        session_gone(&store, &idle),
        "the idle session is torn down around the held one within {CONVERGE_RUNS} one-shots"
    );
    assert_torn_down(&store, &idle);
    assert_licensed(&store, &positions(&committed));
    assert_eq!(
        store.value(&mpu_key(&damaged)).as_deref(),
        Some(&b"not a session record"[..]),
        "the held record is left as it was"
    );
    assert_eq!(
        store.counted(),
        1,
        "only the idle session's slot is released"
    );
    for (run, (status, stderr)) in runs.iter().enumerate() {
        assert!(
            stderr.contains("reaper-session-held"),
            "one-shot {run} names the hold:\n{stderr}"
        );
        assert!(
            !status.success(),
            "one-shot {run} exits non-zero while a session is held: {status}"
        );
    }
}
