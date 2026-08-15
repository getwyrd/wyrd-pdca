# PR description

## Summary
**User impact:** an operator draining a storage server (to decommission it) could
get permanently stuck if even one multipart ("segmented") object, or one corrupted
metadata record, existed anywhere in the store: the maintenance pass that moves data
off the draining server would abort entirely, so *no* data moved and *no* server in
the store could ever be decommissioned — with no indication of which object was the
problem.

This change makes the drain pass skip past the one object it can't handle (naming it
so it can be repaired) and keep moving everything else, and it stops reporting the
drain as safely finished when work was actually skipped or refused.

## What to look at
The core change is in the loop that scans committed objects and decides what to
evacuate off a draining server (`crates/custodian/src/rebalance.rs`, functions
`plan_evacuations` and `evacuate_chunk`). Previously, hitting one unreadable or
multipart object made the whole function return an error and stop. Now each object
is read through the same shared "resolve this object's data map" helper the other
maintenance loops (garbage collection, restore) already use; a fault is attributed to
just that object (logged and skipped) instead of aborting everything, and an
evacuation the code cannot yet perform for a multipart object is refused (nothing is
written) rather than silently dropped or treated as success.

To reproduce the old behavior: seed a store with one multipart object and one
ordinary ("flat") object that has data on the draining server, then run the drain
pass — before this change it errors out and nothing moves; after this change the
flat object's data is still evacuated.

## Root cause
The pass read each object's chunk map directly out of its metadata record and used
`?` on the very first fault (a segmented object or an undecodable record), which
propagates that one object's failure to end the entire scan for the whole store,
rather than confining it to that object.

## Fix
Both read sites now go through `metadata::resolve_chunk_map`, downcasting the error
to distinguish an object-local fault (contained: named, counted, and skipped, with
the walk continuing) from a store-level fault (still propagated, since that isn't one
object's problem). An evacuation owed by a chunk whose bytes live in a segmented
record is refused — nothing is written, and it's logged once per object, not once per
chunk. The pass's answer now reflects this: it reports `Blocked` (not `Satisfied`)
whenever it skipped or refused something, `Changed` when other work converged, and
`Satisfied` only when nothing was withheld — so a healthy multipart object that
doesn't hold anything on the draining server no longer blocks certification by
itself. Writing evacuated data for a segmented object is intentionally left to a
follow-up change; this only fixes the incorrect abort/skip/certify behavior around it.

## Verification
- **Claim:** a segmented or unreadable object no longer aborts the whole drain pass;
  unrelated flat work still proceeds; the pass never reports the drain satisfied over
  work it skipped or refused.
- **Checked:** `crates/custodian/src/rebalance.rs:158-164` and `:255-261` on
  `origin/main` (339da46) — the two sites that previously ended the whole scan with
  `?` on the first segmented/undecodable object.
- **Test:** `crates/custodian/tests/segmented_map_rebalance.rs` (new file, five
  cases) — fails (asserts red) against `origin/main`'s `rebalance.rs`, where every
  case aborts on the first segmented/undecodable object instead of completing; passes
  with this fix applied.

Fixes #696
