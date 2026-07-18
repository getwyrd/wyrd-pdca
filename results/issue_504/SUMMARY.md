# Result — issue 504 / copy-object-empty-overwrite-guard

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `CopyObject` (`PUT /dst-bucket/key` with an `x-amz-copy-source` header) is
- Success criterion: a SigV4-signed `PUT /bucket/key` carrying an `x-amz-copy-source`
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the one defect — a PUT carrying `x-amz-copy-source` must not fall through to

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

Review of issue #504: refuse S3 CopyObject-form PUTs before body storage so an unsupported copy cannot erase the destination object.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete: reject the unsupported header form with S3 `NotImplemented`, preserve existing bytes, and leave ordinary PUT behavior intact (`crates/server/tests/s3_copy_object_guard.rs:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent base run reproduced the data-loss path: the preservation test received 200 instead of 501 while the ordinary-PUT control passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The change stays within the specified refusal slice and executes before body consumption, so no server-side-copy or SigV4 policy expansion needs approval (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently confirmed targeted red→green plus clean format and affected-package Clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` entry point is absent from the supplied target and could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | Refusing the distinct request form at dispatch removes the destructive fall-through before storage rather than masking its post-write symptom; no capability-probe smell requires root-cause adjudication (`crates/gateway-s3/src/lib.rs:563`). |
| T1 Structure | PASS | The production guard remains in the existing method-dispatch boundary and the wire regression is a standalone integration test, matching the repository's applicable seams (`crates/gateway-s3/src/lib.rs:563`; `crates/server/tests/s3_copy_object_guard.rs:166`). |
| T2 Shape | PASS | The observable contract is covered at its boundaries: 501 plus S3 error code, byte-identical destination preservation, and an unaffected ordinary PUT control (`crates/server/tests/s3_copy_object_guard.rs:191`). |
| T3 Runtime | PASS | The in-process TCP test exercised the production signing and gateway path; patched execution passed both cases without an external service (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| T4 Contribution | PASS | Affected-path history across all available local/remote refs, pickaxe for `x-amz-copy-source`, and closed GitHub PR searches for both `CopyObject` and the header found no superseding prior or rejected work (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | The refusal is a reversible safety boundary with no ambiguous scope or upstream semantic dependency; full copy semantics remain explicitly outside this decision (`crates/gateway-s3/src/lib.rs:574`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether refusal-until-full-copy is the right product behavior for S3 clients — it prevents data loss but intentionally returns 501 for CopyObject workflows (`crates/gateway-s3/src/lib.rs:580`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus clean format and affected-package Clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` entry point is absent from the supplied target and could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- [x] Validation — fitness-to-purpose — Decide whether refusal-until-full-copy is the right product behavior for S3 clients — it prevents data loss but intentionally returns 501 for CopyObject workflows (`crates/gateway-s3/src/lib.rs:580`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
