# Build notes — #1029 (657.2c): a part commit answered `CommitUnknownResult` is settled or kept in doubt

Target: `getwyrd/wyrd` @ the stack base `pdca-integration/main-r9dd123c9ed70`
(`1f6d6a4`, which folds in #1027 and #1028). All `path:line` below are on the patched tree
(`$PDCA_WORKTREE`).

## Iteration 2 — what changed, and why

The carry-forward said the production logic is judged correct and must not change, and asked for
tests that pin the doubt-keeping store-fault sites the adversary found unpinned. This iteration
does exactly that:

- `crates/core/src/multipart/commit.rs` and `docs/design/architecture/05-building-block-view.md`
  are **byte-identical** to iteration 1's diff (checked by splitting both `patch.diff`s per file and
  comparing: `commit.rs SAME`, `05-building-block-view.md SAME`).
- The test file gains one helper and two legs, placed right after the existing kept-doubt leg
  (`crates/core/tests/multipart_upload_part_commit.rs:4889-5027`):
  - `assert_store_fault` (`:4890`): the cause is `Stop::Store` with the wrapper's exact fault text,
    so a leg cannot pass on some other store fault.
  - `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt` (`:4906`) — the carry-forward's
    "concrete leg". Round 1 `Act::hold()`, round 2 `Act::fail()`: `InDoubt { batch: Commit,
    may_still_commit: true, cause: Store("scripted store fault") }`, round 1's own report kept, no
    compensation, no read after round 2, slot still held; then the held batch lands `Committed`, own
    part, one release. Pins `commit.rs:1325`. A contrast run (round 1 `unknown(false, false)` not
    applied) shows the other side: the slot read after round 1 settles it, so round 2's failure is a
    plain `Stopped { Store, slot_released: false }`.
  - `c_a_store_fault_at_each_re_read_under_a_held_commit_keeps_the_doubt` (`:4966`) — a read fault
    under a held round at each read of the re-read: the part read (`commit.rs:1179`), the first
    session read (`:1197`), the slot read (`:1202`), the last session read (`:1211`), and
    `session_gone`'s slot re-read (`:741`, reached by erasing the `mpu:` record right after the slot
    read). Each run asserts `InDoubt`, `may_still_commit: true`, round 1's own report, the cause
    `Store("scripted read fault")`, the exact reads made before the faulted one
    (`reads_after(Commit, 0) == lap[..n]`, so the fault is known to be at that read), one commit, no
    compensation, slot held, no part. It then lands the held batch (after putting the session record
    back, in the `session_gone` run): `Committed`, own part, one release. That last step shows the
    kept `may_still_commit: true` was true, not just conservative.

The fault is injected by the wrapper's existing `fail_reads` (`:508`), armed at the exact gap by the
existing `at_gaps` hook chain (`:3900`); the held batch is the wrapper's existing `How::Hold`
(`:765-772`), landed on the real store by `release_held` (`:522-535`). No new wrapper mechanism.

### Demonstrated reds for this iteration (the adversary's mutant, one site at a time)

Each run: replace `doubt.end(upload_id, Stop::Store(fault))` with
`stopped(upload_id, Stop::Store(fault))` on that one line (the script asserts the line holds the
target exactly once), run the whole test file under `timeout 900`, then restore the fixed file and
check it byte-for-byte. Script and results: `$PDCA_SCRATCH/pdca-builder-1029-reds/mutate_faults.py`,
`reds-faults.json`.

| Site | Failing leg(s) (69 tests) | Failing run (panic text) |
|---|---|---|
| `commit.rs:741` | `c_a_store_fault_at_each_re_read_…` | "the slot read behind a missing session: Err(Stopped { … Store("scripted read fault") …" |
| `commit.rs:1179` | `c_a_store_fault_at_each_re_read_…`, `c_the_kept_doubt_is_the_round_that_may_still_land` | "the part read: Err(Stopped { …" |
| `commit.rs:1197` | `c_a_store_fault_at_each_re_read_…` | "the first session read: Err(Stopped { …" |
| `commit.rs:1202` | `c_a_store_fault_at_each_re_read_…` | "the slot read: Err(Stopped { …" |
| `commit.rs:1211` | `c_a_store_fault_at_each_re_read_…` | "the last session read: Err(Stopped { …" |
| `commit.rs:1325` | `c_a_later_round_failing_definitely_…` | "round 1 held true: Err(Stopped { … Store("scripted store fault") …" |

Before this iteration, the adversary reported all 67 tests green under each of the five sites other
than `:1179`; now each one fails, in the run that targets it.

### The two advisory gates that failed last time

- **C5 (1 missed of 37):** `commit.rs:1332:34`, `Judged::ReadAgain if send` → `true`. Equivalent
  mutant; no test can kill it. Both the adversary and the code review reached this independently. A
  no-send lap only follows a last session read that called `cannot_land()` (`:1213` → `:785`), so
  every round in doubt has `may_land == false`. The next slot read that finds the slot held then
  empties the doubt via `slot_held()` (`:1206`), and `ReadAgain` needs `doubt.any()` (`:1237`). So
  the arm at `:1338` cannot be reached, and the mutant changes nothing. Killing it would take a
  production change (removing the defensive arm), which the carry-forward rules out. **Survivor,
  equivalent.** Expect C5 to report this same one again.
- **C4-diff-cov: "patch.diff does not apply on origin/main".** Not a patch defect. This bundle is
  stacked on #1028 (`Depends on: 1028`; `stack-base` = `pdca-integration/main-r9dd123c9ed70`). The
  gate resolves its base through `run-verify.sh --print-base`
  (`engine/scripts/run-diff-cov.sh:685`), which reads `$PDCA_BASE` / `$PDCA_VERIFY_BASE` /
  `$PDCA_BRIEF_BASE` and falls back to `origin/main` (`engine/scripts/run-verify.sh:264-268`). The
  driver exports those only to rows with `verifies_base = true`, and the C4-diff-cov row has
  `verifies_base = false` (`pdca.toml:1540`). So the row always measures a stacked bundle against
  `origin/main`, where it cannot apply. Expect it to fail the same way again until #1028 merges and
  this branch is rebased, or until that row is given the stack base. This is a PDCA-side config
  question for the human, not something this patch can change.

## The slice as a whole (unchanged from iteration 1)

**The defect.** On the base, `commit_rounds` returned `CommitError::InDoubt` the moment a commit
round was answered unknown (`git show HEAD:crates/core/src/multipart/commit.rs`, the
`// deferred: #1029` arm in `commit_rounds`): no read, no retry. A round that landed was reported in
doubt, and the slot of a round that provably could not land stayed held.

**The change** — `crates/core/src/multipart/commit.rs`:

- `Doubt` / `Unsure` (`commit.rs:754-831`): every round answered unknown, with its own
  `may_land` flag. `cannot_land` (`:785`) marks every round unable to land (all rounds send the same
  batch, so one fact speaks for all); `session_seen` (`:796`) applies a session read (only a record at
  a higher epoch, `Open` or not, counts — damage and a missing record settle nothing); `slot_held`
  (`:808`) drops exactly the rounds that could no longer land **before** that slot read; `end`
  (`:814`) reports the first round that may still land, else the latest (Abort's rule,
  `abort.rs:394-448`).
- `judge_round` (`commit.rs:1167-1262`) replaces `lost_commit`. Same four reads in the same order
  (part, session, slot, session). The part and first-session facts are applied **before**
  `slot_held` (`:1204`), the last-session fact **after** it (`:1213`), which is what makes "a slot read
  made after the change" the only thing that settles. Then, in order: the own-commit proof
  (`:1217`, slot gone + last session read `Same` + a round in doubt → `Landed`), then damage, then the
  #1028 classification, with three doubt-only arms: slot gone under a moved session →
  `InDoubt(SessionMoved)` (`:1230-1234`); slot held under a session first seen moved by the last read →
  `ReadAgain` (`:1237-1239`, the peer is `stage.rs:1713-1732`); slot held, session `Same` → `Again`
  (`:1254-1255`).
- `commit_rounds` (`commit.rs:1271-1355`): keeps the `Doubt`, sends the same batch again while a
  round may land, does at most one read-only lap per round (`:1332-1338`; a second `ReadAgain` is
  unreachable — see the doc at `:226-228` — and ends in doubt rather than looping), and ends through
  `rounds_end` (`:1128-1140`): doubt kept → `InDoubt` with the stop as its cause, no compensation;
  no doubt → the #1028 `unexplained_stop` path unchanged.
- `session_gone` (`commit.rs:722-743`) takes the `Doubt`: slot key absent behind a missing session is
  `NoSuchUpload` with nothing in doubt, `InDoubt(SessionMoved)` (no round can land) with a round in
  doubt; any record there is `SessionRecordMissing`, keeping the doubt. The compensation passes an
  empty `Doubt` (`:1052`), so its behaviour is unchanged.
- Public API: `InDoubt` gains `cause: Stop` (`:391`), as Abort's, Stage's and Create's `InDoubt`
  carry; `may_still_commit` is now "the store said so **and** no read showed it unable to land"
  (`:380-385`), never raised above the store's report. `Stop` gains `SessionMoved` (`:438`) and
  `OutcomeUnknown` (`:441`, the cause the compensation's unchanged immediate stop now names,
  `failed`, `:836`). `CommitRoundsSpent`'s doc and message (`:397-402`, Display) no longer claim every
  round was answered `Conflict`, which is false once rounds can be answered unknown (the one
  existing assertion on that text, `multipart_upload_part_commit.rs:3684-3688`, follows it).
  `CommitError::InDoubt`'s Display appends the cause; `source()` stays the store's report so
  `classify` reads `Indeterminate`.
- Docs: the module doc gains *A commit in doubt* (`commit.rs:114-164`), the stops row and the bounds
  (`:203-204`, `:220-230`) are updated; `docs/design/architecture/05-building-block-view.md:250`
  replaces "settling it is still to land" with what settling now does (rubric: docs currency).

**Design source.** The brief's *Design* section and attempt 7's `Doubt` / module doc
(`results/issue_927/iteration-v7/patch.diff:340-411`, `:799-882`, `:1210-1411`), re-fitted onto
#1028's `lost_commit` shape rather than v7's single-function loop. Deliberately **not** taken from
v7: its rule that a changed non-empty prior settles (corrected at iteration 4 — here a changed prior
settles nothing, `commit.rs:1188-1190`), its compensation settling (#1030's scope), and its
`OwedChunks` / `max_staged_chunks` plumbing (not in this base).

## Test file — `crates/core/tests/multipart_upload_part_commit.rs`

Appended (`:3849-5630`): helpers (`held_count`, `Act::after_all`, `Rig::land_held`,
`fail_reads_from_now`, `at_gaps`, `teardown`, `read_slots`, `idle_fence`, `reserve_freed`,
`state_of`, `part_bytes`, `put_at`, `repoint`, `session_damage`, `assert_record_stop`,
`required_slot`, `lap`, and this iteration's `assert_store_fault`) and 24 legs, mapped to the
success criterion:

| Criterion | Leg |
|---|---|
| (B) landed, answered unknown → `Committed`, released once | `b_a_landed_commit_answered_unknown_is_committed` (`:4160`) |
| (B) unlanded re-upload never `Committed` on the prior, ends own part | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` (`:4190`) |
| (B) held batch, runs (i)(ii)(iii) | `b_a_held_commit_is_never_compensated_and_at_most_one_lands` (`:4225`) |
| (B) rival, key required absent, once per flag | `b_a_rival_after_an_unapplied_unknown_commit_is_compensated` (`:4295`) |
| (B) changed prior settles nothing | `b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt` (`:4344`), `b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back` (`:4408`, real re-place shape, 3 restore modes × 5 gaps), `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back` (`:4515`) |
| (B) read-gap leg, 4 histories × 5 gaps | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` (`:4582`) — not-landed runs pin the exact reads before the compensation |
| C1 | `c1_an_empty_part_is_never_committed_on_its_bytes` (`:4683`) |
| C2 | `c2_a_torn_read_never_answers_committed` (`:4761`) |
| C3, C3′ | `c3_a_landed_commit_then_a_rival_is_committed` (`:4799`) |
| every round out of flight → `Stopped(CommitRoundsSpent)`, released | `c_every_commit_round_out_of_flight_compensates_and_reports_the_release` (`:4841`) |
| the kept doubt | `c_the_kept_doubt_is_the_round_that_may_still_land` (`:4867`) |
| the kept doubt over a later round's definite failure (iteration 2) | `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt` (`:4906`) |
| the kept doubt over a read fault at each re-read (iteration 2) | `c_a_store_fault_at_each_re_read_under_a_held_commit_keeps_the_doubt` (`:4966`) |
| slot rewritten under own id → `InDoubt(SlotLost)` | `c_a_slot_rewritten_under_the_attempts_own_id_during_a_held_commit_is_slot_lost` (`:5033`) |
| slot gone under a moved session | `c_a_slot_gone_under_a_moved_session_with_a_commit_in_doubt_is_in_doubt` (`:5063`) |
| (F) short lease | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` (`:5106`) |
| (F) delayed (i)–(iv), idle-fence shape | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` (`:5143`) |
| (H), landed and not-landed | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` (`:5214`) |
| (R) | `r_damage_under_a_held_commit_keeps_the_doubt` (`:5273`, 24 cases — four record kinds, then five kinds of session damage × gaps 0–3 — plus own-commit-first and prior gone), `r_a_missing_session_behind_a_held_commit_is_in_doubt` (`:5427`), `r_a_session_damaged_and_repaired_mid_round_settles_nothing` (`:5472`) |
| wrapper guard after an unseen landing | `h3_a_round_sent_again_requires_the_bytes_its_own_landed_round_released` (`:5603`) |

**Existing code I had to change** (the brief says "appended"; these are the minimum the new
behaviour forces; unchanged from iteration 1):

1. The two H4 commit legs that pinned the old stop-at-once behaviour
   (`h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch`,
   `h4_a_held_commit_that_lands_later_is_released_once`) are removed (`:2616-2619`); they assert the
   defect. Their histories are covered by the (B) legs above.
2. `h4_a_composed_error_keeps_its_class_and_its_message` (`:2685`) now holds the commit **and**
   conflicts every later round, since a held round alone is now retried and lands.
3. `in_doubt` (`:1494-1527`) checks the cause is named and that the flag is never above the store's
   report (it used to require equality; a read can now lower it).
4. The wrapper's slot guard (`:449`, `:548-555`, `:595-604`, `:628-641`): it dropped an attempt's slot
   from its record once that attempt's own batch released it, so it rejected the retry the design
   requires after a held round lands unseen (the retry requires the bytes that round released). It
   now remembers released bytes and accepts exactly those; H3's rejections are unchanged (the H3 test
   still passes) and a new H3 leg pins both sides.
5. The `CommitRoundsSpent` message assertion in `l_every_loop_ends_after_exactly_its_bound`
   (`:3684-3688`), for the reason above.

## Red → green

- **Runner.** The project's test entry point is `cargo xtask ci` (`[gates] runner =
  "./engine/xtask.sh"`, subcmd `ci`); xtask has no single-file subcommand. For the quick red/green
  checks I ran `cargo test -p wyrd-core --test multipart_upload_part_commit` wrapped in
  `timeout 900`, inside the worktree. Every leg also bounds itself (`within`, `LEG_TIMEOUT` = 60 s,
  `:110`, `:138`). Gate (E) below is the real runner.
- **Post-fix:** 69 passed, 0 failed.
- **Pre-fix, base `commit.rs`:** the test file does not build (iteration 1: 34 errors, 30 × `no
  field cause on InDoubt` and 4 × `no variant SessionMoved`). The new API is part of the fix, so the
  behavioural red is M0 below. As the brief pre-declares, C4-verify will report `PASS (green-only)`.
- **Demonstrated reds** (the brief's list). Each is a temporary edit to `commit.rs`, applied by a
  script that asserts each edit target occurs exactly once, runs the named leg with `--exact`, and
  restores the fixed file (`cmp`-verified). **Re-run in this iteration on the final test file**
  (`$PDCA_SCRATCH/pdca-builder-1029-reds/mutate.py`, `reds.json`). Every row matches iteration 1;
  M0's count grew by the two new legs.

| # | Temporary edit | Named leg | Result |
|---|---|---|---|
| M0 | 657.2b's behaviour in the new API: stop in doubt on the unknown outcome, no read | whole file | **23 of 69 fail**: every new leg that injects an unknown outcome, the two iteration-2 legs included. The 45 kept #1027/#1028 legs pass, and so does the wrapper-guard leg `h3_a_round_sent_again_…`, which injects none |
| M1 | "a part is there" → `Landed` before the slot read | `b_an_unlanded_reupload_…` | FAIL |
| M2 | part bytes equal own record → `Landed` | `c1_…` | FAIL |
| M3 | own-commit proof judged on the first session read | `c2_…` | FAIL |
| M4 | `cannot_land` clears the doubt | `c3_…` | FAIL |
| M5 | another attempt's slot read as `Rewritten` (attempt_id ignored) | `c3_…` (C3′ run) | FAIL |
| M6 | rival branch before the own-commit proof | `c3_…` | FAIL |
| M7 | compensate under the newer epoch while a round is held | `h_a_commit_held_…` | FAIL |
| M8 | a changed non-empty prior settles | both changed-prior legs | FAIL (each leg run separately) |
| M9 | last-session fact applied before `slot_held` | read-gap leg | FAIL |
| M10 | `slot_held` never settles | read-gap leg | FAIL |
| M11 | doubt search → `.last()` | kept-doubt leg | FAIL |
| M12 | a lapsed lease (now ≥ expiry) drops the doubt | (F) delayed leg | FAIL. The plain short-lease leg stays green under it, by design, so the two legs tell "short" from "lapsed" |
| M13a–e | the doubt dropped on each damage arm (torn part, torn slot, damaged last session read, record behind a missing session, prior gone) | (R) | each FAILs |
| M13f | damage at a session read taken as a move | `r_a_session_damaged_and_repaired_…` | FAIL |
| S741–S1325 | the doubt dropped on each store-fault arm (iteration 2) | the two new legs | each FAILs; see the table above |

## Refutation of my own test (forced)

- **(a) Genuine red?** Yes. With the fix reverted the file does not build (the new API is part of the
  fix). With only the settling reverted (M0), 23 legs fail, including both new legs. For this
  iteration's legs specifically: each of the six store-fault mutants (the exact edit the adversary
  used) fails one of the two new legs, in the run that targets that site, while the fixed tree
  passes 69 of 69.
- **(b) Production path?** Yes. Both new legs drive `wyrd_core::multipart::commit::commit_part`
  through `Rig::commit` (`:1098-1101`) over a real `RedbMetadataStore::in_memory()`, staged by the
  real `stage_part`. The wrapper only answers the batch (held, or failed), fails the chosen read, and
  erases the session record on the real store at the chosen gap. Nothing in the leg reimplements
  production logic.
- **(c) Fixture includes the fault?** Yes. The held round is a real batch kept unapplied, and each
  leg lands it on the real store afterwards and asserts `Committed`, so the doubt the error reports
  is shown to be a real one. The read fault is the store's own error path (`Scripted::get` returns
  `Err` before reading, `:706-709`), at exactly the read under test. The leg also checks which reads
  were made before it. The `session_gone` run really removes the `mpu:` record, so the last session
  read takes the `Gone` arm (`commit.rs:1226`) and `session_gone`'s own slot read (`:729`) is the one
  that faults.

## Considered and ruled out (iteration 2)

- **One leg per site** (six `#[tokio::test]`s). Rejected: the five read sites share one setup and
  differ only by gap and key, so a table (as the file's other gap legs do, e.g. `:4582`) keeps them
  aligned. Each run's label is in every assertion message, so a failure names its site (shown in the
  red table).
- **Faulting the session_gone re-read after a terminal delete** instead of erasing only `mpu:`.
  Rejected as the main run. After a terminal delete the held batch can never land, so the test could
  not show that `may_still_commit: true` was right. With only `mpu:` erased, putting it back lets the
  held batch land, which shows it.
- **Changing `commit.rs:1338`** to kill the equivalent C5 mutant. Ruled out by the carry-forward (no
  production change). The arm is unreachable and ends in doubt rather than looping.

## Considered and ruled out (iteration 1)

- **Keep `may_still_commit` equal to the store's report** (Stage/Create do). Rejected: after a read
  shows the session moved, a held round cannot land; reporting `true` would tell the caller to wait
  for something that cannot happen. Lowering it is the brief's "as Abort does" (Abort clears its held
  doubt on a moved session, `abort.rs:426-435`). `outcome` stays the store's raw report.
- **`cause: Option<Stop>`** for the compensation's immediate stop instead of `Stop::OutcomeUnknown`.
  Rejected to match the peers' non-optional `cause`; the variant is documented as only an
  `InDoubt::cause`, and #1030 replaces that stop.
- **A cheaper read-only lap (slot, then session only).** Rejected: the four-read lap keeps one read
  order for every round and keeps `judge_round` single; cost is at most 4 extra reads once per round
  (bounds doc `commit.rs:220-230`).
- **v7's single `commit_part` loop.** Rejected in favour of keeping #1028's `lost_commit` shape
  (now `judge_round`), so the #1028 classification table and its 45 kept legs are unchanged.

## Gate (E)

`./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`), the project's own runner:
**`xtask ci: all checks passed`**, exit 0, twice. The first run started before one doc-comment
wording edit in the new leg's `///` text. The second ran on the final tree (04:42–04:46,
`ci-v2-final.log`), and `patch.diff` was checked to equal `git diff HEAD` of that tree. Every step
ran, none skipped: `typos`, `lint_docs`, `render_site --check` (101 pages, link audit OK), the
gitlink/unsafe/blackbox guards, `cargo fmt --all -- --check`, workspace clippy, build,
`cargo test --workspace --exclude wyrd-dst` (`multipart_upload_part_commit`: `running 69 tests`, all
ok, both new legs listed), `cargo-machete`, `cargo deny` (all three runs, `advisories ok` on the
all-features run), conformance vectors, statics, deploy-guard, and the DST clippy and tests under
`--cfg madsim`. `patch.diff` also applies cleanly (`git apply --check`) to a clean export of the
stack base `1f6d6a4`.

`cargo fmt --all` was run over the tree before the final diff was taken; the only file it could
touch is the test file, and `cargo fmt --check` is part of the gate above.

No external dependency was missing (`typos` and the docs renderer are both present on this host),
so there is no NEEDS-HUMAN item from this beat. No branch was pushed and no PR opened.

## C5 (advisory) survivors

One, equivalent: `commit.rs:1332:34` (`Judged::ReadAgain if send` → `true`), argued above. The six
store-fault sites the adversary found unpinned are now pinned (table above).

## Scratch

`$PDCA_SCRATCH/pdca-builder-1029-reds/`: `mutate.py` + `reds.json` (the brief's demonstrated reds,
re-run this iteration), `mutate_faults.py` + `reds-faults.json` (the six store-fault sites),
`ci-v2.log` / `ci-v2-final.log` (gate E), and `applycheck/` (a clean export of the stack base,
used only for `git apply --check`). The harness owns and reclaims this directory, so nothing was
deleted by hand.
