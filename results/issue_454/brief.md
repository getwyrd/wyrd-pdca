# Brief — issue 454 / gateway-composes-over-cluster-backends

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** gateway-composes-over-cluster-backends
- **Defect:** The `wyrd s3` gateway role hardcodes its backends instead of selecting
  them by config. `cmd_s3` (`crates/server/src/cli.rs:1220-1222`) constructs
  `RedbMetadataStore::open(dir.join("meta.redb"))` + `FsChunkStore::open(dir.join("chunks"))`
  + `MemCoordination::new()` directly and never calls `resolve_backend` /
  `resolve_coordination_backend` / `connect_fanout`. It honours neither
  `--metadata-backend redb|tikv`, `--coordination-backend mem|etcd`, nor `--endpoints`.
  Every other cluster-facing role selects these by config (put/get/custodian via
  `resolve_backend`, #255; d-server via `resolve_coordination_backend`, #449) — the
  gateway was skipped. Consequence: each gateway is a standalone single-node island
  writing a private redb + local disk; the three gateways in `deploy/small-multi-node/`
  are three separate object stores, not one pool over the shared cluster state
  (`deploy/small-multi-node/docker-compose.yml` documents this explicitly, lines 36-43).
- **Success criterion:** Over an **in-process loopback cluster** of real gRPC D-servers
  (the `gateway_cluster.rs` harness), a gateway whose backends are composed **from config**
  — redb metadata + `MemCoordination` + a `connect_fanout(--endpoints)` chunk store — serves
  an S3 PUT whose fragments land on the D-servers (the local `data-dir/chunks` directory
  stays empty, i.e. nothing was written to a local `FsChunkStore`), and a subsequent GET
  reads the object back byte-identically, reconstructed from the D-server fragments. In the
  same slice, `cmd_s3` resolves its backends through `resolve_backend` /
  `resolve_coordination_backend` / `connect_fanout` (the `tikv` / `etcd` arms compile under
  their respective cargo features). Demonstrable by C4-verify at Check: `cargo xtask ci`
  runs the new integration test red→green with no external services (pure-Rust loopback
  gRPC over tonic).
- **Falsifiability:** RED is producible at Check on the **in-process 4-D-server loopback
  harness** (`crates/server/tests/gateway_cluster.rs:38` `spawn_dserver`), driven by
  `cargo test` / `cargo xtask ci` — no Docker, TiKV, or etcd needed for this arm. Pre-fix:
  `cmd_s3`/the gateway ignores `--endpoints` and writes chunks to a local `FsChunkStore`, so
  the assertion "fragments landed on the D-servers AND `data-dir/chunks` is empty" fails
  (and the config-selection composition helper the test drives does not yet exist → the test
  does not compile against the shipped surface). Post-fix: GREEN. The full live TiKV+etcd+9-
  D-server demonstration is **not** this slice's criterion — that is issue #455, off-Check
  (see Verification posture); binding here on the loopback redb+mem+fanout arm keeps RED
  producible on the environment Do gets.
- **Invariant to restore:** Every cluster-facing server role selects its metadata,
  coordination, and chunk backends **by configuration**, not by hardcoded concrete
  constructors — the gateway must compose over the same resolved backends the rest of the
  cluster uses, so a fleet of gateways shares one logical store. Source: the established
  in-repo composition pattern (`resolve_backend` for put/get/custodian, #255/PR #427;
  `resolve_coordination_backend` for d-server, #449) and proposal 0015
  §"Composition, not refactor" (`../wyrd/docs/design/proposals/0015*`, cited in
  `cli.rs:1180`-region doc). This is a **parity/composition** invariant, not a structural
  load-safety one — the fix is the config-driven selection, not the smallest textual diff.
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration branch per INTEGRATION §2 — the M4 slices stack here, not on `main`)
- **Depends on:**
- **Conflicts with:** 458
- **Ordering note:** #454 and #458 both edit `crates/server/src/cli.rs` (the `wyrd s3`
  vs `wyrd d-server` cmd functions and the adjacent usage/`eprintln!` help block ~236-249)
  — no build-on dependency, but a shared file, so they must land in DIFFERENT waves rather
  than build blind on the same base. #454 is the higher-priority critical-path slice (it is
  the code prerequisite the #455 demonstration is gated on), so schedule it FIRST; #458
  rebases onto #454's accepted diff.
- **Surfaces:** data
- **Difficulty:** high — the change is a **two-axis backend dispatch** in `cmd_s3`
  (metadata redb|tikv × coordination mem|etcd, each monomorphizing a distinct
  `Gateway<M, GrpcFanout, Co>` and a generic serve helper), plus `--endpoints` wiring and
  the shared usage/help block. The peers only do single-axis dispatch; a diff-reviewer must
  hold every backend combination's wiring in view. Rated up under uncertainty.
- **Scope:** Wire the `wyrd s3` gateway role to select its backends by configuration —
  honour `--metadata-backend redb|tikv` (via `resolve_backend`), `--coordination-backend
  mem|etcd` (via `resolve_coordination_backend`), and fan chunks to the D-servers over gRPC
  via `--endpoints` (via `connect_fanout`, the same static-endpoints fan-out that backs
  `cluster_put`/`cluster_get`) instead of a local `FsChunkStore`. **All four backend arms are
  REQUIRED deliverables, genuinely wired — not stubbed:** the `tikv` metadata arm resolves
  through `open_tikv_meta` and the `etcd` coordination arm through `open_etcd_coordination`,
  exactly as the peer roles do (`cluster_put` `cli.rs:1254`, `cmd_d_server` `cli.rs:531-536`)
  — a `todo!()`/redb-only shortcut in either arm does NOT satisfy this slice, because #455's
  loop depends on the gateway actually writing to TiKV and coordinating via etcd (only the
  *live* verification of those arms is deferred off-Check, not the code — see Verification
  posture). Also **update `deploy/small-multi-node/docker-compose.yml` gateway services
  `gateway0/1/2` (lines 386-412)** to pass `--metadata-backend tikv --coordination-backend
  etcd --endpoints http://dserver0:50051,…,http://dserver8:50051` with `WYRD_TIKV_PD_ENDPOINTS`
  + `WYRD_ETCD_ENDPOINTS`, mirroring the custodian service (`custodian0` `command`/`environment`,
  compose lines 359-361), and retire the "STANDALONE per #454" comment (compose lines 381-383) —
  this configures the three gateways as one pool (this slice's acceptance) and is the
  configuration precondition #455 demonstrates against. Preserve today's behaviour when a
  flag is absent (default redb/mem; retain the local-FS single-node front door when
  `--endpoints` is unset, so the #367 loopback first-deployment path is not broken). / out of
  scope: STANDING UP and RUNNING the live 9-node demonstration + the day-one durability loop
  (that is #455 — this slice only makes it *possible* by shipping the code + compose config);
  etcd-**discovery**-driven endpoint resolution (this uses STATIC `--endpoints`, exactly as
  put/get and the custodian do — discovery is a later concern, `cli.rs:1186`-region doc; #458
  makes the etcd advertisement routable for that future consumer, and is NOT a precondition
  for #455 as scoped); the public-TLS terminator (proposal 0015 §"Deployment prerequisite").
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read `cmd_s3`
  (`crates/server/src/cli.rs:1193-1249`): it builds `Gateway::new(RedbMetadataStore,
  FsChunkStore, MemCoordination::new())` unconditionally and parses no `--metadata-backend`
  / `--coordination-backend` / `--endpoints` flag. Compare against `cmd_put`
  (`resolve_backend`, `cli.rs:312`), `cmd_d_server` (`resolve_coordination_backend`,
  `cli.rs:511`), and `cluster_put` (`connect_fanout`, `cli.rs:1265`) — the gateway is the
  one role that resolves none of them. The new integration test (below) makes the gap
  executable: with the gateway pointed at loopback D-servers via `--endpoints`, chunks must
  land on the D-servers, not the local disk.
- **External dependencies:** For the **Check-exercised** redb+mem+gRPC-fanout arm: only the
  base toolchain that already builds the gRPC crates (`wyrd-chunkstore-grpc`) — `protoc`
  (tonic/prost codegen), already required by the existing workspace build the gate runs; no
  Docker/TiKV/etcd. The `tikv` metadata and `etcd` coordination arms are `#[cfg(feature)]`-
  gated (as `cluster_put`'s tikv arm is) and only build/run under those cargo features + a
  live stack — verified off-Check on `deploy/small-multi-node/` (#455) and the existing
  endpoint-gated DST live legs (#453). `none` beyond base for the Check arm.
- **Test file:** crates/server/tests/s3_gateway_cluster.rs (new — mirror
  `gateway_cluster.rs`'s `spawn_dserver` loopback harness and `s3_http_wire.rs`'s S3 surface)
- **Verification posture:** (a) BUILT AND EXERCISED at Check: the config-driven backend
  selection in `cmd_s3` and the resulting `Gateway<Redb, GrpcFanout, Mem>` serving path,
  exercised by the new in-process loopback round-trip integration test (real tonic D-servers,
  real HTTP/2 framing — not a double), red→green under `cargo xtask ci`. (b) DEFERRED
  off-Check: the `tikv` metadata + `etcd` coordination arms running against LIVE TiKV/etcd.
  These are cfg-gated code that COMPILES at Check and whose constituent pieces are already
  exercised by SOMETHING — `--metadata-backend` resolution by `backend_selection.rs`, etcd
  coordination by #449's tests, the DST TiKV commit path by #453's endpoint-gated live legs;
  the gateway's composition OVER them is the same monomorphized shape the redb/mem arm
  proves at Check, differing only in the concrete store. Not inert dispatch scaffolding.
  The live end-to-end run (real 9-D-server fan-out + TiKV metadata + etcd) is confirmed by
  the maintainer on the stood-up `deploy/small-multi-node/` stack as part of #455.
- **Production reach:** The gateway resolves D-servers from STATIC `--endpoints`, identical
  to the shipped `cluster_put`/`cluster_get` mode — this is the real production wiring at M4
  (etcd-**discovery**-driven endpoint resolution is a deliberately later concern; #458 makes
  the advertised etcd endpoint routable for that future consumer). The Check seam is
  exercised LOAD-BEARINGLY: real gRPC D-servers over loopback, driving the shipping
  composition, not a test double. The only element that collapses to a stand-in at Check is
  the metadata/coordination CONCRETE (redb/mem vs the production tikv/etcd), covered under
  Verification posture.
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Composition slice — mirror these peer callsites (Do MAY open them):
  metadata selection `let backend = resolve_backend(&parsed)?;` at `cli.rs:312` (`cmd_put`);
  coordination selection `let coordination = resolve_coordination_backend(&parsed)?;` at
  `cli.rs:511` (`cmd_d_server`) with its monomorphizing match dispatch at `cli.rs:530-536`;
  gRPC chunk fan-out `let fanout = connect_fanout(endpoints).await?;` at `cli.rs:1265`
  (`cluster_put`) feeding `cluster_store_put`/`cluster_store_get` (`cli.rs:1136`/`1166`); the
  whole put composition `fn cluster_put` `cli.rs:1254`. Test harness peers:
  `crates/server/tests/gateway_cluster.rs:38` (`spawn_dserver`) and
  `crates/server/tests/s3_http_wire.rs` (the S3 HTTP surface). The `Gateway<M,C,Co>` bound
  `C: PlacementChunkStore` (`crates/server/src/lib.rs:76`) already admits `GrpcFanout`, so no
  trait change is needed.
- **Prior-art check (triage cycles):** Searched `crates/server/src/cli.rs` history by path
  across merged M4 work and the deploy stack: #255/PR #427 added `resolve_backend` for
  put/get/custodian; #449 added `resolve_coordination_backend` for `cmd_d_server`; #364/#448
  added the S3 wire surface (`cmd_s3`, protocol only). The gateway was deliberately left
  standalone — `deploy/small-multi-node/docker-compose.yml:36-43` names #454 as the open gap.
  No prior open/closed/merged PR wires the gateway to the cluster backends. Not a duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
