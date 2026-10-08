- **Slug:** xtask-blackbox-dependency-closure-guard
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** (framed as the gap) Nothing mechanically prevents `wyrd-validate` from linking
  a Wyrd workspace crate. `xtask/src/repo_guard.rs` carries exactly two invariants on `main`
  @ `65ca4fd` — the stray-gitlink scan (`scan_gitlinks`, `:238`) and the
  `#![forbid(unsafe_code)]` crate-root scan (`scan_roots`, `:500`); `grep -n "wyrd-"
  xtask/src/repo_guard.rs` shows the file never mentions a dependency closure. The moment the
  validator's binary links a `wyrd-*` crate it is testing Wyrd's types against Wyrd's types
  and its verdict is self-referential — the property the whole tool rests on, held by nothing
  but habit.
- **Success criterion:** BINDING, every leg inside `cargo xtask ci`:
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
- **Falsifiability:** This child earns a **genuine per-fix red→green**, unlike its sibling —
  confirmed by dry-running `run-verify.sh --classify` on a synthetic patch of this file set,
  which returns `ADDED_TEST xtask/tests/blackbox_dependency_guard.rs` / `CRATE xtask`. `xtask`
  exists on the base, so `GREEN_ONLY` is NOT set: the gate reverts the production change,
  keeps the added test file, and the test must fail. Every red in criteria 2 and 3 is a
  planted input on the ordinary developer harness — synthetic metadata documents, so
  demonstrating the red never requires committing the accident. No topology, no service.
- **Invariant to restore:** *Nothing that ships inside the `wyrd-validate` binary may reach a
  Wyrd workspace crate — and that must be enforced by the gate, not by a doc comment.* Stated
  over the category (the package's whole **normal** dependency closure, transitively AND
  under every feature, optional off-by-default edges included), not over one edge: a guard
  that checked only direct dependencies would pass a validator reaching `wyrd-core` through
  one hop, and a guard that resolved only DEFAULT features would pass one reaching it behind
  an optional feature. Source: proposal 0017 §9
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559-575` — "the **normal**
  dependency closure of its binary target must contain **no** `wyrd-*` crate … Normal, not
  total: `cargo metadata` walks dev-dependencies too, and the §14 fixtures are dev-only"),
  and the repo's own "held by habit until it was load-bearing" rule at
  `xtask/src/main.rs:1438-1442`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On `main` @ `65ca4fd`: `grep -n "wyrd-" xtask/src/repo_guard.rs` → the
  file never mentions a dependency closure; `cargo xtask ci` is green, i.e. today's gate is
  satisfied by a workspace that has no way to violate the property.
- **Scope:** a third invariant in `xtask/src/repo_guard.rs` — the `wyrd-validate` package's
  normal dependency closure, plus its declared normal (including optional) dependencies,
  contain no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can
  read, with its flippable test.
  **/ out of scope:** anything under `crates/validate/**` (that is child-2 — this child adds
  no crate code and no test there); extending the guard to any package other than
  `wyrd-validate`; changing the two existing invariants' behaviour (refactoring their
  registration into the shared lib-side list is in scope and expected, but their verdicts must
  not change); constraining dev-dependencies, which are deliberately unconstrained — #741 and
  proposal 0017 §14's fixtures rely on that.
- **Design constraints Do must honour** (why the obvious implementation is wrong):
  * **Resolve with the graph, and with `--all-features`.** The existing unsafe-forbid guard
    runs `cargo metadata --no-deps`, which omits the resolve graph. This guard needs the
    graph, so it runs `cargo metadata --format-version 1 --locked --all-features` WITHOUT
    `--no-deps` and walks `resolve.nodes`. `--locked` matters: a metadata call without it can
    rewrite `Cargo.lock`, and a guard that mutates the tree it audits is its own defect.
    `--all-features` matters because the resolve graph is FEATURE-DEPENDENT and this repo
    already uses the pattern that hides an edge from it —
    `crates/metadata-tikv/Cargo.toml:11-20` (the off-by-default `tikv` feature) together with
    `:27-30` (`tikv-client`, `tokio`, `async-trait`, `bytes`, each
    `{ workspace = true, optional = true }`) declares optional normal dependencies activated
    only by an off-by-default feature, and `xtask/src/lib.rs:40-47` records that the default
    workspace commands do not cover non-default features at all.
  * **Normal, not total.** Each node's `deps[].dep_kinds[].kind` is `null` for a normal edge
    and `"dev"` for a dev edge. Follow only `kind: null` edges transitively from the
    `wyrd-validate` node, and report every reached package whose name begins with `wyrd-`.
  * **Belt AND braces.** Additionally scan the package's **declared** dependency list
    (`packages[].dependencies`), which is feature-INDEPENDENT and carries `kind` and
    `optional` per entry, and report any `wyrd-*` entry whose `kind` is `null`, optional or
    not. This catches a declaration even if some future feature-resolution subtlety keeps it
    out of the graph, and it is what makes criterion 2's optional case meaningful. Report both
    classes as violations of the same invariant, with wording that says which one fired.
  * **Pure function in, violations out** — the same function `cargo xtask ci` runs is the one
    the test drives over planted documents.
  * `--all-features` widens the resolve only; it compiles nothing, so the guard stays
    sub-second and needs neither the TiKV nor the FoundationDB toolchain the corresponding
    *builds* would (verified offline against this checkout: exit 0, no build).
- **External dependencies:** none
- **Test file:** `xtask/tests/blackbox_dependency_guard.rs` — a NEW file.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer callsites
  Do MAY open (a narrow, deliberate exception to reading the brief only), re-verified on
  `65ca4fd`:
  * **The guard's shape** — `xtask/src/repo_guard.rs:387` (`target_src_paths`: a pure
    function over `cargo metadata` JSON returning `Result<_, String>`, failing closed on a
    document it cannot parse) and `xtask/src/main.rs:1443-1484` (`run_unsafe_forbid_guard`:
    `print_step`, shell out to `cargo metadata`, hand the JSON to the pure function, join
    violations into one error). Call the new guard from `run_ci` beside it, `:1557-1558`.
  * **How this repo makes a `run_ci` call site TEST-VISIBLE** — `xtask/src/main.rs:1486-1544`
    and its doc comment at `:1504-1505`; the list itself is `xtask/src/lib.rs:81`
    (`feature_gated_checks`). This is the model for criterion 1.
  * **The flippable-test idiom** — `xtask/tests/repo_hygiene_guards.rs:35`
    (`scan_gitlinks_is_red_on_an_undeclared_gitlink`: synthetic input, assert exactly one
    violation, assert it names the offender) and `:385`
    (`scan_crate_roots_is_green_over_the_real_workspace_crates`).
  * **Why the guard's subject must already exist** — `xtask/src/repo_guard.rs:421`
    (`unregistered_manifests`): a package under `crates/` that is not a workspace member never
    reaches metadata. The fail-closed branch of criterion 3 must not turn a missing package
    into a pass, and the guard must not become satisfiable by removing `crates/validate` from
    `[workspace] members`.
- **Prior-art check (triage cycles):** searched by affected path on `main` @ `65ca4fd`.
  `git log --oneline -- xtask/src/repo_guard.rs` → six #616 commits, all on the gitlink and
  unsafe-forbid scans; none touches a dependency closure. `gh pr list --search "blackbox"`
  across all states → only PR #765, the merged proposal 0017 document. No closed/rejected
  attempt exists.
- **Surfaces:** data
- **Difficulty:** medium
- **Depends on:** 774
- **Disposition hint:** new-feature

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Reject an empty dependency-kind list before classifying an edge — otherwise incomplete metadata hides a transitive Wyrd dependency and falsely certifies independence; reproduced exit 0 for `[]` versus exit 1 for a normal kind (`xtask/src/repo_guard.rs:759`, `reviewer-empty-kinds.log:1`).; T5 Judgment — Add unknown-kind rejection coverage — two surviving mutations turn unknown strings into silently ignored edges, so the tests do not protect the new parser’s stated fail-closed contract (`xtask/src/repo_guard.rs:607`, `xtask/src/repo_guard.rs:611`, `reviewer-mutants.log:6`).; **Empty `dep_kinds` passes silently (reproduced; this is the T4 blocking finding).** At `xtask/src/repo_guard.rs:759-763`, `normal` starts `false` and stays `false` when `dep_kinds` is `[]`, so the edge is skipped and never walked. Concrete case: `wyrd-validate -> middle` with `"dep_kinds": [{"kind": null}]`, then `middle -> wyrd-core` with `"dep_kinds": []`. Result: `xtask blackbox-guard: … holds no wyrd-* crate`, exit 0. Putting `[]` on the direct edge `wyrd-validate -> middle` hides everything below it the same way (exit 0). The absent-key case is refused (`:750-758`) and tested (`xtask/tests/blackbox_dependency_guard.rs:377`), but the empty-array case is neither. That contradicts the function's own "Fails CLOSED" contract and brief criterion 3. Real cargo never emits an empty `dep_kinds`, so this is hardening, but it is exactly the "silent skip" class in the rubric. Fix: `if kinds.is_empty() { return Err(..) }`, plus a planted test.; **Criterion 1 is not actually pinned: deleting the guard call from `run_ci` leaves every test green.** I replaced `xtask/src/main.rs:1480` (`run_repo_guards()?;`) with a comment and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`. Result: 23 + 15 + 30 + 29 passed, 0 failed. The criterion-1 test (`blackbox_dependency_guard.rs:144-145`) drives the separate `repo-guards` subcommand, not `run_ci`. So it proves `CI_GUARDS` holds the blackbox guard, not that the gate runs `CI_GUARDS`. That is the "defined, tested, never called" hazard the brief says criterion 1 exists to close. The repo's own model already avoids this gap. `feature_gated_checks` is called inside `run_ci_steps` (`main.rs:1426`), and a bin unit test drives that function with a recording executor (`main.rs:1883-1885`). The #257 comment at `main.rs:1913-1917` explicitly rejects tests that "stayed green if the wiring loop was deleted". Fix: route `run_ci_guards` through `run_ci_steps` (or an equally injectable function) and assert in a bin unit test that the recorded sequence contains `CiGuard::BlackboxClosure`.; **The fail-closed branches that decide whether an edge is followed are unpinned. Four hand mutants survive (15/15 green each):**; `xtask/src/repo_guard.rs:759`: An empty `dep_kinds` array leaves `normal` false and silently skips the edge. For `wyrd-validate -> middle -> wyrd-core`, setting the second edge's kinds to `[]` yields a clean result: the manifest check only examines `wyrd-validate`, so it cannot catch the hidden transitive dependency. Reject empty kind lists as indeterminate metadata and add a transitive regression asserting an error. This also corroborates the frozen T4 finding.; `xtask/tests/blackbox_dependency_guard.rs:145`: The new registration test launches all repository guards and requires a real Git index. The frozen `gate-logs/C5-mutants.log` shows this test failing with “not a git repository,” aborting the unmutated baseline before any mutant is tested. Reuse `run_ci_guards`' injected executor (`xtask/src/repo_guard.rs:828`) to record and assert the registered guards without invoking Git; retain the separate real-metadata test. This removes the new test's unnecessary checkout dependency without changing production guard behavior.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T5 Judgment — Exercise the production CI dispatcher with a forbidden dependency — replacing `run_ci_guard` with unconditional success survives every xtask test, leaving CI able to skip enforcement without the claimed regression detecting it (`xtask/src/main.rs:1505`, `xtask/tests/blackbox_dependency_guard.rs:221`, `reviewer-mutants-capped.log:5`).; **Criterion 1 is still beatable one hop further out.** At `xtask/src/main.rs:1510` I replaced `CiGuard::BlackboxClosure => run_blackbox_guard(&workspace_root())` with `CiGuard::BlackboxClosure => Ok(())` and ran `cargo test -p xtask --bins --test blackbox_dependency_guard --test repo_hygiene_guards --test fdb_harness`: 24 + 27 + 30 + 29 passed, 0 failed. After that change `cargo xtask ci` never runs the; `xtask/src/main.rs:1562`: The new `blackbox-guard --workspace` and `--metadata` CLI flags have no corresponding living architecture documentation update; this diff changes only Rust source and tests. The rubric explicitly requires that update in the same PR. Document the new command and flags, its normal-dependency/all-features policy, and its CI integration under `docs/design/architecture/`. The source comments do not satisfy that requirement. This corroborates the frozen T4 review's repeated reports of the same omission; it is one finding.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_775/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
