## Summary
**User impact:** `wyrd-validate` is meant to check a Wyrd deployment from the outside. If it ever started using Wyrd's own code, it would be checking Wyrd against itself: a bug present in both would look correct, and the tool would report a pass it has no right to give. Until now nothing stopped that. One added dependency line would do it, and CI would stay green.

This PR makes `cargo xtask ci` fail whenever `wyrd-validate` can reach a Wyrd crate, directly, through other crates, or behind an optional feature. Dev-dependencies (test-only) stay allowed.

**Merge order:** this depends on the `wyrd-validate` crate from #774 (PR #845). Merge that first. Without it the new check refuses to pass, by design, and `cargo xtask ci` goes red for everyone.

## What to look at
- The new check sits beside the two existing repository checks (no stray submodule links, no `unsafe` code) and runs before any build step.
- Try it: `cargo xtask blackbox-guard` on a tree with #774 prints a clean result. Add `wyrd-core = { workspace = true }` under `[dependencies]` in `crates/validate/Cargo.toml` and run it again: it fails and names `wyrd-core`. Move the same line to `[dev-dependencies]` and it passes.
- `cargo xtask ci-dry-run` runs the gate's checks for real but only prints the cargo build/test steps instead of running them. The tests use it to show that the gate itself stops at a forbidden dependency.
- The two existing checks give the same results as before. Only the way they are registered and called changed.

## Root cause
The rule that `wyrd-validate` must not link a Wyrd crate (proposal 0017 §9) was written down but not enforced. `xtask/src/repo_guard.rs` had checks for stray gitlinks and for `#![forbid(unsafe_code)]`, and nothing that looked at any crate's dependencies.

## Fix
- **The scan** (`xtask/src/repo_guard.rs`): `scan_blackbox_closure` takes a `cargo metadata` document and returns the violations. It follows only normal edges (`kind: null`) out of `wyrd-validate`, transitively, and reports every reached `wyrd-*` crate with the path that reaches it. It also checks the declared dependency list, optional entries included, since that list does not depend on which features are on. `blackbox_metadata` runs `cargo metadata --format-version 1 --locked --all-features`: `--all-features` so an edge behind an off-by-default feature is in the graph, `--locked` so the check never rewrites `Cargo.lock`. It compiles nothing and runs in under a second.
- **Fails closed:** an unparsable document, no resolve graph, a missing or duplicated `wyrd-validate` package, a package missing from the graph, a record with no `id` or `name`, or a dependency edge with a missing, empty, or unknown kind are all errors, never a clean pass.
- **Wiring** (`xtask/src/main.rs`): the guards are listed as data in `xtask::repo_guard::CI_GUARDS` (lib target), and `run_ci_guard` is the single dispatcher. `run_ci` and the new `ci-dry-run` both call `run_ci_steps_in`, which runs the real guards. The only difference is that `ci-dry-run` prints the cargo steps instead of running them. The two existing guards now take the workspace root as an argument instead of looking it up. Nothing else about them changed.
- **Docs:** one bullet in `docs/design/architecture/08-crosscutting-concepts.md` §8.10 describes the repository checks, the new rule and its policy, and the two new commands.

**Known limits, left for follow-up:** build-dependencies (`kind: "build"`) are not checked yet, and a crate counts as Wyrd's by its `wyrd-` name prefix, not by workspace membership. Both match the scope this issue set and are pinned by tests (`a_build_dependency_on_a_wyrd_crate_is_not_a_normal_edge`). Widening either is a separate change. Until then, the error text "Only a dev-dependency may name a wyrd-* crate" (`xtask/src/main.rs:1551`) and the doc's "Dev-dependencies stay unconstrained" are stricter than what the code checks for build-dependencies.

## Verification
Line numbers are on `main` with this patch applied (it applies cleanly on `36f006d`).

- **Claim:** the gate runs the new check, and a forbidden dependency stops it before any cargo step.
  - **Checked:** `xtask/src/repo_guard.rs:576-600` (`CiGuard`, `CI_GUARDS`). `xtask/src/main.rs:1510` (`run_ci_guard`), `:1658` (`run_ci_steps_in`), `:1681` (`run_ci` calls it), `:1699` (`ci-dry-run` calls the same function).
  - **Test:** `xtask/tests/blackbox_dependency_guard.rs:229` `the_gate_stops_at_the_blackbox_guard_on_a_forbidden_dependency` sets up a small workspace where `wyrd-validate` depends on `wyrd-core`. Both existing checks pass, the new one fails naming `wyrd-core`, and no cargo step is printed. `:262` checks that all three checks run in order before the first `cargo fmt`. The unit tests `xtask/src/main.rs:2151` and `:2169` check the order and that a failing check stops the run.
- **Claim:** the check actually catches violations and does not flag dev-dependencies.
  - **Checked:** `xtask/src/repo_guard.rs:651-690` (edge-kind rules), `:736` (`scan_blackbox_closure`).
  - **Test:** `blackbox_dependency_guard.rs:524` direct normal dependency is one violation naming the crate. `:543` the same edge as dev is allowed. `:560` optional, off-by-default is a violation. `:587` reached through a middle crate is a violation naming the path. `:355` a real workspace with an optional feature-gated crate is caught. `:344` the real workspace is clean.
- **Claim:** metadata the check cannot read is a failure, not a pass.
  - **Checked:** `xtask/src/repo_guard.rs:684` (empty `dep_kinds`) and the error branches of `:736` onward.
  - **Test:** `blackbox_dependency_guard.rs:663-830`, one test per broken-input case (unparsable, no resolve graph, missing or duplicate package, missing `id` or `name`, missing, empty, or unknown kinds).
- **Claim:** the check never writes `Cargo.lock`.
  - **Checked:** `xtask/src/repo_guard.rs:622-623` (`--locked`, `--all-features`).
  - **Test:** `blackbox_dependency_guard.rs:395` `the_guard_never_writes_the_lock_file_it_audits`.
- **Red/green:** with the production changes reverted and the new test file kept, all 29 tests fail. With the patch, all 29 pass, plus 25 xtask unit tests. On their own, the reverted-code failures only show that the commands were missing. The mutation runs below show the assertions pin the logic.
- **Mutation testing:** `cargo mutants --in-diff` found 42 mutants on this change: 41 were caught. The one that survived replaces the whole of `run_ci` with success, and no test can cover that without running the entire gate. Hand mutants on the dispatcher, the guard loop, `CI_GUARDS`, `--locked`, `--all-features`, and the empty-kinds check all make at least one test fail.
- **Full gate:** `cargo xtask ci` passes on the stacked tree (with #774): every step ran, and the new check gave a clean result on the real workspace.

Fixes #775
