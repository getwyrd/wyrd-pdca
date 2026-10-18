## Summary
**User impact:** every streamed download of an ordinary (non-segmented) object did
extra memory work on the server before sending the first byte: it made a full copy of
the object's list of chunk locations, then threw the original away. The cost grows with
the object's size and was paid on every such download, the busiest read path in the
gateway. The downloaded data was always correct; this is wasted CPU and memory, not a
correctness bug.

This PR hands that list to the download by moving it instead of copying it, which is
what the code did before the segmented-object resolver was introduced.

## What to look at
- The new owning entry point next to the existing chunk-map resolver in
  `crates/core/src/metadata.rs`, and the one-line switch to it in the streamed GET
  (`get_object_streaming`, `crates/server/src/lib.rs`).
- To exercise it: `cargo test -p wyrd-core --test streamed_read_chunk_map_ownership`.
  To see it catch the old behaviour, change the flat arm's `chunks,` in
  `resolve_chunk_map_owned` to `chunks: chunks.to_vec(),` and run the test again; the
  pointer assertion fails.

## Root cause
`get_object_streaming` owns the committed inode (base `crates/server/src/lib.rs:419`) but
passes it to `resolve_chunk_map` by reference (`:427-428`). For a flat map the resolver
answers `Cow::Borrowed` (base `crates/core/src/metadata.rs:3007`), so
`resolved.chunks.into_owned()` (base `lib.rs:451`) is a `to_vec()` of every `ChunkRef`
and its `placement`. The comment above it (base `lib.rs:447-450`) said it was a move.
bbdb7c51382ade669d07f0580a3f3e343c28c34d had made this a real move with
`ChunkMap::into_flat`; 99c7fcf11a95f4d3aa5718ebc6865ce70621bd4b (#683) moved the read onto
the shared resolver and lost it.

## Fix
- **`metadata::resolve_chunk_map_owned`** (new): same rules as `resolve_chunk_map`
  (flat / segmented / superseded-restart / gone → `Ok(None)`), but takes the
  `InodeRecord` by value. On the flat arm it destructures the record (no `..`, so a new
  field breaks the build here rather than being dropped) and moves the `Vec<ChunkRef>`
  straight out.
- **`OwnedResolvedChunkMap`** (new): carries `size`, `version`, `etag`, `content_type`,
  `modified` and `chunks` by value, rather than a whole `InodeRecord`, so the record
  never has to be cloned to keep its framing.
- **Segmented maps:** the table-order assembly is pulled out of `resolve_snapshot` into
  `assemble_segments`, shared by both entries.
- **Superseded mid-resolve:** hands off to `resolve_current_chunk_map` unchanged. Its
  answer is always `Cow::Owned`, so the `into_owned()` calls there only unwrap. That
  restart path's own copy of a flat successor's list is left as is; it runs only when a
  generation is replaced during the read.
- **Streamed GET:** calls the owning entry and moves `etag` / `content_type` out of the
  answer (they were cloned before). The wrong comment is replaced.
- **Unchanged callers:** the ranged GET, `read::read_object` and the maintenance passes
  borrow on purpose and still use `resolve_chunk_map`.
- **Cleanup:** `ChunkMap::into_flat` had no caller left and is removed. The doc comments
  on `InodeRecord::chunk_map`, `resolve_chunk_map` and `read::committed_inode` now name
  both entries, and `docs/design/architecture/06-runtime-view.md:29` says "one resolver"
  rather than "one call".

## Verification
- **Claim:** for a committed flat object whose resolve did not restart, the reader task
  gets the record's own chunk-list allocation, with no copy of the list in any form.
  - **Checked:** base `crates/server/src/lib.rs:418-451` and `crates/core/src/metadata.rs:3007`
    on `main` @ 7bf7eb906c634d57d28f2ae26fd0b412732948d5 — the copy described above.
  - **Checked:** on this branch, `crates/core/src/metadata.rs:3117-3169`
    (`resolve_chunk_map_owned`; flat arm moves `chunks` out of the destructured record)
    and `crates/server/src/lib.rs:418-454` (calls it, takes framing fields by move). From
    `read::committed_inode` to the reader task there is no `.clone()`, `to_owned()`,
    `into_owned()` or `to_vec()` on an `InodeRecord`, `ChunkMap` or `[ChunkRef]`; the
    one `Arc::clone` is the gateway handle.
  - **Test:** `crates/core/tests/streamed_read_chunk_map_ownership.rs` (new). It builds a
    flat record with 3 chunks, each with a 3-entry placement (an empty `Vec` would hide a
    copy), and asserts the resolved list's `as_ptr()` and the first `placement.as_ptr()`
    equal the originals. Passes with the fix. Fails when the flat arm copies the list
    (`chunks.to_vec()`, which is what `main` does) and when it copies only the placement
    vectors.
- **Claim:** segmented, superseded and deleted objects answer as before.
  - **Test:** the same file. A segmented record answers its chunks in table order; a
    record superseded mid-resolve restarts onto the live generation and is framed by it;
    a record deleted mid-resolve answers `Ok(None)`. All 4 tests pass.
- **Claim:** existing GET behaviour is unchanged.
  - **Test:** `cargo test -p wyrd-server --test read_path --test s3_http_wire
    --test s3_range_conditional --test segmented_object_read` — 4/4, 19/19, 17/17, 2/2.
  - `cargo fmt --all -- --check`, `cargo clippy -p wyrd-core -p wyrd-server --all-targets
    -- -D warnings` and `typos` over the touched files are clean.

**Limits of the evidence:**
- The new test calls a function this PR adds, so with only the production code reverted
  it does not compile. Instead of a red run on `main`, the evidence is the two copying
  variants above, which both fail.
- No test pins the server call site to the owning entry. Switching
  `get_object_streaming` back to `resolve_chunk_map(..).into_owned()` keeps every GET
  suite green, so that line is covered by review only. A server-side pin would need a
  new public item in `wyrd-server`; that is better as a follow-up.
- `cargo xtask ci` currently fails on `main` itself, independent of this PR: the
  all-features advisory scan (`deny-all-features.toml`) reports RUSTSEC-2026-0346 and
  RUSTSEC-2026-0349 on rustls 0.21.12, pulled in through tikv-client 0.4.0. This PR does
  not touch any dependency or deny file. Every other `ci` step passes.

Fixes #686
