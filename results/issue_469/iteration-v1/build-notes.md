# Build notes — issue 469 / fdb-deploy-profiles

## What I built (write-set, disjoint from every other bundle in the batch)

- `deploy/fdb-multi-replica/docker-compose.yml` (new) — a 3-process FoundationDB cluster
  (`fdb0..fdb2`, `double` redundancy), bridge network + one netns per node (static IPs
  `172.30.58.11..13`), plus the fault sidecar reused **as-is** from
  `deploy/tikv-multi-replica/iptables-agent/` (build context points at that dir — the
  Dockerfile is not copied). The sidecar is behind the `fault` compose profile so a plain
  `up` never starts it (its ENTRYPOINT is `["iptables"]`; a no-arg run errors); it is run
  on demand in a target node's netns.
- `deploy/small-multi-node-fdb/docker-compose.yml` (new) — the identical role topology to
  `deploy/small-multi-node/` (3 etcd + 9 D servers + 3 custodians + 3 S3 gateways) with the
  PD+TiKV tier replaced by a 3-process FDB cluster; every wyrd role runs
  `--metadata-backend fdb`, built from #470's `wyrd:fdb` image
  (`deploy/docker/wyrd/Dockerfile`, `--features fdb,etcd`).
- `deploy/README.md` (modified) — a profile matrix naming all six profiles (TiKV/FDB ×
  single-node/multi-replica/small-multi-node), the FDB single-node/multi-replica/single-zone
  sections (the repo had **zero** FDB deploy docs before), which single-zone setup is
  **currently canonical** (TiKV until #442 records go), and the deferred-rename **pairing**
  statement: `small-multi-node/` *is the TiKV peer of* `small-multi-node-fdb/`.
- `xtask/tests/fdb_deploy_profiles.rs` (new) — the binding guard.

I did **not** touch `xtask/src/main.rs`, `xtask/tests/deploy_no_orchestrator_coupling.rs`,
or the root `README.md`; I did not `git mv` `deploy/small-multi-node/`. Recorded decisions
1 and 2 (deferred rename, no bring-up arm) are honoured — that is what keeps this write-set
disjoint from 439 and `Conflicts with` dropped.

Cited peer patterns (on `origin/main` @ `84a9afb`, = the worktree base):
`deploy/tikv-multi-replica/docker-compose.yml:1-35` (bridge/netns iteration-13 fix),
`deploy/tikv-multi-replica/iptables-agent/Dockerfile:12-15` (generic `iptables` image),
`deploy/small-multi-node/docker-compose.yml:46-433` (role topology mirrored),
`crates/server/src/cli.rs:119-181` (`--metadata-backend fdb` resolves + `open_fdb_meta`
reads `WYRD_FDB_CLUSTER_FILE`, default `/etc/foundationdb/fdb.cluster`),
`xtask/tests/deploy_no_orchestrator_coupling.rs:133-220` (docker-availability convention +
compose-config structural check paralleled), `xtask/tests/readme_dev_section.rs:11-22`
(local `workspace_root`/`read` helpers, no `xtask` lib import — so no `xtask/src/` change).

## Why this shape

- **Two directories, not a compose override** — the metadata tier swap (PD+TiKV → FDB) is
  a whole service-set change, and the existing per-backend convention already uses one
  directory per backend profile. This is the brief's Scope (b) instruction.
- **The pairing is discharged in prose (README), not path names** — the rename is deferred
  (Recorded decision 1), so the README matrix is where "two clearly-named peer setups"
  lives. The success criterion's assertion 1 pins exactly that.
- **Sidecar behind a `fault` profile** — `deploy/tikv-multi-replica/` runs its agent as a
  standalone `docker run --network container:<node>`, not a compose service, because the
  fault must be injected *inside a chosen node's* netns. I keep that on-demand model but
  additionally *declare* the sidecar as a profiled service so (i) the brief's assertion 2
  "declares … the fault sidecar" is met, (ii) `docker compose --profile fault build`
  produces `wyrd-iptables:local`, and (iii) a plain `up` still brings up only the 3
  fdbservers. `docker compose config` hides profile-gated services unless the profile is
  active, so the test calls `docker compose --profile fault … config`.

## Live evidence — fdb-multi-replica brought fully up, partitioned, healed (run here)

Docker + compose v5.2.0 + `fdbcli` 7.3.77 all present (`docker info` OK).

```
$ docker compose -f deploy/fdb-multi-replica/docker-compose.yml up -d      # 3 processes
$ … exec fdb0 fdbcli --exec "configure new double ssd"   -> Database created
$ … exec fdb0 fdbcli --exec "status minimal"             -> The database is available.
$ … exec fdb0 fdbcli --exec "status"  (excerpt):
    Redundancy mode        - double
    Coordinators           - 3
    FoundationDB processes - 3
    Machines               - 3
    Fault Tolerance        - 1 machines
$ docker compose --profile fault … build iptables-agent  -> wyrd-iptables:local Built

# partition fdb2 (172.30.58.13) from fdb0/fdb1, in fdb2's netns via the sidecar:
$ docker run --rm --privileged --network container:<fdb2> wyrd-iptables:local \
      -A INPUT  -s 172.30.58.11 -j DROP     (and -s .12; and -A OUTPUT -d .11/.12)
  status (excerpt):
    172.30.58.13:4500  (unreachable)
    FoundationDB processes - 2
    Machines               - 2
    Fault Tolerance        - 0 machines           <-- partition materialised

# heal by replaying the same rules with -D:
  status (excerpt):
    FoundationDB processes - 3
    Machines               - 3
    Fault Tolerance        - 1 machines           <-- link healed
```

The sidecar genuinely partitions and heals a named link, bidirectionally (the cut is
inside the target's netns, so every packet the node sends/receives carries its own IP — the
iteration-13 requirement). The cluster stayed available throughout (double tolerated the
loss). Stack torn down with `down -v`.

## Refute-my-own-test (forced)

- **(a) Genuine red?** YES. I moved both new `deploy/` dirs aside and `git checkout`-ed
  `deploy/README.md` back to `origin/main` (test file left in place) and re-ran: **all 7
  tests failed** (compose files "no such file or directory"; README missing the matrix /
  canonical / pairing strings). Restored the fix → all 7 green again.
- **(b) Production path?** YES. The test drives the **shipped artifacts themselves** — it
  reads the real `deploy/*/docker-compose.yml` and `deploy/README.md` that the patch adds,
  and invokes the real `docker compose config` parser over them. There is no mock, copy, or
  re-implementation: the thing under test *is* the deliverable.
- **(c) Fixture includes the fault?** YES. The fault element is the partition sidecar; the
  `fdb-multi-replica` compose-config test asserts the sidecar **is declared** (via
  `--profile fault`), not curated out — and the live leg above injected a **real**
  partition (fault tolerance dropped to 0) rather than asserting over a healthy fixture.

## Pre-fix/post-fix runner

Run through the target's own cargo in `$PDCA_WORKTREE`: `cargo test -p xtask --test
fdb_deploy_profiles` — 7 passed post-fix, 7 failed with the fix reverted. The unit is
import-light (no `xtask` lib import; only spawns `docker compose config`, which is present
and headless — it parses, starts nothing), so a headless runner is safe. `cargo fmt -p
xtask -- --check` clean and `cargo clippy -p xtask --tests` clean (commit-hook readiness for
`cargo xtask ci`'s fmt/clippy gates). `deploy_no_orchestrator_coupling.rs` is byte-unchanged
and still green (6/6). Patch verified to `git apply --check` cleanly on a fresh `origin/main`
worktree (the C4-verify posture).

I did not run the full `cargo xtask ci` (multi-GB build + DST property suite) — that is
Check's gate and re-runs the real suite; my change adds only non-crate `deploy/` files
(not scanned by the ADR-0010 deploy-guard, which scans `crates/`) and one fmt/clippy-clean,
passing test file, so `C4-ci` is unaffected.

## External dependency not delivered by the wave fold — declared, not worked around

#470's `deploy/docker/wyrd/Dockerfile` and `wyrd:fdb` image are **absent** from the
worktree (`deploy/docker/` does not exist). Per the brief's contingency ("If it is absent
when Do runs, the fold has failed — stop and say so; do not fall back to the test
Dockerfile") I did **not** substitute
`crates/chunkstore-grpc/tests/dserver/Dockerfile` (it installs no FoundationDB client, so
`foundationdb-sys`' `bindgen` build script cannot link `libfdb_c`). `small-multi-node-fdb`
references the correct #470 path/tag so it is right when #470 lands.

This does **not** block the binding criterion or the live leg:
- Binding assertion 2 uses `docker compose config`, which only *parses* — I verified it does
  **not** require the `build.dockerfile` to exist, so the small-multi-node-fdb config check
  passes without #470.
- The live `fdb-multi-replica` leg uses **no** wyrd image (pure FoundationDB + the reused
  iptables sidecar), so it ran fully here.

It blocks only the **already-deferred, maintainer-confirmed** full bring-up of
`small-multi-node-fdb` (21 containers), which the brief declares off-Check with a named
human confirmer.

NEEDS-HUMAN external dependency: #470's `deploy/docker/wyrd/Dockerfile` + `wyrd:fdb` image (absent from the worktree — the wave fold did not deliver #470) — blocks the deferred full bring-up of `deploy/small-multi-node-fdb/` (21 containers) and the "an S3 gateway answers with --metadata-backend fdb" smoke bar; the binding criterion (assertions 1–3) and the live `fdb-multi-replica` leg are unaffected and were exercised here.

## Alternatives ruled out

- **Sidecar as an always-on service (no profile)** — rejected: a plain `up` would run
  `iptables` with no args, which errors and (with `restart:`) loops; and `network_mode:
  container:<one-node>` would pin the fault to a single node, whereas #442 must partition
  any of the three. Cost of the profile approach: one `--profile fault` on the test's
  config call (1 line) vs. a broken `up`.
- **Hostname-based FDB cluster file** (`docker:docker@fdb0:4500,…`) to avoid static IPs —
  rejected in favour of static bridge IPs, matching the proven `tikv-multi-replica`
  topology and the live-tested `fdb-multi-replica`; hostname coordinators add an untested
  resolution path for no benefit here.
- **Extending `deploy_no_orchestrator_coupling.rs`** — forbidden by the brief (a
  modify-in-place patch yields no `ADDED_TEST`, so `C4-verify` takes the green-only branch
  and never demonstrates a per-fix RED). New file per the brief's Test-file section.
