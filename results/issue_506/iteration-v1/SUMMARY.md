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

Task under review: implement S3 HeadObject so signed HEAD requests return GET-equivalent metadata without reading object data, with headers-only 200/404 wire responses.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the signed HTTP seam: stored and absent keys have explicit status, header, body, and non-regression outcomes exercised at `crates/server/tests/s3_head_object.rs:195`. |
| C2 Reproduction (red pre-fix) | PASS | On the folded target base with only the new test added, all three wire tests failed with HEAD returning 405 rather than the required 200/404 at `crates/server/tests/s3_head_object.rs:231`. |
| C3 Change | PASS | The review decision is whether the new protocol behavior stays metadata-only; the production lookup reads the committed inode without opening its chunk map or stream at `crates/server/src/lib.rs:380`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Install and run `cargo machete`, then decide whether its unused-dependency result clears the full CI claim — focused red→green, fmt, Clippy, build, workspace tests, three cargo-deny audits, conformance, statics, and DST passed, but this host lacks that asserted scanner; wire assertions are at `crates/server/tests/s3_head_object.rs:231`. |
| C5 Causal adequacy | PASS | The missing dispatch capability is added directly, with no capability probe or runtime fallback masking an eager side effect; HEAD selects the metadata seam at `crates/gateway-s3/src/lib.rs:705`. |
| T1 Structure | PASS | The architectural decision is whether HEAD belongs on the protocol-neutral object seam; a distinct metadata result avoids coupling the wire handler to a body stream at `crates/gateway-core/src/lib.rs:62`. |
| T2 Shape | PASS | The test is a discoverable integration target under `crates/server/tests/` and drives the public signed loopback path, with the success oracle beginning at `crates/server/tests/s3_head_object.rs:195`. |
| T3 Runtime | PASS | Independently isolated execution observed base 405 failures and patched 200/404 success; the green run passed all three runtime cases, including unchanged GET/PUT/DELETE behavior at `crates/server/tests/s3_head_object.rs:296`. |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path and all-local-ref searches found no `head_object` implementation, but the supplied artifacts contain no closed/rejected PR metadata; the contribution seam is at `crates/gateway-core/src/lib.rs:167`. |
| T5 Judgment | PASS | The scope decision is proportionate to the behavioral gap: one metadata seam, one dispatch arm, its implementer, and wire tests restore HEAD without conditional/range/HeadBucket expansion at `crates/gateway-s3/src/lib.rs:697`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process signed HTTP/1.1 loopback is sufficient evidence for real AWS CLI/SDK interoperability — it proves status, body suppression, metadata parity, and operation non-regression at `crates/server/tests/s3_head_object.rs:195`, but no real client was exercised. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Install and run `cargo machete`, then decide whether its unused-dependency result clears the full CI claim — focused red→green, fmt, Clippy, build, workspace tests, three cargo-deny audits, conformance, statics, and DST passed, but this host lacks that asserted scanner; wire assertions are at `crates/server/tests/s3_head_object.rs:231`.
- [ ] T4 Contribution — Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path and all-local-ref searches found no `head_object` implementation, but the supplied artifacts contain no closed/rejected PR metadata; the contribution seam is at `crates/gateway-core/src/lib.rs:167`.
- [ ] Validation — fitness-to-purpose — Decide whether the in-process signed HTTP/1.1 loopback is sufficient evidence for real AWS CLI/SDK interoperability — it proves status, body suppression, metadata parity, and operation non-regression at `crates/server/tests/s3_head_object.rs:195`, but no real client was exercised.

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Install and run `cargo machete`, then decide whether its unused-dependency result clears the full CI claim — focused red→green, fmt, Clippy, build, workspace tests, three cargo-deny audits, conformance, statics, and DST passed, but this host lacks that asserted scanner; wire assertions are at `crates/server/tests/s3_head_object.rs:231`.; T4 Contribution — Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path and all-local-ref searches found no `head_object` implementation, but the supplied artifacts contain no closed/rejected PR metadata; the contribution seam is at `crates/gateway-core/src/lib.rs:167`.
- By / date: auto-iterate / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
