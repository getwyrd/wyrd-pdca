# PR description

> One logical fix per PR. Documentation-only change to the root README, plus a
> regression guard. No Rust behavior or `docs/` content is touched.

## Root cause

The root `README.md` documented only the standard `cargo build` / `cargo test`
flow (`README.md:77-82`), so the real development workflow — `cargo xtask` (the
merge gate, the Tier-2 container tier, `bench`, `dst`, `conformance`), the
`WYRD_DSERVER_COUNT` knob, and the self-contained `wyrd demo` round-trip — was
undiscoverable without reading source or ADRs. Plain `cargo test` also silently
skips the `#[ignore]`d Tier-2 integration test, so a newcomer running it sees a
green run that never exercised the cluster path.

## Fix

Insert a `## Development & testing` section into `README.md` between the existing
Cargo-flow block and `## Security`:

- a table of the five `cargo xtask` entry points (`ci`, `conformance`, `dst`,
  `integration`, `bench`), noting that `integration` is **not** part of `ci`;
- a note that plain `cargo test` skips the `#[ignore]`d Tier-2 test;
- the **Docker + Compose-plugin** prerequisite for `cargo xtask integration` and
  the `WYRD_DSERVER_COUNT` knob (default 9, minimum 2);
- a "Try it" subsection: `cargo run -p wyrd-server --bin wyrd -- demo`.

It reuses the existing `[ADR-0016]` link reference, so no new link definitions
are added. No Rust or `docs/` files change.

## Verified against

- `README.md:77-82` (`main`) — the only prior build/test guidance; the new
  section is inserted after it, before `## Security` (`README.md:84`).
- `xtask/src/main.rs:29-34` (`main`) — dispatch for `ci`, `conformance`, `dst`,
  `integration`, `bench`; confirms every documented command exists and that
  `integration` is dispatched separately from `run_ci`.
- `xtask/src/main.rs:75,103-107` (`main`) — `const DSERVER_COUNT: usize = 9` and
  the `WYRD_DSERVER_COUNT` read filtered to `>= 2`; confirms "default 9, minimum
  2" is exact.
- `crates/server/src/cli.rs:59,280` (`main`) — `Some("demo") => cmd_demo()`
  dispatch and `cmd_demo`, a self-contained in-memory S3 PUT/GET round-trip;
  confirms the `wyrd demo` invocation is correct.
- `crates/server/Cargo.toml:2,12` (`main`) — package `wyrd-server` building the
  `wyrd` binary; confirms `-p wyrd-server --bin wyrd` is right.

## Test

`xtask/tests/readme_dev_section.rs` — a regression guard. The root README is
covered by no gate (`cargo xtask ci` is Rust-only; the docs linter lints only
`docs/`), so it can drift from the code it documents. The test cross-checks each
documented `cargo xtask` subcommand against `Some("<sub>")` in
`xtask/src/main.rs`, the demo line against `Some("demo")` in
`crates/server/src/cli.rs`, and the `wyrd-server` / `wyrd` names in
`crates/server/Cargo.toml` — so it fails if the README names a command that does
not exist or a command is renamed in source without updating the README.

Red → green: post-fix `cargo test -p xtask --test readme_dev_section` passes;
with `README.md` reverted to `main` it fails at the first assertion (missing
`## Development & testing` section).

Fixes #152
