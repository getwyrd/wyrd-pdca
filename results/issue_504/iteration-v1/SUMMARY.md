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

Review of issue #504: refuse unsupported S3 CopyObject-form PUT requests before they can overwrite destination data with an empty body.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: CopyObject-form PUT must return S3 `NotImplemented` without mutation, while ordinary PUT remains supported (`crates/server/tests/s3_copy_object_guard.rs:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent scratch rerun without the gateway hunk failed at the expected 200-versus-501 assertion, while the ordinary-PUT control passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The change is confined to rejecting the unsupported header before body consumption, so the data-loss path is closed without claiming CopyObject support (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether targeted red→green plus passing fmt/clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | The request is explicitly unsupported, so refusing its discriminating header before the ordinary PUT writer is the invariant-restoring boundary, not a capability probe masking an eager/load-time cause (`crates/gateway-s3/src/lib.rs:577`). |
| T1 Structure | PASS | A separate server integration test exercises the production signed HTTP path and keeps the regression at the externally observable boundary (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| T2 Shape | PASS | The oracle distinguishes refusal status/error code, byte-identical destination survival, and unaffected ordinary PUT behavior, preventing a superficial status-only pass (`crates/server/tests/s3_copy_object_guard.rs:191`). |
| T3 Runtime | PASS | Independent patched execution completed both loopback runtime tests successfully, including the destructive-path regression and ordinary-PUT control (`crates/server/tests/s3_copy_object_guard.rs:220`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected remote work supersedes this contribution — affected-path merged/local-ref history and `-S x-amz-copy-source` found no prior implementation, but closed/rejected forge state was unavailable mechanically (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | The refusal is proportionate to the immediate data-loss risk and leaves full server-side copy outside this patch, avoiding ambiguous scope expansion (`crates/gateway-s3/src/lib.rs:574`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether temporary `501 NotImplemented` is acceptable product behavior until full CopyObject support lands — it prevents corruption but deliberately declines a standard S3 operation (`crates/gateway-s3/src/lib.rs:580`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether targeted red→green plus passing fmt/clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- [ ] T4 Contribution — Confirm no closed/rejected remote work supersedes this contribution — affected-path merged/local-ref history and `-S x-amz-copy-source` found no prior implementation, but closed/rejected forge state was unavailable mechanically (`crates/gateway-s3/src/lib.rs:577`).
- [ ] Validation — fitness-to-purpose — Decide whether temporary `501 NotImplemented` is acceptable product behavior until full CopyObject support lands — it prevents corruption but deliberately declines a standard S3 operation (`crates/gateway-s3/src/lib.rs:580`).

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether targeted red→green plus passing fmt/clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).; T4 Contribution — Confirm no closed/rejected remote work supersedes this contribution — affected-path merged/local-ref history and `-S x-amz-copy-source` found no prior implementation, but closed/rejected forge state was unavailable mechanically (`crates/gateway-s3/src/lib.rs:577`).
- By / date: auto-iterate / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
