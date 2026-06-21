# Add `list_fragments` + `delete_fragment` to `ChunkStore` (M3.2)

> Implements accepted proposal 0005 (Milestone 3 — custodians), step 2,
> §"`ChunkStore`: enumerate + delete". One logical fix per PR.

## Root cause
The `ChunkStore` contract was `put_fragment` / `get_fragment` / `health` only
(`crates/traits/src/lib.rs:73-84` on `main`), so a store could not be *walked* and a
fragment's bytes could not be *deleted*. The two M3 maintenance loops need exactly
those — scrub must enumerate a D server's actual contents to diff against the chunk
map, and GC must reclaim bytes (today's `core::sweep_expired_leases` deletes ledger
entries but no fragment bytes, because the affordance did not exist).

## Fix
Add `list_fragments(&self) -> Result<Vec<FragmentId>>` and
`delete_fragment(&self, id) -> Result<()>` to the trait, the gRPC `ChunkStore`
service, and both backends, additively and keeping the D server dumb:

- **Trait** — both methods are **required** (no default body), matching the binding
  0005 signatures; doc-comments pin the two semantics below. A default returning
  "unsupported" would let a future backend ship un-walkable while compiling green,
  weakening an invariant the maintenance plane depends on.
- **Proto** — new `FragmentListRequest`/`FragmentListResponse { repeated FragmentId
  ids }` and `FragmentDeleteRequest`/`FragmentDeleteResponse`, plus `ListFragments` /
  `DeleteFragment` rpcs on the existing `service ChunkStore`. Purely additive — no
  existing message, field number, or rpc repurposed — so a one-version gap still
  interoperates (ADR-0002 wire rule, §8.7).
- **`chunkstore-fs`** — `list_fragments` walks the `root/<32-hex chunk>/<05-index>.frag`
  layout, the inverse of `fragment_path`; strict name parsing skips a `.tmp` from an
  interrupted put or any foreign entry, so a crash mid-write never surfaces as a
  phantom fragment. `delete_fragment` is `remove_file` with `NotFound` mapped to
  `Ok(())`.
- **`chunkstore-grpc`** — client issues the two rpcs and maps ids via `conv`; the
  D-server handlers delegate to the injected store (store error → `Status::internal`);
  the fan-out unions over disjoint backends for list and routes delete by `index % n`,
  exactly as `put`/`get` do.

Two Do-calls the brief left open are resolved: `delete_fragment` on a missing id is
**idempotent `Ok(())`** (a retried/raced GC reclaim must not error), and
`list_fragments` returns a single `Vec` (0005 specifies the `Vec` signature for M3;
streaming is an explicit out-of-scope 0005 open question). `put`/`get`/`health`
behaviour and the on-disk format are unchanged.

## Verified against
- `crates/traits/src/lib.rs:73-84` (`main`) — the `ChunkStore` trait that previously
  exposed only `put`/`get`/`health`; the two new `async fn`s are added between
  `get_fragment` and `health`.
- `crates/proto/proto/wyrd/v0/chunk.proto:60-63` (`main`) — the existing
  `service ChunkStore` rpc block the additive `ListFragments` / `DeleteFragment` rpcs
  extend; existing rpcs and field numbers untouched.
- `crates/chunkstore-fs/src/lib.rs:70-99` (`main`) — the `impl ChunkStore for
  FsChunkStore` block (the directory-walk + idempotent-delete realisation lands before
  `health` at line 99).
- `crates/chunkstore-grpc/src/client.rs:47-75`,
  `crates/chunkstore-grpc/src/server.rs:46-80`,
  `crates/chunkstore-grpc/src/fanout.rs:57-70` (`main`) — the client, D-server, and
  fan-out `ChunkStore` impls the matching methods extend (each before its `health`).
- On-disk format / conformance vectors unchanged; no new dependency (a `cargo-deny`
  concern and NEEDS-HUMAN per §4 if one appeared — none did).

## Test
- **New, brief-named:** `crates/chunkstore-grpc/tests/list_delete.rs` — one
  `list_and_delete_round_trip(&impl ChunkStore)` body run twice: **in-process** over
  `FsChunkStore` (`list_and_delete_in_process`) and over **local-tonic** via
  `GrpcChunkStore` against a loopback `ChunkStoreService` with real HTTP/2 + prost
  (`list_and_delete_over_grpc`), mirroring `round_trip.rs`. It asserts the full
  criterion: an empty store lists nothing; after three puts `list_fragments` is exactly
  those ids (set equality — order unspecified); the victim's bytes are present *before*
  delete and `Ok(None)` *after*; siblings are unaffected in bytes and in the listing;
  re-delete and delete of an absent id are both `Ok` (idempotence).
- **Supplementary:** `crates/chunkstore-fs/tests/conformance.rs` adds
  `list_and_delete_walk_the_store` and `list_skips_foreign_and_temp_entries`, covering
  the fs-specific walk and strict name parse (the `.tmp`/foreign skip is unreachable
  from the grpc test).
- **Red→green posture:** these are NET-NEW affordances, so "red" is criterion-absence —
  with the trait/grpc production reverted, `list_delete.rs` references methods that do
  not exist and does not compile; with the fix it is green over both in-process and
  local-tonic. Targeted runs: `cargo test -p wyrd-chunkstore-grpc --test list_delete`
  (2 passed), `-p wyrd-chunkstore-fs --test conformance` (9 passed); the five existing
  `ChunkStore` test fakes were updated to the larger contract so the M2 DST/fanout
  properties keep meaning. The real-network docker-compose variant in
  `tier2_integration.rs` is observable only off-Check (needs a Docker host) — confirmed
  by Tier-2 CI, supplementary.

Fixes #140
