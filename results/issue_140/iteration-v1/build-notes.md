# Build notes — issue 140 / m3.2-chunkstore-list-delete

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

## The change, by file (paths on the worktree off `origin/main` @ 3ca818b)

1. **`crates/proto/proto/wyrd/v0/chunk.proto`** — added `FragmentListRequest` /
   `FragmentListResponse { repeated FragmentId ids }` and `FragmentDeleteRequest
   { FragmentId id }` / `FragmentDeleteResponse {}`, and two rpcs on the existing
   `service ChunkStore` (`ListFragments`, `DeleteFragment`). Pure addition —
   existing messages/rpcs and their field numbers are untouched, so a one-version
   gap interoperates (§8.7 / ADR-0002). Codegen is at build time (protox →
   tonic-prost-build), so no `.rs` is committed.

2. **`crates/traits/src/lib.rs:78-95`** (post-edit) — two `async fn`s added to
   `trait ChunkStore` between `get_fragment` and `health`, with the binding 0005
   signatures verbatim. Doc-comments pin the two semantics chosen below.

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
   `TransportError` classification. Added the two request types to the import.

5. **`crates/chunkstore-grpc/src/server.rs`** (`ChunkStoreService`) — the matching
   D-server rpc handlers: `list_fragments` delegates to the injected store and
   maps ids to wire via `conv::to_wire_fragment_id`; `delete_fragment` validates
   the id and delegates. Store errors become `Status::internal`, mirroring the
   existing put/get handlers. The service stays dumb — it moves/enumerates bytes,
   makes no placement judgement.

6. **`crates/chunkstore-grpc/src/fanout.rs`** (`FanoutChunkStore`) — `list_fragments`
   is the union over backends (disjoint by construction: `route` places each index
   on exactly one store, so no de-dup); `delete_fragment` routes by `index % n`
   exactly as `put`/`get` do.

7. **Five existing `ChunkStore` test fakes** updated to satisfy the now-larger
   contract (required for the workspace to compile — see "Required vs. default"):
   `crates/dst/tests/network.rs` (`DStore`, HashMap-backed),
   `crates/core/tests/write_fanout.rs` (`FaultStore`, HashSet-backed),
   `crates/server/tests/dst_read_fanout.rs` (`ArrivalStore`, delegates to inner fs),
   `crates/server/tests/read_fanout.rs` (`FaultStore`, delegates to inner fs).
   Each gets a faithful list/delete over its own storage, so the existing M2 DST
   and read/write-fanout properties keep meaning.

8. **Tests:**
   - `crates/chunkstore-grpc/tests/list_delete.rs` (NEW, the brief-named file) —
     one `list_and_delete_round_trip(&impl ChunkStore)` body run twice: once
     **in-process** over `FsChunkStore`, once over **local-tonic** via
     `GrpcChunkStore` against a loopback `ChunkStoreService` (real HTTP/2 + prost),
     mirroring `round_trip.rs`. It asserts the full criterion: empty store lists
     nothing; after three puts `list_fragments` == exactly those ids (set equality —
     order is unspecified); bytes present **before** delete; `Ok(None)` **after**;
     siblings unaffected in bytes and listing; idempotent re-delete + delete of an
     absent id both `Ok`.
   - `crates/chunkstore-fs/tests/conformance.rs` (supplementary, as the brief
     allows) — `list_and_delete_walk_the_store` and `list_skips_foreign_and_temp_entries`
     cover the fs-specific walk + strict name parse (the `.tmp`/foreign skip is not
     reachable from the grpc test).

## Decisions (the brief's two open Do calls)

- **`delete_fragment` on a missing id → idempotent `Ok(())`** (brief Open
  question; "pick idempotent unless a gate disagrees"). Rationale: GC reclaim is
  retried and can race a concurrent reconstruction's own cleanup; making "already
  gone" an error would force every caller to special-case `NotFound`. Idempotent
  is the contract documented on the trait and realised in fs (`NotFound → Ok`),
  the fanout (delegates), and the gRPC path (the server simply reports success).
- **`list_fragments` returns a single `Vec`** (not a stream) — 0005 specifies the
  `Vec` signature for M3; streaming for a large store is an explicit 0005 Open
  question left out of scope here.

## Required trait methods vs. default impls (the one design fork)

I made both methods **required** (no default body), matching 0005's signature
block exactly. The alternative — give the trait default impls returning an error —
would have avoided editing the five test fakes (8 + 9 + 9 + 8 + ... ≈ 49 added
lines across 4 files). I rejected it on cost-of-correctness, not diff size: a
default that silently "doesn't support enumeration" weakens a contract the whole
maintenance plane (scrub/GC, 0005 §"four custodian loops") depends on every store
honouring, and would let a future backend ship un-walkable while compiling green.
The smallest change that *restores the invariant* ("a store can be walked / a
fragment deleted") is a required method implemented everywhere; the fake edits are
faithful (each backs onto the fake's own storage), so the M2 DST/fanout properties
keep their meaning rather than being stubbed to `unimplemented!()`.

## Scope discipline

- Reverted an **unrelated `Cargo.lock` drift** the build surfaced (the lock had a
  stale `wyrd-proto` entry under `wyrd-core`, pre-existing on `origin/main`; my
  change adds no dependency, so it is not mine to carry).
- Out of scope and untouched, per the brief: the GC/scrub loops that *call* these,
  `sweep_expired_leases` promotion, `put`/`get`/`health` behaviour, and any
  `format_version` / on-disk-format change.

## Verification (red→green)

Run through the project's gate (`./engine/xtask.sh` → `cargo xtask` in
`$PDCA_WORKTREE`); the targeted runs I executed:

- `cargo test -p wyrd-chunkstore-grpc --test list_delete` → **2 passed**
  (`list_and_delete_in_process`, `list_and_delete_over_grpc`).
- `cargo test -p wyrd-chunkstore-fs --test conformance` → **9 passed** (7 prior + 2 new).
- `cargo test -p wyrd-core --test write_fanout`, `-p wyrd-server --test
  read_fanout`/`dst_read_fanout` → compile (fakes updated).
- `cargo clippy -p wyrd-traits -p wyrd-chunkstore-fs -p wyrd-chunkstore-grpc -p
  wyrd-proto --all-targets` → clean (gate is `-D warnings`).
- `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --no-run` → compiles (the
  network.rs `DStore` edits build under the simulator, ADR-0009).
- `cargo fmt` applied to every touched crate (commit-hook readiness).

**Red without the fix:** these are NET-NEW affordances (template posture (a)), so
red is *criterion-absence* — with `traits/src/lib.rs` and `chunkstore-grpc/src`
reverted, `list_delete.rs` references methods that don't exist and does not
compile. That is exactly what the C4-verify gate isolates (revert production, keep
the added test → red), and what the brief specifies for a net-new method.

Real-network (docker-compose) coverage in `tier2_integration.rs` is observable
only off-Check (needs a Docker host) — supplementary, confirmed by Tier-2 CI.
