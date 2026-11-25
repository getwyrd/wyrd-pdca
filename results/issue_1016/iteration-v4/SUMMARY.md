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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): unverifiable — gate exceeded its 7200s timeout

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.00s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

For #1016’s multipart reaper on every custodian startup shape, the startup fix is reproduced, but shutdown ordering and fault-model fidelity need builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The three startup shapes, preserved empty-fleet failure, and unchanged GC gate give concrete acceptance boundaries (brief.md:35; brief.md:91). |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the new tests reproduced both missing-cleanup symptoms: 9 executed, 7 failed, 2 passed, including G2a and G2c (reviewer-red.log:88; reviewer-red.log:120; reviewer-red.log:195). |
| C3 Change | PASS | The patch stays within the five permitted files; reaper policy and the GC whole-fleet condition retain their existing authority (crates/server/src/custodian.rs:724; crates/server/src/custodian.rs:907; brief.md:67). |
| C4 Verification (red→green) | PASS | Restoring the patch passes all 9 subprocess tests and 16 custodian tests, including degraded-fleet cleanup; frozen CI and TiKV compilation also pass, while diff coverage and mutation results remain unavailable (reviewer-restored-green.log:23; reviewer-restored-green.log:38; gate-logs/C4-ci.log:5098; gate-logs/host-tikv.log:211). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make a ready shutdown win at the interval boundary — the real driver began another pass and mutated metadata in 45/100 runs with shutdown and timer both ready (crates/server/src/custodian.rs:733; reviewer-probes.log:11). |
| T1 Structure | PASS | One interval driver and one fenced reaper step cover fleet and record-only operation without changing dependency direction or introducing another correctness clock (crates/server/src/custodian.rs:508; crates/server/src/custodian.rs:558; crates/server/src/custodian.rs:657). |
| T2 Shape | PASS | CLI refusals and the living runtime description agree; formatting, affected-file spelling, docs lint, and the 101-page render/link audit pass (crates/server/src/cli.rs:1046; docs/design/architecture/06-runtime-view.md:94; reviewer-docs.log:2). |
| T3 Runtime | FAIL | A stop can permit a further destructive interval when its timer is also ready, extending shutdown beyond the current pass; this is the same confirmed defect routed through C5 (crates/server/src/custodian.rs:681; crates/server/src/custodian.rs:733; reviewer-probes.log:12). |
| T4 Contribution | FAIL | The frozen batch review remains red, with two distinct classes independently confirmed below; affected-path merged/rejected prior art is now checked, and the contribution-artifact audit is deferred to publish (gate-logs/T4-batch-review.log:10; reviewer-prior-art.log:3; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN [impl] | Model ambiguous commits with the production error contract and assert both outcomes — the seeded store can apply a write then return a Terminal error, bypassing the reaper’s Indeterminate branch (crates/server/tests/custodian_gc.rs:1992; crates/custodian/src/reaper.rs:1238; reviewer-probes.log:3). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve restart-driven eventual cleanup for the intended deployment — an empty fleet still fails after one pass, so full teardown depends on supervisor restarts; the exercised runtime is redb, with network-backend compilation rather than live-cluster operation (crates/server/tests/multipart_reaper_deployed.rs:843; crates/server/src/cli.rs:1304; gate-logs/host-tikv.log:2). |

Two implementation findings remain; neither is inferred merely from a red gate.

1. **Prioritize shutdown before starting another pass (P2).** The unbiased `select!` at `crates/server/src/custodian.rs:733` can choose its timer when shutdown is ready too. An independent probe uses the real `run_reaper_until`, seeds leg A, polls through the first pass, then makes both the shutdown and the registered timer ready before polling again. In 45 of 100 runs it executes another pass and changes the store after the stop was ready (`reviewer-probes.log:11`). Give shutdown priority and add a controlled simultaneous-readiness regression. The probe is in `reviewer-probes.rs:2163`; rerun the compiled probe with `./reviewer-probes reviewer_ready_shutdown_and_timer --nocapture`.

2. **Exercise unknown commit outcomes faithfully (P2).** `FlakyMeta::commit` may apply its batch and then calls `Self::fault()` (`crates/server/tests/custodian_gc.rs:1992`). That helper returns a string error (`crates/server/tests/custodian_gc.rs:1959`), which the actual classifier categorizes as Terminal (`crates/traits/src/lib.rs:803`; independently executed in `reviewer-probes.log:3`). The reaper consequently takes its definite-failure/hold branch instead of `fence_unknown` (`crates/custodian/src/reaper.rs:1238`). Return the seam’s `CommitUnknownResult` for ambiguous writes, distinguish any definite read failures, and ensure the seeded campaign actually exercises both applied and unapplied ambiguous commits. This is the standing rubric’s test-fidelity requirement (`AGENTS.md:203`), not a claim that the production classifier is wrong.

The batch review’s nine entries represent three repeated classes. The shutdown and error-model classes are confirmed above. The third class correctly notes that the parent’s consumed stderr count does not prove when SIGINT reached the child (`crates/server/tests/multipart_reaper_deployed.rs:387`). I do not promote that observation to another required fix: separate new regressions already assert that the no-fleet driver polls shutdown before any store call and that the fleet driver polls it before any probe (`crates/server/tests/custodian_gc.rs:2046`; crates/server/tests/custodian_gc.rs:2149`). Those assertions directly catch the prior late-registration defect. The subprocess result is supporting evidence, not a precise timing proof.

The asserted red→green evidence was independently reproduced. Only production files and the runtime doc were stashed; the new subprocess test remained. The pre-fix run failed its store assertions, not compilation. After `stash pop`, the 9 subprocess and 16 custodian tests passed. `git apply --reverse --check ../patch.diff` then succeeded and no stash remained. Production source was not edited. The reviewer used Rust 1.96.1 for these runs; the target pins 1.96.0, and the separate frozen full-CI result is retained as its own evidence.

The gate evidence has these limits and dispositions:

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-ci | PASS from frozen output; relevant checks independently rerun | Full log ends “all checks passed”; focused tests, formatting, spelling, docs lint and rendering also passed here (gate-logs/C4-ci.log:5098; reviewer-restored-green.log:38; reviewer-docs.log:3). |
| C4-verify | PASS, independently reproduced | Pre-fix 2 passed/7 failed; restored 9 passed (reviewer-red.log:195; reviewer-restored-green.log:38). |
| C4-diff-cov | N/A — base-state caveat; coverage unmeasured | The wrapper could not apply the stacked patch to origin/main; this is not evidence of a defect in the supplied, buildable target (gate-logs/C4-diff-cov.log:10). |
| C5-mutants | N/A — inconclusive | Twenty mutants were discovered, but the 7200-second run was interrupted with no completed verdicts; no survival or adequacy claim is supported (gate-logs/C5-mutants.log:7; gate-logs/C5-mutants.log:10). |
| T4-batch-review | FAIL | Nine entries reduce to the three classes adjudicated above; two remain builder-fixable (gate-logs/T4-batch-review.log:10). |
| T4-contribution | N/A — deferred by design | PR description is drafted after Check; substantive audit must rerun at publish (gate-logs/T4-contribution.log:10). |
| host-tikv | PASS from frozen compilation output | Both metadata-tikv and server feature arms complete clippy; this does not claim live-cluster execution (gate-logs/host-tikv.log:2; gate-logs/host-tikv.log:211). |

The instance-scoped wrapper commands were not available as target commands; their supplied logs were read rather than treating that expected absence as a defect. Neither named external dependency remains unexercised: `typos` ran, and the docs renderer built 101 pages with a successful link audit. Prior art was checked for all five paths on GitHub’s default branch and against the complete file lists of all 24 closed/unmerged PRs. Only #647 overlaps, in the runtime doc; its closing decision concerned splitting a broader segmented-map change, not this reaper dispatch (`reviewer-prior-art.log:4`; `reviewer-prior-art.log:9`). Custodian restart/recovery behavior warrants follow-up observation in the intended deployment or nightly recovery run; no new disk-fault mechanism is introduced.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1016 (625.2, reaper on every custodian startup shape)

I re-ran parts of the proof in my own scratch copy of `$PDCA_TARGET`: the patched tree, the base production files with the patched tests, and two mutants. G2a and G2c hold up. G2b does not show what the brief says it shows. There are also two smaller issues, one in the test model and one in scope.

- NEEDS-HUMAN [human] — **G2b passes on the base. It is a regression guard, not red→green evidence, because the defect it targets does not exist on this base.** I put back the base `cli.rs` and `custodian.rs`, kept the patched `custodian_gc.rs`, and turned off only the two tests that call the new `run_reaper_until`. Then `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1845`) printed `test ... ok`. The brief's premise is wrong: it says the fleet loop's reaper "sits behind GC's whole-fleet gate" and that "G2b reds" (`brief.md:13-15`, `brief.md:54-57`). On the base, the reaper already rides the unconditional reconstruction pass (`git show HEAD:crates/server/src/custodian.rs` lines 673-683), which runs above the GC gate at base line 749. The patched call at `crates/server/src/custodian.rs:840` passes the same arguments as the base call, so on the fleet shape this is a refactor that keeps behaviour the same. The test's own claim (`custodian_gc.rs:67-68`: moving the reaper inside the GC branch turns it red) is a mutation claim and is fine. But "partially degraded fleet reaps nothing while GC is deferred" and its negation (11) should not be counted as a fixed defect. Correct any build-notes claim of a G2b red leg.

- NEEDS-HUMAN [impl] — **`FlakyMeta` models a state no real adapter produces, so the "outcome unknown" path is never exercised.** `crates/server/tests/custodian_gc.rs:1992-1999` sometimes applies the commit and then returns `Err("injected metadata fault")` (`:1959-1960`). `classify` maps a plain error to `ErrorClass::Terminal` (`crates/traits/src/lib.rs:786-803`). So the reaper takes its "the batch did not land" branch (`crates/custodian/src/reaper.rs:1242`), never the `Indeterminate` branch (`reaper.rs:1238`), even though the batch did land. The doc at `custodian_gc.rs:1909` says this models "the outcome a networked store could not confirm", and the seam contract says that outcome is a `CommitUnknownResult` (`crates/traits/src/lib.rs:205-271`). This breaks the rubric's test-fidelity rule. Fix: return `wyrd_traits::CommitUnknownResult { backend: "flaky", code: None, detail, may_still_commit: false }` for a faulted commit (for both draws, or at least the applied one), and check the seeds still converge. This confirms T4's three matching findings at `custodian_gc.rs:1998`.

- NEEDS-HUMAN [human] — **The `--interval-secs 0` refusal changes the empty-fleet failure, which the brief said must stay the same.** Concrete case: `wyrd custodian --interval-secs 0 --endpoints http://127.0.0.1:1 --ids 1 --failure-domains d1`. On the base it panics, exit 101, with "NO configured D server was reachable at startup". Patched, it returns `Err` at `crates/server/src/cli.rs:1059-1066`, which exits 1 (`cli.rs:433-435`) with the interval message and no diagnostic. `brief.md:103` says "MUST NOT change the panic's message or its exit status". Refusing 0 for the healthy fleet role is also outside the brief's scope (`brief.md:90-112`). `a_zero_interval_is_refused_wherever_the_role_loops` (`crates/server/tests/multipart_reaper_deployed.rs:1038`) locks this behaviour in. Nothing in the repo passes 0 (I searched all deploy, docs and test files), so the risk is low. The human should choose: keep it and say so in the PR, or limit the refusal to the no-endpoints shape.

- NEEDS-HUMAN [human] — **During a total fleet outage, the shipped unit now runs a reaper pass about every 2 s, whatever `--interval-secs` says.** `deploy/dist/systemd/wyrd-custodian.service:30-31` uses `Restart=on-failure` with `RestartSec=2`. When every dial is refused at once (D-server processes down, hosts up), each restart runs one full reaper pass and drain (`crates/server/src/cli.rs:1303-1316`) before the panic. The configured interval is 30 s by default (`cli.rs:906`). This is the back-to-back metadata load the patch itself gives as its reason to refuse an interval of 0 (`cli.rs:1056-1058`). The one-pass design was settled in the brief (`brief.md:95-99`), so this is a human call (accept it, or raise `RestartSec`), not a builder fix. Low priority.

- **I tried to break the "register Ctrl-C before the first pass" fix and could not.** Mutant: replace the poll at `crates/server/src/custodian.rs:712-715` with `let stop_requested = false;`. Results:
  - `no_endpoints_role_finishes_its_first_pass_when_stopped_during_it` (`multipart_reaper_deployed.rs:793`) failed 10 runs out of 10: the role was killed by signal 2. My temp dir was on disk, not tmpfs.
  - `no_fleet_loop_rides_out_seeded_store_faults_to_leg_a_teardown` (its `unarmed_calls` check, `custodian_gc.rs:2047`) failed every time.
  - `fleet_loop_polls_its_stop_before_its_first_pass` (`custodian_gc.rs:2113`) also failed every time.

  T4's point is right: `passes_when_interrupted` (`multipart_reaper_deployed.rs:801`) counts the lines the parent has read, so it cannot prove the signal landed mid-pass. But the two in-process tests above pin the property without depending on timing, so rejecting that T4 item with this reason is fair.

- **T4's "unbiased `select!`" finding (`crates/server/src/custodian.rs:733-736`) is real but old and harmless.** The same unbiased select was in the base (`HEAD:crates/server/src/custodian.rs` lines 795-798). It only matters when the stop and the interval timer become ready at the same moment. Then the loop may run one more complete pass and stop at the next wait. It never cuts a pass short. Adding `biased;` fixes it in one line if that is cheaper than arguing about it.

- **Other attacks that failed:**
  - G2a and G2c go red on the base for the reasons the brief predicts (`gate-logs/C4-verify.log`: 0 passes, session as seeded) and green with the fix. In the 9-test file, 7 fail on the base. The 2 that pass on the base (`empty_fleet_restore_one_shot_fails_with_the_store_untouched`, `empty_fleet_role_creates_no_store`) are guards by design. The gate summary "9 test(s) ran red" means 9 ran, not 9 failed.
  - The restore one-shot skips the pass (`cli.rs:1303`).
  - The refusals for fleet-only flags (`cli.rs:1051-1055`) hold.
  - The doc's claim that every TiKV/FDB call has its own deadline is backed by `crates/metadata-tikv/src/lib.rs:137-233` and `crates/metadata-fdb/src/lib.rs:78-89`.
  - The subprocess tests pin `--metadata-backend redb` and drop `WYRD_METADATA_BACKEND` (`multipart_reaper_deployed.rs:258`, `:262`).

- **Two evidence gates produced nothing this round.** C5 mutants hit its 7200 s limit with 20 mutants found and none reported (`gate-logs/C5-mutants.log`). C4 diff coverage did not run because the bundle's base (1015) is not on `origin/main` (`gate-logs/C4-diff-cov.log`). Neither result counts against the fix. It just means the verdict rests only on C4-ci, C4-verify and the reviews.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/tests/custodian_gc.rs:1998`: **The seeded fault model misrepresents ambiguous commits.** `FlakyMeta::commit` sometimes applies the batch and then returns the plain string error from `fault()` (`:1960`). That classifies as `Terminal`, so the reaper takes its definite-failure/hold branch instead of its unknown-outcome branch (`crates/custodian/src/reaper.rs:1238`). Eventual teardown can pass while the new driver test never exercises production ambiguous-commit handling, contrary to the rubric's model-fidelity requirement. Return `CommitUnknownResult` for the simulated ambiguous outcomes (`may_still_commit: false` for this settled model), and assert that both applied and unapplied cases are exercised.

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_reaper_deployed.rs:801`: **The first-pass SIGINT test can pass without interrupting the first pass.** `passes_when_interrupted` records tally lines consumed by the parent (`:387`), not the child's progress. If the parent is delayed while the child finishes the pass, the queued first fence line still triggers SIGINT with a recorded count of zero. A child that registers Ctrl-C only after that pass can then exit successfully with one tally and all 48 fences, satisfying every assertion. Increasing the fixture size does not establish ordering. Add a driver test with a controlled metadata-call barrier, request shutdown while the first pass is demonstrably suspended, then release it and verify completion without starting another pass; retain the subprocess check as a signal-wiring smoke test.

No additional reuse, simplification, or efficiency findings. Validation used target-source inspection and the supplied frozen gate logs; builds and tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Make a ready shutdown win at the interval boundary — the real driver began another pass and mutated metadata in 45/100 runs with shutdown and timer both ready (crates/server/src/custodian.rs:733; reviewer-probes.log:11).
- [ ] T5 Judgment — Model ambiguous commits with the production error contract and assert both outcomes — the seeded store can apply a write then return a Terminal error, bypassing the reaper’s Indeterminate branch (crates/server/tests/custodian_gc.rs:1992; crates/custodian/src/reaper.rs:1238; reviewer-probes.log:3).
- [ ] Validation — fitness-to-purpose — Approve restart-driven eventual cleanup for the intended deployment — an empty fleet still fails after one pass, so full teardown depends on supervisor restarts; the exercised runtime is redb, with network-backend compilation rather than live-cluster operation (crates/server/tests/multipart_reaper_deployed.rs:843; crates/server/src/cli.rs:1304; gate-logs/host-tikv.log:2).
- [ ] **G2b passes on the base. It is a regression guard, not red→green evidence, because the defect it targets does not exist on this base.** I put back the base `cli.rs` and `custodian.rs`, kept the patched `custodian_gc.rs`, and turned off only the two tests that call the new `run_reaper_until`. Then `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1845`) printed `test ... ok`. The brief's premise is wrong: it says the fleet loop's reaper "sits behind GC's whole-fleet gate" and that "G2b reds" (`brief.md:13-15`, `brief.md:54-57`). On the base, the reaper already rides the unconditional reconstruction pass (`git show HEAD:crates/server/src/custodian.rs` lines 673-683), which runs above the GC gate at base line 749. The patched call at `crates/server/src/custodian.rs:840` passes the same arguments as the base call, so on the fleet shape this is a refactor that keeps behaviour the same. The test's own claim (`custodian_gc.rs:67-68`: moving the reaper inside the GC branch turns it red) is a mutation claim and is fine. But "partially degraded fleet reaps nothing while GC is deferred" and its negation (11) should not be counted as a fixed defect. Correct any build-notes claim of a G2b red leg.
- [ ] **`FlakyMeta` models a state no real adapter produces, so the "outcome unknown" path is never exercised.** `crates/server/tests/custodian_gc.rs:1992-1999` sometimes applies the commit and then returns `Err("injected metadata fault")` (`:1959-1960`). `classify` maps a plain error to `ErrorClass::Terminal` (`crates/traits/src/lib.rs:786-803`). So the reaper takes its "the batch did not land" branch (`crates/custodian/src/reaper.rs:1242`), never the `Indeterminate` branch (`reaper.rs:1238`), even though the batch did land. The doc at `custodian_gc.rs:1909` says this models "the outcome a networked store could not confirm", and the seam contract says that outcome is a `CommitUnknownResult` (`crates/traits/src/lib.rs:205-271`). This breaks the rubric's test-fidelity rule. Fix: return `wyrd_traits::CommitUnknownResult { backend: "flaky", code: None, detail, may_still_commit: false }` for a faulted commit (for both draws, or at least the applied one), and check the seeds still converge. This confirms T4's three matching findings at `custodian_gc.rs:1998`.
- [ ] **The `--interval-secs 0` refusal changes the empty-fleet failure, which the brief said must stay the same.** Concrete case: `wyrd custodian --interval-secs 0 --endpoints http://127.0.0.1:1 --ids 1 --failure-domains d1`. On the base it panics, exit 101, with "NO configured D server was reachable at startup". Patched, it returns `Err` at `crates/server/src/cli.rs:1059-1066`, which exits 1 (`cli.rs:433-435`) with the interval message and no diagnostic. `brief.md:103` says "MUST NOT change the panic's message or its exit status". Refusing 0 for the healthy fleet role is also outside the brief's scope (`brief.md:90-112`). `a_zero_interval_is_refused_wherever_the_role_loops` (`crates/server/tests/multipart_reaper_deployed.rs:1038`) locks this behaviour in. Nothing in the repo passes 0 (I searched all deploy, docs and test files), so the risk is low. The human should choose: keep it and say so in the PR, or limit the refusal to the no-endpoints shape.
- [ ] **During a total fleet outage, the shipped unit now runs a reaper pass about every 2 s, whatever `--interval-secs` says.** `deploy/dist/systemd/wyrd-custodian.service:30-31` uses `Restart=on-failure` with `RestartSec=2`. When every dial is refused at once (D-server processes down, hosts up), each restart runs one full reaper pass and drain (`crates/server/src/cli.rs:1303-1316`) before the panic. The configured interval is 30 s by default (`cli.rs:906`). This is the back-to-back metadata load the patch itself gives as its reason to refuse an interval of 0 (`cli.rs:1056-1058`). The one-pass design was settled in the brief (`brief.md:95-99`), so this is a human call (accept it, or raise `RestartSec`), not a builder fix. Low priority.
- [ ] `crates/server/tests/custodian_gc.rs:1998`: **The seeded fault model misrepresents ambiguous commits.** `FlakyMeta::commit` sometimes applies the batch and then returns the plain string error from `fault()` (`:1960`). That classifies as `Terminal`, so the reaper takes its definite-failure/hold branch instead of its unknown-outcome branch (`crates/custodian/src/reaper.rs:1238`). Eventual teardown can pass while the new driver test never exercises production ambiguous-commit handling, contrary to the rubric's model-fidelity requirement. Return `CommitUnknownResult` for the simulated ambiguous outcomes (`may_still_commit: false` for this settled model), and assert that both applied and unapplied cases are exercised.
- [ ] `crates/server/tests/multipart_reaper_deployed.rs:801`: **The first-pass SIGINT test can pass without interrupting the first pass.** `passes_when_interrupted` records tally lines consumed by the parent (`:387`), not the child's progress. If the parent is delayed while the child finishes the pass, the queued first fence line still triggers SIGINT with a recorded count of zero. A child that registers Ctrl-C only after that pass can then exit successfully with one tally and all 48 fences, satisfying every assertion. Increasing the fixture size does not establish ordering. Add a driver test with a controlled metadata-call barrier, request shutdown while the first pass is demonstrably suspended, then release it and verify completion without starting another pass; retain the subprocess check as a signal-wiring smoke test.
- [ ] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- [ ] size backstop — this slice is behaving oversized: patch is 119 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C1 Spec — Decide whether G2c permits convergence across supervisor restarts — the brief requires the full leg-A transition after one invocation and exactly one pass, but an independently exercised invocation leaves `Aborting` and count 1 (brief.md:38; brief.md:96; reviewer-probe.log:3).
- [ ] **G2b was never red, because the defect the brief describes for the fleet shape does not exist on this base.** In the base, `HEAD:crates/server/src/custodian.rs:680` already hands `Some(&reaper)` to the reconstruction pass, which runs every interval. The GC gate is further down, at `HEAD:…/custodian.rs:749`. I ran the new test `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1748`) with `cli.rs` and `custodian.rs` reverted to HEAD: **it passes**. So two lines in the brief are wrong: the Defect line ("That call sits behind GC's whole-fleet gate… `custodian.rs:680-713`") and the Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). The fleet-path edit at `custodian.rs:755` is a refactor that changes no behavior. G2b only guards against a future regression (negation 11), and the green ci row is not red→green evidence for it. This isn't a defect in the patch, but one of the three claimed defects was not real, and the human should know that before sign-off.
- [ ] **The no-endpoints role now creates a store if none exists, then reaps it forever and reports nothing wrong.** Before this patch the no-endpoints role opened nothing. Now `run_reaper_over_backend` uses the store-creating open (`cli.rs:1875-1880`, `:2251-2256`). With a mistyped `--data-dir`, or the shipped systemd unit run without `--metadata-backend fdb` (`deploy/dist/systemd/wyrd-custodian.service:28` pins `--data-dir ${STATE_DIRECTORY}`), the role creates an empty redb store and prints "Running the multipart reaper alone every Ns" for as long as it runs. That is the false all-clear the one-shot was built to refuse (`cli.rs:2258-2270`; the rubric's "never silent success"). The fleet role opens its store the same way, and the doc comment calls this deliberate, so it's a design decision, not a slip.
- [ ] C1 Spec — Approve the corrected acceptance contract — G2b already passes on the supplied base, and G2c completes teardown across restarts rather than one invocation (reviewer-g2b-base.log:6; crates/server/tests/multipart_reaper_deployed.rs:673; brief.md:45).
- [ ] **G2b has no red leg, because the fleet-shape defect in the brief does not exist on this base.** I ran `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (crates/server/tests/custodian_gc.rs) on base `dbb41ef` with only the test added, and it printed `ok`. On the base, the reaper already rides the reconstruction pass (`git show HEAD:crates/server/src/custodian.rs`: `Some(&reaper)` at :680), and that pass sits outside GC's gate (`if fleet.len() == operator_fleet_size` at :749). The brief misreads the base twice: its Defect line ("That call sits behind GC's whole-fleet gate, custodian.rs:680-713") and its Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). So the fleet-loop edit at crates/server/src/custodian.rs:791 is a refactor that changes no behaviour. The PR must not claim it fixes "on a partially degraded fleet it reaps nothing while GC is deferred". G2b does catch negation (11): on the base I gated `Some(&reaper)` behind `fleet.len() == operator_fleet_size`, and G2b failed at custodian_gc.rs:1809. Keep G2b as a regression guard, and strike the fleet-shape defect claim from the summary and the PR text.
- [ ] **Two loops instead of one driver, and only the new loop can drop a pass mid-commit.** The brief's scope says "One reaper interval driver that both deployed shapes run". The patch shares the step (`reaper_pass`) but not the loop:
- [ ] **`wyrd custodian` with no flags now holds the default store's lock until Ctrl-C.** I reproduced this with the patched binary in an empty dir. I started `wyrd custodian --interval-secs 5` (default `--data-dir wyrd-data`), then ran `wyrd put payload.txt --key k1`. It failed with `wyrd: Database already open. Cannot acquire lock.` and exit 1. After Ctrl-C, the same put succeeded. The path is crates/server/src/cli.rs:1184 → `run_reaper_over_backend` → `open_local_meta_redb` (cli.rs:2307), and the store stays open for the life of the loop. On the base, this shape exited at once and never opened the store. The fleet shape already held the lock on the base, and a reaper on redb has to open the store, so this follows from the brief. But it turns a harmless no-op into something that blocks `wyrd put` and `wyrd s3` on the same dev data dir. Decide whether to accept it as dev-only redb behaviour and say so in the usage text.
- [ ] C1 Spec — Accept the corrected baseline and G2c interpretation — G2b already passes before this patch, contrary to the brief, and exactly one pass per empty-fleet invocation requires restarts to complete teardown (`reviewer-red.log:270`; `crates/server/tests/multipart_reaper_deployed.rs:714`; `brief.md:54`).
- [ ] **G2b has no red leg on this base; the brief's premise for it is false.** On the base, `crates/server/src/custodian.rs` (HEAD) already dispatches the reaper inside the reconstruction pass (`Some(&reaper)` at HEAD `:680`), *ahead of* GC's whole-fleet gate (HEAD `:749`), not behind it. So the brief's Defect ("That call sits behind GC's whole-fleet gate … the reaper runs only on a healthy fleet", brief.md:13-15) and Falsifiability ("G2b reds because the reaper sits behind the whole-fleet gate", brief.md:57) do not hold for this base. **Reproduced:** with production code reverted to HEAD and the patched test file kept, `cargo test -p wyrd-server --test custodian_gc deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` → `1 passed`. The fleet-loop edits (`crates/server/src/custodian.rs:823`, `:890`) are therefore a refactor with no change in behavior, and the doc line "Dispatching the reaper only inside GC's whole-fleet branch turns it red" (`crates/server/tests/custodian_gc.rs:68`) describes a hypothetical mutant, not the base. No gate measured a G2b red: C4-verify ran only `multipart_reaper_deployed` (gate-logs/C4-verify.log). The test also never checks that GC was actually deferred (no "gc pass deferred" assertion anywhere in `custodian_gc.rs`, test at `:1753`), so it would pass with the GC gate removed too. A human should decide whether to record G2b as a regression guard only (and correct the brief/§6 record), or confirm the real 625.1 base differs from this folded one.
- [ ] **The new destructive dispatches have no seeded Tier-0 DST coverage.** The rubric's *Test fidelity* class says "a new destructive or concurrent path lands with seeded Tier-0 DST coverage". The no-endpoints loop (`crates/server/src/custodian.rs:722-745`) and the empty-fleet pass (`crates/server/src/cli.rs:1300-1313`) are new paths that fence and delete records, and they are covered only by subprocess tests. The brief forbids editing `crates/dst/` (brief.md:69-71), so the builder cannot close this. A human should decide whether dispatch-only wiring over the DST-covered reaper (625.1) meets the rule, or hand a row to child-3 / a follow-up issue. (Same gap as T4's TEST-GAP at `cli.rs:1299`/`:1300`.)
- [ ] **`--interval-secs 0` is now refused for the existing fleet role as well.** `crates/server/src/cli.rs:1058-1066` refuses 0 for every non-one-shot shape, including `--endpoints … --interval-secs 0`, which the base accepted. A deployment that used 0 would now fail at startup and crash-loop with no reconstruction or GC at all. The brief scopes this slice to "where the pass runs" and says it "adds no reaper logic" (brief.md:8); a new startup refusal on the fleet shape goes past that. Low practical risk: the repo's compose files use 15, and usage/docs are updated. A human should decide whether to keep it as a hot-loop guard or limit it to the no-endpoints shape.

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
- Iteration delta (if iterating): Keep the design (shared reaper step, one driver; G2a/G2c red→green held). Fix only: - Shutdown priority: at `custodian.rs:733` a ready shutdown must win over a ready timer (biased select or check-before-pass). Add a driver test with both ready that asserts no further pass. - Fault model: `FlakyMeta::commit` (`custodian_gc.rs:1992`) returns `CommitUnknownResult { may_still_commit: false, .. }` for faulted commits. Assert that both applied and unapplied draws occur, and that seeds still converge. - Scope: limit the `--interval-secs 0` refusal (`cli.rs:1058-1066`) to the no-endpoints shape. The fleet and empty-fleet shapes stay as on base (panic message and exit 101 unchanged). Update `a_zero_interval_is_refused_wherever_the_role_loops`. - Usage text: say that no-endpoints on redb holds the data-dir lock while it runs. - Records: G2b is a regression guard only (no red on base). Don't claim the degraded-fleet defect as fixed in build-notes. Decided at sign-off (do not reopen): one pass per empty-fleet invocation with restart-driven convergence; store-creating open; no DST rows here. No new features.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
