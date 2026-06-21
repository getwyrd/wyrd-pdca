# Build notes — issue #155 (M2.8 static-endpoints local cluster), iteration 3

## What the carry-forward demanded (and the real root cause)

Iteration 2 was rejected (bundle `iteration-v2/`) because its cluster path composed
the **`Gateway` struct** (`crates/server/src/lib.rs:42`) over the gRPC fan-out, and
the `Gateway` allocates ids from **in-process counters** reset every invocation:

- `next_inode: AtomicU64::new(1)` (`lib.rs:71`), bumped at `lib.rs:126`.
- `next_chunk: AtomicU64::new(1)` (`lib.rs:72`), minted at `lib.rs:146-148`.

Paired with a *persistent* redb under `--data-dir`, the second distinct-key
`wyrd put --endpoints` in a fresh process re-allocates inode 1; `metadata::create`'s
`require_absent(inode_key(id))` then rejects the create as a bogus
`GatewayError::Conflict` ("a concurrent writer won"). The reviewer's required next
step: *route the cluster path's inode allocation through the persisted `meta:next_inode`
counter, exactly as the local-disk path already does via `alloc_inode` (`cli.rs:319`)*,
and add coverage that stores **two** distinct keys across **separate** compositions.

There is a **second, latent** half the inode story masks: chunk ids. The `Gateway`
mints chunk ids from `next_chunk` (1, 2, 3…), also reset per process. Fragments are
stored keyed by `FragmentId { chunk, index }` — on disk `…/<32-hex chunk>/<05-index>.frag`
(`crates/chunkstore-fs/src/lib.rs:148-151`); the gRPC/fanout stores route by the same
id. So once inode allocation is fixed, the *second* object (fresh process) still re-mints
chunk id 1 and **overwrites the first object's fragments on the shared D servers** —
silent cross-object corruption, not just a failed PUT. The local-disk path already
avoids both: `alloc_inode` (persistent inode) **and** `chunk_id_minter(inode_id)` =
`inode << 64 | seq` (`cli.rs:339-348`), unique across objects and stable across
processes.

## The fix: mirror the local-disk path, swap only the chunk store

The minimal change that restores the invariant ("storing several distinct objects
across separate invocations must work, byte-identically") is to make the cluster CLI
path use the **same free-function write/read composition** the local-disk path uses —
`alloc_inode` + `chunk_id_minter` + `write::write_new_object` / `read::read_path`
(`crates/core/src/{write.rs:216,read.rs:160}`) — with the on-disk `FsChunkStore`
swapped for a `FanoutChunkStore<GrpcChunkStore>`. That reuses the proven, persisted,
collision-free id machinery wholesale; it does **not** reinvent allocation in a second
place.

New in `crates/server/src/cli.rs` (after `chunk_id_minter`):
- `parse_endpoints` / `connect_fanout` / `GrpcFanout` — build the
  `FanoutChunkStore<GrpcChunkStore>` from the `--endpoints` list (the brief's chunk
  plane), connecting up front so an unreachable D server is a clear startup error.
- `open_cluster_meta` — opens redb under `--data-dir` (metadata + persisted inode
  allocator held locally; no local chunk store — fragments cross the wire).
- `cluster_store_put` / `cluster_store_get` — `pub` so the test drives the exact
  shipping path. `put` = `alloc_inode` → `chunk_id_minter(inode)` →
  `write::write_new_object` over the fan-out; `get` = `read::read_path`.
- `cmd_put`/`cmd_get` gain an `--endpoints` branch (`cli.rs:148-153`, `cli.rs` get
  branch) before the local-disk `open_backends`, routing to `cluster_put`/`cluster_get`
  on a multi-thread tokio runtime (the gRPC clients are async, unlike the pollster
  local paths).

## Why not keep the `Gateway` struct (the brief names it)

The brief's prose says "composing `Gateway`". I rejected keeping the `Gateway` struct
on a concrete cost, not an adjective:

- Inode alone is insufficient (chunk-id collision corrupts, above), so a "seed the
  counter" patch does not restore the invariant.
- Making `Gateway` allocate **both** ids persistently means restructuring
  `put_object` (`lib.rs:110-138`): chunk ids are minted **inside** `plan_write` via a
  *synchronous* `FnMut() -> ChunkId` (`lib.rs:111-113`) **before** the inode is known,
  whereas a persistent allocation is `async` (read-modify-write over the metadata
  store). Deriving chunk ids from the inode the way the local-disk path does requires
  allocating the inode *first*, i.e. inverting the create/overwrite resolution and the
  plan/commit ordering — a change to the four-phase write composition the brief lists
  **out of scope** ("any change to the … commit protocol"), and one that touches every
  existing `Gateway` user (`demo`, `tests/round_trip.rs`, the Tier-2 test).
- The local-disk CLI path is itself **not** a `Gateway` — it is exactly this
  free-function composition (`cli.rs:155-173`). "Gateway client mode" is the
  access-layer composition (metadata store + fan-out chunk store + the client
  write/read paths), which is what this delivers; the reviewer's explicit instruction
  ("exactly as the local-disk path already does via `alloc_inode`") points at this path.

So the diff adds the cluster path beside the local-disk one and shares the allocator,
rather than rebuilding the write protocol inside `Gateway`.

## Test (red → green)

`crates/server/tests/gateway_cluster.rs` stands up **four** real loopback gRPC D
servers (real tonic transport, as `chunkstore-grpc/tests/round_trip.rs` does — no
in-memory fake), then over **one** `--data-dir`:

1. Composition A (own scope, dropped to release the redb lock — models a first
   `wyrd put` *process*) stores `obj/one`.
2. Composition B (fresh `open_cluster_meta` + `connect_fanout` over the same dir)
   stores the **distinct** key `obj/two`, asserting `Committed` — this is the exact PUT
   that failed as a bogus `Conflict` in iteration 2.
3. **Both** objects round-trip byte-identically. Reading `obj/one` back *after*
   `obj/two`'s PUT is the chunk-id-collision guard: under per-process counters
   `obj/two` would have clobbered `obj/one`'s fragments.
4. A missing key returns `Ok(None)`.

Four servers, not three — matching iteration-1's resolution and the documented
compose: rs(6,3) = 9 fragments needing k=6, so a single-server loss must stay above k.

Red→green proof (project runner, Bash-tool timeout):
- **Red:** with `cli.rs` reverted to `main` (`git stash`), the test fails to compile —
  `unresolved imports … cluster_store_get, cluster_store_put, connect_fanout,
  open_cluster_meta` (the shipping client mode does not exist on `main`).
- **Green:** `cargo test -p wyrd-server --test gateway_cluster` → `1 passed`.
- **Whole gate:** `./engine/xtask.sh ci` (= `cargo xtask ci`: fmt --check, clippy -D
  warnings, build, full test incl. DST, deny, conformance) → **"xtask ci: all checks
  passed"**. `cargo fmt --check` clean, so the patch is commit-ready for the target's
  hooks.

## Docker compose + README (user-facing on-ramp)

- `docker-compose.yml` (repo root; **distinct** from the CI fixture at
  `crates/chunkstore-grpc/tests/docker-compose.yml`): four `d-server` services on
  **fixed** host ports 50051–50054, **a persistent named volume each**, built from the
  existing `crates/chunkstore-grpc/tests/dserver/Dockerfile` (`ENTRYPOINT ["wyrd"]`,
  identical on `main`).
- `README.md` "Run a local cluster": the `docker compose up` → `wyrd put …
  --endpoints … --data-dir …` → `get` → `diff` walk-through, now storing **two**
  distinct objects under one `--data-dir` (closing the one-object window the
  carry-forward flagged), with the static-endpoints / M3-discovery limits called out.

## Verification scope / honesty

`cargo xtask ci` is green and the loopback test exercises the **shipping**
`cluster_store_put`/`get` over a real networked `FanoutChunkStore<GrpcChunkStore>`
(the brief's C4-verify criterion). The end-to-end `docker compose up` across containers
is the brief's *supplementary* manual/nightly tier, not run here. Patch verified to
apply cleanly on a fresh `main` worktree (`git apply --check` exit 0); `cli.rs`,
`lib.rs`, `fanout.rs`, `traits` are byte-identical between the working branch and
`main`, so the green result is representative of the target branch.
