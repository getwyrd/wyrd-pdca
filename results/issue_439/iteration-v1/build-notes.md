# Build notes — issue 439 / fdb-dev-ci-harness

Target branch: `getwyrd/wyrd @ main` (`origin/main` = `b1ccca3`). All edits made in
`$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`. Line citations below are against
that worktree (= `origin/main` + this patch); pre-existing code is cited at its
`origin/main` line.

## What was built

Three deliverables, exactly the brief's (a)/(b)/(c), plus the pinned audit-policy note.

### (a) `xtask::fdb_doctor` — a pure library module + an `xtask fdb-doctor` arm

- New `xtask/src/fdb_doctor.rs` (285 lines), exported at `xtask/src/lib.rs:18` next to
  `deploy_guard` (`origin/main` `xtask/src/lib.rs:16`).
- `diagnose(Vec<(Probe, Outcome)>) -> Report` is **pure**: probe *results* are inputs.
  The three rows are the ones the issue names — `Probe::ClientLibrary`,
  `Probe::ClusterFile`, `Probe::ClusterHealth` (`fdb_doctor.rs:83-95`), each with a
  `remediation()` that names the concrete fix (`:104-131`):
  - missing `libfdb_c` → names `foundationdb-clients` + the pinned `7.3.77`;
  - unreadable cluster file → names `WYRD_FDB_CLUSTER_FILE`
    (`crates/metadata-fdb/src/lib.rs:386` on `origin/main`) and its
    `/etc/foundationdb/fdb.cluster` default (`:390`);
  - unhealthy cluster → names `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`.
- The **impure** half lives in `main.rs`: `probe_client_library` (`xtask/src/main.rs:396`),
  `probe_cluster_file` (`:417`), `probe_cluster_health` (`:433`). Exactly the
  `deploy_guard` split (pure `scan_line`/`scan_dir` in the lib, `run_orchestrator_guard`
  in `main.rs:1208` on `origin/main`), and it is what keeps the binding criterion
  container-free.
- Dispatch arm `Some("fdb-doctor")` (`main.rs:80`), usage line (`:112`), `run_fdb_doctor`
  (`:365`).
- Preflight inside `run_fdb_conformance` (`main.rs:319-337`; the fn is at
  `origin/main:292`). It probes **only** the client-library row — deliberately: the
  cluster file and the cluster are what that job goes on to *create*
  (`write_fdb_cluster_file` `origin/main:382`, `configure_fdb_database` `:334`), so
  probing them at preflight would report a red the job is about to fix. It follows the
  cited skip convention (`docker_available` `origin/main:882`, `is_ci()` `:1508`, and the
  in-function pattern at `:299-312`): **hard failure in CI, warn-and-skip locally.**

`probe_client_library` checks the packages' standard install paths, then falls back to
parsing `ldconfig -p`. It does **not** `dlopen()` the library: `xtask` is
`#![forbid(unsafe_code)]` (`main.rs:51`). Presence on the linker's search path is the
property that actually decides whether `--features fdb` links, so this is the right probe,
not a weaker proxy for one.

### (b) `.github/workflows/fdb-conformance.yml` (new, 141 lines)

Shape taken from the two cited peers: container-job + `docker info` + `timeout-minutes` +
failure-artifact upload (`integration-nightly.yml`), and nightly cron + `workflow_dispatch`
+ an explicit "NOT a required merge-gate status check" rationale (`tier1-jepsen.yml:1-30`).
Adds a `pull_request` trigger whose path filter includes `crates/metadata-fdb/**` (plus
`crates/server/**`, `deploy/fdb-single-node/**`, `xtask/**`, `Cargo.lock`, itself).
Installs the pinned `foundationdb-clients` `.deb` and runs `cargo xtask fdb-conformance`.
It does **not** consume `wyrd:fdb` (#470), per the brief's flagged deviation.

Cron is 06:00 UTC — after tier1-jepsen (02:00), tier1-disk-faults (03:00),
integration-nightly (04:00), tier2-kill-reconstruct (05:00), so the container jobs never
contend for a runner.

### (c) `feature_gated_checks(tikv, fdb)`, moved to the lib

Moved from the **binary** target (`origin/main` `xtask/src/main.rs:1255`, a private free
fn an integration test cannot reach) to `xtask/src/lib.rs:61`, taking **two** booleans.
`run_ci_steps` gained an `fdb_toolchain` parameter (`main.rs:1392`) and now iterates
`xtask::feature_gated_checks(tikv_toolchain, fdb_toolchain)` unconditionally — the
per-row gate moved *into* the function. `fdb_toolchain_available()` reads
`WYRD_FDB_TOOLCHAIN` (`main.rs:1378`), mirroring `tikv_toolchain_available()`
(`origin/main:1273`).

This is the brief's hazard (ii), and it is not hypothetical: `origin/main`'s
`run_ci_steps` wraps the *whole* list in `if tikv_toolchain { … }` (`:1314`). Appending
fdb rows there would have fired the FDB typecheck only when `WYRD_TIKV_TOOLCHAIN` was
set. See RED C below — the test catches exactly that mutation.

### The audit-policy note — `deny.toml:7-29` (header comment)

Records the split the issue names: `cargo deny`/`cargo audit` traverse the Rust graph
(`foundationdb -> foundationdb-sys -> (build) bindgen -> clang-sys -> libloading`, which
is why the ISC entry at `deny.toml:38-45` on `origin/main` exists) but **cannot** see
`libfdb_c`, a system shared object with no lockfile entry and no RUSTSEC advisories; its
advisory surface is tracked by following upstream release notes and bumping the version in
lockstep across the three files that name it. No allowlist / `[advisories]` / `[bans]` row
was added or changed — verified: the diff to `deny.toml` is +22 lines, all inside the
header comment block.

**Nothing under `docs/design/` was touched** (441 owns `07-deployment-view.md` this wave).
Verified: `git diff --stat` lists six files, none under `docs/design/`.

## The three forced questions

**(a) Genuine red?** Yes — three separate reverts, each re-run through
`cargo test -p xtask --test fdb_harness` (the command the Success criterion names):

| Revert | Result |
|---|---|
| **RED A** — fix fully reverted (`fdb_doctor.rs` + workflow deleted, `lib.rs`/`main.rs`/`deny.toml` restored to `origin/main`), test kept | `error[E0432]: unresolved import xtask::fdb_doctor`; `error[E0425]: cannot find function feature_gated_checks` ×5 → **exit 101** |
| **RED B** — everything present, but `Report::is_ok()` mutated to `true` | `doctor_is_red_when_a_failing_probe_outcome_is_planted` **FAILED** (15 passed, 1 failed) |
| **RED C** — the fdb rows gated on `if fdb && tikv` (the exact coupling hazard) | `feature_gated_checks_type_check_the_fdb_surface…` **FAILED** *and* `the_fdb_and_tikv_toolchain_gates_are_independent` **FAILED** (14 passed, 2 failed) |

RED A is the brief's predicted red (module/workflow do not exist). B and C matter more:
they prove the test binds *behaviour*, not non-existence. RED B kills the row logic while
leaving every symbol in place; RED C is precisely the silent, wrong coupling the brief's
hazard (ii) warns about. Post-fix: **16 passed, 0 failed.**

**(b) Production path?** Yes. The test calls `xtask::fdb_doctor::diagnose` and
`xtask::feature_gated_checks` — the *same* functions the production callers use:
`run_fdb_doctor` (`main.rs:365`), `run_fdb_conformance`'s preflight (`main.rs:328`), and
`run_ci_steps` (`main.rs:1408`). One doctor, two call sites; one row list, two call sites
— the `deploy_guard::scan_dir` precedent. No copy, no mock, no re-implementation. The
workflow assertion reads the real `.github/workflows/fdb-conformance.yml` and the real
`xtask/src/main.rs` dispatch table (the `readme_dev_section.rs` pattern).

**(c) Fixture includes the fault?** Yes. `doctor_is_red_when_a_failing_probe_outcome_is_planted`
plants a **real failing `Outcome`** for each of the three probes *in turn*, in an otherwise
healthy fixture, and asserts the verdict flips, that **exactly** the planted probe is in
`failures()`, and that `into_result()`'s `Err` carries that probe's remediation. The
fixture is not curated to exclude the failing row — it is built healthy and then poisoned,
mirroring `scan_dir_is_red_when_an_orchestrator_import_is_planted`
(`xtask/tests/deploy_no_orchestrator_coupling.rs:67`). `a_passing_row_carries_no_remediation`
closes the converse hole (an unconditional `remediation()` would satisfy the planted-red
assertions while telling a healthy operator to reinstall FoundationDB).

## Live evidence (the brief requires this; the host has everything)

Host: `docker info` OK · `/lib/libfdb_c.so` **7.3.77** · `fdbcli` **7.3 (v7.3.77)** —
byte-matching `foundationdb/foundationdb:7.3.77` in the compose file. No external
dependency was missing; nothing was substituted with a code-read.

1. **`cargo xtask fdb-conformance` — EXIT 0.** All five legs green (`--lib` 32 passed;
   `conformance` 1; `contention` 3; `scan` 2; `timeout` 3), cluster torn down cleanly.
   The new preflight ran and passed (client library found) before compose up.
2. **`cargo xtask fdb-doctor` with no cluster** → exit 1, and it printed exactly the
   remediation for the two failing rows:
   ```
   [ok  ] client library (libfdb_c): found at /usr/lib/libfdb_c.so
   [FAIL] cluster file: /etc/foundationdb/fdb.cluster: No such file or directory (os error 2)
          fix: point `WYRD_FDB_CLUSTER_FILE` at a readable cluster file, or install one at …
   [FAIL] cluster health: Unable to read cluster file … 1515 No cluster file found …
          fix: bring the throwaway single-node cluster up with `docker compose -f deploy/…up -d` …
   ```
3. **`cargo xtask fdb-doctor` against a REAL healthy cluster** (compose up + `configure new
   single memory` + cluster file) → all three rows `[ok  ]`, exit 0. This matters: real
   `fdbcli --exec "status minimal"` prints `The database is available.` — I confirmed the
   literal output rather than trusting my own fixture string. It is why
   `cluster_status_is_healthy` matches `database is available` and **not** `available`:
   fdbcli's failure output is `The database is unavailable`, which *contains* the
   substring "available". A naive needle reports a dead cluster healthy. Asserted both
   ways in `an_unavailable_cluster_is_not_read_as_available`.
4. **Both `--features fdb` rows compile for real** on this host:
   `cargo check -p wyrd-metadata-fdb --features fdb --tests` and
   `cargo check -p wyrd-server --features fdb --tests` → both `Finished`. So the rows
   `feature_gated_checks(_, true)` emits are not merely recorded argv — they type-check
   the previously-uncompiled `#[cfg(feature = "fdb")]` surface.
5. **`./engine/xtask.sh ci` (the gating `C4-ci`) — green on this machine with
   `WYRD_FDB_TOOLCHAIN` unset**, i.e. `cargo xtask ci` stays container-free and does not
   link `libfdb_c`. `cargo fmt --all` and `cargo clippy -p xtask --all-targets` are clean
   (commit-hook readiness).

## Decisions, and what I ruled out

**The workflow runs `cargo xtask ci` only on the nightly/dispatch legs, not on PRs.**
The `WYRD_FDB_TOOLCHAIN` gate is useless unless *something* sets it, and this runner is
the only one in the repo that installs `libfdb_c`. Three options:

- *Set `WYRD_FDB_TOOLCHAIN: "1"` and never run `ci`.* Rejected: the env var would be
  decorative, and the two rows would type-check nothing anywhere. That is the "switch
  nobody flips" failure — inert scaffolding, which the brief's verification posture
  explicitly forbids.
- *Duplicate the rows as literal steps* (`run: cargo check -p wyrd-server --features fdb
  --tests`). Rejected on drift, not on size: it copies a row list that
  `xtask::feature_gated_checks` already single-sources (ADR-0016), so a future third row
  would land in the lib and silently not run in CI.
- **Chosen:** `WYRD_FDB_TOOLCHAIN: "1"` at job level + one `cargo xtask ci` step guarded
  by `if: github.event_name != 'pull_request'`. Single-sourced, no duplicated list.
  Concrete cost: **one extra step, ~15 min of runner time, on the nightly leg only.** On a
  PR it is skipped, because `ci.yml` already runs `cargo xtask ci` (minus the fdb rows)
  and the `fdb-conformance` step above it already compiles `wyrd-metadata-fdb --features
  fdb`; the only surface the PR leg then misses is the server's selection arms, which the
  nightly catches. Making it unconditional would add that ~15 min to **every** PR touching
  `crates/server/**` or `xtask/**` to re-check one crate.

**No standalone `cargo xtask fdb-doctor` step in the workflow.** I wrote one, then removed
it. At the only point it could sit (after installing the client package, before
`fdb-conformance`) the cluster is not up, so two of its three rows are red by construction
and the step needs `|| true` to pass — a step that cannot fail. Its client-library row is
already run there, for real, by `run_fdb_conformance`'s internal preflight. The doctor is
named in the workflow header comment as the *local* preflight (and that mention is scraped
and checked against the dispatch table by `the_fdb_conformance_workflow_exists_and_only_names_real_subcommands`).

**No new Cargo dependency.** `xtask` still depends only on `wyrd-chunk-format`, `serde`,
`serde_json`. The workflow is asserted with substring checks (`readme_dev_section.rs`
pattern), not a YAML parser. The doctor duplicates `WYRD_FDB_CLUSTER_FILE` and
`/etc/foundationdb/fdb.cluster` as literals rather than depending on `wyrd-metadata-fdb`
(whose `fdb` feature would drag `libfdb_c` into the gate's build graph, and whose adoption
is an ADR-0003 §2 / INTEGRATION §4 human-only decision). The duplication is not left
unguarded: `the_doctors_cluster_file_literals_match_the_drivers_own` reads
`crates/metadata-fdb/src/lib.rs` and fails if the driver renames either const, and
`the_pinned_fdb_version_agrees_across_the_image_the_client_and_the_doctor` fails on a
partial version bump across compose / workflow / `FDB_VERSION`.

**`main.rs`'s existing unit test was updated, not weakened.**
`ci_type_checks_feature_gated_metadata_scenario` (`origin/main:1555`) kept both of its
assertions under the new two-arg signature, and a sibling
`ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone` was added. Both still drive
the real `run_ci_steps` wiring with a recording executor, so deleting the wiring loop
flips them red.

## Not done / for the human

- The workflow's **green on a GitHub-hosted runner** is off-Check by construction (the
  brief's declared deferral) — nobody can observe it from `cargo xtask ci`. Everything it
  *invokes* is proven to exist (dispatch-table assertion) and proven to work on this host
  (live evidence 1–4 above). The one thing unproven is the hosted execution itself: the
  `foundationdb-clients_7.3.77-1_amd64.deb` download URL and `sudo dpkg --install` on
  `ubuntu-latest`. Confirm on the first nightly run / on the PR that lands it.
- `git stash` mishap during the red check: `git stash push -- <paths>` refused an untracked
  path and the follow-up `pop` applied a **pre-existing** stash (`stash@{0}`, the human's
  "pre-rebase … dirty docs") into the worktree. I restored the tree
  (`git reset` + `git checkout --force -- docs/design/` + removed the two untracked draft
  files it wrote) and **`stash@{0}` is intact** — `git stash list` still shows all five
  entries, and the pop errored before dropping it. The subsequent reds used file copies,
  not `git stash`. Final `git status --short` shows only this bundle's six files. Flagging
  because it touched shared `.git` state, even though it is fully reverted.
