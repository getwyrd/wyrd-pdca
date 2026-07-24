# Result — issue 509 / delete-objects-bulk

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: bulk `DeleteObjects` — `POST /bucket?delete` with an XML body of keys —
- Success criterion: against the in-process loopback S3 gateway with several objects
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: route bucket-scoped `POST /bucket?delete` to a bulk handler; parse the

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

Review of issue #509: implement S3 bulk `DeleteObjects` (`POST /bucket?delete`) with bounded XML parsing, idempotent per-key deletion, and `DeleteResult` responses.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is concrete and wire-observable: SDK interoperability, idempotent absent-key handling, quiet results, malformed/over-limit refusal, and escaped-key deletion are each decidable at `crates/server/tests/s3_delete_objects.rs:163`. |
| C2 Reproduction (red pre-fix) | PASS | Independently applying only the new test to wave base `99ef6e6` produced 0/5 passing, with the stock SDK receiving `501 NotImplemented` at `crates/server/tests/s3_delete_objects.rs:208`. |
| C3 Change | FAIL | The XML contract is incomplete: undeclared entities are preserved instead of rejected and numeric character references are not decoded, so malformed input can be accepted and a valid escaped key can address the wrong object at `crates/gateway-s3/src/lib.rs:1236`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused red→green plus fmt/clippy is sufficient without the asserted aggregate wrapper — the wire suite changed from 0/5 to 5/5 at `crates/server/tests/s3_delete_objects.rs:163`, but `./engine/xtask.sh` and `./engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate gate cannot be independently rerun. |
| C5 Causal adequacy | PASS | The change removes the bucket-dispatch cause by routing the supported POST before the denylist, with no capability probe or symptom guard at `crates/gateway-s3/src/lib.rs:1897`. |
| T1 Structure | PASS | The feature stays within the S3 wire adapter and a wire-level server integration test, preserving the existing gateway deletion seam at `crates/gateway-s3/src/lib.rs:1902`. |
| T2 Shape | FAIL | Decide the required XML-conformance boundary before acceptance — the current entity handling contradicts `MalformedXML` semantics and cannot round-trip numeric references at `crates/gateway-s3/src/lib.rs:1247`. |
| T3 Runtime | PASS | The in-process stock-SDK/HTTP suite passed 5/5 after the patch, including deletion read-back, quiet mode, malformed input, the 1000-key bound, and `&amp;` at `crates/server/tests/s3_delete_objects.rs:238`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing gateway history and no history for the new test, but the available local refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution at `crates/gateway-s3/src/lib.rs:1`. |
| T5 Judgment | PASS | The patch remains within the planned additive route/parser/test scope and introduces no new dependency or unrelated policy change at `crates/gateway-s3/src/lib.rs:1063`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the exercised SDK wire behavior and current XML subset are fit for real recursive-delete clients — automated loopback coverage passes, but sign-off must weigh the entity-conformance defect before enabling user-facing bulk deletion at `crates/server/tests/s3_delete_objects.rs:334`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy is sufficient without the asserted aggregate wrapper — the wire suite changed from 0/5 to 5/5 at `crates/server/tests/s3_delete_objects.rs:163`, but `./engine/xtask.sh` and `./engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate gate cannot be independently rerun.
- [ ] T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing gateway history and no history for the new test, but the available local refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution at `crates/gateway-s3/src/lib.rs:1`.
- [ ] Validation — fitness-to-purpose — Decide whether the exercised SDK wire behavior and current XML subset are fit for real recursive-delete clients — automated loopback coverage passes, but sign-off must weigh the entity-conformance defect before enabling user-facing bulk deletion at `crates/server/tests/s3_delete_objects.rs:334`.

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
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy is sufficient without the asserted aggregate wrapper — the wire suite changed from 0/5 to 5/5 at `crates/server/tests/s3_delete_objects.rs:163`, but `./engine/xtask.sh` and `./engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate gate cannot be independently rerun.; T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing gateway history and no history for the new test, but the available local refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution at `crates/gateway-s3/src/lib.rs:1`.
- By / date: auto-iterate / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
