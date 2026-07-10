# Build notes — issue 469 / fdb-deploy-profiles (iteration 2)

## Context: what changed since iteration 1

Iteration 1 was sound but rejected at sign-off for two actionable reasons, both now
addressed. Critically, **#470 is merged** (PR #497, worktree base `6d3d1db`), so
`deploy/docker/wyrd/Dockerfile` and a buildable `wyrd:fdb` image now exist in the target
tree — the hard-precondition blocker iteration 1 declared NEEDS-HUMAN is cleared. This
iteration rebuilds against that merged base and addresses both reviewer items.

## Write-set (disjoint from every other bundle in the batch)

- `deploy/fdb-multi-replica/docker-compose.yml` (new) — 3-process FDB cluster
  (`fdb0..fdb2`, `double` redundancy), bridge network + one netns per node (static IPs
  `172.30.58.11..13`), plus the fault sidecar reused **as-is** from
  `deploy/tikv-multi-replica/iptables-agent/` (build context points at that dir; the
  Dockerfile is not copied). Sidecar behind the `fault` compose profile so a plain `up`
  never starts it.
- `deploy/small-multi-node-fdb/docker-compose.yml` (new) — identical role topology to
  `deploy/small-multi-node/` (3 etcd + 9 D servers + 3 custodians + 3 S3 gateways) with
  the PD+TiKV tier replaced by a 3-process FDB cluster; the 3 custodians and 3 gateways run
  `--metadata-backend fdb`, built from #470's `wyrd:fdb` image
  (`deploy/docker/wyrd/Dockerfile`, `--features fdb,etcd`).
- `deploy/README.md` (modified) — profile matrix naming all six profiles, the FDB
  single-node/multi-replica/single-zone sections (the repo had **zero** FDB deploy docs
  before), which single-zone setup is **currently canonical**, and the deferred-rename
  **pairing** statement: `small-multi-node/` *is the TiKV peer of* `small-multi-node-fdb/`.
- `xtask/tests/fdb_deploy_profiles.rs` (new) — the binding guard, with the tightened
  backend assertion (below).

Untouched, per Recorded decisions 1 & 2: `xtask/src/main.rs`,
`xtask/tests/deploy_no_orchestrator_coupling.rs` (confirmed byte-unchanged — `git diff
--stat` empty and its 6 tests still green), root `README.md`; no `git mv` of
`deploy/small-multi-node/`. That keeps the write-set disjoint from 439 (no `Conflicts
with`, no C4-verify stale-patch risk).

Cited peer patterns (worktree base `6d3d1db` = `origin/main` post-#470/#497):
`deploy/tikv-multi-replica/docker-compose.yml:11-25` (bridge/netns iteration-13 fix),
`deploy/tikv-multi-replica/iptables-agent/Dockerfile:12-15` (generic `iptables` image),
`deploy/small-multi-node/docker-compose.yml:231-433` (role topology; :360 custodian /
:393 gateway open `--metadata-backend tikv`, d-servers open none),
`deploy/docker/wyrd/Dockerfile:36-66` (#470 build stage: FDB client install + feature
build), `crates/server/src/cli.rs:119-181` (`--metadata-backend fdb` resolves at :120,
`open_fdb_meta` reads `WYRD_FDB_CLUSTER_FILE` default `/etc/foundationdb/fdb.cluster`),
`xtask/tests/deploy_no_orchestrator_coupling.rs:133-220` (docker-availability convention +
compose-config structural check paralleled).

## Reviewer item 1 — tautological backend assertion tightened

Old (iter 1, `:948-951`): `merged.contains("--metadata-backend") && merged.contains("fdb")`
— vacuous, since "fdb" occurs dozens of times via image/service/volume names; it passes
even if all three gateways are flipped to `tikv`.

New (`fdb_single_zone_wires_every_metadata_role_to_fdb_and_none_to_tikv`, a
**pure-filesystem, unconditional** test — part of the binding RED): count the
`--metadata-backend <value>` PAIRS in the raw compose source (roles use the JSON-array
command form, so flag+value are one literal) and assert:
- `total = matches("\"--metadata-backend\"") >= 6` (3 custodians + 3 gateways, mirroring
  the TiKV peer; d-servers open none),
- `matches("\"--metadata-backend\", \"fdb\"") == total` (every role names fdb),
- `matches("\"--metadata-backend\", \"tikv\"") == 0` (no role names tikv).

Proven to bind against the reviewer's exact scenario: flipping one gateway to
`--metadata-backend tikv` fails with `every --metadata-backend … must name fdb; 1 of 6 do
not` (left 5, right 6). The Docker-gated `small_multi_node_fdb_compose_config_is_structurally_valid`
additionally asserts no `tikv` value survives in the *rendered* config, so a config-time
rewrite can't slip one past either.

Note on the reviewer's wording ("EVERY wyrd role (dservers, custodians, gateways) opens
`--metadata-backend fdb`"): d-servers deliberately open **no** metadata backend — the TiKV
peer's d-servers pass only `--coordination-backend etcd`
(`deploy/small-multi-node/docker-compose.yml:239`), never `--metadata-backend`. Mirroring
the peer exactly, the FDB stack's metadata roles are the 3 custodians + 3 gateways; the
assertion binds those six and forbids `tikv` everywhere, which is the reviewer's intent
(no role may silently open the wrong backend).

## Reviewer item 2 — exercised beyond parse-only, now that #470 supplies `wyrd:fdb`

**Build context resolves + the image builds (run here).**
`docker compose -f deploy/small-multi-node-fdb/docker-compose.yml config` renders
`build.context=<worktree root>`, `build.dockerfile=deploy/docker/wyrd/Dockerfile` (present
on disk), `args.FEATURES=fdb,etcd`. `docker compose … build dserver0` **built cleanly
(exit 0)**: the build stage ran `cargo build --release --locked --bin wyrd --features
fdb,etcd` (log: "Compiling wyrd-server … Finished `release` … in 20.93s"), the runtime
stage installed the pinned `foundationdb-clients_7.3.77`, and it tagged `wyrd:fdb`.
Verified the produced image is genuinely FDB-capable, not a default-feature stub:
- `ldd /usr/local/bin/wyrd` inside the image → `libfdb_c.so => /lib/libfdb_c.so` (linked).
- `docker run … wyrd:fdb custodian … --metadata-backend fdb …` → the binary **accepts**
  the fdb backend and starts the custodian ("leader for zone `z` … on the fdb backend"),
  i.e. it reaches the real fdb store path — NOT the `metadata backend 'fdb' requires
  building with --features fdb` rejection a feature-absent binary emits
  (`crates/server/src/cli.rs:122-124`).
- Image label `dev.wyrd.fdb.libfdb_c.version = 7.3.77`.

**Configured harness (`./engine/xtask.sh ci`, the C4-ci gate) run to green.** Ran the
project's own configured oracle in `$PDCA_WORKTREE` (not a scratch substitute): fmt
`--check`, clippy `-D warnings`, build, the whole test suite (incl. DST property tests),
`cargo deny`, and conformance. Final line "xtask ci: all checks passed", **exit 0** (a
clean run with no concurrent git ops; an earlier run legitimately failed fmt because I had
not yet run `cargo fmt` — fixed, re-run green). This is the configured-oracle evidence the
reviewer asked for.

## Live evidence — fdb-multi-replica brought up, partitioned, healed

(Carried from iteration 1; the stack is byte-identical and Docker + `fdbcli` 7.3.77 are
present here.) `up -d` (3 processes) → `configure new double ssd` → `status`: Redundancy
`double`, 3 coordinators, 3 processes, Fault Tolerance 1 machine. Sidecar built
(`wyrd-iptables:local`); partitioning fdb2 from fdb0/fdb1 inside fdb2's netns dropped the
cluster to 2 processes / Fault Tolerance 0 (partition materialised); replaying with `-D`
restored 3 / Fault Tolerance 1 (healed). The cut is bidirectional by construction (in the
target's netns, the iteration-13 requirement).

## Refute-my-own-test (forced, recorded)

- **(a) Genuine red?** YES. With both new `deploy/` dirs moved aside and `deploy/README.md`
  reset to base (test file kept in place), `cargo test -p xtask --test fdb_deploy_profiles`
  → **all 7 tests FAILED** (compose files "no such file or directory"; README missing the
  matrix/canonical/pairing strings). Restoring the fix → 7 green. Additionally, the tightened
  assertion specifically goes red when a single gateway is flipped to `tikv` (5 of 6 fdb),
  proving it is not vacuous.
- **(b) Production path?** YES. The test drives the **shipped artifacts themselves** — it
  reads the real `deploy/*/docker-compose.yml` and `deploy/README.md` the patch adds and
  runs the real `docker compose config` parser over them. No mock/copy/re-implementation;
  the thing under test *is* the deliverable. The image leg drives the real #470 Dockerfile
  and the real `wyrd:fdb` binary.
- **(c) Fixture includes the fault?** YES. The `fdb-multi-replica` compose-config test
  asserts the fault sidecar **is declared** (via `--profile fault`), not curated out; the
  live leg injected a **real** partition (Fault Tolerance → 0), not an assertion over a
  healthy fixture.

## Pre-fix/post-fix runner

`cargo test -p xtask --test fdb_deploy_profiles` in `$PDCA_WORKTREE`: 7 passed post-fix,
7 failed with the fix reverted. Import-light unit (no `xtask` lib import; only spawns
`docker compose config`, which parses and starts nothing — headless-safe).
`cargo fmt -p xtask -- --check` exit 0 (formatted) and `cargo clippy -p xtask --tests`
clean. `deploy_no_orchestrator_coupling.rs` byte-unchanged, its 6 tests green. Patch
`git apply --check`s cleanly on a fresh base reset to `6d3d1db` (= C4-verify's
`origin/main` posture). Full `./engine/xtask.sh ci` green (exit 0).

## Deferred, off-Check (unchanged from the brief's declaration)

The full `small-multi-node-fdb` 21-container bring-up and the "an S3 gateway answers with
`--metadata-backend fdb` end-to-end" smoke bar remain the maintainer-confirmed leg (brief
"Verification posture"). This iteration exercised the *image build* and the *per-service
role/config parse*, plus proved the binary is FDB-capable; the only thing not run is the
21-container orchestration itself. There is deliberately no `cargo xtask
deploy-small-multi-node-fdb` arm (Recorded decision 2 — it would put `xtask/src/main.rs` in
this write-set, which 439 also writes). `deploy/README.md` spells out the manual bring-up
command for the confirmer. Production reach is the same smoke bar as the TiKV peer (#454
standalone gateway, #455 no closed write path) — topology bring-up parity, which is what
this slice claims and what #442 needs.

## Alternatives ruled out

- **Sidecar as an always-on service (no profile)** — rejected: a plain `up` would run
  `iptables` with no args, which errors and (with `restart:`) loops; and
  `network_mode: container:<one-node>` would pin the fault to one node, whereas #442 must
  partition any of the three. Cost of the profile approach: one `--profile fault` on the
  test's config call (1 line) vs. a broken `up`.
- **Asserting only on the rendered `docker compose config` output for the backend check** —
  rejected as the *primary* guard: the config-config leg is Docker-gated and
  `small_multi_node_fdb_compose_config_is_structurally_valid` skips cleanly with no Docker
  (`:220`-style), so a backend assertion written only there could never be relied on to go
  RED at Check. The binding backend assertion is therefore pure-filesystem (unconditional);
  the rendered-config no-`tikv` check is an additional belt-and-braces leg.
- **Extending `deploy_no_orchestrator_coupling.rs`** — forbidden by the brief (a
  modify-in-place patch yields no `ADDED_TEST`, so `C4-verify` takes the green-only branch
  and never demonstrates a per-fix RED). New file per the brief's Test-file section.
