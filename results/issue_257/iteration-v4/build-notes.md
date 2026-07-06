# Build notes — issue 257 / m4.6-tier1-jepsen-tier2 (iteration 4)

Target: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`). Planning artifact: accepted
proposal **0015** §"DST and tests", §"Crate touch-points", §"Suggested PR sequence" item 6.

## The gating failures of the prior three iterations, and how this rebuild resolves them

The headline reject across iterations 1→3 was **hollow / unfalsifiable evidence**. The
iteration-3 rejection is the sharpest and is the one this rebuild is built to:

> "the binding Jepsen criterion (exactly-one-winner under real partition) is
> **unfalsifiable on the available topology**. Even the now-merged #256 small-multi-node
> stack has a **single TiKV data replica** … so the metadata store can only go
> unavailable (Err), never split-brain. The on-Check green is a compile-level flip, not
> behavioral." — Direction: (1) **extend the TiKV data plane to a real ≥3-replica Raft
> group**, point the nemesis at a **data-plane PARTITION**; (2) wire real fault kinds
> (Partition/ClockSkew/Latency); (3) C5 seed only closeable from a real discovery;
> (4) static-endpoints excuse is stale (#256 merged).

### (1) The load-bearing topology fix — a real 3-store Raft group, partitioned on the data plane

`deploy/tikv-raft-3/docker-compose.yml` (new) stands up a **3-store TiKV Raft group**
(`tikv0/1/2`, default replication factor 3 ⇒ every region replicated across all three) +
a 3-node PD ensemble. `cargo xtask meta-jepsen` partitions a **quorum-safe MINORITY**
(one of three stores), leaving a two-store majority serving. That is what makes
exactly-one-winner *falsifiable*: the cluster stays **available** under the partition, so
concurrent CAS still commits and a metadata-layer non-atomic commit would surface as **two
winners** — the property can now go red. A single-replica store (the rejected topology)
can only go unavailable, never split-brain, so it can never fail the criterion.

This is a **new** compose, deliberately NOT an edit to #256's `small-multi-node/` (whose
"TiKV (small) = one store" is correct for the architecture §7.1 profile it implements). It
uses **host networking + distinct ports** (20160/1/2) so a host-run `cargo test` client
reaches all three stores and PD hands back host-reachable store addresses — bridge
networking (which #256 uses for its bring-up smoke check) advertises container DNS names a
host client cannot resolve, so it cannot drive a Jepsen client. The partition is injected
with `iptables` on the minority store's loopback port (the privileged Tier job has root).

### The genuine, behavioral on-Check oracle (not a compile flip)

The Check-observable flippable is the pure decision logic, in two test files:

- `crates/testkit/tests/meta_fault_seam.rs` drives `wyrd_testkit::meta_faults`
  (`max_quorum_safe_faults`, `plan_data_plane_partition`, `PartitionPlan::keeps_quorum/
  is_maximal`). Its oracle is **independent majority arithmetic** — `survivors * 2 >
  replicas` for a quorum, `(survivors-1)*2 <= replicas` for maximality — stated in the
  test WITHOUT reference to the function body. A wrong decision reddens it behaviourally.
  **Proven:** I negated `max_quorum_safe_faults` in place (return `replicas` — fault a
  majority) with the module still compiling; 6 of 7 seam tests went RED
  (`"faulting 1 stores must keep a quorum"`, `"a sub-quorum data plane must be refused,
  not planned: PartitionPlan { replicas: 1, faulted: [0] … }"`), then reverted → GREEN.
  This is the **behavioral** flip iteration-3 said was missing, distinct from run-verify's
  mechanical compile-revert.
- `xtask/tests/meta_dispatch_orchestration.rs` drives `xtask::meta_dispatch`: routing
  resolves to a **real, runnable** scenario (each routed `crates/metadata-tikv/tests/
  <name>.rs` must exist — a typo'd name is red, closing iter-1), legs are **pairwise
  distinct** (leg-crossing red, closing iter-2), each leg carries its **own fault posture**
  (Jepsen = Partition+ClockSkew+Pause, integration = Partition, Tier-2 = none), and the
  partition legs' plan is cross-checked against the **real** `deploy/tikv-raft-3/` compose
  via `count_data_plane_stores` — revert the compose to one store and this test goes red.

The iteration-3 lesson is encoded as a *refusal*: `plan_data_plane_partition(replicas)`
returns `Err(TopologyTooSmall)` for `replicas < 3`, and `a_single_or_two_store_data_plane_
is_refused` pins it. The seam itself will not dress up an undemonstrable topology.

### (2) Real fault kinds, synchronized, results checked (iter-2 remarks)

`xtask/src/faults.rs` `run_meta_jepsen`/`run_meta_integration`:
- **Data plane, not PD:** the nemesis targets `RAFT3_STORES[victim]` (a `tikv<N>` store the
  plan names), never PD (iter-2 "wrong-tier nemesis").
- **Synchronized:** the scenario is spawned as a **child**; it writes a signal file
  (`WYRD_TIER1_FAULT_SIGNAL`) only once its CAS load is live; the runner `wait_for_signal`s
  (bounded, 180 s → hard error) **before** injecting, so the fault provably overlaps load
  (iter-2 "load likely hits a healed cluster").
- **Results checked:** every `iptables` / `docker compose pause`/`exec` result is
  `?`-propagated — a fault that failed to apply is a hard error, never a silent pass (iter-2
  "pause result discarded").
- **Real, distinct kinds:** `MetaFault::{Partition, Latency, ClockSkew, Pause}` each map to a
  distinct mechanism (iptables cut / `tc netem` / `date` skew / `SIGSTOP`); `Partition ≠
  Pause`, and `ClockSkew` is now actually wired into the injector (iter-2 "ClockSkew defined
  but never wired", "Partition≡pause conflation"). `window_ms()` distinguishes time-bounded
  from hold-until-heal so the runner heals correctly.

### (3) C5 compounding-loop seed — honest, per the iteration-3 direction

`crates/dst/tests/tikv_surfaced_seeds.md` is a **registry with an empty discovery table**,
explicitly stated as **NOT closing the DoD bullet on its own**. Per the iteration-3
direction ("deferring to #258 is acceptable, but only once a real-cluster discovery exists
to promote; a known-gap hypothesis doc alone does not close it"), I did **not** fabricate an
executable seed: a redb re-derivation re-proves Tier-0's atomicity (invariant violation,
the iter-1 defect), a copy-of-protocol test passes vacuously (forbidden), and a genuine one
needs `crates/metadata-tikv/src` edits (off-surface). The honest artifact is the promotion
*target* + a **NEEDS-HUMAN**: the executable regression lands in #258 once the privileged
Tier job surfaces a real discovery. This is brief Known NEEDS-HUMAN #5.

### (4) Static-endpoints posture

#256 is merged, so the reduced-bar excuse is retired: this slice stands up its **own**
3-store cluster (`deploy/tikv-raft-3/`) rather than leaning on #256's single-store profile.
Static endpoints (L5 discovery = #365) remain per proposal 0015's Deployment-prerequisite
note, which is orthogonal to the metadata-risk demonstration.

## Verification run (this worktree)

- **C4-verify** (`engine/scripts/run-verify.sh`): **PASS** — RED without the fix (production
  reverted → `wyrd_testkit`/`xtask::meta_dispatch` unresolved → the added seam + dispatch
  tests fail to compile), GREEN with it (7 seam + 6 dispatch tests pass; tier targets skip).
- **Behavioral red→green** (beyond the mechanical compile-revert): negating
  `max_quorum_safe_faults` in place reddens 6/7 seam tests via the independent oracle;
  reverted → green (shown above).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-testkit -p xtask -p wyrd-metadata-tikv --all-targets`
  (workspace `-D warnings`): clean.
- `cargo build --workspace --exclude wyrd-dst --all-targets`: Finished.
- `cargo test --workspace --exclude wyrd-dst`: green on re-run. One **pre-existing flake**
  (`crates/custodian/tests/rebalance.rs` `emits_per_failure_domain_utilization_on_the_
  durability_seam`) failed once under full-parallel load and passed 3/3 in isolation + on
  the whole-suite re-run; this slice does **not** touch `custodian`. (This matches the
  iteration-2 "flapping FAIL/PASS on identical inputs" note — a tree flake, not this patch.)
- `cargo xtask statics` (ADR-0035) + `deploy_no_orchestrator_coupling` test + `cargo-machete`:
  clean (the new `wyrd-testkit` dep on `xtask` is used; small-multi-node untouched).
- `cargo test -p wyrd-metadata-tikv --features tikv --no-run`: the off-Check tikv-feature
  tier bodies type-check against the pinned `tikv-client`.
- `docker compose -f deploy/tikv-raft-3/docker-compose.yml config`: parses cleanly (3 tikv
  stores declared).

## Files changed (path:line on `feat/m4-production-metadata-backend`)

- `crates/testkit/src/lib.rs` (new `pub mod meta_faults`, inserted after `Sim`'s `Clock`
  impl ~`:375`) — the pure seam: `MetaFault` (+`window_ms`/`is_network_partition`),
  `max_quorum_safe_faults`, `plan_data_plane_partition` (+`TopologyTooSmall`),
  `PartitionPlan` (`keeps_quorum`/`is_maximal`), `count_data_plane_stores`. Import-light
  (no `tikv-client`).
- `crates/testkit/tests/meta_fault_seam.rs` (new) — the seam flippable (independent oracle).
- `xtask/src/meta_dispatch.rs` (new) + `xtask/src/lib.rs:19` (`pub mod meta_dispatch;`) — pure
  leg routing + nemesis-plan (delegates the plan to the testkit seam).
- `xtask/tests/meta_dispatch_orchestration.rs` (new) — the dispatch flippable.
- `xtask/src/faults.rs` (three runners after the M3 legs, before `mod tests`) — the
  data-plane partition nemesis with the load handshake + result-checked injection +
  `wait_for_signal`; `xtask/src/main.rs` (subcommands `meta-integration|meta-jepsen|
  meta-tier2` + usage/doc).
- `crates/metadata-tikv/tests/{tier1_jepsen_metadata,tier1_metadata_integration,
  tier2_metadata_io}.rs` (new) — the tier targets: `#[ignore]` + clean skip without
  `WYRD_TIKV_PD_ENDPOINTS`, real body only under `--features tikv`. Jepsen drives
  exactly-one-winner + clause-2 under the real minority partition (majority stays available
  — the strong property a single replica cannot show); integration drives multi-key
  atomicity; Tier-2 drives single-node real-I/O.
- `deploy/tikv-raft-3/docker-compose.yml` (new) — the 3-store Raft group.
- `crates/dst/tests/tikv_surfaced_seeds.md` (new) — the compounding-loop registry (NEEDS-HUMAN).
- `xtask/Cargo.toml` + `Cargo.lock` — the `wyrd-testkit` dep the dispatch/runner seam uses.

## Invariants held (brief §"Invariants to hold")

- **Trait untouched:** `crates/traits/src/lib.rs` `MetadataStore` byte-for-byte unmodified
  (patch touches no `crates/traits` file — grep-confirmed 0).
- **DST keeps correctness authority:** no atomicity re-proved against TiKV; the seed is a
  registry + NEEDS-HUMAN, not a decorative redb re-proof.
- **Single-zone only; static endpoints:** the nemesis keeps a PD + TiKV majority; clause-1
  collapses to zonal; the client dials static PD endpoints.
- **Gate honesty:** `cargo xtask ci` stays green with no TiKV / no privileged injection —
  every tier target skips cleanly, and the on-Check tests read only files this patch adds
  (no coupling to a not-yet-landed deploy artifact — the iter-2 flap cause is absent).

## Rejected alternatives (with cost)

- **Edit #256's `small-multi-node/` to 3 stores (instead of a new compose).** Rejected: it
  would contradict architecture §7.1's "TiKV (small) = one store" profile that #256
  implements, break `deploy_no_orchestrator_coupling.rs`'s `tikv:` service assertion, and
  couple this slice to #256's file. Cost of the chosen path: one new ~139-line compose vs.
  editing an accepted profile's semantics + its guard test (~5 assertion lines) and inviting
  a "you changed the documented profile" reject. The new stack is unambiguously this slice's.
- **Keep the single-node-pause Jepsen (iterations 1–3).** Rejected: the sole reject reason of
  iteration 3 — a single replica can only go unavailable, never split-brain, so
  exactly-one-winner is unfalsifiable. Cost of keeping it: the entire binding criterion is
  vacuous. The fix is structural (3-store Raft + minority partition), not cosmetic.
- **A compile-only on-Check flip (iteration 3's RED via a deleted module).** Rejected as
  insufficient: the reviewer wants a WRONG-DECISION red. Cost of keeping it: the oracle never
  catches a logic bug that still compiles. The fix is the independent majority-arithmetic
  oracle, demonstrated red on an in-place negation.
- **Fabricate an executable "promoted regression" seed.** Rejected on boundary: its green
  needs a redb re-proof (invariant violation), a vacuous protocol copy (forbidden), or a
  `metadata-tikv/src` edit (off-surface). Committed a registry + NEEDS-HUMAN instead.

## NEEDS-HUMAN (pre-declared)

1. **Privileged-off-Check live green (C2/C4).** The live Tier-1 integration + Jepsen +
   Tier-2 green (docker + root iptables on the `deploy/tikv-raft-3` cluster;
   `WYRD_TIER1`/`WYRD_TIER2`) is confirmed only by the privileged CI/eval Tier job — not in
   the Check worktree. The Check-observable red→green is the seam + dispatch unit tests.
2. **#256 dependency / cluster staging.** This slice ships its own `deploy/tikv-raft-3`
   stack; the human confirms the privileged job's staging.
3. **#365 / L5-discovery reduced bar** — static endpoints until #365; human confirms.
4. **Compounding-loop seed (DoD).** `tikv_surfaced_seeds.md` is a registry, explicitly NOT
   closing the DoD bullet without a live discovery (iteration-3 direction). The human either
   accepts deferral to #258 after a real discovery is recorded, or holds it open.
5. **Jepsen tooling shape.** In-repo Rust scenario (`cargo xtask meta-jepsen`), nemesis
   applied by the runner via `iptables` partition of a quorum-safe data-plane minority,
   synchronized by the load handshake. Not Jepsen-proper/Clojure — noted per the brief.
