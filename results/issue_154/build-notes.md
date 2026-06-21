# Build notes — issue #154 (m2-7 follow-up: build/bench/repo hygiene), Iteration 2

Withheld from the reviewer. Rationale + what I ruled out. Target: `getwyrd/wyrd @
main`. The teardown item (item 2) is **stacked on #150** per the brief's
Iteration-1 carry-forward; all `path:line` citations below are against the
post-#150 base (`main` + #150's `xtask/src/main.rs` rework), which is what the
patch.diff is expressed against.

## What changed since Iteration 1 (the carry-forward)

Iteration 1 was rejected at sign-off for ONE reason: item 2 introduced a
standalone `with_teardown` Drop-guard that **collided** with #150's
`finish_integration` (both rework the same `run_integration` teardown at
`xtask/src/main.rs:113-118` on `main`). The sign-off direction: "#150 lands first
and introduces `finish_integration` (capture-logs-before-teardown) … Rebuild
#154's panic-safe teardown on top of #150's `finish_integration` so the two
compose instead of colliding — i.e. preserve both capture-before-teardown (#150)
and teardown-on-panic-unwind (#154). The other six hygiene items reviewed clean
and are unaffected."

So this iteration changes **only item 2**; items 1, 3, 4, 5, 6, 7 are carried
over unchanged from v1 (re-implemented fresh, identical content). The v1 artifacts
are preserved in `iteration-v1/`.

### How item 2 now composes with #150 (the rework)

#150's `finish_integration(result, capture_logs, teardown)` already guarantees
capture-before-teardown on a **failure** — but only if control *reaches* it. The
structure on the #150 base is:

```rust
let result = (|| { … run_integration_test(&endpoints) })();   // body runs here
finish_integration(result, || compose_logs(&compose), || compose_down(&compose))?;
```

A `panic!` inside the body (vs an `Err` return) unwinds **past** the
`finish_integration` call entirely: the cluster leaks AND #150's diagnostics are
never captured. That is exactly #154 item 2's defect, now made worse because a
panic also defeats #150's operability guarantee.

The fix is a thin panic-safe wrapper that **delegates** to #150's
`finish_integration` rather than replacing it (`xtask/src/main.rs`,
`finalize_panic_safe`):

```rust
fn finalize_panic_safe<B, F>(body: B, finalize: F) -> Result<(), String> {
    let outcome = catch_unwind(AssertUnwindSafe(body));
    let (result, panic) = match outcome {
        Ok(result) => (result, None),
        Err(panic)  => (Err("Tier-2 integration test panicked".into()), Some(panic)),
    };
    let finished = finalize(result);          // <- #150's finish_integration runs here
    if let Some(panic) = panic { resume_unwind(panic); }
    finished
}
```

`run_integration` now calls:

```rust
finalize_panic_safe(
    || { …body… },
    |result| finish_integration(result, || compose_logs(&compose), || compose_down(&compose)),
)?;
```

Both invariants now hold on **every** exit path:
- normal `Ok` → finalize with Ok → teardown only (no spurious log capture);
- `Err` return → finalize with Err → **capture-before-teardown (#150)**, error
  propagated unchanged;
- `panic!` → caught, finalize as a failure → **capture-before-teardown (#150)
  still runs**, cluster torn down (#154), then the panic resumes (not swallowed,
  preserving the backtrace/abort semantics).

Note the `finished?` is sequenced *after* `resume_unwind`, so on the panic path
the synthesised `Err` is discarded and the original panic propagates — the panic
is never silently converted into a normal error return.

#### Why delegate-to-`finish_integration`, not a single combined function

I rejected folding capture+teardown+panic-safety into one new function (which is
what would re-collide with #150). Cost of that rejected approach, concretely: it
would re-introduce `finish_integration`'s body (the `if result.is_err() {
capture_logs() }` ordering, ~6 lines) inside #154's diff and **delete** #150's
function — a diff that *reverts* #150's hunk and re-adds an equivalent, i.e. a
textual conflict with #150 on `xtask/src/main.rs:123-144`. The delegating wrapper
instead leaves #150's `finish_integration` byte-for-byte intact (it appears only
as unchanged *context* in patch.diff at the `@@ … fn finish_integration` hunk
boundary), so the two patches compose with zero overlap on #150's lines.

#### Why `catch_unwind` here (vs the v1 Drop guard)

v1 used a Drop guard because that avoided unwind-safety bounds. But composing with
#150 needs the **result value** (`Ok`/`Err`) handed to `finish_integration` so it
knows whether to capture logs — a Drop guard has no access to the body's return
value, only to "did we unwind." `catch_unwind` yields exactly that
`Result<Result<…>, panic>` discrimination, which is what lets the panic path feed
a synthesised failure into #150's capture-before-teardown. The `AssertUnwindSafe`
wrap is sound: the body only borrows `compose`/`count` and the closures are
re-runnable; nothing observes a half-mutated state after the catch (we re-raise).

## Item-by-item (1, 3, 4, 5, 6, 7 unchanged from v1)

1. **Dockerfile** (`crates/chunkstore-grpc/tests/dserver/Dockerfile:12,15` on
   `main`): `FROM rust:1.96-bookworm` → `rust:1.96.0-bookworm` to match the exact
   patch in `rust-toolchain.toml:4` (`channel = "1.96.0"`); a floating minor tag
   re-resolving to a newer patch is what forces a build-time toolchain
   re-download. Added `--locked` to `cargo build --release` so the container build
   consumes the committed `Cargo.lock` rather than silently resolving newer deps.

3. **Bench `Cluster` doc** (`crates/core/benches/throughput.rs:51-53`): the doc
   claimed "dropping the cluster shuts them down" — false; `_servers:
   Vec<JoinHandle<()>>`, and dropping a tokio `JoinHandle` *detaches* the task
   (keeps running), it does not abort. The brief allowed "corrected **or** servers
   aborted on drop". I chose the **doc correction** (comment-only, zero behavior
   change) over an `impl Drop`+`.abort()` (a ~8-line behavior change the brief
   scopes out — "anything requiring a design decision … out of scope"). The
   detached servers are harmless: the bench process exit reclaims them.

4. **`WYRD_DSERVER_COUNT` warns** (`xtask/src/main.rs:103-107` on `main`): the
   `.filter(|&n| n >= 2).unwrap_or(DSERVER_COUNT)` silently turns `0`/`1`/garbage/
   empty into 9 — a typo'd `WYRD_DSERVER_COUNT=1` runs a 9-server cluster with no
   signal. Extracted a **pure** `resolve_dserver_count(Option<String>) -> (usize,
   Option<String>)`: returns the count plus an optional warning for a *rejected
   explicit* value (unset stays silent — nothing was rejected). `run_integration`
   prints the warning via `eprintln!`. Pure fn → unit-tested without a container.

5. **`.github/dependabot.yml`** (new): `cargo` + `github-actions` ecosystems,
   weekly. The repo is gated by a `cargo deny check` advisory wall (ADR-0003 §2);
   without scheduled bumps a new RUSTSEC advisory lands as a surprise CI failure
   rather than a tracked update PR. The `github-actions` ecosystem is non-inert:
   `.github/workflows/` exists with 7 pinned-action workflows (this addresses the
   v1 reviewer's "confirm the workflows surface exists" note). Validated
   well-formed (`yaml.safe_load`, `version: 2`, both ecosystems).

6. **Inert `.dockerignore` line** (`.dockerignore:7`): `results/` listed but
   nothing in the Wyrd repo produces a `results/` dir (it's a PDCA-harness
   concept). Removed; `target/`, `.git/`, `**/*.swp` retained.

7. **Tier-numbering cross-map note**
   (`docs/design/architecture/10-quality-risks-glossary.md` §13.2, after line 99):
   the doc numbers strategy tiers 0–3 where "Tier 2 = a single real machine",
   colliding with the code/CI labels from proposal 0004's taxonomy where "Tier-2" =
   the container integration test. Added a one-paragraph blockquote disambiguating
   the two schemes. `lint_docs.py` OK; `render_site.py --check` link audit OK.

## Working-tree / branch mechanics (and the #150 stacking)

The target checkout arrived dirty on `fix/155-…` with #155's uncommitted WIP. To
build #154 on a clean base I:

1. `git add -A && git stash push -m "issue-155 wip parked by issue-154 builder"`
   (non-destructive; `stash@{0}` recoverable for the #155 cycle).
2. `git checkout fix/154-…` (was clean == `main`).
3. **Applied #150's patch** (`results/issue_150/patch.diff`) to the working tree
   as the composition base — the carry-forward establishes "#150 lands first". It
   applies cleanly to `main`.
4. Layered #154's edits on top (the seven items; item 2 extends #150's
   `#[cfg(test)] mod tests` rather than adding a second `mod tests`, which would be
   a duplicate-module compile error).

The working tree the C4 gate sees is therefore `main + #150 + #154`, and it passes
`cargo xtask ci` whole (proving the two compose). `patch.diff` records **only**
#154's delta: I extracted it by building a throwaway `tmp-base150` commit
(`main + #150`) and `git diff tmp-base150 --staged`, then deleted that branch.
That is why #150's `.github/workflows/integration-nightly.yml` and its
`finish_integration`/`compose_logs` do **not** appear in patch.diff — only as
unchanged context.

**Merge-ordering caveat for sign-off (V/NEEDS-HUMAN):** this patch must land
**after** #150. If #150's `finish_integration` shape changes before it merges,
item 2's `finalize_panic_safe` call site (`|result| finish_integration(result, …)`)
must be re-pointed accordingly. Do not co-schedule #150 and #154 in one concurrent
wave (brief §Scheduling note).

Stashes left for the driver/next cycles: `stash@{0}` = #155 WIP (restore for #155),
`stash@{1}` = the superseded #154 v1 WIP, `stash@{2}` = #151 WIP. Mirrors the v1
pattern (each cycle parks the prior dirty tree).

## Test — red→green proof

The brief names no test path; its Success criterion is "`cargo xtask ci` green +
inspection confirms each cited change." Items 2 and 4 are the only behaviorally
testable ones, so the tests ship **inline** in `xtask/src/main.rs`, extending
#150's `mod tests` (the natural Rust home — `finalize_panic_safe` /
`resolve_dserver_count` / `finish_integration` are crate-private). A verbatim copy
of the #154 additions is in the bundle as `test_xtask_hygiene.rs`.

- **Import-light:** pure unit tests (`RefCell`, `catch_unwind`) — no
  tonic/tokio/docker at load, so a headless runner can't hang or crash.
- **Green:** `cargo test -p xtask` → **8 passed** (2 from #150's base + 6 #154).
- **Red:** reverted only the #154 *non-test* code (restored `main`'s
  `.unwrap_or` clamp and #150's inline `finish_integration(result, …)` call,
  removed `resolve_dserver_count` + `finalize_panic_safe`) while keeping the #154
  tests → `cargo test -p xtask` fails to compile with `E0425: cannot find
  function resolve_dserver_count` / `finalize_panic_safe` (7 errors). The seam the
  tests assert against does not exist pre-fix — the silent clamp and the
  panic-unsafe finalization have no unit to pin — which is the defect. Restored →
  green.
- The `panic_finalizes_capture_then_teardown_then_resumes` test drives
  `finalize_panic_safe` **through** `finish_integration` with order-recording
  closures, asserting `["capture_logs", "teardown"]` ordering AND panic
  propagation — i.e. it pins the *composition* of both invariants, not just #154's.

Run through the project's own runner (`cargo test` as `cargo xtask ci` invokes,
and the full `./engine/xtask.sh ci`), not a hand-rolled command.

## Commit-readiness

- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p xtask --all-targets -- -D warnings` → clean;
  `cargo clippy -p wyrd-core --all-targets -- -D warnings` → clean (covers the bench).
- `python3 docs/publishing/tools/lint_docs.py` → OK;
  `render_site.py --check` → link audit OK (the `docs-check` PR gate).
- `python3 -c "yaml.safe_load(...)"` on `.github/dependabot.yml` → OK.
- Full `./engine/xtask.sh ci` (= `cargo xtask ci`: fmt/clippy/build/test/deny/
  conformance/dst) → **`xtask ci: all checks passed`**, exit 0.

## STOP discipline

Draft only. No PR opened / marked ready / merged.
