# Place chunk fragments across distinct failure domains (M3 custodian)

> One logical change: stand up the custodian plane and make chunk placement
> failure-domain-aware end to end (proposal 0005, Milestone 3 — custodians, slice 3
> plus slice-1's undelivered "retire `index % n`" + registration label).

## Root cause
Chunk placement was still M2's stateless identity route `index % n`
(`crates/core/src/write.rs:73`), which is blind to failure domains, so a chunk's `n`
fragments were not guaranteed to occupy `n` distinct racks/power/switches — a single
failure-domain loss could take two fragments and RS(6,3)'s durability math was not
claimable within a zone. There was no `custodian` crate, no failure-domain-aware
selector, no failure-domain label on D-server registration, and no durability-telemetry
seam — the prerequisites for failure-domain-aware placement and single-active repair.

## Fix
- **Single active, fenced custodian** — `crates/custodian/src/leadership.rs`: campaigns
  through the existing `Coordination::elect_leader` (`crates/traits/src/lib.rs:288`) and
  rejects a deposed leader's action via the monotonic fencing token
  (`crates/traits/src/lib.rs:326-328`).
- **Failure-domain selector** — `crates/core/src/placement.rs`:
  `select_distinct_domains(topo, n)` returns `n` ids across `n` distinct opaque domains
  or `SelectorError::InsufficientDomains` when domains `< n`. Kept thin (opaque domain
  id + distinctness invariant + per-domain utilization); the selection order is
  illustrative, only the distinct-domain guarantee is contractual.
- **Write path rewired (production)** — `crates/core/src/write.rs`: `WritePlan::place`
  runs the selector and overwrites each chunk's placement; `write_fragments` fans each
  fragment to `put_fragment_at(placement[index], …)` (was the domain-blind
  `put_fragment`), so the committed `ChunkRef.placement` records the distinct-domain
  choice, not the identity vector, and the record-driven read resolves from it.
- **Registration label** — `crates/server/src/dserver.rs`: a D server's registration
  carries an opaque `failure_domain`, surfaced via `--failure-domain`/`--id`, from which
  `discover_topology` composes the `Topology` the selector places against.
- **Durability telemetry seam** — `crates/custodian/src/telemetry.rs`: a backend-agnostic
  OpenTelemetry seam emitting the first custodian metric over **both** a Prometheus
  registry and OTLP push, no backend hardcoded. The `custodian` crate depends only on
  `traits`/`core` (+ the tracing/OpenTelemetry stack), never a concrete backend.

Builds on #139 (placement record + stable `DServerId`), which left the write at identity
`index % n` and added no registration failure-domain label; both are retired here.

## Verified against
- `crates/core/src/write.rs:73` (target `main`) — the pre-change identity placement
  `(0..c.fragments.len()).collect()` this change retires at the write commit.
- `crates/traits/src/lib.rs:53` (target `main`) — `DServerId` doc already names `#141`
  as the failure-domain selector work this delivers; the id is opaque (compared, not
  interpreted), matching `FailureDomain`'s opaque label.
- `crates/traits/src/lib.rs:288,326-328` (target `main`) — `Coordination::elect_leader`
  and the `Leadership` fencing token the single-active fence is built on (pre-existing
  seam, unchanged).
- `cargo xtask ci` on `$PDCA_WORKTREE` off `origin/main` (`ae6be66`, includes #139/#140):
  fmt `--check`, clippy `-D warnings`, build (incl. `--cfg madsim` DST), test,
  `cargo deny check`, conformance, `cargo-machete` — all green.

## Test
- `crates/core/tests/domain_placement.rs` (new) — the flippable red→green: a write into
  a topology where `index % n` would collide domains records a **distinct-domain**
  placement (not the identity vector) and the read reconstructs from that record.
  Reverting `WritePlan::place` to the identity `(0..n)` turns it red
  (`left: [0..8] == right: [0..8]`).
- `crates/custodian/tests/skeleton.rs` (new) — an elected leader is fenced and a deposed
  leader is rejected (negation-red: flipping `FencedZone::authorize`'s comparison admits
  the deposed leader); the selector yields `n`-distinct-domain placement and refuses when
  domains `< n`; the exporter emits a first metric.
- `crates/server/tests/failure_domain_registration.rs` (new, supporting) — the
  registration label round-trips and `discover_topology` feeds a distinct-domain
  placement / refusal.

**NEEDS-HUMAN (carried to sign-off, per the brief):** the new `tracing` /
`tracing-opentelemetry` / `opentelemetry{,_sdk,-prometheus,-otlp}` / `prometheus`
dependencies require the ADR-0003 three-test audit + `deny.toml` allowlist review
regardless of a green `cargo deny` gate (`deny.toml` needed no change); live dual-export
(Prometheus scrape / OTLP collector) is supplementary off-Check evidence.

Fixes #141
