# Serve grpc.health.v1 liveness/readiness probes from the d-server

## Summary
**User impact:** an operator's deployment supervisor — systemd, Kubernetes, a load
balancer — cannot tell whether a wyrd storage node is alive and ready to serve. The
node checks its own storage health internally, but nothing exposes that to the
outside, so the only signal a supervisor gets is "the process exists": an unhealthy
node keeps receiving traffic and failing it, and the first evidence is errors on the
client's side, if anyone kept them.

This PR makes the d-server answer the standard gRPC health-checking protocol
(`grpc.health.v1`) on a stable, operator-configurable address, with readiness
reflecting the storage backend's own health check — so any off-the-shelf supervisor
that speaks the standard protocol can detect and route around an unhealthy node,
without wyrd-specific tooling.

**Merge order:** opened against `pdca-integration/main`, the integration branch
already carrying #575 — both rework the d-server's serve composition, and this
diff's context depends on that change, so it does not apply on `main` directly.
Lands on `main` when the integration branch folds.

## What to look at
- The probe surface: `crates/server/src/dserver.rs` — `serve` now optionally binds a
  **second, unlayered** listener for the health service, keeps its readiness status
  fresh from the store's `health()`, and shuts both listeners down on the same
  signal. The deliberate policy decisions: errors read fail-closed (NOT_SERVING),
  and overload is *not* unreadiness — the probe must answer precisely when the data
  plane is shedding.
- The operator knob: `crates/server/src/cli.rs` — the `--health-bind ADDR` flag,
  defaulting to a stable, documented `127.0.0.1:50052` beside the data plane's
  `127.0.0.1:50051`, so the probe endpoint is a known address, never an OS-assigned
  ephemeral port a supervisor cannot discover.
- To try it: run `wyrd d-server`, then
  `grpcurl -plaintext 127.0.0.1:50052 grpc.health.v1.Health/Check` (or
  `grpc_health_probe -addr 127.0.0.1:50052`). The startup log prints the probe
  address.

## Root cause
On the base branch no health service is registered — the tonic builder adds only the
chunk-store service (`crates/server/src/dserver.rs:682` on the base), and
`grep -rn "tonic_health" crates/ Cargo.toml` has no hits — so a
`grpc.health.v1.Health/Check` returns UNIMPLEMENTED. The store-side seam already
existed with no consumer: `ChunkStore::health` (`crates/traits/src/lib.rs:408`) is
implemented by the fs backend but feeds no probe, per proposal 0010's observability
floor (item 7: reuse the standard health proto, no bespoke service).

## Fix
- **New dependency `tonic-health`** (workspace `Cargo.toml`), version-matched to
  tonic 0.14. MIT-licensed, already covered by the `cargo deny` allowlist.
- **`crates/server/src/dserver.rs`:** `serve` optionally serves the `grpc.health.v1`
  service on a configured `health_bind` address through its **own, unlayered**
  `Server::builder()` (`crates/server/src/dserver.rs:866-877`) — outside the
  load-shed/concurrency-limit admission stack by construction, no per-service escape
  hatch. Readiness is keyed on the chunk-store service's registered name, set
  NOT_SERVING *before* serving (`crates/server/src/dserver.rs:740-743`) so an early
  probe reads fail-closed rather than NOT_FOUND, then refreshed on a bounded cadence
  (default 3s, `with_health_refresh_interval`) by re-reading the store's `health()`:
  `Healthy`/`Degraded` ⇒ SERVING, `Unhealthy` **or** `Err(_)` ⇒ NOT_SERVING
  (`crates/server/src/dserver.rs:759-760`). The store now sits behind an `Arc` so the
  refresher polls the *same* instance the data plane serves; the refresher is aborted
  when `serve` returns (`crates/server/src/dserver.rs:906`), and both listeners stop
  on the same shutdown signal.
- **`crates/server/src/cli.rs`:** the `--health-bind ADDR` flag
  (`crates/server/src/cli.rs:632-637`), defaulting to the stable
  `DEFAULT_HEALTH_BIND` (`127.0.0.1:50052`), carried through `DServerParams`
  (`crates/server/src/cli.rs:1382`) into `.with_health_bind(...)`
  (`crates/server/src/cli.rs:1416`); the startup log prints the probe address.
- **Library default is *no* probe** (`health_bind: None`): in-process callers that
  spin several servers (as the existing `crates/server/tests/dserver.rs` suite does)
  are not forced onto one fixed port, while the deployable `wyrd d-server` role
  always enables the probe on the stable default — the surface a real supervisor
  dials.

## Verification
- **Claim:** `Check` reports SERVING while the store's `health()` is `Healthy`.
  **Test:** `crates/server/tests/health_probe.rs:239` — drives the real
  `DServer::serve` composition over loopback TCP and queries with the real generated
  `grpc.health.v1` client; no mock health service exists anywhere in the test.
- **Claim:** `Check` flips to NOT_SERVING within a bounded wait once the store
  reports `Unhealthy`, recovers to SERVING, and flips again once `health()` returns
  `Err` — the fail-closed half.
  **Checked:** `crates/server/src/dserver.rs:759-760` — both `Ok(Unhealthy)` and
  `Err(_)` map to NOT_SERVING; the mapping lives only in production code.
  **Test:** `crates/server/tests/health_probe.rs:280` — asserts all four
  transitions.
- **Claim:** the probe still answers — with a real serving status, not
  RESOURCE_EXHAUSTED — while the data plane is saturated at its admission bound.
  **Checked:** `crates/server/src/dserver.rs:866-877` — the health service is served
  by its own builder, never added to the admission-layered one.
  **Test:** `crates/server/tests/health_probe.rs:354` — holds the single
  `max_concurrent_requests` slot with a real in-flight `get_fragment` (entry
  confirmed via a channel before the probe is dialed) and asserts the probe answers
  SERVING while the slot is held.
- **Claim:** the address a supervisor dials is the *configured* one, not an
  ephemeral read-back: each test reserves a concrete loopback address, hands it to
  `with_health_bind` — the same knob `--health-bind` feeds
  (`crates/server/src/cli.rs:632-637` → `:1416`) — asserts the server echoes it
  (`crates/server/tests/health_probe.rs:165`), and dials exactly that address.
- **Red→green:** with the production files reverted to the base and the test kept,
  the suite fails (no `tonic_health` dependency, no `with_health_bind`); with this
  change, all 3 tests pass. Full gate green: `cargo xtask ci` (fmt, clippy
  `-D warnings`, build, tests, `cargo deny` — `tonic-health` 0.14.6 is MIT,
  allowlisted — and conformance).

Fixes #576
