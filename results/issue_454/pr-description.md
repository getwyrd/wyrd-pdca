## Summary
**User impact:** Operators running more than one `wyrd s3` gateway got several
independent single-node object stores instead of one shared front door. Passing
`--metadata-backend`, `--coordination-backend`, or `--endpoints` to `wyrd s3` did
nothing — every gateway silently wrote objects to its own local disk. So an object
uploaded through one gateway could not be read back through another, and objects
were never stored across the cluster's storage servers for durability.

This PR wires the `wyrd s3` gateway to select its metadata, coordination, and chunk
backends from configuration — the same flags every other cluster role already
honours — so a fleet of gateways serves one pool over the shared cluster storage.

Reported in #454.

## What to look at
The `wyrd s3` command in the server CLI (`cmd_s3`) and the small `serve_s3_role`
helper it now delegates to. To try it: start `wyrd s3` with `--endpoints` pointing
at a set of storage servers, upload an object with any S3 client, and confirm the
object's fragments land on the storage servers while the gateway's own
`<data-dir>/chunks` directory stays empty; a subsequent download returns the object
byte-for-byte. The new integration test does exactly this in-process against real
loopback gRPC storage servers — `cargo test -p wyrd-server --test s3_gateway_cluster`.

## Root cause
`cmd_s3` constructed its backends with hardcoded concrete constructors — a local
redb metadata file, a local-disk chunk store, and in-process coordination — and
never read the `--metadata-backend`, `--coordination-backend`, or `--endpoints`
flags. Every peer role already selects these by configuration (`wyrd put`/`get`/
`custodian` via `resolve_backend`, `wyrd d-server` via `resolve_coordination_backend`);
the gateway was the one role skipped when that composition pattern was introduced.

## Fix
`cmd_s3` now resolves its metadata and coordination backends through the same
`resolve_backend` / `resolve_coordination_backend` helpers the peer roles use, and,
when `--endpoints` is supplied, fans each object's chunks out to the configured
storage servers over gRPC (`connect_fanout`) rather than writing a local chunk
store. The composition is factored into a `serve_s3_role` helper so both the cluster
front door and the existing single-node local-disk front door (preserved when
`--endpoints` is absent) run the identical serve path. All four metadata ×
coordination combinations (redb/tikv × mem/etcd) are wired through the production
helpers, not stubbed. The `deploy/small-multi-node` compose file now configures its
three gateways against the shared cluster metadata, coordination, and storage
servers so they form one pool. Standing up and running the live multi-node
demonstration is tracked separately (#455); this PR ships the wiring it depends on.

## Verification
- **Claim:** A gateway composed from configuration (redb metadata + in-process
  coordination + `--endpoints` chunk fan-out) serves an S3 PUT whose fragments land
  on the storage servers with its own local chunk directory left empty, and a later
  GET returns the object byte-for-byte, reconstructed from those fragments.
- **Checked:** `crates/server/src/cli.rs:1220-1222` on `feat/m4-production-metadata-backend`
  is the hardcoded backend construction this change removes; the fix routes `cmd_s3`
  through the peer composition pattern established at `cli.rs:312` (`wyrd put`),
  `cli.rs:511` (`wyrd d-server`), and `cli.rs:1265` (cluster fan-out via
  `connect_fanout`).
- **Test:** `crates/server/tests/s3_gateway_cluster.rs` — a new integration test
  stands up four real loopback gRPC storage servers and drives an S3 PUT→GET with a
  stock `aws-sdk-s3` client through the same `serve_s3_role` the CLI runs. It fails
  to build before the fix (the config-selection composition helper it drives does not
  exist) and passes after (`test result: ok. 1 passed`), asserting the byte-identical
  round trip, that fragments fanned out across multiple storage servers, and that the
  gateway's local chunk directory stayed empty.
- **Gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test, deny,
  conformance) passes on default features; `cargo check -p wyrd-server --features
  tikv,etcd` compiles all four backend arms.

Fixes #454
