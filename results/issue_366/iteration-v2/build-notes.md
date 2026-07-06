# Build notes — issue 366 / obs-floor-observability (keystone slice: items 1–2, iteration 2)

**Target branch:** `getwyrd/wyrd @ feat/m4-production-metadata-backend`
(worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`). All `path:line`
citations are against that tree, post-edit.

## What changed from iteration 1 (addressing the carry-forward)

Iteration 1 delivered the day-one signal, but sign-off rejected it and named two required
changes (brief.md:194–197). This iteration does **both**, keeping the day-one signal green
through the new wiring:

1. **Extract the telemetry seam into a shared `crates/telemetry` crate.** The seam is no
   longer anchored in `custodian`. `DurabilityTelemetry` / `ExporterConfig` / `TelemetryError`
   / `metrics_layer` / `gather_prometheus` now live in the new `wyrd-telemetry` crate
   (`crates/telemetry/src/lib.rs:78,126,145`), added as a workspace member
   (`Cargo.toml` members + `wyrd-telemetry` workspace dep). `custodian` depends on it and
   **re-exports** so every M3 consumer that names `wyrd_custodian::DurabilityTelemetry`
   compiles unchanged (`crates/custodian/src/lib.rs:46`). This is the maintainer decision
   T5(a) the carry-forward records (0010 Open questions → extract), and it means the
   request-plane (item 4) / capacity-plane (item 5) consumers and the `server` role can share
   one export path rather than forking a second (0010 invariant: *reuse, don't rebuild*; *no
   backend leaks into a leaf crate*).

2. **Deliver the deployable custodian process in the `server` crate.** `server` now depends on
   `custodian` + `telemetry` (`crates/server/Cargo.toml`) — it is the one crate that wires
   concretes (ADR-0010) — and owns the runnable role:
   - `CustodianService` (`crates/server/src/custodian.rs:52`) **installs the telemetry handle
     at role entry** (`new`, `:66`, builds the `tracing`→OTel dispatch once over the handle's
     provider) and **runs the leader-elected loop through it** (`reconcile_pass`, `:89`,
     delegates to the real fenced `reconcile_step` — the anti-#141 single control point —
     with the handle's metrics bridge installed scoped for the pass).
   - A **continuous run loop** `run_until` (`:123`) reconciles once per interval until Ctrl-C
     — the spine the binary drives.
   - The `wyrd custodian` **subcommand** (`crates/server/src/cli.rs:144,488`) constructs the
     service from operator flags (`--zone`, `--endpoints`, `--otlp-endpoint`, `--interval-secs`),
     campaigns for single-active leadership, wires the reconstruction plane over the configured
     D-server fleet, and runs `run_until` to Ctrl-C. This is the `wyrd custodian --otlp-endpoint …`
     bring-up command the M4 day-one blueprint makes true, and #366 is its sole owner
     (2026-07-04 decision). The custodian is no longer a `dst`-only dependency and the server
     no longer "runs no custodian sweep".

The **gauge fix** carried over from iteration 1 (accepted at C1–C5, `crates/custodian/src/
reconstruction.rs:516`): the under-replicated count is a *level*, so `emit_under_replicated`
emits a `gauge.` field, not a `monotonic_counter.` — only a gauge returns to zero through an
accumulating Prometheus registry (a counter exports as `..._total` pinned at 1). The DST
property's capture key follows (`crates/dst/tests/custodian.rs:1023,1046`).

## Why this shape (and why not the rejected iteration-1 shape)

The carry-forward is an **invariant-to-restore** brief: "the durability signal must be
observable through a *runnable process*, and the telemetry seam must not be anchored in
`custodian`." So the target is the smallest change that restores those two invariants, not the
smallest diff.

- The role lives in **`server`**, not `custodian`, because ADR-0010 says `server` is the one
  crate that knows concretes; the deployable process (leadership campaign, fleet composition,
  export-backend selection, run loop) is exactly that composition. Iteration 1 put a
  library-only `CustodianRole` in `custodian` and left the deployable half deferred — that is
  what T4/the sign-off rejected as "not the library alone" being met only nominally.
- The bridge is installed **scoped** per pass via `WithSubscriber` (`custodian.rs:105`), not a
  global `set_global_default`, in `CustodianService` (a library seam). The process-global
  install belongs to the binary; `crates/server` is deliberately **absent** from the ADR-0035
  statics-scan list (`xtask/src/main.rs:636,640`), so the binary entry *may* install a global
  subscriber later (item 3), but the reusable service seam stays gate-clean. The dispatch is
  built once and `Arc`-cloned per pass, so one callsite-interest registration covers the role.

### Alternatives considered and rejected

- **Keep the telemetry seam in `custodian`, re-export from there** (iteration-1 shape).
  Rejected by the recorded maintainer decision (brief.md:195, T5(a)); it forces the future
  request/capacity/server consumers to depend on `custodian` (the maintenance-plane crate)
  just to emit telemetry — an inverted dependency. Extraction cost is one new 28-line manifest
  + a moved file (git tracks it as a rename, `crates/custodian/src/telemetry.rs → crates/
  telemetry/src/lib.rs`, 40 lines of doc/context changed, zero logic changed).
- **Put the runnable role's loop behind a new parallel entry point** rather than delegating to
  `reconcile_step`. Rejected: that forks the fenced control point (the #141 hazard). `reconcile_pass`
  adds *only* `.with_subscriber(dispatch)` around the real `reconcile_step` (`custodian.rs:105`).
- **Install a global fmt/EnvFilter subscriber + `/metrics` scrape endpoint in the binary now.**
  Rejected as out-of-keystone: `--log-level`/`RUST_LOG` structured logging (0010 item 3) and the
  `tonic-health` readiness / scrape endpoint (item 7) are their own floor slices. The binary
  wires the OTLP push surface (`--otlp-endpoint`, the production day-one path) and the in-process
  Prometheus registry; the live scrape is the DEFERRED off-Check evidence (brief.md:67–70).
- **Compose the live D-server fleet + failure-domain topology from etcd discovery.** Rejected /
  deferred: cross-process single-active election and dynamic discovery await an etcd-backed
  `Coordination` behind the same seam (ADR-0006) — a composition swap, the *other* half of
  0015's prerequisite, explicitly out of scope (brief.md:106). `cmd_custodian` uses the
  in-process `MemCoordination` (same documented limitation as `wyrd d-server`) and derives one
  failure domain per configured endpoint (the fan-out placement discipline the CLI already
  proves), so the binary is honestly runnable; the live multi-node run is the #367 gate.

## Test — red → green, driving PRODUCTION code (headless)

`crates/server/tests/custodian_day_one.rs`
(`under_replicated_gauge_rises_then_returns_to_zero_through_the_wired_role`, `:298`).
It builds a `CustodianService` over a Prometheus telemetry handle, kills D-server 1
(architecture §7.4 day-one step 4), runs two fenced passes through the **production**
`CustodianService::reconcile_pass` — the *same* method the `wyrd custodian` binary's `run_until`
loop calls — and reads `reconstruction_under_replicated` back off `service.telemetry().
gather_prometheus()`: pass 1 → **1**, pass 2 → **0**. The path exercised is
`CustodianService::reconcile_pass → reconcile_step → reconstruction::reconcile →
emit_under_replicated → DurabilityTelemetry → gather_prometheus` — all production. The
`MemMeta`/`MemDServer`/`Fleet` are the standard in-memory trait doubles the whole custodian
suite uses, not a re-implementation.

**Placement.** In the `server` crate (its own test binary), because the role under test now
lives there and the binding criterion reads the **real** OTel Prometheus provider back — which
`crates/dst` deliberately avoids under madsim (it uses a bespoke `MetricCapture`). It runs
headless in `cargo test --workspace` (no GUI/network/display; in-memory stores only).

**Red → green (verified through `cargo test`, the project's runner; the wrapper is
`./engine/xtask.sh ci` per `pdca.toml`):**
- *Green (with the fix):* `cargo test -p wyrd-server --test custodian_day_one` → `1 passed`.
- *Red (emit reverted to `monotonic_counter.`, role/test kept):* the metric exports as
  `reconstruction_under_replicated_total` (a counter), so the un-suffixed gauge query is `None`
  and the pass-1 `Some(1.0)` assertion fails first (`left: None, right: Some(1.0)`), reproduced
  and restored. Two independent reds: this assertion red, **and** a compile red pre-patch
  (`CustodianService` does not exist).
- DST property 6 under madsim (`RUSTFLAGS=--cfg madsim … cargo test -p wyrd-dst --test custodian
  durability_emission_rises_then_returns_to_zero`) → `1 passed` with the renamed `gauge.` key.

## Commit-readiness

- `cargo fmt -p wyrd-telemetry -p wyrd-custodian -p wyrd-server -- --check` → clean (rustfmt run).
- `cargo clippy -p wyrd-telemetry -p wyrd-custodian -p wyrd-server --all-targets` → clean, no warnings.
- `cargo build --workspace` and `cargo test --workspace --no-run` → clean (whole tree + all test
  binaries compile with the new manifest topology).
- **No new third-party dependency enters the graph:** `wyrd-telemetry` uses the *same*
  already-approved deps the workspace already carried via `custodian` (opentelemetry / prometheus
  / tracing-*), and `server`'s new deps are the workspace crates `wyrd-custodian` / `wyrd-telemetry`
  + `tracing` / `tracing-subscriber` (already in the graph). So the ADR-0003 / `deny.toml` audit
  surface is unchanged — no NEEDS-HUMAN for a new dependency.
- The full `cargo xtask ci` (C4-ci gate: fmt/clippy/build/test/deny/conformance + DST sweep) was
  not run end-to-end here (it sweeps 50 DST seeds); the targeted fmt/clippy/build/test above cover
  the touched surface, and the gate re-runs it at Check.

## Pre-existing flake to be aware of (NOT introduced here)

The custodian `reconstruction` and `rebalance` test binaries carry one telemetry test each in a
shared binary with sibling tests that hit the same `emit_*` callsites under no subscriber. Under
default multi-thread `cargo test`, `tracing`'s process-global per-callsite **interest cache** can
race and drop the telemetry test's metric (issue #214 — the reason `gc_telemetry.rs` /
`backfill_telemetry.rs` are already isolated into their own binaries). I observed an intermittent
fail of `reconstruction::emits_the_three_repair_metrics…` / `an_aborted_repair…` and
`rebalance::emits_per_failure_domain_utilization…`. **Verified pre-existing:** I stashed my entire
change (tree at base) and reproduced the identical intermittent fail on the base branch
(`an_aborted_repair_is_not_counted_as_a_successful_repair`, 1-of-5 runs). My change touches neither
test file nor any emit callsite's interest behaviour (counter-vs-gauge is orthogonal to callsite
caching), so it neither introduces nor worsens the race. Flagging it so a single red run at C4-ci
is recognised as this known flake, not a regression; isolating those two tests into their own
binaries (the #214 fix) is a separate cleanup, not this keystone.

## Known NEEDS-HUMAN / carry-forward for later slices

- **Milestone decomposition.** This bundle carries the keystone (items 1–2): the extracted seam +
  the runnable process + the day-one signal through it. Items 3 (EnvFilter logging), 4 (request
  RED), 5 (capacity signals), 6 (typed errors), 7 (`tonic-health` readiness) are follow-on slices.
- **Typed-errors × M4.4 (#255) sequencing** (0010 item 6) — must be *recorded* before the two run
  in parallel; untouched here (human call).
- **Live-exporter evidence is off-Check** — the real Prometheus scrape / OTLP collector run on a
  Tier-2 node against the day-one checklist is the pre-agreed sign-off item (brief.md:180–182), not
  a surprise; this bundle proves the in-process `gather_prometheus` read-back and ships the OTLP
  push wiring.
- **Adversary carry-over (iteration 1):** the gauge reads 0 for a chunk that has lost more than `m`
  fragments (`Assessment::Unrepairable` emits nothing) — "gauge = 0" does not distinguish healthy
  from below-`k` unrecoverable. That is an existing property of the M3 emission the gauge change
  surfaces, not new logic; a distinct "unrecoverable" signal is a follow-on floor refinement worth
  a human note.
- **Cross-process leadership / dynamic discovery** — the binary uses in-process `MemCoordination`
  (documented, same as `d-server`); the etcd-backed `Coordination` + gateway process role is the
  other half of 0015's prerequisite, its own body of work (brief.md:106).
