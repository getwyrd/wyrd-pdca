# M2.7 follow-up: build / bench / repo hygiene

> One logical change: the grouped set of low-severity hygiene items the brief
> for #154 accepted as a single maintenance bundle. No data-path behavior change.

## Root cause
Reviewing the M2.7 Tier-2 integration + throughput-bench work (#117) surfaced
seven residual nits — a floating build base, a panic-leaky test teardown, a
silent env-var clamp, a wrong bench doc-comment, a missing dependabot config, an
inert `.dockerignore` line, and a Tier-numbering collision in the docs. None
breaks correctness today, but each is a way the build, the harness, or the docs
can mislead later, and none was tracked elsewhere.

## Fix
- **Reproducible d-server image** — `crates/chunkstore-grpc/tests/dserver/Dockerfile`:
  pin `FROM rust:1.96.0-bookworm` (matching `rust-toolchain.toml`) and add
  `--locked` to the release build, so the container can't re-resolve to a newer
  toolchain patch or newer dependencies than the gated workspace.
- **Panic-safe Tier-2 teardown** — `xtask/src/main.rs`: run the integration body
  through a new `finalize_panic_safe` wrapper that catches an unwind, finalizes a
  panic as a failure (so logs are captured before the cluster is torn down), then
  resumes the unwind. It delegates to — rather than replaces — the #150
  `finish_integration` finalizer, so a panicking run now keeps both invariants
  (capture-before-teardown, no leaked cluster).
- **Non-silent `WYRD_DSERVER_COUNT`** — `xtask/src/main.rs`: extract a pure
  `resolve_dserver_count` that returns a warning for a rejected explicit value
  (`0`/`1`/garbage); an unset var stays silent.
- **Correct bench doc** — `crates/core/benches/throughput.rs`: the `Cluster`
  doc-comment now says dropping the cluster *detaches* the server tasks (a dropped
  `JoinHandle` does not abort) and that they are reclaimed at process exit
  (comment-only; no behavior change).
- **`.github/dependabot.yml`** (new): cargo + github-actions ecosystems, weekly,
  so advisory-wall (`cargo deny check`) bumps arrive as tracked PRs.
- **`.dockerignore`**: drop the inert `results/` entry.
- **Architecture glossary** — `docs/design/architecture/10-quality-risks-glossary.md`:
  add a note disambiguating the strategy's Tier 0-3 realism ladder from the
  code/CI "Tier-1"/"Tier-2" test taxonomy.

## Verified against
- `crates/chunkstore-grpc/tests/dserver/Dockerfile:12,15` — the floating
  `FROM rust:1.96-bookworm` and the unlocked `cargo build --release --bin wyrd`
  this PR pins/locks.
- `xtask/src/main.rs:103-106` — the silent `.filter(|&n| n >= 2).unwrap_or(...)`
  clamp (`DSERVER_COUNT = 9`, `xtask/src/main.rs:75`) replaced by the warning path.
- `xtask/src/main.rs:113-118` — the closure-then-`compose_down(&compose)`
  teardown that unwinds past finalization on a panic; this is the spot the #150
  rework lands first and the panic-safe wrapper then composes on top of.
- `crates/core/benches/throughput.rs:53` — the "dropping the cluster shuts them
  down" doc line corrected.
- `.dockerignore:7` — the inert `results/` line removed.
- `docs/design/architecture/10-quality-risks-glossary.md:99,117` — the Tier 0-3
  ladder and the "Tier 2 — First real-world hardware experience" heading that the
  new note cross-maps to the code's "Tier-2" container suite.

## Test
Items 2 (panic-safe teardown) and 4 (`WYRD_DSERVER_COUNT`) are the only
behaviorally testable ones; both ship inline `#[cfg(test)]` unit tests in
`xtask/src/main.rs` (the crate-private fns have no other home). Red→green:
reverting the non-test code makes the tests fail to compile (`resolve_dserver_count`
/ `finalize_panic_safe` don't exist pre-fix) — the seam they assert against is
exactly the defect. `panic_finalizes_capture_then_teardown_then_resumes` drives
the wrapper *through* `finish_integration` and asserts `["capture_logs",
"teardown"]` ordering plus panic propagation, pinning the #150+#154 composition.
The remaining items are inspection-verified (Dockerfile pin, doc corrections,
dependabot config, `.dockerignore`); the whole bundle passes `cargo xtask ci`
(fmt / clippy `-D warnings` / build / test / deny / conformance / DST), exit 0.

**Merge ordering:** the panic-safe teardown is stacked on the #150
capture-before-teardown rework and must land after it (the wrapper delegates to
#150's `finish_integration`). Do not co-schedule #150 and #154 in one concurrent
wave.

Fixes #154
