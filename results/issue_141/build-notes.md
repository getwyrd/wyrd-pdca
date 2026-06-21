# Build notes — issue 141 / m3.3-custodian-skeleton (iteration 2)

Target: `getwyrd/wyrd` @ `origin/main` (`ae6be66`, includes #139/#140). All edits in
`$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`). Built to proposal **0005** (read in
full) as the authoritative plan; the brief only points at it.

## What I built (the five Success-criterion legs, Option B)

1. **Single active custodian, fenced** (BINDING) — `crates/custodian/src/leadership.rs`.
   `Custodian::elect` campaigns via the existing `Coordination::elect_leader`
   (`traits/src/lib.rs:286-288`); the `FencedZone` tracks the current (highest)
   leadership term and `authorize` **rejects** any action stamped with an older
   fencing token — the monotonic-token rejection of a deposed leader's coordination
   action (0005 §"Single active custodian, fenced", `0005:362-367`). `reconcile_step`
   (`reconciliation.rs`) is the fenced control point exercising it. The decisive
   *second* guard — the version-conditional commit (`0005:200-203`) — belongs to the
   reconstruction slice (M3.6) and is explicitly out of scope here.

2. **Failure-domain-aware selector** (BINDING distinctness/refusal) —
   `crates/core/src/placement.rs`. `select_distinct_domains(topo, n)` returns `n`
   ids across `n` distinct opaque domains, or `SelectorError::InsufficientDomains`
   when domains < n. Kept thin (opaque domain id + distinctness invariant +
   per-domain utilization, `0005:251`); the selection *order* is ILLUSTRATIVE, only
   the distinct-domain guarantee is contractual (`0005:235`). It lives in `core` so
   it is the **same** selector the write fan-out uses (`0005:241-242`), re-exported
   from the custodian crate for later re-placement.

3. **Production write path wired (Option B)** (BINDING) — `crates/core/src/write.rs`
   + `crates/server/src/dserver.rs`. `WritePlan::place(topology)` runs the selector
   and overwrites each chunk's placement; `write_fragments` now fans each fragment to
   `put_fragment_at(placement[index], …)` (was `put_fragment`, domain-blind
   `index % n`); `write_new_object_placed` is the wired end-to-end entry point. The
   committed `ChunkRef.placement` therefore reflects the **distinct-domain** choice,
   not the identity vector, and the read resolves from it (`read.rs:80-86`, unchanged
   — already record-driven from #139). On the server, a D server's registration now
   carries `DServerRegistration { id, endpoint, failure_domain }` (`0005:194-196`);
   `discover_topology` composes the production `Topology` the selector places against;
   `--failure-domain`/`--id` flags surface it on the `d-server` role.

4. **Durability-plane OTel seam** (BINDING dual-export) —
   `crates/custodian/src/telemetry.rs`. `DurabilityTelemetry` wires an
   `SdkMeterProvider` over **both** a Prometheus-scrapeable registry **and** an OTLP
   push exporter (`opentelemetry-otlp`, real tonic build), selected by
   `ExporterConfig` with **no backend hardcoded** (ADR-0012, `0005:338-340`). The
   first custodian metric is emitted through `tracing` + `tracing-opentelemetry`
   (`MetricsLayer`, a `monotonic_counter.custodian_active` event) and read back
   in-process off the Prometheus surface — the ILLUSTRATIVE assertion; a live scrape /
   collector run is supplementary off-Check evidence (posture b).

5. **Dependency boundary** (BINDING) — `crates/custodian/Cargo.toml`. Deps are
   `traits` / `core` (+ the `tracing`/OpenTelemetry stack) only — **never** a concrete
   backend (ADR-0010, `0005:421-422`). `proto` is permitted but unused by the
   skeleton (the maintenance-RPC seam lands in the loop slices 4–7), so it is not
   declared — keeping `cargo-machete` green; the boundary set is a subset of the
   permitted `traits/core/proto+tracing`. `coordination-mem` (a concrete) appears only
   as a **dev-dependency** (a test/dev composition may name a concrete, ADR-0010).

## Red→green (proven through the project runner, `./engine/xtask.sh ci`)

- `crates/core/tests/domain_placement.rs` — the flippable leg. Reverting
  `WritePlan::place` to the pre-Option-B identity (`(0..n)`) makes assertion (1)
  fail: `left: [0..8] == right: [0..8]` (verified). Restored → green.
- `crates/custodian/tests/skeleton.rs` — NET-NEW infra, demonstrated via negation:
  flipping `FencedZone::authorize`'s `<` to `>` admits the deposed leader →
  `elected_leader_is_fenced_and_deposed_leader_rejected` FAILS (verified). Restored →
  green. Selector distinctness/refusal and the telemetry emission are likewise
  flippable (negate distinctness / drop the metric emit).
- `crates/server/tests/failure_domain_registration.rs` — supporting: the registration
  label round-trips and `discover_topology` feeds a distinct-domain placement /
  refusal.

Full gate: `xtask ci: all checks passed` — fmt (`--check`), clippy `-D warnings`,
build (incl. `--cfg madsim` DST), test, `cargo deny check`, conformance,
`cargo-machete`. `cargo fmt --all` applied; commit-ready.

## Alternatives considered (with cost)

- **Bare in-memory telemetry stub** instead of the real OTel stack — rejected: the
  carry-forward T5 explicitly requires "dual Prometheus+OTLP, not a bare in-memory
  stub". The real `opentelemetry-otlp` resolved on **tonic 0.14.6** — the workspace's
  existing tonic line — so it added **zero** version-skew (no second tonic), and
  `cargo deny` stays green (all new licenses Apache-2.0/MIT, already allow-listed; 0
  `deny.toml` lines changed).
- **Threading a topology through `write_new_object` itself** (one entry point) —
  rejected: it would force every M0–M2 caller (single-store `Gateway`, the cluster
  client) to supply a ≥-n-domain topology or the selector refuses; a single-store
  gateway has **1** domain, so `rs(6,3)` writes would start erroring. A separate
  `write_new_object_placed` keeps the identity-default path intact (M0–M2 green) while
  wiring the placed path — `+34` lines vs. a churn that would touch `Gateway::put_object`,
  `cluster_store_put`, and ~6 server tests.
- **Changing `write_fragments`' bound to `PlacementChunkStore`** (vs. a parallel
  function) — chosen: it is the smallest change that lets the fan-out honour a chosen
  id, and the default `put_fragment_at` delegates to `put_fragment`, so identity
  placement preserves M2 routing exactly. Cost: one added `impl PlacementChunkStore`
  (defaults) on the `FaultStore` test fake (`write_fanout.rs`, +5 lines).

## NEEDS-HUMAN at sign-off (flagged by the brief, posture c)

- **New dependencies** (`tracing`, `tracing-subscriber`, `tracing-opentelemetry`,
  `opentelemetry{,_sdk,-prometheus,-otlp}`, `prometheus`): the `cargo deny` gate is
  green, but ADR-0003's **three-test audit + `deny.toml` allow-list review** is a
  human decision regardless (INTEGRATION §4/§10). `deny.toml` needed **no** change.
- **Live dual-export evidence**: the in-process Prometheus read-back is asserted; a
  live Prometheus scrape / OTLP collector run is supplementary off-Check (human/CI).

## Out of scope (per 0005, not built)

The four loops' *behaviour* (GC/scrub/reconstruction/rebalance, slices 4–7), the
commit-point-atomic relocation CAS (slice 6), the DST property campaign +
Tier-1/Tier-2 fault injection (slice 8), dashboards/CLI (ADR-0013). The reconciliation
loop here is a fenced **scaffold** only.
