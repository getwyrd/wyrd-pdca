# Result — issue 431 / read-block-fault-repair-obligation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Reed-Solomon foreground read path reads AROUND a permanent block-layer
- Success criterion: A foreground RS read that encounters a block-layer read fault on
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical fix in `crates/core/src/read.rs`: distinguish

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

Review of issue #431: ensure a degraded foreground Reed-Solomon read records a repair obligation for a permanent block-layer fault without misclassifying it as corruption.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: preserve successful reconstruction, enqueue durable damage, and keep the reason outside the corruption class (`crates/core/tests/read_block_fault_repair.rs:249`). |
| C2 Reproduction (red pre-fix) | PASS | In a clean base snapshot carrying only the new test, `cargo test -p wyrd-core --test read_block_fault_repair` failed because the observed queue was `[]` rather than the damaged chunk (`crates/core/tests/read_block_fault_repair.rs:260`). |
| C3 Change | PASS | The affected behavior remains confined to foreground read classification and its two existing queue-producing callers, so unrelated transient faults retain their prior treatment (`crates/core/src/read.rs:396`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted` (`crates/chunkstore-grpc/tests/list_delete.rs:55`). |
| C5 Causal adequacy | PASS | The permanence decision uses the shared typed classifier at the fetch-error decision point and creates the previously missing obligation, rather than probing an optional capability or masking an eager side effect (`crates/core/src/read.rs:396`). |
| T1 Structure | PASS | The production seam is exercised from a dedicated integration-test file, keeping the regression independently flippable (`crates/core/tests/read_block_fault_repair.rs:249`). |
| T2 Shape | PASS | The oracle checks reconstructed bytes, the shared queue entry, and its stored non-corruption reason, which are the externally material acceptance outcomes (`crates/core/tests/read_block_fault_repair.rs:249`). |
| T3 Runtime | PASS | The patched target's focused test completed with 1 passed and exercised the async read and metadata queue path (`crates/core/tests/read_block_fault_repair.rs:250`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear for contribution — local merged/all-ref history shows no equivalent fix, but closed/rejected remote work cannot be mechanically established from the supplied artifacts (`crates/core/src/read.rs:396`). |
| T5 Judgment | PASS | The change preserves the specified corruption/transient boundary while assigning permanent block damage its own telemetry and repair reason, with no ambiguous scope expansion (`crates/core/src/read.rs:470`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process `BlockReadFault` test adequately represents production block/backend propagation — this determines whether the verified queue behavior is operationally representative (`crates/core/tests/read_block_fault_repair.rs:250`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted` (`crates/chunkstore-grpc/tests/list_delete.rs:55`).
- [ ] T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history shows no equivalent fix, but closed/rejected remote work cannot be mechanically established from the supplied artifacts (`crates/core/src/read.rs:396`).
- [ ] Validation — fitness-to-purpose — Decide whether the in-process `BlockReadFault` test adequately represents production block/backend propagation — this determines whether the verified queue behavior is operationally representative (`crates/core/tests/read_block_fault_repair.rs:250`).

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
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted` (`crates/chunkstore-grpc/tests/list_delete.rs:55`).; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history shows no equivalent fix, but closed/rejected remote work cannot be mechanically established from the supplied artifacts (`crates/core/src/read.rs:396`).
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
