# Build notes — issue 656 / multipart-admission-create-abort (iteration 2)

Target: `getwyrd/wyrd` `main` @ `7bf7eb9`. Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
Every `path:line` below is on the patched tree unless it says "base". The patch applies cleanly to
`7bf7eb9` (`git apply --cached --check` against a fresh index of the base tree).

## What this iteration changed, and why

I started from the iteration-1 patch (`iteration-v1/patch.diff`, applied unchanged) and fixed the
five bugs the sign-off listed, each with a regression test that fails on the iteration-1 code (see
"Reds against iteration 1" below). Shape and scope are unchanged, as the sign-off asked.

### 1. "This is my own session" only while a batch is in doubt (`lost()` / `owns()`)

- `lost()` now takes `in_doubt` (`crates/core/src/multipart/store.rs:504`, called with
  `doubt.is_some()` from `attempt()`), and accepts a session at `mpu:<id>` as this create's own
  only when `in_doubt` holds (`store.rs:516`). With nothing in doubt, any session there is a
  collision and the create re-mints. `owns()` (`store.rs:200`) is now only reached while a batch
  is in doubt: from the attempt's first read (`store.rs:400`, guarded by `doubt`), from `lost()`
  under `in_doubt`, and from `settle()` (which only runs with a doubt).
- I also took the adversary's free fix: `lost()` now reads `seggrp:<nonce>` **before**
  `mpu:<id>` (`store.rs:513`). The in-doubt batch writes both at once, so a landing between the
  two reads now shows as this create's own session instead of a collision. On `main` this race
  needs #659's decrement to become reachable, so I did not write a test for it; the order is
  stated in a comment.
- Tests: `c_an_identity_repeated_for_the_same_object_is_re_minted_never_the_other_upload`
  (`crates/core/tests/multipart_create_abort.rs:1157`) — same object, (a) same id and nonce,
  (b) same id with another nonce, (c) same id and nonce after the first upload was aborted. Each
  must re-mint and get its own session. And, for the two surviving `owns()` `&&`→`||` mutants:
  after the fix those mutants can only matter while in doubt, so
  `f_in_doubt_a_session_differing_in_one_field_is_a_collision_not_this_create` (`:2134`) drops
  the create's batch under `CommitUnknownResult { may_still_commit: true }` and then puts at its
  `mpu:<id>` a session that differs from its own in exactly one field (nonce / object / parent).
  The answer must be `Unsettled { Collided }`. By hand: the nonce or parent variant makes both
  mutants answer `Created`, the object variant kills the second.

### 2. Settle before ending (Create's last attempt; also fixes item 5)

- `create_session` no longer returns from inside the loop on a refusal, a budget exhaustion or a
  fault. It breaks out with `ending: Result<Refusal>` (`store.rs:298-335`), and if a batch is in
  doubt it calls `settle()` (`store.rs:338`, `:366`). `settle()` reads `mpu:<id>` once: this
  create's own session → `Created`; not found and out of flight → the ending stands as it would
  with nothing in doubt; may still land (or the read failed) → `Unsettled`, naming the identity.
- This is what 0016 says at `0016:2025-2027` ("re-reads to distinguish landed (proceed) from
  definitively-not-landed (retry or refuse), and with `may_still_commit == false` a single
  re-read settles it").
- The old `refuse()` helper is gone (`settle()` replaces it).
- Test: `f_an_unknown_outcome_on_the_last_contention_attempt_is_settled` (`:2025`). A rival is
  admitted before each of the first 45 commits; commit 46 (the last the shipped budget allows)
  answers `CommitUnknownResult`. Five variants: applied + `may_still_commit` true → `Created`;
  applied + false → `Created`; dropped + false → plain `SlowDown { AdmissionContention { 46 } }`,
  nothing written; dropped + true → `Unsettled { SlowDown { AdmissionContention { 46 } } }`;
  dropped and then another session takes the id → `Unsettled` too, never `Created` (this fifth
  case was added after the first mutation run showed `settle()`'s ownership guard unchecked).
  Iteration 1 answered `Unsettled` in the first three.

### 3. Abort answers the last round's re-read

- `abort_session` (`store.rs:626`) now loops with a round counter and checks the budget **after**
  answering the read (`store.rs:649`). So the final round's re-read goes through `abort_answer`
  like every other read: `Aborting` → `AlreadyAborting`, gone → `NoSuchUpload`. Only a session
  still `Open` after `ABORT_ROUNDS` rounds ends in `RoundsExhausted` or the last round's
  `CommitUnknownResult`.
- Tests (split in two so each schedule shows its own red):
  `d_a_fence_landing_under_the_last_rounds_unknown_outcome_answers_already_aborting` (`:1658`,
  three dropped unknowns, then the fourth applied under an unknown) and
  `d_a_rival_abort_winning_the_last_round_answers_already_aborting` (`:1681`, three rounds lost
  to a reopened session, then a real rival `abort_session` on the inner store wins before the
  fourth). Both must answer `AlreadyAborting` with one fence and one obligation.
- Mutant at old `store.rs:594` (unknown-error guard → `true`):
  `d_a_plain_store_fault_on_the_fence_is_returned_not_retried` (`:1534`) — a plain store fault
  on the fence commit must be returned as it came after one commit, session still `Open`. With
  the guard forced to `true` the Abort would re-read and commit again and answer `Fenced`. I added
  the Create twin too, `f_a_plain_store_fault_on_a_create_commit_ends_the_create` (`:2171`):
  without doubt, the fault itself; with a batch in doubt, `Unsettled { Fault }`.

### 4. Contention budget from the profile, not the shipped 46

- `contention_budget(local)` = `local.max_sessions()` (`store.rs:291`, `:356`). The create only
  commits against a ledger whose profile equals `local` (else `ProfileSkew`), and G8 ties a
  stored `max_sessions` to its profile, so this is also the stored limit. For the shipped
  profile it is exactly `MAX_ADMISSION_CAS_ATTEMPTS` (46), so nothing changes there. It is the
  relation the landed knob validator already demands (`contention >= sessions`,
  `crates/core/src/multipart.rs:5305-5306`, `KnobClamp::AdmissionCasBelowSessions`).
- I did **not** floor it at 46 for smaller profiles. Floor would be `.max(MAX_ADMISSION_CAS_ATTEMPTS)`
  (one call). I left it out because the sign-off said "derive it from the effective profile", the
  derivation holds at any size (among creates, `max_sessions` lost CASes means the ledger filled),
  and a floor would re-introduce a fixed number the profile does not justify. Human: if you want
  more slack for churn on tiny profiles, that one call is where it goes.
- Doc on the constant updated to say it is the shipped-profile value
  (`crates/core/src/multipart.rs:5068-5071`).
- Test: `a_prime_a_larger_profile_gets_the_contention_budget_its_own_session_limit_derives`
  (`:912`) — profile with `w_ref = 2 × W_REF` (`max_sessions` 93), **47** creators through the
  same commit barrier as A′, so the last loses the ledger CAS 46 times and must still be
  admitted. A′'s body moved into a helper, `forced_admission` (`:825`), shared by both A′ tests.

### 5. A late Create that fills the ledger between the recovery's reads

- Fixed by item 2's `settle()`: the `SessionCap` refusal found during recovery now ends in a
  final look at `mpu:<id>`, which finds the session the late batch installed.
- Tests: `f_a_late_create_that_fills_the_ledger_between_recovery_reads_is_created` (`:1971`)
  — tiny profile (2 sessions), one existing session, the create's batch held and landed at the
  recovery's next `mpuctl` read (between its `mpu:` look and its ledger read). The ledger is then
  full; the create must answer `Created` under its original id. And the same ordering on the
  shipped profile was added to the F loop (`f_an_unknown_create_outcome_...`, `:1761`, variant
  "F3 applied between the recovery's reads"): there the re-drive is built on the ledger the late
  batch moved, loses only its `require_absent`s, and `lost()` (in doubt) finds the session.

### The other surviving mutants

- `contended - 1` at old `store.rs:317` (backoff index): the test `Host` now records each jitter
  it hands out, and `Host::assert_backoff` (`multipart_create_abort.rs:207`) asserts
  `waits[i] == admission_backoff_millis(i, jitters[i])` for every wait. Called from A′ (both
  tests, every creator) and from `c_a_collision_does_not_spend_the_contention_budget` (45 waits).
  Any shift of the index changes the window for the first four waits.
- DST strikes: `STRIKES` is now `u64::MAX` (every commit the resolver accepts is struck,
  `crates/dst/tests/multipart_admission.rs:69`). An Abort ending with the last round's
  `CommitUnknownResult` is now an accepted end (`AbortEnd::Exhausted`), counted, and both sweeps
  assert it happened at least once (`:372`, `:396`). With the old `STRIKES = 3` both sweeps fail
  that assertion ("no Abort reached the end of its round budget across the sweep"), so the
  raised strikes are what reach the path. The terminal invariant is unchanged and still holds.

### Test-side changes that are not new legs

- `COMMIT_CEILING` (`multipart_create_abort.rs:87`): the interposer panics past 4096 commits.
  redb's `get` and `commit` never await inside, so a writer whose retry loop is unbounded never
  yields and the 60 s `within()` deadline cannot fire — the first mutation run showed this as a
  TIMEOUT on `rounds += 1` → `*= 1`. With the ceiling that mutant fails in 1.6 s with "the writer
  made more than 4096 commits: its retry loop is unbounded". The highest legitimate count is
  A′ at 47 creators, 1128 commits.

- The interposer's plans `Compete`, `Unknown` and `Reopen` merged into one `Plan::Script`
  (commit-numbered: rivals, unknown outcomes, reopens, a rival Abort, a plain fault). `Unknown`
  gained `Applied { may_still_commit }`, `DroppedThenSquatted(Squat)` and `LateAtLedgerRead`;
  held batches now carry when they land (`Landing`). All state is still in struct fields, no
  `static` (ADR-0035).
- `Host::instant()` records waits without sleeping them, for legs whose schedule the interposer
  forces (the waits change nothing there but run time). A′ with 8 creators keeps real sleeps.

## Reds against iteration 1

I ran the new test file against iteration 1's `store.rs` (extracted from
`iteration-v1/patch.diff`, swapped in, run, then the fixed file copied back and `cmp`-checked).
Command: `cargo test -p wyrd-core --test multipart_create_abort` (what C4-verify runs), under
`timeout 600`. Result: `20 passed; 6 failed` — one or two per bug:

```
c_an_identity_repeated_for_the_same_object_is_re_minted_never_the_other_upload ... FAILED   (bug 1)
  left: UploadId("000000000000000000000000000000a1")
 right: UploadId("000000000000000000000000000000a2")
f_an_unknown_outcome_on_the_last_contention_attempt_is_settled ... FAILED                  (bug 2)
  the create answered: Unsettled(... ended: Refused(SlowDown { pressure: AdmissionContention { attempts: 46 } }) })
d_a_fence_landing_under_the_last_rounds_unknown_outcome_answers_already_aborting ... FAILED (bug 3)
  called `Result::unwrap()` on an `Err` value: CommitUnknownResult { backend: "interposer", ... may_still_commit: true }
d_a_rival_abort_winning_the_last_round_answers_already_aborting ... FAILED                  (bug 3)
  called `Result::unwrap()` on an `Err` value: RoundsExhausted { rounds: 4 }
a_prime_a_larger_profile_gets_the_contention_budget_its_own_session_limit_derives ... FAILED (bug 4)
  the create was refused: SlowDown { pressure: AdmissionContention { attempts: 46 } }
f_a_late_create_that_fills_the_ledger_between_recovery_reads_is_created ... FAILED          (bug 5)
  the create answered: Unsettled(... ended: Refused(SlowDown { pressure: SessionCap { count: 2, max_sessions: 2 } }) })
test result: FAILED. 20 passed; 6 failed
```

The mutant-killing tests (`f_in_doubt_a_session_differing_...`, the two plain-fault tests, the
backoff assertions) pass on iteration 1 too — they target mutants, not iteration-1 bugs. The
mutation run below is their evidence.

## Demonstrated reds required by the brief — temporary edits to the fixed code, all reverted

Each was a temporary edit to `crates/core/src/multipart/store.rs`; the fixed file was copied back
and `cmp`-checked after each run.

**Red 1 — contention budget sized like the collision budget** (`store.rs:291` →
`contention_budget(local).min(MAX_UPLOAD_ID_ATTEMPTS)`, i.e. 2). A′ fails on every run because the
barrier forces the losing order:

```
a_prime_every_creator_is_admitted_through_more_lost_cas_than_the_collision_budget ... FAILED
  the create was refused: SlowDown { pressure: AdmissionContention { attempts: 2 } }
a_prime_a_larger_profile_gets_the_contention_budget_its_own_session_limit_derives ... FAILED
c_a_collision_does_not_spend_the_contention_budget ... FAILED
f_an_unknown_outcome_on_the_last_contention_attempt_is_settled ... FAILED
a_free_running_creates_are_all_admitted_at_8_10_and_16 ... FAILED   (this run; schedule-dependent)
test result: FAILED. 21 passed; 5 failed
```

**Red 2 — a per-session decrement on Abort** (the fence batch also `require`s `mpuctl` and puts
it at `count - 1`). The (E) count assertions fire:

```
d_abort_fences_an_open_session_with_its_obligation_in_one_batch ... FAILED
  assertion `left == right` failed: (E) the fence never moves the count   left: 0  right: 1
d_two_concurrent_aborts_answer_fenced_and_already_aborting ... FAILED
d_an_unknown_fence_outcome_is_settled_by_a_re_read ... FAILED
d_a_fence_landing_under_the_last_rounds_unknown_outcome_answers_already_aborting ... FAILED
d_a_rival_abort_winning_the_last_round_answers_already_aborting ... FAILED
test result: FAILED. 21 passed; 5 failed
```

**Red 3 — re-mint on an unknown outcome** (`Step::Unknown` split out: drop the doubt, mint a new
identity). The one-session / one-increment assertion fires:

```
f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id ... FAILED
  assertion `left == right` failed: F1 applied: exactly one session for this create:
  [UploadId("00000000000000010000000000000001"), UploadId("00000000000000010000000000000003")]
  left: 2  right: 1
(+ 5 more F tests)
test result: FAILED. 20 passed; 6 failed
```

**DST reds** (`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test
multipart_admission`, the flags `cargo xtask dst` uses), same edits:

- re-mint on unknown → all four DST tests fail with `creator N was refused: SlowDown { pressure:
  SessionCap { count: 46, max_sessions: 46 } }`. With every commit struck, re-minting strands
  sessions until the ledger is full — the exact failure `0016:2015-2017` warns about ("a run of
  metadata timeouts can exhaust `MAX_SESSIONS` that way").
- decrement on Abort → all four fail on `1. every mpu: record is counted, and nothing else is`
  (`left: 6, right: 3`).
- `STRIKES = 3` (the old value) → both sweeps fail `no Abort reached the end of its round budget
  across the sweep`.

## Refuting my own test

- **(a) Genuine red?** Yes. Reverting this iteration's fixes (iteration 1's `store.rs`) turns 6
  tests red, each on the assertion that names its bug (above). The three brief-required mutation
  reds and the three DST reds also fire. Reverting the whole patch is a compile error, because
  the functions do not exist on base — the pre-declared C4-verify UNVERIFIABLE (exit 77).
- **(b) Production path?** Yes. Every test calls the production `create_session`,
  `abort_session` and `fence_batch` against a real `RedbMetadataStore::in_memory()`; the DST
  uses the repo's simulated-FoundationDB model. The `Interposer` only delays, reorders, injects
  or answers commits; every read and every applied batch goes to the real store, whose
  preconditions decide every `Conflict`. The rival Abort in the last-round test is a real
  `abort_session` on the inner store. Nothing re-implements Create or Abort.
- **(c) Fixture includes the fault?** Yes. A′ forces the losing order (asserted loss counts
  `{0..N-1}`, for N = 8 and 47); the last-attempt test really admits 45 rivals and really answers
  `CommitUnknownResult` on commit 46, applied or not; the late-ledger test really lands the
  batch between the two recovery reads (asserted via `late == (Committed, false)`); the squat
  test really installs the foreign session; the repeated-identity test really repeats the id and
  nonce; the DST strikes every accepted commit and asserts the exhaustion path was reached.

## Decisions carried from iteration 1 (unchanged)

1. Every-mint-collides is `Err(CreateFault::IdentitiesExhausted)`, not a `Refusal` (a broken
   identity source is a server fault, not backpressure). Human: brief (C) says "refuses"; this is
   still the one place to change if you read that as requiring a `Refusal` value.
2. Core runs the unknown-outcome re-drive itself (the brief let Do choose).
3. A′'s forced schedule is a commit barrier (a `get` barrier cannot tell the attempt's read from
   the classifier's re-read); the test asserts the forced shape directly.
4. `fence_batch` takes `&UploadId`; restore's `Fence` carries the parsed id
   (`crates/custodian/src/restore.rs`). Restore's tests are unchanged.
5. Initial epoch 0, attempts 0.

## Carry-forward: the bucket deletion fence (#511)

Unchanged from iteration 1, and settled at the last sign-off: Create carries only the marker half
of ADR-0046 §4. The precondition sits in one place, `create_batch` (`store.rs:481-485`), with an
in-code `// deferred: #511` marker.

## Open question 2 — constructors

No decoder was loosened. `assert_admitted` still re-encodes every stored session and compares
bytes (with and without `content_type`); every ledger read goes through `decode_admission_record`.

## The rejected review finding

The T4 finding "awaits without a timeout" (`store.rs:386` in iteration 1) was rejected by the
human at sign-off. I recorded that in `review-rejected.md` in the bundle, in the format the T4
gate reads (`<file:line> | <CLASS> | <MATCH> | <reason>`), so it does not block again. The
reason cites `crates/traits/src/lib.rs:1334-1345` (every `MetadataStore` operation terminates;
networked backends own the deadline).

## Gates I ran

- **The new test file**: `cargo test -p wyrd-core --test multipart_create_abort` (what C4-verify
  runs, under `timeout`) — `26 passed; 0 failed`, about 0.4 s.
- **C5 mutation pass**, through the project's own script (`scripts/mutants-in-diff` with
  `PDCA_BUNDLE` and `PDCA_WORKTREE` set — the C5-mutants row's command), over this `patch.diff`:
  - first run: `56 mutants tested in 5m: 1 missed, 43 caught, 11 unviable, 1 timeouts` — missed
    `settle()`'s ownership guard → `true` (`store.rs:374`), timeout on `rounds += 1` → `*= 1`
    (`store.rs:652`). Fixed by the fifth last-attempt case and `COMMIT_CEILING` (above).
  - final run: **`56 mutants tested in 3m: 45 caught, 11 unviable`** — none missed, no
    timeouts. The 11 unviable are all "replace the return value with `Default::default()`" on
    types with no `Default` (`AdmissionRecord`, `SessionRecord`, `Identity`, `CreateOutcome`,
    `Step`, `BoxError`, `AbortOutcome`, restore's `Plan`); they cannot compile.
  - Iteration 1's five survivors are all caught now: the two `owns()` `&&`→`||`, the two
    `contended - 1` backoff-index mutants, and the Abort unknown-error guard → `true`.
- **Full gate**, `./engine/xtask.sh ci` (= `cargo xtask ci`) on this worktree, about 75 s of
  wall time on a warm build cache: typos ✓, `lint_docs.py` ✓, `render_site.py --check` ✓ (99 pages, link audit OK),
  gitlink / unsafe / blackbox guards ✓, `cargo fmt --check` ✓,
  `cargo clippy --workspace --exclude wyrd-dst --all-targets` ✓, build ✓,
  `cargo test --workspace --exclude wyrd-dst` ✓ (no `test result: FAILED` anywhere in the log;
  `multipart_create_abort` 26/26, restore's **unmodified** `restore_open_fence` 11/11 and
  `restore_completing_fence` 7/7, and `custodian_gc` 10/10 in 0.17 s — the suite iteration 1's
  C4-ci run timed out in, so that timeout was the environment, not the code), `cargo-machete` ✓,
  `cargo deny check` ✓, then:
- **✗ `cargo deny --all-features --config deny-all-features.toml check advisories`** — the same
  two RustSec advisories as iteration 1, against `rustls 0.21.12` (RUSTSEC-2026-0346,
  RUSTSEC-2026-0349), via `wyrd-metadata-tikv → tikv-client 0.4.0 → … → tokio-rustls 0.24.1`.
  **Not caused by this patch**: `Cargo.lock`, `Cargo.toml`, `deny.toml` and
  `deny-all-features.toml` are unchanged against `7bf7eb9` (`git diff --quiet` confirms), so the
  base fails the same way. It is a dependency / `deny.toml` decision outside this slice. **Expect
  C4-ci to be red on this step for every bundle until someone deals with it.**
- `ci` stops at the first failure, so I ran the remaining steps through the same wrapper:
  `./engine/xtask.sh conformance` ✓ (5 valid + 6 invalid vectors), `./engine/xtask.sh statics` ✓
  (ADR-0035: no DST-reachable shared mutable global state — the scan covers `crates/core` and
  `crates/dst`; the interposer and the DST host keep state in fields, and the new test-file items
  are `const`s), and `./engine/xtask.sh dst` ✓ — madsim clippy ✓, then every DST file green at `MADSIM_TEST_NUM=50`, about 21 minutes (1222 s of it the existing `custodian.rs`), including the new `multipart_admission` (4/4). The one step left, the
  ADR-0010 orchestrator guard, has no subcommand of its own; it scans for orchestrator /
  Kubernetes imports, and this patch adds none.
- Also: DST clippy under `--cfg madsim` ✓; `cargo doc -p wyrd-core --no-deps` shows no warning in
  `multipart.rs` or `multipart/store.rs` (the crate's doc build fails on base already, on links in
  `write.rs`; the gate does not run it). The target repo has no commit hooks configured
  (`core.hooksPath` unset, no hook files); `cargo fmt --all -- --check` and `typos` are clean on
  the final tree.

## External dependencies

Both listed dependencies were present: `typos` (clean on every changed file) and the docs
renderer (run inside `cargo xtask ci`). No unlisted dependency was needed.
