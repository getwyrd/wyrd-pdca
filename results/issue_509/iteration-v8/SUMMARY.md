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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: patch.diff does not apply on origin/main — the bundle is stale; rebase Do.
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

Review of issue 509: implement signed S3 `DeleteObjects` bulk deletion with strict fail-closed XML handling, bounded input, idempotent per-key results, and quiet-mode output.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is concrete and falsifiable across success, idempotency, quiet mode, malformed input, limits, escaping, and post-delete reads (`brief.md:9`). |
| C2 Reproduction (red pre-fix) | PASS | The retained wire suite independently ran 0/11 on the production base, with bucket-only POST returning `InvalidRequest` instead of DeleteObjects behavior; the discriminator begins at `crates/server/tests/s3_delete_objects.rs:252`. |
| C3 Change | FAIL | A malformed processing instruction can still authorize deletion: `skip_pi` accepts any bytes through the next `?>` without requiring a PI target/name or enforcing the reserved `xml` rules, and it is admitted both around and inside the root (`crates/gateway-s3/src/lib.rs:1486`, `crates/gateway-s3/src/lib.rs:1560`, `crates/gateway-s3/src/lib.rs:1712`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently reproduced focused 0/11→11/11 plus 6/6 parser tests is sufficient without the aggregate wrapper — `engine/xtask.sh` and `engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate pass cannot be rerun; focused green starts at `crates/server/tests/s3_delete_objects.rs:252`. |
| C5 Causal adequacy | FAIL | The fail-closed root cause remains unresolved: the scanner claims document well-formedness yet skips PI grammar wholesale, so inputs such as `<? ?><Delete><Object><Key>victim</Key></Object></Delete>` can reach deletion instead of `MalformedXML` (`crates/gateway-s3/src/lib.rs:1712`). |
| T1 Structure | PASS | The routing, bounded buffer, parser, rendering, and wire coverage remain localized to the S3 gateway and its integration test, with no seam or dependency expansion (`crates/gateway-s3/src/lib.rs:812`, `crates/server/tests/s3_delete_objects.rs:1`). |
| T2 Shape | FAIL | The accepted-input shape is broader than well-formed XML because processing instructions are terminator-scanned rather than grammar-validated (`crates/gateway-s3/src/lib.rs:1713`). |
| T3 Runtime | FAIL | The impact is destructive: keys are collected before execution and every collected key is then passed to `delete_object`, so an accepted malformed-PI body can remove objects (`crates/gateway-s3/src/lib.rs:1192`, `crates/gateway-s3/src/lib.rs:1702`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether merged-history plus closed-PR text search is adequate prior-art coverage — affected-path `git log --all` found gateway history and none for the new test, while the available GitHub search returned no closed/rejected DeleteObjects work but cannot mechanically prove affected-file coverage (`crates/gateway-s3/src/lib.rs:1`). |
| T5 Judgment | FAIL | The safety decision owed is whether to retain a bespoke XML scanner after repeated same-class omissions; another unvalidated XML production permits malformed destructive requests, so acceptance should wait for a complete grammar audit or a vetted parser (`crates/gateway-s3/src/lib.rs:1362`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the feature is fit for real recursive-delete clients after the malformed-PI safety defect is fixed — focused SDK/HTTP tests exercise the loopback stack, but the brief reserves AWS CLI round-trips for human acceptance (`crates/server/tests/s3_delete_objects.rs:252`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether independently reproduced focused 0/11→11/11 plus 6/6 parser tests is sufficient without the aggregate wrapper — `engine/xtask.sh` and `engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate pass cannot be rerun; focused green starts at `crates/server/tests/s3_delete_objects.rs:252`.
- [ ] T4 Contribution — Decide whether merged-history plus closed-PR text search is adequate prior-art coverage — affected-path `git log --all` found gateway history and none for the new test, while the available GitHub search returned no closed/rejected DeleteObjects work but cannot mechanically prove affected-file coverage (`crates/gateway-s3/src/lib.rs:1`).
- [ ] Validation — fitness-to-purpose — Decide whether the feature is fit for real recursive-delete clients after the malformed-PI safety defect is fixed — focused SDK/HTTP tests exercise the loopback stack, but the brief reserves AWS CLI round-trips for human acceptance (`crates/server/tests/s3_delete_objects.rs:252`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected on a fifth consecutive same-class, destructive defect in the hand-rolled XML scanner: `skip_pi` scans to the next `?>` without validating the PI target (`<? ?>` has no PITarget), so `<? ?><Delete>…<Key>victim</Key>…</Delete>` parses successfully and DELETES the key instead of answering 400 MalformedXML (advisory §5: C3/C5/T2/T3/T5 FAIL, patch.diff crates/gateway-s3/src/lib.rs:715-723). This is not a Do-quality failure this round — the previous four holes (multi-root, junk-after-tag, duplicate-attribute, `<`-in-attribute-value) were each closed correctly. The APPROACH is the root cause: hand-reimplementing XML well-formedness as a growing set of point checks, with completeness proven by adversarial example, on a code path where any missed production destroys data. Each fix closes one production and leaves the next unaudited (this round it was the PI target; the `skip_pi` "scan to terminator, don't validate" shape is the same leniency the earlier rounds were meant to eliminate). Do NOT re-plan another hole-by-hole tokenizer. Plan should reconsider the parser approach per the iteration-7 carry-forward (brief.md:153): either (a) adopt a vetted XML parser — the human-gated dependency decision the brief deferred (ADR-0003 three-test audit + deny.toml) — or (b) restructure so the scanner is a provably total well-formedness pass over a CLOSED production set, not a pile of point checks; and/or (c) shrink the trust surface so parser leniency cannot authorize a destructive delete. The routing split, per-key idempotent semantics, byte/key bounds, and the wire test harness are reviewer-confirmed sound (C1/C2/T1 PASS) — the re-plan is scoped to the XML validation strategy, not the whole feature. Note: C4-verify also failed as stale (patch.diff does not apply on origin/main because 509 sits on 507's routing split); the next attempt must rebase onto the folded base so red→green can actually be reproduced.
- By / date: Eduard Ralph / 2026-07-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
