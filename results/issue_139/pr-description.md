# Record fragment placement so a moved fragment is still found

Implements step 1 of accepted proposal 0005 (Milestone 3 — custodians),
§"The placement record": record placement at the write commit point and
resolve each fragment from that record on read.

## Root cause

M2 routes a fragment statelessly — `FanoutChunkStore::route(index)` returns
`stores[index % n]` (`crates/chunkstore-grpc/src/fanout.rs:51-52`) — and the
committed chunk map carries no location (`ChunkRef { id, scheme, len }`,
`crates/core/src/metadata.rs:73-78`). A read therefore finds a fragment only
because nothing has moved it; the first custodian relocation (the premise of
Milestone 3) makes `index % n` resolve to the wrong D server and the read
fails.

## Fix

The chunk map now carries a per-fragment placement record and the read path
consumes it in place of `index % n`:

- `ChunkRef` gains `placement: Vec<DServerId>` — the stable D-server id holding
  each fragment by index — marked `#[serde(default)]` so it is additive on the
  never-yet-deployed schema (`crates/core/src/metadata.rs`). A pre-M3 record
  decodes with an empty vector and the read falls back to the per-index
  identity placement, so M0–M2 data reads through the same path. (Carrying a
  `Vec` drops `Copy` from `ChunkRef`; clone fix-ups are the only churn in the
  touched tests.)
- The write commit records the identity placement the fan-out used —
  `placement = (0..n)` in `WritePlan::chunk_refs()` (`crates/core/src/write.rs`)
  — so the committed map mirrors where the write put each fragment.
- The read resolves each fragment to `fragment_dserver(chunk, index)` and
  fetches via `PlacementChunkStore::get_fragment_at` rather than `get_fragment`
  (`crates/core/src/read.rs`).
- A new `PlacementChunkStore: ChunkStore` seam introduces the stable
  `DServerId` (a `u64`, keeping the `traits` crate dependency-free) and
  by-id addressing with identity defaults
  (`crates/traits/src/lib.rs`). A single store and the `index % n` fan-out are
  each their own location authority, so they opt in with a one-line impl and
  use the defaults — production routing is byte-for-byte unchanged this slice.
  A blanket `impl<T: ChunkStore>` was rejected: coherence would then forbid a
  relocatable store its own impl, making the regression below unwritable.

Out of scope (later 0005 slices / #141): the custodian-aware relocatable
fan-out, the version-conditional location update used by repair/rebalance, and
threading the stable id through `Coordination` registration / discovery.

## Verified against

- `crates/core/src/metadata.rs:73-78` (target `main`) — `ChunkRef` had no
  location field; placement is added additively (`#[serde(default)]`), the
  `MetadataStore` trait is untouched.
- `crates/chunkstore-grpc/src/fanout.rs:48-52` (target `main`) — the
  `index % n` route and its docstring noting "the recorded-placement question
  is settled at M3"; the fan-out is no longer the location authority and the
  docstring is updated.
- `crates/core/src/read.rs:80-112` (target `main`) — `read_chunk` fetched every
  fragment with `get_fragment` keyed only on index; it now resolves through the
  placement record.
- `crates/core/src/write.rs:60-69` (target `main`) — `chunk_refs()` built a
  `ChunkRef` without placement; it now records the identity vector at commit.
- `crates/traits/src/lib.rs:73` (target `main`) — `ChunkStore` is the only
  store trait; `DServerId` + `PlacementChunkStore` are layered beside it with
  no methods added to `ChunkStore`.

## Test

`crates/core/tests/placement_record.rs` (new) — two in-process properties, each
surviving a real redb metadata-store reopen (the process-restart equivalent):

1. `write_records_placement_read_resolves_after_reopen` — the four-phase write
   records a length-`n` placement vector at commit; after reopen the read
   reconstructs the object from it.
2. `moved_fragment_resolved_from_record_after_reopen` (binding) — every
   fragment is placed at a D server that `index % n` would not select; after
   reopen the read still reconstructs the chunk, with a guard asserting the
   recorded placement diverges from `index % n` at every index, so a green read
   can only come from consuming the record.

Red before / green after is confirmed by `run-verify.sh` (C4-verify); the
whole gate (`cargo xtask ci`, incl. the M0–M2 suites and the DST sweep) passes.

Fixes #139
