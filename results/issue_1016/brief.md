# server: the multipart reaper runs on every deployed custodian startup shape (625.2)

> Second of three slices #625 (multipart reaper) was split into on 2026-10-30. It builds on
> 625.1 (child-1), which lands the reaper pass, `ReaperContext` / `ReaperPolicy`, the operator
> one-shot, and the reaper's dispatch from `reconcile_step` on the fleet loop's existing
> call. Do reads ONLY this file. Design: 0016 decision 6 (`0016:1794-2277`), above all the
> requirement that the reaper be running wherever the verbs are exposed (`0016:234-268`). This
> slice adds no reaper logic. It decides **where the pass runs**.

- **Slug:** multipart-reaper-deployed-shapes
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 625.1, the reaper runs from the one-shot and from the fleet loop's
  `reconcile_step` call. That call sits behind GC's whole-fleet gate (`if fleet.len() ==
  operator_fleet_size`, `crates/server/src/custodian.rs:680-713`), so the reaper runs only on
  a healthy fleet. Two other startup shapes reach no pass at all.
  - **Without `--endpoints`** the role prints "nothing to reconstruct" and **exits**
    (`crates/server/src/cli.rs:1083-1100`). The reaper is record-only, so a role with no fleet
    still has work to do.
  - **With `--endpoints` given and none reachable**, the role panics
    (`cli.rs:1161-1174`) before it reaches the loop (`:1243`).

  So production reaps nothing on a no-endpoints or a fully degraded deployment, and on a
  partially degraded fleet it reaps nothing while GC is deferred.
- **Success criterion:** a NEW test file `crates/server/tests/multipart_reaper_deployed.rs`,
  using the same subprocess mechanics as 625.1 and a store seeded **exactly as 625.1's leg A**
  (an idle `Open` session: `created_at_millis = now − 14 days`, progress stamps at `now − 10
  days`, leases lapsed at `now − 9 days`, one committed `part:`/`psum:` pair, crashed `sidx:`
  residue, `mpuctl.count = 1`). The leg-A transition means: `mpu:`, `part:`, `psum:`, `sidx:`
  and `slot:` empty for the session; the count back to its pre-session value; an `orphan:`
  mark for every position of both byte sets; and both session `retire:` ranges empty. The
  seeding helpers are **copied** into this file and not shared: an added
  `crates/server/tests/support/mod.rs` would be classified as a test target by
  `run-verify.sh:141-144`.
  **(G2a) The no-endpoints shape (binding).** Start the **ordinary** `wyrd custodian` role as a
  subprocess with no `--endpoints`, a short `--interval-secs` and **no one-shot flag anywhere
  in argv**. Let it run one bounded interval, terminate it, and assert the leg-A transition.
  **(G2c) The empty-fleet shape (binding).** Start the ordinary role with `--endpoints` naming
  one address nothing listens on (e.g. `http://127.0.0.1:1`, refused at once), with matching
  `--ids` / `--failure-domains`, a short connect timeout if the role takes one, and no one-shot
  flag. Wait for exit, with a bound. Assert all three:
  - a **non-zero** exit status;
  - stderr contains the existing diagnostic ("NO configured D server was reachable at
    startup", `cli.rs:1170`);
  - the leg-A transition happened.

  **(G2b) The fleet shape, degraded (`cargo xtask ci` evidence, not C4).** Append a case to the
  existing `crates/server/tests/custodian_gc.rs`. It already drives the real run loop with an
  operator fleet larger than the reachable one (`drive_deployed_loop_operator`, `:447-480`; the
  startup-partial case, `:886`). Seed leg A's fixture, drive the loop with
  `operator_fleet_size` greater than the servers it is given (so GC is deferred), and assert
  the leg-A transition. It goes in a modified file because it calls
  `run_reconstruction_until`, whose signature this slice may change.
- **Falsifiability:** on 625.1's folded base, G2a reds on the store assertion because the role
  exits at once. G2c reds on the store assertion because the role panics before any pass; its
  exit status is non-zero on both trees, so the store assertion is what discriminates. G2b
  reds because the reaper sits behind the whole-fleet gate. All three reds are in-process
  assertions with no fleet and no container. Do records the base commit, confirms 625.1 is on
  it, and records the RED leg's executed and failed test counts.
- **Invariant to restore:** the actor that drives an abandoned session out exists on every
  deployment that runs the custodian role (0016 `:234-268`). Without it, 625.1's invariant
  holds only on a healthy fleet. C-1 applies.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1015
- **Conflicts with:**
- **Ordering note:** wave 2 of #625's split, in parallel with child-3 by file partition. This
  slice owns `crates/server/src/cli.rs`, `crates/server/src/custodian.rs`,
  `crates/server/tests/custodian_gc.rs`, its new test file and
  `docs/design/architecture/06-runtime-view.md`. It MUST NOT edit anything under
  `crates/custodian/`, `crates/core/`, `crates/dst/` or `08-crosscutting-concepts.md`, which
  child-3 owns. If it must, raise a §6 item. Construct the policy with `ReaperPolicy::default()`
  as 625.1 does, and never add window flags. #633 must be re-pointed to this slice and to
  child-3 after acceptance.
- **Surfaces:** data
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/multipart_reaper_deployed.rs`, a **NEW** file, not
  `#![cfg]`-gated. G2b is appended to the existing `crates/server/tests/custodian_gc.rs`.
- **Verification posture:** red→green at Check for G2a and G2c, and ci evidence for G2b.
  Negations recorded in `build-notes.md`:
  - (11) dispatch the reaper only inside GC's whole-fleet branch ⇒ G2b fails;
  - (12) leave the no-endpoints early exit in place ⇒ G2a fails;
  - (13) skip the reaper pass on the empty-fleet startup ⇒ G2c fails.

  Do records in `build-notes.md` the call path from `cmd_custodian` to the reaper for each of
  the three shapes.
- **Production reach:** this slice *is* the production reach. G2a and G2c run the ordinary role
  binary with no one-shot flag. G2b runs the fleet role's real interval driver. Do MUST NOT add
  a test-only entry.
- **Scope:**
  - **One reaper interval driver** that both deployed shapes run every interval. The
    no-endpoints role runs it instead of exiting, and the fleet role runs the same driver.
  - The reaper runs however much of the fleet is reachable, **outside** GC's whole-fleet gate.
    GC keeps its gate unchanged.
  - **The empty-fleet startup path (settled 2026-10-22):**
    - Run **exactly one** reaper pass through `reconcile_step`, under the leadership the role
      already holds (`Custodian::elect`, `cli.rs:1028`, runs before the endpoints check). The
      pass runs after the fleet comes back empty and before the existing panic. Then fail as
      today.
    - The panic stays. `connect_fleet` returns only the servers reachable at startup and never
      re-dials (`custodian.rs:247-263`), so a role left up on an empty fleet would vanish
      cleanly.
    - Do MUST NOT change the panic's message or its exit status, or the start-degraded
      behaviour for a partly reachable fleet.
    - The pass is bounded by 625.1's per-pass budgets.
  - Docs: the startup shapes in `06-runtime-view.md`, appended to 625.1's reaper paragraph.

  **Out of scope:**
  - any reaper decision or window (625.1 / child-3);
  - the keep-alive-and-redial alternative for the empty fleet (⚑ the human may file it; it
    changes the reconstruction plane's startup);
  - GC's own gating.
- **Repro instruction:** on 625.1's base, seed leg A's fixture into a temp redb store, drop
  it, and run `wyrd custodian --interval-secs 1` with no `--endpoints` as a bounded
  subprocess. It exits at once, and on reopening the store the records are exactly as
  seeded. Then run it with `--endpoints http://127.0.0.1:1` and matching ids: it panics with
  the diagnostic, and the store is unchanged.
- **Citations expected:**
  - `cmd_custodian` (`cli.rs:913-1010`), the no-endpoints exit (`:1083-1100`), the empty-fleet
    panic (`:1161-1174`) and the dispatch (`:1241-1259`);
  - `run_reconstruction_until` (`custodian.rs:521-545`), the whole-fleet gate (`:680-713`) and
    `connect_fleet` (`:247-263`);
  - `drive_deployed_loop_operator` (`custodian_gc.rs:447-480`).

  `path:line` for every change.
- **Prior-art check (triage cycles):** the plan reviews of 2026-10-20 and 2026-10-22 on #625
  (`results/issue_625/plan-advisory-plan-reviewer.md`, finding 2) found the empty-fleet path.
  No reaper wiring exists before 625.1. Re-run by path on your base.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Strengthen the scheduling oracles — restarting G2a until convergence admits a role that reaps only at startup, while G2c never asserts exactly one pass per invocation (crates/server/tests/multipart_reaper_deployed.rs:234; crates/server/tests/multipart_reaper_deployed.rs:510; crates/server/tests/multipart_reaper_deployed.rs:544).; T4 Contribution — Complete the affected-path merged and closed/rejected prior-art check — this target contains only one synthetic base commit, no remote and no PR archive, so the required history decision cannot be settled from these inputs (brief.md:126; reviewer-prior-art.log:1).; T5 Judgment — Pin subprocesses to the seeded redb backend — inherited `WYRD_METADATA_BACKEND` overrides the temporary-store assumption and can select a network store in feature-enabled builds; a harmless invalid value independently makes both tests fail (crates/server/tests/multipart_reaper_deployed.rs:152; crates/server/src/cli.rs:289; reviewer-backend-isolation.log:62).; **`--reconcile-after-restore` on a dead fleet now changes the store before it fails.** `crates/server/src/cli.rs:1251-1296` (the empty-fleet reaper pass) runs before the restore one-shot's own branch at `cli.rs:1329`. The no-endpoints branch refuses the restore one-shot *before* reaping (`cli.rs:1164-1175`); the empty-fleet branch has no such guard. Reproduced: I seeded leg A and ran `wyrd custodian --reconcile-after-restore --endpoints http://127.0.0.1:1 --ids 1 --failure-domains domain-1 --connect-timeout-secs 1`. It exited 101 with the usual diagnostic, but `mpu:` went from `Open@0` to `Aborting@1`, 3 `orphan:` marks were written and a `retire:bytes:` obligation was created. On the base, the same command panics and leaves the store untouched. The restore pass is "explicit, never automatic" and fences resurrected sessions itself under its own restore-fence generation (`crates/custodian/src/restore.rs:64-69`, `:94-100`). Now the reaper acts on a just-restored store before that generation is opened, under a command that then fails. The brief scopes the pass to "the ordinary role". Fix: skip the pass when `reconcile_after_restore` is set, and add a test that the store stays byte-identical.; **G2a does not prove that the role reaps "every interval".** `run_until` restarts the role up to `CONVERGE_RUNS = 6` times (`crates/server/tests/multipart_reaper_deployed.rs:66`, `:234`). The restarts do the work that later intervals should do. I tried a mutant where the no-endpoints branch runs one pass (`std::future::ready(())`) and then idles on `shutdown.await` forever. G2a **passes** (3 restarts, 9.1 s). With `CONVERGE_RUNS = 1`, the mutant fails ("did not converge within 1 runs") and the real fix passes in 3.1 s. The brief's G2a also asks for a single bounded run. Fix: give G2a a single window, with no restarts.; **G2c does not pin the "exactly one pass" rule** (brief Scope: "Run exactly one reaper pass"). I tried a mutant where the empty-fleet call passes `tokio::time::sleep(3s)` as its shutdown, so the role runs several passes before it panics. G2c **passes**. After one real pass, the session is still present as `Aborting@1` and its `retire:bytes:` obligation is still standing (I saw this in the restore repro above). So asserting after run 0 that `mpu:` still exists and is `Aborting` would catch the mutant. The brief's literal G2c (one run, then all three asserts) can't be met, because teardown takes 2–3 passes. The restart loop is a reasonable workaround, but the test doc at `multipart_reaper_deployed.rs:530` should say so.; **`EMPTY_FLEET_REAPER_PASS_BOUND` does nothing on redb, the default backend.** The comments at `cli.rs:908-913` and `cli.rs:1267-1269` say the pass is bounded "the store open included". But `open_local_meta_redb` (`cli.rs:2251-2256`) is synchronous, and the redb adapter's `get`/`scan`/`scan_page`/`commit` (`crates/metadata-redb/src/lib.rs:200-342`) never await. So the whole pass finishes inside the first poll of `tokio::time::timeout` (`cli.rs:1270`), and the timer can never fire. The bound only works for tikv/fdb. (Dropping a TiKV transaction on timeout is safe: `crates/metadata-tikv/src/lib.rs:1142` uses `CheckLevel::Warn`.) Fix: either say the bound covers only the networked backends, or run the redb arm on a blocking thread. Not reproduced, since I can't make redb hang; this comes from reading the code. Low practical risk.; **The new subprocess tests take the backend from the caller's environment.** `role()` (`multipart_reaper_deployed.rs:152-158`) neither passes `--metadata-backend redb` nor clears `WYRD_METADATA_BACKEND`, and `resolve_backend` falls back to that variable (`cli.rs:292`). On a build with `--features tikv` and that variable set to `tikv`, this destructive pass would run against the configured cluster instead of the temp redb store. Fix: pass the flag explicitly or clear the variable.; `crates/server/src/cli.rs:1270`: The new empty-fleet reaper also runs for `--reconcile-after-restore`, before that mode is handled at line 1329. A restore command with all D servers unavailable previously failed without touching metadata; it now aborts eligible uploads, deletes staged records, and writes orphan marks before reporting failure. Restrict this reaper dispatch to the ordinary role and add a restore-mode regression asserting that the seeded store remains unchanged on refusal.; `crates/server/tests/multipart_reaper_deployed.rs:155`: The subprocess does not select `--metadata-backend redb`. It inherits `WYRD_METADATA_BACKEND`, which `resolve_backend` honors even when `--data-dir` is supplied (`crates/server/src/cli.rs:289`). With the corresponding feature and connection environment enabled, these destructive tests can reap an actual TiKV/FDB store while leaving their temporary redb fixture untouched. Explicitly select redb in the shared command builder.; `crates/server/src/cli.rs:1270`: The new 60-second timeout cannot enforce its bound on redb. The wrapped future synchronously opens the database at `crates/server/src/cli.rs:1898`; redb reads and commits also execute synchronously inside their async methods (`crates/metadata-redb/src/lib.rs:200`, `crates/metadata-redb/src/lib.rs:295`). Slow or stalled filesystem work never yields to the timeout, so the empty-fleet startup can exceed the promised deadline without reaching its restart-triggering failure. Use a deadline mechanism effective during synchronous work and validate that failure path.; `crates/server/tests/multipart_reaper_deployed.rs:510`: G2a restarts the no-endpoints process up to six times through `run_until` (line 234). An implementation that runs one pass at startup and then merely stays alive passes: successive test-driven restarts finish the durable teardown. Require convergence within one bounded process lifetime, or assert multiple pass emissions from that same process, to detect a missing interval driver.; `crates/server/tests/multipart_reaper_deployed.rs:544`: G2c checks eventual teardown, nonzero exit, and the diagnostic, but never verifies exactly one reaper pass per invocation. Running several passes before panicking satisfies every assertion. Check each invocation's pass emissions or its expected intermediate store state before restarting; the existing `reaper_passes` telemetry provides a production observation point (`crates/custodian/src/reaper.rs:1375`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Preserve progress across pass deadlines — three timeouts on successful delayed reads leave an eligible upload Open, while an uncancelled pass reaches it (crates/server/src/custodian.rs:583; crates/custodian/src/reaper.rs:777; reviewer-timeout-probe.log:4).; T5 Judgment — Add seeded cancellation/recovery coverage — the new timeout/shutdown path drops destructive work, while synchronous-redb tests and existing awaited unknown-outcome cases do not exercise that drop (crates/server/src/custodian.rs:587; crates/server/tests/multipart_reaper_deployed.rs:204; crates/dst/tests/custodian.rs:8052).; **G2a still does not prove "every interval": it only counts passes.** The mutant: change crates/server/src/custodian.rs:612 from `tokio::time::sleep(interval)` to `tokio::time::sleep(interval.saturating_sub(interval))`. The no-fleet loop then runs passes back to back and ignores `--interval-secs`. `no_endpoints_role_reaps_every_interval_in_one_lifetime` **passes**, and the whole test takes 0.13 s with `--interval-secs 1`. The check at crates/server/tests/multipart_reaper_deployed.rs:641 (`run.passes >= PASSES_IN_ONE_RUN`) measures no time. Fix: in `run_role` (:305), record the `Instant` of each tally line, and assert that the first-to-fifth gap is at least `(PASSES_IN_ONE_RUN - 1) x interval`, minus a small slack. A loop that ignores its interval hammers a networked metadata store, so this is a property the brief asks for, not a matter of taste.; **The doc overstates the time bound on the default backend.** The sentences appended to docs/design/architecture/06-runtime-view.md:94 say: "Each of those passes is bounded in time; on a networked store a pass past the bound is abandoned". On redb, no pass has a time bound. The patch's own doc comment on `NO_FLEET_REAPER_PASS_BOUND` (crates/server/src/custodian.rs:146-150) says so. Reword along these lines: "On a networked store each of those passes is bounded in time, and a pass past the bound is abandoned …; on redb, only the per-pass budgets bound it."; **A role that now stays up silently drops `--gc-expired-pending` when there is no `--endpoints`.** crates/server/src/cli.rs:1054 arms `expired_pending`, but the no-endpoints branch (cli.rs:1161-1188) never reads it, and its banner doesn't mention it. `wyrd custodian --gc-expired-pending` now runs for days, never sweeps, and never says so. The base ignored the flag too, but it exited at once. The one-shot already refuses this exact conflict (`reaper_one_shot_conflict`, cli.rs:1039). Low priority: a one-line warning or a refusal on this branch fixes it.; `crates/server/src/custodian.rs:583`: **The whole-pass timeout can repeatedly starve later uploads and retirement.** The reaper saves its position only after finishing the walk (`crates/custodian/src/reaper.rs:776`), and retirement starts only after the reaper returns (`crates/custodian/src/reconciliation.rs:199`). Twenty valid, still-active uploads with 10,000 summaries each require about 800 summary-page reads; at 100 ms per successful read, that already exceeds the new 60-second deadline. Cancellation discards the walk's progress, so the next interval rereads the same prefix and again skips later abandoned uploads and the entire drain. Use the existing backend operation deadlines and work budgets, or preserve progress between bounded slices. Add a delayed-backend regression proving that later uploads and retirement still progress.; `crates/server/tests/multipart_reaper_deployed.rs:641`: **G2a does not verify the configured interval.** The test interrupts after five pass tallies and checks only their count, eventual teardown and successful shutdown. Replacing the interval sleep with a zero-duration sleep would still satisfy every assertion, allowing continuous metadata scans despite `--interval-secs 1`. Check spacing between pass timestamps, or exercise the driver with a controlled timer and assert that another pass cannot start before the configured interval elapses.; `docs/design/architecture/06-runtime-view.md:94`: **The new time-bound claim includes a backend that is not time-bounded.** “Each of those passes is bounded in time” contradicts the explicit redb exception at `crates/server/src/custodian.rs:147`: synchronous redb work never yields to the timeout or shutdown race. Restrict the elapsed-time guarantee to networked backends and describe redb's per-pass work budgets, matching the implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 10 mutants tested in 6m: 1 missed, 6 caught, 3 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 10 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Arm SIGINT handling before destructive work — the new no-fleet path can terminate during its first pass because the shutdown future is first polled afterward; the real-binary probe exits on signal 2 after fencing an upload (`crates/server/src/cli.rs:1145`; `crates/server/src/custodian.rs:715`; `reviewer-signal-probe.log:16`).; T5 Judgment — Cover the first-pass interruption and new dispatch/retry lifecycle — the subprocess test signals only after five passes, while existing seeded tests bypass the new driver, contrary to the rubric for new destructive paths (`crates/server/tests/multipart_reaper_deployed.rs:661`; `crates/dst/tests/custodian.rs:7438`; `AGENTS.md:203`).; **A Ctrl-C during the first pass kills the role by signal, contradicting the new docs. Reproduced.** `shutdown` is a lazy async block (`crates/server/src/cli.rs:1145-1148`), and tokio installs the SIGINT handler only when `ctrl_c()` is first polled. `run_until` skips the `select!` on the first interval (`crates/server/src/custodian.rs:715-720`), so nothing polls `shutdown` until the first pass has finished. Until then SIGINT keeps its default action. Case: seed 200 idle `Open` sessions, run `wyrd custodian --data-dir D --metadata-backend redb --interval-secs 1` (no `--endpoints`), and send SIGINT when the "Running the multipart reaper alone" banner is read: **10/10 runs ended by signal 2**, 0 pass tallies, no "shutting down" line. That breaks the claims at `crates/server/src/custodian.rs:680-681` ("a pass, once started, runs to its end, so a Ctrl-C never drops one mid-commit") and `docs/design/architecture/06-runtime-view.md:94` ("a stop takes effect between two passes, never during one"). In my runs the kill landed before the first commit, so the store was unchanged, and redb commits are atomic. What actually breaks is the exit status (a supervisor sees a signal death, not exit 0) and the false doc claims. G2a only sends SIGINT after the 5th tally (`crates/server/tests/multipart_reaper_deployed.rs:661`), so it never covers this window. Fix: poll `shutdown` once before the first pass (e.g. a `biased` `select!` against `std::future::ready(())`, or register `tokio::signal::unix::signal(SignalKind::interrupt())` before the loop), and add a G2a variant that interrupts on the banner and asserts exit 0 plus "shutting down". This confirms the T4 blocking findings at `cli.rs:1212` / `custodian.rs:715`.; **Register Ctrl-C handling before the first reaper pass.** At `crates/server/src/custodian.rs:715`, the first interval skips polling `shutdown` and starts the new no-fleet reaper pass at `crates/server/src/custodian.rs:732`. The CLI puts `tokio::signal::ctrl_c()` inside that lazy future (`crates/server/src/cli.rs:1145`), so its signal handler is not registered yet. A SIGINT during a slow first pass can terminate the newly added no-endpoints role immediately, interrupting its destructive work instead of finishing the pass and exiting cleanly as promised. Register the listener before starting the pass while keeping shutdown between passes. Add a regression that interrupts the first pass; the current test waits until five passes have emitted (`crates/server/tests/multipart_reaper_deployed.rs:661`), after registration has already happened.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design (shared reaper step, one driver; G2a/G2c red→green held). Fix only: - Shutdown priority: at `custodian.rs:733` a ready shutdown must win over a ready timer (biased select or check-before-pass). Add a driver test with both ready that asserts no further pass. - Fault model: `FlakyMeta::commit` (`custodian_gc.rs:1992`) returns `CommitUnknownResult { may_still_commit: false, .. }` for faulted commits. Assert that both applied and unapplied draws occur, and that seeds still converge. - Scope: limit the `--interval-secs 0` refusal (`cli.rs:1058-1066`) to the no-endpoints shape. The fleet and empty-fleet shapes stay as on base (panic message and exit 101 unchanged). Update `a_zero_interval_is_refused_wherever_the_role_loops`. - Usage text: say that no-endpoints on redb holds the data-dir lock while it runs. - Records: G2b is a regression guard only (no red on base). Don't claim the degraded-fleet defect as fixed in build-notes. Decided at sign-off (do not reopen): one pass per empty-fleet invocation with restart-driven convergence; store-creating open; no DST rows here. No new features.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design (shared reaper step, one driver; G2a/G2c red→green held). Fix only:
  - Shutdown priority: at `custodian.rs:733` a ready shutdown must win over a ready timer (biased select or check-before-pass). Add a driver test with both ready that asserts no further pass.
  - Fault model: `FlakyMeta::commit` (`custodian_gc.rs:1992`) returns `CommitUnknownResult { may_still_commit: false, .. }` for faulted commits. Assert that both applied and unapplied draws occur, and that seeds still converge.
  - Scope: limit the `--interval-secs 0` refusal (`cli.rs:1058-1066`) to the no-endpoints shape. The fleet and empty-fleet shapes stay as on base (panic message and exit 101 unchanged). Update `a_zero_interval_is_refused_wherever_the_role_loops`.
  - Usage text: say that no-endpoints on redb holds the data-dir lock while it runs.
  - Records: G2b is a regression guard only (no red on base). Don't claim the degraded-fleet defect as fixed in build-notes.
  Decided at sign-off (do not reopen): one pass per empty-fleet invocation with restart-driven convergence; store-creating open; no DST rows here.
  No new features.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
