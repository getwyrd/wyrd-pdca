# Build notes — issue #117 / m2.7-tier2-integration-throughput-bench

PR step 7 (final) of accepted proposal `0004-milestone-2-networked-d-servers.md`.
Planning artifact read in full; governing sections cited inline below.

## What the success criterion actually requires (and how each part is met)

The brief's success criterion has three obligations. Two are the **Check criterion**
(must hold at sign-off); the third is **supplementary**, post-merge evidence.

1. **Tier-2 container integration test** — write→read under `rs(6,3)` across
   *multiple real networked gRPC D servers in containers*, byte-identical.
   → `crates/chunkstore-grpc/tests/tier2_integration.rs` +
   `crates/chunkstore-grpc/tests/docker-compose.yml` +
   `crates/chunkstore-grpc/tests/dserver/Dockerfile`, run by `cargo xtask integration`.
2. **`cargo xtask bench` aggregate write/read throughput across D-server counts** —
   → `crates/core/benches/throughput.rs`, wired as a second `[[bench]]` and into
   `run_bench`.
3. **Both compile and are wired into `cargo xtask`** — this is the gating Check
   criterion (proposal § Benchmarks: CI's obligation is the data path builds *no
   shared bottleneck*; "green on the nightly container job … supplementary evidence
   that clears after merge, not the Check criterion"). Verified: both targets
   compile under `cargo build --all-targets` (the CI lane), and `cargo xtask
   integration` / `cargo xtask bench` invoke them.

## Design decisions

### Tier-2 test exercises the real data path, not the full Gateway
The test drives `core::write::write_fragments` (parallel fan-out) +
`core::read::read_object_from` (any-k-arrive-first) through a
`FanoutChunkStore<GrpcChunkStore>` whose clients dial the containers. This is the
M2 data path the proposal says Tier-2 must validate against reality (§ "Tier-2 …
real tonic — real HTTP/2 framing, real prost (de)serialization … real connection
lifecycle"). The four-phase commit metadata is unchanged M0/M1 and explicitly *not*
the Tier-2 risk, so it is not re-driven here (it is covered by Tier-0/1).
`crates/server/src/lib.rs:28` (`DEFAULT_DURABILITY = rs(6,3)`) and
`crates/chunkstore-grpc/src/fanout.rs:51` (index `i` → store `i % n`) are the
behaviours the test composes. I did **not** pull `wyrd-server` in as a dev-dep to
use `Gateway` directly: it would create a dev-dependency cycle
(`chunkstore-grpc` ← (dev) `server` → `chunkstore-grpc`) and add weight, for no gain
over driving `core` directly. Cost of that alternative: a cycle cargo only tolerates
through dev-deps plus the entire `server` build (redb, the CLI) on the test path.

### Orchestration via `docker compose` in xtask — and **no new crate dependency**
The brief's sign-off note anticipated a `testcontainers` dependency (→ ADR-0003
three-test audit + `deny.toml` allowlist). I deliberately did **not** add it. The
criterion names "docker-compose / testcontainers" as alternatives; `xtask` brings
the cluster up with `docker compose up --scale dserver=N`, resolves each replica's
ephemeral host port (`docker compose port --index i`), exports
`WYRD_DSERVER_ENDPOINTS`, runs the `#[ignore]`d test with `--ignored`, and tears
down unconditionally. Concrete cost avoided: `testcontainers` pulls `bollard` +
`hyper`/`tokio` transitives → a non-trivial new license surface to vet, for an
ability (`docker compose`) the host already provides. Result: this change adds
**zero new external crates** — the only `Cargo.toml` additions are
workspace-internal/already-pinned dev-deps (`wyrd-core` to chunkstore-grpc;
`wyrd-chunkstore-grpc`/`-fs`/`tonic`/`tokio`/`tokio-stream`/`tempfile` to core,
all already in `[workspace.dependencies]`). So the ADR-0003 dep wall is *not*
triggered — strictly lower risk than the anticipated path. (Flagged for the human;
if the reviewer prefers `testcontainers` for in-test container lifecycle control,
that is a follow-up, not a correctness gap.)

### Multi-stage Dockerfile (portability over speed)
Host is Ubuntu 26.04 (glibc 2.43); every Debian base image is older, so a
host-built binary will not run in the container (`GLIBC_2.43 not found`). The
Dockerfile therefore compiles `wyrd` inside `rust:1.96-bookworm` and runs it on
`debian:bookworm-slim` (matching glibc 2.36). Cost: a cold image build recompiles
the workspace (slow once), cached thereafter. This is the only portable option:
no musl target/linker is installed on the host, and pinning the runtime base to the
host's libc would break on any other host. `.dockerignore` keeps `target/`/`.git`
out of the context.

### Throughput bench uses in-process loopback tonic
Per proposal § Benchmarks the *number* "lands on real hardware"; CI's job is
compilation + proving no shared bottleneck. The bench stands up N real tonic D
servers over loopback (same wire stack as `tests/round_trip.rs`) and sweeps
counts {1,3,9} for both write and read, reported as bytes/s — so scaling with
D-server count is visible without containers (which would break a criterion run).

### Gating discipline
`tier2_integration` is `#[ignore]`d *and* no-ops when `WYRD_DSERVER_ENDPOINTS` is
unset, so the default `cargo test` / `cargo xtask ci` lane never needs Docker
(success criterion NOTE). `run_integration` warn-skips locally when Docker is
absent and hard-fails in CI (mirrors `cargo_deny_check`). The nightly container job
is `.github/workflows/integration-nightly.yml` (schedule + dispatch, not a required
PR check) — the proposal's "first container job in CI".

## Red → green (via the project runner `./engine/xtask.sh`)
- **RED (main worktree):** `cargo xtask integration` → `unknown task 'integration'`;
  `cargo test -p wyrd-chunkstore-grpc --test tier2_integration` → `no test target
  named tier2_integration`. The capability is absent.
- **GREEN (this branch):** `./engine/xtask.sh integration` builds the image, scales
  9 D-server containers, and the test writes/reads a multi-chunk object under
  rs(6,3) byte-identical over real tonic; `cargo xtask bench` runs the throughput
  bench (verified in test-mode: all counts `Success`).

## Changes (path:line on the target branch `main`)
- `xtask/src/main.rs:27` — `integration` subcommand; `run_bench` (`xtask/src/main.rs:56`)
  also runs `throughput`.
- `crates/core/Cargo.toml:29` — bench dev-deps + `[[bench]] throughput`.
- `crates/core/benches/throughput.rs` — new.
- `crates/chunkstore-grpc/Cargo.toml:38` — `wyrd-core` dev-dep.
- `crates/chunkstore-grpc/tests/tier2_integration.rs`,
  `tests/docker-compose.yml`, `tests/dserver/Dockerfile` — new.
- `.dockerignore`, `.github/workflows/integration-nightly.yml` — new.

## Verified / not verified
- Verified: both new targets compile (`build --all-targets` equivalent); fmt clean;
  clippy `-D warnings` clean on the touched crates; throughput bench runs end-to-end
  over real loopback tonic; RED proven on a `main` worktree.
- Verified post-build: full `./engine/xtask.sh integration` container run (see
  cycle log) — the named test green against 9 real containers.
- Not modeled by any PDCA gate: the target's own commit hooks (DCO `-s`); the human
  applies `git commit -s` at publish.
