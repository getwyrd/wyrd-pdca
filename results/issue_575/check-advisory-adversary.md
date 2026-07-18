# Adversarial review — issue 575 / request-red-capacity-signals (iteration 2)

Scope of this iteration per the carry-forward (brief.md:181): ONE defect — a mid-stream
body fault on a streaming S3 GET was classed by the head-time `Terminal` default instead
of by the seam's classifier. I assumed the fix and the reviewer were wrong and tried to
prove it. Verdict up front: **attempted six refutations; all failed.** The one soft spot
in the *gate's* evidence I closed myself by re-running a sharper experiment.

## Refutation attempts (all grounded on $PDCA_TARGET)

- **The red→green evidence is over-broad — and I closed the gap myself.** The C4-verify
  red leg reverts to `$PDCA_VERIFY_BASE`, where the new test file cannot even compile
  (it names `S3Gateway::with_metrics_dispatch`, `DurabilityTelemetry::metrics_dispatch`
  — APIs the base lacks), so the gate's "red" never isolates *this iteration's* defect.
  I re-ran the decisive experiment in a scratch clone: reverted ONLY the Err-arm
  classification (`crates/gateway-s3/src/lib.rs:542-543` back to iteration-1's
  `self.record("failed")`) and ran the new test —
  `the_request_plane_classes_a_mid_stream_fault_by_the_seam_not_the_head` **fails**
  (sample lands on `class="terminal"`), and with the fix restored all 6 tests **pass**
  (green re-run: `6 passed; 0 failed`, 0.24s). That is an assertion-level, defect-specific
  red→green, stronger than what the gate recorded. The compile-failure-red structural
  issue was already adjudicated at the previous sign-off (brief.md:181, carried as a §10
  Act candidate) — not re-filed.
- **Tried: the test passes for the wrong reason (tautology / parallel path).** No. The
  fixture drives the real router over a real loopback socket; the classified error is the
  `axum::Error` the production body machinery wraps stream errors in, and
  `wyrd_traits::classify` (`crates/traits/src/lib.rs:535-550`) walks `source()` to reach
  the `TransientFault` — verified against the real types, not a mock. The test also pins
  the discriminating negative (`class="terminal"` == 0.0 against a *preregistered* zero
  series, `crates/gateway-s3/src/lib.rs:159-171`), so a head-time-only implementation
  cannot sneak through — proven by the red run above. And the fixture's shape matches
  production: the real Gateway's reader task sends a mid-read fault **as a stream `Err`**
  before ending (`crates/server/src/lib.rs:332-343`), exactly what the fixture emulates.
- **Tried: the early-EOF sibling path still mislabels a transient truncation.** The
  `Poll::Ready(None)`-short arm keeps the head-time `Terminal`
  (`crates/gateway-s3/src/lib.rs:546-558`). Concrete production input needed: a transient
  fault that surfaces as early EOF *without* an `Err`. There isn't one — the production
  reader (`crates/server/src/lib.rs:334-340`) always delivers the `Err` through the
  channel before breaking; a silent early stop requires the reader task itself dying,
  which is genuinely unnameable, and `Terminal` is `classify`'s own answer for the
  unnameable. Could not refute.
- **Tried: the timeout/cancel attribution is a wall-clock guess.** The code claims
  exactness (`crates/server/src/dserver.rs` AdmissionGuard docs). Verified against
  vendored tonic 0.14.6: `GrpcTimeout::call` evaluates `inner: self.inner.call(req)`
  (which stamps `started`) *before* arming its `sleep`
  (`tonic-0.14.6/src/transport/service/grpc_timeout.rs:41-62`), and tonic's per-connection
  wrap order (`transport/server/mod.rs:1234-1239`) puts `GrpcTimeout` directly around the
  user layer stack — so `elapsed >= request_timeout` at drop time is sound for the timeout
  side. Residual: a client cancellation that tonic processes *after* the deadline has
  elapsed (cancel at t≈199ms, drop at t≈201ms of a 200ms timeout) is counted `timed_out`.
  Inherent to drop-based observation, boundary-only, invisible at operator granularity —
  noted, not actionable.
- **Tried: per-connection `Overloaded` pollutes the server-wide shed counter.** No:
  tonic applies `concurrency_limit_per_connection` + `load_shed(true)`
  (`crates/server/src/dserver.rs:671-672`) *outside* the user layer stack
  (`tonic-0.14.6/src/transport/server/mod.rs:1234-1239`), so a per-connection shed never
  transits `ShedObserver`. It is silent instead — disclosed in code and carried as
  getwyrd/wyrd#584, already cleared by the human (brief.md:181). Not re-filed.
- **Tried: metric callsites latch `Interest::never` under a restrictive production
  `RUST_LOG`, so a live role emits nothing.** No: `tracing_core::Dispatch::new` registers
  the dispatcher and rebuilds the whole callsite interest cache
  (`tracing-core-0.1.36/src/dispatcher.rs:472-481`, `src/callsite.rs:484-487`), and each
  role builds its metrics dispatch at startup before serving — so combined interest stays
  ≥ `sometimes` and `enabled()` is consulted per-dispatch. The test's `Once` guard covers
  the test-binary variant of the same hazard.

## Verdict on the reviewer's claims

No unwarranted claim found in `check-gates.json` for this iteration's scope: C4-ci is a
deterministic gate; C4-verify's "red without the fix" is literally true (if bluntly so —
compile-red, adjudicated previously); and the carry-forward's single-defect scoping was
respected by the diff (the gateway/dserver/cli/telemetry bulk is the iteration-1 bundle
carried forward unchanged in substance, plus the Err-arm classification and its test).
I could not refute the fix.
