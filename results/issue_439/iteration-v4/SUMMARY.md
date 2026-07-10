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

Review target: issue 439 adds a FoundationDB CI/dev harness so `fdb-conformance` is invoked, FDB feature arms are type-checked, and local failures get actionable preflight diagnostics.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines a bounded harness slice: doctor module, workflow invocation, FDB feature checks, and audit-policy note; the target change maps to those surfaces at `xtask/src/lib.rs:18`, `.github/workflows/fdb-conformance.yml:36`, and `deny.toml:7`. |
| C2 Reproduction (red pre-fix) | PASS | Reverse-applying `patch.diff` made `cargo test -p xtask --test fdb_harness` fail with no such test target, then reapplying restored it green; this confirms the added harness is the red→green evidence at `xtask/tests/fdb_harness.rs:1`. |
| C3 Change | PASS | The patch covers the specified behavior: dispatches `fdb-conformance`/`fdb-doctor`, gates through the doctor, adds independent FDB feature rows, and adds the workflow/audit note at `xtask/src/main.rs:80`, `xtask/src/main.rs:312`, `xtask/src/lib.rs:72`, `.github/workflows/fdb-conformance.yml:125`, and `deny.toml:15`. |
| C4 Verification (red→green) | PASS | I reran `cargo test -p xtask --test fdb_harness` green, `cargo xtask ci` green, and direct FDB type-checks green; the red side failed as expected without the patch, and the CI wiring is pinned at `xtask/src/main.rs:1404` and `xtask/src/main.rs:1704`. |
| C5 Causal adequacy | NEEDS-HUMAN | The decision owed is whether these Docker/`libfdb_c` capability probes are the intended preflight, not a guard over a removable load-time cause; this matters because the fix intentionally skips locally or fails in CI before the stack at `xtask/src/fdb_doctor.rs:313` and `xtask/src/fdb_doctor.rs:341`. |
| T1 Structure | PASS | The logic/effect split is grounded: pure doctor and feature rows live in the lib target, while `main.rs` only supplies measured effects and dispatch at `xtask/src/lib.rs:18`, `xtask/src/lib.rs:72`, `xtask/src/main.rs:391`, and `xtask/src/main.rs:312`. |
| T2 Shape | PASS | The workflow shape matches the constrained artifact: PR filters include both FDB backend and server paths, xtask invocations are bare command-head `run:` steps, and the harness checks evasion cases at `.github/workflows/fdb-conformance.yml:42`, `.github/workflows/fdb-conformance.yml:134`, `.github/workflows/fdb-conformance.yml:146`, and `xtask/tests/fdb_harness.rs:744`. |
| T3 Runtime | NEEDS-HUMAN | The live FDB topology was not exercised here because Docker daemon access is denied; I observed `cargo xtask fdb-conformance` locally skip with remediation and `cargo xtask fdb-doctor` find `libfdb_c` but no cluster file, so a Docker-enabled runner must confirm the real compose run behind `.github/workflows/fdb-conformance.yml:130`. |
| T4 Contribution | NEEDS-HUMAN | Local affected-path history and grep found no prior `fdb_doctor` or FDB workflow, but closed/rejected PR prior art was not mechanically accessible in this sandbox; the human must clear that external prior-art check because it affects duplicate-work risk for `.github/workflows/fdb-conformance.yml:1` and `xtask/src/fdb_doctor.rs:1`. |
| T5 Judgment | PASS | No source-grounded patch defect surfaced in the rerun gates; remaining risks are explicit human decisions rather than deterministic failures, with the non-required hosted-workflow policy stated at `.github/workflows/fdb-conformance.yml:27`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off is required by design: decide whether container-free evidence plus direct FDB feature type-checks is enough before the first GitHub-hosted FDB workflow green, because that external execution is the user-visible purpose of `.github/workflows/fdb-conformance.yml:134`. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The decision owed is whether these Docker/`libfdb_c` capability probes are the intended preflight, not a guard over a removable load-time cause; this matters because the fix intentionally skips locally or fails in CI before the stack at `xtask/src/fdb_doctor.rs:313` and `xtask/src/fdb_doctor.rs:341`.
- [ ] T3 Runtime — The live FDB topology was not exercised here because Docker daemon access is denied; I observed `cargo xtask fdb-conformance` locally skip with remediation and `cargo xtask fdb-doctor` find `libfdb_c` but no cluster file, so a Docker-enabled runner must confirm the real compose run behind `.github/workflows/fdb-conformance.yml:130`.
- [ ] T4 Contribution — Local affected-path history and grep found no prior `fdb_doctor` or FDB workflow, but closed/rejected PR prior art was not mechanically accessible in this sandbox; the human must clear that external prior-art check because it affects duplicate-work risk for `.github/workflows/fdb-conformance.yml:1` and `xtask/src/fdb_doctor.rs:1`.
- [ ] Validation — fitness-to-purpose — Human sign-off is required by design: decide whether container-free evidence plus direct FDB feature type-checks is enough before the first GitHub-hosted FDB workflow green, because that external execution is the user-visible purpose of `.github/workflows/fdb-conformance.yml:134`.

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
- Iteration delta (if iterating): Rebuild against the same brief — do NOT re-plan. Gating C4-ci is green and the harness (doctor module, workflow, feature-gated checks, audit note) is real and worth keeping — do not churn it. What to fix — the adversary's landed refutation on the one seam this issue has reopened for four iterations: 1. The assertion-5 "impure adapter" guard is SUBSTRING-only, not data-flow. It pins that the adapter body *contains the text* `std::env::var` (fdb_harness.rs:541) and `Path::new(`/`.exists()` (:547) — character checks, not behaviour. Two one-line semantic-equivalent mutations keep all 28 tests green while reintroducing the exact iteration-1 false-negative: - main.rs:393 `&|name| std::env::var(name).ok()` -> `&|name| { let _ = std::env::var(name); None }` (env read discarded, closure always None -> working FDB_CLIENT_LIB_PATH build reported "missing" -> false-skip locally / hard-fail in CI). - main.rs:394 `&|candidate| Path::new(candidate).exists()` -> `&|candidate| { let _ = ...; true }` (library always reported present -> preflight proceeds into the container stack with no libfdb_c -> linker error minutes in, the very thing the preflight exists to prevent). FIX: close the DEFECT CLASS, not the two literal mutation spellings. Add the behavioural live-adapter test the brief already offered (drive the real adapter against a fake environment/filesystem where only the FDB_CLIENT_LIB_PATH dir has libfdb_c, and assert the probe resolves it; and a fake where nothing exists, asserting it reports missing) so a discarded env-read or a hard-coded `true` flips a Check test red. The brief's intent is "the false-negative cannot be reintroduced," not "the two named mutations go red." 2. Minor, fold in while there: fdb_doctor.rs:297 HEALTH_COMMAND is `fdbcli --exec "status minimal"` but the real probe runs `fdbcli -C <file> --exec "status minimal"`. Cosmetic (advisory text), align it. Carried-forward NEEDS-HUMAN that remain open for the rebuild's sign-off: T3 runtime (live FDB topology never exercised — Docker denied here; a Docker-enabled runner must confirm the real compose run), T4 prior-art (closed/rejected PR state not mechanically settled), and fitness-to-purpose (container-free evidence before the first hosted FDB workflow green).
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
