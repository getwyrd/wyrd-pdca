# Add a gateway client mode to run a local distributed cluster from the CLI

## Root cause

Wyrd had no user-facing way to exercise the distributed system on one machine:
`wyrd put`/`get` were local-disk-only (`crates/server/src/cli.rs:141`,
`crates/server/src/cli.rs:188`), and the only multi-container setup was the CI
fixture (`crates/chunkstore-grpc/tests/docker-compose.yml`) on ephemeral ports
driven by `cargo xtask integration`. The composable `Gateway` allocates ids
from in-process counters reset every process (`crates/server/src/lib.rs:71-72`),
so a cluster path built on it could not store more than one distinct object per
`--data-dir` across invocations — `metadata::create`'s
`require_absent(inode_key(id))` (`crates/core/src/metadata.rs:144`) rejects the
re-allocated inode as a spurious conflict, and the re-minted chunk id 1 would
overwrite the prior object's fragments.

## Fix

Add an `--endpoints` mode to `put`/`get` that composes a
`FanoutChunkStore<GrpcChunkStore>` from a configured endpoint list and fans each
object's erasure-coded fragments out over gRPC, holding metadata locally under
`--data-dir`. The cluster path mirrors the local-disk path exactly — same
`alloc_inode` (persisted `meta:next_inode`) + inode-derived `chunk_id_minter` +
`write::write_new_object` / `read::read_path`, swapping only the on-disk chunk
store for the gRPC fan-out — so distinct objects across separate invocations get
distinct, persisted, non-colliding ids. Ships with a user-facing
`docker-compose.yml` (four D servers, fixed published ports 50051–50054, a
persistent volume each — distinct from the CI fixture) and a README "Run a local
cluster" walk-through. No change to the `ChunkStore` trait, proto, or commit
protocol; dynamic discovery / placement / rebalance stay milestone-3.

## Verified against

- `crates/server/src/cli.rs:141`,`crates/server/src/cli.rs:188` — the local-disk
  `put`/`get` paths the new `--endpoints` branch sits before; the cluster branch
  routes ahead of `open_backends` so the local-disk behavior is unchanged.
- `crates/server/src/cli.rs:321`,`crates/server/src/cli.rs:341` — `alloc_inode`
  (persisted `meta:next_inode`) and `chunk_id_minter` (`inode << 64 | seq`), the
  persisted id machinery the cluster path reuses wholesale.
- `crates/server/src/lib.rs:71-72` — `Gateway`'s in-process `next_inode` /
  `next_chunk` counters, reset per process; the reason the cluster path does not
  compose the `Gateway` struct.
- `crates/core/src/metadata.rs:144` — `require_absent(inode_key(id))`, the create
  guard that turned a re-allocated inode into the spurious conflict on main.
- `crates/chunkstore-grpc/src/fanout.rs:26`,`crates/chunkstore-grpc/src/lib.rs:28`
  — `FanoutChunkStore` and the `GrpcChunkStore` client the cluster path dials and
  composes; previously the only user was the type definition itself.
- `crates/chunkstore-grpc/tests/docker-compose.yml` (main) — the CI-only fixture
  the new root `docker-compose.yml` is kept distinct from.

## Test

`crates/server/tests/gateway_cluster.rs` (new) stands up four real loopback gRPC
D servers and, over one `--data-dir`, stores two distinct keys across two
separate gateway compositions (modelling separate `wyrd put` processes), then
round-trips both byte-identically and asserts a missing key returns `Ok(None)`.
It drives the shipping `cluster_store_put` / `cluster_store_get` functions
directly. Red→green: on `main` the test fails to compile (the client mode does
not exist); with the change, `cargo test -p wyrd-server --test gateway_cluster`
passes and `cargo xtask ci` (fmt/clippy/build/test incl. DST/deny/conformance)
is green. The containerized `docker compose up` + `wyrd put/get` across
containers is the brief's supplementary manual/nightly tier, exercised by hand.

Fixes #155
