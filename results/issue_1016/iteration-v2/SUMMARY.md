# Result — issue 1016 / multipart-reaper-deployed-shapes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 625.1, the reaper runs from the one-shot and from the fleet loop's
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
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper_deployed.rs`,
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 10 mutants tested in 6m: 1 missed, 6 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 10 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.88s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #1016: make the multipart reaper run on every ordinary custodian startup shape; the startup regressions pass, but timeout-induced starvation requires implementation iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Approve the corrected acceptance contract — G2b already passes on the supplied base, and G2c completes teardown across restarts rather than one invocation (reviewer-g2b-base.log:6; crates/server/tests/multipart_reaper_deployed.rs:673; brief.md:45). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the fix leaves four executable tests: G2a, G2c and usage fail; restore refusal passes, and both binding startup failures include unchanged-store assertions (reviewer-red.log:400; crates/server/tests/multipart_reaper_deployed.rs:623; crates/server/tests/multipart_reaper_deployed.rs:705). |
| C3 Change | PASS | The five changed files respect the slice boundary, retain the empty-fleet failure and restore exclusion, and leave GC's whole-fleet condition intact (crates/server/src/cli.rs:1273; crates/server/src/custodian.rs:858; patch.diff:1). |
| C4 Verification (red→green) | PASS | Restoring the patch gives 4/4 subprocess tests and 13/13 custodian tests; independent formatting, spelling, docs lint/render and FDB type-check pass; full CI is supported by its frozen log (reviewer-green-final.log:20; reviewer-green-final.log:30; reviewer-fdb-check-final.log:1; gate-logs/C4-ci.log:5097). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve progress across pass deadlines — three timeouts on successful delayed reads leave an eligible upload Open, while an uncancelled pass reaches it (crates/server/src/custodian.rs:583; crates/custodian/src/reaper.rs:777; reviewer-timeout-probe.log:4). |
| T1 Structure | PASS | Backend composition stays in server, the shared reaper step uses MetadataStore, default policy and one lifecycle clock, and no new global state or dependency is introduced (crates/server/src/custodian.rs:493; crates/server/src/custodian.rs:514; crates/server/src/cli.rs:1892). |
| T2 Shape | FAIL | Operators are promised a time bound for every no-endpoints pass although synchronous redb cannot yield to it; qualify the living architecture description (docs/design/architecture/06-runtime-view.md:94; crates/server/src/custodian.rs:145). |
| T3 Runtime | FAIL | A responsive but slow backend can repeatedly consume the whole-pass deadline before later sessions or retirement get a turn, defeating the intended cleanup service (crates/server/src/custodian.rs:583; crates/custodian/src/reconciliation.rs:199; reviewer-timeout-probe.log:1). |
| T4 Contribution | FAIL | Affected-path prior art is checked, but retained review findings remain unresolved; the contribution-artifact audit is N/A until its mandatory publish rerun (reviewer-prior-art.json:1; gate-logs/T4-batch-review.log:10; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add seeded cancellation/recovery coverage — the new timeout/shutdown path drops destructive work, while synchronous-redb tests and existing awaited unknown-outcome cases do not exercise that drop (crates/server/src/custodian.rs:587; crates/server/tests/multipart_reaper_deployed.rs:204; crates/dst/tests/custodian.rs:8052). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide deployment fitness — real FDB/TiKV startup and cancellation were not exercised, so runtime evidence rests on redb and a delayed trait fixture; feature compilation does not establish production recovery behavior (crates/server/src/cli.rs:1901; crates/server/src/cli.rs:1908; reviewer-timeout-probe.rs:14; gate-logs/host-tikv.log:212). |

Source citations above resolve under `$PDCA_TARGET` (`target/`); brief, patch and evidence citations resolve in this review directory. The target contains the prerequisite reaper and the full patch: reverse-application checking succeeds. Its synthetic base is `75bed4d`, labelled `pre-fix base 6e44de924cc8a451a71ce7f7ed63aca76c67f509`. Both temporary stashes were restored; no judged source was edited.

**Three retained findings explain the implementation verdict.**

1. **[P2] Whole-pass cancellation can repeatedly discard scan progress.** The new timeout at `crates/server/src/custodian.rs:583` drops the entire reaper/retirement future after 60 seconds. The reaper saves its listing position only after its session walk (`crates/custodian/src/reaper.rs:765`); retirement runs only after that reaper returns (`crates/custodian/src/reconciliation.rs:199`). A prefix of active sessions can therefore consume every pass, leaving a later abandoned session unvisited even though individual requests succeed.

   I compiled `reviewer-timeout-probe.rs` against the patched libraries without changing them. It seeds four fresh Open sessions followed by one idle Open session, uses the real `CustodianService::run_reaper_until`, and makes slot/summary reads complete successfully after ten simulated seconds each. At 185 simulated seconds there have been three timeout messages, four session-listing starts and 18 completed delayed reads; the victim remains Open and the cursor is absent. One uncancelled `reap_abandoned_uploads_once` over the same store moves it to Aborting. The complete result is `reviewer-timeout-probe.log:1`; the built probe is runnable with `./reviewer-timeout-probe` from this directory. This establishes repeated starvation under slow progress, not a claim of data loss. Use a deadline design that preserves progress while still bounding external waits, and retain this case as a regression.

2. **[P2] Cancellation of an in-flight destructive pass lacks the required seeded regression.** Both timeout and shutdown can drop the future at `crates/server/src/custodian.rs:587`; the empty-fleet path also cancels through its timeout (`crates/server/src/cli.rs:1274`). The new subprocess tests explicitly use redb, whose calls execute synchronously (`crates/metadata-redb/src/lib.rs:200`). Existing reaper DST coverage models ambiguous results, including late landing, but awaits the struck pass to completion (`crates/dst/tests/custodian.rs:8052`). It does not cancel that future during commit. Exercise shutdown/deadline cancellation with both a commit that lands and one that does not, resume the pass, and assert fragment evidence and exactly-once admission release. This is a test-fidelity finding under the standing rubric, not an assertion that cancellation currently corrupts data.

3. **[P3] The architecture description overstates the deadline.** `docs/design/architecture/06-runtime-view.md:94` says each no-endpoints pass is bounded in time. The implementation correctly documents that synchronous redb work cannot be interrupted by this timer (`crates/server/src/custodian.rs:145`); its local open is synchronous too (`crates/server/src/cli.rs:2307`). Carry that qualification into the living doc. The earlier demand for a hard redb timeout is not repeated: this report asks for an accurate statement of the implemented bound.

**The positive evidence establishes the ordinary startup paths, with two acceptance qualifications.**

- Independent red: `cargo test --offline -p wyrd-server --test multipart_reaper_deployed`, with production changes stashed and the new test retained, executed four tests and failed three (`reviewer-red.log:405`). Independent green after restoration: the same target plus `--test custodian_gc` passed all 17 tests (`reviewer-green-final.log:20`, `:30`). Green also ran with `WYRD_METADATA_BACKEND=reviewer-invalid`; explicit redb selection and environment removal isolate the destructive subprocesses (`crates/server/tests/multipart_reaper_deployed.rs:198`).
- G2a now proves multiple passes in one process and clean SIGINT shutdown (`crates/server/tests/multipart_reaper_deployed.rs:620`). G2c checks exactly one pass, failure status and the existing diagnostic on every invocation (`:674`), then verifies teardown across restarts. The restore-mode regression verifies byte-identical metadata and zero reaper passes (`:731`). These discharge the corresponding previous-attempt findings.
- G2b is additional coverage of behavior already present on this base. I independently retained the new G2b test while stashing production changes: it passed, one test executed (`reviewer-g2b-base.log:6`). The base's reaper call was already before GC's whole-fleet branch, as the removed code in `patch.diff:433` also shows. Correct the brief's claim that this leg is red pre-fix; do not manufacture a regression or blame target staleness.
- G2c's literal single-invocation teardown criterion conflicts with the required exactly-one-pass rule: this fixture needs multiple passes. The test now explicitly describes supervisor restarts (`crates/server/tests/multipart_reaper_deployed.rs:26`). Human acceptance should settle that contract rather than silently treating multiple invocations as one.

**Gate evidence is narrower than an unconditional all-green claim.**

- **PASS, independently rerun:** `cargo fmt --all -- --check`, `typos`, `python3 docs/publishing/tools/lint_docs.py`, and `python3 docs/publishing/tools/render_site.py --check --out ../pdca-reviewer-1016-docs`; rendering produced 101 pages and a successful link audit. Thus both named external dependencies, spelling and docs rendering, were exercised. `cargo check --offline -p wyrd-server --features fdb` also passed (`reviewer-fdb-check-final.log:1`); this is a type-check, not a linked executable or live-cluster test.
- **PASS, frozen full-run evidence:** `gate-logs/C4-ci.log` records the full workspace checks and seeded DST run, ending in success at line 5097; the new degraded-fleet case appears at line 3043. `gate-logs/host-tikv.log:2` names the TiKV and server `tikv,etcd` clippy commands, whose successful completion is recorded at line 212. Instance-root wrappers were not available here; their absence is expected. These full gates were adjudicated from their captured output, not represented as independently rerun in full.
- **Not measured, target/base caveat:** diff coverage failed because the bundle did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied disposable target does apply and compile. This is a stacked-base/coverage limitation, not a C4 patch defect.
- **FAIL recorded, limited interpretation:** mutation testing found one missed, six caught and three unviable mutants (`gate-logs/C5-mutants.log:13`). The survivor replaces the feature-gated network-store open helper. The log does not establish a runnable defective networked build: default tests exclude that helper, and feature clippy is compilation evidence only. It identifies a coverage blind spot, not another proven functional bug.
- **FAIL, reviewed rather than copied:** the batch log lists ten findings (`gate-logs/T4-batch-review.log:10`). I retain starvation, merge the two cancellation-test entries into finding 2, and retain the documentation mismatch. I do not elevate the duplicated interval-spacing concern to a separate finding: the actual implementation sleeps on the supplied interval (`crates/server/src/custodian.rs:612`), while the strengthened one-lifetime test covers the prior startup-only defect. The missing-store and ignored-GC-flag claims impose one-shot restrictions on an ordinary role without a corresponding requirement: ordinary backend composition creates redb as the fleet path does, and the no-endpoints diagnostic explicitly announces reaper-only work (`crates/server/src/cli.rs:1178`, `:1894`, `:1936`). Likewise, zero interval was already accepted by the shared CLI parser (`crates/server/src/cli.rs:994`); this patch does not establish a new nonzero-only contract. These are not independently retained defects.
- **N/A, deferred by design:** contribution artifacts are drafted after Check; `gate-logs/T4-contribution.log:10` explicitly assigns their substantive audit to publish. No human clearance is requested for that deferred row.

**Affected-path prior art is checked, including rejected work.** GitHub main-history queries returned 55 commits for `cli.rs`, 9 for `custodian.rs`, 8 for `custodian_gc.rs`, none for the new test path, and 26 for the runtime-view doc. Closed-PR searches also ran by each affected path. I inspected the changed-file lists of all 24 returned closed/unmerged PRs: only [PR #647](https://github.com/getwyrd/wyrd/pull/647) overlaps an affected file, and its exact runtime-view patch concerns segmented publication/read semantics, not reaper startup. Relevant merged startup predecessors include [PR #450](https://github.com/getwyrd/wyrd/pull/450), [PR #461](https://github.com/getwyrd/wyrd/pull/461) and [PR #559](https://github.com/getwyrd/wyrd/pull/559). Query URLs, commit identifiers and file lists are preserved in `reviewer-prior-art.json:1`; no competing reaper-startup solution was found. The previous local-history limitation is therefore discharged.

The capability-probe smell test does not fire: the restore-mode exclusion is a command boundary, not a probe hiding an eager capability dependency. Existing tracked deferrals were left settled. No new dependency, persisted field, parser, workflow, unsafe crate root or DCO finding is introduced by this review. `INTEGRATION.md` was absent from the supplied target, so no additional project-specific human-only list was available beyond the supplied protocol. Real networked-store recovery remains the explicit validation limitation above; Tier-1/Tier-2 follow-up should focus on cancellation and restart behavior after finding 1 is corrected.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/custodian.rs:583`: **The whole-pass timeout can repeatedly starve later uploads and retirement.** The reaper saves its position only after finishing the walk (`crates/custodian/src/reaper.rs:776`), and retirement starts only after the reaper returns (`crates/custodian/src/reconciliation.rs:199`). Twenty valid, still-active uploads with 10,000 summaries each require about 800 summary-page reads; at 100 ms per successful read, that already exceeds the new 60-second deadline. Cancellation discards the walk's progress, so the next interval rereads the same prefix and again skips later abandoned uploads and the entire drain. Use the existing backend operation deadlines and work budgets, or preserve progress between bounded slices. Add a delayed-backend regression proving that later uploads and retirement still progress.

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_reaper_deployed.rs:641`: **G2a does not verify the configured interval.** The test interrupts after five pass tallies and checks only their count, eventual teardown and successful shutdown. Replacing the interval sleep with a zero-duration sleep would still satisfy every assertion, allowing continuous metadata scans despite `--interval-secs 1`. Check spacing between pass timestamps, or exercise the driver with a controlled timer and assert that another pass cannot start before the configured interval elapses.

- NEEDS-HUMAN [impl] — `docs/design/architecture/06-runtime-view.md:94`: **The new time-bound claim includes a backend that is not time-bounded.** “Each of those passes is bounded in time” contradicts the explicit redb exception at `crates/server/src/custodian.rs:147`: synchronous redb work never yields to the timeout or shutdown race. Restrict the elapsed-time guarantee to networked backends and describe redb's per-pass work budgets, matching the implementation.

Reviewed the target source and frozen gate evidence; no tests were rerun. No additional reuse or simplification findings.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Approve the corrected acceptance contract — G2b already passes on the supplied base, and G2c completes teardown across restarts rather than one invocation (reviewer-g2b-base.log:6; crates/server/tests/multipart_reaper_deployed.rs:673; brief.md:45).
- [ ] C5 Causal adequacy — Preserve progress across pass deadlines — three timeouts on successful delayed reads leave an eligible upload Open, while an uncancelled pass reaches it (crates/server/src/custodian.rs:583; crates/custodian/src/reaper.rs:777; reviewer-timeout-probe.log:4).
- [ ] T5 Judgment — Add seeded cancellation/recovery coverage — the new timeout/shutdown path drops destructive work, while synchronous-redb tests and existing awaited unknown-outcome cases do not exercise that drop (crates/server/src/custodian.rs:587; crates/server/tests/multipart_reaper_deployed.rs:204; crates/dst/tests/custodian.rs:8052).
- [ ] Validation — fitness-to-purpose — Decide deployment fitness — real FDB/TiKV startup and cancellation were not exercised, so runtime evidence rests on redb and a delayed trait fixture; feature compilation does not establish production recovery behavior (crates/server/src/cli.rs:1901; crates/server/src/cli.rs:1908; reviewer-timeout-probe.rs:14; gate-logs/host-tikv.log:212).
- [ ] **G2b has no red leg, because the fleet-shape defect in the brief does not exist on this base.** I ran `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (crates/server/tests/custodian_gc.rs) on base `dbb41ef` with only the test added, and it printed `ok`. On the base, the reaper already rides the reconstruction pass (`git show HEAD:crates/server/src/custodian.rs`: `Some(&reaper)` at :680), and that pass sits outside GC's gate (`if fleet.len() == operator_fleet_size` at :749). The brief misreads the base twice: its Defect line ("That call sits behind GC's whole-fleet gate, custodian.rs:680-713") and its Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). So the fleet-loop edit at crates/server/src/custodian.rs:791 is a refactor that changes no behaviour. The PR must not claim it fixes "on a partially degraded fleet it reaps nothing while GC is deferred". G2b does catch negation (11): on the base I gated `Some(&reaper)` behind `fleet.len() == operator_fleet_size`, and G2b failed at custodian_gc.rs:1809. Keep G2b as a regression guard, and strike the fleet-shape defect claim from the summary and the PR text.
- [ ] **G2a still does not prove "every interval": it only counts passes.** The mutant: change crates/server/src/custodian.rs:612 from `tokio::time::sleep(interval)` to `tokio::time::sleep(interval.saturating_sub(interval))`. The no-fleet loop then runs passes back to back and ignores `--interval-secs`. `no_endpoints_role_reaps_every_interval_in_one_lifetime` **passes**, and the whole test takes 0.13 s with `--interval-secs 1`. The check at crates/server/tests/multipart_reaper_deployed.rs:641 (`run.passes >= PASSES_IN_ONE_RUN`) measures no time. Fix: in `run_role` (:305), record the `Instant` of each tally line, and assert that the first-to-fifth gap is at least `(PASSES_IN_ONE_RUN - 1) x interval`, minus a small slack. A loop that ignores its interval hammers a networked metadata store, so this is a property the brief asks for, not a matter of taste.
- [ ] **Two loops instead of one driver, and only the new loop can drop a pass mid-commit.** The brief's scope says "One reaper interval driver that both deployed shapes run". The patch shares the step (`reaper_pass`) but not the loop:
- [ ] **`wyrd custodian` with no flags now holds the default store's lock until Ctrl-C.** I reproduced this with the patched binary in an empty dir. I started `wyrd custodian --interval-secs 5` (default `--data-dir wyrd-data`), then ran `wyrd put payload.txt --key k1`. It failed with `wyrd: Database already open. Cannot acquire lock.` and exit 1. After Ctrl-C, the same put succeeded. The path is crates/server/src/cli.rs:1184 → `run_reaper_over_backend` → `open_local_meta_redb` (cli.rs:2307), and the store stays open for the life of the loop. On the base, this shape exited at once and never opened the store. The fleet shape already held the lock on the base, and a reaper on redb has to open the store, so this follows from the brief. But it turns a harmless no-op into something that blocks `wyrd put` and `wyrd s3` on the same dev data dir. Decide whether to accept it as dev-only redb behaviour and say so in the usage text.
- [ ] **The doc overstates the time bound on the default backend.** The sentences appended to docs/design/architecture/06-runtime-view.md:94 say: "Each of those passes is bounded in time; on a networked store a pass past the bound is abandoned". On redb, no pass has a time bound. The patch's own doc comment on `NO_FLEET_REAPER_PASS_BOUND` (crates/server/src/custodian.rs:146-150) says so. Reword along these lines: "On a networked store each of those passes is bounded in time, and a pass past the bound is abandoned …; on redb, only the per-pass budgets bound it."
- [ ] **A role that now stays up silently drops `--gc-expired-pending` when there is no `--endpoints`.** crates/server/src/cli.rs:1054 arms `expired_pending`, but the no-endpoints branch (cli.rs:1161-1188) never reads it, and its banner doesn't mention it. `wyrd custodian --gc-expired-pending` now runs for days, never sweeps, and never says so. The base ignored the flag too, but it exited at once. The one-shot already refuses this exact conflict (`reaper_one_shot_conflict`, cli.rs:1039). Low priority: a one-line warning or a refusal on this branch fixes it.
- [ ] `crates/server/src/custodian.rs:583`: **The whole-pass timeout can repeatedly starve later uploads and retirement.** The reaper saves its position only after finishing the walk (`crates/custodian/src/reaper.rs:776`), and retirement starts only after the reaper returns (`crates/custodian/src/reconciliation.rs:199`). Twenty valid, still-active uploads with 10,000 summaries each require about 800 summary-page reads; at 100 ms per successful read, that already exceeds the new 60-second deadline. Cancellation discards the walk's progress, so the next interval rereads the same prefix and again skips later abandoned uploads and the entire drain. Use the existing backend operation deadlines and work budgets, or preserve progress between bounded slices. Add a delayed-backend regression proving that later uploads and retirement still progress.
- [ ] `crates/server/tests/multipart_reaper_deployed.rs:641`: **G2a does not verify the configured interval.** The test interrupts after five pass tallies and checks only their count, eventual teardown and successful shutdown. Replacing the interval sleep with a zero-duration sleep would still satisfy every assertion, allowing continuous metadata scans despite `--interval-secs 1`. Check spacing between pass timestamps, or exercise the driver with a controlled timer and assert that another pass cannot start before the configured interval elapses.
- [ ] `docs/design/architecture/06-runtime-view.md:94`: **The new time-bound claim includes a backend that is not time-bounded.** “Each of those passes is bounded in time” contradicts the explicit redb exception at `crates/server/src/custodian.rs:147`: synchronous redb work never yields to the timeout or shutdown race. Restrict the elapsed-time guarantee to networked backends and describe redb's per-pass work budgets, matching the implementation.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- [ ] C1 Spec — Decide whether G2c permits convergence across supervisor restarts — the brief requires the full leg-A transition after one invocation and exactly one pass, but an independently exercised invocation leaves `Aborting` and count 1 (brief.md:38; brief.md:96; reviewer-probe.log:3).
- [ ] **G2b was never red, because the defect the brief describes for the fleet shape does not exist on this base.** In the base, `HEAD:crates/server/src/custodian.rs:680` already hands `Some(&reaper)` to the reconstruction pass, which runs every interval. The GC gate is further down, at `HEAD:…/custodian.rs:749`. I ran the new test `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1748`) with `cli.rs` and `custodian.rs` reverted to HEAD: **it passes**. So two lines in the brief are wrong: the Defect line ("That call sits behind GC's whole-fleet gate… `custodian.rs:680-713`") and the Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). The fleet-path edit at `custodian.rs:755` is a refactor that changes no behavior. G2b only guards against a future regression (negation 11), and the green ci row is not red→green evidence for it. This isn't a defect in the patch, but one of the three claimed defects was not real, and the human should know that before sign-off.
- [ ] **The no-endpoints role now creates a store if none exists, then reaps it forever and reports nothing wrong.** Before this patch the no-endpoints role opened nothing. Now `run_reaper_over_backend` uses the store-creating open (`cli.rs:1875-1880`, `:2251-2256`). With a mistyped `--data-dir`, or the shipped systemd unit run without `--metadata-backend fdb` (`deploy/dist/systemd/wyrd-custodian.service:28` pins `--data-dir ${STATE_DIRECTORY}`), the role creates an empty redb store and prints "Running the multipart reaper alone every Ns" for as long as it runs. That is the false all-clear the one-shot was built to refuse (`cli.rs:2258-2270`; the rubric's "never silent success"). The fleet role opens its store the same way, and the doc comment calls this deliberate, so it's a design decision, not a slip.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Preserve progress across pass deadlines — three timeouts on successful delayed reads leave an eligible upload Open, while an uncancelled pass reaches it (crates/server/src/custodian.rs:583; crates/custodian/src/reaper.rs:777; reviewer-timeout-probe.log:4).; T5 Judgment — Add seeded cancellation/recovery coverage — the new timeout/shutdown path drops destructive work, while synchronous-redb tests and existing awaited unknown-outcome cases do not exercise that drop (crates/server/src/custodian.rs:587; crates/server/tests/multipart_reaper_deployed.rs:204; crates/dst/tests/custodian.rs:8052).; **G2a still does not prove "every interval": it only counts passes.** The mutant: change crates/server/src/custodian.rs:612 from `tokio::time::sleep(interval)` to `tokio::time::sleep(interval.saturating_sub(interval))`. The no-fleet loop then runs passes back to back and ignores `--interval-secs`. `no_endpoints_role_reaps_every_interval_in_one_lifetime` **passes**, and the whole test takes 0.13 s with `--interval-secs 1`. The check at crates/server/tests/multipart_reaper_deployed.rs:641 (`run.passes >= PASSES_IN_ONE_RUN`) measures no time. Fix: in `run_role` (:305), record the `Instant` of each tally line, and assert that the first-to-fifth gap is at least `(PASSES_IN_ONE_RUN - 1) x interval`, minus a small slack. A loop that ignores its interval hammers a networked metadata store, so this is a property the brief asks for, not a matter of taste.; **The doc overstates the time bound on the default backend.** The sentences appended to docs/design/architecture/06-runtime-view.md:94 say: "Each of those passes is bounded in time; on a networked store a pass past the bound is abandoned". On redb, no pass has a time bound. The patch's own doc comment on `NO_FLEET_REAPER_PASS_BOUND` (crates/server/src/custodian.rs:146-150) says so. Reword along these lines: "On a networked store each of those passes is bounded in time, and a pass past the bound is abandoned …; on redb, only the per-pass budgets bound it."; **A role that now stays up silently drops `--gc-expired-pending` when there is no `--endpoints`.** crates/server/src/cli.rs:1054 arms `expired_pending`, but the no-endpoints branch (cli.rs:1161-1188) never reads it, and its banner doesn't mention it. `wyrd custodian --gc-expired-pending` now runs for days, never sweeps, and never says so. The base ignored the flag too, but it exited at once. The one-shot already refuses this exact conflict (`reaper_one_shot_conflict`, cli.rs:1039). Low priority: a one-line warning or a refusal on this branch fixes it.; `crates/server/src/custodian.rs:583`: **The whole-pass timeout can repeatedly starve later uploads and retirement.** The reaper saves its position only after finishing the walk (`crates/custodian/src/reaper.rs:776`), and retirement starts only after the reaper returns (`crates/custodian/src/reconciliation.rs:199`). Twenty valid, still-active uploads with 10,000 summaries each require about 800 summary-page reads; at 100 ms per successful read, that already exceeds the new 60-second deadline. Cancellation discards the walk's progress, so the next interval rereads the same prefix and again skips later abandoned uploads and the entire drain. Use the existing backend operation deadlines and work budgets, or preserve progress between bounded slices. Add a delayed-backend regression proving that later uploads and retirement still progress.; `crates/server/tests/multipart_reaper_deployed.rs:641`: **G2a does not verify the configured interval.** The test interrupts after five pass tallies and checks only their count, eventual teardown and successful shutdown. Replacing the interval sleep with a zero-duration sleep would still satisfy every assertion, allowing continuous metadata scans despite `--interval-secs 1`. Check spacing between pass timestamps, or exercise the driver with a controlled timer and assert that another pass cannot start before the configured interval elapses.; `docs/design/architecture/06-runtime-view.md:94`: **The new time-bound claim includes a backend that is not time-bounded.** “Each of those passes is bounded in time” contradicts the explicit redb exception at `crates/server/src/custodian.rs:147`: synchronous redb work never yields to the timeout or shutdown race. Restrict the elapsed-time guarantee to networked backends and describe redb's per-pass work budgets, matching the implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
