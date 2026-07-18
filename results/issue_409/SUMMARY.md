# Result — issue 409 / m4-elle-ci-job-dst-regression-loop

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: data-safety verification is currently a one-shot achievement, not a standing
- Success criterion: the standing job and the loop exist and are pinned at Check. The
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the standing job + the loop, four deliverables: (1) the new

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

Review of issue #409: add the standing privileged Elle consistency workflow and close its scheduled-failure-to-permanent-DST-regression loop.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete: the standing job, reporter wiring, artifact retention, and faithful seed replay are independently observable without changing #408 runner semantics (`brief.md`). |
| C2 Reproduction (red pre-fix) | PASS | Independent base rerun retained only the added test and produced 5/5 failures for the absent workflow, reporter entry, and seed anchor; these discriminators are separate tests at `xtask/tests/consistency_ci_job.rs:273`, `:347`, `:407`, and `:443`. |
| C3 Change | PASS | The patch fills the routed seat with schedule/dispatch-only bounded execution, wires scheduled failures into the reporter, and supplies a non-vacuous promotion anchor at `.github/workflows/elle-register-verdict.yml:45`, `.github/workflows/report-nightly-failure.yml:28`, and `crates/dst/tests/commit_ambiguity.rs:1044`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the gate as provisional or rerun it on a host with a writable Cargo advisory DB — named red→green passed 0/5→5/5 and `cargo xtask dst` passed, but full `cargo xtask ci` stopped at `cargo deny` on the read-only advisory lock before completion (`crates/dst/tests/commit_ambiguity.rs:1073`). |
| C5 Causal adequacy | PASS | The fix restores all three missing links rather than adding a capability probe/runtime symptom guard: execution at `.github/workflows/elle-register-verdict.yml:133`, bug capture at `.github/workflows/report-nightly-failure.yml:52`, and durable replay at `crates/dst/tests/commit_ambiguity.rs:1073`. |
| T1 Structure | PASS | The privileged workflow remains thin at its decision seam while Check-time pinning stays in the existing xtask test surface and deterministic replay stays in the existing madsim DST surface (`.github/workflows/elle-register-verdict.yml:125`, `xtask/tests/consistency_ci_job.rs:273`). |
| T2 Shape | PASS | Five independently attributable tests pin the seat, trigger/dispatch shape, emitted artifact directory, reporter entry, and replay anchor; the cross-boundary subcommand check is explicit at `xtask/tests/consistency_ci_job.rs:327`. |
| T3 Runtime | NEEDS-HUMAN | Confirm the first post-merge `workflow_dispatch` completes on GitHub with Docker, FoundationDB client, JVM, and pinned Elle jar, then record its run link — local evidence exercised the DST runtime but cannot exhibit the privileged topology (`.github/workflows/elle-register-verdict.yml:87`). |
| T4 Contribution | PASS | Affected-path history and all-state PR/branch searches found no superseding workflow or test and only the known reporter/commit-ambiguity predecessors; the contribution is additive at `.github/workflows/elle-register-verdict.yml:1`. |
| T5 Judgment | PASS | No patch defect grounded on the current folded #408 target: the invocation and uploaded directory are mechanically coupled to the real dispatch/output seams at `xtask/tests/consistency_ci_job.rs:327` and `:347`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a daily 07:00 UTC, 60-minute, post-merge-only signal with 90-day artifacts provides adequate operational detection and investigation latency — that policy determines whether the loop is useful in practice (`.github/workflows/elle-register-verdict.yml:45`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether to accept the gate as provisional or rerun it on a host with a writable Cargo advisory DB — named red→green passed 0/5→5/5 and `cargo xtask dst` passed, but full `cargo xtask ci` stopped at `cargo deny` on the read-only advisory lock before completion (`crates/dst/tests/commit_ambiguity.rs:1073`).
- [x] T3 Runtime — Confirm the first post-merge `workflow_dispatch` completes on GitHub with Docker, FoundationDB client, JVM, and pinned Elle jar, then record its run link — local evidence exercised the DST runtime but cannot exhibit the privileged topology (`.github/workflows/elle-register-verdict.yml:87`).
- [x] Validation — fitness-to-purpose — Decide whether a daily 07:00 UTC, 60-minute, post-merge-only signal with 90-day artifacts provides adequate operational detection and investigation latency — that policy determines whether the loop is useful in practice (`.github/workflows/elle-register-verdict.yml:45`).
- [x] external dependency: network access to verify the elle-cli release asset

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
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
