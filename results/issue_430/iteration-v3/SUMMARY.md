# Result — issue 430 / fragment-identity-validation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The shared read/repair validation accepts a decoded fragment on
- Success criterion: A store that returns a validly-encoded fragment of the SAME
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical fix — the shared validation boundary in `crates/core`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #430: reject fragments whose decoded index or EC tuple does not match the committed fragment identity before read, repair, or maintenance use.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit and falsifiable: wrong-index and wrong-EC-tuple fragments must never become reconstruction input and must create a repair obligation (`crates/core/tests/fragment_identity.rs:140`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the asserted base failure is acceptable without an independent production-reverted run — the public-surface fixture is deterministic, but the configured `engine/scripts/run-verify.sh` is absent and this artifact-only review could not stash/revert the target (`crates/core/tests/fragment_identity.rs:146`). |
| C3 Change | PASS | The shared admission predicate now covers chunk, requested index, scheme type, and stripe geometry, closing the identified backend-independent integrity boundary (`crates/core/src/repair.rs:58`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused green plus clean fmt/clippy/build is sufficient without a reproduced red and complete CI — all 3 public tests pass, while full `cargo xtask ci` stops only because the host forbids loopback bind, and the asserted wrapper is absent (`crates/core/tests/fragment_identity.rs:151`). |
| C5 Causal adequacy | PASS | The decision is adequately resolved at fragment admission itself: invalid identity is rejected before decoder insertion, with no capability probe or downstream symptom guard (`crates/core/src/read.rs:330`). |
| T1 Structure | PASS | The production boundary is centralized in core and the required public-surface regression suite is isolated in the new test file, matching the intended ownership split (`crates/core/src/repair.rs:58`; `crates/core/tests/fragment_identity.rs:151`). |
| T2 Shape | PASS | The widened helper contract carries the exact expected `FragmentId` and committed `EcScheme`, so callers cannot validate against chunk id alone (`crates/core/src/repair.rs:95`). |
| T3 Runtime | PASS | Applied-target execution passed all three wrong-identity cases, including the same-scheme-type RS geometry case that specifically exercises the `ec_k`/`ec_m` comparisons (`crates/core/tests/fragment_identity.rs:321`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history by path shows only earlier chunk-only validation, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`). |
| T5 Judgment | PASS | No ambiguous scope or symptom-vs-root-cause tradeoff remains: the patch changes only the shared validation boundary, its necessary call sites, and fixtures affected by the stricter contract (`crates/custodian/src/scrub.rs:121`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the exercised adversarial in-process stores and host-limited CI provide sufficient operational confidence for the any-backend never-wrong-bytes assurance (`crates/core/tests/fragment_identity.rs:200`). |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the asserted base failure is acceptable without an independent production-reverted run — the public-surface fixture is deterministic, but the configured `engine/scripts/run-verify.sh` is absent and this artifact-only review could not stash/revert the target (`crates/core/tests/fragment_identity.rs:146`).
- [ ] C4 Verification (red→green) — Decide whether focused green plus clean fmt/clippy/build is sufficient without a reproduced red and complete CI — all 3 public tests pass, while full `cargo xtask ci` stops only because the host forbids loopback bind, and the asserted wrapper is absent (`crates/core/tests/fragment_identity.rs:151`).
- [ ] T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history by path shows only earlier chunk-only validation, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`).
- [ ] Validation — fitness-to-purpose — Decide whether the exercised adversarial in-process stores and host-limited CI provide sufficient operational confidence for the any-backend never-wrong-bytes assurance (`crates/core/tests/fragment_identity.rs:200`).
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

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
- Iteration delta (if iterating): Not a rejection of the approach — gates were green (C4-ci pass, C4-verify red→green pass) and the main review passed C1/C3/C5/T1/T2/T3/T5. Rejected because the §6 items could not be cleared this session: the adversary advisory leaf produced no artifact (substantive gap), and an independent re-run of the verification was requested at sign-off but could not be performed (sign-off host shell failure). Next pass: re-run the adversary leaf so findings exist, keep the deterministic C4-verify/xtask-ci evidence visible to the reviewer, and surface T4 (no closed/rejected upstream PR covers these paths) and validation fitness-to-purpose for the human with whatever remote-PR evidence can be gathered. Do not rework the identity-validation predicate itself (chunk + index + scheme type + stripe geometry at the shared admission boundary) — it was not faulted.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
