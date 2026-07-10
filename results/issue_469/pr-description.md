# Add FoundationDB deploy recipes at all three fixture tiers

## Summary
**User impact:** An operator standing Wyrd up on FoundationDB — the backend
Wyrd targets for production metadata — could only bring it up as a
throwaway single-node testbed. There was no ready-made way to run it as a
replicated cluster or as the full single-zone stack, the two shapes that
are meant to prove it is production-ready, so those setups had to be
hand-assembled. TiKV, the fallback backend, shipped a recipe for every
shape; the single-zone recipe was even wired to TiKV and read as the one
"real" deployment, quietly implying TiKV was the production track.

This PR adds the two missing FoundationDB recipes — a replicated
multi-process cluster with fault injection, and a full single-zone stack —
so FoundationDB has an out-of-the-box deployment at every tier TiKV
already did, and documents the two single-zone stacks as a named pair.

(Tracked as issue #469; this repo does not publish issue-URL links, so the
reference is the `Fixes #469` trailer below.)

## What to look at
Two new `docker-compose.yml` files under `deploy/`, plus a section in
`deploy/README.md`:

- `deploy/fdb-multi-replica/` — three FoundationDB processes plus a fault
  sidecar. Try it: `docker compose -f deploy/fdb-multi-replica/docker-compose.yml up -d`,
  configure once with `fdbcli --exec "configure new double ssd"`, then
  `fdbcli --exec "status minimal"` reports the database available; run the
  sidecar in a node's namespace to partition it and watch fault tolerance
  drop, then heal.
- `deploy/small-multi-node-fdb/` — the full single-zone stack (etcd + 9 D
  servers + 3 custodians + 3 gateways) on FoundationDB. The README spells
  out the manual `docker compose up` bring-up.
- `deploy/README.md` — the profile matrix and the statement that
  `small-multi-node/` is the TiKV peer of `small-multi-node-fdb/`.

The whole change lives under `deploy/` plus one new test file; no
production crate or existing deploy recipe is modified.

## Root cause
FoundationDB had a recipe only at the single-node tier, so the replicated
cluster the fault battery needs and the single-zone stack the
first-deployment gate needs simply did not exist. The single-zone recipe
`deploy/small-multi-node/` was TiKV-wired and unpaired, so its unqualified
name and its backend read as the canonical production choice.

## Fix
Add `deploy/fdb-multi-replica/docker-compose.yml` (a three-process
`double`-redundancy cluster on a bridge network with one netns per node,
reusing the TiKV recipe's generic `iptables` fault sidecar as-is) and
`deploy/small-multi-node-fdb/docker-compose.yml` (the same role topology as
`deploy/small-multi-node/`, metadata tier swapped to FoundationDB, roles
running the `wyrd:fdb` feature image). Extend `deploy/README.md` with a
six-recipe profile matrix, the which-suite-drives-which-tier mapping, the
current-canonical statement, and the explicit `small-multi-node/` ⇄
`small-multi-node-fdb/` pairing so neither name reads as "the" stack. The
rename of `small-multi-node/` → `small-multi-node-tikv/` is deferred to a
follow-up to avoid churning landed paths; the pairing is recorded in prose.

## Verification
- **Claim:** FoundationDB has a recipe at all three fixture tiers, the
  single-zone stack wires every metadata role to FoundationDB and none to
  TiKV, and the README documents the matrix and the single-zone pairing.
  - **Checked:** `deploy/small-multi-node-fdb/docker-compose.yml` — the 3
    custodians (`custodian0..2`) and 3 gateways (`gateway0..2`) all pass
    `--metadata-backend fdb`; the 9 D servers pass none, mirroring the TiKV
    peer `deploy/small-multi-node/docker-compose.yml:360,393` (which passes
    `--metadata-backend tikv`) on the target branch.
  - **Checked:** `deploy/README.md` — profile matrix names all six recipes,
    states which single-zone stack is currently canonical, and records that
    `small-multi-node/` is the TiKV peer of `small-multi-node-fdb/`. On the
    target branch `deploy/README.md` had no FoundationDB deploy docs at all.
  - **Checked:** the roles' `--metadata-backend fdb` is a real, resolvable
    selection — `crates/server/src/cli.rs:120` maps `"fdb"` to the
    FoundationDB store and `:175` opens it — so the recipe is not
    speculative.
- **Claim:** the replicated cluster is genuinely replicated and its fault
  sidecar can partition and heal it.
  - **Checked:** `deploy/fdb-multi-replica/docker-compose.yml` declares
    three `fdbserver` processes plus the `fault`-profiled sidecar. Brought
    up here: `configure new double ssd` → `status` shows redundancy
    `double`, 3 coordinators, fault tolerance 1; partitioning `fdb2` from
    `fdb0`/`fdb1` inside `fdb2`'s namespace dropped it to fault tolerance 0,
    and replaying the rules with `-D` restored it.
- **Test:** `xtask/tests/fdb_deploy_profiles.rs` (new) — fails pre-fix (the
  two recipes do not exist and the README lacks the matrix), passes
  post-fix. It pins both recipes exist, that every single-zone metadata
  role names `fdb` and none names `tikv` (flipping one role to `tikv` turns
  it red), and — where Docker is present — parses each stack with
  `docker compose config` and checks the declared roles. `cargo xtask ci`
  passes.
- **Maintainer-confirmed leg:** the full 21-container `small-multi-node-fdb`
  bring-up and an end-to-end S3 gateway response are exercised by hand
  (command in `deploy/README.md`); this PR builds the `wyrd:fdb` image and
  parses/role-checks every service, and confirms the built binary accepts
  `--metadata-backend fdb`. As with the TiKV peer, an object does not yet
  round-trip through the gateways — the stack is a topology bring-up target.

Fixes #469
