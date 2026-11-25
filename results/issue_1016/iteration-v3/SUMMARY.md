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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 14m: 15 caught, 5 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.32s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1016: run the multipart reaper on every ordinary custodian startup shape; the startup fix works, but first-pass shutdown and its regression coverage need correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Accept the corrected baseline and G2c interpretation — G2b already passes before this patch, contrary to the brief, and exactly one pass per empty-fleet invocation requires restarts to complete teardown (`reviewer-red.log:270`; `crates/server/tests/multipart_reaper_deployed.rs:714`; `brief.md:54`). |
| C2 Reproduction (red pre-fix) | PASS | Independently retained the tests and stashed production changes: eight deployed-role tests executed, six failed, including both binding store-transition assertions; G2b is existing behavior (`reviewer-red.log:377`; `reviewer-red.log:461`). |
| C3 Change | PASS | The five-file change stays within the owned server/runtime-doc surface, keeps the default policy and GC gate, and preserves restore-mode refusal without metadata mutation (`crates/server/src/custodian.rs:487`; `crates/server/src/custodian.rs:890`; `crates/server/src/cli.rs:1300`). |
| C4 Verification (red→green) | PASS | Restoring the patch yields 8/8 deployed-role and 13/13 custodian-GC tests; all three optional backend features type-check; full-CI and coverage limitations are environment/base caveats below, not patch failures (`reviewer-green.log:20`; `reviewer-green.log:34`; `reviewer-features.log:182`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Arm SIGINT handling before destructive work — the new no-fleet path can terminate during its first pass because the shutdown future is first polled afterward; the real-binary probe exits on signal 2 after fencing an upload (`crates/server/src/cli.rs:1145`; `crates/server/src/custodian.rs:715`; `reviewer-signal-probe.log:16`). |
| T1 Structure | PASS | The common driver preserves narrow metadata seams, one lifecycle clock, and separate GC gating; no new dependency or shared global is introduced (`crates/server/src/custodian.rs:508`; `crates/server/src/custodian.rs:701`; `crates/server/src/custodian.rs:890`). |
| T2 Shape | PASS | Unsupported no-fleet flags and zero intervals fail explicitly, and operator help/runtime documentation explain the changed startup behavior; the inaccurate first-pass shutdown guarantee is addressed under C5 (`crates/server/src/cli.rs:1046`; `crates/server/src/cli.rs:1059`; `docs/design/architecture/06-runtime-view.md:94`). |
| T3 Runtime | FAIL | First-pass Ctrl-C violates the advertised graceful-stop contract: the actual subprocess exits with SIGINT, zero completed-pass tallies and no shutdown announcement (`reviewer-signal-probe.log:17`; `crates/server/src/custodian.rs:679`). |
| T4 Contribution | FAIL | The required review still has confirmed shutdown and seeded-dispatch coverage findings to resolve; affected-path prior art is checked, and contribution-artifact auditing is separately N/A until publish (`gate-logs/T4-batch-review.log:10`; `AGENTS.md:221`; `reviewer-prior-art.json:4`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Cover the first-pass interruption and new dispatch/retry lifecycle — the subprocess test signals only after five passes, while existing seeded tests bypass the new driver, contrary to the rubric for new destructive paths (`crates/server/tests/multipart_reaper_deployed.rs:661`; `crates/dst/tests/custodian.rs:7438`; `AGENTS.md:203`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the deployment semantics — a no-endpoints role now remains active over an existing store, and a fully unreachable fleet makes progress across failed starts; operational suitability and restart policy require human sign-off (`crates/server/src/cli.rs:1203`; `crates/server/src/cli.rs:1300`). |

This review is advisory. Source citations above and below are grounded in `$PDCA_TARGET` (`./target`); log, brief and reviewer-artifact citations are relative to this review directory. The supplied target is usable: the patch reverses cleanly in a dry run, and the independent build executes both trees. The independent red/green command was `cargo test --offline --locked -p wyrd-server --test multipart_reaper_deployed --test custodian_gc`. Production changes were restored after the red run; no implementation was edited.

1. **Register shutdown handling before the first reaper pass (C5/T3).** Constructing the async block at `crates/server/src/cli.rs:1145` does not execute `ctrl_c()`. The driver pins it but skips its only poll on the first interval (`crates/server/src/custodian.rs:701`, `:715`), then enters the metadata pass (`:732`). Consequently SIGINT still has its default terminating disposition during that pass. This affects the newly active no-endpoints path, regardless of whether the older fleet path shared the same weakness.

   I reproduced this against the unmodified patched binary, using the supplied leg-A seed plus a second live upload with 10,000 committed parts to keep the initial scan in progress. The probe sends SIGINT only after the actual audit event confirms the first upload's abort fence committed. Observed: signal 2, no completed-pass tally, no graceful shutdown message, and the first upload left `Aborting@1` (`reviewer-signal-probe.log:16`). Register the signal listener before entering the pass, retain the stop notification, and honor it between passes without cancelling an active pass. The standalone reproducer is `reviewer-signal-probe.rs:994`; rerun with `TMPDIR="$PWD/reviewer-tmp" ./reviewer-signal-probe --exact reviewer_sigint_during_first_reaper_pass --nocapture`.

2. **Exercise the new lifecycle rather than only the underlying reaper (T5).** The existing G2a test stops after the fifth tally (`crates/server/tests/multipart_reaper_deployed.rs:661`), by which point signal handling has already been installed. It therefore passes alongside the reproduced first-pass failure. Add that regression and seeded coverage for the new driver/dispatch lifecycle, including delayed or failed store work and stopping between passes. The existing DST helper invokes `reconcile_step` directly (`crates/dst/tests/custodian.rs:7426`); it does not execute `run_reaper_until`, the interval wait, or startup dispatch. The standing requirement is explicit (`AGENTS.md:203`). If satisfying it requires changes under the brief's excluded `crates/dst/` surface, obtain scope re-entry rather than silently expanding this slice (`brief.md:69`).

**The batch review's GC-deferral objection is rejected as a duplicate coverage demand.** Its two occurrences inspect only the newly added fixture, but the same test target already exercises the actual operator-fleet gate with expired pending records and real fragment-preservation assertions (`crates/server/tests/custodian_gc.rs:927`, `:993`, `:1004`). Both existing deferral tests and the new reaper-on-degraded-fleet test pass independently (`reviewer-green.log:9`, `:17`). The patch leaves the gate condition intact (`crates/server/src/custodian.rs:890`). The seven reported blockers therefore collapse into three classes: one reproduced shutdown defect, one missing lifecycle-coverage requirement, and this rejected GC allegation; they are not seven independent defects.

**The brief needs two factual adjustments, not another implementation of G2b.** The supplied base already dispatches the reaper before GC's gate, and the added degraded-fleet test passes with production reverted (`reviewer-red.log:270`). G2a and G2c do independently fail before the fix. G2c's test honestly distinguishes one pass per invocation from eventual teardown across restarts (`crates/server/tests/multipart_reaper_deployed.rs:714`). The human should accept that interpretation or revise the brief's conflicting single-start/full-teardown wording. The patch removes missing dispatch paths; it adds no capability probe masking an eager/load-time cause.

| Gate evidence | Verdict | Re-derived basis |
|------|---------|-------|
| C4-verify | PASS | Independent red: 2 passed, 6 failed of 8; independent green: 8 passed. The frozen log reports the same outcomes (`reviewer-red.log:461`; `reviewer-green.log:34`; `gate-logs/C4-verify.log:14`). |
| C4-ci | PASS | The frozen original-worktree log records a full successful run, including typos, docs rendering and DST (`gate-logs/C4-ci.log:11`; `:19`; `:5092`). The independent rerun's synthetic-index limitation is detailed below. |
| C4-diff-cov | N/A | Coverage was not measured: the wrapper could not apply the stacked patch to its `origin/main`; this is base-state evidence, not a source/compile defect (`gate-logs/C4-diff-cov.log:10`). |
| C5-mutants | PASS | The frozen run reports 20 tested, 15 caught and 5 unviable; no surviving mutant is reported. I did not rerun this instance-scoped campaign or infer unlisted scheduling negations from its summary (`gate-logs/C5-mutants.log:10`). |
| T4-batch-review | FAIL | Frozen review reports 7 blockers and 1 recorded rejection; independent dispositions for its three visible classes are above (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | PR/commit artifacts are intentionally absent at Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen TiKV/server compile succeeds; independent `cargo clippy --offline --locked -p wyrd-server --features tikv,fdb,etcd --tests` also succeeds (`gate-logs/host-tikv.log:211`; `reviewer-features.log:182`). |

**The independent full-CI limitation belongs to the synthetic checkout.** `cargo xtask ci` completed with exit 1: its only two failed steps derive from the missing advisory gitlink; the remaining checks, including seeded DST, completed successfully (`reviewer-ci.log:4460`). It actually ran typos, docs lint/render, formatting, clippy, build and tests. The index-sensitive advisory test fails because `.github/advisory-db` is absent from this synthetic base's git index (`reviewer-ci.log:3804`), and the advisory-pin step consequently cannot run (`:3822`). The patch does not touch that gitlink; the frozen original-worktree log shows the pinned audit passing (`gate-logs/C4-ci.log:4434`). This does not warrant C4 FAIL or an oracle-missing escalation. The brief's external dependencies, `typos` and `docs-renderer`, were exercised successfully (`reviewer-ci.log:2`, `:10`). Feature compilation is not a live TiKV/FDB integration run; production topology and Tier-1/Tier-2 follow-up remain part of deployment validation.

**Affected-path prior art was checked against published merged history and closed/unmerged work.** GitHub's commits API at the supplied base `6e44de924cc8a451a71ce7f7ed63aca76c67f509` returned 56/11/10/0/30 commits for the five affected paths; all result sets fit one 100-entry page. The latest relevant predecessor is [the #1015 reaper commit](https://github.com/getwyrd/wyrd/commit/427e5ec8a1f2085d798a61104c318d39d1a9973c). I then enumerated the actual changed filenames of all 24 returned closed/unmerged PRs, rather than relying on prose path mentions. Only [PR #647](https://github.com/getwyrd/wyrd/pull/647) intersects, through the runtime document: it concerns segmented chunk maps and was [closed for needing smaller scope](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571), not a competing reaper-startup solution. Queries, commits and file intersections are retained in `reviewer-prior-art.json:4`.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Register Ctrl-C handling before the first reaper pass.** At `crates/server/src/custodian.rs:715`, the first interval skips polling `shutdown` and starts the new no-fleet reaper pass at `crates/server/src/custodian.rs:732`. The CLI puts `tokio::signal::ctrl_c()` inside that lazy future (`crates/server/src/cli.rs:1145`), so its signal handler is not registered yet. A SIGINT during a slow first pass can terminate the newly added no-endpoints role immediately, interrupting its destructive work instead of finishing the pass and exiting cleanly as promised. Register the listener before starting the pass while keeping shutdown between passes. Add a regression that interrupts the first pass; the current test waits until five passes have emitted (`crates/server/tests/multipart_reaper_deployed.rs:661`), after registration has already happened.

No additional actionable reuse, simplification, or efficiency findings. Reviewed the target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Accept the corrected baseline and G2c interpretation — G2b already passes before this patch, contrary to the brief, and exactly one pass per empty-fleet invocation requires restarts to complete teardown (`reviewer-red.log:270`; `crates/server/tests/multipart_reaper_deployed.rs:714`; `brief.md:54`).
- [ ] C5 Causal adequacy — Arm SIGINT handling before destructive work — the new no-fleet path can terminate during its first pass because the shutdown future is first polled afterward; the real-binary probe exits on signal 2 after fencing an upload (`crates/server/src/cli.rs:1145`; `crates/server/src/custodian.rs:715`; `reviewer-signal-probe.log:16`).
- [ ] T5 Judgment — Cover the first-pass interruption and new dispatch/retry lifecycle — the subprocess test signals only after five passes, while existing seeded tests bypass the new driver, contrary to the rubric for new destructive paths (`crates/server/tests/multipart_reaper_deployed.rs:661`; `crates/dst/tests/custodian.rs:7438`; `AGENTS.md:203`).
- [ ] Validation — fitness-to-purpose — Approve the deployment semantics — a no-endpoints role now remains active over an existing store, and a fully unreachable fleet makes progress across failed starts; operational suitability and restart policy require human sign-off (`crates/server/src/cli.rs:1203`; `crates/server/src/cli.rs:1300`).
- [ ] **G2b has no red leg on this base; the brief's premise for it is false.** On the base, `crates/server/src/custodian.rs` (HEAD) already dispatches the reaper inside the reconstruction pass (`Some(&reaper)` at HEAD `:680`), *ahead of* GC's whole-fleet gate (HEAD `:749`), not behind it. So the brief's Defect ("That call sits behind GC's whole-fleet gate … the reaper runs only on a healthy fleet", brief.md:13-15) and Falsifiability ("G2b reds because the reaper sits behind the whole-fleet gate", brief.md:57) do not hold for this base. **Reproduced:** with production code reverted to HEAD and the patched test file kept, `cargo test -p wyrd-server --test custodian_gc deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` → `1 passed`. The fleet-loop edits (`crates/server/src/custodian.rs:823`, `:890`) are therefore a refactor with no change in behavior, and the doc line "Dispatching the reaper only inside GC's whole-fleet branch turns it red" (`crates/server/tests/custodian_gc.rs:68`) describes a hypothetical mutant, not the base. No gate measured a G2b red: C4-verify ran only `multipart_reaper_deployed` (gate-logs/C4-verify.log). The test also never checks that GC was actually deferred (no "gc pass deferred" assertion anywhere in `custodian_gc.rs`, test at `:1753`), so it would pass with the GC gate removed too. A human should decide whether to record G2b as a regression guard only (and correct the brief/§6 record), or confirm the real 625.1 base differs from this folded one.
- [ ] **A Ctrl-C during the first pass kills the role by signal, contradicting the new docs. Reproduced.** `shutdown` is a lazy async block (`crates/server/src/cli.rs:1145-1148`), and tokio installs the SIGINT handler only when `ctrl_c()` is first polled. `run_until` skips the `select!` on the first interval (`crates/server/src/custodian.rs:715-720`), so nothing polls `shutdown` until the first pass has finished. Until then SIGINT keeps its default action. Case: seed 200 idle `Open` sessions, run `wyrd custodian --data-dir D --metadata-backend redb --interval-secs 1` (no `--endpoints`), and send SIGINT when the "Running the multipart reaper alone" banner is read: **10/10 runs ended by signal 2**, 0 pass tallies, no "shutting down" line. That breaks the claims at `crates/server/src/custodian.rs:680-681` ("a pass, once started, runs to its end, so a Ctrl-C never drops one mid-commit") and `docs/design/architecture/06-runtime-view.md:94` ("a stop takes effect between two passes, never during one"). In my runs the kill landed before the first commit, so the store was unchanged, and redb commits are atomic. What actually breaks is the exit status (a supervisor sees a signal death, not exit 0) and the false doc claims. G2a only sends SIGINT after the 5th tally (`crates/server/tests/multipart_reaper_deployed.rs:661`), so it never covers this window. Fix: poll `shutdown` once before the first pass (e.g. a `biased` `select!` against `std::future::ready(())`, or register `tokio::signal::unix::signal(SignalKind::interrupt())` before the loop), and add a G2a variant that interrupts on the banner and asserts exit 0 plus "shutting down". This confirms the T4 blocking findings at `cli.rs:1212` / `custodian.rs:715`.
- [ ] **The new destructive dispatches have no seeded Tier-0 DST coverage.** The rubric's *Test fidelity* class says "a new destructive or concurrent path lands with seeded Tier-0 DST coverage". The no-endpoints loop (`crates/server/src/custodian.rs:722-745`) and the empty-fleet pass (`crates/server/src/cli.rs:1300-1313`) are new paths that fence and delete records, and they are covered only by subprocess tests. The brief forbids editing `crates/dst/` (brief.md:69-71), so the builder cannot close this. A human should decide whether dispatch-only wiring over the DST-covered reaper (625.1) meets the rule, or hand a row to child-3 / a follow-up issue. (Same gap as T4's TEST-GAP at `cli.rs:1299`/`:1300`.)
- [ ] **`--interval-secs 0` is now refused for the existing fleet role as well.** `crates/server/src/cli.rs:1058-1066` refuses 0 for every non-one-shot shape, including `--endpoints … --interval-secs 0`, which the base accepted. A deployment that used 0 would now fail at startup and crash-loop with no reconstruction or GC at all. The brief scopes this slice to "where the pass runs" and says it "adds no reaper logic" (brief.md:8); a new startup refusal on the fleet shape goes past that. Low practical risk: the repo's compose files use 15, and usage/docs are updated. A human should decide whether to keep it as a hot-loop guard or limit it to the no-endpoints shape.
- [ ] **Register Ctrl-C handling before the first reaper pass.** At `crates/server/src/custodian.rs:715`, the first interval skips polling `shutdown` and starts the new no-fleet reaper pass at `crates/server/src/custodian.rs:732`. The CLI puts `tokio::signal::ctrl_c()` inside that lazy future (`crates/server/src/cli.rs:1145`), so its signal handler is not registered yet. A SIGINT during a slow first pass can terminate the newly added no-endpoints role immediately, interrupting its destructive work instead of finishing the pass and exiting cleanly as promised. Register the listener before starting the pass while keeping shutdown between passes. Add a regression that interrupts the first pass; the current test waits until five passes have emitted (`crates/server/tests/multipart_reaper_deployed.rs:661`), after registration has already happened.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-
- [ ] C1 Spec — Decide whether G2c permits convergence across supervisor restarts — the brief requires the full leg-A transition after one invocation and exactly one pass, but an independently exercised invocation leaves `Aborting` and count 1 (brief.md:38; brief.md:96; reviewer-probe.log:3).
- [ ] **G2b was never red, because the defect the brief describes for the fleet shape does not exist on this base.** In the base, `HEAD:crates/server/src/custodian.rs:680` already hands `Some(&reaper)` to the reconstruction pass, which runs every interval. The GC gate is further down, at `HEAD:…/custodian.rs:749`. I ran the new test `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`crates/server/tests/custodian_gc.rs:1748`) with `cli.rs` and `custodian.rs` reverted to HEAD: **it passes**. So two lines in the brief are wrong: the Defect line ("That call sits behind GC's whole-fleet gate… `custodian.rs:680-713`") and the Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). The fleet-path edit at `custodian.rs:755` is a refactor that changes no behavior. G2b only guards against a future regression (negation 11), and the green ci row is not red→green evidence for it. This isn't a defect in the patch, but one of the three claimed defects was not real, and the human should know that before sign-off.
- [ ] **The no-endpoints role now creates a store if none exists, then reaps it forever and reports nothing wrong.** Before this patch the no-endpoints role opened nothing. Now `run_reaper_over_backend` uses the store-creating open (`cli.rs:1875-1880`, `:2251-2256`). With a mistyped `--data-dir`, or the shipped systemd unit run without `--metadata-backend fdb` (`deploy/dist/systemd/wyrd-custodian.service:28` pins `--data-dir ${STATE_DIRECTORY}`), the role creates an empty redb store and prints "Running the multipart reaper alone every Ns" for as long as it runs. That is the false all-clear the one-shot was built to refuse (`cli.rs:2258-2270`; the rubric's "never silent success"). The fleet role opens its store the same way, and the doc comment calls this deliberate, so it's a design decision, not a slip.
- [ ] C1 Spec — Approve the corrected acceptance contract — G2b already passes on the supplied base, and G2c completes teardown across restarts rather than one invocation (reviewer-g2b-base.log:6; crates/server/tests/multipart_reaper_deployed.rs:673; brief.md:45).
- [ ] **G2b has no red leg, because the fleet-shape defect in the brief does not exist on this base.** I ran `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (crates/server/tests/custodian_gc.rs) on base `dbb41ef` with only the test added, and it printed `ok`. On the base, the reaper already rides the reconstruction pass (`git show HEAD:crates/server/src/custodian.rs`: `Some(&reaper)` at :680), and that pass sits outside GC's gate (`if fleet.len() == operator_fleet_size` at :749). The brief misreads the base twice: its Defect line ("That call sits behind GC's whole-fleet gate, custodian.rs:680-713") and its Falsifiability line ("G2b reds because the reaper sits behind the whole-fleet gate"). So the fleet-loop edit at crates/server/src/custodian.rs:791 is a refactor that changes no behaviour. The PR must not claim it fixes "on a partially degraded fleet it reaps nothing while GC is deferred". G2b does catch negation (11): on the base I gated `Some(&reaper)` behind `fleet.len() == operator_fleet_size`, and G2b failed at custodian_gc.rs:1809. Keep G2b as a regression guard, and strike the fleet-shape defect claim from the summary and the PR text.
- [ ] **Two loops instead of one driver, and only the new loop can drop a pass mid-commit.** The brief's scope says "One reaper interval driver that both deployed shapes run". The patch shares the step (`reaper_pass`) but not the loop:
- [ ] **`wyrd custodian` with no flags now holds the default store's lock until Ctrl-C.** I reproduced this with the patched binary in an empty dir. I started `wyrd custodian --interval-secs 5` (default `--data-dir wyrd-data`), then ran `wyrd put payload.txt --key k1`. It failed with `wyrd: Database already open. Cannot acquire lock.` and exit 1. After Ctrl-C, the same put succeeded. The path is crates/server/src/cli.rs:1184 → `run_reaper_over_backend` → `open_local_meta_redb` (cli.rs:2307), and the store stays open for the life of the loop. On the base, this shape exited at once and never opened the store. The fleet shape already held the lock on the base, and a reaper on redb has to open the store, so this follows from the brief. But it turns a harmless no-op into something that blocks `wyrd put` and `wyrd s3` on the same dev data dir. Decide whether to accept it as dev-only redb behaviour and say so in the usage text.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Arm SIGINT handling before destructive work — the new no-fleet path can terminate during its first pass because the shutdown future is first polled afterward; the real-binary probe exits on signal 2 after fencing an upload (`crates/server/src/cli.rs:1145`; `crates/server/src/custodian.rs:715`; `reviewer-signal-probe.log:16`).; T5 Judgment — Cover the first-pass interruption and new dispatch/retry lifecycle — the subprocess test signals only after five passes, while existing seeded tests bypass the new driver, contrary to the rubric for new destructive paths (`crates/server/tests/multipart_reaper_deployed.rs:661`; `crates/dst/tests/custodian.rs:7438`; `AGENTS.md:203`).; **A Ctrl-C during the first pass kills the role by signal, contradicting the new docs. Reproduced.** `shutdown` is a lazy async block (`crates/server/src/cli.rs:1145-1148`), and tokio installs the SIGINT handler only when `ctrl_c()` is first polled. `run_until` skips the `select!` on the first interval (`crates/server/src/custodian.rs:715-720`), so nothing polls `shutdown` until the first pass has finished. Until then SIGINT keeps its default action. Case: seed 200 idle `Open` sessions, run `wyrd custodian --data-dir D --metadata-backend redb --interval-secs 1` (no `--endpoints`), and send SIGINT when the "Running the multipart reaper alone" banner is read: **10/10 runs ended by signal 2**, 0 pass tallies, no "shutting down" line. That breaks the claims at `crates/server/src/custodian.rs:680-681` ("a pass, once started, runs to its end, so a Ctrl-C never drops one mid-commit") and `docs/design/architecture/06-runtime-view.md:94` ("a stop takes effect between two passes, never during one"). In my runs the kill landed before the first commit, so the store was unchanged, and redb commits are atomic. What actually breaks is the exit status (a supervisor sees a signal death, not exit 0) and the false doc claims. G2a only sends SIGINT after the 5th tally (`crates/server/tests/multipart_reaper_deployed.rs:661`), so it never covers this window. Fix: poll `shutdown` once before the first pass (e.g. a `biased` `select!` against `std::future::ready(())`, or register `tokio::signal::unix::signal(SignalKind::interrupt())` before the loop), and add a G2a variant that interrupts on the banner and asserts exit 0 plus "shutting down". This confirms the T4 blocking findings at `cli.rs:1212` / `custodian.rs:715`.; **Register Ctrl-C handling before the first reaper pass.** At `crates/server/src/custodian.rs:715`, the first interval skips polling `shutdown` and starts the new no-fleet reaper pass at `crates/server/src/custodian.rs:732`. The CLI puts `tokio::signal::ctrl_c()` inside that lazy future (`crates/server/src/cli.rs:1145`), so its signal handler is not registered yet. A SIGINT during a slow first pass can terminate the newly added no-endpoints role immediately, interrupting its destructive work instead of finishing the pass and exiting cleanly as promised. Register the listener before starting the pass while keeping shutdown between passes. Add a regression that interrupts the first pass; the current test waits until five passes have emitted (`crates/server/tests/multipart_reaper_deployed.rs:661`), after registration has already happened.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1016/review-. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
