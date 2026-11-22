# Build notes — issue 1015 (625.1), iteration 4: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 3's patch (it still applies to the same base) and fixed the four items
the iteration-3 sign-off sent back, plus every other open finding in the last T4 batch. Each fix
has a test that fails with the fix taken out (negation table below). One T4 finding is recorded
as rejected, with the reason, in `review-rejected.md`.

| Item | Status |
|---|---|
| F1 — absent `mpuctl` beside a listed session must refuse the step | Fixed. `Reaped::Refused`, so no retirement leg runs. Step-level test, plus a new subprocess leg. |
| F2 — a reconstruction error must not starve the reaper | Fixed. The reaper and drain run, then the step returns the reconstruction error. Regression over both inputs with a failing `repair:` scan. |
| F3 — X60/X62 over reachable production interleavings | Rewritten over the production `upload_part`: X60 holds the real part commit between the reaper's two progress reads (slot lease live); X62 holds the real staging renewal after its live check, crosses expiry, releases it between the reaper's reads and fence, then shows the fence conflicting **and** the part committing. |
| F4 — bound the reaper cursor | Fixed. A stored cursor longer than a backend key (10 KB) is named and reset, never handed to `scan_page`. Tested against a store that refuses oversized scan keys. |
| Keep foreign-clock teardown working with a matching ledger | Unchanged and still green: subprocess leg E-open. |
| Full `cargo xtask ci` must complete | See *Gate* below. |
| T4: obligation key taken beside an unchanged `Open` session reads as contention (reaper.rs:964) | Fixed. The fence's conflict is classified by the client Abort's own classifier; `Taken` is a hold. |
| T4: empty probe + first Create during the pass reports `Satisfied` (reaper.rs:436) | Fixed. Now `Partial`. |
| T4: race fixtures pre-seed sessions without `mpuctl` (tests.rs:913) | Rewritten. They now run the production `create_session` inside the pass. |
| T4: subprocess test reads stderr only after exit (multipart_reaper.rs:165) | Fixed. stderr is drained on a thread while the child runs. |
| T4: Complete race (i) starts Complete after the reaper fenced (dst:7900) | Rewritten. The reaper now fences between the Complete's read and its fence; the Complete's CAS loses. |
| T4: X19's new session never lands on a later page (dst:7438) | Fixed. The listing is read one session a page; the row asserts the new session is judged in the same pass when its id sorts later. |
| T4: profile swap at count 0 (reaper.rs:956, two findings) | **Recorded as rejected**, with reason (not reachable). See *Decisions*. |
| T4: the three F1 duplicates (reaper.rs:425/:436) and F2 (reconciliation.rs:170) | Fixed with F1 and F2. |

The slot reserve → commit → release gap stays as accepted: code and the ignored DST row are
unchanged except for line numbers. `review-rejected.md` now carries those decision lines at the new
line numbers.

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base in `results/issue_1015/stack-base-tip`.
  Every `path:line` below is on the patched tree (base + `patch.diff`).
- **Prerequisites on the base** (`git log --first-parent 3c600b6`): `issue_1008` (5a28b33),
  `issue_1009` (6eaf793), `issue_1010` (0de399f), `issue_1011` (c27cdb0), `issue_1061` (21467e6),
  `issue_1062` (b452551), `issue_1013` (3c600b6), with #1007 and #1027 merged under them.
  **#659's drain is dispatched from `reconcile_step` on the base** (`retirement::reconcile(ctx)` in
  `3c600b6:crates/custodian/src/reconciliation.rs`).
- **#806 has not landed** (its deferral is still in `crates/custodian/src/gc.rs`), so the profile
  comparison stays one function, `check_admission_profile` (`crates/custodian/src/reaper.rs:318`).
- **Prior-art search on this base:** no `crates/custodian/src/reaper.rs` before this patch, and no
  other custodian pass judges an `Open` session.

## "Rebase onto current main" (still NEEDS-HUMAN, harness side)

Nothing changed here since iteration 3. `origin/main` is still `57aefe0`. With a temporary index
(not touching the worktree): the patch applies on `git merge-tree 3c600b6 origin/main`, and does
**not** apply on bare `origin/main`, because #1061, #1062 and #1013 are not merged there (the first
hunk that fails is `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:935`, a test #1013
edits). The C4-diff-cov row resolves its base without the stack base the C4-verify row uses.
Rerun it with `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70`, or land the prerequisites.

## F1 — a lost ledger refuses the step

- `pass` (`crates/custodian/src/reaper.rs:421`): under an absent ledger the pass only probes the
  listing (`probe`, `:508`, which now returns `Probed`, `:497`). After the last ledger read
  (`:438`), the outcome is decided in one exhaustive match (`:447-478`):
  - empty listing, still absent → `Satisfied` (`:449`);
  - listed session, still absent → **refused** (`:460`), signal names the listed key;
  - unreadable listing, still absent → **refused** (`:470`): whether a session exists is unknown;
  - ledger installed during the pass (matching) → `Partial`, or a hold if the listing was
    unreadable (`:453-457`).
- The refusal uses the same signal as a mismatch (`reaper-profile-mismatch`), with a detail that
  says the ledger is absent. An absent ledger beside sessions is "profile unknown", the same as an
  undecodable one, which iteration 3 already refused.
- `Reaped::Refused` makes `reap` (`crates/custodian/src/reconciliation.rs:259`, `:274`) return
  false, so neither branch of `reconcile_step` runs the retirement leg.
- Tests:
  - `a_lost_ledger_refuses_the_step_and_the_drain_writes_nothing`
    (`crates/custodian/src/reaper/tests.rs:1056`): through the real `reconcile_step` (reaper +
    record-only drain), an `Aborting` session with its obligation, a committed `part:`/`psum:` and
    an owned residue, no `mpuctl` → `Blocked`, store byte-identical, signal emitted.
  - `with_no_ledger_an_empty_store_is_satisfied_and_a_session_refuses_the_pass` (`:1140`).
  - New subprocess leg `lost_ledger_signals_and_mutates_nothing`
    (`crates/server/tests/multipart_reaper.rs:766`): the real one-shot, same shape plus an idle
    `Open` session, no ledger → non-zero exit, signal, store byte-identical. This makes F1 part of
    the binding red→green.
- Foreign-clock teardown with a matching ledger: unchanged; leg E-open
  (`multipart_reaper.rs:627`) still tears the foreign `Aborting` session down.

## F2 — a reconstruction fault no longer starves the reaper

- `reconcile_step` (`crates/custodian/src/reconciliation.rs:169-195`): reconstruction's result is
  kept, the reaper and the retirement leg run whatever it answered, and `reconstruction_first`
  (`:225`) returns reconstruction's error first, then theirs. When both fail, the second is logged
  on `wyrd.custodian.reconcile` rather than dropped.
- Why this is safe: the reaper and the drain read neither the repair queue nor anything
  reconstruction writes; "after reconstruction" was about not letting a drain fault cost a pass
  its repairs, which still holds.
- The deployed loop still logs the step's error as a failed reconstruction pass
  (`crates/server/src/custodian.rs`), which it is.
- Test: `a_reconstruction_fault_does_not_starve_the_reaper_or_the_drain`
  (`crates/custodian/tests/gc.rs:1681`), with a store whose `repair:` scan always fails
  (`RepairQueueUnreadable`, `:1641`) and both the reconstruction and reaper inputs
  (`deployed_step_on`, `:1476`): every step returns the error; the first step fences leg A's idle
  session and marks every fragment position of both byte sets; within four more steps the session
  is deleted and `mpuctl.count` is back to 0.

## F3 — X60 and X62 over production schedules

New test-store machinery in `crates/dst/tests/custodian.rs`: `Hold` (`:7177`) picks one
**production** commit to hold — a part commit (the batch putting the part's `psum:`), or a staging
renewal (puts a slot back over bytes it requires, requires nothing absent, puts no session).
`Hooked` (`:7210`) holds the first such commit at a gate, and `fire` (`:7273`) releases it at the
row's trigger and waits for it to land before the trigger's own read or commit goes on.

- **X60** (`:7468`): a session past `W_open` by its creation and one old part. A production
  `upload_part` for part 2 reserves its slot now (live lease), stages a chunk and sends its part
  commit, which is held. The reaper runs; at its second progress read the commit is released and
  lands. Source-first, the reaper saw the live slot, so the session stays `Open`, and the upload
  returns `Committed` with its `part:`/`psum:` records present. No hand-written commit.
- **X62** (`:7531`), the R2 schedule (`crates/core/tests/multipart_upload_part_stage.rs`) with the
  real reaper: a production `upload_part` reserves, stages chunk 1, then waits on a slow body for
  more than `W_open`, staging renewing the slot and owned lease every half-TTL. Then the crossing
  renewal falls due, checks every lease live (asserted, `now < lease`), and is held. The clock
  crosses that lease. The reaper reads slot and owned lease lapsed, judges idle, and builds the
  fence; at the fence commit the renewal is released and lands (`Committed`), and the fence
  conflicts. Session `Open`, outcome `Partial`; the next pass keeps it (renewed lease live); then
  the body's second chunk arrives and the upload returns `Committed` with its part. Seed-drawn
  ceiling variant: the session is past `W_session`, the ceiling fence lands over the renewal, and
  the upload commits nothing. Over 10 seeds both variants ran (2 idle, 8 ceiling; checked with a
  temporary print, removed).
- `stage_request` (`:7425`) and `hand_body` are shared with the pre-first-chunk row, which is
  otherwise unchanged.
- I didn't add the 0016:2275 "renewal crosses expiry and the reaper reads S_old" variant for the
  owned entries separately: the X62 batch carries the owned lease too, and the reaper reads it
  lapsed before the fence (asserted before the pass).

## F4 — the cursor is bounded

- `is_resumable_cursor` (`crates/custodian/src/reaper.rs:618`): `mpu:` prefix **and** length ≤
  `LEDGER_KEY_MAX_LEN` (10,000, FoundationDB's key limit), the constant the retirement walk already
  uses for its resume point (`crates/custodian/src/retirement.rs:291`, now `pub(crate)`). It need
  not parse, as GC's `orphan:` cursor rule says: a listed key that names no upload can end a
  budget and become the cursor.
- `walk` uses it (`:536`); an unusable value is named (`emit_unusable_cursor`, `:1089`, logs its
  length and first 64 bytes, never the whole value) and rewritten by the existing
  "write only when it moved" logic, since the head differs from what is stored.
- Test: `an_oversized_cursor_is_reset_and_never_handed_to_the_store` (`tests.rs:1240`) with a
  store that refuses a `scan_page` `after` key longer than 10,000 bytes: an `mpu:` cursor of 10,001
  bytes is named, the idle session is fenced from the head, and the cursor is reset to empty. A
  cursor of exactly 10,000 bytes is accepted.

## The other T4 findings

- **Obligation key taken (reaper.rs:964).** The fence's `Conflict` is classified by `lost`
  (`crates/core/src/multipart/abort.rs:381`), the client Abort's classifier, now `pub` with its
  `Lost` enum (`:363`) and generalised to `&(impl MetadataStore + ?Sized)` and `read: &[u8]` so
  the reaper's `&dyn MetadataStore` can call it. Behaviour for the Abort is unchanged. In the
  reaper (`reaper.rs:1028-1049`): `Moved`/`Unexplained` stay "lost, judged next pass"; `Taken` is a
  hold `obligation-key-taken`. A read fault while classifying is the caller's hold (`store-fault`).
  Test: `an_obligation_key_taken_beside_an_unchanged_session_is_held` (`tests.rs:1210`). This adds
  the operator signal; it does not stop #659's drain from acting on the stray obligation, which
  stays the #659 follow-up the sign-off filed.
  - The abort.rs module doc said "one batch builder, two writers"; I updated it to three writers
    (`abort.rs:19`), since the reaper is one.
- **`Satisfied` after an empty probe (reaper.rs:436).** Covered by `(Probed::Empty |
  Probed::Listed(_), false) => {}` (`reaper.rs:453`). Test:
  `a_first_create_during_the_pass_under_this_profile_is_judged_next_pass` (`tests.rs:1115`), both
  timings (Create before the listing is read, and after it was read empty).
- **Race fixtures (tests.rs:913).** The unit store can now run the production `create_session`
  next to a read of `mpuctl` (`LedgerRead`, `Interleave`, `create`, `tests.rs:62`, `:70`, `:357`),
  on a fresh store with a bucket marker, so the ledger and the session land in one batch.
  `a_first_create_during_the_pass_under_another_profile_refuses_the_step` (`:1086`) compares the
  store to what the same Create alone leaves on a fresh store. The operator-CAS case
  (`a_ledger_replaced_during_the_pass_refuses_the_step`, `:1183`) keeps a plain batch, since that
  write is an operator's, not a Create's.
- **stderr pipe (multipart_reaper.rs:165).** A reader thread drains stderr while the child runs
  (`multipart_reaper.rs:166`).
- **Complete race (i) (dst:7900).** `ReaperFirst` (`crates/dst/tests/custodian.rs:8093`) runs a
  whole reaper pass when handed the Complete's fence commit, so the Complete has read `Open` and
  validated, the reaper fences, and the Complete's CAS loses; the Complete answers `NoSuchUpload`
  and nothing is published (`prop_complete_race`, `:8158`).
- **X19 pages (dst:7438).** `listing_page = 1` (`:7678`) reads the listing one session a page. The
  row asserts the new session's slot range was read in the first pass exactly when its id sorts
  after the abandoned one (the retirement leg reads no `Open` session's slot range, so that read
  is the reaper's). With one big page this assertion fails (checked, log
  `neg4-x19-one-page.log`).

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Seven plain `#[test]`s (six from iteration 3, plus `lost_ledger_signals_and_mutates_nothing`).
Each seeds a redb store with canonical raw records, drops it, runs the built `wyrd` binary with
`--reap-abandoned-uploads` last in argv, bounded at 120 s, and asserts on the reopened store.
Convergence bound `CONVERGE_RUNS = 6` (`multipart_reaper.rs:58`).

- **RED** (base `3c600b6` production with only this file: `git checkout 3c600b6 -- Cargo.lock
  crates docs` over the snapshot, `timeout 2400 cargo test -p wyrd-server --test
  multipart_reaper`, then restored with `git checkout <snapshot> -- Cargo.lock crates docs`;
  worktree clean afterwards): **7 ran, 7 failed.** Every one-shot printed
  ``wyrd: flag `--reap-abandoned-uploads` needs a value`` (15 times in the log).
  - A, C: "the store did not converge within 6 one-shots" (`:202`).
  - E-open: the foreign `Aborting` session not torn down (`:681`).
  - H (mismatch): the signal assertion (`:750`). H (lost ledger): the signal assertion (`:793`).
  - Held-session leg: the idle session not torn down (`:836`).
  - B: its exit-status assertion (`:581`). **B's store assertions are vacuous on the base**
    (nothing reaps); its teeth are negation 1.
- **GREEN:** 7/7 pass on the patched tree.

I ran these with `cargo test` under `timeout` rather than `engine/scripts/run-verify.sh`, because
that script creates its own `../wyrd-verify` worktree outside the roots I was given. Check's
C4-verify row runs it.

### Negations (each applied alone, run, then restored with `git checkout -- <file>`)

Driver: `$PDCA_SCRATCH/pdca-builder-1015-scripts/neg4.py`; logs:
`$PDCA_SCRATCH/pdca-builder-1015-logs/neg4-*.log`. Each log was checked for a panic in the named
test and for no compile error (cargo also exits 101 on a build failure). Negation 1 failed to
compile on its first run (dead field); I kept the field read and filtered it out, and the rerun is
what the table shows.

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot stamp and lease ignored | B(ii)/(iv) (server); pre-first-chunk (DST) | both **red** |
| 2 | `psum:` read before `slot:` | X60 (DST, production part commit) | **red**, session `Aborting` |
| 3 | no slot pins on the idle fence | X62 (DST, production renewal) | **red**, session `Aborting` |
| 6 | the drain skips a foreign-clocked session wholesale | E-open `Aborting` half (server) | **red** |
| 10 | idle arm off, ceiling kept | A; held-session leg (server) | both **red** |
| 14 | no profile comparison | H (server) | **red** |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` GC reclaim case; deployed-step test | both **red** |
| 22a | no refusal at the last ledger read | ledger-replaced test; first-Create-other-profile test | both **red** |
| 23 | fleet branch drains on a refusal | deployed-step test | **red** |
| 25 | fence without its `mpu:` precondition | Complete race (DST) | **red** |
| 26 | lost ledger held instead of refused (iteration 3) | lost-ledger step test; no-ledger test; lost-ledger leg (server) | all three **red** |
| 27 | reconstruction error returned before the reaper runs (iteration 3) | reconstruction-fault test (`gc.rs`) | **red** |
| 28 | empty probe certifies `Satisfied` (iteration 3) | first-Create-this-profile test | **red**, `Satisfied` |
| 29 | cursor bounded only by its prefix (iteration 3) | oversized-cursor test | **red** |
| 30 | `Taken` treated as contention (iteration 3) | obligation-key-taken test | **red** |

No negation stayed green. Negations (2) and (3) are DST-only, as the brief says; the integration
test does not claim them. The physical half's red is negation 15: the GC case in `gc.rs` finds no
mark and its convergence assertion fails before GC runs. That case has no same-test red against the
base (it does not compile there) and is not reported as a C4 red→green.

### Unit, integration and DST tests

- `crates/custodian/src/reaper/tests.rs`: 26 tests (6 new or rewritten this round).
- `crates/custodian/tests/gc.rs`: 14 tests, including the GC reclaim case, the deployed-step test
  and the new reconstruction-fault test.
- DST, all in the existing `crates/dst/tests/custodian.rs` (no new file under
  `crates/dst/tests/`): X60, X62, X19, read fault, pre-first-chunk, Complete race, slot cycle
  (ignored). `MADSIM_TEST_NUM=10` under `--cfg madsim` with a separate target dir: 6 passed, 1
  ignored. `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`: clean.

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = this worktree) on the final code,
commit `b91296d`: **exit 0, "xtask ci: all checks passed"**, 4 m 47 s wall (builds were warm). Every
stage ran, none skipped: `typos`, the docs lint and `render_site.py --check`, the gitlink, unsafe
and blackbox guards, `cargo fmt --check`, workspace clippy, build and test, `cargo-machete`,
`cargo deny` (including the all-features advisories wall against the pinned RustSec database),
conformance, the statics and deploy guards, `cargo clippy -p wyrd-dst` and `cargo test -p wyrd-dst`
under `--cfg madsim` at 50 seeds. **2,573 tests passed, 0 failed, 16 ignored.** All seven reaper
DST rows ran in it (six passed, the slot-cycle row ignored as decided). Log:
`$PDCA_SCRATCH/pdca-builder-1015-logs/ci-v5.log`.

- Last round's C4-ci timeout stopped in `crates/server/tests/custodian_gc.rs` with nine tests over
  60 s, one of which (`deployed_run_loop_refuses_duplicate_endpoints`) returns before any reaper
  code runs. In this run that suite passed with no test over 60 s. I found nothing in the patch
  that could stall it; the C4-ci row will show whether it recurs on the gate host.
- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6`: passes.
- `gitleaks stdin` over `patch.diff`: no leaks.
- `patch.diff`: 46 files, 379 KB — production code and `Cargo.lock` 96 KB, tests 236 KB, docs
  46 KB. `reaper.rs` is 1,160 lines. This round's own change over iteration 3 is 11 files, +1,122 /
  −282 lines, most of it tests.

- `cargo fmt --all` and `cargo clippy -p wyrd-custodian -p wyrd-core -p wyrd-server --all-targets
  -- -D warnings`: clean.
- No added line over 100 characters.
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file.

## Refuting my own test

- **(a) Genuine red?** Yes. With the fix reverted (base `crates/`, `docs/` and `Cargo.lock`, only
  the new test file kept), 7 ran and 7 failed, each on a store or signal assertion after the
  one-shot was refused at parse. Every fix of this round also has a negation that turns its own
  test red (26–30), and the brief's negations (1, 2, 3, 6, 10, 14, 15) were rerun on this code.
- **(b) Production path?** Yes. The subprocess test runs the built `wyrd` binary against a real
  redb store: `cmd_custodian` → `reconcile_pass` → `reconcile_step` → the production reaper and
  drain. The unit tests call the production `pass()` and `reconcile_step`; the race fixtures run
  the production `create_session`. The DST rows drive production `upload_part` (its own reserve,
  intents, renewals and part commit), `reserve_slot`, `create_session` and `complete_upload`. X60
  and X62 no longer contain a hand-written commit or renewal; the test store only holds a
  production batch at a gate and lets it through.
- **(c) Fixture includes the fault?** Yes. F1's fixtures have no ledger and real drain work
  (obligation, part, residue); F2's store really fails the `repair:` scan on every step; X62's
  renewal is the production one, checked live, held across expiry; F4's store refuses the oversized
  key the way FoundationDB does; the taken-obligation fixture has the obligation key present beside
  the unchanged session.

## Production reach

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`) →
  `reconcile_step` → reconstruction → reaper → retirement leg
  (`crates/custodian/src/reconciliation.rs:169-195`). Since F2, a reconstruction error no longer
  skips the reaper and the drain.
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1005`, in
  `VALUELESS_FLAGS`) → `run_reaper_once_over_backend` (`cli.rs:1657`) →
  `reap_abandoned_uploads_once` (`custodian.rs:463`) → `reconcile_pass` → `reconcile_step` →
  reaper → `retirement::reconcile_record_only`. Exits non-zero on `Blocked` (`cli.rs:1121`),
  which a refusal is.
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Unchanged from iteration 3. Equal stored profiles don't prove equal operation budgets: `B_ops` is
`KnobSet::batch_ops` (`crates/core/src/multipart.rs:6165`), and `KnobSet::profile` (`:6203`)
leaves it out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:951`; the
   cost, `fence_cost`, `:932`). The reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty).
3. The `B_ops` calibration is 625.3's (#1017).

The largest fence this slice builds (16 slot pins at the shipped profile, widest records) is
checked against both budgets and an independent count of its parts in
`largest_fence_fits_both_batch_budgets` (`reaper/tests.rs:935`). The classification reads after a
lost fence add no batch.

## Profile: who plumbs a non-shipped one

Unchanged: `ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:253`), and the server
builds the policy from the default. The first production writer of `mpuctl` is the S3
`CreateMultipartUpload` verb (#668, child of #508). Whoever makes that profile configurable for the
gateway must hand the custodian the same value through `ReaperPolicy::profile`; until then a store
created under any other profile is refused, loudly.

## Decisions and what I ruled out

- **F1: refuse, not hold.** The sign-off asked for this. Cost of the alternative the adversary
  offered ("document why the drain is safe without a profile"): it would contradict the refusal
  rule the module doc and 06 state. An unreadable listing under an absent ledger is refused too,
  for the same reason (profile unknown and sessions may exist); iteration 3 held it.
- **F2: run the legs, then return the error**, rather than a separate reaper pass in the deployed
  loop. A separate pass would mean a second `reconcile_pass` call and a second `ReaperContext`
  wiring in `run_reconstruction_until` (about 30 lines in `crates/server/src/custodian.rs`), and
  a reaper-only step runs the drain record-only, so it would lose the reconstruction context the
  drain needs to retire by chunk id. The in-step change is about 40 lines in
  `reconciliation.rs` (the reordered branch and `reconstruction_first`).
- **The `Taken` classifier is shared, not copied.** Making `abort::lost` public is a visibility
  and signature change in #895's file, with no change to its records or state machine. A copy in
  the reaper would be about 20 lines doing the same three reads.
- **Profile swap at count 0: rejected rather than "fixed".** The alternative was to treat a
  count-0 ledger like an absent one (probe only, no walk). Cost: about 25 lines plus a new damage
  hold for "count 0 but sessions listed", and that hold has a false positive — a session created
  and torn down inside one pass brings the ledger back to byte-identical count-0 bytes. Since no
  fence can reach a session created after a count-0 read (see `review-rejected.md`), I recorded
  the reason instead.
- **Cursor bound** uses the existing 10 KB backend limit (`LEDGER_KEY_MAX_LEN`), not the 36-byte
  length of a well-formed `mpu:` key: a stored, malformed listed key can legitimately be the cursor
  (GC's rule), and resetting on it could pin the listing to the sessions before it.
- **Docs:** `06-runtime-view.md:94` (reconstruction error, lost ledger, first Create during a
  pass, cursor bound, taken obligation), `08-crosscutting-concepts.md:113` (profile unknown), and
  `05-building-block-view.md:246` (cursor bound), each as a sentence appended or a clause added,
  so child-2 and child-3 can still append.

## Items for the human (NEEDS-HUMAN)

1. **Diff coverage** still can't be measured against bare `origin/main` (see *Rebase*). Rerun the
   row with `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70`, or land #1061/#1062/#1013.
2. **`review-rejected.md`**: decision 2 (profile swap at count 0) is mine, not a sign-off's.
   Keep it, or send it back.
3. Unchanged from iteration 3: the slot-cycle follow-up issue is still to be filed; the two #659
   follow-ups in SUMMARY §10 (stray obligation beside `Open`; post-check first-Create/drain race)
   are untouched, as instructed; fitness of the windows (7 d, 30 d) and the 64-session budget.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: `neg4.py` (negation driver), `dst_block_v4.rs`, `patch-v4.diff`,
  temporary index `idx4`;
- `pdca-builder-1015-logs/`: `ci-v5.log` (the gate run), `v4-red.log`, `v4-green.log`,
  `neg4-*.log`;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target, several GB.

Left for the harness to reclaim. The worktree holds the change as temporary commits on top of
`3c600b6` (`0551670` iteration-3 baseline, then `de67aff`, `70614f4`, `c66d6d8`, `b91296d`);
`patch.diff` is `git diff 3c600b6 <final>`.
