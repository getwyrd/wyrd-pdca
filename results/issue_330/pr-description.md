# scrub: repair a placed fragment that is missing, not just corrupt

## Summary

The background scrub pass is supposed to keep committed data durable — every
fragment a committed chunk map references should end up either present-and-intact
or scheduled for rebuild. It had a gap: a fragment that was simply **missing** from
the storage server that was supposed to hold it was never noticed, so the chunk
stayed silently under-durable and was never queued for reconstruction. A later disk
loss on the remaining copies could turn otherwise-recoverable data into permanent
loss. This change makes scrub treat an absent placed fragment as the same durable
loss as a corrupt one and enqueue the chunk for repair.

## What to look at

- `crates/custodian/src/scrub.rs` — `reconcile`. The pass is now driven off the
  committed reference set (grouped by the D server each fragment is placed on) and
  asks each server for exactly the fragments the chunk map places there, instead of
  iterating whatever the server's own `list_fragments()` happens to return.
- The new `Ok(None)` arm of the per-fragment fetch (previously a bare `continue`)
  and the `emit_missing` helper next to the existing `emit_corruption`.
- To exercise it: commit a chunk map that references a fragment on a D server that
  holds no bytes for it, then run a scrub reconcile — the chunk should land on the
  shared repair queue. See the tests in `crates/custodian/tests/scrub.rs`.

## Root cause

Scrub discovered fragments by walking each D server's `list_fragments()` and
verifying the checksum of any it found that a committed chunk map referenced. A
fragment that is simply absent from a store by definition never appears in that
listing, so it was never visited at all; the fetch's `Ok(None)` case only ever fired
for a fragment that vanished mid-pass and just `continue`d
(`crates/custodian/src/scrub.rs:92` on `main`, `Ok(None) => continue`). Detection was
therefore limited to present-but-corrupt fragments.

## Fix

`reconcile` now builds the set of referenced `(D server, fragment)` placements from
the committed chunk maps and groups it by D server, then fetches each placed fragment
directly by id. A fetch that comes back `Ok(None)` means the D server holds no bytes
for a fragment the chunk map places there — a genuine loss — so the chunk is enqueued
on the same shared repair queue a corrupt fragment produces
(`repair::enqueue_repair(…, "scrub")`). Present-and-intact fragments behave exactly as
before. False positives are structurally excluded: the reference set only contains
placements of *committed* chunk maps (an in-flight write's provisional map is not in
it) and is the same set garbage collection uses as its reclaim safety gate, so scrub
and GC can never disagree about what is "referenced"; a transient fetch error still
propagates rather than being read as absence. The unobservable killed/partitioned-server
case (a server that cannot be reached at all) is deliberately left to a separate
topology-aware detector.

## Verification

- **Claim:** a committed-referenced fragment that is absent from its placed D server is
  enqueued on the shared repair queue — the same durable obligation a corrupt fragment
  produces.
  - **Checked:** `crates/custodian/src/scrub.rs` — the fetch's `Ok(None)` arm now calls
    `emit_missing` and `repair::enqueue_repair(ctx.meta, frag.chunk, "scrub")`, replacing
    the pre-fix `Ok(None) => continue` (`scrub.rs:92` on `main`).
  - **Test:** `crates/custodian/tests/scrub.rs`
    `detects_a_missing_placed_fragment_and_enqueues_for_reconstruction` (~`:888`) — fails
    pre-fix (reconcile returns `Satisfied` with an empty repair queue), passes post-fix
    (returns `Changed`, `queued_repairs == [chunk]`). Confirmed by reverting the new arm
    to `Ok(None) => continue` and observing the test go red.

- **Claim:** the new detection produces no false positive for a fragment that is
  legitimately not-yet-present (an in-flight, uncommitted write) or unreferenced.
  - **Checked:** `crates/custodian/src/scrub.rs` — the pass is driven entirely off the
    committed reference set (`referenced_fragments`), so an orphan or a pending
    (uncommitted) inode's placement is never visited.
  - **Test:** `crates/custodian/tests/scrub.rs`
    `does_not_flag_an_in_flight_pending_writes_fragment_as_missing` (~`:925`) — a pending
    inode's not-yet-placed fragment yields `Satisfied` and an empty queue; passes.

- **Claim:** corruption detection and transient-fault handling are unchanged.
  - **Checked:** `crates/custodian/src/scrub.rs` — the checksum-mismatch and
    integrity-fault arms still `emit_corruption` + enqueue; a transient `Err` still
    propagates via `Err(e) => return Err(e)`.
  - **Test:** the pre-existing `crates/custodian/tests/scrub.rs` corruption and
    transient-fault cases (e.g. `detects_corruption_in_a_full_rs_6_3_placement`,
    `scrub_propagates_a_transient_get_fault_without_enqueuing`) continue to pass.

Reported in [#330](https://github.com/getwyrd/wyrd/issues/330).

Fixes #330
