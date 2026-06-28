# Fix GC deleting live fragments of pre-M3 committed objects

## Summary

Garbage collection could permanently delete a fragment belonging to a live,
committed object when that object's chunk map predates M3 placement records —
silent data loss on data the reader can still serve. This change makes GC's
"do not delete" reference set cover exactly the fragments the read path resolves,
including the identity fallback used for empty or short placement vectors.

## What to look at

- `crates/custodian/src/gc.rs` — `referenced_fragments`. This builds the set of
  fragments GC must never reclaim. The loop previously walked only the explicit
  placement vector; it now walks every fragment index of each committed chunk and
  resolves the holding D-server through the shared helper.
- `crates/core/src/metadata.rs` — the two new `ChunkRef` methods,
  `placed_dserver` and `fragment_count`, which are now the single definition of
  "where does fragment i live" and "how many fragments does this chunk have".
- To exercise it: seed a committed inode whose `ChunkRef` has an empty (or short)
  `placement` vector plus a stale orphan record for one of its fragments, then run
  GC — see the regression tests in `crates/custodian/tests/gc.rs`.

## Root cause

A pre-M3 / mixed-era `ChunkRef` decodes with `placement: vec![]` (the field is
`#[serde(default)]`), and the read path resolves each such fragment to D-server
`index` via an identity fallback. GC's `referenced_fragments` iterated only
`chunk.placement` directly, so for an empty placement vector it protected no
fragments at all, and any stale orphan or expired pending-ledger entry referencing
that fragment let GC call `delete_fragment` on live committed data. Scrub uses the
same reference-set routine, so it shared the gap.

## Fix

`referenced_fragments` now expands each committed chunk to its full fragment count
and resolves each fragment's D-server through the same identity fallback the read
path uses, so GC's protected set equals the read path's resolved placement closure.
The fallback resolution and the scheme-aware fragment count are centralized as two
`ChunkRef` methods — `placed_dserver(index)` and `fragment_count()` — and the read
path (`fragment_dserver`) and reconstruction now delegate to them, so the
resolution has one definition and cannot drift between callers.

## Verification

- **Claim:** A committed fragment that a chunk-map reference resolves to under the
  read path's placement resolution (identity fallback included) is never passed to
  `delete_fragment`.
- **Checked:** `crates/custodian/src/gc.rs:189` on `main` — the pre-fix loop
  `for (index, dserver) in chunk.placement.iter().enumerate()` yields nothing for an
  empty placement vector; the fix walks `0..chunk.fragment_count()` and resolves
  through `chunk.placed_dserver(index)`.
- **Checked:** `crates/core/src/read.rs:99-105` and
  `crates/custodian/src/reconstruction.rs:227-235` on `main` — the two pre-existing
  inline copies of the identity fallback; both now delegate to the shared
  `ChunkRef` methods, so GC matches the read path by construction rather than by
  duplicated logic.
- **Checked:** `crates/core/src/metadata.rs:93` on `main` — `#[serde(default)]` on
  `placement` is why pre-M3 records decode with `vec![]`, the condition the fix
  handles.
- **Test:** `crates/custodian/tests/gc.rs` — three regression cases (empty
  placement with `EcScheme::None`; empty placement with `ReedSolomon{2,1}` and an
  orphan at index > 0; a short `placement: vec![5]` with an orphan at a fallback
  index). Each fails before the fix (GC reclaims the live fragment) and passes
  after (the fragment is skipped as referenced). Full `cargo xtask ci` passes.

Fixes #287
