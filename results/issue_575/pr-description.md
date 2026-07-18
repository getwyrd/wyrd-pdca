# Emit S3 request RED and d-server admission/capacity metrics

## Summary
**User impact:** an operator running a live deployment can see whether the *storage* is
healthy, but not whether the *service* is: nothing reports failing or slow S3 requests,
and a storage server refusing work under load does so silently — the only evidence is an
error on the client's side, if anyone kept it. During an overload or a flaky-fleet
incident, the first questions ("are requests failing? transiently or permanently? are we
shedding load?") cannot be answered from the server at all.

This PR makes both planes observable: the S3 front door reports per-operation latency and
errors (keyed by operation and by the typed failure class), and the d-server reports every
admission decision — admitted, shed, timed out, cancelled — plus an in-flight request
gauge, all through the existing shared telemetry seam. Purely additive instrumentation:
nothing about what is admitted, shed, or served changes.

**Merge order:** depends on #577 — the error counter's `class` label is the typed
`ErrorClass` that PR introduces, consumed from its `classify` seam rather than re-derived
here. This PR should land after it.

## What to look at
- The S3 request plane: `crates/gateway-s3/src/lib.rs` — where the RED sample is raised
  (at transfer *end*, not head time) and how a mid-stream failure is classified.
- The d-server capacity plane: `crates/server/src/dserver.rs` — two observer layers
  positioned around the existing admission stack, one outside the load-shed (to see the
  rejection), one inside the concurrency limit (so "admitted" means admitted).
- The role wiring: `crates/server/src/cli.rs` — the `s3` and `d-server` entries gain the
  metrics provider that until now only the custodian role built; without it the
  instrumentation would emit into a subscriber with no exporter and report nothing.

To try it: run `wyrd s3 …` or `wyrd d-server …` (optionally with
`--otlp-endpoint <collector>`), drive a PUT/GET, a failing GET, an overload past
`--max-concurrent-requests`, and a request past `--request-timeout-secs`, and watch the
`s3_request_*` and `capacity_requests_*` series.

## Root cause
The roles had no metrics provider outside the custodian (`DurabilityTelemetry::new`'s
sole caller is `cmd_custodian`, `crates/server/src/cli.rs:836` on `main`), and neither the
gateway nor the d-server emitted any request/capacity metric (`grep -rn
"monotonic_counter\|histogram\." crates/gateway-s3 crates/server/src/dserver.rs` on
`main` → no hits). A load-shed was decided and discarded inside the tower layer stack
(`crates/server/src/dserver.rs:287-323` on `main`) with no server-side record.

One subtlety this PR gets right that a naive version would not: a streaming GET's failure
class cannot be captured when the response head is built. The head goes out `200` before a
single byte is read, so it carries no error and can only take the fail-safe `terminal`
default — but the fault that ends the transfer arrives later, inside the body stream. A
head-time-only classification counts a transient mid-stream fault (a d-server dying
mid-read) as `terminal`, inverting the transient-vs-terminal distinction on exactly the
long transfers a real fleet fails on.

## Fix
- **Request plane** (`crates/gateway-s3/src/lib.rs`): a `s3_request_duration_ms` histogram
  and a `s3_request_errors` counter keyed by `op` and `class`, emitted as `tracing` metric
  events through the shared `MetricsLayer` bridge (no telemetry backend named in the
  crate). The sample rides the existing end-of-transfer access-row point, so latency is
  the real transfer duration and a `200` head whose body then failed or truncated still
  counts as an error. The body wrapper's `Err` arm classifies the error that actually
  ended the transfer via `wyrd_traits::classify` (`crates/traits/src/lib.rs:535` on the
  #577 base); error-decided-at-head paths keep the head-time verdict stamped by
  `gateway_error_response`.
- **Capacity plane** (`crates/server/src/dserver.rs`): a `ShedObserver` outside the
  load-shed layer counts server-wide sheds (forwarding tower's `Overloaded` unchanged);
  an `AdmissionObserver` inside the concurrency limit raises admitted/timed-out/cancelled
  events and the in-flight gauge, releasing the slot on drop so a cut request cannot leak
  the level. Every `Server::builder()` admission option is untouched.
- **Role wiring** (`crates/server/src/cli.rs`, `crates/telemetry/src/lib.rs`): `wyrd s3`
  and `wyrd d-server` build a `DurabilityTelemetry` exactly as the custodian does
  (Prometheus always; `--otlp-endpoint` adds OTLP push) and hand the servers a
  metrics-only dispatch — carried rather than scoped, because tonic/axum serve each
  connection on a spawned task that does not inherit a scoped subscriber.
- All error/capacity series are pre-registered at zero, so "no errors" and "no metric"
  are distinguishable on a dashboard.

## Verification
- **Claim:** each S3 op records a latency sample under its own `op` label, and a failing
  op is counted by op and typed class.
  **Test:** `crates/server/tests/request_capacity_planes.rs` (new) — tests 1–2 drive a
  real PUT/GET/failing-GET through the production router over a loopback socket and read
  back via the in-process Prometheus surface (`DurabilityTelemetry::gather_prometheus`,
  `crates/telemetry/src/lib.rs:145` on `main`).
- **Claim:** a transient fault raised mid-body, after the `200` head, is counted
  `class="transient"`, not the head-time `terminal` default.
  **Test:** test 3 reads the `200` and real object bytes off the wire *before* injecting
  the fault, then asserts `transient` ≥ 1 and `terminal` == 0 against pre-registered
  series. With only the mid-stream classification reverted, this test fails on exactly
  that assertion (the sample lands on `terminal`); with it, all 6 tests pass.
- **Claim:** admission is observable — admitted events, a shed event on a forced
  overload, a timed-out event past the deadline, and an in-flight gauge that returns to
  zero.
  **Test:** tests 4–6 drive a real d-server (`DServer::serve`, the production layer
  stack) past its tunable bounds; the shed is forced across two connections against a
  server-wide bound of 1, so it exercises the binding limit, not the per-connection cap.
- **Claim:** admission behaviour is unchanged (emission only).
  **Checked:** `crates/server/src/dserver.rs` — the pre-existing
  `LoadShedLayer`/`GlobalConcurrencyLimitLayer`/timeout options (at
  `crates/server/src/dserver.rs:287-323` on `main`) are untouched; the observers only
  wrap them and forward every outcome unaltered. The happy-path test additionally asserts
  the shed counter stays 0.
- Pre-fix absence confirmed on `main`: no request/capacity metric family exists anywhere
  (grep above), and the `s3`/`d-server` roles have no metrics provider — so every
  read-back assertion in the new test file fails on the base.
- Full gate green: `cargo xtask ci` (fmt, clippy `-D warnings`, build, tests, deny,
  conformance); the new test file run 10× with no flake (the fault is gated on a channel,
  not a timing guess); the existing `wyrd-gateway-s3` suite (37 tests) unaffected.

Known follow-ups tracked separately: per-connection sheds remain unobserved (tonic
applies that cap outside the instrumentable stack) — getwyrd/wyrd#584; a live `/metrics`
scrape endpoint — getwyrd/wyrd#585.

Fixes #575
