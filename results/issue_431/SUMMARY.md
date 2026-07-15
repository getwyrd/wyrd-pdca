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

Review of issue #431: ensure a foreground Reed-Solomon read that survives a permanent block read fault records a distinct shared-queue repair obligation without treating it as checksum corruption.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-ready: successful degraded reconstruction, durable queue entry, and non-corruption attribution are independently observable at `crates/core/tests/read_block_fault_repair.rs:249`. |
| C2 Reproduction (red pre-fix) | PASS | In a clean target-HEAD export with only the new test added, the read returned the object but the queue assertion failed as `[]` versus the expected chunk at `crates/core/tests/read_block_fault_repair.rs:260`. |
| C3 Change | PASS | The affected production path uses the existing permanence decision point and records a separate obligation without widening generic transient handling at `crates/core/src/read.rs:396` and `crates/core/src/read.rs:475`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test with `Operation not permitted` at `crates/chunkstore-grpc/tests/list_delete.rs:55`. |
| C5 Causal adequacy | PASS | The repair obligation is attached at the durable-fault classification point rather than guarding an optional capability or retry symptom, so the missed producer is directly repaired at `crates/core/src/read.rs:396`. |
| T1 Structure | PASS | The shared classifier, fault telemetry, and shared repair producer remain separated along their existing responsibilities at `crates/core/src/read.rs:396` and `crates/core/src/read.rs:475`. |
| T2 Shape | PASS | Distinct collection and deduplication preserve the corruption/non-corruption contract while retaining the existing read result shape at `crates/core/src/read.rs:464`. |
| T3 Runtime | PASS | The applied focused test executed successfully and demonstrated byte-identical RS(2,1) recovery plus the queued reason assertions at `crates/core/tests/read_block_fault_repair.rs:250` and `crates/core/tests/read_block_fault_repair.rs:284`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear enough for contribution — local merged/all-ref history shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work cannot be mechanically established from the supplied environment (`crates/core/src/read.rs:396`). |
| T5 Judgment | PASS | No scope re-entry is owed: other transient failures remain deliberately unqueued and the block-fault reason remains distinct from corruption at `crates/core/src/read.rs:400` and `crates/core/src/read.rs:475`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated in-process `BlockReadFault` topology sufficiently represents production block-layer failure propagation — this determines whether the queue behavior at `crates/core/tests/read_block_fault_repair.rs:250` is fit for operational sign-off. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test with `Operation not permitted` at `crates/chunkstore-grpc/tests/list_delete.rs:55`.
- [x] T4 Contribution — Decide whether affected-path prior art is clear enough for contribution — local merged/all-ref history shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work cannot be mechanically established from the supplied environment (`crates/core/src/read.rs:396`).
- [x] Validation — fitness-to-purpose — Decide whether the demonstrated in-process `BlockReadFault` topology sufficiently represents production block-layer failure propagation — this determines whether the queue behavior at `crates/core/tests/read_block_fault_repair.rs:250` is fit for operational sign-off.
- [x] external dependency: loopback-bind (host network sandbox) — blocks the aggregate

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
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_431: reviewer host sandbox forbids loopback bind, blocking aggregate `cargo xtask ci` (gRPC tests) and forcing a NEEDS-HUMAN — consider permitting loopback in the reviewer environment or excluding loopback-dependent tests from the reviewer's aggregate run.
