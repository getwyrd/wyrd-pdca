# Adversarial review — issue #776 (`repoint_chunk` placement-move primitive)

I could not refute the fix. I re-ran the evidence myself and attacked the addressing, the pins, the ceiling and the arbiter path. Every attack either got caught by a test or turned out to change nothing observable. One item needs a human call: the only gating red, which is the T4 docs-currency finding.

## Findings

- NEEDS-HUMAN [human] — **The one gating red (T4 "Docs currency" at `crates/core/src/metadata.rs:3228`) is arguable, and it clashes with the brief's one-file scope.** The rubric's trigger list is "a port, an API operation, an RPC, a CLI flag, or a persisted field". `repoint_chunk` is none of these. It is an in-crate library function with no callers, no wire or persisted change, and no trait seam. The living architecture docs don't track functions at this level either: `resolve_chunk_map` and `commit_chunk_map` appear nowhere under `docs/design/architecture/`. The behavioural claim at `docs/design/architecture/08-crosscutting-concepts.md:85` ("the maintenance loops that … move them … treat a shape they cannot resolve as a typed error") stays true until #777 wires the primitive in. One sentence in that same paragraph does go stale in spirit: "an object whose root can no longer be re-written is an object whose placement can never be repaired". This primitive repairs a segmented object's placement without re-writing the root (`metadata.rs:3312` pins the root but never `put`s it). The human has to choose: (a) reject the T4 finding and record the reason (the doc update ships with #777, where behaviour changes), or (b) widen this child to two files. The brief says a second file means "STOP and hand back", so (b) is a scope change, not a rebuild.

## Attempted refutations that failed (evidence, not findings)

- **Red legs re-run by hand, not taken from build-notes.** I ran these on a scratch copy of `$PDCA_TARGET`:
  - Removing the equality check (`chunk == prior` → `(chunk == prior || true)`, `metadata.rs:3361`) makes 5 tests fail, including `a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts` and `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality`.
  - Removing the root pin from the segmented batch (`:3312`) fails `a_superseded_root_fails_the_batch_or_the_move` and the seeded campaign. Removing it from the flat batch (`:3263`) fails `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root` and the campaign.
  - Removing the ceiling check (`:3328`) fails both ceiling tests.
  - `checked_add` → `wrapping_add` (`:3251`) fails `flat_arm_refuses_a_version_it_cannot_advance`.
  - Removing the segment pin (`:3312`) fails the sibling-edit test and the campaign.
- **Argument-level mutations cargo-mutants never generates** (the class that slipped through in iteration 3). All were caught:
  - `SegmentRecord::new(chunks, byte_offset)` instead of `segment.byte_offset` (`:3310`)
  - `within = byte_offset` without subtracting the segment start (`:3304`)
  - `continue` → `return Conflict` (`:3306`)
  - dropping `retired_or` so every anomaly becomes a plain `Conflict` (`:3320`)
- **Two mutants survived, and neither changes behaviour.**
  - Removing the early `break` in `chunk_at` (`:3358`) is a pure speed-up: `at` only grows, so nothing past the offset can match.
  - Walking the candidate segments in reverse (`:3271`) only matters for a zero-length chunk on a boundary where the *other* candidate is damaged on a live generation. In that case the resolver already refuses the whole object (`read_segments`, `:2939-2986`), so the object is unreadable either way. The brief doesn't set an order.
- **Never-ending conflict for a correct caller.** A caller that plans from a fresh resolve (offset = sum of the lengths before the chunk) always has a candidate segment:
  - A chunk with bytes lives in the segment its first byte falls in.
  - A zero-length chunk sits at most on two segments' shared edge, and `segment_may_hold` admits both (`:3342-3347`).
  - Every segment is non-empty (`SegmentRecord::checked`, `:1376`), so no more than two segments can meet at one offset.
- **Root pin stability.** The segmented arm pins `encode(generation)` (`:3245`), not the raw root bytes. The segmented wire shape has a fixed field order (`SegmentedMapWireOut`, `:1193`). The inode's optional fields are omitted when absent (`:1631-1644`). There is no production producer of segmented roots yet (`checked_for_publication`, `:1727-1736`). So decode→encode matches byte for byte, which `seed_segmented` and the campaign exercise.
- **Gate evidence.**
  - `gate-logs/C4-ci.log:3733` is a full, clean `xtask ci` run (typos, docs render, deny, all 15 `placement_move` tests at `:870-923`), so iteration 4's CI-timeout caveat is resolved.
  - C4-verify being green-only was declared up front in the brief and is expected.
  - The two diff-coverage MISS lines (`:4491`, `:5196`) are `panic!` arms in the tests.
  - Scope matches the brief: one file, about 144 non-comment non-test lines (under the 170 limit), `patch.diff` at 49,677 bytes, no `MAX_ROOT_VALUE_BYTES` comparison, and `commit_chunk_map` untouched.
