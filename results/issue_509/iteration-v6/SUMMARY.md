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

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] leaf produced no usable verdict (needs a human) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

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
- Iteration delta (if iterating): Rejected on the advisory review's C3/C5/T2/T3/T5 findings (the reviewer leaf's original failure was infra — a stale codex models cache, since fixed; the review was re-run manually with the same sandbox/contract, verdict table preserved at /tmp/pdca-review-509-manual/check-review.md). Reviewer-confirmed gap, verified in patch.diff: `validate_attributes` (crates/gateway-s3/src/lib.rs, worktree ~:2045-2057) validates each attribute's syntax one-by-one but tracks no attribute names, so a duplicate attribute — e.g. `<Delete x='1' x='2'><Object><Key>victim</Key></Object></Delete>` — is accepted and its keys are deleted, although XML well-formedness (Unique Att Spec) makes the document malformed. Same class as the iteration-4/5 rejections: a body that must answer 400 MalformedXML can still authorize a deletion, so the patch's own stated invariant ("any body not fully validated is MalformedXml and touches no key") is not yet met. Fix: enforce attribute-name uniqueness within a single tag in `validate_attributes` (track seen names per tag; a repeated name is MalformedXml), keeping the validate-don't-discard discipline so this closes the class, not the instance. Extend coverage: a parser unit test rejecting a duplicate-attribute document, and a wire test asserting 400 + MalformedXML AND that the named key survives, so red→green covers it. Do not change the overall approach — the in-crate parser (no new dependency), the POST /bucket?delete interception before the subresource denylist, per-key idempotent semantics, and the byte/key bounds are reviewer-confirmed sound (C1/C2/T1 PASS; the reviewer independently reproduced 0/9 red on the base).
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
