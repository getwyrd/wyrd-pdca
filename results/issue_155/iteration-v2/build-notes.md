# Build notes — issue 155 / m2.8-local-distributed-cluster-static-endpoints (iteration 2)

## What changed since iteration 1

Iteration 1 was **accepted on substance** — the reviewer found the gateway
client-mode composition clean and `cargo xtask ci` green — but the human sign-off
held it back for one concrete reason (brief §"Iteration 1 — carry-forward"):

> Ship **4** D servers, not 3, in the user-facing on-ramp. Default durability is
> `rs(6,3)` (9 fragments, k=6 to reconstruct). The documented 3-server topology
> (ports 50051/52/53) puts 3 fragments on each server, so losing a single server
> drops to exactly k=6 — zero redundancy headroom. The loopback test already uses
> 4 servers; make the documented topology match it. No change needed to the
> `cli.rs` gateway client mode itself.

So iteration 2 is a **documentation/topology** delta, nothing else:

1. `docker-compose.yml` — add `dserver4` on fixed published port **50054** with
   its own named volume `dserver4-data` (mirrors the dserver1–3 service shape,
   same `crates/chunkstore-grpc/tests/dserver/Dockerfile` build, same internal
   `0.0.0.0:50051` bind). Added a comment block stating *why four* (the rs(6,3)
   headroom argument) so the choice is self-documenting.
2. `README.md` "Run a local cluster" — "three" → "four" D servers; the
   `ENDPOINTS=…` list and the header walk-through now carry the fourth endpoint
   `http://127.0.0.1:50054`; added the same rs(6,3) "four, not three" rationale.

**Unchanged from iteration 1** (the carry-forward explicitly says the reviewed
composition is fine, and `parse_endpoints` already handles an arbitrary-length
list): `crates/server/src/cli.rs` (the `--endpoints` gateway client mode:
`parse_endpoints`, `connect_fanout`, `connect_gateway`, `cluster_put`,
`cluster_get`, the `ClusterGateway`/`GrpcFanout` aliases) and the test
`crates/server/tests/gateway_cluster.rs` are **byte-identical** to the accepted
iteration-1 versions. The test already stands up a **four-server** loopback
cluster, so the documented topology now matches what the regression test proves.

## Why no broader change

The carry-forward is a sign-off *condition*, not a defect in the code path. The
gateway client mode (`Gateway<RedbMetadataStore, FanoutChunkStore<GrpcChunkStore>,
MemCoordination>`, built in `connect_gateway`, `cli.rs`) already composes exactly
what the brief's Success criterion names, and the loopback test already drives a
byte-identical `put`→`get` across four real gRPC D servers. Re-touching `cli.rs`
or the test would re-litigate an already-accepted approach — exactly what the
iterate instruction forbids ("do NOT re-submit the rejected approach unchanged",
and equally, do not churn the accepted parts). The smallest change that satisfies
the human's stated condition is the two-file doc/topology edit above.

I considered making the four-vs-three count a single source (e.g. a generated
endpoint list) so README and compose can't drift. Rejected: there is no shared
config surface between a Markdown walk-through and a compose file, so "single
sourcing" would mean a code generator for two literal lists — far more machinery
than the 1 service block + ~4 edited lines here, for a hand-driven on-ramp the
brief scopes as supplementary (manual/nightly), not gated. Cost of the rejected
route: a new generator + its own test + a CI check, vs. the +18-line compose
service block and 3 edited README lines in this patch.

## Verification (red → green)

The unit under test is `wyrd_server::cli::connect_gateway` — the exact composition
the CLI `--endpoints` path builds — exercised over real tonic/loopback gRPC, no
GUI/display dependency (headless-safe; the `wyrd-server` test target already links
tonic/tokio/redb). Verified in a throwaway worktree off `main` (sharing the warm
`target/` so the run is bounded), through `cargo test`:

- **RED** (revert `cli.rs` to `main`, keep the test):
  `cargo test -p wyrd-server --test gateway_cluster --no-run` →
  `error[E0432]: unresolved import wyrd_server::cli::connect_gateway` (exit 101).
- **GREEN** (restore the fix):
  `cargo test -p wyrd-server --test gateway_cluster` →
  `test gateway_put_get_byte_identical_across_grpc_cluster ... ok` — 1 passed.
- **Format** (the target's commit hook): `cargo fmt --check` → exit 0.
- Iteration 1 already ran the whole gate `./engine/xtask.sh ci`
  (fmt/clippy/build/test incl. DST/deny/conformance) green over the *identical*
  compiled code; this iteration's only delta is two non-compiled text files
  (`docker-compose.yml`, `README.md`) that `cargo xtask ci` does not read, so the
  gate outcome is unchanged. The end-to-end `docker compose up` + cross-container
  `wyrd put/get` flow is the supplementary manual/nightly tier per the brief, not
  the Check criterion.

## Patch / branch notes

- Patch is against **getwyrd/wyrd @ main**; `git apply --check` confirmed it
  applies clean to a fresh `main` worktree.
- `patch.diff` stat: README.md, `crates/server/src/cli.rs`,
  `crates/server/tests/gateway_cluster.rs`, `docker-compose.yml` —
  407 insertions(+), 3 deletions(-).
- STOP discipline observed: no PR opened, nothing pushed, no branch state of other
  in-flight lanes disturbed (all build/verify work done in `/tmp` worktrees off
  `main`).

## Topology — the expected NEEDS-HUMAN (carries forward to §6)

Still a client-side gateway: the `wyrd` binary composes the fan-out and holds
metadata locally (no shared-metadata daemon; static endpoints, no discovery —
ADR-0006 defers etcd/dynamic to M3). The four-server default now gives one-server
loss headroom under rs(6,3). The remaining human call (client-gateway vs.
gateway-daemon topology; static-endpoints as the accepted M2.8 shape) is unchanged
from iteration 1 and is for the reviewer/human at §6.
