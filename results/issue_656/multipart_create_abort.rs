//! Issue #656 — the multipart protocol's first store round trips, driven over a real
//! `RedbMetadataStore` with real preconditions and real contention:
//! `CreateMultipartUpload`'s admission through the serialized fleet ledger `mpuctl` (proposal
//! 0016 `:657`, `:2000-2039`) and `AbortMultipartUpload`'s teardown fence (`:665`, decision 3's
//! row `:977`), both `wyrd_core::multipart::store`.
//!
//! The legs, by the brief's letters:
//!
//! * **(A)** 8, 10 and 16 free-running concurrent creates, each level on a fresh store, on a
//!   multi-thread runtime: every one admitted, ids distinct, and the store holds exactly N
//!   sessions, N markers and a ledger counting N.
//! * **(A′)** 8 creates through [`Interposer`]'s commit barrier, which holds every commit until
//!   every live creator has read the ledger and built its batch — so each round has exactly one
//!   winner and every other creator loses the ledger CAS. The last creator loses it 7 times,
//!   more than `MAX_UPLOAD_ID_ATTEMPTS`, and still gets its session. A collision-sized or shared
//!   budget fails this on every run, not on an unlucky schedule. The same barrier with 47
//!   creators under a profile larger than the shipped one shows the contention budget follows the
//!   profile's own session limit, not the shipped 46.
//! * **(B)** the real bounds — the stored session cap, a profile skew, an absent bucket, a
//!   bucket marker deleted or rewritten between Create's read and its commit, and a session too
//!   large for a state it can later reach, found at the exact encoded-byte boundary through a
//!   store that enforces the backend value ceiling — each leaving the store byte-identical.
//! * **(C)** the two retry budgets kept apart: a collision re-mints, every mint colliding fails,
//!   a collision — found by the read before an identity's first commit, or by a lost commit —
//!   does not spend the contention budget (nor a lost CAS the collision one), and an identity
//!   source that repeats an existing session's identity is re-minted, never handed that session,
//!   even when its first commit's outcome is unknown.
//! * **(D)** Abort per decision 3, including two Aborts forced to race one `Open` session, and the
//!   last round's re-read answered like any other. A fence that may still land (1031) stays in
//!   doubt across later out-of-flight outcomes (1021), a failed re-read and a later plain fault,
//!   and is what an Abort ending without an answer reports — released after the Abort returned,
//!   it lands; only a session seen moved clears that doubt.
//! * **(E)** Abort never moves `mpuctl.count`, and a `Completed` tombstone stays counted.
//! * **(F)** an unknown Create outcome — applied, not applied, and applied *late* (after the
//!   re-drive, or between the recovery's reads) — recovered under the original upload id, with one
//!   session, one marker and one increment; settled even on the create's last permitted attempt;
//!   a batch left in flight still in doubt after a later out-of-flight outcome; and only a batch
//!   in doubt can make a found session this create's own, and only one equal to it in every field
//!   the create wrote.
//!
//! [`Interposer`] keeps all its state in struct fields, never a `static` (ADR-0035).
//!
//! No `#![cfg(...)]` here — this file always compiles and always runs.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Notify;
use wyrd_core::metadata::{
    self, bucket_key, seggrp_key, InodeId, SegmentNonce, MAX_SEGMENT_INDEX, MAX_VALUE_BYTES,
    SEGGRP_MARKER, SEGGRP_PREFIX,
};
use wyrd_core::multipart::store::{
    abort_session, create_session, fence_batch, AbortFault, CreateEnd, CreateFault, CreateHost,
    CreateRequest, UnsettledCreate,
};
use wyrd_core::multipart::{
    admission_backoff_millis, decode_admission_record, decode_retire_obligation,
    decode_session_record, mpu_key, parse_mpu_key, retire_key, AbortOutcome, AdmissionRecord,
    Backpressure, Budget, CreateOutcome, PartScope, Refusal, RetireMode, RetireToken,
    SessionRecord, SessionState, UploadId, MAX_ADMISSION_CAS_ATTEMPTS, MAX_COMPLETE_ATTEMPTS,
    MAX_INFLIGHT_PARTS, MAX_PARTS_PER_SESSION, MAX_PART_CHUNKS, MAX_PART_NUMBER, MAX_SESSIONS,
    MAX_STAGED_CHUNKS, MAX_UPLOAD_ID_ATTEMPTS, MPUCTL_KEY, MPU_PREFIX, W_REF,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_traits::{
    BoxError, CommitOutcome, CommitUnknownResult, MetadataStore, Result, ScanPage, WriteBatch,
};

// ───────────────────────────────── fixtures ─────────────────────────────────

const BUCKET: &str = "photos";
/// A bucket marker as #511's CreateBucket will write one (ADR-0046 decision 2: name and
/// creation time). Create treats it as opaque bytes and requires them exactly.
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":7}"#;
/// The flat-namespace root the gateway binds `{bucket}/{key}` dirents under.
const PARENT: InodeId = 1;
const CREATED_AT: u64 = 1_000;
const CLOCK: &str = "wall";
/// No leg may hang the suite: a stalled barrier or a create that never ends fails here.
const LEG_DEADLINE: Duration = Duration::from_secs(60);
/// What the interposer's plain store fault says.
const INJECTED_FAULT: &str = "an injected store fault";
/// Commits one interposer forwards before it calls the writer runaway — far above any leg's
/// legitimate count (A′ at 47 creators makes 1128). redb's `get` and `commit` never await
/// inside, so a writer looping on them never yields, and [`LEG_DEADLINE`] could not fire: the
/// interposer stops it instead.
const COMMIT_CEILING: u32 = 4096;

fn request(object: &str) -> CreateRequest {
    CreateRequest {
        bucket: BUCKET.to_owned(),
        parent: PARENT,
        object: format!("{BUCKET}/{object}"),
        content_type: None,
        created_at_millis: CREATED_AT,
        clock_source: CLOCK.to_owned(),
    }
}

/// A profile through the one validating route a `Budget` has: its own decode.
fn profile(w_ref: u64, part_chunks: u32, parts: u32, inflight: u32, staged: u32) -> Budget {
    let json = format!(
        "{{\"w_ref\":{w_ref},\"max_part_chunks\":{part_chunks},\"max_parts_per_session\":\
         {parts},\"max_inflight_parts\":{inflight},\"max_staged_chunks\":{staged}}}"
    );
    metadata::decode(json.as_bytes()).expect("a valid profile")
}

/// The shipped profile — the one `MAX_SESSIONS` and `MAX_ADMISSION_CAS_ATTEMPTS` derive from.
fn shipped() -> Budget {
    let budget = profile(
        W_REF,
        MAX_PART_CHUNKS,
        MAX_PARTS_PER_SESSION,
        MAX_INFLIGHT_PARTS,
        MAX_STAGED_CHUNKS,
    );
    assert_eq!(budget.max_sessions(), MAX_SESSIONS);
    budget
}

fn hex(bits: u128) -> String {
    format!("{bits:032x}")
}

fn upload_id(bits: u128) -> UploadId {
    UploadId::new(hex(bits)).expect("32 lowercase hex digits")
}

/// The `seggrp:` key of the nonce minted from `bits`.
fn marker_key(bits: u128) -> Vec<u8> {
    seggrp_key(&SegmentNonce::new(hex(bits)).expect("32 lowercase hex digits"))
}

/// A fresh in-memory store holding the bucket marker and nothing else.
async fn store_with_bucket() -> Arc<RedbMetadataStore> {
    let store = Arc::new(RedbMetadataStore::in_memory().expect("an in-memory store"));
    let put = WriteBatch::new().put(bucket_key(BUCKET), MARKER);
    assert_eq!(store.commit(put).await.unwrap(), CommitOutcome::Committed);
    store
}

/// Every key and value in the store.
async fn dump(store: &impl MetadataStore) -> BTreeMap<Vec<u8>, Bytes> {
    store.scan(b"").await.unwrap().into_iter().collect()
}

async fn ledger(store: &impl MetadataStore) -> AdmissionRecord {
    let bytes = store.get(MPUCTL_KEY).await.unwrap().expect("a ledger");
    decode_admission_record(&bytes).expect("a canonical ledger")
}

async fn session(store: &impl MetadataStore, id: &UploadId) -> (Bytes, SessionRecord) {
    let bytes = store.get(&mpu_key(id)).await.unwrap().expect("a session");
    let record = decode_session_record(&bytes).expect("a canonical session");
    (bytes, record)
}

/// The caller's side of a create: identity bits (a script first, then a counter from `base`),
/// jitter, and a wait — each recorded.
struct Host {
    script: VecDeque<u128>,
    next: u128,
    minted: Vec<u128>,
    waits: Vec<u64>,
    jitters: Vec<u64>,
    jitter: u64,
    /// Record each wait without sleeping it out — for legs whose schedule the interposer forces,
    /// where the wait changes nothing but the run time.
    instant: bool,
}

impl Host {
    fn new(base: u128) -> Self {
        Self::scripted(base, &[])
    }

    fn scripted(base: u128, script: &[u128]) -> Self {
        Self {
            script: script.iter().copied().collect(),
            next: base,
            minted: Vec::new(),
            waits: Vec::new(),
            jitters: Vec::new(),
            jitter: base as u64 ^ 0x9e37_79b9_7f4a_7c15,
            instant: false,
        }
    }

    fn instant(mut self) -> Self {
        self.instant = true;
        self
    }

    /// The identities this host minted, as `(upload id bits, nonce bits)` pairs.
    fn identities(&self) -> Vec<(u128, u128)> {
        self.minted
            .chunks(2)
            .map(|pair| (pair[0], pair[1]))
            .collect()
    }

    /// Every wait is exactly the backoff for its lost CAS: the `i`-th wait (from 0) is
    /// `admission_backoff_millis(i, the jitter drawn for it)`. So the window grows with each
    /// loss — doubling from `BASE` toward `CAP` — rather than staying at, or skipping past, any
    /// one size.
    fn assert_backoff(&self) {
        assert_eq!(self.waits.len(), self.jitters.len(), "one jitter per wait");
        for (attempt, (wait, jitter)) in self.waits.iter().zip(&self.jitters).enumerate() {
            let attempt = u32::try_from(attempt).unwrap();
            assert_eq!(
                *wait,
                admission_backoff_millis(attempt, *jitter),
                "backoff after lost CAS {attempt}"
            );
        }
    }
}

impl CreateHost for Host {
    fn mint(&mut self) -> u128 {
        let bits = self.script.pop_front().unwrap_or_else(|| {
            self.next += 1;
            self.next
        });
        self.minted.push(bits);
        bits
    }

    fn jitter(&mut self) -> u64 {
        self.jitter = self
            .jitter
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.jitters.push(self.jitter);
        self.jitter
    }

    fn wait(&mut self, millis: u64) -> impl Future<Output = ()> + Send {
        self.waits.push(millis);
        let millis = if self.instant { 0 } else { millis };
        tokio::time::sleep(Duration::from_millis(millis))
    }
}

/// A disjoint identity range per creator, so every minted id is distinct by construction and an
/// id names the creator that minted it.
fn base(creator: usize) -> u128 {
    (creator as u128 + 1) << 64
}

fn created(outcome: Result<CreateOutcome>) -> UploadId {
    match outcome.expect("the create answered") {
        CreateOutcome::Created { upload_id } => upload_id,
        CreateOutcome::Refused(refusal) => panic!("the create was refused: {refusal:?}"),
    }
}

/// The create ended `Unsettled`: return what it carries.
fn as_unsettled(err: &BoxError) -> &UnsettledCreate {
    match err.downcast_ref::<CreateFault>() {
        Some(CreateFault::Unsettled(unsettled)) => unsettled,
        other => panic!("expected Unsettled, got {other:?} ({err})"),
    }
}

/// Exactly the sessions `admitted` names exist, each `Open` for the request that created it,
/// each with its own `seggrp:` marker, and `mpuctl` counts exactly them — plus `extra_markers`,
/// `seggrp:` markers that outlived their session and are deliberately not counted.
async fn assert_admitted(
    store: &impl MetadataStore,
    admitted: &[(UploadId, CreateRequest)],
    budget: &Budget,
    extra_markers: &[u128],
) {
    let ids: BTreeSet<&str> = admitted.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids.len(),
        admitted.len(),
        "upload ids must be pairwise distinct"
    );

    let sessions = store.scan(MPU_PREFIX).await.unwrap();
    assert_eq!(
        sessions.len(),
        admitted.len(),
        "exactly one mpu: record per admitted create"
    );
    let mut nonces = BTreeSet::new();
    for (key, bytes) in &sessions {
        let id = parse_mpu_key(key).expect("a canonical mpu: key");
        let (_, wanted) = admitted
            .iter()
            .find(|(admitted, _)| admitted == &id)
            .unwrap_or_else(|| panic!("mpu:{id} was never answered Created"));
        let record = decode_session_record(bytes).expect("a canonical session");
        assert_eq!(record.state(), &SessionState::Open {}, "mpu:{id}");
        assert_eq!(record.parent(), wanted.parent, "mpu:{id}");
        assert_eq!(record.object(), wanted.object, "mpu:{id}");
        assert_eq!(record.content_type(), wanted.content_type.as_deref());
        assert_eq!(record.created_at_millis(), wanted.created_at_millis);
        assert_eq!(record.clock_source(), wanted.clock_source);
        assert_eq!((record.epoch(), record.attempts()), (0, 0), "mpu:{id}");
        assert_eq!(
            &metadata::encode(&record),
            bytes,
            "a session is the codec's own spelling"
        );
        assert!(nonces.insert(seggrp_key(record.segment_nonce())));
    }

    let markers = store.scan(SEGGRP_PREFIX).await.unwrap();
    let mut expected = nonces;
    expected.extend(extra_markers.iter().map(|bits| marker_key(*bits)));
    let found: BTreeSet<Vec<u8>> = markers.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(
        found, expected,
        "exactly one seggrp: marker per session's nonce"
    );
    assert!(markers
        .iter()
        .all(|(_, value)| value.as_ref() == SEGGRP_MARKER));

    let ledger = ledger(store).await;
    assert_eq!(
        ledger.count(),
        admitted.len() as u64,
        "mpuctl counts every admitted session"
    );
    assert_eq!(ledger.profile(), budget);
    assert_eq!(ledger.max_sessions(), budget.max_sessions());
}

// ─────────────────────────────── the interposer ───────────────────────────────

/// A test-local `MetadataStore` over the real redb store that controls **when** a commit runs
/// and what it is answered, and records what each commit lost. Every read and every forwarded or
/// applied batch goes to the real store, whose preconditions decide every `Conflict`.
struct Interposer {
    inner: Arc<RedbMetadataStore>,
    plan: Plan,
    state: Mutex<State>,
    turn: Notify,
}

enum Plan {
    /// Forward every call.
    PassThrough,
    /// Hold every commit until every live participant has reached its commit — so all of a
    /// round's batches are built from the same reads — then release the round. A participant
    /// leaves when a commit of its lands (or errors).
    Barrier { participants: usize },
    /// Before forwarding the first commit, delete the bucket marker (`None`) or rewrite it.
    MoveBucket { to: Option<&'static [u8]> },
    /// Refuse, as FoundationDB does (`value_too_large`), any commit putting a value longer than
    /// [`MAX_VALUE_BYTES`] — with a plain store fault, applying nothing; forward every other.
    ValueCeiling,
    /// Act on commits by their number, from 1.
    Script(Script),
}

/// What the interposer does at a numbered commit (from 1); a commit nothing names is forwarded.
struct Script {
    /// The profile rival creates admit against.
    budget: Budget,
    /// Before forwarding each of these commits, admit a rival session straight on the inner
    /// store — a real create that moves the ledger.
    rivals: Option<RangeInclusive<u32>>,
    /// Answer each of these commits with a `CommitUnknownResult`, the batch's fate by its kind.
    unknown: BTreeMap<u32, Unknown>,
    /// Before forwarding each of these commits, move the session the batch fences to `Open` two
    /// epochs on, with one more Complete attempt spent — what a Complete fence and its rollback
    /// leave (#658).
    reopen: Option<RangeInclusive<u32>>,
    /// Before forwarding this commit, a rival Abort of the session the batch fences runs to its
    /// answer on the inner store.
    rival_abort: Option<u32>,
    /// Answer this commit with a plain store fault — not a `CommitUnknownResult` — applying
    /// nothing.
    fault: Option<u32>,
    /// Fail the first point read after this commit with a plain store fault — the writer's
    /// re-read of what the commit did.
    read_fault: Option<u32>,
    /// Before forwarding this commit, put a `seggrp:` marker at this key straight on the inner
    /// store — a record that appears after the create read its identity's keys vacant.
    plant_marker: Option<(u32, Vec<u8>)>,
}

impl Script {
    fn new(budget: Budget) -> Self {
        Self {
            budget,
            rivals: None,
            unknown: BTreeMap::new(),
            reopen: None,
            rival_abort: None,
            fault: None,
            read_fault: None,
            plant_marker: None,
        }
    }
}

/// What became of a batch the store answered `CommitUnknownResult`.
#[derive(Debug, Clone, Copy)]
enum Unknown {
    /// It was applied before the error.
    Applied { may_still_commit: bool },
    /// It was not applied, and will not be.
    Dropped { may_still_commit: bool },
    /// It was not applied, and a rival then moved the ledger.
    DroppedThenRival,
    /// It was not applied, and another session then took its `mpu:<id>`: the batch's own
    /// session with one field changed — a 2^-128 event by chance, forced here.
    DroppedThenSquatted(Squat),
    /// It is held, and applied right after the **next** commit is forwarded — the recovery
    /// re-drive.
    Late,
    /// It is held, and applied at the next read of `mpuctl` — after the recovery has looked at
    /// `mpu:<id>` and before it reads the ledger.
    LateAtLedgerRead,
    /// It is held — still in flight — until the test releases it ([`Interposer::release`]),
    /// whatever the writer does meanwhile.
    Held,
}

/// The one field a squatting session changes — each one Create wrote and no transition rewrites.
#[derive(Debug, Clone, Copy)]
enum Squat {
    Nonce,
    Object,
    Parent,
    ContentType,
    CreatedAt,
    ClockSource,
}

/// When a held batch lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    AfterNextCommit,
    AtLedgerRead,
    OnRelease,
}

/// Which of a lost batch's preconditions failed when re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Lost {
    /// `require(mpuctl == …)` / `require_absent(mpuctl)`.
    ledger: bool,
    /// `require_absent(mpu:<id>)` / `require_absent(seggrp:<nonce>)`.
    collision: bool,
    /// Anything else (the bucket marker, a session's exact bytes, an obligation key).
    other: bool,
}

#[derive(Default)]
struct State {
    commits: u32,
    live: usize,
    arrived: usize,
    generation: u64,
    /// Ledger-CAS losses per creator, by the upload id its batch puts under `mpu:`.
    ledger_losses: BTreeMap<String, u32>,
    lost: Vec<Lost>,
    committed: Vec<WriteBatch>,
    held: Option<(WriteBatch, Landing)>,
    /// The held batch's own outcome, and whether its `mpu:<id>` was present when it ran.
    late: Option<(CommitOutcome, bool)>,
    rivals: u32,
    rival_aborts: Vec<AbortOutcome>,
    /// Commits [`Plan::ValueCeiling`] refused.
    oversized: u32,
    /// Point reads made through the interposer.
    reads: u32,
    /// The next point read fails ([`Script::read_fault`]).
    fail_read: bool,
}

impl Interposer {
    fn new(inner: Arc<RedbMetadataStore>, plan: Plan) -> Self {
        let live = match &plan {
            Plan::Barrier { participants } => *participants,
            _ => 0,
        };
        Self {
            inner,
            plan,
            state: Mutex::new(State {
                live,
                ..State::default()
            }),
            turn: Notify::new(),
        }
    }

    fn scripted(inner: &Arc<RedbMetadataStore>, script: Script) -> Self {
        Self::new(Arc::clone(inner), Plan::Script(script))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    /// Release the round once everyone still live has arrived.
    fn release_if_full(&self, state: &mut State) {
        if state.arrived > 0 && state.arrived >= state.live {
            state.arrived = 0;
            state.generation += 1;
            self.turn.notify_waiters();
        }
    }

    async fn barrier(&self) {
        let generation = {
            let mut state = self.state();
            state.arrived += 1;
            let generation = state.generation;
            self.release_if_full(&mut state);
            generation
        };
        loop {
            // Registered before the check, so a release between the two is not missed.
            let released = self.turn.notified();
            if self.state().generation > generation {
                return;
            }
            tokio::time::timeout(Duration::from_secs(10), released)
                .await
                .expect("the barrier stalled: a live participant never reached its commit");
        }
    }

    /// Admit one rival session straight on the inner store.
    async fn rival(&self, budget: &Budget) {
        let n = {
            let mut state = self.state();
            state.rivals += 1;
            state.rivals
        };
        let mut host = Host::new((1 << 120) + (u128::from(n) << 32)).instant();
        let rival = request(&format!("rival-{n}"));
        created(create_session(&*self.inner, &rival, budget, &mut host).await);
    }

    /// A rival Abort of the session `batch` fences, run to its answer on the inner store.
    async fn rival_abort(&self, batch: &WriteBatch) {
        let key = session_key(batch).expect("a fence batch puts its session");
        let id = parse_mpu_key(&key).expect("a canonical mpu: key");
        let answer = abort_session(&*self.inner, &id)
            .await
            .expect("the rival Abort answered");
        self.state().rival_aborts.push(answer);
    }

    async fn unknown(&self, kind: Unknown, budget: &Budget, batch: WriteBatch) -> BoxError {
        let may_still_commit = match kind {
            Unknown::Applied { may_still_commit } => {
                let landed = self.inner.commit(batch).await.unwrap();
                assert_eq!(
                    landed,
                    CommitOutcome::Committed,
                    "the applied variant must land"
                );
                may_still_commit
            }
            Unknown::Dropped { may_still_commit } => may_still_commit,
            Unknown::DroppedThenRival => {
                self.rival(budget).await;
                true
            }
            Unknown::DroppedThenSquatted(squat) => {
                self.squat(squat, &batch).await;
                true
            }
            Unknown::Late => {
                self.state().held = Some((batch, Landing::AfterNextCommit));
                true
            }
            Unknown::LateAtLedgerRead => {
                self.state().held = Some((batch, Landing::AtLedgerRead));
                true
            }
            Unknown::Held => {
                self.state().held = Some((batch, Landing::OnRelease));
                true
            }
        };
        Box::new(CommitUnknownResult {
            backend: "interposer",
            code: None,
            detail: "forced by the test".to_owned(),
            may_still_commit,
        })
    }

    /// Install at the create batch's `mpu:<id>` the session it carries with one field changed.
    async fn squat(&self, squat: Squat, batch: &WriteBatch) {
        let (key, value) = batch
            .puts
            .iter()
            .find(|(key, _)| key.starts_with(MPU_PREFIX))
            .expect("a create batch puts its session");
        let own = decode_session_record(value).unwrap();
        let json = String::from_utf8(value.to_vec()).unwrap();
        let created_at = format!("\"created_at_millis\":{},", own.created_at_millis());
        let changed = match squat {
            Squat::Nonce => json.replacen(own.segment_nonce().as_str(), &hex(0x5e17), 1),
            Squat::Object => json.replacen(
                &format!("\"object\":\"{}\"", own.object()),
                "\"object\":\"photos/squatter\"",
                1,
            ),
            Squat::Parent => json.replacen(
                &format!("\"parent\":{},", own.parent()),
                &format!("\"parent\":{},", own.parent() + 1),
                1,
            ),
            Squat::ContentType => match own.content_type() {
                Some(declared) => json.replacen(
                    &format!("\"content_type\":\"{declared}\""),
                    "\"content_type\":\"application/x-squatter\"",
                    1,
                ),
                None => json.replacen(
                    &created_at,
                    &format!("\"content_type\":\"application/x-squatter\",{created_at}"),
                    1,
                ),
            },
            Squat::CreatedAt => json.replacen(
                &created_at,
                &format!("\"created_at_millis\":{},", own.created_at_millis() + 1),
                1,
            ),
            Squat::ClockSource => json.replacen(
                &format!("\"clock_source\":\"{}\"", own.clock_source()),
                "\"clock_source\":\"squatter-clock\"",
                1,
            ),
        };
        let other = decode_session_record(changed.as_bytes()).expect("a canonical session");
        let same = [
            other.segment_nonce() == own.segment_nonce(),
            other.object() == own.object(),
            other.parent() == own.parent(),
            other.content_type() == own.content_type(),
            other.created_at_millis() == own.created_at_millis(),
            other.clock_source() == own.clock_source(),
        ];
        assert_eq!(
            same.iter().filter(|same| !**same).count(),
            1,
            "{squat:?}: exactly one field differs"
        );
        let put = WriteBatch::new().put(key.clone(), changed.into_bytes());
        assert_eq!(
            self.inner.commit(put).await.unwrap(),
            CommitOutcome::Committed
        );
    }

    /// Move the session `batch` fences to `Open@E+2` with one more attempt spent.
    async fn reopen(&self, batch: &WriteBatch) {
        let key = session_key(batch).expect("a fence batch puts its session");
        let bytes = self.inner.get(&key).await.unwrap().expect("a session");
        let record = decode_session_record(&bytes).unwrap();
        let (epoch, attempts) = (record.epoch(), record.attempts());
        let reopened = String::from_utf8(bytes.to_vec()).unwrap().replacen(
            &format!("\"epoch\":{epoch},\"attempts\":{attempts},"),
            &format!("\"epoch\":{},\"attempts\":{},", epoch + 2, attempts + 1),
            1,
        );
        let moved = decode_session_record(reopened.as_bytes()).expect("a canonical session");
        assert_eq!(
            (moved.epoch(), moved.state()),
            (epoch + 2, &SessionState::Open {})
        );
        let put = WriteBatch::new().put(key, reopened.into_bytes());
        assert_eq!(
            self.inner.commit(put).await.unwrap(),
            CommitOutcome::Committed
        );
    }

    /// Apply the held batch now, if it lands `at` this point.
    async fn land_held(&self, at: Landing) {
        let held = {
            let mut state = self.state();
            match &state.held {
                Some((_, landing)) if *landing == at => state.held.take(),
                _ => None,
            }
        };
        let Some((held, _)) = held else { return };
        self.apply_held(held).await;
    }

    /// Apply the held batch now, however it was to land — the in-flight batch landing **after**
    /// the writer returned. Returns its own outcome.
    async fn release(&self) -> CommitOutcome {
        let (held, _) = self.state().held.take().expect("a held batch to release");
        self.apply_held(held).await;
        self.state().late.expect("the released batch's outcome").0
    }

    async fn apply_held(&self, held: WriteBatch) {
        let key = session_key(&held).expect("a batch puts its session");
        let present = self.inner.get(&key).await.unwrap().is_some();
        let outcome = self.inner.commit(held).await.unwrap();
        self.state().late = Some((outcome, present));
    }

    /// Re-read each precondition of a lost batch and name the ones that no longer hold.
    async fn lost(&self, batch: &WriteBatch) -> Lost {
        let mut lost = Lost::default();
        for pre in &batch.preconditions {
            let now = self.inner.get(&pre.key).await.unwrap();
            if now == pre.expected {
                continue;
            }
            if pre.key == MPUCTL_KEY {
                lost.ledger = true;
            } else if pre.expected.is_none()
                && (pre.key.starts_with(MPU_PREFIX) || pre.key.starts_with(SEGGRP_PREFIX))
            {
                lost.collision = true;
            } else {
                lost.other = true;
            }
        }
        lost
    }

    async fn observe(&self, batch: &WriteBatch, outcome: &Result<CommitOutcome>) {
        match outcome {
            Ok(CommitOutcome::Conflict) => {
                let lost = self.lost(batch).await;
                let mut state = self.state();
                if lost.ledger {
                    if let Some(id) = session_key(batch) {
                        let id = String::from_utf8_lossy(&id).into_owned();
                        *state.ledger_losses.entry(id).or_default() += 1;
                    }
                }
                state.lost.push(lost);
            }
            Ok(CommitOutcome::Committed) | Err(_) => {
                let mut state = self.state();
                if outcome.is_ok() {
                    state.committed.push(batch.clone());
                }
                if matches!(self.plan, Plan::Barrier { .. }) {
                    state.live -= 1;
                    self.release_if_full(&mut state);
                }
            }
        }
    }
}

/// The `mpu:` key a batch puts — its creator's (or its fenced session's) identity.
fn session_key(batch: &WriteBatch) -> Option<Vec<u8>> {
    batch
        .puts
        .iter()
        .map(|(key, _)| key)
        .find(|key| key.starts_with(MPU_PREFIX))
        .cloned()
}

#[async_trait]
impl MetadataStore for Interposer {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let fail = {
            let mut state = self.state();
            state.reads += 1;
            std::mem::take(&mut state.fail_read)
        };
        if fail {
            return Err(INJECTED_FAULT.into());
        }
        if key == MPUCTL_KEY {
            self.land_held(Landing::AtLedgerRead).await;
        }
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
    ) -> Result<ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let n = {
            let mut state = self.state();
            state.commits += 1;
            state.commits
        };
        assert!(
            n <= COMMIT_CEILING,
            "the writer made more than {COMMIT_CEILING} commits: its retry loop is unbounded"
        );
        match &self.plan {
            Plan::PassThrough => {}
            Plan::Barrier { .. } => self.barrier().await,
            Plan::MoveBucket { to } => {
                if n == 1 {
                    let moved = match to {
                        Some(bytes) => WriteBatch::new().put(bucket_key(BUCKET), *bytes),
                        None => WriteBatch::new().delete(bucket_key(BUCKET)),
                    };
                    self.inner.commit(moved).await.unwrap();
                }
            }
            Plan::ValueCeiling => {
                let widest = batch.puts.iter().map(|(_, value)| value.len()).max();
                if let Some(bytes) = widest.filter(|bytes| *bytes > MAX_VALUE_BYTES) {
                    self.state().oversized += 1;
                    return Err(format!(
                        "value_too_large: a {bytes}-byte value, past {MAX_VALUE_BYTES}"
                    )
                    .into());
                }
            }
            Plan::Script(script) => {
                if script.read_fault == Some(n) {
                    self.state().fail_read = true;
                }
                if let Some((at, key)) = &script.plant_marker {
                    if *at == n {
                        let plant = WriteBatch::new().put(key.clone(), SEGGRP_MARKER);
                        self.inner.commit(plant).await.unwrap();
                    }
                }
                if script.rivals.as_ref().is_some_and(|at| at.contains(&n)) {
                    self.rival(&script.budget).await;
                }
                if script.reopen.as_ref().is_some_and(|at| at.contains(&n)) {
                    self.reopen(&batch).await;
                }
                if script.rival_abort == Some(n) {
                    self.rival_abort(&batch).await;
                }
                if script.fault == Some(n) {
                    return Err(INJECTED_FAULT.into());
                }
                if let Some(kind) = script.unknown.get(&n) {
                    return Err(self.unknown(*kind, &script.budget, batch).await);
                }
            }
        }
        let outcome = self.inner.commit(batch.clone()).await;
        self.observe(&batch, &outcome).await;
        self.land_held(Landing::AfterNextCommit).await;
        outcome
    }
}

async fn within<T>(leg: impl Future<Output = T>) -> T {
    tokio::time::timeout(LEG_DEADLINE, leg)
        .await
        .expect("the leg ran past its deadline")
}

// ──────────────────────────── (A) free-running admission ────────────────────────────

/// **(A)** 8, 10 and 16 concurrent creates — the levels #508's rejected attempts falsely refused
/// (8 → 2 refused, 10 → 3, 16 → 3; aws-cli's default `max_concurrent_requests` is 10) — each
/// level on its own fresh store, all asserted. Not the red (redb's `get` and `commit` never await
/// inside, so a valid schedule may serialize them); (A′) forces the losing order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_free_running_creates_are_all_admitted_at_8_10_and_16() {
    let budget = shipped();
    for creators in [8usize, 10, 16] {
        let store = store_with_bucket().await;
        let tasks: Vec<_> = (0..creators)
            .map(|creator| {
                let store = Arc::clone(&store);
                tokio::spawn(async move {
                    // Half declare a content type, so both spellings of the optional field are
                    // written and must re-encode byte-identically.
                    let request = CreateRequest {
                        content_type: (creator % 2 == 1).then(|| "image/png".to_owned()),
                        ..request(&format!("obj-{creator}"))
                    };
                    let mut host = Host::new(base(creator));
                    let outcome = create_session(&*store, &request, &budget, &mut host).await;
                    (request, outcome)
                })
            })
            .collect();
        let mut admitted = Vec::new();
        for task in tasks {
            let (request, outcome) = within(task).await.unwrap();
            admitted.push((created(outcome), request));
        }
        assert_admitted(&*store, &admitted, &budget, &[]).await;
    }
}

// ─────────────────────────── (A′) admission under a forced schedule ───────────────────────────

/// `creators` creates admitting against `budget` through the commit barrier: every round, each
/// live creator has read the ledger before any of them may commit, so one wins and every other
/// loses the ledger CAS. Asserts every one is admitted, that the barrier forced exactly that shape
/// — the k-th admitted lost k-1 times, and lost nothing but the ledger CAS — that a lost CAS never
/// re-mints, and that every lost CAS is followed by exactly its backoff. Returns the loss counts,
/// sorted.
async fn forced_admission(budget: Budget, creators: usize, instant: bool) -> Vec<u32> {
    let inner = store_with_bucket().await;
    let store = Arc::new(Interposer::new(
        Arc::clone(&inner),
        Plan::Barrier {
            participants: creators,
        },
    ));
    let tasks: Vec<_> = (0..creators)
        .map(|creator| {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                let request = request(&format!("obj-{creator}"));
                let mut host = Host::new(base(creator));
                host.instant = instant;
                let outcome = create_session(&*store, &request, &budget, &mut host).await;
                (request, outcome, host)
            })
        })
        .collect();
    let mut admitted = Vec::new();
    let mut hosts = Vec::new();
    for task in tasks {
        let (request, outcome, host) = within(task).await.unwrap();
        admitted.push((created(outcome), request));
        hosts.push(host);
    }
    assert_admitted(&*inner, &admitted, &budget, &[]).await;

    let losses: Vec<u32> = {
        let state = store.state();
        assert!(
            state
                .lost
                .iter()
                .all(|lost| lost.ledger && !lost.collision && !lost.other),
            "under the barrier every lost commit lost the ledger CAS and nothing else: {:?}",
            state.lost
        );
        admitted
            .iter()
            .map(|(id, _)| {
                let key = String::from_utf8_lossy(&mpu_key(id)).into_owned();
                state.ledger_losses.get(&key).copied().unwrap_or(0)
            })
            .collect()
    };
    let mut forced = losses.clone();
    forced.sort_unstable();
    let rounds = u32::try_from(creators).unwrap();
    assert_eq!(
        forced,
        (0..rounds).collect::<Vec<_>>(),
        "the barrier forces one winner per round: the k-th admitted lost k-1 times"
    );
    // Losing the CAS spends no collision budget: each creator minted exactly one identity, and
    // backed off once per lost CAS, by the backoff for that loss.
    for (host, lost) in hosts.iter().zip(&losses) {
        assert_eq!(host.identities().len(), 1, "a lost CAS must never re-mint");
        assert_eq!(host.waits.len(), *lost as usize, "one backoff per lost CAS");
        host.assert_backoff();
    }
    forced
}

/// **(A′)** 8 creators through the commit barrier. The k-th admitted lost the ledger CAS exactly
/// `k - 1` times — so one creator lost 7 times, more than `MAX_UPLOAD_ID_ATTEMPTS` (2), and every
/// one is still admitted. A budget shared with, or sized like, the collision budget refuses the
/// creators of round 3 onward on every run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_prime_every_creator_is_admitted_through_more_lost_cas_than_the_collision_budget() {
    let forced = forced_admission(shipped(), 8, false).await;
    let most = *forced.last().unwrap();
    assert!(
        most > MAX_UPLOAD_ID_ATTEMPTS,
        "a creator lost the ledger CAS {most} times, past MAX_UPLOAD_ID_ATTEMPTS = \
         {MAX_UPLOAD_ID_ATTEMPTS}, and was still admitted"
    );
}

/// **(A′)** The contention budget is the admitting profile's own session limit, not the shipped
/// one's. Under a profile that admits more sessions than the shipped 46, the same barrier with
/// `MAX_ADMISSION_CAS_ATTEMPTS + 1` (47) creators makes the last one lose the ledger CAS 46
/// times — every loss another session admitted, with the ledger still far from full — and it is
/// admitted. A budget fixed at the shipped 46 refuses it with `AdmissionContention` at 46/93: the
/// #508 false `503`, on a host with more memory.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_prime_a_larger_profile_gets_the_contention_budget_its_own_session_limit_derives() {
    let larger = profile(
        2 * W_REF,
        MAX_PART_CHUNKS,
        MAX_PARTS_PER_SESSION,
        MAX_INFLIGHT_PARTS,
        MAX_STAGED_CHUNKS,
    );
    let creators = MAX_ADMISSION_CAS_ATTEMPTS as usize + 1;
    assert!(
        larger.max_sessions() > creators as u64,
        "the profile must have room for every creator: {}",
        larger.max_sessions()
    );
    let forced = forced_admission(larger, creators, true).await;
    assert_eq!(
        *forced.last().unwrap(),
        MAX_ADMISSION_CAS_ATTEMPTS,
        "the last creator lost the ledger CAS as often as the shipped budget allows in all, and \
         was still admitted"
    );
}

// ─────────────────────────── (B) the bounds, each writing nothing ───────────────────────────

/// **(B)** At the ledger's **stored** `max_sessions`, a create answers `SlowDown { SessionCap }`
/// and writes nothing. A two-session profile (`U_ref = 2`, `W_ref = 4`) reaches the cap with
/// real creates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_create_at_the_stored_session_cap_is_refused_and_writes_nothing() {
    let tiny = profile(4, 1, 1, 1, 1);
    assert_eq!(tiny.max_sessions(), 2);
    let store = store_with_bucket().await;
    let mut admitted = Vec::new();
    for creator in 0..2 {
        let request = request(&format!("obj-{creator}"));
        let mut host = Host::new(base(creator));
        let outcome = create_session(&*store, &request, &tiny, &mut host).await;
        admitted.push((created(outcome), request));
    }
    assert_admitted(&*store, &admitted, &tiny, &[]).await;

    let before = dump(&*store).await;
    let mut host = Host::new(base(2));
    let outcome = create_session(&*store, &request("obj-2"), &tiny, &mut host).await;
    let pressure = Backpressure::SessionCap {
        count: 2,
        max_sessions: 2,
    };
    assert_eq!(
        outcome.unwrap(),
        CreateOutcome::Refused(Refusal::SlowDown { pressure })
    );
    assert_eq!(
        dump(&*store).await,
        before,
        "a refused create writes nothing"
    );
}

/// **(B)** A create whose local profile differs from the stored one answers `ProfileSkew` and
/// writes nothing — even when both derive the same `max_sessions`: equal quotients can hide
/// unequal footprints, which is why the profile is stored whole (`0016:348`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_create_whose_profile_differs_from_the_ledger_is_refused_and_writes_nothing() {
    let stored = shipped();
    let store = store_with_bucket().await;
    let mut host = Host::new(base(0));
    created(create_session(&*store, &request("obj-0"), &stored, &mut host).await);

    let skewed = profile(
        W_REF - 1,
        MAX_PART_CHUNKS,
        MAX_PARTS_PER_SESSION,
        MAX_INFLIGHT_PARTS,
        MAX_STAGED_CHUNKS,
    );
    assert_ne!(skewed, stored);
    assert_eq!(skewed.max_sessions(), stored.max_sessions());
    let before = dump(&*store).await;
    let mut host = Host::new(base(1));
    let outcome = create_session(&*store, &request("obj-1"), &skewed, &mut host).await;
    assert_eq!(
        outcome.unwrap(),
        CreateOutcome::Refused(Refusal::ProfileSkew {
            local: skewed,
            ledger: stored,
        })
    );
    assert_eq!(
        dump(&*store).await,
        before,
        "a refused create writes nothing"
    );
}

/// **(B)** A create for a bucket with no `bucket:` record answers `NoSuchBucket` and writes
/// nothing — on an empty store (the bootstrap arm) and beside a live ledger (the CAS arm).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_create_for_an_absent_bucket_is_refused_and_writes_nothing() {
    let budget = shipped();
    let empty = RedbMetadataStore::in_memory().unwrap();
    let mut host = Host::new(base(0));
    let outcome = create_session(&empty, &request("obj"), &budget, &mut host).await;
    assert_eq!(
        outcome.unwrap(),
        CreateOutcome::Refused(Refusal::NoSuchBucket)
    );
    assert!(
        dump(&empty).await.is_empty(),
        "a refused create writes nothing"
    );

    let store = store_with_bucket().await;
    created(create_session(&*store, &request("obj-0"), &budget, &mut Host::new(base(1))).await);
    let before = dump(&*store).await;
    let elsewhere = CreateRequest {
        bucket: "videos".to_owned(),
        ..request("obj-1")
    };
    let outcome = create_session(&*store, &elsewhere, &budget, &mut Host::new(base(2))).await;
    assert_eq!(
        outcome.unwrap(),
        CreateOutcome::Refused(Refusal::NoSuchBucket)
    );
    assert_eq!(
        dump(&*store).await,
        before,
        "a refused create writes nothing"
    );
}

/// **(B)** A bucket marker deleted, or rewritten, between Create's read and its commit — the
/// interposer acting at commit — answers `NoSuchBucket`, and the create writes nothing: every
/// key but the marker the interposer itself moved is byte-identical.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_bucket_marker_moved_between_read_and_commit_answers_no_such_bucket() {
    const REWRITTEN: &[u8] = br#"{"name":"photos","created_at_millis":8}"#;
    let budget = shipped();
    for (to, with_ledger) in [(None, false), (Some(REWRITTEN), true), (None, true)] {
        let inner = store_with_bucket().await;
        if with_ledger {
            let mut host = Host::new(base(9));
            created(create_session(&*inner, &request("obj-0"), &budget, &mut host).await);
        }
        let mut before = dump(&*inner).await;
        let store = Interposer::new(Arc::clone(&inner), Plan::MoveBucket { to });
        let mut host = Host::new(base(1));
        let outcome = create_session(&store, &request("obj-1"), &budget, &mut host).await;
        assert_eq!(
            outcome.unwrap(),
            CreateOutcome::Refused(Refusal::NoSuchBucket)
        );
        match to {
            Some(bytes) => before.insert(bucket_key(BUCKET), Bytes::from_static(bytes)),
            None => before.remove(&bucket_key(BUCKET)),
        };
        assert_eq!(
            dump(&*inner).await,
            before,
            "no mpu:, no seggrp:, no mpuctl write (marker moved to {to:?})"
        );
        let state = store.state();
        assert_eq!(
            state.lost,
            vec![Lost {
                other: true,
                ..Lost::default()
            }]
        );
        assert!(state.committed.is_empty());
    }
}

/// The widest form of each state the session encoded as `open` (an `Open@0` record) can later be
/// rewritten to — `Open` after a rolled-back Complete, `Aborting`, `Completing`, `Completed` —
/// spelled from those very bytes with every number the decoder lets grow at its widest, and each
/// accepted by the real decoder. Returns `(state, encoded record)` pairs.
fn widest_later_states(open: &[u8]) -> Vec<(&'static str, String)> {
    let text = std::str::from_utf8(open).unwrap();
    let tail = "\"epoch\":0,\"attempts\":0,\"state\":{\"kind\":\"Open\"}}";
    assert!(text.ends_with(tail), "an Open@0 record: {text}");
    let head = &text[..text.len() - tail.len()];
    let record = decode_session_record(open).expect("a canonical session");
    // The object as a JSON string, escapes and all — `Completing` names it a second time.
    let name = String::from_utf8(metadata::encode(&record.object()).to_vec()).unwrap();
    let (max, parent, hex) = (u64::MAX, record.parent(), "f".repeat(64));
    let segments = MAX_SEGMENT_INDEX + 1;
    let states = [
        ("Open", r#"{"kind":"Open"}"#.to_owned()),
        ("Aborting", r#"{"kind":"Aborting"}"#.to_owned()),
        (
            "Completing",
            format!(
                "{{\"kind\":\"Completing\",\"fenced_at_millis\":{max},\"segments_written\":\
                 {segments},\"publish_target\":{{\"parent\":{parent},\"name\":{name},\"epoch\":\
                 {max}}}}}"
            ),
        ),
        (
            "Completed",
            format!(
                "{{\"kind\":\"Completed\",\"completion\":{{\"inode\":{max},\"version\":{max},\
                 \"etag\":\"{hex}-{MAX_PART_NUMBER}\",\"completed_at_millis\":{max},\
                 \"complete_fingerprint\":\"{hex}\"}}}}"
            ),
        ),
    ];
    states
        .into_iter()
        .map(|(kind, state)| {
            let widest = format!(
                "{head}\"epoch\":{max},\"attempts\":{},\"state\":{state}}}",
                u32::MAX
            );
            decode_session_record(widest.as_bytes())
                .unwrap_or_else(|err| panic!("the widest {kind} must decode: {err}"));
            (kind, widest)
        })
        .collect()
}

/// The encoded length of the widest state in [`widest_later_states`].
fn widest_len(open: &[u8]) -> usize {
    widest_later_states(open)
        .iter()
        .map(|(_, record)| record.len())
        .max()
        .unwrap()
}

/// `open` (an `Open@0` record declaring a content type) with its content type `extra` bytes
/// longer — the record a request with that longer content type would install, accepted by the
/// real decoder.
fn with_longer_content_type(open: &[u8], extra: usize) -> Vec<u8> {
    let record = decode_session_record(open).unwrap();
    let declared = record.content_type().expect("a declared content type");
    let longer = format!("{declared}{}", "x".repeat(extra));
    let text = String::from_utf8(open.to_vec()).unwrap().replacen(
        &format!("\"content_type\":\"{declared}\""),
        &format!("\"content_type\":\"{longer}\""),
        1,
    );
    decode_session_record(text.as_bytes()).expect("a canonical session");
    text.into_bytes()
}

/// **(B)** A session must fit, under the backend value ceiling, **every** state it can reach —
/// found at the exact boundary, on encoded bytes, through [`Plan::ValueCeiling`], a store that
/// refuses any value past `MAX_VALUE_BYTES` as FoundationDB does.
///
/// The object name is built from characters that each encode wider than they are long (`"` and
/// `\` are escaped, a control character becomes `\u0001`, `é` is two UTF-8 bytes), and
/// `Completing` names it twice; the content type then sets the size to the byte. The oracle is
/// the test's own: each state's widest spelling, built from the stored bytes and accepted by the
/// real decoder ([`widest_later_states`]).
///
/// * **At the ceiling** (widest later state exactly `MAX_VALUE_BYTES`): admitted; the client
///   Abort fences it through the enforcing store, and a second such session moved to
///   `Completing` is fenced by the restore teardown through it too.
/// * **One byte over**: refused with `SessionTooLarge`, before any read or commit, writing
///   nothing — although its `Open@0` record and its `Aborting@1` fence would both fit, so a check
///   on the record being written would admit it.
/// * **The `Open@0` record exactly at the ceiling** (the case a check on the written record
///   admits): its `Aborting@1` fence is 4 bytes longer and the enforcing store refuses the real
///   Abort — the slot would never come back. Create refuses it, writing nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_session_too_large_for_a_later_state_is_refused_and_writes_nothing() {
    let budget = shipped();
    let object = format!("{BUCKET}/{}", "\"\\\u{1}é".repeat(1_000));
    let sized = |content_type: String| CreateRequest {
        object: object.clone(),
        content_type: Some(content_type),
        ..request("unused")
    };

    // The probe: one session with a one-byte content type, on a plain store.
    let plain = store_with_bucket().await;
    let probe = created(
        create_session(
            &*plain,
            &sized("x".to_owned()),
            &budget,
            &mut Host::new(base(9)),
        )
        .await,
    );
    let (probe_bytes, _) = session(&*plain, &probe).await;
    let name = String::from_utf8(metadata::encode(&object).to_vec()).unwrap();
    assert!(
        name.len() > object.len() + 5_000,
        "the object encodes thousands of bytes wider than it is long ({} vs {}), so only a \
         measure of the encoding finds this boundary",
        name.len(),
        object.len()
    );
    // The oracle is the widest the decoder accepts: one segment more, or a part more in the ETag,
    // is refused.
    for (kind, widest) in widest_later_states(&probe_bytes) {
        let wider = match kind {
            "Completing" => widest.replacen(
                &format!("\"segments_written\":{}", MAX_SEGMENT_INDEX + 1),
                &format!("\"segments_written\":{}", MAX_SEGMENT_INDEX + 2),
                1,
            ),
            "Completed" => widest.replacen(
                &format!("-{MAX_PART_NUMBER}\""),
                &format!("-{}\"", MAX_PART_NUMBER + 1),
                1,
            ),
            _ => continue,
        };
        assert_ne!(wider, widest);
        assert!(
            decode_session_record(wider.as_bytes()).is_err(),
            "{kind}: no wider spelling decodes"
        );
    }
    let at = 1 + MAX_VALUE_BYTES - widest_len(&probe_bytes);
    let at_ceiling = with_longer_content_type(&probe_bytes, at - 1);
    assert_eq!(widest_len(&at_ceiling), MAX_VALUE_BYTES);
    let over = with_longer_content_type(&probe_bytes, at);
    assert_eq!(widest_len(&over), MAX_VALUE_BYTES + 1);
    let widest_kind = |open: &[u8]| {
        let states = widest_later_states(open);
        states
            .iter()
            .max_by_key(|(_, record)| record.len())
            .map(|(kind, _)| *kind)
            .unwrap()
    };
    assert_eq!(widest_kind(&over), "Completing");

    // At the ceiling: admitted, and every transition it is put through lands.
    let inner = store_with_bucket().await;
    let store = Interposer::new(Arc::clone(&inner), Plan::ValueCeiling);
    let request_at = sized("x".repeat(at));
    let mut ids = Vec::new();
    for creator in 0..2 {
        let mut host = Host::new(base(creator));
        ids.push(created(
            create_session(&store, &request_at, &budget, &mut host).await,
        ));
    }
    let (open_at, _) = session(&*inner, &ids[0]).await;
    assert_eq!(
        widest_len(&open_at),
        MAX_VALUE_BYTES,
        "exactly at the ceiling"
    );
    assert_eq!(
        abort_session(&store, &ids[0]).await.unwrap(),
        AbortOutcome::Fenced
    );
    let target = session(&*inner, &ids[1]).await.1.object().to_owned();
    let target = String::from_utf8(metadata::encode(&target).to_vec()).unwrap();
    restate(
        &store,
        &ids[1],
        1,
        1,
        &format!(
            "{{\"kind\":\"Completing\",\"fenced_at_millis\":2000,\"segments_written\":0,\
             \"publish_target\":{{\"parent\":{PARENT},\"name\":{target},\"epoch\":1}}}}"
        ),
    )
    .await;
    let (completing, record) = session(&*inner, &ids[1]).await;
    let teardown = record
        .completing_teardown(&ids[1])
        .expect("Completing@1 has a teardown");
    let owed = [teardown.bytes().clone(), teardown.records().clone()];
    let fence = fence_batch(&ids[1], &completing, teardown.session(), &owed);
    assert_eq!(store.commit(fence).await.unwrap(), CommitOutcome::Committed);
    assert_eq!(
        store.state().oversized,
        0,
        "nothing at the ceiling was refused"
    );

    // One byte over: refused before anything, though `Open@0` and `Aborting@1` would both fit.
    let request_over = sized("x".repeat(at + 1));
    let open_over = decode_session_record(&over).unwrap();
    let fenced_over = open_over.open_teardown(&probe).unwrap();
    assert!(over.len() < MAX_VALUE_BYTES);
    assert!(metadata::encode(fenced_over.session()).len() < MAX_VALUE_BYTES);
    // The sign-off's case: a request whose `Open@0` record is itself exactly at the ceiling.
    let open_at_limit = {
        let short = request("short");
        let probe = created(
            create_session(
                &*plain,
                &CreateRequest {
                    content_type: Some("x".to_owned()),
                    ..short
                },
                &budget,
                &mut Host::new(base(8)),
            )
            .await,
        );
        let (bytes, _) = session(&*plain, &probe).await;
        let open = with_longer_content_type(&bytes, MAX_VALUE_BYTES - bytes.len());
        assert_eq!(open.len(), MAX_VALUE_BYTES);
        open
    };
    let record_at_limit = decode_session_record(&open_at_limit).unwrap();
    let request_at_limit = CreateRequest {
        bucket: BUCKET.to_owned(),
        parent: record_at_limit.parent(),
        object: record_at_limit.object().to_owned(),
        content_type: record_at_limit.content_type().map(str::to_owned),
        created_at_millis: record_at_limit.created_at_millis(),
        clock_source: record_at_limit.clock_source().to_owned(),
    };

    // What a check on the written record alone would admit: the `Open@0` record lands on the
    // enforcing store, and then the real Abort can never fence it.
    let leaked = upload_id(0x1eac);
    let put = WriteBatch::new().put(mpu_key(&leaked), open_at_limit.clone());
    assert_eq!(store.commit(put).await.unwrap(), CommitOutcome::Committed);
    let err = abort_session(&store, &leaked)
        .await
        .expect_err("the Aborting@1 fence is 4 bytes past the ceiling");
    assert!(err.to_string().contains("value_too_large"), "{err}");
    assert_eq!(store.state().oversized, 1);
    let put = WriteBatch::new().delete(mpu_key(&leaked));
    assert_eq!(store.commit(put).await.unwrap(), CommitOutcome::Committed);

    for (name, refused) in [
        ("one byte over", request_over),
        ("Open@0 at the ceiling", request_at_limit),
    ] {
        let before = dump(&*inner).await;
        let (commits, reads) = {
            let state = store.state();
            (state.commits, state.reads)
        };
        let mut host = Host::new(base(7));
        let err = create_session(&store, &refused, &budget, &mut host)
            .await
            .expect_err("too large for a later state");
        match err.downcast_ref::<CreateFault>() {
            Some(CreateFault::SessionTooLarge { bytes, ceiling }) => {
                assert_eq!(*ceiling, MAX_VALUE_BYTES, "{name}");
                assert!(*bytes > MAX_VALUE_BYTES, "{name}: {bytes}");
                if name == "one byte over" {
                    assert_eq!(*bytes, MAX_VALUE_BYTES + 1, "{name}: measured to the byte");
                }
            }
            other => panic!("{name}: expected SessionTooLarge, got {other:?} ({err})"),
        }
        assert!(err.to_string().contains("value ceiling"), "{err}");
        assert_eq!(dump(&*inner).await, before, "{name}: nothing written");
        let state = store.state();
        assert_eq!(state.commits, commits, "{name}: no commit attempted");
        assert_eq!(state.reads, reads, "{name}: nothing read");
    }
}

// ─────────────────────────────── (C) two retry budgets ───────────────────────────────

/// **(C)** The first minted upload id collides with an existing session: the create re-mints and
/// is admitted under the new identity, leaving the other session untouched. When **every** one of
/// `MAX_UPLOAD_ID_ATTEMPTS` mints collides — on `mpu:<id>` or on `seggrp:<nonce>` — the create
/// fails with `IdentitiesExhausted` and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_colliding_identity_is_re_minted_and_every_mint_colliding_writes_nothing() {
    const X: u128 = 0xa1;
    const N1: u128 = 0xb1;
    let budget = shipped();
    let store = store_with_bucket().await;
    let first = request("first");
    let x =
        created(create_session(&*store, &first, &budget, &mut Host::scripted(0, &[X, N1])).await);
    assert_eq!(x, upload_id(X));
    let (x_bytes, _) = session(&*store, &x).await;

    // One collision on `mpu:<X>`, then a fresh identity.
    let second = request("second");
    let mut host = Host::scripted(0, &[X, 0xb2, 0xa2, 0xb3]);
    let y = created(create_session(&*store, &second, &budget, &mut host).await);
    assert_eq!(y, upload_id(0xa2), "admitted under the re-minted id");
    assert_eq!(host.identities(), vec![(X, 0xb2), (0xa2, 0xb3)]);
    assert!(
        host.waits.is_empty(),
        "a collision is not contention: no backoff"
    );
    assert_eq!(
        session(&*store, &x).await.0,
        x_bytes,
        "the other session is untouched"
    );
    assert_eq!(
        session(&*store, &y).await.1.segment_nonce().as_str(),
        hex(0xb3),
        "the re-minted identity carries its own nonce"
    );
    assert_admitted(&*store, &[(x, first), (y, second)], &budget, &[]).await;

    // Every mint collides: twice on `mpu:<X>`, then twice on `seggrp:<N1>`.
    for script in [[X, 0xc1, X, 0xc2], [0xd1, N1, 0xd2, N1]] {
        let before = dump(&*store).await;
        let mut host = Host::scripted(0, &script);
        let err = create_session(&*store, &request("third"), &budget, &mut host)
            .await
            .expect_err("every identity collided");
        match err.downcast_ref::<CreateFault>() {
            Some(CreateFault::IdentitiesExhausted { attempts }) => {
                assert_eq!(*attempts, MAX_UPLOAD_ID_ATTEMPTS);
            }
            other => panic!("expected IdentitiesExhausted, got {other:?} ({err})"),
        }
        assert_eq!(host.identities().len(), MAX_UPLOAD_ID_ATTEMPTS as usize);
        assert!(err.to_string().contains("collided"), "{err}");
        assert_eq!(
            dump(&*store).await,
            before,
            "a failed create writes nothing"
        );
    }
}

/// **(C)** An identity source that repeats an existing session's identity, for the **same**
/// object, gets a collision and a re-mint — never that session's upload id. Nothing of this
/// create is in doubt, so nothing but a collision can have put a session at its `mpu:<id>`,
/// however much that session looks like the one it would have written: the same id and nonce,
/// the same id with another nonce, and the same id and nonce once the first upload is aborted.
/// Each create is admitted under its second identity, with its own session and one more count,
/// and the first session is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_identity_repeated_for_the_same_object_is_re_minted_never_the_other_upload() {
    const X: u128 = 0xa1;
    const N1: u128 = 0xb1;
    let budget = shipped();
    for (name, second_nonce, abort_first) in [
        ("same id and nonce", N1, false),
        ("same id, another nonce", 0xb9, false),
        ("same id and nonce, first aborted", N1, true),
    ] {
        let store = store_with_bucket().await;
        let same = request("same");
        let mut host = Host::scripted(0, &[X, N1]);
        let x = created(create_session(&*store, &same, &budget, &mut host).await);
        assert_eq!(x, upload_id(X));
        if abort_first {
            assert_eq!(
                abort_session(&*store, &x).await.unwrap(),
                AbortOutcome::Fenced
            );
        }
        let (x_bytes, _) = session(&*store, &x).await;
        let count = ledger(&*store).await.count();

        let mut host = Host::scripted(0, &[X, second_nonce, 0xa2, 0xb2]);
        let y = created(create_session(&*store, &same, &budget, &mut host).await);
        assert_eq!(
            y,
            upload_id(0xa2),
            "{name}: re-minted, not the other upload"
        );
        assert_eq!(
            host.identities(),
            vec![(X, second_nonce), (0xa2, 0xb2)],
            "{name}"
        );
        assert_eq!(session(&*store, &x).await.0, x_bytes, "{name}: untouched");
        let (_, record) = session(&*store, &y).await;
        assert_eq!(record.segment_nonce().as_str(), hex(0xb2), "{name}");
        assert_eq!(record.object(), same.object, "{name}");
        assert_eq!(ledger(&*store).await.count(), count + 1, "{name}: counted");
    }
}

/// **(C)** A collision never spends the contention budget, whichever way it is found. The
/// create's first nonce collides with a `seggrp:` marker that outlived its session (an adopted
/// group's marker does, `0016:499-527`, so it is legitimately uncounted):
///
/// * **by the read before the identity's first commit** — the marker is there before the create
///   starts, so no commit is spent on that identity;
/// * **by a lost commit** — the marker lands after that read, just before the first commit, which
///   then loses only its `require_absent(seggrp:<nonce>)`.
///
/// Either way the create re-mints once, and a rival is then admitted before each of its next
/// `MAX_ADMISSION_CAS_ATTEMPTS - 1` commits, so it loses the ledger CAS that many times. It is
/// admitted on its last permitted attempt. One budget counting both would have refused it a
/// commit earlier.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_collision_does_not_spend_the_contention_budget() {
    const ORPHAN_NONCE: u128 = 0x5e5e;
    let budget = shipped();
    let last = MAX_ADMISSION_CAS_ATTEMPTS;
    for by_commit in [false, true] {
        let name = if by_commit {
            "found by a lost commit"
        } else {
            "found by the read"
        };
        let inner = store_with_bucket().await;
        let script = if by_commit {
            Script {
                plant_marker: Some((1, marker_key(ORPHAN_NONCE))),
                rivals: Some(2..=last),
                ..Script::new(budget)
            }
        } else {
            let orphan = WriteBatch::new().put(marker_key(ORPHAN_NONCE), SEGGRP_MARKER);
            assert_eq!(
                inner.commit(orphan).await.unwrap(),
                CommitOutcome::Committed
            );
            Script {
                rivals: Some(1..=last - 1),
                ..Script::new(budget)
            }
        };
        let store = Interposer::scripted(&inner, script);
        let target = request("target");
        let mut host = Host::scripted(base(0), &[0x7a, ORPHAN_NONCE]).instant();
        let id = created(within(create_session(&store, &target, &budget, &mut host)).await);

        let (lost, rivals, commits) = {
            let state = store.state();
            (state.lost.clone(), state.rivals, state.commits)
        };
        let collision_only = Lost {
            collision: true,
            ..Lost::default()
        };
        let ledger_only = Lost {
            ledger: true,
            ..Lost::default()
        };
        let ledger_losses = if by_commit {
            assert_eq!(
                lost.first(),
                Some(&collision_only),
                "{name}: the first commit lost only to the collision"
            );
            &lost[1..]
        } else {
            &lost[..]
        };
        assert_eq!(ledger_losses.len(), (last - 1) as usize, "{name}");
        assert!(
            ledger_losses.iter().all(|lost| *lost == ledger_only),
            "{name}: every other lost commit lost only the ledger CAS: {lost:?}"
        );
        assert_eq!(commits, last + u32::from(by_commit), "{name}");
        assert_eq!(
            host.identities().len(),
            2,
            "{name}: one re-mint, for the one collision"
        );
        assert_eq!(
            host.waits.len(),
            (last - 1) as usize,
            "{name}: one backoff per lost CAS, none for the collision"
        );
        host.assert_backoff();
        assert_eq!(rivals, last - 1, "{name}");

        // The rivals plus the target fill the shipped ledger exactly; the orphan marker is extra.
        let sessions = inner.scan(MPU_PREFIX).await.unwrap();
        assert_eq!(sessions.len() as u64, MAX_SESSIONS, "{name}");
        assert_eq!(ledger(&*inner).await.count(), MAX_SESSIONS, "{name}");
        let (_, record) = session(&*inner, &id).await;
        assert_eq!(record.object(), target.object);
        assert_eq!(record.segment_nonce().as_str(), hex(host.identities()[1].1));
        let markers = inner.scan(SEGGRP_PREFIX).await.unwrap();
        assert_eq!(markers.len() as u64, MAX_SESSIONS + 1, "{name}");
    }
}

/// **(C)** + **(F)** An identity source that repeats an existing upload's identity, now with the
/// create's first commit answered `CommitUnknownResult` **without** landing. An unknown outcome
/// says nothing about a session that was there before the create began, so it never makes that
/// session this create's own — the existing upload's id and nonce for the very same request
/// (every field equal, so no comparison of the record could tell them apart), its id with another
/// nonce, and its nonce under another id. Create reads both keys before an identity's first
/// commit, so each collision is found while nothing is in doubt: the create re-mints, the commit
/// answered unknown is the second identity's, and its recovery re-drives that one. The create is
/// admitted under the second identity with its own count, and the first upload is untouched —
/// whether the unknown batch may still commit or not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_an_identity_repeated_under_an_unknown_outcome_is_never_the_other_upload() {
    const X: u128 = 0xa1;
    const N1: u128 = 0xb1;
    let budget = shipped();
    for (name, repeated) in [
        ("same id and nonce", (X, N1)),
        ("same id, another nonce", (X, 0xb9)),
        ("same nonce, another id", (0xa9, N1)),
    ] {
        for may_still_commit in [true, false] {
            let name = format!("{name}, may_still_commit = {may_still_commit}");
            let inner = store_with_bucket().await;
            let same = request("same");
            let mut host = Host::scripted(0, &[X, N1]);
            let x = created(create_session(&*inner, &same, &budget, &mut host).await);
            let (x_bytes, _) = session(&*inner, &x).await;

            let store = Interposer::scripted(
                &inner,
                Script {
                    unknown: [(1, Unknown::Dropped { may_still_commit })].into(),
                    ..Script::new(budget)
                },
            );
            let mut host = Host::scripted(0, &[repeated.0, repeated.1, 0xa2, 0xb2]).instant();
            let outcome = within(create_session(&store, &same, &budget, &mut host)).await;
            let y = match outcome {
                Ok(CreateOutcome::Created { upload_id }) => upload_id,
                other => panic!("{name}: expected Created, got {other:?}"),
            };
            assert_eq!(y, upload_id(0xa2), "{name}: never the other upload's id");
            assert_eq!(host.identities(), vec![repeated, (0xa2, 0xb2)], "{name}");
            assert_eq!(session(&*inner, &x).await.0, x_bytes, "{name}: untouched");
            {
                let state = store.state();
                assert_eq!(
                    state.commits, 2,
                    "{name}: the unknown commit and its re-drive"
                );
                assert_eq!(state.committed.len(), 1, "{name}");
                assert_eq!(
                    session_key(&state.committed[0]),
                    Some(mpu_key(&y)),
                    "{name}: the re-drive installs the second identity"
                );
                assert!(state.lost.is_empty(), "{name}: {:?}", state.lost);
            }
            assert_admitted(&*inner, &[(x, same.clone()), (y, same)], &budget, &[]).await;
        }
    }
}

// ────────────────────── (D) + (E) Abort, and the count it never moves ──────────────────────

/// Create one session for creator `creator` and return its upload id.
async fn one_session(store: &impl MetadataStore, creator: usize) -> UploadId {
    let request = request(&format!("obj-{creator}"));
    created(create_session(store, &request, &shipped(), &mut Host::new(base(creator))).await)
}

/// Replace a session's state, as #658's Complete would: the codec's own bytes for `record` at
/// `epoch` with `attempts` and the JSON `state`, built from the record's canonical encoding so
/// every other field keeps its exact spelling.
async fn restate(
    store: &impl MetadataStore,
    id: &UploadId,
    epoch: u64,
    attempts: u32,
    state: &str,
) {
    let (bytes, _) = session(store, id).await;
    let open = String::from_utf8(bytes.to_vec()).unwrap();
    let tail = "\"epoch\":0,\"attempts\":0,\"state\":{\"kind\":\"Open\"}}";
    assert!(open.ends_with(tail), "{open}");
    let restated = format!(
        "{}\"epoch\":{epoch},\"attempts\":{attempts},\"state\":{state}}}",
        &open[..open.len() - tail.len()]
    );
    decode_session_record(restated.as_bytes()).expect("a canonical restated session");
    let put = WriteBatch::new().put(mpu_key(id), restated.into_bytes());
    assert_eq!(store.commit(put).await.unwrap(), CommitOutcome::Committed);
}

/// **(D)** + **(E)** Abort of `Open@E` answers `Fenced`: the session is `Aborting@E+1` and
/// `retire:bytes:s:<id>:<E>` holds exactly `open_teardown`'s `{session, all}` obligation,
/// committed in the **same** batch as the fence — the one `fence_batch` builds. A second Abort
/// answers `AlreadyAborting`. The ledger count never moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_abort_fences_an_open_session_with_its_obligation_in_one_batch() {
    let inner = store_with_bucket().await;
    let id = one_session(&*inner, 0).await;
    let count = ledger(&*inner).await.count();
    let (read, open) = session(&*inner, &id).await;
    let expected = open.open_teardown(&id).expect("Open@0 has a teardown");

    let store = Interposer::new(Arc::clone(&inner), Plan::PassThrough);
    assert_eq!(
        abort_session(&store, &id).await.unwrap(),
        AbortOutcome::Fenced
    );
    assert_eq!(
        ledger(&*inner).await.count(),
        count,
        "(E) the fence never moves the count"
    );

    let (_, fenced) = session(&*inner, &id).await;
    assert_eq!(fenced.state(), &SessionState::Aborting {});
    assert_eq!(fenced.epoch(), open.epoch() + 1);
    assert_eq!(&fenced, expected.session());
    let token = RetireToken::Session {
        upload_id: id.clone(),
        epoch: open.epoch(),
        part: None,
    };
    let key = retire_key(RetireMode::Bytes, &token);
    assert_eq!(key, expected.obligation().key());
    let value = inner.get(&key).await.unwrap().expect("the obligation");
    assert_eq!(value, metadata::encode(expected.obligation().payload()));
    let (mode, _, owed) = decode_retire_obligation(&key, &value).expect("a canonical obligation");
    assert_eq!(mode, RetireMode::Bytes);
    assert!(owed.session() && owed.parts() == Some(&PartScope::All));

    // One commit carried both, under the exact-bytes and absent-key preconditions.
    let committed = store.state().committed.clone();
    assert_eq!(committed.len(), 1, "the fence is one batch");
    let batch = &committed[0];
    let built = fence_batch(
        &id,
        &read,
        expected.session(),
        std::slice::from_ref(expected.obligation()),
    );
    assert_eq!(batch.preconditions, built.preconditions);
    assert_eq!(batch.puts, built.puts);
    assert!(batch.deletes.is_empty() && built.deletes.is_empty());
    assert_eq!(batch.preconditions.len(), 2);
    assert_eq!(batch.preconditions[0].key, mpu_key(&id));
    assert_eq!(batch.preconditions[0].expected.as_ref(), Some(&read));
    assert_eq!(
        (
            &batch.preconditions[1].key,
            &batch.preconditions[1].expected
        ),
        (&key, &None)
    );
    let puts: Vec<&[u8]> = batch.puts.iter().map(|(key, _)| key.as_slice()).collect();
    assert_eq!(puts, vec![mpu_key(&id).as_slice(), key.as_slice()]);
    assert!(batch
        .puts
        .iter()
        .all(|(key, _)| key.as_slice() != MPUCTL_KEY));

    let before = dump(&*inner).await;
    assert_eq!(
        abort_session(&store, &id).await.unwrap(),
        AbortOutcome::AlreadyAborting
    );
    assert_eq!(
        dump(&*inner).await,
        before,
        "an idempotent Abort writes nothing"
    );
    assert_eq!(ledger(&*inner).await.count(), count, "(E)");
}

/// **(D)** + **(E)** Two concurrent Aborts of one `Open` session answer one `Fenced` and one
/// `AlreadyAborting`, never `OperationAborted` — once forced to race (both read `Open` before
/// either commits) and again free-running. The count never moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn d_two_concurrent_aborts_answer_fenced_and_already_aborting() {
    for forced in [true, false] {
        for round in 0..8 {
            let inner = store_with_bucket().await;
            let id = one_session(&*inner, round).await;
            let count = ledger(&*inner).await.count();
            let plan = if forced {
                Plan::Barrier { participants: 2 }
            } else {
                Plan::PassThrough
            };
            let store = Arc::new(Interposer::new(Arc::clone(&inner), plan));
            let aborts: Vec<_> = (0..2)
                .map(|_| {
                    let (store, id) = (Arc::clone(&store), id.clone());
                    tokio::spawn(async move { abort_session(&*store, &id).await })
                })
                .collect();
            let mut answers = Vec::new();
            for abort in aborts {
                answers.push(within(abort).await.unwrap().unwrap());
            }
            answers.sort_by_key(|answer| format!("{answer:?}"));
            assert_eq!(
                answers,
                vec![AbortOutcome::AlreadyAborting, AbortOutcome::Fenced],
                "forced = {forced}, round {round}"
            );
            assert_eq!(
                ledger(&*inner).await.count(),
                count,
                "(E) the count never moves"
            );
            if forced {
                let state = store.state();
                assert_eq!(state.committed.len(), 1);
                assert_eq!(
                    state.lost,
                    vec![Lost {
                        other: true,
                        ..Lost::default()
                    }]
                );
            }
            let (_, fenced) = session(&*inner, &id).await;
            assert_eq!(
                (fenced.state(), fenced.epoch()),
                (&SessionState::Aborting {}, 1)
            );
            let obligations = inner.scan(b"retire:").await.unwrap();
            assert_eq!(obligations.len(), 1, "one fence, one obligation");
        }
    }
}

/// **(D)** + **(E)** Abort of a `Completing` session answers `OperationAborted`; of a `Completed`
/// tombstone, or of an upload id with no record, `NoSuchUpload`. Each leaves the store
/// byte-identical, and the tombstone stays counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_abort_refuses_completing_completed_and_unknown_sessions_writing_nothing() {
    let store = store_with_bucket().await;
    let completing = one_session(&*store, 0).await;
    let completed = one_session(&*store, 1).await;
    let target = session(&*store, &completing).await.1.object().to_owned();
    restate(
        &*store,
        &completing,
        1,
        1,
        &format!(
            "{{\"kind\":\"Completing\",\"fenced_at_millis\":2000,\"segments_written\":0,\
             \"publish_target\":{{\"parent\":{PARENT},\"name\":\"{target}\",\"epoch\":1}}}}"
        ),
    )
    .await;
    restate(
        &*store,
        &completed,
        2,
        1,
        &format!(
            "{{\"kind\":\"Completed\",\"completion\":{{\"inode\":7,\"version\":1,\"etag\":\
             \"{}-1\",\"completed_at_millis\":3000,\"complete_fingerprint\":\"{}\"}}}}",
            "ab".repeat(32),
            "cd".repeat(32)
        ),
    )
    .await;
    let count = ledger(&*store).await.count();
    assert_eq!(count, 2, "(E) a Completed tombstone stays counted");

    let absent = upload_id(0xdead);
    for (id, answer) in [
        (&completing, Refusal::OperationAborted),
        (&completed, Refusal::NoSuchUpload),
        (&absent, Refusal::NoSuchUpload),
    ] {
        let before = dump(&*store).await;
        let outcome = abort_session(&*store, id).await.unwrap();
        assert_eq!(outcome, AbortOutcome::Refused(answer));
        assert_eq!(
            dump(&*store).await,
            before,
            "a refused Abort writes nothing"
        );
        assert_eq!(ledger(&*store).await.count(), count, "(E)");
    }
    assert_eq!(store.scan(MPU_PREFIX).await.unwrap().len() as u64, count);
}

/// **(D)** A fence that loses while the session stays byte-identical has lost to its own
/// obligation key — damage, never a silent success or an endless retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_taken_obligation_key_is_a_typed_fault() {
    let store = store_with_bucket().await;
    let id = one_session(&*store, 0).await;
    let token = RetireToken::Session {
        upload_id: id.clone(),
        epoch: 0,
        part: None,
    };
    let squatter = WriteBatch::new().put(retire_key(RetireMode::Bytes, &token), &b"{}"[..]);
    assert_eq!(
        store.commit(squatter).await.unwrap(),
        CommitOutcome::Committed
    );
    let before = dump(&*store).await;
    let err = abort_session(&*store, &id).await.expect_err("damage");
    assert!(
        matches!(
            err.downcast_ref::<AbortFault>(),
            Some(AbortFault::ObligationKeyTaken { .. })
        ),
        "{err}"
    );
    assert!(err.to_string().contains("retire:bytes:s:"), "{err}");
    assert_eq!(dump(&*store).await, before);
}

/// **(D)** A plain store fault on the fence commit — not a `CommitUnknownResult` — is returned as
/// it came, after the one commit, and never retried: nothing says the store can take another
/// attempt, and nothing about the fault is a protocol answer. The session stays `Open`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_plain_store_fault_on_the_fence_is_returned_not_retried() {
    let inner = store_with_bucket().await;
    let id = one_session(&*inner, 0).await;
    let before = dump(&*inner).await;
    let store = Interposer::scripted(
        &inner,
        Script {
            fault: Some(1),
            ..Script::new(shipped())
        },
    );
    let err = within(abort_session(&store, &id))
        .await
        .expect_err("the fault is returned");
    assert_eq!(err.to_string(), INJECTED_FAULT);
    assert!(err.downcast_ref::<CommitUnknownResult>().is_none());
    assert_eq!(store.state().commits, 1, "one commit, never retried");
    assert_eq!(dump(&*inner).await, before, "nothing written");
    assert_eq!(
        session(&*inner, &id).await.1.state(),
        &SessionState::Open {}
    );
}

/// **(D)** + **(E)** An unknown fence outcome is never read as a `Conflict`: the Abort re-reads
/// and answers from what it finds — the fence landed (`AlreadyAborting`), or it did not and the
/// identical batch is committed again (`Fenced`), or it was held and lands after that re-drive,
/// which it then loses. One fence and one obligation either way, and the count never moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_unknown_fence_outcome_is_settled_by_a_re_read() {
    for (name, kind, answer) in [
        (
            "applied",
            Unknown::Applied {
                may_still_commit: true,
            },
            AbortOutcome::AlreadyAborting,
        ),
        (
            "not applied",
            Unknown::Dropped {
                may_still_commit: true,
            },
            AbortOutcome::Fenced,
        ),
        (
            "out of flight",
            Unknown::Dropped {
                may_still_commit: false,
            },
            AbortOutcome::Fenced,
        ),
        ("applied late", Unknown::Late, AbortOutcome::Fenced),
    ] {
        let inner = store_with_bucket().await;
        let id = one_session(&*inner, 0).await;
        let count = ledger(&*inner).await.count();
        let (_, open) = session(&*inner, &id).await;
        let expected = open.open_teardown(&id).expect("Open@0 has a teardown");
        let store = Interposer::scripted(
            &inner,
            Script {
                unknown: [(1, kind)].into(),
                ..Script::new(shipped())
            },
        );
        let outcome = within(abort_session(&store, &id)).await.unwrap();
        assert_eq!(outcome, answer, "{name}");
        assert_eq!(ledger(&*inner).await.count(), count, "{name}: (E)");
        assert_eq!(&session(&*inner, &id).await.1, expected.session(), "{name}");
        let owed = (
            expected.obligation().key(),
            metadata::encode(expected.obligation().payload()),
        );
        assert_eq!(inner.scan(b"retire:").await.unwrap(), vec![owed], "{name}");
        let state = store.state();
        assert!(state.lost.is_empty(), "{name}: {:?}", state.lost);
        if matches!(kind, Unknown::Late) {
            assert_eq!(state.late, Some((CommitOutcome::Conflict, true)), "{name}");
        }
    }
}

/// Rounds one Abort may spend: one per epoch a session can be `Open` in.
const ABORT_ROUNDS: u32 = MAX_COMPLETE_ATTEMPTS + 1;

/// Abort an `Open` session through `script`, which spends every round the Abort may spend, and
/// assert the last round's re-read was answered: `AlreadyAborting`, with one fence and one
/// obligation in the store and the count unmoved (E). Returns the interposer for more checks.
async fn abort_through_the_last_round(script: Script) -> Interposer {
    let inner = store_with_bucket().await;
    let id = one_session(&*inner, 0).await;
    let count = ledger(&*inner).await.count();
    let store = Interposer::scripted(&inner, script);
    let outcome = within(abort_session(&store, &id)).await;
    assert_eq!(
        outcome.unwrap(),
        AbortOutcome::AlreadyAborting,
        "the last re-read found the session Aborting"
    );
    {
        let state = store.state();
        assert_eq!(state.commits, ABORT_ROUNDS, "every round spent");
        assert!(state.committed.is_empty(), "no fence committed through it");
    }
    let (_, fenced) = session(&*inner, &id).await;
    assert_eq!(fenced.state(), &SessionState::Aborting {});
    let token = RetireToken::Session {
        upload_id: id.clone(),
        epoch: fenced.epoch() - 1,
        part: None,
    };
    let obligations = inner.scan(b"retire:").await.unwrap();
    assert_eq!(obligations.len(), 1, "one fence, one obligation");
    assert_eq!(obligations[0].0, retire_key(RetireMode::Bytes, &token));
    assert_eq!(ledger(&*inner).await.count(), count, "(E)");
    store
}

/// **(D)** + **(E)** The last round's re-read is answered like every other, never discarded for
/// an error. The first three fence commits answer `CommitUnknownResult` without landing, and the
/// fourth — the last the Abort may make — lands **and** answers `CommitUnknownResult`. Its re-read
/// finds the session `Aborting`, so the answer is `AlreadyAborting`, not the unknown error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_fence_landing_under_the_last_rounds_unknown_outcome_answers_already_aborting() {
    let dropped = Unknown::Dropped {
        may_still_commit: true,
    };
    let applied = Unknown::Applied {
        may_still_commit: true,
    };
    let unknown: BTreeMap<u32, Unknown> = (1..ABORT_ROUNDS)
        .map(|n| (n, dropped))
        .chain([(ABORT_ROUNDS, applied)])
        .collect();
    abort_through_the_last_round(Script {
        unknown,
        ..Script::new(shipped())
    })
    .await;
}

/// **(D)** + **(E)** Two Aborts never end in an error, even on the last round. The first three
/// fence commits lose to the session being reopened under them (a Complete fence rolled back),
/// and before the fourth — the last — a rival Abort fences it. The fourth loses, its re-read finds
/// the session `Aborting`, and the answer is `AlreadyAborting`, not `RoundsExhausted`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_rival_abort_winning_the_last_round_answers_already_aborting() {
    let store = abort_through_the_last_round(Script {
        reopen: Some(1..=ABORT_ROUNDS - 1),
        rival_abort: Some(ABORT_ROUNDS),
        ..Script::new(shipped())
    })
    .await;
    assert_eq!(store.state().rival_aborts, vec![AbortOutcome::Fenced]);
}

/// **(D)** An `Open` session at the last epoch has no fence to move to: a typed fault, nothing
/// written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_open_session_at_the_last_epoch_is_a_typed_fault() {
    let store = store_with_bucket().await;
    let id = one_session(&*store, 0).await;
    restate(&*store, &id, u64::MAX, 0, "{\"kind\":\"Open\"}").await;
    let before = dump(&*store).await;
    let err = abort_session(&*store, &id)
        .await
        .expect_err("no epoch left");
    assert_eq!(
        err.downcast_ref::<AbortFault>(),
        Some(&AbortFault::EpochExhausted)
    );
    assert!(err.to_string().contains("last epoch"), "{err}");
    assert_eq!(dump(&*store).await, before);
}

/// **(D)** Abort's await is bounded: a session found `Open` again before every fence lands — as a
/// Complete fence and its rollback would leave it — ends the Abort after one round per epoch the
/// session can be `Open` in (`MAX_COMPLETE_ATTEMPTS + 1`), with a typed fault and no obligation
/// written, rather than retrying forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_an_abort_that_keeps_losing_to_a_reopened_session_is_bounded() {
    let inner = store_with_bucket().await;
    let id = one_session(&*inner, 0).await;
    let count = ledger(&*inner).await.count();
    let store = Interposer::scripted(
        &inner,
        Script {
            reopen: Some(1..=u32::MAX),
            ..Script::new(shipped())
        },
    );
    let err = within(abort_session(&store, &id))
        .await
        .expect_err("it never fenced");
    let rounds = ABORT_ROUNDS;
    assert_eq!(
        err.downcast_ref::<AbortFault>(),
        Some(&AbortFault::RoundsExhausted { rounds })
    );
    assert!(err.to_string().contains(&rounds.to_string()), "{err}");
    let (commits, committed) = {
        let state = store.state();
        (state.commits, state.committed.len())
    };
    assert_eq!(commits, rounds, "one fence attempt per round");
    assert_eq!(committed, 0);
    let (_, record) = session(&*inner, &id).await;
    assert_eq!(
        (record.state(), record.epoch()),
        (&SessionState::Open {}, 2 * u64::from(rounds))
    );
    assert!(inner.scan(b"retire:").await.unwrap().is_empty());
    assert_eq!(ledger(&*inner).await.count(), count, "(E)");
}

/// The [`CommitUnknownResult`] an Abort ended with.
fn as_unknown(err: &BoxError) -> &CommitUnknownResult {
    err.downcast_ref::<CommitUnknownResult>()
        .unwrap_or_else(|| panic!("expected a CommitUnknownResult, got: {err}"))
}

/// How an Abort that ends without an answer must end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbortEnds {
    /// With a `CommitUnknownResult` carrying this `may_still_commit`.
    Unknown { may_still_commit: bool },
    /// With the injected plain fault, as it came.
    Fault,
    /// With [`AbortFault::RoundsExhausted`].
    Exhausted,
}

/// One Abort leg ending without an answer: what its script does, how it must end, and what
/// becomes of the fence the script holds in flight when it is released after the Abort returned.
struct DoubtLeg {
    name: &'static str,
    script: Script,
    ends: AbortEnds,
    /// `Some`: the held fence's outcome on release. `None`: nothing is held.
    released: Option<CommitOutcome>,
}

/// Run `leg` on a fresh `Open@0` session and assert how it ended — then release the held fence,
/// **after** the Abort returned, and assert what it did. A fence that lands leaves exactly
/// `open_teardown`'s session and obligation; otherwise no fence of this Abort landed, and the
/// only obligation is a rival Abort's, if the script ran one. The count never moves (E).
async fn abort_ending_in_doubt(leg: DoubtLeg) {
    let DoubtLeg {
        name,
        script,
        ends,
        released,
    } = leg;
    let inner = store_with_bucket().await;
    let id = one_session(&*inner, 0).await;
    let count = ledger(&*inner).await.count();
    let (_, open) = session(&*inner, &id).await;
    let expected = open.open_teardown(&id).expect("Open@0 has a teardown");
    let store = Interposer::scripted(&inner, script);
    let err = within(abort_session(&store, &id))
        .await
        .expect_err("the Abort ends without an answer");
    match ends {
        AbortEnds::Unknown { may_still_commit } => assert_eq!(
            as_unknown(&err).may_still_commit,
            may_still_commit,
            "{name}: {err}"
        ),
        AbortEnds::Fault => {
            assert_eq!(err.to_string(), INJECTED_FAULT, "{name}");
            assert!(
                err.downcast_ref::<CommitUnknownResult>().is_none(),
                "{name}"
            );
        }
        AbortEnds::Exhausted => assert_eq!(
            err.downcast_ref::<AbortFault>(),
            Some(&AbortFault::RoundsExhausted {
                rounds: ABORT_ROUNDS
            }),
            "{name}: {err}"
        ),
    }
    // A rival Abort's fence is the only one that may have landed by now.
    let rival = store.state().rival_aborts == vec![AbortOutcome::Fenced];
    let fenced = |record: &SessionRecord| record.state() == &SessionState::Aborting {};
    assert_eq!(
        inner.scan(b"retire:").await.unwrap().len(),
        usize::from(rival),
        "{name}: no fence of this Abort has landed when it returns"
    );
    assert_eq!(fenced(&session(&*inner, &id).await.1), rival, "{name}");
    assert_eq!(store.state().held.is_some(), released.is_some(), "{name}");
    if let Some(outcome) = released {
        assert_eq!(store.release().await, outcome, "{name}: the held fence");
    }
    if released == Some(CommitOutcome::Committed) {
        assert_eq!(&session(&*inner, &id).await.1, expected.session(), "{name}");
        let owed = (
            expected.obligation().key(),
            metadata::encode(expected.obligation().payload()),
        );
        assert_eq!(inner.scan(b"retire:").await.unwrap(), vec![owed], "{name}");
    } else {
        // Nothing of this Abort landed, even once the held fence was released.
        assert_eq!(
            inner.scan(b"retire:").await.unwrap().len(),
            usize::from(rival),
            "{name}"
        );
        assert_eq!(fenced(&session(&*inner, &id).await.1), rival, "{name}");
    }
    assert_eq!(ledger(&*inner).await.count(), count, "{name}: (E)");
}

/// **(D)** A fence that may still land is never forgotten by a later round — the mixed-code run.
/// One fence commit answers `CommitUnknownResult { may_still_commit: true }` and stays in flight
/// (1031); every other answers it with `may_still_commit: false` without landing (1021). Every
/// re-read finds the session `Open` and unchanged, so the Abort spends every round, and it must
/// end with the doubt that can still land — not the last round's out-of-flight one, which would
/// tell its caller no fence is pending. The held fence then lands after the Abort returned.
/// Held at the first commit, and at the second, after an out-of-flight one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_fence_that_may_still_land_outlives_later_out_of_flight_outcomes() {
    let out_of_flight = Unknown::Dropped {
        may_still_commit: false,
    };
    for (name, held_at) in [("held at commit 1", 1), ("held at commit 2", 2)] {
        let unknown = (1..=ABORT_ROUNDS)
            .map(|n| {
                let kind = if n == held_at {
                    Unknown::Held
                } else {
                    out_of_flight
                };
                (n, kind)
            })
            .collect();
        abort_ending_in_doubt(DoubtLeg {
            name,
            script: Script {
                unknown,
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Committed),
        })
        .await;
    }
}

/// **(D)** A re-read that fails never drops a fence's unknown outcome; it ends the Abort with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_failed_re_read_ends_the_abort_with_the_fence_still_in_doubt() {
    let out_of_flight = Unknown::Dropped {
        may_still_commit: false,
    };
    for leg in [
        // Right after a fence left in flight (1031).
        DoubtLeg {
            name: "after a fence left in flight",
            script: Script {
                unknown: [(1, Unknown::Held)].into(),
                read_fault: Some(1),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Committed),
        },
        // Mixed codes: a fence left in flight, then an out-of-flight unknown whose re-read fails.
        DoubtLeg {
            name: "after 1031, then 1021",
            script: Script {
                unknown: [(1, Unknown::Held), (2, out_of_flight)].into(),
                read_fault: Some(2),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Committed),
        },
        // An out-of-flight unknown alone: the read that was to settle it failed, so it is still
        // unknown — that outcome, not the read's fault.
        DoubtLeg {
            name: "after an out-of-flight unknown",
            script: Script {
                unknown: [(1, out_of_flight)].into(),
                read_fault: Some(1),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: false,
            },
            released: None,
        },
        // A fence left in flight, then a fence lost to a rival Abort whose re-read fails: without
        // that read the Abort cannot tell its own fence did not land first.
        DoubtLeg {
            name: "after a lost fence, a fence still in flight",
            script: Script {
                unknown: [(1, Unknown::Held)].into(),
                rival_abort: Some(2),
                read_fault: Some(2),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Conflict),
        },
        // An out-of-flight unknown settled by a re-read that found the session unchanged, then a
        // fence lost to a reopened session whose re-read fails: nothing is in doubt.
        DoubtLeg {
            name: "after a lost fence, nothing in doubt",
            script: Script {
                unknown: [(1, out_of_flight)].into(),
                reopen: Some(2..=2),
                read_fault: Some(2),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Fault,
            released: None,
        },
    ] {
        abort_ending_in_doubt(leg).await;
    }
}

/// **(D)** A plain store fault on a later fence never drops an earlier fence's doubt: the Abort
/// ends with that fence's `CommitUnknownResult { may_still_commit: true }`, not the fault, which
/// says only that the later fence installed nothing — and the earlier fence, released after the
/// Abort returned, lands. Also with an out-of-flight unknown (1021) between them. With only a
/// settled out-of-flight unknown before the fault, nothing is in doubt and the fault is returned
/// as it came.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_later_plain_fault_ends_the_abort_with_the_fence_still_in_doubt() {
    let out_of_flight = Unknown::Dropped {
        may_still_commit: false,
    };
    for leg in [
        DoubtLeg {
            name: "1031, then a fault",
            script: Script {
                unknown: [(1, Unknown::Held)].into(),
                fault: Some(2),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Committed),
        },
        DoubtLeg {
            name: "1031, then 1021, then a fault",
            script: Script {
                unknown: [(1, Unknown::Held), (2, out_of_flight)].into(),
                fault: Some(3),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: true,
            },
            released: Some(CommitOutcome::Committed),
        },
        DoubtLeg {
            name: "1021, then a fault",
            script: Script {
                unknown: [(1, out_of_flight)].into(),
                fault: Some(2),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Fault,
            released: None,
        },
    ] {
        abort_ending_in_doubt(leg).await;
    }
}

/// **(D)** Only a read that finds the session moved clears a fence's doubt: every fence requires
/// the bytes it read, and a session never returns to bytes it has left, so the held fence can
/// never land — and, released after the Abort returned, it loses.
///
/// * Moved under later fences: commit 1 is left in flight, then the session is reopened (a
///   Complete fence rolled back) before every later fence, which loses. Nothing is in doubt at
///   the end, so the Abort ends `RoundsExhausted`.
/// * Moved under the held fence itself, and then every later fence out of flight (1021): the
///   Abort ends with the last round's out-of-flight unknown, not the held one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_a_session_seen_moved_clears_the_doubt_of_a_fence_that_can_no_longer_land() {
    let out_of_flight = Unknown::Dropped {
        may_still_commit: false,
    };
    for leg in [
        DoubtLeg {
            name: "moved under later fences",
            script: Script {
                unknown: [(1, Unknown::Held)].into(),
                reopen: Some(2..=ABORT_ROUNDS),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Exhausted,
            released: Some(CommitOutcome::Conflict),
        },
        DoubtLeg {
            name: "moved under the held fence",
            script: Script {
                unknown: [(1, Unknown::Held)]
                    .into_iter()
                    .chain((2..=ABORT_ROUNDS).map(|n| (n, out_of_flight)))
                    .collect(),
                reopen: Some(1..=1),
                ..Script::new(shipped())
            },
            ends: AbortEnds::Unknown {
                may_still_commit: false,
            },
            released: Some(CommitOutcome::Conflict),
        },
    ] {
        abort_ending_in_doubt(leg).await;
    }
}

// ─────────────────────────── (F) an unknown Create outcome ───────────────────────────

/// **(F)** The first create commit answers `CommitUnknownResult` — applied (F1), not applied
/// (F2), or held and applied **late** (F3) — plus F2 with `may_still_commit == false`, F2 with a
/// rival moving the ledger before the re-drive, and F3 landing between the recovery's look at
/// `mpu:<id>` and its ledger read. Each recovers under the **original** upload id and nonce: one
/// identity minted, `Created` with it, exactly one `mpu:` record and one `seggrp:` marker for it,
/// and the ledger up by exactly one for it. A late batch landing after the re-drive loses its own
/// `require_absent(mpu:<id>)`; one landing before the re-drive commits makes the re-drive lose
/// that precondition instead, and the create finds its own session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id() {
    let budget = shipped();
    for (name, kind) in [
        (
            "F1 applied",
            Unknown::Applied {
                may_still_commit: true,
            },
        ),
        (
            "F2 not applied",
            Unknown::Dropped {
                may_still_commit: true,
            },
        ),
        ("F3 applied after the re-drive", Unknown::Late),
        (
            "F3 applied between the recovery's reads",
            Unknown::LateAtLedgerRead,
        ),
        (
            "F2, out of flight",
            Unknown::Dropped {
                may_still_commit: false,
            },
        ),
        ("F2, the ledger moved", Unknown::DroppedThenRival),
    ] {
        let inner = store_with_bucket().await;
        let earlier = one_session(&*inner, 9).await;
        let count = ledger(&*inner).await.count();
        let store = Interposer::scripted(
            &inner,
            Script {
                unknown: [(1, kind)].into(),
                ..Script::new(budget)
            },
        );
        let target = request("target");
        let mut host = Host::new(base(0));
        let outcome = within(create_session(&store, &target, &budget, &mut host)).await;
        let id = created(outcome);

        // The store first: one session and one increment for this create.
        let rivals = u64::from(store.state().rivals);
        let sessions: Vec<UploadId> = inner
            .scan(MPU_PREFIX)
            .await
            .unwrap()
            .iter()
            .map(|(key, _)| parse_mpu_key(key).unwrap())
            .filter(|session| session != &earlier)
            .collect();
        assert_eq!(
            sessions.len() as u64,
            1 + rivals,
            "{name}: exactly one session for this create: {sessions:?}"
        );
        assert_eq!(
            ledger(&*inner).await.count(),
            count + 1 + rivals,
            "{name}: the ledger rose by exactly one for this create"
        );
        assert_eq!(
            sessions.iter().filter(|session| **session == id).count(),
            1,
            "{name}"
        );

        // Then the identity: the original one, never a second.
        let identities = host.identities();
        assert_eq!(
            identities.len(),
            1,
            "{name}: no second identity is ever minted"
        );
        let (id_bits, nonce_bits) = identities[0];
        assert_eq!(
            id,
            upload_id(id_bits),
            "{name}: Created under the original upload id"
        );
        let (_, record) = session(&*inner, &id).await;
        assert_eq!(
            record.segment_nonce().as_str(),
            hex(nonce_bits),
            "{name}: the original nonce"
        );
        assert_eq!(record.object(), target.object);
        let markers = inner.scan(SEGGRP_PREFIX).await.unwrap();
        let ours = marker_key(nonce_bits);
        assert_eq!(
            markers.iter().filter(|(key, _)| *key == ours).count(),
            1,
            "{name}"
        );
        assert_eq!(
            markers.len() as u64,
            2 + rivals,
            "{name}: one marker per session"
        );

        // The recovery itself. A landed batch is found by a re-read, with no second commit;
        // otherwise exactly one re-drive commits, never losing — its ledger precondition was read
        // afresh, so a rival that moved the ledger meanwhile cannot make it `Conflict`, as a
        // replay of the in-doubt batch would — and it installs the original identity.
        let state = store.state();
        match kind {
            Unknown::Applied { .. } => {
                assert!(state.lost.is_empty(), "{name}: {:?}", state.lost);
                assert!(state.committed.is_empty(), "{name}: no second commit");
            }
            Unknown::LateAtLedgerRead => {
                // The re-drive was built on the ledger the late batch had just moved, so it lost
                // only its identity's `require_absent`s — to its own session.
                assert_eq!(
                    state.lost,
                    vec![Lost {
                        collision: true,
                        ..Lost::default()
                    }],
                    "{name}"
                );
                assert!(
                    state.committed.is_empty(),
                    "{name}: the late batch is the one"
                );
                assert_eq!(
                    state.late,
                    Some((CommitOutcome::Committed, false)),
                    "{name}"
                );
            }
            _ => {
                assert!(
                    state.lost.is_empty(),
                    "{name}: the re-drive lost: {:?}",
                    state.lost
                );
                assert_eq!(state.committed.len(), 1, "{name}: one re-drive");
                let puts: Vec<&[u8]> = state.committed[0]
                    .puts
                    .iter()
                    .map(|(key, _)| key.as_slice())
                    .collect();
                let (session_key, group_key) = (mpu_key(&id), marker_key(nonce_bits));
                assert_eq!(
                    puts,
                    vec![session_key.as_slice(), group_key.as_slice(), MPUCTL_KEY],
                    "{name}: the re-drive installs the original identity"
                );
            }
        }
        if matches!(kind, Unknown::Late) {
            assert_eq!(
                state.late,
                Some((CommitOutcome::Conflict, true)),
                "F3: the late batch must lose, its mpu:<id> already present"
            );
        }
    }
}

/// **(F)** While a batch may still land, the create never answers a refusal that would strand
/// it: here the ledger fills before the re-drive, the batch never lands, and the create ends
/// `Unsettled`, naming the identity the in-doubt batch carries — not `SessionCap`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_an_in_doubt_create_never_answers_a_refusal_that_strands_its_session() {
    let tiny = profile(4, 1, 1, 1, 1);
    let inner = store_with_bucket().await;
    created(create_session(&*inner, &request("first"), &tiny, &mut Host::new(base(9))).await);
    let store = Interposer::scripted(
        &inner,
        Script {
            unknown: [(1, Unknown::DroppedThenRival)].into(),
            ..Script::new(tiny)
        },
    );
    let mut host = Host::new(base(0));
    let err = create_session(&store, &request("target"), &tiny, &mut host)
        .await
        .expect_err("in doubt at the cap");
    let unsettled = as_unsettled(&err);
    let (id_bits, nonce_bits) = host.identities()[0];
    assert_eq!(unsettled.upload_id, upload_id(id_bits));
    assert_eq!(unsettled.segment_nonce.as_str(), hex(nonce_bits));
    assert!(unsettled.unknown.may_still_commit);
    assert!(matches!(
        unsettled.ended,
        CreateEnd::Refused(Refusal::SlowDown {
            pressure: Backpressure::SessionCap { .. }
        })
    ));
    assert!(
        err.to_string().contains(unsettled.upload_id.as_str()),
        "the fault names the upload id that may still land: {err}"
    );
    assert_eq!(
        host.identities().len(),
        1,
        "nothing re-minted while in doubt"
    );
}

/// **(F)** A batch left in flight stays in doubt whatever a later commit reports — the mixed-code
/// run. Under a profile whose contention budget is two attempts, commit 1 answers
/// `CommitUnknownResult { may_still_commit: true }` and stays in flight (1031), and commit 2
/// answers it with `may_still_commit: false` without landing (1021). The budget is spent and the
/// settling look finds nothing, but the first batch may still land, so the create ends
/// `Unsettled`, naming its identity — not `SlowDown { AdmissionContention }`. Released after the
/// create returned, that batch lands: the refusal would have stranded an admitted, counted
/// session that no client was told of.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_batch_left_in_flight_stays_in_doubt_after_a_later_out_of_flight_outcome() {
    let tiny = profile(4, 1, 1, 1, 1);
    assert_eq!(
        tiny.max_sessions(),
        2,
        "a contention budget of two attempts"
    );
    let inner = store_with_bucket().await;
    let store = Interposer::scripted(
        &inner,
        Script {
            unknown: [
                (1, Unknown::Late),
                (
                    2,
                    Unknown::Dropped {
                        may_still_commit: false,
                    },
                ),
            ]
            .into(),
            ..Script::new(tiny)
        },
    );
    let target = request("target");
    let mut host = Host::new(base(0)).instant();
    let err = within(create_session(&store, &target, &tiny, &mut host))
        .await
        .expect_err("in doubt when the budget is spent");
    let unsettled = as_unsettled(&err);
    let (id_bits, nonce_bits) = host.identities()[0];
    assert_eq!(unsettled.upload_id, upload_id(id_bits));
    assert_eq!(unsettled.segment_nonce.as_str(), hex(nonce_bits));
    assert!(
        unsettled.unknown.may_still_commit,
        "the doubt of the batch still in flight, not the later out-of-flight one"
    );
    assert!(
        matches!(
            &unsettled.ended,
            CreateEnd::Refused(Refusal::SlowDown {
                pressure: Backpressure::AdmissionContention { attempts: 2 }
            })
        ),
        "{:?}",
        unsettled.ended
    );
    assert_eq!(host.identities().len(), 1, "nothing re-minted");
    assert_eq!(store.state().commits, 2, "the whole budget spent");
    assert!(
        inner.scan(MPU_PREFIX).await.unwrap().is_empty(),
        "nothing has landed when the create returns"
    );

    // After the create returned, the batch it reported in flight lands: admitted and counted.
    assert_eq!(store.release().await, CommitOutcome::Committed);
    assert_admitted(
        &*inner,
        &[(unsettled.upload_id.clone(), target)],
        &tiny,
        &[],
    )
    .await;
}

/// **(F)** While a batch is in doubt, a re-drive that loses **only the ledger CAS** — a rival
/// admitted between its ledger read and its commit — is contention, not a sign the earlier batch
/// landed: only this create's own session at `mpu:<id>` would mean that. The create backs off and
/// re-drives the same identity again, and is admitted exactly once: one identity, one session,
/// one count, beside the rival's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_an_in_doubt_re_drive_that_loses_the_ledger_is_retried_not_taken_as_landed() {
    let budget = shipped();
    for may_still_commit in [true, false] {
        let name = format!("may_still_commit = {may_still_commit}");
        let inner = store_with_bucket().await;
        let store = Interposer::scripted(
            &inner,
            Script {
                unknown: [(1, Unknown::Dropped { may_still_commit })].into(),
                rivals: Some(2..=2),
                ..Script::new(budget)
            },
        );
        let target = request("target");
        let mut host = Host::new(base(0)).instant();
        let id = created(within(create_session(&store, &target, &budget, &mut host)).await);

        // The store first: the session it was answered `Created` for exists, and is counted.
        let sessions = inner.scan(MPU_PREFIX).await.unwrap();
        assert!(
            sessions.iter().any(|(key, _)| *key == mpu_key(&id)),
            "{name}: answered Created, so mpu:{id} must exist"
        );
        assert_eq!(sessions.len(), 2, "{name}: the rival's and this create's");
        assert_eq!(inner.scan(SEGGRP_PREFIX).await.unwrap().len(), 2, "{name}");
        assert_eq!(
            ledger(&*inner).await.count(),
            2,
            "{name}: the rival and this create"
        );
        let (id_bits, nonce_bits) = host.identities()[0];
        let (_, record) = session(&*inner, &id).await;
        assert_eq!(record.segment_nonce().as_str(), hex(nonce_bits), "{name}");
        assert_eq!(record.object(), target.object, "{name}");

        // Then the path: the original identity, re-driven past a lost CAS.
        assert_eq!(host.identities().len(), 1, "{name}: nothing re-minted");
        assert_eq!(id, upload_id(id_bits), "{name}: the original identity");
        assert_eq!(
            host.waits.len(),
            2,
            "{name}: one backoff for the unknown outcome, one for the lost CAS"
        );
        {
            let state = store.state();
            assert_eq!(
                state.commits, 3,
                "{name}: unknown, lost re-drive, committed re-drive"
            );
            assert_eq!(
                state.lost,
                vec![Lost {
                    ledger: true,
                    ..Lost::default()
                }],
                "{name}"
            );
            assert_eq!(state.committed.len(), 1, "{name}");
            assert_eq!(session_key(&state.committed[0]), Some(mpu_key(&id)));
            assert_eq!(state.rivals, 1);
        }
    }
}

/// **(F)** The in-doubt batch lands **between** the recovery's look at `mpu:<id>` (absent) and
/// its ledger read — and it took the last slot, so that read finds the ledger full. The create
/// looks at `mpu:<id>` once more before ending on that `SessionCap`, finds its own session, and
/// answers `Created` under the original identity, with the one increment its batch made: the
/// session it admitted is never stranded behind a refusal or an `Unsettled`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_late_create_that_fills_the_ledger_between_recovery_reads_is_created() {
    let tiny = profile(4, 1, 1, 1, 1);
    assert_eq!(tiny.max_sessions(), 2);
    let inner = store_with_bucket().await;
    let first = request("first");
    let x = created(create_session(&*inner, &first, &tiny, &mut Host::new(base(9))).await);
    let store = Interposer::scripted(
        &inner,
        Script {
            unknown: [(1, Unknown::LateAtLedgerRead)].into(),
            ..Script::new(tiny)
        },
    );
    let target = request("target");
    let mut host = Host::new(base(0));
    let id = created(within(create_session(&store, &target, &tiny, &mut host)).await);
    let (id_bits, _) = host.identities()[0];
    assert_eq!(
        id,
        upload_id(id_bits),
        "Created under the original upload id"
    );
    assert_eq!(host.identities().len(), 1, "nothing re-minted");
    {
        let state = store.state();
        assert_eq!(
            state.late,
            Some((CommitOutcome::Committed, false)),
            "the in-doubt batch landed between the recovery's reads"
        );
        assert!(
            state.committed.is_empty(),
            "no re-drive: the ledger was full"
        );
        assert!(state.lost.is_empty());
    }
    assert_admitted(&*inner, &[(x, first), (id, target)], &tiny, &[]).await;
}

/// **(F)** An unknown outcome on the create's **last** permitted attempt is still settled. A rival
/// is admitted before each of the create's first `MAX_ADMISSION_CAS_ATTEMPTS - 1` commits, so the
/// last one it may make is commit 46, which answers `CommitUnknownResult`:
///
/// * applied, whether or not it says it may still commit — the settling look finds the session,
///   and the answer is `Created` under the original identity, never an error for a session that
///   is admitted and counted;
/// * not applied, out of flight — the look settles that it never landed, so the create ends as it
///   would have with nothing in doubt: `SlowDown { AdmissionContention }`, nothing written;
/// * not applied, possibly still in flight — the look proves nothing, so the create ends
///   `Unsettled`, naming the identity;
/// * not applied, and another session then took its `mpu:<id>` — the look finds a session that
///   is not this create's own, which is no answer either: `Unsettled`, never `Created` with
///   another upload's id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_an_unknown_outcome_on_the_last_contention_attempt_is_settled() {
    /// What the create must end with.
    enum Ends {
        Created,
        Refused,
        Unsettled,
    }
    let budget = shipped();
    let last = MAX_ADMISSION_CAS_ATTEMPTS;
    let contention = Refusal::SlowDown {
        pressure: Backpressure::AdmissionContention { attempts: last },
    };
    for (name, kind, ends) in [
        (
            "applied, may still commit",
            Unknown::Applied {
                may_still_commit: true,
            },
            Ends::Created,
        ),
        (
            "applied, out of flight",
            Unknown::Applied {
                may_still_commit: false,
            },
            Ends::Created,
        ),
        (
            "dropped, out of flight",
            Unknown::Dropped {
                may_still_commit: false,
            },
            Ends::Refused,
        ),
        (
            "dropped, may still commit",
            Unknown::Dropped {
                may_still_commit: true,
            },
            Ends::Unsettled,
        ),
        (
            "dropped, its id then taken by another session",
            Unknown::DroppedThenSquatted(Squat::Nonce),
            Ends::Unsettled,
        ),
    ] {
        let inner = store_with_bucket().await;
        let store = Interposer::scripted(
            &inner,
            Script {
                rivals: Some(1..=last - 1),
                unknown: [(last, kind)].into(),
                ..Script::new(budget)
            },
        );
        let target = request("target");
        let mut host = Host::new(base(0)).instant();
        let outcome = within(create_session(&store, &target, &budget, &mut host)).await;
        assert_eq!(store.state().commits, last, "{name}: every attempt spent");
        assert_eq!(host.identities().len(), 1, "{name}: nothing re-minted");
        let (id_bits, nonce_bits) = host.identities()[0];
        let ours = mpu_key(&upload_id(id_bits));
        let landed = matches!(ends, Ends::Created);
        match ends {
            Ends::Created => {
                let id = created(outcome);
                assert_eq!(id, upload_id(id_bits), "{name}");
                let (_, record) = session(&*inner, &id).await;
                assert_eq!(record.segment_nonce().as_str(), hex(nonce_bits), "{name}");
            }
            Ends::Refused => {
                assert_eq!(
                    outcome.unwrap(),
                    CreateOutcome::Refused(contention.clone()),
                    "{name}"
                );
            }
            Ends::Unsettled => {
                let err = outcome.expect_err("in doubt at the end");
                let unsettled = as_unsettled(&err);
                assert_eq!(unsettled.upload_id, upload_id(id_bits), "{name}");
                assert_eq!(unsettled.segment_nonce.as_str(), hex(nonce_bits));
                assert!(
                    matches!(&unsettled.ended, CreateEnd::Refused(refusal) if *refusal == contention),
                    "{name}: {:?}",
                    unsettled.ended
                );
            }
        }
        // This create's own session exists exactly when its batch landed, and so does its count.
        let own = inner
            .get(&ours)
            .await
            .unwrap()
            .and_then(|found| decode_session_record(&found).ok())
            .is_some_and(|found| found.segment_nonce().as_str() == hex(nonce_bits));
        assert_eq!(own, landed, "{name}");
        let admitted = u64::from(last - 1) + u64::from(landed);
        assert_eq!(ledger(&*inner).await.count(), admitted, "{name}");
    }
}

/// **(F)** Only this create's own session — equal in every field the create wrote and no
/// transition rewrites: nonce, bucket inode, object, content type, creation stamp and its clock —
/// is taken for its in-doubt batch landed. Here the batch is dropped and a session differing in
/// exactly one of those then takes its `mpu:<id>` (after the create read it vacant): each time
/// the recovery reads a collision. The identity cannot be re-minted while its batch may still
/// land, so the create ends `Unsettled { Collided }`, naming it — never `Created` with another
/// upload's id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_in_doubt_a_session_differing_in_one_field_is_a_collision_not_this_create() {
    let budget = shipped();
    for squat in [
        Squat::Nonce,
        Squat::Object,
        Squat::Parent,
        Squat::ContentType,
        Squat::CreatedAt,
        Squat::ClockSource,
    ] {
        // Both spellings of the content type: absent (the squatter declares one) and declared
        // (the squatter declares another).
        for content_type in [None, Some("image/png")] {
            let inner = store_with_bucket().await;
            let store = Interposer::scripted(
                &inner,
                Script {
                    unknown: [(1, Unknown::DroppedThenSquatted(squat))].into(),
                    ..Script::new(budget)
                },
            );
            let target = CreateRequest {
                content_type: content_type.map(str::to_owned),
                ..request("target")
            };
            let mut host = Host::new(base(0)).instant();
            let err = within(create_session(&store, &target, &budget, &mut host))
                .await
                .expect_err("a collision while in doubt");
            let unsettled = as_unsettled(&err);
            let (id_bits, _) = host.identities()[0];
            let name = format!("{squat:?}, content type {content_type:?}");
            assert_eq!(unsettled.upload_id, upload_id(id_bits), "{name}");
            assert!(
                matches!(unsettled.ended, CreateEnd::Collided),
                "{name}: {:?}",
                unsettled.ended
            );
            assert_eq!(host.identities().len(), 1, "{name}: nothing re-minted");
            assert_eq!(store.state().commits, 1, "{name}: no re-drive");
        }
    }
}

/// **(F)** A plain store fault on a create commit — not a `CommitUnknownResult` — ends the create
/// with that fault, after the one commit, writing nothing. With an earlier batch in doubt the same
/// fault ends it `Unsettled { Fault }`, naming the identity that batch carries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_a_plain_store_fault_on_a_create_commit_ends_the_create() {
    let budget = shipped();

    let inner = store_with_bucket().await;
    let before = dump(&*inner).await;
    let store = Interposer::scripted(
        &inner,
        Script {
            fault: Some(1),
            ..Script::new(budget)
        },
    );
    let mut host = Host::new(base(0)).instant();
    let err = within(create_session(
        &store,
        &request("target"),
        &budget,
        &mut host,
    ))
    .await
    .expect_err("the fault ends the create");
    assert_eq!(err.to_string(), INJECTED_FAULT);
    assert!(err.downcast_ref::<CreateFault>().is_none());
    assert_eq!(store.state().commits, 1, "never retried");
    assert_eq!(dump(&*inner).await, before, "nothing written");

    let inner = store_with_bucket().await;
    let store = Interposer::scripted(
        &inner,
        Script {
            unknown: [(
                1,
                Unknown::Dropped {
                    may_still_commit: true,
                },
            )]
            .into(),
            fault: Some(2),
            ..Script::new(budget)
        },
    );
    let mut host = Host::new(base(0)).instant();
    let err = within(create_session(
        &store,
        &request("target"),
        &budget,
        &mut host,
    ))
    .await
    .expect_err("the fault ends the create");
    let unsettled = as_unsettled(&err);
    let (id_bits, _) = host.identities()[0];
    assert_eq!(unsettled.upload_id, upload_id(id_bits));
    assert!(
        matches!(&unsettled.ended, CreateEnd::Fault(fault) if fault.to_string() == INJECTED_FAULT),
        "{:?}",
        unsettled.ended
    );
    assert!(err.to_string().contains(INJECTED_FAULT), "{err}");
}
