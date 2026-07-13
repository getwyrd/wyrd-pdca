# Brief — issue 399 / tier1-jepsen-live-network-partition

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-jepsen-live-network-partition
- **Defect:** The Tier-1 Jepsen consistency leg (ADR-0039;
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` + `xtask/src/faults.rs`)
  injects its "unreachable node" fault with `docker pause`/`unpause`. That is a
  **freezer-cgroup process freeze** (the whole container is suspended), which is Jepsen's
  `:pause` nemesis — NOT its `:partition` nemesis. A frozen node stops its own clock; a
  truly network-partitioned node stays **live** — it keeps running, times out, expires its
  own leases, and can attempt a stale action when the network heals. The leg is therefore a
  strictly weaker nemesis than the "partition and heal" it is meant to model, and cannot
  exercise Wyrd against a still-live-but-isolated node. ADR-0039 records this honestly and
  names the upgrade as additive (this issue, #399).
- **Success criterion:** Demonstrable at C4-verify (Check, container-free): the leg gains a
  **network-partition nemesis distinct from the existing process-freeze (pause) nemesis**,
  represented as a pure, test-observable value/decision in `xtask/src/faults.rs` (mirroring
  the existing `jepsen_dispatch` born-at-tier pattern) AND exercised by a new born-at-tier
  oracle that asserts **node-liveness-during-isolation** (the isolated node stays running on
  its own clock, not frozen). Both are covered by non-`#[ignore]` unit tests that run inside
  `cargo xtask ci`'s `cargo test --workspace`, INCLUDING a negative control that goes RED if
  the partition leg collapses back to a freeze/pause nemesis. The `#[ignore]`d scenario body
  that injects the live partition compiles and type-checks under `cargo test --workspace` at
  Check (the born-at-tier compile bar). The end-to-end live run is deferred off-Check (see
  Verification posture) — do NOT scope the binding criterion to it.
- **Falsifiability:** The Check-binding criterion is made to go RED at Check by pointing the
  new nemesis decision at the freeze (pause) mechanism: the node-liveness oracle / the pure
  dispatch unit test then fails. Because this is net-new coverage, Do MUST demonstrate that
  red via a temporary negation (a stub that routes the partition leg to `docker pause`, or a
  planted "container is paused" input to the oracle), proving the new seam is load-bearing
  rather than resting red on non-existence. The **live end-to-end** criterion (a real
  network partition that genuinely isolates a still-RUNNING container, and every ADR-0015
  property holding across the heal) can go RED only on the privileged Docker
  `WYRD_TIER1=1` `tier1-jepsen` runner — it CANNOT go red on the container-free C4-verify
  worktree Do is pointed at. That is a declared deferred posture confirmed by that job, not a
  Plan-blocking gap: the deliverable IS built and exercised at Check (the pure decision +
  oracle unit tests + the compile/type-check of the scenario), exactly as ADR-0039's whole
  Tier-1 approach and its two merged sibling legs establish. NOTE on teeth: over today's
  *dumb* D-servers the pause and live-partition nemeses are observably equivalent for the
  repair-path OUTCOME (ADR-0039 says so); the falsifiable NEW property this slice adds is
  therefore **node liveness during isolation** (container state stays `running`, never
  `paused`), not a new distributed-consistency violation. The D-server is NOT clock-inert —
  it runs self-clocked timers under a live partition that a freeze suspends: a
  registration-lease **renewal loop** (`--lease-ttl-secs` / `--renew-secs`, the
  `serve(coord, lease, renew_interval, …)` call at `crates/server/src/cli.rs:309`) and a
  server-side **request timeout** (`AdmissionControl.request_timeout`, `cli.rs:276`) — so a
  live partition is a genuinely (if
  marginally) stronger nemesis today. BUT the headline "stale action from a live isolated node
  on heal" teeth do NOT bite yet: the D-server's coordination backend is **process-local
  `MemCoordination`** (`cli.rs:233-237` — a separate-process registry "awaits an etcd (or
  static-endpoint) backing behind the same trait (ADR-0006)"), so the lease renews against the
  node's own memory and a network partition produces no cross-node lease-expiry / stale
  re-registration observable to the repair path. That teeth turns on only when coordination
  becomes a **networked backend (etcd / static endpoints, ADR-0006)** — not merely at M7, and
  not with today's in-process coordination (out of scope — see below).
- **Invariant to restore:** The Tier-1 isolation nemesis must isolate a **live** node —
  network-level unreachability while the node keeps running on its own clock (Jepsen's
  `:partition`), not a process freeze that suspends the node (Jepsen's `:pause`) — while the
  ADR-0015 repair-path contract (read-after-commit, no torn/stale reads, commit-point-atomic
  repair that converges exactly once across the heal) continues to hold. Source: ADR-0039
  (`docs/design/adr/0039-tier1-consistency-in-repo-scenario.md`, Accepted — "A stronger
  network-level partition that keeps the isolated node *live* is an additive upgrade to this
  leg (#399), not a change to the contract asserted") and the Jepsen nemesis taxonomy
  (`:pause` ≠ `:partition`). This is additive to an Accepted ADR, so it needs NO new/
  superseding ADR (the upgrade is already sanctioned in 0039's text; §2 immutability is not
  engaged).
- **Repo + branch target:** getwyrd/wyrd @ main   (single-slice test-infra enhancement; milestone-9 "Foundations", independent of the M4 metadata backend — INTEGRATION §2)
- **Depends on:** none
- **Conflicts with:** none
- **Ordering note:** Standalone; not part of a batch. Deliberately does NOT depend on / stack
  on the M4 integration branch. The nearest peer — the #257 metadata Tier-1 leg's netns
  `iptables`-agent partition (`xtask/src/metadata_faults.rs`, `deploy/tikv-multi-replica/
  iptables-agent/`) — landed via PR #453 on `feat/m4-production-metadata-backend`, which is
  **not on `main`** and only merges to `main` when M4 completes. Do targets `main` and MUST
  NOT assume that infra (the `wyrd-iptables:local` image, the netns map, the multi-replica
  bridge) exists; the D-server cluster here is a different, simpler topology (see below).
- **Surfaces:** data (backend / test + xtask infrastructure only; no GUI).
- **Difficulty:** medium — additive and confined to the test/xtask layer (no production
  reconcile-path or public-API change), but it spans `xtask/src/faults.rs`, the scenario
  test `tier1_jepsen_consistency.rs`, and the existing `.github/workflows/tier1-jepsen.yml`
  job, and mirrors an established sibling. Blast radius is the Tier-1 test infra, not
  Wyrd's production code.
- **Scope:** Add a **network-level partition nemesis** to the Tier-1 Jepsen leg that keeps
  the isolated node LIVE (running on its own clock, network-unreachable) for the duration of
  the fault window, then heals it — asserting the same ADR-0015 properties across the
  live-partition-and-heal that the leg already asserts, PLUS the node-liveness property that
  distinguishes it from the freeze nemesis. Keep the existing `docker pause` freeze leg as a
  separate, cheaper nemesis (do not delete it). Wire the new leg into the existing privileged
  `WYRD_TIER1=1` `tier1-jepsen` CI job. Leave the CHOICE of partition mechanism (e.g.
  `docker network disconnect`/reconnect, an in-container `iptables`/`tc` packet drop, or
  another that keeps the container in the `running` state) to Do — the invariant is a *live*
  network partition, not a named tool.
  / out of scope: changing the ADR-0015 contract or any production reconcile/reconstruction
  code; the literal public Jepsen/Elle credibility artifact (#329, separate); the cross-node
  stale-action-on-heal teeth, which are GATED on coordination becoming a networked backend
  (etcd / static endpoints, ADR-0006) — today the D-server renews its registration lease
  against process-local `MemCoordination` (`crates/server/src/cli.rs:233-237`), so a partition
  yields no cross-node lease-expiry; this slice does NOT build that networked-coordination
  world (the M7 / proposal 0014 failover-DR consumer, and ADR-0006 backing generally, future);
  editing ADR-0039 or proposal 0005 (frozen — the upgrade is already sanctioned in 0039).
- **Repro instruction:** On `main`, inspect the Tier-1 leg: `xtask/src/faults.rs`
  `run_jepsen_scenario` names `partition_container = "{JEPSEN_PROJECT}-dserver-2"`
  (`faults.rs:266`) and the scenario at
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` isolates it with
  `docker pause` (Phase 2, `tier1_jepsen_consistency.rs:831-842`) and heals with
  `docker unpause` (Phase 3, `:901-911`). `docker pause` suspends the container via the
  freezer cgroup → `docker inspect -f '{{.State.Status}}'` reports `paused`, i.e. the node's
  clock is frozen. There is currently NO leg in which the isolated container stays `running`
  while network-unreachable. The gap: the leg's "network partition" comment describes a
  nemesis it does not actually inject.
- **External dependencies:** For the **Check-gated** born-at-tier portion (the pure decision
  + oracle unit tests + the scenario compile/type-check): `none` beyond the base Rust
  toolchain — it builds and runs unprivileged and container-free. For the **deferred
  off-Check** live run: Docker (compose plugin) AND the capability to inject a live
  network-level partition on the runner (e.g. `docker network disconnect`, or `NET_ADMIN`
  for an in-container `iptables`/`tc` cut) against the 10-container RS(6,3) D-server cluster
  (`JEPSEN_DSERVER_COUNT = 10`). These are provided by the existing privileged
  `tier1-jepsen` GitHub job (`WYRD_TIER1=1`), NOT by Check. Do MUST declare any further
  dependency it discovers (rather than silently working around it) — in particular it MUST
  NOT reach for the M4-only `wyrd-iptables:local` fault-agent image, which is not on `main`.
- **Test file:** `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` (ships the
  new born-at-tier node-liveness oracle + its negative-control unit test, and the
  `#[ignore]`d scenario that injects the live partition). The pure nemesis-decision unit
  test ships in `xtask/src/faults.rs`'s `#[cfg(test)] mod tests` (mirroring
  `jepsen_dispatch_routes_to_in_repo_scenario_not_external_command`). Both must fail when the
  partition leg is negated to a freeze/pause and pass with the fix.
- **Verification posture:** Split, and BOTH parts must be honoured. (a) NET-NEW born-at-tier
  coverage at Check where "red" is criterion-ABSENCE: the pure partition-nemesis
  decision/value and the node-liveness oracle, unit-tested (non-`#[ignore]`) inside
  `cargo xtask ci`, with a negative control demonstrated RED via a temporary negation (route
  the partition leg to `docker pause`) — per the forcing function, Do captures a
  *demonstrated* red, not a red resting on non-existence. What IS built AND exercised at
  Check: the pure decision, the oracle, and the compile/type-check of the scenario body.
  (b) DEFERRED off-Check: the live network-partition-and-heal end-to-end run is observable
  only on the privileged `WYRD_TIER1=1` `tier1-jepsen` job (a Docker host with partition
  capability) — it is INERT at Check. Who confirms the deferred green: the maintainer via
  that CI job's run. The deferred deliverable is itself BUILT (the scenario is real
  API-bound Rust, compiled at Check) and exercised by the born-at-tier unit tests + compile
  check — never merely inert dispatch scaffolding.
- **Production reach:** N/A in the usual sense — this is verification infrastructure, not a
  production seam. Flagged only to be explicit: over today's dumb D-servers the live-partition
  nemesis and the pause nemesis drive the SAME repair-path outcome (ADR-0039); the new leg's
  load-bearing distinguishing signal is node-liveness (the container stays `running`), which
  the live job asserts directly. The cross-node stronger-nemesis value (lease-expiry / stale
  re-registration on heal) is gated on networked coordination (ADR-0006) — today's lease
  renews in-process (`crates/server/src/cli.rs:233-237`) — so it is future work (out of scope),
  not something this slice can exhibit.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer callsites
  Do MAY open to mirror the established composition (all on `main`):
  - `xtask/src/faults.rs:179` — the pure `jepsen_dispatch` decision + its non-`#[ignore]`
    unit test (`faults.rs:599`, `fn jepsen_dispatch_routes_to_in_repo_scenario_not_external_command`):
    the born-at-tier "flippable value" pattern the new nemesis
    decision must mirror (a value with both alternatives representable, bound by a Check-time
    unit test — NOT a hardcoded match arm).
  - `xtask/src/faults.rs:297` — `run_jepsen_test`: how the leg exports fault targets to the
    scenario via env vars (`WYRD_TIER1_VICTIM_CONTAINER`, `WYRD_TIER1_PARTITION_CONTAINER`);
    the new leg's partition wiring mirrors this env-var plumbing.
  - `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:831-842` (pause) and `:901-911`
    (unpause) — the current freeze injection points the new live-partition leg parallels; and
    the oracle-helper + negative-control unit-test shape at `:207-543` (`assert_*` helpers +
    their non-`#[ignore]` unit tests) the new node-liveness oracle must follow.
  Do MUST NOT open or assume the M4-only `xtask/src/metadata_faults.rs` /
  `deploy/tikv-multi-replica/iptables-agent/` peer — it is not on `main`.
- **Prior-art check (triage cycles):** Searched by file path across merged history and open/
  closed work. `xtask/src/faults.rs` `run_jepsen` + `tier1_jepsen_consistency.rs`: the
  pause-based leg was shipped by #250 (merged, on `main`) — this issue is its sanctioned
  additive upgrade (ADR-0039 names #399 explicitly). The real-network-partition PATTERN
  exists in the metadata Tier-1 leg (#257, PR #453) but on `feat/m4-production-metadata-backend`,
  not `main`, and for a different (TiKV) tier. No prior or closed/rejected attempt at #399
  itself. No superseding-ADR concern (additive to Accepted ADR-0039).
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Live `WYRD_TIER1=1 cargo xtask jepsen` run (human, at sign-off): ProcessFreeze leg passes end-to-end; NetworkPartition leg FAILS at Phase 3 heal (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1547`) — `Store(Unavailable("tcp connect error" … 127.0.0.1:<port> … Connection refused))` dialing the reconnected isolated node's host-published port. Root cause is the chosen partition mechanism, not a Wyrd consistency violation: `docker network disconnect` tears down the container's published-port proxy and `docker network connect` does not restore it on heal (reconnect commonly reassigns the IP and does not re-establish the original host-port forwarding), so the isolated node is unreachable at the endpoint the test holds after "heal." The leg breaks its own reachability before the ADR-0015-across-heal assertions can run — i.e. the live partition-and-heal deliverable the issue exists to add is not actually demonstrated. What to change next (keep scope as briefed): - Replace `docker network disconnect`/`connect` with a partition mechanism that keeps the container's network identity and host-published port mapping intact across the fault window — e.g. an in-container `iptables`/`tc` packet drop (a brief-named alternative), OR re-resolve + re-dial the endpoint on heal so Phase 3 uses the restored route rather than the torn-down mapping. - Verify the fix on the privileged live runner: the NetworkPartition leg must reach Phase 3+ green (node stays `running` during isolation AND every ADR-0015 property holds across the heal), captured as bundle evidence so Check T3/T5/Validation can be cleared next pass. - Preserve what is sound and green: the pure `IsolationNemesis` decision + its unit test, the `assert_node_live_during_isolation` oracle + negative controls, and the ProcessFreeze leg (unaffected — do not disturb).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
