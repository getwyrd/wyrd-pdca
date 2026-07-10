# Adversarial review — issue 439 / fdb-dev-ci-harness (iteration 3)

Advisory only; does not gate. Grounded on `$PDCA_TARGET` (`b1ccca3`) with the patch applied.
Focus: the two carry-forward defect classes (scraper binds *mention* not *execution*;
production wiring left outside Check) that reopened iterations 1 and 2.

## Refutation attempts that landed

- **NEEDS-HUMAN — `xtask/tests/fdb_harness.rs:604-612`: the workflow↔dispatch scraper still
  counts a *mention-as-argument* as an execution.** `xtask_subcommands` matches `cargo xtask
  <sub>` at **any** position in a line via `tokens.windows(3)` (`:608-609`). The iteration-2
  fix only stripped trailing/full-line `#` comments (`strip_shell_comment`, `:524`); the
  window scan is unchanged. Concrete failing case: replace the conformance step's
  `run: cargo xtask fdb-conformance` (`.github/workflows/fdb-conformance.yml:141`) with
  `run: echo would run cargo xtask fdb-conformance` (or `run: : cargo xtask fdb-conformance`).
  The job then executes **no** xtask command, yet `xtask_subcommands` returns
  `["fdb-conformance"]`, so `the_fdb_conformance_workflow_executes_only_real_subcommands`
  (`:751`) stays **GREEN** — success-criterion assertion 3 satisfied for the wrong reason,
  the exact class iterations 1 and 2 were reopened for. The demonstrated-red
  `run_script_scraping_ignores_prose_and_is_red_on_a_workflow_that_runs_nothing` (`:662`)
  exercises only the comment shapes (`echo DISABLED # cargo xtask …`, `:681`), never a
  bare command taking those tokens as arguments, so the hole is uncovered. The iteration-2
  reviewer named the correct fix — "require the token be the command head, not any window";
  the builder chose comment-stripping instead, leaving this residual.

- **NEEDS-HUMAN — `xtask/src/main.rs:391-393` and `:312-315`: the impure probe/preflight
  wiring is still outside Check.** The builder moved the `FDB_CLIENT_LIB_PATH` *decision*
  into the lib (`fdb_doctor::probe_client_library`, unit-tested with fake effects at
  `xtask/tests/fdb_harness.rs:640`), which addresses the core of iteration-2's complaint.
  But the main.rs adapter that supplies the real effect is unguarded: mutate the env closure
  at `main.rs:393` from `&|name| std::env::var(name).ok()` to `&|_| None` and the
  iteration-1 false-negative returns verbatim — a working `FDB_CLIENT_LIB_PATH` build is
  reported "missing" → local false-green skip / CI hard-fail — with **every** Check test
  still green (they drive the lib fn with their own fakes). Symmetrically,
  `run_fdb_conformance` (`:309-316`) could be mutated to
  `run_gated_conformance(true, false, Outcome::ok("x"), …)`, hardcoding a passing preflight
  and bypassing the real `docker_available()` / `probe_client_library()` probes; the only
  guard on this call site, `conformance_body_is_gated` (`fdb_harness.rs:479`), checks solely
  that the body names `run_gated_conformance` and does not call `fdb_compose(` — never the
  arguments. Whether these thin impure adapters warrant a Check guard, or are acceptable
  untested wiring over a tested pure core, is the adjudication.

## Attempted and could not refute (recorded as a strong signal)

- **Toolchain-coupling independence** (`xtask/src/lib.rs:713` `feature_gated_checks(tikv, fdb)`
  + `main.rs:1440-1447` `run_ci_steps` env lookup). Tried the pre-#439 mutation shapes — gate
  the loop on the tikv boolean, read `WYRD_TIKV_TOOLCHAIN` where fdb is meant. `run_ci_steps`
  resolves each gate from its own env name and is driven by `recorded_invocations` with a fake
  environment (`ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone`), so both mutations
  flip red. The call site that chooses the arguments is now *inside* the tested `run_ci_steps`,
  closing iteration-1 item 2.
- **Preflight branches** (`fdb_doctor::run_gated_conformance`): `gated_run` counts stack
  entries per branch; deleting the gate or swallowing a failure flips red. Binds.
- **Drift guards**: verified the doctor's duplicated literals against the driver —
  `crates/metadata-fdb/src/lib.rs:386` (`WYRD_FDB_CLUSTER_FILE`) and `:390`
  (`/etc/foundationdb/fdb.cluster`) match the doctor's constants, and `cluster_file_path`
  mirrors the driver's `cluster_file` trim/blank-fallback (`lib.rs:428-431`). The three-file
  version pin (`FDB_VERSION`/`FDB_IMAGE`/compose/workflow) goes red on a partial bump.
- **`cluster_status_is_healthy`** needle (`fdb_doctor.rs:311`): could not construct a real
  `status minimal` line that flips it wrong; `unavailable`/`configuration missing`/empty all
  correctly fail.

## Noted, not a refutation

- `the_audit_policy_records_what_cargo_deny_cannot_see` asserts only that `deny.toml`'s header
  contains the substrings `libfdb_c` and `release notes` — an inherently weak comment-presence
  test, but the brief pins the audit-policy note to that header block by design, so this is
  scope compliance, not a defect.
- The PR-leg `cargo check -p wyrd-server --features fdb --tests` and the workflow's own hosted
  green are off-Check (deferred by the brief's Verification posture); `crates/server` does
  declare the `fdb` feature (`crates/server/Cargo.toml:31`), so the command is well-formed.
