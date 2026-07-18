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
