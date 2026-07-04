# Build notes — issue #256 / m4.5-deploy-tikv-pd-etcd

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend`, base commit
`225d3bd` (tip at Do time). Built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt`), on a local branch `feat/m4.5-deploy-tikv-pd-etcd`
off that tip. Planning artifact read in full: proposal 0015
(`docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`)
§"Deployment" (`0015:423-463`), §"Crate touch-points" (`0015:557-596`), §"Suggested PR
sequence" item 5 (`0015:701-712`), plus ADR-0010, ADR-0006, architecture §7.1/§7.2/§7.5
(`docs/design/architecture/07-deployment-view.md:13-88`), and the M4 first-deployment
blueprint (`docs/design/architecture/m4-first-deployment-blueprint.md`).

## What was built

1. **`deploy/small-multi-node/docker-compose.yml`** (new, 250 lines) — the single-zone
   "Small multi-node Production" bring-up: a 3-node PD ensemble (`pd0`/`pd1`/`pd2`), one
   TiKV-small store (`tikv`), a *separate* 3-node etcd ensemble for L5 Coordination
   (`etcd0`/`etcd1`/`etcd2`, ADR-0006), and three local-disk D servers (`dserver0..2`)
   reusing the `wyrd-dserver:local` image + `crates/chunkstore-grpc/tests/dserver/Dockerfile`
   the root `docker-compose.yml:24-67` already builds. Lives under `deploy/`, outside the
   Cargo workspace (ADR-0010, `docs/design/adr/0010-pluggable-deployment-substrate.md:18-20`).
2. **`xtask/src/deploy_guard.rs`** (new lib module, 89 lines) — the ADR-0010
   "no code couples to orchestrator APIs" scan: `scan_line`/`collect_rs_files`/`scan_dir`,
   mirroring the shape of the existing ADR-0035 statics gate
   (`xtask/src/main.rs:590-673` pre-patch, `statics_scan_line`/`statics_collect_rs`/`run_statics`).
   Exposed via `xtask/src/lib.rs:16` (`pub mod deploy_guard;`, alongside the pre-existing
   `pub mod disk_faults;` at `xtask/src/lib.rs:16` pre-patch).
3. **`xtask/src/main.rs`** — wired `run_orchestrator_guard()` into `run_ci()` right after
   `run_statics()?;` (pre-patch `xtask/src/main.rs:794`), and added the
   `deploy-small-multi-node` subcommand (`run_deploy_small_multi_node`, mirroring
   `run_tikv_conformance` at pre-patch `xtask/src/main.rs:161-185`: `docker_available()`
   gate, hard-fail in CI / warn-skip locally, bring up → wait for every port → always tear
   down).
4. **`deploy/README.md`** — a new `## small-multi-node/` section, mirroring the existing
   `## tikv-single-node/` section's shape (pre-patch `deploy/README.md:8-29`).
5. **`xtask/tests/deploy_no_orchestrator_coupling.rs`** (new, 204 lines) — the two
   Check-time signals the brief names, both here (see "Test evidence" below).

## Why this shape (and what else was on the table)

**Bridge network + distinct published host ports, not host networking (rejected).**
`deploy/tikv-single-node/docker-compose.yml:16,27` uses `network_mode: host` because it
has exactly one `pd` and one `tikv` — a client on the host reaches both directly at their
well-known ports. The new stack needs **three** replicas each of `pd` and `etcd`, all
defaulting to client port 2379 internally
(`docs/design/architecture/m4-first-deployment-blueprint.md:109`: "note both default to
client port 2379, so remap one when co-located"). Host networking gives every container
the SAME network namespace as the host, so three containers cannot each bind `2379` on
it — there is no way to make host networking work for same-port replicas without an
in-container port remap that would make the images diverge from their upstream defaults.
Bridge networking (compose's default) with per-service DNS names (`etcd0`, `pd1`, …) for
inter-service traffic and distinct **host**-published ports for the eval runner's
readiness probe is the standard, idiomatic answer, and it is also the shape the blueprint
itself describes for the real deployment (a private network/vSwitch,
`m4-first-deployment-blueprint.md:194-199`). Cost of the alternative (forcing host
networking): would require an in-container config to remap PD1/PD2/etcd1/etcd2's listen
ports away from their images' documented `2379`/`2380` defaults — three extra `--client-urls`
overrides per extra replica, still colliding on `20160`/`20180` for TiKV replicas if ever
scaled, and diverging from every public PD/etcd doc example a future maintainer would
compare against. Not adopted.

**A fresh `deploy/small-multi-node/docker-compose.yml`, not an edit to the repo-root
`docker-compose.yml` (explicit Scope constraint).** The root file
(`docker-compose.yml:1-75`) is the D-server-only dev/demo stack (M2.8, issue #155);
editing it to add TiKV/PD/etcd would conflate a hand-driven dev convenience with a
production-topology CI/eval fixture and contradict the brief's Scope line verbatim
("it is **not** an edit to the repo-root `docker-compose.yml`"). Not adopted.

**The orchestrator-coupling guard as a named `cargo xtask` step wired into `run_ci`, not
only a test-local assertion (chosen; cost shown for the rejected alternative).** The
simplest possible diff would drop `xtask/src/deploy_guard.rs` and `xtask/src/lib.rs`'s new
line entirely, and instead inline the scan/needle list as a private helper directly inside
`xtask/tests/deploy_no_orchestrator_coupling.rs` — since that test file already runs as
part of `cargo test --workspace` (itself part of `cargo xtask ci`,
`xtask/src/main.rs:790` pre-patch), the invariant would still be enforced on every CI run.
That saves the ~28-line `run_orchestrator_guard` function and the one-line `run_ci` wiring
(`xtask/src/main.rs:794` post-patch) — roughly 30 lines smaller. I did not take it, because
the project's own precedent for exactly this shape of check — ADR-0035's DST
global-mutable-state gate — is a **named, independently-invocable** `run_statics` step
(`xtask/src/main.rs:727-766` pre-patch) with its own labeled `print_step` output and its
own place in `run_ci`'s step list, not a bare test assertion; a maintainer runs
`cargo xtask ci` and sees a labeled `xtask deploy-guard (ADR-0010 no-orchestrator-coupling
gate)` line (confirmed in the local `cargo xtask ci` run below) exactly as they see
`xtask statics (ADR-0035 DST global-state gate)` today — one enforcement style across the
two structural ADR guards the project has, per ADR-0016 ("the same checks run … CI logic
lives in Rust (xtask), not YAML"). The cost of the alternative was cheap (≈30 fewer
lines), but it would fork the project's established pattern for a materially similar
guard for no real savings, and would make the invariant harder to invoke/diagnose in
isolation (no `cargo xtask deploy-guard`-shaped named failure, just "some test in the
workspace failed"). The library-module split (`xtask::deploy_guard` used from both
`main.rs` and the test) also means the test drives the *exact* function `cargo xtask ci`
runs, not a re-implementation — the same "one guard, two call sites" property
`xtask/src/disk_faults.rs` + `xtask/tests/disk_faults_orchestration.rs` already establish
for the Tier-1 fault harness (though that pair's second call site
(`crates/custodian/tests/tier1_disk_faults.rs:235`) explicitly INLINES the logic instead of
depending on `xtask` as a crate — "inlined from `xtask::disk_faults` to avoid cross-crate
dep" — so my `run_orchestrator_guard` calling `xtask::deploy_guard::scan_dir` directly from
`main.rs` is a *tighter* single-source than that precedent, made possible because both
call sites here already live in the same package).

**A lightweight grep-style scan (`ORCHESTRATOR_NEEDLES` substring match on non-comment
lines), not a `syn`-based AST parse of every `use` item (rejected).** ADR-0035's own gate
states its philosophy directly: "a lightweight grep-style gate … not a full reachability
analysis" (`xtask/src/main.rs:722-724` pre-patch, comment above `run_statics`). I mirrored
that stated design choice rather than reaching for a real parser. Cost of the rejected
alternative: `xtask/Cargo.toml:11-14` would need a new `syn` (+ `proc-macro2`) dependency
declared directly (today `xtask` only depends on `wyrd-chunk-format`, `serde`,
`serde_json`), and the scan logic would grow from the ~35 lines of `scan_line`/`scan_dir`
to parsing every file into a `syn::File`, walking every `Item::Use`, and resolving path
segments — on the order of 80-120 additional lines for a benefit that does not materialize
in practice: the guard only scans `.rs` files (never `deploy/README.md`'s own "Kubernetes
is available, never required" prose), and `scan_line_ignores_comments_and_prose` /
`orchestrator_needles_are_non_empty_and_import_shaped` in the test file directly pin down
that the substring approach does not false-positive on comments or bare prose. Not
adopted.

**D-server count: 3, not 9 (a CI/eval sizing choice, not a durability claim).** The
blueprint's RS(6,3)-at-full-strength sizing wants 9 D servers
(`m4-first-deployment-blueprint.md:25-45`), but that is the *production durability*
sizing question, which this CI/eval fixture is not answering — the Success criterion only
requires "local-disk D servers" (plural) exist and the compose file parses; it does not
require a specific count or an RS(6,3) placement proof. Three keeps the fixture light for
CI/eval (mirrors `deploy/tikv-single-node/`'s own "throwaway … for CI" framing) while still
demonstrating multiple, independently-labeled failure domains (`--failure-domain fd0/fd1/fd2`).

**Did not attempt a real, live `docker compose up` of the full stack during Do (a genuine
scope boundary, not an oversight).** I started `cargo xtask deploy-small-multi-node` once
to sanity-check the command's shape, saw it begin pulling the etcd/PD/TiKV images (a slow,
network-bound, multi-image operation exactly like the *deferred* bucket the brief's
Verification posture describes), and immediately tore it down
(`docker compose -p wyrd-small-multi-node-m45 … down -v`) rather than let a Do-time
sanity pass balloon into the actual off-Check bring-up. The brief is explicit that the
live stack-boots-and-peers-discover-through-L5 DoD is **deferred** (needs a Docker host +
the etcd-`Coordination` + gateway/custodian prerequisite, tracked as #365 + an untracked
item) — so I only built and exercised the two Check-time signals the brief names (the
guard + the compose-config validity check), and left the actual bring-up as a real,
authored, but off-Check runner, exactly as `run_tikv_conformance` already is for the M4.1
slice. A human/operator can validate the live bring-up with:
```sh
cargo xtask deploy-small-multi-node   # brings the stack up, waits for every port, tears down
# or by hand:
docker compose -f deploy/small-multi-node/docker-compose.yml up -d
docker compose -f deploy/small-multi-node/docker-compose.yml ps
docker compose -f deploy/small-multi-node/docker-compose.yml down -v
```

## Test evidence (red → green)

Ran through the project's own runner (`cargo xtask ci`, per
`docs/INTEGRATION.md`'s "Verification runner" row — `engine/xtask.sh` delegates to it
verbatim), not a hand-rolled invocation.

- **Pre-fix RED (a demonstrated compile failure, not "resting red on non-existence"):**
  moved `xtask/src/deploy_guard.rs` and `deploy/small-multi-node/` aside and reverted
  `xtask/src/lib.rs`, keeping only the new test file. `cargo test -p xtask --test
  deploy_no_orchestrator_coupling` fails to compile:
  `error[E0432]: unresolved import 'xtask::deploy_guard'` — the test cannot build without
  the production module, exactly the same discriminator
  `xtask/tests/disk_faults_orchestration.rs:26-27`'s own doc comment describes for its
  sibling seam.
- **Post-fix GREEN:** restored the three files; `cargo test -p xtask --test
  deploy_no_orchestrator_coupling` → `6 passed; 0 failed`. Also verified by applying
  `patch.diff` to a **clean worktree at the base commit** (`git worktree add … 225d3bd`,
  `git apply --check` + `git apply`, then the same test command) — same 6/6 pass, proving
  the patch is self-contained and applies cleanly against the target branch tip.
- **Full-suite green:** `cargo xtask ci` (fmt, clippy, build, `cargo test --workspace
  --exclude wyrd-dst` — which includes the new test — cargo-machete, cargo-deny,
  conformance, the new `deploy-guard` step, DST) → `xtask ci: all checks passed`,
  including the labeled `xtask deploy-guard (ADR-0010 no-orchestrator-coupling gate)` /
  `xtask deploy-guard: no workspace crate imports an orchestrator API (ADR-0010)` lines.
- **Formatting:** `cargo fmt -p xtask` run over every touched file;
  `cargo fmt -p xtask -- --check` clean. No pre-commit-hook framework exists in this repo
  beyond `cargo xtask ci` itself (`CONTRIBUTING.md`: "Before opening a pull request, run:
  `cargo xtask ci`" — no `.pre-commit-config.yaml`/husky found), so this is the full
  commit-readiness bar.

## Scope discipline

No file under `crates/` changed (confirmed by `git diff --cached --stat` against the base
— only `deploy/`, `xtask/`, and `deploy/README.md` touched), matching the brief's
"out of scope" list verbatim (no change to `crates/`, `traits`, the on-disk format, or the
`MetadataStore` contract). `xtask/tests/deploy_no_orchestrator_coupling.rs`'s own
`scan_dir_is_green_over_the_real_workspace_crates` test additionally *proves* this as a
runtime assertion, not just a diff-stat claim.
