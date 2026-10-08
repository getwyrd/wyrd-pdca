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
