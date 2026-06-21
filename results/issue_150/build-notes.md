# Build notes — issue #150 / ci-harden-nightly-tier2-job

Target: getwyrd/wyrd @ `main` (HEAD `a829993`).

## Success criterion, restated

On a failing integration run, container diagnostics (`docker compose logs`) are
captured **before** the cluster is torn down, AND the `tier2` job carries a
`timeout-minutes` bound. Invariant to restore: *a failed automated run must
capture its diagnostic evidence before destroying the environment that produced
it — teardown never precedes log/artifact capture.*

## Root cause (two sentences)

In `run_integration` the teardown `compose_down(&compose)` ran *before* the
failure was propagated (`xtask/src/main.rs:117-118` on `main`), so by the time
`result?` surfaced the error the containers, their logs and volumes were already
destroyed — and the `tier2` workflow job never ran `docker compose logs`, had no
`timeout-minutes`, and no `if: failure()` surfacing
(`.github/workflows/integration-nightly.yml:28-47` on `main`). A failed nightly
Tier-2 run (the *only* place a Tier-2 regression surfaces, ADR-0009) therefore
preserved nothing to diagnose it.

## The change

Two coordinated edits — the Rust fix restores the ordering (the real cause); the
YAML fix bounds the job and ships the captured evidence as an artifact.

### 1. `xtask/src/main.rs` — capture before teardown (the invariant)

- Extracted `finish_integration(result, capture_logs, teardown)`
  (`xtask/src/main.rs:130` post-fix): on `result.is_err()` it calls
  `capture_logs()` **then** `teardown()`; on success only `teardown()`; the
  original `result` is propagated unchanged. Teardown still always runs, so a run
  never leaks containers — the property the old unconditional `compose_down`
  protected is kept.
- Rewired `run_integration` to call it (`xtask/src/main.rs:118` post-fix),
  replacing the `compose_down(&compose); result?;` pair (`:117-118` pre-fix).
- Added `compose_logs(compose)` (`xtask/src/main.rs:181` post-fix):
  `docker compose ... logs --no-color --timestamps`, echoed to the job log **and**
  persisted to `target/tier2-logs/docker-compose.log` for the workflow artifact.
  Best-effort: a capture failure only warns and never masks the test result
  (same posture as `compose_down`).

This is generic over the two actions specifically so the ordering is unit-testable
**without a container runtime** — the test drives it with order-recording
closures, keeping the unit import-light for the headless `cargo xtask ci` runner.

### 2. `.github/workflows/integration-nightly.yml` — bound + surface

- `timeout-minutes: 45` on the `tier2` job (`:31` post-fix) — a hung cold build
  fails fast instead of drifting toward the 6h GitHub default.
- `if: failure()` step "Upload container diagnostics" using
  `actions/upload-artifact@v4` (`:50-61` post-fix) uploading `target/tier2-logs/`
  with `if-no-files-found: ignore`. This is verified by deterministic file
  inspection at Check (the YAML is not exercised by `cargo xtask ci`, per the
  brief).

## Why this shape, and alternatives ruled out

- **Why the Rust change is the real fix, not a workflow-only `if: failure()`
  `docker compose logs` step.** A workflow step that runs `docker compose logs`
  *after* `cargo xtask integration` returns would capture **nothing**: xtask's
  own `compose_down` has already run `down -v --remove-orphans`, destroying the
  containers. The cause is the in-process ordering, so it must be fixed in
  `run_integration`. The workflow step only *ships* what the Rust change captured
  before teardown. Cost of the workflow-only alternative is not "heavier" — it is
  **incorrect** (empty artifact), so it is ruled out on correctness.
- **Why extract `finish_integration` rather than inline the `if result.is_err()`
  block.** Inlining would restore the invariant in production but leave it
  **untestable without Docker** — and a headless runner has no container runtime,
  so the regression could only be checked by live nightly evidence (explicitly
  *supplementary*, not the Check criterion). The extraction adds one small generic
  function (15 lines incl. doc) and makes the ordering a deterministic, Docker-free
  unit test that runs inside `cargo xtask ci`. That is the minimal change that both
  restores the invariant *and* proves it red→green.
- **Why persist to a file in addition to echoing to stdout.** The job log already
  shows the logs (good for at-a-glance triage), but a downloadable artifact is
  what survives log truncation/expiry and is what the brief's
  `actions/upload-artifact@v4` path requires.
- **`timeout-minutes` value (45).** Brief gives no number; 45 min comfortably
  covers a cold image build + 9-server cluster + the integration test while being
  far under the 6h default. Easily tuned later; the criterion is only that a bound
  *exists*.

## Scope discipline

Touched only the nightly Tier-2 operability path: log-capture-before-teardown,
`timeout-minutes`, `if: failure()` upload. Did **not** touch the gating model
(Tier-2 stays nightly + non-required, ADR-0009), the data path, the throughput
bench, or the Docker build-cache (all out of scope). No tracking-issue step added
(`issues: write` not requested); `permissions: contents: read` is left as-is since
artifact upload does not require write permissions.

Note on conflict with #154 (scheduling): #154 item 2 reworks the same
`run_integration` teardown. Per the brief, land the teardown rework once and do
not co-schedule the two concurrently.

## Verification (red → green)

Runner: the unit under test is Docker-free, so the focused, bounded
`cargo test -p xtask` exercises it (it is the same test `cargo xtask ci` runs via
`cargo test --workspace`); no hand-rolled/unbounded container invocation was used.

- **Red (pre-fix):** with the test present but `finish_integration` absent,
  `cargo test -p xtask` →
  `error[E0425]: cannot find function 'finish_integration' in this scope`.
- **Green (post-fix):** `cargo test -p xtask` → `2 passed; 0 failed`.
- `cargo fmt -p xtask` applied; `cargo clippy -p xtask --all-targets -- -D warnings`
  clean (commit-ready for the target's fmt/clippy hooks).
- YAML validated: `jobs.tier2.timeout-minutes = 45`; final step
  "Upload container diagnostics" present.

The C4-ci gate (`cargo xtask ci`) covers the xtask Rust change; the YAML change is
for deterministic file inspection at Check, as the brief specifies.

## Citations (path:line on `main` / post-fix)

- Pre-fix teardown-before-propagation: `xtask/src/main.rs:117-118` (`main`).
- Post-fix ordering: `xtask/src/main.rs:118` (call), `:130` (`finish_integration`),
  `:181` (`compose_logs`).
- Pre-fix workflow: `.github/workflows/integration-nightly.yml:28-47` (`main`).
- Post-fix workflow: `:31` (`timeout-minutes`), `:50-61` (`if: failure()` upload).
