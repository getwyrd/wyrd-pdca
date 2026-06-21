# Build notes — issue 140 / m3.2-chunkstore-list-delete (iteration 2)

## Carry-forward addressed (from iteration 1)

The iteration-1 sign-off rejected PR #186 not on its design but on its **base**:
it was built off pre-M3.1 `main` and conflicted with M3.1 (#185, since merged) on
`chunkstore-fs/src/lib.rs` and `chunkstore-grpc/src/fanout.rs`. The driver's
note: "Re-Do off current main (which now carries the placement record) for a
clean rebuild."

This iteration is a **clean rebuild off current `main` (d7bf64d, M3.1 merged)**.
The design is unchanged (the iteration-1 sign-off did not fault it); what changed
is that every hunk now lands against the post-M3.1 tree:

- `chunkstore-fs/src/lib.rs` — M3.1 added `PlacementChunkStore` to the
  `use wyrd_traits::{…}` line and an `impl PlacementChunkStore for FsChunkStore`.
  The import hunk now extends `{ChunkStore, FragmentId, Health, PlacementChunkStore,
  Result}` → adds `ChunkId`, instead of the pre-M3.1 `{ChunkStore, FragmentId,
  Health, Result}`. No conflict.
- `chunkstore-grpc/src/fanout.rs` — M3.1 rewrote the doc header and added an
  `impl PlacementChunkStore for FanoutChunkStore`. The `list_fragments` /
  `delete_fragment` additions slot into the existing `impl ChunkStore` block
  after `get_fragment`, untouched by M3.1's edits. No conflict.
- **New** `crates/core/tests/placement_record.rs` (added by M3.1) carries a
  `Fleet` fake implementing `ChunkStore`; it did not exist in iteration 1. It
  now gets `list_fragments` (union over its per-server `HashMap`s) and
  `delete_fragment` (routes by the fake's own `index_route`, mirroring its
  `put_fragment`), so `wyrd-core`'s test suite still compiles and the M3.1
  placement properties keep their meaning.

`git apply --check` confirms the bundle patch applies cleanly on d7bf64d.

## What the brief asked for

Add the two `ChunkStore` affordances M1/M2 left out (accepted proposal 0005,
§"`ChunkStore`: enumerate + delete", PR-sequence step 2):

- `list_fragments(&self) -> Result<Vec<FragmentId>>` — scrub walks the store.
- `delete_fragment(&self, id: FragmentId) -> Result<()>` — GC reclaims bytes.

…on the trait, the gRPC `ChunkStore` service, and **both** backends, additively
(fields/rpcs never repurposed, ADR-0002 wire rule), keeping the D server dumb.

**Success criterion:** a store can be enumerated and a fragment's bytes deleted
over real tonic and in-process; `list_fragments` returns exactly the held ids,
and after `delete_fragment(id)` a `get_fragment(id)` returns `Ok(None)` while
other fragments are unaffected.

## The change, by file (paths on `origin/main` @ d7bf64d)

1. **`crates/traits/src/lib.rs:92-95`** — two required `async fn`s added to
   `trait ChunkStore` between `get_fragment` and `health`, with the binding 0005
   signatures verbatim. Doc-comments pin the two semantics chosen below. Also
   corrected the now-stale parenthetical in `PlacementChunkStore`'s doc
   (`lib.rs:105-107` on main) that said `ChunkStore` "gains **no** methods (the
   enumerate/delete additions are a separate slice)" — this *is* that slice, so
   the sentence would otherwise ship false. Minimal accuracy fix; no behaviour.

2. **`crates/proto/proto/wyrd/v0/chunk.proto:41,63`** — added
   `FragmentListRequest` / `FragmentListResponse { repeated FragmentId ids }`
   and `FragmentDeleteRequest { FragmentId id }` / `FragmentDeleteResponse {}`,
   and two rpcs on the existing `service ChunkStore` (`ListFragments`,
   `DeleteFragment`). Pure addition — existing messages/rpcs and their field
   numbers are untouched, so a one-version gap interoperates (§8.7 / ADR-0002).
   Codegen is at build time (protox → tonic-prost-build), so no `.rs` is
   committed and no new dependency is pulled (cargo-deny stays green; the only
   `Cargo.lock` delta the build surfaced is a pre-existing stale `wyrd-proto`
   entry under `wyrd-dst`, **not mine** — excluded from the patch).

3. **`crates/chunkstore-fs/src/lib.rs`** — `list_fragments` is the ILLUSTRATIVE
   directory walk: it inverts `fragment_path`'s `root/<32-hex chunk>/<05-index>.frag`
   layout across two directory levels. New private helpers `parse_chunk_dir_name`
   (exactly 32 hex digits → `ChunkId`) and `parse_fragment_file_name` (strip
   `.frag`, parse `u16`) make the parse strict, so a `.tmp` from an interrupted
   put or any foreign entry is skipped, never a phantom fragment. A missing root
   reads as an empty walk. `delete_fragment` is `fs::remove_file` with `NotFound`
   mapped to `Ok(())` (idempotent). Added `ChunkId` to the `wyrd_traits` import.

4. **`crates/chunkstore-grpc/src/client.rs`** (`GrpcChunkStore`) — `list_fragments`
   issues `ListFragments` and maps each wire id back via `conv::from_wire_fragment_id`;
   `delete_fragment` issues `DeleteFragment`. Both reuse the existing
   `TransportError` classification.

5. **`crates/chunkstore-grpc/src/server.rs`** (`ChunkStoreService`) — the matching
   D-server rpc handlers: `list_fragments` delegates to the injected store and
   maps ids to wire via `conv::to_wire_fragment_id`; `delete_fragment` validates
   the id and delegates. Store errors become `Status::internal`, mirroring the
   existing put/get handlers. The service stays dumb.

6. **`crates/chunkstore-grpc/src/fanout.rs`** (`FanoutChunkStore`) — `list_fragments`
   is the union over backends (disjoint by construction: `route` places each index
   on exactly one store, so no de-dup); `delete_fragment` routes by `index % n`
   exactly as `put`/`get` do.

7. **Five `ChunkStore` test fakes** updated to satisfy the now-larger contract
   (required for the workspace to compile): `crates/core/tests/write_fanout.rs`
   (`FaultStore`, HashSet-backed), `crates/core/tests/placement_record.rs`
   (`Fleet`, per-server HashMap — M3.1's, new this iteration), `crates/dst/tests/network.rs`
   (`DStore`, HashMap), `crates/server/tests/read_fanout.rs` and
   `crates/server/tests/dst_read_fanout.rs` (both delegate to an inner fs store).
   Each gets a faithful list/delete over its own storage.

8. **Tests:**
   - `crates/chunkstore-grpc/tests/list_delete.rs` (NEW, the brief-named file) —
     one `list_and_delete_round_trip(&impl ChunkStore)` body run twice: once
     **in-process** over `FsChunkStore`, once over **local-tonic** via
     `GrpcChunkStore` against a loopback `ChunkStoreService` (real HTTP/2 + prost),
     mirroring `round_trip.rs`. It asserts the full criterion: empty store lists
     nothing; after three puts (two chunks, a non-zero EC index) `list_fragments`
     == exactly those ids (set equality — order unspecified); bytes present
     **before** delete; `Ok(None)` **after**; siblings unaffected in bytes and
     listing; idempotent re-delete + delete of an absent id both `Ok`.
   - `crates/chunkstore-fs/tests/conformance.rs` (supplementary, as the brief
     allows) — `list_and_delete_walk_the_store` and
     `list_skips_foreign_and_temp_entries` cover the fs walk + strict name parse
     (the `.tmp`/foreign skip is not reachable from the grpc test).

## Decisions (the brief's two open Do calls)

- **`delete_fragment` on a missing id → idempotent `Ok(())`** (brief Open
  question; "pick idempotent unless a gate disagrees"). Rationale: GC reclaim is
  retried and can race a concurrent reconstruction's own cleanup; making "already
  gone" an error would force every caller to special-case `NotFound`. Idempotent
  is documented on the trait and realised in fs (`NotFound → Ok`), the fanout
  (delegates), and the gRPC path (the server reports success).
- **`list_fragments` returns a single `Vec`** (not a stream) — 0005 specifies the
  `Vec` signature for M3; streaming for a large store is an explicit 0005 Open
  question left out of scope here.

## Required trait methods vs. default impls (the one design fork)

Both methods are **required** (no default body), matching 0005's signature block
exactly. The alternative — default impls returning an error — would avoid editing
the five fakes (≈ 58 added lines across 5 files). Rejected on cost-of-correctness,
not diff size: a default that silently "doesn't support enumeration" weakens a
contract the whole maintenance plane (scrub/GC, 0005) depends on every store
honouring, and would let a future backend ship un-walkable while compiling green.
The smallest change that *restores the invariant* ("a store can be walked / a
fragment deleted") is a required method implemented everywhere; the fake edits are
faithful (each backs onto the fake's own storage), so the M2/M3.1 DST/fanout/
placement properties keep their meaning rather than being stubbed.

## Verification (red→green) — through the worktree, target-cached cargo

> Environment note: the per-cycle `$PDCA_WORKTREE` was `git reset --hard
> origin/main` by the harness repeatedly during this Do beat, wiping uncommitted
> edits mid-run. The durable deliverable is **`patch.diff` in the bundle** (not in
> the worktree); every verification below was run by re-applying that patch
> atomically (`git reset --hard HEAD && git clean -fdq && git apply patch.diff`)
> immediately before the cargo invocation, in a single shell so the reset could
> not interleave. The Rust `target/` dir is untracked, so it survives resets and
> the cargo runs are fast.

- **RED** (criterion-absence, template posture (a)): with **only** the new test
  applied onto unmodified `main` (no production change),
  `cargo test -p wyrd-chunkstore-grpc --test list_delete --no-run` fails with
  `error[E0599]: no method named 'list_fragments' found for reference '&impl
  ChunkStore'` (and three `delete_fragment` siblings) → `could not compile`.
- **GREEN** (patch applied):
  - `cargo test -p wyrd-chunkstore-grpc --test list_delete` → **2 passed**
    (`list_and_delete_in_process`, `list_and_delete_over_grpc` — real loopback tonic).
  - `cargo test -p wyrd-chunkstore-fs --test conformance` → **9 passed** (7 prior + 2 new).
  - `cargo test --no-run` for `wyrd-core` (`write_fanout`, `placement_record`) and
    `wyrd-server` (`read_fanout`, `dst_read_fanout`) → all compile (fakes updated).
  - `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test network --no-run` →
    compiles (the `DStore` edits build under the simulator, ADR-0009).
  - `cargo clippy -p wyrd-traits -p wyrd-chunkstore-fs -p wyrd-chunkstore-grpc
    -p wyrd-proto --all-targets` → **clean** (gate is `-D warnings`), with the
    patch confirmed applied (`grep -c list_fragments traits/src/lib.rs` == 2).
  - `cargo fmt --check` → exit 0 (commit-hook readiness).

The whole-tree gate (`cargo xtask ci`, which bundles fmt/clippy/build/test/deny/
conformance) is Check's to run; the above are the fast red→green sanity pass.

## Not verified here / left to Check

- **Real-network (docker-compose)** coverage in `tier2_integration.rs` is
  observable only off-Check (needs a Docker host) — supplementary, confirmed by
  Tier-2 CI, per the brief's Verification posture.
- The **full `cargo xtask ci`** whole-tree run (Check re-gates the real suite).
