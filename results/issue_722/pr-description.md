## Summary
**User impact:** an operator who drains a storage server to retire it can wait forever.
If that server holds any piece of a large (segmented) object, the drain never finishes
and the server can never be safely taken out of service. Nothing errors; the drain
status just never reaches "done".

This PR makes the drain actually move those pieces, the same way the repair pass
already does, so the drain completes.

This is the drain half of the segmented-placement work; the repair half and the shared
move function landed in #850.

## What to look at
- The change is in the rebalance (drain) pass. Before, it skipped any chunk whose
  location was stored in a segment record and counted the object as "refused". Now it
  hands the chunk to the shared move function from #850, which rewrites whichever
  record holds it and records the old copies for cleanup in the same commit.
- To try it: `cargo test -p wyrd-custodian --test segmented_map_evacuate`. The test
  seeds a segmented object, marks the server holding one of its pieces as draining,
  runs one custodian pass, and checks that the piece moved and the drain made progress.
  Revert `crates/custodian/src/rebalance.rs` and the same test fails.
- Behaviour shifts worth a glance: a chunk shared by two objects is repointed in both;
  corrupt maps are now reported once per object instead of retried every pass; the
  `refused-segmented` audit line and `rebalance_refused_records` counter are removed.

## Root cause
The rebalance pass could only rewrite a flat `inode:` record, so for a chunk whose
`ChunkRef` lives in a `seg:` record it built no plan and marked the object refused
(base `crates/custodian/src/rebalance.rs:371-374`, `:397-398`). A refused object
withholds certification (base `:368`), and nothing ever cleared the refusal, so the
drain stayed owed on every pass.

## Fix
- Remove the segmented refusal and `emit_refused`; every owed chunk gets a plan.
- `evacuate_chunk` calls `metadata::repoint_chunk` (base
  `crates/core/src/metadata.rs:3240`) and adds the `gc::orphan_key` marks for vacated
  positions to the batch it returns, so placement and orphan evidence commit together.
  The hand-built `InodeRecord`, its local ceiling check and its `require`/`put` batch
  are gone; the ceiling check now lives only in the primitive.
- Plans share one root snapshot per object (`EvacObject`) and hold a `ChunkRef` plus
  byte offset, instead of a deep copy of the flat chunk list per plan.
- The move pins the root generation the resolve answered from, not the scan's snapshot.
- Mirrors the repair pass's containment: offsets use `checked_add` and an overflow
  contains the object; a segmented root under a non-canonical key is contained; each
  contained object is named once per pass. A terminal verdict on a root that has since
  moved is treated as a stale plan (conflict), not a refusal.
- Per-(object, chunk) planning is unchanged on purpose: adding cross-object dedup like
  the repair pass's would leave the second object naming the drained server.

**Known limitations, not changed here:**
- A move that loses its compare-and-swap leaves the copied fragment unreferenced and
  unmarked. The flat drain path already does this; this PR extends it to `seg:`
  records. Tracked in #723 (which also owns the "collectable garbage" comments).
- `repoint_chunk` weighs a `seg:` record against `MAX_VALUE_BYTES`, not
  `MAX_ROOT_VALUE_BYTES` (V/2). Leg 3 of the new test is keyed to what the primitive
  does today. Moving the primitive to V/2 (with headroom for future growth), re-keying
  leg 3, and checking for stored `seg:` records between V/2 and V is a follow-up
  against `metadata.rs`.
- `docs/design/architecture/08-crosscutting-concepts.md:89` and the `repoint_chunk`
  doc comment still name only the repair pass as a `seg:` writer; a one-line doc
  follow-up.

## Verification
- **Claim:** a draining server's fragment held in a `seg:` record is moved off it, the
  `seg:` record names the new location, the old position is orphan-marked, the root
  record is unchanged, and the pass answers `Changed`.
  - **Checked:** base `crates/custodian/src/rebalance.rs:371-374` refuses this case;
    the patched `evacuate_chunk` moves it through `repoint_chunk`.
  - **Test:** `crates/custodian/tests/segmented_map_evacuate.rs` leg 1 — fails on
    `main` (`Blocked` vs `Changed`), passes with the fix.
- **Claim:** when two objects share a chunk, neither still names the drained server
  afterwards, and every fragment either names exists.
  - **Test:** same file, leg 2 — fails on `main`, passes with the fix.
- **Claim:** the size ceiling still refuses a repoint that would push a `seg:` record
  over the limit, writing nothing and not certifying.
  - **Test:** leg 3 (and leg 5). Passes on `main` for a different reason, so it is
    bound by deleting the ceiling check in `repoint_chunk`, which turns legs 3 and 5
    red.
- **Claim:** a lost compare-and-swap writes no metadata, publishes no orphan mark,
  does not certify, and leaves the copied fragment in place.
  - **Test:** leg 4, bound by deleting the `seg:` `require` in `repoint_chunk` (leg 4
    goes red).
- **Claim:** each move is all-or-nothing against a racing rewrite or supersede.
  - **Test:** new seeded DST property `drain_repoint_versus_supersede` in
    `crates/dst/tests/custodian.rs` (50 seeds). Deleting either the root or the `seg:`
    `require` turns it red. It checks the two commit preconditions only; the
    read→prepare window is covered by the deterministic legs.
- **Claim:** the move pins the generation the resolve read, not the scan snapshot.
  - **Test:** leg 12 supersedes the root mid-resolve; pinning the scan snapshot turns
    it red.
- **Whole run:** on `main` with only the production change reverted, 11 of 12 legs
  fail (leg 3 passes by design); with the fix, 12 of 12 pass. The existing
  `segmented_map_rebalance.rs` leg that asserted the refusal is rewritten to assert
  the move lands. `cargo xtask ci`, fmt and clippy are clean; diff coverage 97.6%;
  `cargo mutants --in-diff` leaves no surviving mutant.
- **Prior art:** #850 converted only the repair caller; `rebalance.rs` on `main` has no
  call to `repoint_chunk`. #647 (closed for size) is not revived; no open PR touches
  these files.

Fixes #722
