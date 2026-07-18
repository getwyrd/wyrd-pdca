# Build notes — issue 576 / tonic-health-readiness-probes (iteration 2)

Target branch: `pdca-integration/main` (the bundle's resolved base, `stack-base`), tip
`2c568bb` (carries #575, #577, #586). Built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`), which is checked out at that commit;
`git rev-parse HEAD` == `origin/pdca-integration/main`. All `path:line` below are against
that tree.

## What iteration 1 got rejected for, and what changed here

Iteration 1 was rejected at sign-off (advisory C3/C5): the health service bound a
**second, ephemeral** port exposed only through an in-process `health_endpoint()` getter,
so a real supervisor (systemd/k8s/LB) had **no stable or configurable address to dial** —
the operational root cause (an orchestrator can't tell which node is down) was not
removed. The carry-forward instruction: keep the reviewed-as-sound health semantics
(mapping, fail-closed, admission bypass) but give the listener a **stable/configurable
bind address** via `cli.rs` flag plumbing, and **exercise the configured address** in the
test rather than an ephemeral read-back.

This iteration does exactly that:

1. **Stable, operator-configurable health bind address.**
   - `DEFAULT_HEALTH_BIND` — a fixed, non-ephemeral `127.0.0.1:50052`, documented beside
     the data plane's `127.0.0.1:50051` default (`dserver.rs:89-90`).
   - `DServer::with_health_bind(SocketAddr)` builder (`dserver.rs:624-627`) +
     `health_bind() -> Option<SocketAddr>` accessor (`dserver.rs:633-635`).
   - **CLI flag `--health-bind ADDR`** parsed in `cmd_d_server` (`cli.rs:632-637`),
     defaulting to `dserver::DEFAULT_HEALTH_BIND`, carried in `DServerParams.health_bind`
     (`cli.rs:1382`) and applied in `run_d_server` via `.with_health_bind(...)`
     (`cli.rs:1416`). Usage string updated (`cli.rs:366`); the startup log now prints the
     probe address (`cli.rs:1424-1430`).
   - So an operator dials a **known, configured** address — not an OS-assigned ephemeral
     port they cannot discover. The getter now returns the configured value (or `None`),
     never an ephemeral read-back.

2. **The test dials the configured address**, not `health_endpoint()`. Each test
   `reserve_addr()`s a concrete loopback address, hands it to
   `with_health_bind(health_bind)` (the same knob `--health-bind` feeds), asserts the
   server reports `health_bind() == Some(health_bind)` **before** serving, then dials
   `http://{health_bind}` — the exact address it configured
   (`crates/server/tests/health_probe.rs:139-181, 209-...`). The deployment boundary a
   real supervisor crosses is what is exercised.

## What was kept from iteration 1 (reviewed as sound)

- **Mapping** (`dserver.rs:748-751`): `Ok(Healthy|Degraded)` ⇒ SERVING; `Ok(Unhealthy)`
  **and** `Err(_)` ⇒ NOT_SERVING (fail-closed).
- **Readiness keyed on the `ChunkStoreServer`'s own registered name**
  (`<ChunkStoreServer<..> as NamedService>::NAME`), set NOT_SERVING before serving so an
  early probe reads fail-closed, not `NOT_FOUND` (`dserver.rs:740-742, 753-757`). Overall
  empty-name "" service = liveness, left at tonic-health's `Serving` default.
- **Shared store** via `Arc` + `ChunkStoreService::from_arc` (`dserver.rs:714-715`) so the
  refresher polls the SAME instance the data plane serves (the affordance the brief points
  at, `crates/chunkstore-grpc/src/server.rs:57-61`).
- **Admission bypass**: the probe is served by its OWN, unlayered `Server::builder()`
  (`dserver.rs:868-877`), never `.add_service()`d onto the admission-layered data builder —
  genuinely "outside that stack" by construction (criterion (c)).
- **Bounded freshness** (`DEFAULT_HEALTH_REFRESH_INTERVAL = 3s`, overridable,
  `dserver.rs:79`, `:642`) and **no leaked task**: the refresher is `abort()`ed after
  `serve` returns on whichever `select!` arm wins (`dserver.rs:905-907`).

## The one substantive new decision this iteration forced: probe is opt-in at the library

A fixed default health port cannot be always-on at the **library** level: the existing
`crates/server/tests/dserver.rs` suite spins **several** `DServer`s in one process, and a
fixed `50052` makes the 2nd..Nth `TcpListener::bind` fail — `serve()` returns `Err` via
`?` before the data server ever serves, so those tests regress with `ConnectionRefused` on
the **data** endpoint (I hit this: `d_servers_register_serve_and_are_discovered` and
`overload_across_connections_sheds_excess_with_a_retryable_status` both failed until fixed).

So `health_bind` is `Option<SocketAddr>`, **`None` by default** (`dserver.rs:526`,
`:550`): the library building block serves a probe only when a bind address is configured;
`serve()` branches on it (`dserver.rs:726` / `:880`). The **deployable role always enables
it** — `cmd_d_server` defaults `--health-bind` to the stable `DEFAULT_HEALTH_BIND`
(`cli.rs:636`), so a production `wyrd d-server` node is always probeable on a known
address, which is the deployment surface the Success criterion and the reviewer's
root-cause both care about. Rejected alternatives and their cost:

- **Always-on with a fixed default port** — rejected: breaks the two multi-server
  `dserver.rs` tests above (concrete: 2 tests, `ConnectionRefused` on the data endpoint,
  reproduced). Not a smaller diff either.
- **Always-on with an *ephemeral* default port when unconfigured** — rejected: that is
  precisely iteration 1's ephemeral behaviour for the default case (undiscoverable), and
  `health_bind()` would then have to read back an OS-assigned port — the getter the review
  told us to stop relying on. Opt-in keeps the default honest (`None` = "no probe") and
  makes the production default a *stable* address.

## Why the separate listener (not one port), unchanged from iteration 1

The overload policy (criterion (c)) requires the probe to answer *outside* the admission
layers. tonic 0.14's `add_service` requires the leaf service's `Error = Infallible`
(`~/.cargo/.../tonic-0.14.6/src/service/router.rs`), which `LoadShedLayer`'s `Overloaded`
is not, so a single-port "layer only the chunk service" wrap does not type-check as a leaf
wrap; the axum-merge route hits the same `Into<Infallible>` bound
(`axum-0.8.9/src/routing/mod.rs`). Both one-port options therefore need a hand-written
error→response middleware (~25 lines, roughly this patch's own `ShedObserverLayer` size)
to land where a second unlayered builder reaches with zero new middleware. The reviewer
endorsed keeping the separate listener; the fix was the *address*, not the topology.

## Refutation (the three forced questions)

**(a) Genuine red?** Yes — proven by the project's own runner, not a hand-rolled command:
`PDCA_BUNDLE=results/issue_576 PDCA_VERIFY_BASE=origin/pdca-integration/main
./engine/scripts/run-verify.sh` applied `patch.diff` to a clean checkout at the resolved
base, ran the GREEN leg (3/3 pass), then **reverted the production files, kept the test**,
and re-ran: RED — a compile failure (`unresolved crate tonic_health` ×3, `no method
with_health_bind`), because the revert removes the `tonic-health` dependency and the
`DServer` methods the test drives. The script's own verdict: `PASS — red without the fix,
green with it.` The brief's Falsifiability section names this exact compile-fail red as
gate-acceptable. Behaviourally the same three tests would also go red on a *wrong* fix:
criterion (b) reverts to green only if `Err(_)`→NOT_SERVING is implemented; criterion (c)
would time out / read `RESOURCE_EXHAUSTED` if the probe were composed inside the
admission-layered builder (`max_concurrent_requests: 1`, one slot held by a real in-flight
`get_fragment`).

**(b) Production path?** Yes. The test drives `wyrd_server::dserver::DServer::bind`/
`::serve` directly over real loopback TCP, and queries the real
`tonic_health::pb::health_client::HealthClient` against the real `HealthServer` that
`DServer::serve` registers. No mock health service; the whole
`Health{y,…}/Err → Serving/NotServing` mapping lives only in `dserver.rs` — the test only
sets `HealthMode` and asserts the wire status. The `--health-bind` → `with_health_bind`
plumbing the test exercises is the identical builder call `run_d_server` makes
(`cli.rs:1416`).

**(c) Fixture includes the fault?** Yes. (a)/(b) wrap a **real** `FsChunkStore` (the
backend `wyrd d-server` runs) and only substitute a runtime-controllable `health()`; the
put/get/list/delete path is the unmodified real store. (c) saturates the admission bound
with a **real** `GrpcChunkStore` client issuing a **real** `get_fragment` the server
actually admits and holds (via `entered`/`gate`, mirrored from `dserver.rs`'s own
admission-saturation test) — the health `Check` is asserted **while** that request
genuinely holds the one slot (`max_concurrent_requests: 1`), confirmed by
`entered_rx.recv().await` before the probe is dialed. The failing element (the held slot,
the unhealthy/erroring store) is present, not curated out.

## Verification run log (this session, in `$PDCA_WORKTREE`)

- `cargo build -p wyrd-server` — clean.
- `cargo test -p wyrd-server --test health_probe` — 3/3 pass.
- `cargo test -p wyrd-server` (whole crate) — all pass (no regression; the two
  multi-server `dserver.rs` tests that the fixed-port draft broke now pass, because the
  probe is opt-in).
- `cargo fmt --all --check` — clean. `cargo clippy -p wyrd-server --lib --bins --tests -D
  warnings` — clean.
- `cargo deny check licenses bans sources` — `bans ok, licenses ok, sources ok`
  (`tonic-health` 0.14.6 is MIT, already allowlisted).
- `cargo check --workspace --all-targets` — clean.
- `./engine/scripts/run-verify.sh` (the C4-verify runner) — `PASS — red without the fix,
  green with it.`

I did NOT run the whole-tree `cargo xtask ci` (Check's gate; expensive) — every sub-check
it runs for the touched files (fmt, clippy, deny, build, targeted tests) was run above.

## NEEDS-HUMAN (pre-declared by the brief)

`tonic-health` is a **new dependency**: per the brief's "Pre-declared NEEDS-HUMAN" and
INTEGRATION §4, an ADR-0003 three-test audit + `deny.toml` allowlist judgement is a
human-only sign-off item. `cargo deny` mechanically confirms the MIT license (already
allowlisted) but does not substitute for the audit call. Expected at sign-off, not a
surprise.
