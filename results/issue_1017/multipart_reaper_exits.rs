//! The multipart **reaper's terminal exits**, driven through its operator one-shot (#625, slice
//! 625.3; proposal 0016 decision 6, `0016:1794-2277`, and the exit table, `0016:569-579`).
//!
//! The mechanics are `multipart_reaper.rs`'s: every leg seeds a real redb store with raw records,
//! drops it (redb holds an exclusive lock), runs the built binary as a bounded subprocess —
//! `wyrd custodian --data-dir <dir> --reap-abandoned-uploads`, the flag last — reopens the store
//! and asserts on what it durably holds. The seeding helpers are **copied** from that file, not
//! shared, so each test binary stands alone (`run-verify.sh:141-144`). The command is stringly
//! typed and every record is spelled as JSON, so this file names no symbol 625.3 adds and compiles
//! the same on a tree without it.
//!
//! * **D** — a `Completing` session is exited both ways: a stale one inside `W_session` is rolled
//!   back to `Open@E+1`, its attempt's `seg:<g>:<E>:` range drained by the
//!   `retire:records:{seg}` the rollback installs and its parts kept; an over-age one goes
//!   **straight** to `Aborting@E+1` — never rolled back first — and is torn down under the
//!   `Completing` epoch's token. A later epoch's segments are untouched either way. One fenced
//!   moments ago is kept however old its parts are: the window is measured from the fence.
//! * **E-terminal** — the clock guard on the terminal states: a foreign-clocked `Completing`
//!   session past both windows is not rolled back or fenced, and a foreign-clocked `Completed`
//!   session past any tombstone window keeps its record.
//! * **G** — tombstone expiry: a local-clock `Completed` session past `W_tombstone`, with nothing
//!   owed and no residue, is deleted and `mpuctl` lowered by exactly one; one inside the window
//!   keeps its record, which still answers an identical Complete retry `AlreadyCompleted`.
//! * **H-terminal** — a mismatched `mpuctl` profile refuses the pass before the `Completing` arms
//!   too: a stale `Completing` session is not rolled back, and no byte of the store moves.
//! * **F** — teardown over the reaper's path is idempotent and waits for GC's decision: re-running
//!   the one-shot over a partially drained teardown of a session the reaper fenced converges with
//!   no double decrement; a `reclaiming` mark on one of its positions holds the obligation, the
//!   record and the count until GC deletes the key, and the drain then marks it afresh. This is
//!   regression coverage of #659's drain through the reaper's path, green wherever that drain is.
//!
//! Each runs plain `#[test]`s with short-lived runtimes of their own: the role `block_on`s its own.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use wyrd_core::metadata::{
    self, decode_orphan_mark, encode_orphan_mark, orphan_key, seg_key, seg_range_prefix, ChunkRef,
    EcScheme, OrphanMark, SegmentGroup, SegmentRecord,
};
use wyrd_core::multipart::{
    complete_answer, decode_admission_record, decode_session_record, mpu_key, part_key, part_range,
    psum_key, psum_range, retire_key, retire_session_range, sidx_key, sidx_range, slot_key,
    slot_range, AdmissionRecord, CompleteAnswer, Digest, OwnedEntry, PartNumber, PartRecord,
    PartSummary, RetireMode, RetireToken, SessionRecord, SessionState, SlotIndex, SlotRecord,
    StagedPlacement, UploadId, MAX_INFLIGHT_PARTS, MAX_PARTS_PER_SESSION, MAX_PART_CHUNKS,
    MAX_STAGED_CHUNKS, MPUCTL_KEY, W_REF,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_traits::{ChunkId, CommitOutcome, DServerId, FragmentId, MetadataStore, WriteBatch};

const WYRD: &str = env!("CARGO_BIN_EXE_wyrd");

/// The longest one one-shot may take. It runs one pass over a handful of records; two minutes is
/// generous on a loaded CI host, and a hang past it fails the leg rather than the suite.
const ONE_SHOT_BOUND: Duration = Duration::from_secs(120);

/// How many one-shots a leg allows the store to converge in. A fence and the drain's first steps
/// land in the first; the terminal delete waits for the `retire:` obligations to drain, so it lands
/// in the second or third. Six leaves room without hiding a pass that never converges.
const CONVERGE_RUNS: usize = 6;

const MINUTE: u64 = 60 * 1_000;
const DAY: u64 = 24 * 60 * MINUTE;

/// The shortest terminal window 625.3 ships — `W_tombstone`, two minutes. Spelled here, not
/// imported, so this file names no symbol 625.3 adds.
const SHORTEST_TERMINAL_WINDOW: u64 = 2 * MINUTE;

/// Assert that the wall clock is still inside [`SHORTEST_TERMINAL_WINDOW`] of `seeded`, so a leg
/// that then asserts a stamp from `seeded` was judged inside its window asserts what it means
/// rather than a host that ran slower than the window.
fn still_inside_the_window(seeded: u64) {
    let elapsed = wall_now().saturating_sub(seeded);
    assert!(
        elapsed < SHORTEST_TERMINAL_WINDOW,
        "{elapsed} ms passed since seeding — past the {SHORTEST_TERMINAL_WINDOW} ms window this \
         leg's inside-the-window assertion needs"
    );
}

/// The deployment wall clock, read once per leg at seeding. The reaper judges every session
/// against the same source (`clock_source: "wall"`), so the fixture's stamps are written relative
/// to it. Every stamp a leg wants judged past a window sits **days** past it, and every stamp it
/// wants judged inside one sits at most seconds from `now`, so the legs hold for any window inside
/// the ranges 625.3 states (`W_completing` and `W_tombstone` minutes-scale and below `W_open`;
/// `W_session` at least 21 days). A leg that asserts a stamp is still inside the shortest of them
/// ([`SHORTEST_TERMINAL_WINDOW`]) first checks that the wall clock has not passed it meanwhile.
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

    /// Delete `key` blind — what GC's cleanup does to a mark it has consumed (`gc.rs`, the
    /// cleanup commit's unconditional deletes).
    fn delete(&self, key: &[u8]) {
        let key = key.to_vec();
        self.with(async move |meta| {
            let outcome = meta
                .commit(WriteBatch::new().delete(key))
                .await
                .expect("deleted");
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

    /// The session record of `id`, decoded, if present.
    fn session(&self, id: &UploadId) -> Option<SessionRecord> {
        self.value(&mpu_key(id))
            .map(|value| decode_session_record(&value).expect("the session decodes"))
    }

    /// One one-shot over this store: its exit status and what it wrote to stderr.
    fn reap_once(&self) -> (ExitStatus, String) {
        reap_once_at(self.data_dir())
    }

    /// One-shots until `done` holds, at most [`CONVERGE_RUNS`], each run's `(status, stderr)`
    /// returned. The store's convergence is asserted; a leg asserts on the runs itself.
    fn reap_until(&self, done: impl Fn(&Self) -> bool) -> Vec<(ExitStatus, String)> {
        let mut runs = Vec::new();
        for _ in 0..CONVERGE_RUNS {
            if done(self) {
                break;
            }
            runs.push(self.reap_once());
        }
        assert!(
            done(self),
            "the store did not converge within {CONVERGE_RUNS} one-shots\n{}",
            runs.iter()
                .map(|(status, stderr)| format!("exited {status}:\n{stderr}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        runs
    }
}

/// One one-shot pointed at `data_dir`: its exit status and what it wrote to stderr.
fn reap_once_at(data_dir: &Path) -> (ExitStatus, String) {
    let mut command = Command::new(WYRD);
    command
        .arg("custodian")
        .arg("--data-dir")
        .arg(data_dir)
        // Last in argv: on a parser that does not know it as valueless, the command fails at
        // parse and reaps nothing.
        .arg("--reap-abandoned-uploads");
    run_bounded(command)
}

/// Run `command` to its end, within [`ONE_SHOT_BOUND`]: its exit status and what it wrote to
/// stderr.
fn run_bounded(mut command: Command) -> (ExitStatus, String) {
    let mut child = Reaped(
        command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn wyrd"),
    );
    // Read while the child runs: a child whose stderr fills the pipe must never block on it.
    let mut pipe = child.0.stderr.take().expect("stderr");
    let drained = std::thread::spawn(move || {
        let mut stderr = String::new();
        pipe.read_to_string(&mut stderr).map(|_| stderr)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("wait") {
            break status;
        }
        assert!(
            started.elapsed() < ONE_SHOT_BOUND,
            "the command must return after one pass, within {ONE_SHOT_BOUND:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = drained
        .join()
        .expect("the stderr reader")
        .expect("stderr is utf-8");
    (status, stderr)
}

/// The child process, killed on every exit path so a hang never outlives the test.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The reaper tally `name` a one-shot emitted: the number that follows
/// `monotonic_counter.<name>` on its line, however the log format separates the two.
fn tally(stderr: &str, name: &str) -> Option<u64> {
    let line = stderr
        .lines()
        .find_map(|line| line.split(&format!("monotonic_counter.{name}")).nth(1))?;
    let digits: String = line
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
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

/// The session record of `id` (its JSON) in `state` — a state's own JSON object — created at
/// `created`, stamped under `clock`, at `epoch` with `attempts` Complete fences spent.
fn session_json(
    id: &UploadId,
    created: u64,
    clock: &str,
    epoch: u64,
    attempts: u32,
    state: &str,
) -> String {
    format!(
        "{{\"parent\":42,\"object\":\"{}\",\"created_at_millis\":{created},\
         \"clock_source\":\"{clock}\",\"segment_nonce\":\"{}\",\"epoch\":{epoch},\
         \"attempts\":{attempts},\"state\":{state}}}",
        object(id),
        id.as_str()
    )
}

/// The object name the session of `id` publishes to.
fn object(id: &UploadId) -> String {
    format!("bucket/object-{id}")
}

/// An `Open@0` session record.
fn open_session(id: &UploadId, created: u64, clock: &str) -> (Vec<u8>, Bytes) {
    let json = session_json(id, created, clock, 0, 0, r#"{"kind":"Open"}"#);
    (mpu_key(id), canonical::<SessionRecord>(&json))
}

/// A `Completing@epoch` session record — its first Complete fence, so `attempts` 1 — fenced at
/// `fenced`, having written `written` segments.
fn completing_session(
    id: &UploadId,
    created: u64,
    clock: &str,
    epoch: u64,
    fenced: u64,
    written: u32,
) -> (Vec<u8>, Bytes) {
    let state = format!(
        "{{\"kind\":\"Completing\",\"fenced_at_millis\":{fenced},\"segments_written\":{written},\
         \"publish_target\":{{\"parent\":42,\"name\":\"{}\",\"epoch\":{epoch}}}}}",
        object(id)
    );
    let json = session_json(id, created, clock, epoch, 1, &state);
    (mpu_key(id), canonical::<SessionRecord>(&json))
}

/// The fingerprint every `Completed` fixture records: an identical retry sends this.
fn fingerprint() -> String {
    "cd".repeat(32)
}

/// A `Completed@2` session record, its flip landed at `completed`.
fn completed_session(id: &UploadId, created: u64, clock: &str, completed: u64) -> (Vec<u8>, Bytes) {
    let state = format!(
        "{{\"kind\":\"Completed\",\"completion\":{{\"inode\":7,\"version\":1,\"etag\":\"{}-1\",\
         \"completed_at_millis\":{completed},\"complete_fingerprint\":\"{}\"}}}}",
        "ab".repeat(32),
        fingerprint()
    );
    let json = session_json(id, created, clock, 2, 1, &state);
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

/// The segment group of `id`'s session at `epoch`: its own nonce (the fixture's nonce is the
/// upload id's hex) and that epoch.
fn group(id: &UploadId, epoch: u64) -> SegmentGroup {
    SegmentGroup::new(id.as_str(), epoch).expect("a segment group")
}

/// The `seg:` records an attempt at `epoch` wrote over `chunks`, one segment each.
fn segments(id: &UploadId, epoch: u64, chunks: &[ChunkRef]) -> Vec<(Vec<u8>, Bytes)> {
    let group = group(id, epoch);
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let record = SegmentRecord::new(vec![chunk.clone()], i as u64 * chunk.len)
                .expect("a segment record");
            (
                seg_key(&group, i as u32).expect("a segment key"),
                metadata::encode(&record),
            )
        })
        .collect()
}

/// Every record whose key names `id` (its hex appears in the key): what a leg asserts unchanged.
fn records_of(snapshot: &BTreeMap<Vec<u8>, Bytes>, id: &UploadId) -> BTreeMap<Vec<u8>, Bytes> {
    snapshot
        .iter()
        .filter(|(key, _)| key.windows(32).any(|w| w == id.as_str().as_bytes()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
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

/// Whether `id` owes nothing under either `retire:` range.
fn owes_nothing(store: &Store, id: &UploadId) -> bool {
    RetireMode::ALL
        .iter()
        .all(|&mode| store.range(&retire_session_range(mode, id)).is_empty())
}

/// The session-teardown token of `id`'s session at `epoch`: `s:<id>:<epoch>`.
fn session_token(id: &UploadId, epoch: u64) -> RetireToken {
    RetireToken::Session {
        upload_id: id.clone(),
        epoch,
        part: None,
    }
}

/// The stamped, non-`reclaiming` mark at `position`, decoded.
fn stamped_mark(store: &Store, (dserver, frag): (DServerId, FragmentId)) -> OrphanMark {
    let value = store
        .value(&orphan_key(dserver, frag))
        .unwrap_or_else(|| panic!("an orphan: mark on server {dserver}, {frag:?}"));
    let mark = decode_orphan_mark(&value).expect("the mark decodes");
    assert!(!mark.is_reclaiming(), "{frag:?} is stamped, not reclaiming");
    mark
}

// ─────────────────────────────── the legs ───────────────────────────────

/// **D, rollback — a stale `Completing` session inside `W_session` rolls back to `Open@E+1`.** Its
/// completer fenced it at `Completing@1` two days ago — far past `W_completing` — and wrote two
/// segments before it died; the session was created three days ago, inside `W_session`. A later
/// epoch's group (`seg:<g>:3:`) is also present.
///
/// The reaper rolls it back to `Open@2`, `attempts` as the fence left them, and installs
/// `retire:records:{seg:<g>:1}`, which drains the epoch-1 segments and nothing else. The parts
/// still name those segments' chunks, so nothing is orphan-marked and both parts stay; the
/// admission count stays, since the session is `Open` again. On the base the session stays
/// `Completing@1` with its segments, so the store never converges.
#[test]
fn stale_completing_session_rolls_back_to_open_and_drains_only_its_attempts_segments() {
    let store = Store::new();
    let now = wall_now();
    let id = upload(0xd1);
    let chunks = [chunk(0xd1_0001, [1, 2, 3]), chunk(0xd1_0002, [4, 5, 6])];
    let later = [chunk(0xd1_0003, [1, 2, 3])];
    let mut records = vec![
        ledger(1, W_REF),
        completing_session(&id, now - 3 * DAY, "wall", 1, now - 2 * DAY, 2),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &id,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 3 * DAY,
        ));
    }
    records.extend(segments(&id, 1, &chunks));
    let later_segments = segments(&id, 3, &later);
    records.extend(later_segments.clone());
    store.seed(records);
    let parts_before: Vec<_> = [part_range(&id), psum_range(&id)]
        .iter()
        .flat_map(|prefix| store.range(prefix))
        .collect();
    assert_eq!(parts_before.len(), 4, "two parts seeded");

    let runs = store.reap_until(|store| {
        store
            .session(&id)
            .is_some_and(|session| matches!(session.state(), SessionState::Open {}))
            && store.range(&seg_range_prefix(&group(&id, 1))).is_empty()
            && owes_nothing(store, &id)
    });

    let session = store.session(&id).expect("the session stays");
    assert_eq!(session.epoch(), 2, "rolled back at E+1, in one fence");
    assert_eq!(session.attempts(), 1, "the attempt stays charged");
    assert_eq!(session.created_at_millis(), now - 3 * DAY);
    assert!(
        store.range(&seg_range_prefix(&group(&id, 1))).is_empty(),
        "the rolled-back attempt's segments are drained"
    );
    assert_eq!(
        store.range(&seg_range_prefix(&group(&id, 3))),
        later_segments,
        "a later epoch's segments are untouched"
    );
    let parts_after: Vec<_> = [part_range(&id), psum_range(&id)]
        .iter()
        .flat_map(|prefix| store.range(prefix))
        .collect();
    assert_eq!(
        parts_after, parts_before,
        "the parts stay: the session is Open again"
    );
    assert!(
        store.range(b"orphan:").is_empty(),
        "no fragment is orphaned: the parts still protect every segment's chunks"
    );
    assert_eq!(
        store.counted(),
        1,
        "an Open session keeps its admission slot"
    );
    let first = &runs.first().expect("at least one one-shot").1;
    assert_eq!(
        tally(first, "reaper_sessions_rolled_back"),
        Some(1),
        "the first one-shot rolled it back:\n{first}"
    );
    assert_eq!(
        tally(first, "reaper_sessions_fenced_completing_ceiling"),
        Some(0),
        "and did not fence it:\n{first}"
    );
}

/// **D, over-age — an over-age `Completing` session goes straight to `Aborting@E+1`.** Created 40
/// days ago (past `W_session`) and fenced at `Completing@1` two days ago (past `W_completing`
/// too), so both judgments apply and the ceiling must win: `0016:1920-1930` sends it to teardown
/// in **one** batch installing `retire:records:{seg:<g>:1}` beside the teardown's bytes
/// obligation, and never rolls it back first.
///
/// So every mark the teardown writes names the `Completing` epoch's token `s:<id>:1` — a rollback
/// first would have reached `Aborting@3` through `Open@2`, and marked under `s:<id>:2` — the
/// epoch-1 segments drain, a later epoch's stay, the session is deleted and the admission count
/// lowered by exactly one. On the base the session stays `Completing@1`, so the store never
/// converges.
#[test]
fn over_age_completing_session_is_fenced_straight_to_aborting_and_torn_down() {
    let store = Store::new();
    let now = wall_now();
    let (id, kept) = (upload(0xd2), upload(0xd3));
    let chunks = [chunk(0xd2_0001, [1, 2, 3]), chunk(0xd2_0002, [4, 5, 6])];
    let later = [chunk(0xd2_0003, [1, 2, 3])];
    let mut records = vec![
        ledger(2, W_REF),
        completing_session(&id, now - 40 * DAY, "wall", 1, now - 2 * DAY, 2),
        // Kept: created a day ago, nothing to judge.
        open_session(&kept, now - DAY, "wall"),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &id,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 39 * DAY,
        ));
    }
    records.extend(segments(&id, 1, &chunks));
    let later_segments = segments(&id, 3, &later);
    records.extend(later_segments.clone());
    store.seed(records);

    let runs = store.reap_until(|store| session_gone(store, &id) && owes_nothing(store, &id));

    assert_torn_down(&store, &id);
    assert!(
        store.range(&seg_range_prefix(&group(&id, 1))).is_empty(),
        "the attempt's segments are drained"
    );
    assert_eq!(
        store.range(&seg_range_prefix(&group(&id, 3))),
        later_segments,
        "a later epoch's segments are untouched"
    );
    for position in chunks.iter().flat_map(positions) {
        let mark = stamped_mark(&store, position);
        assert_eq!(
            mark.retire_token(),
            Some(session_token(&id, 1)),
            "{position:?} is marked under the Completing epoch's teardown, not a rollback's"
        );
    }
    assert_eq!(store.counted(), 1, "exactly one admission slot is released");
    assert!(store.session(&kept).is_some(), "the other session is kept");
    let first = &runs.first().expect("at least one one-shot").1;
    assert_eq!(
        tally(first, "reaper_sessions_fenced_completing_ceiling"),
        Some(1),
        "the first one-shot fenced it by the ceiling:\n{first}"
    );
    assert_eq!(
        tally(first, "reaper_sessions_rolled_back"),
        Some(0),
        "and never rolled it back:\n{first}"
    );
}

/// **D, kept — a `Completing` session whose fence is fresh is kept, however old its parts.** Its
/// completer fenced it moments ago; the session was created twenty days ago (inside `W_session`)
/// and its two parts committed then, long past `W_completing` measured from them. `W_completing` is
/// measured from the fence instant, never from part progress (F16; the falsification row
/// `0016:1027`), so the one-shot neither rolls it back nor fences it: the session, its parts and
/// its segments are byte for byte as seeded, nothing is owed and the count stands. A reaper that
/// rolled back every `Completing` session on sight would fail here; the base, which judges no
/// `Completing` session, keeps it too.
#[test]
fn completing_session_fenced_moments_ago_is_kept_however_old_its_parts() {
    let store = Store::new();
    let now = wall_now();
    let id = upload(0xd4);
    let chunks = [chunk(0xd4_0001, [1, 2, 3]), chunk(0xd4_0002, [4, 5, 6])];
    let mut records = vec![
        ledger(1, W_REF),
        completing_session(&id, now - 20 * DAY, "wall", 1, now, 2),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &id,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 20 * DAY,
        ));
    }
    records.extend(segments(&id, 1, &chunks));
    store.seed(records);
    let before = records_of(&store.snapshot(), &id);
    assert_eq!(before.len(), 7, "session, 2 parts, 2 summaries, 2 segments");

    let runs: Vec<_> = (0..2).map(|_| store.reap_once()).collect();

    still_inside_the_window(now);
    assert_eq!(
        records_of(&store.snapshot(), &id),
        before,
        "the session, its parts and its segments are as seeded"
    );
    assert!(owes_nothing(&store, &id), "no obligation installed");
    assert_eq!(store.counted(), 1, "the count stands");
    for (run, (status, stderr)) in runs.iter().enumerate() {
        assert!(
            status.success(),
            "one-shot {run} finds nothing to hold: {status}\n{stderr}"
        );
        assert!(
            !stderr.contains("completing-rollback") && !stderr.contains("residency-ceiling"),
            "one-shot {run} fenced nothing:\n{stderr}"
        );
    }
}

/// **E-terminal — the clock guard holds on the terminal states.** Two sessions stamped under a
/// clock the reaper does not read:
///
/// * a `Completing@1` session created 40 days ago and fenced two days ago — past `W_session` and
///   `W_completing` both — with its two segments: neither arm judges it, so its state, epoch,
///   `seg:` range and (absent) `retire:` records are byte for byte as seeded;
/// * a `Completed` session whose flip landed two days ago, nothing owed and no residue: its
///   tombstone window is measured on a clock this custodian does not read, so its record — and its
///   admission slot — survive. Its exit is the operator's terminal-expiry verb (#633).
///
/// Every one-shot names both as held for their clock and exits non-zero.
#[test]
fn foreign_clocked_completing_and_completed_sessions_are_never_judged() {
    let store = Store::new();
    let now = wall_now();
    let (completing, completed) = (upload(0xe4), upload(0xe5));
    let chunks = [chunk(0xe4_0001, [1, 2, 3]), chunk(0xe4_0002, [4, 5, 6])];
    let mut records = vec![
        ledger(2, W_REF),
        completing_session(&completing, now - 40 * DAY, "logical", 1, now - 2 * DAY, 2),
        completed_session(&completed, now - 3 * DAY, "logical", now - 2 * DAY),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &completing,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 39 * DAY,
        ));
    }
    records.extend(segments(&completing, 1, &chunks));
    store.seed(records);
    let before = store.snapshot();
    let (completing_before, completed_before) = (
        records_of(&before, &completing),
        records_of(&before, &completed),
    );
    assert_eq!(
        completing_before.len(),
        7,
        "session, 2 parts, 2 summaries, 2 segments"
    );

    let runs: Vec<_> = (0..2).map(|_| store.reap_once()).collect();

    let after = store.snapshot();
    assert_eq!(
        records_of(&after, &completing),
        completing_before,
        "the foreign Completing session: state, epoch, seg: range and retire: records unchanged"
    );
    assert!(owes_nothing(&store, &completing), "no obligation installed");
    assert_eq!(
        records_of(&after, &completed),
        completed_before,
        "the foreign Completed session's record survives"
    );
    assert_eq!(store.counted(), 2, "both keep their admission slots");
    for (run, (status, stderr)) in runs.iter().enumerate() {
        for id in [&completing, &completed] {
            // The hold itself, on one line: its action, its reason and the session it holds. The
            // `reaper-foreign-clock` notice names every foreign session whether or not it is held,
            // so it is no evidence of the hold.
            let key = format!("mpu:{id}");
            assert!(
                stderr
                    .lines()
                    .any(|line| line.contains("reaper-session-held")
                        && line.contains("\"foreign-clock\"")
                        && line.contains(&key)),
                "one-shot {run} holds {id} for its clock:\n{stderr}"
            );
        }
        assert!(
            !status.success(),
            "one-shot {run} exits non-zero while foreign-clocked sessions wait for an operator: \
             {status}"
        );
    }
}

/// **G — tombstone expiry.** Three local-clock `Completed` sessions with nothing owed:
///
/// * `expired`, whose flip landed two days ago (past `W_tombstone`), with no residue;
/// * `residue`, whose flip landed two days ago too, still holding the owned `sidx:` entry and the
///   slot of a part that crashed in flight before the Complete — residue the owned walk must mark
///   and empty before the record may go (`0016:2203-2215`, finding 2);
/// * `fresh`, whose flip landed as the leg began (inside the window).
///
/// `expired` is deleted; `residue` is deleted only once its residue is walked — every position
/// of the in-flight chunk marked, its `sidx:` and `slot:` ranges empty — so `mpuctl` is lowered
/// by exactly two. `fresh` keeps its record, and an identical Complete retry against it still
/// answers `AlreadyCompleted` with the recorded publication. On the base the retained tombstone
/// input keeps all three, so the store never converges.
#[test]
fn completed_session_past_its_tombstone_window_is_deleted_and_one_inside_it_answers_a_retry() {
    let store = Store::new();
    let now = wall_now();
    let (expired, residue, fresh) = (upload(0x91), upload(0x93), upload(0x92));
    let in_flight = chunk(0x93_0002, [4, 5, 6]);
    store.seed(vec![
        ledger(3, W_REF),
        completed_session(&expired, now - 3 * DAY, "wall", now - 2 * DAY),
        completed_session(&residue, now - 3 * DAY, "wall", now - 2 * DAY),
        slot(&residue, 0, 2, now - 3 * DAY, now - 3 * DAY + MINUTE),
        owned(&residue, 2, &in_flight, now - 3 * DAY + MINUTE),
        completed_session(&fresh, now - DAY, "wall", now),
    ]);
    let fresh_before = store.value(&mpu_key(&fresh));

    store.reap_until(|store| session_gone(store, &expired) && session_gone(store, &residue));
    still_inside_the_window(now);

    assert_torn_down(&store, &expired);
    assert_torn_down(&store, &residue);
    for position in positions(&in_flight) {
        // The walk's own mark: stamped, naming no retirement token.
        let mark = stamped_mark(&store, position);
        assert_eq!(
            mark.retire_token(),
            None,
            "{position:?} is the owned walk's mark"
        );
    }
    assert_eq!(
        store.counted(),
        1,
        "exactly two admission slots are released"
    );
    assert_eq!(
        store.value(&mpu_key(&fresh)),
        fresh_before,
        "the tombstone inside its window is kept byte for byte"
    );
    let session = store.session(&fresh).expect("the fresh tombstone");
    let retried = Digest::from_hex(&fingerprint()).expect("a digest");
    assert!(
        matches!(
            complete_answer(Some(session.state()), Some(&retried)),
            CompleteAnswer::AlreadyCompleted(_)
        ),
        "an identical Complete retry still answers AlreadyCompleted"
    );
    // And the window is not judged once and forgotten: another one-shot keeps the fresh tombstone.
    store.reap_once();
    still_inside_the_window(now);
    assert_eq!(store.value(&mpu_key(&fresh)), fresh_before);
    assert_eq!(store.counted(), 1);
}

/// **H-terminal — a profile mismatch refuses the `Completing` arms too.** The ledger stores a
/// profile with twice the shipped `W_ref`, beside a local-clock `Completing` session whose fence is
/// two days stale and an over-age one: the one-shot emits the operator signal, exits non-zero, and
/// the store is byte-identical — neither is rolled back or fenced. Vacuous on the base, which has
/// no `Completing` arm to refuse.
#[test]
fn profile_mismatch_refuses_the_completing_arms() {
    let store = Store::new();
    let now = wall_now();
    let (stale, over_age) = (upload(0x81), upload(0x82));
    let chunks = [chunk(0x81_0001, [1, 2, 3])];
    let mut records = vec![
        ledger(2, 2 * W_REF),
        completing_session(&stale, now - 3 * DAY, "wall", 1, now - 2 * DAY, 1),
        completing_session(&over_age, now - 40 * DAY, "wall", 1, now - 2 * DAY, 0),
    ];
    records.extend(committed_part(&stale, 1, &chunks, now - 3 * DAY));
    records.extend(segments(&stale, 1, &chunks));
    store.seed(records);
    let before = store.snapshot();

    let (status, stderr) = store.reap_once();

    assert!(
        stderr.contains("reaper-profile-mismatch"),
        "the operator signal:\n{stderr}"
    );
    assert!(!status.success(), "a refused pass exits non-zero: {status}");
    assert_eq!(
        store.snapshot(),
        before,
        "not one byte of the store changed"
    );
}

/// **F, idempotence — re-running the one-shot over a partially drained teardown converges once.**
/// An over-age `Completing` session the reaper fences, with three committed parts and their
/// segments, beside a session that is kept. After the first one-shot its teardown is under way —
/// the session `Aborting`, an obligation still owed — and every later one-shot goes on from there:
/// the count never drops below what the store holds, the session is deleted exactly once, and two
/// one-shots after it is gone change nothing and lower the count no further.
#[test]
fn rerunning_the_one_shot_over_a_partial_teardown_converges_with_no_double_decrement() {
    let store = Store::new();
    let now = wall_now();
    let (id, kept) = (upload(0xf1), upload(0xf2));
    let chunks = [
        chunk(0xf1_0001, [1, 2, 3]),
        chunk(0xf1_0002, [4, 5, 6]),
        chunk(0xf1_0003, [1, 2, 3]),
    ];
    let mut records = vec![
        ledger(2, W_REF),
        completing_session(&id, now - 40 * DAY, "wall", 1, now - 2 * DAY, 3),
        open_session(&kept, now - DAY, "wall"),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &id,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 39 * DAY,
        ));
    }
    records.extend(segments(&id, 1, &chunks));
    store.seed(records);

    store.reap_once();
    let session = store
        .session(&id)
        .expect("one one-shot leaves the teardown under way");
    assert!(
        matches!(session.state(), SessionState::Aborting {}),
        "fenced by the reaper"
    );
    assert_eq!(session.epoch(), 2, "at E+1");
    assert!(!owes_nothing(&store, &id), "an obligation is still owed");
    assert_eq!(
        store.counted(),
        2,
        "no slot released before the terminal delete"
    );

    let mut counts = Vec::new();
    for _ in 0..CONVERGE_RUNS {
        if session_gone(&store, &id) && owes_nothing(&store, &id) {
            break;
        }
        store.reap_once();
        counts.push(store.counted());
    }
    assert!(
        session_gone(&store, &id),
        "converged within {CONVERGE_RUNS}"
    );
    assert_torn_down(&store, &id);
    assert!(
        counts.iter().all(|&count| count == 2 || count == 1),
        "the count only ever steps from 2 to 1: {counts:?}"
    );
    assert_eq!(store.counted(), 1, "exactly one decrement");

    let settled = store.snapshot();
    for _ in 0..2 {
        store.reap_once();
    }
    assert_eq!(store.counted(), 1, "no double decrement");
    assert_eq!(
        records_of(&store.snapshot(), &id),
        records_of(&settled, &id),
        "nothing of the torn-down session reappears"
    );
    assert!(store.session(&kept).is_some(), "the other session is kept");
}

/// **F, the wait — the reaper's teardown waits for GC's decision.** An over-age `Completing`
/// session the reaper fences to `Aborting@2`, one of whose part positions already carries a
/// `reclaiming` mark of another event: GC has decided to reclaim that fragment and not finished.
/// After the one-shot the `mpu:` record is present, the count unchanged, the mark byte-identical
/// and the bytes obligation `s:<id>:1` still owed. Deleting the key — GC's blind delete once it
/// has destroyed the fragment — lets the next one-shots mark the position afresh under this
/// teardown's token (never over the `reclaiming` mark), remove the obligation and the session, and
/// lower the count by exactly one.
#[test]
fn the_reapers_teardown_waits_for_a_reclaiming_mark_and_resumes_once_gc_deletes_it() {
    let store = Store::new();
    let now = wall_now();
    let (id, kept) = (upload(0xf3), upload(0xf4));
    let chunks = [chunk(0xf3_0001, [1, 2, 3]), chunk(0xf3_0002, [4, 5, 6])];
    let held = positions(&chunks[0])[0];
    let held_key = orphan_key(held.0, held.1);
    let reclaiming = encode_orphan_mark(
        &OrphanMark::structured(now - DAY, "g:77:1")
            .expect("a structured mark")
            .into_reclaiming(),
    );
    let mut records = vec![
        ledger(2, W_REF),
        completing_session(&id, now - 40 * DAY, "wall", 1, now - 2 * DAY, 0),
        open_session(&kept, now - DAY, "wall"),
        (held_key.clone(), reclaiming.clone()),
    ];
    for (n, chunk) in chunks.iter().enumerate() {
        records.extend(committed_part(
            &id,
            n as u32 + 1,
            std::slice::from_ref(chunk),
            now - 39 * DAY,
        ));
    }
    store.seed(records);
    let bytes_obligation = retire_key(RetireMode::Bytes, &session_token(&id, 1));

    for _ in 0..2 {
        store.reap_once();
    }

    let session = store.session(&id).expect("the record waits");
    assert!(matches!(session.state(), SessionState::Aborting {}));
    assert_eq!(session.epoch(), 2, "fenced at E+1");
    assert_eq!(store.counted(), 2, "the count is unchanged");
    assert_eq!(
        store.value(&held_key),
        Some(reclaiming),
        "the reclaiming mark is byte-identical"
    );
    assert!(
        store.value(&bytes_obligation).is_some(),
        "the obligation is still owed"
    );

    store.delete(&held_key);
    store.reap_until(|store| session_gone(store, &id) && owes_nothing(store, &id));

    assert_torn_down(&store, &id);
    let fresh = stamped_mark(&store, held);
    assert_eq!(
        fresh.retire_token(),
        Some(session_token(&id, 1)),
        "marked afresh under this teardown's token"
    );
    assert_eq!(store.counted(), 1, "the count drops by exactly one");
    assert!(store.session(&kept).is_some(), "the other session is kept");
}
