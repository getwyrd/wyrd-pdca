## Summary
**User impact:** in rare timing windows, a background cleanup process could delete a
piece of data's storage right after another operation had just started moving that
same data to a new location using the old copy as its reference point. The move would
then succeed against bytes that no longer existed, producing a corrupted placement
record. Separately, some cleanup markers written in a newer format were silently
unreadable, so their storage was never cleaned up and the marker was flagged as
damaged forever.

This change makes the cleanup process write down its decision to reclaim a piece of
storage, durably, before it deletes anything — so the move-in-progress fails safely
instead of landing on deleted bytes — and teaches it to read every marker format the
system can write.

## What to look at
The core change is in the custodian's garbage-collection sweep (the background loop
that reclaims storage no longer in use). Look at how it now records an intent to
reclaim before touching any bytes, and how a competing move that raced it is turned
away instead of silently succeeding. To reproduce the original bug: write an
"orphaned" marker for an unused fragment, let it pass its grace period, and run GC —
on the old code a concurrent move that reads the marker's original bytes as a
precondition could still commit after GC deleted the fragment; with this change that
same commit is rejected.

## Root cause
GC deleted a fragment's bytes and only afterward recorded that it had done so, so a
window existed where another commit could still be preconditioned on the fragment's
now-stale marker. GC also decoded a marker as a bare, unstructured number, so any of
the newer JSON-shaped markers (naming the event that orphaned the fragment, or
recording GC's own reclaim decision) read as unreadable and were never reclaimed.

## Fix
- One shared codec for the three marker shapes — legacy, structured, and
  reclaiming — added beside the existing key helpers in
  `crates/core/src/metadata.rs:88-306`, so every writer and reader of a marker
  agrees on its bytes; decoding accepts exactly what encoding produces.
- GC's sweep in `crates/custodian/src/gc.rs:397-936` now records each mark's
  swap to "reclaiming" as an exact-value compare-and-swap, batched at up to
  `CLEANUP_BATCH` per commit, and only deletes a fragment once that commit lands.
  A mark that changed underneath the swap keeps its fragment; one whose event
  names a still-draining retirement is left alone; a mark already `reclaiming`
  from an interrupted earlier pass is resumed with no second grace check.
- A store fault after some fragments were deleted still commits their already-
  queued marker deletes, best effort, before the fault is reported
  (`crates/custodian/src/gc.rs:1017-1021`).
- Docs updated for the persisted format change:
  `docs/design/architecture/08-crosscutting-concepts.md` §8.7 and
  `docs/design/architecture/06-runtime-view.md` §6.7.

## Verification
- **Claim:** a marker past its grace period is reclaimed only after GC's exact-value
  reclaim commit lands, so a commit preconditioned on the marker's earlier bytes can
  no longer succeed once reclamation begins.
  **Checked:** `crates/custodian/src/gc.rs:314-346` (target branch, pre-fix) — GC
  calls `delete_fragment` before it queues or commits the marker's key delete, with
  nothing recorded in between.
  **Test:** `crates/custodian/tests/gc_reclaim_intent.rs` (new) leg B — fails on the
  target branch (the competing commit is answered `Committed` where it must be
  `Conflict`; a store fault leaves deleted fragments' marker keys behind) and passes
  with the fix.
- **Claim:** all three marker shapes decode, and a value that is none of them is left
  untouched and flagged rather than acted on.
  **Checked:** `crates/custodian/src/gc.rs:811-817` (target branch, pre-fix) — only a
  bare decimal is parsed; anything else is reported unreadable.
  **Test:** `gc_reclaim_intent.rs` leg A — fails on the target branch (a structured or
  reclaiming marker past grace reclaims nothing) and passes with the fix.
- **Claim:** a marker naming a still-draining retirement obligation is not reclaimed
  until that obligation is gone.
  **Test:** `gc_reclaim_intent.rs` leg D — fails on the target branch (no code path
  checks a retirement obligation) and passes with the fix.
- **Claim:** the reclaim-intent commits stay batched, not one per marker.
  **Test:** `gc_reclaim_intent.rs` leg C — fails on the target branch (no marker-key
  commit exists at all) and passes with the fix, pinning exactly the expected number
  of commits over 1,001 markers.
- **Claim:** a real concurrent move can no longer land on a fragment GC just deleted.
  **Test:** `crates/dst/tests/custodian.rs` property 14 (deterministic-simulation
  regression test) — fails on the target branch and passes with the fix.

Fixes #804
