# Build notes — issue 1015 (625.1), iteration 7: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 6's patch (`iteration-v6/patch.diff`; it still applies cleanly to the
same base) and made the four fixes the iteration-6 sign-off asked for. Each has a test that fails
on the iteration-6 behaviour and a negation that turns it red again. I also closed one test gap
the batch review raised that the sign-off did not rule on (leg B, below — please read that one).
No rebase, no change to the slot-cycle gap, no change to #659's drain.

| Item | Status |
|---|---|
| 1. F2 — a fence commit whose outcome is unknown was reported as "left a session in place" | Fixed. The commit error is classified. An unknown outcome gets its own tally (`unknown`), its own audit action (`reaper-fence-outcome-unknown`) and its own counter (`reaper_fences_unknown`); `hold()` is no longer called for it. The test double now applies the batch and then returns the unknown result, and a seeded DST row covers the four ways such a commit can go, a late landing included. |
| 2. F1 — the largest-fence test used a short session | Fixed (test only). The test now asks the production Create for the largest session it admits (99,706 bytes against the 100,000-byte value ceiling), pins all 16 slots at their widest, checks the cost against an independent count, and runs the production pass over that session. |
| 3. The one-shot silently ignored flags | Fixed. `--reap-abandoned-uploads` is refused with `--gc-expired-pending`, `--endpoints`, `--ids` or `--failure-domains`, before it opens the store. |
| 4. A wrong `--data-dir` gave a false "satisfied" | Fixed, both ways the sign-off offered: a redb data dir with no `meta.redb` is refused and nothing is created, and the outcome line now names the store it ran on. |
| **Not in the carry-forward:** leg B passed if the one-shot exited 0 without running the reaper (T4 finding at `multipart_reaper.rs:601`) | Fixed (test only): leg B now asserts the reaper's own tallies. **My call — see "Leg B".** |
| Four T4 findings that are standing decisions, re-raised in other words | Rows added to `review-rejected.md`, each sourced to the sign-off that made the decision. **Please check — see "T4 findings with no code change".** |

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base in `results/issue_1015/stack-base-tip`.
  Every `path:line` below is on the patched tree (base + `patch.diff`).
- **Prerequisites on the base** (`git log 3c600b6`, the `pdca-integrate` commits): `issue_1027`
  (49463ec), `issue_1007` (c9e236e), `issue_1008` (5a28b33), `issue_1009` (6eaf793), `issue_1010`
  (0de399f), `issue_1011` (c27cdb0), `issue_1061` (21467e6), `issue_1062` (b452551),
  `issue_1013` (3c600b6). Checked again this round.
- **#659's drain is dispatched from `reconcile_step` on the base:** `retirement::reconcile(ctx)`
  at `3c600b6:crates/custodian/src/reconciliation.rs:168`.
- **#806 has not landed** (its deferral marker is still at
  `3c600b6:crates/custodian/src/gc.rs:1566`), so the profile comparison stays one function,
  `check_admission_profile` (`crates/custodian/src/reaper.rs:380`), with the doc comment naming
  #806 as the caller to add.
- **Prior-art search on this base:** no `crates/custodian/src/reaper.rs` in `3c600b6`
  (`git ls-tree`), and no other custodian pass judges an `Open` session.
- **No rebase**, per the sign-off: the patch stacks on unmerged prerequisites and cannot apply on
  `origin/main` (`57aefe0`). Diff coverage stays unmeasured until they land (accepted).

## What I read

`brief.md`, and proposal 0016 decision 6 as in earlier rounds. The carry-forward cites line
numbers in the previous attempt, so I applied `iteration-v6/patch.diff` as the starting tree and
read my own previous `build-notes.md`.

**One exception, and why:** I read `review-batch.md` (the T4 report, 8 findings) and
`review-rejected.md`. The brief's failing-gate line points at the review file, the sign-off says
"the recorded rejections in review-rejected.md stand", and that gate blocks on any finding that
is neither fixed nor matched by a row. Reading the 8 findings is how I found that four of them
are decisions already made that the existing rows did not match, and that one (leg B) had no
ruling. I did not read the other check or review files.

## Fix 1 — a fence whose outcome the store could not report

- **What was wrong.** `fence()` sent every commit error to `hold()` as `fence-commit-failed`.
  `hold()` logs "the multipart reaper left a session in place". But an unknown-result commit
  (`CommitUnknownResult`: FoundationDB's 1021/1031, a TiKV commit timeout) may have landed. If
  it did, the retirement leg of the same step tears the upload down while the audit says it was
  left in place, and no fence counter moves.
- **The fix** (`crates/custodian/src/reaper.rs`):
  - The `Err` arm of the fence commit classifies the error with `wyrd_traits::classify`
    (`:1238`). `ErrorClass::Indeterminate` goes to the new `fence_unknown` (`:1251`); every
    other error is a hold as before (the store contract says any other commit error means the
    batch did not land, `crates/traits/src/lib.rs:1301-1316`).
  - `fence_unknown` bumps a new tally, `Pass::unknown` (`:444`), and emits the audit action
    `reaper-fence-outcome-unknown` (`:1255`) with the key, the arm and the store's own message.
    The message says only what is known: the upload may already be `Aborting` or may become so,
    it is counted neither as fenced nor as left in place, the fence is not sent again, and the
    next pass reads which.
  - The counter `reaper_fences_unknown` goes out with the pass's other tallies (`emit_pass`,
    `:1379`), so it is emitted on every way out of a pass that walked (last round's F3).
  - A pass with an unknown outcome is `Reconciled::Blocked` (`Pass::outcome`, `:472`), and it
    leaves a lap unclean (`walk`, `:764`), so a later pass cannot report "nothing left" over it.
  - A late profile refusal now names both numbers: "stopped after fencing N session(s), and
    after M fence(s) whose outcome the store could not report" (`emit_profile_refused`, `:1288`,
    `:1301`). A fence of unknown outcome may be a fenced session too, so leaving it out of that
    message would repeat this round's defect one level up.
  - Module doc, step 4, has a paragraph on it (`:78-86`).
- **Tests** (`crates/custodian/src/reaper/tests.rs`):
  - The store double has a new hook, `unknown_at_next_commit` (`:118`, used in `commit` at
    `:238`): it applies the batch or not (`lands`), then returns a real `CommitUnknownResult`
    with `may_still_commit` set as asked. This is the double the sign-off asked for.
  - `a_fence_of_unknown_outcome_is_counted_as_that_and_settled_by_the_next_pass` (`:1379`): both
    arms × landed or not × may-still-commit or not (8 cases). Each: `unknown` is 1 and every
    other tally 0; `Blocked`; the signal names the session, the arm and the store's message; no
    `reaper-session-held`, no "left a session in place", no `fence-commit-failed`; the counters;
    the store holds `Aborting` if it landed and `Open` if not. Then a second pass: a session
    still `Open` is fenced, one already fenced is not judged, and no unknown signal repeats.
  - `a_fence_that_landed_unreported_is_drained_in_the_same_step` (`:1482`): the reviewer's exact
    case, through the real `reconcile_step`. The fence lands unreported; the retirement leg marks
    the committed part's three positions in the same step; the log has one unknown-outcome
    signal and no "left in place"; the step is `Blocked`.
  - `a_late_refusal_counts_the_fences_of_unknown_outcome` (`:1536`): one unknown fence, one
    committed fence, then the ledger is lost before the last read. The refusal names both counts
    and nothing is drained.
  - `a_lap_that_fenced_lost_or_held_is_not_certified_at_its_end` (`:928`) gained an "in doubt"
    case (`:1004`): the lap is left unclean and its last pass answers `Partial`.
  - `a_failed_fence_commit_is_held` (`:1351`) now also asserts a plain failure is **not**
    reported as unknown.
  - **A seeded DST row**, `reaper_unknown_fence_outcome_lands_at_most_once`
    (`crates/dst/tests/custodian.rs:8723`; the property at `:8016`), in the existing file. It
    uses the simulator's own unknown-commit model (`sim_commit_unknown_result`, codes 1021 and
    1031), and each run covers four ways the struck fence can go, for an arm the seed draws:
    (0) it landed before the store answered; (1) it never lands; (2) it was still in flight and
    lands late, before the next pass; (3) it is offered to the store only after the next pass
    has fenced the session again. The unit double cannot show (2) and (3). Asserted: the struck
    step is `Blocked`; the audit seam has exactly one `reaper-fence-outcome-unknown` and no
    `reaper-session-held`; the fence was sent once (in ways 1–3 the session is byte for byte
    what was read); a late batch lands whole in way 2 and is refused (`Conflict`) in way 3; and
    in every way the upload ends torn down, every position of both chunks marked, both
    `retire:` ranges empty, and the admission count back to 0. 300 seeds: green
    (`v7-dst-unknown-300.log`).
- **Red on the iteration-6 behaviour:** negation 47 restores it (every commit error is a hold).
  The three new unit tests and the DST row fail: e.g. `Pass { unknown: 0, held: 1 }` where
  `(1, 0)` is asserted, and
  the log shows `reaper-session-held … hold="fence-commit-failed"` for an upload the same step
  drained. I could not run the new unit tests against iteration 6's file itself: they name
  `Pass::unknown`, which that file does not have, so they would not compile there. Negation 47
  is the same behaviour under the new names.

## Fix 2 — the largest-fence test builds the largest legal session

Test-only; no production line changed for it.

- **What was wrong.** `widest_open_session()` built a record of about 1.5 KB (a 1,024-byte object
  name, a 256-byte content type). Create admits records up to the 100,000-byte value ceiling, so
  the test could not notice a budget too small for a real large fence.
- **The new test** (`reaper/tests.rs`):
  - `largest_admitted_session` (`:1607`) asks the **production** `create_session` where the
    ceiling is. Create measures a session in its widest later state with the real encoder and
    refuses past `MAX_VALUE_BYTES` (`crates/core/src/multipart/create.rs:345-351`). The helper
    asks for a content type far too long, reads the measured size from the refusal, and derives
    the length that measures exactly the limit. It then checks both sides: that length is
    admitted, one byte more is refused with `bytes = limit + 1`.
  - The object name holds a quote, a backslash and a control character, so the JSON escapes are
    in the measurement; the test asserts the stored record carries them escaped.
  - The name is short on purpose. `Completing` carries the name twice and `Completed` once, so a
    long name binds at `Completing` and leaves an `Open` record of about half the ceiling. A
    short name with a long content type binds at `Completed` and leaves the largest `Open`
    record there is: **99,706 bytes** (and 99,710 once `Aborting`).
  - Create writes `Open@0` with no attempt spent. A fence can meet the same session after
    rollbacks, so the record is re-spelled at epoch `u64::MAX - 1` and attempts `u32::MAX` —
    the headroom Create's check already paid for — and must still decode.
  - `largest_fence_fits_both_batch_budgets` (`:1679`) builds the idle fence over that record
    with all `MAX_INFLIGHT_PARTS` (16) slot pins at the widest slot record. The cost is counted
    from the parts, not by `fence_cost`: 20 operations and **202,838 bytes**, asserted equal to
    `fence_cost` and inside `MAX_BATCH_OPS` (500) and `MAX_BATCH_BYTES` (5,000,000). It also
    asserts the bound that holds for any admitted session whatever its shape (two values at the
    ceiling itself).
  - Last, it runs the production pass over that session with all 16 slots reserved and lapsed:
    the idle arm fences it (`fenced_idle == 1`, `held == 0`) and the store holds exactly the
    `Aborting` record. So the largest legal session is fenced, not held for its size.
- **Why the old test was inert, shown:** with the byte budget cut to 200,000 (negation 54), the
  new test is red and **iteration 6's test is green** (I ran iteration 6's `reaper.rs` and
  `tests.rs` with the same cut: 1 ran, 1 passed, `v7-v6-largest-under-200kb.log`).

## Fix 3 — flags the one-shot would have to ignore

- **What was wrong.** `wyrd custodian --gc-expired-pending --reap-abandoned-uploads` ran the
  reaper, ran no GC, and exited 0. The fleet flags were read and dropped the same way.
- **The fix** (`crates/server/src/cli.rs`): a table of the four flags with why each is refused
  (`REAPER_ONE_SHOT_EXCLUDES`, `:1679`), a helper that returns the refusal for the first one
  present (`reaper_one_shot_conflict`, `:1701`), and the check in `cmd_custodian` (`:1022`) right
  after the existing `--reconcile-after-restore` refusal, before the runtime, the election or the
  store. The message names both flags and says the flag "would be silently ignored".
- **Tests:**
  - `a_flag_the_one_shot_would_ignore_refuses_it_before_it_acts`
    (`crates/server/tests/multipart_reaper.rs:1100`): leg A's idle upload is seeded. For each of
    the four flags the binary exits non-zero, the message names both flags, no pass is reported
    and the store is byte-identical. Then the same store, without the flags, is reaped — so the
    refusals were the flags', not the fixture's.
  - `the_reaper_one_shot_refuses_each_flag_it_would_ignore` (`cli.rs:3162`): the helper, flag by
    flag, and three argument lists it accepts.
- **Red on iteration-6 code:** the subprocess leg fails there (the one-shot runs and exits 0).

## Fix 4 — a data dir with no store

- **What was wrong.** `open_local_meta_redb` creates the directory and an empty store. A
  mistyped `--data-dir` therefore produced a new store and "satisfied — nothing to reap or
  drain", exit 0.
- **The fix** (`cli.rs`): the one-shot's redb arm calls `open_existing_local_meta_redb`
  (`:2116`, called at `:1748`). It refuses when `<data-dir>/meta.redb` is not there, names the
  path, and creates nothing. Other commands keep `open_local_meta_redb`. The outcome line now
  ends with `[store: redb at <path>]` (`:1124`, `reaper_store_name` `:1718`), or the backend's
  name for TiKV and FoundationDB. The sign-off offered "refuse, or at minimum name the store". I
  did both: the refusal covers a path with no store, and the name covers a path that holds a
  different store than the operator meant (the default `wyrd-data`, say).
- **Tests:**
  - `a_data_dir_with_no_store_is_refused_and_none_is_created` (`multipart_reaper.rs:1157`): a
    directory that does not exist, and one that exists and is empty. Each: non-zero exit, the
    message names `<dir>/meta.redb`, no pass reported, and afterwards the missing directory is
    still missing and the empty one still empty.
  - `the_reaper_one_shot_opens_only_a_store_that_exists` (`cli.rs:3199`): the helper refuses,
    creates nothing, and opens the store once a writer has made it; the store name is that path.
  - Leg B asserts the outcome line ends with the store's name (`multipart_reaper.rs:668-671`).
- **Red on iteration-6 code:** the data-dir leg and leg B's store-name assertion fail there.
- **Limit:** the refusal is redb's. A networked backend has no local file to check, and its
  address is its own configuration, not a flag of this command; the outcome line names the
  backend only.

## Leg B — the T4 finding the sign-off did not rule on

The T4 report has 8 findings. The carry-forward fixes three, and four more are standing
decisions (next section). The eighth has no ruling:

> `crates/server/tests/multipart_reaper.rs:601` TEST-GAP: The progressing-upload test accepts a
> zero exit and unchanged metadata without checking the reaper outcome, so it also passes if the
> command exits successfully without running the reaper.

It is right, and the fix is test-only, so I made it rather than leave a blocking finding with no
decision. Leg B (`idle_arm_never_reaps_a_progressing_upload`, `multipart_reaper.rs:590`) now
reads the reaper's own tallies from each run's stderr (`tally`, `:430`): `reaper_passes` is 1,
`reaper_sessions_kept` is 4, both fence counters are 0. It also asserts the outcome word is
"satisfied". Negation 60 is the reviewer's case — the one-shot prints "satisfied" and exits 0
without running a pass — and it turns leg B red at the `reaper_passes` assertion.

If you would rather not have a test read the log's counter lines, drop the `tally` loop
(`multipart_reaper.rs:651-662`) and record a decision row for the finding.

## T4 findings with no code change — rows in `review-rejected.md`

Four of the 8 findings are decisions already made at a sign-off. They blocked because the
reviewer re-worded or re-classed them and the existing rows no longer matched. I added rows in
the reviewer's own words, each naming the sign-off it comes from. None of these is my decision.

1. `reconciliation.rs:187` and `:185`, BUG, "the reaper checks its own metadata store and
   authorizes retirement on the reconstruction store". This is decision 4 (iteration-5 sign-off:
   record as rejected, "one store and clock passed to both, custodian.rs:611-639"). It was
   recorded as CONVENTION; the reviewer raised it as BUG this time.
2. `reaper.rs:1181` (now `:1199`), BUG, "the fence does not pin the admission profile … a
   matching empty ledger … a valid profile swap". This is decisions 2 and 3 (a swap at count
   zero cannot get a session fenced; no `mpuctl` precondition on the fence).
3. `reconciliation.rs:199`, BUG, "a stray teardown obligation beside an unchanged `Open`
   session". This is the iteration-3 sign-off's "Out of scope (filed as #659 follow-ups,
   SUMMARY §10): the stray-obligation-beside-Open drain case … do not change #659's drain",
   kept at every sign-off since. **It had no row at all.** I wrote it up as decision 6.

A cheaper-looking alternative for (1) would be a guard in `reconcile_step` that refuses when the
two contexts name different stores (about 8 lines and a test). I did not add it: the sign-off
rejected the finding rather than asking for a fix, and a pointer comparison would refuse callers
that wrap one store two ways, which the DST rows do. If you want it, say so.

I also refreshed the line numbers of every existing row to the iteration-7 tree. Delete any row
you do not want; the file's header says which rows are this round's.

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Twelve plain `#[test]`s (ten before; two new legs this round). Each one-shot leg seeds a redb
store with canonical raw records, drops it, runs the built `wyrd` binary with
`--reap-abandoned-uploads` last in argv, bounded at 120 s, and asserts on the reopened store.
Convergence bound `CONVERGE_RUNS = 6` (`:67`).

- **RED** (the base tree with only this file: I reverse-applied the whole patch in the worktree,
  put the test file back, ran `timeout 2400 cargo test -p wyrd-server --test multipart_reaper`,
  then re-applied the patch and checked the tree's diff checksum was the one from before):
  **it compiled; 12 ran, 12 failed, 0 passed.** The one-shot printed
  ``wyrd: flag `--reap-abandoned-uploads` needs a value`` 19 times. Log:
  `$PDCA_SCRATCH/pdca-builder-1015-logs/v7-red.log`.
  - A, C: "the store did not converge within 6 one-shots" (`:185`).
  - E-open: the foreign `Aborting` session not torn down (`:768`).
  - H (mismatch) and H (lost ledger): the signal assertions (`:837`, `:884`).
  - Held-session leg: the idle session not torn down (`:931`).
  - No-fleet leg: the hold is not named on stderr (`:999`).
  - Budget leg: the first one-shot failed (`:1046`). Usage leg: no flag in the synopsis (`:1073`).
  - Refused-flag leg and data-dir leg: the refusal message is missing (`:1126`, `:1167`).
  - B: its exit-status assertion (`:647`). **B's store assertions are vacuous on the base**
    (nothing reaps there); its teeth are negations 1 and 60.
- **GREEN:** 12 ran, 12 passed on the patched tree, alone (`v7-green-3.log`) and inside the gate
  run.

I ran these with `cargo test` under `timeout` rather than `engine/scripts/run-verify.sh`, because
that script makes its own `../wyrd-verify` worktree outside the roots I was given. Check's
C4-verify row runs it.

### Red on the iteration-6 code (this round's tests)

The sign-off asked for tests that "fail today".

- **The subprocess test file against iteration 6's production code.** I put iteration 6's
  `reaper.rs` and `cli.rs` back under the final test file, ran it, restored both and checked the
  checksum: **it compiled; 12 ran, 3 failed, 9 passed.** The three are the refused-flag leg, the
  data-dir leg, and leg B (at the store-name assertion). Log: `v7-red-on-v6.log`.
- **Fix 1's unit tests:** red under negation 47, which is iteration 6's behaviour (see Fix 1 for
  why not against the old file).
- **Fix 2's test** is a test-gap fix, so there is no iteration-6 failure to show; what I show
  instead is that iteration 6's test stays green under a budget the new one rejects (negation
  54, and `v7-v6-largest-under-200kb.log`).

### Negations

Driver: `$PDCA_SCRATCH/pdca-builder-1015-scripts/neg7.py`. It applies one break, runs the legs
that must catch it under `timeout 2400`, records how many tests ran and failed and whether the
build compiled, and restores the file. It reuses the earlier rounds' breaks; three were
re-anchored because this round changed the text they edit (39, 40, 42). Logs: `neg7-<name>.log`;
summary `neg7-summary.json`.

**Result, on the final tree** (`neg7-final.out`): 58 negations, 93 legs. **Every leg compiled
and ran.** 91 went red. The 2 that stayed green are two extra probes of the new DST row, and I
report them as green on purpose (see the note under the table). Of the 91, 85 run one test each
(1 ran, 1 failed); the six marked "all reaper unit tests" run the 40-test module and the table
gives how many of the 40 failed. The tree's diff checksum was the same after the run as before
(`860578af…`).

One correction along the way: my first break for negation 57 did not compile (it left a function
unused, and the server crate denies warnings). I rewrote the break so it compiles and re-ran it;
the table is the re-run.

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot stamp and lease ignored | B (server); pre-first-chunk (DST) | both **red** |
| 2 | `psum:` read before `slot:` | X60 (DST) | **red** |
| 3 | no slot pins on the idle fence | X62 (DST) | **red** |
| 6 | the drain skips a foreign-clocked session wholesale | E-open `Aborting` half (server) | **red** |
| 10 | idle arm off, ceiling kept | A; held-session leg (server) | both **red** |
| 14 | no profile comparison | H (server) | **red** |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` GC case (at its convergence assertion, see below); deployed-step test | both **red** |
| 22a | no refusal at the last ledger read (both paths) | ledger-replaced test; first-Create-other-profile test | both **red** |
| 23 | fleet branch drains on a refusal | deployed-step test | **red** |
| 25 | fence without its `mpu:` precondition | Complete race (DST) | **red** |
| 26 | lost ledger held instead of refused | lost-ledger step test; no-ledger test; lost-ledger leg (server) | all **red** |
| 27 | reconstruction error returned before the reaper runs | reconstruction-fault test (`gc.rs`) | **red** |
| 28 | empty probe certifies `Satisfied` | first-Create-this-profile test | **red** |
| 29 | cursor bounded only by its prefix | oversized-cursor test | **red** |
| 30 | `Taken` treated as contention | obligation-key-taken test | **red** |
| 31 | matched → absent ledger accepted | ledger-lost-during-the-pass step test | **red** |
| 32a | no read past a spent budget | 64-session unit test; budget leg (server) | both **red** |
| 32b | a resumed lap never certifies | 65-session unit test; budget leg (server) | both **red** |
| 32c | reaching the end certifies whatever the lap held | fenced/lost/held lap test; three-pass lap test | both **red** |
| 32d | a fault on the read past the budget counts as the end | read-past-the-budget test | **red** |
| 33 | a record-only pass counts as a whole fleet | no-fleet step test; no-fleet leg (server) | both **red** |
| 34 | usage synopsis without the flag | usage leg (server) | **red** |
| 35 | the deployed loop passes no reaper | deployed-loop test (`custodian_gc.rs`) | **red** |
| 36 | the position is kept at a listing fault | persistent-fault test; resumed-pass test | both **red** |
| 37a | `budget == 0 \|\| !reached_end` | all reaper unit tests | **red** (3 of 40 fail) |
| 37b | `budget != 0 && !reached_end` | all reaper unit tests | **red** (5 of 40) |
| 37c | `budget == 0 && reached_end` | all reaper unit tests | **red** (2 of 40) |
| 38 | tallies emitted only when the last read admits | ledger-lost test; last-read-fault test; ledger-replaced test | all **red** |
| 39 | a late refusal says "before acting" | ledger-lost test; ledger-replaced test | both **red** |
| 40 | a first-read refusal says "after fencing 0" | refused-pass test; H (server) | both **red** |
| 41 | a pass refused before acting is counted as a pass | no-ledger test; first-Create-other-profile test | both **red** |
| 42 | the late refusal counts only ceiling fences | ledger-lost test | **red** |
| 43a | resume guard `!reached_end \|\| !faulted` | all reaper unit tests | **red** (17 of 40) |
| 43b | resume guard `!reached_end && faulted` | all reaper unit tests | **red** (7 of 40) |
| 43c | resume guard `reached_end && !faulted` | all reaper unit tests | **red** (15 of 40) |
| 44 | an un-refused no-ledger pass emits no tallies | no-ledger test | **red** |
| 45 | a failed read past the budget is not held | read-past-the-budget test | **red** |
| 46 | a failed read past the budget is held but keeps its position | read-past-the-budget test | **red** |
| 47 | **(this round)** an unknown fence outcome is held, as in iteration 6 | unknown-outcome test; landed-unreported step test; late-refusal test; unknown-outcome row (DST) | all **red** |
| 48 | **(this round)** every commit error is treated as unknown | failed-fence-commit test | **red** |
| 49 | **(this round)** an unknown outcome does not make the pass `Blocked` | unknown-outcome test; landed-unreported step test; lap test; unknown-outcome row (DST) | all **red** |
| 50 | **(this round)** an unknown outcome leaves the lap clean | lap test | **red** |
| 51 | **(this round)** the late refusal leaves out the unknown fences | late-refusal test | **red** |
| 52 | **(this round)** `reaper_fences_unknown` is not emitted | unknown-outcome test; landed-unreported step test; late-refusal test | all **red** |
| 53 | **(this round)** the unknown outcome is named but not counted | unknown-outcome test; landed-unreported step test; late-refusal test; lap test; unknown-outcome row (DST) | all **red** |
| 54 | **(this round)** a byte budget of 200,000 | largest-fence test | **red** (iteration 6's version: green) |
| 55 | **(this round)** no flag is refused | refused-flag leg (server); cli unit test | both **red** |
| 56a–d | **(this round)** one of the four flags is not refused (one negation per flag) | refused-flag leg (server); cli unit test | all 8 **red** |
| 57 | **(this round)** the one-shot creates the store, as in iteration 6 | data-dir leg (server) | **red** |
| 58 | **(this round)** a missing store is not refused | data-dir leg (server); cli unit test | both **red** |
| 59 | **(this round)** the outcome line names the wrong path | B (server); cli unit test | both **red** |
| 60 | **(this round)** the one-shot prints "satisfied" without running a pass | B (server) | **red** |
| 62 | **(this round)** the fence carries neither its `mpu:` precondition nor its obligation `require_absent` | unknown-outcome row (DST) | **red** (the late batch lands: `Committed` where `Conflict` is asserted) |
| 61, 25′ | **(this round, probes)** the fence drops **one** of those two preconditions | unknown-outcome row (DST) | both **green** — see below |

Negations (2) and (3) are DST-only, as the brief says; the integration test does not claim them.

**The two green probes.** The new DST row asserts that a fence arriving after another fence of
the same session is refused. I checked which precondition that rests on by removing each alone
(61: the obligation's `require_absent`; 25′: the `mpu:` requirement, negation 25's break run
against this row). The row stays green both times: either precondition alone refuses the late
batch. Only with both removed (62) does it land. So that one assertion is not a test of either
precondition by itself — negation 25 has its own red leg, the Complete race row — and I do not
claim it as one. It shows the two together hold, and that the assertion can fail.

**Negation 15 and the GC case** (unchanged, restated because the brief asks): with no reaper
dispatched, the GC case fails at its convergence assertion ("the session is torn down within
four passes"), which is the first assertion that depends on the reaper. It does not get as far as
the GC assertions. So "finds no mark and deletes nothing at mark + grace" is true of the store,
but it is not the assertion that fires.

### Unit, integration and DST tests

- `crates/custodian/src/reaper/tests.rs`: 40 tests (3 new this round, 5 changed).
- `crates/server/src/cli.rs`: 2 new unit tests.
- `crates/server/tests/multipart_reaper.rs`: 12 tests (2 new, leg B changed).
- `crates/custodian/tests/gc.rs`, `crates/server/tests/custodian_gc.rs`: unchanged this round.
- DST, all in the existing `crates/dst/tests/custodian.rs` (no file added under
  `crates/dst/tests/`): X60, X62, X19, read fault, pre-first-chunk, Complete race, the new
  unknown-outcome row, and the slot-cycle row (ignored, as decided; now at `:8740`).
- **One new DST row this round** (above, under Fix 1). My first plan was none: Fix 1 changes
  what the pass reports, not what it sends to the store. I added it anyway because the repo's
  rubric asks for seeded DST coverage of a destructive path's fault behaviour, the neighbouring
  restore-fence and sweep paths each have such a row, and the late-landing cases needed a store
  that can hold a batch back, which the unit double is not.

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = this worktree), bounded with
`timeout 5400`, on the final tree: **exit 0, "xtask ci: all checks passed"**. The tree's diff
checksum was the same before and after the run, and it is the checksum of `patch.diff`
(`860578af…`), so the gate tested exactly the patch in the bundle. Every stage ran: `typos`,
`lint_docs.py`, `render_site.py --check` (the two external dependencies the brief names), the
gitlink, unsafe and blackbox guards, `cargo fmt --check`, workspace clippy, build and test,
`cargo-machete`, `cargo deny` (including the all-features advisories check), the conformance,
statics and deploy guards, and `cargo clippy` / `cargo test -p wyrd-dst` under `--cfg madsim`.
**2,596 tests passed, 0 failed, 16 ignored** across 234 test binaries (2,588 last round, plus
this round's 8: three reaper unit tests, two cli unit tests, two subprocess legs, one DST row).
Log: `$PDCA_SCRATCH/pdca-builder-1015-logs/ci-v12.log`. (Two earlier runs this round also
passed, on the trees before the last two changes: `ci-v10.log` before a wording change to the
usage text, `ci-v11.log` before the DST row.)

- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6` (temporary index,
  worktree untouched): passes.
- `gitleaks stdin` over `patch.diff`: no leaks.
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-custodian -p wyrd-server -p
  wyrd-core --all-targets -- -D warnings`: clean (also inside the gate run).
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file (`a45a96c5…`).

**Mutation check** (the C5 gate's own command, `cargo mutants --in-diff patch.diff --no-shuffle
--cap-lints=true`, bounded with `timeout 5400`, output kept out of the worktree): **221 mutants
tested in 20 minutes: 197 caught, 0 missed, 0 timed out, 24 unviable.** It ran on the final
tree, against the `patch.diff` in the bundle (`mutants-v7b.log`). An earlier run this round, on
the tree before the DST row was added, gave the same numbers (`mutants-v7.log`). Last round's
gate run had one timeout; these had none. Lists:
`$PDCA_SCRATCH/pdca-builder-1015-mutants-v7b/mutants.out/`.

`patch.diff`: 46 files, 483 KB — production code and `Cargo.lock` about 124 KB, tests about
308 KB, docs about 49 KB. `reaper.rs` is 1,396 lines (69 KB). This round's own change over
iteration 6 is 7 files, +1,072 / −112 lines (`git diff --numstat` between the two trees):
`reaper.rs` +94 / −42, `reaper/tests.rs` +423 / −45, `cli.rs` +179 / −9 (about 70 of the added
lines are its two unit tests), `multipart_reaper.rs` +170 / −10, `dst/tests/custodian.rs`
+204 / −4, and one line in each of two architecture docs. Production code this round: about
200 added lines across `reaper.rs` and `cli.rs`, comments included.

## Refuting my own test

- **(a) Genuine red?** Yes. With the whole fix reverted (base tree, only the new test file kept),
  the subprocess test compiled and 12 of 12 failed, each on a store, signal or exit assertion
  after the one-shot was refused at parse. Against iteration 6's production code, the two new
  legs and leg B fail. Every one of this round's changes has a negation that turns its test red
  (47–60, 62), and the brief's negations (1, 2, 3, 6, 10, 14, 15) were rerun on this code.
- **(b) Production path?** Yes. The subprocess tests run the built `wyrd` binary against a real
  redb store: `cmd_custodian` → `reconcile_pass` → `reconcile_step` → the production reaper and
  drain. The refusals are the binary's own argument handling and store open. The unit tests call
  the production `pass()`, `reconcile_step`, `fence_batch`, `pin_slots`, `fence_cost` and, for
  the largest session, the production `create_session`.
- **(c) Fixture includes the fault?** Yes. Fix 1's store really applies the fence batch and then
  really returns a `CommitUnknownResult` (the old double errored before applying, which is the
  gap the sign-off named); the DST row also really lands the batch late, or offers it after a
  second fence. Fix 2's session is the one Create itself says is the largest. Fix 3's
  store really holds an upload the one-shot would reap. Fix 4's data dir really has no store.

## Production reach

Unchanged in shape; line numbers refreshed.

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` (`crates/custodian/src/reconciliation.rs:140`) → reconstruction → reaper →
  retirement leg (`:168-195`). A healthy fleet therefore reaps every interval. The loop does not
  special-case `Blocked`; only the one-shot maps it to an exit code.
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1011`, in
  `VALUELESS_FLAGS`, `:2987`) → the flag refusal (`:1022`) → `run_reaper_once_over_backend`
  (`:1735`) → `open_existing_local_meta_redb` (`:2116`) → `reap_abandoned_uploads_once`
  (`custodian.rs:463`) → `reconcile_pass` → `reconcile_step` → reaper →
  `retirement::reconcile_record_only` (`reconciliation.rs:201`). Exits non-zero on `Blocked`
  (`cli.rs:1138`): a refusal, a hold, and now a fence of unknown outcome.
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Unchanged. Equal stored profiles don't prove equal operation budgets: `B_ops` is
`KnobSet::batch_ops` (`crates/core/src/multipart.rs:6165`), and `KnobSet::profile` (`:6203`)
leaves it out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:1134`;
   the cost, `fence_cost`, `:1115`). The reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty) and green in
   the gate run.
3. The `B_ops` calibration is 625.3's (#1017).

The largest fence this slice builds is now measured on the largest session Create admits: 20
operations and 202,838 bytes, against 500 and 5,000,000 (`largest_fence_fits_both_batch_budgets`,
`reaper/tests.rs:1679`). This round adds no batch and no new kind of write.

## Profile: who plumbs a non-shipped one

Unchanged: `ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:315`), and the server
builds the policy from the default. The first production writer of `mpuctl` is the S3
`CreateMultipartUpload` verb (#668, child of #508). Whoever makes that profile configurable for
the gateway must hand the custodian the same value through `ReaperPolicy::profile`; until then a
store created under any other profile is refused, loudly. The deferred clean-lap age bound (F2
of iteration 5) becomes reachable with that same change; it stays deferred to #508 (doc at
`reaper.rs:253-266`, marker at `:790`).

## Decisions and what I ruled out

- **Fix 1: an unknown outcome makes the pass `Blocked`, not `Partial`.** The sign-off fixed the
  audit action and the counter but not the pass's answer. `Blocked` means the one-shot exits
  non-zero. I chose it because every other store fault in this pass already answers `Blocked`
  (they are holds), and because `Partial` would let the one-shot print "re-run" and exit 0 right
  after the store failed to report a commit — the shape the T4 review flagged last round for a
  failed listing read. The cost: the one-shot's last line says "BLOCKED … (NEEDS-HUMAN)" for
  something a re-run settles; the audit line above it says a re-run settles it. To flip it,
  change `Pass::outcome` (`reaper.rs:472`) so `unknown > 0` joins the `Partial` arm: 2 lines,
  and negation 49's three tests change their expected value.
- **Fix 1: no re-read to settle the outcome inside the pass.** GC and the client Abort re-read
  after an unknown commit. Here it would be one more `get` of the session, about 25 lines and
  three more cases to test (landed → count as fenced; out of flight and unchanged → did not
  land; moved → lost). It would not remove the unknown class: when the store says the batch may
  still commit, no re-read can settle it (`crates/traits/src/lib.rs:240-247`). And the next pass
  is that re-read, 30 seconds later in the deployed loop. So I kept the smaller change the
  sign-off described.
- **Fix 1: classify by `wyrd_traits::classify`, not a bare `downcast_ref`.** `classify` walks
  the error's source chain, so a backend that wraps its `CommitUnknownResult` is still read as
  unknown. A bare downcast (what `gc.rs` and `abort.rs` use, because they need the struct's
  fields) would send a wrapped one to the hold path. Same line count.
- **Fix 3: the two loop-tuning flags are not refused.** `--interval-secs` and
  `--connect-timeout-secs` are still accepted with the one-shot. The sign-off named four flags,
  all of which select work the one-shot does not do. These two only tune a loop and a dial the
  one-shot does not have, so nothing the operator asked for goes undone; `--reconcile-after-restore`
  treats `--interval-secs` the same way. To refuse them too, add two rows to
  `REAPER_ONE_SHOT_EXCLUDES` (`cli.rs:1679`) and two cases to each of the two tests.
- **Fix 3: a table and one helper, not four `if`s.** The same line count at four flags, one
  place to add a fifth, and the helper is unit-testable without spawning the binary.
- **Fix 4: refuse and name, not one or the other.** Naming alone leaves the false "satisfied"
  in place and relies on the operator reading the path. Refusing alone does nothing for a wrong
  directory that happens to hold a store. Naming costs about 15 lines (`reaper_store_name` and
  the format string).
- **Fix 4: the check is where the store is opened, after the role's start-up banner.** Moving it
  before the banner would need a second backend match in `cmd_custodian` (about 10 lines) for a
  cosmetic gain; the refusal still happens before any store is created or read.
- **Fix 2: probe Create for the ceiling instead of computing it.** `widest_reachable_bytes` is
  crate-private in `wyrd-core`. Making it public to call from a custodian test is an API change
  for one test. Copying its arithmetic into the test would be a second copy to keep in step.
  The refusal already reports the measured size, so the test asks Create.

## Items for the human (NEEDS-HUMAN)

1. **Leg B now asserts the reaper's tallies** (T4 finding `multipart_reaper.rs:601`). The
   carry-forward did not rule on this finding; I fixed it. Confirm, or drop the assertions and
   record a decision row.
2. **Rows added to `review-rejected.md`** for four re-raised findings, and decision 6 written up
   from the iteration-3 sign-off. I transcribed; the decisions are yours. Check the wording and
   delete what you do not want.
3. **`Blocked` for an unknown fence outcome** (so a non-zero exit from the one-shot). My call
   where the sign-off was silent; the 2-line flip is described above.
4. **`--interval-secs` and `--connect-timeout-secs` stay accepted** with the one-shot. My call;
   the 2-row flip is described above.
5. **The outcome line changed shape**: it now ends with `[store: …]`. Anything that parses that
   line needs to know. (Nothing in the repo does; the test reads the word after "pass".)
6. **Negation 15 and the GC case:** red, but at the convergence assertion, not at the GC
   assertions (unchanged).
7. **Two probes of the new DST row stayed green** (negations 61 and 25′): its "a late fence is
   refused" assertion needs both fence preconditions gone before it fails (62 is red). I report
   it rather than claim more for the row than it shows. See the note under the negation table.
8. **Diff coverage** still can't be measured against bare `origin/main`; accepted at the last
   sign-offs. `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70` would measure it against the
   stack base.
9. Standing, unchanged: the slot-cycle follow-up issue is still to be filed (its tracker number
   is yours to add); the two #659 follow-ups in SUMMARY §10 are untouched; fitness of the windows
   (7 d, 30 d) and of the 64-session budget; the psum-read cost per interval.

No external dependency was missing: `typos` and the docs renderer both ran inside the gate.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: `neg7.py` (this round's negation driver; it loads `neg6.py`,
  `neg5.py` and `neg4.py`), and earlier rounds' files;
- `pdca-builder-1015-logs/`: `ci-v12.log` (the gate run on the final tree), `ci-v11.log`,
  `ci-v10.log`, `v7-red.log`, `v7-red-on-v6.log`, `v7-green-3.log`, `v7-unit-1.log`,
  `v7-cli-unit.log`, `v7-v6-largest-under-200kb.log`, `v7-dst-unknown.log`,
  `v7-dst-unknown-300.log`, `mutants-v7b.log`, `mutants-v7.log`, `neg7-*.log`,
  `neg7-final.out` (the run reported here), `neg7-summary.json`, and earlier rounds' logs;
- `pdca-builder-1015-mutants-v7b/` and `-v7/`: this round's two mutation runs;
  `pdca-builder-1015-mutants/`: last round's;
- `pdca-builder-1015-neg/`: `final-v7.diff` (identical to `patch.diff`), saved copies of the
  files I swapped during the red runs, `review-rejected.v6.md` (the decisions file as I found
  it), temporary git index files;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target the DST negation legs
  build into, about 4 GB. It is the one large item.

Left in place for the harness to reclaim, as in earlier rounds: my instructions pull two ways
(one section says to remove what I created, another says the harness owns these roots and no
`rm` is warranted). Everything is inside the per-issue root the harness made, and the logs are
the evidence these notes point at.

The worktree holds the change uncommitted on top of `3c600b6` (three new files are marked
intent-to-add so `git diff 3c600b6` shows them); `patch.diff` is that diff. I made no commit, no
stash, and pushed nothing.
