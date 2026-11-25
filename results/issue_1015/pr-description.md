## Summary
**User impact:** if a client starts a multipart upload and then goes away without finishing
or cancelling it, that upload stays open for ever. Its uploaded parts keep using storage, and
it keeps one of the limited upload slots. Once enough of these pile up, every new multipart
upload is rejected with `503 SlowDown`, and nothing short of editing the store by hand clears
it.

This PR adds a custodian pass that finds uploads that have been idle for a week, or have
existed for more than 30 days, cancels them, and lets the existing cleanup free their storage.
It also adds an operator command, `wyrd custodian --reap-abandoned-uploads`, that runs one
such pass on demand.

This is the first of three parts of #625 (the multipart reaper). #1016 wires the pass into
the startup shapes this PR does not cover. #1017 adds the `Completing` / `Completed` cases.

**Stacked PR.** This builds on #1066, #1073 and #1082, which are still open (the retirement
drain this pass hands its work to lands there). It will not apply on `main` until they merge.

## What to look at
- **The decision.** An open upload is cancelled when it has made no progress for 7 days and
  nothing is actively writing to it, **or** when it is more than 30 days old, whatever its
  state. "Progress" means its creation time, its last committed part, or its last slot
  reservation. A live upload, or one that has just reserved a slot, is never cancelled by the
  idle rule.
- **When it refuses to act.** If the store's upload-limit profile is not the one this
  custodian was built for, or that record is missing, the pass changes nothing and says why.
  Uploads stamped by a clock the custodian does not own are never judged by age, but uploads
  already being cancelled still get cleaned up.
- **Try it.** On a node's data directory:
  `wyrd custodian --data-dir <dir> --reap-abandoned-uploads`. It needs no `--endpoints`, runs
  one pass, prints what it did and which store it ran on, and exits non-zero if anything was
  held back. Pointing it at a directory with no store, or at an empty `meta.redb`, is refused
  and creates nothing.
- **Known gap, stated, not fixed.** If a request reserves a slot, writes a part and releases
  the slot, all between the pass's reads and its cancel, the pass misses it and cancels a live
  upload. Fixing this needs a change to proposal 0016. It is documented in the module and the
  runtime view, and its DST regression is checked in as `#[ignore]`d. No production path
  creates sessions yet (#668, #508), and the fix must land before one does. The follow-up
  issue has not been filed yet.

## Root cause
Nothing in the custodian ever looked at an `Open` multipart session, and no operator command
ran such a pass, so a session whose client went silent had no way out of `Open`. Its parts
stay protected on purpose (#637), so the storage and the admission slot stay held for good.
Proposal 0016 (decision 6) requires a reaper before release.

## Fix
- **New `crates/custodian/src/reaper.rs`.** One pass over `mpu:` sessions with a resumable
  cursor and a per-pass session budget. For each session, in order: the clock guard, the
  `W_session` ceiling (no slot pins), then the idle arm at `W_open`. The idle arm reads `slot:`
  before `psum:`, keeps the session while any slot or owned `sidx:` lease is unexpired, and
  fences `Open@E → Aborting@E+1` with the free slots pinned. The fence reuses #895's
  `fence_batch` and is checked against `MAX_BATCH_OPS` / `MAX_BATCH_BYTES` before it is
  committed. A read fault, an unparsable key or a stamp ahead of the clock holds that one
  session with a signal. A commit with an unknown outcome is reported as unknown, not as
  "left in place". Pass totals go out on the existing telemetry.
- **`ReaperPolicy` / `ReaperContext`** are public, with stable shapes for #633, #1016 and
  #1017. Defaults: `W_open` = 7 days, `W_session` = 30 days, both bounds checked at compile
  time. The tombstone input for the drain goes through the policy.
- **Profile check.** `check_admission_profile` compares the stored `mpuctl` profile with the
  local `Budget` and runs again as the pass's last read. A mismatch, or a ledger that is absent
  or disappears mid-pass, refuses the whole step, and the dependent retirement leg does not run
  either.
- **`reconcile_step` gains `reaper: Option<&ReaperContext<'_>>`.** With a fleet it runs
  reconstruction → reaper → retirement. A reconstruction error no longer stops the reaper and
  drain from running. With no fleet it runs the reaper → record-only retirement. Every caller is
  updated mechanically, and the deployed loop passes `Some(&reaper)`.
- **Operator one-shot.** `--reap-abandoned-uploads` joins `VALUELESS_FLAGS`. It refuses
  `--reconcile-after-restore`, `--gc-expired-pending`, `--endpoints`, `--ids` and
  `--failure-domains`. It opens the store with the new non-creating
  `RedbMetadataStore::open_existing`, which refuses a missing file, an empty file, or a redb file
  without the metadata table. Its outcome line names the store (redb path, TiKV PD endpoints, or
  FoundationDB cluster file).
- **Docs.** `06-runtime-view.md`, `08-crosscutting-concepts.md` and `05-building-block-view.md`
  describe the reaper loop, the flag and the known gap.

## Verification
Paths and lines are on this branch (the stack base plus this change).

- **Claim:** an idle abandoned upload is cancelled and fully torn down, with every byte it held
  marked for GC.
  - **Checked:** idle arm `crates/custodian/src/reaper.rs:869-912`, fence
    `crates/custodian/src/reaper.rs:1143-1245`.
  - **Test:** `idle_abandoned_session_is_fenced_and_drained_with_every_byte_evidenced`
    (`crates/server/tests/multipart_reaper.rs:526`). The session is 14 days old, so only the idle
    arm can catch it. After at most 6 one-shots, every `mpu:/part:/psum:/sidx:/slot:` range is
    empty, `mpuctl.count` is back to where it started, both `retire:` ranges are empty, and an
    `orphan:` mark exists for every fragment of the committed part and of the crashed residue.
- **Claim:** GC actually frees those fragments, at the grace window and not before.
  - **Test:** `reaper_marks_are_reclaimed_by_gc_at_the_grace_window`
    (`crates/custodian/tests/gc.rs:1344`). One GC pass at `mark + grace − 1` deletes nothing.
    One pass at `mark + grace` deletes every fragment.
- **Claim:** the idle rule never cancels an upload that is making progress.
  - **Checked:** `crates/custodian/src/reaper.rs:884-905`.
  - **Test:** `idle_arm_never_reaps_a_progressing_upload` (`:591`) covers a live `sidx:` lease,
    a reserved slot with a renewed lease and no chunk yet, a recent part commit, and a slot
    reserved 1 hour ago whose lease lapsed. Records and the admission count are unchanged.
- **Claim:** the 30-day ceiling cancels even a live upload.
  - **Checked:** `crates/custodian/src/reaper.rs:864-868`.
  - **Test:** `over_age_session_is_fenced_and_drained_however_live` (`:680`).
- **Claim:** the clock guard skips the age checks only, never the cleanup.
  - **Checked:** `crates/custodian/src/reaper.rs:826-848`.
  - **Test:** `foreign_clocked_open_sessions_survive_and_a_foreign_aborting_one_drains`
    (`:715`).
- **Claim:** a profile mismatch or a missing ledger changes nothing.
  - **Checked:** `crates/custodian/src/reaper.rs:380`, `:595`;
    `crates/custodian/src/reconciliation.rs:140-219`.
  - **Test:** `profile_mismatch_signals_and_mutates_nothing` (`:809`) and
    `lost_ledger_signals_and_mutates_nothing` (`:858`). Both include an already-`Aborting`
    session with a `retire:` obligation, which is left byte-identical.
- **Claim:** the deployed loop reaps.
  - **Checked:** `crates/server/src/custodian.rs:680`.
  - **Test:** `deployed_loop_reaps_an_idle_open_upload_and_tears_it_down`
    (`crates/server/tests/custodian_gc.rs:1559`) and
    `deployed_step_with_reconstruction_and_reaper_refuses_or_drains_in_one_step`
    (`crates/custodian/tests/gc.rs:1550`).
- **Claim:** the one-shot reports truthfully and refuses bad input.
  - **Checked:** `crates/server/src/cli.rs:1011-1029`, `:1671-1765`;
    `crates/metadata-redb/src/lib.rs:73-155`.
  - **Test:** `a_held_session_fails_the_one_shot_and_the_pass_goes_on`,
    `what_only_a_fleet_listing_could_retire_is_held_and_fails_the_one_shot`,
    `rerunning_the_one_shot_until_satisfied_ends_at_and_past_the_pass_budget` (64 and 65
    sessions), `a_flag_the_one_shot_would_ignore_refuses_it_before_it_acts`,
    `a_data_dir_with_no_store_is_refused_and_none_is_created`,
    `an_empty_store_file_is_refused_and_left_empty` (`:903-1254`), plus 7 unit tests for
    `open_existing`.
- **Claim:** the largest fence this code can build fits both batch budgets.
  - **Test:** `largest_fence_fits_both_batch_budgets`
    (`crates/custodian/src/reaper/tests.rs:1679`). It builds a session at Create's real size
    limit with every slot pinned: 20 ops and 202,838 bytes, against limits of 500 and 5,000,000.
- **Concurrency:** seeded DST rows were added to `crates/dst/tests/custodian.rs`: a part commit
  between the two progress reads (X60), a renewal committing after the expired read (X62), a
  session created mid-pass (X19), a read fault, the pre-first-chunk window (live survives,
  crashed is reaped), a Complete race, an unknown fence outcome, and the ignored slot-cycle row
  (`:8735`).
- **Red → green:** `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated, drives the
  real `wyrd` binary as a subprocess). On the base it compiles and **13 of 13 fail**, because the
  flag is unknown. With this change, **13 of 13 pass**.
- **Whole gate:** `cargo xtask ci` passes: 2,605 tests passed, 0 failed, 16 ignored, including
  the `--cfg madsim` DST suite. `cargo mutants --in-diff`: 233 mutants, 207 caught, 0 missed,
  26 unviable. The `fdb,etcd` and `tikv,etcd` clippy rows also pass.
- **Not verified:** the one-shot's outcome line against a live TiKV or FoundationDB cluster.
  The naming code is unit-tested, and the feature-gated arms were run by hand, but those arms
  are not part of `cargo xtask ci`. Diff coverage against `main` can't be measured until the
  stacked PRs merge.

Fixes #1015
