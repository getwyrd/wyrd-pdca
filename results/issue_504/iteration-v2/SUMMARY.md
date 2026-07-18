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

Review of issue #504: refuse unsupported S3 CopyObject PUTs before they can overwrite destination data with an empty request body.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable and limited to refusing CopyObject while preserving the destination and ordinary PUT behavior, which the wire assertions directly encode (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C2 Reproduction (red pre-fix) | PASS | Independently running the added wire test on the base produced the expected red: the copy-form PUT returned 200 rather than the asserted 501, while the ordinary PUT test passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The unsupported operation is rejected before body streaming begins, so the data-loss path is closed without changing ordinary PUT processing (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently confirmed targeted red→green plus passing diff-check, fmt, and affected-package clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | Dispatching the unsupported CopyObject request away from the body-storage path removes the destructive fall-through itself; this is not an optional-capability probe or an eager-load workaround (`crates/gateway-s3/src/lib.rs:577`). |
| T1 Structure | PASS | The policy guard resides at the gateway method-dispatch boundary and the regression coverage is a server integration test that exercises the production wire path (`crates/gateway-s3/src/lib.rs:563`). |
| T2 Shape | PASS | Header-presence dispatch matches CopyObject's distinguishing request shape regardless of value and leaves the existing no-header PUT path reachable (`crates/gateway-s3/src/lib.rs:577`). |
| T3 Runtime | PASS | In the independently run patched wire test, CopyObject refusal preserved the original bytes and the ordinary PUT round-trip also passed (`crates/server/tests/s3_copy_object_guard.rs:202`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed or rejected forge work supersedes this contribution — affected-path history across available refs and `-S x-amz-copy-source` showed no prior implementation, but closed/rejected forge state was not mechanically available (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | Refusal rather than partial copy implementation respects the stated slice and avoids coupling this data-loss fix to the deferred metadata-dependent copy feature (`crates/gateway-s3/src/lib.rs:574`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether returning 501 for every `x-amz-copy-source` PUT is the acceptable product behavior until full CopyObject support lands — clients are protected from overwrite but copy workflows remain unavailable (`crates/gateway-s3/src/lib.rs:577`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus passing diff-check, fmt, and affected-package clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- [ ] T4 Contribution — Confirm no closed or rejected forge work supersedes this contribution — affected-path history across available refs and `-S x-amz-copy-source` showed no prior implementation, but closed/rejected forge state was not mechanically available (`crates/gateway-s3/src/lib.rs:577`).
- [ ] Validation — fitness-to-purpose — Decide whether returning 501 for every `x-amz-copy-source` PUT is the acceptable product behavior until full CopyObject support lands — clients are protected from overwrite but copy workflows remain unavailable (`crates/gateway-s3/src/lib.rs:577`).

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
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus passing diff-check, fmt, and affected-package clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).; T4 Contribution — Confirm no closed or rejected forge work supersedes this contribution — affected-path history across available refs and `-S x-amz-copy-source` showed no prior implementation, but closed/rejected forge state was not mechanically available (`crates/gateway-s3/src/lib.rs:577`).
- By / date: auto-iterate / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
