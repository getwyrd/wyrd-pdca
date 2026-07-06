# Result — issue 257 / m4.6-real-commit-over-madsim-tikv

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: three layers — one BINDING and demonstrable **at Check**, one BINDING but
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: (i) add the **`madsim-tikv-client` cfg-alias** to `crates/metadata-tikv/Cargo.toml`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind Accepted
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

### Advisory — adversary

# Adversarial review — issue_257 (iteration 11)

Scope: the two iter-10 must-fixes (gate the feature-gated compile step; de-tautologise its
guard) plus the red→green evidence they rest on. Grounded on `$PDCA_TARGET`
(`feat/m4.5-deploy-tikv-pd-etcd`, patch staged). Advisory only — I gate nothing.

## Refutation attempts

- **NEEDS-HUMAN — Under Option B the named defect has *no* catching oracle, at-Check or
  off-Check.** The slice exists to close the "await-inside-`commit()`" determinism gap
  (`brief.md:42-56`): the window between `get_for_update` (`crates/metadata-tikv/src/lib.rs:560`)
  and `txn.commit().await` (`:597`). The DST seed explicitly disclaims any TiKV teeth
  ("a production regression in the TiKV commit path **cannot** turn it red",
  `crates/dst/tests/tikv_await_commit_interleaving.rs:25-27`). Option B moved the binding
  correctness bar to the live Tier-1 leg (`brief.md:84-86`) — but that leg
  (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:90-206`) drives a **single,
  sequential client** (confirmed: no `spawn`/`join`/racer anywhere in `run`). Its correctness
  verdict is `converged_exactly_once(version_before, version_after)` (`:200`), which for one
  writer that commits once is `converged_exactly_once(0, 1) == true` *deterministically*,
  independent of whether the commit-point re-check exists. Concretely: weaken `get_for_update`
  to a plain `get` at `lib.rs:560` (the iter-8 acceptance perturbation) and **nothing flips** —
  the seed is redb-only, and the Tier-1 leg has no second writer to lose an update. This is
  exactly the "exactly-one-winner / convergence never changes against a linearizable store"
  hollow flip the Invariant forbids (`brief.md:139`). Whether "correctness authority stays with
  the existing DST/`concurrency.rs`" (Invariant, `brief.md:135-138`) legitimately excuses this,
  or whether the slice's whole thesis is now unmet-but-relabelled, is a sign-off call.

- **NEEDS-HUMAN — the "type-checked at Check" claim may never have been exercised by the
  recorded C4-ci pass.** `feature_gated_checks()` runs only when `WYRD_TIKV_TOOLCHAIN` is set
  (`xtask/src/main.rs:846-847`, emitted conditionally at `:887-891`). The default container-free
  `cargo xtask ci` skips it, so the ~400-line `#[cfg(feature = "tikv")]` scenario
  (`SymmetricPartition`, `pd_store_state`, and the `partition_took_effect`/`heal_is_complete`/
  `consistency_passes` wiring in `tier1_metadata_consistency.rs:90-471`) is **not compiled** by
  any step of a default run. `check-gates.json:33-40` records C4-ci "all checks passed" but does
  not reveal whether `WYRD_TIKV_TOOLCHAIN` was set for that run. If it was not, the iter-9 defect
  is unfixed — a type error inside the scenario still passes the gate — and the docstring's
  "compiles and type-checks it in the whole-tree gate" (`tier1_metadata_consistency.rs:270-275`)
  is false for the run that actually gated. Confirm the recorded C4-ci run exported
  `WYRD_TIKV_TOOLCHAIN`.

- **The tautology-guard fix stops one function short of the iter-10 directive.** iter-10 asked
  that the test "assert `run_ci` invokes the feature-gated check." The new test
  (`xtask/src/main.rs:1128`) drives `run_ci_steps(true/false, recorder)` directly and **never
  calls `run_ci`** nor asserts `run_ci` threads `tikv_toolchain_available()` into it. Concrete
  regression that stays green: replace `run_ci`'s call (`main.rs:898`) with
  `run_ci_steps(false, &mut |a| cargo(a))` — hardcoding the gate off. Then even with the
  toolchain present the feature check never runs and the scenario is never type-checked, yet the
  test passes because it exercises `run_ci_steps(true, …)` directly. This is strictly better than
  the iter-10 tautology (deleting the wiring loop now *does* flip it red), but the
  `run_ci → tikv_toolchain_available → run_ci_steps` seam remains untested. Adjudicate whether
  that meets the directive.

- **Minor (advisory).** `tikv_toolchain_available()` uses `std::env::var_os(...).is_some()`
  (`xtask/src/main.rs:847`), so `WYRD_TIKV_TOOLCHAIN=0` or `=""` counts as *present* and turns
  the tikv compile **on**. The container-free-when-unset invariant still holds; this only
  surprises someone who sets the var to a falsey value. Low severity.

## Attempted but could not refute

- The pure `wyrd-testkit` oracles (`quorum`, `partition_outcome`, `partition_materialized`,
  `converged_exactly_once`, `consistency_passes`, `partition_took_effect`, `heal_is_complete`,
  `crates/testkit/src/lib.rs:946-1049`) — their unit tests use hand-computed expectations, not
  the literal the function returns (`:1066-1199`); a `total/2` off-by-one or a collapsed-signal
  regression genuinely flips them red. These run under default features at Check. Non-tautological.
- The `metadata_tier_dispatch` routing (`xtask/src/metadata_faults.rs:1761`) and its tests
  (`xtask/tests/metadata_faults_orchestration.rs`) — thin, but a default-route inversion flips
  them, mirroring the ratified `jepsen_dispatch` shape.
- The DST seed is honestly relabelled as pure redb coverage claiming no TiKV correctness weight
  (`tikv_await_commit_interleaving.rs:23-71`) — no tautological "self-authored correct branch
  survives" claim remains; exit (b) is executed faithfully.
- The iter-8 codex advisories are addressed: `heal()` sets `healed=true` only on full success
  (`tier1_metadata_consistency.rs:594-621`), and `wait_metadata_cluster_ready` gates on PD/store
  port readiness after `up -d` (`xtask/src/faults.rs:1416-1429`).

Note: I could not re-run the asserted C4-verify red→green — `run-verify.sh` is a harness script
not present under `$PDCA_TARGET`, so its perturbation target is not inspectable from the provided
inputs ({patch.diff, brief.md, check-gates.json}). Given the seed is pure coverage and the
scenario is off-Check, any behavioural at-Check flip must rest on the pure oracle/dispatch unit
tests above; that it is *behavioural* rather than a compile-flip (iter-7 must-fix 5 / v3) could
not be verified here.

### Advisory — codex

- NEEDS-HUMAN — `crates/dst/tests/tikv_await_commit_interleaving.rs:37` says the new Check-time DST seed is redb-only and exhibits "no newly-reachable interleaving", while the Option-B brief still required the fallback seed to be a determinism-gap coverage artifact for the TiKV await-inside-commit interleaving. This may leave the patch with no at-Check artifact for the specific gap it was meant to cover.
- NEEDS-HUMAN — `xtask/src/faults.rs:681` only waits for PD/store TCP ports before running the Tier-1 scenario, while `deploy/tikv-multi-replica/docker-compose.yml:11` relies on PD's default replication factor for "every region" placement. If region peer placement/leader health is still asynchronous after port readiness, the test can start before the exercised keys are actually replicated across the 3-store Raft group, making the minority-voter partition evidence weaker or vacuous.
- NEEDS-HUMAN — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:405` treats PD's `state_name == "Up"` as the peer-side liveness signal and `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:412` expects it to stop being Up within 45s after the iptables cut. Please confirm PD's `/pd/api/v1/stores` `state_name` changes on heartbeat loss for this TiKV version; if it remains the administrative state while liveness is reported elsewhere, a real partition will be classified as a no-op and the privileged Tier-1 leg will always fail.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decision owed: whether the at-Check causal coverage is adequate given the binding redb→TiKV correctness proof lives **entirely off-Check** (Option B, ratified). The env gate `tikv_toolchain_available()` (patch.diff:1588-1590) is a CI-scoping opt-in, not a capability-probe papering a load-time side effect — the C5 symptom-guard smell-test does **not** fire. But the standing question (does the reduced at-Check bar causally certify the swap?) is human-ratified per cycle; this iteration does not disturb it and should not re-open it.
- [ ] T5 Judgment — Decision owed: accept/reject the slice as a whole. The two narrow iter-10 fixes appear correctly resolved (fix-1 gating at patch.diff:1634-1638; fix-2 non-tautological test at patch.diff:1659-1698), but sign-off must judge whether the accumulated posture — coverage-only seed, off-Check binding legs, toolchain-gated type-check — is acceptable to land, and confirm no ratified piece regressed.
- [ ] Validation — fitness-to-purpose — Decision owed at sign-off: does the slice satisfy the brief's Success criterion end-to-end, given the binding correctness evidence (live Tier-1 consistency + Tier-2 green on a real ≥3-replica TiKV cluster) is observable **only** in the privileged `WYRD_TIER1`/`WYRD_TIER2` Tier job, which the reviewer cannot run? Human must (a) confirm the privileged Tier-job legs land green, (b) name who confirms them, (c) re-affirm Option-B, the #365 static-endpoints reduced bar, and the #258 ordering (this slice in the earlier wave; metadata-nemesis ADR routed to the architecture board — patch correctly mints no ADR).
- [ ] `crates/dst/tests/tikv_await_commit_interleaving.rs:37` says the new Check-time DST seed is redb-only and exhibits "no newly-reachable interleaving", while the Option-B brief still required the fallback seed to be a determinism-gap coverage artifact for the TiKV await-inside-commit interleaving. This may leave the patch with no at-Check artifact for the specific gap it was meant to cover.
- [ ] `xtask/src/faults.rs:681` only waits for PD/store TCP ports before running the Tier-1 scenario, while `deploy/tikv-multi-replica/docker-compose.yml:11` relies on PD's default replication factor for "every region" placement. If region peer placement/leader health is still asynchronous after port readiness, the test can start before the exercised keys are actually replicated across the 3-store Raft group, making the minority-voter partition evidence weaker or vacuous.
- [ ] `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:405` treats PD's `state_name == "Up"` as the peer-side liveness signal and `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:412` expects it to stop being Up within 45s after the iptables cut. Please confirm PD's `/pd/api/v1/stores` `state_name` changes on heartbeat loss for this TiKV version; if it remains the administrative state while liveness is reported elsewhere, a real partition will be classified as a no-op and the privileged Tier-1 leg will always fail.

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
- Iteration delta (if iterating): Rejected: the binding Tier-1 correctness leg FAILS in a live ≥3-replica run (privileged host, host-networked pingcap/tikv+pd v8.5.1). Empirically confirmed codex §6 item-6: ConsistencySignals { read_after_commit: true, converged_once: true, fault_materialized: false } — tier1_metadata_consistency.rs:203 panics. Root cause (not flaky infra): the fault-effect oracle keys off the WRONG PD field. pd_sees_target_up() (tier1 test:403) / pd_still_sees_target_up_after() (:412) treat PD /pd/api/v1/stores `state_name == "Up"` as "connected". For PD v8.5.1 `state_name` is the ADMINISTRATIVE state — it stays "Up" through a short partition and only flips to "Down" after max-store-down-time (default ~30min). The scenario's window is ~45s, so state_name never leaves "Up" → partition_took_effect() = false → fault_materialized = false → the leg can NEVER pass, regardless of a real cut. The iptables partition itself is sound (host networking, distinct loopback IPs 127.0.0.1/.2/.3 — the cut applies); only the oracle is wrong. What to change (Do rebuild): - Replace the state_name oracle with a transient-liveness signal: read the target store's `last_heartbeat` from /pd/api/v1/stores and assert it goes STALE (> a few heartbeat intervals) during the cut, or use pd-ctl's derived disconnected status / region leader-health. Do NOT rely on `state_name`. - Re-validate BOTH directions on the live cluster before claiming green: a real cut must set fault_materialized=true (leg passes), and the no-op negative control (skip iptables) must still classify as no-op (leg fails). Capture the logs. - Keep the ratified pieces intact (Option-B posture, the two iter-10 fixes: toolchain gating at run_ci_steps + the non-tautological guard test); no brief/plan change needed — this is a harness-implementation defect only. Not re-opened / still owed after the fix (unchanged NEEDS-HUMAN): item 5 — confirm a WYRD_TIKV_TOOLCHAIN=1 `cargo xtask ci` run actually type-checks the #[cfg(feature= "tikv")] bodies; items 3(a)/(b) — a named human confirms the fixed Tier-1/Tier-2 legs land green.
- By / date: Eduard Ralph / 2026-07-05

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
