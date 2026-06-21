# Brief — issue 150 / ci-harden-nightly-tier2-job

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** ci-harden-nightly-tier2-job
- **Defect / goal:** the nightly Tier-2 container job is the **only** place a Tier-2
  regression surfaces (post-merge, non-required by design, ADR-0009), yet a failed run
  preserves nothing to diagnose it. In `run_integration` (`xtask/src/main.rs:111-118`)
  teardown runs **before** the error is propagated: `compose_down(&compose)` (line 117 —
  `docker compose ... down -v --remove-orphans`, defined at `xtask/src/main.rs:149-151`)
  destroys the containers, their logs, and volumes, then `result?` (line 118) surfaces
  the failure — and the workflow never runs `docker compose logs`. The `tier2` job in
  `.github/workflows/integration-nightly.yml` also has **no `timeout-minutes`** (a hung
  cold build drifts toward the 6h default) and **no failure surfacing** (`permissions:
  contents: read`; no `if: failure()` artifact/issue step).
- **Success criterion:** on a failing integration run, container diagnostics
  (`docker compose logs`) are captured **before** the cluster is torn down — either in
  `run_integration` ahead of `compose_down`, and/or via an `if: failure()` workflow step
  that uploads them with `actions/upload-artifact@v4`; AND the `tier2` job carries a
  `timeout-minutes` bound. At C4-verify the **xtask Rust change** (capturing logs before
  `compose_down`) is gated by `cargo xtask ci` green (build + clippy-clean); the
  **workflow-YAML change** (`timeout-minutes`, the `if: failure()` step) is **not** exercised
  by `cargo xtask ci` — it runs only fmt/clippy/build/test/cargo-deny/conformance/DST and
  never reads `.github/workflows/` — so it is verified by **deterministic file inspection at
  Check** (the reviewer confirms teardown is ordered after diagnostics capture and
  `timeout-minutes` is present). The live container-failure path needs Docker and is
  supplementary nightly evidence, not the Check criterion (mirrors #117's framing).
- **Invariant to restore:** a failed automated run must capture its diagnostic evidence
  before destroying the environment that produced it — teardown never precedes
  log/artifact capture. (General CI-operability property; the ADR-0009 nightly tier is
  only useful if its failures are diagnosable.)
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (CI / xtask only; no GUI, no data-path change)
- **Conflicts with:** 154
- **Scope:** the nightly Tier-2 job's **operability** only — (a) capture container logs on
  failure before teardown, (b) add `timeout-minutes`, (c) optional `if: failure()`
  log/artifact upload (and, if desired, a tracking-issue step needing `issues: write`).
  / **out of scope:** the gating model (Tier-2 stays nightly + non-required — by design,
  ADR-0009; do NOT make it a required check); the data path; the throughput bench; the
  Docker build-cache optimization (separate, optional).
- **Citations expected:** Do cites path:line on `main` for the teardown ordering
  (`xtask/src/main.rs`) and the workflow change (`.github/workflows/integration-nightly.yml`).
- **Prior-art check:** searched the target checkout — `.github/workflows/` holds 7 workflow
  files; the single Tier-2/nightly one is `integration-nightly.yml`, and none of the 7 sets
  `timeout-minutes` or `if: failure()`; `run_integration` teardown ordering is as cited. No
  open/closed PR addresses nightly diagnostics. Net-new.
- **Disposition hint:** likely-fix

## Scheduling note
Conflicts with #154: its item 2 reworks the same `run_integration` teardown
(`xtask/src/main.rs:111-118`) that this issue's log-capture change touches — land the teardown
rework once and do not co-schedule the two in one concurrent wave.

## STOP discipline
Draft only until Check sign-off. A draft PR MAY be opened for CI; it MUST NOT be marked
ready before sign-off accepts.
