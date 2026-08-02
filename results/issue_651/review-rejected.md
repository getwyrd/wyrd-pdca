# review-rejected.md — issue 651 (re-scoped 2026-08-02, slice 4a of 7)

Machine-readable triage decisions for `scripts/review-branch` (T4-batch-review). Each
non-comment line is `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase
from the finding's own rationale (case-insensitive substring). Everything **not** listed
here was FIXED in this iteration and should leave the next run on its own.

> **The pre-split file is archived at `iteration-v5/review-rejected.md`.** Its entries are
> keyed to `resolve.rs`, `backfill.rs`, `reconstruction.rs`, `rebalance.rs` and
> `core/src/metadata.rs` — files this re-scoped slice does not touch at all (they are
> **#681** / **#682**), so those lines can no longer match anything here. The three standing
> rejections from `brief.md` § Do-not-re-earn are restated below at the lines of THIS patch
> where each finding can re-land.

## Standing rejections (`brief.md` § Do-not-re-earn), in the gate's format

<!-- (i) caller-side fan-out timeout — rejected 3x across #508/#636. The ChunkStore /
     MetadataStore IMPLEMENTATION owns the network bound, not the caller
     (crates/traits/src/lib.rs:1000-1012); no custodian await carries one, and this slice
     does not start. Note this patch REMOVES an await from the restore pass (the report half
     no longer re-scans `inode:`); it adds none. -->
crates/custodian/src/restore.rs:234 | BUG | timeout | standing rejection (i), #508/#636 x3: the store implementation owns its own network bound. This await is unchanged from origin/main; no custodian await carries a caller-side timeout, and adding one would put a runtime dependency in a crate whose seam boundary is traits/core/tracing (ADR-0010). This patch removes the pass's SECOND store walk rather than adding one.
crates/custodian/src/restore.rs:0 | BUG | bounded | standing rejection (i), as above — applies to every store await in this pass (`referenced_fragments`, `orphan_leases`, `pending_chunks`, the mark commits), all unchanged from origin/main.
crates/custodian/src/desired_state.rs:187 | BUG | timeout | standing rejection (i), as above; this await is unchanged from origin/main and the drain status gains no store read in this slice.

<!-- (ii) "a genuine store fault should be contained too" — no. A record-level read failure
     is contained (it is that object's own fault); a store fault under the read propagates,
     because a walk that cannot read the metadata store has no reference set at all and
     containing that as "one object is unreadable" would be the wrong answer for every
     object in it. Pinned on the base by #650's
     `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed`. -->
crates/custodian/src/restore.rs:234 | BUG | contain | standing rejection (ii): only a RECORD-level read failure is contained (`gc::referenced_fragments` downcasts `ChunkMapError`); a store fault still propagates through this `?`, and #650's `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed` pins it. Containing a store outage as "one object is unreadable" would report every healthy object as read when none was.
crates/custodian/src/restore.rs:366 | BUG | contain | standing rejection (ii), as above — the report half reads the set the builder already contained; it introduces no second containment site.
crates/custodian/src/desired_state.rs:187 | BUG | contain | standing rejection (ii), as above — a store fault must still blank this query, since nothing about ANY server can be said when the store cannot be read; only a named record's own fault is contained (`PendingUnresolvable`).

<!-- (iii) "`Completed` releases its admission slot" — withdrawn as unsatisfiable. Recorded
     only so the two counters this slice adds are not re-litigated as one: they are plain
     monotonic observation counters, with no session/tombstone concept anywhere near them. -->
crates/custodian/src/restore.rs:598 | CONVENTION | admission slot | standing rejection (iii), withdrawn as unsatisfiable. `restore_unresolvable_records` is a monotonic counter of records this pass could not read — no admission slot, session or tombstone exists in this surface.
crates/custodian/src/desired_state.rs:259 | CONVENTION | admission slot | standing rejection (iii), as above. `drain_unresolvable_records` counts observations (one per blocking record per status read), documented as such at the emitter.

## Scope declines (`brief.md` § Scope — out of scope)

<!-- Findings that are real but belong to a named sibling slice get a decline-with-issue
     reference per AGENTS.md's reviewer protocol ("Out of scope"), not an in-PR fix. -->
crates/custodian/src/restore.rs:0 | BUG | repoint | out of scope — **#682** owns `repoint_chunk`, the record ceilings and the repair/evacuation write path. This slice writes NOTHING to a chunk map; it only reads the reference set.
crates/custodian/src/restore.rs:0 | BUG | reconstruction | out of scope — **#681** owns the resolving namespace walk for reconstruction / backfill / rebalance. This slice adds no custodian-level walk and no `crate::resolve` module; it reads `gc::referenced_fragments` exactly as the base does.
crates/custodian/src/desired_state.rs:0 | BUG | evacuat | out of scope — rebalance's evacuation behaviour over an unreadable record is **#681**'s. This slice changes only what the drain STATUS reports; `rebalance.rs` is untouched.
