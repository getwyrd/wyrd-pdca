# Brief — issue 205 / dserver-grpc-admission-control

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** dserver-grpc-admission-control
- **Defect:** The d-server builds its tonic gRPC server with **all defaults** — a bare
  `Server::builder().add_service(...)` (`crates/server/src/dserver.rs:160`) that sets no
  concurrency limit, no request timeout, and no HTTP/2 inbound-stream / TCP tuning. So
  the server has **no fail-closed behaviour under overload**: unbounded concurrent
  requests all contend for runtime threads and — compounded by the blocking storage I/O
  of the companion issue #204 — an overload spills into **thread-pool exhaustion instead
  of a clean, retryable "busy" signal**. This is the opposite of architecture §8.9
  ("the system fails closed under pressure… shed or slow load predictably… never trade
  correctness for admission", `docs/design/architecture/08-crosscutting-concepts.md:98-107`).
  Per-connection request parallelism is also left at the implicit h2 default
  (`max_concurrent_streams`), and a stuck handler can pin a slot indefinitely for want of
  a request timeout.
- **Success criterion:** When offered more concurrent requests than its configured
  admission limit, the d-server **sheds** the excess with a retryable gRPC status (a
  resource-exhausted / unavailable "busy" signal) rather than admitting them unboundedly
  and exhausting runtime threads; and a handler that hangs past the configured request
  timeout is cut with a deadline status rather than pinning a slot forever. The admission
  limit (and the companion I/O-concurrency posture) is **configurable**, not a hardcoded
  constant. Demonstrable at C4-verify by a flippable test that sets a small admission
  limit, drives more concurrent/long requests than the limit, and asserts the excess
  receives a retryable shed status. BINDING is the fail-closed behaviour (overload →
  retryable status, not thread exhaustion) and that the limit is configurable; the exact
  tonic knobs (`concurrency_limit` / per-connection limit, a tower load-shed+timeout
  layer, request `timeout`, `max_concurrent_streams`, `tcp_nodelay`, HTTP/2 keepalive) are
  ILLUSTRATIVE — Do's call, confirming each control's current default against the pinned
  `tonic`/`h2` version before overriding (or recording why a default is kept).
- **Invariant to restore:** The d-server's request-admission path must **fail closed under
  pressure**: beyond its admission capacity it sheds (or slows) load predictably with a
  retryable signal and bounds the work a single request can hold, never trading
  correctness for admission and never degrading into unbounded contention / thread-pool
  exhaustion; and the capacity is operator-tunable to the device's useful queue depth
  (HDD vs SSD), not a fixed constant. Source: architecture §8.9 admission-control /
  fail-closed-under-pressure (`docs/design/architecture/08-crosscutting-concepts.md:98-107`)
  — internal project invariant (Tier C), authoritative project doc. (Structural fix —
  load/admission-safety per principles.md §1.2; the target is the smallest change that
  restores fail-closed admission, not the smallest diff. Self-test: the invariant is over
  the server's whole admission posture under overload, so it is not satisfiable by, say,
  setting one knob while leaving the overload path unbounded — overload must demonstrably
  shed.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Surfaces:** data
- **Scope:** configure the d-server's tonic `Server` for controlled concurrency and
  backpressure so overload returns a retryable status instead of exhausting threads, bound
  a stuck handler with a request timeout, and set/justify the HTTP-2 stream and TCP
  options — with the limits exposed as configuration carrying HDD/SSD-appropriate
  documented defaults. / out of scope: offloading the blocking storage I/O off the reactor
  (#204 — the companion fix; backpressure and offload are complementary but separate
  logical changes); any change to the architecture ADR/spec text itself (this implements
  the *existing* §8.9 intent — an ADR/spec edit would be a separate, human-authored,
  immutability-gated change per INTEGRATION §2/§4); the temp-path race (#203) and the
  scrub/get corruption contract (#207).
- **Repro instruction:** On `main` @ `c2223a5`, stand up a `DServer` (or the
  `ChunkStoreService` over a tonic `Server` built as in `dserver.rs:160`) and drive far
  more concurrent in-flight requests (or deliberately slow handlers) than the host's
  runtime can serve. Observe that requests queue/contend without bound and no retryable
  "busy"/shed status is returned — the server does not fail closed; a hung handler holds
  its slot indefinitely.
- **Test file:** crates/server/tests/dserver.rs   (with a small configured admission limit,
  excess concurrent requests receive a retryable shed status — red pre-fix where no limit
  exists, green post-fix; see Verification posture)
- **Verification posture:** the binding behaviour is testable in-process — set a low
  admission limit, saturate it with concurrent/blocking requests, and assert the excess
  get a retryable status (resource-exhausted/unavailable) and a hung handler is cut by the
  timeout. The post-fix green is deterministic given a configured small limit; the pre-fix
  "red" is criterion-absence (no shed status exists today because there is no limit), so
  the test is partly net-new coverage rather than a flip of a prior failing assertion —
  Do should still demonstrate the contrast (no shed pre-fix vs. shed post-fix) at the same
  low limit. A full overload/thread-exhaustion load test is supplementary, off-Check
  evidence (a load run), not the binding C4 criterion.
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/server/src/dserver.rs` across
  merged history (`17cfb91`, `186c82f` — introduced/extended `serve` with the bare
  builder; neither added admission control), open PRs (`gh pr list --state open` — none
  touch this file), and closed PRs — no prior or in-flight fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on §6.1 (C5 contested root cause). The fix uses tonic's `.concurrency_limit_per_connection(...)`, which (verified against tonic-0.14.6 src/transport/server/mod.rs: ConcurrencyLimitLayer built per-connection in MakeSvc::call) gives each connection its own semaphore. Aggregate in-flight = connections × max_concurrent_requests is therefore unbounded in connection count, so the server-wide "fail closed under pressure" invariant (§8.9 + brief.md:44-47 self-test) is NOT restored — a many-connection overload still reaches thread-pool exhaustion. The passing C4 only proves single-connection shedding. What to change next: - Add a SERVER-WIDE concurrency bound via a shared-semaphore tower layer (e.g. tower GlobalConcurrencyLimitLayer holding one Arc<Semaphore> cloned across all connections) applied to the service stack via `.layer(...)`, with `load_shed` so over-limit requests are shed globally. Keep the per-connection limit + request timeout as additional layers; the binding requirement is the global cap. Expose the global limit as the operator-tunable knob. T5 test improvements to add: - Add coverage that drives overload across MULTIPLE client connections and asserts the excess is shed — the current single-connection test passes even with only per-connection limiting, so it does not prove the global bound. - Make the shed test robust to GrpcChunkStore opening more than one connection (assert/pin the connection count, or restructure) so it cannot silently stop exercising the shed path.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected only because the gating C4-ci (madsim DST) is red. Verified directly at target d60ef6c: that red is NOT caused by this patch. The failing test, custodian.rs::durability_emission_rises_then_returns_to_zero, runs entirely on the in-memory MemDServer fake (custodian.rs:479) — no tonic/tower/DServer::serve — so the admission-control change cannot influence it by any code path. It reproduces on clean main with the patch reverted (10/10 under load) and is non-deterministic at a fixed MADSIM_TEST_SEED (same seed gives both pass and fail) — a pre-existing flaky/load- sensitive DST failure, not a regression. The admission-control fix itself is sound and source-verified (server-wide GlobalConcurrencyLimitLayer holding one Arc<Semaphore> + LoadShedLayer via Server::layer; cross-connection discriminator test). PRESERVE it — do NOT re-do or re-design the admission code; iteration 1's per-connection scope error is already correctly resolved. Next Do (carry-forward): keep the existing admission-control patch as-is, and in the SAME iteration de-flake the gating DST test so C4-ci goes green deterministically — repair the leaking real-time/scheduling nondeterminism in durability_emission_rises_then_returns_to_zero (ADR-0009: DST must be deterministic), or pin/quarantine it with justification. The bundle cannot accept while the gate is red, but the blocker is the DST determinism, not the fix. Do NOT make the gate green by re-running until a sweep passes — that games a non-deterministic gate.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: madsim DST tests failed with exit status: 101
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
