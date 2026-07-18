# Design proposal — issue 576 / tonic-health-readiness-probes

> Plan artifact (design-proposal form — it adds a new wire surface and a new dependency,
> both design-buy-in items). The design is already ratified in proposal 0010
> (`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`,
> §"Scope boundary" item 7); this brief concretizes it for one Do cycle. Do reads ONLY
> this file (plus the cited peer callsites).

- **Slug:** tonic-health-readiness-probes
- **Kind:** enhancement (design proposal)
- **Goal:** a deployment supervisor (systemd, k8s, a load balancer) can ask a wyrd gRPC
  server "are you alive, and ready to serve?" over the standard gRPC health protocol
  (`tonic-health`, `grpc.health.v1`), with readiness reflecting the storage backend's own
  `health()` — an unhealthy store flips the probe instead of silently serving errors.
- **Success criterion:** against a served d-server (the workspace's gRPC role), a
  `grpc.health.v1.Health/Check` (a) reports SERVING while the backing store's
  `ChunkStore::health()` is `Health::Healthy`; (b) reports NOT_SERVING within a bounded
  wait once the store reports `Health::Unhealthy` OR once `health()` returns `Err`
  (fail-closed — both asserted); and (c) still answers (rather than being shed with
  `RESOURCE_EXHAUSTED`) while the data plane is saturated at its admission bound
  (`max_concurrent_requests` held by an in-flight data RPC) — asserted by the named test
  over an in-process loopback connection at Check (C4-verify red→green); `cargo xtask ci`
  stays green.
- **Falsifiability:** the added test `crates/server/tests/health_probe.rs` is a plain
  `#[tokio::test]` loopback test (peers: `crates/server/tests/dserver.rs`,
  `crates/chunkstore-grpc/tests/round_trip.rs`), so it executes under the gate's bare
  `cargo test`. RED is real on the base toolchain: with production reverted, no health
  service is registered, so the Check RPC returns UNIMPLEMENTED and the assertion fails —
  and since the revert also removes the `tonic-health` dependency (Cargo.toml is a
  reverted production file), the RED leg fails to compile, which the gate equally counts
  as red. Prefer wiring the test so a *behavioural* red is at least conceptually the
  failure (Check-returns-UNIMPLEMENTED), but the compile-fail red is acceptable to the
  gate. No special environment needed.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 575
- **Ordering note:** wave 1 of this batch, alongside 577 (disjoint file sets). Conflicts
  with 575 because both edit `crates/server/src/dserver.rs` (this one adds the health
  service to the `serve` builder; 575 adds admission-event emission to the same layer
  stack) and likely `crates/server/src/cli.rs` — never build the two blind on one base.
- **Surfaces:** data
- **Difficulty:** medium — localized to `crates/server` (dserver.rs, possibly cli.rs
  flag plumbing) plus workspace/crate `Cargo.toml`s, `Cargo.lock`, and possibly
  `deny.toml`; a handful of files, but a new public wire surface and a new dependency.
- **Scope:** register the standard `tonic-health` service on the d-server's tonic server
  (`crates/server/src/dserver.rs:276-323`, the one gRPC role entry in the workspace — the
  `s3` role is HTTP, the custodian serves nothing); derive readiness from the store's
  `health()` at the `crates/traits` seam (`crates/traits/src/lib.rs:357` `Health`,
  `:408` `ChunkStore::health`) — `Healthy`/`Degraded` ⇒ SERVING, `Unhealthy` ⇒
  NOT_SERVING (a degraded store still serves; document the mapping); liveness = the
  process answers at all (tonic-health's default service status). Add the `tonic-health`
  workspace dependency version-matched to tonic 0.14 (root `Cargo.toml:160`). / out of
  scope: an HTTP health endpoint on the `s3` role (0010 item 7 names the standard gRPC
  protocol; the S3 front door's probe story is not in the floor), metrics emission
  (issue 575), typed errors (issue 577), any change to the `Health` enum or the
  `ChunkStore` contract itself, dashboards/alerting.
- **Repro instruction:** on `main` (e47cb88), `grep -rn "tonic-health\|tonic_health"
  crates/ Cargo.toml` → no hits; `crates/server/src/dserver.rs:323` `add_service`
  registers only `ChunkStoreServer`. A `grpc.health.v1.Health/Check` against a running
  `wyrd d-server` returns UNIMPLEMENTED; the only liveness signal is process existence.
- **External dependencies:** none
  (tonic-health is a cargo dependency — fetched by the build, not human-installed; the
  binding test is in-process loopback on the base toolchain.)
- **Test file:** crates/server/tests/health_probe.rs — a NEW file (the C4-verify gate
  earns its red leg only from an *added* `*/tests/*.rs` file; a co-located test degrades
  to green-only). Drive the real `DServer::serve` composition with a store whose
  `health()` the test controls, and query via `tonic_health`'s generated client.
- **Verification posture:** default — flippable red→green at Check via the named test.
  The 0010 DST-property phrasing ("an unhealthy store flips the readiness probe") is
  satisfied by this deterministic in-process test; do NOT put the binding test under
  `#![cfg(madsim)]` or a feature gate — the gate's bare invocation must compile and run
  it (`crates/dst` madsim shims swap tonic, and tonic-health binds real tonic).
- **Citations expected:** Do must cite path:line on `main` for every change. Peer
  callsites Do MAY open: the tonic server builder to extend —
  `crates/server/src/dserver.rs:276-323` (`serve`, `.add_service(...)` at `:323`); the
  `Health` seam — `crates/traits/src/lib.rs:357` / `:408`; the loopback-test shape —
  `crates/chunkstore-grpc/tests/round_trip.rs`.
- **Prior-art check (by affected file path):** merged history of
  `crates/server/src/dserver.rs` carries admission control (#8.9 work) and registration —
  no health service; no `tonic-health` anywhere in merged history, open PRs (only #578,
  disjoint files), or closed work. `health()` itself landed pre-M3 and is implemented at
  `crates/chunkstore-fs/src/lib.rs:320` with no probe consumer (0010 §Motivation,
  verified).
- **Disposition hint:** new-feature

## Motivation

Proposal 0010 §Motivation: "`health()` is implemented but exposed through no
readiness/liveness probe — so an orchestrator cannot tell which node is down." Floor item
7 closes it with the standard protocol, so any supervisor speaking `grpc.health.v1` works
without wyrd-specific tooling. Parent: getwyrd/wyrd#366 (floor item 5).

## Design

Per 0010 item 7: reuse the standard proto via `tonic-health` — "no bespoke service"
(§Crate touch-points, `proto`). Register the health reporter beside `ChunkStoreServer` in
`DServer::serve`. The **behavioural contract is pinned** (mechanism — Arc/clone plumbing,
poll vs per-check — stays Do's, but two incompatible implementations must not both pass):

- **Readiness source:** the store's `health()` (`crates/traits/src/lib.rs:407-408`; note
  `serve` currently moves the store into `ChunkStoreService`, `dserver.rs:286` — sharing
  it is Do's plumbing to solve, not a reason to probe a different store instance).
- **Mapping:** `Healthy`/`Degraded` ⇒ SERVING (a degraded store still serves);
  `Unhealthy` ⇒ NOT_SERVING; **`Err(_)` from `health()` ⇒ NOT_SERVING** (fail closed — a
  store that cannot even report health must not read as ready).
- **Freshness:** status refresh is bounded — an operator-visible cadence (a flag or a
  documented constant of a few seconds), not "whenever"; the state change must be
  observable within a bounded wait (what criterion (b) tests).
- **Startup / shutdown:** report NOT_SERVING until the first successful `health()` read;
  any refresher task ends with `serve` (no leaked task after shutdown).
- **Overload policy (decided here, not left open):** the health service **bypasses the
  data-plane admission layers** — probes are tiny and bounded, and readiness must stay
  answerable exactly when the node is under pressure; a probe shed as
  `RESOURCE_EXHAUSTED` makes supervisors restart an overloaded-but-healthy node and
  amplify the overload. The admission layers wrap everything on the current builder
  (`dserver.rs:288-323`), so Do must compose the health service outside that stack
  (mechanism its own); criterion (c) tests this. Note the deliberate semantic: overload
  is NOT unreadiness — it is what 575's shed events report.

The overall (empty-name) service is the liveness signal.

**Pre-declared NEEDS-HUMAN:** `tonic-health` is a **new dependency** — a project-defined
human-only item (INTEGRATION §4: ADR-0003 three-test audit + `deny.toml` allowlist;
`cargo deny check` runs inside `cargo xtask ci` and will adjudicate the license
mechanically). Expect this as a sign-off row, not a surprise.

## Alternatives considered

A bespoke health RPC on the chunkstore proto — rejected in 0010 (§Crate touch-points:
"reuse the standard gRPC health proto via tonic-health (no bespoke service)"); the entire
value is that standard supervisors already speak it.

## Impact & compatibility

Additive wire surface (0010 §Backward compatibility: "the tonic-health service is new and
standard"); data-path RPCs untouched. One new dependency, version-locked to tonic 0.14.
No on-disk, trait-contract, or consistency change.

## Open questions

- Whether readiness should also fold in coordination-lease state (beyond the store's
  `health()`) — out of the floor; note it in code if tempting, do not build it.
- ~~Probe vs. admission control~~ — **resolved at Plan** (adversarial review, 2026-07-17):
  the health service bypasses the data-plane admission layers; see Design. The decision is
  binding (criterion (c) tests it); if the maintainer prefers overload-reads-as-not-ready,
  that is a sign-off override, not Do's call.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected per advisory review C3/C5: the health service binds a second, ephemeral port exposed only via the in-process `health_endpoint()` getter, so a real supervisor (systemd/k8s/LB) has no stable or configurable address to dial — the operational root cause is not removed. Next attempt: keep the health semantics (mapping, fail-closed, admission bypass — all reviewed as sound) but give the health listener a stable/configurable bind address (cli.rs flag plumbing is already within the brief's scope) so the probe endpoint is discoverable by an operator, and exercise that configured address in the test instead of bypassing the deployment boundary via `health_endpoint()`.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
