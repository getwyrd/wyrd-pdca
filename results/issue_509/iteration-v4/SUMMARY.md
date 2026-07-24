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

Task under review: implement S3 bulk `DeleteObjects` for signed `POST /bucket?delete`, including idempotent per-key results, quiet mode, XML validation/unescaping, and request bounds.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is concrete and falsifiable at the stock-SDK/wire surface, including absent keys, quiet mode, malformed XML, entity escaping, and bounds (`crates/server/tests/s3_delete_objects.rs:210`). |
| C2 Reproduction (red pre-fix) | PASS | Independently running the six wire tests on the issue-507 base with only the new test present produced 0/6 passing (the pre-fix route returned 501, with the oversized upload ending in BrokenPipe), while the asserted success begins at `crates/server/tests/s3_delete_objects.rs:220`. |
| C3 Change | FAIL | Malformed XML with a second `<Delete>` root or non-whitespace text outside the root is accepted and can delete keys: an empty stack permits every later `Delete` as another root and outside-root text is discarded (`crates/gateway-s3/src/lib.rs:2127`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the independently confirmed focused 0/6→6/6 plus the aggregate gate through workspace tests is sufficient — `cargo xtask ci` then stopped only because `cargo deny` could not lock the host's read-only advisory database, so the full green gate was not reproduced (`crates/server/tests/s3_delete_objects.rs:210`). |
| C5 Causal adequacy | FAIL | The routing cause is removed, but accepting a multi-root document contradicts the required `MalformedXML` behavior and may execute deletes from invalid input (`crates/gateway-s3/src/lib.rs:2130`). |
| T1 Structure | PASS | The bucket-only POST interception occurs within the existing bucket-scoped dispatch before the denylist, preserving the object route and other bucket subresources (`crates/gateway-s3/src/lib.rs:1477`). |
| T2 Shape | PASS | The change remains inside the S3 adapter plus wire tests, adds no dependency or gateway-core/storage seam, and bounds buffered bodies at the handler boundary (`crates/gateway-s3/src/lib.rs:1757`). |
| T3 Runtime | FAIL | A signed body such as `<Delete></Delete><Delete><Object><Key>victim</Key></Object></Delete>` reaches deletion instead of 400 because root completion is not terminal (`crates/gateway-s3/src/lib.rs:2143`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found gateway history and no history for the new test, but the supplied local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution (`crates/gateway-s3/src/lib.rs:1`). |
| T5 Judgment | FAIL | The hand-written XML scanner's document-level validation is too permissive for a destructive endpoint, and the malformed-body test covers only plain non-XML text rather than multiple roots/trailing content (`crates/server/tests/s3_delete_objects.rs:310`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the implemented interoperability is fit for recursive/sync deletion after strict malformed-document rejection is added — the stock SDK paths pass, but destructive acceptance of invalid XML remains consequential (`crates/server/tests/s3_delete_objects.rs:220`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether the independently confirmed focused 0/6→6/6 plus the aggregate gate through workspace tests is sufficient — `cargo xtask ci` then stopped only because `cargo deny` could not lock the host's read-only advisory database, so the full green gate was not reproduced (`crates/server/tests/s3_delete_objects.rs:210`).
- [x] T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found gateway history and no history for the new test, but the supplied local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution (`crates/gateway-s3/src/lib.rs:1`).
- [ ] Validation — fitness-to-purpose — Decide whether the implemented interoperability is fit for recursive/sync deletion after strict malformed-document rejection is added — the stock SDK paths pass, but destructive acceptance of invalid XML remains consequential (`crates/server/tests/s3_delete_objects.rs:220`).

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
- Iteration delta (if iterating): Reviewer-confirmed parser bug (verified in patch.diff): `parse_delete` in crates/gateway-s3/src/lib.rs accepts multi-root documents — after the first `<Delete>` root closes, the stack is empty, so a second `<Delete>` root passes the root check and its keys are deleted (e.g. `<Delete></Delete><Delete><Object><Key>victim</Key></Object></Delete>` executes instead of 400 MalformedXML); non-whitespace text outside the root is silently discarded instead of rejected. Fix: make root completion terminal — any token after the root element closes (a second root, or non-whitespace text) is MalformedXML. Extend the malformed-body wire test with multi-root and trailing-content cases so the red→green gate covers this. Do not change the overall approach (in-crate parser, routing, per-key semantics are fine); §6 C4 and T4 were cleared by the human — only the strict-document validation gap blocks acceptance.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
