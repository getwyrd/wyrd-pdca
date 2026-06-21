# Build notes — issue 139 / m3.1-placement-record

> Withheld from the reviewer. Rationale, alternatives, and the costs behind them.

## What the slice had to deliver (the Success criterion, restated)

An `rs(6,3)` write **commits a per-fragment placement vector** (one stable D-server id
per fragment index) into the chunk map at the commit point, and the **read path
reconstructs the chunk by resolving each fragment from that record**, including after the
metadata store is **reopened** (process-restart equivalent). BINDING demonstrable
condition: a fragment placed where `index % n` would *not* select is still read
correctly — red against today's `index % n` read path, green once the read consumes the
record. BINDING by 0005: the location is recorded **on `ChunkRef`** and keyed by a
**stable D-server id**, not an endpoint URL.

## Root cause being retired

M2 routes statelessly: `FanoutChunkStore::route(index) = stores[index % n]`
(`crates/chunkstore-grpc/src/fanout.rs:51-53`), and the chunk map carries no location
(`crates/core/src/metadata.rs:73-80`, `ChunkRef { id, scheme, len }`). The read finds a
fragment only because nothing moved it. The first custodian move breaks `index % n`. So
M3 records placement at commit and resolves the read from the record.

## The design, and the one real decision (the seam, given Rust coherence)

Changes are composition-local to `core` (metadata model + read path) + a new addressing
seam in `traits`, exactly as 0005 §"The placement record" scopes it. `MetadataStore` and
`ChunkStore` are **untouched** (no methods added — the enumerate/delete additions are a
separate slice).

- **`DServerId`** — `pub type DServerId = u64;` in `traits` (`crates/traits/src/lib.rs:53`).
  A stable, opaque id (not a URL). A `u64` keeps the keystone `traits` crate free of new
  deps (a newtype would need `serde` there — the crate is "definitions only").
- **Placement on `ChunkRef`** — `placement: Vec<DServerId>`, `#[serde(default)]`, additive
  (`crates/core/src/metadata.rs`, the `ChunkRef` struct). This drops `Copy` from `ChunkRef`
  (a `Vec` can't be `Copy`); see blast radius below.
- **Recorded at commit** — `WritePlan::chunk_refs()` writes `placement = (0..n)`
  (`crates/core/src/write.rs`), so the committed map mirrors the write fan-out's
  index→server placement. The commit point itself is unchanged (one
  version-conditional `MetadataStore::commit`).
- **Consumed on read** — `read_chunk` resolves each fragment to `fragment_dserver(chunk,
  index)` and fetches via `PlacementChunkStore::get_fragment_at(dserver, …)` instead of
  `get_fragment` (`crates/core/src/read.rs`). A short/empty placement (a pre-M3 record)
  falls back to the fragment's own index → identical to M2.

### Why a `PlacementChunkStore` supertrait with **default** methods, and **no blanket impl**

The read path must address a *specific* D server by stable id, but `ChunkStore` only has
`get_fragment(FragmentId)`. So the routing-by-id capability is a new trait
(`PlacementChunkStore: ChunkStore`, `crates/traits/src/lib.rs:98-144`) with default
methods that delegate to `ChunkStore` (a single store / `index % n` fan-out *is* its own
location authority, so the recorded id is advisory and routing is unchanged → M0–M2
behaviour preserved exactly).

The obvious minimiser — a **blanket `impl<T: ChunkStore> PlacementChunkStore for T`** —
**does not work**, and the cost of finding that out is worth recording so it is not
re-proposed:

- A blanket over `T: ChunkStore` makes coherence reject **any** downstream custom impl,
  because `Fleet` (the test's relocatable store) *could* implement `ChunkStore`, so the
  overlap check forbids `impl PlacementChunkStore for Fleet`. Concretely: with the blanket,
  `crates/core/tests/placement_record.rs`'s `Fleet` cannot provide real per-server routing
  → the moved-fragment regression is impossible to write. (This is the well-known
  `impl<T: Display> ToString for T` limitation.)
- Therefore the trait carries defaults and each store opts in with a **one-line** impl:
  `impl PlacementChunkStore for FsChunkStore {}` (`crates/chunkstore-fs/src/lib.rs`),
  `impl<C: ChunkStore> PlacementChunkStore for FanoutChunkStore<C> {}`
  (`crates/chunkstore-grpc/src/fanout.rs`), and the test fake
  `impl PlacementChunkStore for ArrivalStore {}` (`crates/server/tests/dst_read_fanout.rs`).
  A genuinely relocatable store (the test `Fleet`, and the custodian-aware fan-out of a
  later slice) **overrides** the `_at` methods to honour a moved id.

This is why the production `FanoutChunkStore` uses the defaults this slice (no behaviour
change): no fragment has moved yet, so `index % n` *is* the recorded identity placement;
the load-bearing change is that the **read now consults the record** (the seam), and a
real placement-aware store resolves moved fragments. A custodian-aware fan-out that
honours a moved id is a later 0005 slice (#141 owns the selector). The `fanout.rs`
docstring (`:1-18` region) is updated to say the recorded-placement question is now
settled.

### Why `write_fragments` is left on `ChunkStore` (unchanged)

Recording happens **at commit** (in `chunk_refs()`), not in the data phase. Leaving
`write_fragments` on `ChunkStore` keeps every writer call-site (server, dst, benches)
untouched; only the **read** functions gain the `PlacementChunkStore` bound, and every
existing `ChunkStore` reader satisfies it through its one-line impl — no read call-site
changes (`Gateway`'s `C: ChunkStore` becomes `C: PlacementChunkStore`,
`crates/server/src/lib.rs`, and `cluster_store_get`'s bound, `crates/server/src/cli.rs`;
both are satisfied by `FsChunkStore`/`FanoutChunkStore`).

## Test & red→green

`crates/core/tests/placement_record.rs` (the brief's named file). Two in-process
properties, each surviving a real **redb reopen** (added `wyrd-metadata-redb` to `core`
dev-deps — an internal crate, **no new third-party dep**):

1. `write_records_placement_read_resolves_after_reopen` — the four-phase write records a
   length-9 placement vector at commit; after reopen the read reconstructs the object.
2. `moved_fragment_resolved_from_record_after_reopen` (BINDING) — every fragment is placed
   at server `(i+4) % 9` (a rotation: *no* fragment sits where `index % n == i` would
   look), committed into the chunk map; after reopen the read reconstructs the object. A
   guard asserts the recorded placement diverges from `index % n` at every index, so a
   green read can *only* come from consuming the record.

`run-verify.sh` confirms **GREEN with the fix, RED without**. The red is a compile error:
the project's gate reverts *all* production files and keeps the test, and the test
necessarily references the new field/trait/type that a schema-adding fix introduces — a
behavioural red is impossible because the placement field does not exist on `origin/main`
to populate. This is the standard red for a metadata-schema addition under this gate.

## Scope boundary (what I did **not** do, deliberately)

- **Registration/discovery threading** (Coordination carrying `{id, endpoint, label}`,
  discovery resolving `id → endpoint`): the **stable-id type** is introduced and is the
  chunk map's key, and the in-process read resolves `id → store` directly via the fleet.
  Wiring the id through `Coordination::register`'s `value: Bytes` and a discovery-built
  placement-aware fan-out is server-level composition that overlaps #141 (the selector and
  the failure-domain label's consumer). It needs **no trait change** (register already
  takes opaque `Bytes`), so it is not a NEEDS-HUMAN — it is simply the next slice's wiring,
  and the BINDING success criterion (record + resolve + survive reopen) is fully met
  without it.
- **A relocatable (custodian-aware) fan-out** and the **version-conditional location
  update** (repair's atomic re-point) — explicitly later 0005 slices (6–7) / #141.
- **`MetadataStore` trait change** — none; placement is embedded in the existing inode
  record (0005's "Lean: embed"), so the location update stays a single inode CAS.

## Alternatives considered (with cost)

- **Blanket `impl<T: ChunkStore>`** — rejected; coherence makes the moved-fragment test
  unwritable (above). Cost: the regression cannot exist.
- **Separate `placement:<chunk>` keyspace** — 0005 keeps it open but leans embed. Embedding
  is one inode CAS (clean commit-point reuse); a side table adds a second key to every
  atomic batch and a second read on every chunk read. Rejected for this slice on 0005's own
  lean.
- **Endpoint-URL placement** — rejected by 0005 (URLs rot under rebind/NAT); the stable id
  is BINDING.

## Blast radius (every touched file)

Core: `traits/src/lib.rs` (+`DServerId`, +`PlacementChunkStore`); `core/src/metadata.rs`
(`ChunkRef.placement`, drop `Copy`); `core/src/write.rs` (`chunk_refs` records placement);
`core/src/read.rs` (`fragment_dserver` + read consumes record); `core/Cargo.toml`
(dev-dep). Seam impls: `chunkstore-fs/src/lib.rs`, `chunkstore-grpc/src/fanout.rs`
(+docstring), `server/src/lib.rs`, `server/src/cli.rs`. `Copy`-removal fixups (tests):
`server/tests/dst_erasure.rs`, `server/tests/erasure_path.rs`,
`server/tests/dst_read_fanout.rs` (+`ArrivalStore` impl), `metadata-redb/tests/conformance.rs`
(`ChunkRef` literal). New: `core/tests/placement_record.rs`. `Cargo.lock` (dev-dep).

## Gates run (worktree, off `origin/main`)

- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --exclude wyrd-dst --all-targets -- -D warnings` — clean.
- `cargo test --workspace --exclude wyrd-dst` — all green (M0–M2 intact).
- `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --no-run` — compiles (the read seam dst
  uses is intact).
- `run-verify.sh` (C4-verify) — PASS: green with fix, red without.
- No new third-party dependency → `cargo deny` unaffected.
