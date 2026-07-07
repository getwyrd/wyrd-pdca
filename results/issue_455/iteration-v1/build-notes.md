# Build notes — issue 455 / e2e-closed-write-path

## Target branch / worktree note (read this first)

`$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`) is locally checked out on branch
`feat/m4.5-deploy-tikv-pd-etcd`, **not** a branch literally named
`feat/m4-production-metadata-backend`. I verified before touching anything:

```
git rev-parse HEAD                                   -> 34775e8b8974...
git fetch origin feat/m4-production-metadata-backend
git rev-parse origin/feat/m4-production-metadata-backend -> 34775e8b8974...   (same commit)
```

So the worktree's tip is **byte-identical** to the real, current
`feat/m4-production-metadata-backend` upstream — the local branch *name* differs but the
*content* is exactly the target. The other worktrees in this checkout
(`wyrd.pdca-wt-l0`, `wyrd-verify*`, `wyrd-364-signoff`) sit on a **stale** local ref for
that branch name (`99889ba`, one commit behind — before PR #460 "d-server: advertise a
routable endpoint" merged). I built and tested in `$PDCA_WORKTREE` as instructed (its
content is the real target tip), and I did **not** attempt to check out the
identically-named-but-stale local branch elsewhere.

**Consequence for citations:** PR #460 added `--advertise-addr` handling to
`crates/server/src/cli.rs` / `dserver.rs`, shifting some later line numbers by ~7 lines.
The brief's own citations (written against `99889ba`) are accurate for most callsites I
used (`cli.rs:758`, `:807`, `:838-852`, `:1118`, `:1137`, `:1207`, `:1303`,
`custodian.rs:135-142`, `:143`, `lib.rs:147`) — I re-verified every one directly against
the worktree tip (`34775e8`) with `grep -n`/`Read` before citing it. **One** citation the
brief gives (`cli.rs:582-584`, the redb exclusive-file-lock note) *had* shifted — the
same prose is at `cli.rs:589-590` on the actual tip; I cite the corrected line in the test
file's module doc, not the brief's stale one.

## What I built

`crates/server/tests/closed_write_path.rs` — one `#[tokio::test]` that:

1. spawns **four real, loopback gRPC D-servers** (`FsChunkStore` + `ChunkStoreService`
   over real tonic transport — the `spawn_dserver` helper is copied verbatim from
   `s3_gateway_cluster.rs`/`gateway_cluster.rs`, both cited peers);
2. does a **real gateway S3 PUT** — `wyrd_server::Gateway::put_object` (`lib.rs:147`),
   composed exactly as `serve_s3_dispatch` (`cli.rs:1367`) composes it for the real
   `wyrd s3` role — over a `FanoutChunkStore<GrpcChunkStore>` chunk plane
   (`connect_fanout`, `cli.rs:1118`) and a `RedbMetadataStore` opened at the SAME
   `data_dir/meta.redb` (`open_cluster_meta`, `cli.rs:1137`) the custodian reopens later;
3. drops the gateway (releasing redb's exclusive OS file lock, `cli.rs:589-590`);
4. genuinely **kills** one of the four D-servers (aborts its gRPC server task and
   `.await`s the teardown, so the OS listening port actually closes);
5. drives the **exact production fleet-assembly + repair** seams — `connect_fleet` +
   `require_aligned_topology` (`custodian.rs:143`, `cli.rs:807`),
   `run_reconstruction_over_backend` (`cli.rs:758`), `CustodianService` — over that SAME
   redb file, asserting the backlog gauge reads **1** then, on a fresh pass, **0**;
6. does a fresh gateway **GET** over a second, pre-established `FanoutChunkStore`, and
   asserts byte-identical reconstruction.

No production code changes were needed. The join test demonstrates the write path
(`Gateway::put_object`'s **identity** placement, `DServerId == fragment index`, no
domain-selector call — `lib.rs:147-155`) and the repair path (`connect_fleet`'s
operator-supplied `ids`/`failure_domain`s, `custodian.rs:97-104`) already share one
placement contract, **as long as the operator-supplied ids/domains passed to
`connect_fleet` are aligned to the same `DServerId` space the gateway's fan-out uses**
(0..n-1, endpoint order) — exactly the composition contract ADR-0008 promises. The test
asserts this directly (`inode.chunk_map[0].placement == [0, 1, 2]`), not just implicitly
through the gauge.

## Rationale for the choices I made (with concrete costs)

### 1. Library `Gateway::put_object`, not the HTTP S3 wire + `aws-sdk-s3`

The brief's own citations name **two** acceptable wirings: `serve_s3_role` (driven by
`s3_gateway_cluster.rs` via a spawned HTTP server + `aws-sdk-s3` client) **or** "the
store-sharing S3 PUT over a directly-held `Gateway`" (mirroring `e2e.rs`'s
`Gateway::new(store, chunks, coord)` + "S3 ops"), explicitly leaving the pick to Do. I
chose the second because of a concrete, quantifiable cost difference:

- **HTTP-wire route**: bind a `TcpListener`, `tokio::spawn` the `serve_s3_role` future,
  build `aws-sdk-s3` SigV4 credentials + a client (`sdk_client()` in
  `s3_gateway_cluster.rs` is ~18 lines), issue the PUT/GET as HTTP calls, then — to
  release the redb lock before the custodian reopens the file — `.abort()` the spawned
  task **and** `.await` the aborted handle to guarantee the drop actually ran (tonic
  servers spawn a task per accepted connection — see the mid-run-kill finding below — so
  even the redb-release ordering would need the same abort+await discipline). That's
  ~50+ extra lines and a new dev-dependency edge (already present for other tests, so not
  a *new* Cargo.toml line, but still surface Do doesn't need to touch).
- **Direct `Gateway` route** (what I built): `Gateway::new(meta, fanout,
  MemCoordination::new()).put_object(key, &data).await` — 4 lines; the redb lock releases
  on the ordinary drop of the owning `{ }` scope, no task lifetime to manage at all.

Both drive the identical write composition core (`Gateway::put_object`, the same method
`serve_s3`'s S3 HTTP layer calls into for a PUT) — the HTTP route adds a wire-framing
layer around the *same* call, at a real, size-quantified cost, for a scope this brief
doesn't require ("S3 ops" over a directly-held `Gateway`, the brief's own second sanctioned
wiring, already exercises the composition core).

### 2. `EcScheme::ReedSolomon { k: 2, m: 1 }` over four D-servers, not `rs(6,3)` over nine

The brief's Falsifiability field cites `rs(6,3)` / 9 D-servers only as an **example**
("e.g."). I used `rs(2,1)` over four real D-servers (three used, one held in reserve)
instead, for two concrete reasons:

- **It's the exact scheme + domain shape `custodian_day_one.rs` already establishes**
  (`four_domains()`, `write_rs_2_1`): 3 fragments across domains A,B,C, domain D free —
  proven, precedented, and it lets the assertions mirror that peer test almost line for
  line, which is exactly what "mirror the peer callsite" asks for.
- **Cost**: `rs(6,3)` needs 9 fragments identity-placed across `DServerId` 0..8. Since
  the custodian's fleet keys fragments by the *exact* `DServerId` recorded (no `% n`
  reduction — `reconstruction.rs:138`, `stores: HashMap<DServerId, &dyn ChunkStore> =
  ctx.fleet.iter().copied().collect()`), and only the *gateway's* `FanoutChunkStore`
  reduces `dserver % stores.len()` for routing, giving the custodian a genuine **spare**
  domain to repair into (required for the repair to actually complete rather than land in
  `Assessment::Blocked`, `reconstruction.rs:424-436`) needs a **10th**, wholly separate
  real D-server never wired into the write-side fan-out. That's 10 spawned gRPC servers,
  10 temp dirs, and 10-entry id/domain vectors versus 4 of each — over 2× the moving
  parts for a topology the repo does not otherwise exercise anywhere, to prove exactly
  the same property ("killing one of N is repairable, and repair completes into a genuine
  spare domain").

### 3. Kill the D-server *before* `connect_fleet` dials it, not while already connected

I first wrote the "mid-run death" version (dial while all four are alive via
`connect_fleet`, *then* abort the killed server's task, *then* run the custodian pass)
because it exercises `live_reconstruction_view`'s per-pass health probe
(`custodian.rs:200-223`), the mechanism `custodian_day_one.rs` test #1 covers with a fake
`DeadDServer`. **This failed** — a demonstrated, genuine finding, not a guess: the
custodian's health probe kept succeeding (`Ok(())`) against the "killed" server even
after `server_b.abort()` + `.await`. Root cause: tonic/hyper's `Server::serve_with_incoming`
spawns a task **per accepted connection**; aborting the outer accept-loop task only stops
new connections, it doesn't touch an already-established connection's own task, so the
custodian's already-dialed `GrpcChunkStore` channel kept answering RPCs indefinitely.

Fix: kill the D-server **before** the custodian ever dials it (still *after* the gateway's
write, so the fragment is genuinely placed there first). A fresh dial against a fully
closed listening port gets a real `ECONNREFUSED`-shaped transport error, so
`connect_fleet` reads around it exactly as `custodian_day_one.rs`'s
`connect_fleet_starts_degraded_around_a_startup_down_peer_and_repairs` (its own test #6)
covers — the "custodian (re)started during the incident" case, a legitimate day-one
scenario the brief's own citation list names (`connect_fleet`, `custodian.rs:143`), not a
weaker substitute for the mid-run case. I did **not** paper over the tonic finding with
a fake `Drop`/close call on the client side (that would be pointing the fix at the
*symptom* — an already-open, still-live TCP connection kept answering — rather than the
*cause*, which is that a genuine kill has to close the peer's socket before the probe
runs; the before-connect ordering does that directly, at zero extra code, versus writing
a "force-close-the-channel" workaround that fights the abstraction to fake a state
`connect_fleet` already exists to handle).

## Refuting my own test (forced check)

**(a) Genuine red?** Yes, on two levels:
- *Structural*: this is net-new coverage (Verification posture (a) in the brief) — no
  prior test drives a gateway PUT into a store a custodian then sweeps, so pre-patch the
  named test file (and function) does not exist; `cargo test -p wyrd-server --test
  closed_write_path` fails to even find the test binary.
- *Demonstrated* (the brief's stronger ask, "not rest red on the test's prior
  non-existence"): I temporarily pointed PASS 1's `run_reconstruction_over_backend` at a
  **fresh, empty** data dir instead of the shared `data_dir_path` (the literal "a
  custodian can open a store nothing wrote" symptom the brief's Defect field names),
  reran, and the assertion **failed exactly as predicted**:
  ```
  left: Some(0.0)
  right: Some(1.0)
  ```
  i.e. the custodian silently reported the object healthy (zero repair obligations) even
  though the real D-server was dead — the empty-store symptom, reproduced on demand. I
  then reverted the negation (confirmed via `diff` against the pre-negation file) and
  reran to confirm green. This proves the `Some(1.0)` assertion is load-bearing on the
  custodian actually reading the store the gateway wrote, not vacuously true.

**(b) Production path?** Yes. Every seam the test drives is the real production
function, not a copy:
`Gateway::put_object`/`get_object` (`lib.rs`), `connect_fanout`/`open_cluster_meta`/
`require_aligned_topology`/`run_reconstruction_over_backend` (`cli.rs`, all `pub`),
`connect_fleet`/`CustodianService` (`custodian.rs`, all `pub`). The **one** re-declared
piece is `RealDServerConnector` (8 lines), because production's own `GrpcDServerConnector`
(`cli.rs:838-852`) is a private (non-`pub`) struct — it does the *identical* thing
(`GrpcChunkStore::connect_with_timeout`), so re-declaring it is unavoidable, not a
stand-in for different behaviour. `eprintln!("wyrd custodian: D server ... unreachable at
startup ...")` in the test's own stdout during the run is `connect_fleet`'s own log line
(`custodian.rs:165-168`) firing — direct evidence the real function ran, not a
re-implementation.

**(c) Fixture includes the fault?** Yes. The killed D-server is a **real** process
(a tokio task hosting a real tonic/gRPC server bound to a real loopback TCP port,
backed by a real `FsChunkStore` that actually received the fragment from the real PUT)
— aborted and `.await`ed so its OS-level listening socket is genuinely closed, not a
`DeadDServer` fake and not curated out of the `configured` fleet's *input* (all four
endpoints are always passed to `connect_fleet`; the down one is skipped by production
logic, not by the test's setup).

## Verification run (this session)

- `cargo fmt --all -- --check` — clean (whole workspace).
- `cargo clippy -p wyrd-server --all-targets` and `cargo clippy --workspace --exclude
  wyrd-dst --all-targets` — clean, no warnings (workspace lints include `-D warnings`).
- `cargo check --workspace --exclude wyrd-dst --all-targets` — clean.
- `cargo test --workspace --exclude wyrd-dst` — all green, including
  `closed_write_path::gateway_put_is_a_custodian_visible_repair_obligation_and_round_trips`
  and the two peer tests (`s3_gateway_cluster`, `custodian_day_one`, all 11 of its cases)
  plus `gateway_cluster.rs`/`e2e.rs` — no regressions.
- `cargo test -p wyrd-server --test closed_write_path` (the brief's own Repro command)
  run 5× in a row — stable, no flakes.
- I did not additionally invoke `./engine/xtask.sh ci` end-to-end: every step
  `run_ci_steps` performs (`fmt --check`, `clippy`, `build`, `test`, all `--workspace
  --exclude wyrd-dst`) was run explicitly above and passed; the one step it skips here
  (`feature_gated_checks`, the `tikv`-feature type-check) is itself gated on
  `WYRD_TIKV_TOOLCHAIN`, unset in this environment, so it is a no-op either way — nothing
  is skipped that would otherwise run.

## What I ruled out

- **Hand-writing the object again** (repeating `custodian_day_one.rs`'s
  `write_new_object_placed`) — this is precisely the pattern the brief says to retire for
  this slice; it would prove nothing new.
- **A single-process "gateway" that never releases the redb lock** (keeping one `Gateway`
  alive for both PUT and the custodian's window) — redb's exclusive file lock
  (`cli.rs:589-590`) makes that structurally impossible; the drop-then-reopen pattern
  (already established by `custodian_day_one.rs`'s own backend-open-path test) is the
  correct, minimal realization the brief invites Do to pick.
- **`connect_lazy`/Arc-sharing tricks** to keep one `FanoutChunkStore` valid across the
  D-server kill — unnecessary once I pre-dial **two** independent fan-outs (write-side,
  read-side) while all four servers are alive; the post-repair GET's fan-out simply never
  invokes the dead slot again (the repaired placement points elsewhere), so no special
  channel-liveness handling is needed at all.
- **A fake/mocked D-server death** (e.g. injecting a `DeadDServer`-style store into the
  custodian's fleet directly) — rejected per the brief's own falsifiability ask for a
  *genuine* kill; the real-abort approach costs one extra `.await` and is not meaningfully
  more code.

## External dependencies

None beyond the base Rust toolchain — every crate used (`aws-sdk-s3` is *not* used;
`tonic`, `tokio`, `tokio-stream`, `tempfile`, `wyrd-chunkstore-grpc`,
`wyrd-coordination-mem`, `wyrd-metadata-redb`, `wyrd-custodian`, `wyrd-telemetry`,
`async-trait`) is already a normal (non-optional, non-`tikv`/`etcd`-gated) dependency of
`wyrd-server`, already exercised by the peer tests. No `Cargo.toml` changes were needed.

## STOP discipline

No branch pushed, no PR opened. `patch.diff` is a plain `git diff` of the one new,
untracked test file against the worktree's HEAD (`34775e8`, confirmed identical to
`origin/feat/m4-production-metadata-backend`).
