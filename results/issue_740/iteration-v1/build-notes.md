# Build notes — issue 740 / validate-crate-skeleton-and-blackbox-lint

## What shipped

1. `crates/validate` (package `wyrd-validate`), added to `[workspace] members`
   (`Cargo.toml:30`). Two targets: `src/lib.rs` (pure decisions: `ParsedArgs`,
   `resolve_config`, `resolve_credentials`, `render_config_block`, `run`) and a thin
   `src/main.rs` binary. Both crate roots carry `#![forbid(unsafe_code)]`
   (`crates/validate/src/lib.rs:22`, `crates/validate/src/main.rs:5`). No
   `[dependencies]` — zero new third-party crates, per scope.
2. `xtask::repo_guard::scan_blackbox_dependency_closure` (`xtask/src/repo_guard.rs:616`)
   — the pure, flippable scan. Walks the `cargo metadata --all-features` resolve graph
   transitively (normal edges only) from a target package, AND separately scans the
   package's own declared `dependencies` list (feature-independent — the "belt to the
   graph's braces" the Plan-review response calls for). `BLACKBOX_GUARD_TARGETS`
   (`repo_guard.rs:585`) holds `[("wyrd-validate", "wyrd-")]` — Open question 2's table,
   taken because it cost one `for` loop.
3. `run_blackbox_dependency_guard` (`repo_guard.rs:770`) — the impure wrapper: shells
   `cargo metadata --format-version 1 --locked --all-features` (no `--no-deps` — the
   unsafe-forbid guard's pattern deliberately NOT reused, since this guard needs the
   resolve graph) and hands the JSON to the pure scan for each `BLACKBOX_GUARD_TARGETS`
   entry.
4. `HygieneGuard` + `HYGIENE_GUARDS` (`repo_guard.rs:815-829`) — the lib-side DATA
   registration criterion 1 demands. `run_ci` (`xtask/src/main.rs:1558-1563`) iterates
   `xtask::repo_guard::HYGIENE_GUARDS` directly, beside the existing
   `run_gitlink_guard()?; run_unsafe_forbid_guard()?;` calls (main.rs:1557-1558).
5. `xtask/tests/blackbox_dependency_guard.rs` — the three planted-metadata cases
   (criterion 2), four fail-closed cases, and the criterion-1 wiring assertion (which
   also invokes the registered callable over the real workspace, not just checks its
   name is present).
6. `crates/validate/tests/cli_surface.rs` — criteria 3 and 4, driven against the real
   compiled `wyrd-validate` binary (`env!("CARGO_BIN_EXE_wyrd-validate")`) via
   `std::process::Command`, plus three fast unit-style cases calling
   `resolve_credentials` directly over an injected in-memory lookup.

## Decisions and what I ruled out

**Existing two guards stayed binary-target `fn`s; only the new one moved to lib-side
data.** The brief's citation (`main.rs:1486-1498`, `:1504-1505`) explicitly frames the
hazard as "a guard defined, tested and never called" and says "do not extend that
[pattern]" for the NEW guard — it does not ask for `run_gitlink_guard` /
`run_unsafe_forbid_guard` to be refactored. Moving those two would mean also lifting
`workspace_root()`/`print_step()` into the lib and touching ~15 other call sites in
`main.rs` that use the private `workspace_root()` — a much larger diff for a property
the brief's Success criterion never asks for (it only requires that *this* guard's
registration be lib-side data a test can read). Cost estimate if I had done it anyway:
~120 lines moved out of `main.rs` plus every one of `run_orchestrator_guard`,
`cargo_deny_check`, `run_conformance`, `run_statics`, `run_dst`, `docs_check`,
`typos_check` (all call `workspace_root()`) would need to switch from a bare call to
`xtask::workspace_root()` — a diff an order of magnitude larger than the ~7-line
`run_ci` change I actually made, for no additional coverage of this brief's criteria.
`HYGIENE_GUARDS` is deliberately structured so a *second* guard (gitlink or
unsafe-forbid, or a future one) is a one-entry addition to the same table later, without
redoing this decision.

**Dedup between the graph-walk and declared-list scans, keyed by crate name.** Criterion
2's first planted case ("a normal dependency on a wyrd-* crate") demands *exactly one*
violation. A crate declared directly on `wyrd-validate` is visible to BOTH the resolve
graph walk (one hop) and the declared-dependency list scan (same package, same
`dependencies` array) — without dedup that's two violations for one crate. I keep the
graph-walk's violation (it carries the dependency PATH, the more useful diagnostic) and
only add the declared-list wording when the graph walk did NOT already name that crate —
which is exactly the belt-and-braces case (an edge the graph doesn't reveal). Verified in
`an_optional_off_by_default_normal_edge_is_still_the_violation`, whose fixture
deliberately omits the edge from `resolve.nodes` to model "a future feature-resolution
subtlety keeps it out of the graph" (Design's own phrase) and gets exactly one violation
from the declared-list path alone.

**`--all-features --locked`, no `--no-deps`, one shell-out per `run_blackbox_dependency_guard`
call.** Verified directly on this checkout before writing the function: `cargo metadata
--format-version 1 --locked --all-features` exits 0 offline in ~0.2s and produces both
`resolve.nodes` (for the graph walk) and `packages[].dependencies` (for the declared-list
scan) in one document — no second shell-out needed even with the "two independent
sources" design, since both live in the same JSON.

**Test fixtures are synthetic, minimal JSON strings, not real `cargo metadata` output
trimmed down.** Matches the `xtask/tests/repo_hygiene_guards.rs` idiom the brief cites
(`repo_guard.rs`'s own module doc: "planting a real gitlink in a fixture would itself
commit the accident the guard exists to prevent") — planting a real `wyrd-core`
dependency on `wyrd-validate` would commit the very violation under test. I verified the
JSON *shape* (field names, `dep_kinds[].kind: null` vs `"dev"`, `optional` on declared
deps) against real `cargo metadata --all-features` output on this workspace before
writing the fixtures (see the transcript below) so the synthetic documents are
structurally faithful, not guessed.

**Strict unknown-flag rejection kept as scope item (f), not softened.** The brief
already made this an explicit, argued decision (Design + Plan-review response) — I did
not revisit it; `crates/validate/tests/cli_surface.rs`'s
`an_unrecognised_flag_exits_nonzero_naming_it` covers both a wholly-unknown flag and a
mistyped known one (the concrete footgun the Design section names). I renamed the
mistyped example from the brief's illustrative `--durtaion` to `--duration-zz` after
`typos` (part of `cargo xtask ci`) flagged `durtaion` as a real misspelling of
`duration` — same footgun, a spelling `typos-cli`'s dictionary doesn't also flag.

**No `clap` or any other new dependency.** Per scope and External dependencies — hand-rolled
`ParsedArgs` mirroring `crates/server/src/cli.rs:2495-2532`'s shape (the cited peer
callsite), with the one declared divergence (strict unknown-flag rejection, scope f).

## Peer callsites opened (the narrow exception)

- `xtask/src/repo_guard.rs:387-408` (`target_src_paths`) and `xtask/src/main.rs:1443-1481`
  (`run_unsafe_forbid_guard`) — mirrored for the guard's pure/impure split.
- `xtask/src/main.rs:1486-1544` (`run_ci_steps`) and `xtask/src/lib.rs:81`
  (`feature_gated_checks`) — mirrored for the lib-side data registration.
- `xtask/tests/repo_hygiene_guards.rs:34-49`, `:384-403` — mirrored for the
  flippable-test idiom (planted red, real-tree green).
- `crates/server/src/cli.rs:2495-2532` (`ParsedArgs`) — mirrored for the CLI parsing
  shape; the one deliberate divergence (unknown-flag rejection) is scope item (f), not
  copied.
- `xtask/src/main.rs:1519` (`run_ci_steps(&mut |name| std::env::var_os(name).is_some(), …)`)
  — mirrored for the injected-lookup convention in `resolve_credentials`.

## Three refutation questions (build discipline)

**(a) Genuine red?** Yes, demonstrated three ways, each reverted afterward (worktree is
clean of the refutation edits; `git status --short` after each check confirmed no
residue):

1. Neutered `scan_blackbox_dependency_closure`'s violation logic to `return
   Ok(Vec::new())` right after the fail-closed checks: `cargo test -p xtask --test
   blackbox_dependency_guard` went from 9/9 to 7 passed / **2 FAILED** —
   `a_normal_wyrd_dependency_is_exactly_one_violation_naming_it` (`left: 0, right: 1`) and
   `an_optional_off_by_default_normal_edge_is_still_the_violation` (`left: 0, right: 1`).
   This is criterion 2's red.
2. Emptied `HYGIENE_GUARDS` to `&[]`: `the_blackbox_guard_is_registered_in_hygiene_guards`
   failed with `"the blackbox guard must be registered in HYGIENE_GUARDS, which run_ci
   executes"`. This is criterion 1's red — a guard that passes every planted case but is
   never wired in is caught here, exactly the hazard the brief names.
3. Moved `crates/validate` out of the tree entirely (the literal pre-fix state): `cargo
   test -p wyrd-validate --test cli_surface` failed at the WORKSPACE level (`cargo`
   cannot even resolve the manifest — `failed to read
   .../crates/validate/Cargo.toml` / `No such file or directory`), confirming criteria 3
   and 4 are genuine criterion-absence red pre-fix, per the brief's declared Verification
   posture.

**(b) Production path?** Yes. `xtask/tests/blackbox_dependency_guard.rs` imports and
calls `xtask::repo_guard::scan_blackbox_dependency_closure` and
`xtask::repo_guard::HYGIENE_GUARDS` directly — the exact function and the exact const
`run_ci` consumes (`xtask/src/main.rs:1558-1563` iterates `HYGIENE_GUARDS` verbatim; no
intermediate copy). `crates/validate/tests/cli_surface.rs` spawns
`env!("CARGO_BIN_EXE_wyrd-validate")` — the actual compiled binary cargo builds from
`crates/validate/src/main.rs`, not a re-implementation — for criteria 3/4's process-level
assertions (exit codes, stderr), and additionally calls
`wyrd_validate::resolve_credentials` directly (the same function `main` calls) for the
fast injected-lookup cases.

**(c) Fixture includes the fault?** Yes. The three planted-metadata cases in
criterion 2 each construct a `cargo metadata`-shaped JSON document that DOES contain the
violation under test (a `wyrd-core` normal edge, direct or optional-off-by-default) —
none of them curate the offending crate out. The fail-closed fixtures likewise omit the
`resolve` section / target package on purpose, to prove the guard refuses rather than
passing vacuously. `scan_is_green_over_the_real_workspace_metadata` and
`the_blackbox_guard_is_registered_in_hygiene_guards` (which also *invokes* the registered
callable) both run over the REAL workspace `cargo metadata` output, not a curated
subset — so the "real tree is clean" claim is exercised against the actual dependency
graph `cargo xtask ci` sees, including `wyrd-validate` itself once the patch adds it to
`[workspace] members`.

## Verification

Ran the project's own runner end to end: `PDCA_WORKTREE=<worktree>
./engine/xtask.sh ci` (delegates to `cargo xtask ci` inside the worktree, per
`docs/INTEGRATION.md` §3's "Verification runner"). Full transcript tail:

```
$ xtask blackbox-dependency-guard (proposal 0017 §9: no wyrd-* in a validated package's normal closure)
xtask blackbox-dependency-guard: every validated package's normal dependency closure stays blackbox (proposal 0017 §9)
...
     Running tests/cli_surface.rs (target/debug/deps/cli_surface-c8fbe6c68d719f67)
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
...
     Running tests/blackbox_dependency_guard.rs (target/debug/deps/blackbox_dependency_guard-0bdbead8baf07bcb)
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
...
xtask ci: all checks passed
```

Exit code 0 (`DONE_EXIT=0`), full workspace (`cargo fmt --check`, `clippy --all-targets`,
`build --all-targets`, `test --workspace`, `cargo-machete`, `cargo-deny` ×3,
`run_conformance`, `run_statics`, `run_orchestrator_guard`, `run_dst`) green — the
per-issue KNOWN GATE SHAPE note (`C4-verify` → `PASS (green-only)` for the added
`*/tests/*.rs` files, since this patch creates the crate one of them lives in) is
consistent with `docs/publishing`/`run-verify.sh`'s stated discriminator and needs no
special handling here; the load-bearing red lives in `cargo xtask ci`
(`xtask/tests/blackbox_dependency_guard.rs`), demonstrated above.

Also ran `cargo fmt --all` (applied — the repo's configured formatter) and confirmed
`cargo fmt --all -- --check` clean afterward, and `cargo clippy -p wyrd-validate -p
xtask --all-targets` clean, before emitting the patch — matching the "commit-ready for
the target repo" requirement.

## What I did NOT do (and why)

- **Did not add `--driver-placement`/`--scenario`/etc. semantic validation** (e.g.
  parsing `--duration` as a real duration type). Explicitly out of scope ("scenarios,
  oracle, pools, verdict" per the brief's scope line) — this slice only requires
  presence-validation and echo of the raw string given.
- **Did not implement any S3 call.** Out of scope (#741).
- **Did not add a `deny.toml` entry or touch ADR-0003.** No new dependency was added, so
  there is nothing for the dependency wall to see.
- **Did not generalize `BLACKBOX_GUARD_TARGETS` beyond a `(package, prefix)` pair table.**
  Open question 2 left the shape to Do's judgment; a `(&str, &str)` slice was the
  cheapest form that still satisfies "a second entry is a one-line diff."
