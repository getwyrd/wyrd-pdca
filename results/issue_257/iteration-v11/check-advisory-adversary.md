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
