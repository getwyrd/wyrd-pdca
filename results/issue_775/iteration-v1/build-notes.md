# Build notes — issue 775 (xtask blackbox dependency-closure guard)

Worktree base: `4bda59c` (`origin/pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`),
which carries #774 (`crates/validate`, package `wyrd-validate`). The files this patch
touches are the same on this base as on `main` @ `65ca4fd`, and the brief's citations still
line up (`repo_guard.rs:238/387/500`, `main.rs:1373/1443/1486-1558`, `lib.rs:81`).
Line numbers below are post-patch in the worktree.

## What changed

- `xtask/src/repo_guard.rs`
  - `:586` `BLACKBOX_METADATA_ARGS` = `metadata --format-version 1 --locked --all-features`,
    without `--no-deps` (brief: design constraint 1).
  - `:608` `is_normal_kind`: `null` means normal; `"dev"`/`"build"` mean not normal. A missing
    or unknown `kind` is an `Err`, so the guard never guesses.
  - `:643` `scan_blackbox_closure(json) -> Result<Vec<String>, String>`, the pure function.
    - It decodes every package record strictly: a record with no `id` or no `name` is an `Err`.
      Names always come from package records, never from ids or from the edge `name` (the
      edge name is the extern-crate name, which a rename changes).
      Real ids are opaque: xtask's own id is `path+file:///…/xtask#0.0.0`, with no name in it.
    - The guard's subject (`wyrd-validate`) must exist exactly once. If it is missing, the
      result is `Err` (brief: "must not become satisfiable by removing `crates/validate` from
      members").
    - Manifest check: any declared `wyrd-*` dependency with `kind: null`, optional or not.
    - Graph check: a breadth-first walk over `resolve.nodes` that follows only edges with a
      `kind: null` entry. It reports every reached `wyrd-*` package together with the
      shortest path to it.
    - When both checks hit the same crate, the findings are merged into one line per crate,
      and the wording says which check fired. This is what makes "exactly one violation"
      hold for a direct normal edge, which both checks see.
    - Every one of these is an `Err`: missing `resolve`/`nodes`, a subject with no node, a
      reached package with no node, an edge with no `pkg`, an edge with no `dep_kinds`, an
      edge to a package with no record.
  - `:804` `enum CiGuard { Gitlink, UnsafeForbid, BlackboxClosure }`, plus `:819` `CI_GUARDS`
    and `:828` `run_ci_guards(exec)`. This is the lib-side registration as data (brief:
    criterion 1), modelled on `feature_gated_checks` (`lib.rs:81`) and on `run_ci_steps`
    taking an injected executor.
  - `:869` / `:937`: `run_gitlink_guard` / `run_unsafe_forbid_guard` moved here from
    `main.rs:1373` / `:1443` (base). Their bodies are copied unchanged, except that
    `workspace_root()` is now a `root` parameter and the `print_step` banner moved into
    `CiGuard::step`. The banners and the messages are the same text, so their verdicts don't
    change.
  - `:974` `blackbox_metadata(root)` runs `cargo metadata`. `:994` / `:1003` are the runner
    and `blackbox_guard_verdict(json)`, which formats the result into one error.
- `xtask/src/main.rs`
  - `:1480`: `run_ci` now calls `run_repo_guards()` where it used to call the two guard
    functions directly (base `:1557-1558`).
  - `:1381` `run_repo_guards()` iterates `xtask::repo_guard::run_ci_guards`.
  - `:1393` `run_blackbox_guard(file)`.
  - `:120-121` add the `repo-guards` and `blackbox-guard [METADATA_JSON]` subcommands. The
    module doc (`:5-19`) and `print_usage` are updated to match (docs currency: the xtask
    subcommand list lives in that module doc; `docs/design/architecture/08-…:116`
    deliberately doesn't restate the pipeline).
- `xtask/tests/repo_hygiene_guards.rs:2`: fixed the header comment that named the moved
  functions.
- `xtask/tests/blackbox_dependency_guard.rs`: the new test, with 15 tests.

## Why the test drives the `xtask` binary, not the lib API

My first version called `xtask::repo_guard::{scan_blackbox_closure, CI_GUARDS, …}` directly.
Its green leg passed 18/18, but `run-verify.sh` rated the red leg **UNVERIFIABLE**. With the
fix reverted, the test file doesn't compile (E0432: the symbols don't exist), so no test ran.
The brief promises a genuine runtime red. Every lib symbol the test could name is new, so the
only interface that exists on the base is the binary itself (`CARGO_BIN_EXE_xtask`).

So I added two subcommands, following the existing precedent of `statics`, which is also a
`ci` step with its own subcommand (`main.rs:111`):

- `cargo xtask repo-guards` runs the same `run_repo_guards()` that `run_ci` calls. The test
  asserts the three guard banners appear in order and that the blackbox guard reaches a clean
  verdict on the real tree (criterion 1).
- `cargo xtask blackbox-guard <file>` runs `blackbox_guard_verdict` (and so the production
  `scan_blackbox_closure`) over a planted document. With no file, it runs the real workspace
  guard.

On the base, both subcommands hit `unknown task`, so every test runs and fails. A lib-only
unit test module would duplicate what the binary tests already cover, so I didn't add one.

Rejected: a `WYRD_BLACKBOX_METADATA` environment override read by the `ci` guard. That would
be a way to bypass the gate inside `ci` itself. The file argument exists only on the
standalone subcommand; `ci` always runs `cargo metadata`.

## Remaining gap (criterion 1)

The integration test proves that `cargo xtask repo-guards` runs the blackbox guard, and that
`run_repo_guards` iterates `CI_GUARDS`. It can't see inside `run_ci`'s body, which lives in
the binary target, so the tie between the two is one line: `main.rs:1480`
`run_repo_guards()?;`. That is the smallest binary-side surface I could get down to: one call
to one function, whose entire content is exercised by the test. Making `run_ci` itself
injectable (like `run_ci_steps`) would mean threading `typos_check`, `docs_check`,
`cargo_machete_check`, `cargo_deny_check`, `run_conformance`, `run_statics`,
`run_orchestrator_guard` and `run_dst` through an executor. That is about 10 call sites
rewritten plus a new unit test, all outside this brief's scope.

## Red→green evidence (project runner)

`PDCA_BUNDLE=results/issue_775 WYRD_VERIFY_BASE=origin/pdca-integration/r-a834…/main
./engine/scripts/run-verify.sh` gives:
`run-verify.sh: PASS — red without the fix, green with it (15 test(s) ran red).`
The GREEN leg: 15 passed. A plain hand run without `WYRD_VERIFY_BASE` falls back to
`origin/main`, which doesn't have #774, so both real-workspace tests fail correctly on the
missing subject ("no `wyrd-validate` package"). The driver supplies the integration base.

## Self-refutation

- **(a) Genuine red?** Yes. `run-verify.sh` reverts the production change and keeps the test:
  15/15 tests ran and failed ("unknown task `blackbox-guard`/`repo-guards`"). I also mutated
  individual behaviours in the fixed tree (each mutation run with
  `timeout 600 cargo test -p xtask --test blackbox_dependency_guard`, then reverted):
  - manifest check disabled → `an_optional_off_by_default…` and `a_normal_dependency…` go red;
  - graph walk stops at depth 1 → `a_transitive…_naming_the_path` (and `reached_package_in_no_node`) go red;
  - dev edges followed → `a_dev_dependency…` and `…behind_a_dev_edge…` go red;
  - a missing `resolve` returns `Ok(vec![])` → `a_document_without_a_resolve_graph_is_refused` goes red;
  - `BlackboxClosure` dropped from `CI_GUARDS` → `the_ci_guard_step_runs_the_blackbox_guard…` goes red.
- **(b) Production path?** Yes. Every test spawns the real `xtask` binary. The planted cases go
  through `blackbox_guard_verdict` → `scan_blackbox_closure`, the same functions
  `CiGuard::BlackboxClosure.run` calls in `ci`. The real-tree cases run `cargo metadata
  --locked --all-features` on the actual workspace. Nothing is mocked.
- **(c) Fixture includes the fault?** Yes. The planted documents contain the violating edge
  itself: direct normal, optional-only-in-manifest (absent from the graph, as a
  default-feature resolve would be), and three hops deep. The fail-closed documents remove
  exactly the field under test (`resolve`, `name`, `id`, `dep_kinds`, the subject's node).
  There is also a mutation (see (a)) proving the `CI_GUARDS` membership test binds.

## Other checks run

- `cargo fmt --all`: clean. `cargo clippy -p xtask --all-targets`: clean (with
  `[workspace.lints]` deny).
- `cargo test -p xtask`: all suites green, including `repo_hygiene_guards` and
  `readme_dev_section`.
- `cargo xtask repo-guards` on the real tree: all three guards pass. `Cargo.lock` is untouched
  (`--locked`), and `git status` shows only the four patched files.
- `typos` on the four changed files: clean.
- I did not run the full `cargo xtask ci` (DST sweep, deny, etc.). Check's gates run it.

## Judgement calls

- `kind: "build"` edges are not followed. A build-dependency runs at compile time and doesn't
  link into the shipped binary, and the brief says to follow only `kind: null` edges.
- A crate that is reached by several paths is reported once, with its shortest path.
- If a manifest has no `dependencies` array, or a declared dependency has no `name`, the result
  is `Err` (stricter than the brief asks, same fail-closed posture).
- The `wyrd-` prefix is the check, as the brief says. Every workspace crate except `xtask`
  carries it (checked against the real metadata).
