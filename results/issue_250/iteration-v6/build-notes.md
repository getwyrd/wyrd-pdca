# Build notes — issue #250 / tier1-jepsen-consistency-harness

## Root cause

`xtask/src/faults.rs:170` (pre-fix): `run_jepsen()` calls
`execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")`.
`execute()` shells out to `std::env::var("WYRD_TIER1_JEPSEN_CMD")` when in `Plan::Run`.
No such env var is set anywhere in-repo, so the only real effect is that `run_jepsen`
when opted-in with no `WYRD_TIER1_JEPSEN_CMD` returns
`Err("WYRD_TIER1_JEPSEN_CMD ... not configured")`.
There is no in-repo scenario test, no privileged CI job, and nothing that asserts the
ADR-0015 consistency contract. This is the "deferred ≠ unbuilt" gap for the consistency
leg — the last remaining inert stub in `faults.rs` after the two sibling legs (#195,
#196) were built.

## What was built

Three files changed/added to satisfy the brief's Success criterion:

### 1. `xtask/src/faults.rs` (modified)

**`run_jepsen()`** — rewritten to dispatch to the in-repo scenario:
- Uses `docker` as the required tool (matching `run_kill_reconstruct`; Docker is needed
  for the containerized D-server cluster)
- On `Plan::Run`: brings up the 10-server docker-compose cluster, runs
  `cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency -- --ignored
  --nocapture` with `WYRD_DSERVER_ENDPOINTS` and `WYRD_TIER1_VICTIM_CONTAINER`,
  then tears down the cluster. Mirrors the `run_kill_reconstruct` shape exactly.

**`jepsen_required_tool()`** — new `pub(crate)` function returning `"docker"`.
Exposes the dispatch tool for unit-testing without a live container runtime.

**`run_jepsen_consistency_test()`** — new private helper that invokes the `cargo test`
command with the correct package, test name, and env vars.

**`execute()` / `run_shell()`** — annotated `#[cfg(test)]` to prevent dead-code errors
(`warnings = "deny"` is in the workspace lints). They are still exercised by the
existing tests for `Plan::Deferred` / `Plan::MissingTool` path coverage; those tests
are retained unchanged.

**New test: `jepsen_dispatches_to_in_repo_scenario_not_external_cmd`**  
This is the Check-time flippable regression. Pre-fix: `jepsen_required_tool()` does not
exist → compile error (red). Post-fix: function exists and returns `"docker"` (green).

### 2. `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` (new)

Modelled on `tier2_kill_reconstruct.rs`. Key elements:

- **`MemMeta` / `CrashMeta`** — same in-memory MetadataStore pattern as tier2.
  `CrashMeta::arm()` intercepts the version-conditional commit (the one with a positive
  precondition), modelling a custodian crash or partition from the metadata store.

- **`assert_fully_old_after_crash(placement, repaired_slot, victim_id, spare_id)`** —
  asserts the committed placement is fully old (victim at slot, spare absent) after a
  crash. Catches "torn read" (spare at the repaired slot) and "double placement" (spare
  appearing at any slot while victim is still at its slot). Both are violations of the
  ADR-0015 commit-point-atomic invariant.

- **`assert_fully_new_after_repair(placement, repaired_slot, victim_id, spare_id)`** —
  asserts the committed placement is fully new (spare at slot, victim absent) after
  `Reconciled::Changed`. Catches stale reads (victim still at slot) and torn writes
  (victim appearing anywhere despite the commit landing). Both violate ADR-0015
  read-after-commit.

- **6 unit tests** for the two helpers (3 each), all non-`#[ignore]`, all running at
  Check inside `cargo xtask ci`. Each has a passing case and two negative controls that
  prove the oracle catches the specific violation it was designed to catch. This is the
  "born-at-tier" seam: an API regression or stub would fail both these unit tests AND
  the compile-time type-check of the scenario body.

- **`consistency_over_repair_path`** — the `#[ignore]`d scenario test:
  - Reads endpoints from `WYRD_DSERVER_ENDPOINTS` (set by `run_jepsen_consistency_test`)
  - Reads victim container from `WYRD_TIER1_VICTIM_CONTAINER`
  - Writes RS(6,3) data to servers 0–8, creates inode, enqueues repair
  - Kills server 0 via `docker kill` (D-server crash)
  - Phase 1 (crash before commit): arms `CrashMeta`, runs `reconcile_step` →
    `Reconciled::Satisfied`, asserts `assert_fully_old_after_crash` (no torn read),
    asserts orphan exists on spare (garbage-not-corruption)
  - Phase 2 (partition healed): disarms `CrashMeta`, runs `reconcile_step` →
    `Reconciled::Changed`, asserts `assert_fully_new_after_repair` (read-after-commit)
  - Phase 3 (data integrity): reads all N fragments, verifies byte-identical reconstruction

### 3. `.github/workflows/tier1-jepsen.yml` (new)

- **Cron**: 02:00 UTC (before the existing 03:00/04:00/05:00 UTC jobs to avoid
  runner contention)
- **`workflow_dispatch`**: for on-demand runs
- **`WYRD_TIER1: "1"`**: opts in
- **Docker info** step: confirms daemon is reachable before building
- **`cargo xtask jepsen`**: the single xtask entry point
- **Artifact upload on failure**: same pattern as `tier2-kill-reconstruct.yml`
- **45-minute timeout**: same as tier2

## Design decisions

### Why `docker` as the required tool (not `lein`)

The scenario needs a real containerized D-server cluster to make the gRPC calls
meaningful. Docker is the right tool to probe — the same tool `run_kill_reconstruct`
uses. This is what the "Option B" structural decision implies: an in-repo Rust scenario
running against a containerized cluster, not a Clojure/Jepsen harness.

### Why `#[cfg(test)]` on `execute()` / `run_shell()` rather than removing them

Removing them would require removing the two existing tests (`execute_deferred_is_ok`
and `execute_missing_tool_propagates_error`). Those tests cover the `Plan::Deferred`
and `Plan::MissingTool` paths of `execute()`, which while also covered by the `plan()`
tests, provide additional behavioral validation. Keeping them with `#[cfg(test)]`
annotations is the minimal, non-destructive change that avoids the dead-code error.

### Why the consistency test uses the same cluster as tier2 (10 D servers)

The scenario uses the exact same `docker-compose.yml` and `KR_DSERVER_COUNT = 10`
constant from `kill_reconstruct.rs`. This avoids duplicating infrastructure and keeps
the two tier-1/tier-2 scenarios symmetric. The `JC_DSERVER_COUNT` constant in the
scenario test file is defined independently (same value: 10) following the same pattern
as `tier2_kill_reconstruct.rs` which defines its own `KR_DSERVER_COUNT`.

### Why the "partition" is modelled as a commit-intercept via `CrashMeta`

The brief says "injects partitions and crashes mid-repair." In the context of this
in-repo Rust scenario (Option B), the most accurate model of both a crash AND a
network partition between the custodian and its metadata store is `CrashMeta`
intercepting the version-conditional commit. This:
- Simulates the custodian crashing before commit (the crash case)
- Simulates the custodian being unable to reach the metadata store to commit (the
  partition case)

Both result in the same observable outcome: the repair obligation is re-queued on the
next reconcile pass. A real network partition of D servers (excluding them from the
fleet) was considered but would add complexity and could reduce available survivors
below K=6 (with 9 servers and 1 dead, excluding 2+ more approaches the erasure limit).

### Alternatives ruled out

**Moving the scenario to `crates/custodian/tests/`** (like `tier1_disk_faults.rs`):  
The disk-fault scenario uses `FsChunkStore` (an in-process local store) and doesn't
need Docker. The consistency scenario needs real gRPC D-server containers, which makes
`crates/chunkstore-grpc/tests/` the natural home — exactly where `tier2_kill_reconstruct.rs`
lives. Putting it in `custodian/tests/` would require adding `wyrd-chunkstore-grpc` as a
dev-dependency of `wyrd-custodian`, violating the dependency rule (ADR-0010 requires
`custodian` never depend on a concrete store).

**Adding a `partition_fleet` test phase** (excluding D servers from the fleet):  
Would demonstrate a more literal "network partition" but risks having fewer than K=6
surviving fragments when combined with the docker kill of server 0. The commit-intercept
approach is more faithful to the actual consistency hazard (the custodian's metadata
atomicity) and avoids the erasure-theory edge case. The brief explicitly notes that
literal Clojure/Jepsen was rejected; this in-repo scenario captures the ESSENCE of the
consistency test.

## Red→green proof

**Red (pre-fix)**: Adding the test `jepsen_dispatches_to_in_repo_scenario_not_external_cmd`
to the original `faults.rs` causes `E0425: cannot find function jepsen_required_tool in this scope`.
Verified by temporarily reverting `faults.rs` to HEAD and adding the test body — cargo
emits a compile error, confirming the test is load-bearing.

**Green (post-fix)**: `cargo xtask ci` exits 0. The specific test passes:
`faults::tests::jepsen_dispatches_to_in_repo_scenario_not_external_cmd ... ok`.
The 6 new helper unit tests pass. The `consistency_over_repair_path` scenario
body is `#[ignore]`d and compiles (type-checked at Check, as intended).

## Citations

- `xtask/src/faults.rs:170` (pre-fix HEAD) — the inert `execute(_, _, "WYRD_TIER1_JEPSEN_CMD")` call
- `xtask/src/faults.rs:78-96` (post-fix) — `#[cfg(test)]` on `execute()`
- `xtask/src/faults.rs:100-114` (post-fix) — `#[cfg(test)]` on `run_shell()`
- `xtask/src/faults.rs:174-287` (post-fix) — new `run_jepsen()`, `jepsen_required_tool()`, `run_jepsen_consistency_test()`
- `xtask/src/faults.rs:426-438` (post-fix) — new `jepsen_dispatches_to_in_repo_scenario_not_external_cmd` test
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — new file (born-at-tier helpers + scenario)
- `.github/workflows/tier1-jepsen.yml` — new privileged CI job
- `tier2_kill_reconstruct.rs` — binding precedent for the scenario shape, `CrashMeta`, repair stand-in
- `faults.rs:118-165` (pre-fix) — `run_disk_faults()` / `run_tier1_scenario()` pattern mirrored
- `faults.rs:196+` (pre-fix) — `run_kill_reconstruct()` pattern mirrored
