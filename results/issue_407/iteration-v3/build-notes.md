# Build notes — issue 407 (m4-metadata-nemesis-partition-skew-pause), iteration 3

## Frame

Iterations 1 and 2 established a sound Check-core (the three leg-kind enum, the per-leg
materialization oracles, the `drive_leg` lifecycle rule, the orchestration argv, and the two named
red→green tests). Both prior sign-offs explicitly said **"preserve the Check-core; the live-leg half
is defective."** So iteration 3 starts from the v2 patch (it applies cleanly to `main`) and repairs
ONLY the live-leg defects the iteration-2 carry-forward enumerated, plus its one named minor. I did
not re-open the accepted Check-core except where a carry-forward item required it (the `drive_leg`
panic/heal ordering, item 3, and its guard test).

All citations are `path:line` in `$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt`) after
this patch is applied.

## The iteration-2 carry-forward, point by point

### 1 & 2 — the live-skew defect class: container identity resolved before a recreate

**Root cause (both items are the same bug).** The runner resolved container identity as an ephemeral
**container id** (`container_of` → `docker compose ps -q`, `xtask/src/fdb_faults.rs:81`) ONCE,
pre-campaign, and exported it as `WYRD_TIER1_SKEW_CONTAINER` and inside the netns map. But the
clock-skew leg's `apply()` runs `docker compose up -d --force-recreate <service>`
(`crates/metadata-fault-conformance/src/nemesis.rs`, `ClockSkewLeg::recreate` / `apply`), which mints
a **new** id while keeping the container. Every subsequent `docker exec <old-id>` (the skew probe)
and `docker pause <old-id>` (the later pause leg, reading the stale netns map) then failed — the skew
leg could never materialize on a default run, and the pause leg could receive a stale id.

**Fix — resolve by STABLE COMPOSE NAME, not id.** Docker Compose derives a deterministic container
name (`<project>-<service>-<index>`) that it **reuses across `--force-recreate`** (the id changes,
the name does not); `docker exec|pause|inspect <name>` and `docker run --network=container:<name>`
all accept a name. New helpers, isolated to the #407 path so the #442 battery's proven id-based
`container_of`/`netns_map` are untouched:

- `container_name_of(service)` — `docker compose ps --format '{{.Name}}' <service>`
  (`xtask/src/fdb_faults.rs:364`). We still ASK compose for the name rather than hard-code the
  `<project>-<service>-N` convention, so a compose naming change cannot silently point at nothing
  (the same discipline the original `container_of` doc argued for, `fdb_faults.rs:77-80`).
- `nemesis_netns_map()` — the ip→**name** map (`xtask/src/fdb_faults.rs:401`), the name-based sibling
  of `netns_map()` (`fdb_faults.rs:69`).
- `run_metadata_nemesis` now builds the topology with these (`xtask/src/fdb_faults.rs:445-452`), so
  the netns map AND the skew target survive the skew leg's recreate — valid for every later leg.

This is exactly the carry-forward's prescribed remedy ("Probe by stable compose container NAME … for
ALL legs"), applied at the single resolution point rather than per-leg.

The falsified "structurally impossible" claim in the fdb wiring (iteration-2 carry-forward item 4,
`tier1_metadata_nemesis.rs:122-127`) is rewritten to state the real mechanism: the runner exports the
**stable name**, which is what makes the post-recreate probe resolve
(`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:122-137`). The overclaim is gone.

### 3 — `drive_leg` dropped a heal failure when the workload panicked

**Root cause.** In v2, `std::panic::resume_unwind(panic)` ran BEFORE `heal_result?` and the
`heal_is_complete` check, so a heal that FAILED while the workload also panicked unwound the stack
before the leak was ever inspected — a leaked cut/pause/skew hidden behind the panic, contradicting
the module's own no-leaked-fault claim.

**Fix** (`crates/metadata-fault-conformance/src/nemesis.rs`, `drive_leg` tail + new
`heal_incomplete_reason` helper): heal, then compute the leak verdict ONCE
(`heal_incomplete_reason`), THEN branch on the workload outcome. On a clean heal the original panic is
re-raised unchanged (`resume_unwind`); on a **leaked** fault under a panicking workload we `eprintln!`
the leak and `panic!` naming the leaked fault — a leaked fault is the graver invariant violation and
must never hide behind the workload's own panic. The happy and panicking exit paths now apply the
*same* leak verdict. Doc updated (`# Panics` section).

**Guard test strengthened** (carry-forward asked for this): the panic test previously asserted only
`heal_count >= 1`. Added `drive_leg_surfaces_a_leaked_fault_even_when_the_workload_panics`
(`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:418`) which drives a panicking workload
through a leg whose heal is incomplete and asserts the propagated panic **names the leaked fault**
(not the workload's own message). The existing happy-panic test also now asserts the ORIGINAL panic
propagates on a clean heal (`nemesis_oracles.rs:404-413`).

### 4 — the fdb-feature wiring must make the sign-off run satisfiable

After fixes 1–3 the `WYRD_TIER1=1 cargo xtask metadata-nemesis` run can materialize + heal all three
legs (the skew leg no longer dead-ends on a stale id). The fdb-feature scenario earns no C4 red and
is compiled by no default gate this cycle **by design** — that is the brief's pre-declared verification
posture (type-check boundary pinned at `brief.md:80-88`: only the default-compiled surface is claimed
"compiled by ci"; the `#[cfg(feature="fdb")]` bodies type-check under the privileged
`WYRD_FDB_TOOLCHAIN` opt-in). It is left correct and honest, not claimed green here — see the deferred
surface / NEEDS-HUMAN section below.

### Minor — `parse_tests_run`'s loose `starts_with("test")`

`tail.starts_with("test")` subsumed the other arms and would read `running 5 testbeds …` as a test
count of 5. Tightened to `matches!(tail, "test" | "tests")` — the exact tokens cargo prints
(`xtask/src/nemesis.rs:127-139`). The orchestration test now pins this with a `running N testbeds`
lookalike line that the loose form wrongly accepted (`xtask/tests/nemesis_orchestration.rs:128-140`).

## Why name-based resolution, not per-leg re-resolution (the other carry-forward option)

The carry-forward offered two remedies: "name-based OR post-recreate re-resolution for ALL legs."
Post-recreate re-resolution would require each leg (and the cross-leg netns map, which lives in the
test-process env passed per subprocess) to re-run `docker compose ps` after every recreate — the skew
leg recreates twice (apply + heal), and the map is resolved in the runner process, not the leg
process, so re-resolution would need a new env round-trip per leg (≈ 3 extra `compose ps` shell-outs
threaded through `run_nemesis_leg`'s env plumbing per recreate). Name-based resolution resolves the
identity ONCE and it simply stays valid — 2 small helpers (`container_name_of`,
`nemesis_netns_map`, ~40 lines) versus re-plumbing env across every leg subprocess. Same correctness,
strictly less machinery.

## Blast radius / scope adherence

Touched: `xtask/src/{nemesis.rs,fdb_faults.rs,lib.rs,main.rs}`, `crates/metadata-fault-conformance/`
(the seam + oracles + impls + the oracle test), the fdb-feature wiring under
`crates/metadata-fdb/tests/`, and `deploy/fdb-multi-replica/docker-compose.faketime.yml`. The #442
battery's `container_of`/`netns_map`/`run_fdb_metadata_tier1` are **untouched** — the nemesis got its
own name-based resolvers. `ClusterFault` (`crates/metadata-fault-conformance/src/lib.rs:86`) is
untouched (Design §1: it stays the #442 partition seam, not generalized). Zero new `xtask`
dependencies.

## Refutation of my own test (forced)

- **(a) Genuine red?** YES — verified by actually reverting each substantive change and re-running:
  - Reverting `drive_leg` to the old `resume_unwind`-before-heal ordering →
    `drive_leg_surfaces_a_leaked_fault_even_when_the_workload_panics` FAILS (1 failed / 5 passed),
    restored → 11 passed.
  - Reverting `parse_tests_run` to the loose `starts_with("test")` →
    `the_name_drift_guard_rejects_a_leg_that_ran_zero_tests` FAILS, restored → 4 passed.
  - The C4-verify shape (revert the production hunks, keep both added test files) reddens both files
    at compile time — the imports `wyrd_metadata_fault_conformance::nemesis` / `xtask::nemesis`
    do not resolve — same real red the two prior iterations confirmed (both crates pre-exist).
- **(b) Production path?** YES. The oracle tests call the exact `materialized()` the live
  `confirm_materialized` impls build; the `drive_leg` tests drive the PRODUCTION `drive_leg`
  (the same function the fdb scenario calls) via a `MockLeg` implementing the production `NemesisLeg`
  trait — no re-implementation of the rule under test. The xtask tests drive the production
  `metadata_nemesis_legs` / `nemesis_scenario_args` / `parse_tests_run` the runner itself calls.
- **(c) Fixture includes the fault?** YES. The oracle tests include the did-not-bite fixtures the
  oracle must REJECT (no-op cut, crash-as-partition, served-through freeze, sub-floor/zero-floor
  skew, absent `paused` state). The `drive_leg` mock tests include the failing element directly: an
  un-materialized leg, a failed apply, an incomplete heal, and — new this iteration — a panicking
  workload whose heal ALSO fails (the exact leaked-fault-under-panic case).

## Verification run (project cargo toolchain, in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 11 passed.
- `cargo test -p xtask --test nemesis_orchestration` → 4 passed.
- Targeted red checks (above) → red on revert, green on restore.
- `cargo fmt -p xtask -p wyrd-metadata-fault-conformance -p wyrd-metadata-fdb -- --check` → clean
  (commit-ready for the target's fmt hook).
- `cargo clippy -p wyrd-metadata-fault-conformance -p xtask --all-targets` and
  `cargo clippy -p wyrd-metadata-fdb --all-targets` → clean (workspace denies warnings).
- `cargo test -p wyrd-metadata-fdb --no-run` (DEFAULT, no `fdb`) → compiles: the
  `#[cfg(not(feature="fdb"))]` stubs keep the default `cargo xtask ci` gate green.

The full `cargo xtask ci` gate (fmt/clippy/build/test/deny/conformance over the whole workspace) is
Check's job; the above is the fast Do-side red→green sanity through the project's cargo toolchain.

## Deferred surface (off-Check, pre-declared) + NEEDS-HUMAN

Per the brief's verification posture (net-new coverage + deferred live green, postures (a)+(b)), the
live three-leg runs and the `#[cfg(feature="fdb")]` scenario bodies are opt-in (`WYRD_TIER1=1`) and
off-Check. `libfdb_c` / fdb headers / a ≥3-process `deploy/fdb-multi-replica` cluster with a
privileged in-netns `iptables` sidecar and `libfaketime` are pre-declared doctor rows / a no-check
environment shape (`brief.md:60-66`) — not present on this worktree — so I type-checked only the
DEFAULT (no-fdb) surface here. The live materialization + heal of the three legs is irreducibly
Docker/cluster-bound and cannot be exercised headless on this host; I did NOT fabricate a stand-in.

```
NEEDS-HUMAN external dependency: live deploy/fdb-multi-replica cluster (docker + libfdb_c + fdb headers + in-netns iptables + libfaketime) — blocks the witnessed WYRD_TIER1=1 three-leg run (materialize + heal for partition/skew/pause); the Check-core is exercised, the live legs are not
```

Manual validation the maintainer runs at sign-off (the brief's Open Question — one witnessed run):

```
# 1. type-check the fdb-feature wiring on a host with libfdb_c + fdb headers:
WYRD_FDB_TOOLCHAIN=1 cargo check -p wyrd-metadata-fdb --features fdb --tests
# 2. provide the skew preload (bind-mounted by docker-compose.faketime.yml):
apt-get install -y libfaketime
# 3. run all three legs against the live cluster (stands the stack up, cuts/skews/pauses,
#    confirms each materialized + healed, tears down):
WYRD_TIER1=1 cargo xtask metadata-nemesis
```

The doctor rows are already registered (`pdca.toml` `libfdb_c loadable`, `fdb headers (bindgen)`,
`docker`); the cluster topology + privilege shape is the pre-declared no-check environment shape, so
no new `[[doctor.checks]]` row is proposed.
