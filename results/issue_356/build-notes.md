# Build notes — issue #356 / relocatable-fanout-route-by-placed-dserver

## Root cause (two sentences)

`FanoutChunkStore` took the trait's default `get_fragment_at` / `put_fragment_at`
(`crates/traits/src/lib.rs:306-319`, target branch — pre-fix `fanout.rs:120`), which
ignore the `dserver` argument and delegate to `get_fragment`/`put_fragment` → `route(id.index)`
= `stores[index % n]` (`fanout.rs:58-60`). Once a custodian moves a fragment (placement[i]
repoints to a non-identity D server), the read/write path resolves the *correct* `dserver`
(`crates/core/src/read.rs:127-133` via `ChunkRef::placed_dserver`,
`crates/core/src/write.rs:233-238`) but the fan-out drops it at the trait boundary and routes
by the fragment's own (stale) index instead.

## The fix

`crates/chunkstore-grpc/src/fanout.rs`:
- Added `route_dserver(&self, dserver: DServerId) -> &C` (`:63-74` post-fix), the same
  `% n` indexing `route` already uses (`:58-60`), but keyed by the placement-resolved
  `dserver` instead of the fragment's own index.
- Replaced the empty `impl PlacementChunkStore for FanoutChunkStore<C> {}` (`:120`
  pre-fix) with overrides of `get_fragment_at` / `put_fragment_at` that call
  `route_dserver(dserver)` (`:141-156` post-fix).
- Updated the two doc comments that described the relocatable fan-out as deferred M3
  work (`:12-19`, `:115-119` pre-fix) — that's exactly what this slice now does.

Why this is correct for the "un-moved" (identity) case the brief requires unchanged:
the trait's default `placement(n)` (`crates/traits/src/lib.rs:298-300`) assigns a fresh
chunk's fragment `index` the `DServerId` numerically equal to `index`, and
`write_fragments` falls back to `DServerId::from(*index)` when no placement has been
recorded yet (`crates/core/src/write.rs:233-237`). So for an un-moved fragment
`dserver == index`, and `route_dserver(index as u64)` computes `stores[index as usize % n]`
— bit-for-bit the same store `route(index)` already picked. Only once a custodian commits
a *different* `dserver` for that index does `route_dserver` diverge from `route`, which is
precisely the moved-fragment case the brief targets.

## Alternative considered and rejected: an explicit `DServerId -> store` map

The brief flags this as the illustrative alternative ("Routing by `stores[dserver % n]` vs
a D-server-id→store map is ILLUSTRATIVE — the mechanism is Do's"). I considered changing
`FanoutChunkStore<C>`'s internal `stores: Vec<C>` to `Vec<(DServerId, C)>` (mirroring the
`Fleet` helper in `crates/custodian/tests/rebalance.rs:139-150` / `reconstruction.rs` /
`crates/dst/tests/custodian.rs`, which is itself test-only scaffolding, not a production
type) and resolving `get_fragment_at` by linear lookup instead of modulo arithmetic.

Rejected on cost: `FanoutChunkStore::new(stores: Vec<C>)`'s signature would have to
change to accept `(DServerId, C)` pairs (or a fallible id-assignment scheme), which is a
breaking API change to a `pub fn`. I counted every call site that would need updating
(`grep -rn "FanoutChunkStore::new" crates/ xtask/` on the target branch):

```
crates/core/benches/throughput.rs:92        FanoutChunkStore::new(clients)
crates/dst/tests/network.rs:236,289,346,395,455   FanoutChunkStore::new(clients)  (x5)
crates/server/src/cli.rs:456                 FanoutChunkStore::new(clients)   <- production wiring
crates/server/tests/read_fanout.rs:144       FanoutChunkStore::new(clients)
crates/server/tests/write_fanout.rs:93       FanoutChunkStore::new(clients)
crates/chunkstore-grpc/tests/tier2_integration.rs:95   FanoutChunkStore::new(clients)
```

13 call sites across 7 files, including the one production wiring site
(`crates/server/src/cli.rs:456`, behind `pub type GrpcFanout = FanoutChunkStore<GrpcChunkStore>`
at `:422`) that has no `DServerId` to hand it today (its `clients: Vec<GrpcChunkStore>` is
built purely from configured endpoints, in placement order) — so this alternative would
also need to invent and thread a D-server-id assignment through `cli.rs`'s fleet
construction, well outside this issue's scope (brief "out of scope: ... changing the trait
default or any other `PlacementChunkStore` impl"). The chosen `route_dserver` fix touches
zero of those call sites: `new()`'s signature, and every existing caller, is untouched —
only the two `_at` trait methods change behaviour, and only when `dserver != index`.

I also considered NOT shipping `route_dserver` as a separate method and inlining the
modulo expression directly in `get_fragment_at`/`put_fragment_at`. Rejected for symmetry
with `route` (which exists as a named helper for exactly the same reason) and because
`put_fragment_at` needs the identical expression — a second inline copy is the kind of
duplication `route` itself was already factored out to avoid.

## Test scenarios (inline `#[cfg(test)] mod tests`, `fanout.rs`)

Per the brief's repro instruction (mirroring
`crates/core/tests/placement_record.rs::moved_fragment_resolved_from_record_after_reopen`
but over the fan-out, not the id-indexed fleet):

1. `get_fragment_at_resolves_an_ec_none_fragment_moved_off_its_index_home` — the
   `EcScheme::None` shape (a single fragment at index 0, no redundancy to read around):
   place the fragment directly on a non-identity backend, then fetch via
   `get_fragment_at(moved_dserver, id)`. Pre-fix this is a **total miss** (`Ok(None)`) —
   the exact failure mode the brief's defect section names.
2. `get_fragment_at_resolves_every_fragment_in_a_rotated_reed_solomon_placement` — a
   6-fragment (`rs(4,2)`-shaped) chunk rotated by one store so *every* fragment's
   `dserver` differs from `index % n`, mirroring `placement_record.rs`'s `SHIFT` rotation.
   Pins that each individually moved fragment resolves from its named server.
3. `put_fragment_at_places_on_the_named_dserver_not_index_mod_n` — the write side of the
   same invariant (the brief's "Invariant to restore" covers both `get_fragment_at` and
   `put_fragment_at`).

All three are isolated to the fan-out's `_at` trait boundary: they write fragments
directly to the backing `MemStore` handles (not via `FanoutChunkStore::put_fragment`,
which still — correctly, and unchanged by this fix — routes by `index % n` for the
plain `ChunkStore` API), so a failure can only come from `get_fragment_at` /
`put_fragment_at` themselves, not from write-side routing.

### Why no separate test file in this bundle

The brief's named "Test file" is `crates/chunkstore-grpc/src/fanout.rs`'s inline
`#[cfg(test)] mod tests` (co-located with the fix, not a new `tests/*.rs` file) — I did
not add an end-to-end mirror under `crates/chunkstore-grpc/tests/`, since the inline
location alone already satisfies "the named home must hold a regression that is red
pre-fix and green post-fix" and a mirror would duplicate the same `MemStore` boilerplate
(~50 lines) for marginal benefit (see "How I verified red→green" below for how I covered
the gap that creates in the automated per-fix gate). Per the project's own convention —
`results/issue_250/` shipped the same way (an inline `#[cfg(test)] mod tests` test, named
in its brief, with no separate `.rs` artifact in the bundle, only `patch.diff`) and
`PCDA/quality-cycle.md:61` ("Do: patch.diff + test + build-notes") treats the test as
content *of* `patch.diff`, not a fourth file — there is no `fanout.rs` copy in this
bundle directory; the regression lives entirely in `patch.diff`.

## How I verified red → green (the project's own runner)

1. **Build/lint** (target's own commit-hook tooling): `cargo fmt --package
   wyrd-chunkstore-grpc -- --check` and `cargo clippy -p wyrd-chunkstore-grpc
   --all-targets` both clean after `cargo fmt --package wyrd-chunkstore-grpc` fixed one
   formatting diff (the multi-line `put_fragment_at` body rustfmt collapsed to one line).
2. **GREEN, via the project's configured per-fix gate**: ran
   `PDCA_BUNDLE=results/issue_356 ./engine/scripts/run-verify.sh` from the `wyrd-pdca`
   root (the `C4-verify` gate `pdca.toml:430` wires to). It applies `patch.diff` in the
   isolated `../wyrd-verify` worktree off `origin/main` and runs
   `cargo test -p wyrd-chunkstore-grpc` — **all packages tests pass** (11 lib +
   `list_delete`/`round_trip`/`read_fault_seam`/`tier1_jepsen_consistency`/
   `tier2_integration`/`tier2_kill_reconstruct` suites, the Tier-2 `#[ignore]`d ones
   correctly skipped, no Docker needed). Because the test is co-located with the fix (no
   added `tests/*.rs` file — `run-verify.sh`'s own discriminator, see its header
   comment), the script reports `PASS (green-only)`: it cannot auto-isolate RED for a
   change that touches the same file as its test, by design.
3. **RED, verified by hand in that same isolated worktree** (not a fresh ad hoc command —
   reusing the exact `cargo test -p wyrd-chunkstore-grpc --lib` invocation
   `run-verify.sh` itself runs internally, just with the production hunk manually
   reverted in `../wyrd-verify` since the script can't split a co-located change): with
   `get_fragment_at`/`put_fragment_at` reverted to the original empty
   `impl PlacementChunkStore for FanoutChunkStore<C> {}` and `route_dserver` removed
   (so the revert is byte-for-byte the pre-fix production code, with only the new tests
   kept), all three new tests fail and every pre-existing test still passes:
   ```
   test fanout::tests::get_fragment_at_resolves_an_ec_none_fragment_moved_off_its_index_home ... FAILED
     left: None
    right: Some(b"moved-fragment")
   test fanout::tests::put_fragment_at_places_on_the_named_dserver_not_index_mod_n ... FAILED
   test fanout::tests::get_fragment_at_resolves_every_fragment_in_a_rotated_reed_solomon_placement ... FAILED
     left: None
    right: Some(b"frag-0")
   test result: FAILED. 8 passed; 3 failed; ...
   ```
   This is the exact "miss" the brief's defect section describes. I then restored
   `../wyrd-verify` to clean (`git checkout -- crates/chunkstore-grpc/src/fanout.rs`) so
   it's back to the harness's expected pristine-`origin/main` state for the next run.
4. **Patch hygiene**: confirmed `patch.diff` `git apply --check`s cleanly against a fresh
   `origin/main` checkout (the worktree's `HEAD` already equals `origin/main`, so the
   diff is taken directly off a clean tree, not rebased after the fact).

## Scope discipline

- Did not touch `crates/custodian/src/rebalance.rs` (issue #346's write/drain side —
  explicitly out of scope, "same wave, in parallel", brief "Ordering note").
- Did not change `crates/traits/src/lib.rs`'s default `get_fragment_at`/`put_fragment_at`
  or `placement()` (brief: "out of scope ... changing the trait default or any other
  `PlacementChunkStore` impl"; `FsChunkStore`'s empty impl at
  `crates/chunkstore-fs/src/lib.rs:334` is untouched).
- Did not change `FanoutChunkStore::route` / `ChunkStore::put_fragment`/`get_fragment` —
  the plain (non-`_at`) fan-out API still routes by `id.index % n` exactly as before, so
  an un-moved fragment's write-side placement order is unchanged (brief: "out of scope
  ... any change to write fan-out placement order for an un-moved (identity) fragment,
  which must route exactly as today" — verified by the unchanged-and-still-passing
  `route_places_each_index_on_index_mod_n` test).
