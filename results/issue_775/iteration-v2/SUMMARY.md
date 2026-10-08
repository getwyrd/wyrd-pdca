# Result — issue 775 / xtask-blackbox-dependency-closure-guard

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) Nothing mechanically prevents `wyrd-validate` from linking
  a Wyrd workspace crate. `xtask/src/repo_guard.rs` carries exactly two invariants on `main`
  @ `65ca4fd` — the stray-gitlink scan (`scan_gitlinks`, `:238`) and the
  `#![forbid(unsafe_code)]` crate-root scan (`scan_roots`, `:500`); `grep -n "wyrd-"
  xtask/src/repo_guard.rs` shows the file never mentions a dependency closure. The moment the
  validator's binary links a `wyrd-*` crate it is testing Wyrd's types against Wyrd's types
  and its verdict is self-referential — the property the whole tool rests on, held by nothing
  but habit.
- Success criterion: BINDING, every leg inside `cargo xtask ci`:
  1. **The guard RUNS INSIDE the gate — asserted, not assumed.** Its registration lives in
     the `xtask` **lib** target as data, and a test asserts the blackbox guard is in the list
     `run_ci` executes. Without this, a guard that is defined, tested and never called passes
     every other criterion here. The repo has already engineered against exactly this hazard:
     the feature-gated check list "lives in `xtask::feature_gated_checks` (the lib target) so
     `xtask/tests/fdb_harness.rs` can assert its content directly"
     (`xtask/src/main.rs:1504-1505`), and `run_ci_steps` injects `exec` "so the real wiring is
     exercised without spawning cargo" (`:1486-1498`). Note the shape of the problem:
     `run_gitlink_guard` (`main.rs:1373`) and `run_unsafe_forbid_guard` (`main.rs:1443`) are
     `fn`s in the BINARY target, so no integration test can see whether `run_ci` calls them.
     Do not extend that.
  2. **The guard is flippable, not vacuous.** Fed a synthetic `cargo metadata` document, the
     pure scan function returns: exactly one violation naming the crate when `wyrd-validate`
     has a **normal** dependency on a `wyrd-*` crate; **none** when that same edge is marked
     `"kind": "dev"`; the violation when the `wyrd-*` edge is a normal dependency that is
     **`optional` and off by default**; and the violation when the `wyrd-*` crate is reached
     **transitively**, through at least one intermediate hop, with the violation naming the
     path rather than only the crate. Run over the REAL workspace metadata it returns none.
  3. **The guard fails closed.** An unparsable document, an absent `resolve` section, a
     `package` that appears in no node, or a reached package record whose identity cannot be
     decoded (missing `id` **or** missing `name`) is an `Err` — never a vacuously clean pass,
     and never an opaque-package-id fallback that lets the `wyrd-` prefix check silently miss.
     This is the exact defect three independent review passes found in iteration v3
     (`repo_guard.rs:729`, `:730`, `:751`) and it is promoted to a binding criterion here so
     it is built rather than rediscovered. Mirror `scan_roots`' stated posture of "refusing to
     pass a workspace it cannot see" (`repo_guard.rs:505-510`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: a third invariant in `xtask/src/repo_guard.rs` — the `wyrd-validate` package's
  normal dependency closure, plus its declared normal (including optional) dependencies,
  contain no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can
  read, with its flippable test.
  **/ out of scope:** anything under `crates/validate/**` (that is child-2 — this child adds
  no crate code and no test there); extending the guard to any package other than
  `wyrd-validate`; changing the two existing invariants' behaviour (refactoring their
  registration into the shared lib-side list is in scope and expected, but their verdicts must
  not change); constraining dev-dependencies, which are deliberately unconstrained — #741 and
  proposal 0017 §14's fixtures rely on that.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (27 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.77s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #775’s CI enforcement of `wyrd-validate` dependency isolation: the scanner passes independent checks, but architecture documentation and a regression for the production CI dispatcher need work.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is falsifiable: CI registration, normal/optional/transitive isolation, dev-edge allowance and malformed-metadata refusal are explicit (`brief.md:12`, `brief.md:24`, `brief.md:32`). |
| C2 Reproduction (red pre-fix) | PASS | Retaining the new tests while stashing production changes produces 27 assertion failures, not a compile failure (`reviewer-red.log:39`, `reviewer-red.log:283`). |
| C3 Change | PASS | The three changed files serve the specified guard and its executable tests; the validator crate and existing scanner implementations are outside the diff (`patch.diff:1`, `xtask/src/repo_guard.rs:606`, `xtask/src/main.rs:1607`). |
| C4 Verification (red→green) | PASS | Restoration passes all 110 focused tests; independent fmt, clippy, build and LLVM coverage succeed; full workspace CI is supported by frozen evidence rather than an independent full rerun (`reviewer-restored-green.log:31`, `reviewer-restored-green.log:64`, `reviewer-restored-green.log:100`, `reviewer-restored-green.log:135`, `gate-logs/C4-ci.log:3853`). |
| C5 Causal adequacy | PASS | The scan addresses dependency isolation across normal paths and optional declarations; malformed input is refused, and the previous empty/unknown-kind cases now pass their regressions; no capability-probe symptom workaround is introduced (`xtask/src/repo_guard.rs:695`, `xtask/src/repo_guard.rs:800`, `xtask/src/repo_guard.rs:851`, `xtask/tests/blackbox_dependency_guard.rs:652`, `xtask/tests/blackbox_dependency_guard.rs:671`). |
| T1 Structure | PASS | Lib-side registration and a pure scanner preserve the existing dependency direction and permit isolated checks; actual dispatch coverage remains the separate T5 concern (`xtask/src/repo_guard.rs:606`, `xtask/src/repo_guard.rs:747`, `xtask/src/main.rs:1598`). |
| T2 Shape | PASS | The change introduces no crate, dependency or workflow-filter surface; the new test root forbids unsafe code and formatting passes (`xtask/tests/blackbox_dependency_guard.rs:25`, `patch.diff:1`, `reviewer-checks.log:1`). |
| T3 Runtime | PASS | Real workspace resolution and scanning succeed in 0.293 seconds; optional, renamed and dev-edge fixtures use real Cargo, and the lockfile-preservation test passes (`reviewer-runtime.log:2`, `xtask/src/repo_guard.rs:629`, `xtask/tests/blackbox_dependency_guard.rs:474`, `xtask/tests/blackbox_dependency_guard.rs:524`). |
| T4 Contribution | FAIL | Document the new CLI flags in the living architecture in this PR — omitting that update violates the standing docs-currency requirement (`xtask/src/main.rs:1562`, `AGENTS.md:154`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Exercise the production CI dispatcher with a forbidden dependency — replacing `run_ci_guard` with unconditional success survives every xtask test, leaving CI able to skip enforcement without the claimed regression detecting it (`xtask/src/main.rs:1505`, `xtask/tests/blackbox_dependency_guard.rs:221`, `reviewer-mutants-capped.log:5`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept normal-dependency isolation, with development dependencies unconstrained, as sufficient for the validator’s intended independence — the automated results establish the mechanics, while this policy’s adequacy needs owner sign-off (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559`). |

All source citations above and below resolve against the supplied `target/` (`$PDCA_TARGET`); evidence citations resolve in this review directory. The target contains prerequisite #774’s `wyrd-validate` crate. No target-state mismatch prevented this review.

Two actionable findings remain:

- **The architecture update is missing.** `blackbox-guard` exposes `--workspace` and `--metadata` at `xtask/src/main.rs:1562`, but the patch changes only two Rust implementation files and one test file. Module comments do not discharge `AGENTS.md:154`. Add the command’s purpose and options to the living architecture, for example its build/CI discussion at `docs/design/architecture/08-crosscutting-concepts.md:114`. The frozen batch log’s three entries describe this same finding; count it once.
- **The wiring regression stops before production dispatch.** The shared loop is tested, but `ci-plan` supplies printing callbacks (`xtask/src/main.rs:1682`), and the unit test supplies recording callbacks. Neither executes `run_ci_guard`, the callback actually supplied by `run_ci` (`xtask/src/main.rs:1659`). Independent mutation testing compiled a version whose new dispatcher does nothing and ran the entire xtask suite successfully. Add coverage of the actual dispatch path that fails when the blackbox guard is skipped or its error is swallowed, using a planted forbidden closure. This is a test-adequacy defect, not a claim that the current dispatcher fails: the frozen CI log shows the current guard executing (`gate-logs/C4-ci.log:27`).

The independent checks support the scanner’s behavior, with these limits:

- **Red→green reproduced.** `git stash push` covered only the two production files, leaving the added test present. `cargo test --locked --offline -p xtask --test blackbox_dependency_guard` failed all 27 tests on the base. After `git stash pop`, running `--bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness` passed 24 + 27 + 29 + 30 tests. Post-mutation source hashes match the original patch, and the reviewer stash is gone (`reviewer-integrity.log:1`).
- **Coverage measured.** `cargo llvm-cov test --locked --offline -p xtask --bins --test blackbox_dependency_guard --lcov --output-path ../reviewer-coverage.lcov` passes all 51 selected tests. Added executable lines: scanner 174/174, main 69/81. Uncovered main lines include production dispatch and the `run_ci` call, consistent with T5 (`reviewer-diff-coverage.log:2`). The frozen coverage run’s missing `wyrd-validate` subject is a prerequisite/checkout caveat, not a scanner defect (`gate-logs/C4-diff-cov.log:54`).
- **Mutation testing completed independently.** The frozen run stopped in the existing Git-index test before trying any mutant (`gate-logs/C5-mutants.log:420`); it proves neither survivors nor their absence. Running in the supplied self-contained checkout fixes that host limitation. The first sweep produced 23 caught, 16 unviable and one missed mutant; the unviable cases were rejected by warnings-as-errors. Repeating with `--cap-lints true` tested all 40: 37 caught, three missed (`reviewer-mutants-capped.log:43`). The dispatcher survivor is T5; replacing all of `run_ci` with success exposes the same boundary and is not a second finding. The remaining survivor weakens unknown-option rejection. It does not bypass dependency scanning and is recorded as a minor test gap, not a separate defect; the unmodified binary rejects an unknown option with exit 1 (`reviewer-runtime.log:17`).
- **Other frozen gates adjudicated.** Full workspace CI reports successful fmt/clippy/build/tests, dependency audits, conformance and DST (`gate-logs/C4-ci.log:30`, `gate-logs/C4-ci.log:3224`, `gate-logs/C4-ci.log:3853`). The TiKV log shows both requested clippy builds completing (`gate-logs/host-tikv.log:7`, `gate-logs/host-tikv.log:209`). These harness-scoped commands were not independently repeated in full. **T4 contribution artifacts: N/A** — the deferred gate explicitly owes its substantive audit to publish, when the artifacts exist (`gate-logs/T4-contribution.log:10`). No gate log is missing. Real Cargo, compilation, real workspace metadata and fixtures capable of exhibiting forbidden edges were exercised; no unmet external dependency is being substituted with code-read or a shim.

Prior art was checked by all three affected paths. GitHub’s main history returned 41 commits for `xtask/src/main.rs`, 17 for `xtask/src/repo_guard.rs` and none for the new test; the existing guard history concerns #616. Of 356 closed PRs, all 19 unmerged PRs had their complete changed-file lists checked, with no affected-path match (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:43`, `reviewer-prior-art.log:61`). No competing or rejected dependency-closure implementation was identified. No `INTEGRATION.md` was present in the supplied target; the supplied standing rubric was applied.

### Advisory — adversary

# Adversarial review — issue #775 (blackbox dependency-closure guard)

Verdict: the scan itself held up. Most attacks failed (listed at the end). One real
test gap survives in the CI wiring that criterion 1 exists to pin. Two scope questions
and three gate reds need a human call.

## Findings

- NEEDS-HUMAN [impl] — **Criterion 1 is still beatable one hop further out.** At
  `xtask/src/main.rs:1510` I replaced `CiGuard::BlackboxClosure => run_blackbox_guard(&workspace_root())`
  with `CiGuard::BlackboxClosure => Ok(())` and ran
  `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`:
  24 + 27 + 30 + 29 passed, 0 failed. After that change `cargo xtask ci` never runs the
  #775 guard. The guard is still defined, and still tested through the separate
  `blackbox-guard` subcommand (`main.rs:1559`). That is the "defined, tested, never called"
  hazard the brief names. The unit test at `main.rs:2114` and the `ci-plan` test
  (`xtask/tests/blackbox_dependency_guard.rs:221`) both swap in their own recording/printing
  executor, so neither reaches `run_ci_guard`. The "exhaustive match" defence in the
  `run_ci_guard` doc comment only proves that an arm exists, not that it calls the guard.
  A second variant also survives: `run_ci` passing a no-op executor at `main.rs:1659`
  (`&mut |g| if g.name().is_empty() { run_ci_guard(g) } else { Ok(()) }`) stays green too.
  That one has the same limit as the existing `cargo` executor, and I don't think it needs
  fixing. The arm-level gap is cheap to close: give `run_ci_guard` the root directory as a
  parameter and add a bin unit test that calls `run_ci_guard(CiGuard::BlackboxClosure, <planted violating workspace>)`
  and asserts `Err`. (Today the gate does run it: `gate-logs/C4-ci.log:27-28`.)

- NEEDS-HUMAN [human] — **Build-dependencies are left open, which contradicts the stated
  invariant.** `xtask/src/repo_guard.rs:658-665` treats `"build"` as not normal, on the
  grounds that it "neither links into the shipped binary". `xtask/tests/blackbox_dependency_guard.rs:289`
  locks that in as allowed. Concrete case: `wyrd-validate` adds
  `[build-dependencies] wyrd-proto = { workspace = true }` plus a `build.rs` that writes Wyrd
  types or constants into `OUT_DIR`, which the binary then `include!`s. The guard passes, yet
  the binary ships Wyrd-derived types, which is exactly what proposal 0017 §9 forbids
  ("Nothing that ships in the binary may reach a workspace crate",
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md:566`). The rationale is also
  applied unevenly: a normal edge to a proc-macro crate *is* walked, though a proc-macro's
  own dependencies never link in either. Flagging `wyrd-*` build edges would cost nothing
  for §14, since the fixtures are dev-only. The brief explicitly said "follow only
  `kind: null` edges", so whether to widen this is a scope decision, not a build defect.

- NEEDS-HUMAN [human] — **The match is by name prefix, not workspace membership.**
  `xtask/src/repo_guard.rs:800` and `:859` only flag names starting with `wyrd-`
  (`:617`). §9's rationale says "workspace crate" (`0017-…md:566`). Concrete case: a new
  member `crates/s3-codes` with `name = "s3-codes"`, shared by `wyrd-gateway-s3` and
  `wyrd-validate`, passes the guard, and that is the self-referential verdict the guard
  exists to stop. No check I found forces the `wyrd-` prefix on members (`xtask` itself is
  unprefixed; today it is caught only because it reaches `wyrd-chunk-format`). Adding
  "any reached id listed in `workspace_members` other than the subject" would be a cheap
  second check. The brief specified the prefix, so this is a judgment call.

- NEEDS-HUMAN [human] — **The T4 gate failure (3 "docs currency" blockers) looks like a
  false positive. A human must record the rejection.** `gate-logs/T4-batch-review.log`
  blocks on `main.rs:120` / `:1562` because the new `blackbox-guard --workspace/--metadata`
  and `ci-plan` subcommands have no living-architecture update. But the architecture doc
  says on purpose that it does not repeat the CI pipeline: "the authoritative pipeline
  definition, deliberately not restated here so it cannot drift"
  (`docs/design/architecture/08-crosscutting-concepts.md:116`). The `xtask` subcommands are
  listed in the module doc the patch already updates (`xtask/src/main.rs:5-20`). I read the
  rubric's "CLI flag" as the product CLI, not developer tooling. If the human agrees, record
  the rejection with that citation. If not, the fix is a single line in `08-…md`.

- NEEDS-HUMAN [human] — **The C5 and C4-diff-cov reds are environment faults, not fix
  defects. Separately, merge order matters.** C5 died on its baseline because the mutants
  copy had no `.git`. The failing test is the pre-existing `scan_gitlinks_is_green_over_the_real_index`
  (`gate-logs/C5-mutants.log:414-436`), not this patch's test. I re-ran
  `cargo mutants --in-place --in-diff patch.diff -p xtask` in a git-backed scratch copy:
  40 mutants, 23 caught, 1 missed, 16 unviable. All 16 unviable ones failed only because the
  workspace's deny-warnings lint rejects unused-variable and dead-code mutants. I rebuilt the
  important ones by hand (see below) and the tests killed them. C4-diff-cov failed because
  its worktree had no `wyrd-validate` member (`gate-logs/C4-diff-cov.log`: "cargo metadata
  has no `wyrd-validate` package"). That is the guard correctly refusing to pass without
  its subject (criterion 3). It also shows that if this lands on any `main` without
  `crates/validate` as a member (#774), `cargo xtask ci` goes red for everyone at the first
  guard. Merge #774 first.

- Minor, informational (not routed): the one missed mutant is `main.rs:1563`
  `flag == "--metadata"` → `true`. With that change, `blackbox-guard --typo FILE` scans the
  file instead of printing usage. This only touches the manual-use path, not the gate.

## Red→green evidence

- The red in `gate-logs/C4-verify.log:18-44` is genuine but tells us little: all 27 tests
  fail with the same `xtask: unknown task 'blackbox-guard'` (e.g. `:52`). It proves the
  subcommand was absent, not that any one assertion catches a wrong scan. That is why I
  ran mutation testing (above and below) instead of trusting the per-test reds. The tests
  do drive the production path. `--metadata FILE` runs the real `scan_blackbox_closure`,
  and `--workspace DIR` also runs the real `cargo metadata --locked --all-features` call.
  I found no parallel re-implementation of the scan.

## Refutation attempts that failed (hand mutants, each run against all four xtask test targets)

- Drop `--all-features` (`repo_guard.rs:634`) → killed by `an_optional_crate_behind_an_off_by_default_feature_is_walked`.
- Drop `--locked` (`:633`) → killed by `the_guard_never_writes_the_lock_file_it_audits`.
- Count `"build"` as normal (`:665`) → killed. Make the manifest check ignore `kind` (`:800`) → killed by 4 tests.
- Stop classifying `dep_kinds` after the first normal entry (`:704`) → killed by `an_edge_of_unknown_kind_is_refused`.
- Accept an empty `dep_kinds` (`:695`, the iteration-1 finding) → killed by `an_edge_with_empty_dep_kinds_is_refused`.
- Mark `seen` before classifying the edge (`:851`) → killed by `a_crate_first_met_on_a_dev_edge_is_still_walked_on_a_normal_path`.
- Fall back to the opaque id when a package record is missing (`:854-856`) → killed by `structurally_incomplete_documents_are_refused`.
- Record the path only for direct dependencies (`:859`) → killed by 4 tests.
- Delete the guard loop or move it after the cargo steps (`main.rs:1607`) → killed by `ci_runs_every_repo_guard_before_the_cargo_steps`. Deleting the `run_ci_steps` call in `run_ci` no longer compiles (dead code), so the iteration-1 "delete the call, stay green" finding is fixed at that level.
- Also checked and found sound: renamed dependencies (names come from package records, and a real-cargo test exists), target-specific `[target.'cfg(..)'.dependencies]` (still `kind: null`, so caught), and `--locked` tripping local runs (`cargo xtask` is `cargo run` without `--locked`, `.cargo/config.toml`, so the lock is refreshed before the guard runs).

### Advisory — code-review

- NEEDS-HUMAN [impl] — `xtask/src/main.rs:1562`: The new `blackbox-guard --workspace` and `--metadata` CLI flags have no corresponding living architecture documentation update; this diff changes only Rust source and tests. The rubric explicitly requires that update in the same PR. Document the new command and flags, its normal-dependency/all-features policy, and its CI integration under `docs/design/architecture/`. The source comments do not satisfy that requirement. This corroborates the frozen T4 review's repeated reports of the same omission; it is one finding.

No further introduced correctness bugs or actionable reuse, simplification, or efficiency findings identified. Reviewed the target source and frozen evidence without modifying the target or rerunning gates. C4 CI and all 27 new integration tests passed; mutation testing stopped on an unchanged Git-index test, and coverage was unavailable because its checkout lacked the guard's subject package.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Exercise the production CI dispatcher with a forbidden dependency — replacing `run_ci_guard` with unconditional success survives every xtask test, leaving CI able to skip enforcement without the claimed regression detecting it (`xtask/src/main.rs:1505`, `xtask/tests/blackbox_dependency_guard.rs:221`, `reviewer-mutants-capped.log:5`).
- [ ] Validation — fitness-to-purpose — Accept normal-dependency isolation, with development dependencies unconstrained, as sufficient for the validator’s intended independence — the automated results establish the mechanics, while this policy’s adequacy needs owner sign-off (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559`).
- [ ] **Criterion 1 is still beatable one hop further out.** At `xtask/src/main.rs:1510` I replaced `CiGuard::BlackboxClosure => run_blackbox_guard(&workspace_root())` with `CiGuard::BlackboxClosure => Ok(())` and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`: 24 + 27 + 30 + 29 passed, 0 failed. After that change `cargo xtask ci` never runs the
- [ ] **Build-dependencies are left open, which contradicts the stated invariant.** `xtask/src/repo_guard.rs:658-665` treats `"build"` as not normal, on the grounds that it "neither links into the shipped binary". `xtask/tests/blackbox_dependency_guard.rs:289` locks that in as allowed. Concrete case: `wyrd-validate` adds `[build-dependencies] wyrd-proto = { workspace = true }` plus a `build.rs` that writes Wyrd types or constants into `OUT_DIR`, which the binary then `include!`s. The guard passes, yet the binary ships Wyrd-derived types, which is exactly what proposal 0017 §9 forbids ("Nothing that ships in the binary may reach a workspace crate", `docs/design/proposals/draft/0017-blackbox-validation-tool.md:566`). The rationale is also applied unevenly: a normal edge to a proc-macro crate *is* walked, though a proc-macro's own dependencies never link in either. Flagging `wyrd-*` build edges would cost nothing for §14, since the fixtures are dev-only. The brief explicitly said "follow only `kind: null` edges", so whether to widen this is a scope decision, not a build defect.
- [ ] **The match is by name prefix, not workspace membership.** `xtask/src/repo_guard.rs:800` and `:859` only flag names starting with `wyrd-` (`:617`). §9's rationale says "workspace crate" (`0017-…md:566`). Concrete case: a new member `crates/s3-codes` with `name = "s3-codes"`, shared by `wyrd-gateway-s3` and `wyrd-validate`, passes the guard, and that is the self-referential verdict the guard exists to stop. No check I found forces the `wyrd-` prefix on members (`xtask` itself is unprefixed; today it is caught only because it reaches `wyrd-chunk-format`). Adding "any reached id listed in `workspace_members` other than the subject" would be a cheap second check. The brief specified the prefix, so this is a judgment call.
- [ ] **The T4 gate failure (3 "docs currency" blockers) looks like a false positive. A human must record the rejection.** `gate-logs/T4-batch-review.log` blocks on `main.rs:120` / `:1562` because the new `blackbox-guard --workspace/--metadata` and `ci-plan` subcommands have no living-architecture update. But the architecture doc says on purpose that it does not repeat the CI pipeline: "the authoritative pipeline definition, deliberately not restated here so it cannot drift" (`docs/design/architecture/08-crosscutting-concepts.md:116`). The `xtask` subcommands are listed in the module doc the patch already updates (`xtask/src/main.rs:5-20`). I read the rubric's "CLI flag" as the product CLI, not developer tooling. If the human agrees, record the rejection with that citation. If not, the fix is a single line in `08-…md`.
- [ ] **The C5 and C4-diff-cov reds are environment faults, not fix defects. Separately, merge order matters.** C5 died on its baseline because the mutants copy had no `.git`. The failing test is the pre-existing `scan_gitlinks_is_green_over_the_real_index` (`gate-logs/C5-mutants.log:414-436`), not this patch's test. I re-ran `cargo mutants --in-place --in-diff patch.diff -p xtask` in a git-backed scratch copy: 40 mutants, 23 caught, 1 missed, 16 unviable. All 16 unviable ones failed only because the workspace's deny-warnings lint rejects unused-variable and dead-code mutants. I rebuilt the important ones by hand (see below) and the tests killed them. C4-diff-cov failed because its worktree had no `wyrd-validate` member (`gate-logs/C4-diff-cov.log`: "cargo metadata has no `wyrd-validate` package"). That is the guard correctly refusing to pass without its subject (criterion 3). It also shows that if this lands on any `main` without `crates/validate` as a member (#774), `cargo xtask ci` goes red for everyone at the first guard. Merge #774 first.
- [ ] `xtask/src/main.rs:1562`: The new `blackbox-guard --workspace` and `--metadata` CLI flags have no corresponding living architecture documentation update; this diff changes only Rust source and tests. The rubric explicitly requires that update in the same PR. Document the new command and flags, its normal-dependency/all-features policy, and its CI integration under `docs/design/architecture/`. The source comments do not satisfy that requirement. This corroborates the frozen T4 review's repeated reports of the same omission; it is one finding.
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- [ ] **Build-dependencies are exempt, which lets the validator import exactly what proposal 0017 says it must not.** `is_normal_kind` treats `"build"` as non-normal (`repo_guard.rs:611`). The brief's "follow only `kind: null`" rule directed this, so the builder complied. But proposal 0017 says "A blackbox tool cannot import `DEFAULT_CHUNK_SIZE` or `MAX_ROOT_VALUE_BYTES` — that is the price of §9" (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:336-337`). Concrete case: `crates/validate` declares `[build-dependencies] wyrd-core`, its `build.rs` writes `wyrd_core::…::DEFAULT_CHUNK_SIZE` into `OUT_DIR`, and the binary `include!`s it. My plant of a declared `kind: "build"` `wyrd-core` entry gets a clean verdict, exit 0. Nothing links, but the verdict becomes self-referential. A human should decide whether §9's "normal" was meant to exclude build scripts. The same question covers `#[path = "../../core/src/…"]` source inclusion, which no dependency-graph guard can see. That may be better handled as a follow-up issue than in this PR.
- [ ] **C4-diff-cov ("unverifiable") and C5-mutants ("fail") are environment faults, not evidence against the fix, so this verdict rests on my hand mutants alone.**

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T5 Judgment — Exercise the production CI dispatcher with a forbidden dependency — replacing `run_ci_guard` with unconditional success survives every xtask test, leaving CI able to skip enforcement without the claimed regression detecting it (`xtask/src/main.rs:1505`, `xtask/tests/blackbox_dependency_guard.rs:221`, `reviewer-mutants-capped.log:5`).; **Criterion 1 is still beatable one hop further out.** At `xtask/src/main.rs:1510` I replaced `CiGuard::BlackboxClosure => run_blackbox_guard(&workspace_root())` with `CiGuard::BlackboxClosure => Ok(())` and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`: 24 + 27 + 30 + 29 passed, 0 failed. After that change `cargo xtask ci` never runs the; `xtask/src/main.rs:1562`: The new `blackbox-guard --workspace` and `--metadata` CLI flags have no corresponding living architecture documentation update; this diff changes only Rust source and tests. The rubric explicitly requires that update in the same PR. Document the new command and flags, its normal-dependency/all-features policy, and its CI integration under `docs/design/architecture/`. The source comments do not satisfy that requirement. This corroborates the frozen T4 review's repeated reports of the same omission; it is one finding.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
