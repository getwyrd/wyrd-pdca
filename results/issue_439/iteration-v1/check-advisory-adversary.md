# Adversarial review — issue 439 / fdb-dev-ci-harness (advisory, non-gating)

Method: re-ran `cargo test -p xtask --test fdb_harness` on `$PDCA_TARGET` (16/16 green),
then took a scratch copy of the patched tree and **mutated production code** to see what
the harness actually catches. Also drove the real `fdbcli` 7.3.77 present on this host
against a dead coordinator. Toolchain was fully available; no `NEEDS-HUMAN` below is a
sandbox artefact.

Two mutations that reinstate the exact defects the brief says the slice closes leave the
suite **green**. Both are reproduced below.

---

## Refuted — the evidence does not pin what it claims

- **NEEDS-HUMAN — `xtask/tests/fdb_harness.rs:298-312`: the workflow↔dispatch test passes on a workflow that runs no `xtask` command at all.**
  `xtask_subcommands` (`xtask/tests/fdb_harness.rs:281`) scrapes every `cargo xtask <sub>`
  token in the file, **including the header comment prose**. The workflow mentions
  `cargo xtask fdb-conformance` at `.github/workflows/fdb-conformance.yml:6` and `:33`, and
  `cargo xtask ci` at `:23` and `:67` — before any `run:` key is reached.
  *Concrete failing case, executed:* I deleted the only two steps that invoke xtask
  (`.github/workflows/fdb-conformance.yml:116-117` and `:128-130`), leaving the header,
  `docker info`, the client-package install and the artifact upload. All 16 tests still
  passed — including the one whose message reads *"the workflow must actually run
  `cargo xtask fdb-conformance` — that is the whole job."* The `subs.len() >= 2`
  non-vacuity guard at `:311` is satisfied by prose alone.
  This is gap 1 of the brief ("**no CI job runs it**"). The test asserts the workflow file
  *mentions* the subcommand, never that it *executes* it. Assertion 3 of the success
  criterion is satisfied for the wrong reason. A `run:`-scoped scrape (or asserting the
  token appears after a `run:` line) would restore the intended bind.

- **NEEDS-HUMAN — `xtask/src/main.rs:1433-1436`: the coupling hazard the brief calls "the reason for two parameters" is still reachable, and the patch's own comment says it is not.**
  Nothing in the test suite invokes `run_ci()`; `fdb_toolchain_available()`
  (`xtask/src/main.rs:1378`) has exactly one caller, `run_ci` at `:1435`, and no test
  exercises that argument wiring — `recorded_invocations` (`:1660-1668`) passes booleans
  directly to `run_ci_steps`.
  *Concrete failing case, executed:* changing `:1435` from `fdb_toolchain_available()` to
  `tikv_toolchain_available() && fdb_toolchain_available()` — i.e. brief hazard (ii)
  verbatim, the FDB typecheck firing only when `WYRD_TIKV_TOOLCHAIN` is also set — leaves
  `cargo test -p xtask` **entirely green** (19+6+12+16+3+1+2 passed, 0 failed).
  The comment at `xtask/src/main.rs:1713-1714` states: *"Setting `fdb_toolchain` to
  `tikv_toolchain` in `run_ci` … flips this red."* **It does not.** (Only the naked
  `tikv_toolchain_available(), tikv_toolchain_available()` form goes red, and by accident —
  a `dead_code` lint on the now-unused fn, not by any assertion.) `xtask/src/lib.rs:54-55`
  makes the same unwarranted claim: the fdb_harness assertion covers the *pure function's*
  signature, never the call site that chooses its arguments.

- **`xtask/src/fdb_doctor.rs:14` — "one doctor, two call sites" is asserted in prose, pinned by nothing.**
  *Concrete failing case, executed:* deleting the whole preflight block
  (`xtask/src/main.rs:327-340`) leaves `cargo test -p xtask` green. Likewise no test reaches
  `run_fdb_doctor` (`:369`) or any `probe_*` fn (`:396`, `:417`, `:433`). The load-bearing
  red (`fdb_harness.rs:290`, the planted outcome) exercises `diagnose` → `is_ok` →
  `into_result`, a pure mapping; it does not witness that production ever calls it. That is
  the honest scope of the red→green in `check-gates.json` (`C4-verify`), and it is narrower
  than the brief's "the row logic is load-bearing" reads.

## Refuted — inputs that break the fix

- **`xtask/src/main.rs:396-413` (`probe_client_library`): a supported, working FDB build is reported as a missing client library.**
  `foundationdb-sys` 0.10's `build.rs:61-64` emits `cargo:rustc-link-lib=fdb_c` plus a
  `rustc-link-search` derived from **`FDB_CLIENT_LIB_PATH`**. The probe checks five
  hard-coded paths (`xtask/src/fdb_doctor.rs:50-56`) and `ldconfig -p`.
  *Concrete failing case:* `libfdb_c.so` under `/opt/foundationdb/lib` with
  `FDB_CLIENT_LIB_PATH=/opt/foundationdb/lib` and no `ld.so.conf` entry — the build links
  fine, the probe reports `Failed`. Consequences, both new with this patch: locally
  `cargo xtask fdb-conformance` now prints a warning and **`return Ok(())`**
  (`xtask/src/main.rs:339`) — exit 0, five test legs silently not run, a false green where
  the suite previously ran and passed; in CI (`CI=true`) it **hard-fails**
  (`:333`) a job whose build would have succeeded.
  The doc comment at `xtask/src/main.rs:393-395` — "presence on the linker's search path is
  exactly the property that decides whether the build can succeed" — is therefore false in
  both directions. Reading `FDB_CLIENT_LIB_PATH` as a fourth candidate source would close it.

- **`.github/workflows/fdb-conformance.yml:129` vs `:42`: the PR leg never type-checks the server's `fdb` arms, which is half of gap 2.**
  The path filter includes `crates/server/**` (`:42`) precisely because, per the brief, the
  `#[cfg(feature = "fdb")]` arms in `crates/server/src/cli.rs` are never compiled. But the
  only step that runs `cargo check -p wyrd-server --features fdb` is `cargo xtask ci`, and
  it is skipped on `pull_request` (`:129`). The PR leg runs only `cargo xtask
  fdb-conformance`, whose legs are `cargo test -p wyrd-metadata-fdb --features fdb`
  (`xtask/src/main.rs:566`) — the server crate is never built with the feature on.
  *Concrete failing case:* introduce a type error inside a `#[cfg(feature = "fdb")]` arm of
  `crates/server/src/cli.rs`. `ci.yml` is green (default features); `fdb-conformance.yml`'s
  PR leg is green; the error surfaces only on the 06:00 UTC cron, up to 24h after merge.
  For a `crates/server/**` PR the filter buys nothing it claims to buy.

## Attempted and could not refute

- **The `HEALTHY_STATUS_NEEDLE` (`xtask/src/fdb_doctor.rs:84`).** I ran the real `fdbcli`
  7.3.77 on this host against an unreachable coordinator: stdout is ``The database is
  unavailable; type `status' for more information.`` and **exit code 0**.
  `cluster_status_is_healthy` correctly returns `false`, and matching on text rather than
  exit status is demonstrably the *right* predicate here. No false positive found.
- **`probe_cluster_health` (`xtask/src/main.rs:433-436`) has no timeout on `Command::output()`.**
  I expected a hang on a dead coordinator; measured, `fdbcli` self-bounds and returns in 5s.
  Not a hang. (Adjacent, and *pre-existing*, so out of this diff's scope but worth the
  human's eye: `configure_fdb_database`'s readiness poll at `xtask/src/main.rs:483-508`
  gates on `status.success()` with stdout/stderr nulled — given the exit-0 result above it
  returns `Ok` on attempt 1 whether or not the database is available. This patch introduces
  the tested predicate that would fix it and does not use it there.)
- **`cluster_file_path` (`xtask/src/fdb_doctor.rs:113`)** genuinely mirrors the driver's
  `config::cluster_file` unset/blank/trim semantics (`crates/metadata-fdb/src/lib.rs:424-431`).
- **The drift guards** (`fdb_harness.rs:449`, `:475`) really do read the driver, the compose
  file and the workflow; a partial version bump does go red. `deny.toml`'s header is a
  comment block and `cargo deny` is unaffected (`C4-ci` green, re-run).
- **`actions/checkout@v7` / `actions/upload-artifact@v7`** are consistent with every other
  workflow in this repo (`ci.yml:92`, `integration-nightly.yml:38,59`), not a fabrication.

## On the verdict

`check-gates.json`'s `C4-verify` — *"red without the fix, green with it"* — is literally
true but weak: 15 of the 16 tests are red pre-fix by **non-existence** (`xtask::fdb_doctor`
does not compile), and the sole planted-red exercises a pure two-line mapping. The three
production seams the slice exists to create — the workflow's `run:` steps, `run_ci`'s
choice of `fdb_toolchain_available()`, and `run_fdb_conformance`'s preflight — are each
deletable with the suite still green. A reviewer reading the module docs
(`fdb_doctor.rs:14`, `lib.rs:54-55`, `main.rs:1713`) would take those seams as covered;
they are asserted in prose, not in tests.
