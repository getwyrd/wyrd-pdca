# Brief — issue 458 / d-server-advertise-addr

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** d-server-advertise-addr
- **Defect:** `wyrd d-server` registers, for discovery, the endpoint derived from its
  **bound** socket address. `DServer::bind` sets `endpoint = format!("http://{addr}")`
  from `listener.local_addr()` (`crates/server/src/dserver.rs:191`), and the doc there
  notes "NAT / split-horizon advertisement is a later deployment concern"
  (`dserver.rs:184`). Under the containerized `--bind 0.0.0.0:50051` used by
  `deploy/small-multi-node/`, the value written into etcd L5 coordination is
  `http://0.0.0.0:50051` — a wildcard no other container can dial. `--bind` also requires
  a numeric `SocketAddr` (`crates/server/src/cli.rs:463`), so a routable DNS service name
  cannot be bound as a workaround. There is no way to advertise a routable address distinct
  from the bind address.
- **Success criterion:** A d-server bound to a wildcard/loopback address but given a
  distinct advertise address registers **that advertised endpoint** into L5 coordination:
  after `register`, discovery decodes the `DServerRegistration.endpoint` as
  `http://<advertise>` — NOT the bound (`0.0.0.0`/ephemeral) address. With no advertise
  flag set, the registered endpoint remains the bound-address value exactly as today
  (loopback behaviour preserved). Demonstrable by C4-verify at Check: `cargo xtask ci` runs
  the new register/discover integration test red→green over an in-process `MemCoordination`,
  no external services.
- **Falsifiability:** RED is producible at Check on an **in-process `MemCoordination`
  register→discover→decode** test (the `failure_domain_registration.rs:36-53` model) under
  `cargo test` — no etcd, no Docker. Pre-fix: there is no advertise API and the registered
  endpoint is derived from the bound address, so an assertion that the discovered
  `DServerRegistration.endpoint == "http://dserver-x:50051"` (with the server bound on
  `127.0.0.1:0`) fails — and the builder the test drives does not yet exist, so it does not
  compile against the shipped surface. Post-fix: GREEN. The live cross-container dial (a peer
  container reaching `http://dserverN:50051`) is off-Check config confirmed on the deploy
  stack (see Verification posture); binding here on the in-process registration record keeps
  RED producible on the environment Do gets.
- **Invariant to restore:** The endpoint a server advertises for discovery must be an
  address its consumers can actually dial, decoupled from the wildcard/loopback address it
  binds — a server behind NAT or in a container must be able to publish a routable endpoint
  distinct from its listen socket. Source: the in-code deferral note "split-horizon
  advertisement is a later deployment concern" (`dserver.rs:184`) — now due — and the L5
  discovery contract (proposal 0005 §"The placement record", the `{ id, endpoint,
  failure-domain }` registration record, `dserver.rs:138`). Behavioural/feature gap
  (principles.md §1.1): the fix adds the missing advertise seam; it is not a structural
  load-safety defect.
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration branch per INTEGRATION §2)
- **Depends on:**
- **Conflicts with:** 454
- **Ordering note:** #458 and #454 both edit `crates/server/src/cli.rs` (the `wyrd d-server`
  vs `wyrd s3` cmd functions and the adjacent usage/`eprintln!` help block ~236-249) — no
  build-on dependency, but a shared file, so they land in DIFFERENT waves rather than build
  blind on the same base. Schedule #454 first (critical path for #455); #458 rebases onto
  #454's accepted diff.
- **Surfaces:** data
- **Difficulty:** low — a localized additive change: parse one flag in `cmd_d_server`, carry
  it on `DServerParams`, thread it through `run_d_server` into a `DServer` builder mirroring
  `with_identity`, default to the bind address when unset, plus a one-line-per-service
  compose edit and a usage-string line. One logical change, one call site, no effect
  propagation beyond the registration record.
- **Scope:** Add `--advertise-addr ADDR` to `wyrd d-server` and thread it so the endpoint
  the server **registers** (advertises through L5 `Coordination`) is the advertised address,
  defaulting to the bound address when the flag is unset (preserving today's loopback
  behaviour); set it per-D-server in `deploy/small-multi-node/docker-compose.yml` to each
  server's routable service name (`http://dserverN:50051`). / out of scope: consuming
  etcd-discovered D-server addresses (no live consumer resolves them yet — the custodian
  uses static `--endpoints`, the gateway is standalone until #454); changing `--bind`'s
  `SocketAddr` type; TLS/scheme negotiation (keep the `http://` scheme derivation).
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read `DServer::bind`
  (`crates/server/src/dserver.rs:185-197`): `endpoint` is `format!("http://{addr}")` where
  `addr = listener.local_addr()`. Bind a server on `0.0.0.0:0` and call `registration()`
  / `register` into a `MemCoordination`; the recorded endpoint is `http://0.0.0.0:<port>`.
  There is no builder or flag to override it — `with_identity` (`dserver.rs:202`) sets id +
  failure-domain but not the advertised endpoint. The new test (below) makes this executable.
- **External dependencies:** none beyond the base toolchain — the regression test uses the
  in-process `wyrd-coordination-mem` `MemCoordination` (no etcd, no Docker). The
  `docker-compose.yml` edit is configuration; its live cross-container effect is verified
  off-Check on the deploy stack (see Verification posture), not by the gate.
- **Test file:** crates/server/tests/advertise_addr_registration.rs (new — mirror
  `crates/server/tests/failure_domain_registration.rs`'s bind→register→discover→decode
  harness over `MemCoordination`)
- **Verification posture:** Default flippable regression at Check: the advertise seam
  (flag → `DServerParams` → `run_d_server` → `DServer` builder → registration record) is
  BUILT and EXERCISED by the in-process register/discover integration test, red pre-fix,
  green post-fix under `cargo xtask ci`. The `docker-compose.yml` edit's LIVE effect — a
  peer container successfully dialing the registered `http://dserverN:50051` — is off-Check
  configuration confirmed by the maintainer on the stood-up `deploy/small-multi-node/` stack
  (it is a one-line-per-service `--advertise-addr` addition, not a separate deliverable).
- **Production reach:** This slice fixes the registration INPUT (the advertised endpoint) so
  a future discovery-driven consumer can dial a routable address. At Check the seam is
  exercised load-bearingly by the register/discover test, but note there is **no live
  production consumer** of etcd-discovered D-server addresses yet — the custodian uses static
  `--endpoints`, and the gateway is standalone until #454 (the issue calls this out as
  latent). The advertised value is correct and tested now; the discovery-driven read path
  that consumes it lands in a later slice (a discovery-driven custodian/gateway).
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Composition slice — mirror these peers (Do MAY open them): the endpoint derivation to
  override `endpoint: format!("http://{addr}")` at `crates/server/src/dserver.rs:191` inside
  `DServer::bind` (`dserver.rs:185`); the builder pattern to mirror `pub fn with_identity`
  at `dserver.rs:202`; the registration record `fn registration` / `fn register` at
  `dserver.rs:235`/`246`; the CLI wiring `struct DServerParams` `cli.rs:891`, `fn
  run_d_server` `cli.rs:906` with its `DServer::bind` call `cli.rs:914` and `.with_identity`
  call `cli.rs:916`, and the `--bind` parse in `cmd_d_server` at `cli.rs:463`. Test peer:
  `crates/server/tests/failure_domain_registration.rs:36-53`.
- **Prior-art check (triage cycles):** Searched `crates/server/src/dserver.rs` and
  `cmd_d_server` history by path: #449 added the etcd `Coordination` backend and
  `resolve_coordination_backend`; #141/proposal 0005 added the failure-domain registration
  record. The advertise gap is marked known-deferred in-code ("split-horizon advertisement
  is a later deployment concern", `dserver.rs:184`) — no prior attempt to add an advertise
  flag. No open/closed/merged PR for #458. Not a duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
