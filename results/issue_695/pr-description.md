## Summary
**User impact:** a single damaged or not-yet-migrated object in the metadata store
could silently stop the storage maintenance pass that fills in missing placement
information for *every other* object in the store, and it also stopped the
operator-facing "how much work is left" gauge from being published at all. An
operator watching that gauge for the migration to finish would see nothing, with
no indication why or which object was responsible.

This change makes the maintenance pass read every object through the same shared
lookup path the other maintenance passes already use, so one bad or unsupported
object no longer takes down the whole run: it is reported and skipped, the rest
of the store is still processed, and the pass now says so plainly rather than
silently returning "done" over incomplete work.

## What to look at
The core change is in `crates/custodian/src/backfill.rs`, in the `reconcile`
function's per-object loop and the `emit_remaining` gauge it feeds. Previously a
segmented (not-yet-migrated) object made the whole loop return early. Now each
object is read independently; an unreadable object is logged and skipped, a
segmented object that still needs a fill is logged as "declined" (nothing is
written to it), and the loop moves on to the next object either way. Only once
every object has been read does the pass decide whether it's actually done. The
new test file `crates/custodian/tests/segmented_map_backfill.rs` is a good way to
see this in action -- it seeds a mix of healthy, damaged, and not-yet-migrated
objects in one store and asserts the pass gets through all of them.

## Root cause
`backfill.rs` read the chunk map directly out of each record at two call sites,
each of which hard-`?`-propagated an error the moment it met a segmented
(not-yet-migrated) record -- ending the loop for the whole store rather than for
just that record. GC and restore already read through a shared resolver that
contains failures per object; backfill was the last of the four maintenance
loops still reading inline.

## Fix
Both call sites now go through `wyrd_core::metadata::resolve_chunk_map`, the
resolver GC and restore already use. An object whose bytes won't decode, or
whose generation the resolver refuses, is contained: named on the audit seam,
counted, and skipped, while the walk continues. An object the pass may read but
not fill (its chunks live in `seg:` records) is declined -- nothing is written --
and its still-empty placements stay on the drain gauge. The pass now returns
`Reconciled::Blocked` rather than `Satisfied` whenever it skipped or declined any
object, and the drain gauge is counted in the same walk that fills, instead of by
a separate, redundant scan of the whole namespace.

## Verification
- **Claim:** one segmented or unreadable object no longer stops the pass for the
  rest of the store, each such object is individually reported and counted, a
  healthy record elsewhere is still filled, and the pass never reports "done"
  over a reading that skipped or declined anything.
  - **Checked:** `crates/custodian/src/backfill.rs:120-291` (the `reconcile` loop
    and its outcome decision) and `:299-340` (the gauge and per-object audit
    emitters) on the branch this PR targets.
  - **Test:** `crates/custodian/tests/segmented_map_backfill.rs` -- all five
    cases fail against the unpatched `reconcile` (reverted to its pre-fix form)
    and pass with this change; run via `cargo test -p wyrd-custodian --test
    segmented_map_backfill`.
- **Claim:** a fault unrelated to any one object's chunk map (e.g. the store
  itself becoming unreachable) still fails the whole pass, rather than being
  silently absorbed as a per-object containment.
  - **Checked:** `crates/custodian/src/backfill.rs:130-144` (the downcast that
    tells a chunk-map-specific fault apart from any other error).
  - **Test:** `a_fault_that_is_not_one_objects_map_still_ends_the_pass` in
    `crates/custodian/tests/segmented_map_backfill.rs`.
- **Claim:** existing behavior for a race lost against another writer is
  unchanged (that case is not treated as a blocker).
  - **Checked:** `crates/custodian/tests/backfill.rs:278-325`, an existing test
    left unmodified by this PR, still passes.

Fixes #695
