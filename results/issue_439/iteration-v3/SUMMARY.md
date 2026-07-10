# Result — issue 439 / fdb-dev-ci-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) exists and works —
- Success criterion: `cargo test -p xtask --test fdb_harness` passes on the plain
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Give the shipped-but-ungated `fdb` backend a standing automated signal and an

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue 439 adds a standing FoundationDB conformance signal, FDB feature type-checks, and an actionable local/CI preflight for the shipped `fdb` metadata backend.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The change stays on the specified harness/preflight/typecheck surfaces and puts the audit-policy note in the pinned policy file, so the human decision is not burdened by scope drift (`deny.toml:7`, `.github/workflows/fdb-conformance.yml:36`, `xtask/src/lib.rs:72`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the patch made `cargo test -p xtask --test fdb_harness` red with no such test target; restoring it made the new binding harness green, so the red is tied to this slice's missing artifact (`xtask/tests/fdb_harness.rs:1`). |
| C3 Change | PASS | The patch creates the missing executable signal and preflight path: workflow triggers and PR typechecks exist, `fdb-conformance` dispatch is gated, and feature rows are exported from the lib target (`.github/workflows/fdb-conformance.yml:125`, `xtask/src/main.rs:80`, `xtask/src/main.rs:309`, `xtask/src/lib.rs:72`). |
| C4 Verification (red→green) | PASS | I reproduced red→green manually, reran `cargo xtask ci` successfully, and directly checked both promised FDB feature rows; the unavailable `engine/` wrappers were not the evidence source (`xtask/tests/fdb_harness.rs:751`, `xtask/tests/fdb_harness.rs:792`, `xtask/src/main.rs:1415`). |
| C5 Causal adequacy | PASS | The fix addresses the actual missing automated signal and preflight rather than just masking one error path: the workflow runs the conformance command, the preflight blocks before stack entry, and the custom-prefix client-library case is tested (`.github/workflows/fdb-conformance.yml:134`, `xtask/src/fdb_doctor.rs:341`, `xtask/tests/fdb_harness.rs:240`). |
| T1 Structure | PASS | The pure decision logic is in the `xtask` lib target while impure environment reads stay in `main.rs`, preserving a plain-worktree test seam with no new dependency decision (`xtask/src/lib.rs:18`, `xtask/src/main.rs:391`, `xtask/tests/fdb_harness.rs:554`). |
| T2 Shape | PASS | Workflow execution, path filters, dispatch, and usage line line up, so a PR touching either FDB backend or server FDB arms gets the commands the workflow promises (`.github/workflows/fdb-conformance.yml:42`, `.github/workflows/fdb-conformance.yml:127`, `xtask/src/main.rs:80`, `xtask/src/main.rs:112`). |
| T3 Runtime | NEEDS-HUMAN | Maintainer must confirm the full Docker-backed FDB conformance run on a host with Docker socket access; here `libfdb_c` and `fdbcli` were present, but Docker daemon access was denied and `cargo xtask fdb-conformance` only exercised the local skip path (`xtask/src/fdb_doctor.rs:341`, `xtask/src/main.rs:323`). |
| T4 Contribution | PASS | The contribution avoids the forbidden docs/design overlap and records the external `libfdb_c` audit split in `deny.toml`, so the integration risk is policy visibility rather than an untracked dependency change (`deny.toml:7`, `deny.toml:15`). |
| T5 Judgment | NEEDS-HUMAN | Human must clear closed/rejected PR prior art: merged history by affected paths showed no existing FDB workflow/doctor/harness, but the artifact-only local repo cannot mechanically prove closed/rejected work state (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the hosted PR/nightly workflow green and the live FDB stack run satisfy production-signature intent, because those external outcomes were not observable in this sandbox (`.github/workflows/fdb-conformance.yml:36`, `.github/workflows/fdb-conformance.yml:134`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Maintainer must confirm the full Docker-backed FDB conformance run on a host with Docker socket access; here `libfdb_c` and `fdbcli` were present, but Docker daemon access was denied and `cargo xtask fdb-conformance` only exercised the local skip path (`xtask/src/fdb_doctor.rs:341`, `xtask/src/main.rs:323`).
- [ ] T5 Judgment — Human must clear closed/rejected PR prior art: merged history by affected paths showed no existing FDB workflow/doctor/harness, but the artifact-only local repo cannot mechanically prove closed/rejected work state (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the hosted PR/nightly workflow green and the live FDB stack run satisfy production-signature intent, because those external outcomes were not observable in this sandbox (`.github/workflows/fdb-conformance.yml:36`, `.github/workflows/fdb-conformance.yml:134`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected to re-plan, not to rebuild against the same brief: three iterations have now reproduced the SAME defect class the adversarial pass keeps landing — the workflow↔dispatch guard binds a *mention* of a subcommand, not its *execution* (iter-3: `run: echo would run cargo xtask fdb-conformance` leaves `the_fdb_conformance_workflow_executes_only_real_subcommands` green while the job runs no xtask command). Iterations 1 and 2 named the correct fix ("require the token be the command head, not any window"); Do repeatedly chose a narrower comment-stripping variant instead. That the same hole survives three headless rebuilds is a signal the *brief* is steering Do toward a scrape-based approach that is structurally prone to this class, rather than pinning the binding property the reviewer keeps asking for. For the re-plan, decide at Plan time (not Do time): 1. Specify the workflow↔execution assertion by its binding property — the token must appear as the command HEAD of a real `run:` step — and require the demonstrated-red to cover the mention-as-argument shape, so the hole cannot pass. Consider whether "scrape the workflow text" is the right mechanism at all vs. a stronger structural check. 2. Adjudicate the second adversarial finding: the impure preflight/probe adapter (`main.rs:393` env read, `run_fdb_conformance` call site) is Check-unguarded — mutating it reintroduces the iter-1 false-negative with all tests green. Decide in the brief whether that thin wiring warrants a Check guard or is acceptable untested wiring over a tested pure core; do not leave it for Do to re-decide a third time. The primary reviewer PASSed all rows again; trust the mutations, not the PASS row. §6 (T3 live Docker run, T5 prior art, fitness-to-purpose) was not reached — left unresolved, superseded by the re-plan.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
