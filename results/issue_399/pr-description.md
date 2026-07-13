# Tier-1 Jepsen: add a live network-partition nemesis

## Summary
**User impact:** Wyrd's automated durability suite is supposed to prove that a
storage node which is cut off from the network — but still running — cannot leave
data stale, torn, or double-committed once the network comes back. That guarantee
was never actually being tested. The one test leg that claimed to simulate a
"network partition" instead **froze the entire node** (suspending it, clock and
all), which is a weaker and different failure. So a real-world scenario — a node
that is alive and ticking but unreachable — went unexercised, and a maintainer
reading the test names would reasonably (but wrongly) believe it was covered.

This PR adds a second, distinct fault that isolates the node at the network level
while it keeps running, then heals it and checks the same consistency guarantees —
plus that the node genuinely stayed alive throughout. The original freeze test is
kept as a cheaper, complementary check, not replaced.

This is test-infrastructure only; no production code path changes.

## What to look at
The change lives entirely in the Tier-1 test harness:

- `xtask/src/faults.rs` — the runner now iterates over **two** isolation faults
  (process-freeze, then network-partition) instead of one.
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — a new scenario that
  isolates the node with an in-namespace `iptables` packet drop (so the container
  stays `running` and keeps its published port), plus a `assert_node_live_during_isolation`
  check.
- `crates/chunkstore-grpc/tests/dserver/Dockerfile` — installs `iptables` so the
  throwaway sidecar that injects the drop has the binary available.
- `.github/workflows/tier1-jepsen.yml` — the scheduled privileged job now runs both
  legs (timeout raised accordingly).

To try it, a Docker-capable maintainer runs `WYRD_TIER1=1 cargo xtask jepsen`: it
stands up the cluster twice (once per leg). During the network-partition leg,
`docker inspect -f '{{.State.Status}}' <isolated-container>` reports `running` while
the node is unreachable, and the repair converges cleanly after the drop is flushed.
Everything except that privileged live run executes in an ordinary `cargo test` /
`cargo xtask ci` build.

## Root cause
The Tier-1 leg injected its "unreachable node" fault with `docker pause`/`unpause`, a
freezer-cgroup process freeze (Jepsen's `:pause`) rather than a network cut (Jepsen's
`:partition`). A frozen node stops its own clock, so the suite could not exercise a
node that stays live — running its lease-renewal and request-timeout timers — while
network-isolated, which is what a real partition looks like.

## Fix
Model the isolation mechanism as a value with both alternatives representable
(`ProcessFreeze`, `NetworkPartition`), each routing to its own scenario function, and
run both legs from the harness. The new leg isolates the node with a network-level
`iptables` DROP on its gRPC port, injected by a throwaway sidecar that shares the
node's network namespace — so the node container is never disconnected, paused, or
otherwise touched. It stays `running` and keeps its host-published-port mapping, so
the heal (flush the rule) restores reachability at the same endpoint the test already
holds, and the existing ADR-0015 repair-path assertions run across the heal. The
cheaper freeze leg is retained.

## Verification
- **Claim:** the Tier-1 leg gains a network-partition nemesis distinct from the
  process-freeze nemesis, selected by a pure, test-observable decision.
  - **Checked:** `xtask/src/faults.rs:207` (`IsolationNemesis`, both alternatives) and
    `:245` (`tier1_jepsen_isolation_legs` — the value the run loop iterates) on `main`.
  - **Test:** `xtask/src/faults.rs:755`
    (`tier1_jepsen_isolation_legs_includes_network_partition_not_freeze_only`) —
    fails when the leg is negated to freeze-only, passes with the fix; runs in
    `cargo xtask ci`.
- **Claim:** the isolated node stays live (`running`, never `paused`) for the whole
  fault window — the property the freeze leg cannot exhibit.
  - **Checked:** `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`
    `assert_node_live_during_isolation` and its use in the new scenario
    (`jepsen_consistency_over_repair_under_live_partition_and_crash`, around `:1277`)
    on `main`.
  - **Test:** `node_liveness_during_isolation_fails_when_paused` (and `_when_exited`,
    `_passes_when_running`) in the same file — the `paused` negative control is
    exactly the input a regression to `docker pause` would produce; fails when the
    oracle is widened to accept `paused`, passes with the fix; runs in `cargo xtask ci`.
- **Claim:** the network-partition scenario is real, compiled code, not inert
  scaffolding.
  - **Checked:** the `#[ignore]`d scenario body compiles and type-checks under
    `cargo test --workspace`; the harness wiring
    (`.github/workflows/tier1-jepsen.yml:60`, both legs) and sidecar image support
    (`crates/chunkstore-grpc/tests/dserver/Dockerfile:30`) are in place on `main`.
  - **Test:** local `cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency`
    → 17 passed, 2 ignored (both scenario bodies compiled). The live
    partition-and-heal end-to-end run is deferred to the privileged `WYRD_TIER1=1`
    `tier1-jepsen` job and must be confirmed there by a maintainer:
    `WYRD_TIER1=1 cargo xtask jepsen` — the network-partition leg reaches the post-heal
    repair commit, with the isolated container reporting `running` during isolation.

Fixes #399
