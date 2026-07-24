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
- Iteration delta (if iterating): Rejected on the advisory review's C3/C5/T2/T3/T5 findings. The Check reviewer leaf originally failed on infra (a downgraded codex / stale models_cache.json, since fixed); the review was re-run manually with the same decorrelated sandbox/contract (build-notes withheld, grounded on $PDCA_TARGET @ 07d0244), verdict table preserved at /var/tmp/pdca/pdca-reviewer-509-manual/sandbox/check-review.md. Reviewer-confirmed AND independently verified in patch.diff: `validate_attributes` accepts an XML-forbidden literal `<` inside an attribute value — it locates the closing quote (`value.find(quote)`) but never rejects a `<` between the quotes — so `<Delete x='<'><Object><Key>victim</Key></Object></Delete>` parses successfully and deletes `victim` instead of answering 400 MalformedXML. This is the FOURTH instance of the same destructive class the iteration-4/5/6 rejections were about (multi-root -> junk-after-tag -> duplicate-attribute -> now `<`-in-attribute-value): a body that MUST be rejected can still authorize a deletion, so the patch's own stated invariant ("any body the parser does not fully validate is MalformedXml and touches no key") is still not met. Fix, per the reviewer: in `validate_attributes` reject a literal `<` between the attribute quotes as MalformedXml (per the XML `AttValue` production, which also forbids a raw `&` that does not begin a valid reference — check that too), keeping the validate-don't-discard discipline. Do NOT stop at this single instance: this is now a recurring whack-a-mole, so audit the WHOLE `AttValue`/tag grammar in one pass and close the remaining siblings, not just `x='<'`. Extend coverage with a parser unit test AND a wire test asserting 400 MalformedXML AND that the named key survives, so red->green covers it. The overall approach is otherwise reviewer-confirmed sound (C1/C2 PASS — reviewer reproduced 1/10 red on the base; T1/T4 PASS): the POST /bucket?delete interception before the subresource denylist, per-key idempotent semantics, and the byte/key bounds are fine — do not re-architect them here. Note: the reviewer could not re-run the aggregate C4 (`engine/xtask.sh ci` / `run-verify.sh` are not in $PDCA_TARGET, so C4 came back NEEDS-HUMAN), but the bundle's own C4-ci gate passed in the driver environment — the blocker is the parser gap, not the gate. (If the next Do pass produces yet another same-class hole, escalate to iterate-plan: four repeats suggest the hand-rolled permissive tokenizer patched hole-by-hole may be the root problem, and Plan should reconsider it — a real well-formedness pass, or revisiting the human-gated XML-crate decision the brief deferred.)
- By / date: Eduard Ralph / 2026-07-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
