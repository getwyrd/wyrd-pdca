# Restore the reconstruction success identity for aborted repairs

## Summary

The durability-plane telemetry over-reports how many chunks the custodian
successfully repaired: every reconstruction pass that gives up on a plan it
cannot place still counts that plan as a success, so operators watching the
durability metrics see more repairs than actually committed. This change adds a
dedicated `reconstruction_aborted` counter and uses it to discount those failed
plans, so the metric the dashboards derive as "successful repairs" again equals
the number of chunks that were really committed.

## What to look at

- `crates/custodian/src/reconstruction.rs` — the `reconcile` outcome loop, where
  `RepairOutcome::Aborted` previously matched to `{}` (no metric) and now calls
  the new `emit_aborted`, plus the `emit_aborted` helper itself (it mirrors the
  existing `emit_conflict`).
- The reconstruction counter is emitted **once per plan, up front**, before the
  heavy erasure-decode/commit work — deliberately, because the tracing→OTel
  bridge can drop an event emitted after that section under load. The fix keeps
  that emission untouched and only adds a *late* offset for the aborted case, so
  the signal that matters is never the one at risk of being dropped.
- To exercise it: run a reconstruction pass over a plan set that yields at least
  one committed and one aborted outcome, then read back `reconstruction_repaired`,
  `reconstruction_conflict`, and `reconstruction_aborted` and check that
  `repaired − conflict − aborted` equals the committed count. The added test does
  exactly this against the Prometheus surface.

## Root cause

`reconstruction_repaired` is incremented up front for every plan the pass
attempts, and the documented contract derives successful repairs by subtracting
per-outcome offsets. The lost-CAS-race case was offset on
`reconstruction_conflict`, but the aborted case (the placement selector chose a
server outside the fleet view, so nothing committed) was offset by nothing — so
the derived success count was inflated by the number of aborted plans.

## Fix

Add a `reconstruction_aborted` counter and emit it from the `Aborted` arm of the
outcome loop, symmetric with the existing `reconstruction_conflict` offset. The
up-front `reconstruction_repaired` emission is unchanged, so the load-bearing
signal stays where the OTel bridge reliably captures it; only the offset is
emitted late, degrading identically to a dropped conflict offset if ever lost.
The documented success identity is updated in-code to
`reconstruction_repaired − conflict − aborted`.

## Verification

- **Claim:** after a reconstruction pass over a mix of committed, conflict, and
  aborted plans, the quantity the telemetry defines as "successful repairs"
  equals exactly the count of committed plans.
- **Checked:** `crates/custodian/src/reconstruction.rs:160-161` and `:433` (the
  documented identity, previously `repaired − conflict`) and `:172` (the
  `RepairOutcome::Aborted => {}` arm with no offset) on `main` — these are the
  sites that defined and broke the identity; the change updates the identity and
  offsets the aborted arm.
- **Test:** `crates/custodian/tests/reconstruction.rs` —
  `an_aborted_repair_is_not_counted_as_a_successful_repair` builds a single pass
  with one committed and one aborted plan, observes the committed count
  independently (one obligation drained, one still queued; one inode version
  bumped), and asserts `repaired − conflict − aborted == committed_count`. It
  fails before the fix (derived 2 vs committed 1, over-counted by the one aborted
  plan) and passes after. The assertion is written against the invariant, not a
  fixed metric value, so it stays valid regardless of how the offset is
  implemented.

Fixes #197
