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

Review of issue 504: refuse `CopyObject`-shaped PUT requests before body storage so an unsupported copy cannot erase the destination object.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: return S3 `501 NotImplemented`, preserve existing bytes, and leave ordinary PUT behavior intact (`crates/server/tests/s3_copy_object_guard.rs:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent scratch-base execution failed the core wire test with observed status `200` versus required `501`, while the ordinary-PUT control passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The patch stays within the specified refusal slice and makes the decision before body consumption, avoiding the deferred server-side-copy scope (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently confirmed targeted red→green plus clean fmt, affected-package Clippy, and diff checks is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the target and therefore could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | Refusing the unsupported operation at dispatch removes the destructive fallthrough rather than probing or masking an eager/load-time cause (`crates/gateway-s3/src/lib.rs:577`). |
| T1 Structure | PASS | The real-wire regression is isolated as the requested integration-test crate and covers both the hazardous request and unaffected control (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| T2 Shape | PASS | Header presence, not value parsing or signed-header membership, is the relevant operation discriminator, and it is checked before the storage stream is formed (`crates/gateway-s3/src/lib.rs:577`). |
| T3 Runtime | PASS | Independent patched execution passed both loopback TCP tests, including `501`/S3 error-body assertion, byte-identical destination survival, and ordinary PUT/GET (`crates/server/tests/s3_copy_object_guard.rs:191`). |
| T4 Contribution | PASS | Affected-path history across all local refs, `-S x-amz-copy-source`, and an all-state forge search found no merged, closed, rejected, or open prior implementation (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | The data-loss boundary is unambiguous and the patch neither implements copy nor changes SigV4 semantics, so no additional product or architectural choice is introduced (`crates/gateway-s3/src/lib.rs:572`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether refusing CopyObject with `501` is the acceptable interim client experience — this prevents data loss but intentionally withholds server-side copy until the later metadata-dependent slice (`crates/gateway-s3/src/lib.rs:574`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus clean fmt, affected-package Clippy, and diff checks is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the target and therefore could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- [ ] Validation — fitness-to-purpose — Decide whether refusing CopyObject with `501` is the acceptable interim client experience — this prevents data loss but intentionally withholds server-side copy until the later metadata-dependent slice (`crates/gateway-s3/src/lib.rs:574`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus clean fmt, affected-package Clippy, and diff checks is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the target and therefore could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- By / date: auto-iterate / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
