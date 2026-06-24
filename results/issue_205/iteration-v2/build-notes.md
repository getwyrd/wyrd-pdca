# Build notes — issue 205 / d-server gRPC admission control (iteration 2)

Target: `getwyrd/wyrd @ main` (worktree base `d60ef6c`). All citations are `path:line`
on that base as edited in `$PDCA_WORKTREE`.

## What the carry-forward demanded (and why v1 was rejected)

v1 used tonic's `.concurrency_limit_per_connection(...)`. Verified against
`tonic-0.14.6/src/transport/server/mod.rs:1234-1237` (`MakeSvc::call`): that knob
builds a **fresh** `ConcurrencyLimitLayer::new(limit)` per connection, i.e. a new
`Semaphore` per connection. Aggregate in-flight = `connections × limit` is therefore
unbounded in the number of connections, so a many-connection overload still reaches
thread-pool exhaustion. The §8.9 "fail closed under pressure" invariant is server-wide,
so a per-connection cap does **not** restore it. Sign-off rejected on §6.1 (C5).

## The fix: a server-wide shared-semaphore bound applied via `Server::layer`

`crates/server/src/dserver.rs:159-191` (`DServer::serve`):

```
Server::builder()
    .layer(LoadShedLayer::new())                                  // outermost
    .layer(GlobalConcurrencyLimitLayer::new(admission.max_concurrent_requests))
    .concurrency_limit_per_connection(admission.max_concurrent_requests_per_connection)
    .load_shed(true)
    .timeout(admission.request_timeout)
    .max_concurrent_streams(Some(admission.max_concurrent_streams))
    .tcp_nodelay(admission.tcp_nodelay)
    .http2_keepalive_interval(admission.http2_keepalive_interval)
    .add_service(...)
```

Why this is server-wide (the load-bearing point, verified against source):

- `tonic-0.14.6 mod.rs:784` applies the `.layer()` stack **once**
  (`self.service_builder.service(svc)`), producing a single layered service that
  `MakeSvc` then **clones per connection** (`mod.rs:1228 self.inner.clone()`).
- `tower-0.5.3 src/limit/concurrency/layer.rs:54-58`: `GlobalConcurrencyLimitLayer`
  holds an `Arc<Semaphore>`; its doc says "Cloning this layer will not create a new
  semaphore." `service.rs:92-101` (`Clone`) shares the same `Arc<Semaphore>`. So all
  per-connection clones contend for **one** semaphore → a true server-wide bound,
  independent of connection count. This is the exact property v1 lacked.
- Outer `LoadShedLayer` (verified `tower-0.5.3 src/load_shed/mod.rs:43-64`) turns the
  limit's backpressure (`poll_ready` → `Pending` when the semaphore is exhausted) into
  an immediate **shed**: `call` returns `Overloaded`.
- `Overloaded` → retryable gRPC status: verified `tonic-0.14.6 src/status.rs:365-368`
  (`Status::try_from_error` downcasts `tower::load_shed::error::Overloaded` →
  `Status::resource_exhausted`) and `src/service/recover_error.rs:90-99` (the
  per-connection `RecoverError` maps the propagated error into the wire status).

Layer order: confirmed `tower-0.5.3 src/builder/mod.rs:132` + `tower-layer-0.3.3
src/stack.rs` — the **first** `.layer()` on the tonic builder is the **outermost**, so
`LoadShed` wraps `GlobalConcurrencyLimit`. (Tested empirically too: swapping to
per-connection-only makes the cross-connection test red, see below.)

The request timeout (`.timeout`) cuts a hung handler; verified `mod.rs:224`. tonic
surfaces a server-side handler-timeout cancellation; the client observes `CANCELLED`
or `DEADLINE_EXCEEDED`.

## Configurability (operator-tunable, not a constant)

`AdmissionControl` (`dserver.rs:94-130`) with `with_admission_control`
(`dserver.rs:122-131` region of the builder methods). The **binding** knob is the
server-wide `max_concurrent_requests` (default `DEFAULT_MAX_CONCURRENT_REQUESTS = 64`,
documented HDD-shallow / SSD-deep tuning). CLI exposes `--max-concurrent-requests` and
`--request-timeout-secs` (`crates/server/src/cli.rs:266-281, 301-304`). The
per-connection cap is kept as a documented secondary defense, explicitly *not* the
binding bound (doc-comment `dserver.rs:73-85, 101-105`).

## Test (red→green) — `crates/server/tests/dserver.rs`

Two net-new tests (the criterion is absent pre-fix, per the brief's verification
posture):

1. `overload_across_connections_sheds_excess_with_a_retryable_status` — global limit
   = 1, **two separate `GrpcChunkStore::connect` connections**. Connection A's request
   is admitted and parks holding the one slot; connection B's request (its own fresh
   per-connection budget) must still be shed with `ResourceExhausted`/`Unavailable`.
   This is the discriminator the carry-forward asked for: a per-connection-only limit
   gives B its own slot and admits it.
2. `hung_handler_is_cut_by_the_request_timeout` — a never-returning handler is cut by
   a 200 ms timeout with `Cancelled`/`DeadlineExceeded`.

Both are import-light (tonic/tokio, no GUI/display dep) and self-bounded with
`tokio::time::timeout(5s)` so they cannot hang the runner.

Red→green evidence (project runner, `cargo test -p wyrd-server --test dserver`):

- **Green** with the fix: 4/4 pass.
- **Red** with the production layers reverted to the bare `Server::builder()` (test
  kept): both new tests fail on `Elapsed(())` — the excess request queues / the hung
  handler never gets cut — while the two pre-existing tests stay green.
- **Discrimination check**: with only `.concurrency_limit_per_connection(1)` (the
  rejected v1 shape, no global layer), the cross-connection test **fails** — proving
  the test catches the server-wide vs per-connection distinction, not just "some limit
  exists".

## Rejected alternatives

- **Per-connection limit only (v1)** — rejected: does not bound aggregate in-flight
  (unbounded in connection count); the very gap that failed sign-off. Shown red by the
  discrimination check above.
- **A `tower::buffer` + worker pool** — would bound work but adds a buffer queue and a
  spawned worker (new failure modes, larger surface) when a shared semaphore +
  load-shed already restores the invariant with one shared `Arc<Semaphore>` and no
  extra task. Not minimal for the invariant.
- **Making load-shedding an optional toggle** (v1 had a `load_shed` field) — dropped:
  the success criterion's binding behaviour is a *shed* status, and the invariant is
  "must fail closed". An off-switch on the fail-closed behaviour contradicts the
  invariant, so shedding the server-wide excess is unconditional.

## Dependency note

Added `tower` as a **direct** dependency of `wyrd-server` with only `["limit",
"load-shed"]` features. It is already in the tree transitively via `tonic`; the
lockfile diff is a single dependency edge (`Cargo.lock` +1 line, no new crates), so the
ADR-0003 cargo-deny license surface is unchanged.

## Gate status / caveat

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-server --all-targets`
  (workspace `-D warnings`): clean.
- `cargo xtask ci`: fmt/clippy/build and the entire non-DST workspace test suite pass.
  The madsim **DST seed-sweep** flagged one failure in
  `crates/dst/tests/custodian.rs::durability_emission_rises_then_returns_to_zero`
  (under-replication count) on a randomly-chosen seed. This is **pre-existing and
  unrelated**: that test passes at its reported seed both with and without my changes,
  and the DST suite never exercises `DServer::serve` (it uses an in-memory `MemDServer`
  fake in `custodian.rs:172` and a directly-built madsim-tonic server in
  `network.rs:180`), so my admission layers are not on that path. Flagged for the human
  at sign-off; it is not introduced by this patch and is outside issue #205's scope.
