# Build notes — issue 399 / tier1-jepsen-live-network-partition

(Withheld from the reviewer per the Do contract — this is for the human at sign-off.)

## What I built

Target branch: `getwyrd/wyrd@main`. All line citations below are against that
branch (verified via `git diff main -- <path>` in the `$PDCA_WORKTREE` checkout).

### 1. The pure, test-observable nemesis decision (`xtask/src/faults.rs`)

- `IsolationNemesis` enum (`faults.rs:200-210` post-patch) — two variants,
  `ProcessFreeze` (the existing `docker pause`/`unpause` leg, kept per Scope) and
  `NetworkPartition` (the new leg, #399's upgrade). Mirrors `JepsenDispatch`
  (`faults.rs:160-168` pre-patch) exactly: a value with BOTH alternatives
  representable, not a hardcoded match arm.
- `IsolationNemesis::scenario_fn()` — maps each variant to its OWN `#[ignore]`d
  scenario function name in `tier1_jepsen_consistency.rs`, so a regression that
  points both nemeses at the same function is representable and catchable
  (distinct-function-names assertion in the unit test below).
- `tier1_jepsen_isolation_legs() -> Vec<IsolationNemesis>` — the actual routing
  value `run_jepsen()` iterates on its `Plan::Run` path (`faults.rs`'s
  `run_jepsen`, patched). Currently `vec![ProcessFreeze, NetworkPartition]`.
- Unit test `tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only`
  (in `xtask/src/faults.rs`'s existing `#[cfg(test)] mod tests`, alongside
  `jepsen_dispatch_routes_to_in_repo_scenario_not_external_command`, per the
  brief's Test file field) — asserts the leg list contains `NetworkPartition`
  (not just `ProcessFreeze`), that the freeze leg is still present (not deleted),
  and that each nemesis routes to a distinct scenario function.

### 2. The node-liveness oracle (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`)

- `assert_node_live_during_isolation(state: &str) -> AssertResult` — a sixth
  oracle alongside the five existing `assert_*` helpers (`:207-543` pre-patch),
  same shape: pure, `pub(crate)`, called by the scenario body, unit-tested with
  a positive case and TWO negative controls (`"paused"`, `"exited"`).
- The `"paused"` negative control is literally the falsifiability demonstration
  the brief asks for: it is the exact input the network-partition leg would feed
  the oracle if it collapsed back to `docker pause`.

### 3. The new `#[ignore]`d scenario (`tier1_jepsen_consistency.rs`)

- `jepsen_consistency_over_repair_under_live_partition_and_crash` — a full copy
  of the existing six-phase campaign (`jepsen_consistency_over_repair_under_partition_and_crash`),
  with Phase 2/3 changed from `docker pause`/`unpause` to
  `docker network disconnect`/`connect` (brief's Scope names this mechanism
  first), plus a node-liveness assertion (`docker_container_state` +
  `assert_node_live_during_isolation`) inserted right after the disconnect and
  before the repair attempt in Phase 2.
- `docker_container_state(container) -> String` — the real
  `docker inspect -f '{{.State.Status}}'` shell-out the scenario feeds the
  oracle from (not a pure function — it is a thin, deliberately untested I/O
  wrapper, the same shape as the existing `docker kill`/`pause`/`unpause` calls
  in the sibling scenario).
- `#[tokio::test(...)] #[ignore]` — identical gating to the existing scenario:
  `cargo test --workspace` compiles and type-checks the body (verified: see
  "Compile/type-check" below) without running it.

### 4. Wiring into `run_jepsen()` / the CI job (Scope, not the binding criterion)

- `run_jepsen()` now loops `for nemesis in tier1_jepsen_isolation_legs() { run_jepsen_scenario(test, nemesis)?; }`
  instead of a single call — each leg gets its OWN `docker compose up`/`down`
  cycle (`run_jepsen_scenario`, patched to take `nemesis` and use
  `nemesis.scenario_fn()` as a `cargo test ... --exact <fn>` filter via
  `jepsen_scenario_args`, patched to accept the filter).
- `run_jepsen_test` now also exports `WYRD_TIER1_COMPOSE_NETWORK` (the Compose
  v2 default-network name, `<project>_default`) — the network-partition leg's
  disconnect/reconnect target.
- `.github/workflows/tier1-jepsen.yml` — doc comment updated to describe both
  legs; `timeout-minutes` bumped 45 → 80 (two full cluster lifecycles now run
  sequentially instead of one).

## Why two separate cluster lifecycles, not one shared cluster

Considered running both legs against a single `docker compose up`: cheaper (no
second image-build / cluster-boot), and a smaller diff to `run_jepsen`/
`run_jepsen_scenario` (no `nemesis` threading, ~30 fewer changed lines in
`faults.rs`). **Rejected**: server 0 is `docker kill`ed by Phase 1 of *each*
leg's scenario body (`tier1_jepsen_consistency.rs`, both scenario functions'
"Kill server 0" step) — `docker kill` on an already-killed container fails, so
the second leg's Phase 1 would error immediately. Worse, after leg A's Phase 3
the committed placement is `[9,1,2,...,8]` (all 9 live nodes occupied, server 0
permanently dead) — there is no free 10th domain left for leg B's own
reconstruction to land on (`JC_DSERVER_COUNT = N + 1 = 10`, one spare, already
consumed). Sharing a cluster is not merely costlier here, it is topologically
wrong for the fleet size the brief keeps (`JEPSEN_DSERVER_COUNT = 10`,
unchanged) — Scope doesn't ask to resize the fleet to accommodate two
concurrent repairs. Two independent per-leg lifecycles is the smallest change
that keeps the freeze leg intact (Scope: "do not delete it") without
under-provisioning the new leg.

## Why `docker network disconnect`/`connect`, not `iptables`/`tc`

The brief leaves the mechanism to Do (Scope) but names `docker network
disconnect`/`reconnect` first. Chosen over an in-container `iptables`/`tc` cut
because: (a) it needs no extra container capability — `docker-compose.yml`
(`crates/chunkstore-grpc/tests/docker-compose.yml:1-22`) grants no `cap_add:
NET_ADMIN`, so `iptables`/`tc` inside the container would additionally require
changing the compose file (out of the stated Scope, which only asks for the
xtask/scenario/workflow changes); (b) it is a single external `docker` CLI
call, matching the existing `docker kill`/`pause`/`unpause` shell-out shape
exactly, so the new scenario stays structurally parallel to the existing one
(easier to review, same failure-mode handling).

## Why the compose-network name is a naming-convention assumption, not queried

`run_jepsen_test` derives the network as `format!("{JEPSEN_PROJECT}_default")`
— Docker Compose v2's documented default-network-naming convention — rather
than querying `docker network ls`/inspect post-`compose up` for the actual
network and parsing the result. The query approach is more robust to a future
compose-file network rename, but Do has no Docker in this environment to test
either the convention assumption OR a query-and-parse path, and this is
entirely inside the **deferred, off-Check** live-orchestration path (Verification
posture (b) — confirmed by the maintainer via the real `tier1-jepsen` job, not
by Check). Given neither path is exercisable by Do, the simpler assumption is
used and called out in a code comment at the derivation site
(`xtask/src/faults.rs`, `run_jepsen_test`) so the maintainer can correct it if
`docker-compose.yml` ever names its own network explicitly.

## Why not a single parameterized scenario function (env-var branch) instead of a second `#[ignore]`d fn

Considered collapsing both nemeses into ONE scenario function that reads an env
var (e.g. `WYRD_TIER1_ISOLATION_NEMESIS`) and branches Phase 2/3 on it — smaller
diff (one function body with ~10 branch points instead of a ~340-line
near-duplicate function). **Rejected**: the brief's Citations-expected field
describes the new leg as something that "parallels" the existing pause/unpause
injection points (plural functions), and its Scope says to "keep the existing
docker pause freeze leg as a **separate**" nemesis — read together with
"mirroring the existing `jepsen_dispatch` born-at-tier pattern" (a *routing*
decision between representable alternatives, each independently compiled and
type-checked), a single branchy function would fold the "value with both
alternatives representable" seam into a runtime flag *inside* one test body,
which is exactly the shape the brief's own `jepsen_dispatch` precedent
(`faults.rs:170-189`) was written to move *away from* (a hardcoded match arm
the test binds to only superficially). Keeping two functions also means the
compile/type-check bar (the born-at-tier criterion) independently proves BOTH
injection code paths are real, API-bound Rust — a single function only proves
whichever branch's code exists, with no structural signal distinguishing "both
paths compile" from "one path compiles, the other is dead behind an unset env
var."

## Falsifiability — demonstrated red, then reverted (forced check)

Per the brief's Falsifiability field, I temporarily negated each new seam,
confirmed the bound unit test goes red, then reverted:

**1. `xtask` pure-decision test** — temporarily changed
`tier1_jepsen_isolation_legs()` to `vec![IsolationNemesis::ProcessFreeze]` (the
freeze-only collapse ADR-0039 names as the gap this issue closes):

```
$ cargo test -p xtask --bin xtask faults::tests::tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only
test faults::tests::tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only ... FAILED
thread '...' panicked at xtask/src/faults.rs:749:9:
the Tier-1 Jepsen leg must include a network-level partition nemesis (Jepsen's `:partition`)
distinct from the process-freeze (`:pause`) nemesis — ADR-0039's #399 upgrade; got legs=[ProcessFreeze]
```

Reverted; re-ran — green (`test result: ok. 5 passed`).

**2. Node-liveness oracle test** — temporarily widened
`assert_node_live_during_isolation` to accept `"paused"` as OK (the exact
collapse a network-partition leg regressing to `docker pause` would produce):

```
$ cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency node_liveness
test node_liveness_during_isolation_fails_when_paused ... FAILED
thread '...' panicked at crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:612:5:
a `paused` container must fail the node-liveness-during-isolation oracle — a freezer-cgroup
process freeze suspends the node's own clock
```

Reverted; re-ran — green (`test result: ok. 17 passed; ... 2 ignored`).

## Refutation checklist (forced, per Do instructions)

**(a) Genuine red?** Yes for both new seams, demonstrated above via an actual
revert-and-rerun (not merely reasoned about): the dispatch test and the
liveness-oracle test both fail with the exact message pointing at the
regression, and both go back to green on revert.

**(b) Production path?** Yes. `tier1_jepsen_isolation_legs()` and
`IsolationNemesis::scenario_fn()` are the literal values `run_jepsen()`
iterates/dispatches on (`faults.rs`, patched `run_jepsen` body) — not a copy
consulted only by the test. `assert_node_live_during_isolation` is called
directly by the new `#[ignore]`d scenario body
(`jepsen_consistency_over_repair_under_live_partition_and_crash`) on the real
`docker_container_state()` shell-out result — the same function the unit tests
exercise, not a parallel re-implementation.

**(c) Fixture includes the fault?** For the Check-gated portion: yes — the
negative controls plant the exact anomaly the regression would produce
(`"paused"` container state; a freeze-only leg list), not a curated-out
approximation. For the deferred live E2E (see below): not exercised by Do at
all (no Docker in this environment) — this matches the brief's own declared
Verification posture (b), which puts that confirmation on the maintainer via
the privileged `tier1-jepsen` job, not on Check/Do.

## Compile/type-check bar (the other half of the binding criterion)

```
$ cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency --no-run
   Compiling wyrd-chunkstore-grpc ...
    Finished `test` profile [unoptimized + debuginfo] target(s)
```

`cargo test --workspace` (via `cargo xtask ci`) compiles both `#[ignore]`d
scenario functions; neither runs (both print `... ignored, Tier-1: needs real
containerized D servers`). Confirmed with a full `cargo xtask ci` run (below).

## Full-suite run

`cargo xtask ci` (fmt/clippy/build/test/deny/conformance/statics/DST) — the
project's own gate — passed clean on the second run. The FIRST run hit one
failure, `an_aborted_repair_is_not_counted_as_a_successful_repair`
(`crates/custodian/tests/reconstruction.rs`), in a file my patch does not touch
(`git diff --stat` confirms only `xtask/src/faults.rs`,
`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`, and
`.github/workflows/tier1-jepsen.yml` changed). I reproduced it in isolation
against `main` (git-stashed my changes) — it passed there too, and passed again
in a standalone re-run with my changes present — so it is a pre-existing
parallel-test flake (the failure message shows global OTel-meter counters:
`reconstruction_aborted_total`, `reconstruction_queue_depth_*` — consistent
with cross-test global-metrics-registry contention under `cargo test`'s default
parallel test-thread scheduling), not a regression this patch introduces. The
immediately-following full `cargo xtask ci` re-run was 100% green, including
that test.

`cargo fmt --check` (whole workspace) and
`cargo clippy -p xtask -p wyrd-chunkstore-grpc --all-targets` are both clean —
the patch is commit-ready for the target's own formatter/lint policy
(`[workspace.lints]`, root `Cargo.toml`).

## Deferred / not exercised by Do (matches the brief's own declared posture — not a new gap)

Per the brief's Verification posture (b) and External dependencies field, the
live end-to-end run (`WYRD_TIER1=1 cargo xtask jepsen` actually standing up
containers, disconnecting/reconnecting the network, and observing the
container stay `running`) requires Docker + compose + network-disconnect
capability, which this Do environment does not have — exactly as the brief
already declares ("provided by the existing privileged `tier1-jepsen` GitHub
job... NOT by Check"). This is not a dependency Do discovered that Plan
missed; it is the brief's own stated deferred posture, and the brief
explicitly says "do NOT scope the binding criterion to it." The binding
(Check-gated) criterion — the pure decision + its negative control, the
node-liveness oracle + its negative control, and the scenario's compile/
type-check — is fully demonstrated red→green above.

Concrete manual-validation steps for the maintainer, on a Docker-capable host:

1. `WYRD_TIER1=1 cargo xtask jepsen` — expect TWO leg passes to run in
   sequence (freeze, then network-partition), each with its own
   `docker compose up` → scenario → `docker compose down` cycle.
2. During the second (network-partition) pass, while Phase 2 is disconnected,
   manually run `docker inspect -f '{{.State.Status}}' wyrd-tier1-jepsen-dserver-2`
   and confirm it reports `running` (never `paused`).
3. Confirm both legs' assertions pass (process exit 0) and that
   `target/tier1-logs/docker-compose.log` still captures diagnostics on a
   deliberately-induced failure (unchanged from the existing behaviour, #150).
4. If `crates/chunkstore-grpc/tests/docker-compose.yml` ever declares its own
   `networks:` section, update the `{JEPSEN_PROJECT}_default` assumption in
   `run_jepsen_test` (`xtask/src/faults.rs`) to match.

## Files changed (target `main`)

- `xtask/src/faults.rs` — `IsolationNemesis`, `tier1_jepsen_isolation_legs`,
  `jepsen_scenario_args` (now takes an `--exact` filter), `run_jepsen`,
  `run_jepsen_scenario`, `run_jepsen_test` (all patched/added), plus the new
  unit test and the updated call site of the existing one.
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — new
  `assert_node_live_during_isolation` oracle + 3 unit tests,
  `docker_container_state` helper, new `#[ignore]`d
  `jepsen_consistency_over_repair_under_live_partition_and_crash` scenario,
  module-doc updates.
- `.github/workflows/tier1-jepsen.yml` — doc comment + `timeout-minutes`
  45 → 80 (two leg-runs now).
