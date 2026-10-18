# Build notes — issue 686 / streamed-get-moves-flat-chunk-map

Target: getwyrd/wyrd @ main, worktree base `7bf7eb9`. Line numbers below are on the
patched tree unless marked "base".

## What changed

1. **New owning resolver entry** `metadata::resolve_chunk_map_owned`
   (`crates/core/src/metadata.rs:3124`). Same resolution rules as `resolve_chunk_map`
   (base `crates/core/src/metadata.rs:3042-3054`), but takes the `InodeRecord` **by value**.
   - Flat map: destructures the record and moves `ChunkMap::Flat(chunks)` straight into
     the answer (`metadata.rs:3129-3150`). No `.clone()`, `to_owned()`, `into_owned()` or
     `to_vec()` anywhere on that path.
   - Segmented map: same `read_segments` call `resolve_snapshot` makes, and the chunk list
     is built by `assemble_segments` (`metadata.rs:3020`), which I pulled out of
     `resolve_snapshot` (base `metadata.rs:3010-3019`) so both paths share the
     table-order assembly instead of having two copies of it.
   - Superseded: hands off to `resolve_current_chunk_map`, unchanged
     (`metadata.rs:3164-3176`). Its `ResolvedChunkMap<'static>` has both `Cow`s `Owned`,
     so `into_owned()` there just unwraps (no copy). The restart path's own copy of a flat
     successor's list (base `metadata.rs:3091`) is left alone, as the brief scopes it out.
   - Gone: `Ok(None)`, same as before.
2. **New result type** `OwnedResolvedChunkMap` (`metadata.rs:3074`) with `size`,
   `version`, `etag`, `content_type`, `modified`, `chunks: Vec<ChunkRef>`, plus a private
   `framed_by` helper (`metadata.rs:3092`) for the restart arm.
3. **Server streamed GET** (`crates/server/src/lib.rs:418-455`) now calls
   `resolve_chunk_map_owned(.., inode)` (`lib.rs:432`), passing the owned inode it got
   from `read::committed_inode` (`lib.rs:419`). Framing is read from the owned answer by
   move (`lib.rs:446-451`), and the chunk list is moved into `chunk_map`
   (`lib.rs:455`). The wrong comment (base `lib.rs:447-450`) is replaced with one that
   says what the code does.

## How the record's framing is kept (for check (ii))

The record is **never kept whole and never cloned**. `resolve_chunk_map_owned`
destructures it once (`let InodeRecord { size, version, etag, content_type, modified,
chunk_map, state: _ } = record;`), moves each framing field into
`OwnedResolvedChunkMap`, and moves the flat `Vec<ChunkRef>` into `chunks`. The server then
moves `resolved.etag` / `resolved.content_type` out (no `.clone()` — base `lib.rs:443-444`
cloned them off a borrowed record) and copies the two `Copy` fields `size` and `modified`.
Nothing between `read::committed_inode` and the reader task calls `.clone()` /
`to_owned()` / `into_owned()` / `to_vec()` on an `InodeRecord`, `ChunkMap` or
`[ChunkRef]`.

Why not keep `ResolvedChunkMap` and put `Cow::Owned(record)` in it: the record's
`chunk_map` is the list being moved, so the record would either have to be cloned (the
exact copy the brief forbids) or carry an emptied map that lies about the object. A
separate type with only the framing fields avoids both. `version` is included because the
restart test needs to tell generations apart and it costs nothing; `state` is dropped
because the resolver only ever answers committed generations.

## Alternatives ruled out

- **Change `resolve_chunk_map` to take `InodeRecord` by value.** It has six other callers
  (custodian gc, backfill, rebalance, reconstruction, restore, plus the ranged GET at
  `lib.rs:524` and `read::read_object`) that borrow on purpose. The brief puts them out of
  scope.
- **Call `inode.chunk_map.into_flat()` at the server, as `bbdb7c5` did, before the
  resolver.** That would put a second "flat or segmented?" decision in the gateway, which
  is what PR #683 removed by routing everything through the shared resolver. Keeping the
  shape decision in core is cleaner. `ChunkMap::into_flat` (`metadata.rs:1271`) still has
  no caller after this patch. I didn't remove it or change its doc comment, since that is
  outside this brief.

## Verification

Test: `crates/core/tests/streamed_read_chunk_map_ownership.rs` (new file, copy also in
the bundle at `crates/core/tests/`). Three tests over a real in-memory redb store:
- `a_flat_record_hands_over_its_own_chunk_list_by_move`: 3 `ChunkRef`s, each with a
  3-entry `placement`. It asserts that the outer `as_ptr()` and `chunks[0].placement.as_ptr()`
  are unchanged, that the contents are unchanged, and that all five framing fields match.
- `a_segmented_record_answers_its_segments_in_table_order`: a 2-segment table with its
  `seg:` rows. The chunks equal the source list in order, and framing comes from the record.
- `a_record_superseded_mid_resolve_restarts_onto_the_live_generation`: the caller holds a
  segmented snapshot whose `seg:` rows are gone, and the store root is a new flat
  generation (v3). The segment-absent fault goes to `retired_or`, which returns
  `Superseded`, which leads to a restart. The test asserts the live generation's chunks and
  all of its framing.

Green with the fix (`cargo test -p wyrd-core --test streamed_read_chunk_map_ownership`,
run under `timeout`):

```
test a_segmented_record_answers_its_segments_in_table_order ... ok
test a_flat_record_hands_over_its_own_chunk_list_by_move ... ok
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test result: ok. 3 passed; 0 failed
```

**Demonstrated red** (per the brief's Verification posture). I temporarily changed the
flat arm to `chunks: chunks.to_vec()`, which is exactly what today's
`Cow::Borrowed(..).into_owned()` does, then ran the test and restored the file:

```
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test a_segmented_record_answers_its_segments_in_table_order ... ok
thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' panicked at
crates/core/tests/streamed_read_chunk_map_ownership.rs:134:5:
assertion `left == right` failed: the chunk list is the record's own allocation (moved), not a copy
  left: 0x7eaed4014b10
 right: 0x7eaed4014e50
test result: FAILED. 2 passed; 1 failed
```

C4-verify on a fully reverted base will report **UNVERIFIABLE (77)**: the test calls
`resolve_chunk_map_owned`, which doesn't exist on base, so it can't compile there. The
brief says this up front. The red above is the substitute evidence.

Other runs, all green:
- `cargo test -p wyrd-server --test segmented_object_read --test s3_http_wire --test
  s3_range_conditional --test read_path`: 2 + 19 + 17 + 4 passed.
- `cargo test -p wyrd-core`: every suite ok.
- `cargo clippy -p wyrd-core -p wyrd-server --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check` and `typos` on the touched files: clean (I ran
  `cargo fmt --all`, which reflowed one match arm and the test file).
- `cargo xtask ci`: typos, lint_docs, render_site, gitlink/unsafe/blackbox guards, fmt and
  workspace clippy all passed, and 157 test suites reported ok with no failure. My 3500 s
  `timeout` then killed the run partway through the workspace tests, inside
  `s3_streaming_trailer` (a PUT-path suite this patch doesn't touch). Run alone, that suite
  passes 11/11 in 5.9 s, so the cutoff was my time budget, not a hang. The full gate gets
  re-run at Check.

## Refute-your-own-test answers

- **(a) Genuine red?** Yes, against a copying implementation. With the flat arm changed
  to `.to_vec()`, the flat test failed on the outer-pointer assertion (output above). A
  full revert of the patch can't compile the test, which is expected per the brief and
  will show as C4-verify UNVERIFIABLE.
- **(b) Production path?** Yes. The test calls `wyrd_core::metadata::resolve_chunk_map_owned`,
  the same function `get_object_streaming` now calls (`crates/server/src/lib.rs:432`), over
  the real redb `MetadataStore`. There is no mock and no copy of the logic.
- **(c) Fixture includes the fault?** Yes. The flat list is non-empty (3 chunks) with
  non-empty placements, so a copy gets a new heap address. The superseded case really
  removes the snapshot's `seg:` rows and replaces the root, so the restart branch runs.
  Gap: the test can't see a record clone that was then dropped. That is check (ii), a
  source review, and the destructuring above is what makes it hold.

## Rubric self-check

- Clock: no clock reads added.
- Seams / dependency direction: the new function lives in core beside the resolver, and
  the server calls it through `metadata::`. No new trait.
- `#![forbid(unsafe_code)]` is in the new test file.
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. This
  adds a library function and an in-memory result type, which the living architecture doc
  doesn't list (`resolve_chunk_map` isn't named there either).
- Absent entries: the `Gone` / `Superseded` / absent-segment handling is the existing
  resolver's, reached through the same `read_segments` / `retired_or` calls. No new silent
  skip.
- DST: this isn't a new destructive or concurrent path. It is the existing read
  resolution with ownership changed, and the restart logic is reused unchanged.
