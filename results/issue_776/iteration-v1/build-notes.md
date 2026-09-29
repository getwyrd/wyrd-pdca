# Build notes — #776 seg-record placement-move primitive

Target: getwyrd/wyrd @ main (worktree base `243241e`). One file changed:
`crates/core/src/metadata.rs`. `patch.diff` is 32.5 KB (budget 50 KB). Line numbers
below are in the patched file; the brief's own line numbers (`:380`, `:870`, `:1776`, …)
come from an older main and have since moved, so I cite by symbol too.

## What changed

| Where (patched file) | What |
|---|---|
| `metadata.rs:593-600` | `flat_value_ceiling_crossed` doc: the stale sentence "a segmented root's placement write … must weigh `MAX_ROOT_VALUE_BYTES`" replaced. It now says the segment arm weighs the full `MAX_VALUE_BYTES` and that V/2 only bounds a segment-table publication. The code is unchanged. |
| `metadata.rs:845-851`, `:967-969` | New `ChunkMapError::VersionExhausted { version }` plus its `Display` arm. |
| `metadata.rs:3112-3190` | `pub enum Repoint { Prepared(WriteBatch), Refused { bytes, ceiling }, Conflict }` and the `repoint_chunk` doc. The doc lists the three pins exactly as settled; it does **not** claim to pin "the exact bytes the resolve read". |
| `metadata.rs:3195-3276` | `pub async fn repoint_chunk(store, inode, generation, byte_offset, prior, placement) -> Result<Repoint>` |
| `metadata.rs:3278-3288` | `weighed`: both arms go through `flat_value_ceiling_crossed` (full V). No `MAX_ROOT_VALUE_BYTES` comparison anywhere in the diff; I checked with grep and it appears only in doc text. |
| `metadata.rs:3290-3301` | `segment_may_hold`: finds the candidate segments. |
| `metadata.rs:3303-3320` | `chunk_at`: offset plus equality lookup. |
| `metadata.rs:4385-4893` | `#[cfg(test)] mod placement_move`: 13 tests. |

Semantic non-test lines added: **122**, not counting blank lines and comments (budget
≤170). `commit_chunk_map`, including its segmented refusal (now `metadata.rs:2159-2164`),
is untouched. So are the resolver and read side, every custodian file, and proposal 0016.

## How it maps to the Success criterion

1. **Flat arm.** Mirrors `commit_chunk_map` (`metadata.rs:2152-2180`): `..generation.clone()`
   plus a bumped version, and `require(inode, encode(prior)) + put(inode, encode(next))`.
   One difference: `state` is not set, because a placement move does not change the
   lifecycle. Test: `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata`
   (`:4519`) asserts the whole record: only B's placement and `version` 3→4 change, while
   size, state, etag, content_type and modified stay.
2. **Segmented arm.** Segments are found in the root's own table (`map.segments()`, checked
   by `SegmentedMap::new`, `metadata.rs:1095`). There is no `seg:` range walk. The batch
   `require`s the root and never `put`s it. Test `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root`
   (`:4576`) stores **garbage in segment 0** and moves a chunk in segment 1: the move
   succeeds, so segment 0 was not decoded. The root and segment-0 bytes are asserted unchanged.
3. **Three pins.** (a) Root generation: `root_pin` at `:3204`. (b) The segment record's
   bytes as read **fresh here**: `root_pin.require(key, bytes)` at `:3264-3266`. (c) The
   `ChunkRef`: `chunk == prior` at `:3314`. Test `a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts`
   (`:4618`) covers three cases. A sibling edit before the call is merged (Committed, and
   both edits are present). An edit to the planned chunk gives `Conflict` with the store
   unchanged. An edit landing between prepare and commit makes the batch fail with the
   store unchanged. The docs say plainly that pin (b) is a fresh read, not the resolve's
   bytes, because `ResolvedChunkMap` keeps only the flattened chunks.
4. **Superseded root.** `a_superseded_root_fails_the_batch_or_the_move` (`:4659`): after
   the root flips, the prepared batch's commit returns `Conflict` and the whole store is
   byte-identical. With the retired segment also deleted, the call itself returns
   `Conflict`. With the object deleted, the same. Flat version:
   `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root` (`:4553`).
5. **Ceiling, full V, both arms.** `flat_arm_refuses_a_record_the_move_would_push_past_the_value_ceiling`
   (`:4805`) and `segmented_arm_weighs_its_record_against_the_full_value_ceiling` (`:4823`).
   The records are built in code by `padded` (`:4781`): zero-length filler `ChunkRef`s,
   then one tuning chunk whose placement width and id digit count hit the target byte
   count exactly. The test asserts that. There are no byte literals. Moving `[0]` → `[u64::MAX]`
   adds 19 bytes. At V−18 the move gives `Refused { bytes: V+1, ceiling: V }` and the store
   is byte-identical. The segment arm also shows V−19 → exactly V is **admitted** and
   commits a 100 000-byte segment record. That is far above V/2, so this would fail if
   anyone restored the V/2 bound.
6. **Checked version and malformed decode.** `flat_arm_refuses_a_version_it_cannot_advance`
   (`:4540`): `u64::MAX` → `Err(VersionExhausted)`, and the store is unchanged.
   `a_damaged_segment_of_the_live_generation_is_structural_corruption` (`:4686`):
   undecodable bytes → `SegmentRecordUndecodable{index:1}`. A shifted start →
   `SegmentBoundsMismatch`. A changed length → `SegmentBoundsMismatch`. An absent record →
   `SegmentAbsent`. In each case the root still names the generation. These go through the
   resolver's own arbiter `retired_or` (`metadata.rs:2780`), so "retired, or corrupt?" is
   decided in the same single place as for reads. The salvage patch answered `Conflict` to
   all of these. That hides a record that stays damaged, which is what iteration 2 flagged.
7. **Zero-length chunk at a segment boundary.** `segment_may_hold` admits `offset == end`
   **only when `prior.len == 0`**. A chunk with bytes can only live in the segment that
   holds its first byte. So at most two candidates are read, one at a time, in table order,
   and `chunk_at` equality decides which one is written. Test
   `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality` (`:4742`): Z (empty,
   the tail of seg 0) is written to seg 0 even though seg 1 covers byte 5. Y (empty, the
   head of seg 1) is written to seg 1 after seg 0 is checked and ruled out by equality. A
   Z whose placement changed gives `Conflict` with nothing written. The salvage only
   looked at the half-open covering segment, so a trailing empty chunk could never be
   moved: a permanent `Conflict`.
8. **Direct helper tests.** `segment_may_hold_is_the_half_open_span_plus_an_empty_chunks_end`
   (`:4849`, a table of 8 cases including both edges) and
   `chunk_at_needs_both_the_offset_and_the_reference` (`:4874`, which includes an overflow
   of the lengths → `None`).

## Salvage: what I kept and what I changed

I kept the shape of `/results/issue_711/iteration-v1/patch.diff` (the `Repoint` enum,
`chunk_at`, the flat idiom, the segmented CAS without a root `put`). Changes:
- The doc sites that claimed "conditioned on the exact bytes the move was planned from" /
  "What it pins — the exact bytes the resolve read" are rewritten to the three settled pins.
- The version advance is `checked_add` → `VersionExhausted` (the salvage used `+ 1`).
- Segment anomalies go through `retired_or` instead of a blanket `Conflict`.
- The record extent is checked against the root's `SegmentRef`, the same check
  `read_segments` makes. That makes `byte_offset - segment.byte_offset` safe given
  `segment_may_hold`, so the salvage's `checked_sub` on the record's own offset is gone.
- `covers` is replaced by `segment_may_hold`, which handles an empty chunk at a boundary.
- `weighed` is shared by both arms, so there is one ceiling call site.

## Alternatives I ruled out

- **`VersionExhausted` as a `Repoint` arm instead of an error.** A version at `u64::MAX`
  is a damaged record, not a planning outcome, and the brief asks for no panic and no wrap.
  A typed `ChunkMapError` keeps it attributed to this object, like the other structural
  faults. Cost: one variant (7 lines) plus a 3-line `Display` arm. `ChunkMapError` has no
  exhaustive `match` outside `metadata.rs`: `git grep 'ChunkMapError::'` shows only
  `read.rs:96`, which constructs one.
- **Reading only the half-open covering segment (the salvage).** Cheaper, with one read
  always, but a trailing empty chunk in a non-final segment is then unmovable forever.
  That is a small copy of the C-1 defect this lineage removes. The fix costs one extra
  `get` only for `len == 0` chunks on a boundary.
- **Using `retired_or` for the flat arm.** Not needed: a flat map is one value, and the
  root pin already covers every race.

## Verification posture

As the brief states, this is green-only under C4-verify by design: no new `tests/*.rs`,
all tests are in-crate `#[cfg(test)]`. The binding check is `cargo mutants`.

**`cargo mutants --in-diff patch.diff -p wyrd-core --test-package wyrd-core -- --lib`:
30 mutants, 21 caught, 0 missed, 0 timeout, 9 unviable.** The 9 unviable mutants failed to
compile under the workspace's `unused`/`unreachable` deny lints: `Display::fmt → Ok(default)`,
`repoint_chunk → Ok(default)`, delete the field `version`, `weighed → default`,
`segment_may_hold → true|false`, `chunk_at → None|Some(0)|Some(1)`. I re-applied each one
that could compile by hand, in a lint-clean form (`if black_box(true) { return X; }`), and
every one went **red**:
- `segment_may_hold → true`: 2 failed. `→ false`: 8 failed.
- `chunk_at → None`: 11 failed. `→ Some(0)`: 6 failed. `→ Some(1)`: 5 failed.
- version not advanced (`version: version - 1`): 1 failed (the flat-arm test).

**Named negations, applied, run and reverted (file compared byte-for-byte after each):**
- Delete the `chunk == prior` equality (kept `prior` referenced so the lint lets it
  compile): **4 failed**, among them `a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts`,
  `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality`,
  `flat_arm_conflicts_…`, `chunk_at_needs_…`.
- Delete the root-generation pin (`root_pin = WriteBatch::new()`): **2 failed**,
  `a_superseded_root_fails_the_batch_or_the_move` and
  `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root`.
- Delete the ceiling comparison (`match None::<usize>`): **2 failed**, both ceiling tests.
- Make the version advance unchecked (`generation.version + 1`): **1 failed**,
  `flat_arm_refuses_a_version_it_cannot_advance` (overflow panic).

## Refute-your-own-test

- **(a) Genuine red?** Yes, per mutation rather than per revert: the defect is an absent
  API, so reverting the fix removes the symbols and the tests stop compiling, as the brief
  says. Each piece of behaviour was negated on its own and went red. The numbers are above.
- **(b) Production path?** Yes. The tests call the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at` directly, and apply the returned batch through
  `MetadataStore::commit` on a real redb backend (`RedbMetadataStore::in_memory()`, the
  same one the neighbouring `segmented_shape_invariants` module uses). No mocks, no copies.
- **(c) Fixture includes the fault?** Yes. The faults are put in the store itself: a
  garbage segment 0, a sibling edit, an edit to the planned chunk, an edit between prepare
  and commit, a flipped root, deleted segment and root records, a shifted or resized
  segment record, a `u64::MAX` version, empty chunks on a boundary, and records padded to
  within 18/19 bytes of V. "Nothing written" is a full-store `scan(b"")` compared before
  and after, not a claim about the batch.

## Gates run

- `./engine/xtask.sh ci` (= `cargo xtask ci`) in the worktree: **exit 0, "all checks
  passed"**. That includes typos, docs lint and render, `cargo fmt --check`, clippy
  (workspace and madsim dst), cargo-deny, cargo-machete, conformance, statics and
  deploy-guard. None of the prose gates skipped.
- `cargo fmt -p wyrd-core` was applied before the gate.
- For iteration I used `cargo test -p wyrd-core --lib placement_move` under `timeout 1200`.
  xtask has no scoped test subcommand. 13/13 pass.

## Things for the human to know

- **Docs currency rubric.** This adds a public library function and an error variant in
  `wyrd-core`. It adds no port, gateway/API operation, RPC, CLI flag or persisted field,
  and the brief limits the change to one file. So I did not touch the living architecture
  doc. If you read "API operation" to include core library entry points, the doc update
  belongs with #777, where the primitive gets its first caller.
- The existing doc on `flat_value_ceiling_crossed` still cites a stale `(:2493)` for the
  resolver's `>` refusal (it is now `read_group_range`, about `metadata.rs:2888`). That
  line is outside the sentences I changed, so I left it alone.
- Scratch used: `$PDCA_SCRATCH/pdca-builder-776-{target,mutants,neg,ci.log,added.txt}`.
  I left it for the harness to clean up, following the builder rule against rm-style
  commands.
