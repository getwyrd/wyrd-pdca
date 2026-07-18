# Result — issue 575 / request-red-capacity-signals

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: a live deployment shows *service* health, not just storage health: the S3
- Success criterion: with the patch applied, asserted via in-process Prometheus
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: (1) request-plane RED at the S3 gateway ops — per-op latency + error

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue 575: emit S3 request RED and d-server capacity signals, including correct transient classification for a GET that fails after its 200 response head.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is operationally falsifiable: a real loopback request must expose stable per-op/error-class metrics and capacity events without changing admission behavior (`crates/server/tests/request_capacity_planes.rs:572`). |
| C2 Reproduction (red pre-fix) | PASS | In a scratch checkout retaining the regression test but removing only the body-error reclassification, the exact test failed with `terminal=1` and `transient=0` at `crates/server/tests/request_capacity_planes.rs:619`. |
| C3 Change | PASS | The decision point now consumes the seam classifier when the previously unknowable body error actually surfaces, preserving the existing head-time class for other completion paths (`crates/gateway-s3/src/lib.rs:523`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the deterministic gate record despite incomplete local policy rerun — red→green and fmt/clippy/build/tests reproduced, but `cargo deny` could not lock its read-only advisory DB, so dependency-policy verification rests on `check-gates.json` rather than this reviewer host (`crates/server/tests/request_capacity_planes.rs:619`). |
| C5 Causal adequacy | PASS | The failure is classified at its first observable source rather than hidden by a capability probe or status-code guard, so operators no longer receive the head-time default for an error unavailable at head time (`crates/gateway-s3/src/lib.rs:536`). |
| T1 Structure | PASS | The regression drives the production S3 composition over a loopback connection and proves a successful head plus partial body before injecting the fault (`crates/server/tests/request_capacity_planes.rs:578`). |
| T2 Shape | PASS | The emitted label remains the bounded `ErrorClass` seam value and no per-request cardinality source is introduced (`crates/gateway-s3/src/lib.rs:542`). |
| T3 Runtime | PASS | The focused patched test and all six request/capacity-plane tests passed, including transient-not-terminal read-back after draining the torn body (`crates/server/tests/request_capacity_planes.rs:600`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed or rejected work duplicates these affected paths — local merged history was checked by file path and showed only telemetry/request-id enablers, but closed/rejected review state is not mechanically available in the supplied artifacts (`crates/gateway-s3/src/lib.rs:523`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether unary-RPC equivalence justifies deferring a distinct transport-stream gauge and whether per-domain utilization once per rebalance pass is sufficient — either choice changes whether issue 575 fully closes the ratified observability floor (`crates/server/src/dserver.rs:623`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether in-process Prometheus read-back is sufficient production evidence — for live validation, run `wyrd s3 ... --otlp-endpoint <collector>` and `wyrd d-server ... --otlp-endpoint <collector>`, drive PUT/GET/failing GET plus overload/timeout, and confirm the collector or scrape shows RED classes, admission counters, and an in-flight gauge returning to zero (`crates/server/tests/request_capacity_planes.rs:572`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the deterministic gate record despite incomplete local policy rerun — red→green and fmt/clippy/build/tests reproduced, but `cargo deny` could not lock its read-only advisory DB, so dependency-policy verification rests on `check-gates.json` rather than this reviewer host (`crates/server/tests/request_capacity_planes.rs:619`).
- [x] T4 Contribution — Confirm no closed or rejected work duplicates these affected paths — local merged history was checked by file path and showed only telemetry/request-id enablers, but closed/rejected review state is not mechanically available in the supplied artifacts (`crates/gateway-s3/src/lib.rs:523`).
- [x] T5 Judgment — Decide whether unary-RPC equivalence justifies deferring a distinct transport-stream gauge and whether per-domain utilization once per rebalance pass is sufficient — either choice changes whether issue 575 fully closes the ratified observability floor (`crates/server/src/dserver.rs:623`).
- [x] Validation — fitness-to-purpose — Decide whether in-process Prometheus read-back is sufficient production evidence — for live validation, run `wyrd s3 ... --otlp-endpoint <collector>` and `wyrd d-server ... --otlp-endpoint <collector>`, drive PUT/GET/failing GET plus overload/timeout, and confirm the collector or scrape shows RED classes, admission counters, and an in-flight gauge returning to zero (`crates/server/tests/request_capacity_planes.rs:572`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
