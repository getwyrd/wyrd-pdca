# Build notes — issue 1015 (625.1), iteration 3: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 2's patch (it still applies to the same base) and fixed what the
iteration-2 sign-off sent back. Every fix has a test that goes red when the fix is taken out
(the negation table below).

| Carry-forward item | Status |
|---|---|
| Slot reserve → commit → release race | **Left open, as sign-off decided.** The `#[ignore]` reason now names the follow-up. Docs and module comments state the gap plainly. Decisions file written for the three review findings (see *review-rejected.md*). |
| 1. `latest_psum` must parse the key | Fixed. A `psum:` key that names no part holds the session. |
| 2. Foreign-clocked `Open` sessions must not report "nothing to reap" | Fixed. They are held: the pass is `Blocked`, the one-shot exits non-zero and names them. Leg E-open adjusted. |
| 3. `created_at_millis` ahead of the clock must hold | Fixed, for the progress stamps too. Tolerance is the custodian's existing clock-skew budget, `δ_clock` = 1 s. |
| 4. Session cap must be a resumable budget; churn is not damage | Fixed. 64 sessions a pass, resumed from a stored position. The ledger's `max_sessions` no longer bounds the listing. |
| 5. Absent ledger must not authorize the drain under an unchecked profile | Fixed for every install that lands while the pass runs (profile read again as the pass's last read; no session judged under an absent ledger). One narrower window is left and stated, see item 5. |
| 6. Test the deployed-loop branch (reconstruction + reaper) | New `gc.rs` test, both halves. |
| 7. Pre-first-chunk DST row through the production renewal | Rewritten: a real `upload_part` stalls before its first chunk; staging's own renewal runs; the part commits after the reaper passes. A separate crashed request covers the reap half. |
| Rebase onto current main | Not possible on bare `origin/main` (prerequisites not merged). The patch applies on stack base + current `origin/main`. **NEEDS-HUMAN**, see *Base*. |
| Who plumbs a non-shipped profile | #668 / #508, see *Profile*. |
| T4 test-gap: the Complete race (both orderings) | Added as a new DST row over production Create and Complete. |
| C5 survivors (4) | Each has a killing test now, see *Mutants*. |

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base in `results/issue_1015/stack-base-tip`.
  It is also the remote branch's tip (`git ls-remote origin`). Every `path:line` below is on the
  patched tree (base + `patch.diff`).
- **Prerequisites on the base** (`git log --first-parent 3c600b6`): `issue_1008` (5a28b33),
  `issue_1009` (6eaf793), `issue_1010` (0de399f), `issue_1011` (c27cdb0), `issue_1061` (21467e6),
  `issue_1062` (b452551), `issue_1013` (3c600b6); #1007 and #1027 are merged under it (8932adb,
  e62a746). **#659's drain is dispatched from `reconcile_step` on the base**:
  `git show 3c600b6:crates/custodian/src/reconciliation.rs` line 168, `retirement::reconcile(ctx)`.
- **#806 has not landed.** Its deferral is still at `crates/custodian/src/gc.rs:1566`. So the
  profile comparison stays one function, `check_admission_profile`
  (`crates/custodian/src/reaper.rs:303`), whose doc names #806 as the second caller.
- **Prior-art search on this base:** `crates/custodian/src/reaper.rs` exists only through this
  patch; no other multipart-aware pass in the custodian judges an `Open` session.

## "Rebase onto current main" (NEEDS-HUMAN, harness side)

`origin/main` is `57aefe0`. It holds #1008–#1011 and #1028–#1030 but **not** #1061, #1062 or
#1013. I checked without touching the worktree, using a temporary index:

- `git merge-tree --write-tree 3c600b6 origin/main` merges cleanly, and
  `git apply --cached --check patch.diff` on that tree **succeeds**;
- on bare `origin/main` it fails (`crates/custodian/src/retirement.rs` does not exist there, and
  the hunks over #1013's test edits do not apply).

So I can't make the patch apply on bare `origin/main`: the code it builds on isn't there. Last
round's C4-diff-cov log shows `base_ref: origin/main`, while C4-verify did use the stack base
(it ran 6 tests red). `engine/scripts/run-diff-cov.sh` resolves its base through
`run-verify.sh --print-base` (`$PDCA_BASE > $PDCA_VERIFY_BASE > $WYRD_VERIFY_BASE >
$PDCA_BRIEF_BASE`), so the coverage row seems to run without the stack-base variable C4-verify
gets. That looks like a harness bug (eduralph/pdca-harness), not something this patch can fix.
To measure coverage this round, run the row with
`PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70`. I didn't run it myself: it writes its
own `../wyrd-cov` worktree, outside the roots I was given.

## Item 1 — a `psum:` key must name a part

- `latest_psum` now runs `parse_psum_key` on every key before it reads the value
  (`crates/custodian/src/reaper.rs:790`). A key that does not parse makes the range unjudgeable;
  the session is held as `psum-range-unjudgeable` with the key named, exactly as `read_slots`
  does for a bad `slot:` key (`reaper.rs:708`).
- Test: `damage_holds_the_session_and_the_pass_goes_on` (`crates/custodian/src/reaper/tests.rs:478`)
  now seeds `psum:<id>:not-a-part` holding a summary that decodes and is fresh. Before the fix
  that summary kept the session (`kept: 1`); now it is held (negation 18).

## Item 2 — a foreign-clocked `Open` session is held

- In `visit`, a foreign-clocked session is still counted and signalled
  (`reaper-foreign-clock`) in every state. If it is `Open`, it is also **held** with reason
  `foreign-clock` (`reaper.rs:592`). A foreign session past `Open` is not held: the retirement leg
  tears it down (E-open's `Aborting` half).
- So the pass is `Blocked` and the one-shot exits non-zero, printing its `BLOCKED … NEEDS-HUMAN`
  line (`crates/server/src/cli.rs:1121`). The deployed loop ignores the outcome
  (`crates/server/src/custodian.rs:685`, `Ok(_) => {}`), so a permanent hold changes nothing
  there beyond the signal.
- Leg E-open (`crates/server/tests/multipart_reaper.rs:624`) no longer uses `reap_until` (which
  required exit 0). It runs one-shots until the foreign `Aborting` session is gone, then asserts
  both foreign `Open` sessions survive byte for byte, **and** that every one-shot named both
  (`mpu:<id>` + `foreign-clock`) and exited non-zero.
- Unit: `each_arm_judges_at_its_boundary` (`tests.rs:390`) now expects `held: 1, foreign: 2`,
  outcome `Blocked`, and exactly one hold in the log (the foreign `Open`, not the foreign
  `Aborting`).

## Item 3 — a stamp ahead of the clock is held

- `ahead(stamp, now)` is `stamp > now + δ_clock` (`reaper.rs:675`). `δ_clock` is the custodian's
  existing clock-skew budget, `crate::gc::DELTA_CLOCK_MILLIS` = 1 s (`crates/custodian/src/gc.rs:231`),
  which the doc there calls the one budget "every time-dependent check shares" (ADR-0024). I
  reused it rather than add a reaper-only knob.
- `created_at_millis` is checked first, before the ceiling (`reaper.rs:611`).
- The progress maximum (newest slot `reserved_at`, newest `psum:` `committed_at`) is checked
  too (`reaper.rs:649`), for the same reason: 0016 names "a future-epoch progress stamp defers
  cleanup indefinitely" (`0016:1972`). The ceiling still bounds such a session, since it is
  judged from `created_at` before the ranges are read.
- **The progress check reads the clock again after the ranges** (`reaper.rs:648`). Without that,
  a part that commits while the ranges are being read is stamped later than the reaper's
  earlier reading, and a slow read would turn a normal commit into a spurious hold. The
  judgment itself still uses the earlier reading, which errs toward keeping a session. Both
  reads are the same `ctx.clock` source (ADR-0009).
- Tests: `a_stamp_ahead_of_the_clock_is_held` (`tests.rs:1018`): `created_at` at `T + δ` is judged
  (kept), `T + δ + 1` is held; the same for a `psum:` stamp; a reservation stamp past the budget
  is held. A second half uses a clock that answers `T`, then `T + 10 s`, with a part committed at
  `T + 5 s`: it is kept, not held (negation 20c turns that red).

## Item 4 — a resumable session budget, no cap from the ledger

- The listing reads at most `SESSIONS_PER_PASS` = 64 sessions a pass (`reaper.rs:177`), every
  listed session counted whatever becomes of it. A compile-time assert keeps it above the shipped
  `MAX_SESSIONS`, so a store under the shipped profile is read whole every pass and the cursor is
  never written.
- Where the next pass resumes is stored at `custodian:reaper-cursor` (`reaper.rs:190`), the same
  pattern as the retirement walk's `LEDGER_CURSOR_KEY` (`crates/custodian/src/retirement.rs:209`,
  `walk_ledger`): last key judged when the budget ran out, empty once a pass reaches the end, a
  blind put only when it moved, a value that isn't an `mpu:` key read as the head and named
  (`unusable-reaper-cursor`). A cursor read fault is a hold; a cursor write fault is a hold.
- The old `listing-past-admission` hold is gone. The ledger's `max_sessions` doesn't bound the
  listing: an operator may lower it below the live count (`0016:2082-2084`), and sessions deleted
  and created between two pages list more than the ledger ever held at once.
- A pass certifies `Satisfied` only when it read the whole listing from the head (`Pass::whole`,
  `reaper.rs:354`); otherwise `Partial`.
- The cursor is a new persisted key, so `05-building-block-view.md:246` now names it (docs
  currency), next to the drain's `custodian:owned-walk:` and `custodian:owned-scan:` keys.
- Tests: `a_listing_past_max_sessions_is_judged_whole` (`tests.rs:565`) — 5 sessions under a
  ledger admitting 3, all fenced, none held, no cursor written (negation 21b restores iteration
  2's cut-off and holds the 4th). `the_session_budget_resumes_where_the_last_pass_stopped`
  (`tests.rs:595`) — 66 idle sessions: pass 1 fences 64 and stores the 64th key, pass 2 fences the
  last 2 and resets the cursor, pass 3 from the head is `Partial` (negation 21a: cursor never read).
  `an_unusable_or_spent_cursor_reads_from_the_head` (`:637`), `an_unreadable_cursor_is_held`
  (`:663`).

## Item 5 — no drain under a ledger the pass did not check

What changed (`reaper.rs:403`, `pass`):

- **Absent ledger at the first read → no session is judged.** Nothing was admitted at that read,
  so any session listed afterwards was created after it, under a profile this pass never
  checked. The pass only probes the listing (`probe`, `reaper.rs:456`, one entry).
- **The profile is read again as the pass's last read** (`reaper.rs:420`), right before
  `reconcile_step` runs the retirement leg. A mismatch or an undecodable ledger there refuses the
  step exactly like a mismatch at the first read: `Reaped::Refused`, no retirement leg.
- If the first read was absent, a session was listed, and the last read is **still** absent, the
  session is held as `sessions-without-ledger` (`reaper.rs:430`): every Create writes `mpuctl` in
  the batch that installs its session, so this is a lost ledger, not a race.
- If the last read finds a matching ledger, the session was created during the pass; it is left
  for the next pass (`Partial`).

**What is still open, stated in the module doc (step 5, `reaper.rs:61-70`) and in
`reconcile_step`'s `reap` doc (`crates/custodian/src/reconciliation.rs:225`):** a first Create
that lands *after* the reaper's last read and *before* the retirement leg lists sessions, whose
client then also ends that upload before the leg lists it, gets that one session torn down under
a profile no check read. To close that, the drain's own batches would have to pin `mpuctl`
(#659's batches, out of scope). What the drain does to such a session is its normal teardown in
its own bounded steps, not the unsplittable fence the profile check guards. If you want it closed
anyway, it is a §6 item against #659's layer. Cost sketch: `require(mpuctl == read)` added to the
drain's session-leg commits, roughly 10–20 lines in `retirement/sessions.rs` plus a test, but it
makes every drain commit conflict with every concurrent Create and terminal delete.

Tests (`crates/custodian/src/reaper/tests.rs`, through the real `reconcile_step` where it says so):

- `a_ledger_installed_during_the_pass_under_another_profile_mutates_nothing` (`:903`) — the one
  the sign-off asked for. No `mpuctl` at the first read; the store installs a mismatched one right
  after that read. Through `reconcile_step` (reaper + record-only drain), with an idle session and
  an `Aborting` upload with its obligation present: the step is `Blocked`, and the store equals
  its old self plus the Create's ledger, nothing else. Iteration 2's code fenced the idle session
  and drained the `Aborting` one here (negation 22b reproduces that).
- `a_ledger_replaced_during_the_pass_refuses_the_step` (`:979`) — matching at the first read,
  replaced under another profile during the pass: `Blocked`, `Aborting` upload untouched
  (negation 22a).
- `a_ledger_installed_during_the_pass_under_this_profile_defers_the_judgment` (`:929`) and
  `sessions_with_no_ledger_are_held_and_an_empty_store_is_satisfied` (`:950`).

## Item 6 — the deployed loop's branch

`deployed_step_with_reconstruction_and_reaper_refuses_or_drains_in_one_step`
(`crates/custodian/tests/gc.rs:1525`) calls `reconcile_step` with a `ReconstructionContext` over a
7-server in-memory fleet **and** the reaper, as `run_reconstruction_until` does
(`crates/server/src/custodian.rs:680`):

- **Mismatched profile** (ledger at twice the shipped `W_ref`): one step is `Blocked` and the store
  is byte-identical, including an `Aborting` upload's `retire:bytes:` obligation, its part and its
  owned residue.
- **Matching profile:** one step returns `Changed`, the idle session is `Aborting`, and every
  fragment position of both its byte sets already carries the drain's mark **in that same step**.
  Later steps delete it.
- Negation 23 (fleet branch ignores a refusal) and 23b (fleet branch drops the reaper) each turn
  it red.

I factored leg A's fixture out of the existing `gc.rs` reaper case into `idle_session`
(`gc.rs:1273`) so both tests share it; the existing case is otherwise unchanged.

## Item 7 — the pre-first-chunk DST row, through production staging

`prop_pre_first_chunk_window` (`crates/dst/tests/custodian.rs:7529`):

- **Live half.** A real `wyrd_core::multipart::commit::upload_part` runs against the row's store,
  fleet (`fleet_of`, four in-memory D servers) and topology (`four_domains`). Its body is a
  channel that sends nothing. A hand-fired timer (`HandTimer`, `:7470`) on the row's manual clock
  (the same one the reaper reads) drives staging: the row advances the clock past the half-TTL,
  fires one permit, and waits until **staging's own renewal** has moved the slot lease
  (`renewal_batch`, `crates/core/src/multipart/stage.rs:1620-1631`). Nothing writes the slot but
  staging. A reaper pass (`reconcile_step`, reaper + record-only drain) runs after every renewal
  and the session must stay `Open`. This continues past `W_open` of the reservation. Then the
  body sends one chunk and closes; staging writes the intent and fragments and `commit_part`
  commits. The row asserts `UploadPartOutcome::Committed`, the `part:`/`psum:` records, and the
  slot released — all after every reaper pass.
- **Crashed half.** A separate session whose request reserved its slot through the production
  `reserve_slot` (`stage.rs:1257`) at `t0` and then did nothing. While its reservation is within
  `W_open` it stays `Open` (the fixed stamp is progress); once past `W_open` with the lease long
  lapsed, the reaper fences it.
- Negation 1 (slot ignored) turns it red at "live at …". If staging stopped renewing, the row
  panics in `lease_moved` ("the production staging never moved … slot lease").
- This needed `futures-util` as a dev-dependency of `wyrd-dst` (`crates/dst/Cargo.toml:39`) for the
  body stream; it is already in the workspace and lockfile (one `Cargo.lock` line).

## The Complete race (T4 test-gap finding at dst `custodian.rs:7837`)

New row `prop_complete_race` (`crates/dst/tests/custodian.rs:7878`), registered as
`reaper_complete_race_both_interleavings` (`:8241`), over the production `create_session`,
`complete_upload` and reaper (`0016:2267`):

- **(i) reaper first:** the pass fences the idle session; the Complete that follows answers
  `Refused(NoSuchUpload)` and no dirent is published.
- **(ii) Complete first:** a store wrapper (`CompleteFirst`, `:7810`) runs a whole production
  Complete when it is handed the reaper's fence commit, so the Complete fences and publishes after
  the reaper's reads and before its fence. The fence loses on its `mpu:` precondition; the session
  stays `Completed`; after three more passes the object is published and no fragment position of
  its part carries an `orphan:` mark.
- Negation 25 (fence without its `mpu:` precondition) turns (ii) red.

## C5 survivors from iteration 2

- `read_slots` `+ → -` / `+ → *` on the page limit: the limit is now `RANGE_PAGE`; the expression
  is gone.
- `read_slots` deleted paging arm: `a_live_slot_past_the_first_page_keeps_the_session`
  (`tests.rs:1068`) puts a live slot on the second page (store pages hold 3).
- `fence_cost` `+ → -` on `puts + deletes`: `fence_cost_counts_every_operation_and_byte`
  (`tests.rs:1094`) counts a batch with a delete.

I did not rerun `cargo mutants`; Check's gate does.

## The slot-cycle gap and review-rejected.md

Unchanged in code, as sign-off decided. Changes:

- `#[ignore]` reason (`crates/dst/tests/custodian.rs:8247`) now says it was accepted at #1015's
  sign-off and names the follow-up: durable progress evidence, a 0016 change, needed before
  #668/#508 create sessions in production; tracker issue to be filed.
- Module doc *Known gap* (`reaper.rs:81-101`) and `06-runtime-view.md:94` say the same.
- I wrote `results/issue_1015/review-rejected.md` with one decision (class BUG, phrase
  "reserve") at the pins, the idle branch, the fence commit and the ignored row. Iteration 2
  left this to you; I wrote it this time because the sign-off's "accepted gap" is that decision,
  and without it T4 blocks again on the same three findings. Delete the file if you'd rather
  triage it yourself.

## Operation capacity (the brief's leg-H note)

Equal stored profiles don't prove equal operation budgets: `B_ops` is `KnobSet::batch_ops`
(`crates/core/src/multipart.rs:6165`, set at `:6194`), and `KnobSet::profile` (`:6203`) leaves it
out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:891`, used
   in `fence`, `:900`). `:5908` says #625 must not re-derive it; the reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff --stat 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty) and
   green in the gate.
3. The `B_ops` calibration (`multipart.rs:5945`, "Uncalibrated") is 625.3's (#1017).

The largest fence this slice builds (16 slot pins at the shipped profile, widest records) is
checked against both budgets, and against an independent count of its parts, in
`largest_fence_fits_both_batch_budgets` (`tests.rs:787`).

## Profile: who plumbs a non-shipped one

`ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:233`), and the server builds the
policy from the default (`crates/server/src/custodian.rs:474`, `:590`). No configuration builds
another `Budget` today. The first production writer of `mpuctl` is the S3 `CreateMultipartUpload`
verb (#668, child of #508), which passes its `Budget` to `create_session`. Whoever makes that
profile configurable for the gateway (#668, or #508 as the parent) must hand the custodian the
same value through `ReaperPolicy::profile`; until then a store created under any other profile is
refused, loudly. The field doc says this (`reaper.rs:216`).

## Production reach

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` → reconstruction → `reap()` (`crates/custodian/src/reconciliation.rs:179`,
  `:225`) → `retirement::reconcile`. Policy from `ReaperPolicy::default()` (`custodian.rs:590`);
  tombstone routed through it (`:628`).
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1005`,
  valueless in `VALUELESS_FLAGS`) → `run_reaper_once_over_backend` (`cli.rs:1657`) →
  `reap_abandoned_uploads_once` (`custodian.rs:463`) → `reconcile_pass` → `reconcile_step` →
  `reap()` (`reconciliation.rs:189`) → `retirement::reconcile_record_only`. Exits non-zero on
  `Blocked` (`cli.rs:1121`).
- No test-only entry point.

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Six plain `#[test]`s, each seeding a redb store with canonical raw records, dropping it, running
the built `wyrd` binary with the flag last, bounded at 120 s, and asserting on the reopened store.
Convergence bound `CONVERGE_RUNS = 6` (`multipart_reaper.rs:57`).

- **RED** (base `3c600b6` production with only this file; `git checkout 3c600b6 -- crates docs`
  in the snapshot, then `timeout 2400 cargo test -p wyrd-server --test multipart_reaper`):
  **6 ran, 6 failed.** Every one-shot printed ``wyrd: flag `--reap-abandoned-uploads` needs a
  value`` (14 times across the legs).
  - A and C: "the store did not converge within 6 one-shots" (`:199`).
  - E-open: the foreign `Aborting` session not torn down (`:678`).
  - H: the signal assertion (`:747`).
  - Held-session leg: the idle session not torn down (`:791`).
  - B: the exit-status assertion (`:578`). **B's store assertions are vacuous on the base**
    (nothing reaps); its real teeth are negations 1 and 1b.
- **GREEN:** 6/6 pass on the patched tree.

I ran these with `cargo test` under `timeout`, not through `engine/scripts/run-verify.sh`: that
script writes its own `../wyrd-verify` worktree, outside the roots I was given. Check's C4-verify
row runs it.

### Negations (each applied alone, run, then restored with `git checkout <snapshot> -- <file>`)

Logs are under `$PDCA_SCRATCH/pdca-builder-1015-logs/neg-*.log`.

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot stamp and lease ignored | B (server); pre-first-chunk (DST) | both **red**: "no record of a progressing upload moved"; "live at …" |
| 1b | slot stamp ignored, lease kept | B(iv) | **red** |
| 2 | `psum:` read before `slot:` | X60 (DST) | **red**, session `Aborting` |
| 3 | no slot pins on the idle fence | X62 (DST) | **red**, session `Aborting` |
| 6 | foreign-clocked session skipped wholesale (in the drain, since the reaper never touches non-`Open`) | E-open | **red** at the `Aborting` session's teardown (`:678`) |
| 10 | idle arm off, ceiling kept | A; held-session leg | both **red** |
| 14 | no profile comparison | H (server); four unit tests | **red** |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` GC case; deployed-step test | **red** (the GC case at its convergence assertion, before its GC passes) |
| 16 | a refusal still runs the retirement leg | H | **red**, "the store is untouched" |
| 17 | a read fault ends the step | DST read-fault row; unit damage test | **red** |
| 18 | `psum:` key not parsed | unit damage test | **red**, `kept: 1` |
| 19 | foreign `Open` not held | unit boundary test; E-open (server) | **red** |
| 20a/b/c | `created_at` ahead kept / progress ahead kept / progress judged at the earlier reading | unit stamp test | **red** each |
| 21a | cursor never read | budget-resume and cursor tests | **red** |
| 21b | iteration 2's cut-off at `max_sessions` | listing-past-max test | **red**, `held: 1` |
| 22a | no last-read profile check | ledger installed / replaced tests | **red** |
| 22b | absent ledger walks the listing (iteration 2) | ledger-installed tests; no-ledger test | **red** |
| 23 | fleet branch drains on a refusal | deployed-step test | **red**, "the store is untouched" |
| 23b | fleet branch drops the reaper | deployed-step test | **red** |
| 25 | fence without its `mpu:` precondition | Complete race (DST) | **red** |

No negation stayed green. Four first runs failed to compile (the workspace denies dead code);
I added `#[allow(dead_code)]` to the negated variant and reran them, and the table shows the
reruns.

### Unit and integration tests

- `crates/custodian/src/reaper/tests.rs`: 23 tests (11 new this round).
- `crates/custodian/tests/gc.rs`: the GC reclaim case (unchanged apart from the shared fixture)
  and the new deployed-step test.
- DST, all in the existing `crates/dst/tests/custodian.rs` (no new file under
  `crates/dst/tests/`): X60, X62, X19, read fault, pre-first-chunk (rewritten), Complete race
  (new), slot cycle (ignored). `MADSIM_TEST_NUM=10` under `--cfg madsim` with a separate target
  dir: 6 passed, 1 ignored.

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = the cycle worktree) on the code at
snapshot `eb1a61c`: **exit 0, "xtask ci: all checks passed"**. 2,568 tests passed, 0 failed, 16
ignored, DST at 50 seeds. The new tests all ran in it. After that run I changed only doc comments
(`reaper.rs` step 5 and `Reaped::Refused`, `reconciliation.rs`'s `reap` doc) and then reran
`cargo fmt --check`, `cargo clippy -p wyrd-custodian --all-targets -D warnings`, the 23 unit tests
and `typos` on every touched file: all clean.

- `gitleaks stdin` over the whole patch: no leaks.
- `git apply --check --whitespace=error-all` against `3c600b6`: passes; plain `--check` against
  `3c600b6` merged with `origin/main` (`57aefe0`): passes.
- `patch.diff`: 45 files, 334 KB. Non-test code and docs are about 127 KB (the 172 KB diff of
  `crates/*/src` and `docs/` includes the 44 KB `reaper/tests.rs`); `reaper.rs` alone is about
  1,075 lines. The rest is tests and the mechanical `None` ripple of the widened `reconcile_step`.
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file.

## Refuting my own test

- **(a) Genuine red?** Yes. With the fix reverted (base `crates/` and `docs/`, only the new test
  file kept), 6 ran and 6 failed, as listed above. Every carry-forward fix also has a negation
  that turns its own test red (table).
- **(b) Production path?** Yes. The subprocess test runs the built `wyrd` binary against a real
  redb store: `cmd_custodian` → `reconcile_pass` → `reconcile_step` → the production reaper and
  drain. The unit tests call the production `pass()`/`reconcile()`/`reconcile_step`. The DST rows
  drive production `upload_part`, `reserve_slot`, `create_session` and `complete_upload`; the
  pre-first-chunk row no longer writes slot bytes itself.
- **(c) Fixture includes the fault?** Yes. A seeds both byte sets with leases lapsed and asserts
  marks on every position; H seeds an `Aborting` upload with obligation, part and residue under a
  mismatched profile; item 5's test installs the mismatched ledger during the pass; the
  pre-first-chunk row has a live stalled request **and** a crashed one; the Complete race runs a
  real Complete inside the reaper's read→fence window.

## Decisions and what I ruled out

- **Foreign `Open` as a hold, not a new outcome.** The carry-forward allowed either. A hold reuses
  the existing `Blocked` path (signal, counter, non-zero exit) at the cost of one `if` in `visit`.
  A separate "reported" outcome would need a new `Reconciled`-level value or a second channel to
  the CLI, roughly 30–50 lines across `reaper.rs`, `reconciliation.rs` and `cli.rs`, for the same
  operator-visible result.
- **Reusing `δ_clock` (1 s) as the tolerance.** It is the custodian's stated shared skew budget.
  A reaper-only window (say an hour) would be a new knob with no derivation behind it. The cost of
  1 s is a possible one-pass hold if two clocks disagree past the budget the fleet already
  assumes, which is the misconfiguration the hold should surface. The second clock read after the
  ranges keeps slow store reads from causing it.
- **Persisted cursor rather than a cursor in `ReaperContext`.** An in-memory cursor would reset on
  every one-shot, so repeated one-shots would never get past the first 64 sessions — the exact
  shielding the review found. The store-backed cursor mirrors `walk_ledger`.
- **Closing item 5 fully** needs the drain's batches to pin `mpuctl` (#659's layer), see item 5.
  Having the custodian bootstrap `mpuctl` itself when absent would also close it, but that
  changes 0016's "the first Create initializes it" rule (`0016:2004`, `:2086`), which is out of
  scope.
- **Docs:** `06` and `08` updated as the brief assigns. `05-building-block-view.md:246` updated
  too, because the cursor is a new persisted key and the paragraph said nothing else writes these
  records in production; docs currency makes that a merge requirement.

## Items for the human (NEEDS-HUMAN)

1. **Diff coverage can't be measured against bare `origin/main`** until #1061/#1062/#1013 merge,
   and the coverage row doesn't seem to pick up the stack base. See *Rebase onto current main*.
   Rerun the row with `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70`, or raise it upstream.
2. **The item-5 residual window** (a Create plus a client end of that upload, both between the
   reaper's last read and the drain's listing). Accept as stated, or open a §6 item against #659.
3. **review-rejected.md** — I recorded the slot-cycle gap as decided at sign-off. Keep or delete.
4. **Fitness of the windows** (7 d idle, 30 d ceiling) and of the 64-session budget: still owed
   from iteration 1.
5. **The slot-cycle follow-up issue** is still to be filed (SUMMARY §10).

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: the negation driver and specs, a temporary git index, patch copies;
- `pdca-builder-1015-logs/`: `ci-v4.log` (the passing gate run), `v3-red.log`, `v3-green.log`,
  `neg-*.log`;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target, several GB.

Left for the harness to reclaim. The worktree holds the change as three temporary commits on top
of `3c600b6` (`ee8bf2c` iteration-2 baseline, `eb1a61c`, `e9035cd`); `patch.diff` is
`git diff 3c600b6 e9035cd`.
