# PR description

## Summary

The d-server's gRPC transport ran with all defaults, so it could not fail
closed under load: when it was offered more concurrent requests than it could
serve, the excess queued without bound and fought for runtime threads instead
of being refused, and a handler that hung pinned its slot indefinitely — under
sustained overload the server degraded into thread-pool exhaustion rather than
shedding load. This change configures a server-wide admission bound with load
shedding and a per-request timeout, so excess load is refused immediately with
a retryable "busy" status and a stuck handler is cut loose.

## What to look at

- `crates/server/src/dserver.rs` — `DServer::serve` now builds the tonic
  `Server` with a load-shed layer over a **server-wide** concurrency limit (one
  shared semaphore, applied via `Server::layer` so it is cloned — not
  re-created — per connection), plus a request timeout and HTTP/2 / TCP tuning.
  The new `AdmissionControl` struct holds the knobs; `with_admission_control`
  sets them.
- `crates/server/src/cli.rs` — `--max-concurrent-requests` and
  `--request-timeout-secs` expose the limit to operators.
- `crates/server/tests/dserver.rs` — two tests exercise the behaviour. To
  reproduce by hand: set `max_concurrent_requests: 1`, hold the one slot on one
  connection, and issue a request on a *second* connection — it comes back with
  a retryable status instead of queuing.
- `crates/dst/tests/custodian.rs` — a separate, test-only determinism fix (see
  Fix, point 3); no production code there changes.

## Root cause

`DServer::serve` constructed the transport as a bare
`Server::builder().add_service(...)` (`crates/server/src/dserver.rs:160` on
`main`), setting no concurrency limit, no timeout, and no stream/TCP tuning, so
there was no point at which an over-capacity request could be refused. The
intended behaviour is the opposite — the system is meant to shed or slow load
predictably and never trade correctness for admission
(`docs/design/architecture/08-crosscutting-concepts.md:98-107`).

## Fix

1. **Server-wide admission bound.** A `GlobalConcurrencyLimitLayer` holding one
   shared `Arc<Semaphore>` is applied via `Server::layer`, capping concurrent
   in-flight requests across *all* connections; an outer `LoadShedLayer` turns
   the limit's backpressure into an immediate refusal that tonic maps to a
   retryable `RESOURCE_EXHAUSTED` status. A per-connection limit alone would not
   suffice — aggregate in-flight would still grow without bound in the number of
   connections — so the shared, server-wide semaphore is the binding bound.
2. **Bounded per-request work and transport.** A request `timeout` cuts a hung
   handler loose with a deadline status; `max_concurrent_streams`, `tcp_nodelay`
   and an HTTP/2 keepalive interval bound and tune the connection. All limits
   live in `AdmissionControl` with documented HDD/SSD-oriented defaults and are
   operator-tunable via CLI flags, not hardcoded.
3. **Test determinism (test-only).** The durability-metrics simulation test read
   `tracing` events back through a *scoped* subscriber, but `tracing` caches a
   process-global interest per callsite the first time it is hit. Under parallel
   test execution a non-capturing test could register the shared callsite first
   and cache it "never", leaving the capture empty and flaking the assertion. A
   permissive process-global default is now installed once up front, so the
   cache can never be poisoned to "never"; the scoped capture still routes the
   values it asserts on. This touches only `crates/dst/tests/custodian.rs`.

## Verification

- **Claim:** Offered more concurrent requests than its configured limit, the
  d-server sheds the excess with a retryable status instead of admitting it
  unboundedly.
  - **Checked:** `crates/server/src/dserver.rs` `DServer::serve` — the load-shed
    layer wraps a server-wide shared-semaphore concurrency limit; this is the
    location that on `main` was the bare `Server::builder()` at
    `crates/server/src/dserver.rs:160`.
  - **Test:** `crates/server/tests/dserver.rs`
    `overload_across_connections_sheds_excess_with_a_retryable_status` — with a
    server-wide limit of 1, one slot held on connection A, a request on a second
    connection B returns `ResourceExhausted`/`Unavailable`. Fails pre-fix (no
    limit ⇒ B queues and the bounded wait elapses), passes post-fix.

- **Claim:** A handler that hangs past the configured timeout is cut with a
  deadline status rather than pinning its slot forever.
  - **Checked:** `crates/server/src/dserver.rs` `DServer::serve` — the
    `.timeout(admission.request_timeout)` layer on the builder.
  - **Test:** `crates/server/tests/dserver.rs`
    `hung_handler_is_cut_by_the_request_timeout` — a never-returning handler is
    cut by a 200 ms timeout with `Cancelled`/`DeadlineExceeded`. Fails pre-fix
    (no timeout), passes post-fix.

- **Claim:** The limit is configuration, not a fixed constant, tunable to the
  device's useful queue depth.
  - **Checked:** `crates/server/src/dserver.rs` `AdmissionControl` (struct +
    `Default`) and `crates/server/src/cli.rs` (`--max-concurrent-requests`,
    `--request-timeout-secs`), which on `main` had no such flags.

- **Claim:** The intended invariant is fail-closed-under-pressure — shed or slow
  load predictably, never trade correctness for admission.
  - **Checked:** `docs/design/architecture/08-crosscutting-concepts.md:98-107`
    (§8.9 Admission control and backpressure).

- **Claim:** The durability-metrics test no longer flakes under parallel runs.
  - **Checked:** `crates/dst/tests/custodian.rs` `install_metric_dispatch`,
    called first in each simulation test; the emitting callsites are
    `crates/custodian/src/reconstruction.rs:418,425,435-436`, read back through
    the scoped capture at `crates/dst/tests/custodian.rs:952,975`.
  - **Test:** the same `custodian` simulation tests, run repeatedly under
    parallel threads with the 50-seed sweep — flaked before, green after.

Fixes #205
