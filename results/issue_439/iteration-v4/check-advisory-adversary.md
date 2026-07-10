# Adversarial review — issue 439 / fdb-dev-ci-harness

Reproduced the red→green at `$PDCA_TARGET` with the real toolchain (cargo 1.96.0):
`cargo test -p xtask --test fdb_harness` → **28 passed** post-fix; the brief's literal
pinned mutation `&|name| std::env::var(name).ok()` → `&|_| None`
(`xtask/src/main.rs:393`) turns `the_client_library_adapter_supplies_the_real_environment_read`
**red**. So the evidence is real and exercises the production path (the doctor unit tests,
`run_gated_conformance`, `feature_gated_checks` and the committed workflow are the same
functions/artifacts production uses — no parallel re-implementation, no mocked-away defect).

## Refutation that landed

- **NEEDS-HUMAN — the assertion-5 "impure adapter" guard is substring-only; it pins the two
  *named* mutation strings but not the *defect class* they stand for, and the exact
  iteration-1 false-negative can be reintroduced with the whole suite green.**
  `xtask/tests/fdb_harness.rs:541` asserts the adapter body merely *contains the text*
  `std::env::var`, and `:547` that it contains `Path::new(` and `.exists()`. These are
  character checks, not data-flow checks. Verified on the target:
  - `xtask/src/main.rs:393` `&|name| std::env::var(name).ok()` →
    `&|name| { let _ = std::env::var(name); None }` — text preserved, env read **discarded**,
    closure always returns `None`. This is iteration-1's exact false-negative (a working
    `FDB_CLIENT_LIB_PATH` build reported "missing" → local false-green skip / CI hard-fail).
    Result: **all 28 tests still pass.**
  - `xtask/src/main.rs:394` `&|candidate| Path::new(candidate).exists()` →
    `&|candidate| { let _ = Path::new(candidate).exists(); true }` — always reports the
    library present, so `preflight` proceeds into the container stack with **no `libfdb_c`**,
    producing the linker-error-minutes-in the preflight exists to prevent. Result: **all 28
    tests still pass.**
  The brief (Success criterion 5, Re-plan §2) accepts the structural body-assertion idiom and
  requires only that the two *named* mutations go red — and they do (verified). So by the
  brief's letter the criterion is met. But iteration 4 existed *specifically* because this seam
  reopened three times, and the guard it shipped closes the two literal mutation spellings
  while a one-line semantic equivalent walks straight through. A human should decide at
  sign-off whether "the named mutation goes red" is the intended bar, or whether the intent was
  "the false-negative cannot be reintroduced" — the two differ here, and only a behavioral
  live-adapter test (which the brief also offered) would have closed the class.

## Attempted and could **not** refute

- **The workflow head-binding (assertion 3).** Tried every non-enumerated evasion I could
  construct beyond the four in the test — `eval cargo xtask fdb-conformance`,
  `sh -c "cargo xtask …"`, `env FOO=bar cargo xtask …`. All have a non-`cargo` head, so
  `xtask_head_subcommand` returns `None` and, were the real step written that way,
  `the_fdb_conformance_workflow_runs_only_real_dispatched_subcommands` goes **red** (fail-safe).
  The scraper genuinely counts only a bare `cargo xtask <sub>` head; no *mention* is counted.
  The three-iteration recurrence (a `windows()` scan counting a mention as an execution) is
  really fixed.
- **`feature_gated_checks(tikv, fdb)` independence (assertions 4).** The two gates are read by
  name from an injected lookup in `run_ci_steps`; the tikv-only / fdb-only / neither / both
  cases are asserted on the real production function and its real call site. No coupling to
  `WYRD_TIKV_TOOLCHAIN` survives.
- **The version-drift and cluster-file-literal guards.** They read the real
  `deploy/fdb-single-node/docker-compose.yml`, `deny.toml`, the workflow, and
  `crates/metadata-fdb/src/lib.rs` — not copies. A partial bump fails.
- **`cargo xtask ci` (gating C4-ci) stays green offline** — with both toolchain env vars unset,
  `feature_gated_checks(false,false)` is empty, so the gate compiles no feature tree. Confirmed
  by the passing suite.

## Minor (not gating, not a defect)

- `xtask/src/fdb_doctor.rs:297` `HEALTH_COMMAND` is `fdbcli --exec "status minimal"`, but the
  real probe at `xtask/src/main.rs` runs `fdbcli -C <file> --exec "status minimal"`. Cosmetic:
  the constant is advisory remediation text, not the invocation. The `status minimal`
  health-needle (`database is available`) correctness is a nightly/live-leg claim I could not
  exercise here (no running cluster) — not refuted, not confirmed.
