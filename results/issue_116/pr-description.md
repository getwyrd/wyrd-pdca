# Add Tier-1 network DST over the real gRPC ChunkStore on madsim

> Targets `getwyrd/wyrd@main`. One logical change: proposal 0004 suggested PR
> step 6 (the Tier-1 network-DST campaign). Draft — the maintainer marks ready.

## Root cause
The gRPC `ChunkStore` data path built across M2.1–M2.5 (proto service, client +
D-server service, discovery, parallel fan-out write, any-`k` read) has **no
deterministic network-fault coverage**: `testkit` exposes `Clock`/`Disk` seams
but no network seam, and `wyrd-dst` exercises only the in-process commit
protocol — so a drop, delay, partition, or on-the-wire corruption on the real
wire path is unverified and could regress silently.

## Fix
Grow a **network seam** in `testkit` and run the *real* `GrpcChunkStore` wire
code on madsim's deterministic simulated network under seed-reproducible faults,
asserting proposal 0004's five Tier-1 properties. The transport is genuine, not
a fake: `tonic` is cfg-aliased to `madsim-tonic` under `--cfg madsim` (the
proposal's primary path, not the recorded in-sim-fake fallback), so the same
client + service code compiles and runs unchanged on the simulated network.

- Pin `madsim-tonic` / `madsim-tonic-build` once in `[workspace.dependencies]`
  (`Cargo.toml`).
- Cfg-alias `tonic` → `madsim-tonic` in `proto` and `chunkstore-grpc`; `proto`'s
  `build.rs` branches on `CARGO_CFG_MADSIM` (madsim build feeds protox's
  descriptor set to `madsim-tonic-build` via `file_descriptor_set_path` +
  `skip_protoc_run`, so no system `protoc` either way, per ADR-0016), and
  `proto/src/lib.rs` switches to `tonic::include_proto!`. The client, server, and
  fan-out source is untouched — it already speaks `tonic::…`.
- Add the network-fault model to `testkit` (`NetFault`, `NetFaultInjector`,
  `SeededNetFaults` with seed-derived link selection), transport-free and
  mirroring the existing `Disk`/`FaultInjector` shape, with two unit tests.
- Add `crates/dst/tests/network.rs` (new) asserting the five Tier-1 properties
  over `GrpcChunkStore` + `FanoutChunkStore` against `N=9` simulated D servers,
  run by `cargo xtask dst`. Faulted links are drawn from the run seed so a
  bug-finding seed replays identically (ADR-0009).

## Verified against
- `crates/testkit/src/lib.rs:110` — the `FaultPoint` enum (with `DiskWrite`/
  `DiskSync` but no network point): confirmed `testkit` had only `Clock`/`Disk`
  seams, the gap the network seam fills.
- `crates/chunkstore-grpc/src/client.rs:28` — `GrpcChunkStore::connect`: confirmed
  it resolves the endpoint through `tonic` so it maps onto madsim's transport with
  no source change.
- `crates/chunkstore-grpc/src/server.rs:46` — `ChunkStoreService`'s
  `ChunkStoreRpc` impl: confirmed the generated service-trait signature is
  identical under `madsim-tonic-build`, so the service compiles unchanged under
  madsim.
- `crates/chunkstore-grpc/src/fanout.rs:57` — `FanoutChunkStore`'s `ChunkStore`
  impl: confirmed the real M2.4 fan-out path is the code the campaign drives.
- `crates/proto/build.rs:18` and `crates/proto/src/lib.rs:12` — the normal-build
  `tonic_prost_build::compile_fds` codegen and `include!(… wyrd.v0.rs)`: confirmed
  the cfg branch is additive and leaves the non-madsim build's stubs intact.
- `crates/dst/tests/concurrency.rs` — the only DST test on `main` (in-process
  commit protocol): confirmed there is no `GrpcChunkStore`-over-madsim coverage
  before this change; `network.rs` does not exist on `main`.
- `Cargo.toml:86` — the forward-looking `madsim-tonic … (added in M2.6)` note:
  confirmed `madsim-tonic` was only a comment, never a dependency, before this PR.
- Planning artifact: `docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  §"DST and integration tests" → Tier-1 (the five properties) and suggested PR
  step 6; `docs/design/adr/0009-deterministic-simulation-testing.md` (DST as the
  repro substrate; a bug-finding seed becomes a permanent regression test).

## Test
New regression campaign `crates/dst/tests/network.rs` (`#![cfg(madsim)]`), run by
the project gate `./engine/xtask.sh dst` (`cargo xtask dst`, `--cfg madsim`,
`MADSIM_TEST_NUM=50`):

- **Red** — with the `testkit` seam reverted to `main`,
  `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test network --no-run` fails
  with `error[E0432]: unresolved imports wyrd_testkit::NetFault,
  wyrd_testkit::SeededNetFaults`; on unmodified `main` the file does not exist and
  the sweep runs only the in-process commit suite — the exact defect.
- **Green** — `./engine/xtask.sh dst` passes across the 50-seed sweep:
  `concurrency` (1) + `network` (5). No bug-finding seed surfaced, so none is
  committed.
- Non-madsim gate intact (`cargo xtask ci`): fmt `--check`, clippy `-D warnings`,
  `cargo test --workspace` (incl. the real-tonic `chunkstore-grpc` round-trip),
  and `cargo deny check` all clean — the cfg-alias does not disturb the normal
  build, and the new transitives are already covered by the `deny.toml` allowlist.

Fixes #116
