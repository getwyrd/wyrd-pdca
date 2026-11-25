# Adversarial review — issue 1016 (625.2: the reaper on every custodian startup shape)

I re-ran the proofs myself (cargo 1.96.0, scratch clones of base `dbb41ef` and of base + patch, since removed).

- **G2a, G2c and the usage test:** the C4-verify red→green holds up. They run the real `wyrd` binary with no one-shot flag over a real redb store, and on the base they fail on the store assertion. One wording slip in the gate line: "4 test(s) ran red" means 4 tests ran, but only 3 failed. The fourth, the restore guard, passes on both trees, which is what a guard should do.
- **G2b:** it **passes on the unpatched base**. Details in the first finding.

## Findings

- NEEDS-HUMAN [human] — **G2b has no red leg, because the fleet-shape defect in the brief does not exist on this base.** I ran `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (crates/server/tests/custodian_gc.rs) on base `dbb41ef` with only the test added, and it printed `ok`. On the base, the reaper already rides the reconstruction pass (`git show HEAD:crates/server/src/custodian.rs`: `Some(&reaper)` at :680), and that pass sits outside GC's gate (`if fleet.len() == operator_fleet_size` at :749). The brief misreads the base twice: its Defect line ("That call sits behind GC's whole-fleet gate, custodian.rs:680-713") and its Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). So the fleet-loop edit at crates/server/src/custodian.rs:791 is a refactor that changes no behaviour. The PR must not claim it fixes "on a partially degraded fleet it reaps nothing while GC is deferred". G2b does catch negation (11): on the base I gated `Some(&reaper)` behind `fleet.len() == operator_fleet_size`, and G2b failed at custodian_gc.rs:1809. Keep G2b as a regression guard, and strike the fleet-shape defect claim from the summary and the PR text.

- NEEDS-HUMAN [impl] — **G2a still does not prove "every interval": it only counts passes.** The mutant: change crates/server/src/custodian.rs:612 from `tokio::time::sleep(interval)` to `tokio::time::sleep(interval.saturating_sub(interval))`. The no-fleet loop then runs passes back to back and ignores `--interval-secs`. `no_endpoints_role_reaps_every_interval_in_one_lifetime` **passes**, and the whole test takes 0.13 s with `--interval-secs 1`. The check at crates/server/tests/multipart_reaper_deployed.rs:641 (`run.passes >= PASSES_IN_ONE_RUN`) measures no time. Fix: in `run_role` (:305), record the `Instant` of each tally line, and assert that the first-to-fifth gap is at least `(PASSES_IN_ONE_RUN - 1) x interval`, minus a small slack. A loop that ignores its interval hammers a networked metadata store, so this is a property the brief asks for, not a matter of taste.

- NEEDS-HUMAN [human] — **Two loops instead of one driver, and only the new loop can drop a pass mid-commit.** The brief's scope says "One reaper interval driver that both deployed shapes run". The patch shares the step (`reaper_pass`) but not the loop:
  - The no-fleet loop (crates/server/src/custodian.rs:582-592) wraps every pass in a 60 s `timeout` and races it against Ctrl-C. On TiKV or FDB, a pass can therefore be dropped while a commit is in flight.
  - The fleet loop (custodian.rs:904-907) checks shutdown only between passes and has no bound on a pass. It never drops a pass, but a hung reaper pass stalls it for good.

  So the same reaper runs under two different cancellation rules. The patch's comments argue that a dropped pass is the same as a crash, because every commit is conditional, and that looks right. But the rubric's Test-fidelity rule asks for seeded Tier-0 DST coverage (DST: deterministic simulation testing) of a new destructive or concurrent path, and `crates/dst/` is outside this slice's file partition. The human has two options: accept the difference and file a DST follow-up (plus a matching bound for the fleet loop), or drop the timer and the mid-pass shutdown race so both shapes behave the same.

- NEEDS-HUMAN [human] — **`wyrd custodian` with no flags now holds the default store's lock until Ctrl-C.** I reproduced this with the patched binary in an empty dir. I started `wyrd custodian --interval-secs 5` (default `--data-dir wyrd-data`), then ran `wyrd put payload.txt --key k1`. It failed with `wyrd: Database already open. Cannot acquire lock.` and exit 1. After Ctrl-C, the same put succeeded. The path is crates/server/src/cli.rs:1184 → `run_reaper_over_backend` → `open_local_meta_redb` (cli.rs:2307), and the store stays open for the life of the loop. On the base, this shape exited at once and never opened the store. The fleet shape already held the lock on the base, and a reaper on redb has to open the store, so this follows from the brief. But it turns a harmless no-op into something that blocks `wyrd put` and `wyrd s3` on the same dev data dir. Decide whether to accept it as dev-only redb behaviour and say so in the usage text.

- NEEDS-HUMAN [impl] — **The doc overstates the time bound on the default backend.** The sentences appended to docs/design/architecture/06-runtime-view.md:94 say: "Each of those passes is bounded in time; on a networked store a pass past the bound is abandoned". On redb, no pass has a time bound. The patch's own doc comment on `NO_FLEET_REAPER_PASS_BOUND` (crates/server/src/custodian.rs:146-150) says so. Reword along these lines: "On a networked store each of those passes is bounded in time, and a pass past the bound is abandoned …; on redb, only the per-pass budgets bound it."

- NEEDS-HUMAN [impl] — **A role that now stays up silently drops `--gc-expired-pending` when there is no `--endpoints`.** crates/server/src/cli.rs:1054 arms `expired_pending`, but the no-endpoints branch (cli.rs:1161-1188) never reads it, and its banner doesn't mention it. `wyrd custodian --gc-expired-pending` now runs for days, never sweeps, and never says so. The base ignored the flag too, but it exited at once. The one-shot already refuses this exact conflict (`reaper_one_shot_conflict`, cli.rs:1039). Low priority: a one-line warning or a refusal on this branch fixes it.

## Notes (no action needed)

- C5's one MISSED mutant (crates/server/src/cli.rs:1966, `open_within_no_fleet_bound` → `Ok(Default::default())`) comes from a cfg gate. The function is under `#[cfg(any(feature = "tikv", feature = "fdb"))]` (cli.rs:1962), so the default-feature mutant build never compiles it. It is not a gap in the default build, but no test exercises that bound under any feature set.
- C4 diff coverage was not measured, because the patch does not apply on the harness's origin/main. From here I can't tell whether the harness copy is out of date or main has moved under cli.rs/custodian.rs.

## Attempted and could not refute

- The restore guard (crates/server/src/cli.rs:1273): `empty_fleet_restore_one_shot_fails_with_the_store_untouched` (crates/server/tests/multipart_reaper_deployed.rs:721) compares the whole store byte for byte.
- G2c's "exactly one pass": every start must emit exactly one tally (multipart_reaper_deployed.rs:684, `run.passes != 1`), so a mutant that runs several passes fails it.
- Backend pinning: `role()` (multipart_reaper_deployed.rs:198) passes `--metadata-backend redb` and removes `WYRD_METADATA_BACKEND` from the child's environment.
- The panic message and the exit status on the empty-fleet path are unchanged (cli.rs:1293-1298). An error from the pass is logged, not returned.
- The fleet-loop refactor (custodian.rs:791) passes the same inputs to `reconcile_pass` as the base did: the reaper context, `Some(&ctx)` and `clock.now_millis()`.

<!-- pdca:leaf-complete -->
