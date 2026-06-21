# Build notes — issue 155 / m2.8-local-distributed-cluster-static-endpoints

## What the brief asks for (Success criterion)

Three things, all user-facing:

1. **A static-endpoints gateway client mode** — `wyrd gateway --endpoints …` *or*
   an `--endpoints` path for `put`/`get` — composing `Gateway`
   (`crates/server/src/lib.rs:42`) over a `FanoutChunkStore<GrpcChunkStore>`
   (`crates/chunkstore-grpc/src/fanout.rs:26`, `client.rs:21`) built from a
   configured endpoint list.
2. **A user-facing `docker-compose.yml`** (separate from the test fixture at
   `crates/chunkstore-grpc/tests/docker-compose.yml:9`) with **fixed published
   ports** + a volume per D server.
3. **A README "Run a local cluster" walk-through.**

C4-verify criterion: the client mode compiles and a test drives `put` → `get`
across a `FanoutChunkStore<GrpcChunkStore>` *in-process over loopback gRPC* (as
`crates/chunkstore-grpc/tests/round_trip.rs:34` and `tier2_integration.rs:64` do),
asserting a **byte-identical** round-trip; `cargo xtask ci` green. The
`docker compose up` + cross-container flow is supplementary (manual/nightly), not
the Check criterion.

## Chosen shape: the `--endpoints` path on `put`/`get` (not a new `gateway` verb)

The brief offers both; I took the `--endpoints` path because it is the smaller,
more discoverable surface and reuses the existing `put`/`get` argument shape and
`--data-dir`/`--chunk-size`/`--durability` flags verbatim. A separate `wyrd
gateway put …` verb would need nested sub-command parsing in the flag-based
`ParsedArgs` (`cli.rs:357`, which has no notion of a sub-verb after the first
positional) — extra parsing code for no user benefit. Concretely the verb route
would add a `cmd_gateway` dispatcher plus its own `put`/`get` positional
disambiguation (~30–40 lines) versus the two `if let Some(raw) =
parsed.flag("endpoints")` guards (8 lines total) I added.

## The composition (the load-bearing change)

`crates/server/src/cli.rs`:

- `connect_fanout(endpoints)` — dials one `GrpcChunkStore::connect` per endpoint
  and wraps them in `FanoutChunkStore::new` (the exact M2 placement primitive).
  Connecting up front turns an unreachable D server into a clear startup error,
  not a mid-write failure.
- `connect_gateway(data_dir, endpoints)` — opens the local redb metadata store
  + `MemCoordination`, builds the fan-out, and returns
  `Gateway<RedbMetadataStore, FanoutChunkStore<GrpcChunkStore>, MemCoordination>`
  (the `ClusterGateway` alias). This is the single composition the CLI **and** the
  test both call, so the test exercises the shipping client mode rather than a
  re-implementation.
- `cluster_put` / `cluster_get` spin a multi-thread tokio runtime (the gRPC
  clients are async; the local-disk paths stay sync via pollster) and run
  `Gateway::put_object` / `get_object` — the same S3 PUT/GET paths the in-process
  gateway uses, but every fragment now crosses the wire.

I deliberately routed through `Gateway::{put_object,get_object}` (not the raw
`core::write`/`read` functions the local `put`/`get` use) because the brief's
criterion is explicitly *"composing `Gateway`"* — the reviewer is told to check
that `Gateway` sits over `FanoutChunkStore<GrpcChunkStore>`.

## Test: `crates/server/tests/gateway_cluster.rs`

In-process loopback, mirroring `round_trip.rs`: stands up **four** real gRPC D
servers (each `ChunkStoreService` over a fresh `FsChunkStore`, bound to an
ephemeral 127.0.0.1 port), then calls the *same* `connect_gateway` the CLI uses,
PUTs a multi-chunk payload under rs(6,3) (9 fragments per chunk fanned across the
4 servers), GETs it back, and asserts byte-identical. Also checks a miss returns
`Ok(None)`.

It is import-light by the brief's standard — the `server` test target already
links tonic/tokio/redb; there is no GUI/display dependency, and the headless CI
runner builds and runs it fine (confirmed below). The unit under test
(`connect_gateway`) pulls in no heavier deps than the existing `dserver.rs` /
`round_trip.rs` integration tests.

Red→green proven through the project runner path:
- **Red (no fix):** `git stash`ed `crates/server/src/cli.rs`, ran
  `cargo test -p wyrd-server --test gateway_cluster` →
  `error[E0432]: unresolved import wyrd_server::cli::connect_gateway`.
- **Green (with fix):** same command → `1 passed`.
- **Whole gate:** `./engine/xtask.sh ci` (delegates `cargo xtask ci`:
  fmt --check, clippy -D warnings, build, test incl. DST, cargo-deny, conformance)
  → `xtask ci: all checks passed`. (The two `wyrd-dst … generated 1 warning` lines
  are pre-existing in a crate I did not touch; `ci` is still green.)

## docker-compose.yml (root, user-facing) + README

Root `docker-compose.yml`: three named services `dserver1..3` on **fixed** host
ports `50051/50052/50053`, each with a named persistent volume
(`dserverN-data:/data`), built from the existing
`crates/chunkstore-grpc/tests/dserver/Dockerfile` (context `.`). Distinct from the
CI fixture, which is a single scaled service on ephemeral ports driven by
`cargo xtask integration` (`xtask/src/main.rs:183`) — I called that distinction
out in both files so nobody hand-drives the fixture or scales the user compose.

README gains a "Run a local cluster" section: `docker compose up --build -d`, the
`ENDPOINTS=…` put/get/`diff` round-trip, `docker compose down -v`, and an explicit
limits note (static endpoints only; metadata held locally so put/get share
`--data-dir`; M3 owns discovery/placement/rebalance).

## Topology — the expected NEEDS-HUMAN (brief §Sign-off note)

This implements the **client-side gateway** topology: the `wyrd` binary composes
the fan-out and holds metadata locally; there is no shared-metadata gateway
daemon. Two consequences a reviewer/human should weigh at §6:

1. **Static endpoints, not discovery.** Matches the brief's in-scope line and
   ADR-0006's "static/etcd deferred"; dynamic discovery is M3 (#139/#141).
2. **Local metadata ⇒ single-gateway shape.** Because `Gateway` allocates inode
   ids from an in-process `AtomicU64` (`lib.rs:52,71,126`) that is **not**
   persisted, two *separate* `wyrd put --endpoints` processes would each start at
   inode 1 and collide. The in-process Check criterion (one gateway instance does
   put then get) is unaffected, and the README documents the share-`--data-dir`
   single-gateway flow. Persisting/seeding the allocator (or a gateway daemon) is
   the M3 topology question the brief flags as a human call — I did **not** widen
   scope into `lib.rs`'s allocator to chase the cross-process case, since the brief
   explicitly defers that and names the in-process round-trip as the criterion.

## Out of scope (untouched, per brief)

No change to the `ChunkStore` trait / proto / commit protocol; no etcd / dynamic
`Coordination` backend; no placement records / rebalance; no mTLS. `fanout.rs`,
`client.rs`, `lib.rs`'s `Gateway` are reused as-is.
