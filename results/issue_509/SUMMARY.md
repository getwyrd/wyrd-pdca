# Result — issue 509 / delete-objects-bulk

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: bulk `DeleteObjects` — `POST /bucket?delete` with an XML body of keys —
- Success criterion: against the in-process loopback S3 gateway with several objects
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: replace the request-body XML parse with **`roxmltree`** and keep everything

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

Review of issue 509: implement safe S3 bulk DeleteObjects over `POST /bucket?delete`, with fail-closed XML parsing and idempotent per-key results.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is decision-complete for routing, signed bounded input, XML semantics, idempotency, and destructive-request atomic rejection; the implementation boundary is anchored at `crates/gateway-s3/src/lib.rs:1496`. |
| C2 Reproduction (red pre-fix) | PASS | On current `origin/main` with only the new wire test present, the real toolchain ran 15 tests and reproduced 0/15 passing (normally 501 rather than the required outcomes), grounding the discriminator at `crates/server/tests/s3_delete_objects.rs:260`. |
| C3 Change | PASS | The patch stays within the declared dependency, gateway handler, and wire-test surfaces; malformed input is rejected before the first destructive call at `crates/gateway-s3/src/lib.rs:1871` and deletion begins only at `crates/gateway-s3/src/lib.rs:1882`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept CI with the dependency audit rerun elsewhere — focused wire verification was independently 0/15 red then 15/15 green and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not acquire `/home/eddie/.cargo/advisory-dbs/db.lock` because this host exposes it read-only; the new dependency enters at `Cargo.toml:139`. |
| C5 Causal adequacy | PASS | The prior hole-by-hole XML validation cause is removed rather than runtime-guarded: the whole document is parsed before effects at `crates/gateway-s3/src/lib.rs:1914`, and non-character children fail closed at `crates/gateway-s3/src/lib.rs:1974`. |
| T1 Structure | PASS | Routing, bounded buffering, parsing, effects, and rendering have separate seams, preserving the object-path denylist while intercepting the bucket operation at `crates/gateway-s3/src/lib.rs:1489`. |
| T2 Shape | PASS | The boundary enforces the consequential request shapes before mutation—2 MiB byte cap at `crates/gateway-s3/src/lib.rs:420`, exactly one key per object at `crates/gateway-s3/src/lib.rs:1925`, and 1–1000 keys at `crates/gateway-s3/src/lib.rs:1953`. |
| T3 Runtime | PASS | Real loopback SDK/raw-wire execution passed all 15 cases, including deletion/idempotency at `crates/server/tests/s3_delete_objects.rs:260` and malformed-body victim survival at `crates/server/tests/s3_delete_objects.rs:473`. |
| T4 Contribution | PASS | The added wire suite is independently red on the base and exercises both successful interoperability and destructive fail-closed regressions; the comment-split wrong-key discriminator is at `crates/server/tests/s3_delete_objects.rs:427`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether `roxmltree 0.21.1` satisfies the project dependency audit and whether prior art is sufficiently cleared — merged history was checked by every affected path and the closed-PR search found no issue-509/DeleteObjects candidate, but closed/rejected work cannot be mechanically indexed by affected path here; dependency use is at `crates/gateway-s3/Cargo.toml:38`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process stock-SDK coverage is sufficient evidence for the intended `aws s3 rm --recursive` / `aws s3 sync --delete` workflows — run each command against the patched gateway and confirm multi-key deletion plus `--delete` reconciliation, since the AWS CLI acceptance path was not exercised by the automated gate. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether to accept CI with the dependency audit rerun elsewhere — focused wire verification was independently 0/15 red then 15/15 green and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not acquire `/home/eddie/.cargo/advisory-dbs/db.lock` because this host exposes it read-only; the new dependency enters at `Cargo.toml:139`.
- [x] T5 Judgment — Decide whether `roxmltree 0.21.1` satisfies the project dependency audit and whether prior art is sufficiently cleared — merged history was checked by every affected path and the closed-PR search found no issue-509/DeleteObjects candidate, but closed/rejected work cannot be mechanically indexed by affected path here; dependency use is at `crates/gateway-s3/Cargo.toml:38`.
- [x] Validation — fitness-to-purpose — Decide whether the in-process stock-SDK coverage is sufficient evidence for the intended `aws s3 rm --recursive` / `aws s3 sync --delete` workflows — run each command against the patched gateway and confirm multi-key deletion plus `--delete` reconciliation, since the AWS CLI acceptance path was not exercised by the automated gate.

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
- By / date: Eduard Ralph / 2026-07-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
