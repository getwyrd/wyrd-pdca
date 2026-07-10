# Build notes — issue 439 / fdb-dev-ci-harness (iteration 2)

Target branch: `getwyrd/wyrd @ main` (`b1ccca3`). All `path:line` citations below are
against the patched tree in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, whose
base is `origin/main`. `patch.diff` was verified to `git apply --check` cleanly onto a
pristine `origin/main` worktree, and to be green, `rustfmt`-clean and
`clippy -D warnings`-clean there.

---

## 1. What the carry-forward demanded, and what I did

The sign-off rationale was blunt: *"the tests meant to pin the three seams this slice
exists to create are vacuous, and shipped comments assert coverage that does not exist …
Trust the mutations, not the PASS row."* Four items. I treated each as **"make the
mutation red,"** not "reword the comment," except where the honest answer was to remove
the seam a comment was claiming.

### Item 1 — the workflow↔dispatch test passed on a workflow that runs no `xtask` command

**Cause:** `xtask_subcommands` scraped `cargo xtask <sub>` from the *whole file text*,
so the header comment satisfied the contract. Deleting both `run:` steps left 16/16 green.

**Fix (cause, not symptom):** the scrape is now scoped to what a `run:` key actually
executes. `run_script_lines` (`xtask/tests/fdb_harness.rs:467`) walks the YAML, captures
inline `run: <cmd>` and `run: |` block bodies, and drops YAML `#` lines *and* shell `#`
lines. `xtask_subcommands` (`:510`) then reads only those. Prose cannot satisfy it.

The scraper itself now carries a **planted red**
(`run_script_scraping_ignores_prose_and_is_red_on_a_workflow_that_runs_nothing`,
`xtask/tests/fdb_harness.rs:568`): a synthetic workflow that mentions
`cargo xtask fdb-conformance` in a header comment, in a step `name:`, and in a shell
comment, and executes none of it, must scrape to `[]` — while `docker info` and
`set -euo pipefail` are still captured, so the scraper is not merely blind.

*Refuted, executed:* deleting both real `run:` xtask steps →
`the_fdb_conformance_workflow_executes_only_real_subcommands` **FAILS**
(`Executed: []`). This is the exact mutation that left v1 fully green.

### Item 2 — comments claiming a red that did not exist

Two false claims (`main.rs:1713-1714`, `lib.rs:54-55`, `fdb_doctor.rs:14`). I did **not**
delete the comments and move on; I made both claims true by removing the *ability* to
write the bug.

**(a) `run_ci`'s choice of arguments.** v1's `run_ci` computed
`run_ci_steps(tikv_toolchain_available(), fdb_toolchain_available(), …)`. Nothing tested
`run_ci`, so `fdb = tikv && fdb` stayed green. Guarding that with a test would still leave
the call site rewritable.

Instead `run_ci_steps` now takes the **environment lookup itself**
(`xtask/src/main.rs:1375`, `toolchain: &mut dyn FnMut(&str) -> bool`) and resolves both
gates *by name* internally (`:1406-1409`), from `xtask::TIKV_TOOLCHAIN_ENV` /
`xtask::FDB_TOOLCHAIN_ENV` (`xtask/src/lib.rs:23`, `:34`). `run_ci` passes
`std::env::var_os(name).is_some()` (`:1418`) — a lookup that is *uniform in the name*,
so there is no longer a place to write "fdb = tikv". `tikv_toolchain_available()` and the
proposed `fdb_toolchain_available()` are gone; the two env names are the single source.

This is the same injection precedent the file already used for `exec`. It costs 4 lines
net at the call site and it is exactly the `docs/principles.md` §1.2 preference: remove the
cause (a boolean pair chosen in an untestable function) rather than guard the symptom.

*Refuted, executed* — both mutations are now red in `cargo test -p xtask`:
- `feature_gated_checks(toolchain(TIKV), toolchain(TIKV) && toolchain(FDB))` — brief
  hazard (ii) verbatim, and the adversary's mutation → `tests::ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone` **FAILS**.
- `if toolchain(TIKV) { for check in feature_gated_checks(true, toolchain(FDB)) … }` —
  the pre-#439 shape → same test **FAILS**.

The comments at `xtask/src/main.rs:1360-1366` and `xtask/src/lib.rs:62-65` now describe a
red that I ran.

**(b) "one doctor, two call sites" / the deletable preflight.** In v1 the preflight was a
block inside `run_fdb_conformance`; deleting it left the suite green.

`run_fdb_conformance` (`xtask/src/main.rs:309`) is now **nothing but** the gate:

```rust
fdb_doctor::run_gated_conformance(
    docker_available(), is_ci(), probe_client_library(),
    &mut || fdb_conformance_stack(&compose),
)
```

The container stack moved to `fdb_conformance_stack` (`:323`) and is **injected as a
closure** into `fdb_doctor::run_gated_conformance` (`xtask/src/fdb_doctor.rs:341`), which
lives in the lib target. `xtask/tests/fdb_harness.rs` therefore drives the *production*
gate with the stack replaced by a call counter and covers all six branches (`:341`,
`:368`, `:388`). "The preflight never runs" is now a failing assertion, not prose.

There is no "block to delete" any more: bypassing the preflight means deleting the
command. The residual — someone rewriting `run_fdb_conformance`'s body to call
`fdb_conformance_stack` directly — is caught by
`the_conformance_command_delegates_to_the_gate_and_is_red_when_it_does_not`
(`xtask/tests/fdb_harness.rs:427`), which reads `run_fdb_conformance`'s real body out of
`main.rs` and asserts it delegates and never touches `fdb_compose`. **That scanner carries
its own planted red** (a synthetic ungated body, `:431`), so it is load-bearing rather
than a scanner that always says yes. (This is the `deploy_no_orchestrator_coupling.rs`
pattern; it *complements* the reachability fix rather than substituting for it, which is
what the brief forbade.)

*Refuted, executed:*
- `run_fdb_conformance` bringing the stack up directly → that test **FAILS**.
- `run_gated_conformance` gutted to always `stack()` →
  `a_missing_client_library_stops_the_job_before_a_container_is_started` and
  `a_missing_docker_stops_the_job_before_a_container_is_started` both **FAIL**.

### Item 3 — `probe_client_library` reported a working FDB build as a missing client

**Verified against the authoritative source, not recall:**
`~/.cargo/registry/src/index.crates.io-*/foundationdb-sys-0.10.0/build.rs:61-64` reads

```rust
if let Ok(lib_path) = env::var("FDB_CLIENT_LIB_PATH") {
    println!("cargo:rustc-link-search=native={lib_path}");
}
```

So a client under a custom prefix with that variable set **links fine**, and v1's probe
reported `Failed` — a *new* false green locally (`return Ok(())`, five legs silently
skipped) and a *new* hard CI failure on a job whose build would have succeeded.

**Fix:** `fdb_doctor::CLIENT_LIB_PATH_ENV` (`xtask/src/fdb_doctor.rs:61`) and the pure
`client_library_search_paths` (`:374`) put `$FDB_CLIENT_LIB_PATH/libfdb_c.so` **first**,
ahead of the five standard prefixes, with `ldconfig -p` still the last resort.
`probe_client_library` (`xtask/src/main.rs:385`) consumes it. Trailing slashes normalise;
`/` does not collapse to the empty path. The false doc comment is replaced
(`xtask/src/main.rs:374-384`).

*Refuted, executed:* dropping the env branch → `the_client_library_search_honours_fdb_client_lib_path`
(`xtask/tests/fdb_harness.rs:202`) **FAILS**.

*Also verified live on the real binary* (three runs of `cargo xtask fdb-doctor`):

| `FDB_CLIENT_LIB_PATH` | client-library row |
|---|---|
| `/tmp/fdb-custom-prefix/lib/` (trailing slash) | `ok — found at /tmp/fdb-custom-prefix/lib/libfdb_c.so` |
| unset | `ok — found at /usr/lib/libfdb_c.so` |
| `/tmp/definitely-not-here` | `ok — found at /usr/lib/libfdb_c.so` (falls back) |

### Item 4 — the PR leg never type-checked the server's `fdb` arms

**Cause:** `cargo xtask ci` carried `if: github.event_name != 'pull_request'`, and the
conformance driver's legs build `-p wyrd-metadata-fdb` only. So `crates/server/**` in the
path filter promised a check nothing performed.

**Fix:** a new unconditional step, `Type-check the fdb feature arms`
(`.github/workflows/fdb-conformance.yml:125-128`), runs on **every** trigger:

```
cargo check -p wyrd-metadata-fdb --features fdb --tests
cargo check -p wyrd-server --features fdb --tests
```

I chose "run the check" over "drop `crates/server/**` from the filter" because dropping it
would leave gap 2 of the issue half-open — the server's nine `#[cfg(feature = "fdb")]`
arms would still be compiled by nothing until the 06:00 cron. The whole-gate
`cargo xtask ci` step stays nightly-only (it adds ~15 min/PR for no new signal, since
`ci.yml` already runs it minus the fdb rows).

Crucially, the two commands are not free text: `the_pull_request_leg_type_checks_every_fdb_feature_arm_it_filters_on`
(`xtask/tests/fdb_harness.rs:652`) derives them from
`xtask::feature_gated_checks(false, true)` and asserts the workflow runs each **verbatim
and without an `if:`**. A row added to the function but not to the workflow is red, and
vice versa.

*Refuted, executed (two variants):*
- deleting the `cargo check -p wyrd-server …` line → **FAILS**.
- putting the step behind `if: github.event_name != 'pull_request'` → **FAILS**.

---

## 2. What I deliberately did NOT change

Per the carry-forward's "WHAT SURVIVED ADVERSARIAL ATTACK — keep as-is, do not churn":
`HEALTHY_STATUS_NEEDLE`, the drift guards, `cluster_file_path`, `probe_cluster_health`'s
lack of a timeout, and the `@v7` action pins are all carried over unchanged.

`configure_fdb_database`'s readiness poll (`xtask/src/main.rs:551-576`) still gates on
`status.success()` with stdout/stderr nulled, which given `fdbcli`'s exit-0-when-unavailable
behaviour returns `Ok` on attempt 1 regardless. **Pre-existing, out of scope, untouched** —
the carry-forward records it as a §10 Act candidate. This patch adds the tested predicate
(`cluster_status_is_healthy`) that would fix it and deliberately does not wire it there.

Also unchanged, per the brief's explicit constraints: no `docs/design/` file is touched
(441 owns `07-deployment-view.md` in the same wave); no new Cargo dependency (the workflow
is asserted with plain-text/substring checks — `xtask` has no YAML parser and adding one
would trigger the ADR-0003 §2 audit); no `Makefile`; no `deny.toml` allowlist/advisory/ban
edit (header comment only); no consumption of the `wyrd:fdb` image (#470).

## 3. Alternatives rejected, with the cost shown

- **Keep `feature_gated_checks()` private in `main.rs` and text-scrape it from the test.**
  Rejected: the brief forbids it explicitly, and it is the wrong shape — an integration
  test under `xtask/tests/` links the *lib*, so the only honest fix is the move the brief
  names. Concretely, `xtask/src/lib.rs` grows 85 lines and `main.rs` sheds 32.

- **Cover `run_ci`'s call site by keeping `fdb_toolchain_available()` and adding a test
  that mutates the process environment.** Rejected on correctness, not cost:
  `std::env::set_var` is process-global and Rust test binaries are multi-threaded, so a
  test that sets `WYRD_FDB_TOOLCHAIN` races every other test in the binary. The env-lookup
  injection is 6 lines (`main.rs:1375`, `:1406-1409`, `:1418-1420`) and removes the hazard
  structurally instead of observing it.

- **Move `run_ci_steps` itself into the lib** so `fdb_harness.rs` could call it. Rejected:
  it would relocate ~40 lines of cargo-step list that has nothing to do with this slice,
  and `main.rs`'s existing `mod tests` already owns that unit test
  (`ci_type_checks_feature_gated_metadata_scenario`, present on `origin/main`). The new
  `ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone` sits beside it, and both are
  run by the whole-tree `cargo xtask ci`. Cost of the rejected move: +40/-40 lines across
  two files for zero additional coverage.

- **Drop `crates/server/**` from the workflow's path filter** (the carry-forward's
  alternative for item 4). Rejected: it makes the filter honest by *narrowing the promise*
  rather than by keeping it, leaving the server's `#[cfg(feature = "fdb")]` arms
  (`crates/server/src/cli.rs:101,119,167,372,458,713,839,1479` and `:1550`, `:1598`) compiled by no PR
  check at all. The chosen fix is 4 lines of workflow (`:125-128`) and closes gap 2 on the
  PR leg. Cost of the rejected option: 1 line deleted, gap 2 left open until the cron.

---

## 4. Forced self-refutation (the three questions)

**(a) Genuine red? — YES, and not only by non-existence.**

Two distinct reds were executed:

1. *Whole-fix reverted, test kept* (the `C4-verify` contract). On a pristine `origin/main`
   worktree with only `xtask/tests/fdb_harness.rs` added:
   `error[E0432]: unresolved import 'xtask::fdb_doctor'`,
   `error[E0425]: cannot find function 'feature_gated_checks' in crate 'xtask'`,
   `cannot find value 'TIKV_TOOLCHAIN_ENV' / 'FDB_TOOLCHAIN_ENV'` → exit 101. RED.
   This is the weak red the adversary rightly discounted, so:

2. *Per-seam mutation reds*, each run against the fully-patched tree. Every one of these
   was **green** in iteration 1:

   | # | Mutation (production code) | Result |
   |---|---|---|
   | M1 | delete both workflow `run:` steps that invoke `cargo xtask` | RED — `the_fdb_conformance_workflow_executes_only_real_subcommands` |
   | M2 | `feature_gated_checks(toolchain(TIKV), toolchain(TIKV) && toolchain(FDB))` | RED — `tests::ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone` |
   | M2b | gate the whole loop on the tikv boolean (pre-#439 shape) | RED — same test |
   | M3 | `run_fdb_conformance` enters the stack directly (preflight deleted) | RED — `the_conformance_command_delegates_to_the_gate_and_is_red_when_it_does_not` |
   | M3b | `run_gated_conformance` always `stack()` (preflight gutted) | RED — 2 tests |
   | M4 | `client_library_search_paths` ignores `FDB_CLIENT_LIB_PATH` | RED — `the_client_library_search_honours_fdb_client_lib_path` |
   | M5 | drop `cargo check -p wyrd-server --features fdb` from the workflow | RED — `the_pull_request_leg_type_checks_every_fdb_feature_arm_it_filters_on` |
   | M5b | put the type-check step behind `if: … != 'pull_request'` | RED — same test |

   All eight restored afterwards; the tree is green (24/24 in `fdb_harness`, 19/19 in the
   `xtask` bin, and `cargo xtask ci` → *"all checks passed"*).

**(b) Production path? — YES.** No mocks, no re-implementations. The tests call the same
functions production calls: `fdb_doctor::diagnose` (the one `run_fdb_doctor` renders and
the preflight consumes), `fdb_doctor::run_gated_conformance` (**literally the whole body of
`run_fdb_conformance`**), `fdb_doctor::client_library_search_paths` (called by
`probe_client_library`), `xtask::feature_gated_checks` (called by `run_ci_steps`), and
`main.rs`'s real `run_ci_steps` driven through a recording executor. The only injected
substitute is the *container stack closure* — the thing that cannot run headless — and the
assertion is precisely that it is **not entered**. The workflow and `main.rs` are read from
disk, not from fixtures.

**(c) Fixture includes the fault? — YES.** The planted-red fixtures each *contain* the
failing element rather than curating it out:
- `doctor_is_red_when_a_failing_probe_outcome_is_planted` plants a real `Outcome::failed`
  for **each of the three probes in turn** (not a curated healthy set).
- `run_script_scraping_ignores_prose_…` feeds a workflow that *does* mention the command in
  comments and a step `name:` — the exact shape that fooled v1 — and demands `[]`.
- `the_conformance_command_delegates_…` feeds a body that *does* call `fdb_compose` — the
  exact pre-#439 shape — and demands rejection.
- `a_missing_client_library_stops_the_job_…` passes a *failing* `Outcome` and asserts the
  stack is entered **0** times, with the counterpart
  `a_ready_environment_enters_the_stack_exactly_once_…` proving the gate is not simply
  "never run" (non-vacuity).

---

## 5. Evidence: the runs

Through the project's own runner (`./engine/xtask.sh`, which `cd`s to `$PDCA_WORKTREE`
and execs `cargo xtask`) unless noted.

- `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** on this machine, which has
  neither `WYRD_TIKV_TOOLCHAIN` nor `WYRD_FDB_TOOLCHAIN` set. The brief's "`cargo xtask ci`
  stays green on a machine with no FDB" holds: with both gates undeclared,
  `feature_gated_checks(false, false)` is empty and nothing links `libfdb_c`.
- `cargo test -p xtask --test fdb_harness` → **24 passed**.
- `cargo test -p xtask` (all targets) → 19 + 6 + 12 + 24 + 3 + 1 + 2 passed, 0 failed.
- `cargo fmt --all -- --check` → clean (applied `cargo fmt --all` before finalising; the
  target's commit hook runs it).
- `cargo clippy -p xtask --all-targets -- -D warnings` → clean.
- `git apply --check patch.diff` on a pristine `origin/main` worktree → applies cleanly;
  re-ran the full `cargo test -p xtask`, `cargo fmt --check` and `clippy -D warnings`
  **there**, all green — so the patch is commit-ready against the target, not just against
  my worktree.

### Supplementary live leg — the brief REQUIRED this, and it ran

Host has Docker, `libfdb_c` **7.3.77** (`/lib/libfdb_c.so`), `fdbcli` **7.3 (v7.3.77)** —
byte-matching the compose image `foundationdb/foundationdb:7.3.77` and the `fdb-7_3` crate
pin. Nothing was missing; no code-read was substituted for a run.

- **`./engine/xtask.sh fdb-doctor`** (no cluster up) → exactly the intended output:
  `[ok  ] client library (libfdb_c): found at /usr/lib/libfdb_c.so`, then `[FAIL] cluster
  file` naming `WYRD_FDB_CLUSTER_FILE` and the `/etc/foundationdb/fdb.cluster` default, then
  `[FAIL] cluster health` naming
  `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`. Exit non-zero.
- **`./engine/xtask.sh fdb-conformance`** → **passed end-to-end**: preflight `Proceed`,
  compose up, `configure new single memory`, all five `--features fdb` legs green
  (32 + 1 + 3 + 2 + 3 = 41 tests), stack torn down, `EXIT=0`,
  *"FoundationDB passed the shared MetadataStore conformance suite and the contention
  properties"*. This is the live evidence that the new preflight does not break the
  previously-working job — the `Proceed` branch exercised for real.
- **`cargo xtask fdb-doctor` × 3** with `FDB_CLIENT_LIB_PATH` set / unset / bogus (table in
  §1 item 3) — the item-3 regression exercised on the real binary, not just in the unit test.

No `NEEDS-HUMAN external dependency` declarations: every dependency the brief named was
present and used.

## 6. Honest limits (for the human at sign-off)

- **The workflow's green on a GitHub-hosted runner is still deferred** — the brief declares
  this. What is *proven* here: the file parses under the same `run:`-scoped reader the test
  uses; every `cargo xtask <sub>` it executes exists in `main.rs`'s dispatch table; the two
  `cargo check` commands it runs are `feature_gated_checks(false, true)` verbatim; the FDB
  version pin agrees across the compose image, the workflow env, and `fdb_doctor::FDB_VERSION`.
  What is *not* proven: that `dpkg --install foundationdb-clients_7.3.77-1_amd64.deb`
  succeeds on `ubuntu-latest`, and that the release asset URL is correct. Both are observable
  only on the first PR/nightly run. Same posture as `tier1-jepsen.yml` when it landed.
- **`probe_cluster_health` has no explicit timeout** on `Command::output()`. Measured on
  7.3.77: `fdbcli` self-bounds at ~5s against a dead coordinator. Carried over unchanged
  from v1, where the adversary attempted and failed to refute it.
- **`run_fdb_conformance`'s one-line delegation** is guarded by a source-level scanner
  (with its own planted red), not by a runtime assertion. A runtime assertion is impossible
  headless — entering the real body means starting a container. I flag this rather than
  claim more than it proves.
