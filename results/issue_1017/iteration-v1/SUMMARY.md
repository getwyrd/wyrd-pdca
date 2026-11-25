# Result — issue 1017 / multipart-reaper-terminal-exits

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 625.1, two states still have no exit when no client verb arrives.
  - **`Completing`:** a gateway that died holding the fence leaves the session `Completing`
    forever. It is never rolled back at `W_completing`, and when over-age it is never moved to
    `Aborting`.
  - **`Completed`:** #659's drain retains the tombstone, because its deployed tombstone input
    says "retain" until #625 supplies the window. The record and its `mpuctl` slot are
    therefore held forever.

  Two more gaps fall to this slice:
  - The grace window `G_orphan` is not yet bound to the strict inequality 0016 assigns to #625.
  - `B_ops` rests on a per-operation latency "assumed, not measured" (`SLOWEST_OP_MILLIS`,
    `crates/core/src/multipart.rs:5082-5105`), whose replacement the source assigns to #625.
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper_exits.rs`
  drives the reaper through 625.1's one-shot as a subprocess (the same mechanics; the seeding
  helpers are **copied** into this file, not shared, per `run-verify.sh:141-144`). Every leg
  asserts on durable store state.
  **(D) `Completing` is exited, both ways.**
  - A `Completing` session whose `fenced_at_millis` is older than `W_completing`, and which is
    **not** over-age, rolls back to `Open@E+1` and installs `retire:records:{seg:<g>:<E>}` for
    that attempt's segments.
  - A `Completing` session that **is** over-age (`now - created_at > W_session`) goes
    **straight** to `Aborting@E+1`, in one batch that also installs that obligation. It is
    never rolled back first (`0016:1918-1930`).
  - Assert that the named epoch's `seg:` range drains empty and that a **later** epoch's
    segments are untouched.

  **(E-terminal) The clock guard on the terminal states.**
  - A foreign-clocked `Completing` session whose `fenced_at_millis` is far past `W_completing`
    is **not** rolled back: its state, epoch, `seg:` range and `retire:` records are all
    unchanged.
  - A foreign-clocked `Completed` session keeps its record, because its tombstone window
    cannot be judged. Assert that the record **survives**. Its only exit is #633's
    terminal-expiry verb.

  **(G) Tombstone expiry.** A local-clock `Completed` session whose `completed_at_millis` is
  older than `W_tombstone`, with no session `retire:` obligation left and an empty `sidx:`
  range, has its record deleted and `mpuctl.count` decremented by exactly one. A `Completed`
  session inside `W_tombstone` keeps its record, and an identical Complete retry still answers
  `AlreadyCompleted` (`multipart.rs:4789-4795`).

  **(H-terminal)** With a mismatched `mpuctl.profile`, a stale `Completing` session is not
  rolled back either (625.1's check guards every arm).

  **(F) Teardown over the composed reaper path is idempotent and waits for GC's decision.**
  The `orphan:` mark guard (arms (0), (a), (b), (c), and fail-closed on undecodable values) is
  **#659 child-1's** code and tested there. This slice does not implement, copy or re-test it
  arm by arm. It asserts only what the reaper's path adds on top:
  - Run the one-shot twice over a partially drained obligation of a session the reaper
    fenced. It converges to the same terminal state with **no double-decrement**.
  - **The reaper's teardown waits for the obligation.** In an `Aborting` session's obligation,
    one position carries a `reclaiming` mark. After the one-shot, the `mpu:` record is
    present, the count is unchanged, the mark is byte-identical, and the obligation is still
    there. The test then deletes that key, as GC's blind delete would (`gc.rs:804-824`), and
    runs the one-shot again. The position gets a fresh mark under `require_absent`, the
    obligation and the session are gone, and the count drops by exactly one.

  **`G_orphan` boundary** (`cargo xtask ci` evidence, appended to the existing
  `crates/custodian/tests/gc.rs`): pin the tick at which a mark written by a fence becomes
  reclaimable, and show that `G_orphan > W_write + δ_clock` holds **strictly**. GC's grace check
  is inclusive (`gc.rs:172-176`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the `Completing` rollback at `W_completing` and the over-age `Completing → Aborting` edge,
    both installing `retire:records:{seg:<g>:<E>}`, through the existing fenced transitions;
  - the clock guard applied to `W_completing` and `W_tombstone`;
  - the `W_tombstone` judgment, which replaces "retain" as #659's tombstone input via
    `ReaperPolicy`;
  - the knob values `W_completing`, `W_tombstone` and **`G_orphan`**: each inside 0016's range
    and cited at its definition, with `G_orphan > W_write + δ_clock` and, where a repoint can
    intervene, `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, `:2497-2509`);
  - the calibration experiment;
  - docs in `08-crosscutting-concepts.md`.

  **Not this slice's to choose:** `W_ref`, `B_bytes`, `B_ops`, the profile tuple and
  `MAX_SESSIONS` are #655's single value set, consumed unchanged.

  **Out of scope:**
  - the `Open` arms (625.1) and the deployed wiring (625.2);
  - the operator abort and terminal-expiry verbs and the foreign-clock alarm (#633);
  - the `orphan:` mark guard, including arm (0) (`reclaiming`), and its DST rows (#659
    child-1), and X56 (#660);
  - #806, #630, FU-2 and FU-4;
  - ADRs, specs, and any edit to 0016;
  - any change to the records or state machine of the slices below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: cargo test --workspace --exclude wyrd-dst: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 84 mutants tested in 11m: 20 missed, 54 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 22 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.67s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1017’s multipart reaper exits, tombstone expiry, orphan grace and batch calibration: focused exits work, but clock safety, test fidelity and integration need another iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Durable D/G outcomes, clock ownership, obligation ordering and slice boundaries are explicit and falsifiable; `brief.md:27`, `brief.md:43`, `brief.md:89`. |
| C2 Reproduction (red pre-fix) | PASS | Independent production-stash run executed seven tests with six failures, including both Completing exits and tombstone expiry; `reviewer-evidence/red.log:7`, `reviewer-evidence/red.log:278`. |
| C3 Change | NEEDS-HUMAN | Decide Plan re-entry and sibling ownership — the new core transition API exceeds the constants/comment-only partition, while repairing the failing server fixture and runtime documentation touches expressly reserved files; `brief.md:92`, `brief.md:100`, `crates/core/src/multipart.rs:2796`. |
| C4 Verification (red→green) | FAIL | Focused tests independently turn green, but the existing deployed-retirement test turns from passing on the base to failing with the patch; this is a real integration regression, not a host or stale-base failure; `reviewer-evidence/green-restored.log:16`, `reviewer-evidence/ci-baseline.log:8`, `crates/server/tests/custodian_gc.rs:1531`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct clock identity propagation — a logical-clock context deletes a foreign wall tombstone and retains its own expired tombstone, defeating the promised foreign-clock hold; `crates/custodian/src/reaper.rs:392`, `crates/custodian/src/reconciliation.rs:203`, `reviewer-evidence/clock-probe.log:4`. |
| T1 Structure | PASS | Existing shared fencing and retirement seams keep state changes and cleanup obligations atomic, and calibration adds only backend dev-dependencies on core; `crates/custodian/src/reaper.rs:537`, `crates/custodian/src/reaper.rs:1356`, `crates/metadata-conformance/src/lib.rs:1608`. |
| T2 Shape | PASS | The rollback preserves attempt accounting and scopes retirement to the fenced epoch without changing persisted record shape; the independent D cases preserve later-epoch segments; `crates/core/src/multipart.rs:2797`, `crates/server/tests/multipart_reaper_exits.rs:593`, `crates/server/tests/multipart_reaper_exits.rs:679`. |
| T3 Runtime | NEEDS-HUMAN | Require live first-attempt calibration evidence before accepting the latency claim — FDB reports unavailable and TiKV has no configured endpoint, so this review has compilation plus source-reported timings, not exercised backend bounds; `reviewer-evidence/fdb-health.log:1`, `crates/core/src/multipart.rs:5975`, `brief.md:167`. |
| T4 Contribution | FAIL | Reconcile the living architecture before contribution closure — its building-block and runtime views still promise retained tombstones while the new crosscutting text promises expiry; the contribution-artifact audit itself is deferred/N/A until publish; `docs/design/architecture/05-building-block-view.md:246`, `docs/design/architecture/06-runtime-view.md:92`, `docs/design/architecture/08-crosscutting-concepts.md:114`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Strengthen the evidence for the required crash and race cases — the crash adapter permits later commits and the flip-first leg can pass without an intercepted rollback; strict new-window boundaries also lack killing tests; `crates/dst/tests/custodian.rs:8766`, `crates/dst/tests/custodian.rs:8555`, `gate-logs/C5-mutants.log:18`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept or revise the fifteen-minute recovery/retry windows and resulting admission pressure — 46 retained completion slots limit sustained completions and late retries lose their recorded result; `crates/custodian/src/reaper.rs:249`, `crates/custodian/src/reaper.rs:260`, `docs/design/architecture/08-crosscutting-concepts.md:114`. |

Source citations are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory. This report is advisory and does not gate acceptance.

**The clock defect is reproduced through the production reconciliation path.** `ReaperContext.clock_source` identifies its supplied clock, but `ReaperPolicy::default()` independently fixes the tombstone source to `wall`. The record-only reconciliation path passes that policy alongside the context's clock (`crates/custodian/src/reconciliation.rs:199`), and `Tombstone::passed` trusts the policy's identity (`crates/custodian/src/retirement.rs:430`). A standalone probe against the compiled, unchanged target seeds one wall-stamped and one logical-stamped Completed session, then runs a logical context at 10,000,000 ms. It prints `Blocked`, then `Satisfied`, then `Satisfied`; the foreign wall record is gone and the expired logical record remains. See `reviewer-evidence/clock_probe.rs:27` and `reviewer-evidence/clock-probe.log:1`; rerun with `./reviewer-evidence/clock-probe`. Bind the tombstone judgment to the actual context clock identity and cover both directions. The ordinary deployed wall context does not exhibit this mismatch. The C5 capability-probe smell rule does not independently fire: these timestamp/identity checks are intrinsic lifecycle predicates, not an optional-capability fallback.

**The full-CI failure needs a coordinated fixture update.** The retained-tombstone test publishes at 3,000 ms (`crates/server/tests/custodian_gc.rs:1470`) and later insists the record survives (`:1531`). Under the new policy its age exceeds fifteen minutes. Running that exact test with production changes stashed passes; restoring the changes makes it fail at the same assertion as both frozen CI attempts. Preserve its partial-fleet safety assertions while bringing its retention fixture into the intended window. `brief.md:100` reserves this file to child-2, so the ownership/scope decision is explicit rather than a request to change out-of-scope code silently. The contradictory architecture statements likewise need coordinated updates.

**Passing DST runs do not establish the claimed crash and race coverage.** These are builder-fixable evidence defects:

- `CrashAt::commit` injects only one error and forwards every later commit (`crates/dst/tests/custodian.rs:8764`). The reaper catches per-session errors and continues (`crates/custodian/src/reaper.rs:854`); reconciliation can then drain obligations in the same step. Recovery assertions inspect the store only after that whole step and six more passes (`crates/dst/tests/custodian.rs:8885`). Stop/cancel the step at the selected crash point, or prevent all subsequent I/O until explicit restart, and inspect the crash boundary state. A transient commit failure is not a process crash.
- The flip-first adapter explicitly publishes the held flip after reconciliation when no rollback fence was intercepted (`crates/dst/tests/custodian.rs:8555`). Its assertions merely establish that a pass ran and publication succeeded (`:8707`), so that fallback satisfies them. Require a witness that the flip actually landed between the rollback read and its attempted commit.
- The new terminal-window comparisons use strict `>` (`crates/custodian/src/reaper.rs:1084`, `:1101`; `crates/custodian/src/retirement.rs:431`). The added unit checks pin constant values, while the old boundary test still exercises the Open arms. Frozen mutants changing these comparisons to `>=` survive (`gate-logs/C5-mutants.log:18`, `:21`, `:31`). Add exact-boundary and neighboring-tick assertions for the new judgments. Do not infer from the mutant log that every survivor lacks coverage in the separate server acceptance suite.
- The new TiKV calibration namespace contains only PID and fixed shape/run tag (`crates/metadata-tikv/tests/conformance.rs:72`). Reusing a PID against a persistent cluster reuses the keys left by the previous measurement and fails `require_absent`; include a per-run identity as the FDB calibration already does (`crates/metadata-fdb/tests/conformance.rs:207`). This is a calibration reproducibility defect, not a demonstrated latency-bound violation.

**Independent reruns confirm the main exits and identify the limits of the evidence.** The disposable target's synthetic base is `f23e78c`, representing `6e44de924cc8a451a71ce7f7ed63aca76c67f509`. The base includes the prior reaper and retirement machinery; the reclaiming guard is present at `crates/custodian/src/retirement/marks.rs:307`. I stashed tracked changes, retained the new acceptance test, ran red, restored the stash, and ran green. `git apply --reverse --check ../patch.diff` succeeds on the restored target. No implementation was edited.

| Check | Independently observed result |
|---|---|
| `cargo test -p wyrd-server --test multipart_reaper_exits` | Before: 7 executed, 1 passed, 6 failed. After restoration: 7 passed. Logs: `reviewer-evidence/red.log:278`, `reviewer-evidence/green-restored.log:16`. |
| Existing deployed-retirement regression, selected with `--exact` | Base: 1 passed. Patch: 1 failed at `custodian_gc.rs:1531`; `reviewer-evidence/ci-regression.log:18`. |
| `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian reaper_` | 9 passed, 1 already-ignored known-gap test; both added scenarios run; `reviewer-evidence/dst.log:240`. |
| New completing-fence GC grace test, selected with `--exact` | 1 passed; `reviewer-evidence/gc-boundary.log:54`. |
| Formatting, `typos`, docs lint/render, xtask statics | Passed; render audited 101 pages, and statics found no forbidden global state; `reviewer-evidence/docs-render.log:2`, `reviewer-evidence/statics.log:3`. |
| Both `host-tikv` clippy commands | Passed under the repository's pinned Rust 1.96.0; `reviewer-evidence/host-tikv.log`. This compiles tests; it does not run live calibration. |
| Live backend availability | `fdbcli -C "$WYRD_FDB_CLUSTER_FILE" --exec 'status minimal'` reports the database unavailable; `WYRD_TIKV_PD_ENDPOINTS` is unset. Docker itself is available. |

Independent runtime tests used installed Rust 1.96.1; formatting and the successful TiKV clippy rerun used the repository's 1.96.0 pin. An initial clippy invocation from the scratch directory selected a toolchain without clippy; rerunning from the target resolved it (`reviewer-evidence/host-tikv-attempt1.log`). That attempt is a host/tool-selection caveat, not a patch defect.

The two F fixtures fail on the base at the prerequisite new Completing-to-Aborting fence (`crates/server/tests/multipart_reaper_exits.rs:920`, `:1007`), before evaluating the reclaiming guard. Their red results therefore do **not** show a broken #659 drain, despite the brief's expectation that F would be base-green. Similarly, E's base failure is its operator-status assertion, not evidence that the base changed a foreign session. D/G supply the substantive red reproduction. No builder rationale was consulted.

**Frozen gate evidence is adjudicated at its actual scope.** Instance-scoped wrappers were not treated as missing target files, and their output was read rather than blindly accepting the result labels.

| Gate | Assessment | Evidence |
|---|---|---|
| C4-ci | FAIL | Both attempts fail the same existing retention assertion; independently reproduced. Other logged checks, including DST, complete; `gate-logs/C4-ci.log:3063`, `:6257`. |
| C4-verify | PASS | Seven tests run; six fail on the base and all pass with the fix, matching the independent run; `gate-logs/C4-verify.log:10`. |
| C4-diff-cov | Unmeasured; base-state caveat | Its origin/main patch application fails before coverage runs. The supplied folded target compiles and runs; this is not evidence of a patch compilation defect; `gate-logs/C4-diff-cov.log:10`. |
| C5-mutants | Limited test evidence | 84 tested, 54 caught, 20 missed, 10 unviable. Reviewed the survivors against source; did not rerun the full mutation campaign or equate its surviving counters with logic defects; `gate-logs/C5-mutants.log:13`. |
| T4-batch-review | Confirmed findings require disposition | The 22 entries contain repeated clock, namespace and test-gap findings. Independently consolidated and grounded above; the raw count is not 22 distinct defects; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | Artifacts are absent by design and the substantive audit reruns at publish; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS for compilation | Frozen commands both finish successfully and independent pinned-toolchain rerun agrees; `gate-logs/host-tikv.log:214`. |

The GC constant duplication is real, but the deployed grace already has its own strict late-write bound at `crates/server/src/custodian.rs:124`; the new test passes with the shipped value. I do not classify this as a current unsafe-grace defect. Broader calibration shapes are outside the two explicitly prescribed by `brief.md:154`; the observed fixture cannot establish a universal worst-case latency bound. Concurrent terminal drainers are explicitly #660's scope (`brief.md:140`), so the batch review's requests to add those races are not raised here. The existing ignored slot-cycle deferral is likewise not reopened.

**Prior art was checked by affected path on both the folded base and published history, including closed work.** GitHub history for `crates/custodian/src/` at base `6e44de9` identifies the prior reaper (`427e5ec8`) and retirement slices (`f54db83b`, `3a392c8f`, `20e6e219`); `crates/metadata-conformance/src/lib.rs` has the established contract runner but no calibration. Closed-PR path searches returned 65 custodian matches and 6 conformance matches. The unmerged match, [PR #647](https://github.com/getwyrd/wyrd/pull/647), concerns segmented-map representation/consumers, not these terminal exits. A reaper-specific closed search found prerequisite work, not a rejected implementation of this change. Query URLs and results are saved in `reviewer-evidence/prior-art.json`. The main-branch reaper-path history is empty while the folded base has it, supporting the coverage gate's ordering caveat.

**Human sign-off still owes scope, real-backend calibration and operating-policy decisions.** The source's FDB 23/38 ms and TiKV 281/337 ms claims have no raw calibration output in the supplied gate logs. The default conformance test names in C4-ci are insufficient proof: the tests can return after environment/feature skips (`crates/metadata-fdb/tests/conformance.rs:55`, `crates/metadata-tikv/tests/conformance.rs:87`). On a host with healthy clusters, run the brief's `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance`, preserve every first-attempt measurement, and assess the single-node topology's relevance before accepting the claimed bound. No constants should be revalued here if a run exceeds 2,500 ms. No `INTEGRATION.md` was supplied or found in the target, so no additional §4 human-only list could be applied. Tier-1 disk-fault and Tier-2 kill/reconstruct observation remain appropriate supplementary durability follow-ups under `AGENTS.md`.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1017 (625.3: `Completing` exits, tombstone expiry, `G_orphan`, `B_ops`)

Verdict: **refuted on evidence, not on logic.** The new arms match 0016's reaper pseudocode
(`0016:2190-2215`), and I could not break the code paths themselves. But the tests never check
that a healthy Complete is left alone, so a reaper that rolls back every `Completing` session on
sight passes the whole suite (proved by mutation below). Separately, the gating CI run is red
because of this patch.

- NEEDS-HUMAN [human] — **C4-ci is red because of this patch, and the fix lies in a file this slice may not edit.** `crates/server/tests/custodian_gc.rs:1531` (child-2's file, which the brief forbids) stamps a tombstone at `completed_at_millis: 3_000` and drives the deployed loop on a clock about 10,000,000 ms later. Under the new default `Tombstone::Window` (`crates/custodian/src/reaper.rs:390-393`, `W_TOMBSTONE_MILLIS` = 900,000 at `:270`), that is past the window, so the record is deleted and the assertion "its tombstone — stays" fails on both attempts (`gate-logs/C4-ci.log:3063`, `:6257`). The same change makes three living-doc or code statements false, all in files outside this slice's list: `crates/server/src/custodian.rs:625-627` ("until it is, every `Completed` multipart session keeps its record"), `docs/design/architecture/05-building-block-view.md:246` ("an input the deployed custodian sets to keep it until `W_tombstone` is measured") and `06-runtime-view.md:92` ("the deployed custodian keeps every completed upload's record for now"). The rubric's docs-currency rule is a MUST. C4-diff-cov also says the patch no longer applies on `origin/main`. A human has to decide merge order with child-2, or grant the edit.

- NEEDS-HUMAN [impl] — **No test checks that a fresh `Completing` session is kept. I proved it by mutation.** In a scratch copy I changed `crates/custodian/src/reaper.rs:1101` to `if true || now.saturating_sub(fenced_at_millis) > policy.w_completing_millis {`, so every owned `Completing` session is rolled back on sight. Results: all 7 tests in `multipart_reaper_exits.rs` pass, all 40 `wyrd-custodian` unit tests pass, every `crates/custodian/tests/*` binary passes (including `gc.rs`), and both new DST rows (`reaper_completing_rollback_race_both_interleavings`, `reaper_crash_at_each_step_of_the_completing_exits`, run under `--cfg madsim`, 4 seeds) pass. Every `Completing` fixture is fenced two days back (`multipart_reaper_exits.rs:564,655,726,862,863,902,987`) or `W_completing + draw(1, HOUR)` back (`crates/dst/tests/custodian.rs`, both new rows). So the `pass.kept` branch at `reaper.rs:1104` is never reached. In production this mutant would make the next 30 s pass roll back every live Complete, and its flip would then fail on the bumped epoch. 0016's own falsification rows for this are not implemented: `0016:1027` and `:2266` say to fence a Complete long after its last part, move the clock past `W_completing` measured from that part but not from `fenced_at`, and check the reaper does not roll it back. Fix: add that leg (old `created_at` and old part, fence a few seconds old, session byte-identical after the one-shot), plus a unit row at exactly `W_completing` (kept) and `W_completing + 1` (rolled back), as `each_arm_judges_at_its_boundary` already does for the `Open` arms.

- NEEDS-HUMAN [impl] — **No test pins the edge of any new window.** C5's surviving `>`→`>=` mutants at `crates/custodian/src/reaper.rs:1084` (the `W_session` ceiling for `Completing`), `:1101` (`W_completing`) and `crates/custodian/src/retirement.rs:431` (`W_tombstone`) are real gaps. The G leg uses stamps 2 days old against 10 s old (`multipart_reaper_exits.rs:801`, `:805`), and the file says this choice is deliberate (`:74-77`). There is also no unit test of `Tombstone::passed` (`retirement.rs:419-435`). Fix: add a unit test with `now - completed_at` equal to W (kept), W + 1 (deleted), a foreign `clock_source` (kept) and a non-`Completed` state (false). The other C5 survivors (`completing_rollback -> None`, `==`/`<` at `:1084`/`:1101`, `!=` and `||` at `retirement.rs:430-431`) would be caught by the server tests, which the mutants run evidently doesn't include. I don't count those.

- NEEDS-HUMAN [human] — **`W_tombstone` = 15 min with `MAX_SESSIONS` = 46 (`crates/core/src/multipart.rs:6076`) limits the whole deployment to about 3 completed multipart uploads per minute.** The patch's own doc says so (`crates/custodian/src/reaper.rs:265-268`). Concrete case: `aws s3 cp --recursive` of 100 files of 10 MB each (the AWS CLI switches to multipart above 8 MB). The 47th `CreateMultipartUpload` gets `503 SlowDown` and stays refused until the first tombstones age out 15 minutes later. The window exists for a client re-sending a Complete whose answer it lost. SDK retries do that within seconds to tens of seconds, so a 1–2 minute window would cover them and raise the ceiling 7–15×. This is not a regression: the base keeps tombstones forever. But the value is this slice's to choose (0016 gives no numeric range), and it is a fitness-to-purpose decision for a human.

- NEEDS-HUMAN [human] — **The `G_orphan` bound sits on a copy, not on the value production uses.** `crates/custodian/src/gc.rs:304` adds `G_ORPHAN_MILLIS = 60_000` with compile-time checks, but no production code reads it. The deployed GC uses `GC_GRACE_WINDOW_MILLIS = crate::cli::LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`), which is already held strictly above the late-write deadline (`:124`). The new boundary test injects the copy (`crates/custodian/tests/gc.rs:1862`), and its doc comment says "GC then runs at the deployed grace, [`G_ORPHAN_MILLIS`]" (`:1764`), which is false. If `LEASE_TTL_MILLIS` changed, the test and both checks would stay green. Linking the two needs either a server edit or a `pub` re-export (`LEASE_TTL_MILLIS` is `pub(crate)`, `crates/server/src/cli.rs:80`), and server files belong to child-2. So this is a §6 decision. At minimum the false comment should change. The batch review saw the separate constant; the false comment and the server-ownership block are new here.

- NEEDS-HUMAN [impl] — **The calibration reports the operation count it was asked for, not the one it sent.** `crates/metadata-conformance/src/lib.rs:1655` records `BatchTiming { ops, .. }` straight from the argument, and nothing checks that the batch built from `keys = ops / 2` (`:1639`) really has `ops` operations. The unit test at `:1808` calls `calibration_batch(&store, batch, OPS / 2, BYTES)`, repeating the halving instead of testing it. That is why C5's `/`→`*` mutant at `:1639` survives. Under that mutant a 2,000-operation batch would print "500 operations" and could be written into `SLOWEST_OP_MILLIS`'s doc as the measured figure. Fix: inside `calibrate_batch_ops`, count `built.preconditions + puts + deletes` and assert it equals `ops`. Also, `crates/core/src/multipart.rs:5975-5976` describes batch (i) as "500 `require_absent` puts", but it is 250 `require_absent` checks plus 250 puts.

- NEEDS-HUMAN [human] — **No evidence this round backs the measured `B_ops` figures.** The doc at `crates/core/src/multipart.rs:5975-5982` and `docs/design/architecture/08-crosscutting-concepts.md:114` give 23/38 ms on FDB 7.3.77 and 281/337 ms on TiKV 8.5.1. No FDB or TiKV conformance log exists in `gate-logs/`, and `host-tikv` only compiles. As the brief says, Eduard should re-run `cargo xtask fdb-conformance` and `tikv-conformance` and record the first-attempt figures in SUMMARY §9 before merge.

- Claim in `check-gates.json`: the C4-verify row says "7 test(s) ran red", but the red leg shows `1 passed; 6 failed` (`gate-logs/C4-verify.log:288`). The test that passed is `profile_mismatch_refuses_the_completing_arms`, which the brief already predicted would pass on the base. It's an overstatement, not a defect.

- Attacks I tried that did not land:
  - **Rollback destroys part data.** It doesn't: `retire:records:` only deletes records (`crates/custodian/src/retirement.rs:751`, no marks), and D asserts `orphan:` stays empty.
  - **Over-age sessions get rolled back first.** They don't: the marks carry token `s:<id>:1` (`multipart_reaper_exits.rs:683-690`), so negation (15) is real.
  - **`lost_any` misclassifies a lost fence when the second obligation key is taken** (`reaper.rs:1442-1457`). It classifies correctly.
  - **Profile mismatch still allows tombstone expiry.** It doesn't: `Refused` skips the retirement leg (`crates/custodian/src/reconciliation.rs:274-277`).
  - **Flip/rollback race.** Exact-bytes preconditions on both sides prevent it.
  - **The hard-coded `WALL_CLOCK_SOURCE` tag in the default policy** (the batch review's clock finding, `reaper.rs:392`) is latent only: both production `ReaperContext`s pass `WALL_CLOCK_SOURCE` (`crates/server/src/custodian.rs:478`, `:637`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:392`: The default tombstone policy hard-codes `wall` independently of `ReaperContext::clock_source`. With a logical-clock context and the default policy, the reaper holds a wall-stamped Completed session as foreign, but `crates/custodian/src/reconciliation.rs:201` still passes that logical clock and wall-tagged policy to retirement. `crates/custodian/src/retirement.rs:430` can then expire that foreign tombstone and release its slot; conversely, local logical-clock tombstones never expire. Derive the expiry tag from the clock-owning context, or fail closed on a mismatch, and cover both cases.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8766`: `CrashAt` injects one commit error and immediately resumes forwarding subsequent commits. The reaper absorbs fence errors (`crates/custodian/src/reaper.rs:1430`), and reconciliation still runs retirement, so the alleged crash-after-fence run can drain obligations before the test begins recovery. This tests a transient failure rather than the durable state at a process crash. Stop the reconciliation future at the injection point, or latch the store unavailable until restart, and assert that no later mutation lands before recovery.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8555`: The flip-first race silently publishes through a fallback when `FlipGate` never intercepts a rollback fence. Its assertions at `crates/dst/tests/custodian.rs:8708` only establish that a reaper pass ran and publication succeeded, which also holds when this interleaving never occurs. Require the gate to have fired and assert that the intercepted rollback returns `Conflict`; otherwise this half of the test can pass without exercising the claimed race.

- NEEDS-HUMAN [impl] — `crates/metadata-tikv/tests/conformance.rs:72`: Calibration namespaces contain only the PID and deterministic shape/run tag, and successful runs leave their keys behind. Reusing a PID against the same cluster, including separate containers with identical PIDs, reuses those keys and makes `require_absent` fail before latency can be measured. Add a per-invocation unique namespace component, as the new FDB calibration already does at `crates/metadata-fdb/tests/conformance.rs:201`.

- NEEDS-HUMAN [human] — `crates/custodian/src/gc.rs:304`: The new `G_ORPHAN_MILLIS` duplicates the deployed grace without supplying it: production still derives its value independently at `crates/server/src/custodian.rs:115`, while the new boundary test injects only the new constant at `crates/custodian/tests/gc.rs:1862`. Both values currently equal 60 seconds, but either can change while the test continues certifying a different reclaim tick. Reuse one definition or mechanically assert their equality. Wiring the server requires coordination with child-2 because the brief explicitly excludes that file from this slice.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide Plan re-entry and sibling ownership — the new core transition API exceeds the constants/comment-only partition, while repairing the failing server fixture and runtime documentation touches expressly reserved files; `brief.md:92`, `brief.md:100`, `crates/core/src/multipart.rs:2796`.
- [ ] C5 Causal adequacy — Correct clock identity propagation — a logical-clock context deletes a foreign wall tombstone and retains its own expired tombstone, defeating the promised foreign-clock hold; `crates/custodian/src/reaper.rs:392`, `crates/custodian/src/reconciliation.rs:203`, `reviewer-evidence/clock-probe.log:4`.
- [ ] T3 Runtime — Require live first-attempt calibration evidence before accepting the latency claim — FDB reports unavailable and TiKV has no configured endpoint, so this review has compilation plus source-reported timings, not exercised backend bounds; `reviewer-evidence/fdb-health.log:1`, `crates/core/src/multipart.rs:5975`, `brief.md:167`.
- [ ] T5 Judgment — Strengthen the evidence for the required crash and race cases — the crash adapter permits later commits and the flip-first leg can pass without an intercepted rollback; strict new-window boundaries also lack killing tests; `crates/dst/tests/custodian.rs:8766`, `crates/dst/tests/custodian.rs:8555`, `gate-logs/C5-mutants.log:18`.
- [ ] Validation — fitness-to-purpose — Accept or revise the fifteen-minute recovery/retry windows and resulting admission pressure — 46 retained completion slots limit sustained completions and late retries lose their recorded result; `crates/custodian/src/reaper.rs:249`, `crates/custodian/src/reaper.rs:260`, `docs/design/architecture/08-crosscutting-concepts.md:114`.
- [ ] **C4-ci is red because of this patch, and the fix lies in a file this slice may not edit.** `crates/server/tests/custodian_gc.rs:1531` (child-2's file, which the brief forbids) stamps a tombstone at `completed_at_millis: 3_000` and drives the deployed loop on a clock about 10,000,000 ms later. Under the new default `Tombstone::Window` (`crates/custodian/src/reaper.rs:390-393`, `W_TOMBSTONE_MILLIS` = 900,000 at `:270`), that is past the window, so the record is deleted and the assertion "its tombstone — stays" fails on both attempts (`gate-logs/C4-ci.log:3063`, `:6257`). The same change makes three living-doc or code statements false, all in files outside this slice's list: `crates/server/src/custodian.rs:625-627` ("until it is, every `Completed` multipart session keeps its record"), `docs/design/architecture/05-building-block-view.md:246` ("an input the deployed custodian sets to keep it until `W_tombstone` is measured") and `06-runtime-view.md:92` ("the deployed custodian keeps every completed upload's record for now"). The rubric's docs-currency rule is a MUST. C4-diff-cov also says the patch no longer applies on `origin/main`. A human has to decide merge order with child-2, or grant the edit.
- [ ] **No test checks that a fresh `Completing` session is kept. I proved it by mutation.** In a scratch copy I changed `crates/custodian/src/reaper.rs:1101` to `if true || now.saturating_sub(fenced_at_millis) > policy.w_completing_millis {`, so every owned `Completing` session is rolled back on sight. Results: all 7 tests in `multipart_reaper_exits.rs` pass, all 40 `wyrd-custodian` unit tests pass, every `crates/custodian/tests/*` binary passes (including `gc.rs`), and both new DST rows (`reaper_completing_rollback_race_both_interleavings`, `reaper_crash_at_each_step_of_the_completing_exits`, run under `--cfg madsim`, 4 seeds) pass. Every `Completing` fixture is fenced two days back (`multipart_reaper_exits.rs:564,655,726,862,863,902,987`) or `W_completing + draw(1, HOUR)` back (`crates/dst/tests/custodian.rs`, both new rows). So the `pass.kept` branch at `reaper.rs:1104` is never reached. In production this mutant would make the next 30 s pass roll back every live Complete, and its flip would then fail on the bumped epoch. 0016's own falsification rows for this are not implemented: `0016:1027` and `:2266` say to fence a Complete long after its last part, move the clock past `W_completing` measured from that part but not from `fenced_at`, and check the reaper does not roll it back. Fix: add that leg (old `created_at` and old part, fence a few seconds old, session byte-identical after the one-shot), plus a unit row at exactly `W_completing` (kept) and `W_completing + 1` (rolled back), as `each_arm_judges_at_its_boundary` already does for the `Open` arms.
- [ ] **No test pins the edge of any new window.** C5's surviving `>`→`>=` mutants at `crates/custodian/src/reaper.rs:1084` (the `W_session` ceiling for `Completing`), `:1101` (`W_completing`) and `crates/custodian/src/retirement.rs:431` (`W_tombstone`) are real gaps. The G leg uses stamps 2 days old against 10 s old (`multipart_reaper_exits.rs:801`, `:805`), and the file says this choice is deliberate (`:74-77`). There is also no unit test of `Tombstone::passed` (`retirement.rs:419-435`). Fix: add a unit test with `now - completed_at` equal to W (kept), W + 1 (deleted), a foreign `clock_source` (kept) and a non-`Completed` state (false). The other C5 survivors (`completing_rollback -> None`, `==`/`<` at `:1084`/`:1101`, `!=` and `||` at `retirement.rs:430-431`) would be caught by the server tests, which the mutants run evidently doesn't include. I don't count those.
- [x] **`W_tombstone` = 15 min with `MAX_SESSIONS` = 46 (`crates/core/src/multipart.rs:6076`) limits the whole deployment to about 3 completed multipart uploads per minute.** The patch's own doc says so (`crates/custodian/src/reaper.rs:265-268`). Concrete case: `aws s3 cp --recursive` of 100 files of 10 MB each (the AWS CLI switches to multipart above 8 MB). The 47th `CreateMultipartUpload` gets `503 SlowDown` and stays refused until the first tombstones age out 15 minutes later. The window exists for a client re-sending a Complete whose answer it lost. SDK retries do that within seconds to tens of seconds, so a 1–2 minute window would cover them and raise the ceiling 7–15×. This is not a regression: the base keeps tombstones forever. But the value is this slice's to choose (0016 gives no numeric range), and it is a fitness-to-purpose decision for a human.
- [ ] **The `G_orphan` bound sits on a copy, not on the value production uses.** `crates/custodian/src/gc.rs:304` adds `G_ORPHAN_MILLIS = 60_000` with compile-time checks, but no production code reads it. The deployed GC uses `GC_GRACE_WINDOW_MILLIS = crate::cli::LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`), which is already held strictly above the late-write deadline (`:124`). The new boundary test injects the copy (`crates/custodian/tests/gc.rs:1862`), and its doc comment says "GC then runs at the deployed grace, [`G_ORPHAN_MILLIS`]" (`:1764`), which is false. If `LEASE_TTL_MILLIS` changed, the test and both checks would stay green. Linking the two needs either a server edit or a `pub` re-export (`LEASE_TTL_MILLIS` is `pub(crate)`, `crates/server/src/cli.rs:80`), and server files belong to child-2. So this is a §6 decision. At minimum the false comment should change. The batch review saw the separate constant; the false comment and the server-ownership block are new here.
- [ ] **The calibration reports the operation count it was asked for, not the one it sent.** `crates/metadata-conformance/src/lib.rs:1655` records `BatchTiming { ops, .. }` straight from the argument, and nothing checks that the batch built from `keys = ops / 2` (`:1639`) really has `ops` operations. The unit test at `:1808` calls `calibration_batch(&store, batch, OPS / 2, BYTES)`, repeating the halving instead of testing it. That is why C5's `/`→`*` mutant at `:1639` survives. Under that mutant a 2,000-operation batch would print "500 operations" and could be written into `SLOWEST_OP_MILLIS`'s doc as the measured figure. Fix: inside `calibrate_batch_ops`, count `built.preconditions + puts + deletes` and assert it equals `ops`. Also, `crates/core/src/multipart.rs:5975-5976` describes batch (i) as "500 `require_absent` puts", but it is 250 `require_absent` checks plus 250 puts.
- [ ] **No evidence this round backs the measured `B_ops` figures.** The doc at `crates/core/src/multipart.rs:5975-5982` and `docs/design/architecture/08-crosscutting-concepts.md:114` give 23/38 ms on FDB 7.3.77 and 281/337 ms on TiKV 8.5.1. No FDB or TiKV conformance log exists in `gate-logs/`, and `host-tikv` only compiles. As the brief says, Eduard should re-run `cargo xtask fdb-conformance` and `tikv-conformance` and record the first-attempt figures in SUMMARY §9 before merge.
- [ ] `crates/custodian/src/reaper.rs:392`: The default tombstone policy hard-codes `wall` independently of `ReaperContext::clock_source`. With a logical-clock context and the default policy, the reaper holds a wall-stamped Completed session as foreign, but `crates/custodian/src/reconciliation.rs:201` still passes that logical clock and wall-tagged policy to retirement. `crates/custodian/src/retirement.rs:430` can then expire that foreign tombstone and release its slot; conversely, local logical-clock tombstones never expire. Derive the expiry tag from the clock-owning context, or fail closed on a mismatch, and cover both cases.
- [ ] `crates/dst/tests/custodian.rs:8766`: `CrashAt` injects one commit error and immediately resumes forwarding subsequent commits. The reaper absorbs fence errors (`crates/custodian/src/reaper.rs:1430`), and reconciliation still runs retirement, so the alleged crash-after-fence run can drain obligations before the test begins recovery. This tests a transient failure rather than the durable state at a process crash. Stop the reconciliation future at the injection point, or latch the store unavailable until restart, and assert that no later mutation lands before recovery.
- [ ] `crates/dst/tests/custodian.rs:8555`: The flip-first race silently publishes through a fallback when `FlipGate` never intercepts a rollback fence. Its assertions at `crates/dst/tests/custodian.rs:8708` only establish that a reaper pass ran and publication succeeded, which also holds when this interleaving never occurs. Require the gate to have fired and assert that the intercepted rollback returns `Conflict`; otherwise this half of the test can pass without exercising the claimed race.
- [ ] `crates/metadata-tikv/tests/conformance.rs:72`: Calibration namespaces contain only the PID and deterministic shape/run tag, and successful runs leave their keys behind. Reusing a PID against the same cluster, including separate containers with identical PIDs, reuses those keys and makes `require_absent` fail before latency can be measured. Add a per-invocation unique namespace component, as the new FDB calibration already does at `crates/metadata-fdb/tests/conformance.rs:201`.
- [ ] `crates/custodian/src/gc.rs:304`: The new `G_ORPHAN_MILLIS` duplicates the deployed grace without supplying it: production still derives its value independently at `crates/server/src/custodian.rs:115`, while the new boundary test injects only the new constant at `crates/custodian/tests/gc.rs:1862`. Both values currently equal 60 seconds, but either can change while the test continues certifying a different reclaim tick. Reuse one definition or mechanically assert their equality. Wiring the server requires coordination with child-2 because the brief explicitly excludes that file from this slice.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: cargo test --workspace --exclude wyrd-dst: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 22 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- [ ] size backstop — this slice is behaving oversized: patch is 148 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the slice whole: production logic is ~9 KB (the 148 KB is tests, comments and diff context), so this is not a split. C4-ci is red because tombstone expiry ages out the fixture at `crates/server/tests/custodian_gc.rs:1470`/`:1531` (passes on base, fails with the patch). NARROW SCOPE EXCEPTION (granted at sign-off): this slice MAY edit, in child-2's (#1016) files, ONLY (1) the tombstone timestamp/age in `custodian_gc.rs`'s deployed-retirement test so the completed upload is inside the window when asserted — keep every partial-fleet safety assertion, and keep the change base-neutral — and (2) the tombstone wording in `06-runtime-view.md` (and the matching stale "retained tombstone" text in `05-building-block-view.md`) so the living docs agree with `08-crosscutting-concepts.md`. Nothing else in `crates/server/*`. Expect a small merge conflict with #1016; whichever lands second resolves it. Also justify the `core/multipart.rs` transition API beyond constants/doc in build-notes.md, or move it into the custodian. Fix in place: - Clock identity: bind the tombstone judgment to `ReaperContext::clock_source`, not the hard-coded `wall` in `ReaperPolicy::default()` (`reaper.rs:392`, `retirement.rs:430`); test both directions (a logical context holds a wall tombstone as foreign and expires its own). - DST: `CrashAt` must stop all I/O at the crash point (a real crash, not one transient error) and assert the boundary state; the flip-first race must witness the flip landing between the rollback read and its commit, with no fallback publish. - Boundary tests: exact-tick and neighbour-tick tests for `W_session` (Completing), `W_completing` and `W_tombstone` (`reaper.rs:1084`, `:1101`, `retirement.rs:431`); a test that a fresh `Completing` session is kept. - `G_orphan`: have production read the bound the test checks (no unread copy at `gc.rs:304` vs `server/src/custodian.rs:115`); since server is child-2's, if wiring needs a server edit, raise it as a §6 item rather than editing. - Calibration: per-run identity in the TiKV namespace (as FDB has); record the ops actually sent; keep the raw first-attempt FDB/TiKV output in build-notes, or state plainly that no live cluster was available — do not quote figures without it. Decided at sign-off: set `W_TOMBSTONE_MILLIS` to 2 minutes (2 * MINUTE_MILLIS), not 15 — covers normal SDK Complete retries and lifts the ceiling from ~3 to ~23 completed multipart uploads/min with MAX_SESSIONS = 46. Keep `W_TOMBSTONE < W_OPEN`; update the doc comment's throughput figure, the unit check (`assert_eq!(W_TOMBSTONE_MILLIS, 120_000)`), the tests and `08-crosscutting-concepts.md` to match, and size the `custodian_gc.rs` fixture fix against the 2-minute window.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
