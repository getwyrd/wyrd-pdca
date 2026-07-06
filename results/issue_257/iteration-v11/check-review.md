# Check review — issue_257 / m4.6-real-commit-over-madsim-tikv (iteration 11)

**Task under review.** A test-evidence slice for M4: extend the Tier-1 (integration +
consistency-over-the-swap, in-repo Rust scenario per ADR-0039) and Tier-2 lines across the
redb→TiKV metadata-backend swap, plus a determinism-gap DST seed — without editing
`crates/metadata-tikv/src` or `crates/traits`. This is **iteration 11**; iterations 1–9's
posture (Option-B: the real ADR-0015-on-TiKV correctness proof lives off-Check on a privileged
≥3-replica cluster; the at-Check bar is pure oracles + dispatch + a coverage-only seed) is
**ratified and explicitly not to be re-opened**. The iteration-10 rejection was **narrow — two
fixes to the newly-added feature-gated compile step**: (1) it was wired *unconditionally* into
`run_ci`, breaking the no-TiKV-CI invariant by compiling the pre-1.0 `tikv-client` tree; (2) its
guard test only restated the `feature_gated_checks()` constant (the tautology shape), staying
green if the wiring loop were deleted. This iteration must fix exactly those two.

**Grounding caveat.** `PDCA_TARGET` is unset and no target checkout is present, so per protocol I
ground every citation on `patch.diff` and **cannot independently re-run** `cargo xtask ci` or
`run-verify.sh`. Line cites below are `patch.diff` line numbers. The recorded gate results
(`check-gates.json`) are reflected as recorded, not re-executed — this is a target-state
limitation, **not** a patch defect, and I do not raise it as a C4 blocker.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Iteration-11 spec is the narrow iteration-10 directive: gate the feature-gated compile behind a toolchain flag, and make its guard test exercise real wiring (brief §"Iteration 10 carry-forward"). Well-scoped and unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | No gate configured, but the pre-fix red is reasoned structurally from the diff: the iter-10 tautology test stayed green if the wiring loop was deleted; the replacement (patch.diff:1659-1698) drives `run_ci_steps` with a recording executor and asserts the metadata check IS invoked with toolchain vs is NOT without — deleting the loop or dropping the guard flips a *behavioural* assertion (not a compile error). Could not execute (target unset). |
| C3 Change | PASS | Change is confined and invariant-preserving: `run_ci` refactored to `run_ci_steps(tikv_toolchain_available(), …)` (patch.diff:1599-1645), new `tikv_toolchain_available()` env gate (patch.diff:1588-1590), rewritten guard test (patch.diff:1659-1698). Byte-for-byte no `crates/metadata-tikv/src` and no `crates/traits` edit — the changed-file set (Cargo.lock, dst seed, metadata-tikv/Cargo.toml dev-dep, tier tests, testkit, deploy compose, xtask) confirms the invariant holds. |
| C4 Verification (red→green) | PASS (on record; not re-run) | `check-gates.json` records `C4-ci` gating PASS ("xtask ci: all checks passed") and `C4-verify` PASS ("red without the fix, green with it"). I could not re-execute (no checkout). Fix-1 is genuine: with `WYRD_TIKV_TOOLCHAIN` unset the `if tikv_toolchain` guard (patch.diff:1634-1638) skips the tikv compile, so the default offline gate stays container-free — the exact iter-10 defect, now closed. Not raised as a FAIL: the unset target is a grounding limitation, not a patch fault. |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: whether the at-Check causal coverage is adequate given the binding redb→TiKV correctness proof lives **entirely off-Check** (Option B, ratified). The env gate `tikv_toolchain_available()` (patch.diff:1588-1590) is a CI-scoping opt-in, not a capability-probe papering a load-time side effect — the C5 symptom-guard smell-test does **not** fire. But the standing question (does the reduced at-Check bar causally certify the swap?) is human-ratified per cycle; this iteration does not disturb it and should not re-open it. |
| T1 Structure | PASS | Testability-by-injection: `run_ci_steps` takes an `exec` closure so the wiring is unit-drivable without spawning cargo (patch.diff:1599-1645); pure routing/arithmetic split into `xtask::metadata_faults` and `wyrd_testkit` (patch.diff:1725-1791, 943-1048). Sound structure. |
| T2 Shape | PASS | Public surface is coherent: `feature_gated_checks()` returns steps as data (patch.diff:1570-1579); testkit exposes independent `partition_took_effect`/`heal_is_complete`/`consistency_passes`/`converged_exactly_once` signals rather than one collapsed bit (patch.diff:995-1048). |
| T3 Runtime | PASS (reasoned; not executed) | The new guard test runs under `cargo test --workspace` inside `cargo xtask ci`; the coverage seed runs under `--cfg madsim` via `run_dst` (per iter-7 ratified finding); pure oracle tests run at Check. All are compiled+run paths, not committed-but-unexecuted. Could not execute here (target unset). |
| T4 Contribution | PASS | The incremental iteration-11 contribution is genuine: it closes a real gate-honesty hole (unconditional tikv compile) and replaces a real tautology (constant-restatement) with a wiring-exercising test. The flagship seed is honestly relabelled as pure redb coverage with NO correctness weight (patch.diff:19-90) — no overclaim. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: accept/reject the slice as a whole. The two narrow iter-10 fixes appear correctly resolved (fix-1 gating at patch.diff:1634-1638; fix-2 non-tautological test at patch.diff:1659-1698), but sign-off must judge whether the accumulated posture — coverage-only seed, off-Check binding legs, toolchain-gated type-check — is acceptable to land, and confirm no ratified piece regressed. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed at sign-off: does the slice satisfy the brief's Success criterion end-to-end, given the binding correctness evidence (live Tier-1 consistency + Tier-2 green on a real ≥3-replica TiKV cluster) is observable **only** in the privileged `WYRD_TIER1`/`WYRD_TIER2` Tier job, which the reviewer cannot run? Human must (a) confirm the privileged Tier-job legs land green, (b) name who confirms them, (c) re-affirm Option-B, the #365 static-endpoints reduced bar, and the #258 ordering (this slice in the earlier wave; metadata-nemesis ADR routed to the architecture board — patch correctly mints no ADR). |

## Notes for the human (§6 candidates)

- **C5 / posture (ratified, do not re-open):** Option-B is settled across iterations; confirm this
  iteration leaves it untouched (it does — no `metadata-tikv/src`/`traits` edit; only the xtask
  gate wiring + test changed).
- **T5:** verify the two iter-10 fixes independently — deleting the `for check in
  feature_gated_checks()` loop (patch.diff:1635-1637) must flip the `with_toolchain` assertion red;
  removing the `if tikv_toolchain` guard (patch.diff:1634) must flip the `without_toolchain`
  assertion red. Both are behavioural, not compile-flips.
- **Validation:** the off-Check Tier-1/Tier-2 green is the binding correctness evidence and cannot
  be reproduced at Check — it needs the privileged Docker host + ≥3-replica cluster. Run: stand up
  `deploy/tikv-multi-replica`, export endpoints + `WYRD_TIER1_ISOLATED_IP`, then
  `cargo xtask metadata-tier1` (WYRD_TIER1=1) and `metadata-tier2` (WYRD_TIER2=1); confirm the
  independent read-after-commit / exactly-once / fault-materialized signals all hold across the heal.
- **Could not re-run gates:** `PDCA_TARGET` unset, no checkout — `cargo xtask ci` / `run-verify.sh`
  results are reflected as recorded in `check-gates.json`, not re-executed by me.
