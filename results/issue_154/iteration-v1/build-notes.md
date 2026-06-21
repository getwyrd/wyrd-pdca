# Build notes — issue #154 (m2-7 follow-up: build/bench/repo hygiene)

Withheld from the reviewer. Rationale + what I ruled out, per the seven hygiene
items. Target: `getwyrd/wyrd @ main` (HEAD `a829993`). All `path:line` citations
are against that clean `main`.

## Working-tree note (why a fresh branch)

The target checkout `../wyrd` arrived **dirty**: branch
`fix/151-ci-enforce-rust-gate-and-dco-required` with issue 151's uncommitted edits
(incl. a `finish_integration` rework of `run_integration` and untracked
`.github/ISSUE_TEMPLATE/`, `CONTRIBUTING.md`, …). That is a *different* in-flight
cycle, and its version of `run_integration` does **not** match what this brief
describes (the brief cites clean `main`'s closure-then-`compose_down`). I therefore:

1. `git stash push -u -m "issue-151 wip parked by issue-154 builder"` (non-destructive
   — 151's work is recoverable from the stash).
2. Branched `fix/154-m2-7-followup-build-bench-repo-hygiene` from `main` (a clean tree).

All edits + the gate run are against that clean branch, so `patch.diff` applies to
`main` with no 151 contamination. **Scheduling caveat (brief §Scheduling note):** item 2
reworks the same `run_integration` teardown as #150/#151 — land the teardown rework once;
do not co-schedule this with 150/151 in one wave. The stash must be restored for the 151
cycle afterward.

## Item-by-item

### 1. Dockerfile — pin base, add `--locked`
`crates/chunkstore-grpc/tests/dserver/Dockerfile:12,15`. `FROM rust:1.96-bookworm` →
`rust:1.96.0-bookworm` to match the exact patch pinned in `rust-toolchain.toml:4`
(`channel = "1.96.0"`) — a floating minor tag re-resolving to a newer patch is what
triggers a build-time toolchain re-download. Added `--locked` to `cargo build --release`
so the container build consumes the committed `Cargo.lock` rather than silently resolving
newer deps (matches the gated workspace).

### 2. Panic-safe Tier-2 teardown — **the load-bearing behavior change**
`xtask/src/main.rs` `run_integration` (clean `main` lines 109–118). On `main` the test
runs in a closure, `compose_down(&compose)` is a **post-call statement**, then `result?`.
A `panic!` inside `run_integration_test` (as opposed to an `Err` return) unwinds *past*
the `compose_down` line → the cluster leaks.

**Fix (Drop guard, per the brief's offered mechanism):** added `with_teardown(body,
teardown)` which owns `teardown` in a `Drop` guard, so teardown fires on **both** the
normal-return path and the panic-unwind path, then the panic resumes. `run_integration`
now calls it; teardown can no longer be skipped.

**Why a Drop guard over `catch_unwind`:** `catch_unwind` requires the body to be
`UnwindSafe` (the closure captures `compose`/`count` and calls `resolve_endpoints`, which
would need `AssertUnwindSafe` wrapping) and would *swallow*-then-resume the panic — more
machinery for the same effect. The guard is ~13 lines, panic-safe by construction
(`Drop` runs during unwind), and needs no unwind-safety bounds. It also restores the
invariant the comment already claims ("tear down unconditionally so a failed run never
leaks containers") rather than guarding a symptom — the cause was *teardown not on the
unwind path*, and the guard moves teardown onto every path.

This is the one item with a behavioral regression test (below).

### 3. Bench `Cluster` doc corrected
`crates/core/benches/throughput.rs:51-53`. The doc claimed "dropping the cluster shuts
them down" — false: `_servers: Vec<JoinHandle<()>>`; dropping a tokio `JoinHandle`
*detaches* the task (it keeps running), it does not abort it. The brief allows "corrected
**or** servers aborted on drop". I chose the **doc correction** (comment-only, zero
behavior change) over adding `Drop`/`.abort()` because the detached servers are harmless
here — the bench process exit reclaims them, and aborting on drop is a behavior change the
brief scopes *out* (no design decision / no data-path change). Cost of the rejected
alternative: a new `impl Drop for Cluster` + storing abort handles, ~8 lines of behavior
change for no observed benefit. New doc states the tasks are *detached* and reclaimed at
process exit.

### 4. `WYRD_DSERVER_COUNT` warns on a rejected value — **also behaviorally tested**
`xtask/src/main.rs` `run_integration` (clean `main` lines 103–107). On `main`,
`.filter(|&n| n >= 2).unwrap_or(DSERVER_COUNT)` silently turns `0`/`1`/garbage/empty into
9 — a typo'd `WYRD_DSERVER_COUNT=1` runs a 9-server cluster with no signal. Extracted a
**pure** `resolve_dserver_count(raw: Option<String>) -> (usize, Option<String>)`: returns
the count plus an optional warning for a *rejected explicit* value (None/unset stays
silent — no value was rejected). `run_integration` prints the warning via `eprintln!`.
Pure fn → unit-tested without spawning a container.

### 5. `.github/dependabot.yml` added
New file. `cargo` + `github-actions` ecosystems, weekly. Rationale: the repo is gated by a
`cargo deny check` advisory wall (ADR-0003 §2); without scheduled bumps a new RUSTSEC
advisory lands as a surprise CI failure rather than a tracked update PR. The
github-actions ecosystem keeps the pinned `actions/*` in `.github/workflows/` current.
Validated as well-formed YAML (`version: 2`, two ecosystems).

### 6. Inert `.dockerignore` line removed
`.dockerignore:7`. `results/` listed but nothing in the repo produces a `results/` dir
(that path is a PDCA-harness concept, not Wyrd's). Removed the dead line; `target/`,
`.git/`, `**/*.swp` retained.

### 7. Tier-numbering cross-map note
`docs/design/architecture/10-quality-risks-glossary.md` §13.2 (after the intro at line
99). The architecture doc numbers strategy tiers 0–3 where "Tier 2 = a single real
machine" (line 117), which collides with the code/CI labels from **proposal 0004's** test
taxonomy where "Tier-2" = the container integration test (`cargo xtask integration`,
`crates/chunkstore-grpc/tests/tier2_integration.rs`) and "Tier-1" = the in-process
DST/wire suite. Added a one-paragraph blockquote note disambiguating the two schemes and
saying to read a bare "Tier 2" by its source. Docs lint + internal-link render audit pass.

## Test — red→green proof

The brief names **no** test path; its Success criterion is "`cargo xtask ci` green +
inspection confirms each cited change." Items 2 and 4 are the only ones with testable
behavior, so I added a `#[cfg(test)] mod tests` **inline in `xtask/src/main.rs`** (the
natural Rust home — the units `with_teardown` / `resolve_dserver_count` are private). A
verbatim copy is in the bundle as `test_xtask_hygiene.rs` for the reviewer.

- **Import-light by design:** these are pure unit tests (`Cell`, `catch_unwind`) — no
  tonic/tokio/docker pulled in at load. They can't hang or crash a headless runner.
- **Green:** `cargo test -p xtask` → 6 passed.
- **Red:** reverted `xtask/src/main.rs` to `main`, re-appended *only* the test module →
  `cargo test -p xtask` fails to compile with `E0425: cannot find function
  resolve_dserver_count` / `with_teardown` (7 errors). The seam the tests exercise does
  not exist pre-fix — the silent clamp and the unguarded post-call teardown have no unit
  to assert against — which is exactly the defect. Restored the fix → green again.

Run through the project's own cargo runner (the same `cargo test --workspace` that
`cargo xtask ci` invokes), not a hand-rolled command.

## Commit-readiness

- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p xtask --all-targets -- -D warnings` → clean;
  `cargo clippy -p wyrd-core --all-targets -- -D warnings` → clean (covers the bench).
- `python3 docs/publishing/tools/lint_docs.py` → OK; `render_site.py --check` → link
  audit OK (the target's docs-check PR gate).
- Full `./engine/xtask.sh ci` (= `cargo xtask ci`) run as the authoritative C4 gate.

## STOP discipline
Draft only. No PR opened/marked-ready/merged.
