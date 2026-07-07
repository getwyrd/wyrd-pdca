# Build notes — issue 454 / gateway-composes-over-cluster-backends

**Target branch (worktree base):** `feat/m4.5-deploy-tikv-pd-etcd`
(`$PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt`). All `path:line` citations below are
against that tree, post-edit.

## What the invariant is (and why the fix is shaped this way)

The brief names an **Invariant to restore** (parity/composition, not load-safety): *every
cluster-facing server role selects its metadata, coordination, and chunk backends by
configuration, not by hardcoded constructors.* The gateway was the one role that resolved
none of them — `cmd_s3` built `Gateway::new(RedbMetadataStore::open(dir/meta.redb),
FsChunkStore::open(dir/chunks), MemCoordination::new())` unconditionally
(base `cli.rs:1218-1222`). The target here is therefore *the smallest change that restores
the config-driven selection*, not the smallest textual diff (`docs/principles.md` §1.2/§2).
So the fix mirrors the established peer pattern rather than bolting a flag onto the
hardcode:

- metadata via `resolve_backend` — as `cmd_put` (`cli.rs:312`), `cmd_get`
  (`cli.rs:399`), custodian (`cli.rs:613`);
- coordination via `resolve_coordination_backend` — as `cmd_d_server` (`cli.rs:511`);
- chunk plane via `connect_fanout(--endpoints)` — the same static-endpoints fan-out that
  backs `cluster_put`/`cluster_get` (`cli.rs:1423`/`1466`).

## The change

`cmd_s3` (`cli.rs:1193`) now resolves `backend = resolve_backend(&parsed)`
(`cli.rs:1224`), `coordination = resolve_coordination_backend(&parsed)` (`cli.rs:1225`),
and an optional `--endpoints` list (`parse_endpoints`), then hands them to a factored-out
composition core. Three new helpers:

- `serve_s3_role` (`cli.rs:1289`, **pub**) — the composition core. `Some(endpoints)` ⇒
  the **cluster** front door: chunk plane = `connect_fanout(endpoints)` (`cli.rs:1303`),
  no local `FsChunkStore`. `None` ⇒ the single-node local-FS front door
  (`open_local_chunks`), preserving today's #367 loopback behaviour.
- `serve_s3_dispatch<C>` (`cli.rs:1340`) — the **two-axis** metadata × coordination
  dispatch, each `(backend, coordination)` arm monomorphizing a distinct
  `Gateway<M, C, Co>`. The `tikv`/`etcd` arms are `#[cfg(feature=…)]`-gated exactly as the
  peers' single-axis matches are (`cluster_put` `cli.rs:1424`, `cmd_d_server`
  `cli.rs:530`), so the default build compiles only `(Redb, Mem)`.
- `serve_s3<M, C, Co>` (`cli.rs:1387`, **pub**) — `gateway.recover()` (retains the #364
  durability finding-1 id-allocator recovery that the old inline path did) + serve the S3
  wire surface. Generic over all three seams; `Gateway<M,C,Co>: ObjectGateway`
  (`lib.rs:205`) already admits every combination, so **no trait change** is needed
  (brief §Citations).

All four backend arms are genuinely wired — `tikv` resolves through `open_tikv_meta`,
`etcd` through `open_etcd_coordination`, identical to the peer roles; no `todo!()`/redb
shortcut.

`usage()` (`cli.rs:240`) advertises the new flags. `deploy/small-multi-node/
docker-compose.yml` gateways `gateway0/1/2` now pass `--metadata-backend tikv
--coordination-backend etcd --endpoints http://dserver0…dserver8` with
`WYRD_TIKV_PD_ENDPOINTS` + `WYRD_ETCD_ENDPOINTS` and `depends_on` tikv/etcd/dservers,
mirroring the custodian service; the "STANDALONE per #454" header + section comments are
retired (this is the configuration precondition #455 demonstrates against).

## Why the test drives `serve_s3_role`, not `cmd_s3` directly

`cmd_s3` binds a listener on a fixed `--s3-listen` and `serve()`s until Ctrl-C — it never
returns, and it prints the ephemeral port to stderr, so it is not directly drivable in an
in-process round trip. So — exactly as `cluster_put` factors out `cluster_store_put` for
`gateway_cluster.rs` — the composition core is factored into `serve_s3_role`, which takes
a **pre-bound listener** the test owns. The test (`tests/s3_gateway_cluster.rs`) stands up
a 4-D-server loopback gRPC cluster (mirrors `gateway_cluster.rs::spawn_dserver`), calls
`serve_s3_role(Redb, Mem, data_dir, Some(&endpoints), …, listener)` — the **same**
production code `cmd_s3` runs for the `--endpoints` arm — and drives an S3 PUT→GET with a
**stock `aws-sdk-s3` client** (mirrors `s3_http_wire.rs::real_sdk_interop`). Because the
test calls the real dispatch, the "local chunks empty" assertion is discriminating: had the
dispatch wrongly composed over a local `FsChunkStore`, `data-dir/chunks` would fill.

The two-axis `tikv`/`etcd` arms are cfg-gated and not built by the default gate; their
composition-over-backends is the same monomorphized shape the redb/mem arm proves at Check,
differing only in the concrete store (Verification posture (b)). They are compiled here
(see below) but their live behaviour is #455, off-Check.

## Refuting my own test (forced check)

- **(a) Genuine red?** Yes. Stashed the `cli.rs` change (test file untracked, so it
  remained) and rebuilt: `error[E0432]: unresolved import
  wyrd_server::cli::serve_s3_role — no serve_s3_role in cli`. The test does not compile
  against the shipped surface without the fix (exactly the brief's RED). With the fix:
  `test result: ok. 1 passed`.
- **(b) Production path?** Yes. The test drives `wyrd_server::cli::serve_s3_role` — the
  identical function `cmd_s3` invokes (the call is at `cli.rs:1260`) — over
  `connect_fanout` + redb + `MemCoordination`, and the S3 wire is driven by a real
  `aws-sdk-s3` client against a real `axum` listener + real tonic D-servers. No mock,
  copy, or re-implementation of the gateway.
- **(c) Fixture includes the fault?** Yes. The failing element is "chunks must cross the
  wire to real D-servers, not a local disk". The fixture is four **real** loopback gRPC
  D-servers; the test counts the `.frag` files that actually land on their on-disk stores
  (`total_fragments > 0`, `servers_with_fragments >= 2` for rs(6,3)'s 9 fragments) and
  asserts the local `data-dir/chunks` store stays empty. The D-servers are included, not
  curated out.

## Cost / alternatives considered

- **Rejected: add a `--endpoints` flag onto the existing hardcoded composition** (keep
  `FsChunkStore`, branch only on metadata). That guards the symptom and leaves the
  chunk-plane hardcode — it would not restore the invariant (chunks would still hit local
  disk). The invariant, not diff size, is the deciding axis here.
- **Rejected: test `cmd_s3` end-to-end by spawning the process.** `cmd_s3` blocks on
  `serve()` and only announces its ephemeral port on stderr; capturing it in-process is
  brittle. Factoring `serve_s3_role` (the peer pattern) is the smaller, honest seam and
  keeps the unit import-light and headless (no display, real loopback sockets only).
- **Diff size:** `cli.rs` +174/-16 (net +158, most of it the doc-commented two-axis
  dispatch the brief rates "high — hold every combination's wiring in view"); test +231;
  compose +30/-19. No trait/`lib.rs` change.

## Verification performed (headless, in `$PDCA_WORKTREE`)

- `cargo check -p wyrd-server` (default features) — clean.
- `cargo test -p wyrd-server --test s3_gateway_cluster` — **1 passed** (green).
- RED proof: change stashed ⇒ test fails to compile (`no serve_s3_role in cli`).
- `cargo fmt -p wyrd-server -- --check` — clean (commit-ready).
- `cargo clippy -p wyrd-server --tests --all-targets` (default features) — clean.
- `cargo check -p wyrd-server --features tikv,etcd` — **compiles**: all four dispatch
  arms (`(Redb,Mem)`, `(Tikv,Mem)`, `(Redb,Etcd)`, `(Tikv,Etcd)`) build.
- `cargo test -p wyrd-server --lib --test gateway_cluster` — 8 + 1 passed (no regression
  in the shared composition helpers).

### Pre-existing, out-of-scope note (not introduced by this change)

`cargo clippy -p wyrd-server --features tikv,etcd` reports two
`needless_question_mark` findings at `cli.rs:139` (`open_tikv_meta`) and `cli.rs:201`
(`open_etcd_coordination`). I confirmed these are **pre-existing**: they persist with my
`cli.rs` change stashed. They are in helpers I did not modify, and the gating check
`cargo xtask ci` runs on **default features** (no tikv/etcd), where clippy is fully clean.
Fixing them is outside this slice (unrelated lint, not part of restoring the #454
invariant); flagging here for the maintainer's awareness.

## Deferred (off-Check) — how to validate the live arms at sign-off

Not a NEEDS-HUMAN external-dependency blocker: the brief's Check-exercised arm
(redb + `MemCoordination` + gRPC fan-out) is fully driven above with no external services.
The `tikv`/`etcd` arms are cfg-gated code that **compiles** at Check; their live run is
#455's criterion, off-Check. To validate manually on the stood-up stack:

```
# from deploy/small-multi-node/ (image built --features tikv,etcd)
docker compose up -d
# S3 PUT/GET through a gateway; then confirm chunk fragments live on the D servers and
# metadata in TiKV (not a per-gateway local redb):
aws --endpoint-url http://localhost:8081 s3 cp ./obj s3://wyrd-bucket/k    # gateway0
aws --endpoint-url http://localhost:8082 s3 cp s3://wyrd-bucket/k ./got    # gateway1 → same object
```
A GET through a *different* gateway than the PUT returning the object byte-identical is the
"one pool, not three islands" acceptance (#455).
