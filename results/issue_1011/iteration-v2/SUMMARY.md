# Result — issue 1011 / multipart-complete-value-ceiling

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Complete's flip writes each part set inside its own obligation value: the
  published parts' `retire:records:` and the unnamed parts' `retire:bytes:`. Part numbers can be
  spread past S3's range (core's format allows up to 999,999, `multipart.rs:939`). Then a set can
  encode past `MAX_VALUE_BYTES`, and the store refuses that flip for good (FoundationDB `2103
  value_too_large`). After child-2, such a Complete fences, is refused at the flip, releases and
  returns the stopped error (child-2's J), every time. The session can never publish, only be
  aborted. The `all` wildcard is reserved for teardown (`multipart.rs:3174-3177`).
- Success criterion: A NEW test file `crates/core/tests/multipart_complete_value_ceiling.rs`.
  It runs over a wrapper that refuses any value past `MAX_VALUE_BYTES` (as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does). Sessions are made by #894's
  Create, and parts are planted as canonical records.
  **(I1)** Parts 1 to 10,000, Complete naming the odd ones: it publishes, and both `{parts}`
  obligations fit (about 58.9 KB each).
  **(I2)** Parts 1, 3, …, 19,999 (10,000 parts), Complete naming all of them (about 129 KB
  encoded): Complete refuses `Refused(SessionTooLarge { bytes, limit })` with **zero commits**.
  The session stays `Open` with `attempts` unchanged, and an Abort then fences it.
  **(I3) The boundary is the stored value, not the bare set.**
  - Parts 1, 3, …, 15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes
    and the `{"parts":…}` value to 99,991, so it publishes. The installed obligation's stored
    length is at most `MAX_VALUE_BYTES`.
  - Parts 1, 3, …, 15,871 (7,936 parts): the set is 99,995 bytes but the value is 100,005, so
    Complete refuses `SessionTooLarge` with zero commits.

  If the real payload carries more than `parts`, move the pair so it still straddles the ceiling,
  and record the numbers in `build-notes.md`.
  **(I4) A part added after the first read.** A set fits when Complete first reads it. The
  wrapper plants one more unnamed part just before the fence commits, so the unnamed parts'
  `retire:bytes:` value crosses the ceiling. Complete fences, refuses `SessionTooLarge` against the
  frozen set, and releases (`Open@E+2`, `attempts` restored).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the encoded-value check, before fencing and again against the frozen set, through
    `RetireObligation`;
  - the `SessionTooLarge` doc (`multipart.rs:4515-4526`), extended to say Complete answers it;
  - the new test file.

  Out of scope:
  - any change to `UploadPart` or the part-number range;
  - any new `Refusal` variant;
  - other children's legs;
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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 16 mutants tested in 72s: 9 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1011/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.54s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #1011 change correctly refuses oversized Complete part-set obligations before fencing and after freezing; prior-art confirmation and the refusal-policy decision remain for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | I1–I4 define observable acceptance, stored-value boundaries, zero-write refusal and fence release; the policy choice is explicitly reserved for sign-off (brief.md:20, brief.md:97). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production changes while retaining the new tests reproduced four behavioral failures: oversized published/unnamed values reach the stopped publication error instead of the specified refusal (reviewer-red.log:193, reviewer-red.log:218). |
| C3 Change | PASS | The change stays within Complete admission and obligation encoding; the living architecture documents the altered API behavior as the standing rubric requires (target/crates/core/src/multipart/complete.rs:1138, target/docs/design/architecture/05-building-block-view.md:252). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all nine ceiling tests pass; all 85 existing Complete tests also pass. Frozen full-CI evidence is green; diff coverage remains unmeasured because its remote base rejected the patch (reviewer-green.log:18, reviewer-complete-regressions.log:94, gate-logs/C4-ci.log:4871, gate-logs/C4-diff-cov.log:10). |
| C5 Causal adequacy | PASS | Measuring the actual obligation value prevents the impossible flip, and repeating validation after fencing covers the intervening unnamed part; this adds no capability probe or load-time workaround (target/crates/core/src/multipart/complete.rs:961, target/crates/core/src/multipart/complete.rs:1011, target/crates/core/src/multipart/complete.rs:1192). |
| T1 Structure | PASS | The held fence owns the validated obligations subsequently installed by the flip, preserving the existing MetadataStore seam and preventing independent reconstruction from diverging (target/crates/core/src/multipart/complete.rs:1024, target/crates/core/src/multipart/complete.rs:1836). |
| T2 Shape | PASS | Sizing and persistence share the existing payload codec, preserving omitted-field encoding and accepting exactly 100,000 bytes; formatting, spelling and docs lint pass independently (target/crates/core/src/multipart.rs:4382, target/crates/core/tests/multipart_complete_value_ceiling.rs:562, reviewer-fmt.log:4, reviewer-typos.log:4, reviewer-docs.log:5). |
| T3 Runtime | PASS | Both oversized pre-fence sets write nothing, and the injected concurrent part produces only a fence and release with attempts restored; the tests exercise a real redb store and verify persisted state (target/crates/core/tests/multipart_complete_value_ceiling.rs:512, target/crates/core/tests/multipart_complete_value_ceiling.rs:637, target/crates/core/tests/multipart_complete_value_ceiling.rs:690, reviewer-green.log:18). |
| T4 Contribution | NEEDS-HUMAN | Confirm the affected-file prior-art audit across merged and closed/rejected work — the supplied repository has one synthetic base commit and no remotes, and the brief's issue references plus aggregate review log cannot establish that audit (reviewer-prior-art.log:5, brief.md:91, gate-logs/T4-batch-review.log:10). |
| T5 Judgment | PASS | The carry-forward implementation concern is addressed: the new unnamed-set case requires zero commits and byte-identical session state, while the boundary and race controls remain green (brief.md:110, target/crates/core/tests/multipart_complete_value_ceiling.rs:676, reviewer-green.log:9). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Choose Complete-time SessionTooLarge refusal or earlier UploadPart rejection of out-of-range numbers — the brief leaves this client-visible policy undecided; the passing core tests establish behavior, not that product choice (brief.md:97, brief.md:83). |

Independent execution confirms the requested behavior. In the disposable target I ran `git stash push`, retained the untracked test file, ran `cargo test --offline --locked -p wyrd-core --test multipart_complete_value_ceiling`, restored with `git stash pop`, and repeated the command. The pre-fix run compiled and failed four tests, with five passing; the restored run passed all nine. The red failures were the expected publication-size errors, not toolchain failures. The added case at target/crates/core/tests/multipart_complete_value_ceiling.rs:676 closes the previous review's unnamed-set coverage hole.

Additional independent checks passed: the existing `multipart_complete` suite (85 tests), `cargo clippy --offline --locked -p wyrd-core --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `typos`, and the documentation linter. Their outputs are in reviewer-complete-regressions.log:94, reviewer-clippy.log:146, reviewer-fmt.log:4, reviewer-typos.log:4 and reviewer-docs.log:5. The named external dependency, `typos`, was exercised without a substitute. A reverse-application check confirms the restored target matches patch.diff; the stash is empty and no implementation changes were made.

The frozen gate evidence supports these narrower conclusions; instance-scoped harness wrappers were not rerun from another checkout:

- **C4-ci — PASS from log:** the captured run includes actual compilation, tests, dependency checks, documentation rendering and DST, ending in success (gate-logs/C4-ci.log:32, gate-logs/C4-ci.log:4231, gate-logs/C4-ci.log:4492, gate-logs/C4-ci.log:4871).
- **C4-verify — PASS independently reproduced:** the frozen four-failure/nine-pass result agrees with this review's rerun (gate-logs/C4-verify.log:10, reviewer-red.log:218, reviewer-green.log:18).
- **C4-diff-cov — recorded FAIL, base-state caveat:** its log shows only that patch.diff did not apply to origin/main, so no coverage percentage was produced. The supplied target compiles and passes; this is not evidence of a patch compilation or correctness defect (gate-logs/C4-diff-cov.log:10).
- **C5-mutants — PASS from log:** 16 mutations were attempted, nine caught and seven unviable; the log establishes no surviving viable mutant, not exhaustive causal coverage (gate-logs/C5-mutants.log:13).
- **T4-batch-review — PASS from log:** the aggregate reports zero blocking findings; it contains no file-scoped prior-art evidence (gate-logs/T4-batch-review.log:10).
- **T4-contribution — N/A:** contribution artifacts are absent by design at Check; the substantive audit reruns at publish (gate-logs/T4-contribution.log:10).
- **host-tikv — PASS from log:** both requested feature-enabled clippy builds completed; this establishes compilation, not a live TiKV service run (gate-logs/host-tikv.log:2, gate-logs/host-tikv.log:211).

The target's standing rubric was applied, including serialization identity, dependency direction, clock consistency, shared mutable state and documentation currency. No new clock, external await, persisted field, destructive path or capability probe was introduced. Production S3 wiring remains explicitly outside this slice (brief.md:83); the regression fixture itself can exhibit the forbidden failure, as the independent red run demonstrates. No additional unmet external dependency was identified.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1011 (multipart Complete value ceiling), iteration 2

Result: **I could not refute the fix.** I re-ran the red→green myself, ran seven hand-made mutants, and looked for inputs that break it. None got through. The findings below are about the gate evidence, not the patch.

## Attempts that failed to break it

- **Red→green, re-run on a scratch copy of `$PDCA_TARGET`.** I reverted `crates/core/src/multipart.rs` and `crates/core/src/multipart/complete.rs` to `HEAD` and kept the new test. 4 tests fail: I2, I3-past, I4 and I4-already-past (`crates/core/tests/multipart_complete_value_ceiling.rs:502`, `:579`, `:629`, `:675`). Each one gets `Stopped(… ValueOverCeiling { phase: Flip })` where it expected `SessionTooLarge`. With the patch all 9 pass. The test calls the real `complete_upload` over `RedbMetadataStore` (`:369`), not a copy of the logic. Its sizes are written out as literals (`:89-96`), and every stored obligation is compared byte for byte against a spelling built in the test itself (`:409-413`), so a matching bug in the codec could not hide a failure.
- **Mutants (all caught):** (1) remove the check: I2/I3/I4/I4-already fail. (2) **The mutant from round 1**: the check before the fence measures only the published set. Now caught by I4-already-past at `:691` ("zero commits"). The round-1 finding is fixed. (3) Check only before the fence: I4 fails at `:443`. (4) Check only after the fence: I2/I3-past/I4-already fail on their zero-commit asserts (`:513`, `:591`, `:691`). (5) Measure the bare set, i.e. value length − 10: 4 fail. (6) `>` changed to `>=` at `complete.rs:1195`: the at-ceiling leg (`:562`) fails. (7) Measure only the unnamed set: I2 and I3-past fail. This backs the brief's SELF-TEST claim that a guard on either side alone fails I4 or I2.
- **The measured obligations are the installed ones.** `validate` mints them (`complete.rs:1138`) at `placed.epoch()`, which is the fence epoch E+1 in both calls (`:961`, `:1011`). `HeldFence` carries them, and the flip installs exactly those (`:1837`) through the same `RetireObligation::value()` (`multipart.rs:4382`). Nothing gets measured that is not then written.
- **Can a set grow after the fence?** No. A part commit needs `mpu == Open@E` (`commit.rs:13`), so nothing lands once the fence is up. The published set is the fixed named list. Only the unnamed set can grow, and only between the first read and the fence, which is exactly what I4 covers. The I4 setup is realistic: a part commit only *requires* `mpu:` and never rewrites it, so a part landing just before the fence does not make the fence conflict.
- **The branch that skips the check while a fence is in doubt** (`complete.rs:960`). A fence can only be in doubt after a fence commit was sent, and a commit is only sent after the check before it passed. So a set that is too large under doubt must have grown after that check. That is the frozen-set case, which the release handles, and the `SessionTooLarge` doc ("nothing written … otherwise against the set the fence froze") stays accurate.
- **Other limits in the flip batch.** The batch byte budget is 5 MB (`multipart.rs:5507`), so two obligations near 100 KB plus the root cannot cross it. The superseded obligation is bounded by the prior's flat map (158 chunks at most) or by a segment-group handle. No other value in the flip grows with the part count.
- **The claim that nothing inside S3's range crosses** (`complete.rs` module doc, *The value ceiling*). There are at most 5,000 runs in 1–10,000. The even set comes to 4·5 + 45·7 + 450·9 + 4,500·11 + 13 bytes of runs, plus 4,999 commas and a 12-byte frame, which is 58,909 bytes. That matches `EVEN_TO_10_000_VALUE` (`:90`). The other layouts I tried came out slightly smaller. All are far under 100,000.
- **Rubric classes.** *Absent entries*: `PartNumberSet::from_numbers` returns `None` only for an empty set (`multipart.rs:3431`), so the unnamed obligation is never silently dropped. *Test fidelity / DST*: the patch adds no new destructive or concurrent path, only a new reason for the existing fence release, so no new Tier-0 DST leg is owed. *Docs currency*: `05-building-block-view.md` is updated.

## Findings

- The C4-verify row in `check-gates.json` says "9 test(s) ran red". That count is wrong. `gate-logs/C4-verify.log` and my own re-run both show **4** red ("5 passed; 4 failed"). The other 5 (I1, I3-inside, I3-at, I4-control, the wrapper probe) are controls that pass with or without the fix, which is what controls should do. The red→green verdict holds. Only the count in the summary line is wrong (it's a bug in the harness script, not the patch).
- NEEDS-HUMAN [human] — **All the evidence was gathered on a base that is not `main`.** C4-diff-cov failed again with "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). Iteration 1 failed the same way, and the rebuild did not change it. So C4-ci, C4-verify, C5 and my own runs were all done against `b74afe3`, and diff coverage was never measured. The brief lists this child as conflicting with #1010 around the fence code (`crates/core/src/multipart/complete.rs:960-1011`), so a rebase could touch exactly the lines this fix depends on. A human should refresh the base, or accept the gap, before sign-off. Sending it back to Do as-is is likely to reproduce the same failure.
- NEEDS-HUMAN [human] — **The T4-batch-review row looks too thin for a gating pass.** It claims 3 review passes over a 1,163-line diff with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 14.0 s (`gate-logs/T4-batch-review.log`). That is fast for three model reviews. `review-batch.md` is not among my inputs, so I can't tell whether the passes actually ran on this bundle or reused an earlier result. Worth a look before treating the row as a pass. I am not claiming any defect in the patch here.
- Nothing to route back to Do: I found no failing input, logic slip or weak assertion for the builder to fix. The design question the brief carries (refuse at Complete or at `UploadPart`) is still open for sign-off. Nothing in this patch settles it either way.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: introduced correctness bugs, or actionable reuse, simplification and efficiency issues.

- Correctness: both validation passes measure the stored obligation values, admit equality at the ceiling, and pass the frozen set's obligations to publication (`crates/core/src/multipart/complete.rs:961`, `crates/core/src/multipart/complete.rs:1011`, `crates/core/src/multipart/complete.rs:1192`, `crates/core/src/multipart/complete.rs:1836`). The added unnamed-set regression checks zero commits and unchanged session bytes (`crates/core/tests/multipart_complete_value_ceiling.rs:676`).
- Reuse: part-set construction uses the existing canonical constructors, and sizing and writes share the same payload encoder (`crates/core/src/multipart/complete.rs:1171`, `crates/core/src/multipart.rs:4382`).

Validation evidence was inspected, not rerun: CI passed; all nine new tests passed, with four refusal cases failing when the production fix was reverted; mutation testing reported nine caught and seven unviable mutants. Diff coverage was not measured because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Confirm the affected-file prior-art audit across merged and closed/rejected work — the supplied repository has one synthetic base commit and no remotes, and the brief's issue references plus aggregate review log cannot establish that audit (reviewer-prior-art.log:5, brief.md:91, gate-logs/T4-batch-review.log:10).
- [ ] Validation — fitness-to-purpose — Choose Complete-time SessionTooLarge refusal or earlier UploadPart rejection of out-of-range numbers — the brief leaves this client-visible policy undecided; the passing core tests establish behavior, not that product choice (brief.md:97, brief.md:83).
- [ ] **All the evidence was gathered on a base that is not `main`.** C4-diff-cov failed again with "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). Iteration 1 failed the same way, and the rebuild did not change it. So C4-ci, C4-verify, C5 and my own runs were all done against `b74afe3`, and diff coverage was never measured. The brief lists this child as conflicting with #1010 around the fence code (`crates/core/src/multipart/complete.rs:960-1011`), so a rebase could touch exactly the lines this fix depends on. A human should refresh the base, or accept the gap, before sign-off. Sending it back to Do as-is is likely to reproduce the same failure.
- [ ] **The T4-batch-review row looks too thin for a gating pass.** It claims 3 review passes over a 1,163-line diff with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 14.0 s (`gate-logs/T4-batch-review.log`). That is fast for three model reviews. `review-batch.md` is not among my inputs, so I can't tell whether the passes actually ran on this bundle or reused an earlier result. Worth a look before treating the row as a pass. I am not claiming any defect in the patch here.
- [ ] T5 Judgment — Choose rejection at Complete versus restricting part numbers at UploadPart — the brief leaves this policy unapproved, and it determines whether clients can accumulate a session that cannot publish (`brief.md:97`, `crates/core/src/multipart.rs:4950`).
- [ ] **Diff coverage was never measured, and all green evidence is on a base that is not `origin/main`.** `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main — the bundle is stale". `check-gates.json` still says `overall: pass` because that row does not gate. C4-ci, C4-verify and C5 all ran on the pre-fix base `b74afe3`. The brief says this bundle conflicts with #1010 around the fence (`complete.rs:920-1036`, the code this patch edits). A human should decide which case this is: a stack whose parents (#1008–#1010) have not landed yet (expected), or a base that has gone stale. If it is stale, rebase and re-run the C4 gates before sign-off.
- [ ] **The gating review looks too fast to have run.** `gate-logs/T4-batch-review.log:10` reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 19.03 s, for three model passes over an 80 KB diff. Zero findings of any kind, not even noise to drop, in 19 s suggests a cached or empty run. Someone should open `review-batch.md` and confirm the three passes actually reviewed this diff before treating T4 as green.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Confirm the affected-file prior-art audit across merged and closed/rejected work — the supplied repository has one synthetic base commit and no remotes, and the brief's issue references plus aggregate review log cannot establish that audit (reviewer-prior-art.log:5, brief.md:91, gate-logs/T4-batch-review.log:10).. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
