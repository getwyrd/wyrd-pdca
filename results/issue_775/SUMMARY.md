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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (29 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.03s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #775: enforce `wyrd-validate`'s independence from `wyrd-*` normal dependencies, including transitive and optional edges, inside `cargo xtask ci`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The independence boundary, dev/build exclusions, fail-closed inputs, and observable CI registration are concrete and falsifiable (`brief.md:12`, `brief.md:46`; `xtask/src/repo_guard.rs:618`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the regression file produces 29 assertion failures because the guard commands do not exist; this reproduces the specified missing enforcement (`reviewer-red.log:41`, `reviewer-red-green.log:2`). |
| C3 Change | PASS | The four affected files stay within guard implementation, regression coverage, and required living documentation; validator crate code and existing guard policies are unchanged (`patch.diff:1`, `xtask/src/main.rs:1511`, `docs/design/architecture/08-crosscutting-concepts.md:119`). |
| C4 Verification (red→green) | PASS | Restoring the patch yields 29/29 regression tests and 113/113 targeted tests passing; full frozen CI is green, while the independent full rerun stops at a read-only advisory-cache lock (`reviewer-final-green.log:67`, `gate-logs/C4-ci.log:3859`, `reviewer-ci.log:3224`). |
| C5 Causal adequacy | PASS | Real Cargo fixtures demonstrate rejection of forbidden direct, renamed, and optional transitive dependencies through the production dispatcher; missing/unknown edge kinds fail closed, and no capability fallback masks a load-time cause (`xtask/tests/blackbox_dependency_guard.rs:229`, `xtask/tests/blackbox_dependency_guard.rs:356`, `xtask/src/repo_guard.rs:674`). |
| T1 Structure | PASS | Lib-side registration and a pure scanner feed the shared production guard phase, with the actual CI call grounded in the target; this keeps enforcement and test semantics aligned (`xtask/src/repo_guard.rs:595`, `xtask/src/repo_guard.rs:736`, `xtask/src/main.rs:1681`). |
| T2 Shape | PASS | Package identities, normal-edge classification, and visited-node tracking preserve renamed/transitive detection while leaving dev/build edges unconstrained; malformed required records return errors (`xtask/src/repo_guard.rs:651`, `xtask/src/repo_guard.rs:747`, `xtask/src/repo_guard.rs:840`). |
| T3 Runtime | PASS | Real all-features metadata and planted workspaces execute successfully; lockfile refusal and stopping before cargo steps are exercised without a service or replacement tool (`xtask/src/repo_guard.rs:618`, `xtask/tests/blackbox_dependency_guard.rs:395`, `reviewer-final-green.log:67`). |
| T4 Contribution | PASS | The CLI policy is documented, and affected-path searches of merged history plus all 19 closed/unmerged PRs found no competing guard; contribution-artifact auditing is N/A until publish (`docs/design/architecture/08-crosscutting-concepts.md:119`, `reviewer-prior-art-summary.log:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Mutation tests catch the new dispatcher and scanner regressions, including the prior unknown-kind gaps; 41/42 variants are caught, with the remaining whole-CI-entry mutation explicitly limiting the evidence below (`reviewer-mutants-capped.log:4`, `pdca-reviewer-775-scratch/mutants.out/caught.txt:5`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the normal-edge/name-prefix boundary as sufficient independence for the blackbox validator, considering the whole-CI mutation coverage limit and reliance on frozen evidence beyond the local advisory-cache failure (`brief.md:46`, `reviewer-mutants-capped.log:3`, `reviewer-ci.log:3224`). |

No confirmed patch defect remains in this review. Source citations above resolve in `$PDCA_TARGET`; evidence citations resolve in this review directory. The target contains the prerequisite validator package (`Cargo.toml:33`, `crates/validate/Cargo.toml:2`), and reverse-application checking confirms that it still matches `patch.diff` after all experiments.

The independent red→green is executable evidence. I used `git stash push` for the three tracked production/documentation files, kept the new test file, ran `cargo test --offline --locked -p xtask --test blackbox_dependency_guard`, and restored with `git stash pop`. All 29 tests ran and failed before restoration; all passed afterward. After mutation testing, `cargo test --offline --locked -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness` again passed 25 + 29 + 30 + 29 tests (`reviewer-red-green.log:17`, `reviewer-final-green.log:32`).

The coverage and mutation logs require different adjudication from a patch failure:

- **Coverage recovered independently.** The frozen run rejected metadata lacking `wyrd-validate` (`gate-logs/C4-diff-cov.log:58`). The independent `cargo llvm-cov test --offline --locked -p xtask --test blackbox_dependency_guard` passed all 29 tests against this target. Intersecting LCOV executable lines with the diff gives 168/168 scanner lines and 71/73 main-file lines executed (`reviewer-diff-coverage.log:3`). The two uncovered main-file lines are the existing submodule configuration path and the outer `run_ci` call.
- **Mutation evidence now reaches the tests.** The frozen run stopped at the existing Git-index test in its unmutated baseline, so it tested no mutants (`gate-logs/C5-mutants.log:425`). Running `cargo mutants --in-place --in-diff ../patch.diff -p xtask` here produced 22 caught and 20 unviable variants. Repeating with `RUSTFLAGS=--cap-lints=warn` made every variant compile: 41 caught, one missed, zero timeouts (`reviewer-mutants-capped.log:4`). Replacing `run_ci_guard` or `run_ci_steps_in` with success is caught. The survivor replaces the entire `run_ci` body with success (`xtask/src/main.rs:1675`); the test evidence therefore protects the shared guard phase, without establishing mutation coverage of the outer CI entry point. Actual execution of that entry point printed and passed the blackbox guard in both the independent and frozen CI runs (`reviewer-ci.log:18`, `gate-logs/C4-ci.log:27`).
- **Full local CI has a host limitation.** Formatting, docs lint/render, Clippy, workspace build/tests, and cargo-machete passed before cargo-deny failed to lock its read-only advisory cache (`reviewer-ci.log:3224`). I used the frozen full log for the remainder: default/all-feature dependency audits, conformance, static/orchestrator scans, and DST complete successfully (`gate-logs/C4-ci.log:3251`, `gate-logs/C4-ci.log:3266`, `gate-logs/C4-ci.log:3859`). The separate TiKV log records real compilation of both specified feature surfaces (`gate-logs/host-tikv.log:209`). No missing compiler or substitute service underlies the new guard's evidence.

Contribution evidence is complete for this stage. The frozen batch-review summary reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). **T4-contribution: N/A** — the artifacts are deliberately drafted later, and the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`). Independent GitHub queries checked merged commit history for all four affected paths and the changed files of every closed/unmerged PR in the 356-PR closed inventory. The only overlapping closed/unmerged work, PR #647, concerns segmented metadata documentation rather than dependency enforcement (`reviewer-prior-art-summary.log:3`, `reviewer-prior-art-overlap.json:1`).

### Advisory — adversary

# Adversarial review — #775 blackbox dependency-closure guard

**I could not refute the fix.** The scan, the gate wiring and the fail-closed paths held up
against every input I tried. Two items need a human decision. Neither one needs a rebuild.

## What I tried and could not break

- **Re-ran the proof** on a scratch copy of `$PDCA_TARGET`:
  `cargo test -p xtask --bins --test blackbox_dependency_guard` → 25 + 29 passed, including
  `the_real_workspace_has_no_violation` (`xtask/tests/blackbox_dependency_guard.rs:344`).
- **Ran the mutation test that the C5 gate never got to run** (C5 has failed during setup in all
  three iterations). `cargo mutants --in-place --cap-lints true --in-diff patch.diff -p xtask`:
  42 mutants, 41 caught, 1 missed. The one survivor is `run_ci -> Ok(())`
  (`xtask/src/main.rs:1671`). That function runs the whole gate and spawns cargo, so no test can
  drive it. Its guard wiring is a single line (`main.rs:1681`) into `run_ci_steps_in`, and every
  mutant in that function and below it is caught (`run_ci_steps_in`, `run_ci_guard`,
  `run_blackbox_guard`, `blackbox_verdict`, `scan_blackbox_closure`, `edge_is_normal`,
  `is_normal_kind`). Last round's hole, a `BlackboxClosure => Ok(())` arm at `main.rs:1511`, is
  now caught by `the_gate_stops_at_the_blackbox_guard_on_a_forbidden_dependency`, which runs
  the real dispatch through `ci-dry-run`.
- **Hand mutants:** removing `--all-features` or `--locked` from `BLACKBOX_METADATA_ARGS`
  (`xtask/src/repo_guard.rs:618`) turns a real-cargo test red in each case.
- **Workspaces resolved by real cargo, with the guard run on the
  `cargo metadata --locked --all-features` output:**
  - A normal dependency on `wyrd-core` that only applies on `cfg(windows)` → caught by both
    the manifest check and the graph check.
  - A third-party crate outside the workspace, whose optional `wyrd-core` dependency is turned on
    only by a `wyrd-validate` feature (`x = ["middle/extra"]`) → caught through the graph.
  - A normal dependency whose own *build*-dependency is `wyrd-core` → passes. That is correct,
    since it never links into the binary.
- **Where names come from:** names are read only from package records
  (`repo_guard.rs:736` onward), never from ids or edge names. Tests cover the renamed and
  opaque-id cases.

## Findings

- NEEDS-HUMAN [human] — **A build-dependency on a Wyrd crate passes the guard, and the error
  text says it doesn't.** `is_normal_kind` treats `"build"` like `"dev"`
  (`xtask/src/repo_guard.rs:654`), and a test locks that in
  (`xtask/tests/blackbox_dependency_guard.rs:552`). Concrete case, reproduced: a workspace where
  `wyrd-validate` has `[build-dependencies] wyrd-core = { path = "../core" }` and a `build.rs`
  → `blackbox-guard` prints "holds no wyrd-* crate" and exits 0. A build script that emits
  `cargo:rustc-env=MAGIC={}` from `wyrd_chunk_format::MAGIC`
  (`crates/chunk-format/src/lib.rs:25`) would put Wyrd's own constant into the validator binary,
  so the validator checks Wyrd against Wyrd, which is the outcome the guard exists to prevent.
  This matches the brief's literal "follow only `kind: null`". But proposal 0017 §9 justifies
  its exemption only for "what the test harness links", meaning dev-dependencies. Whichever way
  that is decided, the violation message at `xtask/src/main.rs:1551` ("Only a dev-dependency may
  name a wyrd-* crate") and the architecture doc
  (`docs/design/architecture/08-crosscutting-concepts.md:119`, "Dev-dependencies stay
  unconstrained") describe a stricter rule than the code enforces. Decide whether to forbid
  build-deps, or to allow them and say so in both places.
- NEEDS-HUMAN [human] — **Land #774 first, or `cargo xtask ci` goes red on `main`.** The guard
  correctly refuses a workspace with no `wyrd-validate` package (`repo_guard.rs:736` onward,
  criterion 3). The C4-diff-cov row shows this happening for real: it ran in a checkout at
  `36f006d` that has no `crates/validate`, and `the_real_workspace_has_no_violation` failed with
  "cargo metadata has no `wyrd-validate` package" (`gate-logs/C4-diff-cov.log`). That
  "unverifiable" result comes from the stale checkout, not from the fix. It also shows what
  every contributor would see if this patch merged before #774.

## Notes on the gate evidence (nothing for the builder to do)

- **The C4-verify red leg is thinner than "29 tests ran red" suggests.** All 29 fail for the same
  reason, `xtask: unknown task 'blackbox-guard'` (`gate-logs/C4-verify.log:53`). That shows the
  subcommand was missing, not that each case's logic was missing. The bin unit tests in
  `main.rs` (`ci_runs_every_repo_guard_before_the_cargo_steps`) are reverted along with the
  production code, so they never show red at all. The mutation run above is what shows the
  assertions actually pin the logic.
- **The C5 "fail" comes from the harness, not the patch.** cargo-mutants copies the tree without
  `.git`, so the existing `scan_gitlinks_is_green_over_the_real_index`
  (`xtask/tests/repo_hygiene_guards.rs:137`) fails the unmutated baseline. Even with that fixed,
  20 of the 42 mutants are unviable unless `--cap-lints true` is passed, because the workspace
  sets `warnings = "deny"` (`Cargo.toml:230`). That covers every "replace the body with `Ok(..)`"
  mutant on the new scan functions, so a harness fix needs both a git checkout (or `--in-place`)
  and `--cap-lints true`.

### Advisory — code-review

No findings. This diff is clean on both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the patch against the read-only target source, including dependency-kind classification, transitive traversal, malformed-input handling, production CI dispatch, regression assertions, and the architecture documentation update.

Validation limits: frozen `C4-ci` and `C4-verify` pass, with all 29 new integration tests green. `C4-diff-cov` produced no coverage because its checkout lacked `wyrd-validate`. `C5-mutants` stopped at an unchanged Git-index test in the unmutated baseline; all 29 new tests passed there, but no mutants were tested. These logs do not establish an introduced defect. No gates were rerun or target files changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the normal-edge/name-prefix boundary as sufficient independence for the blackbox validator, considering the whole-CI mutation coverage limit and reliance on frozen evidence beyond the local advisory-cache failure (`brief.md:46`, `reviewer-mutants-capped.log:3`, `reviewer-ci.log:3224`).
- [x] **A build-dependency on a Wyrd crate passes the guard, and the error text says it doesn't.** `is_normal_kind` treats `"build"` like `"dev"` (`xtask/src/repo_guard.rs:654`), and a test locks that in (`xtask/tests/blackbox_dependency_guard.rs:552`). Concrete case, reproduced: a workspace where `wyrd-validate` has `[build-dependencies] wyrd-core = { path = "../core" }` and a `build.rs` → `blackbox-guard` prints "holds no wyrd-* crate" and exits 0. A build script that emits `cargo:rustc-env=MAGIC={}` from `wyrd_chunk_format::MAGIC` (`crates/chunk-format/src/lib.rs:25`) would put Wyrd's own constant into the validator binary, so the validator checks Wyrd against Wyrd, which is the outcome the guard exists to prevent. This matches the brief's literal "follow only `kind: null`". But proposal 0017 §9 justifies its exemption only for "what the test harness links", meaning dev-dependencies. Whichever way that is decided, the violation message at `xtask/src/main.rs:1551` ("Only a dev-dependency may name a wyrd-* crate") and the architecture doc (`docs/design/architecture/08-crosscutting-concepts.md:119`, "Dev-dependencies stay unconstrained") describe a stricter rule than the code enforces. Decide whether to forbid build-deps, or to allow them and say so in both places.
- [x] **Land #774 first, or `cargo xtask ci` goes red on `main`.** The guard correctly refuses a workspace with no `wyrd-validate` package (`repo_guard.rs:736` onward, criterion 3). The C4-diff-cov row shows this happening for real: it ran in a checkout at `36f006d` that has no `crates/validate`, and `the_real_workspace_has_no_violation` failed with "cargo metadata has no `wyrd-validate` package" (`gate-logs/C4-diff-cov.log`). That "unverifiable" result comes from the stale checkout, not from the fix. It also shows what every contributor would see if this patch merged before #774.
- [x] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- [x] **Build-dependencies are exempt, which lets the validator import exactly what proposal 0017 says it must not.** `is_normal_kind` treats `"build"` as non-normal (`repo_guard.rs:611`). The brief's "follow only `kind: null`" rule directed this, so the builder complied. But proposal 0017 says "A blackbox tool cannot import `DEFAULT_CHUNK_SIZE` or `MAX_ROOT_VALUE_BYTES` — that is the price of §9" (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:336-337`). Concrete case: `crates/validate` declares `[build-dependencies] wyrd-core`, its `build.rs` writes `wyrd_core::…::DEFAULT_CHUNK_SIZE` into `OUT_DIR`, and the binary `include!`s it. My plant of a declared `kind: "build"` `wyrd-core` entry gets a clean verdict, exit 0. Nothing links, but the verdict becomes self-referential. A human should decide whether §9's "normal" was meant to exclude build scripts. The same question covers `#[path = "../../core/src/…"]` source inclusion, which no dependency-graph guard can see. That may be better handled as a follow-up issue than in this PR.
- [x] **C4-diff-cov ("unverifiable") and C5-mutants ("fail") are environment faults, not evidence against the fix, so this verdict rests on my hand mutants alone.**
- [x] **Build-dependencies are left open, which contradicts the stated invariant.** `xtask/src/repo_guard.rs:658-665` treats `"build"` as not normal, on the grounds that it "neither links into the shipped binary". `xtask/tests/blackbox_dependency_guard.rs:289` locks that in as allowed. Concrete case: `wyrd-validate` adds `[build-dependencies] wyrd-proto = { workspace = true }` plus a `build.rs` that writes Wyrd types or constants into `OUT_DIR`, which the binary then `include!`s. The guard passes, yet the binary ships Wyrd-derived types, which is exactly what proposal 0017 §9 forbids ("Nothing that ships in the binary may reach a workspace crate", `docs/design/proposals/draft/0017-blackbox-validation-tool.md:566`). The rationale is also applied unevenly: a normal edge to a proc-macro crate *is* walked, though a proc-macro's own dependencies never link in either. Flagging `wyrd-*` build edges would cost nothing for §14, since the fixtures are dev-only. The brief explicitly said "follow only `kind: null` edges", so whether to widen this is a scope decision, not a build defect.
- [x] **The match is by name prefix, not workspace membership.** `xtask/src/repo_guard.rs:800` and `:859` only flag names starting with `wyrd-` (`:617`). §9's rationale says "workspace crate" (`0017-…md:566`). Concrete case: a new member `crates/s3-codes` with `name = "s3-codes"`, shared by `wyrd-gateway-s3` and `wyrd-validate`, passes the guard, and that is the self-referential verdict the guard exists to stop. No check I found forces the `wyrd-` prefix on members (`xtask` itself is unprefixed; today it is caught only because it reaches `wyrd-chunk-format`). Adding "any reached id listed in `workspace_members` other than the subject" would be a cheap second check. The brief specified the prefix, so this is a judgment call.
- [x] **The T4 gate failure (3 "docs currency" blockers) looks like a false positive. A human must record the rejection.** `gate-logs/T4-batch-review.log` blocks on `main.rs:120` / `:1562` because the new `blackbox-guard --workspace/--metadata` and `ci-plan` subcommands have no living-architecture update. But the architecture doc says on purpose that it does not repeat the CI pipeline: "the authoritative pipeline definition, deliberately not restated here so it cannot drift" (`docs/design/architecture/08-crosscutting-concepts.md:116`). The `xtask` subcommands are listed in the module doc the patch already updates (`xtask/src/main.rs:5-20`). I read the rubric's "CLI flag" as the product CLI, not developer tooling. If the human agrees, record the rejection with that citation. If not, the fix is a single line in `08-…md`.
- [x] **The C5 and C4-diff-cov reds are environment faults, not fix defects. Separately, merge order matters.** C5 died on its baseline because the mutants copy had no `.git`. The failing test is the pre-existing `scan_gitlinks_is_green_over_the_real_index` (`gate-logs/C5-mutants.log:414-436`), not this patch's test. I re-ran `cargo mutants --in-place --in-diff patch.diff -p xtask` in a git-backed scratch copy: 40 mutants, 23 caught, 1 missed, 16 unviable. All 16 unviable ones failed only because the workspace's deny-warnings lint rejects unused-variable and dead-code mutants. I rebuilt the important ones by hand (see below) and the tests killed them. C4-diff-cov failed because its worktree had no `wyrd-validate` member (`gate-logs/C4-diff-cov.log`: "cargo metadata has no `wyrd-validate` package"). That is the guard correctly refusing to pass without its subject (criterion 3). It also shows that if this lands on any `main` without `crates/validate` as a member (#774), `cargo xtask ci` goes red for everyone at the first guard. Merge #774 first.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Forbid `wyrd-*` build-dependencies in the blackbox guard, and align the violation message + architecture doc (#775 sign-off).
- Blackbox guard: match workspace membership, not only the `wyrd-` name prefix (#775 sign-off).
