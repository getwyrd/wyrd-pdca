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

Task under review: implement S3 bulk `DeleteObjects` on `POST /bucket?delete`, including strict malformed-XML rejection, bounded requests, idempotent per-key results, and quiet/entity handling.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is concrete and falsifiable across SDK success, idempotency, quiet mode, malformed documents, key/body bounds, and entity decoding; the stock-SDK wire oracle is exercised at `crates/server/tests/s3_delete_objects.rs:225`. |
| C2 Reproduction (red pre-fix) | PASS | With only the production fix reversed in a scratch clone, the retained wire suite reproduced 0/8 passing: the base returned 501 `NotImplemented` (and the oversized upload hit the resulting early-close `BrokenPipe`), grounding the pre-fix symptom at `crates/server/tests/s3_delete_objects.rs:225`. |
| C3 Change | FAIL | Decide whether accepting syntactically invalid XML is compatible with the promised `MalformedXML` contract — the tokenizer reduces an end tag to its first whitespace-delimited token, so `</Key garbage>` is accepted as `</Key>` and can authorize deletion instead of returning 400 at `crates/gateway-s3/src/lib.rs:1993`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused 0/8→8/8 plus clean `cargo fmt --check` and clippy is sufficient — both asserted aggregate/red→green wrapper scripts are absent from the target, so those reported gates could not be independently rerun; focused green begins at `crates/server/tests/s3_delete_objects.rs:225`. |
| C5 Causal adequacy | FAIL | The route fixes the original 501, but the success criterion requires malformed XML to be rejected and the hand scanner still treats arbitrary closing-tag suffixes as discarded attributes, leaving a deletion-authorizing validation gap at `crates/gateway-s3/src/lib.rs:2045`. |
| T1 Structure | PASS | The change stays within the S3 adapter and its wire integration test, preserving the existing gateway deletion seam; the SDK-level contribution boundary is visible at `crates/server/tests/s3_delete_objects.rs:235`. |
| T2 Shape | FAIL | The parser's event shape discards all tag syntax after the first token, so it cannot distinguish a valid close tag from malformed close-tag content; that representation loses information required by the contract at `crates/gateway-s3/src/lib.rs:1990`. |
| T3 Runtime | FAIL | Although all 8 supplied loopback tests pass, malformed inputs beyond their cases can reach the delete fan-out because malformed closing tags are normalized rather than rejected; the current malformed wire coverage is only the plain non-XML case at `crates/server/tests/s3_delete_objects.rs:326`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution. |
| T5 Judgment | FAIL | The remaining strict-document gap has destructive impact: a request that should be rejected can still supply a key for deletion, so accepting the minimal parser tradeoff is unsafe at `crates/gateway-s3/src/lib.rs:2045`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the feature is fit for real S3 clients after malformed-tag rejection is made strict — wire happy paths and named regressions pass, but destructive behavior demands sign-off against adversarial malformed documents at `crates/server/tests/s3_delete_objects.rs:338`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether focused 0/8→8/8 plus clean `cargo fmt --check` and clippy is sufficient — both asserted aggregate/red→green wrapper scripts are absent from the target, so those reported gates could not be independently rerun; focused green begins at `crates/server/tests/s3_delete_objects.rs:225`.
- [ ] T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution.
- [ ] Validation — fitness-to-purpose — Decide whether the feature is fit for real S3 clients after malformed-tag rejection is made strict — wire happy paths and named regressions pass, but destructive behavior demands sign-off against adversarial malformed documents at `crates/server/tests/s3_delete_objects.rs:338`.

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
- Iteration delta (if iterating): Rejected on the §5 reviewer findings (C3/C5/T2/T3/T5): the hand tokenizer applies first-whitespace-token reduction (`local_name`, `split_whitespace().next()`) to END tags, so `</Key garbage>` is accepted as `</Key>` — a malformed document such as `<Delete><Object><Key>victim</Key garbage></Object></Delete>` deletes `victim` instead of answering 400 MalformedXML. Same destructive class as the iteration-4 rejection: a body that must be rejected can still authorize a deletion. Fix: make end-tag lexing strict — an end tag is exactly `</` Name optional-whitespace `>`; any other content after the name is MalformedXML (per the XML ETag production). Review whether start-tag suffixes need the same strictness (start tags may carry attributes, but arbitrary junk like `<Delete garbage>` should not silently pass); prefer validating tag syntax over discarding it, so this stops being a per-iteration whack-a-mole. Extend the malformed-body wire tests with the `</Key garbage>` case asserting 400 + MalformedXML AND that the named key survives, plus matching parser unit tests, so red→green covers it. Do not change the overall approach — in-crate parser (no new dependency), routing, per-key idempotent semantics, byte/key bounds are all fine and reviewer-confirmed.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
