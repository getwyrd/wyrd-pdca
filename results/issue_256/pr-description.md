# deploy: add the small multi-node production bring-up stack

## Summary
**User impact:** An operator who wants to stand up the single-zone "Small
multi-node Production" tier has no recipe to run. The only thing under `deploy/`
is a throwaway single-node TiKV pair used by the conformance suite — nothing
composes the actual production topology (TiKV plus its coordinator, an L5
coordination store, and disk-backed storage servers), so bringing the tier up
means assembling it by hand from the architecture docs.

This PR adds `deploy/small-multi-node/` — a docker-compose stack that composes
the profile's four component roles — plus a `cargo xtask deploy-small-multi-node`
runner that brings it up, waits for it, and tears it down.

## What to look at
- **`deploy/small-multi-node/docker-compose.yml`** — the new stack: a 3-node PD
  ensemble (`pd0`–`pd2`), one `TiKV-small` store (`tikv`), a *separate* 3-node
  etcd ensemble for L5 Coordination (`etcd0`–`etcd2`), and three local-disk
  D servers (`dserver0`–`dserver2`). It uses the default bridge network with
  per-service DNS names and distinct published host ports (the single-node stack
  used host networking, which cannot give three same-port replicas distinct
  addresses). The D servers reuse the existing `wyrd-dserver:local` image and the
  `wyrd d-server` role — this is a fresh stack, not an edit to the repo-root dev
  `docker-compose.yml`.
- **`xtask/src/main.rs` → `run_deploy_small_multi_node`** — the bring-up runner,
  docker-gated exactly like `tikv-conformance` (hard failure in CI, warn-and-skip
  locally). It is **not** part of `cargo xtask ci`.
- **`xtask/src/deploy_guard.rs` + `run_orchestrator_guard` in `xtask/src/main.rs`**
  — a new `cargo xtask ci` step (`deploy-guard`) that keeps orchestrator coupling
  out of the crates.
- **Exercise it** (needs Docker): `docker compose -f
  deploy/small-multi-node/docker-compose.yml config` to validate the topology,
  then `cargo xtask deploy-small-multi-node` on a Docker host to bring it up and
  tear it down.

## Root cause
The deployment tier was only ever built as far as its first slice needed: a
single-node fixture for the conformance suite. The multi-replica production
topology (a PD ensemble, a dedicated etcd ensemble for coordination, and several
disk-backed storage servers) had no composed artifact and no runner, so there was
nothing to stand up and nothing to keep it structurally honest.

## Fix
Add `deploy/small-multi-node/docker-compose.yml` composing the four roles, a
`deploy/README.md` section documenting how to run it, and a docker-gated
`cargo xtask deploy-small-multi-node` runner (mirroring the existing
`tikv-conformance` pattern). To preserve the "substrate stays pluggable"
invariant — the stack lives outside the Cargo workspace and no crate may couple
to an orchestrator API — a new `deploy-guard` step in `cargo xtask ci` scans
every `.rs` file under `crates/` and fails the build if a Kubernetes/orchestrator
client import appears.

The stack stands up on **static endpoints** today. Live peer discovery through
L5 depends on an etcd-backed Coordination backend and runnable gateway/custodian
process roles that land in separate work (tracked as #365, plus the
gateway/custodian roles), so that behaviour is deliberately out of scope here and
is not claimed by this PR.

## Verification
- **Claim:** the stack composes the profile's four component roles.
  - **Checked:** `deploy/small-multi-node/docker-compose.yml:31` (`services:`),
    `:35` (`etcd0`, L5 coordination ensemble), `:97` (`pd0`, PD ensemble), `:147`
    (`tikv`, TiKV-small), `:174` (`dserver0`, local-disk D servers) — all ten
    services declared with non-colliding published host ports.
  - **Test:** `xtask/tests/deploy_no_orchestrator_coupling.rs:147`
    (`small_multi_node_compose_config_is_structurally_valid`) — asserts
    `docker compose config` parses the stack and that all four roles and their
    pinned images are present (docker-gated: hard failure in CI, skip locally).

- **Claim:** no workspace crate couples to an orchestrator/k8s API, and the guard
  is load-bearing (not resting green on the absence of any such code).
  - **Checked:** `xtask/src/deploy_guard.rs:28` (the import needles) and `:73`
    (`scan_dir`), wired into the CI gate at `xtask/src/main.rs:773`
    (`run_orchestrator_guard`) and `:822` (invoked inside `run_ci`), exported for
    reuse at `xtask/src/lib.rs:16`.
  - **Test:** `xtask/tests/deploy_no_orchestrator_coupling.rs:66`
    (`scan_dir_is_red_when_an_orchestrator_import_is_planted`) plants a real
    `use kube::Client;` import in a temp fixture and asserts the *same* scan the
    CI gate runs catches it exactly once — fails without the guard, passes with
    it; `:101` (`scan_dir_is_green_over_the_real_workspace_crates`) asserts the
    invariant holds over the current tree.

- **Claim:** the bring-up is real and the change stays within `deploy/` + `xtask/`.
  - **Checked:** `run_deploy_small_multi_node` at `xtask/src/main.rs:285` brings
    the stack up, waits for the external components to accept connections, and
    always tears it down; no file under `crates/` is touched.
  - **Test:** `cargo xtask ci` (fmt, clippy, build, `cargo test --workspace`
    including the new test, cargo-deny, conformance, the new `deploy-guard` step)
    passes; the new test is red pre-fix (the test cannot compile without the
    guard module) and green post-fix, confirmed by applying the change to a clean
    checkout at the target branch base.

Note: the live bring-up on a Docker host and L5 peer discovery are deferred (they
need a container host plus the coordination backend above) and are confirmed
off-PR by an operator / CI-eval run, ultimately the first-deployment gate (#367).

Fixes #256
