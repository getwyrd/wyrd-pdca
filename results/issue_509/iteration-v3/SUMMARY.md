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

Review of issue 509: implement S3 bulk `DeleteObjects` so signed `POST /bucket?delete` deletes requested keys and returns contract-compliant XML responses.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is testable at the wire: success, idempotent absence, quiet mode, malformed XML, the 1000-key limit, and entity decoding are concretely defined in `brief.md:9`. |
| C2 Reproduction (red pre-fix) | PASS | A scratch checkout of target HEAD plus only the new wire test produced the required assertion-level red: 0/5 tests passed because the base returned 501 at `crates/server/tests/s3_delete_objects.rs:208`. |
| C3 Change | FAIL | Malformed XML must be rejected, but a second top-level `<Delete>` is accepted whenever the stack becomes empty again, and text outside the root is silently discarded; this can delete keys from a document that is not well-formed XML at `crates/gateway-s3/src/lib.rs:2127`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently confirmed focused 0/5→5/5 plus clean fmt/clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` and red→green wrapper are absent from the target, so their broader coverage could not be reproduced; the focused green begins at `crates/server/tests/s3_delete_objects.rs:208`. |
| C5 Causal adequacy | PASS | The change removes the unsupported bucket-POST route and delegates to the existing delete primitive, with no capability probe or runtime symptom guard at `crates/gateway-s3/src/lib.rs:1477`. |
| T1 Structure | PASS | The feature remains inside the S3 adapter and its wire integration test, preserving the gateway-core and storage seams at `crates/gateway-s3/src/lib.rs:1803`. |
| T2 Shape | FAIL | The fixed-schema parser does not enforce one document element despite promising balanced, well-formed input, so its accepted-input shape is broader than the S3 XML contract at `crates/gateway-s3/src/lib.rs:2116`. |
| T3 Runtime | PASS | The applied target passed all five loopback/SDK tests, including actual deletion, quiet output, malformed input, over-limit input, and entity-escaped keys at `crates/server/tests/s3_delete_objects.rs:208`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution. |
| T5 Judgment | PASS | No scope re-entry or unresolved architectural choice is required to judge the patch; the parser defect is mechanically decidable against the explicit malformed-XML criterion at `crates/gateway-s3/src/lib.rs:2120`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this Alpha implementation is fit for real recursive-delete workflows — focused SDK wire behavior passed, but operator acceptance with `aws s3 rm --recursive` and `aws s3 sync --delete` remains outside the automated evidence at `crates/server/tests/s3_delete_objects.rs:208`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether independently confirmed focused 0/5→5/5 plus clean fmt/clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` and red→green wrapper are absent from the target, so their broader coverage could not be reproduced; the focused green begins at `crates/server/tests/s3_delete_objects.rs:208`.
- [ ] T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution.
- [ ] Validation — fitness-to-purpose — Decide whether this Alpha implementation is fit for real recursive-delete workflows — focused SDK wire behavior passed, but operator acceptance with `aws s3 rm --recursive` and `aws s3 sync --delete` remains outside the automated evidence at `crates/server/tests/s3_delete_objects.rs:208`.

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
- Iteration delta (if iterating): Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed focused 0/5→5/5 plus clean fmt/clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` and red→green wrapper are absent from the target, so their broader coverage could not be reproduced; the focused green begins at `crates/server/tests/s3_delete_objects.rs:208`.; T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution.
- By / date: auto-iterate / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
