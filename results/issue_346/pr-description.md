# Evacuate pre-M3 chunks when draining a D server

## Summary
When an operator drains a D server to decommission it, the rebalance loop
quietly skipped chunks written before the `placement` field was introduced —
leaving their only fragment on the draining server, where it was lost the
moment the server was removed. This change makes rebalance resolve those
chunks' fragment locations the same way every other path does, so they are
evacuated to a healthy server and the placement record is rewritten in full.

## What to look at
- `crates/custodian/src/rebalance.rs`, `plan_evacuations`: the fix resolves the
  full fragment set once through `ChunkRef::placed_dserver` and uses that single
  vector for the evacuation scan, the survivor-domain spread, and the
  `EvacPlan.placement` that gets committed.
- `evacuate_chunk` is intentionally **unchanged** — it already cloned, indexed,
  and committed `plan.placement` correctly *given* a full-length vector; the bug
  was that planning didn't guarantee that precondition.
- To reproduce: commit an inode whose `ChunkRef.placement` is empty (the pre-M3
  shape), mark the identity-resolved D server draining, and run the reconcile
  loop. Before the change nothing happens; after, the fragment is moved and the
  committed placement is full-length.
- Regression tests: `crates/custodian/tests/rebalance.rs`, the two
  `evacuates_a_pre_m3_chunk_with_empty_placement_*` cases.

## Root cause
`plan_evacuations` scanned the raw `ChunkRef.placement` vector, which decodes
empty for a pre-M3 / mixed-era record, instead of resolving each fragment through
the shared identity-placement fallback. The empty scan produced no evacuation
candidates (so the chunk was skipped), and the same raw vector was stored into
the plan and later cloned, indexed, and committed — so even a forced move would
have panicked on the empty vector or persisted a too-short placement record.

## Fix
Materialize the full `0..fragment_count()` placement once via
`ChunkRef::placed_dserver` and reuse that one vector for the evacuation scan, the
survivor-domain computation, and the placement stored in the plan. This is the
same resolution the read path, GC, scrub, and reconstruction already use, so a
mixed-era chunk now has one placement closure across every path. The committed
record is always full-length and the moved index no longer names the draining
server.

## Verification
- **Claim:** A chunk whose `placement` is empty and whose identity-resolved D
  server is draining is evacuated to a healthy, distinct-domain server, and the
  committed placement is full-length (`== fragment_count()`) with the moved index
  no longer naming the draining server.
- **Checked (the defect):** `crates/custodian/src/rebalance.rs:151-152,163-164,175`
  on `main` — evacuation selection, survivor domains, and the carried placement
  all read the raw (empty) vector; `crates/custodian/src/rebalance.rs:221,224,245,253`
  — that carried vector is cloned, indexed, and committed verbatim, so an empty
  one panics and a short one commits a malformed record.
- **Checked (the shared resolution):** `crates/core/src/metadata.rs:119` on `main`
  — `ChunkRef::placed_dserver` applies the identity fallback (`placement[i]` if
  present, else D-server `i`) over `fragment_count()` (`:103`); the same idiom is
  already used in `crates/custodian/src/reconstruction.rs:230-232` and
  `crates/custodian/src/gc.rs:197-199`.
- **Test:** `crates/custodian/tests/rebalance.rs` —
  `evacuates_a_pre_m3_chunk_with_empty_placement_ec_none` and
  `evacuates_a_pre_m3_chunk_with_empty_placement_reed_solomon_index_gt_zero` fail
  before the change (no plan produced — status stays `Satisfied`, nothing moved)
  and pass after (status `Changed`, committed placement `vec![1]` / `vec![0, 3, 2]`,
  draining server no longer referenced, object still reads back). The Reed–Solomon
  case also asserts the `n` fragments still span `n` distinct failure domains, so
  the survivor-domain resolution is exercised for a draining fragment at index > 0.

Fixes #346
