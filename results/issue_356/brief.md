# Brief — issue 356 / relocatable-fanout-route-by-placed-dserver

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** relocatable-fanout-route-by-placed-dserver
- **Defect:** `FanoutChunkStore` implements `PlacementChunkStore` with the **default**
  `get_fragment_at` / `put_fragment_at` (`crates/chunkstore-grpc/src/fanout.rs:120`, the
  empty `impl … {}`; defaults in `crates/traits/src/lib.rs:306-319`), which **ignore the
  `dserver` argument** and delegate to `get_fragment(id)` → `route(id.index)` =
  `stores[index % n]`. The read path resolves each fragment's placed D server
  (`crates/core/src/read.rs:127,132` via `fragment_dserver` → `ChunkRef::placed_dserver`)
  and passes it to `get_fragment_at`, but the fan-out **drops the resolved D server at the
  trait boundary** and routes by fragment index. That is correct only while placement is
  identity. Once a custodian **moves** a fragment (rebalance evacuation or reconstruction
  re-placement repoints `placement[i]`), a read through the fan-out fetches it from the
  **old** store: `EcScheme::None` → total miss → the object is unreadable; `ReedSolomon`
  → the read silently "reads around" the wrong-located fragment, masking the gap while
  eroding the redundancy margin until enough moves drop below `k`. The code names this as
  deferred work (`fanout.rs:17-19`: "Honouring a moved id … is a later M3 slice").
- **Success criterion:** A fragment whose committed placement repoints it to a
  **non-identity** D server, fetched through a `FanoutChunkStore` via
  `get_fragment_at(placed_dserver, id)`, is returned **from the store the placement
  names** — not from `stores[index % n]`. The regression test (a moved / rotated placement
  served over the fan-out) is red pre-fix (routed to the old store → miss) and green
  post-fix, covering `EcScheme::None` (total miss) and a Reed-Solomon move (fragment at an
  index that routes differently from its moved server). BINDING: the moved fragment is
  returned through the fan-out. Routing by `stores[dserver % n]` vs a D-server-id→store
  map is ILLUSTRATIVE — the mechanism is Do's, provided a moved id resolves to its store.
- **Invariant to restore:** A `PlacementChunkStore` must **honour the placement the caller
  resolved**: `get_fragment_at` / `put_fragment_at` fetch/place from the D server named by
  their `dserver` argument, not from `index % n` — so the location authority is the
  committed chunk map (every placement-consuming path agreeing on resolution), and a
  resolved placement is never dropped at the store trait boundary. Source: the trait
  contract itself — `crates/traits/src/lib.rs:289-290` states a "genuinely relocatable
  fleet (a custodian-aware store, later M3 slices) overrides them to honour a moved id,"
  and `fanout.rs:15-19` records exactly this as the deferred M3 slice; proposal 0005 "the
  placement record" (`docs/principles.md` §6 — placement / trait-boundary category: the
  already-resolved placement must not be discarded at the boundary).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`)
- **Depends on:** (none)
- **Conflicts with:** (none)
- **Ordering note:** #356 (read / fan-out side) and #346 (write / drain side) are the two
  twins of the same #292 placement-fallback class, but they touch **disjoint files**
  (`chunkstore-grpc/src/fanout.rs` here vs `custodian/src/rebalance.rs` there) with no
  build-on dependency and no shared resource — they run in the **same wave, in parallel**.
  Both build on the already-merged placement-record machinery (093732d / #139,
  `ChunkRef::placed_dserver`), which is on `main`; no in-batch prerequisite.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** make the fan-out honour the placed D server its `PlacementChunkStore`
  callers resolve — when asked for (or to place) a fragment at a resolved D server, it must
  fetch from / write to **that** server, so a moved fragment is found through the same
  store the placement record names, matching the id-indexed `GrpcChunkStore` fleet path. /
  out of scope: the write-side rebalance evacuation fallback (#346); changing the trait
  default or any other `PlacementChunkStore` impl (the id-indexed `GrpcChunkStore` fleet
  already honours moves); the placement-resolution definition itself; any change to write
  fan-out placement order for an un-moved (identity) fragment, which must route exactly as
  today.
- **Repro instruction:** In `crates/chunkstore-grpc/src/fanout.rs`'s test harness (the
  inline `#[cfg(test)] mod tests` with `MemStore`, which records which backend a put
  landed on): place a fragment on a non-identity store, then call
  `get_fragment_at(moved_dserver, id)` on the `FanoutChunkStore` where
  `moved_dserver != id.index % n`. Today the call returns the wrong store's content (a miss
  / `None`); after the fix it returns the moved fragment. Mirror
  `crates/core/tests/placement_record.rs::moved_fragment_resolved_from_record_after_reopen`
  (a rotation so every fragment lives off its `index % n` home) but over the fan-out rather
  than the id-indexed fleet; cover `EcScheme::None` and a Reed-Solomon move.
- **Test file:** crates/chunkstore-grpc/src/fanout.rs   (the inline `#[cfg(test)] mod tests`;
  an end-to-end mirror in `crates/chunkstore-grpc/tests/` is acceptable if Do prefers, but
  the named home must hold a regression that is red pre-fix and green post-fix)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path. `crates/chunkstore-grpc/src/fanout.rs`:
  the placement-record slice (093732d / #139, "Record fragment placement so a moved
  fragment is still found") landed the empty `impl PlacementChunkStore for FanoutChunkStore {}`
  at `:120` and explicitly **deferred** the relocatable fan-out (the `:17-19` "later M3
  slice" note); later mutant-killing test work (f98cba7 / #225) did not add the override.
  No open/closed PR and no branch (`git branch -a` for 356/fanout/relocatable) addresses
  it. Genuine open defect — this issue **is** that deferred slice.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
