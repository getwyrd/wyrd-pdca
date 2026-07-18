# Build notes — issue 575 / request-red-capacity-signals

Withheld from the reviewer; written for the human at sign-off.

## What I built

Three things, matching the brief's Scope (1)/(2)/(3):

1. **Request-plane RED** (`crates/gateway-s3/src/lib.rs`) — a per-op latency histogram
   (`s3_request_duration_ms`, label `op`) and an error counter (`s3_request_errors`, labels
   `op` + `class`), raised as `tracing` metric events on the existing seam.
2. **Capacity-plane signals** (`crates/server/src/dserver.rs`) — `capacity_requests_admitted`
   / `_shed` / `_timed_out` / `_cancelled` counters and the `capacity_requests_in_flight`
   gauge, from two tower observer layers positioned around the *existing* admission stack.
3. **The role wiring** (`crates/server/src/cli.rs`) — `cmd_s3` and `cmd_d_server` now build a
   `DurabilityTelemetry` and hand it down, mirroring `cmd_custodian` (`cli.rs:797-802`). This
   was the brief's verified-missing enabler; without it (1) and (2) emit into a subscriber
   with no metrics provider and report nothing.

Plus one shared-seam addition: `DurabilityTelemetry::metrics_dispatch()`
(`crates/telemetry/src/lib.rs`).

## The decision that shaped everything: a *carried* dispatch, not a scoped one

The custodian composes telemetry by wrapping each pass in a scoped dispatch
(`custodian.rs:310`, the peer the brief cites). I could not copy that, and the reason is
worth the human's attention because it is the thing that would have made this bundle a
convincing fake:

**tonic and axum `tokio::spawn` a task per connection** (verified:
`tonic-0.14.6/src/transport/server/mod.rs:925`), and a spawned task does **not** inherit a
scoped `tracing` dispatch. A `with_subscriber(...)` around the serve future reaches no
handler. So a server role's instrumentation must *carry* its sink and enter it around each
(synchronous) emission — which is what `metrics_dispatch()` + `with_metrics_dispatch(...)`
do.

This is also why **both** test legs drive a **real loopback listener** rather than
`Router::oneshot`. A `oneshot` test runs the handler on the test's own task, where a scoped
dispatch *does* work — so it would have gone green against wiring that emits **nothing in
production**. That is precisely the "green mechanical check on something adjacent" trap.
Driving the real listener costs nothing (the `s3_http_wire.rs` / `dserver.rs` fixtures
already do it) and proves the spawn path.

Why the dispatch is **metrics-only** rather than `logging::dispatch(log, writer,
metrics_layer)` like the custodian's: the custodian's dispatch is current for a whole pass,
so it must carry the log layers or it would swallow that pass's audit lines. Mine is current
for exactly one metric event, so adding a `fmt` layer would only echo every metric to stderr
as a second log row — one extra line per request on a hot path. Everything else the role logs
keeps going to the global subscriber (#527), untouched.

Production correctness check I made explicitly: `Dispatch::new` registers with tracing's
callsite registry (`tracing-core-0.1.36/src/dispatcher.rs:479`), and interests combine with
`Interest::and` → `sometimes` when they disagree. So even under `wyrd s3 --log-level error`
(where the global log dispatch alone would register the info-level metric callsites as
`never`), the metric callsites resolve to `sometimes` and the per-event `enabled()` check
against the *current* (metrics) dispatch lets them through. Metrics are not silenced by the
log level.

## Two real bugs I found in my own work

I am flagging these because both were found by the forced refutation below, not by the happy
path — and one of them was a genuine production concurrency defect.

### 1. The in-flight gauge had a lost-update race (production bug)

First version used `AtomicI64::fetch_sub` and then emitted the computed level *outside* the
atomic. Two requests completing concurrently compute levels `1` and `0` and can then emit
them in the **opposite** order — latching a last-value gauge at `1`. The server is idle and
the gauge says a request is in flight, indefinitely.

This is exactly the "rises but never returns to zero" failure the custodian's day-one suite
exists to rule out, and it is **invisible to any test that drives one request at a time**. It
surfaced as a ~50% flake (`left: Some(1.0), right: Some(0.0)`) only because the test holds
**two** concurrent requests. I nearly dismissed it as a test-timing artifact; it was not.

Fix: `CapacityPlane::record_in_flight` holds a `Mutex<i64>` **across the emission**, so the
last value recorded is always the last value counted. Cost, honestly: one lock/unlock per
admission and per completion (2 per RPC), critical section = an integer update plus one
`tracing` event, no I/O and no await — on a path that already takes an admission semaphore,
parses HTTP/2 and touches a disk. I rejected the lock-free alternatives: re-loading the
atomic inside the emit narrows but does **not** close the race (both loads can still emit out
of order). The genuinely right instrument is an OTel **ObservableGauge** (a callback read at
collection time — no lock, no ordering, no per-request cost), but reaching for the OTel meter
directly bypasses the one-instrumentation-path discipline the brief explicitly requires
("Emitting via direct OTel meters … would bypass ADR-0012 — follow the established seam"), and
`tracing-opentelemetry`'s `gauge.` bridge only drives synchronous gauges. Flagged below as a
possible follow-up, not smuggled in here.

### 2. Pre-registering the latency histogram was wrong *and* faked the test green

I pre-registered every RED series at zero (577's `ErrorClass::ALL` doc explicitly asks for
this, naming #575). For the **counter** that is right and harmless: `add(0)` is value-neutral,
and it makes a healthy gateway read "0 errors" instead of "no data".

For the **histogram** it was a real bug: `record(0)` is a genuine observation, so it seeds
every op's latency distribution with a phantom 0ms sample and reports a front door faster than
it is.

It also made my own test hollow: `s3_request_duration_ms_count{op="put"} >= 1` was satisfied
by the pre-registration *alone*. The emission-removed refutation run caught it — that test was
the one leg that stayed **green with the measurement deleted**. Fixed both ends: dropped the
histogram pre-registration (documented why the two instruments differ), and tightened the
assertion from `>= 1` to an exact `== 1.0` per op, plus `op="delete" == 0.0` to prove the op
label is a real key rather than decoration.

## Mechanism choices worth review

**Layer positioning (two layers, not one).** `ShedObserver` is the outermost user layer — it
must be outside `LoadShedLayer` to see the `Overloaded` rejection at all. `AdmissionObserver`
is the innermost — inside `GlobalConcurrencyLimitLayer`, whose permit is acquired in
`poll_ready`, so *reaching* it **is** holding a slot. That is what makes "admitted" mean
admitted and keeps a shed request off the in-flight gauge entirely. One outermost layer would
have had to count every arrival and retract the shed ones — which is how an in-flight gauge
starts reporting load that was never accepted.

**Timeout detection is drop-based, and the ordering is proven, not guessed.** tonic applies
`.timeout()` **outside** the user layer stack (`GrpcTimeout(user_stack)`,
`mod.rs:1234-1239`), so a deadline cut is never a `Poll::Ready(Err)` from inside — the inner
future is simply **dropped**. `AdmissionGuard` therefore reports from `Drop`, splitting
timeout from cancellation on elapsed time. The comparison is exact rather than a wall-clock
heuristic: `GrpcTimeout::call` evaluates `inner: self.inner.call(req)` **before** `sleep:
sleep(timeout)` (struct fields evaluate in order), so the guard's `started` is stamped no
later than the deadline's start; tokio's `sleep` is documented to wait *at least* its
duration; therefore when the deadline fires, `elapsed >= request_timeout` holds. A client that
hangs up early, or sets a tighter `grpc-timeout` header (tonic takes the min), correctly reads
as `cancelled` — attributing that to *our* deadline would be a lie about whose deadline fired.

I rejected replacing tonic's `.timeout()` with a tower `TimeoutLayer` inside the user stack
(which would make the cut directly observable): it changes behaviour — tonic's `GrpcTimeout`
also honours the client's `grpc-timeout` header and maps its own error — and the brief puts
any change to admission *behaviour* out of scope. I also rejected arming a parallel
observation `Sleep` per request: exact, but it doubles per-request timer registrations, where
the drop path costs one `Instant::now()`.

**Boxed observer futures.** Both layers return `Pin<Box<dyn Future ...>>`. Projecting a pinned
inner future without a `pin-project` dependency needs `unsafe`, which both crates
`#![forbid]`. Cost: one allocation per request per layer (2/RPC) on a path that already boxes
per request inside tonic (`BoxCloneService`) and then does fragment I/O. I did **not** add
`pin-project-lite` — the brief says a new dependency is a declare-and-stop, and this does not
warrant one.

**The class is stamped, not re-derived.** `gateway_error_response` runs `wyrd_traits::classify`
(577) where the backend's error still exists and puts the verdict in a response extension; the
completion point reads it. Re-deriving the class from the HTTP status at completion would be a
second, divergent classifier — and it could not work anyway: the S3 mapping deliberately
answers `500 InternalError` for *both* a transient fault and a may-have-landed commit, so the
wire status does not carry the distinction the counter reports. A response with no extension
(unsigned 403, bad path 400, absent key 404, bad verb 405) defaults to `Terminal`, which is
`classify`'s own documented fail-safe default and correct on the merits.

## Scope discipline

- Per-failure-domain utilization is **not** touched (`rebalance.rs:320-326` already ships it);
  I reused its `capacity_` naming convention for the new capacity series.
- No transport-stream gauge (descoped by the brief: all chunkstore RPCs are unary).
- No admission **behaviour** change: every observer forwards its inner outcome unaltered, and
  every `Server::builder()` policy option is byte-for-byte unchanged. The shed test asserts
  `capacity_requests_admitted == 1` (only A), which would break if the observers perturbed
  what is admitted.
- No new dependency. `tower`'s `Layer`/`Service` are unconditional re-exports; the crates I
  touched already had `tracing` + `wyrd-traits`. `gateway-s3` deliberately does **not** gain a
  `wyrd-telemetry` dependency — it takes a plain `tracing::Dispatch`, so no telemetry backend
  leaks into a leaf crate (0010's invariant). The ~4-line `emit_into` shim is duplicated in
  `gateway-s3` and `dserver.rs` for exactly that reason; sharing it would mean pulling the OTel
  stack into the S3 crate.

## Known limitation (for the human, deliberately not worked around)

The shed observer sees the **server-wide** shed only — the bound `AdmissionControl` documents
as binding and the one the brief names (`max_concurrent_requests`, `:100`). tonic applies the
secondary **per-connection** cap (`.concurrency_limit_per_connection` + `.load_shed(true)`)
*outside* the user layer stack entirely, so its shed is unreachable from any `Server::layer`.
Covering it would mean moving that cap into the user stack — a behaviour change, out of scope.
Documented in the code rather than papered over.

## Before declaring done — the three refutation questions

**(a) Genuine red?** Yes, twice over.
- *Full revert* (production stashed, test kept): fails to compile — `no method named
  with_metrics_dispatch / metrics_dispatch` (4× E0599). This is the C4-verify gate's red leg.
- *Stronger, surgical revert* — this is the one that matters: I kept **every** public API and
  **both** layers in the stack and deleted **only the emission bodies**. Result:
  **0 passed; 5 failed**. So the assertions bind the emitted signal, not the API's existence.
  This run is what exposed the hollow histogram assertion (§"two real bugs" above) — before
  the fix it was `1 passed; 4 failed`, with the latency leg green against a gateway that
  measured nothing.

**(b) Production path?** Yes. Each test stands up the real role over a real loopback listener
and reads back from the handle the role wired: the S3 leg drives `S3Gateway::new(...)
.with_metrics_dispatch(...).serve(listener)` — the composition `cli::serve_s3` builds — over a
real `Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>`, with SigV4 signatures from
the production `sigv4::sign`. The d-server leg drives `DServer::bind(...)
.with_admission_control(...).with_metrics_dispatch(...).serve(...)` — the composition
`cli::run_d_server` builds — over real tonic/HTTP-2 with a real `GrpcChunkStore` client. No
metric is re-emitted by the test; every assertion reads
`DurabilityTelemetry::gather_prometheus`.

**(c) Fixture includes the fault?** Yes — each fault is *injected and real*, not curated out:
- the failing op is a real `wyrd_traits::TransientFault` (577's seam type) wrapping a backend
  error, classified by production `classify` walking the source chain — the test asserts the
  `transient` label AND asserts `class="terminal"` is **0**, so a fail-safe default could not
  masquerade as a correct classification;
- the shed is a *forced* overload — a real admission slot held open across two separate
  connections against a server-wide bound of 1, with the excess request confirmed to error;
- the timeout is a *real* hung handler cut by a real `request_timeout`, and the test confirms
  it reached the handler before being cut (so the deadline cut a genuinely in-flight request),
  and asserts `capacity_requests_cancelled == 0` so a mis-split cannot pass;
- the in-flight gauge is driven by **two concurrent** held-open requests — which is the only
  reason the lost-update race was caught at all.

## Possible follow-ups (not filed, for the human to judge)

1. **ObservableGauge for in-flight.** Would remove the per-RPC mutex entirely and is the
   semantically right instrument for a level, but needs either a `tracing-opentelemetry`
   `gauge.`-callback bridge or an ADR-0012 carve-out to touch the meter directly. Not a
   correctness gap today (the mutex is correct); a hot-path cost question.
2. **A scrape endpoint.** `metrics_dispatch()` wires the Prometheus registry, but no role
   exposes an HTTP `/metrics` listener — a live scrape is still off-Check/manual (the brief
   scopes it that way). `--otlp-endpoint` push works today on both new roles.
3. **Per-connection shed visibility** (the limitation above), if it ever matters.

## Verification run

- `cargo test -p wyrd-server --test request_capacity_planes` — 5/5 green; run **10×** to
  confirm the concurrency fix (the pre-fix flake reproduced ~50% of full-binary runs, and
  never when the test was run alone).
- `cargo clippy --workspace --exclude wyrd-dst --all-targets` — clean (`-D warnings`). Note
  `cargo check -p <crate>` does **not** build `#[cfg(test)]` targets; the gate's
  `--all-targets` caught two in-crate `gateway-s3` unit tests constructing `AccessLogged`
  that I had missed. They assert the *access row*, not RED, so they pass `metrics: None`.
- `cargo fmt --all` applied — the gate's `fmt --check` rejected my hand formatting, which is
  what the target's commit hook would have done at publish time.
- `./engine/xtask.sh ci` — full gate.
