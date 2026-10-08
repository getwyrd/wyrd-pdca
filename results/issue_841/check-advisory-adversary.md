# Adversarial review — #841 (809.3) restore fence for Open upload sessions

Verdict: **I could not refute the fix.** One test-strength regression the patch causes in a
neighbouring file (a judgment call because the brief froze that file), and two non-blocking notes.

## Findings

- NEEDS-HUMAN [human] — `crates/custodian/tests/restore_staged_report.rs:574-578` (child-1's
  leg `an_untrusted_staged_record_is_named_needs_no_human_and_is_not_a_clean_bill`): the leg's
  assertion (b), `!report.is_clean()` "over a chunk held on an untrusted record", **now passes
  for the wrong reason.** Its fixture seeds two `Open` sessions (`:504`, `:524`), which the
  new fence moves to `Aborting`. That sets `sessions_fenced: 2`, and `is_clean()` is false from
  that alone (`crates/custodian/src/restore.rs:286`). The premise the leg states at `:490` and
  checks at `:546-553` ("the held record is the run's ONLY finding") is no longer true.
  **Reproduced:** in a scratch copy I removed `&& self.staged_untrusted.is_empty()` from
  `is_clean()` (`restore.rs:288`). `cargo test -p wyrd-custodian` (every test binary, this leg
  included) still passes 100%. On the base the same mutant fails this leg, because nothing else
  makes that report unclean. The predicate is still pinned, but only by unit tests over
  hand-built reports in another crate (`crates/server/src/cli.rs:3224`, `:3068`). No test
  through the production `reconcile_after_restore` checks it any more. The fix is small: seed
  that leg's sessions as `Aborting`, or assert `sessions_fenced: 2` and check the untrusted
  verdict another way. But the brief allowed only the `segment_nonce` change in this file
  ("nothing else in it"), so a human has to approve the edit or a follow-up. That is why this is
  tagged `[human]`, not `[impl]`.

- Note (no action needed) — `docs/design/architecture/m4-first-deployment-blueprint.md:627-631`
  and `docs/design/architecture/06-runtime-view.md:65` list five reasons a session can be "NOT
  FENCED". The code has seven (`crates/custodian/src/restore.rs`, `SessionUnsettled`). The
  lists leave out `KeyNamesNoUpload` and `LostConflict`. The CLI paragraph prints each reason in
  words (`cli.rs` `unsettled_causes`), so an operator is not left stuck. This is a small docs
  mismatch, not worth a rebuild on its own.

- Note (not a refutation) — the `C4-diff-cov` row failed because "patch.diff does not apply on
  origin/main" (`gate-logs/C4-diff-cov.log`). The real base is main plus child-1 and child-2, so
  the gate ran against the wrong base and diff coverage was never measured. That is a harness
  problem, not evidence against the fix. The `LostConflict` branch (`restore.rs:777-779`) is the
  only fence branch I found that no test reaches.

## What I tried to refute, and could not

- **Red→green is real.** `gate-logs/C4-verify.log`: all 9 legs fail by assertion on the reverted
  base, then pass. The red run's `Debug` output shows child-1's fields, and the seeded sessions
  decode with the nonce, so the red base is the right one (main + child-1 + child-2). Every leg
  calls the production `reconcile_after_restore`. Nothing is re-implemented in the test.
- **F-atomic would catch a split fence in either order.** With the session written first, run
  `a2` leaves `Aborting` and no obligation. With the obligation written first, run `a1` leaves an
  orphan obligation. `assert_fenced` also requires a single applied batch to carry both writes.
- **The double's preconditions do not hide a missing guard.** Drop `require(key, read)`
  (`restore.rs:755`) and F-race sees `Aborting` over `Completing@4`. Drop
  `require_absent(obligation)` (`:756`) and F-collision sees the foreign `{session}` overwritten.
  The double's `apply` checks preconditions exactly as `Precondition` defines them
  (`crates/traits/src/lib.rs:1490-1498`).
- **Iteration 2's `clean=true` defect is fixed and pinned.** `restore.rs:1320` now
  uses `fence_finished && report.is_clean()`. The new otherwise-clean P3 variant asserts
  `clean=false`, `needs_human=true`, and it ran red on the base.
- **Iteration 2's untested `open_teardown` contract is now tested.** A `wyrd-core` unit test
  covers every non-`Open` state and `u64::MAX`, and round-trips key and payload through
  `decode_retire_obligation` (CI log line 900). C5 now reports 0 missed mutants.
- **The fenced record always decodes.** `open_teardown` skips `TryFrom` validation, but the only
  cross-field rule applies to `Completing` (`crates/core/src/multipart.rs:2312-2337`). So
  `Aborting@E+1`, built from a record that already decoded, is always valid, and its encoding is
  canonical. The test's `aborting()` helper asserts the byte-for-byte round trip.
- **An unknown commit outcome is never read as a conflict.** An `Err` from `commit` propagates
  as-is (`restore.rs:783-786`). `Conflict` arrives as `Ok(CommitOutcome::Conflict)` per the seam
  contract. Re-running after a commit that actually landed sees `Aborting@E+1` and skips it.
- **Paging** follows `walk_staged_range` exactly (`gc.rs:1714-1730`). The fence's writes go to
  keys already listed, or to `retire:`, so they cannot shift the cursor.
- **The writes-during-restore legs in staged_protection still exercise their race.** Their hooks
  fire on reads of the staged ranges, which happen before the fence, and each leg asserts its
  concurrent batch `Committed` (e.g. `staged_protection.rs:1268-1272`).
- **Rubric: not raised.** Bounded awaits: the comment follows the repo's existing #508/#636
  convention (`reconstruction.rs:611-613`, `restore.rs:922-923`). Tier-0 DST coverage: settled
  by the `// deferred: #843` marker. No new clock read.
