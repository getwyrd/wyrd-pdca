# Build notes — issue 576 / tonic-health-readiness-probes

## What I built

`crates/server/src/dserver.rs` (`DServer::bind`/`serve`, `main` line numbers cited
against my target branch — the resolved stack base `pdca-integration/main` at
`ab01947`, per `results/issue_576/stack-base`; see "Base branch" note below):

- **A second listener** (`health_listener`, bound in `bind()` alongside the existing
  data-plane `listener`, `dserver.rs:528-536`) hosting the standard
  `grpc.health.v1.Health` service (`tonic_health`), exposed via a new
  `DServer::health_endpoint()` accessor (`dserver.rs:611-616`).
- **Readiness derivation** in `serve()` (`dserver.rs:686-756`): a `HealthReporter` /
  `HealthServer` pair from `tonic_health::server::health_reporter()`; readiness is set
  on the `ChunkStoreServer`'s own registered name (`"wyrd.v0.ChunkStore"`, reused
  rather than inventing a bespoke identifier) and refreshed every
  `health_refresh_interval` (default `DEFAULT_HEALTH_REFRESH_INTERVAL = 3s`,
  overridable via a new `with_health_refresh_interval` builder method) by re-reading
  the store's `health()`: `Healthy`/`Degraded` → SERVING, `Unhealthy`/`Err(_)` →
  NOT_SERVING (fail-closed). Set to NOT_SERVING **before** anything is served, so a
  probe landing before the first read completes reads fail-closed rather than
  NOT_FOUND. The **liveness** signal (the empty-name `""` overall service) is left at
  `tonic-health`'s own default (`Serving`, set the instant the reporter is created) —
  untouched — because that default already *is* "the process answers at all" (brief's
  Design, "the overall (empty-name) service is the liveness signal").
- **A shared store**: `serve()` now wraps `self.store` in `Arc` and constructs
  `ChunkStoreService::from_arc(Arc::clone(&store))` (an existing, unused affordance at
  `crates/chunkstore-grpc/src/server.rs:57-61`) instead of `ChunkStoreService::new`, so
  the readiness-refresh task polls the exact same store instance the data plane serves
  — the brief flags this ("sharing it is Do's plumbing to solve") and the fix is a
  three-line change using a method that already existed for this purpose.
- **The health service bypasses the admission stack** by being served on its **own,
  unlayered `Server::builder()`** on `health_listener`, rather than being
  `.add_service()`d onto the same builder the admission layers (`ShedObserverLayer`,
  `LoadShedLayer`, `GlobalConcurrencyLimitLayer`, `AdmissionObserverLayer`) wrap.
- **Shutdown fan-out**: the single `shutdown: impl Future` parameter is now bounded
  `Send + 'static` and consumed by one spawned task that fires a `tokio::sync::watch`
  channel once, cloned into two receivers — one per tonic server — so both the
  data-plane and health listeners stop on the same signal. The two server futures are
  joined (`tokio::join!`), not raced, so a fast health-server shutdown can't drop the
  data-plane server mid-drain. The readiness-refresh task is `tokio::spawn`ed and
  explicitly `.abort()`ed after the servers finish, so it never outlives `serve`.
- **`Cargo.toml` / `crates/server/Cargo.toml`**: added `tonic-health = "0.14"`
  (workspace) / `tonic-health.workspace = true` (server crate). `Cargo.lock` picked up
  `tonic-health 0.14.6` (matches the pinned `tonic 0.14.6`) and one new edge
  (`tokio-stream` → `tokio-util`, an already-present package, no new crate).
- **`crates/server/tests/health_probe.rs`** (new file): three `#[tokio::test]`s, one
  per success criterion (a/b/c).

## Why the "separate port" mechanism, and what I ruled out

The brief pins the overload policy ("the health service bypasses the data-plane
admission layers") but leaves the *mechanism* to Do. I considered three shapes before
picking a second listener:

1. **Wrap `ChunkStoreServer` in the admission layers before `.add_service()`, register
   the health service unwrapped on the same `Server::builder()`, drop the builder-level
   `.layer()` calls.** Ruled out: `tonic::transport::Server::add_service` (and
   `tonic::service::Routes::add_service`) require the leaf service's
   `Error = Infallible` (verified in the vendored source,
   `~/.cargo/registry/.../tonic-0.14.6/src/service/router.rs:26-36`). `LoadShedLayer`'s
   `Overloaded` and `GlobalConcurrencyLimitLayer`'s errors are NOT `Infallible` — they
   are real runtime `Err`s tonic's own `RecoverErrorLayer` (applied outside the
   `Routes` boundary, `tonic-0.14.6/src/transport/server/mod.rs:1234-1239`) converts to
   HTTP responses. A wrapped leaf service therefore cannot satisfy `add_service`'s
   bound; this path does not type-check as a leaf-level wrap.
2. **Merge two `axum::Router`s (`Routes::into_axum_router()` / `Routes::from`), with the
   admission layers applied only to the chunk sub-router via `axum::Router::layer()`
   before merging.** Ruled out on the same principle: `axum::Router::layer` (axum
   0.8.9, `~/.cargo/registry/.../axum-0.8.9/src/routing/mod.rs:303-317`) requires
   `<L::Service as Service<Request>>::Error: Into<Infallible>` — i.e. the layered
   service must itself be infallible. Making `LoadShedLayer`/`GlobalConcurrencyLimitLayer`
   satisfy that would mean hand-writing a wrapper that catches `Overloaded` INSIDE the
   layer and converts it directly to a `RESOURCE_EXHAUSTED` `Response` before it ever
   becomes a type-level `Err` — new middleware code, not a call-site reshuffle, and it
   duplicates logic tonic's own `RecoverErrorLayer` already does for the whole-server
   case. Concretely this is not a "swap two lines" cost: it is a new
   `Service`/`Layer` impl (roughly the size of this patch's own `ShedObserverLayer`,
   ~25 lines) plus the merge/convert plumbing, to land at the SAME place the
   two-listener design reaches with zero new middleware.
3. **A second listener + a second, unlayered `Server::builder()`** (what I built):
   genuinely outside the admission stack by construction — no per-service layering
   trick needed, because there is no shared builder to leak through. Costs one extra
   `TcpListener::bind` in `DServer::bind` (already async, already fallible via `?`) and
   one accessor (`health_endpoint()`); does not touch the existing `endpoint()` /
   `with_advertise_addr` contract at all. This is also a common real-world pattern
   (Kubernetes' `grpc` probe type accepts an explicit `port:`, distinct from the
   service port, precisely so a probe is never contended by the data plane) — not a
   workaround invented for this patch.

I did not find a fourth option that keeps ONE port AND avoids hand-written
error-to-response middleware, given tonic 0.14's `Infallible`-at-the-leaf design (a
property of the vendored dependency, verified by reading its source, not assumed).

## Why the store is shared via `Arc` + `ChunkStoreService::from_arc`, not re-opened

The brief explicitly flags this ("note `serve` currently moves the store into
`ChunkStoreService`, `dserver.rs:286` — sharing it is Do's plumbing to solve, not a
reason to probe a different store instance"). A second, independently-opened store
instance would be the "locally-reasonable but globally-wrong" trap the brief's
peer-callsite exception exists to head off: for `FsChunkStore` a second instance over
the same directory does not even necessarily observe the SAME health signal a
concurrent writer sees, and for a networked backend it could dial a different node
entirely. `ChunkStoreService::from_arc(Arc<S>)` already exists in
`crates/chunkstore-grpc/src/server.rs:57-61` for exactly this "share with another role
in the same process" case, so no new production surface was needed in that crate —
only `dserver.rs` now calls the affordance that was already there.

## Why `Check`-polling in the test, not the `Watch` streaming RPC

`tonic-health` also exposes a server-streaming `Watch` RPC that would avoid polling.
I considered it (and it would work), but `wait_for_check`'s bounded poll-until-expected
loop is simpler to read and reason about (no stream lifetime / cancellation
bookkeeping in the test), and the freshness cadence is itself the thing under test
(`with_health_refresh_interval` set to 20ms) — a 10ms poll interval against a 5s
timeout budget is not doing anything a `Watch` stream would meaningfully improve on
for THIS assertion. Noted as a rejected-on-simplicity alternative, not a correctness
one: cost was "one more RPC shape + `tokio_stream::StreamExt` in the test" against no
behavioural benefit for what's being asserted.

## Base branch note

The brief's `Repo + branch target` says `getwyrd/wyrd @ main`, but
`results/issue_576/stack-base` (written by Plan) names `pdca-integration/main`, whose
tip at the time I built (`ab01947`) already carries issues #575 (admission
observability — the capacity-plane types in `dserver.rs`) and #577 (typed transient
errors) merged. I built against `$PDCA_WORKTREE`, which was already checked out at that
commit, and confirmed there is no drift between it and
`origin/pdca-integration/main` on every file this patch touches (`git diff
ab01947..origin/pdca-integration/main -- Cargo.toml Cargo.lock crates/server/Cargo.toml
crates/server/src/dserver.rs` — empty). This matches the brief's own "Conflicts with:
575" / "Ordering note" — 575 lands first on `dserver.rs`, this issue's Do work is on
top of it, consistent with how the two would actually be sequenced in one dserver.rs
lineage.

## Reading beyond the brief's named citations

Brief's `Citations expected` names three peer callsites Do MAY open:
`dserver.rs:276-323` (the `serve` builder), `crates/traits/src/lib.rs:357`/`:408`
(the `Health` seam — landed at `:554`/`:605` on my actual base, shifted by #577's
additions upstream in the same file), and `crates/chunkstore-grpc/tests/round_trip.rs`
(the loopback-test shape). Beyond those, to implement I additionally read:

- `crates/chunkstore-grpc/src/server.rs` (found `ChunkStoreService::from_arc`, the
  affordance the brief's "sharing it is Do's plumbing to solve" note points at) and
  `crates/chunkstore-grpc/src/client.rs` (the `Endpoint::try_from(...).connect()`
  pattern my test's `health_client` helper mirrors) — both are the production
  implementation of the ONE cited peer test (`round_trip.rs` exercises exactly this
  server/client pair), so I read the two source files that test's citation points at.
- `crates/server/tests/dserver.rs` (the existing test module for the file the brief
  hands me at `:276-323`) — to understand `DServer::serve`'s actual call shape
  (`register` → `serve`, the `AdmissionControl` construction) before touching its
  signature, and to see the established `GateStore`-style pattern for saturating the
  server-wide admission bound (needed for criterion (c)). This file is also named in
  the brief's own Falsifiability paragraph as a "peer" (alongside `round_trip.rs`),
  though only `round_trip.rs` is repeated in the formal `Citations expected` allow-list
  — flagging this explicitly since it is a looser reading than the strict one-citation
  rule, done because criterion (c) specifically requires re-deriving the SAME
  admission-saturation mechanism `dserver.rs`'s own test already established as
  correct for this exact crate, and inventing an different, untested way to saturate
  `max_concurrent_requests` seemed like exactly the "locally-reasonable but
  globally-wrong" trap the exception exists to avoid.
- `~/.cargo/registry/.../tonic-0.14.6/src/transport/server/mod.rs`,
  `.../src/service/router.rs`, and `~/.cargo/registry/.../axum-0.8.9/src/routing/mod.rs`
  — the new dependency's (and axum's, transitively load-bearing to the "two listeners
  vs. one" decision) own source, to confirm the `Infallible`-at-the-leaf constraint
  that rules out options 1/2 above rather than assuming it.
- `~/.cargo/registry/.../tonic-health-0.14.6/src/{lib,server}.rs` — the new
  dependency's own source, to get `HealthReporter`/`health_reporter()`/
  `set_service_status`/`ServingStatus` right (there is no other source for this).

I judge all of the above as implementation research on the target repo's own source
and the new dependency's own source (never another issue's brief, another PDCA cycle's
artifacts, or the conformance ruleset) — the kind of reading "narrow input" is not
meant to foreclose — but I'm naming it plainly rather than silently going past the
formal citation list, per the instruction to flag departures.

## Refutation (the three questions)

**(a) Genuine red?** Yes. Ran `engine/scripts/run-verify.sh` (the project's own
C4-verify runner) with `PDCA_BUNDLE=results/issue_576
PDCA_VERIFY_BASE=origin/pdca-integration/main`: it applies `patch.diff` to a clean
worktree at that base, runs `cargo test -p wyrd-server --test health_probe` (GREEN: 3
passed), then reverts the production files only (keeping the test) and re-runs: RED —
a **compile failure** (`E0433 cannot find module tonic_health` ×3,
`E0599 no method named with_health_refresh_interval`), because reverting removes the
`tonic-health` dependency and the `DServer` methods the test calls. The brief's
Falsifiability section names this exact shape as an acceptable red
("since the revert also removes the `tonic-health` dependency ... the RED leg fails to
compile, which the gate equally counts as red"), and the script's own verdict was
`PASS — red without the fix, green with it.` (exit 0).

**(b) Production path?** Yes. The test drives `wyrd_server::dserver::DServer::bind`/
`::serve` directly (not a copy) over a real loopback TCP connection, and asserts
against the real `tonic_health::pb::health_client::HealthClient` talking to the real
`tonic_health::server::HealthServer` `DServer::serve` registers — no mock health
service, no re-implementation of the mapping logic in the test (the test only sets
`HealthMode` and asserts the wire status; the `Healthy`/`Degraded`/`Unhealthy`/`Err`→
`Serving`/`NotServing` mapping lives solely in `dserver.rs`).

**(c) Fixture includes the fault?** Yes for all three. (a)/(b): `ControllableStore`
wraps a REAL `FsChunkStore` (the same backend `wyrd d-server` runs in production,
`crates/chunkstore-fs`) and only substitutes a runtime-controllable `health()` — the
put/get/list/delete path is the real store, unmodified. (c): the admission bound is
saturated with a REAL `GrpcChunkStore` client issuing a REAL `get_fragment` RPC that
the server actually admits and holds open (via the `entered`/`gate` mechanism mirrored
from `crates/server/tests/dserver.rs`'s own admission-saturation test) — not a
fixture that excludes the saturating request; the health check is asserted WHILE that
request genuinely holds the one available slot (`max_concurrent_requests: 1`),
confirmed via `entered_rx.recv().await` before the health `Check` is issued.

## Verification run log (this session)

- `cargo build -p wyrd-server --lib` — clean (production compiles).
- `cargo test -p wyrd-server --test health_probe` — 3/3 pass in ~0.11s.
- `cargo fmt -p wyrd-server -- --check` — clean (ran `cargo fmt -p wyrd-server` once to
  apply the project's formatting to the new test file; re-checked clean after).
- `cargo clippy -p wyrd-server --lib --tests -- -D warnings` — clean.
- `cargo deny check licenses bans sources` — `bans ok, licenses ok, sources ok`
  (one pre-existing unrelated warning: an unmatched `"ISC"` allowance, not introduced by
  this patch). `tonic-health` is `MIT`, already on the `deny.toml` allowlist.
- `cargo test -p wyrd-server --test dserver --test request_capacity_planes --test
  read_fanout --test write_fanout` — all pre-existing tests still pass (no regression
  from the `serve()` signature/body change).
- `cargo test -p wyrd-server --tests --no-run` — every test binary in the crate still
  compiles (the `shutdown: impl Future<Output = ()> + Send + 'static` bound tightening
  does not break any existing caller, including `cli.rs`'s `cmd_d_server`).
- `cargo check --workspace --all-targets` (default features) — clean.
- `./engine/scripts/run-verify.sh` (`PDCA_VERIFY_BASE=origin/pdca-integration/main`) —
  `PASS — red without the fix, green with it.` (exit 0). This is the "project's own
  test runner" the harness instructions name; I did not hand-roll a raw `cargo test`
  invocation as the basis for the red/green claim above — the script's own verdict is
  quoted above.

I did **not** run the full whole-tree `cargo xtask ci` (the driver's own C4-ci gate,
which also runs conformance/DST/deny-advisories/fmt/clippy across the entire
workspace) inside this Do session — it is Check's gate to run, and a full run is
expensive; every sub-check `cargo xtask ci` would exercise for the crate/files this
patch touches (fmt, clippy, deny licenses/bans/sources, build, targeted tests) was run
individually above and is clean.

## NEEDS-HUMAN (pre-declared by the brief, not a discovery of mine)

`tonic-health` is a new dependency. Per the brief ("Pre-declared NEEDS-HUMAN") and
INTEGRATION §4, this is a project-defined human-only sign-off item (ADR-0003 three-test
audit + `deny.toml` allowlist judgement) — `cargo deny check` mechanically confirms the
license (`MIT`, already allowlisted) but does not substitute for the human audit call.
Expected at sign-off, not a surprise raised here.
