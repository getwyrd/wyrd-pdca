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

Issue 439 review: add an automated FoundationDB conformance signal, FDB feature type-check coverage, and an actionable local FDB preflight without making the ordinary gate depend on Docker or libfdb_c.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The reviewed task is bounded to FDB harness coverage and preflight: the patch exposes `xtask::fdb_doctor`, wires `fdb-conformance`/`fdb-doctor`, adds FDB feature checks, and leaves docs/design untouched (`xtask/src/lib.rs:18`, `xtask/src/main.rs:80`, `xtask/src/main.rs:1406`). |
| C2 Reproduction (red pre-fix) | PASS | Clean `HEAD` archive reproduced red for the binding test (`cargo test -p xtask --test fdb_harness` -> no such test target), while the patched test contains planted-red checks for doctor rows, workflow scraping, and preflight bypass (`xtask/tests/fdb_harness.rs:241`, `xtask/tests/fdb_harness.rs:567`, `xtask/tests/fdb_harness.rs:426`). |
| C3 Change | PASS | The change directly covers the three issue gaps: workflow PR/nightly execution and PR FDB type-check rows (`.github/workflows/fdb-conformance.yml:36`, `.github/workflows/fdb-conformance.yml:125`), preflighted conformance (`xtask/src/main.rs:309`), and audit-policy text for libfdb_c visibility (`deny.toml:7`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Binding red→green and `cargo xtask ci` were rerun green, but this sandbox cannot access Docker (`docker info` permission denied), so the live `cargo xtask fdb-conformance` stack and hosted workflow execution remain unexercised external dependencies (`xtask/src/fdb_doctor.rs:341`, `.github/workflows/fdb-conformance.yml:134`). |
| C5 Causal adequacy | PASS | The fix removes the ungated-signal cause rather than only masking symptoms: CI executes conformance and both FDB feature checks, and the local skip/fail decision is isolated to the explicit environment preflight (`.github/workflows/fdb-conformance.yml:127`, `.github/workflows/fdb-conformance.yml:135`, `xtask/src/fdb_doctor.rs:313`). |
| T1 Structure | PASS | The pure decision logic is in the lib target and the impure probing stays in the binary, matching the testable seam the brief required (`xtask/src/lib.rs:18`, `xtask/src/fdb_doctor.rs:276`, `xtask/src/main.rs:385`). |
| T2 Shape | PASS | Workflow commands are actual `run:` commands and match real dispatch/type-check rows, so prose-only mentions and nonexistent subcommands are guarded (`xtask/tests/fdb_harness.rs:467`, `xtask/tests/fdb_harness.rs:611`, `xtask/tests/fdb_harness.rs:651`). |
| T3 Runtime | NEEDS-HUMAN | Local runtime behavior was observed only up to preflight: `fdb-doctor` reports missing cluster file/health with remediation and `fdb-conformance` skips locally because Docker is unavailable, so a privileged runner must confirm the compose stack actually runs (`xtask/src/main.rs:323`, `xtask/src/fdb_doctor.rs:347`). |
| T4 Contribution | NEEDS-HUMAN | Local merged-history checks by affected path found no prior FDB workflow and no doctor commits, but closed/rejected PR history is not mechanically available in this sandbox; human must clear the non-local prior-art check (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`). |
| T5 Judgment | PASS | The patch preserves the ordinary unprivileged gate while adding a separate privileged signal; no scope creep into docs/design or new Cargo dependencies was observed (`xtask/src/lib.rs:72`, `.github/workflows/fdb-conformance.yml:27`, `deny.toml:29`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the non-required GitHub workflow plus off-band libfdb_c release-note tracking is sufficient production coverage, because Check cannot observe the first hosted PR/nightly run or upstream advisory process (`.github/workflows/fdb-conformance.yml:144`, `deny.toml:15`). |

### Advisory — adversary

# Adversarial review — issue 439 / fdb-dev-ci-harness (iteration 2)

Method: re-ran `cargo test -p xtask --test fdb_harness` on `$PDCA_TARGET` (24 green) and
`cargo test -p xtask --bin xtask` (green), then mutated production code / the workflow to
see whether the four carry-forward fixes actually *bind*. Toolchain present (cargo 1.96);
red→green reproducible here.

## Findings

- **NEEDS-HUMAN — carry-forward fix #1 is incomplete: `xtask_subcommands` still counts a
  *mention* as an execution when the mention is a trailing inline `#` comment on a `run:`
  line** (`xtask/tests/fdb_harness.rs:467` `run_script_lines`, `:510` `xtask_subcommands`).
  `run_script_lines` strips only lines that *start* with `#` (`:480`, `:488`); a trailing
  shell comment survives, and `xtask_subcommands` (`:516`) then tokenises it as a real
  command. Concrete, demonstrated case: I replaced the conformance step with
  `run: echo DISABLED # cargo xtask fdb-conformance` — the job now runs `echo DISABLED` and
  invokes **no** xtask command, yet
  `the_fdb_conformance_workflow_executes_only_real_subcommands` stayed **green**. That is
  exactly the "bind execution rather than mention" guarantee the iteration-1 rejection asked
  for, and the module doc at `fdb_harness.rs:1649-1651`/`:1643` claims ("scraping … from the
  raw file text would let a workflow that *mentions* … satisfy the dispatch contract");
  the demonstrated-red test `run_script_scraping_ignores_prose…` only exercises **full-line**
  comments, never a trailing one, so the hole is not covered. The shipped workflow is clean
  today (no live false-positive), but a future edit like
  `run: cargo xtask ci # was cargo xtask fdb-conformance` would let a reviewer believe the
  conformance job runs when it does not — the very regression #1 was reopened to prevent.
  Fix: strip inline `#…` before tokenising (respecting quotes), or require the token be the
  command head, not any window.

- **NEEDS-HUMAN — the production reading of `FDB_CLIENT_LIB_PATH` (carry-forward fix #3) is
  not guarded at Check** (`xtask/src/main.rs:386`, `probe_client_library`). The *pure*
  search-path logic is covered (`client_library_search_paths`, asserted by
  `the_client_library_search_honours_fdb_client_lib_path`), but the impure wiring that reads
  the env var and feeds it in is not. Mutation: change `main.rs:386` to
  `let configured: Option<String> = None;` — reintroducing the exact false-negative the
  iteration-1 rejection described (a `/opt/foundationdb/lib` client with `FDB_CLIENT_LIB_PATH`
  set, no ld.so entry: links fine, probes Failed → local false-green skip / CI hard-fail) —
  and **all** tests stay green (`--test fdb_harness` 24 passed, `--bin xtask` 19 passed).
  The shipped `main.rs:386` is correct; this is a coverage gap, and the brief does disclose
  the probe impure half as "exercised for real by the nightly job." Flagged because it is the
  precise defect class that was rejected once and can silently regress with no Check signal.

- The fdb↔tikv **coupling** guard (fix #2) binds — but only in the **bin** unit test, not in
  the file the brief names as the success criterion. Mutating `main.rs:1408` so the fdb
  argument reads `xtask::TIKV_TOOLCHAIN_ENV` leaves `cargo test -p xtask --test fdb_harness`
  **fully green** (24 passed); it is caught only by
  `tests::ci_type_checks_the_fdb_feature_on_the_fdb_toolchain_alone` under
  `cargo test --bin xtask`. `cargo xtask ci` runs both, so the merge gate does catch it —
  informational, not a hole, but the brief's stated `--test fdb_harness` command alone does
  not exercise the call-site coupling.

- Minor: `the_conformance_command_delegates_to_the_gate_and_is_red_when_it_does_not`
  (`fdb_harness.rs:~1611`) verifies delegation by substring only; the *argument order* at
  `main.rs:312` (`run_gated_conformance(docker_available(), is_ci(), probe_client_library(), …)`)
  is unverified — a swapped `docker_available()`/`is_ci()` would flip CI-vs-local behaviour
  and no Check test would catch it (impure, `run_fdb_conformance` never executed in tests).

## Attempted and could NOT refute

- **`cargo check -p wyrd-server --features fdb` / `-p wyrd-metadata-fdb --features fdb` are
  valid.** Suspected the workflow/nightly gate would run an invalid-feature command (a new
  "filter promising a check it can't perform"). Both features exist and forward correctly
  (`crates/server/Cargo.toml` `fdb = ["dep:wyrd-metadata-fdb", "wyrd-metadata-fdb/fdb"]`;
  `crates/metadata-fdb/Cargo.toml` `fdb = [...]`). Refuted.
- **Fix #4 (PR-leg type-checks the server fdb arms) is solid.** Both mutations go red:
  dropping the `cargo check -p wyrd-server --features fdb --tests` line, and adding
  `if: github.event_name != 'pull_request'` to the type-check step, each fail
  `the_pull_request_leg_type_checks_every_fdb_feature_arm_it_filters_on`. The rows match
  `feature_gated_checks(false, true)` verbatim and the `crates/server/**` filter is asserted.
- Doctor pure logic, planted-red (`doctor_is_red_when_a_failing_probe_outcome_is_planted`),
  the `available`/`unavailable` needle, cluster-file fallback, and the version drift guards
  all bind under mutation. No tautology found in group (1),(2),(5).

Net: the fix largely holds and fixes #4/#2/#5 bind under mutation. The residual concerns are
fix #1 being incomplete for inline comments (a reopened-regression class) and fix #3's
production wiring being Check-unguarded — both advisory; the human decides at sign-off.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Binding red→green and `cargo xtask ci` were rerun green, but this sandbox cannot access Docker (`docker info` permission denied), so the live `cargo xtask fdb-conformance` stack and hosted workflow execution remain unexercised external dependencies (`xtask/src/fdb_doctor.rs:341`, `.github/workflows/fdb-conformance.yml:134`).
- [ ] T3 Runtime — Local runtime behavior was observed only up to preflight: `fdb-doctor` reports missing cluster file/health with remediation and `fdb-conformance` skips locally because Docker is unavailable, so a privileged runner must confirm the compose stack actually runs (`xtask/src/main.rs:323`, `xtask/src/fdb_doctor.rs:347`).
- [ ] T4 Contribution — Local merged-history checks by affected path found no prior FDB workflow and no doctor commits, but closed/rejected PR history is not mechanically available in this sandbox; human must clear the non-local prior-art check (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`).
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether the non-required GitHub workflow plus off-band libfdb_c release-note tracking is sufficient production coverage, because Check cannot observe the first hosted PR/nightly run or upstream advisory process (`.github/workflows/fdb-conformance.yml:144`, `deny.toml:15`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the adversarial reviewer's mutation findings — the same class of issue that reopened iteration 1. Rebuild against the same brief; do NOT re-plan. Trust the mutations, not the primary PASS row. What to fix: 1. Carry-forward fix #1 is still incomplete: the workflow<->dispatch guard counts a *mention* as an execution when it is a TRAILING inline `#` comment on a `run:` line. `run_script_lines` (xtask/tests/fdb_harness.rs:467) strips only lines that START with `#`; `xtask_subcommands` (:510) then tokenises the surviving trailing comment as a real command. Demonstrated: replacing the conformance step with `run: echo DISABLED # cargo xtask fdb-conformance` leaves `the_fdb_conformance_workflow_executes_only_real_subcommands` GREEN while the job invokes no xtask command. The demonstrated-red (`run_script_scraping_ignores_prose…`) exercises only full-line comments, so the hole is uncovered. FIX: strip inline `#…` before tokenising (respecting quotes), or require the token be the command head, not any window — and extend the demonstrated-red to the trailing-comment shape. 2. Fix #3's production wiring is Check-unguarded. The pure search-path logic (`client_library_search_paths`) is covered, but the impure read of FDB_CLIENT_LIB_PATH at xtask/src/main.rs:386 is not. Mutating it to `let configured: Option<String> = None;` reintroduces the exact false-negative iteration 1 was rejected for (false-green skip locally / hard-fail in CI) and ALL tests stay green. Add a Check-level guard on that wiring so the rejected defect class cannot silently regress. What survived and must NOT be churned: fixes #2, #4, #5 bind under mutation (the fdb/tikv toolchain coupling, the PR-leg server fdb type-check, the doctor pure logic / planted-red / drift guards). Keep them as-is.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
