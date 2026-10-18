//! Issue #894 — `CreateMultipartUpload`'s **admission and session creation**,
//! `wyrd_core::multipart::create::create_session`, driven over a real store
//! (`RedbMetadataStore::in_memory()`), real preconditions and real contention, on a multi-thread
//! tokio runtime (proposal 0016's *Create session* row, `0016:657`, and its unknown-outcome rule,
//! `0016:2009-2027`).
//!
//! "Writes nothing" below means the store is byte-identical before and after: no `mpu:`, no
//! `seggrp:` and no `mpuctl` write.
//!
//! * **(A)** free-running storms of 8, 10 and 16 creates on an empty store all admit — the
//!   levels #508's rejected attempts falsely refused.
//! * **(A′)** a store wrapper forces the losing schedule: every commit is held until every live
//!   creator has built its batch, so each round has one winner and every other creator loses the
//!   `mpuctl` CAS. The last creator loses far more than `MAX_UPLOAD_ID_ATTEMPTS` times and is
//!   still admitted, and under a profile larger than the shipped one, one creator more than the
//!   shipped `MAX_SESSIONS` is admitted too. The waits each create asks for are recorded.
//! * **(B)** the real bounds, each writing nothing: a full ledger, a profile skew, a missing
//!   bucket, a bucket marker deleted or rewritten under the create, and the size headroom,
//!   found on encoded bytes through a wrapper that enforces the value ceiling.
//! * **(C)** the collision budget and the contention budget never share, and an identity source
//!   that repeats an existing session's identity, byte for byte, gets a re-mint, never that
//!   session.
//! * **(F)** an unknown commit outcome is recovered under the original identity, with the
//!   `mpuctl` precondition rebuilt from a fresh read, the doubt kept, and a settling read before
//!   the create gives up.
//!
//! The wrappers keep their state in struct fields, never in a `static` (ADR-0035).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::watch;
use wyrd_core::metadata::{self, InodeId, SegmentNonce, MAX_SEGMENT_INDEX, MAX_VALUE_BYTES};
use wyrd_core::multipart::create::{
    create_session, CreateError, CreateRequest, CreateRuntime, Unsettled,
};
use wyrd_core::multipart::{
    admission_backoff_millis, decode_admission_record, decode_retire_obligation,
    decode_session_record, mpu_key, parse_mpu_key, AdmissionRecord, Backpressure, Budget,
    CreateOutcome, Refusal, SessionRecord, SessionState, UploadId, MAX_ADMISSION_CAS_ATTEMPTS,
    MAX_INFLIGHT_PARTS, MAX_PARTS_PER_SESSION, MAX_PART_CHUNKS, MAX_PART_NUMBER, MAX_SESSIONS,
    MAX_STAGED_CHUNKS, MAX_UPLOAD_ID_ATTEMPTS, MPUCTL_KEY, MPU_PREFIX, W_REF,
};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_traits::{
    BoxError, CommitOutcome, CommitUnknownResult, MetadataStore, Result as StoreResult, ScanPage,
    WriteBatch,
};

// ─────────────────────────────── fixtures ───────────────────────────────

const BUCKET: &str = "photos";
/// The root inode every Alpha object dirent lives under, as `<bucket>/<key>` (ADR-0046 decision 3).
const ROOT: InodeId = 0;
const CLOCK: &str = "wall";
const CREATED_AT: u64 = 1_000;
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":1}"#;
/// The marker of a bucket deleted and created again under the same name.
const RECREATED_MARKER: &[u8] = br#"{"name":"photos","created_at_millis":2}"#;
/// XORed into an identity's number to derive its nonce, so a nonce never spells its upload id.
const NONCE_SALT: u128 = 0x5a5a_5a5a_5a5a_5a5a_5a5a_5a5a_5a5a_5a5a;
/// No leg may hang the suite: a schedule that never settles fails here instead.
const LEG_TIMEOUT: Duration = Duration::from_secs(120);

fn object(name: &str) -> String {
    format!("{BUCKET}/{name}")
}

fn request<'a>(object: &'a str, content_type: Option<&'a str>) -> CreateRequest<'a> {
    CreateRequest {
        bucket: BUCKET,
        parent: ROOT,
        object,
        content_type,
        created_at_millis: CREATED_AT,
        clock_source: CLOCK,
    }
}

/// A [`Budget`] decoded from its wire spelling — the only way one comes into existence.
fn budget(w_ref: u64, mpc: u32, mpps: u32, mip: u32, msc: u32) -> Budget {
    let json = format!(
        "{{\"w_ref\":{w_ref},\"max_part_chunks\":{mpc},\"max_parts_per_session\":{mpps},\
         \"max_inflight_parts\":{mip},\"max_staged_chunks\":{msc}}}"
    );
    metadata::decode(json.as_bytes()).expect("a legal profile")
}

/// The shipped profile: the five numbers `mpuctl` stores, at their shipped values.
fn shipped() -> Budget {
    let profile = budget(
        W_REF,
        MAX_PART_CHUNKS,
        MAX_PARTS_PER_SESSION,
        MAX_INFLIGHT_PARTS,
        MAX_STAGED_CHUNKS,
    );
    assert_eq!(profile.max_sessions(), MAX_SESSIONS);
    profile
}

/// A profile admitting exactly `n` sessions: every per-session cap at 1, so `U_ref` is 2 and
/// `W_ref = 2n` derives `max_sessions = n`.
fn profile_of(n: u64) -> Budget {
    let profile = budget(2 * n, 1, 1, 1, 1);
    assert_eq!(profile.max_sessions(), n);
    profile
}

/// The `mpuctl` bytes of a ledger at `count` under `profile` — this codec's own spelling.
fn ledger_bytes(count: u64, profile: &Budget) -> Vec<u8> {
    let profile_json = metadata::encode(profile);
    let bytes = format!(
        "{{\"count\":{count},\"max_sessions\":{},\"profile\":{}}}",
        profile.max_sessions(),
        std::str::from_utf8(&profile_json).unwrap()
    )
    .into_bytes();
    decode_admission_record(&bytes).expect("a canonical ledger");
    bytes
}

/// Identity number `n`: upload id `n`, nonce `n ^ NONCE_SALT`, both 32 lowercase hex.
fn identity(n: u128) -> (UploadId, SegmentNonce) {
    (
        UploadId::new(format!("{n:032x}")).unwrap(),
        SegmentNonce::new(format!("{:032x}", n ^ NONCE_SALT)).unwrap(),
    )
}

/// The caller's runtime: an identity source, a fixed jitter, and a wait that only records.
struct Script {
    /// Identities handed out first, in order (a repeating source puts an existing one here).
    queue: VecDeque<(UploadId, SegmentNonce)>,
    /// Then fresh identities `next + 1`, `next + 2`, …
    next: u128,
    minted: Vec<(UploadId, SegmentNonce)>,
    jitter: u64,
    /// Every wait asked for, in order.
    waits: Vec<u64>,
}

impl Script {
    fn fresh(base: u128) -> Self {
        Self::repeating(Vec::new(), base)
    }

    fn repeating(first: Vec<(UploadId, SegmentNonce)>, base: u128) -> Self {
        Self {
            queue: first.into(),
            next: base,
            minted: Vec::new(),
            jitter: u64::MAX,
            waits: Vec::new(),
        }
    }
}

impl CreateRuntime for Script {
    fn mint(&mut self) -> (UploadId, SegmentNonce) {
        let minted = self.queue.pop_front().unwrap_or_else(|| {
            self.next += 1;
            identity(self.next)
        });
        self.minted.push(minted.clone());
        minted
    }

    fn jitter(&mut self) -> u64 {
        self.jitter
    }

    fn wait(&mut self, millis: u64) -> impl Future<Output = ()> + Send {
        self.waits.push(millis);
        std::future::ready(())
    }
}

async fn store_with_bucket() -> RedbMetadataStore {
    let store = RedbMetadataStore::in_memory().unwrap();
    store
        .commit(WriteBatch::new().put(metadata::bucket_key(BUCKET), MARKER))
        .await
        .unwrap();
    store
}

/// Every record in the store, in key order.
async fn snapshot(store: &impl MetadataStore) -> Vec<(Vec<u8>, Bytes)> {
    let mut all = store.scan(b"").await.unwrap();
    all.sort();
    all
}

/// The stored ledger, decoded — and re-encoded to exactly the bytes stored: every later create
/// and the terminal delete CAS it on those bytes (serialization identity).
async fn ledger(store: &impl MetadataStore) -> Option<AdmissionRecord> {
    let bytes = store.get(MPUCTL_KEY).await.unwrap()?;
    let record = decode_admission_record(&bytes).expect("a valid ledger");
    assert_eq!(
        metadata::encode(&record),
        bytes,
        "mpuctl round-trips byte for byte"
    );
    Some(record)
}

/// Every session in the store, decoded, by upload id — each re-encoded to exactly the bytes
/// stored, since every later transition CASes the session on them (serialization identity).
async fn sessions(store: &impl MetadataStore) -> BTreeMap<String, SessionRecord> {
    let mut found = BTreeMap::new();
    for (key, value) in store.scan(MPU_PREFIX).await.unwrap() {
        let id = parse_mpu_key(&key).expect("a canonical mpu: key");
        let record = decode_session_record(&value).expect("a valid session record");
        assert_eq!(
            metadata::encode(&record),
            value,
            "{id} round-trips byte for byte"
        );
        found.insert(id.as_str().to_owned(), record);
    }
    found
}

/// The admission invariant over the whole store: exactly the `expected` sessions (upload id →
/// object), each `Open` under the root for its object, one `seggrp:` marker per session nonce and
/// none other, and `mpuctl` counting exactly them.
async fn assert_population(store: &impl MetadataStore, expected: &BTreeMap<String, String>) {
    let found = sessions(store).await;
    let ids: BTreeSet<&String> = found.keys().collect();
    assert_eq!(
        ids,
        expected.keys().collect(),
        "the store must hold exactly the admitted sessions"
    );
    let mut nonces = BTreeSet::new();
    for (id, record) in &found {
        assert!(
            matches!(record.state(), SessionState::Open {}),
            "{id}: a created session is Open"
        );
        assert_eq!(record.parent(), ROOT, "{id}");
        assert_eq!(record.object(), expected[id], "{id}");
        assert_eq!(record.epoch(), 0, "{id}");
        assert_eq!(record.attempts(), 0, "{id}");
        assert_eq!(record.clock_source(), CLOCK, "{id}");
        assert_eq!(record.created_at_millis(), CREATED_AT, "{id}");
        assert!(nonces.insert(metadata::seggrp_key(record.segment_nonce())));
    }
    let groups: BTreeSet<Vec<u8>> = store
        .scan(metadata::SEGGRP_PREFIX)
        .await
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            assert_eq!(&value[..], metadata::SEGGRP_MARKER);
            key
        })
        .collect();
    assert_eq!(groups, nonces, "one seggrp: marker per session nonce");
    let count = ledger(store).await.map_or(0, |ledger| ledger.count());
    assert_eq!(
        count,
        expected.len() as u64,
        "mpuctl counts every session, in the batch that installed it"
    );
}

fn created(outcome: &Result<CreateOutcome, CreateError>) -> UploadId {
    match outcome {
        Ok(CreateOutcome::Created { upload_id }) => upload_id.clone(),
        other => panic!("expected Created, got {other:?}"),
    }
}

fn unknown(may_still_commit: bool) -> BoxError {
    BoxError::from(CommitUnknownResult {
        backend: "test wrapper",
        code: None,
        detail: "the wrapper withheld the outcome".to_owned(),
        may_still_commit,
    })
}

/// The `mpu:<id>` key a create's batch puts — how a wrapper tells creators apart.
fn session_key_of(batch: &WriteBatch) -> Vec<u8> {
    batch
        .puts
        .iter()
        .map(|(key, _)| key)
        .find(|key| key.starts_with(MPU_PREFIX))
        .expect("a create batch puts its session")
        .clone()
}

/// What the batch requires of `mpuctl`: `Some(bytes)`, or `None` for `require_absent`.
fn ledger_precondition(batch: &WriteBatch) -> Option<Bytes> {
    batch
        .preconditions
        .iter()
        .find(|pre| pre.key == MPUCTL_KEY)
        .expect("a create batch preconditions the ledger")
        .expected
        .clone()
}

async fn within<T>(leg: impl Future<Output = T>) -> T {
    tokio::time::timeout(LEG_TIMEOUT, leg)
        .await
        .expect("the leg settled in time")
}

// ─────────────────────── (A) a free-running storm ───────────────────────

/// **(A)** On a fresh, empty store, 8, 10 and 16 concurrent creates all answer `Created`, each
/// level on its own store. These are the levels #508's rejected attempts falsely refused
/// (8 → 2 refused, 10 → 3, 16 → 3; aws-cli's default `max_concurrent_requests` is 10). redb never
/// awaits inside `get` or `commit`, so a valid schedule may serialize them; (A′) is the leg that
/// forces the losing one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_free_running_create_storm_on_an_empty_store_admits_every_create() {
    within(async {
        for creators in [8u128, 10, 16] {
            let store = Arc::new(store_with_bucket().await);
            let mut tasks = Vec::new();
            for i in 0..creators {
                let store = Arc::clone(&store);
                tasks.push(tokio::spawn(async move {
                    let name = object(&format!("storm-{i}"));
                    // Half declare a content type and half do not, so both spellings are stored.
                    let content_type = i.is_multiple_of(2).then_some("application/octet-stream");
                    let asked = request(&name, content_type);
                    let mut script = Script::fresh(i << 64);
                    let outcome = create_session(&*store, &shipped(), &asked, &mut script).await;
                    (name, outcome)
                }));
            }
            let mut expected = BTreeMap::new();
            for task in tasks {
                let (name, outcome) = task.await.unwrap();
                let id = created(&outcome);
                assert!(
                    expected.insert(id.as_str().to_owned(), name).is_none(),
                    "upload ids are pairwise distinct"
                );
            }
            assert_eq!(expected.len() as u128, creators);
            assert_population(&*store, &expected).await;
            for record in sessions(&*store).await.values() {
                let index: u128 = record.object().rsplit('-').next().unwrap().parse().unwrap();
                let declared = index
                    .is_multiple_of(2)
                    .then_some("application/octet-stream");
                assert_eq!(record.content_type(), declared, "{}", record.object());
            }
        }
    })
    .await;
}

// ──────────────────── (A′) the forced losing schedule ────────────────────

/// A wrapper that holds every commit until **every live creator** has read the ledger and built
/// its batch, then releases the round at once. Every batch of a round carries the same `mpuctl`
/// precondition, so redb's serialized commit admits exactly one and every other creator loses the
/// CAS. Creators are told apart by the `mpu:<id>` key their batch puts.
struct Barrier {
    inner: RedbMetadataStore,
    round: Mutex<Round>,
    released: watch::Sender<u64>,
}

#[derive(Default)]
struct Round {
    /// Creators still running.
    live: usize,
    /// Creators whose commit is held in the current round.
    waiting: usize,
    generation: u64,
    /// CAS losses per creator, by its `mpu:<id>` key.
    losses: BTreeMap<Vec<u8>, u32>,
}

impl Barrier {
    fn new(inner: RedbMetadataStore, creators: usize) -> Self {
        Self {
            inner,
            round: Mutex::new(Round {
                live: creators,
                ..Round::default()
            }),
            released: watch::channel(0).0,
        }
    }

    fn release_if_full(round: &mut Round, released: &watch::Sender<u64>) {
        if round.live > 0 && round.waiting == round.live {
            round.waiting = 0;
            round.generation += 1;
            released.send_replace(round.generation);
        }
    }

    /// A creator returned, whatever it answered: it commits no more.
    fn retire(&self) {
        let mut round = self.round.lock().unwrap();
        round.live -= 1;
        Self::release_if_full(&mut round, &self.released);
    }

    fn losses(&self) -> BTreeMap<Vec<u8>, u32> {
        self.round.lock().unwrap().losses.clone()
    }
}

#[async_trait]
impl MetadataStore for Barrier {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        self.inner.scan(prefix).await
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> StoreResult<ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        let creator = session_key_of(&batch);
        let mut released = self.released.subscribe();
        let generation = {
            let mut round = self.round.lock().unwrap();
            let generation = round.generation;
            round.waiting += 1;
            Self::release_if_full(&mut round, &self.released);
            generation
        };
        released
            .wait_for(|current| *current != generation)
            .await
            .expect("the barrier outlives its creators");
        let outcome = self.inner.commit(batch).await?;
        if outcome == CommitOutcome::Conflict {
            *self
                .round
                .lock()
                .unwrap()
                .losses
                .entry(creator)
                .or_default() += 1;
        }
        Ok(outcome)
    }
}

/// `creators` creates through the [`Barrier`], each with its own identity source and a jitter of
/// `u64::MAX`, so each wait it asks for is exactly the backoff window.
async fn forced_storm(profile: Budget, creators: usize) -> (Arc<Barrier>, Vec<(UploadId, Script)>) {
    let barrier = Arc::new(Barrier::new(store_with_bucket().await, creators));
    let mut tasks = Vec::new();
    for i in 0..creators {
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            let name = object(&format!("forced-{i}"));
            let mut script = Script::fresh((i as u128) << 64);
            let outcome =
                create_session(&*barrier, &profile, &request(&name, None), &mut script).await;
            barrier.retire();
            (outcome, script)
        }));
    }
    let mut results = Vec::new();
    for task in tasks {
        let (outcome, script) = task.await.unwrap();
        results.push((created(&outcome), script));
    }
    (barrier, results)
}

/// The per-creator half of (A′): every creator minted once (a lost CAS never re-mints), lost the
/// CAS once per round it did not win, and asked after its `k`-th loss for
/// `admission_backoff_millis(k, jitter)`, so its window grows.
fn assert_forced_schedule(barrier: &Barrier, results: &[(UploadId, Script)]) -> u32 {
    let losses = barrier.losses();
    let mut per_creator: Vec<u32> = results
        .iter()
        .map(|(id, _)| losses.get(&mpu_key(id)).copied().unwrap_or(0))
        .collect();
    for ((id, script), lost) in results.iter().zip(&per_creator) {
        assert_eq!(
            script.minted.len(),
            1,
            "{id}: a lost CAS never spends an identity"
        );
        let asked: Vec<u64> = (0..*lost)
            .map(|k| admission_backoff_millis(k, script.jitter))
            .collect();
        assert_eq!(
            script.waits, asked,
            "{id}: after its k-th lost CAS a create asks for admission_backoff_millis(k, jitter)"
        );
        assert!(
            script.waits.windows(2).all(|pair| pair[0] <= pair[1]),
            "{id}: the backoff window never shrinks"
        );
    }
    per_creator.sort_unstable();
    let rounds: Vec<u32> = (0..results.len() as u32).collect();
    assert_eq!(
        per_creator, rounds,
        "the barrier forces one winner per round: the k-th admitted creator lost exactly k times"
    );
    *per_creator.last().unwrap()
}

/// **(A′)** Eight creates through the forced losing schedule: all eight answer `Created`, the
/// ledger counts eight, and one creator lost the CAS seven times — far more than
/// `MAX_UPLOAD_ID_ATTEMPTS` — and still got its session, with one identity. A contention budget
/// capped at the collision budget refuses it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted() {
    within(async {
        let (barrier, results) = forced_storm(shipped(), 8).await;
        let most = assert_forced_schedule(&barrier, &results);
        assert!(
            most > MAX_UPLOAD_ID_ATTEMPTS,
            "the forced schedule must make one creator lose more than MAX_UPLOAD_ID_ATTEMPTS \
             ({MAX_UPLOAD_ID_ATTEMPTS}) times; it lost {most}"
        );
        assert_eq!(ledger(&barrier.inner).await.unwrap().count(), 8);
        let longest = results
            .iter()
            .map(|(_, s)| &s.waits)
            .max_by_key(|w| w.len())
            .unwrap();
        assert!(
            longest.last() > longest.first(),
            "the window grows across a create's losses: {longest:?}"
        );
        let expected = sessions(&barrier.inner).await.into_iter();
        let expected = expected
            .map(|(id, r)| (id, r.object().to_owned()))
            .collect();
        assert_population(&barrier.inner, &expected).await;
    })
    .await;
}

/// **(A′), second run.** Under a profile whose `max_sessions` is above the shipped
/// `MAX_SESSIONS`, one creator more than the shipped limit goes through the same barrier. All are
/// admitted, and the last lost the CAS `MAX_SESSIONS` times — the whole of the fixed
/// `MAX_ADMISSION_CAS_ATTEMPTS`. The contention budget follows the profile in effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_prime_the_contention_budget_follows_the_profile_in_effect() {
    within(async {
        let profile = profile_of(MAX_SESSIONS + 18);
        assert!(profile.max_sessions() > MAX_SESSIONS);
        let creators = MAX_SESSIONS as usize + 1;
        let (barrier, results) = forced_storm(profile, creators).await;
        let most = assert_forced_schedule(&barrier, &results);
        assert_eq!(
            most, MAX_ADMISSION_CAS_ATTEMPTS,
            "the last creator lost as many CASes as the fixed budget allows, and was admitted"
        );
        let ledger = ledger(&barrier.inner).await.unwrap();
        assert_eq!(ledger.count(), creators as u64);
        assert_eq!(ledger.profile(), &profile);
    })
    .await;
}

// ──────────────── the scripted wrapper (B, C and F legs) ────────────────

/// What [`Scripted`] does with the create's next commit.
#[derive(Debug, Clone, Copy)]
enum OnCommit {
    /// Delegate unchanged.
    Pass,
    /// Admit this many other sessions (real creates on the inner store) first, so this commit's
    /// `mpuctl` CAS loses.
    AdmitFirst(usize),
    /// Delete the bucket marker first.
    DeleteMarkerFirst,
    /// Rewrite the bucket marker first (the bucket deleted and created again).
    RewriteMarkerFirst,
    /// Plant this batch's own `mpu:<id>` first: a collision that arrives as a lost commit.
    PlantSessionFirst,
    /// Apply the batch or not, then answer `CommitUnknownResult { may_still_commit }`.
    Unknown {
        applied: bool,
        may_still_commit: bool,
    },
    /// Keep the batch unapplied and answer `CommitUnknownResult { may_still_commit: true }`; it
    /// lands later, when the wrapper or the test releases it.
    Hold,
    /// Answer `CommitUnknownResult { may_still_commit }` unapplied, and admit this many other
    /// sessions before answering.
    UnknownThenAdmit {
        may_still_commit: bool,
        admit: usize,
    },
    /// Delegate; once it has committed, release the held batch too (it lands late).
    PassThenReleaseHeld,
}

/// One commit the create under test made.
#[derive(Debug, Clone)]
struct Seen {
    session_key: Vec<u8>,
    ledger: Option<Bytes>,
}

#[derive(Default)]
struct ScriptedState {
    plan: VecDeque<OnCommit>,
    /// Any commit carrying a put to this key answers `CommitUnknownResult { may_still_commit:
    /// false }` and is not applied.
    poisoned: Option<Vec<u8>>,
    held: Option<WriteBatch>,
    /// On the first `mpuctl` read while a batch is held: release it, then admit this many others.
    on_ledger_read: Option<usize>,
    /// What the held batch met when it was released.
    late: Option<CommitOutcome>,
    commits: Vec<Seen>,
    next_other: u128,
}

/// A store wrapper that acts on the create's commits (and, for F5, one read) as scripted.
/// Other creates it admits run directly on the inner store, so only the create under test is
/// scripted.
struct Scripted {
    inner: RedbMetadataStore,
    profile: Budget,
    state: Mutex<ScriptedState>,
}

impl Scripted {
    async fn new(profile: Budget, plan: &[OnCommit]) -> Self {
        Self {
            inner: store_with_bucket().await,
            profile,
            state: Mutex::new(ScriptedState {
                plan: plan.iter().copied().collect(),
                next_other: 1 << 100,
                ..ScriptedState::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ScriptedState> {
        self.state.lock().unwrap()
    }

    /// Admit `n` other sessions through real creates on the inner store.
    async fn admit(&self, n: usize) {
        for _ in 0..n {
            let id = {
                let mut state = self.state();
                state.next_other += 1;
                state.next_other
            };
            let name = object(&format!("other-{id}"));
            let mut script = Script::repeating(vec![identity(id)], 0);
            let outcome = create_session(
                &self.inner,
                &self.profile,
                &request(&name, None),
                &mut script,
            )
            .await;
            created(&outcome);
        }
    }

    /// Apply the held batch now; what it met.
    async fn release_held(&self) -> CommitOutcome {
        let held = self.state().held.take().expect("a batch is held");
        let outcome = self.inner.commit(held).await.unwrap();
        self.state().late = Some(outcome);
        outcome
    }

    fn commits(&self) -> Vec<Seen> {
        self.state().commits.clone()
    }
}

#[async_trait]
impl MetadataStore for Scripted {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        let trigger = {
            let mut state = self.state();
            if key == MPUCTL_KEY && state.held.is_some() {
                state.on_ledger_read.take()
            } else {
                None
            }
        };
        if let Some(others) = trigger {
            assert_eq!(self.release_held().await, CommitOutcome::Committed);
            self.admit(others).await;
        }
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        self.inner.scan(prefix).await
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> StoreResult<ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        let session_key = session_key_of(&batch);
        let (act, poisoned) = {
            let mut state = self.state();
            state.commits.push(Seen {
                session_key: session_key.clone(),
                ledger: ledger_precondition(&batch),
            });
            let poisoned = state.poisoned.as_ref() == Some(&session_key);
            (state.plan.pop_front().unwrap_or(OnCommit::Pass), poisoned)
        };
        if poisoned {
            return Err(unknown(false));
        }
        let bucket = metadata::bucket_key(BUCKET);
        match act {
            OnCommit::Pass => self.inner.commit(batch).await,
            OnCommit::AdmitFirst(others) => {
                self.admit(others).await;
                self.inner.commit(batch).await
            }
            OnCommit::DeleteMarkerFirst => {
                self.inner.commit(WriteBatch::new().delete(bucket)).await?;
                self.inner.commit(batch).await
            }
            OnCommit::RewriteMarkerFirst => {
                let rewrite = WriteBatch::new().put(bucket, RECREATED_MARKER);
                self.inner.commit(rewrite).await?;
                self.inner.commit(batch).await
            }
            OnCommit::PlantSessionFirst => {
                let (_, value) = batch.puts.iter().find(|(k, _)| *k == session_key).unwrap();
                let plant = WriteBatch::new().put(session_key.clone(), value.clone());
                self.inner.commit(plant).await?;
                self.inner.commit(batch).await
            }
            OnCommit::Unknown {
                applied,
                may_still_commit,
            } => {
                if applied {
                    assert_eq!(self.inner.commit(batch).await?, CommitOutcome::Committed);
                }
                Err(unknown(may_still_commit))
            }
            OnCommit::Hold => {
                self.state().held = Some(batch);
                Err(unknown(true))
            }
            OnCommit::UnknownThenAdmit {
                may_still_commit,
                admit,
            } => {
                self.admit(admit).await;
                Err(unknown(may_still_commit))
            }
            OnCommit::PassThenReleaseHeld => {
                let outcome = self.inner.commit(batch).await?;
                if outcome == CommitOutcome::Committed {
                    self.release_held().await;
                }
                Ok(outcome)
            }
        }
    }
}

/// Run one create through `store` as `name`, with identity source `script`.
async fn create_through(
    store: &impl MetadataStore,
    profile: &Budget,
    name: &str,
    script: &mut Script,
) -> Result<CreateOutcome, CreateError> {
    create_session(store, profile, &request(name, None), script).await
}

/// The store's records, minus the bucket marker the wrapper itself moved.
async fn snapshot_without_marker(store: &impl MetadataStore) -> Vec<(Vec<u8>, Bytes)> {
    let bucket = metadata::bucket_key(BUCKET);
    let mut all = snapshot(store).await;
    all.retain(|(key, _)| *key != bucket);
    all
}

// ──────────────────── (B) the real bounds, writing nothing ────────────────────

/// **(B)** A ledger at its stored `max_sessions` answers `SlowDown { SessionCap }`, naming the
/// ledger's own numbers, and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_full_ledger_answers_session_cap_and_writes_nothing() {
    within(async {
        let store = store_with_bucket().await;
        let profile = profile_of(3);
        let full = ledger_bytes(3, &profile);
        store
            .commit(WriteBatch::new().put(MPUCTL_KEY, full))
            .await
            .unwrap();
        let before = snapshot(&store).await;
        let mut script = Script::fresh(1);
        let outcome = create_through(&store, &profile, &object("full"), &mut script).await;
        let full = Backpressure::SessionCap {
            count: 3,
            max_sessions: 3,
        };
        assert_eq!(
            outcome.unwrap(),
            CreateOutcome::Refused(Refusal::SlowDown { pressure: full })
        );
        assert_eq!(snapshot(&store).await, before, "a refusal writes nothing");
    })
    .await;
}

/// **(B)** A create whose own profile differs from the one stored in `mpuctl` answers
/// `ProfileSkew`, naming both, and writes nothing — even with room in the ledger.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_profile_skew_is_refused_and_writes_nothing() {
    within(async {
        let store = store_with_bucket().await;
        let stored = profile_of(3);
        store
            .commit(WriteBatch::new().put(MPUCTL_KEY, ledger_bytes(1, &stored)))
            .await
            .unwrap();
        let before = snapshot(&store).await;
        let local = profile_of(4);
        let mut script = Script::fresh(1);
        let outcome = create_through(&store, &local, &object("skew"), &mut script).await;
        assert_eq!(
            outcome.unwrap(),
            CreateOutcome::Refused(Refusal::ProfileSkew {
                local,
                ledger: stored
            })
        );
        assert_eq!(snapshot(&store).await, before);
    })
    .await;
}

/// **(B)** A bucket with no `bucket:` record answers `NoSuchBucket` and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_missing_bucket_answers_no_such_bucket_and_writes_nothing() {
    within(async {
        let store = RedbMetadataStore::in_memory().unwrap();
        let before = snapshot(&store).await;
        let mut script = Script::fresh(1);
        let outcome = create_through(&store, &shipped(), &object("nobucket"), &mut script).await;
        assert_eq!(
            outcome.unwrap(),
            CreateOutcome::Refused(Refusal::NoSuchBucket)
        );
        assert_eq!(snapshot(&store).await, before);
    })
    .await;
}

/// **(B)** A bucket marker deleted, or rewritten, between the create's read and its commit (the
/// wrapper acts at commit) answers `NoSuchBucket`. The create writes nothing: no `mpu:`, no
/// `seggrp:`, no `mpuctl`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket() {
    within(async {
        for act in [OnCommit::DeleteMarkerFirst, OnCommit::RewriteMarkerFirst] {
            let store = Scripted::new(shipped(), &[act]).await;
            let before = snapshot_without_marker(&store.inner).await;
            let mut script = Script::fresh(1);
            let outcome = create_through(&store, &shipped(), &object("moved"), &mut script).await;
            assert_eq!(
                outcome.unwrap(),
                CreateOutcome::Refused(Refusal::NoSuchBucket),
                "{act:?}"
            );
            assert_eq!(
                store.commits().len(),
                1,
                "{act:?}: one commit, then the refusal"
            );
            assert_eq!(
                snapshot_without_marker(&store.inner).await,
                before,
                "{act:?}"
            );
        }
    })
    .await;
}

/// A store wrapper that enforces the value ceiling every backend inherits, as FoundationDB does:
/// a batch putting a value over [`MAX_VALUE_BYTES`] fails, whole. It counts every read and commit.
struct Ceiling {
    inner: RedbMetadataStore,
    reads: AtomicUsize,
    commits: AtomicUsize,
}

#[async_trait]
impl MetadataStore for Ceiling {
    async fn get(&self, key: &[u8]) -> StoreResult<Option<Bytes>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> StoreResult<Vec<(Vec<u8>, Bytes)>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.scan(prefix).await
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> StoreResult<ScanPage> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> StoreResult<CommitOutcome> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        if let Some((key, value)) = batch.puts.iter().find(|(_, v)| v.len() > MAX_VALUE_BYTES) {
            return Err(BoxError::from(format!(
                "value too large: {} bytes under {} (limit {MAX_VALUE_BYTES})",
                value.len(),
                String::from_utf8_lossy(key)
            )));
        }
        self.inner.commit(batch).await
    }
}

/// The JSON spelling of a string, escapes included — the encoder's own.
fn json_string(text: &str) -> String {
    String::from_utf8(metadata::encode(&text).to_vec()).unwrap()
}

/// This test's own spelling of every state a session created from `request` with `nonce` can
/// reach, every number at the widest the decoder allows. Each is decoded by the production
/// decoder, which proves it a legal, canonical record of that session.
fn widest_states(request: &CreateRequest<'_>, nonce: &SegmentNonce) -> Vec<Vec<u8>> {
    let max = u64::MAX;
    let object = json_string(request.object);
    let content_type = request
        .content_type
        .map(|ct| format!("\"content_type\":{},", json_string(ct)))
        .unwrap_or_default();
    let digest = "f".repeat(64);
    let states = [
        r#"{"kind":"Open"}"#.to_owned(),
        r#"{"kind":"Aborting"}"#.to_owned(),
        format!(
            "{{\"kind\":\"Completing\",\"fenced_at_millis\":{max},\"segments_written\":{},\
             \"publish_target\":{{\"parent\":{},\"name\":{object},\"epoch\":{max}}}}}",
            MAX_SEGMENT_INDEX + 1,
            request.parent,
        ),
        format!(
            "{{\"kind\":\"Completed\",\"completion\":{{\"inode\":{max},\"version\":{max},\
             \"etag\":\"{digest}-{MAX_PART_NUMBER}\",\"completed_at_millis\":{max},\
             \"complete_fingerprint\":\"{digest}\"}}}}"
        ),
    ];
    states
        .iter()
        .map(|state| {
            let bytes = format!(
                "{{\"parent\":{},\"object\":{object},{content_type}\"created_at_millis\":{},\
                 \"clock_source\":{},\"segment_nonce\":\"{nonce}\",\"epoch\":{max},\
                 \"attempts\":{},\"state\":{state}}}",
                request.parent,
                request.created_at_millis,
                json_string(request.clock_source),
                u32::MAX,
            )
            .into_bytes();
            decode_session_record(&bytes).expect("a reachable state the decoder accepts");
            bytes
        })
        .collect()
}

fn widest_len(request: &CreateRequest<'_>, nonce: &SegmentNonce) -> usize {
    widest_states(request, nonce)
        .iter()
        .map(Vec::len)
        .max()
        .unwrap()
}

/// **(B) Size headroom.** The boundary is found on **encoded bytes**: the object name is made of
/// characters JSON escapes six bytes wide, so its character count is a sixth of what it costs,
/// and `Completing` carries it twice. A request whose widest reachable state is one byte under
/// the ceiling, or exactly at it, is admitted and can be torn down through a store that enforces
/// the ceiling; one byte over is refused before any read or commit, naming the size and the
/// ceiling, and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_the_size_headroom_is_measured_on_encoded_bytes() {
    within(async {
        let name = format!("{BUCKET}/{}", "\u{1}".repeat(8_000));
        let (_, nonce) = identity(7);
        let bare = widest_len(&request(&name, Some("")), &nonce);
        assert!(bare < MAX_VALUE_BYTES);
        assert!(
            name.len() * 2 < MAX_VALUE_BYTES / 4,
            "a check on character length would see room to spare"
        );
        let at_ceiling = MAX_VALUE_BYTES - bare;
        for (pad, widest) in [
            (at_ceiling - 1, MAX_VALUE_BYTES - 1),
            (at_ceiling, MAX_VALUE_BYTES),
            (at_ceiling + 1, MAX_VALUE_BYTES + 1),
        ] {
            let content_type = "a".repeat(pad);
            let asked = request(&name, Some(&content_type));
            assert_eq!(
                widest_len(&asked, &nonce),
                widest,
                "the probe is where it claims"
            );
            let store = Ceiling {
                inner: store_with_bucket().await,
                reads: AtomicUsize::new(0),
                commits: AtomicUsize::new(0),
            };
            let before = snapshot(&store.inner).await;
            let mut script = Script::repeating(vec![identity(7)], 100);
            let outcome = create_session(&store, &shipped(), &asked, &mut script).await;
            if widest > MAX_VALUE_BYTES {
                assert_eq!(
                    outcome.unwrap(),
                    CreateOutcome::Refused(Refusal::SessionTooLarge {
                        bytes: widest as u64,
                        limit: MAX_VALUE_BYTES as u64,
                    })
                );
                assert_eq!(
                    store.reads.load(Ordering::SeqCst),
                    0,
                    "refused before any read"
                );
                assert_eq!(store.commits.load(Ordering::SeqCst), 0, "or any commit");
                assert_eq!(
                    snapshot(&store.inner).await,
                    before,
                    "and it writes nothing"
                );
                continue;
            }
            let id = created(&outcome);
            let key = mpu_key(&id);
            let stored = store.inner.get(&key).await.unwrap().unwrap();
            let open = decode_session_record(&stored).unwrap();
            // Torn down by the Abort fence's own route, through the ceiling.
            let teardown = open
                .open_teardown(&id)
                .expect("an Open session has a teardown");
            let obligation = teardown.obligation();
            let payload = metadata::encode(obligation.payload());
            decode_retire_obligation(&obligation.key(), &payload).unwrap();
            let fence = WriteBatch::new()
                .require(key.clone(), stored.clone())
                .require_absent(obligation.key())
                .put(key.clone(), metadata::encode(teardown.session()))
                .put(obligation.key(), payload);
            assert_eq!(
                store.commit(fence).await.unwrap(),
                CommitOutcome::Committed,
                "a session admitted {} byte(s) under the ceiling can be torn down",
                MAX_VALUE_BYTES - widest
            );
            // And every state it can reach, at its widest, is a value the ceiling accepts.
            let mut current = store.inner.get(&key).await.unwrap().unwrap();
            for state in widest_states(&asked, &nonce) {
                let next = Bytes::from(state);
                let write = WriteBatch::new()
                    .require(key.clone(), current.clone())
                    .put(key.clone(), next.clone());
                assert_eq!(store.commit(write).await.unwrap(), CommitOutcome::Committed);
                current = next;
            }
        }
    })
    .await;
}

// ───────────── (C) two retry budgets, and no adopted session ─────────────

/// **(C)** When the first minted identity collides — its upload id with an existing `mpu:`
/// record, or its nonce with an existing `seggrp:` marker — the create re-mints and succeeds,
/// leaving the existing session as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_colliding_identity_is_re_minted() {
    within(async {
        let profile = shipped();
        for (label, repeat) in [("upload id", false), ("nonce", true)] {
            let store = store_with_bucket().await;
            let mut first = Script::repeating(vec![identity(1)], 0);
            let existing =
                created(&create_through(&store, &profile, &object("a"), &mut first).await);
            let planted = store.get(&mpu_key(&existing)).await.unwrap().unwrap();
            let colliding = if repeat {
                // A fresh upload id that brings the existing session's nonce with it.
                (identity(2).0, identity(1).1)
            } else {
                // The existing upload id with a fresh nonce.
                (identity(1).0, identity(3).1)
            };
            let mut script = Script::repeating(vec![colliding.clone(), identity(4)], 0);
            let id = created(&create_through(&store, &profile, &object("b"), &mut script).await);
            assert_eq!(id, identity(4).0, "{label}: re-minted after the collision");
            assert_eq!(script.minted, vec![colliding, identity(4)], "{label}");
            assert_eq!(
                store.get(&mpu_key(&existing)).await.unwrap().unwrap(),
                planted,
                "{label}: the existing session is untouched"
            );
            let expected = BTreeMap::from([
                (existing.as_str().to_owned(), object("a")),
                (id.as_str().to_owned(), object("b")),
            ]);
            assert_population(&store, &expected).await;
        }
    })
    .await;
}

/// **(C)** When every one of `MAX_UPLOAD_ID_ATTEMPTS` minted identities collides, the create ends
/// with a typed error and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_every_identity_colliding_ends_in_a_typed_error_writing_nothing() {
    within(async {
        let store = store_with_bucket().await;
        let profile = shipped();
        let mut first = Script::repeating(vec![identity(1)], 0);
        created(&create_through(&store, &profile, &object("a"), &mut first).await);
        let before = snapshot(&store).await;
        let repeats = vec![identity(1); MAX_UPLOAD_ID_ATTEMPTS as usize + 1];
        let mut script = Script::repeating(repeats, 0);
        let outcome = create_through(&store, &profile, &object("b"), &mut script).await;
        let attempts = match outcome {
            Err(CreateError::IdentityCollisions { attempts }) => attempts,
            other => panic!("expected IdentityCollisions, got {other:?}"),
        };
        assert_eq!(attempts, MAX_UPLOAD_ID_ATTEMPTS);
        assert_eq!(script.minted.len(), MAX_UPLOAD_ID_ATTEMPTS as usize);
        assert_eq!(snapshot(&store).await, before);
    })
    .await;
}

/// **(C)** The two budgets never share. A create that loses the `mpuctl` CAS three times — more
/// than `MAX_UPLOAD_ID_ATTEMPTS` — is admitted with its first identity. And under a profile whose
/// contention budget is a single attempt, a collision met on a read, or arriving as a lost commit,
/// re-mints without spending it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_the_contention_and_collision_budgets_never_share() {
    within(async {
        // Lost CASes spend no identity.
        let losses = MAX_UPLOAD_ID_ATTEMPTS as usize + 1;
        let plan = vec![OnCommit::AdmitFirst(1); losses];
        let store = Scripted::new(shipped(), &plan).await;
        let mut script = Script::fresh(0);
        let id = created(&create_through(&store, &shipped(), &object("cas"), &mut script).await);
        assert_eq!(script.minted.len(), 1, "a lost CAS never re-mints");
        assert_eq!(store.commits().len(), losses + 1);
        assert_eq!(script.waits.len(), losses);
        assert_eq!(
            ledger(&store.inner).await.unwrap().count(),
            losses as u64 + 1
        );
        assert!(store.inner.get(&mpu_key(&id)).await.unwrap().is_some());

        // A collision spends no contention budget: profile_of(1) allows a single attempt.
        let tight = profile_of(1);
        assert_eq!(tight.admission_cas_attempts(), 1);
        for (label, plan) in [
            ("on a read", vec![]),
            ("as a lost commit", vec![OnCommit::PlantSessionFirst]),
        ] {
            let store = Scripted::new(tight, &plan).await;
            if plan.is_empty() {
                // A record already under the first identity's upload id, outside any ledger.
                let planted = WriteBatch::new().put(mpu_key(&identity(1).0), b"planted".as_slice());
                store.inner.commit(planted).await.unwrap();
            }
            let mut script = Script::repeating(vec![identity(1), identity(2)], 0);
            let outcome = create_through(&store, &tight, &object("tight"), &mut script).await;
            assert_eq!(created(&outcome), identity(2).0, "{label}");
            assert!(
                script.waits.is_empty(),
                "{label}: a collision is not a lost CAS"
            );
            assert_eq!(ledger(&store.inner).await.unwrap().count(), 1, "{label}");
        }
    })
    .await;
}

/// **(C) No adopted session, byte for byte.** The identity source repeats an **existing**
/// session's identity, and the planted record is exactly what this create would write (same
/// upload id, nonce, bucket, object, content type, `created_at_millis` and `clock_source`), so no
/// field comparison can tell them apart. The wrapper makes any commit for the repeated identity
/// return `CommitUnknownResult { may_still_commit: false }` unapplied. The create answers
/// `Created` under a **different** upload id, the planted record is byte-identical afterwards,
/// and `mpuctl.count()` went up by exactly one. A create that decided ownership by comparing the
/// record it found would commit, meet the unknown outcome, re-read, and adopt the planted session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_a_repeated_identity_is_re_minted_never_adopted() {
    within(async {
        let profile = shipped();
        let store = Scripted::new(profile, &[]).await;
        let name = object("same-request");
        let mut first = Script::repeating(vec![identity(1)], 0);
        let planted_id = created(
            &create_session(
                &store.inner,
                &profile,
                &request(&name, Some("text/plain")),
                &mut first,
            )
            .await,
        );
        let planted_key = mpu_key(&planted_id);
        let planted = store.inner.get(&planted_key).await.unwrap().unwrap();
        store.state().poisoned = Some(planted_key.clone());
        let before = ledger(&store.inner).await.unwrap().count();

        let mut script = Script::repeating(vec![identity(1), identity(2)], 0);
        let outcome = create_session(
            &store,
            &profile,
            &request(&name, Some("text/plain")),
            &mut script,
        )
        .await;
        let id = created(&outcome);
        assert_ne!(
            id, planted_id,
            "the existing session is never answered as Created"
        );
        assert_eq!(id, identity(2).0);
        assert!(
            store
                .commits()
                .iter()
                .all(|seen| seen.session_key != planted_key),
            "no batch is ever built under an identity read present"
        );
        assert_eq!(
            store.inner.get(&planted_key).await.unwrap().unwrap(),
            planted
        );
        assert_eq!(ledger(&store.inner).await.unwrap().count(), before + 1);
    })
    .await;
}

// ───────────── (F) an unknown outcome, recovered under the original id ─────────────

/// The shared end state of F1–F3: one session under the original identity, one marker, one
/// count, one identity minted, and the re-drive's ledger precondition the fresh value.
async fn assert_recovered(
    store: &Scripted,
    script: &Script,
    outcome: &Result<CreateOutcome, CreateError>,
) {
    let id = created(outcome);
    assert_eq!(script.minted.len(), 1, "no second upload id was minted");
    assert_eq!(
        id, script.minted[0].0,
        "recovered under the original upload id"
    );
    let expected = BTreeMap::from([(id.as_str().to_owned(), object("f"))]);
    assert_population(&store.inner, &expected).await;
    let record = &sessions(&store.inner).await[id.as_str()];
    assert_eq!(
        record.segment_nonce(),
        &script.minted[0].1,
        "and the original nonce"
    );
    for seen in store.commits() {
        assert_eq!(
            seen.session_key,
            mpu_key(&id),
            "every commit is under the one identity"
        );
    }
}

/// **F1**: the unknown outcome's batch was applied before the error. The re-read finds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f1_an_applied_unknown_outcome_is_adopted_by_the_re_read() {
    within(async {
        let plan = [OnCommit::Unknown {
            applied: true,
            may_still_commit: true,
        }];
        let store = Scripted::new(shipped(), &plan).await;
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &shipped(), &object("f"), &mut script).await;
        assert_recovered(&store, &script, &outcome).await;
        assert_eq!(store.commits().len(), 1, "settled by the read, no re-drive");
    })
    .await;
}

/// **F2**: the unknown outcome's batch was not applied (and never is). The re-drive commits under
/// the same identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f2_an_unapplied_unknown_outcome_is_re_driven_under_the_same_identity() {
    within(async {
        let plan = [OnCommit::Unknown {
            applied: false,
            may_still_commit: true,
        }];
        let store = Scripted::new(shipped(), &plan).await;
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &shipped(), &object("f"), &mut script).await;
        assert_recovered(&store, &script, &outcome).await;
        assert_eq!(store.commits().len(), 2);
    })
    .await;
}

/// **F3**: the batch is held and applied **after** the recovery's own commit has landed, so the
/// late batch loses its `require_absent(mpu:<id>)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f3_a_late_landing_batch_loses_to_the_re_drive() {
    within(async {
        let plan = [OnCommit::Hold, OnCommit::PassThenReleaseHeld];
        let store = Scripted::new(shipped(), &plan).await;
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &shipped(), &object("f"), &mut script).await;
        assert_eq!(
            store.state().late,
            Some(CommitOutcome::Conflict),
            "the late batch lost its require_absent(mpu:<id>)"
        );
        assert_recovered(&store, &script, &outcome).await;
    })
    .await;
}

/// **F4**: the unknown outcome lands on the **last** attempt the contention budget allows. The
/// create settles it by reading before it returns, and answers `Created` because the batch
/// landed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f4_an_unknown_outcome_on_the_last_budgeted_attempt_is_settled_before_returning() {
    within(async {
        let profile = profile_of(3);
        let plan = [
            OnCommit::AdmitFirst(1),
            OnCommit::AdmitFirst(1),
            OnCommit::Unknown {
                applied: true,
                may_still_commit: false,
            },
        ];
        assert_eq!(plan.len() as u32, profile.admission_cas_attempts());
        let store = Scripted::new(profile, &plan).await;
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &profile, &object("f"), &mut script).await;
        let id = created(&outcome);
        assert_eq!(id, script.minted[0].0);
        assert_eq!(script.minted.len(), 1);
        assert_eq!(store.commits().len(), 3);
        let ledger = ledger(&store.inner).await.unwrap();
        assert_eq!(
            ledger.count(),
            3,
            "two others and this one, each counted once"
        );
        assert!(store.inner.get(&mpu_key(&id)).await.unwrap().is_some());
    })
    .await;
}

/// **F5**: another create fills the ledger between the recovery's reads. The create re-checks its
/// own `mpu:<id>` before giving up on the capacity refusal, and answers `Created` because its
/// earlier batch is there. The wrapper acts on the recovery's ledger read: it releases the held
/// batch (which lands) and admits creates until the ledger is full, so that read finds it full.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f5_a_capacity_refusal_met_in_recovery_is_checked_against_the_earlier_batch() {
    within(async {
        let profile = profile_of(3);
        let store = Scripted::new(profile, &[OnCommit::Hold]).await;
        store.state().on_ledger_read = Some(2);
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &profile, &object("f"), &mut script).await;
        assert_eq!(store.state().late, Some(CommitOutcome::Committed));
        let id = created(&outcome);
        assert_eq!(id, script.minted[0].0);
        assert_eq!(script.minted.len(), 1);
        assert_eq!(
            store.commits().len(),
            1,
            "the held batch is the one that landed"
        );
        let ledger = ledger(&store.inner).await.unwrap();
        assert_eq!(ledger.count(), ledger.max_sessions(), "the ledger is full");
        assert_eq!(sessions(&store.inner).await.len(), 3);
        assert!(store.inner.get(&mpu_key(&id)).await.unwrap().is_some());
    })
    .await;
}

/// **F6, the doubt is kept.** On a profile whose contention budget is two attempts, commit 1 is
/// held (`may_still_commit: true`) and commit 2 returns `CommitUnknownResult { may_still_commit:
/// false }`, unapplied. The create ends with an error that still says `may_still_commit: true` and
/// names the upload id and nonce — never a contention refusal. Released after the create returned,
/// the held batch lands, and the store then holds that one session, counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f6_a_batch_that_may_still_land_keeps_the_create_in_doubt() {
    within(async {
        let profile = profile_of(2);
        let plan = [
            OnCommit::Hold,
            OnCommit::Unknown {
                applied: false,
                may_still_commit: false,
            },
        ];
        assert_eq!(plan.len() as u32, profile.admission_cas_attempts());
        let store = Scripted::new(profile, &plan).await;
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &profile, &object("f"), &mut script).await;
        let doubt = match outcome {
            Err(CreateError::InDoubt(doubt)) => doubt,
            other => panic!("expected the doubt to be kept, got {other:?}"),
        };
        let (upload_id, nonce) = script.minted[0].clone();
        assert_eq!(script.minted.len(), 1);
        assert!(doubt.may_still_commit, "the held batch may still land");
        assert_eq!(doubt.upload_id, upload_id);
        assert_eq!(doubt.segment_nonce, nonce);
        assert!(
            matches!(
                doubt.cause,
                Unsettled::Refused(Refusal::SlowDown {
                    pressure: Backpressure::AdmissionContention { attempts: 2 }
                })
            ),
            "{:?}",
            doubt.cause
        );
        assert!(store
            .inner
            .get(&mpu_key(&upload_id))
            .await
            .unwrap()
            .is_none());

        assert_eq!(store.release_held().await, CommitOutcome::Committed);
        let expected = BTreeMap::from([(upload_id.as_str().to_owned(), object("f"))]);
        assert_population(&store.inner, &expected).await;
    })
    .await;
}

/// **F7, the `mpuctl` precondition is rebuilt, not replayed (`0016:2018-2023`).** Commit 1 returns
/// `CommitUnknownResult { may_still_commit: false }` and is not applied; before the recovery's
/// commit the wrapper admits a second create, moving `mpuctl` from `c` to `c + 1` with room left.
/// The first create answers `Created` under its original identity, both sessions exist with their
/// markers, and the ledger counts `c + 2`. A replay of the original batch carries the stale
/// `require(mpuctl == c)` and could never commit, however often it were driven.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read() {
    within(async {
        let profile = shipped();
        let plan = [OnCommit::UnknownThenAdmit {
            may_still_commit: false,
            admit: 1,
        }];
        let store = Scripted::new(profile, &plan).await;
        store.admit(2).await;
        let c = ledger(&store.inner).await.unwrap().count();
        assert_eq!(c, 2);
        let mut script = Script::fresh(0);
        let outcome = create_through(&store, &profile, &object("f"), &mut script).await;
        let id = created(&outcome);
        assert_eq!(script.minted.len(), 1);
        assert_eq!(id, script.minted[0].0);
        let commits = store.commits();
        assert_eq!(commits.len(), 2);
        let stale = commits[0].ledger.clone().expect("the ledger was present");
        let fresh = commits[1].ledger.clone().expect("the ledger was present");
        assert_eq!(decode_admission_record(&stale).unwrap().count(), c);
        assert_eq!(decode_admission_record(&fresh).unwrap().count(), c + 1);
        assert_eq!(ledger(&store.inner).await.unwrap().count(), c + 2);
        let all = sessions(&store.inner).await;
        assert_eq!(all.len() as u64, c + 2);
        assert!(all.contains_key(id.as_str()));
        let expected = all
            .iter()
            .map(|(k, r)| (k.clone(), r.object().to_owned()))
            .collect();
        assert_population(&store.inner, &expected).await;
    })
    .await;
}
