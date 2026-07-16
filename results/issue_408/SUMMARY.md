# Result — issue 408 / m4-checked-consistency-run-elle-report

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: one opt-in command runs the checked register + directory workload against the
- Success criterion: the run pipeline exists end-to-end and refuses to overstate
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: (1) the Elle-EDN **format-contract fix at its source**: amend

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

Task under review: implement issue #408’s opt-in FoundationDB consistency run, checker-compatible Elle histories, non-vacuity gating, and published witnessed-run report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is concrete enough to judge: a report is licensed only by concurrent register evidence, materialized fault evidence, and a determinate directory read (`xtask/src/consistency_run.rs:223`). |
| C2 Reproduction (red pre-fix) | PASS | In a temporary clean-base tree with the added test and fixtures retained, `cargo test -p xtask --test consistency_run_orchestration` fails on the absent `xtask::consistency_run` import at `xtask/tests/consistency_run_orchestration.rs:26`; with the patch it passes 40/40. |
| C3 Change | PASS | The change reaches the required production seams—default-compiled decision logic, an opt-in runner, and the feature-gated live scenario—rather than stopping at a fixture or report (`xtask/src/lib.rs:16`, `xtask/src/main.rs:108`, `crates/server/tests/consistency_run_fdb.rs:314`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the independently confirmed focused red→green is sufficient without a rerun of the full asserted CI oracle: `engine/xtask.sh` is absent, and socket-binding server tests are blocked by this host’s `Operation not permitted`; this matters because the claimed whole-tree green remains provisional (`xtask/tests/consistency_run_orchestration.rs:26`, `crates/server/tests/consistency_observable.rs:62`). |
| C5 Causal adequacy | PASS | The patch removes the serialization/observation causes by emitting the accepted model vocabulary and retaining transport-failed operations as indeterminate; it does not add a capability probe or guard around an expected capability (`crates/server/src/consistency_workload.rs:690`, `crates/server/src/consistency_observable.rs:53`). |
| T1 Structure | PASS | Decision logic remains in the default xtask library while privileged Docker/JVM I/O is isolated in the runner, preserving a host-independent test graph (`xtask/src/consistency_run.rs:38`, `xtask/src/consistency_run_runner.rs:8`). |
| T2 Shape | PASS | The summary seam rejects unknown fields and carries typed nemesis evidence, per-pool outcomes, delete-check results, member mapping, and final-read determinacy needed to prevent silent evidence loss (`xtask/src/consistency_run.rs:237`). |
| T3 Runtime | NEEDS-HUMAN | Re-exercise `WYRD_TIER1=1 cargo xtask consistency-run` on the declared three-node topology with Docker, FDB client/header support, Java, and the real elle-cli jar; those external dependencies were not exercised here, so runtime confidence rests on the committed witnessed artifact and pure tests (`docs/design/reviews/m4-checked-consistency-run.md:134`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether remote prior art is clear: affected-path local history confirms the merged #406 substrate, but this artifact-only environment cannot mechanically settle closed/rejected PRs or remote branches; duplicate ownership would undermine the additive contribution (`crates/server/src/consistency_workload.rs:583`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether a single non-disruptive partition run is a sufficiently credible public milestone artifact—the report states all 720 operations succeeded and explicitly excludes disruptive-fault behavior, so the value depends on the intended strength of the claim (`docs/design/reviews/m4-checked-consistency-run.md:86`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the published report’s two real Elle `true` verdicts under a materialized but client-transparent partition satisfy #329’s credibility purpose despite covering one topology, one run, and weaker in-house checks for deletes (`docs/design/reviews/m4-checked-consistency-run.md:95`). |

### Advisory — adversary

# Adversarial review — issue #408, iteration v5 (m4-checked-consistency-run-elle-report)

Posture: assumed the patch wrong and the reviewer fooled; tried to prove it. Toolchain was
available (cargo, java, unzip, the pinned elle-cli jar), so every verdict below is
execution-backed, none provisional.

## Refutation attempts that FAILED (each independently re-run in a scratch clone of `origin/main` @ e0e39c1 + patch.diff)

- **Red→green re-run** — green leg: `cargo test -p xtask --test consistency_run_orchestration`
  (40 pass) and `cargo test -p wyrd-server --test consistency_workload` (18 pass) on the patched
  clone. Red leg: reverting the production modules while keeping the added tests fails both
  binaries (E0432/E0599). The red is compile-fail (ADDED_TEST shape), exactly as the brief's
  Falsifiability section pre-declared — and the tests carry real behavioral assertions
  (token-keyed verdicts, nested `deny_unknown_fields`, gate ordering), so post-merge behavioral
  regressions also flip red, not just deletion.
- **The live verdicts are real and history-sensitive, not tautological.** Re-ran the actual jar
  (sha256 matches the report: `c9ba9b9f…f539a`) over the run's own histories at
  `target/consistency-run/`: register → `true`, directory → `true`, reproducing the report.
  Known-bad fixtures → `false`/`false`; degraded-final-read fixture → `:unknown`. Mutating the
  live directory history (final-read element `1`→`999`, i.e. one acknowledged `:add` dropped)
  flips the checker to `false` — the `true` is earned by the history's content.
- **The committed artifact is not a hand-transcription.** The verbatim block in
  `docs/design/reviews/m4-checked-consistency-run.md:161-170` is byte-identical to the
  runner-emitted `target/consistency-run/report.md` (diff clean), and every number in it
  cross-checks against `target/consistency-run/run-summary.json` (120/121/480 ops, counts,
  member map, determinate composed read).
- **Every v4 sign-off defect has a production fix AND a Check-time pin**, verified at the target:
  single-writer delete-pool keys (`crates/server/src/consistency_workload.rs:115`; pins at
  `crates/server/tests/consistency_workload.rs:380,432` — the banded trap is exhibited, not just
  avoided); unresolved-probe degrade to `:info` with bounded re-probe
  (`crates/server/src/consistency_workload.rs:845`, `crates/server/tests/consistency_run_fdb.rs:614-671`;
  pin at `consistency_workload.rs:279`); quiesce before the composed read
  (`crates/server/tests/consistency_run_fdb.rs:98,229`); nested `deny_unknown_fields` + nested
  unknown-field test (`xtask/src/consistency_run.rs:142,169,213,238`;
  `xtask/tests/consistency_run_orchestration.rs:394`); `unzip` in the preflight
  (`xtask/src/consistency_run.rs:629,654`); directory op-count no longer counts sweep probes
  (`crates/server/src/consistency_workload.rs:1002`; pin at `consistency_workload.rs:350`).
- **Gate-wiring attacks that found nothing:** `:unknown`-with-exit-0 parses inconclusive and the
  runner's final match refuses non-Pass pairs (`xtask/src/consistency_run_runner.rs:561-572`);
  a delete-pool `false` fails the run even when the vacuity gate would already have blocked it
  (`consistency_run_runner.rs:551-559`, independence pinned at
  `consistency_run_orchestration.rs:489`); the fixtures self-check runs against the SAME jar
  before any verdict is acted on (`consistency_run_runner.rs:537` precedes the verdict match);
  INV-1 stale-status/dropped-op fabrications from v3 are gone (`OpFailed::into_record` at
  `crates/server/src/consistency_observable.rs:85`, used at `consistency_run_fdb.rs:592-595,658`).

## Findings

- NEEDS-HUMAN — **The materialized fault never touched a single op — is this the "checked run
  under failure" #329 DoD item 2 intends?** The run's own summary shows 720/720 ops `ok`,
  `info: 0`, `fail: 0` across all three pools (`target/consistency-run/run-summary.json`;
  disclosed prominently at `docs/design/reviews/m4-checked-consistency-run.md:86-94`): the
  partition of one of three nodes was fully absorbed by FDB's quorum, so the checked histories
  are observationally identical to ones recorded on a healthy cluster — only the leg's typed
  evidence distinguishes the runs. This *conforms to the brief* (Design §4 gates exactly on
  INV-2 + materialized evidence, both attested) and the report names it as its most important
  caveat, so it is not an implementation defect — but whether a fully-absorbed fault is a
  strong enough witnessed run for the public credibility artifact, or whether the maintainer
  should demand a re-run with a quorum-costing / client-visible fault before ready-mark, is a
  fitness-to-purpose call only the human can make at sign-off.
- `crates/server/tests/consistency_run_fdb.rs:675-698` — `register_outcome_counts` re-derives
  the ok/fail/info classification (`is_indeterminate` + per-kind 2xx/404 arms) in the one file
  no Check gate compiles, duplicating production's `register_completion_type`
  (`crates/server/src/consistency_workload.rs:719`). The two agree today (verified by
  inspection), and the counts are report-fidelity only — never verdict-bearing — so this is a
  drift risk, not a defect: if the production classification ever changes, the summary's
  outcome counts silently diverge from what the EDN says. Cheap hardening: count by
  `register_completion_keyword` instead. Not worth blocking on.

## Verdict

Attempted to refute the red→green evidence, the verdict parser, the gate wiring, the composed
final read, the delete-pool construction, the seam contract, and the committed report's
authenticity — could not. The one open question (the absorbed fault) is architectural/fitness,
already disclosed by the artifact itself, and flagged above for the human.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether the independently confirmed focused red→green is sufficient without a rerun of the full asserted CI oracle: `engine/xtask.sh` is absent, and socket-binding server tests are blocked by this host’s `Operation not permitted`; this matters because the claimed whole-tree green remains provisional (`xtask/tests/consistency_run_orchestration.rs:26`, `crates/server/tests/consistency_observable.rs:62`).
- [x] T3 Runtime — Re-exercise `WYRD_TIER1=1 cargo xtask consistency-run` on the declared three-node topology with Docker, FDB client/header support, Java, and the real elle-cli jar; those external dependencies were not exercised here, so runtime confidence rests on the committed witnessed artifact and pure tests (`docs/design/reviews/m4-checked-consistency-run.md:134`).
- [x] T4 Contribution — Decide whether remote prior art is clear: affected-path local history confirms the merged #406 substrate, but this artifact-only environment cannot mechanically settle closed/rejected PRs or remote branches; duplicate ownership would undermine the additive contribution (`crates/server/src/consistency_workload.rs:583`).
- [x] T5 Judgment — Decide whether a single non-disruptive partition run is a sufficiently credible public milestone artifact—the report states all 720 operations succeeded and explicitly excludes disruptive-fault behavior, so the value depends on the intended strength of the claim (`docs/design/reviews/m4-checked-consistency-run.md:86`).
- [x] Validation — fitness-to-purpose — Decide whether the published report’s two real Elle `true` verdicts under a materialized but client-transparent partition satisfy #329’s credibility purpose despite covering one topology, one run, and weaker in-house checks for deletes (`docs/design/reviews/m4-checked-consistency-run.md:95`).
- [x] **The materialized fault never touched a single op — is this the "checked run
- [x] external dependency: unzip — the off-Check runner reads the elle-cli version out of the jar (META-INF/maven/elle-cli/elle-cli/pom.properties) via `unzip -p`, because elle-cli 0.1.9 has NO --version flag (verified on this host: it prints `Unknown option: "--version"` and exits 0). Plan's External-dependencies list names docker/libfdb_c/fdb-headers/java/elle-cli but not unzip, and pdca.toml still has no row for it (the v4 carry-forward asked for one; it has not landed). It IS present on this host, so nothing was blocked and no evidence is missing — the witnessed run completed and read the checker's version from the jar — but the run hard-fails at preflight without it, so it belongs in the registered set rather than being discovered by a maintainer mid-run.

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
- By / date: Eduard Ralph / 2026-07-16

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_408: file a getwyrd/wyrd tracker issue for a witnessed consistency-run under a **client-visible / quorum-costing fault** (fault escapes the cluster's tolerance envelope, so the history carries `info > 0` ops) — post-0.1-alpha, alongside the scheduled real-hardware/long-duration campaign; addresses the #408 report's absorbed-fault caveat (adversary finding).
- issue_408: register **`unzip`** as an external dependency of the consistency-run preflight (pdca.toml external-dependencies rows + the brief template's External-dependencies list) — the runner hard-fails at preflight without it (`unzip -p` reads the elle-cli version from the jar; elle-cli 0.1.9 has no `--version` flag); v4 carry-forward, still unlanded.
