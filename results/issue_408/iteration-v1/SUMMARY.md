# Result — issue 408 / m4-checked-consistency-run-elle-report

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: one opt-in command runs the #406 checked register + directory workload against
- Success criterion: the run pipeline exists end-to-end and refuses to overstate
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the checked-run pipeline — one opt-in xtask subcmd, the live scenario test in

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

Review of issue #408: add an opt-in, non-vacuous FoundationDB consistency run under nemesis that checks exported histories with Elle and publishes the witnessed report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary distinguishes the Check-testable orchestration core from the privileged witnessed run and names the required report evidence (`brief.md:8`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the claimed pre-fix red only after rerunning the unavailable `engine/scripts/run-verify.sh` — the target has Cargo but not that gate script, so I could not independently reproduce the red leg (`xtask/tests/consistency_run_orchestration.rs:30`). |
| C3 Change | PASS | The patch supplies the default-compiled orchestration seam and the opt-in runner entry needed to make the new workflow reachable (`xtask/src/lib.rs:16`, `xtask/src/main.rs:106`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the independently observed 23/23 named-test green plus the recorded gate results suffice without a reviewer red/full-CI rerun — `cargo test -p xtask --test consistency_run_orchestration` passed, but the asserted full/red→green runner is absent (`xtask/tests/consistency_run_orchestration.rs:39`). |
| C5 Causal adequacy | PASS | The change gates the credibility claim on both concurrency and fault materialization before invoking Elle, addressing vacuous-run causality without adding a capability probe (`xtask/src/consistency_run.rs:193`). |
| T1 Structure | PASS | The host-independent decisions remain in the default `xtask` library while privileged Docker/JVM I/O stays in the runner, preserving the intended dependency boundary (`xtask/src/consistency_run_runner.rs:20`). |
| T2 Shape | FAIL | The public artifact cannot substantiate how the fault materialized: the scenario hard-codes `true` and the JSON/report carry only that boolean, discarding the required typed evidence and its context (`crates/server/tests/consistency_run_fdb.rs:194`, `crates/server/tests/consistency_run_fdb.rs:215`, `xtask/src/consistency_run_runner.rs:352`). |
| T3 Runtime | NEEDS-HUMAN | Run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=/path/to/elle-cli.jar cargo xtask consistency-run` on the privileged ≥3-process FDB topology and confirm both model verdicts, materialized fault, teardown, and report — this host lacks Java/the jar and did not exercise Docker topology (`xtask/src/consistency_run_runner.rs:396`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected-work prior art is clear before contribution — affected-path merged history shows the prerequisite #407/#442 work but local history cannot mechanically settle rejected or unmerged alternatives (`crates/server/Cargo.toml:107`). |
| T5 Judgment | NEEDS-HUMAN | Approve only after the first real witnessed report is committed and review whether `set-full` remains practical — no `docs/design/reviews/` report is present in this patch, so the promised public credibility artifact is still externally undischarged (`xtask/src/consistency_run_runner.rs:366`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a real Elle verdict under a demonstrably materialized production-like fault is credible enough for #329 — Check exercised only pure orchestration and curated fixtures, not the external checker/topology (`xtask/tests/consistency_run_orchestration.rs:3`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the claimed pre-fix red only after rerunning the unavailable `engine/scripts/run-verify.sh` — the target has Cargo but not that gate script, so I could not independently reproduce the red leg (`xtask/tests/consistency_run_orchestration.rs:30`).
- [ ] C4 Verification (red→green) — Decide whether the independently observed 23/23 named-test green plus the recorded gate results suffice without a reviewer red/full-CI rerun — `cargo test -p xtask --test consistency_run_orchestration` passed, but the asserted full/red→green runner is absent (`xtask/tests/consistency_run_orchestration.rs:39`).
- [ ] T3 Runtime — Run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=/path/to/elle-cli.jar cargo xtask consistency-run` on the privileged ≥3-process FDB topology and confirm both model verdicts, materialized fault, teardown, and report — this host lacks Java/the jar and did not exercise Docker topology (`xtask/src/consistency_run_runner.rs:396`).
- [ ] T4 Contribution — Decide whether closed/rejected-work prior art is clear before contribution — affected-path merged history shows the prerequisite #407/#442 work but local history cannot mechanically settle rejected or unmerged alternatives (`crates/server/Cargo.toml:107`).
- [ ] T5 Judgment — Approve only after the first real witnessed report is committed and review whether `set-full` remains practical — no `docs/design/reviews/` report is present in this patch, so the promised public credibility artifact is still externally undischarged (`xtask/src/consistency_run_runner.rs:366`).
- [ ] Validation — fitness-to-purpose — Decide whether a real Elle verdict under a demonstrably materialized production-like fault is credible enough for #329 — Check exercised only pure orchestration and curated fixtures, not the external checker/topology (`xtask/tests/consistency_run_orchestration.rs:3`).

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
- Iteration delta (if iterating): Rejected at sign-off on the reviewer's findings; the pipeline shape is right but the credibility artifact cannot yet substantiate itself: - T2 FAIL (primary): the live scenario hard-codes `nemesis_materialized = true` and the run summary/report carry only that boolean. Propagate the #407 typed materialization evidence (what the leg observed: which fault, on which target, how it provably bit) through `run-summary.json` and into the report, instead of asserting the boolean from `drive_leg`'s contract (crates/server/tests/consistency_run_fdb.rs:194,215; xtask/src/consistency_run_runner.rs:352). - T3/T5/Validation: no real witnessed run exists — the privileged `WYRD_TIER1=1 ... cargo xtask consistency-run` was never executed on a Docker/JVM host, and the promised first witnessed report under docs/design/reviews/ is absent. The next attempt should include (or be verified against) an actual live run and its committed report, and confirm `set-full` is practical for the directory model. Keep the existing pure-core/runner split and the non-vacuity gate — those were reviewed as sound; the delta is evidence fidelity plus the witnessed run, not a redesign.
- By / date: Eduard Ralph / 2026-07-16

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
