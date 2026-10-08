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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (15 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.18s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #775’s CI enforcement of `wyrd-validate`’s independent normal dependency closure: one reproduced fail-closed defect and one test gap need a rebuild.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The independence boundary and allowed dev dependencies are explicit and falsifiable; the prerequisite validator exists in the supplied target (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559`, `crates/validate/Cargo.toml:2`, `brief.md:24`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production patch while retaining the new test produced 15 failures because the guard commands do not exist before the fix (`reviewer-red.log:181`, `xtask/tests/blackbox_dependency_guard.rs:39`). |
| C3 Change | PASS | The changes stay within the requested xtask guard, registration, and test surface; existing hygiene scan semantics and validator code remain unchanged (`xtask/src/repo_guard.rs:819`, `xtask/src/main.rs:1480`, `patch.diff:1`). |
| C4 Verification (red→green) | PASS | Restoring the patch produced 15/15 passing tests; 29 existing hygiene tests, pinned-toolchain clippy, fmt, typos, and the LLVM coverage test run also passed; full CI and TiKV compilation are supported by frozen logs, not an independent full rerun (`reviewer-restored-green.log:22`, `reviewer-green.log:72`, `gate-logs/C4-ci.log:3837`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Reject an empty dependency-kind list before classifying an edge — otherwise incomplete metadata hides a transitive Wyrd dependency and falsely certifies independence; reproduced exit 0 for `[]` versus exit 1 for a normal kind (`xtask/src/repo_guard.rs:759`, `reviewer-empty-kinds.log:1`). |
| T1 Structure | PASS | CI and the standalone guard execute the same lib-owned registration and pure scanner, keeping enforcement centralized (`xtask/src/repo_guard.rs:819`, `xtask/src/repo_guard.rs:828`, `xtask/src/main.rs:1383`, `xtask/src/main.rs:1480`). |
| T2 Shape | PASS | One finding per crate retains its dependency path; normal edges and declared optional dependencies are covered while dev/build edges remain outside the stated boundary (`xtask/src/repo_guard.rs:698`, `xtask/src/repo_guard.rs:735`, `xtask/src/repo_guard.rs:778`). |
| T3 Runtime | PASS | Real locked all-features metadata resolves successfully without backend services; the visited set bounds graph traversal and prevents cyclic traversal (`xtask/src/repo_guard.rs:586`, `xtask/src/repo_guard.rs:735`, `xtask/tests/blackbox_dependency_guard.rs:172`, `reviewer-restored-green.log:20`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit must rerun at publish. The separate batch-review defect is independently confirmed under C5 (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add unknown-kind rejection coverage — two surviving mutations turn unknown strings into silently ignored edges, so the tests do not protect the new parser’s stated fail-closed contract (`xtask/src/repo_guard.rs:607`, `xtask/src/repo_guard.rs:611`, `reviewer-mutants.log:6`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether enforcing the all-features normal dependency boundary, after the C5/T5 repairs, is sufficient evidence of validator independence for proposal 0017; this does not establish the validator’s operational correctness (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:562`). |

The confirmed defect is acceptance of unclassifiable metadata. At `xtask/src/repo_guard.rs:759`, `normal` starts false; an empty `dep_kinds` array leaves it false and the edge is skipped at line 763. A document with `wyrd-validate -> middle -> wyrd-core`, with a normal first edge and an empty kind list on the second, prints the clean-verdict message and exits 0. Changing only that list to `[{"kind": null}]` reports the full forbidden path and exits 1; a dev-only list correctly exits 0. This violates the absent-entry rule (`AGENTS.md:175`) and the scanner’s fail-closed posture. Reject the empty list and add a transitive regression that expects a scanner error, not a clean verdict. Reproduce from this review directory with:

```sh
./pdca-reviewer-775-build/debug/xtask blackbox-guard pdca-reviewer-775-empty-kinds.json
./pdca-reviewer-775-build/debug/xtask blackbox-guard pdca-reviewer-775-normal-kinds.json
```

The mutation rerun establishes a separate test gap, not a second current parser bug. `cargo mutants --in-diff ../patch.diff --copy-vcs true --package xtask --jobs 2` completed against copies of the supplied target: 40 mutants, 17 caught, 16 unviable, and 7 missed (`reviewer-mutants.log:13`). Two missed mutations at `xtask/src/repo_guard.rs:611` accept unknown dependency kinds as non-normal; production currently rejects them. Exercise an unknown kind on a transitive edge so the direct manifest scan cannot conceal the regression. The other five survivors affect unchanged, relocated #616 gitlink error handling at lines 907–908; they are not new implementation findings in this patch. The C5 smell-test found no optional-capability probe masking a load-time cause.

Verification evidence supports the successful build and also explains the frozen auxiliary failures. The disposable target was restored byte-for-byte after the stash/red/pop/green cycle. Independent commands used the real Cargo toolchain and real workspace metadata, without a shim or substitute topology. The frozen full CI log shows the guard running (`gate-logs/C4-ci.log:27`) and the complete gate passing; the TiKV log records both requested compilations. I reran the focused tests, clippy, format/spelling checks, coverage, and mutations, rather than the instance-scoped gate wrappers. The frozen coverage failure lacked `wyrd-validate` in its separate checkout (`gate-logs/C4-diff-cov.log:42`); the local LLVM run has all 15 tests passing (`reviewer-coverage.log`). The frozen mutation baseline lacked `.git` (`gate-logs/C5-mutants.log:184`); retaining the real Git metadata fixed that prerequisite and allowed the campaign above. Neither frozen failure is a C4 patch defect. No declared external dependency remains undischarged (`brief.md:99`).

Prior art was checked by affected file path, including closed work. The disposable target has one synthesized base commit, so I queried GitHub’s merged commit history for `xtask/src/repo_guard.rs` and all 356 closed/merged PRs’ changed-file lists for the four affected paths, completing the two paginated file lists. There were 33 matching merged PRs and no matching closed-unmerged PR; the prior guard work is #616, not an earlier dependency-closure implementation (`reviewer-prior-art.log:1`, `reviewer-prior-art-closed.log:33`). This discharges the prior-art investigation without relying on the brief’s keyword search. All source citations above are relative to the supplied `$PDCA_TARGET`; review and gate logs are relative to this directory. The supplied target includes the prerequisite crate and was readable throughout. No harness `INTEGRATION.md` was supplied, and none exists in the target.

### Advisory — adversary

# Adversarial review — #775 blackbox dependency-closure guard

I rebuilt the patched tree in a scratch copy (`cargo test -p xtask --test blackbox_dependency_guard`: 15/15 green) and ran the built `xtask blackbox-guard <file>` over planted `cargo metadata` documents. The C4-verify red→green holds, but the red is shallow: all 15 tests go red pre-fix for a single reason, `xtask: unknown task 'blackbox-guard'` (gate-logs/C4-verify.log). That proves the subcommand exists. It does not prove each assertion catches a broken scan. C5-mutants tested nothing (see the last bullet), so I mutated the code by hand. Three findings survive: one fail-open input, one untested wiring line, and a set of fail-closed branches no test pins.

- NEEDS-HUMAN [impl] — **Empty `dep_kinds` passes silently (reproduced; this is the T4 blocking finding).** At `xtask/src/repo_guard.rs:759-763`, `normal` starts `false` and stays `false` when `dep_kinds` is `[]`, so the edge is skipped and never walked. Concrete case: `wyrd-validate -> middle` with `"dep_kinds": [{"kind": null}]`, then `middle -> wyrd-core` with `"dep_kinds": []`. Result: `xtask blackbox-guard: … holds no wyrd-* crate`, exit 0. Putting `[]` on the direct edge `wyrd-validate -> middle` hides everything below it the same way (exit 0). The absent-key case is refused (`:750-758`) and tested (`xtask/tests/blackbox_dependency_guard.rs:377`), but the empty-array case is neither. That contradicts the function's own "Fails CLOSED" contract and brief criterion 3. Real cargo never emits an empty `dep_kinds`, so this is hardening, but it is exactly the "silent skip" class in the rubric. Fix: `if kinds.is_empty() { return Err(..) }`, plus a planted test.

- NEEDS-HUMAN [impl] — **Criterion 1 is not actually pinned: deleting the guard call from `run_ci` leaves every test green.** I replaced `xtask/src/main.rs:1480` (`run_repo_guards()?;`) with a comment and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`. Result: 23 + 15 + 30 + 29 passed, 0 failed. The criterion-1 test (`blackbox_dependency_guard.rs:144-145`) drives the separate `repo-guards` subcommand, not `run_ci`. So it proves `CI_GUARDS` holds the blackbox guard, not that the gate runs `CI_GUARDS`. That is the "defined, tested, never called" hazard the brief says criterion 1 exists to close. The repo's own model already avoids this gap. `feature_gated_checks` is called inside `run_ci_steps` (`main.rs:1426`), and a bin unit test drives that function with a recording executor (`main.rs:1883-1885`). The #257 comment at `main.rs:1913-1917` explicitly rejects tests that "stayed green if the wiring loop was deleted". Fix: route `run_ci_guards` through `run_ci_steps` (or an equally injectable function) and assert in a bin unit test that the recorded sequence contains `CiGuard::BlackboxClosure`.

- NEEDS-HUMAN [impl] — **The fail-closed branches that decide whether an edge is followed are unpinned. Four hand mutants survive (15/15 green each):**
  1. A missing `kind` in `is_normal_kind` (`repo_guard.rs:616`) changed to `Ok(false)`.
  2. An unknown kind (`:612`) changed to `Ok(false)`.
  3. The duplicate-`wyrd-validate` check (`:667`) disabled.
  4. A resolve node with no `deps` array (`:745`) treated as a leaf.

  Mutants 1, 2 and 4 each turn a fail-closed path into a silent pass that hides the closure below that point. The built binary does refuse the missing-kind and unknown-kind (`"proc-macro"`) plants today; only the tests are missing. Add one planted-document case per branch, in the `assert_refused` style (`blackbox_dependency_guard.rs:292`).

- NEEDS-HUMAN [human] — **Build-dependencies are exempt, which lets the validator import exactly what proposal 0017 says it must not.** `is_normal_kind` treats `"build"` as non-normal (`repo_guard.rs:611`). The brief's "follow only `kind: null`" rule directed this, so the builder complied. But proposal 0017 says "A blackbox tool cannot import `DEFAULT_CHUNK_SIZE` or `MAX_ROOT_VALUE_BYTES` — that is the price of §9" (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:336-337`). Concrete case: `crates/validate` declares `[build-dependencies] wyrd-core`, its `build.rs` writes `wyrd_core::…::DEFAULT_CHUNK_SIZE` into `OUT_DIR`, and the binary `include!`s it. My plant of a declared `kind: "build"` `wyrd-core` entry gets a clean verdict, exit 0. Nothing links, but the verdict becomes self-referential. A human should decide whether §9's "normal" was meant to exclude build scripts. The same question covers `#[path = "../../core/src/…"]` source inclusion, which no dependency-graph guard can see. That may be better handled as a follow-up issue than in this PR.

- NEEDS-HUMAN [human] — **C4-diff-cov ("unverifiable") and C5-mutants ("fail") are environment faults, not evidence against the fix, so this verdict rests on my hand mutants alone.**
  - **C4-diff-cov:** it ran in `/home/eddie/wyrd/wyrd-cov-l1`, whose metadata has no `wyrd-validate` package, so the guard correctly refused (gate-logs/C4-diff-cov.log). That checkout apparently predates #774. The failure also confirms that merging #775 ahead of #774 would make `cargo xtask ci` hard-red on `main`, so the merge order must be enforced.
  - **C5-mutants:** it failed its unmutated baseline because the cargo-mutants copy has no `.git`, so `repo-guards` → gitlink guard exits 128. This is not new with this patch. `repo_hygiene_guards.rs:129-137` already requires a git checkout; the new test simply runs first.

- Informational, not a refutation: the guard identifies Wyrd crates only by the `wyrd-` name prefix (`repo_guard.rs:698`, `:771`). Proposal 0017 words the invariant more broadly: "Nothing that ships in the binary may reach a workspace crate" (`0017…md:566-567`). Today every workspace member except `xtask` has the prefix, and `xtask` normally depends on `wyrd-chunk-format`, so it would be caught one hop later. There is no live hole. A future member without the prefix (or named `wyrd_x`) would slip through, though. Checking `workspace_members` or a null `source` as a second test would close that without depending on names.

Tried and could not break:
- Renamed declared deps: caught, because the scan reads names from package records, not edge names.
- Target-specific (`cfg(windows)`) edges mixed with dev edges: caught.
- Dev edges deeper in the graph: correctly not followed.
- Missing `id` or `name`, a subject package with no resolve node, a reached package with no resolve node, `resolve` null or absent, an unparsable document: all refused.
- The moved gitlink and unsafe-forbid guards: their verdicts are unchanged (the code moved without changing behaviour).
- `--locked --all-features` over the real tree: green.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `xtask/src/repo_guard.rs:759`: An empty `dep_kinds` array leaves `normal` false and silently skips the edge. For `wyrd-validate -> middle -> wyrd-core`, setting the second edge's kinds to `[]` yields a clean result: the manifest check only examines `wyrd-validate`, so it cannot catch the hidden transitive dependency. Reject empty kind lists as indeterminate metadata and add a transitive regression asserting an error. This also corroborates the frozen T4 finding.

- NEEDS-HUMAN [impl] — `xtask/tests/blackbox_dependency_guard.rs:145`: The new registration test launches all repository guards and requires a real Git index. The frozen `gate-logs/C5-mutants.log` shows this test failing with “not a git repository,” aborting the unmutated baseline before any mutant is tested. Reuse `run_ci_guards`' injected executor (`xtask/src/repo_guard.rs:828`) to record and assert the registered guards without invoking Git; retain the separate real-metadata test. This removes the new test's unnecessary checkout dependency without changing production guard behavior.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Reject an empty dependency-kind list before classifying an edge — otherwise incomplete metadata hides a transitive Wyrd dependency and falsely certifies independence; reproduced exit 0 for `[]` versus exit 1 for a normal kind (`xtask/src/repo_guard.rs:759`, `reviewer-empty-kinds.log:1`).
- [ ] T5 Judgment — Add unknown-kind rejection coverage — two surviving mutations turn unknown strings into silently ignored edges, so the tests do not protect the new parser’s stated fail-closed contract (`xtask/src/repo_guard.rs:607`, `xtask/src/repo_guard.rs:611`, `reviewer-mutants.log:6`).
- [ ] Validation — fitness-to-purpose — Decide whether enforcing the all-features normal dependency boundary, after the C5/T5 repairs, is sufficient evidence of validator independence for proposal 0017; this does not establish the validator’s operational correctness (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:562`).
- [ ] **Empty `dep_kinds` passes silently (reproduced; this is the T4 blocking finding).** At `xtask/src/repo_guard.rs:759-763`, `normal` starts `false` and stays `false` when `dep_kinds` is `[]`, so the edge is skipped and never walked. Concrete case: `wyrd-validate -> middle` with `"dep_kinds": [{"kind": null}]`, then `middle -> wyrd-core` with `"dep_kinds": []`. Result: `xtask blackbox-guard: … holds no wyrd-* crate`, exit 0. Putting `[]` on the direct edge `wyrd-validate -> middle` hides everything below it the same way (exit 0). The absent-key case is refused (`:750-758`) and tested (`xtask/tests/blackbox_dependency_guard.rs:377`), but the empty-array case is neither. That contradicts the function's own "Fails CLOSED" contract and brief criterion 3. Real cargo never emits an empty `dep_kinds`, so this is hardening, but it is exactly the "silent skip" class in the rubric. Fix: `if kinds.is_empty() { return Err(..) }`, plus a planted test.
- [ ] **Criterion 1 is not actually pinned: deleting the guard call from `run_ci` leaves every test green.** I replaced `xtask/src/main.rs:1480` (`run_repo_guards()?;`) with a comment and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`. Result: 23 + 15 + 30 + 29 passed, 0 failed. The criterion-1 test (`blackbox_dependency_guard.rs:144-145`) drives the separate `repo-guards` subcommand, not `run_ci`. So it proves `CI_GUARDS` holds the blackbox guard, not that the gate runs `CI_GUARDS`. That is the "defined, tested, never called" hazard the brief says criterion 1 exists to close. The repo's own model already avoids this gap. `feature_gated_checks` is called inside `run_ci_steps` (`main.rs:1426`), and a bin unit test drives that function with a recording executor (`main.rs:1883-1885`). The #257 comment at `main.rs:1913-1917` explicitly rejects tests that "stayed green if the wiring loop was deleted". Fix: route `run_ci_guards` through `run_ci_steps` (or an equally injectable function) and assert in a bin unit test that the recorded sequence contains `CiGuard::BlackboxClosure`.
- [ ] **The fail-closed branches that decide whether an edge is followed are unpinned. Four hand mutants survive (15/15 green each):**
- [ ] **Build-dependencies are exempt, which lets the validator import exactly what proposal 0017 says it must not.** `is_normal_kind` treats `"build"` as non-normal (`repo_guard.rs:611`). The brief's "follow only `kind: null`" rule directed this, so the builder complied. But proposal 0017 says "A blackbox tool cannot import `DEFAULT_CHUNK_SIZE` or `MAX_ROOT_VALUE_BYTES` — that is the price of §9" (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:336-337`). Concrete case: `crates/validate` declares `[build-dependencies] wyrd-core`, its `build.rs` writes `wyrd_core::…::DEFAULT_CHUNK_SIZE` into `OUT_DIR`, and the binary `include!`s it. My plant of a declared `kind: "build"` `wyrd-core` entry gets a clean verdict, exit 0. Nothing links, but the verdict becomes self-referential. A human should decide whether §9's "normal" was meant to exclude build scripts. The same question covers `#[path = "../../core/src/…"]` source inclusion, which no dependency-graph guard can see. That may be better handled as a follow-up issue than in this PR.
- [ ] **C4-diff-cov ("unverifiable") and C5-mutants ("fail") are environment faults, not evidence against the fix, so this verdict rests on my hand mutants alone.**
- [ ] `xtask/src/repo_guard.rs:759`: An empty `dep_kinds` array leaves `normal` false and silently skips the edge. For `wyrd-validate -> middle -> wyrd-core`, setting the second edge's kinds to `[]` yields a clean result: the manifest check only examines `wyrd-validate`, so it cannot catch the hidden transitive dependency. Reject empty kind lists as indeterminate metadata and add a transitive regression asserting an error. This also corroborates the frozen T4 finding.
- [ ] `xtask/tests/blackbox_dependency_guard.rs:145`: The new registration test launches all repository guards and requires a real Git index. The frozen `gate-logs/C5-mutants.log` shows this test failing with “not a git repository,” aborting the unmutated baseline before any mutant is tested. Reuse `run_ci_guards`' injected executor (`xtask/src/repo_guard.rs:828`) to record and assert the registered guards without invoking Git; retain the separate real-metadata test. This removes the new test's unnecessary checkout dependency without changing production guard behavior.
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the shipped test did not pass under llvm-cov, so diff coverage was not measured
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Reject an empty dependency-kind list before classifying an edge — otherwise incomplete metadata hides a transitive Wyrd dependency and falsely certifies independence; reproduced exit 0 for `[]` versus exit 1 for a normal kind (`xtask/src/repo_guard.rs:759`, `reviewer-empty-kinds.log:1`).; T5 Judgment — Add unknown-kind rejection coverage — two surviving mutations turn unknown strings into silently ignored edges, so the tests do not protect the new parser’s stated fail-closed contract (`xtask/src/repo_guard.rs:607`, `xtask/src/repo_guard.rs:611`, `reviewer-mutants.log:6`).; **Empty `dep_kinds` passes silently (reproduced; this is the T4 blocking finding).** At `xtask/src/repo_guard.rs:759-763`, `normal` starts `false` and stays `false` when `dep_kinds` is `[]`, so the edge is skipped and never walked. Concrete case: `wyrd-validate -> middle` with `"dep_kinds": [{"kind": null}]`, then `middle -> wyrd-core` with `"dep_kinds": []`. Result: `xtask blackbox-guard: … holds no wyrd-* crate`, exit 0. Putting `[]` on the direct edge `wyrd-validate -> middle` hides everything below it the same way (exit 0). The absent-key case is refused (`:750-758`) and tested (`xtask/tests/blackbox_dependency_guard.rs:377`), but the empty-array case is neither. That contradicts the function's own "Fails CLOSED" contract and brief criterion 3. Real cargo never emits an empty `dep_kinds`, so this is hardening, but it is exactly the "silent skip" class in the rubric. Fix: `if kinds.is_empty() { return Err(..) }`, plus a planted test.; **Criterion 1 is not actually pinned: deleting the guard call from `run_ci` leaves every test green.** I replaced `xtask/src/main.rs:1480` (`run_repo_guards()?;`) with a comment and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`. Result: 23 + 15 + 30 + 29 passed, 0 failed. The criterion-1 test (`blackbox_dependency_guard.rs:144-145`) drives the separate `repo-guards` subcommand, not `run_ci`. So it proves `CI_GUARDS` holds the blackbox guard, not that the gate runs `CI_GUARDS`. That is the "defined, tested, never called" hazard the brief says criterion 1 exists to close. The repo's own model already avoids this gap. `feature_gated_checks` is called inside `run_ci_steps` (`main.rs:1426`), and a bin unit test drives that function with a recording executor (`main.rs:1883-1885`). The #257 comment at `main.rs:1913-1917` explicitly rejects tests that "stayed green if the wiring loop was deleted". Fix: route `run_ci_guards` through `run_ci_steps` (or an equally injectable function) and assert in a bin unit test that the recorded sequence contains `CiGuard::BlackboxClosure`.; **The fail-closed branches that decide whether an edge is followed are unpinned. Four hand mutants survive (15/15 green each):**; `xtask/src/repo_guard.rs:759`: An empty `dep_kinds` array leaves `normal` false and silently skips the edge. For `wyrd-validate -> middle -> wyrd-core`, setting the second edge's kinds to `[]` yields a clean result: the manifest check only examines `wyrd-validate`, so it cannot catch the hidden transitive dependency. Reject empty kind lists as indeterminate metadata and add a transitive regression asserting an error. This also corroborates the frozen T4 finding.; `xtask/tests/blackbox_dependency_guard.rs:145`: The new registration test launches all repository guards and requires a real Git index. The frozen `gate-logs/C5-mutants.log` shows this test failing with “not a git repository,” aborting the unmutated baseline before any mutant is tested. Reuse `run_ci_guards`' injected executor (`xtask/src/repo_guard.rs:828`) to record and assert the registered guards without invoking Git; retain the separate real-metadata test. This removes the new test's unnecessary checkout dependency without changing production guard behavior.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
