## Summary
**User impact:** when a server holding part of a very large object is lost, the
background repair cannot put that part back on a healthy server. Large objects keep
their chunk list split across several metadata records ("segments"), and nothing in the
tree can rewrite one of those records. So the object's lost redundancy is never
restored, and its repair job is refused on every pass. Smaller objects, whose chunk list
fits in a single record, repair normally.

This PR adds the missing building block: a function in `wyrd-core` that prepares the
metadata update to move one chunk to new servers, whether the object's chunk list is
stored in one record or split across segments. It does not change repair behaviour yet.
Nothing calls it until the follow-up (#777) wires it into the repair loop.

Tracked in #776, split out of the larger repair work in #682.

## What to look at
- The one new public function, `repoint_chunk` in `crates/core/src/metadata.rs`, and
  its doc comment, which spells out what the prepared update checks and what each
  outcome tells the caller.
- It never writes anything itself. It hands back a batch the caller adds its own
  changes to and commits once, the same way repair already commits a flat move.
- To try it: `cargo test -p wyrd-core --lib placement_move` runs the 15 new tests,
  including a seeded race campaign. The full gate is `cargo xtask ci`.

## Root cause
Repair's only placement writer, `repair_chunk`, takes the prior chunk map with
`as_flat()` and aborts when the map is segmented
(`crates/custodian/src/reconstruction.rs:1143-1150` on `main`). Below it,
`commit_chunk_map` refuses a segmented map outright (`crates/core/src/metadata.rs:2146-2150`
on `main`), and no other function can compose a compare-and-swap on a `seg:` record.
`backfill.rs:112` on `main` already records this gap with a `deferred: #682` marker
naming `repoint_chunk`.

## Fix
One file, `crates/core/src/metadata.rs`, all additions (line numbers are on this branch):

- `repoint_chunk` (`:3232-3328`) takes a resolved generation, the chunk's byte offset,
  the `ChunkRef` the caller planned from, and the new placement. It returns
  `Repoint::Prepared(WriteBatch)`, `Refused`, `VersionExhausted` or `Conflict`
  (`:3113-3150`).
  - **Flat map:** rewrites the inode record the way `commit_chunk_map` does:
    `version + 1` (checked, so it cannot wrap), `..generation.clone()` so ADR-0047
    metadata and `state` are kept (`:3251-3268`).
  - **Segmented map:** finds the covering segment from the root's own table, with no
    `seg:` range scan and no other segment decoded (`:3272-3276`). Only that one segment
    record is rewritten. The root's bytes are pinned but never written.
- **What the batch pins:** the root generation's bytes (`:3249`), the segment row's
  freshly read bytes (`:3316`), and the planned chunk itself. The chunk is found by
  offset **and** equality with the planned `ChunkRef` (`chunk_at`, `:3359-3371`). So a
  concurrent edit to a *sibling* chunk still merges, while an edit to the planned chunk
  or a superseded root makes the batch fail with nothing written. The doc comment does
  not claim the move pins "the exact bytes the resolve read". For a segmented map it
  does not, and a sibling edit is deliberately allowed.
- **Size ceiling:** both arms weigh the re-encoded record through the existing
  `flat_value_ceiling_crossed` (full `MAX_VALUE_BYTES`, `main :549`, `:602`) before
  anything is written (`weighed`, `:3331-3339`). The root-only half-ceiling
  (`MAX_ROOT_VALUE_BYTES`) is not used, because this never re-encodes the root. A
  segment row that is readable today stays repairable.
- **Damaged input:** a live segment row already over the ceiling is refused before it is
  decoded (`:3287-3293`, mirroring `read_group_range`, `main :2875`). Decode and bounds
  faults go through the resolver's own `retired_or` (`main :2767`): a conflict if the
  generation was retired, a typed `ChunkMapError` if it is still live. Damage is never
  hidden by a rewrite.
- **Caller errors:** a replacement placement whose length does not match the chunk's
  fragment count is refused before any read, as its own `MalformedReplacement` type
  (`:3161-3175`, `:3240-3247`). It is not a `ChunkMapError`, so a planner bug cannot be
  mistaken for a damaged object.
- `commit_chunk_map`, the read side and every custodian file are untouched. No new
  dependency.

Left for #777, where behaviour changes: wiring this into repair, the architecture-doc
update (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7; the doc comment
carries a `deferred: #777` marker), and a madsim scenario in `crates/dst`. A few API
points should also be settled when the first caller exists:
- A flat-arm `Conflict` that comes from a caller offset mistake rather than a race
  should be made visible, not retried silently.
- `Refused` and `VersionExhausted` are judged on the planned generation alone. The
  caller should re-resolve before escalating either one to an operator.

## Verification
- **Claim:** the move lands in whichever record holds the chunk.
  **Checked:** `:3251-3276`.
  **Test:** `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata`
  (version +1, metadata and `Pending` state kept);
  `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root` (root bytes
  unchanged; a move on a segment's first byte succeeds even when the previous segment is
  garbage, so no neighbour is decoded).
- **Claim:** a sibling edit merges; an edit to the planned chunk conflicts and writes
  nothing.
  **Checked:** `:3316`, `:3365`.
  **Test:** `a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts`.
  "Nothing written" is a full-store scan compared before and after.
- **Claim:** a superseded root fails the batch cleanly.
  **Checked:** `:3249`.
  **Test:** `a_superseded_root_fails_the_batch_or_the_move`,
  `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root`.
- **Claim:** the full value ceiling is enforced before any write, and exactly-on-ceiling
  is allowed.
  **Checked:** `:3331-3339` → `flat_value_ceiling_crossed` (`main :602`).
  **Test:** `flat_arm_refuses_a_record_the_move_would_push_past_the_value_ceiling`,
  `segmented_arm_weighs_its_record_against_the_full_value_ceiling`,
  `a_segment_row_over_the_ceiling_is_never_decoded_nor_rewritten`.
- **Claim:** version advance never wraps; damaged live segments surface as corruption;
  wrong-length placements are refused.
  **Test:** `flat_arm_refuses_a_version_it_cannot_advance`,
  `a_damaged_segment_of_the_live_generation_is_structural_corruption`,
  `a_replacement_placement_must_name_every_fragment`.
- **Claim:** addressing is exact, including a zero-length chunk on a segment boundary.
  **Test:** `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality`,
  `chunk_at_needs_both_the_offset_and_the_reference`,
  `segment_may_hold_is_the_half_open_span_plus_an_empty_chunks_end`.
- **Claim:** holds under concurrency.
  **Test:** `seeded_moves_race_each_other_and_the_roots_retirement`: 256 seeded
  `wyrd_testkit::Sim` campaigns of racing moves, root overwrites, root deletes and
  reclaimed segments, all planned through the production resolver.

**Why there is no failing-before test:** the defect is a missing API, so a test cannot
exist before the fix without naming the new function. The tests live in-crate
(`mod placement_move`, `:4441-5249`), and the proof that they bite is mutation testing
instead:
- `cargo mutants --in-diff` reports 27 caught, 0 missed, 9 unviable.
- Hand-applied negations each turn the expected test red: dropping the planned-chunk
  equality, the root pin, the ceiling check, the checked version add, the over-ceiling
  guard, and the segment-bytes pin.
- `cargo xtask ci` passes on this branch, including the madsim leg.

Fixes #776
