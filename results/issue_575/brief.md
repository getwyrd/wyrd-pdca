# Design proposal — issue 575 / request-red-capacity-signals

> Plan artifact (design-proposal form — cross-crate instrumentation of the request and
> capacity planes). The design is already ratified in proposal 0010
> (`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`,
> §"Scope boundary" items 4–5); this brief concretizes it for one Do cycle. Do reads
> ONLY this file (plus the cited peer callsites).

- **Slug:** request-red-capacity-signals
- **Kind:** enhancement (design proposal)
- **Goal:** a live deployment shows *service* health, not just storage health: the S3
  gateway emits per-operation RED (latency + error-by-class), and the capacity plane
  emits admission admitted/shed/timed-out events and in-flight/stream gauges — all
  through the shared `wyrd-telemetry` seam. (0010 item 5's third signal, per-failure-
  domain utilization, is already merged — see Scope / Prior-art.)
- **Success criterion:** with the patch applied, asserted via in-process Prometheus
  read-back (`DurabilityTelemetry::gather_prometheus`,
  `crates/telemetry/src/lib.rs:145`): (a) driving an S3 gateway op (a PUT and a GET,
  plus one failing op) through the real router records a per-op latency measurement and
  an error counter **labelled by op and by the stable class label 577 exports** (see
  Depends-on contract in Design) — the test MUST fully drain the GET response body
  before reading back, because op completion is deliberately deferred to body
  completion (`crates/gateway-s3/src/lib.rs:460-466`); (b) driving a loopback d-server
  records, each asserted separately: an admitted event on the happy path, a shed event
  when driven past its admission bound (`max_concurrent_requests`,
  `crates/server/src/dserver.rs:100`), a timed-out event when a request exceeds
  `request_timeout` (both operator-tunable via `AdmissionControl`, `:95-127`, so the
  test can force each deterministically), and an in-flight RPC gauge that rises while
  requests are held open and **returns to zero** after they complete. Demonstrable by C4-verify
  at Check; `cargo xtask ci` stays green. A live Prometheus scrape / OTLP push is
  supplementary off-Check evidence only (0010 §DST and tests).
- **Falsifiability:** the added test `crates/server/tests/request_capacity_planes.rs`
  is a plain `#[tokio::test]` file (peers: `crates/server/tests/custodian_day_one.rs`
  for the read-back pattern, `crates/server/tests/s3_http_wire.rs` for driving the
  router), so it executes under the gate's bare `cargo test`. RED is real on the base
  toolchain: with production reverted, the read-back contains none of the request/
  capacity metric families (today NO per-op or admission metric exists anywhere —
  verified: `monotonic_counter`/`histogram` events exist only in custodian loops,
  `crates/core/src/read.rs:191-200`, and telemetry itself) and the assertions fail. No
  cluster, docker, or collector is needed for the binding criterion.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 577
- **Conflicts with:** 576
- **Ordering note:** wave 2 of this batch. Depends on 577 because 0010 item 4 keys the
  request-plane error counter "by the typed failure class from item 6" — this bundle
  builds on the folded wave-1 base carrying 577's enum (the C4-verify gate honours the
  driver's `$PDCA_VERIFY_BASE` for exactly this, `engine/scripts/run-verify.sh:186-193`).
  Conflicts with 576: both edit `crates/server/src/dserver.rs` (576 adds the health
  service to the same `serve` builder this bundle instruments) and likely
  `crates/server/src/cli.rs`.
- **Surfaces:** data
- **Difficulty:** high — cross-crate reach: `crates/gateway-s3` (per-op instrumentation),
  `crates/server` (cli.rs role wiring, dserver.rs admission emission), possibly
  `crates/core`; a diff reviewer must hold the whole telemetry composition in view.
- **Scope:** (1) request-plane RED at the S3 gateway ops — per-op latency + error
  counter keyed by op and typed failure class, emitted inside the existing `s3.request`
  span machinery (`crates/gateway-s3/src/lib.rs:433`, dispatch at `:477`, verb match at
  `:557`); (2) capacity-plane signals at the d-server — admitted / shed / timed-out
  events and an **in-flight RPC gauge** around the admission layer stack
  (`crates/server/src/dserver.rs:95-127`, `:287-323`), which today sheds **silently**
  (visible only as a client-side `RESOURCE_EXHAUSTED`); (3) the **metrics-provider wiring
  at the `s3` and `d-server` role entries** that (1)+(2) need to emit at all — mirror the
  custodian role's composition (see Design; this is a required enabler, verified missing).
  / out of scope: **per-failure-domain utilization — it already ships**
  (`crates/custodian/src/rebalance.rs:320-326` emits `gauge.capacity_domain_utilization`
  per domain on the custodian role, which has a metrics provider; do NOT re-implement it —
  see Open questions for the one residual human call); a separate **transport-stream
  gauge** — 0010 item 5 says "in-flight-request and concurrent-stream gauges", but every
  chunkstore RPC is unary (`crates/proto/proto/wyrd/v0/chunk.proto:80-87`), so in-flight
  RPCs and concurrent streams coincide in practice, and h2-level stream counting has no
  seam in the stack (`dserver.rs:109` merely passes a cap to tonic at `:317`) — descoped,
  flagged in Open questions; dashboards, alerting rules, tracing
  beyond the floor (span graph → future ADR, per 0010), the typed enum itself (577
  provides it; if a class label must be threaded, consume — never redefine — 577's types),
  health probes (576), any change to admission *behaviour* (emission only: what is
  admitted/shed must not change).
- **Repro instruction:** on `main` (e47cb88): `grep -rn "monotonic_counter\|histogram\."
  crates/gateway-s3 crates/server/src/dserver.rs` → no hits (no request/capacity
  emission); `crates/server/src/dserver.rs:287-323` sheds with no event; and
  `DurabilityTelemetry::new` has no caller outside `cmd_custodian`
  (`crates/server/src/cli.rs:836`) — so even an instrumented gateway would emit into a
  subscriber with no metrics provider in the `s3`/`d-server` roles.
- **External dependencies:** none
  (the telemetry stack — wyrd-telemetry, tracing-opentelemetry, the Prometheus registry —
  is already vendored, #450; the binding test is in-process on the base toolchain. The
  live scrape/OTLP leg is off-Check and human-run.)
- **Test file:** crates/server/tests/request_capacity_planes.rs — a NEW file (the
  C4-verify gate earns its red leg only from an *added* `*/tests/*.rs` file; a
  co-located test degrades to green-only). It must exercise the production composition
  (the real router / the real `serve` layer stack), not a hand-assembled copy — the
  `custodian_day_one.rs` discipline.
- **Verification posture:** default — flippable red→green at Check via in-process
  read-back (the M3 C4-verify pattern 0010 §DST names). Do not gate the test behind
  `#![cfg(madsim)]` or a feature.
- **Citations expected:** Do must cite path:line on the resolved base for every change.
  Peer callsites Do MAY open (composition slice — mirror, don't re-derive): the
  telemetry handle + exporter construction and scoped-dispatch composition of the
  custodian role — `crates/server/src/cli.rs:797-836` (`ExporterConfig` selection →
  `DurabilityTelemetry::new`) and `crates/server/src/custodian.rs:310`
  (`logging::dispatch(log, writer, telemetry.metrics_layer())`); the metric-event idiom —
  `crates/core/src/read.rs:191-200`; the read-back assertion pattern —
  `crates/server/tests/custodian_day_one.rs`.
- **Prior-art check (by affected file path):** merged: #450 extracted the shared
  `wyrd-telemetry` seam (`crates/telemetry`, commit e65cf69); #527/#531 installed the
  log subscriber at every role entry (`crates/server/src/cli.rs:358` → `init_global`)
  and #529 minted the S3 request id (`crates/gateway-s3/src/request_id.rs`) — enablers,
  not duplicates: none emit request-RED or admission metrics. **Two corrections to the
  tracker notes:** (i) "the sink already exists" holds for the custodian role only; the
  `s3` / `d-server` roles install the *log* layers with no metrics provider — hence Scope
  item (3). (ii) per-failure-domain utilization is ALREADY MERGED prior art —
  `crates/custodian/src/rebalance.rs:320-326` (`emit_domain_utilization`, cited to
  proposal 0005:341-343) emits `gauge.capacity_domain_utilization` per domain each
  rebalance pass — so it is out of scope here, not re-implemented. Open PRs: only #578
  (consistency-CI, files `crates/server/src/consistency_*` / `xtask` — disjoint from this
  bundle's set; do not touch those files). No closed/rejected work covers the request-RED
  or admission planes.
- **Disposition hint:** new-feature

## Motivation

The custodian's durability plane is watchable since #450/#527; the request and capacity
planes are dark — nothing reports failing/slow requests, and load-shed is silent
(0010 §Motivation). Items 4–5 are the floor's remaining emission work. Parent:
getwyrd/wyrd#366 (floor item 4).

## Design

Per 0010 items 4–5 and §Crate touch-points. Request plane: RED counters, not traces —
latency histogram + error counter per op, class label from 577's classification, emitted
via `tracing` metric events so the existing `MetricsLayer` bridge carries them (ADR-0012
dual-export, no backend hardcoded). **Depends-on contract (577):** 577's brief requires it
to export a public class *value* with a stable, bounded label form (not just boolean
predicates) — the counter's `class` label is that value's label form; consume it, never
re-derive classification locally. Capacity plane: admission events where the d-server
layer stack decides (mechanism for observing tower's load-shed is Do's — but behaviour
must not change) and in-flight/stream gauges. Per-failure-domain utilization already
emits on the custodian role (`rebalance.rs:320-326`) — reuse its naming/label conventions
(`domain` label) for consistency; do not duplicate it. Role wiring: give
the `s3` and `d-server` entries a telemetry handle + metrics layer exactly as
`cmd_custodian`/`custodian.rs:310` compose theirs, exporter selected by the same
`ExporterConfig` convention (`--otlp-endpoint` ⇒ Both, else Prometheus).

## Alternatives considered

Weighed in 0010: full tracing/span graph rejected for the floor; "adequate, not elegant —
RED counters" is the ratified shape. Emitting via direct OTel meters instead of `tracing`
events would bypass the one-instrumentation-path discipline (ADR-0012) the custodian set —
follow the established seam.

## Impact & compatibility

Purely additive instrumentation (0010 §"What carries over, unchanged"): no commit
protocol, consistency, or on-disk change; admission behaviour unchanged (emission only).
No new dependency expected (the OTel stack is vendored — if Do finds one is needed, that
is a declare-and-stop, not a workaround). Cardinality: keep labels to op + class +
domain — no per-key/per-tenant labels (0010 §Open questions).

## Open questions

- Whether the in-flight gauge is derivable without touching the tower layer ordering —
  Do's mechanism call; the invariant is that a forced load-shed is observable as an event,
  not just a client status code (0010 PR-sequence item 5 DoD).
- **For the human at sign-off:** the already-merged per-failure-domain utilization gauge
  (`rebalance.rs:320-326`) emits once per *rebalance pass*, not continuously. If 0010
  item 5 is read as wanting a steadier cadence, that is a separate follow-up issue — it
  is deliberately NOT in this bundle's scope, so 575 can close item 5's remaining gap
  (admission events + gauges) without re-opening merged work.
- **For the human at sign-off:** the separate concurrent-stream gauge is descoped (see
  Scope: all chunkstore RPCs are unary, so it duplicates the in-flight RPC gauge, and
  h2-level counting has no seam). If a true transport-stream signal is wanted later
  (e.g. once a streaming RPC exists), file it then — do not have Do invent a
  transport-level mechanism for a floor slice.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Scope the rebuild to ONE defect (adversary impl finding): a transient mid-stream body failure on a streaming S3 GET is counted class="terminal" because the RED class is captured once at head time (crates/gateway-s3/src/lib.rs:653-655, None => Terminal); the body wrapper marks the op errored only later. Fix: classify the seam error in the body wrapper's Err arm (crates/gateway-s3/src/lib.rs:510-514, truncated at :521-522) through the existing wyrd_traits::classify — consume, never re-derive, per the 577 contract. Add a mid-stream fault-injection test (fault raised inside the GET body stream after the 200 head, e.g. d-server dying mid-read): assert s3_request_errors{op="get",class="transient"} increments and class="terminal" does not — the current suite only injects faults at head time, so this path is untested. Do NOT expand scope. Reviewed and deliberately excluded from this iteration: the compile-failure red leg (structural to new-feature bundles; carried as a §10 Act candidate — gate-level fix), and the silent per-connection sheds (behavior-affecting layer move, outside this brief's emission-only constraint; disclosed in code and covered by getwyrd/wyrd#584). All other verdicts stand: C1-C3, C5, T1-T4 pass; §6 items 1-3 cleared by the human (C4 full-CI replication gap = reviewer sandbox artifact, T5 descopes tracked in #584, live-scrape validation tracked in #585).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
