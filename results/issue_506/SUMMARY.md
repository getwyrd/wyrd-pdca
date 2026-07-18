# Result — issue 506 / head-object

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `HeadObject` (`HTTP HEAD /bucket/key`) returns **405 MethodNotAllowed** —
- Success criterion: through the real wire path, a signed `HEAD /bucket/key` for a
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: add a `HEAD` arm to the object dispatch that resolves the key's **metadata

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Review of issue 506: implement signed S3 HeadObject so stored objects return GET-equivalent metadata without a body and missing keys return headers-only 404.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the real signed HTTP seam: status, empty body, GET-equivalent metadata, absent-key behavior, and unchanged PUT/GET/DELETE are asserted at `crates/server/tests/s3_head_object.rs:183`. |
| C2 Reproduction (red pre-fix) | PASS | In an isolated target clone with the production changes stashed and the new wire test retained, the focused test failed with actual status 405 versus expected 200 at `crates/server/tests/s3_head_object.rs:231`. |
| C3 Change | PASS | The change stays within the required object seam, S3 dispatch, concrete gateway, and wire test; the production lookup is metadata-only at `crates/server/src/lib.rs:380`. |
| C4 Verification (red→green) | PASS | Restoring the patch changed the focused wire suite from the reproduced 405 failure to 3/3 passing; workspace fmt/clippy/build/tests, cargo-machete, cargo-deny, conformance, statics, and DST also passed independently, with the success assertions grounded at `crates/server/tests/s3_head_object.rs:231`. |
| C5 Causal adequacy | PASS | The missing dispatch capability is added directly and resolves through a metadata-only seam rather than a probe, fallback guard, or data stream at `crates/gateway-s3/src/lib.rs:705` and `crates/gateway-core/src/lib.rs:167`. |
| T1 Structure | PASS | The wire layer remains generic over `ObjectGateway`, while storage-specific inode resolution stays in the concrete gateway at `crates/gateway-core/src/lib.rs:172` and `crates/server/src/lib.rs:380`. |
| T2 Shape | PASS | A body-free `ObjectMeta` type makes the metadata-only contract explicit and prevents callers from manufacturing or discarding a stream at `crates/gateway-core/src/lib.rs:62`. |
| T3 Runtime | PASS | The loopback suite observed 200 with matching metadata and no body, 404 with no body, and unchanged GET/PUT/DELETE behavior at `crates/server/tests/s3_head_object.rs:231`, `crates/server/tests/s3_head_object.rs:282`, and `crates/server/tests/s3_head_object.rs:320`. |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path merged/all-local-ref history contains no prior `head_object`, but the supplied artifacts and target Git metadata do not expose closed/rejected PR records; the new contribution seam is at `crates/gateway-core/src/lib.rs:172`. |
| T5 Judgment | PASS | The patch requires no capability probe, scope re-entry, visual judgment, or external dependency; the only policy decision is the explicitly specified metadata-only HEAD contract at `crates/gateway-s3/src/lib.rs:705`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the observed signed loopback semantics are sufficient release evidence for real S3 clients — the automated wire assertions cover the contract at `crates/server/tests/s3_head_object.rs:231`, but product fitness remains a sign-off judgment. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path merged/all-local-ref history contains no prior `head_object`, but the supplied artifacts and target Git metadata do not expose closed/rejected PR records; the new contribution seam is at `crates/gateway-core/src/lib.rs:172`.
- [x] Validation — fitness-to-purpose — Decide whether the observed signed loopback semantics are sufficient release evidence for real S3 clients — the automated wire assertions cover the contract at `crates/server/tests/s3_head_object.rs:231`, but product fitness remains a sign-off judgment.
- [x] external dependency remains: `cargo-machete` was the only tool gap and it is now

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
- issue_506: SUMMARY assembler clipped a build-notes sentence mid-line and dropped its leading "No", turning "No NEEDS-HUMAN external dependency remains…" into a spurious §6 item — fix line extraction/truncation.
