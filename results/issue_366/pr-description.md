# Add a deployable data-repair custodian with durability metrics

## Summary
**User impact:** An operator running Wyrd in production had no way to see
whether their stored data was healthy. The background worker that rebuilds
lost redundancy could only be called as a library — there was no process to
deploy, no health/metrics surface installed, and no signal to watch. If a
storage node died, nothing told the operator the system had noticed the loss
or repaired it; the durability of their data was effectively invisible.

This PR makes `wyrd custodian` a real, deployable process: it elects a
single active leader per zone, installs a telemetry handle at startup, runs
the reconstruction loop against the configured metadata backend and storage
fleet, and exports a durability metric that **rises when a node is killed and
returns to zero once redundancy is restored**.

## What to look at
- **The runnable role** — `crates/server/src/cli.rs`, `cmd_custodian`:
  argument parsing, backend open, leader election, fleet connect, and the
  reconstruction loop. This is the process entry a deployment launches with
  `wyrd custodian --otlp-endpoint … --endpoints … --ids … --failure-domains …`.
- **The live fleet view** — `crates/server/src/custodian.rs`,
  `live_reconstruction_view`: probes each configured D-server's reachability
  each pass and repairs *around* an unreachable peer.
- **The signal split** — `crates/custodian/src/reconstruction.rs`, `assess`
  and the `emit_*` helpers: how each durability condition is classified and
  which metric it lands on.
- **Reproduce it:** `cargo test -p wyrd-server --test custodian_day_one`
  drives the process end-to-end through `cmd_custodian`; the "kill a
  D-server → rise then return to zero" drill is the load-bearing case.

## Root cause
The durability-telemetry seam (`DurabilityTelemetry` / `ExporterConfig`) was
anchored in the custodian library with no runnable caller, no binary
installed a `tracing` subscriber or telemetry handle, and the under-replicated
count was emitted as a monotonic counter — which can only climb, so it could
never express "repaired back to healthy". Nothing wired the repair loop to a
deployable process, so the day-one durability signal did not exist end to end.

## Fix
- Extract the telemetry seam into a shared, backend-agnostic `wyrd-telemetry`
  crate (re-exported from `custodian` so existing callers are unchanged), and
  wire `wyrd custodian` in the `server` crate as a leader-elected, deployable
  process that installs the handle and runs reconstruction over the real
  metadata backend.
- Make the under-replicated metric a **gauge** (a level), so it returns to
  zero once redundancy is restored.
- Route each non-repairable-now condition onto its own distinct signal
  instead of the backlog gauge, so a permanent or transient condition can
  never pin the day-one signal above zero:
  - **confirmed data loss** (a chunk below its recoverable fragment count) →
    a dedicated high-severity data-loss counter + a needs-attention audit line;
  - **transiently unreachable node** (rolling restart / partition) → a
    distinct lower-severity level, so a routine restart does not raise a false
    data-loss alarm;
  - **repair with no free failure domain** to place a rebuild → its own level,
    cleared when capacity returns;
  - **malformed placement** → its own counter + audit line.
- Fail loud on startup if the *entire* fleet is unreachable (a supervisor can
  restart it) instead of exiting silently, while still starting degraded and
  repairing around a single down peer.
- Purely additive: no commit protocol, consistency contract, or on-disk
  format is changed.

## Verification
- **Claim:** after a killed D-server the under-replicated durability count
  rises, then returns to zero once redundancy is restored — observable through
  the process's Prometheus export.
  - **Checked:** `crates/custodian/src/reconstruction.rs` — `emit_under_replicated`
    emits a `gauge` (not a counter), fed only by the auto-repairable set in
    `reconcile`/`assess`; read back via
    `wyrd_telemetry::DurabilityTelemetry::gather_prometheus`.
  - **Test:** `crates/server/tests/custodian_day_one.rs` — the day-one kill
    drill drives `cmd_custodian` end to end and asserts the gauge goes 1 → 0.

- **Claim:** the "returns to zero" shape holds on a *populated* store — a
  pre-existing malformed chunk or a permanent, un-reconstructable loss does
  not floor the gauge above zero.
  - **Checked:** `crates/custodian/src/reconstruction.rs`, `assess` — malformed
    placements and below-recoverable losses are classified onto their own
    signals (`emit_needs_human`, `emit_data_loss`), never the under-replicated
    gauge.
  - **Test:** `crates/custodian/tests/reconstruction.rs` —
    `under_replicated_gauge_excludes_malformed_so_it_returns_to_zero` and
    `under_replicated_gauge_excludes_unrepairable_data_loss_so_it_returns_to_zero`
    drive two passes over a populated store and assert 1 → 0 while the distinct
    signals fire.

- **Claim:** a transiently unreachable node (rolling restart / partition) does
  not raise a false permanent-data-loss alarm and recovers when the node
  returns.
  - **Checked:** `crates/custodian/src/reconstruction.rs`, `assess` — a
    below-recoverable shortfall explained only by servers dropped from the
    live view this pass classifies as `Unreachable` (its own lower-severity
    level), not `Unrepairable`; the dropped set is supplied by
    `live_reconstruction_view` in `crates/server/src/custodian.rs`.
  - **Test:** `crates/server/tests/custodian_day_one.rs` —
    `a_transient_below_k_outage_does_not_false_alarm_data_loss_and_recovers`.

- **Claim:** an all-unreachable fleet at startup fails loud (non-zero exit),
  not silently.
  - **Checked:** `crates/server/src/cli.rs`, `cmd_custodian` — the empty-fleet
    case panics with a diagnostic naming the endpoint count; the single-down-peer
    case still starts degraded via `connect_fleet`.
  - **Test:** `crates/server/tests/custodian_day_one.rs` —
    `cmd_custodian_fails_loud_when_the_whole_fleet_is_unreachable_at_startup`.

Full-suite check: `cargo xtask ci` (fmt, clippy `-D warnings`, build,
workspace tests, `cargo deny`, conformance) passes on
`feat/m4-production-metadata-backend`.

## Scope and known follow-ups
This is the keystone slice of the observability floor: the deployable
custodian, the shared telemetry crate, and the durability signal. It is
additive to the M4 integration branch and does not change any backend's
production status. Two operator-facing notes for reviewers and the deployment
runbook:
- Which metric moves on a node kill is capacity-dependent: with spare
  failure-domain capacity the kill shows as the under-replicated gauge rising
  then returning to zero; on a minimum-width cluster the same kill surfaces on
  the repair-blocked level and stays there until capacity is added.
- Distinguishing a transiently-down node from a permanently-dead one requires
  the lease/membership backend tracked in #365; until it lands, a dead node
  reads on the unreachable level rather than the data-loss counter. Real
  cross-host single-active fencing on the TiKV backend also follows #365, and
  the process is honest in its logs about not enforcing it today.

Fixes #366
