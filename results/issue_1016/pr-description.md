## Summary
**User impact:** an S3 multipart upload whose client goes away is meant to be cleaned up
automatically after a while. On two common deployments that never happened: a custodian
started without a D-server fleet just exited, and a custodian whose whole fleet was down at
startup crashed before doing any cleanup. On those deployments abandoned uploads stayed open
for good, and the space their parts used was never freed.

This PR makes the custodian run that cleanup on every way it can be started. With no fleet it
now stays up and cleans up every interval. With an unreachable fleet it does one cleanup pass,
then fails exactly as before.

This is the second of three parts of #625. It builds on the first part, #1097, and must merge
after it. The diff does not apply to `main` without #1097.

## What to look at
- **No fleet:** `wyrd custodian --data-dir <dir> --interval-secs 1` with no `--endpoints`.
  Before, it printed "nothing to reconstruct" and exited. Now it prints "Running the multipart
  reaper alone every 1s", runs a cleanup pass each second, and exits 0 on Ctrl-C.
- **Fleet down:** `wyrd custodian --endpoints http://127.0.0.1:1 --ids 1 --failure-domains d1`.
  It still fails with "NO configured D server was reachable at startup" and exit 101. The
  difference is that it runs one cleanup pass first.
- **Fleet up (whole or partial):** no change in behaviour.
- **New refusals with no `--endpoints`:** a data dir with no store, `--ids`,
  `--failure-domains`, `--gc-expired-pending` and `--interval-secs 0`. Before, the role
  exited 0 and ignored them. The usage text says so.

## Root cause
The reaper from #1097 runs only from the fleet role's interval loop. The no-endpoints branch
of `cmd_custodian` returned before that loop, and the empty-fleet branch panicked before it.

## Fix
- `crates/server/src/custodian.rs`: one private interval driver, `run_until` (`:694`), now
  runs both the fleet loop (`run_reconstruction_until`, `:636`, signature unchanged) and a new
  no-fleet loop (`run_reaper_until`, `:545`). Both call the same reaper step, `reaper_pass`
  (`:508`). The stop signal is polled once before the first pass, so Ctrl-C is handled from
  the start (`:714-717`). The wait between passes is `biased` toward the stop (`:739-743`).
  The fleet loop still calls the reaper outside GC's whole-fleet gate (`:847` vs `:914`), and
  the gate is unchanged.
- `crates/server/src/cli.rs`: the no-endpoints branch (`:1193-1223`) opens the existing store
  and runs `run_reaper_until` until Ctrl-C. The empty-fleet branch (`:1283-1330`) runs one
  `reap_abandoned_uploads_once` pass, except under `--reconcile-after-restore` (`:1311`). It
  logs any pass error and then hits the unchanged `panic!` (`:1325`). Usage text: `:512-526`.
- `docs/design/architecture/06-runtime-view.md:94`: the reaper paragraph now describes the
  three startup shapes.

## Verification
- **Claim:** with no `--endpoints`, the ordinary role (no one-shot flag) tears down an
  abandoned upload within one process lifetime, with passes spaced by `--interval-secs`.
  - **Checked:** `crates/server/src/cli.rs:1193-1223` → `custodian.rs:545` → `:694`.
  - **Test:** `crates/server/tests/multipart_reaper_deployed.rs:831`
    (`no_endpoints_role_reaps_every_interval_in_one_lifetime`). It runs the real `wyrd`
    binary on a seeded redb store. It also checks that a second `wyrd` cannot open the
    locked store while the role is up. Fails before the fix: the role exits at once and the
    store is unchanged.
- **Claim:** with an unreachable fleet, every start runs exactly one pass and then exits 101
  with the old message. Restarts finish the teardown.
  - **Checked:** `crates/server/src/cli.rs:1283-1330`.
  - **Test:** `multipart_reaper_deployed.rs:1017`
    (`empty_fleet_role_runs_one_reaper_pass_then_fails_as_before`). Fails before the fix on
    the store assertion. The exit status is 101 on both trees.
- **Claim:** `--reconcile-after-restore` on an unreachable fleet fails without touching the
  store.
  - **Checked:** `cli.rs:1311`.
  - **Test:** `multipart_reaper_deployed.rs:1074`. It passes on both trees: it guards
    against a reaper pass running before the restore pass.
- **Claim:** a Ctrl-C during the first pass lets that pass finish, and the role exits 0.
  - **Checked:** `custodian.rs:714-717`.
  - **Test:** `multipart_reaper_deployed.rs:934`. It freezes the role with SIGSTOP inside
    the pass, sends SIGINT, then resumes it. Linux-only: elsewhere it reports as ignored.
    Without the early poll, the role is killed by signal 2.
- **Claim:** a stop that is ready when the interval ends wins over the next pass.
  - **Checked:** `custodian.rs:739-743`.
  - **Test:** `crates/server/tests/custodian_gc.rs:2254`. Fails when `biased` is removed.
- **Claim:** on a degraded fleet (GC deferred), the reaper still runs.
  - **Test:** `custodian_gc.rs:1866`. This already worked after #1097, so the test passes
    on both trees and only guards against a regression. It fails if the reaper call is moved
    behind GC's gate.
- **Claim:** the no-fleet loop survives store faults. Commits with an unknown outcome are
  drawn both applied and not applied.
  - **Test:** `custodian_gc.rs:2063`. All 32 seeds reach full teardown.
- **New-file test run:** with the fix, 9 tests run and 9 pass. With the production change
  reverted, 9 run and 7 fail. The 2 that pass on both trees are the restore guard and the
  "creates no store" check. `cargo xtask ci` passes.

**Test-only change outside the feature:** `custodian_gc.rs:1250`
(`register_meter_drop_callsites`) works around a deadlock between `tracing-core` 0.1.36 and
the OpenTelemetry meter provider's drop. It shows up when tests build and drop custodian
services in parallel. The deadlock exists on `main` (about 1 hang in 100 runs of
`custodian_day_one`). The new tests in `custodian_gc` made it more likely there, so this PR
guards that binary only.

**Known gaps:** no test can yet catch a pass cut short by Ctrl-C on the fleet and no-fleet
loops, because the test stores never suspend in the middle of a pass. The usage-text test
inherits `RUST_LOG` from the caller.

Fixes #1016
