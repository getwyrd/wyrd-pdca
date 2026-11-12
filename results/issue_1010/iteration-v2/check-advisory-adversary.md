# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

I tried to refute this patch three ways: re-running red→green, hand-written mutants (17 small deliberate breaks of the fix), and two probe tests run in a scratch copy of `$PDCA_TARGET`. Two findings hold up. Everything else I attacked held (listed at the end).

## Findings

- NEEDS-HUMAN [human] — **The fence phase fences again with a stale stamp after a rollback under an unknown fence. This confirms the T4 blocker.** `crates/core/src/multipart/complete.rs:877-927`: after an unknown fence, the re-read is answered "as after a `Conflict`". So an `Open@E+2` session is treated as a fresh round, and `:905` mints a new fence with the same `request.fenced_at_millis`. Probe (scratch only): the fence answers `CommitUnknownResult` (both `may_still_commit` values), and before the re-read the session is rolled back to `Open@2, attempts 1`. This is the G1-open shape, but in the fence phase. Result: a second fence `Completing@3 { fenced_at_millis: 2000 }`, with `attempts 2`, and then `Published`. The reaper only rolls back a fence when `now - fenced_at > W_completing`, so this new fence is eligible for rollback from the instant it lands, and each repeat burns one of `MAX_COMPLETE_ATTEMPTS`. This contradicts:
  - the patch's own module doc (`complete.rs:118-122`: "a second fence stamped with it would be past the window from the start");
  - the building-block-view text it adds ("this call does not fence again, since its fence time is the one the rollback found too old");
  - 0016:1005-1009, which says `fenced_at_millis` is the fence instant;
  - the sign-off decision already approved for the publication phase.

  Data safety holds: the flip still requires the fence's bytes. I tag it `[human]` because the brief's fence table says "any other state | answered from it, as after a `Conflict`", so the code follows the brief, and fixing it means extending the approved publication-phase rule to the fence phase. A narrow fix to consider: after an unknown fence of this call, a re-read that finds `placed.charged` (the charged `Open@E+2`, which is the reaper-rollback shape if the reaper (#625) writes the same bytes) answers `OperationAborted`, while `placed.restored` (a rival fenced and gave the attempt back) may still fence again. That keeps `g6_held_a_read_that_finds_the_session_moved_clears_a_held_fence` (which moves to `Open@2, attempts 0`) green.

- NEEDS-HUMAN [impl] — **G1 is pinned only for flat objects. Its segmented path is untested.** At `crates/core/src/multipart/complete.rs:1472` (`unknown_flips.push(...)`), changing it to push only `if !writes_segments` survives all 78 legs. Under that mutant, a wide-fixture first round (fence `Pass`, segment batch `Pass`, flip applied and answered `Unknown`) answers `Ok(AlreadyCompleted(..))` instead of `Published` for the object this very request published. That is the brief's G1 defect, on the path large uploads take. I ran the same probe on the real patched code and it answers `Published` for both `may_still_commit` values, so the code is right but nothing guards it. `g1_a_flip_that_landed_answered_unknown_is_published_once` uses only `two_parts` (flat). The segmented round appears only in G3, behind failing reads. Fix: add a `staged_wide()` run to G1 (plan `[Pass; 1 + SEGMENT_BATCHES]` + `Unknown { applied: true, .. }`), asserting `Published`, `assert_published_once` and `assert_every_segment_named`.

## Evidence re-run (no action needed)

- **C4-verify's "pass" is green-only.** Its log says the red could not be isolated. But the test does live in its own file (`crates/core/tests/multipart_complete.rs`; the gate seems to only look for added test files). I ran the red myself:
  - pre-fix `complete.rs` + `multipart.rs` with the new test file: 14 of 16 new legs fail;
  - with the fix: 78/78 pass.
  
  The two that pass before the fix (`g3_a_failed_re_read…`, `g5_…`) are expected, since the base answered in-doubt to every unknown outcome. Their reds come from mutants instead: removing the own-bytes check (`complete.rs:947-951`) fails G5 and G0-fence, and dropping the unsettled doubt (`complete.rs:1844`) fails G3 and G3-fence.
- **Every probe from the carry-forward is now killed:**
  - `holds()` → false (`g6_held_no_check…`);
  - the `stop` closure ignoring the doubt (`g3_fence…`, `g6_held_no_check…`);
  - no settle at the top of the fence loop (`g6_spent…`, `g6_held_a_read…`);
  - no doubt check before a definite refusal at `:1481` (`g2_refused…`);
  - `in_doubt` order swapped (`g2_first…`, `g3_fence…`);
  - settle leaving the unsettled doubt (`g6_spent…`, `g2_settled…`);
  - no pin (4 legs) and no re-pin (`g4_repin…`);
  - a held doubt never cleared (`g6_held_a_read…`, `g4_landed…`);
  - `end_publication` ignoring the doubt (4 legs).

  Beyond the one above, the only survivor was matching just the latest unknown flip instead of any (`:1509-1511`). I judge it equivalent: an earlier held flip can land only on the same target read, and that gives byte-identical `Completed` bytes.
- **C5-mutants' "pass" does not cover the gap above.** `cargo mutants` replaces whole bodies and flips operators. It never generates a guarded push like the one in the [impl] finding.

## Attacked and could not refute

- G5's identical fences, in both orders.
- Skipping the pre-fence check while a fence is held: any fence placed that way is validated behind the fence and released on rejection.
- Retrying an out-of-flight unknown when the re-read finds `Open@E` unchanged: `crates/traits/src/lib.rs:240-247` makes one read final.
- The held doubt staying correct across a second held outcome (re-pin).
- The fence-phase doubt not carrying into publication: a `Committed` retry replaces the `Open@E` bytes the held fence requires, and the epoch only rises.
- No release while a batch is held, on all `end_publication` sites and at `:1481`.
- `Published` coming only from the exact `Completed` bytes of flips this call sent, never invented.
- Bounds: every unknown outcome spends a `FENCE_ROUNDS` or `R_PUBLISH` round.
- Rubric (*Transactions*): `CommitUnknownResult` is never read as `Conflict`, and a held doubt outranks a definite refusal.
<!-- pdca:leaf-complete -->
