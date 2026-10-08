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
