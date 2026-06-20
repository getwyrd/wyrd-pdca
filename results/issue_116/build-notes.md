# Build notes — issue 116 / m2.6-tier1-network-dst

Target branch: `getwyrd/wyrd @ main` (`f3048d9`). Work branch
`feat/m2.6-tier1-network-dst`. Planning artifact read as authoritative:
`docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`,
§"DST and integration tests (the heart of M2)" → **Tier-1** (proposal lines
283–313), the §"Crate touch-points" rows (336–364), and **suggested PR step 6**
(445–447); plus `docs/design/adr/0009-deterministic-simulation-testing.md`.

## What the success criterion required, and what "done" means here

`cargo xtask dst` (the `--cfg madsim` sweep, `MADSIM_TEST_NUM=50`) green, with a
new network-DST campaign asserting the **five Tier-1 properties over the real
`GrpcChunkStore` on madsim's simulated network**. The end result is not "a test
that passes" but "the real M2.1–M2.5 wire code exercised under seed-reproducible
network faults." So the load-bearing decision was the transport, not the test
shape: the test had to drive the genuine `GrpcChunkStore` client + the genuine
`ChunkStoreService`, over a gRPC transport madsim can fault — i.e. `madsim-tonic`
cfg-aliased as `tonic` (proposal's **primary**, not the in-sim-fake fallback).

## Diagnosis: why the primary (madsim-tonic) was viable, verified not recalled

The brief asserts the version risk is retired; I confirmed it against the actual
crates rather than trusting the note:
- `cargo info madsim-tonic` → `0.6.0+0.14` exists, `tonic/0.14`, madsim `>=0.2.20`
  (workspace has `0.2.34`). Read its source in the registry cache:
  - `madsim-tonic/src/lib.rs` — under `cfg(madsim)` it is the simulator; under
    `not(madsim)` it is `pub use tonic::*` (a transparent drop-in for the real
    build), so the cfg-alias is safe in both modes.
  - `src/transport/server.rs` — `Router::serve(addr: SocketAddr)`; the server
    runs on a madsim node.
  - `src/transport/channel.rs` — `Endpoint::try_from(String)` +
    `connect()` resolve through `madsim::net::lookup_host` → the existing
    `GrpcChunkStore::connect` (`crates/chunkstore-grpc/src/client.rs:28`) maps
    onto it with **no source change**.
  - `madsim-tonic-build/src/server.rs:211` — the generated service-trait method
    signature is byte-for-byte the same as `tonic-prost-build`'s
    (`async fn put_fragment(&self, Request<..>) -> Result<Response<..>, Status>`),
    so `ChunkStoreService`'s impl (`crates/chunkstore-grpc/src/server.rs:46`)
    compiles unchanged under madsim. Module/struct names match too
    (`chunk_store_server::ChunkStore`, `ChunkStoreServer`).
  - `madsim-tonic-build/src/prost.rs:607` — emits the sim stubs into
    `OUT_DIR/sim/<pkg>.rs`; `madsim_tonic::include_proto!` includes that path,
    while real `tonic::include_proto!` includes `OUT_DIR/<pkg>.rs` — so a single
    `tonic::include_proto!("wyrd.v0")` resolves correctly in both modes.

This is what made the cfg-alias a *narrow* change rather than a fork of the wire
code.

## The change (one logical change = proposal step 6)

1. **Workspace `Cargo.toml`** — pin `madsim-tonic` / `madsim-tonic-build` once
   (`Cargo.toml`, the `[workspace.dependencies]` block after `madsim`).
2. **`proto`** — cfg-alias `tonic`→`madsim-tonic` under `cfg(madsim)`
   (`crates/proto/Cargo.toml`); `build.rs` branches on `CARGO_CFG_MADSIM`
   (`crates/proto/build.rs:30`): normal build keeps `tonic-prost-build`'s
   `compile_fds`; the madsim build feeds protox's descriptor set to
   `madsim-tonic-build` via `file_descriptor_set_path` + `skip_protoc_run`, so
   **no system protoc** is needed either way (ADR-0016). `src/lib.rs:17` switches
   to `tonic::include_proto!`.
3. **`chunkstore-grpc`** — same cfg-alias (`crates/chunkstore-grpc/Cargo.toml`);
   `client.rs`/`server.rs`/`fanout.rs` are untouched — they already speak
   `tonic::…`, which now resolves to the simulator under madsim.
4. **`testkit`** — the **network seam** (`crates/testkit/src/lib.rs`): extend
   `FaultPoint` with `FragmentPut`/`FragmentFetch` (line ~117); add `NetFault`
   (Drop/Delay/Partition/Corrupt), `NetFaultInjector`, and `SeededNetFaults`
   (seed-reproducible selection of *which* links to fault), import-light (no
   transport dep — rand only), mirroring the `Disk`/`FaultInjector` shape. Two
   unit tests cover reproducibility-and-bound and per-store reporting.
5. **`dst`** — `crates/dst/tests/network.rs` (new): the Tier-1 campaign + the
   commit-suite re-run over gRPC; `Cargo.toml` gains the needed dev-deps and the
   `cfg(madsim)` `tonic = madsim-tonic` alias.

`Cargo.lock` is updated (commit-ready). `cargo deny` passes unchanged: the new
transitives (chrono, async-stream, tower 0.5, tracing, madsim-tonic) are all
already covered by the MIT/Apache/etc. allow-list (`deny.toml`), so no licence
edit was needed.

## How each property is asserted over the *real* wire

The transport is genuine end-to-end: each property builds `GrpcChunkStore`
clients to `N=9` D-server nodes (each running `ChunkStoreServer::new(
ChunkStoreService::from_arc(store))` over `madsim-tonic`'s `Server`), wrapped in
the real `FanoutChunkStore`, and drives `wyrd_core::{write,read}` — the actual
M2.4 fan-out and M2.5 any-`k` paths.

- **P1 durability** — clean write, then each of the `n` fragments is fetched back
  individually over gRPC from its placed D server (`network.rs` test 1).
- **P2 k-of-n with drops** — after a clean write, `NetSim::clog_node` partitions
  up to `m` seed-chosen D servers; the any-`k` read reconstructs byte-identical
  from the `k` survivors and never blocks on the dropped `m` (test 2).
- **P3 re-read-on-corruption** — up to `m` seed-chosen D servers corrupt their
  `get` bytes; each fails the client-side `decode` checksum, is treated absent,
  and is read around (test 3).
- **P4 fail-closed** — one partitioned D server makes a fan-out put hang; the
  write under a deadline aborts *before* commit; the object never exists and the
  acked fragments are leased garbage the sweep reclaims (test 4).
- **P5 commit suite over gRPC** — `exactly_one_concurrent_writer_wins`,
  replayed with the networked store: four writers fan out over gRPC then race the
  metadata CAS; exactly one wins, version bumps once (test 5). Proves the trait
  seam is real (arc M2).

`SeededNetFaults` draws the faulted set from `madsim::runtime::Handle::seed()`,
so a bug-finding seed replays the *same* faults — the ADR-0009 permanent-
regression rule. (No bug-finding seed surfaced across the 50-seed sweep; none to
commit.)

## A design choice worth flagging: in-memory D-server store behind a real wire

The D server's injected `S: ChunkStore` is an in-memory `DStore`, not
`FsChunkStore`. This is **not** the rejected "ChunkStore-level in-sim fake"
(which would replace the *transport* with a fake and is the proposal's recorded
retreat) — the transport here is the real `GrpcChunkStore` over `madsim-tonic`.
`DStore` is exactly the "fault-injecting fake under DST" the service is *designed*
to host (proposal §"D server": generic over an injected `S`, `FsChunkStore` in
prod, a fake under DST). Cost of the alternative (FsChunkStore per node): madsim
sandboxes/【simulates】per-node filesystem state, so 9 real `FsChunkStore` opens ×
5 tests × 50 seeds risks cross-node fs interference and is slower; the in-mem fake
keeps each D server's storage deterministic and the sweep ~6 s, and it still
honours the *verify-on-put* contract (`DStore::put_fragment` decodes and rejects a
non-fragment, like `FsChunkStore`). The thing under test — the wire — is real.

## Verification (red → green), via the project runner

- **Red:** with the `testkit` seam reverted to `main`,
  `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test network --no-run` fails:
  `error[E0432]: unresolved imports wyrd_testkit::NetFault,
  wyrd_testkit::SeededNetFaults`. On unmodified `main` the file does not exist
  and `cargo xtask dst` runs only the in-process commit suite — i.e. **no
  network-fault coverage**, the exact defect.
- **Green:** `./engine/xtask.sh dst` (the project's gate runner → `cargo xtask
  dst`, `--cfg madsim`, `MADSIM_TEST_NUM=50`) passes: `concurrency` (1) +
  `network` (5) green.
- Non-madsim gate intact: `cargo fmt --all -- --check` clean (commit hook);
  `cargo clippy --workspace --exclude wyrd-dst --all-targets -- -D warnings`
  clean; `cargo test --workspace --exclude wyrd-dst` green (incl. the real-tonic
  `chunkstore-grpc` round-trip and `proto` stubs — the alias did not disturb the
  normal build); `cargo deny check` ok.

## STOP

No PR opened. Draft-only until Check sign-off, per the brief's STOP discipline.
