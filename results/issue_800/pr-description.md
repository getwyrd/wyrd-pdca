## Summary
**User impact:** garbage-collection bookkeeping for object storage grows over
time and never shrinks. Every time the system repairs a fragment that went
missing, it leaves behind a small tracking record at that fragment's old
location — and nothing ever removes that record, even long after it's known
to be safe to remove. Over the life of a deployment this bookkeeping
accumulates without bound, making every GC pass slower to walk.

This change makes GC also delete those leftover tracking records once it can
prove, from its own listing of what's actually stored, that no write can ever
land there again.

## What to look at
The new logic lives in `crates/custodian/src/gc.rs`, in
`Sweep::sweep_fragment_less_marks` and `Sweep::commit_sweep`, called at the
end of each GC pass (`Sweep::run`). The easiest way to see the effect: mark a
storage position as orphaned when no fragment is actually stored there,
advance the clock well past the deadline, and run a GC pass — before this
change the tracking record survives forever; after it, the record is deleted
once it's safely past the point where a delayed write could still land.

## Root cause
GC only ever reaches a tracking record ("orphan mark") by way of a fragment
listing from a live storage server — so a mark whose position has no
fragment is invisible to the existing walk and is never cleaned up. Several
paths on `main` already write such marks (fragment repair moves marks to a
now-empty position), and the design behind this work adds more of them by
intent.

## Fix
Each GC pass now runs a second phase after its normal fragment walk: it
looks at every tracking record it read this pass, and deletes the ones whose
position no listing *from this same pass* reported — but only once the
record is older than a fixed deadline (`LATE_WRITE_DEADLINE_MILLIS`, 41
seconds), derived from the slowest write path plus a clock-skew allowance.
That deadline guarantees the listing was taken after the last moment a
delayed write could still land, so the delete rests on an observation of
"nothing is here," never on the record's age alone. Records on a server not
currently in the fleet, at a still-referenced position, or holding a value
of unrecognized shape are left alone. Deletes are conditioned on the exact
bytes read and batched, so a record rewritten concurrently survives, and a
delete is only counted once its commit is durable.

A compile-time check ties the new deadline to the existing storage grace
window, so the two can never be tuned past each other silently.

## Verification
- **Claim:** a tracking record whose position holds no fragment, once past
  the late-write deadline and confirmed empty by this pass's own listing, is
  deleted, audited, and counted.
  **Checked:** `crates/custodian/src/gc.rs`, `Sweep::sweep_fragment_less_marks`
  (deadline + protection checks) and `Sweep::commit_sweep` (conditional
  batched delete, claim only after a durable commit).
  **Test:** `crates/custodian/tests/gc_mark_sweep.rs` (new) — 16 cases,
  including the deadline boundary, protected/unlisted/out-of-fleet records,
  a concurrent rewrite racing the delete, a mid-batch commit fault, and a
  differently-encoded key that must never be treated as the same record.
  All fail on `main` (the record survives every pass); all pass with this
  change.
- **Claim:** the new deadline can never be configured to exceed the storage
  system's existing grace window.
  **Checked:** `crates/server/src/custodian.rs` — a compile-time assertion
  comparing `LATE_WRITE_DEADLINE_MILLIS` against `GC_GRACE_WINDOW_MILLIS`
  directly, not a copied value.
  **Test:** build-time only; a regression here is a compile failure, not a
  runtime test.
- **Claim:** under a concurrent rewrite of a tracking record during a GC
  pass, the record is never lost incorrectly.
  **Checked:** `crates/dst/tests/custodian.rs`, the appended
  `sweep_under_a_concurrent_restamp` property and its coverage companion.
  **Test:** run under the project's deterministic-simulation test suite
  (`cargo xtask ci`), 50 seeds; passes with this change, fails with a
  reverted (blind-delete) version of the fix.

Fixes #800
