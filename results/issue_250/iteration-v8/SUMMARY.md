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

# Check review — issue 250 / tier1-jepsen-consistency-harness (iteration 8)

> Advisory, artifact-only, decorrelated. Inputs: patch.diff, brief.md, check-gates.json
> (build-notes.md withheld by design). `$PDCA_TARGET` was unreadable in this sandbox
> (env/printenv access blocked), so per protocol citations ground on `patch.diff` alone.
> Gating gate C4-ci (`cargo xtask ci`) = PASS and is not disputed as a mechanical result;
> the findings below concern what that green does and does not verify.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief carries a crisp BINDING success criterion (brief.md:30-59) with three deliverables — dispatch rewire, in-repo Rust scenario, privileged workflow — all present in the diff; spec is well-formed and the patch targets it. |
| C2 Reproduction (red pre-fix) | FAIL | The brief's own flippable regression (brief.md:155-160) — assert `run_jepsen`'s dispatch routes to the in-repo scenario *rather than* the external `WYRD_TIER1_JEPSEN_CMD` — is still absent. Routing tests (patch.diff:1651-1664, 1192-1207) assert over the constant `consistency_test_cargo_args()`, never over the runner. By the patch's own admission (patch.diff:1628-1633) the "red" is a compile failure from deleting `xtask/src/jepsen.rs`, not a behavioral dispatch flip. A regression that re-points `run_jepsen` to the shell-out + drops the orphan helpers leaves every routing test green — the iter-6/7 rejection recurs, restructured. **Decision owed:** does a module-deletion compile-seam red satisfy "the dispatch wiring is unit-tested" (brief.md:38-42), or is this the same unbuilt-binding the maintainer rejected twice? |
| C3 Change | PASS | The three structural deliverables land coherently and single-purpose: `run_jepsen` rewired off `execute(...,"WYRD_TIER1_JEPSEN_CMD")` to `run_jepsen_scenario`→`run_jepsen_consistency_test` (patch.diff:1069-1160), `execute`/`run_shell` deleted, single-sourced args added (patch.diff:1139-1145), scenario + workflow net-new. |
| C4 Verification (red→green) | NEEDS-HUMAN | C4-ci (fmt/clippy/build/test) is genuinely green and verifies the new code *compiles and type-checks* (the #[ignore]d scenario binds the production `reconcile_step`/`reconstruction::reconcile` API). But the per-fix C4-verify red→green rests on the C2 compile seam, not a test that flips with the dispatch. **Decision owed:** accept compile-only exercise of the dispatch as adequate verification, or hold for a test that fails iff `run_jepsen` routes to the external command — given two prior C4-verify rejections on this exact point. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause per brief is "the leg was never built — inert dispatch scaffolding" (brief.md:20-28). The patch builds real harness code, so it is not a symptom-guard (the `consistency_required_tool()=="docker"`/`tool_available` probe at patch.diff:1071-1077 is the sanctioned deferred-tier gate mirroring siblings, not a guard over a load-time cause — C5 smell-test does not fire). But "built" is demonstrated at Check only by compilation; the production dispatch path is never executed or asserted. **Decision owed:** is the production repair path genuinely "driven and the contract asserted" as the invariant demands (brief.md:61-80), or still scaffolding whose only Check exercise is a type-check? |
| T1 Structure | PASS | New `xtask::jepsen` module wired via lib.rs + main.rs (patch.diff:1580-1595), scenario sibling to `tier2_kill_reconstruct.rs`, workflow modelled on `tier2-kill-reconstruct.yml` — mirrors the two merged sibling legs as the brief directs. |
| T2 Shape | PASS | Helpers are pure, host-independent functions. Note (non-blocking): two *separate* oracles exist — lib `xtask::jepsen::check_read_after_commit`/`check_no_duplicate_placement` over a `ConsistencyEvent` history (patch.diff:1294-1393, `#[allow(dead_code)]` in the binary build) versus the scenario's own `assert_no_torn_reads`/`assert_read_after_commit_from_survivors`/`assert_repair_fired` (patch.diff:304-412). The lib oracle the Check tests cover is not the oracle the live scenario uses — surfaced under T4. |
| T3 Runtime | NEEDS-HUMAN | The live consistency run (real cluster, partitions, crashes mid-repair) is deferred/off-Check by design (ADR-0016); not observable at Check. **Decision owed:** maintainer (Eduard Ralph) must confirm the first `tier1-jepsen.yml` `workflow_dispatch` run is green — read-after-commit holds, no torn/stale reads, repair neither lost nor duplicated — before the deferred-posture leg is trusted. |
| T4 Contribution | NEEDS-HUMAN | Option B substance + the `enqueue_repair` #196 stand-in (patch.diff:708) are within the accepted scope, and prior-art-by-path ran (brief.md:213-227: faults.rs history `0b5fea3`/`02983aa`, no existing tier1-jepsen.yml). **Decision owed:** the pre-declared ADR item — Option B changes the *how* of proposal 0005, which names "Jepsen" literally (`0005:408`); weigh a clarifying/superseding ADR — plus filing the two follow-on issues (literal-Jepsen artifact; missing-fragment detection gap), and whether the unused lib `ConsistencyEvent` oracle (T2) earns its keep or is decorative coverage. |
| T5 Judgment | NEEDS-HUMAN | Taste/judgment call hinges on the recurring C2/C4 verification-binding defect. Fork-discipline: target is getwyrd/wyrd@main (no maintenance branches), not a cross-version cherry-pick, so §3/§4 are not the live risk; validation is against clean main. **Decision owed:** does the maintainer accept the single-sourced-args + compile-seam approach as "built and unit-tested," or require the dispatch test the brief and two prior sign-offs called for before sign-off. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Always human at sign-off. **Decision owed:** does this leg actually make Wyrd's ADR-0015 consistency contract over the repair path *true and demonstrated* (not merely compilable), and is the deferred live-green evidence sufficient — the fitness question the five prior iterations kept failing. |

## Lead finding (advisory, non-gating)

C2 FAIL is the iteration-6/7 defect recurring in restructured form. Iter-7's fix list had
two steps; iter-8 completed step 1 (single-source `consistency_test_cargo_args()`) but not
step 2 (**bind a test to the production dispatch**). The fix needed is small and named in the
carry-forward: assert over the command `run_jepsen_consistency_test` actually builds, or drive
`run_jepsen`'s `Plan::Run` branch, so the red fails iff the dispatch regresses to the external
shell-out — not merely because the net-new module was deleted.

### Advisory — codex

- `xtask/src/jepsen.rs:140` — `check_read_after_commit` treats `ReadObserved { write_id: None }` as valid even after a `WriteCommitted` event, and the added test codifies that behavior at `xtask/tests/jepsen_orchestration.rs:104`. That weakens the ADR-0015 read-after-commit oracle: a committed value becoming unreadable after repair should be a violation, not an allowed “not found” read.
- NEEDS-HUMAN — `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:607` — the live scenario injects a container kill, but I do not see any real network partition injection before or during the repair path. The brief/workflow describe “partitions and crashes”; as written, the harness appears to cover crash/kill-and-repair consistency only, not consistency under partition.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — C4-ci (fmt/clippy/build/test) is genuinely green and verifies the new code *compiles and type-checks* (the #[ignore]d scenario binds the production `reconcile_step`/`reconstruction::reconcile` API). But the per-fix C4-verify red→green rests on the C2 compile seam, not a test that flips with the dispatch. **Decision owed:** accept compile-only exercise of the dispatch as adequate verification, or hold for a test that fails iff `run_jepsen` routes to the external command — given two prior C4-verify rejections on this exact point.
- [ ] C5 Causal adequacy — Root cause per brief is "the leg was never built — inert dispatch scaffolding" (brief.md:20-28). The patch builds real harness code, so it is not a symptom-guard (the `consistency_required_tool()=="docker"`/`tool_available` probe at patch.diff:1071-1077 is the sanctioned deferred-tier gate mirroring siblings, not a guard over a load-time cause — C5 smell-test does not fire). But "built" is demonstrated at Check only by compilation; the production dispatch path is never executed or asserted. **Decision owed:** is the production repair path genuinely "driven and the contract asserted" as the invariant demands (brief.md:61-80), or still scaffolding whose only Check exercise is a type-check?
- [ ] T3 Runtime — The live consistency run (real cluster, partitions, crashes mid-repair) is deferred/off-Check by design (ADR-0016); not observable at Check. **Decision owed:** maintainer (Eduard Ralph) must confirm the first `tier1-jepsen.yml` `workflow_dispatch` run is green — read-after-commit holds, no torn/stale reads, repair neither lost nor duplicated — before the deferred-posture leg is trusted.
- [ ] T4 Contribution — Option B substance + the `enqueue_repair` #196 stand-in (patch.diff:708) are within the accepted scope, and prior-art-by-path ran (brief.md:213-227: faults.rs history `0b5fea3`/`02983aa`, no existing tier1-jepsen.yml). **Decision owed:** the pre-declared ADR item — Option B changes the *how* of proposal 0005, which names "Jepsen" literally (`0005:408`); weigh a clarifying/superseding ADR — plus filing the two follow-on issues (literal-Jepsen artifact; missing-fragment detection gap), and whether the unused lib `ConsistencyEvent` oracle (T2) earns its keep or is decorative coverage.
- [ ] T5 Judgment — Taste/judgment call hinges on the recurring C2/C4 verification-binding defect. Fork-discipline: target is getwyrd/wyrd@main (no maintenance branches), not a cross-version cherry-pick, so §3/§4 are not the live risk; validation is against clean main. **Decision owed:** does the maintainer accept the single-sourced-args + compile-seam approach as "built and unit-tested," or require the dispatch test the brief and two prior sign-offs called for before sign-off.
- [ ] Validation — fitness-to-purpose — Always human at sign-off. **Decision owed:** does this leg actually make Wyrd's ADR-0015 consistency contract over the repair path *true and demonstrated* (not merely compilable), and is the deferred live-green evidence sufficient — the fitness question the five prior iterations kept failing.
- [ ] `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:607` — the live scenario injects a container kill, but I do not see any real network partition injection before or during the repair path. The brief/workflow describe “partitions and crashes”; as written, the harness appears to cover crash/kill-and-repair consistency only, not consistency under partition.

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
- Iteration delta (if iterating): Why rejected: three iterations (6, 7, 8) have hit the same wall. The brief asks for a Check test that flips when run_jepsen's dispatch regresses to the external WYRD_TIER1_JEPSEN_CMD shell-out, but the module's structure has nowhere to host such a test. The routing decision ("which harness to run") is welded inside the private, docker-spawning run_jepsen_scenario / run_jepsen_consistency_test (xtask/src/faults.rs); the only pure seams the design exposes are plan() (the gating decision) plus the extracted consistency_test_cargo_args() constant. A test over that constant does not prove the dispatch actually uses it, and C4-verify is satisfied by a net-new-module compile-seam red (revert -> jepsen.rs gone -> use fails to compile), so neither the gate nor the available test surface ever forces a behavioral flip. This is a design gap, not a builder miss: re-running the same brief will reproduce the same shape. #250 is the first tier whose harness actually changes (external shell-out -> in-repo scenario), so "route to X not Y" is a meaningful claim for the first time, and the inherited sibling structure has no slot for it. Re-plan the seam, not just the test: 1. Routing seam (primary). Restructure run_jepsen so the routing decision is a returnable value (e.g. the cargo invocation, or a Plan-like enum) that a Check-time unit test can assert routes to the in-repo scenario rather than the external env command, with the docker spawn placed downstream of that value. The flippable regression must fail iff the dispatch regresses to the shell-out, not merely because a net-new module was deleted. 2. Partition injection (scope decision). Tier-1 MUST inject a real network partition, not only a container kill. The brief and tier1-jepsen.yml promise "partitions and crashes" (0005:408); the current scenario covers crash/kill-and-repair only. Specify the partition fault in the brief so the harness delivers what the milestone names. 3. Dual oracle (cleanup). The lib ConsistencyEvent oracle that the Check tests cover (xtask/src/jepsen.rs check_read_after_commit / check_no_duplicate_placement) is NOT the oracle the live scenario uses (the scenario's own assert_* helpers), so Check coverage exercises an oracle the real run never touches. Unify them or drop the decorative one. Also fix that check_read_after_commit treats a committed value becoming unreadable (ReadObserved { write_id: None } after a WriteCommitted) as valid, which weakens the ADR-0015 read-after-commit guarantee.
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
