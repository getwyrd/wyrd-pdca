# Deployed custodian runs garbage collection: deletes finally free disk space

## Summary
**User impact:** deleting or overwriting an object never frees any disk space on a
running cluster. The old data's fragments stay on the storage servers forever, so
disk usage only ever grows — every delete, overwrite, completed repair, and
rebalance quietly leaks the bytes it displaced.

This PR makes the deployed custodian role actually run garbage collection: orphaned
bytes are physically reclaimed once a reader-safe grace period has elapsed, and
never before.

## What to look at
The crux is the custodian's periodic maintenance loop, which gains a third,
separately fault-isolated garbage-collection step, and the `wyrd custodian`
command entry, which now refuses a fleet listing the same storage server twice
(a duplicated identity would trick the collector into deleting live data).

To try it: `cargo test -p wyrd-server --test custodian_gc`. The suite drives the
real deployed role over an in-memory fleet with a logical clock — write an object,
delete it, advance the clock past the grace window, run the role: the bytes are
gone; inside the window, or for a still-live object, nothing is touched.

Two safety postures this PR takes deliberately, for the maintainer to weigh:

- **Grace value is a floor, not a proven bound.** The 60 s window reuses the longer
  of the two pending-lease TTLs the system already trusts (60 s CLI, 30 s gateway),
  the same derivation the shipped restore pass uses. No reader version-hold
  mechanism exists yet, so no value can currently be *proven* reader-safe; the
  doc-comments say so explicitly.
- **Whole-fleet gate pauses under any outage.** GC runs only when every
  operator-configured server is visible; while any one is unreachable (or
  decommissioned but still listed in `--endpoints`), all reclamation pauses,
  indefinitely. A paused reclaim recovers in full on the next whole-fleet pass; a
  false "collected" is a permanent leak. Two known edges: the endpoint-uniqueness
  refusal is string-equality (two textual aliases of one box pass it — an
  operator-attested trust assumption until identity attestation lands), and a peer
  dropped at a degraded boot is not re-dialed, so recovery from a degraded start
  needs a custodian restart (follow-up candidate).

## Deployment sequencing
GC's expired-pending input treats the bare lease TTL as its grace, so a write that
merely *looks* expired against the custodian's clock (a lease stamped at logical
time zero, or a slow >30 s gateway PUT) is a mid-flight hazard on a shared backend.
**Do not run `wyrd custodian` against a shared write-taking backend before #490's
lease-conditional commit merges**; the residual mid-pass race is tracked as #557.
The hazard is surfaced in the code comments, not closed here.

## Root cause
Both `reconcile_pass` calls in the deployed run loop `run_reconstruction_until`
pass `None` where the `GcContext` goes (`crates/server/src/custodian.rs:442` and
`:456` on `main`); the only `GcContext` the server crate constructs
(`custodian.rs:350`) belongs to the post-restore pass merged in
dc503cd6d9d0b8bb2e3d64bb88a206a6857b52bb, which marks collectable fragments but
never deletes. A delete orphans fragments into the ledger for GC to reap
(`metadata::unlink`, `crates/core/src/metadata.rs:369-408`), so with no GC pass in
the run loop nothing is ever reaped — the gc module itself notes deployment was
deferred (`crates/custodian/src/gc.rs:61-63`).

## Fix
- `run_reconstruction_until` runs a third, distinct fenced GC pass after scrub and
  reconstruction (`crates/server/src/custodian.rs`), constructing a `GcContext`
  over the metadata store and the live fleet. Distinct so a GC store fault degrades
  only GC (never suppresses scrub/repair); ordered last so it works from the
  freshly committed placement and cannot reclaim a just-re-placed fragment.
- The pass is gated on `fleet.len() == operator_fleet_size`, threaded from the
  `--endpoints` count (`crates/server/src/cli.rs`): GC's expired-pending input
  retires chunk-wide evidence, so a partial sweep could strand a missing server's
  fragment forever. Gating on the operator count (not `unreachable.is_empty()`)
  also covers the startup-degraded case where `connect_fleet` already dropped a
  boot-unreachable peer. Deferral preserves all evidence for a later pass.
- Grace window derived, never a magic constant: `GC_GRACE_WINDOW_MILLIS =
  LEASE_TTL_MILLIS` (60 s), the longer trusted lease timescale, documented as a
  conservative floor; the restore pass's `RESTORE_GRACE_WINDOW_MILLIS` doc-comment
  is corrected to the same two-timescale rationale (it claimed one timescale and
  deferred a derivation this PR now ships).
- The duplicate `--endpoints` / `--ids` refusal is hoisted out of the
  `--reconcile-after-restore` block to run unconditionally before any dial: with a
  deleting pass now armed on the run-loop path, a box under two identities would
  leave a live fragment protected as one identity and unreferenced as the other —
  collectable. Refusing early on every path replaces two branch-local copies that
  could drift.

## Verification
- **Claim:** the deployed role (the same wiring `wyrd custodian` drives) reclaims
  an orphaned fragment's bytes once its grace deadline elapses, and a live object
  loses nothing.
  **Checked:** `crates/server/src/custodian.rs:442,456` on `main` — both run-loop
  passes hand GC `None`; `custodian.rs:350` — the only `GcContext`, marking-only.
  **Test:** `crates/server/tests/custodian_gc.rs`
  (`deployed_role_reclaims_orphaned_bytes_after_grace_elapses`) — fails on `main`
  (bytes remain forever), passes with the fix.
- **Claim:** nothing is reclaimed before the grace deadline, and the exact boundary
  instant (`orphaned_at + grace`) reclaims.
  **Test:** `deployed_role_keeps_orphaned_bytes_within_the_grace_window` and
  `deployed_role_reclaims_at_the_exact_grace_boundary` (fails if the boundary
  regresses from `>=` to `>`).
- **Claim:** a partial fleet view — one server unreachable mid-run, or dropped at a
  degraded startup — defers GC and preserves the missing server's orphan/pending
  evidence for a later whole-fleet pass.
  **Test:** `deployed_role_defers_gc_and_preserves_a_skipped_servers_evidence` and
  `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial`; the latter
  fails by assertion if the gate regresses to `unreachable.is_empty()`.
- **Claim:** expired pending-lease garbage (a crashed write's fan-out) is reclaimed
  through the deployed loop, not only delete-orphans.
  **Test:** `deployed_role_reclaims_expired_pending_lease_garbage`.
- **Claim:** the run-loop path refuses duplicated `--endpoints` / `--ids` before
  dialing.
  **Checked:** `crates/server/src/cli.rs:940-967` on `main` — the refusal exists
  only inside the `--reconcile-after-restore` block; the run-loop path has none.
  **Test:** `deployed_run_loop_refuses_duplicate_endpoints` /
  `deployed_run_loop_refuses_duplicate_ids`
  (`crates/server/tests/custodian_gc.rs:866,901`), driving the real
  `cli::cmd_custodian` entry — fail without the hoisted refusal, pass with it.
- **Gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, workspace tests,
  deny, conformance) green; the new suite is 8/8, and the sibling
  `custodian_day_one` (15) and `closed_write_path` (1) suites stay green.
  One honest caveat: because closing the startup-partial hole required
  `run_reconstruction_until` to learn the operator fleet size, a *full* revert of
  the production files turns this test file's red into a compile error rather than
  an assertion; the behavioural binding is shown by the gate-only and refusal-only
  regressions above.

Fixes #554
