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

Review of issue #431: ensure a foreground Reed-Solomon read that survives a permanent block-layer shard fault records a distinct shared-queue repair obligation without corruption signaling.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable and distinguishes durable block damage from checksum corruption: reconstruct the object, queue the chunk, and retain a non-corruption reason (`crates/core/tests/read_block_fault_repair.rs:249`). |
| C2 Reproduction (red pre-fix) | PASS | In an isolated `HEAD` snapshot retaining only the new test, `cargo test -p wyrd-core --test read_block_fault_repair` failed at the empty queue (`crates/core/tests/read_block_fault_repair.rs:260`), while reconstruction had already succeeded (`crates/core/tests/read_block_fault_repair.rs:250`). |
| C3 Change | PASS | The production scope is confined to classifying the established permanent-fault type and carrying its repair obligation through both queue-owning read APIs (`crates/core/src/read.rs:396`, `crates/core/src/read.rs:475`, `crates/core/src/read.rs:526`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test at `crates/chunkstore-grpc/tests/list_delete.rs:55` with `Operation not permitted`. |
| C5 Causal adequacy | PASS | The durable-fault classifier is the existing system decision point, and the fault now creates the missing obligation directly rather than probing for an optional capability or masking a load-time cause (`crates/core/src/read.rs:396`; `crates/traits/src/lib.rs:339`). |
| T1 Structure | PASS | Fault classification, telemetry, collection, and queue production remain at their existing seams, with no new cross-layer dependency (`crates/core/src/read.rs:184`, `crates/core/src/read.rs:396`, `crates/core/src/read.rs:475`). |
| T2 Shape | PASS | The test exercises an RS(2,1) degraded read, exact queue membership, and the stored non-corruption reason through public store contracts (`crates/core/tests/read_block_fault_repair.rs:201`, `crates/core/tests/read_block_fault_repair.rs:260`, `crates/core/tests/read_block_fault_repair.rs:272`). |
| T3 Runtime | PASS | The patched focused test completed with 1 passed, confirming byte reconstruction and durable queue insertion on the exercised asynchronous read path (`crates/core/tests/read_block_fault_repair.rs:249`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work could not be mechanically established from the supplied environment (`crates/core/src/read.rs:396`). |
| T5 Judgment | PASS | The distinct fault class preserves the operator-visible semantic boundary: block damage receives its own counter and repair reason rather than inflating corruption signals (`crates/core/src/read.rs:197`, `crates/core/src/read.rs:475`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the exercised typed-fault test adequately represents production block-device and gRPC fault propagation — sign-off matters because the fixture validates the shared classifier contract, not a live damaged-device topology (`crates/core/tests/read_block_fault_repair.rs:119`; `crates/traits/src/lib.rs:326`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test at `crates/chunkstore-grpc/tests/list_delete.rs:55` with `Operation not permitted`.
- [ ] T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work could not be mechanically established from the supplied environment (`crates/core/src/read.rs:396`).
- [ ] Validation — fitness-to-purpose — Decide whether the exercised typed-fault test adequately represents production block-device and gRPC fault propagation — sign-off matters because the fixture validates the shared classifier contract, not a live damaged-device topology (`crates/core/tests/read_block_fault_repair.rs:119`; `crates/traits/src/lib.rs:326`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test at `crates/chunkstore-grpc/tests/list_delete.rs:55` with `Operation not permitted`.; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work could not be mechanically established from the supplied environment (`crates/core/src/read.rs:396`).
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
