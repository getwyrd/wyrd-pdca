# Result — issue 1010 / multipart-complete-unknown-outcomes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 to child-3, any `CommitUnknownResult` on a commit of Complete's
  returns the in-doubt error. That is safe, but coarse in two ways:
  - a flip that **landed** and was answered unknown makes Complete return an error for an object
    it published;
  - an unknown segment write or initial fence that did not land stops a Complete that could
    resume or retry.

  0016 requires the publisher to re-read and recover (`0016:942-964`). Two traps must stay closed:
  - an unchanged re-read after `may_still_commit: true` does not prove the held batch will not
    land (`crates/traits/src/lib.rs:240-247`);
  - after an unknown **initial** fence, a re-read showing `Completing@E+1` with the proposed
    bytes is not proof of ownership. A `Completing` record names no request
    (`multipart.rs:2186-2194`).
- Success criterion: New legs appended to `crates/core/tests/multipart_complete.rs`. The
  wrapper gains a **hold** step: it keeps a batch with `may_still_commit: true` and lands it later
  (the shape of `crates/core/tests/multipart_abort.rs:961-980`). The G0 cases for the flip, the
  segment batch and the unapplied initial fence are replaced by G1, G4 and G6. The G0 cases for
  the applied fence, the release and the failed read stay.
  **(G1)** The wrapper applies the flip and answers `CommitUnknownResult`. Complete answers
  `Published` once. The inode version advanced once, and exactly one set of obligations exists.
  **(G2) A held flip.** The wrapper holds the flip (`may_still_commit: true`) and answers every
  later commit `Conflict` without applying it, until the rounds run out. Complete returns the
  in-doubt error. Released afterwards, the held flip lands: the object is published once, with
  one set of obligations.
  **(G3) A failed re-read.** After an unknown flip outcome, the re-read of `mpu:<id>` fails.
  Complete returns the in-doubt error, never an answer.
  **(G4) An unknown segment write.** A segment batch answers `CommitUnknownResult {
  may_still_commit: false }`, applied in one run and not in another. Complete re-reads, resumes
  from `segments_written` at the same epoch, writes no `seg:` record twice, and publishes once.
  **(G5) An unknown INITIAL fence never proves ownership.** With the clock fixed, Complete A
  (parts 1, 2) and Complete B (parts 1, 3) both read `Open@E` and build byte-identical fences.
  - **Run 1:** the wrapper holds A's fence, lets B's commit (a definite success), then answers A's
    `CommitUnknownResult { may_still_commit: false }` unapplied.
  - **Run 2:** the wrapper applies A's fence and answers it unknown, then B's fence conflicts.

  In both runs, A's re-read finds `Completing@E+1` with exactly its proposed bytes, and A returns
  the in-doubt error with no `seg:` record and no flip. In run 1, B publishes parts 1 + 3 once.
  In run 2, B answers `Refused(OperationAborted)`, nothing is published, and the session stays
  `Completing@E+1` for the reaper (#625).
  **(G6) An unknown initial fence that did not land is retried.** The first fence answers
  `CommitUnknownResult { may_still_commit: false }` unapplied, and the re-read finds `Open@E`
  unchanged. Complete retries the fence (bounded), gets a definite success, and publishes once.
  Every earlier leg still passes.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the re-read, resume, retry and settling logic after a `CommitUnknownResult` on the initial
    fence, segment batches, flip and release;
  - the in-doubt error's doubt bookkeeping (the first `may_still_commit: true` outcome is kept,
    as `abort.rs` does);
  - the wrapper's hold step.

  Out of scope:
  - any change to the error type's arms (child-1 owns them);
  - any record shape change (none is needed);
  - the value ceiling (child-5);
  - the reaper (#625), the gateway's same-request recovery (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 2m: 14 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.93s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1010, recovering multipart Complete from unknown commit outcomes: one reproduced initial-fence retry defect needs correction; the existing regression suite passes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Recovery, uncertain ownership and held-commit semantics are falsifiable and scoped; the approved client-retry interpretation remains settled (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:942`, `crates/traits/src/lib.rs:240`). |
| C2 Reproduction (red pre-fix) | PASS | Removing the production recovery change while retaining its tests produces 14 assertion failures, including G1, G4 and G6 (`crates/core/tests/multipart_complete.rs:5407`, `reviewer-red.log:355`). |
| C3 Change | PASS | The four affected files stay within recovery, its tests and documentation; the living architecture describes the changed operation, with no persisted-schema or dependency change (`docs/design/architecture/05-building-block-view.md:252`). |
| C4 Verification (red→green) | PASS | Restoring the identical patch passes all 662 core tests, including all 78 multipart-completion tests; diff coverage remains unmeasured because of the acknowledged stacked base (`reviewer-red-green.log:15`, `reviewer-restored-green.log:250`, `gate-logs/C4-diff-cov.log:11`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct recovery after an unknown initial fence rereads a newer Open epoch — the reproduced retry reuses an expired timestamp and charges another immediately rollback-eligible attempt; require a fresh client retry and a regression (R1; `crates/core/src/multipart/complete.rs:943`, `crates/core/src/multipart/complete.rs:905`). |
| T1 Structure | PASS | Recovery stays behind the existing MetadataStore seam and shared publication committer, preserving dependency direction (`crates/core/src/multipart/complete.rs:854`, `crates/core/src/multipart/complete.rs:1455`). |
| T2 Shape | PASS | Exact session bytes remain the ownership and CAS boundary; the added doubt bookkeeping introduces no wire-format change (`crates/core/src/multipart/complete.rs:947`, `crates/core/src/multipart/complete.rs:1795`). |
| T3 Runtime | PASS | Retry work remains bounded by fence/publication budgets and the store's deadline contract; no additional runtime or clock source is introduced (`crates/core/src/multipart/complete.rs:919`, `crates/core/src/multipart/complete.rs:1349`, `crates/core/src/multipart/complete.rs:1537`). |
| T4 Contribution | FAIL | The required review has one unresolved defect, independently reproduced as R1; affected-path prior art is checked, while contribution-artifact auditing is N/A until publish (`gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The carry-forward doubt-bookkeeping gaps now have discriminating regressions; accepted scope/size decisions and tracked DST deferral remain settled, and no capability-probe workaround is introduced (`crates/core/tests/multipart_complete.rs:5985`, `crates/core/tests/multipart_complete.rs:6463`, `crates/core/src/multipart/complete.rs:782`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected recovery behavior satisfies #1010's intended core contract — automated evidence establishes in-process behavior, while acceptance of this slice remains a human decision (`crates/core/src/multipart/complete.rs:255`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:942`). |

Source citations above resolve in the supplied `$PDCA_TARGET`; log citations resolve in this review directory. The target was readable, contained the prerequisite implementation, and compiled. This review is advisory; it does not decide acceptance.

**R1 — An unknown initial fence can be recreated with its expired timestamp (P2).** The new reread at `crates/core/src/multipart/complete.rs:943` loops into `doubt.settle` at line 877. A newer `Open` record then passes `answer_without_fence`, and line 905 mints another fence with the original request stamp. If the initial fence landed and expired before that reread, the new fence is already eligible for rollback. This wastes another attempt and can repeatedly prevent progress. The same timestamp hazard is already recognized for publication recovery at `crates/core/src/multipart/complete.rs:220`; the expiry rule is `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1005`.

The independent probe committed the actual initial fence to the real Redb store, performed a conditional rollback to `Open@2`, and only then delivered the unknown outcome. The patched function returned `Held`, committed twice, and left `Completing@3`, `attempts=2`, `fenced_at_millis=2000`, reusing the first fence's stamp (`reviewer-probe.log:3`). The assertion requiring `OperationAborted` failed (`reviewer-probe.log:7`). This models the reaper's valid state transition; it does not claim to execute the deferred reaper implementation. Reproduce with `./reviewer-fence-probe reviewer_unknown_initial_fence --nocapture`; the standalone probe source is `reviewer-fence-probe.rs:6540`. Correct the initial-fence recovery branch and retain this schedule as a regression. Existing G6 tests cover unchanged Open, and the newer-Open test removes the bucket, which prevents it from reaching this second fence (`crates/core/tests/multipart_complete.rs:5917`, `crates/core/tests/multipart_complete.rs:6059`).

**Independent verification supports the main recovery behavior, but does not discharge R1.** Stashing only `complete.rs` retained the new tests and produced 64 passes / 14 failures; restoring it passed all 662 core tests. The complete diff's SHA-256 matched before and after (`reviewer-red-green.log:15`). Typos, documentation lint and diff whitespace checks passed (`reviewer-checks.log:1`). The declared `typos` dependency was present and exercised.

The independent mutation run exercised all 20 generated mutants: 11 caught and 9 unviable (`reviewer-mutants.log:5`). Three were unviable solely because mutation-created unused code triggered denied warnings; rerunning those with `--cap-lints true` caught all three (`reviewer-mutants-lints.log:5`). Thus 14 mutants were caught and six were type-invalid, agreeing with the frozen gate's aggregate result (`gate-logs/C5-mutants.log:13`). No generated mutant survived; R1 is a missing scenario beyond that generated set.

**Frozen gates and prior art are accounted for.** The full CI log records successful formatting, clippy, build, tests, dependency checks, conformance and DST (`gate-logs/C4-ci.log:30`, `gate-logs/C4-ci.log:4836`); the TiKV log records both requested feature builds completing. Local formatting could not be rerun because this sandbox lacks the rustfmt component (`reviewer-fmt.log:1`), so formatting relies on the frozen successful run. The green-only verifier does not itself demonstrate red; the independent stash run supplies that evidence. The diff-coverage red is the already-cleared base-ordering limitation, not a compilation defect or a new approval request. The contribution-artifact row is **N/A**: its subject is drafted later and its substantive check reruns at publish (`gate-logs/T4-contribution.log:10`). The failed batch-review finding is substantiated by R1.

Merged history was queried by all four affected paths. All 445 closed PRs were enumerated, and changed-file lists for all 19 closed-unmerged PRs were checked against those paths; none overlapped. The relevant merged predecessor is #1032, and Abort's #909 precedent is present in the multipart history (`reviewer-prior-art.log:1`, `reviewer-prior-art-results.json`). No unresolved prior-art decision remains.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:939`: **Unknown initial-fence recovery can place an already-expired fence.** If the initial fence lands but answers unknown, and the reaper rolls `Completing@E+1` back to `Open@E+2` before the recovery read, the new retry path feeds that newer `Open` into the loop. `doubt.settle` clears the doubt, and `Placed::mint` at `crates/core/src/multipart/complete.rs:905` stamps the next fence with the original `request.fenced_at_millis`. That timestamp has already exceeded `W_completing`, so the new attempt is immediately eligible for another rollback, wasting attempts and potentially interrupting publication again. The publication recovery path already avoids this case by returning `OperationAborted`. Preserve retries of unchanged `Open@E`, but avoid re-fencing a recovered newer epoch with the old stamp; let a fresh client retry supply its timestamp. Add a wrapper regression where an applied-unknown initial fence is rolled back before its reread while the bucket and parts remain valid.

No additional correctness or reuse/simplification/efficiency findings. Reviewed the supplied diff against the target source and frozen gate evidence; gates were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct recovery after an unknown initial fence rereads a newer Open epoch — the reproduced retry reuses an expired timestamp and charges another immediately rollback-eligible attempt; require a fresh client retry and a regression (R1; `crates/core/src/multipart/complete.rs:943`, `crates/core/src/multipart/complete.rs:905`).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected recovery behavior satisfies #1010's intended core contract — automated evidence establishes in-process behavior, while acceptance of this slice remains a human decision (`crates/core/src/multipart/complete.rs:255`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:942`).
- [ ] **The fence phase fences again with a stale stamp after a rollback under an unknown fence. This confirms the T4 blocker.** `crates/core/src/multipart/complete.rs:877-927`: after an unknown fence, the re-read is answered "as after a `Conflict`". So an `Open@E+2` session is treated as a fresh round, and `:905` mints a new fence with the same `request.fenced_at_millis`. Probe (scratch only): the fence answers `CommitUnknownResult` (both `may_still_commit` values), and before the re-read the session is rolled back to `Open@2, attempts 1`. This is the G1-open shape, but in the fence phase. Result: a second fence `Completing@3 { fenced_at_millis: 2000 }`, with `attempts 2`, and then `Published`. The reaper only rolls back a fence when `now - fenced_at > W_completing`, so this new fence is eligible for rollback from the instant it lands, and each repeat burns one of `MAX_COMPLETE_ATTEMPTS`. This contradicts:
- [ ] **G1 is pinned only for flat objects. Its segmented path is untested.** At `crates/core/src/multipart/complete.rs:1472` (`unknown_flips.push(...)`), changing it to push only `if !writes_segments` survives all 78 legs. Under that mutant, a wide-fixture first round (fence `Pass`, segment batch `Pass`, flip applied and answered `Unknown`) answers `Ok(AlreadyCompleted(..))` instead of `Published` for the object this very request published. That is the brief's G1 defect, on the path large uploads take. I ran the same probe on the real patched code and it answers `Published` for both `may_still_commit` values, so the code is right but nothing guards it. `g1_a_flip_that_landed_answered_unknown_is_published_once` uses only `two_parts` (flat). The segmented round appears only in G3, behind failing reads. Fix: add a `staged_wide()` run to G1 (plan `[Pass; 1 + SEGMENT_BATCHES]` + `Unknown { applied: true, .. }`), asserting `Published`, `assert_published_once` and `assert_every_segment_named`.
- [ ] `crates/core/src/multipart/complete.rs:939`: **Unknown initial-fence recovery can place an already-expired fence.** If the initial fence lands but answers unknown, and the reaper rolls `Completing@E+1` back to `Open@E+2` before the recovery read, the new retry path feeds that newer `Open` into the loop. `doubt.settle` clears the doubt, and `Placed::mint` at `crates/core/src/multipart/complete.rs:905` stamps the next fence with the original `request.fenced_at_millis`. That timestamp has already exceeded `W_completing`, so the new attempt is immediately eligible for another rollback, wasting attempts and potentially interrupting publication again. The publication recovery path already avoids this case by returning `OperationAborted`. Preserve retries of unchanged `Open@E`, but avoid re-fencing a recovered newer epoch with the old stamp; let a fresh client retry supply its timestamp. Add a wrapper regression where an applied-unknown initial fence is rolled back before its reread while the bucket and parts remain valid.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- [ ] size backstop — this slice is behaving oversized: patch is 138 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Production logic holds except one gap; the design stands with one extension decided here. - Human decision: extend the approved publication-phase rule to the fence phase. After an unknown initial fence, if the re-read finds the session rolled back by the reaper (the charged Open@E+2 shape), answer OperationAborted so the client retries with a fresh timestamp. Never mint a new fence with the original request.fenced_at_millis. A rival's give-back (restored attempt, e.g. Open@2 attempts 0) may still fence again, so g6_held_a_read_that_finds_the_session_moved_clears_a_held_fence stays green. This overrides the brief's fence-table row "any other state | answered as after a Conflict" for that case. Update the module doc (complete.rs:118-122) and the building-block-view text to match. - Add the reviewer's probe as a regression: applied-unknown initial fence, rolled back to Open@2 before the re-read, bucket and parts still valid -> OperationAborted, no second fence (reviewer-fence-probe.rs). - Add a staged_wide() run to G1 (plan [Pass; 1 + SEGMENT_BATCHES] + Unknown { applied: true }) asserting Published, assert_published_once and assert_every_segment_named, so the unknown_flips push at complete.rs:1472 is guarded on the segmented path. - Size backstop (138 KB) is overridden again: do not split. Production code is unchanged from v1; the growth is the tests requested last round. - Diff coverage "not measured" remains the stacked-base artifact, already cleared.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
