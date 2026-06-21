# Brief — issue 152 / docs-readme-dev-testing-section

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** docs-readme-dev-testing-section
- **Defect / goal:** `README.md` (Repository layout, lines 77-82) documents only
  `cargo build` / `cargo test`. The real development workflow lives in `cargo xtask` and
  is absent from the on-ramp: a newcomer cannot discover the merge gate (`cargo xtask ci`),
  the Tier-2 tier (`cargo xtask integration`, which needs Docker + the compose plugin),
  `cargo xtask bench`, `dst`, or `conformance` without reading source / ADRs.
  `WYRD_DSERVER_COUNT` (read at `xtask/src/main.rs:103-107`) is undocumented; plain
  `cargo test` silently skips the `#[ignore]`d Tier-2 test; and a working CLI exists
  (`crates/server/src/cli.rs:55-84`: `wyrd demo` is a self-contained PUT/GET round-trip)
  that the README — which says only "not yet deployable" — never mentions.
- **Success criterion:** `README.md` gains a "Development & testing" section that (a) lists
  the `cargo xtask` entry points (`ci` = local merge gate, `integration` = Tier-2
  containers, `bench`, `dst`, `conformance`), (b) states the Docker + compose-plugin
  prerequisite for `cargo xtask integration`, (c) documents `WYRD_DSERVER_COUNT`, and (d)
  gives a "Try it" line — `cargo run -p wyrd-server --bin wyrd -- demo`. Verified at Check by
  deterministic inspection: every command / subcommand named matches the actual xtask
  dispatch (`xtask/src/main.rs`) and CLI (`cli.rs:55-84`) on the target branch, and the
  `wyrd demo` invocation is correct. (`cargo xtask ci` is Rust-only and `docs-check` lints
  only `docs/` — neither covers the root `README.md`; both stay green because this touches no
  Rust and no `docs/` file, which is supplementary, not the criterion.)
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (docs only — no GUI, no logic change)
- **Scope:** README additions only. / **out of scope:** `CONTRIBUTING.md` + templates
  (paired issue #153); restructuring the docs site; any change to xtask / CLI behavior.
- **Citations expected:** Do cites `README.md`, `xtask/src/main.rs` (subcommands +
  `WYRD_DSERVER_COUNT`), and `crates/server/src/cli.rs:55-84` on `main`.
- **Prior-art check:** README §"Repository layout" (lines 77-82) is the only build/test
  guidance; `README.md` names `xtask` only as a crate-table row (line 75) — `grep -ri "cargo
  xtask" README.md` is empty, so no README guidance describes the `cargo xtask` command
  surface (it is named only in docs/design ADRs / proposals). Net-new on-ramp section.
- **Disposition hint:** likely-fix

## STOP discipline
Draft only until Check sign-off.
