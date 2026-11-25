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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 19 mutants tested in 12m: 14 caught, 5 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.65s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #1016 patch makes the ordinary custodian reap without a usable fleet and passes independent red→green checks; one low-priority test-isolation defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The latest sign-off resolves the original G2c convergence ambiguity and treats G2b as a regression guard; those decisions govern this review (brief.md:153, brief.md:161). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the two production files while retaining the new test reproduced 7 failures out of 9 executed tests, including the unchanged-store failures for both missing startup paths (reviewer-red.log:456; crates/server/tests/multipart_reaper_deployed.rs:831). |
| C3 Change | PASS | All five changed paths belong to this slice; the restore refusal remains metadata-preserving and the GC whole-fleet condition remains intact (crates/server/src/cli.rs:1311; crates/server/src/custodian.rs:914). |
| C4 Verification (red→green) | PASS | Restoring the patch yielded 9/9 subprocess and 17/17 driver tests; both network-backend feature checks compile; frozen full CI passed, with the independent full-CI checkout limitation recorded below (reviewer-green.log:26, reviewer-green.log:41; reviewer-feature-checks.log:107; gate-logs/C4-ci.log:5099). |
| C5 Causal adequacy | PASS | Missing dispatch is corrected at the ordinary-role entry points; interval spacing, one pass per empty-fleet invocation and restore isolation are observed, without a capability probe masking a load-time cause (crates/server/src/cli.rs:1220; crates/server/src/cli.rs:1311; crates/server/tests/multipart_reaper_deployed.rs:892). |
| T1 Structure | PASS | Backend selection stays in server wiring and the shared driver consumes the existing MetadataStore seam; no child-3 crate or new dependency is introduced (crates/server/src/cli.rs:1951; crates/server/src/custodian.rs:694). |
| T2 Shape | PASS | One interval driver and one reaper step serve both looping shapes; default policy and the existing correctness clock remain shared (crates/server/src/custodian.rs:514; crates/server/src/custodian.rs:723). |
| T3 Runtime | PASS | The deployed redb paths demonstrate sustained reaping, successful first-pass interruption, unchanged empty-fleet failure and shutdown priority; existing GC-deferral tests also pass (crates/server/tests/multipart_reaper_deployed.rs:934; crates/server/tests/custodian_gc.rs:2254; reviewer-green.log:26). |
| T4 Contribution | FAIL | Review completeness still has the confirmed logging-environment test defect; affected-path prior art is complete, and the contribution-artifact audit is deferred to publish, not failed (crates/server/tests/multipart_reaper_deployed.rs:1288; reviewer-prior-art.log:1; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN [impl] | Isolate the usage subprocess from caller logging configuration — an inherited invalid RUST_LOG prevents usage from printing and makes this regression fail for an unrelated reason (crates/server/tests/multipart_reaper_deployed.rs:1288; reviewer-log-env.log:7). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm deployment readiness on the intended topology — runtime evidence covers real redb and seeded in-memory faults, while live TiKV/FDB operation and supervisor restart behavior were not exercised (crates/server/tests/multipart_reaper_deployed.rs:279; crates/server/tests/custodian_gc.rs:2063; docs/design/architecture/06-runtime-view.md:94). |

The remaining implementation finding is a false test failure, not a demonstrated reaper defect. Running `RUST_LOG='[' cargo test -p wyrd-server --test multipart_reaper_deployed usage_says_what_the_role_does_with_no_fleet -- --exact` fails because logging initialization refuses the inherited filter before printing usage. The subprocess at `crates/server/tests/multipart_reaper_deployed.rs:1288` bypasses the command helper that pins logging at line 286. Clear `RUST_LOG` for this usage invocation. The independent reproduction is captured in `reviewer-log-env.log:1`.

Verification supports the dispatch change, with two checkout caveats:

- The independent red leg ran against the disposable target's pre-fix production files, then `git stash pop` restored the patch. All 26 affected tests passed afterward. Formatting, clippy with warnings denied, typos, docs lint and rendering/link audit passed (`reviewer-checks.log:1`). Both `fdb,etcd` and `tikv,etcd` server test surfaces type-checked (`reviewer-feature-checks.log:1`, `reviewer-feature-checks.log:108`). The brief's named external tools, typos and docs-renderer, were actually exercised.
- The independent full CI run completed with two failed checks, both caused by a missing `.github/advisory-db` gitlink in this synthetic checkout: the index test fails and the dependency audit cannot resolve its pin (`reviewer-ci.log:3640`, `reviewer-ci.log:4295`; `xtask/tests/advisory_db.rs:65`). That is a checkout limitation, not a patch defect. The frozen run fetched the pinned advisory database, ran the dependency checks and completed successfully (`gate-logs/C4-ci.log:4442`, `gate-logs/C4-ci.log:5099`).
- Diff coverage was not measured because the gate's `origin/main` base could not accept this stacked patch (`gate-logs/C4-diff-cov.log:10`). The supplied disposable target does build and pass its regressions, and reverse-applying the patch passes `--check`; the coverage failure does not establish a compile or applicability defect here. The frozen mutation run reports 14 caught and 5 unviable mutants, with no survivors (`gate-logs/C5-mutants.log:13`); this is log evidence, not an independent mutation rerun.

The frozen batch review contains twelve entries, including duplicates; they are not twelve independently confirmed defects (`gate-logs/T4-batch-review.log:10`). Their dispositions are:

- **Confirmed:** the usage test's inherited `RUST_LOG` failure, reported above.
- **Already covered:** the three GC-deferral test objections overlook the existing startup-partial regression, which checks surviving fragments and pending evidence and then reclamation after fleet recovery (`crates/server/tests/custodian_gc.rs:1017`). It passed independently. G2b remains a regression guard, as sign-off requires.
- **Settled by sign-off:** delayed-unknown-commit and non-DST scheduling objections do not reopen the accepted fault-model scope. The model now explicitly uses `may_still_commit: false` and asserts both applied and unapplied outcomes (`crates/server/tests/custodian_gc.rs:2041`, `crates/server/tests/custodian_gc.rs:2120`; brief.md:157, brief.md:162).
- **Not established as new defects:** swallowed Ctrl-C registration errors already exist on the base; the telemetry initialization concern did not reproduce in 30 complete runs with 64 test threads (`reviewer-telemetry-race.log:1`); localhost port 1 is the brief's explicit refused-endpoint example. The Unix `kill` dependency limits the G2a test's portability, but the supplied deployment/CI contract does not establish a Windows test requirement. These observations do not establish a failure of the reviewed Linux paths.
- **Not a demonstrated shutdown regression:** the first-pass test does not independently assert drain completion, but checks an interrupt delivered during fencing, all fences, exactly one tally and exit 0; the single-lifetime G2a test separately verifies the full teardown. The current driver awaits the entire reaper/retirement step before observing shutdown (`crates/server/src/custodian.rs:754`; crates/custodian/src/reconciliation.rs:199). A tally alone was not treated as proof of the full transition.

Prior art was checked by all five affected paths against merged GitHub history and the changed-file lists of all 24 closed, unmerged PRs (`reviewer-prior-art.log:1`). Only [closed PR #647](https://github.com/getwyrd/wyrd/pull/647) overlaps an affected path, the architecture document; its segmented-map work does not duplicate this startup wiring. The synthetic local history alone was not used to assert that no prior art exists.

`T4-contribution` is **N/A** at Check: `pr-description.md` is intentionally not drafted yet, and the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). The settled empty-fleet restart convergence, store-open decision and absence of new DST rows are not reopened. Tier-1/Tier-2 custodian campaigns remain follow-up operational evidence, not substitute failures for the passing dispatch regressions.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1016 (625.2: the reaper on every custodian startup shape)

I rebuilt the patched tree in scratch and ran both test binaries. All 26 tests pass: 9 in `multipart_reaper_deployed`, 17 in `custodian_gc`. The subprocess suite also passed 10 of 10 repeat runs. Then I tried mutants against the fix. I found one real gap in the tests and no defect in the production code.

## Refutation attempts that landed

- NEEDS-HUMAN [impl] — **No test can see the "a pass, once started, runs to its end" rule, because no test store ever suspends mid-pass.** That rule is claimed at `crates/server/src/custodian.rs:680-682`, at `docs/design/architecture/06-runtime-view.md:94` ("a stop takes effect between two passes, never during one … the pass in hand finishes") and in the name of `no_endpoints_role_finishes_its_first_pass_when_stopped_during_it` (`crates/server/tests/multipart_reaper_deployed.rs:934`, `:990` "it ran to its end"). But redb, `MemMeta` and `FlakyMeta` all answer inside one poll, and the custodian crate has no `yield_now`, sleep or timeout inside a pass. So a whole pass always finishes in a single poll in every test. **The mutant:** at `custodian.rs:755-757`, race the no-fleet pass against the stop: pin the pass, then `select! { biased; r = &mut pass => r, _ = &mut shutdown => return Ok(()) }`. **All 26 tests still pass.** On TiKV or FoundationDB, where each store call really suspends, that mutant drops the pass at its next await when Ctrl-C arrives. That is exactly what the docs promise cannot happen. **The probe:** a store wrapper that calls `tokio::task::yield_now().await` before passing each call to `MemMeta`, seeded with leg A, and a stop that becomes ready after 2 store calls. On the mutant the probe goes red: the loop returns after 2 calls and the session is still `Open@0`. On the real fix it passes: the pass fences the session before the stop takes effect. **Fix:** add that probe to `custodian_gc.rs` (about 40 lines, no new dependencies), and do the same for the fleet loop, which shares the driver. The subprocess first-pass test does catch what it was written for: removing the pre-poll at `custodian.rs:714-717` turned it red 3 of 3 times (exit by signal 2), and three in-process tests caught it too. What that test proves is "SIGINT is caught". It does not prove "the pass is not cut short".

## Claims I checked and could not refute

- **Red→green evidence (C4-verify).** The gate's red leg is real. On the base, 7 of the 9 new tests fail, each on the property it names (`gate-logs/C4-verify.log`). The role exits at once with "nothing to reconstruct", and the empty-fleet role panics with 0 passes. One wording nit, not a refutation: the gate says "9 test(s) ran red", but only 7 failed. `empty_fleet_restore_one_shot_fails_with_the_store_untouched` and `empty_fleet_role_creates_no_store` pass on the base by design, because they guard behaviour the base already has.
- **Negation (11), G2b.** Running the fleet reaper only inside GC's whole-fleet branch (`custodian.rs:846`) turns `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`custodian_gc.rs:1866`) red. T4 says three times that G2b never checks GC deferral. That is true of G2b alone, but the gate is not left unguarded: removing GC's gate (`custodian.rs:914`, `|| true`) turns two existing tests red, `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial` and `deployed_role_defers_gc_and_preserves_a_skipped_servers_evidence`.
- **Production entry points.** G2a and G2c run the real `wyrd` binary with the one-shot flag asserted absent from argv (`multipart_reaper_deployed.rs:1575-1586`). The backend is pinned to redb and `WYRD_METADATA_BACKEND` is removed from the child's environment (`:1591-1603`). Neither test re-implements the role.
- **Empty-fleet pass bound.** The comment at `cli.rs:1299-1301` says every store call in the pass, the open included, is bounded on TiKV and FDB. That is true: TiKV has its operation deadline covering `get`/`scan`/`connect` (`crates/metadata-tikv/src/lib.rs:137-269`), and FDB has its per-transaction timeout plus the bounded readiness probe at connect (`crates/metadata-fdb/src/lib.rs:78-89`, `cli.rs:321-335`). So a total outage of the store too still reaches the panic. It does not hang.
- **Shipped deploy shapes.** The systemd unit and both multi-node compose files always pass `--endpoints` (`deploy/dist/env/custodian.env.example:31`, `deploy/small-multi-node/docker-compose.yml:360`). The new no-endpoints loop and the redb lock it holds do not change any shipped topology.
- **T4's deadlock claim** (`custodian_gc.rs:1231`: older tests drop roles without `enable_log_callsites`). Six tests do skip it, through `drive_deployed_loop_operator` (`custodian_gc.rs:507-516`). Even so, 300 back-to-back runs of the `custodian_gc` binary gave 0 hangs and 0 failures. I could not reproduce it.
- **T4 blockers I'd call noise.** The `kill`-on-Windows finding (`multipart_reaper_deployed.rs:831`) cannot fail in this repo's CI, because every workflow runs `ubuntu-latest`. The ctrl-c-registration-failure finding (`cli.rs:1153-1156`) behaves the same in the existing fleet role, and SIGINT registration on a runtime built with `enable_all` (`cli.rs:2717-2720`) cannot fail in practice.
- **C4 diff-cov red.** This is a harness base mismatch ("does not apply on origin/main": the slice is stacked on #1015). It says nothing about coverage. Coverage of the changed lines has not been measured in any of the five rounds.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_reaper_deployed.rs:1288`: The new usage test inherits `RUST_LOG`, unlike the role tests that pin their log level. With a malformed value such as `RUST_LOG='nonsense=;;'`, logging initialization rejects the environment before printing usage (`crates/server/src/cli.rs:472`, `crates/server/src/logging.rs:205`); the expected exit status still matches, but every usage-text assertion fails despite correct usage. Clear `RUST_LOG` on this child command. Low priority; confirmed by source inspection, not a new execution.

No additional introduced production correctness bugs or actionable reuse/simplification/efficiency findings identified. Reviewed the frozen gate evidence; no target files were changed and no gates were rerun. The brief's settled decisions were not reopened.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Isolate the usage subprocess from caller logging configuration — an inherited invalid RUST_LOG prevents usage from printing and makes this regression fail for an unrelated reason (crates/server/tests/multipart_reaper_deployed.rs:1288; reviewer-log-env.log:7).
- [x] Validation — fitness-to-purpose — Confirm deployment readiness on the intended topology — runtime evidence covers real redb and seeded in-memory faults, while live TiKV/FDB operation and supervisor restart behavior were not exercised (crates/server/tests/multipart_reaper_deployed.rs:279; crates/server/tests/custodian_gc.rs:2063; docs/design/architecture/06-runtime-view.md:94).
- [x] **No test can see the "a pass, once started, runs to its end" rule, because no test store ever suspends mid-pass.** That rule is claimed at `crates/server/src/custodian.rs:680-682`, at `docs/design/architecture/06-runtime-view.md:94` ("a stop takes effect between two passes, never during one … the pass in hand finishes") and in the name of `no_endpoints_role_finishes_its_first_pass_when_stopped_during_it` (`crates/server/tests/multipart_reaper_deployed.rs:934`, `:990` "it ran to its end"). But redb, `MemMeta` and `FlakyMeta` all answer inside one poll, and the custodian crate has no `yield_now`, sleep or timeout inside a pass. So a whole pass always finishes in a single poll in every test. **The mutant:** at `custodian.rs:755-757`, race the no-fleet pass against the stop: pin the pass, then `select! { biased; r = &mut pass => r, _ = &mut shutdown => return Ok(()) }`. **All 26 tests still pass.** On TiKV or FoundationDB, where each store call really suspends, that mutant drops the pass at its next await when Ctrl-C arrives. That is exactly what the docs promise cannot happen. **The probe:** a store wrapper that calls `tokio::task::yield_now().await` before passing each call to `MemMeta`, seeded with leg A, and a stop that becomes ready after 2 store calls. On the mutant the probe goes red: the loop returns after 2 calls and the session is still `Open@0`. On the real fix it passes: the pass fences the session before the stop takes effect. **Fix:** add that probe to `custodian_gc.rs` (about 40 lines, no new dependencies), and do the same for the fleet loop, which shares the driver. The subprocess first-pass test does catch what it was written for: removing the pre-poll at `custodian.rs:714-717` turned it red 3 of 3 times (exit by signal 2), and three in-process tests caught it too. What that test proves is "SIGINT is caught". It does not prove "the pass is not cut short".
- [x] `crates/server/tests/multipart_reaper_deployed.rs:1288`: The new usage test inherits `RUST_LOG`, unlike the role tests that pin their log level. With a malformed value such as `RUST_LOG='nonsense=;;'`, logging initialization rejects the environment before printing usage (`crates/server/src/cli.rs:472`, `crates/server/src/logging.rs:205`); the expected exit status still matches, but every usage-text assertion fails despite correct usage. Clear `RUST_LOG` on this child command. Low priority; confirmed by source inspection, not a new execution.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review
- [x] size backstop — this slice is behaving oversized: patch is 141 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] C1 Spec — Decide whether G2c permits convergence across supervisor restarts — the brief requires the full leg-A transition after one invocation and exactly one pass, but an independently exercised invocation leaves `Aborting` and count 1 (brief.md:38; brief.md:96; reviewer-probe.log:3).
- [x] **G2b was never red, because the defect the brief describes for the fleet shape does not exist on this base.** In the base, `HEAD:crates/server/src/custodian.rs:680` already hands `Some(&reaper)` to the reconstruction pass, which runs every interval. The GC gate is further down, at `HEAD:…/custodian.rs:749`. I ran the new test `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1748`) with `cli.rs` and `custodian.rs` reverted to HEAD: **it passes**. So two lines in the brief are wrong: the Defect line ("That call sits behind GC's whole-fleet gate… `custodian.rs:680-713`") and the Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). The fleet-path edit at `custodian.rs:755` is a refactor that changes no behavior. G2b only guards against a future regression (negation 11), and the green ci row is not red→green evidence for it. This isn't a defect in the patch, but one of the three claimed defects was not real, and the human should know that before sign-off.
- [x] **The no-endpoints role now creates a store if none exists, then reaps it forever and reports nothing wrong.** Before this patch the no-endpoints role opened nothing. Now `run_reaper_over_backend` uses the store-creating open (`cli.rs:1875-1880`, `:2251-2256`). With a mistyped `--data-dir`, or the shipped systemd unit run without `--metadata-backend fdb` (`deploy/dist/systemd/wyrd-custodian.service:28` pins `--data-dir ${STATE_DIRECTORY}`), the role creates an empty redb store and prints "Running the multipart reaper alone every Ns" for as long as it runs. That is the false all-clear the one-shot was built to refuse (`cli.rs:2258-2270`; the rubric's "never silent success"). The fleet role opens its store the same way, and the doc comment calls this deliberate, so it's a design decision, not a slip.
- [x] C1 Spec — Approve the corrected acceptance contract — G2b already passes on the supplied base, and G2c completes teardown across restarts rather than one invocation (reviewer-g2b-base.log:6; crates/server/tests/multipart_reaper_deployed.rs:673; brief.md:45).
- [x] **G2b has no red leg, because the fleet-shape defect in the brief does not exist on this base.** I ran `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (crates/server/tests/custodian_gc.rs) on base `dbb41ef` with only the test added, and it printed `ok`. On the base, the reaper already rides the reconstruction pass (`git show HEAD:crates/server/src/custodian.rs`: `Some(&reaper)` at :680), and that pass sits outside GC's gate (`if fleet.len() == operator_fleet_size` at :749). The brief misreads the base twice: its Defect line ("That call sits behind GC's whole-fleet gate, custodian.rs:680-713") and its Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). So the fleet-loop edit at crates/server/src/custodian.rs:791 is a refactor that changes no behaviour. The PR must not claim it fixes "on a partially degraded fleet it reaps nothing while GC is deferred". G2b does catch negation (11): on the base I gated `Some(&reaper)` behind `fleet.len() == operator_fleet_size`, and G2b failed at custodian_gc.rs:1809. Keep G2b as a regression guard, and strike the fleet-shape defect claim from the summary and the PR text.
- [x] **Two loops instead of one driver, and only the new loop can drop a pass mid-commit.** The brief's scope says "One reaper interval driver that both deployed shapes run". The patch shares the step (`reaper_pass`) but not the loop:
- [x] **`wyrd custodian` with no flags now holds the default store's lock until Ctrl-C.** I reproduced this with the patched binary in an empty dir. I started `wyrd custodian --interval-secs 5` (default `--data-dir wyrd-data`), then ran `wyrd put payload.txt --key k1`. It failed with `wyrd: Database already open. Cannot acquire lock.` and exit 1. After Ctrl-C, the same put succeeded. The path is crates/server/src/cli.rs:1184 → `run_reaper_over_backend` → `open_local_meta_redb` (cli.rs:2307), and the store stays open for the life of the loop. On the base, this shape exited at once and never opened the store. The fleet shape already held the lock on the base, and a reaper on redb has to open the store, so this follows from the brief. But it turns a harmless no-op into something that blocks `wyrd put` and `wyrd s3` on the same dev data dir. Decide whether to accept it as dev-only redb behaviour and say so in the usage text.
- [x] C1 Spec — Accept the corrected baseline and G2c interpretation — G2b already passes before this patch, contrary to the brief, and exactly one pass per empty-fleet invocation requires restarts to complete teardown (`reviewer-red.log:270`; `crates/server/tests/multipart_reaper_deployed.rs:714`; `brief.md:54`).
- [x] **G2b has no red leg on this base; the brief's premise for it is false.** On the base, `crates/server/src/custodian.rs` (HEAD) already dispatches the reaper inside the reconstruction pass (`Some(&reaper)` at HEAD `:680`), *ahead of* GC's whole-fleet gate (HEAD `:749`), not behind it. So the brief's Defect ("That call sits behind GC's whole-fleet gate … the reaper runs only on a healthy fleet", brief.md:13-15) and Falsifiability ("G2b reds because the reaper sits behind the whole-fleet gate", brief.md:57) do not hold for this base. **Reproduced:** with production code reverted to HEAD and the patched test file kept, `cargo test -p wyrd-server --test custodian_gc deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` → `1 passed`. The fleet-loop edits (`crates/server/src/custodian.rs:823`, `:890`) are therefore a refactor with no change in behavior, and the doc line "Dispatching the reaper only inside GC's whole-fleet branch turns it red" (`crates/server/tests/custodian_gc.rs:68`) describes a hypothetical mutant, not the base. No gate measured a G2b red: C4-verify ran only `multipart_reaper_deployed` (gate-logs/C4-verify.log). The test also never checks that GC was actually deferred (no "gc pass deferred" assertion anywhere in `custodian_gc.rs`, test at `:1753`), so it would pass with the GC gate removed too. A human should decide whether to record G2b as a regression guard only (and correct the brief/§6 record), or confirm the real 625.1 base differs from this folded one.
- [x] **The new destructive dispatches have no seeded Tier-0 DST coverage.** The rubric's *Test fidelity* class says "a new destructive or concurrent path lands with seeded Tier-0 DST coverage". The no-endpoints loop (`crates/server/src/custodian.rs:722-745`) and the empty-fleet pass (`crates/server/src/cli.rs:1300-1313`) are new paths that fence and delete records, and they are covered only by subprocess tests. The brief forbids editing `crates/dst/` (brief.md:69-71), so the builder cannot close this. A human should decide whether dispatch-only wiring over the DST-covered reaper (625.1) meets the rule, or hand a row to child-3 / a follow-up issue. (Same gap as T4's TEST-GAP at `cli.rs:1299`/`:1300`.)
- [x] **`--interval-secs 0` is now refused for the existing fleet role as well.** `crates/server/src/cli.rs:1058-1066` refuses 0 for every non-one-shot shape, including `--endpoints … --interval-secs 0`, which the base accepted. A deployment that used 0 would now fail at startup and crash-loop with no reconstruction or GC at all. The brief scopes this slice to "where the pass runs" and says it "adds no reaper logic" (brief.md:8); a new startup refusal on the fleet shape goes past that. Low practical risk: the repo's compose files use 15, and usage/docs are updated. A human should decide whether to keep it as a hot-loop guard or limit it to the no-endpoints shape.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Follow-up (#1016): the usage-text test (`multipart_reaper_deployed.rs:1288`) inherits `RUST_LOG`; clear it on that child command.
- Follow-up (#1016): no test can catch a pass cut short by Ctrl-C (test stores never suspend mid-pass); add the adversary's yield-per-call store probe for the no-fleet and fleet loops in `custodian_gc.rs`.
