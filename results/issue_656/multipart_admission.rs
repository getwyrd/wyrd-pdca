//! **Multipart admission and teardown under commit ambiguity** (#656, leg G) — seeded Tier-0
//! DST coverage of the first concurrent multipart writers (`AGENTS.md`: a new concurrent path
//! lands with it), `wyrd_core::multipart::store::{create_session, abort_session}`.
//!
//! Concurrent Creates race the one serialized ledger `mpuctl`, and Aborts race each other and the
//! Creates still in flight, on the simulated-FoundationDB model with the commit-ambiguity nemesis
//! armed — the two legs `commit_ambiguity.rs` drives, by the same codes:
//!
//! * **1021 `commit_unknown_result`** — the struck batch is out of flight: landed or not, and a
//!   re-read settles which;
//! * **1031 `transaction_timed_out`** — the struck batch may land *after* the error, after the
//!   writer's settling re-read and after its re-drive, until the forced deferral
//!   ([`SimFdbMetadataStore::quiesce`]) runs every one still in flight through the resolver;
//!
//! and a third leg that **mixes** the two within one run, each commit's code drawn from a
//! seed-derived coin, so one writer can meet a 1031 and then a 1021 — the order in which a doubt
//! that may still land must outlive a later out-of-flight one.
//!
//! The invariant, asserted after **every** seed over the terminal (post-`quiesce`) store — the
//! relation C-1 asks of these batches (`docs/principles.md` §5), never a count of answers:
//!
//! 1. the number of `mpu:` records equals `mpuctl.count()`;
//! 2. every `mpu:` record has its `seggrp:` marker, and no marker is without its session;
//! 3. every `Aborting` session has its `retire:bytes:` `{session, all}` obligation;
//!
//! and, because a session no client was told about is one only the reaper can reclaim, every
//! `mpu:` record is one some Create answered `Created` for — the unknown-outcome recovery under
//! the original upload id (`0016:2009-2027`) holding across both codes.
//!
//! The nemesis strikes **every** commit the resolver accepts, so an Abort can be struck on every
//! round it may spend: some seeds drive one to the end of its round budget with the session still
//! `Open`, where it ends with a `CommitUnknownResult`. The sweeps require that path to be reached,
//! and the invariant holds there too. That error's `may_still_commit` is checked as well: under
//! one code it is the code's own promise (1021 never leaves a fence in flight, 1031 never says it
//! is out of flight), and a session every Abort of which ended saying no fence of it may still
//! land must still be `Open` once every batch in flight has been resolved.
//!
//! Requires `--cfg madsim` (set by `cargo xtask dst`, which sweeps the `#[madsim::test]` entry
//! points over 50 seeds); without it this file compiles to nothing.

#![forbid(unsafe_code)]
#![cfg(madsim)]

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_core::metadata::{self, bucket_key, seggrp_key, SEGGRP_PREFIX};
use wyrd_core::multipart::store::{abort_session, create_session, CreateHost, CreateRequest};
use wyrd_core::multipart::{
    decode_admission_record, decode_retire_obligation, decode_session_record, parse_mpu_key,
    retire_key, AbortOutcome, Budget, CreateOutcome, PartScope, RetireMode, RetireToken,
    SessionState, UploadId, MAX_INFLIGHT_PARTS, MAX_PARTS_PER_SESSION, MAX_PART_CHUNKS,
    MAX_STAGED_CHUNKS, MPUCTL_KEY, MPU_PREFIX, W_REF,
};
use wyrd_traits::{
    CommitOutcome, CommitUnknownResult, MetadataStore, Result, ScanPage, WriteBatch,
};

#[path = "support/mod.rs"]
mod support;
use support::{
    FdbFidelity, FdbObservations, SimFdbMetadataStore, SIM_COMMIT_UNKNOWN_RESULT,
    SIM_TRANSACTION_TIMED_OUT,
};

const BUCKET: &str = "photos";
const MARKER: &[u8] = br#"{"name":"photos","created_at_millis":7}"#;
/// Concurrent creators. Creator `n` aborts its own session as soon as it is admitted: twice at
/// once when `n % 3 == 0`, so Aborts race each other and the Creates still running; once when
/// `n % 3 == 1`, so an Abort meets every strike on its own; and not at all when `n % 3 == 2`, so
/// `Open` sessions stay counted beside fenced ones.
const CREATORS: usize = 6;
/// Commits the nemesis strikes per seed: all of them. Every accepted Create and Abort batch is
/// struck, so an Abort can be struck on all four rounds it may spend and reach the end of its
/// budget. A lone Abort does so whenever none of its four struck fences lands — 1 in 16 under
/// 1021 — so the sweeps reach that path by design, not by the luck of one seed set; two racing
/// Aborts reach it only when all of both their fences miss. A Create still reaches an answer:
/// each struck batch lands with probability 1/2 (1021) or 1/3 (1031), its re-reads find it, and
/// its contention budget allows 46 attempts — so the scenario fails if a Create ends without one.
const STRIKES: u64 = u64::MAX;
/// Seeds the plain-`#[test]` sweeps drive, so each sees every fate of a struck commit (landed /
/// not landed / still in flight), which no single seed can show.
const SWEEP_SEEDS: u64 = 64;

/// The shipped profile, through the one validating route a `Budget` has.
fn shipped() -> Budget {
    let json = format!(
        "{{\"w_ref\":{W_REF},\"max_part_chunks\":{MAX_PART_CHUNKS},\"max_parts_per_session\":\
         {MAX_PARTS_PER_SESSION},\"max_inflight_parts\":{MAX_INFLIGHT_PARTS},\
         \"max_staged_chunks\":{MAX_STAGED_CHUNKS}}}"
    );
    metadata::decode(json.as_bytes()).expect("the shipped profile is valid")
}

/// The caller's side of a create inside the simulator: seed-derived identity bits and jitter
/// (so a failing seed replays the same identities), and a simulated-time wait.
struct SimHost {
    rng: rand_chacha::ChaCha8Rng,
}

impl SimHost {
    fn new(creator: usize) -> Self {
        use rand::SeedableRng;
        let seed = madsim::runtime::Handle::current().seed();
        Self {
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(seed ^ ((creator as u64 + 1) << 48)),
        }
    }
}

impl CreateHost for SimHost {
    fn mint(&mut self) -> u128 {
        use rand::Rng;
        (u128::from(self.rng.next_u64()) << 64) | u128::from(self.rng.next_u64())
    }

    fn jitter(&mut self) -> u64 {
        use rand::Rng;
        self.rng.next_u64()
    }

    fn wait(&mut self, millis: u64) -> impl Future<Output = ()> + Send {
        madsim::time::sleep(Duration::from_millis(millis))
    }
}

/// Which ambiguity codes a leg's nemesis strikes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codes {
    /// Every struck commit with this one code.
    One(i32),
    /// Each commit's code drawn from a seed-derived coin: 1021 or 1031.
    Mixed,
}

/// Salt for the mixed leg's code coin, so it draws apart from the model's own fault RNG, which
/// is seeded from the same run seed.
const MIX_SALT: u64 = 0x6d69_7865_642d_636f;

/// The store the writers see: the simulated-FoundationDB model, re-armed before each commit with
/// that commit's code when the leg mixes codes, and forwarded to unchanged otherwise. It records
/// the code each writer's struck batches were answered with. Its state is in struct fields, never
/// a `static` (ADR-0035).
struct Nemesis {
    meta: Arc<SimFdbMetadataStore>,
    /// `Some` when the leg mixes codes: the seed-derived coin choosing each commit's code.
    mix: Option<Mutex<rand_chacha::ChaCha8Rng>>,
    /// The codes each writer's struck batches were answered with, in order.
    struck: Mutex<BTreeMap<Writer, Vec<i32>>>,
}

/// A writer, as its batches name it: the `mpu:` key a batch puts, and whether the batch is a
/// fence (it installs a `retire:` obligation) rather than a create.
type Writer = (Vec<u8>, bool);

/// Writers whose struck batches met 1031 and then, on a later batch, 1021: the order in which a
/// doubt that may still land must outlive a later out-of-flight one.
#[derive(Debug, Default, Clone, Copy)]
struct MixedOrders {
    creates: u64,
    fences: u64,
}

impl Nemesis {
    /// Arm `meta`'s nemesis for `codes`, behind this wrapper. Inside the madsim runtime only.
    fn arm(meta: Arc<SimFdbMetadataStore>, codes: Codes) -> Self {
        let mix = match codes {
            Codes::One(code) => {
                meta.arm_commit_ambiguity(code, STRIKES);
                None
            }
            Codes::Mixed => {
                use rand::SeedableRng;
                let seed = madsim::runtime::Handle::current().seed();
                Some(Mutex::new(rand_chacha::ChaCha8Rng::seed_from_u64(
                    seed ^ MIX_SALT,
                )))
            }
        };
        Self {
            meta,
            mix,
            struck: Mutex::new(BTreeMap::new()),
        }
    }

    fn mixed_orders(&self) -> MixedOrders {
        let mut orders = MixedOrders::default();
        for ((_, fence), codes) in self.struck.lock().unwrap().iter() {
            let first_1031 = codes.iter().position(|c| *c == SIM_TRANSACTION_TIMED_OUT);
            let mixed =
                first_1031.is_some_and(|at| codes[at..].contains(&SIM_COMMIT_UNKNOWN_RESULT));
            if mixed && *fence {
                orders.fences += 1;
            } else if mixed {
                orders.creates += 1;
            }
        }
        orders
    }
}

#[async_trait]
impl MetadataStore for Nemesis {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.meta.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        self.meta.scan(prefix).await
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<ScanPage> {
        self.meta.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        if let Some(mix) = &self.mix {
            use rand::Rng;
            let code = if mix.lock().unwrap().next_u64() % 2 == 0 {
                SIM_COMMIT_UNKNOWN_RESULT
            } else {
                SIM_TRANSACTION_TIMED_OUT
            };
            self.meta.arm_commit_ambiguity(code, STRIKES);
        }
        let writer = batch
            .puts
            .iter()
            .find(|(key, _)| key.starts_with(MPU_PREFIX))
            .map(|(key, _)| key.clone());
        let fence = batch
            .puts
            .iter()
            .any(|(key, _)| key.starts_with(b"retire:"));
        let outcome = self.meta.commit(batch).await;
        if let (Err(fault), Some(writer)) = (&outcome, writer) {
            if let Some(code) = fault
                .downcast_ref::<CommitUnknownResult>()
                .and_then(|unknown| unknown.code)
            {
                let mut struck = self.struck.lock().unwrap();
                struck.entry((writer, fence)).or_default().push(code);
            }
        }
        outcome
    }
}

/// How far one seed reached.
#[derive(Debug, Default, Clone, Copy)]
struct Reach {
    exhausted: Exhausted,
    orders: MixedOrders,
}

/// Aborts that spent every round with the session still `Open`, by what their
/// `CommitUnknownResult` said.
#[derive(Debug, Default, Clone, Copy)]
struct Exhausted {
    /// `may_still_commit: true` — a fence of the Abort may still land.
    in_flight: u64,
    /// `may_still_commit: false` — no fence of the Abort is in flight.
    out_of_flight: u64,
}

/// What every writer answered, for the terminal check.
#[derive(Default)]
struct Answers {
    created: BTreeSet<UploadId>,
    /// Sessions some Abort answered `Fenced` or `AlreadyAborting` for.
    aborted: BTreeSet<UploadId>,
    /// Sessions whose every Abort ended at its budget saying no fence of it may still land.
    out_of_flight: BTreeSet<UploadId>,
    exhausted: Exhausted,
}

/// How one Abort ended: a protocol answer, or — every round it may spend struck, the session
/// still `Open` after the last — a `CommitUnknownResult` saying whether a fence of it may still
/// land.
enum AbortEnd {
    Answered(AbortOutcome),
    Exhausted { may_still_commit: bool },
}

/// One creator: admit a session, then abort it as many times at once as [`CREATORS`] says.
async fn creator(meta: Arc<Nemesis>, n: usize) -> (UploadId, Vec<AbortEnd>) {
    let request = CreateRequest {
        bucket: BUCKET.to_owned(),
        parent: 1,
        object: format!("{BUCKET}/obj-{n}"),
        content_type: None,
        created_at_millis: 1_000,
        clock_source: "wall".to_owned(),
    };
    let mut host = SimHost::new(n);
    let outcome = create_session(&*meta, &request, &shipped(), &mut host).await;
    let id = match outcome {
        Ok(CreateOutcome::Created { upload_id }) => upload_id,
        Ok(CreateOutcome::Refused(refusal)) => panic!("creator {n} was refused: {refusal:?}"),
        Err(fault) => panic!("creator {n} ended without an answer: {fault}"),
    };
    let abort_count = match n % 3 {
        0 => 2,
        1 => 1,
        _ => return (id, Vec::new()),
    };
    let aborts: Vec<_> = (0..abort_count)
        .map(|_| {
            let (meta, id) = (Arc::clone(&meta), id.clone());
            madsim::task::spawn(async move { abort_session(&*meta, &id).await })
        })
        .collect();
    let mut ends = Vec::new();
    for abort in aborts {
        match abort.await.unwrap() {
            Ok(answer) => ends.push(AbortEnd::Answered(answer)),
            // The only error an Abort ends with here: every round struck, and the last round's
            // re-read still found the session `Open` (an answer it found would have been given).
            Err(fault) => match fault.downcast_ref::<CommitUnknownResult>() {
                Some(unknown) => ends.push(AbortEnd::Exhausted {
                    may_still_commit: unknown.may_still_commit,
                }),
                None => {
                    panic!("an Abort of creator {n}'s session ended without an answer: {fault}")
                }
            },
        }
    }
    (id, ends)
}

/// The scenario at one seed: fixture, then the armed race, then the deferral, then the invariant.
/// Returns how far it reached.
async fn admission_under_ambiguity_over(meta: Arc<SimFdbMetadataStore>, codes: Codes) -> Reach {
    // The bucket marker, written with the nemesis disarmed so ambiguity strikes the writers under
    // test rather than the fixture.
    let put = WriteBatch::new().put(bucket_key(BUCKET), MARKER);
    assert_eq!(meta.commit(put).await.unwrap(), CommitOutcome::Committed);
    let store = Arc::new(Nemesis::arm(Arc::clone(&meta), codes));

    let tasks: Vec<_> = (0..CREATORS)
        .map(|n| madsim::task::spawn(creator(Arc::clone(&store), n)))
        .collect();
    let mut answers = Answers::default();
    for task in tasks {
        let (id, ends) = task.await.unwrap();
        let mut out_of_flight = !ends.is_empty();
        for end in &ends {
            match end {
                // Never `OperationAborted`: each answer is a success the table allows for an
                // `Open` or `Aborting` session.
                AbortEnd::Answered(AbortOutcome::Fenced | AbortOutcome::AlreadyAborting) => {
                    answers.aborted.insert(id.clone());
                    out_of_flight = false;
                }
                AbortEnd::Answered(other) => panic!("mpu:{id}: an Abort answered {other:?}"),
                AbortEnd::Exhausted { may_still_commit } => {
                    // One code fixes the flag: 1021 never leaves a fence in flight, and 1031
                    // never says one is out of flight.
                    if let Codes::One(code) = codes {
                        assert_eq!(
                            *may_still_commit,
                            code == SIM_TRANSACTION_TIMED_OUT,
                            "mpu:{id}: an Abort under {code} alone ended saying \
                             may_still_commit = {may_still_commit}"
                        );
                    }
                    if *may_still_commit {
                        answers.exhausted.in_flight += 1;
                        out_of_flight = false;
                    } else {
                        answers.exhausted.out_of_flight += 1;
                    }
                }
            }
        }
        if out_of_flight {
            answers.out_of_flight.insert(id.clone());
        }
        assert!(
            answers.created.insert(id),
            "two creates answered one upload id"
        );
    }

    // 1031 promises nothing: a struck batch may still be in flight. Force every one through the
    // resolver so the invariant is asserted over the store's terminal state.
    meta.quiesce();
    assert_eq!(meta.in_flight(), 0, "quiesce resolves every deferred batch");
    assert_terminal_invariant(&*meta, &answers).await;
    Reach {
        exhausted: answers.exhausted,
        orders: store.mixed_orders(),
    }
}

/// The invariant (see the module docs), read from the store itself — independent of what the
/// writers believed.
async fn assert_terminal_invariant(meta: &impl MetadataStore, answers: &Answers) {
    let sessions = meta.scan(MPU_PREFIX).await.unwrap();
    let ledger = meta
        .get(MPUCTL_KEY)
        .await
        .unwrap()
        .expect("a ledger after any admission");
    let ledger = decode_admission_record(&ledger).expect("a canonical ledger");
    assert_eq!(
        sessions.len() as u64,
        ledger.count(),
        "1. every mpu: record is counted, and nothing else is"
    );

    let mut markers = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for (key, value) in &sessions {
        let id = parse_mpu_key(key).expect("a canonical mpu: key");
        let record = decode_session_record(value).expect("a canonical session");
        let marker = seggrp_key(record.segment_nonce());
        assert!(
            meta.get(&marker).await.unwrap().is_some(),
            "2. mpu:{id} has no seggrp: marker"
        );
        markers.insert(marker);
        match record.state() {
            SessionState::Open {} => {
                assert!(
                    !answers.aborted.contains(&id),
                    "mpu:{id} was aborted but is Open"
                );
            }
            SessionState::Aborting {} => {
                // A fence landed. Had every Abort of it said none of its fences may still land,
                // and none answered, one of them misreported a fence still in flight.
                assert!(
                    !answers.out_of_flight.contains(&id),
                    "mpu:{id} is Aborting, but every Abort of it ended saying no fence of it may \
                     still land"
                );
                let token = RetireToken::Session {
                    upload_id: id.clone(),
                    epoch: record.epoch() - 1,
                    part: None,
                };
                let key = retire_key(RetireMode::Bytes, &token);
                let owed = meta.get(&key).await.unwrap().unwrap_or_else(|| {
                    panic!("3. Aborting mpu:{id} has no retire:bytes: obligation")
                });
                let (_, _, owed) =
                    decode_retire_obligation(&key, &owed).expect("a canonical obligation");
                assert!(
                    owed.session() && owed.parts() == Some(&PartScope::All),
                    "mpu:{id}"
                );
            }
            other => panic!("mpu:{id} is {other:?}; nothing here completes"),
        }
        ids.insert(id);
    }
    let stray: Vec<Vec<u8>> = meta
        .scan(SEGGRP_PREFIX)
        .await
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| !markers.contains(key))
        .collect();
    assert!(
        stray.is_empty(),
        "2. seggrp: markers with no session: {stray:?}"
    );
    assert_eq!(
        ids, answers.created,
        "every session is one a Create answered Created for — none stranded, none lost"
    );
    for id in &answers.aborted {
        let (_, value) = sessions
            .iter()
            .find(|(key, _)| parse_mpu_key(key).as_ref() == Ok(id))
            .expect("an aborted session");
        let state = decode_session_record(value).unwrap().state().clone();
        assert_eq!(
            state,
            SessionState::Aborting {},
            "mpu:{id} answered an Abort"
        );
    }
}

/// One seed of the scenario: what the store observed, and how far the seed reached.
fn run(seed: u64, codes: Codes) -> (FdbObservations, Reach) {
    let rt = madsim::runtime::Runtime::with_seed_and_config(seed, madsim::Config::default());
    rt.block_on(async move {
        let meta = Arc::new(SimFdbMetadataStore::with_fidelity(
            FdbFidelity::CommitUnknownResult,
        ));
        let reach = admission_under_ambiguity_over(Arc::clone(&meta), codes).await;
        (meta.observations(), reach)
    })
}

/// One seed under `codes`, inside the madsim test's own runtime; refuses a vacuous seed.
async fn one_seed(codes: Codes) {
    let meta = Arc::new(SimFdbMetadataStore::with_fidelity(
        FdbFidelity::CommitUnknownResult,
    ));
    admission_under_ambiguity_over(Arc::clone(&meta), codes).await;
    let obs = meta.observations();
    assert!(
        obs.ambiguous_conditional_commits >= 1,
        "the {codes:?} nemesis must strike a Create or Abort batch — a seed it never struck is \
         vacuous"
    );
}

/// **The 1021 leg, swept by `cargo xtask dst`** (`MADSIM_TEST_NUM=50`): out-of-flight ambiguity
/// on Create and Abort batches, settled by the writers' own re-reads.
#[madsim::test]
async fn admission_invariants_hold_under_1021() {
    one_seed(Codes::One(SIM_COMMIT_UNKNOWN_RESULT)).await;
}

/// **The 1031 leg, swept by `cargo xtask dst`**: a struck batch may land after the writer's
/// re-read and re-drive; the deferral resolves it against the store's state at that point.
#[madsim::test]
async fn admission_invariants_hold_under_1031() {
    one_seed(Codes::One(SIM_TRANSACTION_TIMED_OUT)).await;
}

/// **The mixed leg, swept by `cargo xtask dst`**: each commit struck with 1021 or 1031 by a
/// seed-derived coin, so a writer whose batch was left in flight can meet an out-of-flight
/// outcome on its next — and must still report the first as possibly landing.
#[madsim::test]
async fn admission_invariants_hold_under_mixed_codes() {
    one_seed(Codes::Mixed).await;
}

/// Across a 1021 sweep the struck batches both **landed** (so a writer's re-read must find its
/// own session or fence) and **did not** (so it must re-drive the same identity), some Abort was
/// struck on every round it may spend and ended at its budget — and the invariant held at every
/// seed.
#[test]
fn the_1021_sweep_sees_struck_batches_land_and_not_land() {
    let (mut landed, mut not_landed, mut exhausted) = (0u64, 0u64, 0u64);
    for seed in 0..SWEEP_SEEDS {
        let (obs, reach) = run(seed, Codes::One(SIM_COMMIT_UNKNOWN_RESULT));
        assert!(
            obs.ambiguous_conditional_commits >= 1,
            "seed {seed} was never struck"
        );
        assert_eq!(
            obs.commits_left_in_flight, 0,
            "1021 is out of flight (seed {seed})"
        );
        landed += obs.ambiguous_commits_that_landed;
        not_landed += obs.ambiguous_commits - obs.ambiguous_commits_that_landed;
        exhausted += reach.exhausted.out_of_flight + reach.exhausted.in_flight;
    }
    assert!(landed >= 1, "no struck batch ever landed across the sweep");
    assert!(
        not_landed >= 1,
        "every struck batch landed across the sweep"
    );
    assert!(
        exhausted >= 1,
        "no Abort reached the end of its round budget across the sweep"
    );
}

/// Across a 1031 sweep some struck batches were left **in flight** and then **landed late** or
/// were **rejected** at the deferral — the cases where an in-doubt Create must never mint a
/// second identity and a late fence must lose to the one already applied — some Abort ended at
/// its round budget, and the invariant held at every seed.
#[test]
fn the_1031_sweep_sees_batches_land_late_and_be_rejected_late() {
    let (mut in_flight, mut late, mut rejected, mut exhausted) = (0u64, 0u64, 0u64, 0u64);
    for seed in 0..SWEEP_SEEDS {
        let (obs, reach) = run(seed, Codes::One(SIM_TRANSACTION_TIMED_OUT));
        assert!(
            obs.ambiguous_conditional_commits >= 1,
            "seed {seed} was never struck"
        );
        in_flight += obs.commits_left_in_flight;
        late += obs.deferred_landings;
        rejected += obs.deferred_rejections;
        exhausted += reach.exhausted.out_of_flight + reach.exhausted.in_flight;
    }
    assert!(
        exhausted >= 1,
        "no Abort reached the end of its round budget across the sweep"
    );
    assert!(
        in_flight >= 1,
        "no struck batch was ever left in flight across the sweep"
    );
    assert!(
        late >= 1,
        "no in-flight batch ever landed late across the sweep"
    );
    assert!(
        rejected >= 1,
        "no in-flight batch was ever rejected late across the sweep"
    );
}

/// Across a mixed sweep some struck batches were left **in flight** (1031), and some Create and
/// some Abort fence met a 1031 and then, on a later batch, a 1021 — the order in which a doubt
/// that may still land must outlive a later out-of-flight one. Some Abort ended at its round
/// budget, and the invariant held at every seed, with every exhausted Abort's `may_still_commit`
/// checked against the terminal store.
#[test]
fn the_mixed_sweep_sees_a_writer_meet_1031_then_1021() {
    let (mut in_flight, mut exhausted) = (0u64, 0u64);
    let mut orders = MixedOrders::default();
    for seed in 0..SWEEP_SEEDS {
        let (obs, reach) = run(seed, Codes::Mixed);
        assert!(
            obs.ambiguous_conditional_commits >= 1,
            "seed {seed} was never struck"
        );
        in_flight += obs.commits_left_in_flight;
        orders.creates += reach.orders.creates;
        orders.fences += reach.orders.fences;
        exhausted += reach.exhausted.out_of_flight + reach.exhausted.in_flight;
    }
    assert!(
        in_flight >= 1,
        "no struck batch was ever left in flight across the sweep"
    );
    assert!(
        orders.creates >= 1,
        "no Create met 1031 and then 1021 across the sweep"
    );
    assert!(
        orders.fences >= 1,
        "no Abort fence met 1031 and then 1021 across the sweep"
    );
    assert!(
        exhausted >= 1,
        "no Abort reached the end of its round budget across the sweep"
    );
}
