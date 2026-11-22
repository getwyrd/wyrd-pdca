# Build notes — issue 1015 (625.1), iteration 5: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 4's patch (`iteration-v4/patch.diff`; it still applies cleanly to the
same base) and fixed the five items the iteration-4 sign-off sent back. Each fix has a test that
fails with the fix taken out (negation table below). I did not rebase, did not touch the
slot-cycle gap, and did not change #659's drain, as instructed.

| Item | Status |
|---|---|
| 1. F1 — ledger lost mid-pass (matched at the first read, absent at the last) | Fixed. Now a store-wide `Reaped::Refused`, so no retirement leg runs. Step-level `reconcile_step` test that deletes `mpuctl` just before the second read. |
| 2. F2 — GC case ran three passes each side of the grace window | Fixed. ONE pass at `mark + grace − 1` (nothing deleted, every mark still there) and ONE at `mark + grace` (every fragment and mark gone, outcome `Changed`). |
| 3. No-fleet hold (`ByChunk::whole`'s `None` arm) had no test | Added a record-only step test and a one-shot leg. Removing the arm turns both red. See the note on the fixture shape under *F3*. |
| 4. Usage text | Added `[--reap-abandoned-uploads]` to the custodian synopsis and one line describing it. A subprocess test reads the usage text. |
| 5. Exact-budget boundary: "rerun until Satisfied" unreachable at 64 and 65 sessions | Fixed in two parts: a one-entry read past a spent budget (64), and a "clean lap" tag on the stored cursor so the pass that ends a lap can certify it (65 and beyond). Regressions at 64, 65 and 129 sessions, unit and one-shot. |

What the reviewer will see that is new in shape: the reaper's cursor record
(`custodian:reaper-cursor`) can now carry a `clean-lap:` prefix. That is a design call I made to
satisfy item 5; the alternatives and their costs are under *Decisions*.

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base in `results/issue_1015/stack-base-tip`.
  Every `path:line` below is on the patched tree (base + `patch.diff`).
- **Prerequisites on the base** (`git log 3c600b6`): `issue_1027` (49463ec, PR #1041 e62a746),
  `issue_1007` (c9e236e, PR #1032 8932adb), `issue_1008` (5a28b33), `issue_1009` (6eaf793),
  `issue_1010` (0de399f), `issue_1011` (c27cdb0), `issue_1061` (21467e6), `issue_1062`
  (b452551), `issue_1013` (3c600b6).
- **#659's drain is dispatched from `reconcile_step` on the base:** `retirement::reconcile(ctx)`
  at `3c600b6:crates/custodian/src/reconciliation.rs:168`.
- **#806 has not landed** (its deferral is still at `3c600b6:crates/custodian/src/gc.rs:1566`), so
  the profile comparison stays one function, `check_admission_profile`
  (`crates/custodian/src/reaper.rs:347`), with the doc comment naming #806 as the caller to add.
- **Prior-art search on this base:** no `crates/custodian/src/reaper.rs` in `3c600b6`
  (`git ls-tree`), and no other custodian pass judges an `Open` session.
- **No rebase**, per the sign-off: the patch stacks on unmerged prerequisites and cannot apply on
  `origin/main` (`57aefe0`). Diff coverage stays unmeasured until they land (accepted).

## What I read

`brief.md`; proposal 0016 decision 6 was read in earlier rounds and nothing in this round's five
items changes how the pass follows it. Because the carry-forward cites line numbers in the
previous attempt, I applied `iteration-v4/patch.diff` as the starting tree and read my own previous
`build-notes.md` for the negation definitions. I also kept `review-rejected.md` current (line
numbers only). I did not read the review or check files.

## F1 — a ledger lost during the pass refuses the step

- `pass` (`crates/custodian/src/reaper.rs:459`). The first read (`:463`) matched, the walk ran,
  and the last read (`:476`) finds `mpuctl` absent. Before, `refusal()` (`:535`) accepted
  `Absent` and the early return let the step go on to the drain. Now (`:487-494`) that case
  emits the profile-refused signal and returns `None`, which `reconcile` (`:448`) turns into
  `Reaped::Refused`, and `reap` (`crates/custodian/src/reconciliation.rs:259`) then keeps both
  branches of `reconcile_step` from running the retirement leg (`:185`, `:199`).
- Why "lost" is the right reading: no writer deletes `mpuctl`. Create puts it
  (`crates/core/src/multipart/create.rs:695`) and the terminal delete puts the decremented value
  (`crates/custodian/src/retirement/sessions.rs:985`). I checked every writer with
  `grep MPUCTL_KEY`.
- The refusal is unconditional on matched → absent, as the sign-off asked. Fences the pass
  already committed stay (they were made under the profile the first read matched); the
  `Reaped::Refused` doc says so (`reaper.rs:368-375`).
- Test: `a_ledger_lost_during_the_pass_refuses_the_step_and_the_drain_writes_nothing`
  (`crates/custodian/src/reaper/tests.rs:1375`), through the real `reconcile_step` (reaper +
  record-only drain). Fixture: a matching ledger; an `Aborting` upload with its `{session, all}`
  obligation, a committed `part:`/`psum:` and an owned `sidx:` residue; and an idle `Open`
  upload. The store deletes `mpuctl` just before the pass's second read
  (`LedgerRead::BeforeSecond`). Asserts: `Blocked`; the store equals exactly "ledger gone + the
  idle upload fenced with its obligation" (so no mark, no deletion, no drain); the signal says
  "absent at its last read"; no `retire-` line in the log.
- Red without the fix (negation 31): the step answers `Changed` — the drain ran.

## F2 — the GC case runs one pass on each side of the grace window

- `reaper_marks_are_reclaimed_by_gc_at_the_grace_window` (`crates/custodian/tests/gc.rs:1344`):
  one `reconcile_step` with the GC input at `T0 + GRACE − 1` (`:1425-1437`), then assert every
  fragment present and every `orphan:` mark still there; one at `T0 + GRACE` (`:1452-1464`), then
  assert the outcome is `Changed`, zero fragments present and every mark gone.
- It passed first time with single passes; the three-pass loops were never needed.
- As the brief says, this case has no same-test red against the base (it does not compile
  there). Its red is negation 15. **Where it goes red:** with no reaper dispatched, the case
  fails at its convergence assertion (`gc.rs:1368`, "the session is torn down within four
  passes"), which is the first assertion that depends on the reaper. It does not get as far as
  the GC assertions. So "finds no mark and deletes nothing at mark + grace" is true of the
  store, but it is not the assertion that fires. I'm stating that rather than claiming the
  brief's wording literally.

## F3 — a record-only pass holds what only a fleet listing could retire

- The code under test is `ByChunk::whole`'s `None` arm
  (`crates/custodian/src/retirement.rs:428-436`), reached from two callers: the session leg's
  owned walk (`crates/custodian/src/retirement/sessions.rs:735`) and the obligation walk's
  by-chunk part (`crates/custodian/src/retirement/marks.rs:555`).
- **The fixture is two uploads, not the one the sign-off sketched.** I first built it as one
  `Aborting` upload holding both the undecodable `sidx:` and the wrong-length `part:`. The log
  showed only the `sidx:` hold: the upload's obligation waits for its owned residue
  (`wait="owned-residue-first"`), so the obligation walk never reaches the part while the
  `sidx:` entry stands. With one upload the `marks.rs:555` caller is never run. Two uploads —
  one with the undecodable owned entry, one with the wrong-length part — run both callers.
- Step test: `a_record_only_step_holds_what_only_a_fleet_listing_could_retire`
  (`reaper/tests.rs:1435`), two passes: `Blocked`, store byte-identical (whole snapshot, so no
  bookkeeping write either), one `retire-session-held` line and one `retire-obligation-held`
  line, each with `hold="partial-fleet"`, the "a record-only pass" detail and the record's key.
- One-shot leg: `what_only_a_fleet_listing_could_retire_is_held_and_fails_the_one_shot`
  (`crates/server/tests/multipart_reaper.rs:904`), same fixture in redb, two one-shots: both
  holds named on stderr, the outcome line says `BLOCKED`, non-zero exit, store byte-identical.
- Red with the arm removed (negation 33): both tests fail. With the arm gone, the pass lists an
  empty fleet, finds nothing, and deletes the records with no mark — the leak the arm prevents.

## F4 — usage text

- `usage()` (`crates/server/src/cli.rs:479`): `[--reap-abandoned-uploads]` added to the
  custodian synopsis (`:484`) and one line under it (`:494`): "runs ONE multipart reaper pass
  and exits (#625): metadata-only, needs no --endpoints; exits non-zero when a session is held
  or the profile check refuses."
- It is one line, as asked, about 190 characters. The neighbouring flag descriptions wrap at
  about 110. If the reviewers prefer the wrapped style, it is a two-line split and a one-line
  change to the test.
- Test: `usage_names_the_one_shot_and_what_it_needs` (`multipart_reaper.rs:1001`) runs `wyrd`
  with no command (bounded, like the one-shot), expects exit 2, and checks the synopsis and the
  line's wording. Red without the flag in the synopsis (negation 34) and red on the base.

## F5 — "rerun until satisfied" is reachable at and past the pass budget

**What was wrong.** `scan_page` returns `next = Some` on a full page whether or not anything
follows it (`crates/testkit/src/lib.rs:857`, "`next` = the last key returned on a full page").
So a listing of exactly 64 sessions spent the budget on a full page, looked unfinished, wrote
the cursor, and answered `Partial`; the next pass read nothing, reset the cursor, and answered
`Partial` again because it had not started at the head. At 65 the same: a pass could only say
`Satisfied` if it alone read the listing head to end, which a listing longer than the budget
never allows. The one-shot's "re-run until it reports satisfied" (`cli.rs:1112`) could not end.

**The fix, two parts** (`walk`, `reaper.rs:612`):

1. **One read past a spent budget** (`:679-685`): when the budget hits zero without the listing
   having ended, the pass reads at most one more entry (`probe`, `:563`, the same helper step 1
   uses, now taking an `after` key) and judges none. Empty means the listing ended at the
   budget. A fault on that read is named (`emit_end_unknown`, `:1212`) and leaves the end
   unknown: `Partial`, no hold (every listed session was judged), and the next pass resumes
   there and holds what it cannot read.
2. **A lap that spans passes can be certified** (`:628-636`, `:688`, `:692-695`, `:714`): the
   stored cursor carries a `clean-lap:` prefix (`CLEAN_LAP_TAG`, `:234`) while every pass since
   the one that started at the head fenced nothing, lost no fence and held nothing. The pass
   that reaches the end may answer `Satisfied` only if the lap before it was clean
   (`pass.whole = reached_end && lap_clean`); its own fences, losses and holds are judged by
   `Pass::outcome` as before (`:433`). A lap in which anything happened ends `Partial`, and the
   next lap starts clean. `read_cursor` (`:586`) and `cursor_value` (`:602`) are the only code
   that knows the prefix; the bound from iteration 4 (`is_resumable_cursor`, `:725`) applies to
   the key after the prefix is stripped.

**What `Satisfied` now claims:** every session listed was judged once in a head-to-end lap and
none was fenced, contended or held. A held session anywhere in the lap keeps every pass of that
lap off `Satisfied`. This is the property iteration 2's item 2 was about (no "nothing to reap"
while something waits for an operator); the simple alternative below would have broken it.

**Tests** (`reaper/tests.rs`, store pages of three entries):
- `a_listing_that_ends_at_the_budget_is_read_whole` (`:819`): 64 kept sessions, three passes,
  each `Satisfied`, store byte-identical (no cursor written).
- `a_clean_lap_past_the_budget_is_satisfied_at_its_end` (`:840`): 65 kept sessions, two laps:
  `Partial` with the cursor tagged after the 64th, then `Satisfied` with the cursor at the head.
- `a_lap_that_fenced_lost_or_held_is_not_certified_at_its_end` (`:876`): 65 sessions, three
  cases. Fenced in the first pass: the lap ends `Partial`, the next lap ends `Satisfied`. Held
  (a foreign-clocked `Open` session): every lap is `Blocked` then `Partial`, never `Satisfied`.
  Fence lost to a slot renewal: the lap ends `Partial`, the next ends `Satisfied`.
- `a_lap_stays_unclean_through_the_passes_after_the_one_that_acted` (`:959`): 129 sessions, a
  lap of three passes. The first fences; the second finds nothing and still leaves the cursor
  untagged; the third ends `Partial`. The next lap is tagged after passes one and two and ends
  `Satisfied`.
- `a_fault_reading_past_the_budget_is_not_read_as_the_end` (`:1004`).
- `a_stored_cursor_reads_as_the_head_a_position_or_neither` (`:1038`): the codec, including the
  bare tag, a tagged non-key and a tagged oversized key (all unusable).
- One-shot: `rerunning_the_one_shot_until_satisfied_ends_at_and_past_the_pass_budget`
  (`multipart_reaper.rs:964`): 64 uploads report `satisfied`, `satisfied`; 65 report `partial`,
  `satisfied`, `partial`, `satisfied`; every run exits 0, no `mpu:` record moves, the count
  stays. The file spells the budget as the number 64 because it may name no symbol the patch
  adds.

**Docs:** `06-runtime-view.md:94` (the extra read, and what the position records),
`08-crosscutting-concepts.md:113` (when a pass reports nothing left; the ledger that disappears
during the pass), `05-building-block-view.md:246` (the cursor's value). Each is a sentence or
clause in place, so child-2 and child-3 can still append.

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Ten plain `#[test]`s (seven from iteration 4, plus the no-fleet leg, the budget leg and the
usage leg). Each one-shot leg seeds a redb store with canonical raw records, drops it, runs the
built `wyrd` binary with `--reap-abandoned-uploads` last in argv, bounded at 120 s
(`run_bounded`, `:192`), and asserts on the reopened store. Convergence bound `CONVERGE_RUNS = 6`
(`:63`).

- **RED** (the base tree with only this file: I reverse-applied the whole patch in the worktree,
  put the test file back, ran `timeout 2400 cargo test -p wyrd-server --test multipart_reaper`,
  then re-applied the patch and checked the tree's diff checksum was unchanged):
  **10 ran, 10 failed, 0 passed.** It compiled. The one-shot printed
  ``wyrd: flag `--reap-abandoned-uploads` needs a value`` 17 times. Log:
  `$PDCA_SCRATCH/pdca-builder-1015-logs/v5-red.log`.
  - A, C: "the store did not converge within 6 one-shots" (`:181`).
  - E-open: the foreign `Aborting` session not torn down (`:712`).
  - H (mismatch) and H (lost ledger): the signal assertions (`:781`, `:824`).
  - Held-session leg: the idle session not torn down (`:867`).
  - No-fleet leg: the hold is not named on stderr (`:935`).
  - Budget leg: the first one-shot failed (`:982`). Usage leg: no flag in the synopsis (`:1009`).
  - B: its exit-status assertion (`:612`). **B's store assertions are vacuous on the base**
    (nothing reaps there); its teeth are negation 1.
- **GREEN:** 10/10 on the patched tree, alone and inside the gate run.

I ran these with `cargo test` under `timeout` rather than `engine/scripts/run-verify.sh`,
because that script makes its own `../wyrd-verify` worktree outside the roots I was given.
Check's C4-verify row runs it.

### Negations (each applied alone, run, then the file restored and its checksum checked)

Driver: `$PDCA_SCRATCH/pdca-builder-1015-scripts/neg5.py` (it reuses iteration 4's edits and adds
this round's); logs: `$PDCA_SCRATCH/pdca-builder-1015-logs/neg5-<name>.log`. The driver records,
per leg, how many tests passed and failed and whether the build compiled. **Every leg: compiled,
1 ran, 1 failed.** All were rerun on this round's code.

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot stamp and lease ignored | B (server); pre-first-chunk (DST) | both **red** |
| 2 | `psum:` read before `slot:` | X60 (DST) | **red** |
| 3 | no slot pins on the idle fence | X62 (DST) | **red** |
| 6 | the drain skips a foreign-clocked session wholesale | E-open `Aborting` half (server) | **red** |
| 10 | idle arm off, ceiling kept | A; held-session leg (server) | both **red** |
| 14 | no profile comparison | H (server) | **red** |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` GC case (at its convergence assertion, see *F2*); deployed-step test | both **red** |
| 22a | no refusal at the last ledger read | ledger-replaced test; first-Create-other-profile test | both **red** |
| 23 | fleet branch drains on a refusal | deployed-step test | **red** |
| 25 | fence without its `mpu:` precondition | Complete race (DST) | **red** |
| 26 | lost ledger held instead of refused | lost-ledger step test; no-ledger test; lost-ledger leg (server) | all **red** |
| 27 | reconstruction error returned before the reaper runs | reconstruction-fault test (`gc.rs`) | **red** |
| 28 | empty probe certifies `Satisfied` | first-Create-this-profile test | **red** |
| 29 | cursor bounded only by its prefix | oversized-cursor test | **red** |
| 30 | `Taken` treated as contention | obligation-key-taken test | **red** |
| 31 | **(this round)** matched → absent ledger accepted | ledger-lost-during-the-pass step test | **red** (`Changed`, the drain ran) |
| 32a | **(this round)** no read past a spent budget | 64-session unit test; budget leg (server) | both **red** |
| 32b | **(this round)** a resumed lap never certifies (iteration 4's rule) | 65-session unit test; budget leg (server) | both **red** |
| 32c | **(this round)** reaching the end certifies whatever the lap held | fenced/lost/held lap test; three-pass lap test | both **red** |
| 32d | **(this round)** a fault on the read past the budget counts as the end | fault-past-the-budget test | **red** |
| 33 | **(this round)** a record-only pass counts as a whole fleet | no-fleet step test; no-fleet leg (server) | both **red** |
| 34 | **(this round)** usage synopsis without the flag | usage leg (server) | **red** |

No negation stayed green. Negations (2) and (3) are DST-only, as the brief says; the
integration test does not claim them. F2 has no negation of its own: it is a test change, and
its red is negation 15.

### Unit, integration and DST tests

- `crates/custodian/src/reaper/tests.rs`: 34 tests (8 new this round).
- `crates/custodian/tests/gc.rs`: 14 tests, including the GC case, the deployed-step test and
  the reconstruction-fault test.
- DST, all in the existing `crates/dst/tests/custodian.rs` (unchanged this round; no file added
  under `crates/dst/tests/`): X60, X62, X19, read fault, pre-first-chunk, Complete race, and the
  slot-cycle row (ignored, as decided).

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = this worktree) on the final tree:
**exit 0, "xtask ci: all checks passed"**, 226 s wall (builds were warm). The tree's diff
checksum was the same before and after the run, and it is the checksum of `patch.diff`
(`6d28ff62…`), so the gate tested exactly the patch in the bundle. Every stage ran, none
skipped: `typos`, `lint_docs.py`, `render_site.py --check` (the two external dependencies the
brief names), the gitlink, unsafe and blackbox guards, `cargo fmt --check`, workspace clippy,
build and test, `cargo-machete`, `cargo deny` (including the all-features advisories check),
the statics and deploy guards, and `cargo clippy` / `cargo test -p wyrd-dst` under
`--cfg madsim`. **2,584 tests passed, 0 failed, 16 ignored** across 234 test binaries. The ten
subprocess tests, the 34 reaper unit tests, the three reaper cases in `gc.rs` and the seven
reaper DST rows (six passed, the slot-cycle row ignored as decided) all ran in it. Log:
`$PDCA_SCRATCH/pdca-builder-1015-logs/ci-v6.log`.

The run took under four minutes, with no test near a timeout; last round's C4-ci worry did not
show up here.

- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6` (temporary index,
  worktree untouched): passes, and the tree it produces is the worktree's tree.
- `gitleaks stdin` over `patch.diff`: no leaks.
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-custodian -p wyrd-server -p
  wyrd-core --all-targets -- -D warnings`: clean. `typos` over every file this round touched:
  clean.
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file.

`patch.diff`: 46 files, 414 KB — production code and `Cargo.lock` 106 KB, tests 261 KB, docs
47 KB. `reaper.rs` is 1,281 lines (61 KB). This round's own change over iteration 4 is 8 files,
+843 / −131 lines (`git diff --numstat` between the two trees): `reaper.rs` +169 / −48,
`reaper/tests.rs` +412 / −4, `multipart_reaper.rs` +210 / −45, `gc.rs` +47 / −30, `cli.rs`
+2 / −1, and one line in each of the three architecture docs.

## Refuting my own test

- **(a) Genuine red?** Yes. With the whole fix reverted (base tree, only the new test file
  kept), 10 ran and 10 failed, each on a store, signal or outcome assertion after the one-shot
  was refused at parse. Each of this round's fixes also has its own negation that turns its own
  test red (31–34), and the brief's negations (1, 2, 3, 6, 10, 14, 15) were rerun on this code.
- **(b) Production path?** Yes. The subprocess tests run the built `wyrd` binary against a real
  redb store: `cmd_custodian` → `reconcile_pass` → `reconcile_step` → the production reaper and
  drain. The usage leg runs the same binary. The unit tests call the production `pass()` and
  `reconcile_step`; the GC case calls the production `reconcile_step` with production GC.
- **(c) Fixture includes the fault?** Yes. F1's store really deletes `mpuctl` between the two
  reads, with real drain work waiting. F3's fixture holds the records that can only be retired by
  chunk id, and the pass really has no fleet. F5's stores hold exactly 64, 65 and 129 sessions,
  and the held/fenced/lost cases put the session that dirties the lap in its first pass. The
  budget leg seeds 64 and 65 real session records in redb.

## Production reach

Unchanged from iteration 4.

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` (`crates/custodian/src/reconciliation.rs:140`) → reconstruction → reaper →
  retirement leg (`:168-195`). A healthy fleet therefore reaps every interval.
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1006`, in
  `VALUELESS_FLAGS`, `:2883`) → `run_reaper_once_over_backend` (`:1658`) →
  `reap_abandoned_uploads_once` (`custodian.rs:463`) → `reconcile_pass` → `reconcile_step` →
  reaper → `retirement::reconcile_record_only` (`reconciliation.rs:201`). Exits non-zero on
  `Blocked` (`cli.rs:1121`), which a refusal and a hold both are.
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Unchanged. Equal stored profiles don't prove equal operation budgets: `B_ops` is
`KnobSet::batch_ops` (`crates/core/src/multipart.rs:6165`), and `KnobSet::profile` (`:6203`)
leaves it out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:1058`;
   the cost, `fence_cost`, `:1039`). The reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty) and green in
   the gate run.
3. The `B_ops` calibration is 625.3's (#1017).

The largest fence this slice builds (16 slot pins at the shipped profile, widest records) is
checked against both budgets and an independent count of its parts in
`largest_fence_fits_both_batch_budgets` (`reaper/tests.rs:1223`). This round adds no batch: the
cursor write is the same single put as before, and the read past the budget is a read.

## Profile: who plumbs a non-shipped one

Unchanged: `ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:282`), and the server
builds the policy from the default. The first production writer of `mpuctl` is the S3
`CreateMultipartUpload` verb (#668, child of #508). Whoever makes that profile configurable for
the gateway must hand the custodian the same value through `ReaperPolicy::profile`; until then a
store created under any other profile is refused, loudly.

## Decisions and what I ruled out

- **F5: a tag on the cursor, rather than the alternatives.**
  - *"Any pass that reaches the end certifies"* (`pass.whole = reached_end;`, a one-line change,
    and the rule GC's `orphan:` walk uses). Rejected on correctness, not size: with 65 sessions
    and a held one among the first 64, the second pass would print "satisfied — nothing to reap
    or drain" and exit 0 while that session waits for an operator. Negation 32c is this variant;
    two tests go red on it.
  - *Only the read past the budget* (the 7-line block at `reaper.rs:679-685`, no tag). Fixes 64
    and nothing longer: negation 32b shows 65 never reaching `satisfied`.
  - *A second record for the lap state* instead of a prefix on the cursor: one more `get` and
    one more `put` per pass on a long listing, against zero extra store operations for the
    prefix. The prefix costs about 45 lines (`CLEAN_LAP_TAG`, `Resume`, `read_cursor`,
    `cursor_value`, and the three changed lines in `walk`).
  - *A bigger `SESSIONS_PER_PASS`:* moves the boundary, does not remove it.
  - The trade I accepted: the cursor is no longer "a position and nothing else". It now also
    decides one report (`Satisfied` vs `Partial` at a lap's end). It still decides no judgment,
    fence or drain, and only the walk writes it. The module doc says this at `reaper.rs:219-233`.
- **F5: a fault on the read past the budget is named, not held.** A hold makes the one-shot exit
  non-zero. Every session the pass listed was judged, so nothing is waiting on an operator yet;
  if the fault persists, the next pass hits it reading for real and holds it then (asserted in
  the test's second pass).
- **F1: unconditional refusal** on matched → absent, even if the listing is empty by then. The
  sign-off asked for "store-wide Refused", and a ledger that vanishes is damage whatever is
  listed. Making it depend on the listing would add a third read path (about 10 lines) for no
  case I can name.
- **F3: two uploads** (see *F3*). The single-upload fixture is 6 lines shorter and leaves one of
  the two callers of the arm unrun.
- **F4: one line** (see *F4*).

## Items for the human (NEEDS-HUMAN)

1. **The cursor record's new prefix** (`clean-lap:`) is a design call made inside this slice to
   close item 5. If you would rather have the simpler "reaching the end certifies" rule and
   accept its false "satisfied", it is the one-line change in negation 32c, plus dropping the
   tag code.
2. **Negation 15 and the GC case:** red, but at the convergence assertion rather than at the GC
   assertions (see *F2*). If you want the red to land on "deletes nothing at mark + grace", the
   convergence loop would have to stop asserting, which weakens the green side. I left it.
3. **F3's fixture is two uploads**, not one, for the reason given.
4. **Diff coverage** still can't be measured against bare `origin/main`; accepted at the last
   sign-off. `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70` would measure it against the
   stack base.
5. **`review-rejected.md`:** line numbers refreshed for this round's tree; no decision added or
   changed. Decision 2 (profile swap at count 0) is still mine, not a sign-off's.
6. Standing, unchanged: the slot-cycle follow-up issue is still to be filed; the two #659
   follow-ups in SUMMARY §10 are untouched; fitness of the windows (7 d, 30 d) and of the
   64-session budget.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: `neg5.py` (this round's negation driver; it imports `neg4.py`),
  and earlier rounds' files;
- `pdca-builder-1015-logs/`: `ci-v6.log` (the gate run), `v5-red.log`, `neg5-*.log`,
  `neg5-run.out`;
- `pdca-builder-1015-neg/`: file backups and checksums, `final.diff`, the interdiff against
  iteration 4, temporary git index files;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target, several GB.

Left in place for the harness to reclaim. The worktree holds the change uncommitted
on top of `3c600b6` (three new files are marked intent-to-add so `git diff 3c600b6` shows them);
`patch.diff` is that diff. I made no commit and pushed nothing.
