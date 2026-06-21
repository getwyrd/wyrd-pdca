# Brief — issue 155 / m2.8-local-distributed-cluster-static-endpoints

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** m2.8-local-distributed-cluster-static-endpoints
- **Supporting artifacts:** `../wyrd/docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  (M2 scope — this extends it past PR step 7 with a user-facing client path) and ADR-0006
  (Coordination backends; etcd/static deferred). Supporting context, not a governing
  pointer — this brief is self-contained.
- **Defect / goal:** there is **no user-facing way to run and exercise the distributed
  system on one machine**. The only multi-container setup
  (`crates/chunkstore-grpc/tests/docker-compose.yml`) is a CI fixture — target-only
  ephemeral ports, driven solely by `cargo xtask integration` injecting
  `WYRD_DSERVER_ENDPOINTS` into an in-code `FanoutChunkStore<GrpcChunkStore>`. The CLI
  cannot drive a cluster: `wyrd put` / `get` are local-disk-only
  (`crates/server/src/cli.rs:141`, `open_backends`), `wyrd d-server` registers via
  process-local `MemCoordination` so no separate process can discover it
  (`cli.rs:213-216`), and `wyrd demo` is in-process (`cli.rs:280-308`).
- **Success criterion:** a static-endpoints gateway client mode lands — e.g. `wyrd gateway
  --endpoints …`, or an `--endpoints` path for `put` / `get` — composing `Gateway` (generic
  over its ChunkStore, `crates/server/src/lib.rs:42`) over a `FanoutChunkStore<GrpcChunkStore>`
  built from a configured endpoint list; AND a user-facing `docker-compose.yml` (separate
  from the test fixture) with **fixed published ports** stands up N D-servers; AND a README
  "Run a local cluster" walk-through. Demonstrable at C4-verify: the new client mode compiles
  and a test drives `put` → `get` across a `FanoutChunkStore<GrpcChunkStore>` (in-process
  loopback, as `tests/round_trip.rs` and the Tier-2 test do) asserting a **byte-identical**
  round-trip; `cargo xtask ci` green. The end-to-end `docker compose up` + `wyrd put/get`
  across containers is exercised manually / on the nightly tier — supplementary, not the
  Check criterion.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (CLI / data-plane; no GUI)
- **Scope:** (a) a static-endpoints gateway client mode composing
  `Gateway` + `FanoutChunkStore<GrpcChunkStore>`; (b) a user-facing compose with fixed ports
  + a volume per D-server; (c) a README local-cluster walk-through. / **out of scope:**
  cross-process discovery via etcd / a non-static `Coordination` backend, stable placement
  records, rebalance (all M3 — #139 / #141); mTLS / PKI; multi-region; any change to the
  `ChunkStore` trait / proto / commit protocol.
- **Citations expected:** Do cites `crates/server/src/{cli.rs,lib.rs}`,
  `crates/chunkstore-grpc/src/fanout.rs`, and the existing
  `crates/chunkstore-grpc/tests/{tier2_integration.rs,docker-compose.yml}` on `main`.
- **Prior-art check:** `grep -rn "FanoutChunkStore\|WYRD_DSERVER_ENDPOINTS" crates` outside
  tests/benches finds only the type definition (`fanout.rs`) — no user-facing composition;
  `cli.rs` has no endpoint-driven mode; the only compose is the test fixture. Predecessors
  #114 (fan-out write), #115 (any-k read), and #117 (networked gRPC D servers) are merged on
  `main` — none is an open blocking bundle. Net-new (this is the M2.8 step).
- **Disposition hint:** likely-fix

## Sign-off note (expected NEEDS-HUMAN)
Topology is a human call: a client-side gateway (the `wyrd` binary composes the fan-out store
over the containers' published ports, metadata held locally) vs a gateway-daemon container
holding shared metadata; and whether static-endpoints is the accepted M2.8 shape (vs waiting
for the M3 discovery work). Expect the reviewer to raise it; the human decides at §6.

## STOP discipline
Draft only until Check sign-off. A draft PR MAY be opened for CI; it MUST NOT be marked ready
before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Ship 4 D servers, not 3, in the user-facing on-ramp. The default durability is rs(6,3): 9 fragments needing k=6 to reconstruct. The README "Run a local cluster" walk-through and root docker-compose.yml currently document only 3 D servers (ports 50051/52/53) = 3 fragments each, so losing a single server drops to exactly k=6 — zero redundancy headroom on the documented setup. The loopback test already uses 4 servers; make the documented topology match it. Update docker-compose.yml (add dserver4 on a fixed port, e.g. 50054, with its own volume) and the README endpoint list / walk-through to four endpoints. No change needed to the cli.rs gateway client mode itself — the composition reviewed clean and parse_endpoints already handles an arbitrary-length list.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the cluster (--endpoints) path allocates inodes from Gateway's in-process counter (lib.rs:71 AtomicU64::new(1)), reset every invocation, while pairing it with a persistent redb under --data-dir. Because metadata::create enforces require_absent(inode_key(id)) (metadata.rs:144), the SECOND distinct-key `wyrd put --endpoints` in a fresh process re-allocates inode 1 and fails as a misleading "concurrent writer won" Conflict — so only one distinct object per --data-dir survives across invocations. Required next: route the cluster path's inode allocation through the persisted meta:next_inode counter, exactly as the local-disk path already does via alloc_inode (cli.rs:319+). M2.8 is a human-testing on-ramp and the poking must be more than one simple round-trip: storing several distinct objects across separate invocations must work. Also: add regression coverage that stores TWO distinct keys across separate gateway compositions (fresh process / new connect_gateway over the same --data-dir) and round-trips both — the current single-put test and single-object README walk-through both stay inside the one-object window and miss this. Topology decision (item 1) is settled and NOT the reason for iterating: static-endpoints client-side gateway with locally-held metadata is accepted as the M2.8 human-testing shape; dynamic discovery / shared-metadata daemon stays M3.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
