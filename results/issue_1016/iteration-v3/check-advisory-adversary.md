# Adversarial review — issue 1016 (625.2, multipart reaper on every deployed startup shape)

Lens: refute the red→green evidence and the verdict. Toolchain was available; the cases below
were re-run in scratch copies of `$PDCA_TARGET` (base = HEAD with `crates/server/src/{cli,custodian}.rs`
reverted; fix = the target tree as given).

## Findings

- NEEDS-HUMAN [human] — **G2b has no red leg on this base; the brief's premise for it is false.**
  On the base, `crates/server/src/custodian.rs` (HEAD) already dispatches the reaper inside the
  reconstruction pass (`Some(&reaper)` at HEAD `:680`), *ahead of* GC's whole-fleet gate (HEAD
  `:749`), not behind it. So the brief's Defect ("That call sits behind GC's whole-fleet gate … the
  reaper runs only on a healthy fleet", brief.md:13-15) and Falsifiability ("G2b reds because the
  reaper sits behind the whole-fleet gate", brief.md:57) do not hold for this base. **Reproduced:** with
  production code reverted to HEAD and the patched test file kept,
  `cargo test -p wyrd-server --test custodian_gc deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred`
  → `1 passed`. The fleet-loop edits (`crates/server/src/custodian.rs:823`, `:890`) are therefore a
  refactor with no change in behavior, and the doc line "Dispatching the reaper only inside GC's whole-fleet branch
  turns it red" (`crates/server/tests/custodian_gc.rs:68`) describes a hypothetical mutant, not the
  base. No gate measured a G2b red: C4-verify ran only `multipart_reaper_deployed`
  (gate-logs/C4-verify.log). The test also never checks that GC was actually deferred (no
  "gc pass deferred" assertion anywhere in `custodian_gc.rs`, test at `:1753`), so it would pass
  with the GC gate removed too. A human should decide whether to record G2b as a regression guard only
  (and correct the brief/§6 record), or confirm the real 625.1 base differs from this folded one.

- NEEDS-HUMAN [impl] — **A Ctrl-C during the first pass kills the role by signal, contradicting
  the new docs. Reproduced.** `shutdown` is a lazy async block (`crates/server/src/cli.rs:1145-1148`),
  and tokio installs the SIGINT handler only when `ctrl_c()` is first polled. `run_until` skips the
  `select!` on the first interval (`crates/server/src/custodian.rs:715-720`), so nothing polls
  `shutdown` until the first pass has finished. Until then SIGINT keeps its default action. Case: seed 200 idle `Open`
  sessions, run `wyrd custodian --data-dir D --metadata-backend redb --interval-secs 1` (no
  `--endpoints`), and send SIGINT when the "Running the multipart reaper alone" banner is read:
  **10/10 runs ended by signal 2**, 0 pass tallies, no "shutting down" line. That breaks the claims
  at `crates/server/src/custodian.rs:680-681` ("a pass, once started, runs to its end, so a Ctrl-C
  never drops one mid-commit") and `docs/design/architecture/06-runtime-view.md:94` ("a stop takes
  effect between two passes, never during one"). In my runs the kill landed before the first
  commit, so the store was unchanged, and redb commits are atomic. What actually breaks is the exit
  status (a supervisor sees a signal death, not exit 0) and the false doc claims. G2a only sends SIGINT after the 5th tally
  (`crates/server/tests/multipart_reaper_deployed.rs:661`), so it never covers this window. Fix: poll
  `shutdown` once before the first pass (e.g. a `biased` `select!` against `std::future::ready(())`,
  or register `tokio::signal::unix::signal(SignalKind::interrupt())` before the loop), and add a G2a
  variant that interrupts on the banner and asserts exit 0 plus "shutting down". This confirms the T4
  blocking findings at `cli.rs:1212` / `custodian.rs:715`.

- NEEDS-HUMAN [human] — **The new destructive dispatches have no seeded Tier-0 DST coverage.** The
  rubric's *Test fidelity* class says "a new destructive or concurrent path lands with seeded Tier-0
  DST coverage". The no-endpoints loop (`crates/server/src/custodian.rs:722-745`) and the
  empty-fleet pass (`crates/server/src/cli.rs:1300-1313`) are new paths that fence and delete
  records, and they are covered only by subprocess tests. The brief forbids editing `crates/dst/`
  (brief.md:69-71), so the builder cannot close this. A human should decide whether dispatch-only
  wiring over the DST-covered reaper (625.1) meets the rule, or hand a row to child-3 / a follow-up
  issue. (Same gap as T4's TEST-GAP at `cli.rs:1299`/`:1300`.)

- NEEDS-HUMAN [human] — **`--interval-secs 0` is now refused for the existing fleet role as well.**
  `crates/server/src/cli.rs:1058-1066` refuses 0 for every non-one-shot shape, including
  `--endpoints … --interval-secs 0`, which the base accepted. A deployment that used 0 would now fail
  at startup and crash-loop with no reconstruction or GC at all. The brief scopes this slice to "where the
  pass runs" and says it "adds no reaper logic" (brief.md:8); a new startup refusal on the fleet
  shape goes past that. Low practical risk: the repo's compose files use 15, and usage/docs are updated.
  A human should decide whether to keep it as a hot-loop guard or limit it to the no-endpoints shape.

## Attempted to refute; could not

- **G2a/G2c red→green.** gate-logs/C4-verify.log shows both failing on the base for the right
  reason (0 passes, session as seeded; the base role exits at once or panics) and green with the
  fix. The base runs fail on the store and pass-count assertions, not on an exit-status
  tautology.
- **G2a "every interval".** The span check (`multipart_reaper_deployed.rs:691`) kills the
  zero-sleep mutant from iteration 2. One process, no restarts, SIGINT, then exit 0.
- **G2c "exactly one pass".** Each start is checked with `passes != 1` (`:736`), and the first start
  must change the session, so a multi-pass or no-pass mutant fails.
- **Restore one-shot on an empty fleet.** `cli.rs:1300` skips the pass, and the test at `:773`
  checks that the store snapshot is byte-identical. Holds.
- **Backend isolation.** `custodian()` pins `--metadata-backend redb` and drops
  `WYRD_METADATA_BACKEND` (`multipart_reaper_deployed.rs:237-247`). Holds.
- **Unbounded open on a networked store in the empty-fleet path.** `TikvMetadataStore::connect`
  runs under the adapter deadline (`crates/metadata-tikv/src/lib.rs:1050-1062`), so the panic is
  still reached. Holds.
- **Fleet-loop control flow after the sleep moved.** No `continue` or early return in the fleet
  branch skips or doubles the wait (`custodian.rs:715-720` vs HEAD `:795-798`). The only `continue`
  is the no-fleet branch's (`:745`), which then waits. Holds.
- **No-fleet refusals and store creation.** `no_fleet_conflict` runs before anything is opened
  (`cli.rs:1051-1055`). `open_no_fleet_meta` uses `open_existing` on redb, and both no-store tests
  check that no `meta.redb` is created. Holds.

<!-- pdca:leaf-complete -->
