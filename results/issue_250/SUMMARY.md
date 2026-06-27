# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 consistency-over-repair leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **STRUCTURAL DECISION (maintainer-chosen Option B, mirroring the two
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`; no
- Scope (one logical fix) / out of scope: Build the Tier-1 consistency-over-repair leg as an in-repo Rust scenario (Option B):

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
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

# Check review — issue 250 / tier1-jepsen-consistency-harness

Advisory, artifact-only. Inputs: `patch.diff`, `brief.md`, `check-gates.json`
(`build-notes.md` withheld by design). **$PDCA_TARGET was not reachable from the
review sandbox** (no `xtask/src/faults.rs` resolvable; `env`/expansion blocked),
so per the fallback rule every citation below grounds on `patch.diff` line
numbers, not target `path:line`. This is a target-state caveat, not a patch
defect — I do **not** raise any "cannot apply / cannot compile" blocker.

## Re-derived central finding (load-bearing)

The brief's primary, eight-iteration-recurring requirement (Success criterion §1;
Test file; Invariant): **"Reverting `run_jepsen` to `execute(…,
"WYRD_TIER1_JEPSEN_CMD")` must turn the test red."** It does not.

- The flippable test `jepsen_dispatch_routes_to_in_repo_scenario_not_external_command`
  (patch 1573-1607) asserts **only** on the return value of `jepsen_scenario_args()`
  (patch 1298-1309). It never references `run_jepsen`, `run_jepsen_scenario`, or
  `run_jepsen_test`.
- `jepsen_scenario_args()` is a standalone `pub(crate)` fn. Reverting `run_jepsen`'s
  `Plan::Run => run_jepsen_scenario()` (patch 1345) back to the external shell-out
  leaves `jepsen_scenario_args()` intact, so the test stays **green**.
- The dispatch test is therefore decoupled from the dispatch decision — the iter-6/7/8
  tautology re-shaped, not closed. This is independently confirmed by the non-gating
  gate `C4-verify`: *"the test PASSES without the fix, so it does not catch the bug
  (no red)."*

The gating gate `C4-ci` (xtask ci: fmt/clippy/build/test/deny/conformance) passed —
the harness compiles, type-checks, and the oracle unit tests + negative controls run.
That proves the born-at-tier compile seam and the oracle, but **not** the routing flip
the brief makes load-bearing.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and testable — three binding parts + the explicit red-iff-shell-out flip bar (brief §Success criterion §1-3); spec well-formed. |
| C2 Reproduction (red pre-fix) | FAIL | The asserted pre-fix red is not reproduced: the dispatch-reversion the brief names leaves the bound test green (patch 1573-1607 reads only `jepsen_scenario_args()` 1298-1309). Human must decide if a non-reproducing red is acceptable for the leg whose whole point was a real red. |
| C3 Change | PASS | Coherent, single-purpose: removes `execute`/`run_shell` shell-out, routes to in-repo scenario gating on `docker`, adds scenario + oracle + workflow, mirrors merged siblings (patch 1328-1542). |
| C4 Verification (red→green) | FAIL | Re-derived: reverting `run_jepsen` dispatch (patch 1345) does **not** turn the test red — it only tests a standalone fn the runner happens to call; matches non-gating `C4-verify` FAIL. `C4-ci` (gating) passed but only verifies compile + oracle, not the routing flip. Decision owed: this is the exact iter-6/7/8 failure the re-plan targeted — accept a routing seam that does not flip on regression? |
| C5 Causal adequacy | NEEDS-HUMAN | The defect's core ("no test bound to the routing it claims") is not causally closed — the test binds to `jepsen_scenario_args()`, not to `run_jepsen`'s dispatch choice. Human must decide whether the routing-seam root cause is actually removed or merely re-papered. (No capability-probe-over-load-time smell: `tool_available("docker")` is the designed sibling gating pattern, not a guard over a present capability.) |
| T1 Structure | PASS | Jepsen compose helpers namespaced `wyrd-tier1-jepsen` to avoid Tier-2 collision; phases/oracles cleanly separated; `finalize_panic_safe`/`finish_integration` reuse mirrors the merged Tier-2 pattern (patch 1354-1542). |
| T2 Shape | PASS | One shared oracle: the five `assert_*` helpers are called by both the live scenario body (patch 899, 1032, 1038, 1132) and the Check unit tests (patch 467-641) — no decorative second oracle (the iter-8 finding addressed). |
| T3 Runtime | NEEDS-HUMAN | Only the oracle unit tests + compile run at Check; the live partition/crash/heal runtime (the property the leg exists to demonstrate) is `#[ignore]`d and deferred to the privileged `tier1-jepsen.yml` job. Human must confirm the deferred live green, since no Check evidence exercises the production reconcile path. |
| T4 Contribution | NEEDS-HUMAN | Option B changes the *how* of accepted, immutable proposal 0005, which names "Jepsen" literally (`0005:408`). Brief pre-declares this: maintainer must decide whether a new clarifying/superseding ADR is required to record the in-repo Rust scenario as the Tier-1 substrate. |
| T5 Judgment | NEEDS-HUMAN | Given C2/C4 reproduce the recurring routing-seam gap, the maintainer must judge whether this iteration genuinely advances past iter-6/7/8 or repeats it; also the two pre-declared follow-on issues (literal-Jepsen artifact; missing-fragment detection product gap) must be filed. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether this leg actually makes Wyrd's ADR-0015 contract-over-repair true under partition+crash is decided only by the maintainer reviewing the first live `tier1-jepsen.yml` (`workflow_dispatch`) run — off-Check by declared posture. |

## Notes for sign-off

- **Prior-art check ran** by affected path (brief §Prior-art): no Tier-1 Jepsen
  scenario or `tier1-jepsen.yml` exist; siblings #195/#196 are pattern precedent, not
  duplicates. Mechanically settled — not raised as a separate NEEDS-HUMAN.
- **Fork discipline** §3/§4 do not apply: this targets getwyrd/wyrd `main` directly,
  not a cross-version cherry-pick onto a drifted fork branch.
- The decisive blocker is C2/C4: the flippable regression is not bound to
  `run_jepsen`'s dispatch, so the brief's red-iff-shell-out bar is unmet. Recommend
  the test invoke the real dispatch decision (e.g. assert over the value `run_jepsen`/
  `run_jepsen_scenario` actually consumes on the `Plan::Run` path) so a reversion to
  `execute(…, "WYRD_TIER1_JEPSEN_CMD")` flips it red.

### Advisory — codex

- NEEDS-HUMAN — `xtask/src/faults.rs:544`: the regression test still only calls `jepsen_scenario_args()` directly, so it does not prove that `run_jepsen` consumes that route. A regression that changes the `Plan::Run` arm at `xtask/src/faults.rs:194` back to an external shell-out while leaving the helper in place would keep this test green, which misses the brief's required red iff dispatch regresses to `WYRD_TIER1_JEPSEN_CMD`.
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:833`: the partition phase pauses a D-server and then awaits `reconcile_step`, but `GrpcChunkStore::get_fragment` awaits the tonic RPC without any request deadline at `crates/chunkstore-grpc/src/client.rs:87`. A `docker pause`d container can leave the call hanging instead of returning the expected transient error, so the privileged nightly is likely to run until the 45-minute workflow timeout rather than exercising the heal/convergence assertions.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The defect's core ("no test bound to the routing it claims") is not causally closed — the test binds to `jepsen_scenario_args()`, not to `run_jepsen`'s dispatch choice. Human must decide whether the routing-seam root cause is actually removed or merely re-papered. (No capability-probe-over-load-time smell: `tool_available("docker")` is the designed sibling gating pattern, not a guard over a present capability.)
- [ ] T3 Runtime — Only the oracle unit tests + compile run at Check; the live partition/crash/heal runtime (the property the leg exists to demonstrate) is `#[ignore]`d and deferred to the privileged `tier1-jepsen.yml` job. Human must confirm the deferred live green, since no Check evidence exercises the production reconcile path.
- [ ] T4 Contribution — Option B changes the *how* of accepted, immutable proposal 0005, which names "Jepsen" literally (`0005:408`). Brief pre-declares this: maintainer must decide whether a new clarifying/superseding ADR is required to record the in-repo Rust scenario as the Tier-1 substrate.
- [ ] T5 Judgment — Given C2/C4 reproduce the recurring routing-seam gap, the maintainer must judge whether this iteration genuinely advances past iter-6/7/8 or repeats it; also the two pre-declared follow-on issues (literal-Jepsen artifact; missing-fragment detection product gap) must be filed.
- [ ] Validation — fitness-to-purpose — Whether this leg actually makes Wyrd's ADR-0015 contract-over-repair true under partition+crash is decided only by the maintainer reviewing the first live `tier1-jepsen.yml` (`workflow_dispatch`) run — off-Check by declared posture.
- [ ] `xtask/src/faults.rs:544`: the regression test still only calls `jepsen_scenario_args()` directly, so it does not prove that `run_jepsen` consumes that route. A regression that changes the `Plan::Run` arm at `xtask/src/faults.rs:194` back to an external shell-out while leaving the helper in place would keep this test green, which misses the brief's required red iff dispatch regresses to `WYRD_TIER1_JEPSEN_CMD`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: discontinued
- Iteration delta (if iterating): Why discontinued: eighth recurrence of the same routing-seam gap (C2/C4). The flippable test `jepsen_dispatch_routes_to_in_repo_scenario_not_external_command` (patch.diff:1573) binds only to `jepsen_scenario_args()` (patch.diff:1298), not to `run_jepsen`'s `Plan::Run => run_jepsen_scenario()` dispatch (patch.diff:1345), so reverting the dispatch to the external `WYRD_TIER1_JEPSEN_CMD` shell-out leaves the test green — the brief's load-bearing red-iff-reversion bar is unmet, as in iter-6/7/8. Repeated PDCA Do/Plan iterations have not closed it. Where the work goes instead: the maintainer will do the implementation interactively, outside the PDCA cycle. Not handed to a fresh PDCA brief.
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
