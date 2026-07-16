# Build notes — issue 407 (m4-metadata-nemesis-partition-skew-pause)

## What I built, and why it satisfies the Success criterion

The criterion has three clauses; the change delivers each and proves the Check-testable core
red→green:

1. **Three leg kinds behind one importable seam in `wyrd-metadata-fault-conformance`.**
   New module `crates/metadata-fault-conformance/src/nemesis.rs` (`pub mod nemesis;` at
   `crates/metadata-fault-conformance/src/lib.rs:65`) exposes:
   - `NemesisLegKind::{Partition, ClockSkew, ProcessPause}` + `NemesisLegKind::ALL` (the three
     fault classes, enumerable by both the battery and #408);
   - one lifecycle trait `NemesisLeg` (`plan → apply → confirm_materialized → heal →
     confirm_healed`) with an associated typed `Evidence`, plus `drive_leg` — the runner both the
     battery and #408 consume WITHOUT reopening the lifecycle (the "stable public API" the
     wave-0 ordering note demands);
   - three live-leg impls `PartitionLeg` / `ProcessPauseLeg` / `ClockSkewLeg` — ordinary-`std`
     `docker`/`fdbcli` shell-outs, **no `libfdb_c` linkage**, so they compile unconditionally in
     this lib and are importable by the battery's tests and by #408's `crates/server/tests/`.
   The partition leg **re-implements** the `MasterIsolation` technique (in-netns symmetric
   `iptables` DROP + survivor-side `status json` reachability oracle) because that impl is
   test-binary-private and cannot be imported (peer callsite
   `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:232`, `fn iptables` at `:216`,
   `fn rules` at `:197`). The existing `ClusterFault`
   (`crates/metadata-fault-conformance/src/lib.rs:85`) is left **untouched** — partition-shaped by
   contract, still the #442 battery's seam (Design §1).

2. **Each leg carries a materialization oracle that refuses a fault that did not bite.** Each
   `*Evidence::materialized()` is **pure decision logic over recorded observations** — the same
   code the live impls build from `docker inspect` / `status json` output (e.g.
   `PartitionLeg::confirm_materialized` builds `PartitionEvidence` then the caller checks
   `.materialized()`), so a regression flips both the unit test and the live leg. `drive_leg`
   fails the run as **inconclusive** when `.materialized()` is false (the #442 "a note is not a
   gate" rule), and fails an incomplete heal via `wyrd_testkit::heal_is_complete`. Oracles reuse
   the merged arithmetic where it fits: partition delegates to
   `wyrd_testkit::partition_took_effect` and reachability to
   `wyrd_testkit::fdb_peer_sees_target_live`.
   - partition: survivor reachability flips true→false WHILE the container stays `running` (a
     partition, not a crash);
   - pause: three observed serving transitions (serve → freeze/`paused` → serve), never a single
     probe;
   - skew: `|container_clock − harness_clock| ≥ non-zero floor` (a zero floor never materializes).

3. **Host-independent logic exercised red→green at Check by the two named tests.**
   - `crates/metadata-fault-conformance/tests/nemesis_oracles.rs` — enumeration + per-leg oracle
     arithmetic + the parse helpers the live impls read (`inspected_status_is_{paused,running}`,
     `clock_offset_secs`).
   - `xtask/tests/nemesis_orchestration.rs` — leg enumeration (`metadata_nemesis_legs`), dispatch
     (`NemesisLegKind::scenario_fn`, injective over the campaign), and runner-arg building
     (`nemesis_scenario_args`, carries `--features fdb`, targets `wyrd-metadata-fdb`, runs the
     `--ignored --exact` leg). New module `xtask/src/nemesis.rs` (`pub mod nemesis;` at
     `xtask/src/lib.rs:20`), mirroring `xtask/src/metadata_faults.rs:53` /
     `xtask/src/faults.rs:245` and the orchestration-test pattern at
     `xtask/tests/metadata_faults_orchestration.rs:1-25` and `xtask/src/faults.rs:1013`.

`xtask` gains **zero new dependencies** (its own leg-kind enum; it does NOT depend on the
conformance crate) — `xtask/Cargo.toml:11-14` unchanged (Design §1).

## Why this shape, and alternatives ruled out

- **Not widening `ClusterFault`.** The brief pins it: `ClusterFault` is partition-shaped by
  contract and stays the #442 seam. A new lifecycle trait carries backend-agnosticism instead
  (its backend-shaped parts are leg-impl data). Widening `ClusterFault` to cover pause/skew would
  have forced `topology()`/`peers_*` methods onto legs that have no partition topology — a
  contortion, and it would have touched every existing `ClusterFault` caller (the two live
  scenarios + `run_consistency_under_fault`, ~5 impl sites) for no gain.
- **Oracle as typed evidence + pure `materialized()`, not a bare bool from the live impl.** This
  is the only way the decision logic is Check-testable without Docker while remaining the exact
  code the live leg runs (the `wyrd_testkit` idiom: `partition_took_effect` etc.). A live impl
  returning a pre-computed bool would leave the decision untested at Check — the vacuum the brief
  forbids.
- **Skew via container-scoped `libfaketime`, not `date -s`/CAP_SYS_TIME** (Design §3,
  Alternatives): per-container, reversible, harness clock untouched. Deploy delta is the additive
  override `deploy/fdb-multi-replica/docker-compose.faketime.yml` (a separate file — the base
  `up` is unaffected, confirmed by `xtask` `fdb_multi_replica_declares_three_processes…` still
  green).

## Refutation of my own test (forced)

- **(a) Genuine red?** YES. I moved both new production modules aside and `git checkout`ed the two
  `lib.rs` files (reverting only production, keeping both test files — the C4-verify shape) and
  re-ran: both went RED with `error[E0432]: could not find 'nemesis' in
  'wyrd_metadata_fault_conformance'` / `in 'xtask'`. Both crates pre-exist, so the red legs are
  real (not green-only). Restored, both GREEN (5 passed / 3 passed).
- **(b) Production path?** YES. The tests drive the production functions the fix adds — the
  `*Evidence::materialized()` oracles and parse helpers the live `PartitionLeg`/`ProcessPauseLeg`/
  `ClockSkewLeg` impls call, and the `metadata_nemesis_legs`/`scenario_fn`/`nemesis_scenario_args`
  the runner + the fdb-feature scenario wiring reference. No copy/mock/re-implementation: the
  oracle methods under test ARE the ones `confirm_materialized`/`drive_leg` invoke, and the fdb
  scenario `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` selects legs by these very
  `scenario_fn` names.
- **(c) Fixture includes the fault?** YES. The oracle tests include the *did-not-bite* fixtures
  the oracle must REJECT — a no-op cut (`peers_saw_target_during: true`), a crash masquerading as
  a partition (`target_running_during: false`), a target that kept serving through a "freeze", a
  below-floor and a zero-floor skew — alongside the genuinely-materialized case. The enumeration
  test includes all three legs and asserts distinct slugs/functions (a dropped or collapsed leg
  is red, not merely absent).

## Verification run (project toolchain, in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 5 passed.
- `cargo test -p xtask --test nemesis_orchestration` → 3 passed.
- `cargo fmt --all -- --check` → clean (commit-ready for the target's fmt hook).
- `cargo clippy -p wyrd-metadata-fault-conformance -p xtask -p wyrd-metadata-fdb --all-targets`
  → clean (no `-D warnings` triggers — `cargo xtask ci` runs clippy denying warnings).
- `cargo test -p wyrd-metadata-fdb --no-run` (default, **no** `fdb` feature) → compiles: the
  fdb-feature scenario bodies are cfg'd out with `#[cfg(not(feature="fdb"))]` stubs, exactly like
  the sibling `tier1_metadata_consistency.rs:384`, so the default `cargo xtask ci` gate stays
  green.
- `cargo test -p xtask` (deploy-guard + fdb-profile suites) → all green; the new compose override
  is a separate file and does not perturb the base-compose parse tests.

## Deferred surface (off-Check) and what the human validates at sign-off

Per the brief's verification posture (net-new coverage + deferred live green), the live three-leg
runs against `deploy/fdb-multi-replica` are opt-in (`WYRD_TIER1=1`) and off-Check. The
fdb-feature scenario wiring `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` is type-checked
ONLY under the privileged `WYRD_FDB_TOOLCHAIN` opt-in (it links `libfdb_c`), which this worktree
does not have — I verified the DEFAULT (no-fdb) build compiles, but could NOT type-check the
`#[cfg(feature="fdb")]` bodies or run the live legs here. This matches the brief's Open Question
(a witnessed local `WYRD_TIER1=1` run, since #409's CI job does not exist yet).

NEEDS-HUMAN external dependency: fdb-toolchain (libfdb_c + fdb headers) — blocks type-checking the
`#[cfg(feature="fdb")]` bodies of `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` and the
witnessed live three-leg run; both are already-registered doctor rows (`libfdb_c`, `fdb-headers`)
and a live-topology shape (≥3-process `deploy/fdb-multi-replica` + privileged in-netns `iptables`
sidecar + `libfaketime` in the node containers), which the brief pre-declared as no-check
environment shapes.

Manual validation steps for the maintainer at sign-off (one witnessed run):

```
# 1. type-check the fdb-feature wiring on a host with libfdb_c + fdb headers:
WYRD_FDB_TOOLCHAIN=1 cargo check -p wyrd-metadata-fdb --features fdb --tests
# 2. install the skew preload in the node image path and bind it:
apt-get install -y libfaketime            # provides libfaketime.so.1
# 3. run the three legs against the live cluster (stands up deploy/fdb-multi-replica, cuts/
#    skews/pauses, confirms each materialized + healed):
WYRD_TIER1=1 cargo xtask fdb-metadata-tier1     # (or the per-leg cargo test --exact <scenario_fn>)
```

The skew leg additionally needs the runner to export `WYRD_TIER1_COMPOSE_FILE`,
`WYRD_TIER1_FAKETIME_OVERRIDE` (→ `docker-compose.faketime.yml`), and optionally
`WYRD_TIER1_SKEW_SO` (host path to `libfaketime.so.1`) — documented in the override file's header.
Wiring `cargo xtask fdb-metadata-tier1` (or a new `metadata-nemesis` subcommand) to iterate these
legs against the standing cluster is #408's composition step (it owns `main.rs`/the checked
workload); this slice deliberately lands only the thin xtask enum/dispatch/args module + the
importable conformance seam so 408 builds on the folded result rather than blind beside it.

## Scope adherence

In scope and delivered: the three-leg seam + oracles + pure plan/dispatch logic + the two
Check-time tests + the minimal deploy delta (libfaketime override) + the feature-gated fdb wiring.
Out of scope and NOT touched: the checked #406 workload under the nemesis and the report (#408),
the #409 CI job, the pre-existing TiKV tier1 rename-timeout, the production backend default, any
refactor of `wyrd-metadata-fault-conformance`/`wyrd-testkit` beyond the new module, dynamic
mid-run skew (v1 is a static per-leg offset).
