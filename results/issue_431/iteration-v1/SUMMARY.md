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

Review of issue #431: ensure a degraded foreground Reed-Solomon read records permanent block-layer shard faults on the shared repair queue without treating them as checksum corruption.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: reconstruct from the surviving k shards, queue the damaged chunk, and retain a non-corruption classification; the production decision point is grounded at `crates/traits/src/lib.rs:326`. |
| C2 Reproduction (red pre-fix) | PASS | In an isolated checkout of target HEAD with only the new test added, `cargo test -p wyrd-core --test read_block_fault_repair` failed because the queue was `[]` rather than the affected chunk at `crates/core/tests/read_block_fault_repair.rs:260`. |
| C3 Change | PASS | The change stays within the foreground read/repair obligation boundary: permanent faults are separated at `crates/core/src/read.rs:396` and queued with a distinct audit reason at `crates/core/src/read.rs:474`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted`, so the asserted complete green was not reproducible here (`crates/chunkstore-grpc/tests/list_delete.rs:55`). |
| C5 Causal adequacy | PASS | Permanence is classified through the existing system-wide source-chain decision rather than an optional-capability fallback, so the repair obligation follows the durable-fault cause at `crates/core/src/read.rs:396` and ordinary transient errors remain outside it at `crates/core/src/read.rs:413`. |
| T1 Structure | PASS | Responsibility remains separated: read code records findings and the established queue API owns persistence, with the handoff at `crates/core/src/read.rs:474` and queue contract at `crates/core/src/repair.rs:73`. |
| T2 Shape | PASS | The test drives the public foreground read with RS(2,1), two surviving shards, and the shared metadata queue, making the asserted externally relevant shape explicit at `crates/core/tests/read_block_fault_repair.rs:197`. |
| T3 Runtime | PASS | The patched target's focused test passed and demonstrated byte-identical recovery plus a durable queue entry and distinct reason at `crates/core/tests/read_block_fault_repair.rs:249`; aggregate runtime coverage remains subject to C4's host caveat. |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows the earlier telemetry work but no equivalent fix, while closed/rejected remote work could not be mechanically established from the supplied artifacts. |
| T5 Judgment | PASS | The patch restores the stated durability invariant without widening transient-fault policy or conflating block failure with corruption, as shown by the separate block-fault metric at `crates/core/src/read.rs:197` and repair reason at `crates/core/src/read.rs:475`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether an in-process typed-fault test is sufficient evidence for production fitness — it proves classifier/read/queue behavior at `crates/core/tests/read_block_fault_repair.rs:131`, but sign-off still owns whether the real block-device and gRPC fault paths need exercising. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted`, so the asserted complete green was not reproducible here (`crates/chunkstore-grpc/tests/list_delete.rs:55`).
- [ ] T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows the earlier telemetry work but no equivalent fix, while closed/rejected remote work could not be mechanically established from the supplied artifacts.
- [ ] Validation — fitness-to-purpose — Decide whether an in-process typed-fault test is sufficient evidence for production fitness — it proves classifier/read/queue behavior at `crates/core/tests/read_block_fault_repair.rs:131`, but sign-off still owns whether the real block-device and gRPC fault paths need exercising.

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted`, so the asserted complete green was not reproducible here (`crates/chunkstore-grpc/tests/list_delete.rs:55`).; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows the earlier telemetry work but no equivalent fix, while closed/rejected remote work could not be mechanically established from the supplied artifacts.
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
