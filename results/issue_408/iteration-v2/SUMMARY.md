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

Review of issue #408: add an opt-in, non-vacuous FoundationDB consistency run under nemesis, obtain real Elle verdicts, and publish the witnessed credibility report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: Check covers the host-independent orchestration while sign-off requires the real FDB/Elle run and committed report. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The red-leg decision remains provisional — this sandbox cannot create the external worktree Git index lock, so it could not stash production changes and run only `xtask/tests/consistency_run_orchestration.rs:1`; the asserted red result was not independently reproduced. |
| C3 Change | FAIL | Decide whether this is ready despite missing required deliverables — the patch has no committed witnessed report, and the real-fixture self-check exercises only register fixtures, leaving `set-full` checker acceptance unproven (`xtask/src/consistency_run_runner.rs:319`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The named green test independently passed 24/24 (`xtask/tests/consistency_run_orchestration.rs:1`), but red could not be reproduced because Git metadata is read-only and the full `cargo xtask ci` rerun stopped on a sandbox-denied loopback bind unrelated to this patch. |
| C5 Causal adequacy | PASS | No capability-probe/runtime-guard smell is introduced; the non-vacuity decision directly rejects either absent concurrency or absent fault materialization (`xtask/src/consistency_run.rs:222`). |
| T1 Structure | PASS | The human must preserve the clean dependency boundary; the default-compiled pure contract is public in xtask while privileged I/O remains in the binary runner (`xtask/src/lib.rs:16`, `xtask/src/main.rs:82`). |
| T2 Shape | FAIL | Decide whether the evidence schema is sufficient — the brief requires op/outcome counts, but `RunSummary` carries only two aggregate operation counts, weakening diagnosis of failed versus successful workload operations (`xtask/src/consistency_run.rs:160`). |
| T3 Runtime | NEEDS-HUMAN | Docker, loadable FDB tooling, Java, elle-cli, and the privileged three-node topology were not exercised, so runtime confidence rests on code-read and curated fixtures; run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=<jar> cargo xtask consistency-run` and require a materialized-fault summary plus `true` for both models (`xtask/src/consistency_run_runner.rs:395`). |
| T4 Contribution | NEEDS-HUMAN | Prior art is mechanically visible in merged history by affected paths, but closed/rejected work could not be independently queried in this restricted artifact-only review; confirm no superseding #408 work before accepting the additive surface (`xtask/src/consistency_run.rs:1`). |
| T5 Judgment | NEEDS-HUMAN | Maintainer judgment is owed on retaining `set-full` at witnessed-run scale and accepting the generated report as the public credibility artifact; both choices determine whether the issue’s externally recognizable claim is supportable (`xtask/src/consistency_run.rs:237`, `xtask/src/consistency_run_runner.rs:342`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Sign-off must decide whether a real, non-vacuous FDB+nemesis run and independently recognized Elle results substantiate the public consistency claim; pure tests alone cannot establish that purpose (`crates/server/tests/consistency_run_fdb.rs:300`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The red-leg decision remains provisional — this sandbox cannot create the external worktree Git index lock, so it could not stash production changes and run only `xtask/tests/consistency_run_orchestration.rs:1`; the asserted red result was not independently reproduced.
- [ ] C4 Verification (red→green) — The named green test independently passed 24/24 (`xtask/tests/consistency_run_orchestration.rs:1`), but red could not be reproduced because Git metadata is read-only and the full `cargo xtask ci` rerun stopped on a sandbox-denied loopback bind unrelated to this patch.
- [ ] T3 Runtime — Docker, loadable FDB tooling, Java, elle-cli, and the privileged three-node topology were not exercised, so runtime confidence rests on code-read and curated fixtures; run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=<jar> cargo xtask consistency-run` and require a materialized-fault summary plus `true` for both models (`xtask/src/consistency_run_runner.rs:395`).
- [ ] T4 Contribution — Prior art is mechanically visible in merged history by affected paths, but closed/rejected work could not be independently queried in this restricted artifact-only review; confirm no superseding #408 work before accepting the additive surface (`xtask/src/consistency_run.rs:1`).
- [ ] T5 Judgment — Maintainer judgment is owed on retaining `set-full` at witnessed-run scale and accepting the generated report as the public credibility artifact; both choices determine whether the issue’s externally recognizable claim is supportable (`xtask/src/consistency_run.rs:237`, `xtask/src/consistency_run_runner.rs:342`).
- [ ] Validation — fitness-to-purpose — Sign-off must decide whether a real, non-vacuous FDB+nemesis run and independently recognized Elle results substantiate the public consistency claim; pure tests alone cannot establish that purpose (`crates/server/tests/consistency_run_fdb.rs:300`).
- [ ] external dependency: java + elle-cli (WYRD_ELLE_CLI_JAR) — blocks the

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
- Iteration delta (if iterating): Witnessed run performed at sign-off (WYRD_TIER1=1 cargo xtask consistency-run, elle-cli 0.1.9, live 3-node FDB cluster): the pipeline ran end-to-end — partition materialized with real typed evidence, genuinely concurrent history (120 register + 180 directory ops), non-vacuity gate and fixtures self-check behaved correctly (exit 1, no fabricated verdict) — but the REAL Elle checker rejected the EDN histories for BOTH models, so no verdict/report is obtainable: - rw-register (real history AND both committed fixtures): "Don't know how to create ISeq from: java.lang.Long" — Elle expects :value to be a transaction (vector of micro-ops, e.g. [[:w :x 1]]); the #406-landed serializer emits single ops with scalar :value and :f :read/:write. - set-full (directory history): "No matching clause: :contains" → :unknown — the jepsen set-full checker expects :add/:read set semantics, not the create/delete/probe vocabulary emitted. The brief's premise "(a) serializes its history via the landed Elle-EDN serializers" is falsified: those serializers are not checker-compatible, and the brief marks "#406 workload/serializer semantics (consume, don't rework)" OUT of scope — so the needed fix is unreachable under the current plan. Replan must: (1) redraw the scope boundary so the EDN format contract can be fixed (amend #406's serializers or add an export-time translation owned by #408); (2) pin the format to what elle-cli actually accepts (transaction-shaped :value micro-ops for rw-register; the set model's :add/:read vocabulary for set-full) and make the committed golden fixtures REAL elle-cli-accepted samples, not Wyrd-vocabulary pins; (3) revisit whether set-full is the right model for the directory workload (the open T5 question). Confirms reviewer C3 FAIL (checker acceptance unproven). Environment is now fully provisioned for the next witnessed run (java + WYRD_ELLE_CLI_JAR=/home/eddie/Downloads/elle-cli-bin-0.1.9/target/elle-cli-0.1.9-standalone.jar).
- By / date: Eduard Ralph / 2026-07-16

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_408: `java` missing on the sign-off host — the doctor row exists (WARN) but the witnessed-run dependency was not provisioned before sign-off; consider provisioning guidance/bootstrap for the off-Check consistency-run leg.
- issue_408: elle-cli standalone jar missing (`WYRD_ELLE_CLI_JAR` unset) on the sign-off host — same gap: the doctor WARN row did not translate into the jar being in place for the witnessed run.
