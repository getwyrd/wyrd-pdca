# xtask: capture Tier-2 container logs before teardown on failure

## Root cause
In `run_integration` the cluster was torn down (`compose_down`) *before*
the test failure was propagated (`xtask/src/main.rs:117-118` on `main`),
so by the time `result?` surfaced the error the containers, their logs and
volumes were already destroyed — and the nightly `tier2` workflow job never
ran `docker compose logs`, had no `timeout-minutes`, and no `if: failure()`
surfacing. A failed nightly Tier-2 run — the only place a Tier-2 regression
surfaces (ADR-0009) — therefore preserved nothing to diagnose it.

## Fix
Restore the invariant that a failed run captures diagnostics before
destroying the environment. A small `finish_integration(result, capture,
teardown)` helper runs `capture` then `teardown` on failure and only
`teardown` on success, propagating `result` unchanged; teardown still always
runs, so a run never leaks containers. A new `compose_logs` runs
`docker compose logs --no-color --timestamps`, echoes them to the job log,
and persists a copy to `target/tier2-logs/docker-compose.log` (best-effort:
a capture failure only warns and never masks the test result). The nightly
workflow gains `timeout-minutes: 45` on the `tier2` job and an
`if: failure()` step that uploads `target/tier2-logs/` via
`actions/upload-artifact@v4`, so the captured diagnostics survive job-log
truncation. Scope is the Tier-2 job's operability only; the gating model is
unchanged (Tier-2 stays nightly and non-required, ADR-0009).

## Verified against
- `xtask/src/main.rs:117-118` (`main`) — pre-fix teardown ordering:
  `compose_down(&compose); result?;` tears down before the error propagates;
  the fix reorders this to capture-then-teardown.
- `xtask/src/main.rs:149` (`main`) — `compose_down` runs
  `down -v --remove-orphans`, confirming logs/volumes are destroyed at
  teardown, so capture must precede it.
- `.github/workflows/integration-nightly.yml:28-47` (`main`) — the `tier2`
  job has no `timeout-minutes` and no `if: failure()` surfacing; confirmed
  no workflow in the repo sets either (`git -C ../wyrd grep` on `origin/main`
  returns none).

## Test
`xtask/src/main.rs` unit tests (in the `#[cfg(test)]` module): `cargo test -p
xtask`. `failure_captures_logs_before_teardown` drives `finish_integration`
with order-recording closures and asserts `[capture_logs, teardown]` with the
error propagated unchanged; `success_tears_down_without_capturing_logs`
asserts a passing run runs only `teardown` and stays `Ok`. The helper is
generic over the two actions so the ordering is verified without a container
runtime, and the test runs inside `cargo xtask ci`. Red before the fix
(`E0425: cannot find function 'finish_integration'`), green after
(`2 passed`). The workflow-YAML change is not exercised by `cargo xtask ci`
and is verified by deterministic file inspection at Check (per the brief);
the live container-failure path is supplementary nightly evidence.

Fixes #150
