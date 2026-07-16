# Build notes — issue 407 (m4-metadata-nemesis-partition-skew-pause), iteration 2

## What this iteration changes vs v1 (the carry-forward, point by point)

v1's Check-core (oracle arithmetic, enumeration, red→green) was accepted as sound; the live-leg
half was rejected on five defects. Each is addressed below, with the target-branch citation.

### 1. Pause lifecycle must ENCLOSE the workload (was: unpaused before the workload ran)

**Root cause of the redesign.** In v1 `ProcessPauseLeg::confirm_materialized` unpaused (`docker
unpause`) and polled `served_after` — but `drive_leg` runs the workload *after*
`confirm_materialized`, so the workload ran on an already-unpaused node. Fixed by restructuring the
whole `drive_leg` contract so the fault **encloses** the workload
(`crates/metadata-fault-conformance/src/nemesis.rs`, `drive_leg` at ~:290):

```
plan → apply(inject) → confirm_materialized(sample, DO NOT heal) → [gate] → workload() → heal → confirm_healed
```

- `PauseEvidence` (nemesis.rs:~155) no longer carries `served_after`; its `materialized()` gate is
  `served_before && !served_during && inspected_paused_during` — provable *while still paused*.
- The third serve→pause→**serve** transition is now the heal gate: `confirm_healed` polls the
  survivor's view and `heal_is_complete(applied_rules, healed, live_after)` enforces it — the same
  recovery contract the partition leg already had. This is *uniform* across all three legs, not a
  pause special-case.
- `ProcessPauseLeg::confirm_materialized` (nemesis.rs:~610) now runs a **45-second settle-window
  poll** for `!target_served()` (mirrors `PartitionLeg::confirm_materialized` and
  `MasterIsolation::peers_still_see_target_live_after`, `tier1_metadata_consistency.rs:279`) and
  **does not unpause** — the single immediate probe that made v1 near-deterministically
  inconclusive is gone.

### 2. Skew leg default triple-mismatch (service / override / probe disagreed)

v1: override hardcoded `fdb1`, test defaulted `WYRD_TIER1_SKEW_SERVICE=fdb2`, and the probe read
`all.last()` independent of `service` — a default run could never materialize. Fixed by making the
**runner the single source of truth**:

- `fdb_faults::run_metadata_nemesis` (xtask/src/fdb_faults.rs:~360) resolves ONE service
  (`FDB_TIER1_SKEW_SERVICE = "fdb2"`, xtask/src/fdb_faults.rs:~350) and exports both
  `WYRD_TIER1_SKEW_SERVICE=fdb2` **and** `WYRD_TIER1_SKEW_CONTAINER=container_of("fdb2")` — the
  probe container is *derived from the same service name*, so it cannot disagree.
- `deploy/fdb-multi-replica/docker-compose.faketime.yml` now targets `fdb2` (was `fdb1`),
  matching the runner.
- `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` `run_clock_skew` reads
  `WYRD_TIER1_SKEW_SERVICE` + `WYRD_TIER1_SKEW_CONTAINER` (no more `all.last()`).
- The false "non-master node" comment is **dropped**: the recreate restabilizes before the
  measured window regardless of role (see #4), so no non-master precondition is needed or enforced.

### 3. A runnable entry point (v1 referenced a nonexistent `run_metadata_nemesis`)

New `cargo xtask metadata-nemesis` subcommand (xtask/src/main.rs:84 dispatch; usage string
updated at ~:122; doc bullet added). It calls `fdb_faults::run_metadata_nemesis`
(xtask/src/fdb_faults.rs:~360), which stands up `deploy/fdb-multi-replica`, configures it, resolves
the topology, and drives all three legs via `xtask::nemesis::metadata_nemesis_legs()`, tearing the
stack down panic-safely (`crate::finalize_panic_safe`, xtask/src/main.rs:941 — the same guard the
#442 battery uses). This makes the brief's sign-off open question — *one witnessed `WYRD_TIER1=1`
run of the three legs* — literally satisfiable: `WYRD_TIER1=1 cargo xtask metadata-nemesis`. Every
doc string now names this subcommand, not the old `fdb-metadata-tier1` claim.

### 4. `drive_leg` must not leak fault state on non-happy paths

`drive_leg` (nemesis.rs:~290) now heals on **every** exit path (mirroring `MasterIsolation`'s
`Drop` guard, `tier1_metadata_consistency.rs:336-349`):
- `apply()` failure → `heal()` before returning (a partial `iptables` cut never leaks).
- non-materialized bail → `heal()` before the inconclusive error.
- a **panicking** workload → `catch_unwind` runs the heal, then `resume_unwind` re-raises the
  original panic unchanged (the shipped #408 workload panics by design on a violation).
- The skew fault is applied in **`apply()`**, never `plan()` (`ClockSkewLeg::plan` is now
  readiness-only, `apply` does the recreate-with-override + restabilize wait). A `plan` failure
  therefore leaves the node un-skewed; a failed `apply` recreate is healed (recreate WITHOUT the
  override), so a half-applied skew never leaks.

### 5. Guard the central rule with tests; fix the false `--exact` safety claim; fdbcli timeout

- **Mock-leg `drive_leg` tests** (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs`, the
  `MockLeg`/`MockEvidence` + five `drive_leg_*` tests). These GUARD the #442 rule at Check with no
  docker: deleting the inconclusive bail reddens `drive_leg_refuses_the_workload_when_the_fault_did_not_materialize`;
  deleting the `heal_is_complete` check reddens `drive_leg_fails_an_incomplete_heal`; deleting the
  heal-on-apply-failure reddens `drive_leg_heals_when_apply_fails_and_never_runs_the_workload`;
  deleting the `catch_unwind` reddens `drive_leg_heals_before_re_raising_a_panicking_workload`.
- **`--exact` name-drift hole closed.** A `cargo test --exact <fn>` that matches nothing runs 0
  tests and exits 0 — a silent green no-op. New pure `parse_tests_run` / `nemesis_leg_ran_exactly_one`
  (xtask/src/nemesis.rs:~130) read the executed count off the output; `run_nemesis_leg`
  (xtask/src/fdb_faults.rs:~430) captures output and FAILS a leg unless exactly one test ran. Both
  helpers are unit-tested (`nemesis_orchestration.rs::the_name_drift_guard_rejects_a_leg_that_ran_zero_tests`).
  The v1 test comment claiming behavioural safety over a renamed constant is removed.
- **fdbcli `--timeout 10`** added to `survivor_status_json` (nemesis.rs:~470), matching
  `support::status_json` (`crates/metadata-fdb/tests/support/mod.rs:49-60`) — an unbounded `fdbcli`
  against a half-cut cluster can hang the leg.

## Seam shape (unchanged from the brief's Design §1) and why

- The lifecycle trait, evidence types, oracle arithmetic **and** the three live impls live in
  `wyrd-metadata-fault-conformance` (importable by the battery and #408); `xtask` gets ONLY the
  leg-kind enum + dispatch + argv + the pure test-count guard — **zero new dependencies**
  (`xtask/Cargo.toml:11-14` unchanged). `ClusterFault`
  (`crates/metadata-fault-conformance/src/lib.rs:86`) is left untouched — partition-shaped by
  contract, still the #442 seam.
- **Why not widen `ClusterFault`** (cost, concretely): pause/skew have no partition `topology()` /
  `peers_*` shape, so widening would force those methods onto legs that cannot answer them, and
  would touch all ~5 `ClusterFault` sites (the two live scenarios + `run_consistency_under_fault`
  + the two backend impls). The new trait carries backend-agnosticism as leg-impl *data* instead —
  0 lines changed in any existing `ClusterFault` caller.

## Refutation of my own test (forced)

- **(a) Genuine red?** YES. I dropped only the two `pub mod nemesis;` lines (reverting the
  production wiring, keeping BOTH added test files — the C4-verify shape) and re-ran: both went RED
  to compile —`error[E0432]: unresolved import 'xtask::nemesis'` and the same for
  `wyrd_metadata_fault_conformance::nemesis`. Both crates pre-exist, so the red is real, not
  green-only. Restored → GREEN (10 passed / 4 passed).
- **(b) Production path?** YES. The oracle tests call the exact `PartitionEvidence`/`PauseEvidence`/
  `SkewEvidence::materialized()` the live `confirm_materialized` impls build, and the `drive_leg`
  tests drive the **production** `drive_leg` — the same function the fdb scenario
  (`tier1_metadata_nemesis.rs`) calls — via a `MockLeg` that implements the production `NemesisLeg`
  trait. No copy/re-implementation of the rule under test. The xtask tests drive the production
  `metadata_nemesis_legs` / `scenario_fn` / `nemesis_scenario_args` / `parse_tests_run` the runner
  itself calls (`run_nemesis_leg`).
- **(c) Fixture includes the fault?** YES. The oracle tests include the did-not-bite fixtures the
  oracle must REJECT (no-op cut, crash-as-partition, below/zero-floor skew, served-through freeze,
  no-`paused`-state absence-of-service). The `drive_leg` mock tests include the failing element
  directly: an un-materialized leg (`MockLeg::new(false, ..)`), a failed apply, an incomplete heal,
  and a panicking workload — each asserting both the FAILURE and that `heal_count >= 1` (no leak).

## Verification run (project toolchain, in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 10 passed.
- `cargo test -p xtask --test nemesis_orchestration` → 4 passed.
- Red check (drop the two `pub mod nemesis;` lines, keep both test files) → both RED (unresolved
  import), then restored → both GREEN.
- `cargo fmt -p xtask -p wyrd-metadata-fault-conformance -p wyrd-metadata-fdb -- --check` → clean
  (commit-ready for the target's fmt hook).
- `cargo clippy -p wyrd-metadata-fault-conformance -p xtask --all-targets` and
  `cargo clippy -p wyrd-metadata-fdb --all-targets` → clean (workspace lints deny warnings).
- `cargo build -p xtask` (the binary consuming `run_metadata_nemesis`) → compiles.
- `cargo test -p wyrd-metadata-fdb --no-run` (DEFAULT, no `fdb` feature) → compiles: the
  `#[cfg(not(feature="fdb"))]` stubs keep the default `cargo xtask ci` gate green, same shape as
  `tier1_metadata_consistency.rs`.

## Deferred surface (off-Check, pre-declared by the brief) — human validates at sign-off

Per the brief's verification posture (net-new coverage + deferred live green, postures (a)+(b)),
the live three-leg runs and the `#[cfg(feature="fdb")]` scenario bodies are opt-in and off-Check.
`libfdb_c` / fdb headers are **absent** on this worktree (confirmed:
`python3 -c "import ctypes; ctypes.CDLL('libfdb_c.so')"` fails; `/usr/include/foundationdb/fdb_c.h`
absent) — both are already-registered doctor rows (`libfdb_c`, `fdb-headers`), and the ≥3-process
`deploy/fdb-multi-replica` + privileged in-netns `iptables` + `libfaketime` topology is a
pre-declared no-check environment shape. So I could type-check only the DEFAULT (no-fdb) surface
here; the fdb-feature bodies compile under `WYRD_FDB_TOOLCHAIN`, which the maintainer runs at
sign-off. This is the brief's Open Question, not a dependency Plan missed.

Manual validation the maintainer runs at sign-off (one witnessed run):

```
# 1. type-check the fdb-feature wiring on a host with libfdb_c + fdb headers:
WYRD_FDB_TOOLCHAIN=1 cargo check -p wyrd-metadata-fdb --features fdb --tests
# 2. provide the skew preload (bind-mounted by docker-compose.faketime.yml):
apt-get install -y libfaketime            # provides libfaketime.so.1
# 3. run all three legs against the live cluster (stands up deploy/fdb-multi-replica, cuts/
#    skews/pauses, confirms each materialized + healed, tears down):
WYRD_TIER1=1 cargo xtask metadata-nemesis
```

## Scope adherence

In scope and delivered: the three-leg seam + oracles + pure plan/dispatch logic + the runnable
`metadata-nemesis` subcommand + the two Check-time tests + the minimal deploy delta (libfaketime
override) + the feature-gated fdb wiring. Out of scope and untouched: the checked #406 workload
under the nemesis and the report (#408), the #409 CI job, the pre-existing TiKV tier1 rename
timeout, the production backend default, any refactor of `wyrd-metadata-fault-conformance` /
`wyrd-testkit` beyond the new module, dynamic mid-run skew (v1 is a static per-leg offset).
