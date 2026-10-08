# Build notes — #775 xtask blackbox dependency-closure guard (iteration 3)

Target: `getwyrd/wyrd` @ main, stacked on #774. The patch applies to the cycle worktree's base
`4bda59c` (`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which carries #774's
`crates/validate`). Line numbers are **post-patch** unless marked `base:`.

Files: `xtask/src/repo_guard.rs` (+325/-3), `xtask/src/main.rs` (+248/-31),
`docs/design/architecture/08-crosscutting-concepts.md` (+1),
`xtask/tests/blackbox_dependency_guard.rs` (new, 29 tests).

## What this iteration changes, and why

Iteration 2's scanner (`repo_guard.rs`) passed review and is kept as is, except that the unused
`CiGuard::name` is gone. The two carry-forward findings were both about the wiring and the docs.

### 1. The real guard dispatch is now exercised with a forbidden dependency (T5)

Last round, `run_ci_guard -> Ok(())` and `BlackboxClosure => Ok(())` survived every test: the
tests drove `run_ci_steps` with printing or recording closures, never with the dispatcher
`run_ci` actually passed. Each round the reviewer moved one hop further out, so this round I
removed the hop instead of testing one more of them.

- The guards take the workspace root as a parameter: `run_gitlink_guard(root)`
  (`main.rs:1394`), `run_unsafe_forbid_guard(root)` (`main.rs:1464`), `run_blackbox_guard(root)`
  (`main.rs:1532`). The only edit to the two #616 guards is `workspace_root()` → the `root`
  parameter (`base: main.rs:1375`, `:1451`), so with `root = workspace_root()` their verdicts are
  unchanged.
- `run_ci_guard(guard, root)` (`main.rs:1510`) is the single dispatcher.
- New `run_ci_steps_in(root, exec)` (`main.rs:1658`) wraps `run_ci_steps` with the REAL guards
  (`|guard| run_ci_guard(guard, root)`) and the real env lookup. `run_ci` calls it with this
  workspace and `cargo` (`main.rs:1681`, replacing `base: main.rs:1557-1561`).
- New `cargo xtask ci-dry-run [--workspace DIR]` (`main.rs:1699`, dispatched at `:120`) calls the
  SAME `run_ci_steps_in` with a printing `exec`. That is the only difference from `run_ci`'s
  call, so the test can run the gate's real guard phase over a planted workspace.
- `ci-plan` (iteration 2) is removed: it printed guard names without running them, which is
  exactly what let the dispatcher mutants survive.
- `blackbox-guard` with no argument now goes through `run_ci_guard(CiGuard::BlackboxClosure, …)`
  too (`main.rs:1563`). `blackbox-guard --workspace` is removed (the planted-workspace tests use
  `ci-dry-run --workspace` instead), so the CLI surface is `blackbox-guard [--metadata FILE]` and
  `ci-dry-run [--workspace DIR]`.

Tests that pin this:

- `the_gate_stops_at_the_blackbox_guard_on_a_forbidden_dependency` (`tests/…:229`): a planted
  workspace (laid out like this repo — `crates/*`, every root `forbid(unsafe_code)`, its own
  `git init`, so the two #616 guards pass) whose `wyrd-validate` depends on `wyrd-core`.
  `ci-dry-run` must fail with exactly one violation naming `wyrd-core`, after both #616 guards
  printed their pass lines, and with **no** cargo step printed.
- `the_gate_runs_every_repo_guard_before_any_cargo_step` (`tests/…:262`): the same layout with a
  dev-dependency instead. All three guard banners and pass lines appear in `CI_GUARDS` order,
  before the first `cargo fmt` line.
- Bin unit tests: `ci_runs_every_repo_guard_before_the_cargo_steps` (`main.rs:2151`) and new
  `a_failing_guard_stops_ci_before_any_cargo_step` (`main.rs:2169`), which pins that a guard's
  `Err` is returned unchanged and nothing runs after it.

### 2. Docs currency (T4, gating)

Added one bullet to the living architecture doc, `docs/design/architecture/08-crosscutting-concepts.md:119`
(§8.10 Build, test, and CI): the repo-hygiene guards as a class, the blackbox property and its
policy (`--locked --all-features`, normal edges only, declared list too, dev-deps unconstrained,
fail closed), and the two new commands. The iteration-2 adversary argued this was a false
positive because §8.10 says it does not restate the pipeline (`08-…md:116`). I kept that
sentence true: the bullet describes the enforced property and the developer entry points, not
the pipeline's steps or order. Adding it costs one line; rejecting the finding would have cost
a human decision at sign-off.

### 3. Minor survivor from last round: unknown-option rejection

`unknown_arguments_are_a_usage_error` (`tests/…:308`) covers bad flags and missing values for
both commands, including the removed `blackbox-guard --workspace`, and a `--workspace` that does
not exist. It asserts the usage text, not just failure, so a match guard mutated to `true` (which
would read the file and fail differently) still goes red.

## Alternatives ruled out (with cost)

- **Route only `blackbox-guard --workspace` through `run_ci_guard`, keep `ci-plan`.** About 10
  fewer lines than `ci-dry-run`, and it kills the two dispatcher mutants. But the guard loop and
  the real dispatcher would still never run together: `guard(check)?` → `let _ = guard(check);`
  in the loop would survive, because in that design no executor passed to the loop ever returns
  `Err` (reasoned from iteration 2's tests, not run). In this design `ci-dry-run` over a
  forbidden workspace catches it (see the mutant table), and so does the new
  `a_failing_guard_stops…` unit test.
- **Make `run_ci` itself testable** (inject every step: typos, docs, machete, deny, conformance,
  statics, deploy-guard, dst). Roughly 60–80 lines of new seams in `run_ci` for the 8 steps that
  are not #775's, and the outer `run_ci()` that builds the real executors would still be the
  untestable boundary. Out of proportion to this issue.
- **`ci --dry-run` instead of a new task name.** On the base, `ci` ignores extra arguments, so the
  red leg would run the whole real gate from inside `cargo test` (nested cargo, minutes, lock
  waits until the gate's timeout). A new task name makes the base print `unknown task` and exit 1
  at once. Same reasoning as iteration 2.
- **Call new lib API from the named test.** It would not compile on the base, and
  `run-verify.sh`'s `_red_verdict` reports a red leg with 0 tests ran as UNVERIFIABLE. Every test
  in the named file drives the binary for that reason.
- **Keep `blackbox-guard --workspace` as well as `ci-dry-run --workspace`.** One more flag to
  document and test, and it would test the guard outside the gate's wiring, which is the gap this
  round closes.

## Verification (red → green)

Runner: `cargo test -p xtask --test blackbox_dependency_guard` under `timeout 900`, judged with
the gate's own hooks (`engine/scripts/run-verify.sh --classify / --tests-ran / --red-verdict`).
I did not run `run-verify.sh` end to end: it creates a `../wyrd-verify` worktree and a
`pdca-verify` branch beside the primary checkout, outside the roots the builder may write to.

- `--classify` → `ADDED_TEST xtask/tests/blackbox_dependency_guard.rs`, `CRATE xtask`.
- GREEN (patch applied): 29 ran, 29 passed. Bin unit tests: 25 passed.
- RED (`git checkout HEAD -- xtask/src/main.rs xtask/src/repo_guard.rs
  docs/design/architecture/08-crosscutting-concepts.md`, test kept): rc 101, `--tests-ran` 29,
  29 failed, `--red-verdict 101 29` → `PASS`. All 29 outputs contain `xtask: unknown task`, and
  every panic is one of the test's own assertions (none in fixture setup). Files restored
  afterwards from saved copies; `git diff HEAD` was byte-identical to `patch.diff`.

### Hand mutants (each applied alone, tests run with `--no-fail-fast`, file restored)

Three rows compiled under the workspace lints and ran as-is (`run_ci_guard` body, swallowed
error, `CI_GUARDS` drop). Everything else ran with `RUSTFLAGS=--cap-lints
warn` in a scratch target dir: a mutant that leaves a function unused does not compile under the
deny-warnings lints and silently runs no tests. My first pass reported three such mutants as
"no failures" for exactly that reason; re-run with capped lints, all three go red.

| Mutant | Goes red |
|---|---|
| `run_ci_guard` body → `Ok(())` | 7 tests incl. `the_gate_stops_at…`, `the_gate_runs_every…`, `the_real_workspace_has_no_violation` |
| `BlackboxClosure => Ok(())` in `run_ci_guard` | 6 tests incl. both `the_gate_…` tests |
| `run_ci_steps_in`'s guard closure → `\|_\| Ok(())` | 6 tests incl. both `the_gate_…` tests, `a_dry_run_with_no_argument…` |
| `guard(check)?` → `let _ = guard(check);` | `the_gate_stops_at…`, `a_failing_guard_stops…`, 3 real-cargo tests |
| guard loop deleted | 8 tests incl. both bin tests and both `the_gate_…` tests |
| `BlackboxClosure` dropped from `CI_GUARDS` | 7 tests incl. both bin tests and both `the_gate_…` tests |
| `ci-dry-run`: delete the no-argument arm | `a_dry_run_with_no_argument_checks_this_workspace` |
| `ci-dry-run`: `--workspace` match guard → `true` | `unknown_arguments_are_a_usage_error` |
| `blackbox-guard`: `--metadata` match guard → `true` | `unknown_arguments_are_a_usage_error` |
| `blackbox-guard`: delete the no-argument arm | `the_real_workspace_has_no_violation` |
| drop `--all-features` | `an_optional_crate_behind_an_off_by_default_feature_is_walked` |
| drop `--locked` | `the_guard_never_writes_the_lock_file_it_audits` |
| skip the empty-`dep_kinds` check | `an_edge_with_empty_dep_kinds_is_refused` |
| `"dev" \|\| "build"` guard → `true` | `an_edge_of_unknown_kind_is_refused`, `a_declared_dependency_of_unknown_kind_is_refused` |
| mark `seen` before classifying the edge | `a_crate_first_met_on_a_dev_edge_is_still_walked_on_a_normal_path` |

### cargo-mutants on the diff

`cargo mutants --in-place --in-diff patch.diff -p xtask --cap-lints true` (cargo-mutants 27.1.0),
run in a git-backed scratch copy of the worktree (tracked files plus the new test, `git init` +
one commit), so the pre-existing git-index test can pass on the baseline:

- Baseline: `cargo test --package=xtask` green, every xtask target (incl. 29 + 25 new tests).
- **42 mutants: 41 caught, 1 missed, 0 unviable, 0 timeouts.**
- The one miss: `main.rs:1675:5: replace run_ci -> Result<(), String> with Ok(())` — the whole
  gate replaced by success; see "Residual gaps".
- Caught include every dispatcher and wiring mutant last round's reviewer named
  (`run_ci_guard`, `run_ci_steps`, `run_ci_steps_in`, `run_blackbox_guard`, `blackbox_verdict`),
  both `delete match arm []` and all four match-guard mutants of the two CLI commands, and the
  #616 guards themselves (`run_gitlink_guard -> Ok(())` is caught by the two `the_gate_…` tests
  and `a_dry_run_with_no_argument…`, because they assert the guard's pass line).

The copy was made before the comment-only edit at `main.rs:1587`; that edit replaces one line,
so every line number above still matches the final source.

### Other checks

- `cargo fmt --all -- --check` clean; `typos` clean on all four files; `lint_docs.py` OK;
  `cargo clippy -p xtask --all-targets` clean under the workspace lints; `cargo test -p xtask`
  all targets green (incl. `repo_hygiene_guards` 29, `fdb_harness` 30).
- **Full gate:** `./engine/xtask.sh ci` (the configured `[gates] runner`, run in the cycle
  worktree, `timeout 7000`): `xtask ci: all checks passed`, rc 0. Every step ran, none
  warn-skipped: typos, docs lint, docs render, gitlink-guard, unsafe-guard, **blackbox-guard**
  (clean verdict on the real workspace), fmt, clippy, build, test (incl. the 29 new tests and
  both new bin tests), cargo-machete, the cargo-deny runs, statics, deploy-guard, DST clippy and
  test. One comment-only edit came after this run (`main.rs:1587`, "`run_ci` passes" →
  "`run_ci_steps_in` passes" in `run_ci_steps`' doc); fmt, typos, docs lint, clippy and
  `cargo test -p xtask` were re-run after it, all green.

## Refuting my own test

- **(a) Genuine red?** Yes. With `main.rs`, `repo_guard.rs` and the doc reverted to `4bda59c`
  and the test kept, all 29 tests compile, run and fail (rc 101, 29 failed, verdict `PASS`).
  Each assertion needs the guard's own text or exit status; the base's `unknown task` output
  satisfies none of them.
- **(b) Production path?** Yes. Every test runs the real `xtask` binary
  (`CARGO_BIN_EXE_xtask`). `ci-dry-run --workspace` runs the production `run_ci_steps_in` →
  `run_ci_steps` loop → `run_ci_guard` → the three real guards, including the real
  `cargo metadata --locked --all-features` call. Only the cargo build/test steps are replaced
  by printing, and that executor is the one injection point `run_ci` also uses.
  `blackbox-guard --metadata` runs the production `scan_blackbox_closure`. No copy of the scan or
  the wiring exists in the test.
- **(c) Fixture includes the fault?** Yes. The gate-level red fixture is a real cargo workspace
  whose `wyrd-validate` has a normal dependency on `wyrd-core`; the other red fixtures contain the
  forbidden edge itself (optional feature-gated crate, rename, direct, 3-hop transitive) or the
  malformed input (empty `dep_kinds`, unknown kinds, missing id/name/nodes, no resolve). The
  planted workspaces pass the two #616 guards on purpose, so the only thing that can stop the
  dry run is the blackbox guard.

## Residual gaps and notes for sign-off

- **The outermost boundary stays untested**: replacing all of `run_ci` with `Ok(())`, or deleting
  its `run_ci_steps_in(&workspace_root(), …)` line (`main.rs:1681`), survives. That line also
  runs fmt/clippy/build/test, which have the same boundary today; no test can run the whole gate.
  Everything below it is now exercised with a forbidden input.
- **C5 (cargo-mutants gate) environment fault, not caused by this patch.** Last round's C5 died on
  its unmutated baseline because the pre-existing `scan_gitlinks_is_green_over_the_real_index`
  (`xtask/tests/repo_hygiene_guards.rs`) needs a git index, and the copy cargo-mutants makes has
  no `.git`. My new tests do not need the repo's index (`a_dry_run_with_no_argument…` asserts only
  the start of the run, so it passes either way). The C5 row will likely fail the same way again
  until `scripts/mutants-in-diff` runs in a git-backed copy or `--in-place`.
- **C4-diff-cov environment fault.** Last round its worktree lacked #774's `crates/validate`, so
  the guard correctly refused ("no `wyrd-validate` package"). Same for this round if the stack
  base is not applied there. It also means: **merge #774 before this**, or `cargo xtask ci` goes
  red for everyone at the blackbox guard.
- **Deferred to the human (from round 2, not changed here):** build-dependencies are not
  constrained (brief: follow only `kind: null`), and the match is by `wyrd-` name prefix, not
  workspace membership (brief: the `wyrd-` prefix). Both are pinned by tests as currently scoped.
- New ways for `cargo xtask ci` to fail: a stale `Cargo.lock` (`--locked`), and an offline machine
  that never fetched the optional TiKV/FDB crates (`--all-features` loads them; `cargo deny
  --all-features` in the same gate already needs them).
- The `cargo metadata` and `git` spawns have no timeout, same as the existing #616 guards. They are
  synchronous, not awaits.
- No new external dependency: cargo and git, which the gate already requires.
