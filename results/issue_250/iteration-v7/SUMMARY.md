# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 consistency-over-repair leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **STRUCTURAL DECISION (the maintainer chose Option B, mirroring the
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
- Scope (one logical fix) / out of scope: Build the Tier-1 consistency-over-repair leg as an in-repo Rust scenario

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

# Check review — issue 250 / tier1-jepsen-consistency-harness (iteration 7)

Advisory, artifact-only. Inputs: patch.diff, brief.md, check-gates.json (build-notes.md
withheld). `$PDCA_TARGET` was not readable in this environment, so every citation below is
grounded on `patch.diff` line numbers, per the unset-target rule. This is a target-state
caveat on grounding, **not** a patch defect, and nothing below is a stale-target "cannot
apply/compile" claim.

## Verdict table (complete 5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and binding (brief.md:30-59): rewire `run_jepsen` off the absent `WYRD_TIER1_JEPSEN_CMD` to an in-repo scenario, the scenario as buildable Rust driving the production reconcile path, and a privileged `tier1-jepsen.yml`. All three are present in the diff. |
| C2 Reproduction (red pre-fix) | FAIL | The iter-6 rejection's exact requirement — a behavioral red that fails when `run_jepsen` still routes to the external command — is **not met**. `jepsen_routing.rs` (patch.diff:1190+) tests only the parallel `xtask::jepsen` constants; it never exercises `run_jepsen`/`run_jepsen_consistency_test`, which hardcode their cargo args (patch.diff:1021-1031) and never call that module. Reverting the `faults.rs` rewire to the old shell-out leaves every routing test green. Human owns: is a compile-seam red over net-new scaffolding acceptable when iter-6 rejected this identical tautology, restructured? |
| C3 Change | PASS | Scope matches the brief: `run_jepsen` rewire + net-new scenario + workflow + dispatch-metadata module; production reconstruction code untouched (that was #251). No scope creep into the disk-fault/kill-reconstruct legs. |
| C4 Verification (red→green) | NEEDS-HUMAN | C4-ci gate passed (gating); C4-verify passed mechanically (advisory). But the red→green it asserts is a **compile failure from deleting `xtask/src/jepsen.rs`** (jepsen.rs:8-13), decorrelated from the dispatch defect. The dispatch rewire in `run_jepsen` has **no flippable test bound to it** — the human must decide whether the born-at-tier compile-seam clears the dispatch-correctness bar the brief sets (brief.md:38-42, "that dispatch wiring … is unit-tested"). |
| C5 Causal adequacy | PASS | Root cause ("leg never built", brief.md:20-28) is addressed in substance: the scenario drives production `reconcile_step → reconstruction::reconcile` (patch.diff:698-712, 782-796), not a stub. No symptom-guard smell — the `Plan::Deferred/MissingTool` gating mirrors the merged siblings; the test-side env-var skips are harness setup, not a capability probe over a load-time side effect. Note: dispatch args are duplicated (jepsen.rs vs faults.rs), not single-sourced — that duplication is the mechanical root of the C2 gap. |
| T1 Structure | PASS | Mirrors the merged sibling structure (`run_disk_faults`/`run_kill_reconstruct`). Smell, non-blocking: `xtask::jepsen::consistency_test_cargo_args()` is never called by `run_jepsen_consistency_test` (patch.diff:1021-1031 re-list the same args) — the metadata module exists to be tested rather than to be used. |
| T2 Shape | PASS | Scenario follows `tier2_kill_reconstruct.rs` shape (CrashMeta commit-intercept, RS(6,3), `#[ignore]`d body, enqueue_repair stand-in tagged `tier1-jepsen-test`, patch.diff:642). Helper unit tests carry negative controls (patch.diff:418-491), satisfying the #146 load-bearing-oracle posture for the helpers. |
| T3 Runtime | NEEDS-HUMAN | Live run is deferred/off-Check by design. Coupling risk to verify at first `tier1-jepsen.yml` run: `run_jepsen` stands up `KR_DSERVER_COUNT` servers via the **Tier-2** docker-compose.yml (patch.diff:967-975), but the scenario asserts exactly `JC_DSERVER_COUNT == 10` endpoints (patch.diff:578-584). If Tier-2's count ≠ 10, the live run aborts on the assert. Human owns: confirm the reused compose actually provisions 10 servers. |
| T4 Contribution | NEEDS-HUMAN | Pre-declared (brief.md:104-108): Option B builds an in-repo Rust scenario where accepted proposal 0005 (`0005:408`) names "Jepsen" literally. Human owns: weigh a clarifying/superseding ADR, and the naming choice (`tier1-jepsen.yml`, `jepsen` module) for a harness that is deliberately **not** Jepsen — a future reader could mistake it for the literal artifact. |
| T5 Judgment | NEEDS-HUMAN | Prior-art/fork discipline mechanically settled by file path (no `tier1-jepsen.yml` / scenario on origin/main per brief.md:213-227; siblings are pattern precedent, not duplicates). Human owns: the two pre-declared follow-on issues to file (literal-Jepsen credibility artifact + missing-fragment detection product gap, brief.md:108-113) and confirming Option B against clean upstream `main`, not the drifted iteration branches. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Off-Check by design: the consistency contract holding on a live cluster (read-after-commit, no torn/stale reads, commit-point-atomic repair) is observable only in the first `tier1-jepsen.yml workflow_dispatch` run, confirmed by the maintainer (brief.md:178-187). Human owns: does a green live run, plus the planted-anomaly negative controls, actually demonstrate Wyrd's ADR-0015 contract over the repair path? |

## Lead finding for sign-off

C2/C4 are the gating concern and a **re-occurrence of the iteration-6 rejection class**. The
patch relocates the tautology rather than removing it: the flippable "red" is now a compile
failure from deleting the net-new `xtask/src/jepsen.rs` metadata module, while the production
dispatch inside `run_jepsen` (the thing whose correctness the brief and the iter-6 carry-forward
demand be verified) is exercised by **no test** — `faults.rs` is a binary module outside the lib,
and `jepsen_routing.rs` asserts over constants that `run_jepsen_consistency_test` does not consume.
A behavioral test that drives `run_jepsen`'s `Plan::Run` branch (or that makes
`run_jepsen_consistency_test` call `jepsen::consistency_test_cargo_args()` as the single source of
truth, then asserts the routing) would close this. As-is, the dispatch rewire could regress to the
external shell-out with the suite still green.

### Advisory — codex

- xtask/src/faults.rs:192 duplicates the `"docker"` requirement and, further down, hard-codes the `cargo test` argument list instead of using `xtask::jepsen::{required_tool, consistency_test_cargo_args}`. The new routing tests assert only the metadata module (`xtask/tests/jepsen_routing.rs:48`), so the production `run_jepsen` path can drift back to a wrong tool or target while those tests still pass; make the runner consume the tested metadata, or move the test to exercise the runner-built command directly.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — C4-ci gate passed (gating); C4-verify passed mechanically (advisory). But the red→green it asserts is a **compile failure from deleting `xtask/src/jepsen.rs`** (jepsen.rs:8-13), decorrelated from the dispatch defect. The dispatch rewire in `run_jepsen` has **no flippable test bound to it** — the human must decide whether the born-at-tier compile-seam clears the dispatch-correctness bar the brief sets (brief.md:38-42, "that dispatch wiring … is unit-tested").
- [ ] T3 Runtime — Live run is deferred/off-Check by design. Coupling risk to verify at first `tier1-jepsen.yml` run: `run_jepsen` stands up `KR_DSERVER_COUNT` servers via the **Tier-2** docker-compose.yml (patch.diff:967-975), but the scenario asserts exactly `JC_DSERVER_COUNT == 10` endpoints (patch.diff:578-584). If Tier-2's count ≠ 10, the live run aborts on the assert. Human owns: confirm the reused compose actually provisions 10 servers.
- [ ] T4 Contribution — Pre-declared (brief.md:104-108): Option B builds an in-repo Rust scenario where accepted proposal 0005 (`0005:408`) names "Jepsen" literally. Human owns: weigh a clarifying/superseding ADR, and the naming choice (`tier1-jepsen.yml`, `jepsen` module) for a harness that is deliberately **not** Jepsen — a future reader could mistake it for the literal artifact.
- [ ] T5 Judgment — Prior-art/fork discipline mechanically settled by file path (no `tier1-jepsen.yml` / scenario on origin/main per brief.md:213-227; siblings are pattern precedent, not duplicates). Human owns: the two pre-declared follow-on issues to file (literal-Jepsen credibility artifact + missing-fragment detection product gap, brief.md:108-113) and confirming Option B against clean upstream `main`, not the drifted iteration branches.
- [ ] Validation — fitness-to-purpose — Off-Check by design: the consistency contract holding on a live cluster (read-after-commit, no torn/stale reads, commit-point-atomic repair) is observable only in the first `tier1-jepsen.yml workflow_dispatch` run, confirmed by the maintainer (brief.md:178-187). Human owns: does a green live run, plus the planted-anomaly negative controls, actually demonstrate Wyrd's ADR-0015 contract over the repair path?

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
- Iteration delta (if iterating): Rejected: C2 FAIL recurs as the iteration-6 tautology, restructured. The flippable "red" is a compile failure from deleting the net-new xtask/src/jepsen.rs metadata module — it is not bound to the production dispatch. run_jepsen_consistency_test (faults.rs, patch.diff:1021-1031) hand-types its cargo-test args instead of consuming jepsen::consistency_test_cargo_args(); the two arg lists are duplicated, not single-sourced. As a result jepsen_routing.rs only asserts over constants the runner never reads, faults.rs is a binary module outside the lib and so is unreachable by the integration test, and reverting the faults.rs rewire back to the old WYRD_TIER1_JEPSEN_CMD shell-out leaves every routing test green. The dispatch the brief requires be "unit-tested" (brief.md:38-42) has no flippable test bound to it. What to change next (carry-forward): - Make run_jepsen_consistency_test consume jepsen::consistency_test_cargo_args() as the single source of truth: delete the hand-typed arg list in faults.rs and build the Command from the shared function, so reverting the rewire breaks the test. - Then bind the test to the production path — assert over the runner-built command, or drive run_jepsen's Plan::Run branch directly — so the red fails iff the dispatch regresses to the external shell-out. Compile-seam-over-scaffolding is not sufficient; this was the iter-6 rejection class.
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
