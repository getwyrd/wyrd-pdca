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

Review of issue 575: add observable S3 request RED metrics and d-server admission/in-flight capacity signals through the shared telemetry seam.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding criterion is concrete and observable in-process: PUT/GET latency, typed-class failures, admitted/shed/timed-out events, and a gauge returning to zero are separately asserted at `crates/server/tests/request_capacity_planes.rs:297`. |
| C2 Reproduction (red pre-fix) | PASS | In a scratch clone of the 577 prerequisite base, retaining the new test while reverting production caused compilation to fail on the absent S3, d-server, and telemetry dispatch APIs cited at `crates/server/tests/request_capacity_planes.rs:234`; the scratch tree was removed afterward. |
| C3 Change | PASS | The patch addresses both dark planes at their production composition seams, and the unchanged behavior boundary is exercised by real-router and real-server-stack calls at `crates/server/tests/request_capacity_planes.rs:305` and `crates/server/tests/request_capacity_planes.rs:468`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the independently confirmed focused red→green despite incomplete full-CI reproduction — all five binding tests pass, but `cargo xtask ci` reached `cargo deny check` and failed because its advisory lock path is read-only, so the asserted all-checks-green row could not be independently affirmed. |
| C5 Causal adequacy | PASS | The evidence exercises the previously absent production sinks and decision points rather than a capability probe or fallback: typed failure separation is bound at `crates/server/tests/request_capacity_planes.rs:357`, and overload/timeout decisions are bound at `crates/server/tests/request_capacity_planes.rs:568` and `crates/server/tests/request_capacity_planes.rs:628`. |
| T1 Structure | PASS | The cross-crate ownership boundary remains coherent: leaf services accept an opaque dispatch while the role composition owns telemetry lifetime; the real d-server composition is exercised at `crates/server/tests/request_capacity_planes.rs:443`. |
| T2 Shape | PASS | The exported label space is bounded and the tests prove labels contribute semantically, including zero DELETE samples and transient-not-terminal separation at `crates/server/tests/request_capacity_planes.rs:340` and `crates/server/tests/request_capacity_planes.rs:388`. |
| T3 Runtime | PASS | Independent runtime execution passed all five in-process production-composition tests, including two concurrent held requests rising to 2 and returning to 0 at `crates/server/tests/request_capacity_planes.rs:514`. |
| T4 Contribution | PASS | Affected-path history and closed-PR searches found prior telemetry/router/admission enablers but no closed or rejected request-RED/capacity-plane duplicate; the new binding test path has no prior closed PR, and its production scope begins at `crates/server/tests/request_capacity_planes.rs:297`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether unary chunkstore RPCs justify deferring a distinct transport-stream gauge and whether rebalance-pass cadence is sufficient for the already-merged domain-utilization signal — accepting this scope closes item 5 without those separate signals. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process Prometheus evidence is sufficient for deployment fitness — it proves emission and read-back at `crates/server/tests/request_capacity_planes.rs:320`, but does not exercise the supplementary live scrape/OTLP operational path. |

### Advisory — adversary

# Adversarial review — issue 575 / request-red-capacity-signals

Skeptic's pass over `patch.diff` + `check-gates.json`, grounded on `$PDCA_TARGET`
(`wyrd.pdca-wt`). Tower/tonic mechanism claims were checked against the vendored
`tonic-0.14.6` sources, not taken from the patch's comments.

## Findings

- NEEDS-HUMAN [impl] — **A transient mid-stream body failure is counted `class="terminal"`.**
  The RED class is captured once, at head time, from the response extension
  (`crates/gateway-s3/src/lib.rs:653-655` — `None` ⇒ `Terminal`), but a streaming GET that
  returns a `200` head and then fails is only *later* marked errored via the body wrapper
  (`crates/gateway-s3/src/lib.rs:510-514` `record("failed")`, `:521-522` `"truncated"`,
  errored-flag at `:468`). Concrete failing case: a d-server dying mid-read raises a
  `TransientFault` inside the body stream → `s3_request_errors{op="get",class="transient"}`
  stays 0 and `class="terminal"` increments — the exact transient-vs-terminal distinction the
  patch's own comments (and the brief's Depends-on contract: "consume, never re-derive")
  say the counter exists to carry. The seam error is available in the `Err(e)` arm at `:510`
  and could be run through `wyrd_traits::classify` there; the added test only injects a fault
  at head time (`FaultyGateway` fails before the head is built), so this path is untested and
  the green suite cannot notice.

- NEEDS-HUMAN — **The red leg is a compile-failure red, not the assertion red the brief
  claims.** The added test hard-references patch-introduced APIs
  (`S3Gateway::with_metrics_dispatch`, `DServer::with_metrics_dispatch`,
  `DurabilityTelemetry::metrics_dispatch` — `crates/server/tests/request_capacity_planes.rs:1366,1604,1296`),
  so with production reverted the test target does not compile; `run-verify.sh` scores any
  non-zero `cargo test` exit as red (`engine/scripts/run-verify.sh:391-431` — the zero-tests
  guard only protects the exit-0 path). C4-verify's "PASS — red without the fix, green with
  it" is therefore technically true but degenerate: *any* test calling a new API earns the
  same red, and the brief's falsifiability claim ("the read-back contains none of the …
  families and the assertions fail") was never actually demonstrated. Mitigation: the green
  leg's assertions are genuinely discriminating (exact per-op counts, class split,
  gauge-returns-to-zero), so the residual risk is confined to the evidence's strength, not
  an identified defect — but the human should know the red→green proof is weaker than the
  gates row reads.

- NEEDS-HUMAN — **Per-connection sheds stay silent; `capacity_requests_shed` can read 0
  during a real overload.** The counter observes only the server-wide bound; the secondary
  per-connection cap (`crates/server/src/dserver.rs:671-672` —
  `.concurrency_limit_per_connection(..)` + `.load_shed(true)`) is applied by tonic in
  `MakeSvc::call` *outside* the user layer stack (verified: `tonic-0.14.6`
  `src/transport/server/mod.rs:1234-1239`), so its `Overloaded` never reaches
  `ShedObserver`. Concrete case: one client fanning 65 concurrent RPCs down one connection
  with the per-conn cap at 64 gets a `RESOURCE_EXHAUSTED` while the shed counter — and the
  admitted counter — record nothing. The code discloses this honestly
  (`dserver.rs:322-323`), but the brief's descope list does not, and an operator alerting on
  `capacity_requests_shed` will read "no shedding" in exactly this overload shape. Whether
  that gap is acceptable for the floor (or wants the per-conn cap moved into the observed
  stack) is an architecture/fitness call, not an iteration.

- **(unmarked, minor)** The role-entry glue is executed by no test: the S3 tests compose
  `S3Gateway::new(..).with_metrics_dispatch(..)` directly rather than driving
  `cli::serve_s3` / `run_d_server`, so the actual wiring at
  `crates/server/src/cli.rs:1403` and `:1997` (and the `ExporterConfig` selection in
  `cmd_s3`/`cmd_d_server`) would stay green if regressed to `None`. The composition the
  tests stand up mirrors the cli's ~4 glue lines faithfully today; noted as residual
  coverage, not a defect.

## Refutations attempted that did not land

- **Layer-ordering / "admitted means admitted".** Verified against tower + `tonic-0.14.6`:
  first `.layer()` is outermost, `GlobalConcurrencyLimit` acquires its permit in
  `poll_ready` before inner `call`, so `AdmissionObserver::call` does imply a held slot;
  `ShedObserver` outside `LoadShedLayer` does see the server-wide `Overloaded`. Could not break it.
- **Timeout vs. cancellation attribution.** `AdmissionGuard::started` is stamped during
  `GrpcTimeout::call`'s struct-literal evaluation, before the `sleep` is created
  (`tonic-0.14.6` `src/transport/service/grpc_timeout.rs:58-61`), so
  `elapsed >= request_timeout` at deadline-fire is guaranteed and the test's
  `cancelled == 0` assertion is deterministic, not a race. Could not break it.
- **In-flight gauge latching under concurrency.** The mutex spans the *emission*, and the
  tracing→OTel bridge records synchronously in `on_event`, so two concurrent finishes
  cannot emit levels out of order. Could not break it.
- **Preregistration tautology.** The latency assertions demand *exact* sample counts and
  the histogram is deliberately not preregistered, so a series minted at registration time
  cannot satisfy them; the `class="terminal" == 0.0` assertion survives the preregistered
  zero. Could not break it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite incomplete full-CI reproduction — all five binding tests pass, but `cargo xtask ci` reached `cargo deny check` and failed because its advisory lock path is read-only, so the asserted all-checks-green row could not be independently affirmed.
- [x] T5 Judgment — Decide whether unary chunkstore RPCs justify deferring a distinct transport-stream gauge and whether rebalance-pass cadence is sufficient for the already-merged domain-utilization signal — accepting this scope closes item 5 without those separate signals.
- [x] Validation — fitness-to-purpose — Decide whether the in-process Prometheus evidence is sufficient for deployment fitness — it proves emission and read-back at `crates/server/tests/request_capacity_planes.rs:320`, but does not exercise the supplementary live scrape/OTLP operational path.
- [ ] **A transient mid-stream body failure is counted `class="terminal"`.**
- [ ] **The red leg is a compile-failure red, not the assertion red the brief
- [ ] **Per-connection sheds stay silent; `capacity_requests_shed` can read 0

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Scope the rebuild to ONE defect (adversary impl finding): a transient mid-stream body failure on a streaming S3 GET is counted class="terminal" because the RED class is captured once at head time (crates/gateway-s3/src/lib.rs:653-655, None => Terminal); the body wrapper marks the op errored only later. Fix: classify the seam error in the body wrapper's Err arm (crates/gateway-s3/src/lib.rs:510-514, truncated at :521-522) through the existing wyrd_traits::classify — consume, never re-derive, per the 577 contract. Add a mid-stream fault-injection test (fault raised inside the GET body stream after the 200 head, e.g. d-server dying mid-read): assert s3_request_errors{op="get",class="transient"} increments and class="terminal" does not — the current suite only injects faults at head time, so this path is untested. Do NOT expand scope. Reviewed and deliberately excluded from this iteration: the compile-failure red leg (structural to new-feature bundles; carried as a §10 Act candidate — gate-level fix), and the silent per-connection sheds (behavior-affecting layer move, outside this brief's emission-only constraint; disclosed in code and covered by getwyrd/wyrd#584). All other verdicts stand: C1-C3, C5, T1-T4 pass; §6 items 1-3 cleared by the human (C4 full-CI replication gap = reviewer sandbox artifact, T5 descopes tracked in #584, live-scrape validation tracked in #585).
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_575: T5 descopes tracked as getwyrd/wyrd#584 (Foundations milestone) — concurrent-stream gauge deferred until a streaming RPC exists, plus the domain-utilization cadence question.
- issue_575: live-scrape validation gap tracked as getwyrd/wyrd#585 — extend deploy/small-multi-node-fdb compose with a Prometheus server as the standing environment for off-Check observability evidence.
- issue_575: run-verify.sh scores a compile-failure as red the same as an assertion-red, so new-feature bundles (test references new APIs) earn a degenerate red leg — have the gate distinguish and report compile-fail red vs. assertion red.
- issue_575: reviewer sandbox mounts the cargo-deny advisory-db/lock path read-only, so the reviewer's independent `xtask ci` replication dies at `cargo deny check` and C4 full-CI can't be independently affirmed — fix the sandbox (writable advisory path or pre-fetched db) so future reviews don't raise this NEEDS-HUMAN every cycle.
