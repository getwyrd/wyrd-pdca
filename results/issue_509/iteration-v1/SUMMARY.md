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

Review of issue 509: implement S3 bulk `DeleteObjects` so signed `POST /bucket?delete` requests delete all named keys and return a conforming per-key XML result.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract distinguishes present/absent keys, quiet output, malformed and oversized input, entity decoding, and post-delete reads, giving independently observable outcomes at `crates/server/tests/s3_delete_objects.rs:160`. |
| C2 Reproduction (red pre-fix) | PASS | On dependency #507's base with only the new test added, all 5 cases fail through the existing 501 `NotImplemented` response, confirming the missing bucket operation at `crates/server/tests/s3_delete_objects.rs:202`. |
| C3 Change | PASS | The change remains within the agreed wire-layer and integration-test scope, with bounded request handling at `crates/gateway-s3/src/lib.rs:1063` and no dependency, storage, or core-seam change. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused red→green plus independent fmt/clippy is sufficient without rerunning the aggregate gate — all 5 wire tests changed from failing to passing at `crates/server/tests/s3_delete_objects.rs:163`, but the asserted `./engine/xtask.sh ci` script was absent from the supplied target/clone, so its full checks remain provisional. |
| C5 Causal adequacy | PASS | The missing bucket-scoped operation is implemented directly before the denylist at `crates/gateway-s3/src/lib.rs:1902`; no capability probe or runtime symptom guard was introduced. |
| T1 Structure | PASS | Production behavior is localized to the existing S3 dispatcher/module and exercised through a dedicated server integration test at `crates/server/tests/s3_delete_objects.rs:1`, preserving crate seams. |
| T2 Shape | PASS | The implementation has explicit 1000-key/2 MiB bounds and rejects invalid input before deletion at `crates/gateway-s3/src/lib.rs:1416`, matching the required request/response shape. |
| T3 Runtime | PASS | The independently run loopback suite passes all 5 cases, including stock-SDK signing, idempotent deletion, quiet output, malformed/over-limit rejection, entity round-trip, and 404 readback at `crates/server/tests/s3_delete_objects.rs:202`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the available prior-art search is complete — affected-path `git log --all` found merged history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but available refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution. |
| T5 Judgment | PASS | The additive route reuses the established delete seam, preserves other `?delete` refusals, bounds buffering, and exercises the real SDK wire path; the tradeoff is proportionate at `crates/gateway-s3/src/lib.rs:1359`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process stock-SDK coverage is sufficient evidence for the user-facing recursive-delete workflows — it proves protocol behavior at `crates/server/tests/s3_delete_objects.rs:202`, while real `aws s3 rm --recursive` / `aws s3 sync --delete` acceptance remains off-Check. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether focused red→green plus independent fmt/clippy is sufficient without rerunning the aggregate gate — all 5 wire tests changed from failing to passing at `crates/server/tests/s3_delete_objects.rs:163`, but the asserted `./engine/xtask.sh ci` script was absent from the supplied target/clone, so its full checks remain provisional.
- [ ] T4 Contribution — Decide whether the available prior-art search is complete — affected-path `git log --all` found merged history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but available refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution.
- [ ] Validation — fitness-to-purpose — Decide whether the in-process stock-SDK coverage is sufficient evidence for the user-facing recursive-delete workflows — it proves protocol behavior at `crates/server/tests/s3_delete_objects.rs:202`, while real `aws s3 rm --recursive` / `aws s3 sync --delete` acceptance remains off-Check.

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus independent fmt/clippy is sufficient without rerunning the aggregate gate — all 5 wire tests changed from failing to passing at `crates/server/tests/s3_delete_objects.rs:163`, but the asserted `./engine/xtask.sh ci` script was absent from the supplied target/clone, so its full checks remain provisional.; T4 Contribution — Decide whether the available prior-art search is complete — affected-path `git log --all` found merged history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but available refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution.
- By / date: auto-iterate / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
