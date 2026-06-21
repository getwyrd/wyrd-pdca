# Tier-2 container integration test + D-server throughput bench (M2.7)

> Final step (PR step 7) of accepted proposal
> `docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
> (§ "Tier-2 — integration against real backends", § "Benchmarks").
> One logical change: realize PR step 7 only.

## Root cause
This is not a defect fix but the last build step of Milestone 2: the Tier-2
integration tier the proposal says is "born at M2" did not yet exist, and there
was no aggregate write/read throughput benchmark across D-server counts. Steps
1-6 (#112-#116) built and DST-validated the data path in simulation only, so
nothing yet proved it against real networked transport or made the §10 Q6
throughput-scaling claim measurable.

## Fix
- A gated Tier-2 integration test drives an end-to-end S3-style write -> read
  under `rs(6,3)` across multiple real, networked gRPC D servers in containers
  and asserts the read is byte-identical to the write — exercising real tonic /
  HTTP-2 / prost / connection lifecycle, which no in-process test covers.
- A `cargo xtask integration` task stands the cluster up under `docker compose`
  (`--scale dserver=N`), resolves each replica's ephemeral host port into
  `WYRD_DSERVER_ENDPOINTS`, runs the test with `--ignored`, and tears the
  cluster down unconditionally. Docker absent → warn-skip locally, hard-fail in
  CI (mirrors `cargo_deny_check`).
- The test is `#[ignore]`d and no-ops when its endpoint list is unset, so the
  default `cargo test` and the `cargo xtask ci` lane never need Docker. A nightly
  (and on-demand) workflow runs it — the first container job in CI, not a
  required per-PR check.
- A second benchmark, run by `cargo xtask bench`, measures aggregate write/read
  throughput across D-server counts {1,3,9} over real in-process tonic, so
  scaling is visible on a laptop and the target compiles in CI. Tracked, not
  gated (matching the EC micro-bench).
- No new external crate: orchestration reuses the host's `docker compose`, so the
  ADR-0003 dependency audit is not triggered. The `ChunkStore` trait, the
  `wyrd.v0` proto, and the commit protocol are unchanged.

## Verified against
- `crates/chunkstore-grpc/tests/tier2_integration.rs` — the new container test;
  composes `core::write::write_fragments` + `core::read::read_object_from`
  through a `FanoutChunkStore<GrpcChunkStore>` dialing the containers and asserts
  byte-identical read-back.
- `crates/server/src/lib.rs:28` — `DEFAULT_DURABILITY = rs(6,3)`, the gateway
  default the test and bench reproduce.
- `crates/chunkstore-grpc/src/fanout.rs:51-52` — `route(index) = index % n`, the
  placement primitive that spreads a chunk's 9 fragments across distinct servers.
- `xtask/src/main.rs` — the new `integration` subcommand (dispatch + compose
  up/port/down) and `run_bench` extended to also run `throughput`; usage string
  updated.
- `crates/core/Cargo.toml:45` and `crates/core/benches/throughput.rs` — the new
  `[[bench]] throughput` target and its harness.
- `crates/chunkstore-grpc/tests/docker-compose.yml`,
  `crates/chunkstore-grpc/tests/dserver/Dockerfile`, `.dockerignore` — the
  container cluster definition (multi-stage build for glibc portability).
- `.github/workflows/integration-nightly.yml` — the nightly container job
  (schedule + dispatch, not a required status check).

## Test
The Check criterion is that both new targets compile and are wired into
`cargo xtask`; the container run is supplementary, post-merge evidence.
- RED on a `main` worktree: `cargo xtask integration` → `unknown task
  'integration'`; `cargo test -p wyrd-chunkstore-grpc --test tier2_integration`
  → no such test target.
- GREEN on this branch: both targets compile under `cargo build --all-targets`
  (the CI lane); `cargo xtask bench` runs the throughput bench end-to-end over
  real loopback tonic; and `cargo xtask integration` builds the image, scales 9
  D-server containers, and the test reads back a multi-chunk `rs(6,3)` object
  byte-identical over real tonic.

Fixes #117
