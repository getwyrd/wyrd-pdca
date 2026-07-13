# Build notes — issue 399 / tier1-jepsen-live-network-partition (iteration 2)

(Withheld from the reviewer per the Do contract — this is for the human at sign-off.)

Target branch: `getwyrd/wyrd@main`. All line citations are against the
`$PDCA_WORKTREE` checkout of that branch (`fix/399-tier1-live-network-partition`,
cut from `main` at bd5470a; `git diff main..HEAD` was empty before this patch).

## The carry-forward this iteration addresses

Iteration 1 was rejected at sign-off. The live `WYRD_TIER1=1 cargo xtask jepsen`
run (human) showed: ProcessFreeze leg passed end-to-end; the NetworkPartition leg
FAILED at Phase 3 heal — `Store(Unavailable("tcp connect error" … 127.0.0.1:<port> …
Connection refused))` dialing the reconnected isolated node. **Root cause named in the
carry-forward:** iteration 1 used `docker network disconnect`/`connect`, which tears
down the container's published-port proxy and does NOT restore the host-port forwarding
on reconnect. The leg broke its own reachability before the ADR-0015-across-heal
assertions could run — so the live partition-and-heal deliverable the issue exists to
add was never actually demonstrated.

The carry-forward's directive: **replace `docker network disconnect`/`connect` with a
mechanism that keeps the container's network identity and host-published port mapping
intact across the fault window** (it names an in-container `iptables`/`tc` packet drop as
the first alternative, or re-dialing the restored route on heal), and **preserve what is
sound and green** (the pure `IsolationNemesis` decision + unit test, the
`assert_node_live_during_isolation` oracle + negative controls, and the untouched
ProcessFreeze leg).

## What changed vs iteration 1 (and what is preserved verbatim)

**Preserved (the sound, green Check-binding seams — carry-forward says do not disturb):**
- `IsolationNemesis` value with both alternatives representable + `tier1_jepsen_isolation_legs()`
  (`xtask/src/faults.rs:208`, `:245`) and its unit test
  `tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only`
  (`faults.rs:755`) — mirrors the `jepsen_dispatch` born-at-tier pattern
  (`faults.rs:179`, cited by the brief).
- `assert_node_live_during_isolation` oracle (`tier1_jepsen_consistency.rs:390`) + its
  positive case and two negative controls (`"paused"`, `"exited"`) at `:596-635`.
- The ProcessFreeze leg (`docker pause`/`unpause`, `tier1_jepsen_consistency.rs:832`/`:901`)
  — untouched.

**Changed (the mechanism — the ONE thing the carry-forward rejected):**
The live-partition leg no longer disconnects the container from the compose network.
Instead it injects a **network-level packet drop that never removes the container from
its network**, via an in-netns `iptables` sidecar:

- New helper `docker_netns_grpc_drop(image, container, verb)`
  (`tier1_jepsen_consistency.rs:706`) runs
  `docker run --rm --net=container:<isolated> --cap-add=NET_ADMIN --user=0
  --entrypoint=iptables <image> {-A|-D} INPUT -p tcp --dport 50051 -j DROP`.
  The sidecar **shares the isolated D-server's network namespace** (`--net container:`),
  so the DROP rule it adds applies to that node's traffic — the standard
  chaos/istio-init technique. The D-server container itself is **never disconnected,
  paused, `exec`'d into, or otherwise touched**.
- Phase 2 (`:1470`-ish, in the new scenario): inject `-A`, then read
  `docker_container_state` (`:683`) and assert `assert_node_live_during_isolation` —
  the container is `running` (trivially and correctly: a network partition must be
  orthogonal to process state).
- Phase 3 heal: `-D` to flush the rule. Because the container never left its network,
  its published-port DNAT/proxy is intact, so `reconcile_step` re-reaches server 1 at the
  **same `127.0.0.1:<host-port>` endpoint the scenario already holds** — the exact failure
  iteration 1 hit is structurally removed.
- Plumbing: `run_jepsen_test` now exports `WYRD_TIER1_DSERVER_IMAGE`
  (`xtask/src/faults.rs:404`, value `wyrd-dserver:test` — `JEPSEN_DSERVER_IMAGE`,
  `faults.rs:146`) instead of iteration 1's `WYRD_TIER1_COMPOSE_NETWORK`. The new leg
  reuses the D-server image as its iptables sidecar, so no compose-network name is needed.
- `crates/chunkstore-grpc/tests/dserver/Dockerfile:23-32` installs `iptables` in the
  runtime image (the sidecar reuses this image; the D-server process still runs as the
  non-root `dserver` user with no added capability — the binary just has to be present).

**Not changed:** `docker-compose.yml` is untouched. The sidecar carries `--cap-add
NET_ADMIN` on its own throwaway `docker run`; the long-lived D-servers keep the #286
non-root/no-extra-caps hardening. That is deliberately a smaller blast radius than the
alternative of granting every `dserver` service `NET_ADMIN` in the shared compose (which
would also weaken the Tier-2 integration / kill-reconstruct clusters that share the file).

## Why this mechanism, and why not the other options (with costs)

- **`docker network disconnect`/`connect` (iteration 1)** — rejected: it is the exact
  root cause the carry-forward names. Disconnect removes the container's interface;
  reconnect assigns a new IP and does NOT recreate the published-port DNAT rule (that is
  created only at container start), so the host port is permanently dead → Phase 3
  `Connection refused`. Not fixable by tuning; the mechanism destroys the endpoint.
- **Re-resolve + re-dial on heal (carry-forward's second option)** — rejected: it cannot
  work on top of `disconnect`/`connect`, because after reconnect there is **no** host-port
  forwarding to re-resolve (`docker compose port` returns nothing). It only helps if the
  mapping still exists — which is precisely what the iptables approach guarantees and
  `disconnect` destroys. Choosing "keep the mapping" dominates "re-find the lost mapping".
- **In-container `iptables` via `docker exec` + `cap_add: [NET_ADMIN]` in
  docker-compose.yml** — viable, but costs a change to the *shared* compose file that
  grants NET_ADMIN to every `dserver` in every tier (Tier-2 integration +
  kill-reconstruct too), plus a `docker exec --user 0` into a container whose image would
  still need iptables. The netns-sidecar gets the same effect with **compose.yml = 0 lines
  changed** and NET_ADMIN scoped to a throwaway container, so the running D-servers keep
  their #286 hardening. Same Dockerfile cost (iptables in the image) either way.
- **Host-firewall DROP on the published loopback port (no image/compose change)** —
  rejected: it needs the cargo-test process to run `iptables` on the host, i.e. host root
  / passwordless sudo from inside a test — an unstated environment assumption, and it
  never touches the container so the node-liveness oracle would be checking a container
  the mechanism provably can't affect (weaker signal than isolating the node's own netns).

The load-bearing NEW property this slice adds is **node liveness during isolation**
(container stays `running`, never `paused`) — ADR-0039 says the repair-path OUTCOME is
unchanged over today's dumb, process-local-`MemCoordination` D-servers, so the cross-node
"stale action on heal" teeth are explicitly out of scope (gated on networked coordination,
ADR-0006). The iptables-DROP mechanism keeps the node fully live (its lease-renewal loop
and request-timeout keep ticking — `crates/server/src/cli.rs:309`/`:276`), which is the
faithful `:partition` the freeze `:pause` cannot model.

## Falsifiability — demonstrated red, then reverted (forced check)

Both new Check-binding seams were negated, shown red, and reverted:

1. **`xtask` pure-decision test** — set `tier1_jepsen_isolation_legs()` to
   `vec![IsolationNemesis::ProcessFreeze]` (the freeze-only collapse ADR-0039 names):
   ```
   test faults::tests::tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only ... FAILED
   panicked at xtask/src/faults.rs:754: the Tier-1 Jepsen leg must include a network-level
   partition nemesis (Jepsen's `:partition`) … got legs=[ProcessFreeze]
   ```
   Reverted → `test result: ok. 5 passed`.

2. **Node-liveness oracle test** — widened `assert_node_live_during_isolation` to accept
   `"paused"` (the exact collapse a partition leg regressing to `docker pause` produces):
   ```
   test node_liveness_during_isolation_fails_when_paused ... FAILED
   panicked at crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:612: a `paused`
   container must fail the node-liveness-during-isolation oracle …
   ```
   Reverted → `test result: ok. 17 passed; 2 ignored`.

## Refutation checklist (forced, per Do instructions)

- **(a) Genuine red?** YES — both negations above were actually reverted-and-rerun (not
  reasoned): each fails with the exact regression message, each returns to green on revert.
- **(b) Production path?** YES. `tier1_jepsen_isolation_legs()` /
  `IsolationNemesis::scenario_fn()` are the literal values `run_jepsen()` iterates on
  (`faults.rs:305-313`), not a copy. `assert_node_live_during_isolation` is called by the
  new `#[ignore]`d scenario body (`tier1_jepsen_consistency.rs:1277`,
  `docker_container_state` → oracle) — the same function the unit tests exercise.
- **(c) Fixture includes the fault?** For the Check-gated portion: YES — the negative
  controls plant the exact anomaly (`"paused"` state; a freeze-only leg list). For the
  deferred live E2E (see below): NOT exercised by Do — no Docker in this environment,
  matching the brief's declared Verification posture (b).

## NEEDS-HUMAN — the deferred live run (Verification posture (b))

The binding, Check-gated criterion (pure decision + oracle + negative controls + the
`#[ignore]`d scenario's compile/type-check) is fully demonstrated red→green above and is
container-free. The **live network-partition-and-heal end-to-end run** is, per the brief's
own Verification posture (b) and External dependencies, observable ONLY on the privileged
`WYRD_TIER1=1` `tier1-jepsen` runner (Docker + compose + `NET_ADMIN`-capable
`docker run`). Do has no Docker here, so it CANNOT reproduce the iteration-1 failure or
confirm the fix live. Because iteration 1 was rejected specifically on that live run, the
maintainer must re-run it to confirm the new mechanism heals cleanly:

```
NEEDS-HUMAN external dependency: Docker + privileged tier1-jepsen runner (WYRD_TIER1=1) — cannot run the live network-partition-and-heal E2E at Check; the iteration-1 heal failure and this mechanism fix are only observable on that runner. Confirm the NetworkPartition leg reaches Phase 3+ green.
```

**Manual validation steps for the maintainer (Docker-capable host):**
1. `WYRD_TIER1=1 cargo xtask jepsen` — expect TWO leg passes in sequence (ProcessFreeze,
   then NetworkPartition), each with its own `docker compose up`→scenario→`down` cycle.
2. During the NetworkPartition pass, while Phase 2 is isolated, run
   `docker inspect -f '{{.State.Status}}' wyrd-tier1-jepsen-dserver-2` → must report
   `running` (never `paused`). Optionally `docker exec` / `iptables -L INPUT` in the
   netns to see the DROP rule present, then absent after heal.
3. Confirm Phase 3 heal reaches `Reconciled::Changed` (repair commits) — i.e. server 1 is
   reachable again at the SAME `127.0.0.1:<host-port>` (the iteration-1 `Connection
   refused` must be gone) — and Phases 4–6 (exactly-once, read-after-commit, data
   integrity) pass.
4. Sanity: `docker run --rm --net=container:<isolated> --cap-add=NET_ADMIN --user=0
   --entrypoint=iptables wyrd-dserver:test -L INPUT` succeeds (iptables present in the
   image; netns sharing works on the runner's Docker).

## Local sanity (container-free, via cargo in `$PDCA_WORKTREE`)

- `cargo test -p xtask --bin xtask faults::tests` → 5 passed.
- `cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency` → 17 passed, 2
  ignored (both scenario functions compiled + type-checked, neither run — the born-at-tier
  compile bar).
- `cargo fmt --all --check` clean; `cargo clippy -p xtask -p wyrd-chunkstore-grpc
  --all-targets` clean; `typos` clean on all four touched files — commit-ready for the
  target's own hooks. (The full `cargo xtask ci` suite is Check's gate; iteration 1 noted
  a pre-existing parallel-test flake in `crates/custodian/tests/reconstruction.rs`
  unrelated to this test-infra patch.)

## Files changed (target `main`)

- `xtask/src/faults.rs` — `IsolationNemesis` (+`scenario_fn`), `tier1_jepsen_isolation_legs`,
  `JEPSEN_DSERVER_IMAGE`, `jepsen_scenario_args` (now `--exact <fn>`), `run_jepsen` loop,
  `run_jepsen_scenario`/`run_jepsen_test` (thread `nemesis`, export
  `WYRD_TIER1_DSERVER_IMAGE`), + the new unit test and updated dispatch-test callsite.
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `assert_node_live_during_isolation`
  oracle + 3 unit tests, `docker_container_state` + `docker_netns_grpc_drop` helpers,
  `DSERVER_GRPC_PORT`, the new `#[ignore]`d
  `jepsen_consistency_over_repair_under_live_partition_and_crash` scenario, module-doc updates.
- `crates/chunkstore-grpc/tests/dserver/Dockerfile` — install `iptables` in the runtime stage
  (for the netns sidecar; D-server process unchanged, still non-root).
- `.github/workflows/tier1-jepsen.yml` — doc comment (both legs, iptables mechanism) +
  `timeout-minutes` 45 → 80 (two leg runs now).
