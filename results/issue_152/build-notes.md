# Build notes — issue 152 / docs-readme-dev-testing-section

## What the brief asks (Success criterion)

Add a "Development & testing" section to the root `README.md` that (a) lists the
`cargo xtask` entry points (`ci`, `integration`, `bench`, `dst`, `conformance`),
(b) states the Docker + compose-plugin prerequisite for `cargo xtask integration`,
(c) documents `WYRD_DSERVER_COUNT`, and (d) gives a "Try it" line
`cargo run -p wyrd-server --bin wyrd -- demo`. Verified at Check by deterministic
inspection that every command named matches the actual xtask dispatch and CLI on
`main`, and the `wyrd demo` invocation is correct.

## Verification against the target branch (origin/main), not recall

Every documented command was confirmed against the source on `origin/main`:

- **xtask subcommands** — `xtask/src/main.rs:30-35` dispatches `ci`, `conformance`,
  `gen-vectors`, `dst`, `integration`, `bench`. The five the brief requires all
  exist. `integration` is explicitly **not** part of `run_ci` (doc comment
  `xtask/src/main.rs:11-15`; `run_ci` at `xtask/src/main.rs:~226` does not call it)
  and needs a container runtime.
- **Docker + compose prerequisite** — `run_integration` (`xtask/src/main.rs`,
  `docker_available()` runs `docker info`; `compose_up`/`docker_compose` run
  `docker compose …`). A hard failure in CI, warn-and-skip locally — its own
  message says "Install Docker (and the compose plugin)".
- **`WYRD_DSERVER_COUNT`** — read in `run_integration`: `std::env::var(
  "WYRD_DSERVER_COUNT") … .filter(|&n| n >= 2).unwrap_or(DSERVER_COUNT)` with
  `const DSERVER_COUNT: usize = 9`. So "default 9, minimum 2" in the README is
  exact, not approximate.
- **`wyrd demo`** — `crates/server/src/cli.rs:59` dispatches `Some("demo") =>
  cmd_demo()`; `cmd_demo` (`crates/server/src/cli.rs:280-308`) is a self-contained
  in-memory S3 PUT/GET round-trip (no data dir, no cluster). Package is
  `wyrd-server` and the binary is `wyrd` (`crates/server/Cargo.toml:2,11-12`), so
  `cargo run -p wyrd-server --bin wyrd -- demo` is the correct invocation.

## The change

`README.md` — net-new `## Development & testing` section inserted between the
existing "standard Cargo flow" block (ends `README.md:82` on origin/main) and
`## Security` (`README.md:84`). Reuses the already-defined `[ADR-0016]` link
reference (`README.md:101`), so no new link defs. A table of the five xtask
entry points, the "plain `cargo test` skips the `#[ignore]`d Tier-2 test" note,
the Docker/compose prerequisite + `WYRD_DSERVER_COUNT`, and a "Try it" subsection.
No Rust or `docs/` files are changed by the documentation itself (the brief notes
neither `cargo xtask ci` nor `docs-check` covers the root README).

## The test

`xtask/tests/readme_dev_section.rs` — a regression guard, because the root README
is in nobody's gate (xtask `ci` is Rust-only; the docs linter lints only `docs/`),
so the on-ramp can silently drift from the code. It is **import-light** (std `fs`
only — no GUI/heavy deps, safe on a headless runner) and lives in the `xtask`
crate, which already owns the command surface it documents.

It does more than assert strings are present in the README: for each xtask
subcommand and for the demo line it cross-checks the **source** on the same tree —
`main_rs.contains("Some(\"<sub>\")")`, `cli_rs.contains("Some(\"demo\")")`,
and the `wyrd-server` / `wyrd` names in `crates/server/Cargo.toml`. So the test
fails if the README documents a command that does not exist *or* if a command is
renamed in source without updating the README — it guards the criterion's
"matches the actual dispatch", not just "text exists".

### Red → green (proven)

- Post-fix: `cargo test -p xtask --test readme_dev_section` → `1 passed`.
- Pre-fix: with `README.md` reverted to origin/main, the same test fails at the
  first assertion — `README is missing the `## Development & testing` section`.

Run with a targeted `cargo test -p xtask --test readme_dev_section` rather than
the whole-tree `./engine/xtask.sh ci` (the C4-ci gate) on purpose: the wrapper
delegates wholesale to `cargo xtask ci`, which has no single-test entry point and
runs fmt + clippy + build + `cargo deny` + conformance + a 50-seed madsim DST
sweep — minutes of work for a std-only file-reading test that cannot hang. Check's
C4-ci gate re-runs the full suite; this was the fast red→green sanity pass.

## Commit-readiness

- `cargo fmt --all -- --check` → clean (rustfmt reflowed one `assert!` in the test;
  applied).
- `cargo clippy -p xtask --all-targets -- -D warnings` → clean.

## Workspace note (for the human)

`../wyrd` (the gate checkout) currently holds **issue 151's uncommitted changes**
on branch `fix/151-…` (modified `xtask/src/main.rs`, CI workflows). To avoid
clobbering that in-flight work, I built and verified in a dedicated worktree off
`origin/main` (`git worktree add /tmp/wyrd-152 origin/main`, branch
`fix/152-readme-dev-testing-section`). `patch.diff` is `git diff` against
`origin/main` and `git apply --check`s clean onto a fresh `origin/main` tree
(verified). My README/test changes touch different lines than 151's, so there is
no content conflict; the patch is independent.

## Alternatives considered

- **Plain string-presence test on README only** — rejected: it would pass even if
  someone renamed `cargo xtask integration` in source, exactly the drift the brief
  wants caught. The source cross-check costs ~10 extra lines and three more file
  reads (all std `fs`), no new dependency — cheap, and it is what makes the test
  guard the real criterion.
- **No test, "docs-only" statement** — rejected: a mechanical guard is feasible and
  load-light here, and the README's whole problem is that it is *ungated*; a test
  is the cheapest way to keep it honest going forward.
- **Putting the test in `crates/server`** — rejected: the section is primarily about
  the `xtask` command surface; `xtask` is the crate that owns those commands and
  can read the workspace README via `CARGO_MANIFEST_DIR/..`.
