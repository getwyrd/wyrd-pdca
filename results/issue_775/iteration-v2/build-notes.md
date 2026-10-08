# Build notes — #775 xtask blackbox dependency-closure guard (iteration 2)

Target: `getwyrd/wyrd` @ main, stacked on #774. The patch applies to the cycle worktree's base
`4bda59c` (`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which carries #774's
`crates/validate`). Line numbers below are **post-patch** unless marked `base:`.

Files: `xtask/src/repo_guard.rs` (+336/-3), `xtask/src/main.rs` (+178/-20),
`xtask/tests/blackbox_dependency_guard.rs` (new, 787 lines, 27 tests).

## What changed

**Lib (`xtask/src/repo_guard.rs`)**

- `CiGuard` enum (`:576`) + `CiGuard::name` (`:588`) + `CI_GUARDS` (`:606`) — the guard
  registration as data in the lib target (criterion 1). Order kept: gitlink, unsafe-forbid,
  then the new blackbox guard.
- `BLACKBOX_PACKAGE` (`:614`), `WYRD_CRATE_PREFIX` (`:617`), `BLACKBOX_METADATA_ARGS`
  (`:629`: `metadata --format-version 1 --locked --all-features`, no `--no-deps`), and
  `blackbox_metadata(dir)` (`:640`) which spawns exactly those args.
- `is_normal_kind` (`:662`): `null` → normal, `"dev"`/`"build"` → not normal, anything else
  or a missing field → `Err`.
- `edge_is_normal` (`:685`): missing **or empty** `dep_kinds` → `Err` (`:695`, the
  iteration-1 T4/C5 finding); classifies every entry, normal if any entry is.
- `scan_blackbox_closure` (`:747`): pure JSON in, violations out. Decodes every package
  record's `id` and `name` up front (`:762`, `:764`) — names only ever come from records,
  never from ids or edge names; duplicate subject → `Err` (`:770`); missing subject → `Err`
  (`:779`, mentions `[workspace] members`); manifest check over the subject's declared
  `dependencies` (normal kind, optional or not); then a BFS over normal resolve edges from
  the subject (`:851`) that classifies every edge before the `seen` check, so a crate first
  met on a dev edge is still walked when a normal path reaches it. Subject in no node
  (`:828`), reached package in no node (`:837`), edge to a package with no record (`:855`)
  are all `Err`. One violation per crate, merging both checks and saying which fired.

**Bin (`xtask/src/main.rs`)**

- `run_ci_steps` (`:1598`) takes a new `guard` executor and runs every `CI_GUARDS` entry
  through it before `cargo fmt` (`:1607`). This replaces the two direct calls in `run_ci`
  (`base: xtask/src/main.rs:1557-1558`), so the guards are now inside the function the
  recording-executor tests drive. `run_ci` (`:1648`) passes `run_ci_guard` (`:1659`).
- `run_ci_guard` (`:1505`): exhaustive `match` from `CiGuard` to the real guard functions.
  `run_gitlink_guard` / `run_unsafe_forbid_guard` bodies are untouched (verdicts cannot
  change).
- `run_blackbox_guard` (`:1527`) mirrors `run_unsafe_forbid_guard` (`base: :1443-1484`):
  `print_step`, `cargo metadata`, pure scan, violations joined into one `Err`
  (`blackbox_verdict`, `:1535`).
- Two new subcommands, documented in the module header (`:13`, `:16`) and usage:
  - `blackbox-guard [--workspace DIR | --metadata FILE]` (`:120`, `:1559`);
  - `ci-plan` (`:119`, `:1679`): runs `run_ci_steps` with printing executors.
- Bin unit test `ci_runs_every_repo_guard_before_the_cargo_steps` (`:2114`) with a
  `recorded_steps` helper (`:2091`); `recorded_invocations` (`:2082`) now filters the cargo
  steps out of it, so the four existing wiring tests are unchanged in meaning.

## How each carry-forward item is addressed

1. **Empty `dep_kinds` passed silently** → `edge_is_normal` rejects `[]` (`repo_guard.rs:695`).
   Test `an_edge_with_empty_dep_kinds_is_refused` (`tests/…:652`) plants the reviewer's exact
   case (`validate -> middle` normal, `middle -> wyrd-core` with `[]`) and the direct-edge
   variant. Hand mutant "skip the empty check" → this test goes red.
2. **Unknown-kind mutants survived** → `an_edge_of_unknown_kind_is_refused` (`:671`: unknown
   kind, unknown kind *after* a normal one, entry with no `kind`) and
   `a_declared_dependency_of_unknown_kind_is_refused` (`:690`). A `build` kind test (`:289`)
   pins the other side of the `"dev" || "build"` guard. Hand mutants "match guard → `true`"
   and "stop classifying after the first normal kind" → red.
3. **Criterion 1 not pinned (deleting the guard call from `run_ci` stayed green)** → the
   guards now run inside `run_ci_steps`, and two tests drive that function: the bin unit test
   (as the reviewer asked) and `the_ci_gate_runs_the_blackbox_guard_before_any_cargo_step`
   (`tests/…:221`) through `cargo xtask ci-plan`. Hand mutants "delete the guard loop" and
   "drop `BlackboxClosure` from `CI_GUARDS`" → both tests red (confirmed with
   `--no-fail-fast`).
4. **Registration test needed a git index (broke the C5 mutants baseline)** → nothing in the
   new test file spawns git. `ci-plan` prints the plan without running any guard; the real
   guard tests only spawn `cargo metadata` / `cargo generate-lockfile --offline`.

## Why the named test drives the binary, not lib functions

`engine/scripts/run-verify.sh` (C4-verify) keeps the added test file, reverts the production
files, and judges the red leg by `_red_verdict`: a test file that does not **compile** against
the base reports `UNVERIFIABLE`, not red (`run-verify.sh` `_red_verdict`, the `!=0 / 0` cell).
Any call to `xtask::repo_guard::scan_blackbox_closure` / `CI_GUARDS` from
`xtask/tests/blackbox_dependency_guard.rs` would not compile on the base, so all 27 tests would
be UNVERIFIABLE. Driving `CARGO_BIN_EXE_xtask` keeps the file compiling on the base, where every
test fails by assertion (`xtask: unknown task`). This is also why the reviewer's suggestion to
"reuse `run_ci_guards`' injected executor" from the integration test was not taken literally;
the bin unit test covers that ask instead, and `ci-plan` gives the named file the same check.

## Alternatives ruled out (with cost)

- **Move the #616 guard bodies into the lib (iteration 1's shape).** Iteration 1's main.rs hunk
  was `@@ -1363,123 +1373,33 @@` — ~120 lines deleted from main.rs and re-added in
  repo_guard.rs, with no behaviour change. Not needed: the lib-side list plus a 9-line
  exhaustive dispatch (`main.rs:1505-1512`) gives a testable registration, and leaving the two
  bodies byte-identical makes "their verdicts must not change" true by construction.
- **Move `run_ci_steps` into the lib** so an integration test could call it. ~45 lines moved
  plus doc edits at `lib.rs:35-37` and `:73-75`, and it still would not help the named test:
  calling it from `xtask/tests/` is new lib API, which fails to compile in the red leg (above).
- **A `repo-guards` subcommand that runs all guards for real** (iteration 1). Requires a git
  index for the gitlink guard; that is what made the C5 baseline fail.
- **`ci --dry-run` instead of a new `ci-plan` name.** On the base, `ci` ignores extra args, so
  the red leg would run the *whole* gate from inside `cargo test` (nested cargo on the same
  target dir — at best minutes, at worst a lock wait until the gate's timeout). A new task name
  makes the base print `unknown task` and exit 1 immediately.
- **Synthetic documents only (no `--workspace`).** Then nothing pins `--all-features` or
  `--locked`: a planted document already contains whatever graph the test wrote. The real-cargo
  tests (`:474`, `:524`) are the only ones that go red when either flag is dropped (confirmed by
  hand mutants below). Cost: ~70 test lines and ~5 lines of argument matching.

## Verification (red → green)

Commands used: `timeout 900 cargo test --quiet -p xtask --test blackbox_dependency_guard`
in the cycle worktree, judged with the gate's own hooks
(`run-verify.sh --classify / --tests-ran / --red-verdict`).

- `--classify` → `ADDED_TEST xtask/tests/blackbox_dependency_guard.rs`, `CRATE xtask`.
- GREEN (patch applied): rc 0, 27 tests ran, 27 passed.
- RED (`git checkout HEAD -- xtask/src/main.rs xtask/src/repo_guard.rs`, test kept): rc 101,
  27 tests ran, 27 failed; `--red-verdict 101 27` → `PASS`. Files restored afterwards and
  `git diff HEAD` re-checked byte-identical to `patch.diff`.

I did not run `run-verify.sh` end to end: it creates a `../wyrd-verify` worktree and a
`pdca-verify` branch beside the primary checkout, which is outside the roots the builder may
write to. The steps above are the same two legs, run with a timeout, judged by the same
functions. Check's C4-verify will run the real script.

Hand mutants (each applied alone, test run, file restored; `--no-fail-fast` where noted):

| Mutant | Goes red |
|---|---|
| drop `--all-features` from `BLACKBOX_METADATA_ARGS` | `an_optional_crate_behind_an_off_by_default_feature_is_walked` |
| drop `--locked` | `the_guard_never_writes_the_lock_file_it_audits` |
| delete the guard loop in `run_ci_steps` | bin `ci_runs_every_repo_guard_before_the_cargo_steps` + `the_ci_gate_runs_the_blackbox_guard_before_any_cargo_step` |
| drop `BlackboxClosure` from `CI_GUARDS` | same two |
| skip the empty-`dep_kinds` check | `an_edge_with_empty_dep_kinds_is_refused` |
| `"dev" \|\| "build"` guard → `true` | `an_edge_of_unknown_kind_is_refused`, `a_declared_dependency_of_unknown_kind_is_refused` |
| stop classifying after the first normal kind | `an_edge_of_unknown_kind_is_refused` |
| mark `seen` before classifying the edge | `a_crate_first_met_on_a_dev_edge_is_still_walked_on_a_normal_path` |
| fall back to the id when a package record is missing | `structurally_incomplete_documents_are_refused` |

Other checks: `cargo fmt --all -- --check` clean (after `cargo fmt --all`); `cargo clippy -p
xtask --all-targets` clean under the workspace lints (one `type_complexity` hit fixed with a
`Breakage` alias, `tests/…:710`); `typos` clean on the three files; `cargo test -p xtask` all
targets green (existing `repo_hygiene_guards`, `fdb_harness`, etc. unchanged). Full
`cargo xtask ci` via `./engine/xtask.sh ci`: see the last section.

## Refuting my own test

- **(a) Genuine red?** Yes. With `xtask/src/main.rs` and `xtask/src/repo_guard.rs` reverted to
  `4bda59c` and the test file kept, all 27 tests compile, run, and fail (rc 101, 27 failed).
  Each assertion is specific enough that the base's `unknown task` output cannot satisfy it
  (refusals assert the guard's own reason text; passes assert exit 0 and, for the real tree,
  the clean-verdict line).
- **(b) Production path?** Yes. Every test runs the real `xtask` binary
  (`CARGO_BIN_EXE_xtask`): `--metadata` runs the production `scan_blackbox_closure`;
  `--workspace` and the no-argument form also run the production `blackbox_metadata` (the real
  `cargo metadata --locked --all-features` call); `ci-plan` runs the production
  `run_ci_steps`. No copy of the scan exists in the test.
- **(c) Fixture includes the fault?** Yes. The red fixtures contain the forbidden edge itself:
  a normal `wyrd-core` edge, an optional off-by-default `wyrd-core` declaration, a 3-hop
  transitive edge, real cargo workspaces whose `wyrd-validate` depends on `wyrd-core` through an
  optional feature-gated crate or a rename, and every malformed-input case the brief and
  reviewers named (empty `dep_kinds`, unknown kinds, missing id/name, missing nodes, no
  resolve). Package ids in the planted documents are name-free, and edge names are junk, so a
  scan that read names from either would miss every violation.

## Residual gaps / notes for sign-off

- **`run_ci`'s own body stays untestable**, as before: replacing `&mut run_ci_guard` with a
  no-op closure in `run_ci` (`main.rs:1659`) would keep every test green — the same is true of
  the existing `&mut |args| cargo(args)` executor. A cargo-mutants `run_ci_guard -> Ok(())`
  mutant will also survive (the real tree is clean, so the real guard and the mutant both
  return `Ok`). The wiring that *is* pinned is `run_ci_steps` + `CI_GUARDS`, which is where the
  iteration-1 gap was.
- **`build` dependencies are not constrained.** The brief says to follow only `kind: null`
  edges and report only `kind: null` declarations, so a `[build-dependencies] wyrd-core` would
  pass (pinned by `a_build_dependency_on_a_wyrd_crate_is_not_a_normal_edge`). A build script
  linking a Wyrd crate could still feed Wyrd's types into generated code. The human may want a
  follow-up if that matters for the blackbox property.
- **`--all-features` needs every optional crate's source.** `cargo metadata` without
  `--no-deps` loads all packages in the resolved graph. On this host they are cached (the guard
  passes offline in about a second); a fresh offline laptop that never fetched the TiKV / FDB
  trees would see the guard fail with cargo's download error. That is fail-closed, and
  `cargo deny --all-features` in the same gate has the same need.
- **`--locked` makes a stale `Cargo.lock` a guard failure** before the build step. Intended by
  the brief; noted because it is a new way for `cargo xtask ci` to fail.
- The `cargo metadata` spawn is not wrapped in a timeout — same as the existing unsafe guard's
  `cargo metadata --no-deps` (`base: main.rs:1452-1456`).
- No new external dependency: everything runs with the cargo toolchain already required.

## Full gate

`./engine/xtask.sh ci` (the configured `[gates] runner`, `subcmd = "ci"`, run in the cycle
worktree with the patch applied, under a 7000 s timeout): **`xtask ci: all checks passed`,
rc 0.** Every step ran, none warn-skipped, in this order: typos, docs lint, docs render,
`xtask gitlink-guard`, `xtask unsafe-guard`, **`xtask blackbox-guard`** (passed: "wyrd-validate's
normal dependency closure holds no wyrd-* crate"), fmt, clippy, build, test (incl.
`blackbox_dependency_guard` 27/27 and the bin test `ci_runs_every_repo_guard_before_the_cargo_steps`),
cargo-machete, the three `cargo deny` runs, statics, deploy-guard, DST clippy + test.
`Cargo.lock` was unchanged after the run.
