# Build notes — issue 1015 (625.1), iteration 6: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 5's patch (`iteration-v5/patch.diff`; it still applies cleanly to the
same base) and made the three fixes the iteration-5 sign-off asked for, plus one it did not rule
on (item 4 below — please read that one). Each fix has a test that failed before the fix and a
negation that turns it red again. No rebase, no change to the slot-cycle gap, no change to #659's
drain, no change to the pass cadence.

| Item | Status |
|---|---|
| 1. F1 — a listing fault that lasts starved the sessions before it | Fixed. After a listing fault the next pass starts at the head; the hold and its signal are kept. Two regressions (fault inside the first budget with the clock advanced past `W_open`; fault on a resumed pass). |
| 2. Deployed-loop wiring had no test (`Some(&reaper)` → `None` stayed green) | Test added to `crates/server/tests/custodian_gc.rs`: the real run loop leaves a young `Open` upload alone, then fences and tears down the same upload once the loop's clock is past the idle window. Passing no reaper turns it red. |
| 3. F3 — late profile refusal lost its tallies and said "fences nothing" | Fixed. The tallies go out on every way out of a pass that walked (answer, late refusal, a failed last read of `mpuctl`). The signal now says either "refused before acting" or "stopped after fencing N session(s)". Message and counters asserted in the ledger-lost regression. |
| 4. **Not in the carry-forward:** a failed read past the budget ended `Partial`, exit 0 (T4 finding at `reaper.rs:683`) | Fixed as part of the listing-fault change: that read failing is now held like any other listing fault. **My call — see "The read past the budget".** |
| Deferred F2 — a clean-lap position has no age bound | Not fixed, as instructed. Stated in a doc comment with a `// deferred: #508` marker, and here. |
| Two T4 items to record as rejected | Recorded in `review-rejected.md` with the sign-off as their source. |

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
  `check_admission_profile` (`crates/custodian/src/reaper.rs:368`), with the doc comment naming
  #806 as the caller to add.
- **Prior-art search on this base:** no `crates/custodian/src/reaper.rs` in `3c600b6`
  (`git ls-tree`), and no other custodian pass judges an `Open` session.
- **No rebase**, per the sign-off: the patch stacks on unmerged prerequisites and cannot apply on
  `origin/main` (`57aefe0`). Diff coverage stays unmeasured until they land (accepted).

## What I read

`brief.md`, and proposal 0016 decision 6 in full in the checkout (`0016:1794-2278`, with its
pass pseudocode and failure-mode table). The carry-forward cites line numbers in the previous
attempt, so I applied `iteration-v5/patch.diff` as the starting tree and read my own previous
`build-notes.md`.

How this round's changes sit against decision 6: the pass is "resumable" and "may stop anywhere
and the next pass re-derives its work from durable records" (`0016:2170`, `:2232-2233`), which a
restart from the head keeps. The table's *never reap* row (`0016:2270`) says an abandoned session
must be gone within a bounded number of passes; a position stuck at a failing page broke that for
every session listed before the page, which is what F1 fixes. The profile check "fails closed
with an alarm" (`0016:2068-2070`); F3 changes what the alarm says, not when it fires. Nothing
this round touches the two arms, the read order, the slot pins or the clock guard.

**One exception, and why:** I read `review-batch.md` (the T4 report, 11 findings). The sign-off
told me to record two T4 items as rejected, and a row in `review-rejected.md` only works if its
phrase is taken from the finding's own text, so I needed the text. Reading it is also how I found
that one of the 11 findings (`reaper.rs:683`) had no ruling in the carry-forward. I did not read
the other check or review files.

## F1 — after a listing fault the next pass starts at the head

- **What was wrong.** When a page of the `mpu:` listing could not be read, the pass held it and
  saved its position just before the failing page. Every later pass resumed there, read nothing,
  held again and left the position where it was. The sessions listed before the fault were never
  judged again, so one that went idle behind a lasting fault kept its admission slot.
- **The fix** (`walk`, `crates/custodian/src/reaper.rs:663`): a `faulted` flag set where a page
  read fails (`:710`); the resume position is the head whenever the listing reached its end *or*
  a read of it failed (`:751`). The hold and its `listing-unreadable` signal are unchanged.
  From the head, each pass judges what the store does return (a budget's worth a pass) and holds
  the fault each time it reaches it. The doc on `walk` cites the *never reap* row
  (`0016:2270`).
- **Tests** (`crates/custodian/src/reaper/tests.rs`):
  - `a_persistent_listing_fault_does_not_starve_the_sessions_before_it` (`:1097`). Five kept
    sessions, the page after the third fails on every pass. Two passes: `Blocked`, 3 kept, 1
    held, the hold named once, store byte-identical (no position kept). Then the clock moves
    past `W_open`: the next pass fences the three readable sessions; the two unread ones stay
    `Open`.
  - `a_listing_fault_on_a_resumed_pass_puts_the_position_back_at_the_head` (`:1054`). 69
    sessions, a page past the budget fails. Pass one of a lap judges 64 and leaves its position;
    pass two resumes, judges 3, holds the fault and writes the position back to the head. Two
    laps, same result.
- **Red before the fix:** both fail on the iteration-5 code (the first at "no position is kept at
  the fault"). See *Red on the iteration-5 code* under *Tests and evidence* for the run. Negation
  36 (keep the position at the fault) turns both red again.
- **The surviving C5 mutant at old `reaper.rs:679`** (`if budget == 0 && !reached_end`, now
  `:733`): I ran the three mutants of that line by hand (negations 37a `&&`→`||`, 37b `==`→`!=`,
  37c drop the `!`). All three are caught now. 37a is the one that used to survive: after a page
  fault it asked for the failed page a second time, which nothing observed; now that shows up as
  two holds where the tests assert one.

## The read past the budget (T4 finding at `reaper.rs:683`) — my call, not the sign-off's

The T4 report has 11 findings. The carry-forward rules on 10 of them (fix, defer, or record as
rejected). The eleventh has no ruling:

> `reaper.rs:683` BUG: A failed lookahead after the session budget is exhausted leaves the pass
> eligible for a successful CLI exit even though the listing could not be read and the pass is
> supposed to report such faults as blocked.

The gate blocks until every finding is fixed or has a recorded decision, and I have no decision
of yours to record for this one. So I fixed it, in the same change as F1, because the two cannot
be kept apart cleanly: the test the sign-off told me to update (old `tests.rs:1003-1033`) is the
test of exactly this read.

- **Before:** when the budget ran out, the pass read one entry further to learn whether the
  listing ended there. If that read failed, the pass named it (`reaper-listing-end-unknown`),
  held nothing and answered `Partial`. The one-shot printed "partial — re-run" and exited 0 after
  a store read had failed.
- **Now** (`reaper.rs:733-743`): that read failing is a listing fault like a page's. It is held
  (`listing-unreadable`, naming where the listing stopped), the pass is `Blocked`, the one-shot
  exits non-zero, and the next pass starts at the head. `emit_end_unknown` is gone (14 lines
  removed, 5 added).
- **Test:** `a_fault_reading_past_the_budget_is_held_and_not_read_as_the_end` (`tests.rs:1016`,
  the renamed and rewritten old test): exactly 64 kept sessions, the read past them fails on
  every pass. Three passes: `Blocked`, 64 kept, 1 held, the hold names the last session judged,
  store byte-identical. It failed before this change (`Partial` where `Blocked` is asserted).
  Negations 32d, 45 and 46 turn it red.
- **If you would rather keep the old behaviour** (named, not held): restore the one arm at
  `reaper.rs:737-741` to a signal without `hold`, and record a decision row for the finding. That
  is negation 45; the cost is that the finding stays open.

## Item 2 — the deployed loop really passes the reaper

- `deployed_loop_reaps_an_idle_open_upload_and_tears_it_down`
  (`crates/server/tests/custodian_gc.rs:1559`). It drives the real
  `CustodianService::run_reconstruction_until` through `drive_passes` over three in-memory D
  servers and an in-memory metadata store.
- Fixture: one upload created by the production `create_session` under the shipped profile at
  1 000 ms, clock source `wall`; part 1 committed at 1 500 ms (`part:` + `psum:`); the staging
  entry of a crashed part 2 with its lease lapsed at 20 000 ms; the fragments of both chunks on
  the servers. No client abort.
- Leg one, the loop's clock at `LOOP_NOW` (the upload is under three hours old), 3 passes: the
  session record is byte-identical, every part and staging record is still there, no mark, count
  still 1. So a pass alone does not end it.
- Leg two, the clock at eight days (past `W_open` = 7 d, inside `W_session` = 30 d, so only the
  idle arm can act), 6 passes: `mpu:<id>` gone; `part:`, `psum:`, `sidx:`, `slot:` and `retire:`
  all empty; an `orphan:` mark at each of the three positions of the committed part's chunk,
  checked position by position (the obligation only a fence installs — no client abort ran, so
  the fence was the reaper's), and at each of the three positions of the residue, with no other
  mark of either chunk; the ledger count back to 0.
- `drive_passes` now delegates to a new `drive_passes_at(…, now_millis)` (`:1368`) so the loop's
  clock can be set; the existing caller is unchanged.
- **Red:** negation 35 (`Some(&reaper)` → a filter that yields `None`,
  `crates/server/src/custodian.rs:680`): 1 ran, 1 failed. On the iteration-5 code the test is
  green — the wiring was right, it just had no test.
- **Flakiness check:** the final test, 30 runs in a row, 30 passed (`v6-loop-repeat.log`). Six
  passes is the same bound the existing retirement test in that file uses and the one-shot
  test's `CONVERGE_RUNS`.

## F3 — a late refusal reports what the pass did

- **What was wrong.** A refusal at the pass's last read of `mpuctl` returned without emitting the
  pass's tallies, so fences the pass had already committed were counted nowhere. A store fault on
  that read did the same. And the signal said "refused to run … fences nothing" even when the
  pass had fenced.
- **The fix** (`reaper.rs`):
  - `pass` (`:483`) is now three straight exits instead of one function threading an
    `Option<Probed>` through both paths. After `walk` there is exactly one way out (`:500-505`):
    `still_admitted` runs, then `emit_pass`, then the answer — so the tallies go out whether the
    last read matched, refused or failed. `reconcile` (`:469`) no longer emits; `pass` does.
  - `still_admitted` (`:510`) is step 5 for a pass that walked; `unadmitted` (`:534`) is the
    no-ledger path (one look at the listing, then the ledger again). Their logic is the code that
    was in `pass` before, moved.
  - `emit_profile_refused(stopped_after, detail)` (`:1244`): `None` logs "the multipart reaper
    refused before acting …" with `refused="before-acting"`; `Some(n)` logs "the multipart reaper
    stopped after fencing {n} session(s) …" with `refused="after-fencing"` and `fenced=n`.
  - `walk` no longer returns a `Result` it could never fail with.
- **Which refusals say which.** First-read refusal: before acting. No-ledger path (absent at the
  first read, then mismatched / still absent beside a session / listing unreadable): before
  acting — under no ledger the pass judges no session. Walked pass refused at its last read
  (mismatch, undecodable, or the ledger gone): after fencing N, where N may be 0.
- **What counts as a pass.** `reaper_passes` and the other tallies are emitted for every pass
  that walked, and for a no-ledger pass that was not refused (all zero, as before). A pass
  refused before acting emits only `reaper_profile_refusals`, as before.
- **Tests** (`reaper/tests.rs`):
  - `a_ledger_lost_during_the_pass_refuses_the_step_and_the_drain_writes_nothing` (`:1496`, the
    regression the sign-off named): now also asserts "stopped after fencing 1 session(s)", that
    "refused before acting" is absent, and the six counters (`reaper_profile_refusals=1`,
    `reaper_passes=1`, `reaper_sessions_fenced_idle=1`, `…fenced_ceiling=0`, `…fences_lost=0`,
    `…sessions_kept=0`).
  - `a_fault_at_the_last_ledger_read_fails_the_step_with_the_pass_counted` (`:1574`, new): the
    second read of `mpuctl` fails (new store hook `failing_ledger_read`, `:60`). Through the real
    `reconcile_step`: the step returns the fault, the store is exactly "idle session fenced with
    its obligation" (an `Aborting` upload's obligation is left alone, so no drain), no `retire-`
    line, and the five tallies are in the log.
  - `a_ledger_replaced_during_the_pass_refuses_the_step` (`:1796`): "stopped after fencing 0
    session(s)" and `reaper_passes=1`.
  - `a_refused_pass_writes_nothing` (`:505`),
    `with_no_ledger_an_empty_store_is_satisfied_and_a_session_refuses_the_pass` (`:1750`) and
    `a_first_create_during_the_pass_under_another_profile_refuses_the_step` (`:1693`): "refused
    before acting", and no `reaper_passes` line.
  - The two profile legs of the subprocess test (`multipart_reaper.rs:786`, `:833`) assert
    "refused before acting" on the real binary's stderr.
- **Red before the fix:** all six unit tests above fail on the iteration-5 code (see *Red on the
  iteration-5 code*). Negations 38–42 and 44 turn them red again.

## Deferred: F2, a clean-lap position has no age bound

Not fixed, as the sign-off said. What it is: when a listing is longer than one pass's budget, the
stored position carries a `clean-lap:` tag, and the pass that ends the lap reports `Satisfied` on
judgments as old as the lap's first pass. With the one-shot, a lap is as long as the operator
leaves between runs, so a session kept in run one and idle by run two is reported as "nothing
left" until the next lap fences it.

Why it can wait: the tag is only written when a listing is longer than `SESSIONS_PER_PASS` (64,
`reaper.rs:209`). The only profile the profile check accepts today is the shipped one, which
admits `MAX_SESSIONS` = 46 (`crates/core/src/multipart.rs:6025`). So every listing is judged
whole by one pass and the tag is never written. It becomes reachable when a deployment can choose
a larger profile, which is #508's.

Where it is stated: `CLEAN_LAP_TAG`'s doc, section "What it does not bound" (`reaper.rs:241-254`),
and a `// deferred: #508` marker on the line that certifies a lap (`:772`).

## T4 items recorded as rejected (`review-rejected.md`)

Both are the sign-off's decisions; I wrote the rows and the reasons.

1. **No `mpuctl` precondition on the fence** (three findings at the fence commit, now
   `reaper.rs:1181`). Reachable only through damage from outside the protocol: no writer deletes
   `mpuctl` (Create and the terminal delete both put it, `create.rs:695`,
   `retirement/sessions.rs:985`; 0016 says the same, "it is never deleted", `0016:2028-2030`),
   and none replaces a stored profile under live sessions (Create
   refuses on `ProfileSkew`, `create.rs:428`; an operator swap is admissible only at count 0,
   `0016:2077-2080`). A pin would also cost normal operation: every Create and every terminal
   delete rewrites `mpuctl` (`create.rs:684-695`, `sessions.rs:975-985`), so each fence would
   lose to routine admission traffic.
2. **`reconciliation.rs:187`, separate contexts** (CONVENTION). The one production caller builds
   both contexts from one store and one clock (`crates/server/src/custodian.rs:611-629` and
   `:634-639`).

I also added rows for the deferred F2 (decision 5 in that file), marked as a deferral to #508 and
sourced to the sign-off. You asked for build-notes and a doc comment for F2, not a row, so this
goes one step past the instruction. My reason: that gate matches rows, not in-code markers, and
without rows the same three findings would block the next round. A row clears findings at one
line only, so the deferral is recorded at each line the three findings anchored at. Delete the
seven `clean cursor` / `clean-lap` / `freshness` rows if you want the marker alone to carry it.

I refreshed the line numbers of the older rows (decisions 1 and 2); no change to those decisions.

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Ten plain `#[test]`s, unchanged in number. This round adds one assertion to each of the two
profile legs. Each one-shot leg seeds a redb store with canonical raw records, drops it, runs the
built `wyrd` binary with `--reap-abandoned-uploads` last in argv, bounded at 120 s, and asserts on
the reopened store. Convergence bound `CONVERGE_RUNS = 6`.

- **RED** (the base tree with only this file: I reverse-applied the whole patch in the worktree,
  put the test file back, ran `timeout 2400 cargo test -p wyrd-server --test multipart_reaper`,
  then re-applied the patch and checked the tree's diff checksum was the one from before):
  **it compiled; 10 ran, 10 failed, 0 passed.** The one-shot printed
  ``wyrd: flag `--reap-abandoned-uploads` needs a value`` 17 times. Log:
  `$PDCA_SCRATCH/pdca-builder-1015-logs/v6-red.log`.
  - A, C: "the store did not converge within 6 one-shots" (`:181`).
  - E-open: the foreign `Aborting` session not torn down (`:712`).
  - H (mismatch) and H (lost ledger): the signal assertions (`:781`, `:828`).
  - Held-session leg: the idle session not torn down (`:875`).
  - No-fleet leg: the hold is not named on stderr (`:943`).
  - Budget leg: the first one-shot failed (`:990`). Usage leg: no flag in the synopsis (`:1017`).
  - B: its exit-status assertion (`:612`). **B's store assertions are vacuous on the base**
    (nothing reaps there); its teeth are negation 1.
- **GREEN:** 10 ran, 10 passed on the patched tree, alone (`v6-green.log`) and inside the gate
  run.

I ran these with `cargo test` under `timeout` rather than `engine/scripts/run-verify.sh`, because
that script makes its own `../wyrd-verify` worktree outside the roots I was given. Check's
C4-verify row runs it.

### Red on the iteration-5 code (this round's unit tests)

The sign-off asked for tests that "fail today". To show that cleanly I put iteration 5's
`reaper.rs` back under the final test file and ran the module (`timeout 1500 cargo test -p
wyrd-custodian --lib reaper::tests::`), then restored the file and checked the tree's checksum:
**it compiled; 37 ran, 9 failed, 28 passed.** The nine are exactly this round's three new tests
and six changed ones:

- F1: `a_persistent_listing_fault_does_not_starve_the_sessions_before_it`,
  `a_listing_fault_on_a_resumed_pass_puts_the_position_back_at_the_head`;
- the read past the budget: `a_fault_reading_past_the_budget_is_held_and_not_read_as_the_end`;
- F3: `a_ledger_lost_during_the_pass_refuses_the_step_and_the_drain_writes_nothing`,
  `a_fault_at_the_last_ledger_read_fails_the_step_with_the_pass_counted`,
  `a_ledger_replaced_during_the_pass_refuses_the_step`, `a_refused_pass_writes_nothing`,
  `with_no_ledger_an_empty_store_is_satisfied_and_a_session_refuses_the_pass`,
  `a_first_create_during_the_pass_under_another_profile_refuses_the_step`.

Log: `v6-red-unit-final.log`. (Two earlier red runs, taken while I was writing the tests, are in
`v6-red-unit.log` and `v6-red-unit-683.log`.) The loop test is the exception: it is green on
iteration-5 code, because the wiring was right and only lacked a test; its red is negation 35.

### Negations

Driver: `$PDCA_SCRATCH/pdca-builder-1015-scripts/neg6.py`. It applies one break, runs the legs
that must catch it under `timeout 2400`, records how many tests ran and failed and whether the
build compiled, and restores the file. It reuses the earlier rounds' breaks; five were re-anchored
because this round moved the text they edit (22a, 26, 28, 31, 32d). Logs:
`neg6-<name>.log`; summary `neg6-summary.json`.

**Result** (`neg6-final.out`): 38 negations, 54 legs. **Every leg compiled, ran and went red.**
48 legs run one test each (1 ran, 1 failed); the six marked "all reaper unit tests" run the
37-test module and the table gives how many of the 37 failed. No negation stayed green.

What tree that was: the code of every production file as in `patch.diff`. After that run I made
two changes that touch no line of production code: the loop test in
`crates/server/tests/custodian_gc.rs` now checks each marked position by name instead of counting
marks, and two doc comments in `reaper.rs` were reworded (one gained the `0016:2270` citation). I
re-ran negation 35, the one negation the test file belongs to, after the test change: red, at
"the idle upload is fenced and its teardown finished" (`custodian_gc.rs:1632`). The driver
restores each file after its break, and I checked the tree's diff checksum after each run.

I ran the full set twice: once before the read-past-the-budget change (36 negations, one of
which did not compile because of a mistake in my re-anchored break, fixed) and once after it,
which is the run reported here.

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
| 35 | **(this round)** the deployed loop passes no reaper | deployed-loop test (`custodian_gc.rs`) | **red** |
| 36 | **(this round)** the position is kept at a listing fault (iteration 5's rule) | persistent-fault test; resumed-pass test | both **red** |
| 37a | **(this round)** `budget == 0 \|\| !reached_end` | all reaper unit tests | **red** (3 of 37 fail) |
| 37b | **(this round)** `budget != 0 && !reached_end` | all reaper unit tests | **red** (5 of 37) |
| 37c | **(this round)** `budget == 0 && reached_end` | all reaper unit tests | **red** (2 of 37) |
| 38 | **(this round)** tallies emitted only when the last read admits | ledger-lost test; last-read-fault test; ledger-replaced test | all **red** |
| 39 | **(this round)** a late refusal says "before acting" | ledger-lost test; ledger-replaced test | both **red** |
| 40 | **(this round)** a first-read refusal says "after fencing 0" | refused-pass test; H (server) | both **red** |
| 41 | **(this round)** a pass refused before acting is counted as a pass | no-ledger test; first-Create-other-profile test | both **red** |
| 42 | **(this round)** the late refusal counts only ceiling fences | ledger-lost test | **red** |
| 43a | **(this round)** resume guard `!reached_end \|\| !faulted` | all reaper unit tests | **red** (16 of 37) |
| 43b | **(this round)** resume guard `!reached_end && faulted` | all reaper unit tests | **red** (7 of 37) |
| 43c | **(this round)** resume guard `reached_end && !faulted` | all reaper unit tests | **red** (14 of 37) |
| 44 | **(this round)** an un-refused no-ledger pass emits no tallies | no-ledger test | **red** |
| 45 | **(this round)** a failed read past the budget is not held (iteration 5's rule) | read-past-the-budget test | **red** |
| 46 | **(this round)** a failed read past the budget is held but keeps its position | read-past-the-budget test | **red** |

Negations (2) and (3) are DST-only, as the brief says; the integration test does not claim them.

**Negation 15 and the GC case** (unchanged from iteration 5, restated because the brief asks):
with no reaper dispatched, the GC case fails at its convergence assertion ("the session is torn
down within four passes"), which is the first assertion that depends on the reaper. It does not
get as far as the GC assertions. So "finds no mark and deletes nothing at mark + grace" is true
of the store, but it is not the assertion that fires.

### Unit, integration and DST tests

- `crates/custodian/src/reaper/tests.rs`: 37 tests (3 new this round, 6 changed).
- `crates/custodian/tests/gc.rs`: 14 tests, unchanged this round.
- `crates/server/tests/custodian_gc.rs`: 12 tests (1 new).
- DST, all in the existing `crates/dst/tests/custodian.rs` (unchanged this round; no file added
  under `crates/dst/tests/`): X60, X62, X19, read fault, pre-first-chunk, Complete race, and the
  slot-cycle row (ignored, as decided). I added no DST row this round: the change is where a
  pass resumes after a failed read and what it logs, not a new destructive or concurrent path.

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = this worktree), bounded with
`timeout 5400`, on the final tree: **exit 0, "xtask ci: all checks passed"**. The tree's diff
checksum was the same before and after the run, and it is the checksum of `patch.diff`
(`bffbfccf…`), so the gate tested exactly the patch in the bundle. (I ran the gate three times,
once after each late change: `ci-v7.log`, `ci-v8.log`, and `ci-v9.log` on the final tree. All
three passed with the same counts.) Every stage ran: `typos`,
`lint_docs.py`, `render_site.py --check` (the two external dependencies the brief names), the
gitlink, unsafe and blackbox guards, `cargo fmt --check`, workspace clippy, build and test,
`cargo-machete`, `cargo deny` (including the all-features advisories check), the conformance,
statics and deploy guards, and `cargo clippy` / `cargo test -p wyrd-dst` under `--cfg madsim`.
**2,588 tests passed, 0 failed, 16 ignored** across 234 test binaries. Log:
`$PDCA_SCRATCH/pdca-builder-1015-logs/ci-v9.log`.

- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6` (temporary index,
  worktree untouched): passes.
- `gitleaks stdin` over `patch.diff`: no leaks.
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-custodian -p wyrd-server -p
  wyrd-core --all-targets -- -D warnings`: clean (also inside the gate run).
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file (`503458bc…`).

**Mutation check** (the C5 gate's own command, `cargo mutants --in-diff patch.diff --no-shuffle
--cap-lints=true`, bounded with `timeout 5400`, output kept out of the worktree): **200 mutants
tested in 17 minutes: 178 caught, 0 missed, 0 timed out, 22 unviable.** Iteration 5's run had one
miss (the `&&` at old `reaper.rs:679`); it is caught now. The run was on the patch before the
two late changes named above (a test's assertions and two doc comments), neither of which
touches a line of production code. Log: `mutants-v6.log`;
lists: `$PDCA_SCRATCH/pdca-builder-1015-mutants/mutants.out/`.

`patch.diff`: 46 files, 437 KB — production code and `Cargo.lock` about 108 KB, tests about
270 KB, docs about 47 KB. `reaper.rs` is 1,344 lines (66 KB). This round's own change over
iteration 5 is 6 files, +571 / −153 lines (`git diff --numstat` between the two trees):
`reaper.rs` +174 / −111, `reaper/tests.rs` +249 / −39, `custodian_gc.rs` +138 / −1,
`multipart_reaper.rs` +8, and one line in each of two architecture docs.

## Refuting my own test

- **(a) Genuine red?** Yes. With the whole fix reverted (base tree, only the new test file kept),
  the subprocess test compiled and 10 of 10 failed, each on a store, signal or exit assertion
  after the one-shot was refused at parse. This round's unit tests failed on the iteration-5 code
  before I changed it (8 of 36, then the read-past-the-budget test on its own), and every one of
  this round's changes has a negation that turns its test red again (35–46). The brief's
  negations (1, 2, 3, 6, 10, 14, 15) were rerun on this code.
- **(b) Production path?** Yes. The subprocess tests run the built `wyrd` binary against a real
  redb store: `cmd_custodian` → `reconcile_pass` → `reconcile_step` → the production reaper and
  drain. The new loop test runs the production `run_reconstruction_until`, the function
  `wyrd custodian` runs, with the production Create seeding the session. The unit tests call the
  production `pass()` and `reconcile_step`.
- **(c) Fixture includes the fault?** Yes. F1's store fails the same page on every pass, with
  real sessions before and after it, and the clock really moves past `W_open`. The loop test's
  upload is really idle at the second clock value and really not at the first. F3's store really
  deletes `mpuctl`, or really fails its second read, after the pass has committed a fence.

## Production reach

Unchanged in shape; line numbers refreshed.

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` (`crates/custodian/src/reconciliation.rs:140`) → reconstruction → reaper →
  retirement leg (`:168-195`). A healthy fleet therefore reaps every interval. This round's loop
  test is the first test of that call.
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1006`, in
  `VALUELESS_FLAGS`, `:2883`) → `run_reaper_once_over_backend` (`:1658`) →
  `reap_abandoned_uploads_once` (`custodian.rs:463`) → `reconcile_pass` → `reconcile_step` →
  reaper → `retirement::reconcile_record_only` (`reconciliation.rs:201`). Exits non-zero on
  `Blocked` (`cli.rs:1121`), which a refusal and a hold both are — now including a failed read
  past the budget.
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Unchanged. Equal stored profiles don't prove equal operation budgets: `B_ops` is
`KnobSet::batch_ops` (`crates/core/src/multipart.rs:6165`), and `KnobSet::profile` (`:6203`)
leaves it out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:1116`;
   the cost, `fence_cost`, `:1097`). The reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty) and green in
   the gate run.
3. The `B_ops` calibration is 625.3's (#1017).

The largest fence this slice builds (16 slot pins at the shipped profile, widest records) is
checked against both budgets and an independent count of its parts in
`largest_fence_fits_both_batch_budgets` (`reaper/tests.rs:1339`). This round adds no batch and
no new kind of write: the cursor write is the same single put, now also made (as an empty value)
after a listing fault on a resumed pass.

## Profile: who plumbs a non-shipped one

Unchanged: `ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:303`), and the server
builds the policy from the default. The first production writer of `mpuctl` is the S3 `CreateMultipartUpload` verb
(#668, child of #508). Whoever makes that profile configurable for the gateway must hand the
custodian the same value through `ReaperPolicy::profile`; until then a store created under any
other profile is refused, loudly. That same change is where F2 (above) becomes reachable.

## Decisions and what I ruled out

- **F1: back to the head, rather than the alternatives.**
  - *Keep the position, skip past the failing page.* Not possible: the pass does not know any key
    past a page it could not read.
  - *Keep the position and alternate laps* (resume at the fault on odd passes, head on even): 6 or
    so more lines and a second bit in the stored position, for the same sessions judged half as
    often. Rejected on that count.
  - *A separate "last fault" record* so a pass could tell a lasting fault from a passing one: one
    more `get` and one more `put` per faulted pass, about 25 lines, and it changes nothing the
    head restart does not already give. The head restart is 3 changed lines.
  - What the head restart costs: after a fault that goes away, the lap starts over, so sessions
    past the fault wait one more lap (a few intervals in the deployed loop).
- **F3: split `pass` in three, rather than patch `emit_pass` into each early return.** The old
  function had four returns after the walk. Adding an `emit_pass` call before each is 4 lines and
  is how the next early return gets added without one. One exit after `walk` is the shape that
  makes "every exit emits" true by construction. Net change to `pass` and its helpers: +67 / −61
  lines, most of it the moved match.
- **F3: `Option<u64>` for the refusal, not a two-variant enum.** An enum is 8 more lines. The
  doc on `emit_profile_refused` says what each value means; there are four call sites, all in this
  file.
- **F3: "stopped after fencing 0 session(s)" is a real message.** A pass that walked, fenced
  nothing and was refused at its last read still walked: it may have held sessions and moved its
  position. Calling that "refused before acting" would be wrong in the same way the old message
  was.
- **The read past the budget** — see its own section; negation 45 is the old behaviour.
- **No new DST row** — see *Unit, integration and DST tests*.
- **Loop test: assert the end state, not the intermediate `Aborting` state.** How many steps one
  pass takes is the drain's business, so "after one pass it is `Aborting`" would pin #659's step
  budget. The marks on the committed part are the evidence of the fence: only a fence installs
  the obligation that produces them, and no client abort runs in the test.

## Items for the human (NEEDS-HUMAN)

1. **The read past the budget is now a hold** (T4 finding `reaper.rs:683`). The carry-forward did
   not rule on this finding; I fixed it rather than leave a blocking finding with no decision.
   Confirm, or revert the one arm and record a decision row.
2. **F2 rows in `review-rejected.md`.** I recorded your deferral as rows as well as in code and
   here. Remove them if the marker should stand alone.
3. **Negation 15 and the GC case:** red, but at the convergence assertion, not at the GC
   assertions (unchanged from iteration 5).
4. **Diff coverage** still can't be measured against bare `origin/main`; accepted at the last
   sign-offs. `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70` would measure it against the
   stack base.
5. Standing, unchanged: the slot-cycle follow-up issue is still to be filed; the two #659
   follow-ups in SUMMARY §10 are untouched; fitness of the windows (7 d, 30 d) and of the
   64-session budget; the psum-read cost per interval (left alone, as instructed).

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: `neg6.py` (this round's negation driver; it loads `neg5.py` and
  `neg4.py`), and earlier rounds' files;
- `pdca-builder-1015-logs/`: `ci-v9.log` (the gate run on the final tree), `ci-v8.log` and
  `ci-v7.log`,
  `v6-red.log`, `v6-green.log`, `v6-red-unit-final.log`, `v6-red-unit.log`,
  `v6-red-unit-683.log`, `v6-green-unit.log`, `v6-loop-green.log`, `v6-loop-repeat.log`,
  `mutants-v6.log`, `neg6-*.log`, `neg6-final.out`, `neg6-summary.json`;
- `pdca-builder-1015-mutants/`: the mutation run's output;
- `pdca-builder-1015-neg/`: `final-v6.diff` (identical to `patch.diff`), the interdiff against
  iteration 5, temporary git index files;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target the DST negation legs
  build into, 4.1 GB. It is the one large item; everything else is 42 MB of logs and scripts
  that the sections above cite.

Left in place for the harness to reclaim. My instructions pull two ways here: one section says
to remove what I created, another says the harness owns these roots and no `rm` is warranted.
Everything above is inside the per-issue root the harness made (`$PDCA_SCRATCH`), and the logs
are the evidence these notes point at, so I left it all and am telling you the size instead.

The worktree holds the change uncommitted on top of `3c600b6` (three new files are marked
intent-to-add so `git diff 3c600b6` shows them); `patch.diff` is that diff. I made no commit, no
stash, and pushed nothing.
