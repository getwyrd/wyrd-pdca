# Adversarial review — #1017 (slice 625.3), iteration 3

**Bottom line.** The main fix this round holds up. The publication stamp is now read just before the flip (`crates/core/src/multipart/complete.rs:1655`). I reverted it in a scratch copy, reading the stamp once before the fence as the old code did, and both new regression tests went red. I found no input that breaks the production logic. I did find two test gaps that would let real regressions merge green: one the builder can fix, one that needs a scope decision.

## Findings

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8242`: the DST (deterministic simulation test) helper `complete()` stamps every publication with a frozen `&|| at`, which is the fence time `t0`. The reaper reads a separate `ManualClock` set to `stale = t0 + W_completing + draw` (`:8713`). So the race rows model a world where a tombstone (the `Completed` record kept so a retry can be answered) is already past `W_tombstone` when its flip lands. That is the exact defect this iteration fixed. I ran both consequences under `--cfg madsim`:
  1. **Leg (ii), flip-first (`:8770`–`:8830`).** The flip that wins the race publishes a tombstone, and the three reaps at `:8812` delete it at once. I added a probe assertion that the record survives those reaps. It failed with `stamp=86440527892 reaper_now=86444215601 age_ms=3687709 tombstone_kept=false`. The shipped row passes anyway, because it never checks that the record it just published survives.
  2. **Fix reverted.** With the stamp read once before the fence (the pre-fix behaviour, patched into `complete_upload` in scratch), all 9 `reaper_*` DST rows stay green, both race rows included. The seeded Tier-0 coverage therefore does not guard the defect this iteration fixed (rubric: *Test fidelity*; DST is the correctness authority, ADR-0009).
  - **Fix:** drive the publication stamp from the reaper's `ManualClock`. Advance that clock past `W_completing` after the fence commits, to model the stalled completer. Then in leg (ii), assert that the record is still `Completed` after the reaps and that an identical retry answers `AlreadyCompleted`. The leg then goes red on the reverted fix.

- NEEDS-HUMAN [human] — `crates/server/src/custodian.rs:638`: no test sees the deployed fleet loop's tombstone input (`tombstone: policy.tombstone()`) expire a tombstone. In scratch I changed it to `wyrd_custodian::Tombstone::Retain`, and the three server test files that exercise the reaper and the deployed loop all still passed: `custodian_gc` 12/12, `multipart_reaper` 13/13, `multipart_reaper_exits` 8/8.
  - Leg G goes through the one-shot. The one-shot reads `reaper.policy.tombstone()` through `reconcile_record_only` (`crates/custodian/src/retirement.rs:631-636`), not this line.
  - The only test of the deployed loop's tombstone handling (`crates/server/tests/custodian_gc.rs:1472`, `:1534`) asserts that the record *stays*.
  - So the slice's invariant, that in the deployed shape every `Completed` session has an exit, rests on a line nothing tests. The leg that would close this belongs in `custodian_gc.rs`, which is child-2's file (#1016). It is outside this slice's scope exception, which covers only the fixture timestamp.
  - **Decision needed:** widen the exception for one deployed-loop expiry leg, or have #1016 carry it.

## Evidence notes (no action needed)

- **The C4-verify row overstates.** `check-gates.json` says "8 test(s) ran red", but the red leg reads `2 passed; 6 failed` (`gate-logs/C4-verify.log:304`).
  - The two that pass on the base are `completing_session_fenced_moments_ago_is_kept_however_old_its_parts` (`crates/server/tests/multipart_reaper_exits.rs:734`) and `profile_mismatch_refuses_the_completing_arms` (`:934`). Both are vacuous on the base, as the brief predicts.
  - That gate also never covers this round's main fix. I checked it by hand: with the stamp read before the fence, `d_a_slow_completes_tombstone_lasts_the_whole_window_after_its_publication` (`crates/custodian/tests/multipart_session_terminal.rs:1389`) fails at `:1413`, and `the_publication_stamp_is_read_at_the_flip_behind_every_segment_batch` (`crates/core/tests/multipart_complete.rs:7195`) fails at `:7172` (`left: 2500`).
  - The fix is properly evidenced. Only the harness wording is off.
- **C5's one surviving mutant is equivalent.** It is at `crates/metadata-conformance/src/lib.rs:1651` (`+` → `-` before `built.deletes.len()`). Calibration batches carry no deletes, so the value is unchanged. This is not a test gap.

## Attempted, could not refute

- **Unknown-outcome flip whose tombstone the reaper deletes before the recovery read** (`complete.rs:1719-1724`, after which `moved` answers `NoSuchUpload` at `:1456-1457`, a T4 BUG). For this to happen, more than `W_tombstone` (2 min) plus a full drain must pass between the flip's commit and the `get` that immediately follows it. I found no bounded path that long, and 0016 accepts `NoSuchUpload` after the window (`0016:2863`).
- **`ReadyFlip::flip` accepting any store** (`crates/core/src/staged_publication.rs:508`, a T4 BUG). The only caller passes the same store it ran the segment phase on (`complete.rs:1670`). This is an API footgun, not a reachable failure.
- **Rebuilt flip refused after the segments landed** (`complete.rs:1655-1669`). `overwrite_modified` cannot fail when a stamp is present (`crates/core/src/metadata.rs:2409-2429`). The only thing left that could refuse it is the byte budget changing with the stamp's digit count, and millisecond stamps stay 13 digits until the year 2286.
- **`judge_completing`** (`crates/custodian/src/reaper.rs:1107`). I checked the following and found no failing case:
  - the ceiling is judged before the rollback;
  - both boundaries use strict `>` and have exact-tick unit tests;
  - a fresh fence is kept however old the parts are;
  - any completer write between the reaper's read and its fence turns into a `Conflict`;
  - `lost_any` classifies each required-absent key.
- **Tombstone clock identity** (`crates/custodian/src/retirement.rs:425`, `crates/custodian/src/reconciliation.rs:189`). The window is judged on the reaper's own clock and `clock_source` pair. Tests cover both directions and the no-reaper case, which fails closed as `Blocked`. No failing case found.
<!-- pdca:leaf-complete -->
