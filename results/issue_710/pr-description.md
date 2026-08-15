## Summary
**User impact:** A repair or drain operation could grow an object's placement
record past the size the storage backend allows, without ever refusing the
write. On a backend that enforces its own size limit, that repair or drain
just fails with an opaque error that looks like a transient hiccup. On a
backend that doesn't enforce it, the oversized record gets written
successfully — and from then on, *every* future repair of that object fails,
because every repair has to rewrite the whole record and the backend will
never accept it again. Separately, a drain that failed to actually move a
fragment off a server could still be reported as complete, which could lead
an operator to decommission a server that was still holding data.

This PR makes both write paths check the record size before writing, refuse
the write cleanly if it would be too big, and stop reporting a drain/repair
pass as successful when a move didn't actually happen.

## What to look at
The core addition is one small helper, `flat_value_ceiling_crossed`, that
checks an already-encoded record against the backend's known size limit
before it's written. Both callers — the repair loop and the drain
(evacuation) loop — now call this helper immediately before their commit and
back out cleanly (writing nothing at all: no fragment copy, no record) if
the record would be too big. Separately, both loops now treat "the move
didn't happen" (refused, aborted, or lost the underlying compare-and-swap)
as one fact: the pass can no longer report success while a fragment is still
sitting on the server it was supposed to leave.

To exercise it: `cargo test -p wyrd-custodian --test placement_ceiling`
seeds a record right at the size limit, forces a repair/drain to grow it
past that limit by moving a fragment onto a large server id, and checks the
record is untouched, the obligation stays queued, and the pass does not
report success.

## Root cause
Every placement-maintaining write in this codebase is a compare-and-swap on
the whole encoded record (`require(prior) + put(next)`), but no write path
weighed the re-encoded record against the backend's value ceiling
(`MAX_VALUE_BYTES`, `crates/core/src/metadata.rs:327`) before committing it.
Separately, the drain loop's `EvacOutcome::Aborted => {}` arm was silent, so
a pass that moved nothing still reported the drain as satisfied.

## Fix
- `crates/core/src/metadata.rs:356-384` — adds `flat_value_ceiling_crossed`,
  a pure check of already-encoded bytes against `MAX_VALUE_BYTES`, admitting
  a record landing exactly on the ceiling and refusing only past it.
- `crates/custodian/src/rebalance.rs:508-527` and
  `crates/custodian/src/reconstruction.rs:915-929` — both call the new
  check on the exact bytes they are about to commit, and return a new
  `Refused { bytes, ceiling }` outcome before any fragment copy or metadata
  write happens, after the existing transient checks (missing/off-fleet
  fragments) so a compound failure is still named by its recoverable cause.
- `crates/custodian/src/rebalance.rs:159-165` (`EvacOutcome::persisted`) —
  a move that didn't land (refused, aborted, or lost its CAS) now withholds
  `Reconciled::Satisfied` for the whole pass, replacing the previously
  silent `Aborted => {}` arm.
- `crates/custodian/src/reconstruction.rs:1141-1148` and
  `crates/custodian/src/rebalance.rs:664-673` — the new refusal is counted
  on its own metric and folded into the existing documented success
  identity (`repaired - conflict - aborted`, now `- ceiling_refused` too),
  so it can never inflate the reported success count.
- `crates/custodian/tests/rebalance.rs:963-975` and `:1336-1350` — two
  existing tests that pinned "no move still counts as satisfied" are
  updated to the new, correct expectation.

## Verification
- **Claim:** a repair/drain that would grow a placement record past the
  backend's value ceiling is refused before anything is written, the
  obligation stays queued, and the pass does not report success.
  **Checked:** `crates/core/src/metadata.rs:356-384` (the ceiling check,
  admits exactly at the limit, refuses past it),
  `crates/custodian/src/rebalance.rs:508-527`,
  `crates/custodian/src/reconstruction.rs:915-929` (both call sites, before
  any write).
  **Test:** `crates/custodian/tests/placement_ceiling.rs` —
  `a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted`
  and `an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain`
  fail on `main` (oversized record commits / drain reports satisfied) and
  pass with this change.
- **Claim:** a move that didn't persist (refused, aborted, or a lost
  compare-and-swap) never lets the pass report the drain as satisfied.
  **Checked:** `crates/custodian/src/rebalance.rs:150-165`
  (`unmoved`/`EvacOutcome::persisted`).
  **Test:** `crates/custodian/tests/rebalance.rs:963-975` and `:1336-1350`,
  plus `an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain`
  in `placement_ceiling.rs`.
- **Claim:** a refused move is never counted as a successful repair.
  **Checked:** `crates/custodian/src/reconstruction.rs:1141-1148`
  (`emit_ceiling_refused`, joins the `repaired - conflict - aborted -
  ceiling_refused` identity).
  **Test:** `placement_ceiling.rs` —
  `a_ceiling_refused_repair_is_subtracted_from_the_reported_successes`,
  which mixes one repaired, one refused, and one aborted chunk in a single
  pass and checks the identity holds.

Fixes #710
