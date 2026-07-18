# Result — issue 505 / sigv4-aws-chunked-trailer-framing

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the SigV4 layer accepts only the classic streaming sentinel
- Success criterion: a `STREAMING-UNSIGNED-PAYLOAD-TRAILER` PUT and a
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical change in `crates/gateway-s3`: (a) `streaming.rs` — extend the

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

Review of issue #505: fully consume and verify SigV4 `aws-chunked` checksum-trailer PUT framing so current SDK uploads round-trip without weakening fail-closed authentication.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding decision is explicit: accept both trailer sentinels only with complete framing, signature, checksum, declaration, and decoded-length validation, while preserving classic streaming behavior (`brief.md:9`). |
| C2 Reproduction (red pre-fix) | PASS | A scratch archive of target `HEAD` plus the base-compatible wire test compiled and failed with 403 versus expected 200 at `crates/server/tests/s3_streaming_trailer.rs:463`, directly reproducing the stated refusal. |
| C3 Change | PASS | The relevant impact is confined to trailer admission, checksum calculation, decoder validation/error mapping, and tests; the object-write seam remains unchanged, with admission grounded at `crates/gateway-s3/src/sigv4.rs:502` and fail-closed consumption at `crates/gateway-s3/src/streaming.rs:390`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the independently confirmed focused red→green despite the aggregate gate's host-only deny failure — the base wire test failed 403→200 at `crates/server/tests/s3_streaming_trailer.rs:463`, patched tests passed 11/11, and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not lock the read-only advisory database. |
| C5 Causal adequacy | PASS | The fix removes the decoder limitation that caused closed-set refusal by consuming and validating trailer bytes, rather than adding a capability probe or bypass guard (`crates/gateway-s3/src/streaming.rs:342`, `crates/gateway-s3/src/sigv4.rs:547`). |
| T1 Structure | PASS | The change stays within the brief's gateway surface and adds the classifier-recognized integration test file at `crates/server/tests/s3_streaming_trailer.rs:1`; the pre-existing object gateway boundary is not altered. |
| T2 Shape | PASS | The chosen shape preserves streaming and bounds both chunk and trailer buffering, so accepting trailer framing does not introduce an unbounded pre-auth body buffer (`crates/gateway-s3/src/streaming.rs:292`, `crates/gateway-s3/src/streaming.rs:368`). |
| T3 Runtime | PASS | The in-process runtime evidence passed 11/11, including both sentinels, crc32/crc32c/sha256 byte-identical GETs, forged signatures, checksum tampering, malformed trailers, garbage, and decoded-length mismatch (`crates/server/tests/s3_streaming_trailer.rs:439`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected work duplicates this contribution — affected-path `git log --all` found only original gateway commit `47bd35c` and no trailer consumer, but closed/rejected PR state was not mechanically available (`crates/gateway-s3/src/streaming.rs:390`). |
| T5 Judgment | PASS | The security/compatibility tradeoff is resolved fail-closed: unsupported declarations are rejected before body admission and accepted bodies require authenticated trailers where signed plus checksum equality before publication (`crates/gateway-s3/src/sigv4.rs:564`, `crates/gateway-s3/src/streaming.rs:438`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process wire client adequately represents current stock SDK framing for release fitness — it proves protocol-level red→green and all binding failure edges, while the brief explicitly leaves a real default-configured `aws s3 cp` run supplementary (`crates/server/tests/s3_streaming_trailer.rs:434`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite the aggregate gate's host-only deny failure — the base wire test failed 403→200 at `crates/server/tests/s3_streaming_trailer.rs:463`, patched tests passed 11/11, and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not lock the read-only advisory database.
- [x] T4 Contribution — Decide whether closed/rejected work duplicates this contribution — affected-path `git log --all` found only original gateway commit `47bd35c` and no trailer consumer, but closed/rejected PR state was not mechanically available (`crates/gateway-s3/src/streaming.rs:390`).
- [x] Validation — fitness-to-purpose — Decide whether the in-process wire client adequately represents current stock SDK framing for release fitness — it proves protocol-level red→green and all binding failure edges, while the brief explicitly leaves a real default-configured `aws s3 cp` run supplementary (`crates/server/tests/s3_streaming_trailer.rs:434`).

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
