## Summary

When a custodian moves a fragment to a different D server — during a rebalance
evacuation or a reconstruction re-placement — reads through the chunk-store
fan-out kept fetching that fragment from its *old* location. For a
single-fragment object (no erasure-coding redundancy) the object became
unreadable; for a Reed-Solomon object the read silently recovered around the
misplaced fragment, hiding the loss while eating into the redundancy margin
until enough moves dropped it below `k`. This change makes the fan-out's
placement-aware read/write methods follow the D server the placement record
names, so a moved fragment is found where it actually lives.

## What to look at

- `crates/chunkstore-grpc/src/fanout.rs` — the `PlacementChunkStore`
  implementation for `FanoutChunkStore` (the `get_fragment_at` /
  `put_fragment_at` methods) and the small new `route_dserver` helper next to
  the existing `route`.
- To exercise it: the inline tests in that same file place a fragment directly
  on a *non-identity* backend (where a custodian relocation would have left it),
  then fetch it via `get_fragment_at(moved_dserver, id)` where
  `moved_dserver != id.index % n`. They cover both the `EcScheme::None`
  single-fragment shape and a Reed-Solomon placement rotated so every fragment
  sits off its `index % n` home.

## Root cause

`FanoutChunkStore` implemented `PlacementChunkStore` with the trait's *default*
`get_fragment_at` / `put_fragment_at` (`crates/traits/src/lib.rs:306-319`),
which ignore the resolved `dserver` argument and delegate to
`get_fragment` / `put_fragment`, i.e. `stores[index % n]`
(`crates/chunkstore-grpc/src/fanout.rs:58`). The read path resolves each
fragment's placed D server and passes it to `get_fragment_at`
(`crates/core/src/read.rs:103-104,132-133`), but the fan-out dropped that
resolved server at the trait boundary and re-routed by the fragment's own
(now stale) index — so once a fragment had moved, the fan-out fetched it from
the store it used to live on.

## Fix

Replace the empty `impl PlacementChunkStore for FanoutChunkStore<C> {}`
(`crates/chunkstore-grpc/src/fanout.rs:120`) with overrides of
`get_fragment_at` / `put_fragment_at` that route by the resolved `dserver`
through a new `route_dserver` helper — the same `% n` mapping `route` already
uses, but keyed by the placement-resolved D server instead of the fragment's
index. Because the default identity placement assigns `dserver == index`, an
un-moved fragment computes exactly the store it routed to before, so its
behaviour is unchanged; only a moved fragment (`dserver != index`) diverges and
lands on the store its placement record names. No `FanoutChunkStore::new` call
site changes — only the two `_at` methods change behaviour, and only for a
genuinely moved fragment.

## Verification

- **Claim:** a fragment whose committed placement repoints it to a non-identity
  D server, fetched via `get_fragment_at(placed_dserver, id)`, is returned from
  the store the placement names — not from `stores[index % n]`. The same holds
  for `put_fragment_at` on the write side.
- **Checked:** `crates/traits/src/lib.rs:289-290` documents that a "genuinely
  relocatable fleet … overrides them to honour a moved id" — this impl is that
  override. `crates/core/src/read.rs:103-104,132-133` is the caller that
  resolves and passes the placed D server, which the fan-out now honours instead
  of discarding.
- **Test:** `crates/chunkstore-grpc/src/fanout.rs` (inline tests) —
  `get_fragment_at_resolves_an_ec_none_fragment_moved_off_its_index_home`,
  `get_fragment_at_resolves_every_fragment_in_a_rotated_reed_solomon_placement`,
  and `put_fragment_at_places_on_the_named_dserver_not_index_mod_n`. All three
  fail pre-fix (the moved read returns `None` / the write lands on the wrong
  store) and pass post-fix; the un-moved identity path stays pinned by the
  unchanged `route_places_each_index_on_index_mod_n`. `cargo xtask ci` is green.

Fixes #356
