# Result — issue 1018 / mpu-operator-abort

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #625's reaper deliberately **skips** a session whose `clock_source` it does not
  own (`0016:1889-1907`, `:1984-1998`). For a foreign-clocked `Open` or `Completing` session, no
  actor in the system can move it. It keeps its records, its staged bytes and its admission slot
  forever. `MAX_SESSIONS` of them is a permanent `503 SlowDown` on `CreateMultipartUpload` with
  no exit. 0016 names the operator as the exit (`0016:255-262`), but no operator verb exists.
  `crates/server/src/cli.rs` has no multipart or session-management subcommand
  (`:412-428`).
- Success criterion: one new test file (see `Test file`), every leg asserted on durable
  store state or on the subprocess's exit status and stderr.
  **(A) Operator abort of an `Open` session.** Seed an `Open@E` session with staged parts.
  - `wyrd mpu abort --upload-id <ID> --data-dir <DIR>` exits 0.
  - The session is left `Aborting@E+1` with `retire:bytes:{session}` installed (the Open
    door's payload, `0016:664`).
  - A following pass of #625's operator one-shot, invoked as argv, tears it down to zero:
    records gone, `orphan:` marks written, `mpuctl.count` decremented. This shows the verb hands
    off to the ordinary clock-free path instead of growing a second teardown.
  **(B) Abort of a `Completing` session ALSO retires that attempt's segments.** Seed a
  `Completing@E` session that has already written `seg:<g>:<E>:*` records, plus a decoy
  `seg:<g>:<E+2>:*` range.
  - The verb leaves the session `Aborting@E+1` **and** `retire:records:{seg:<g>:<E>}` present.
  - `retire:bytes:{session, parts}` is installed too (`0016:665`).
  - The decoy later-epoch range is **not** named by any obligation (F18).
  - A following one-shot pass drains the named range empty and leaves the decoy intact.
  **(C) Abort refuses where 0016 says it must, and the refusal is DISCRIMINATING.**
  - A `Completed` session (tombstone) and an absent session are **not** abortable.
  - An already-`Aborting` session is idempotent: exit 0, no second fence, no double obligation,
    epoch unchanged.
  - **"Store unchanged + non-zero" is not an oracle here.** On the base, an unknown command
    also leaves the store untouched and exits non-zero (**2**, `cli.rs:419-423`).
  - Each refusal therefore asserts the **handled** exit status **1** (`ExitCode::FAILURE`,
    `cli.rs:429-433`) **and** a stderr message naming the session's state. For `Completed`, the
    message says abort does not apply to a terminal session. (The pointer to the expiry verb is
    added and asserted by the sibling child.)
  **One-batch atomicity is REQUIRED but deliberately NOT in the binding criterion.**
  - Both doors MUST commit their state change and their obligation(s) in one batch
    (`0016:664-665`). For the `Completing` door, no interleaving may leave a fenced session whose
    segments have no reclaimer (the X57 hole).
  - An end-state scan cannot tell one batch from two, so Do owes atomicity outside C4, in two
    parts:
    - (i) seeded DST cases **appended to the existing** `crates/dst/tests/custodian.rs`, one per
      door, that interrupt between the two commits and show no reachable unreclaimed state;
    - (ii) the batch construction, stated in `build-notes.md` for the reviewer.
  - If Do cannot construct the interleaving, say so and record the property as a code-read
    obligation. Do not claim a test proves it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - **The `wyrd mpu` subcommand group** in `crates/server/src/cli.rs`: a dispatch arm, a
    `usage()` line, `ParsedArgs` / `VALUELESS_FLAGS` conventions (`:2244-2284`), and
    `cmd_custodian`'s stray-positional refusal and its "a value after a valueless flag does NOT
    disable it" rule (`:913-930`).
  - **The `abort` verb.** Exact argv, verbatim: `wyrd mpu abort --upload-id <ID> --data-dir
    <DIR>`, plus the role's standard `--metadata-backend <B>` and log flags. Flags only, exactly
    one `--upload-id`, no clock-source flag.
  - **The fence.** It fences `Open@E → Aborting@E+1` or `Completing@E → Aborting@E+1` in **one**
    batch preconditioned on the session record's exact bytes (`WriteBatch::require`,
    `crates/traits/src/lib.rs:825-843`). **The two doors carry different payloads and must not
    be conflated:**
    - `Open` gets 1 put + `retire:bytes:{session}` (`0016:664`);
    - `Completing` gets 1 put + `retire:bytes:{session, parts}` + `retire:records:{seg:<g>:<E>}`
      for **that fence epoch only** (`0016:665`).
  - **Reuse, don't rewrite.** The `Completing` door is "one shape for all three doors" with the
    restore fence and the `W_session` edge: **reuse** the path #658 landed. Do not write a third
    copy.
  - **Refusals.** Refuse `Completed` / absent sessions with the handled error. Be idempotent
    against an already-`Aborting` session.
  - **Tests and docs:**
    - the DST cases appended to `crates/dst/tests/custodian.rs`;
    - the living architecture docs (`docs/design/architecture/06-runtime-view.md`,
      `08-crosscutting-concepts.md`), updated for the `mpu` group and the abort verb, including
      grant term (iii). This is a merge requirement (`AGENTS.md` "Docs currency").

  **Out of scope:**
  - the `expire` verb, the session-record field, and **any** edit under `crates/custodian/src/`
    (the sibling child);
  - the alarm;
  - the reaper's windows, clock guard and teardown (#625);
  - the record classes and state machine (#693/#658/#659);
  - any S3 wire-visible verb (#508);
  - the 0008 management plane;
  - FU-2 / FU-3 / FU-4;
  - any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016.

  Anything else is a Check §6 item, not a silent edit to a layer below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: Fixed
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 32 mutants tested in 12m: 2 missed, 18 caught, 11 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.83s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

For #1018’s operator abort of foreign-clocked `Open`/`Completing` uploads, the core exit works, but operator reporting, input handling, and regression coverage need correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Restored admission capacity, exact-attempt reclamation, and discriminating refusals are falsifiable acceptance criteria; the operator exit is required by the design, not a contested workaround (`brief.md:29`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:255`). |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new test retained, all four tests executed and failed on unknown `mpu`, rather than compilation or zero tests (`review-red-green.log:11`, `review-red-green.log:235`). |
| C3 Change | PASS | The change stays within the abort exit and supporting reuse; expiry/alarm/schema work remains outside this patch, and the living docs record host authority and management-plane migration (`crates/core/src/multipart/abort.rs:381`; `docs/design/architecture/06-runtime-view.md:96`). |
| C4 Verification (red→green) | PASS | Independent restoration produced four passing subprocess tests; four DST tests and the spelling/format/docs checks also passed; coverage remains unmeasured because its gate could not apply the stack to `origin/main` (`review-red-green.log:271`; `review-dst.log:246`; `review-scanners.log:1`; `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve uncertainty about who fenced and whether this invocation wrote anything—independent probes returned `Fenced` for a rival’s commit and `NoSuchUpload` after this invocation’s fence had landed and been reclaimed (`crates/core/src/multipart/abort.rs:464`; `crates/core/src/multipart/abort.rs:468`; `crates/server/src/cli.rs:3202`). |
| T1 Structure | PASS | Correctness stays behind the existing metadata trait and atomic batch builder, with no additional teardown authority or correctness clock; the Completing door reuses the landed teardown constructor (`crates/core/src/multipart/abort.rs:90`; `crates/core/src/multipart/abort.rs:403`; `crates/core/src/multipart/abort.rs:487`). |
| T2 Shape | FAIL | Malformed argv can bypass the promised unknown-flag refusal and mutate an unintended directory named `--bogus`; the real binary reproduced the durable change (`crates/server/src/cli.rs:3094`; `crates/server/src/cli.rs:3102`; `crates/server/src/cli.rs:3248`). |
| T3 Runtime | FAIL | The printed teardown command loses the selected store; following it reported “satisfied” on the default store while the intended upload still occupied an admission slot (`crates/server/src/cli.rs:3173`; `review-probes.log:58`). |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art by affected path—the supplied target exposes only one synthetic commit and no remote/history audit, so previous rejection or duplicate work cannot be settled from this bundle (`review-prior-art.log:1`; `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Strengthen the regression oracle: parser mutants survive, subprocess fixtures inherit backend selection, and cancellation tests omit late commits and the second-obligation conflict case; these gaps conceal operator-visible failures (`gate-logs/C5-mutants.log:13`; `crates/server/tests/multipart_operator_abort.rs:151`; `crates/dst/tests/custodian.rs:8642`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the interim host-access workflow and coordinated release with the reaper; assess the unexercised production late-commit case, whose assurance currently rests on the atomic-batch code and a cancellation model that cannot apply a dropped commit later (`brief.md:169`; `crates/dst/tests/support/mod.rs:344`; `crates/dst/tests/custodian.rs:8642`). |

This is an advisory review. The binding red→green result is genuine, but it does not discharge the operator-visible defects below. Source paths in this report are relative to `$PDCA_TARGET`; reviewer and frozen logs are relative to the review directory. The target matched the patch, and `git apply --reverse --check ../patch.diff` succeeded after restoration (`review-red-green.log:274`).

1. **Operator outcomes overstate the evidence after an unknown commit.** A rival can produce byte-identical `Aborting` bytes. `Doubt::landed` treats those bytes as proof that this invocation committed, and the new CLI emits `operator-abort-fenced` (`crates/core/src/multipart/abort.rs:604`; `crates/server/src/cli.rs:3176`). An independent wrapper over real redb let a separate abort commit while rejecting this invocation’s batch with `CommitUnknownResult`; both invocations reported `Fenced` (`review-probes.log:3`). Separately, a wrapper applied this invocation’s fence and modeled the legal reclaimed state before its settling read. The outcome was `NoSuchUpload` (`review-probes.log:5`), which the new CLI renders as “nothing written” and exit 1. A later absence establishes that the old fence cannot still land; it does not establish that it never landed. Preserve that distinction in the operator outcome/reporting and add regressions. The helper’s older client-abort behavior is not a request to expand this PR; the new operator attribution and no-write claims are the affected surface.

2. **Malformed arguments and the follow-up hint can select the wrong store.** `ParsedArgs` consumes the token after a valued flag even when that token is another option, and `unknown_flag` sees only map keys. With an existing `--bogus/meta.redb`, the real invocation `wyrd mpu abort --upload-id ID --data-dir --bogus --metadata-backend redb` exited 0 and durably changed `Open@0` to `Aborting@1` (`review-probes.log:8`). Validate missing values/option tokens before store access. Independently, the literal hint `wyrd custodian --reap-abandoned-uploads` omits both `--data-dir` and an explicitly selected backend. In a two-store probe it exited 0 and reported the default store satisfied, while the selected store retained `Aborting@1` and `mpuctl.count = 1`. Preserve the effective store selection in runnable follow-up guidance (`crates/server/src/cli.rs:3173`).

3. **The submitted tests miss relevant failure cases.** The frozen mutation run reports two survivors. Replaying the exact parser mutations in an isolated copy showed that removing `unknown_flag` accepts an unsupported clock option, and changing the duplicate-name comparison allows a repeated upload ID after a different option has already repeated (`review-parser-mutants.log:1`; `crates/server/src/cli.rs:3240`). These are non-equivalent mutations, not cosmetic survivors. The subprocess helpers also omit `--metadata-backend redb`; running the unchanged Completing test with a harmless invalid `WYRD_METADATA_BACKEND` failed before aborting the fixture (`review-probes.log:30`). Explicitly isolate both subprocess helpers from inherited backend selection.

   The seeded cancellation tests do exercise both sides of a commit and passed independently, but dropping the simulated commit future before `apply` prevents a later write; there is no surviving server-side operation in this model (`crates/dst/tests/support/mod.rs:344`). Add a schedule where an already-submitted commit outlives the operator and verify reclamation obligations after it lands. Also retain a regression for a Completing fence whose **records** obligation alone is occupied: the independent probe returned the correct `ObligationKeyTaken` and preserved the whole store (`review-probes.log:14`), so this is a submitted-coverage gap, not an observed implementation failure. The frozen logs do not expose the brief’s five named negation runs, including the split-batch experiment; this review does not claim those experiments were reproduced.

The 15 entries in the batch-review log collapse to eight distinct concerns. This review confirms the rival attribution, post-drain no-write claim, swallowed option, incomplete teardown hint, backend-dependent fixtures, second-key coverage gap, and late-commit coverage gap. It rejects environment-based backend precedence **by itself** as a new defect: the brief explicitly requests the standard backend selection, and the established resolver gives an explicit backend flag precedence over the environment (`brief.md:103`; `crates/server/src/cli.rs:290`). The unsafe fixture inheritance and missing backend in the hint remain separate, actionable findings. No recorded tracked-issue deferral was reopened, and no DCO finding was inferred from commit inspection.

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | PASS | Frozen output shows the real typos check, docs render/link audit, Rust checks, and DST suite ending successfully (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:5090`); independent fmt, typos, docs lint, and render also passed (`review-scanners.log:1`). The full workspace CI suite was not independently repeated. |
| C4-verify | PASS | Independent stash/test/pop/test: 4 failed, then 4 passed; both builds compiled the same new test (`review-red-green.log:235`, `review-red-green.log:271`). |
| C4-diff-cov | N/A | No coverage measurement was produced: the wrapper could not apply the stack to `origin/main` (`gate-logs/C4-diff-cov.log:10`). This is a base-selection caveat, not a demonstrated patch compilation defect. |
| C5-mutants | FAIL | Frozen sweep: 2 missed, 18 caught, 11 unviable, 1 timeout (`gate-logs/C5-mutants.log:16`). Both survivors have independently demonstrated behavioral differences in the extracted parser (`review-parser-mutants.log:1`); the full 32-mutant sweep was not repeated. |
| T4-batch-review | FAIL | The frozen row contains substantive unresolved findings as well as duplicates; dispositions are recorded above (`gate-logs/T4-batch-review.log:10`). Its external result-file path was not followed. |
| T4-contribution | N/A | Deferred by design: contribution artifacts are drafted after Check, and their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen output shows successful TiKV and server `tikv,etcd` clippy builds, ending normally (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`); this establishes compilation, not a live-cluster execution. |

The brief’s named external tools, `typos` and `docs-renderer`, were actually exercised and passed. Independent DST execution used `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test --offline --locked -p wyrd-dst --test custodian operator_abort_`; all four selected tests passed. There is no capability-probe symptom guard in this diff. Both doors reuse the existing all-parts retirement representation, and only Completing adds its exact-attempt records obligation (`crates/core/src/multipart.rs:2741`; `crates/core/src/multipart.rs:2767`).

Affected-path history was investigated for all seven changed files. The available copy contains one synthetic pre-fix commit, no remote, and no closed/rejected-work records; the supplied logs record no prior-art audit (`review-prior-art.log:1`). No `INTEGRATION.md` is supplied in the bundle or target, so no additional project-specific human-only items can be inferred. Fitness-to-purpose and the brief’s coordinated-release sign-off remain human decisions.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1018 operator abort (`wyrd mpu abort`)

**Bottom line:** I could not refute the fence itself. The one-batch claim, the X57 claim (no
fenced session with segments nothing reclaims), the F18 claim (only the fence epoch's segment
range) and the refusal logic all held up, including against a negation I built and ran myself.
What I could refute is narrower: test strength (two surviving mutants, one new path untested)
and one false operator message. None of it is a data-safety hole.

## Refutation attempts that failed (the fix holds)

- **Red→green evidence.** I re-ran GREEN in scratch: `cargo test -p wyrd-server --test
  multipart_operator_abort` passed 4 of 4. RED (`gate-logs/C4-verify.log`): 4 tests ran and
  4 failed, so it is not a zero-test run. Each failed on the handled-exit assertion (got exit 2
  `unknown command mpu`, expected 0 or 1) at
  `crates/server/tests/multipart_operator_abort.rs:730`, `:662`, `:583`. That is the
  discriminating oracle the brief asked for, not "store unchanged + non-zero". The tests drive
  the real binary by argv, so they exercise the production path.
- **Negation 2 (fence and records obligation in two batches), run by me.** In a scratch copy I
  changed `fence_rounds` (`crates/core/src/multipart/abort.rs:487`) to commit `obligations[..1]`
  and then the second obligation in its own batch. Both `Completing`-door DST cases went red at
  `crates/dst/tests/custodian.rs:8676` ("Completing door, cut at 3125us": session fenced, records
  obligation missing). The `Open` door has one obligation, so it can't be split and stayed
  green. So the DST does tell one batch from two. T4's TEST-GAP that "the cut drops the commit
  future, so a late landing is missed" doesn't weaken this: a single batch that lands late
  still lands whole and can't produce a third state.
- **X57 via a live completer.** Each segment batch requires the session at its starting cursor
  and puts the moved cursor (`crates/core/src/multipart/complete.rs:1567-1572`). The fence
  requires the exact bytes it read (`crates/core/src/multipart/abort.rs:97`). So no segment can
  land after the fence. A segment batch that lands first moves the bytes, and the abort
  re-plans from `Lost::Moved` with the new record. I couldn't build a stranding interleaving.
- **F18.** `completing_teardown` names `SegmentGroup(nonce, publish_target.epoch)`
  (`crates/core/src/multipart.rs:2703-2708`, `:2777`), and decode pins `publish_target.epoch`
  to the session epoch. Leg B's decoy assertion and the DST payload check both cover it.
- **CLI argv, probed on the real binary:** `--clock-source logical` → exit 1, unknown flag.
  `--data-dir d1 --data-dir d2 --upload-id A --upload-id B` → exit 1, "more than once". An
  uppercase id is refused. `--data-dir --bogus` on redb → exit 1, "no metadata store at
  --bogus/meta.redb", and nothing is created. `wyrd mpu` and `wyrd mpu expire` → exit 2.

## Findings

- NEEDS-HUMAN [impl] — **Test D doesn't pin grant term (ii) or the unknown-flag refusal; both
  C5 survivors are real gaps.** `crates/server/tests/multipart_operator_abort.rs:711-735` only
  repeats `--upload-id` on its own. Under the surviving mutant at
  `crates/server/src/cli.rs:3240` (`seen == name` → `!=`), the argv
  `--data-dir d1 --data-dir d2 --upload-id A --upload-id B` is **accepted**: `data-dir` enters
  `repeated` first, `upload-id` is never pushed, and the verb fences B. That breaks grant term
  (ii), and the suite stays green. Under the second survivor (`cli.rs:3272`, `unknown_flag` →
  `None`), `--clock-source logical` is silently accepted, although the brief's Scope ("no
  clock-source flag") and `06-runtime-view.md` say it is refused. The real code is correct (I
  probed both). Add those two argv cases to test D, each asserting exit 1 and nothing written.
- NEEDS-HUMAN [impl] — **The new second-key conflict path has no test.** `lost_any`
  (`crates/core/src/multipart/abort.rs:520-535`, used at `:491-498`) is new. Its "second
  obligation key taken" branch is reached only by the `Completing` door. No case seeds
  `retire:records:s:<id>:<E>` already present beside an unchanged `Completing@E` record and
  checks the answer: `ObligationKeyTaken` naming the *records* key, exit 1, nothing written.
  The existing Taken coverage only reaches the first key. (T4 raised the same gap.)
- NEEDS-HUMAN [impl] — **The CLI can say "nothing written" when the fence did write.** At
  `crates/core/src/multipart/abort.rs:464-469`, after a `CommitUnknownResult`, suppose the fence
  landed and the custodian finished the teardown before the one re-read. Then `doubt.settle(None)`
  clears the doubt, `absent()` answers `NoSuchUpload`, and `crates/server/src/cli.rs:3202` prints
  "is absent … nothing written", exit 1. The fence did write. This is unlikely, because the
  whole drain must finish inside one read's window. But the loop knows better: `Doubt::puts` is
  non-empty at that point. Report "possibly fenced and already torn down" instead of "absent;
  nothing written". (T4 raised it.)
- NEEDS-HUMAN [human] — **Leg B passes against `{session, all}`, not the criterion's
  `{session, parts}`.** The brief (leg B and Scope) and `0016:665` say
  `retire:bytes:{session, parts}`. The patch installs `{session, all}`, and the test asserts
  `PartScope::All` (`crates/server/tests/multipart_operator_abort.rs:601-602`). This is the
  restore fence's existing shape, a documented deviation for the value ceiling (#842)
  (`crates/core/src/multipart.rs:2759-2760`), which the brief's "reuse, don't rewrite" told Do
  to reuse. `all` is a superset, and it is sound because no part commits outside `Open`. I think
  it is the right call. But it departs from the literal criterion, and the patch's
  `RetirePayload` table edit (`crates/core/src/multipart.rs:4157-4158`) now records that the
  operator abort and the reaper's planned `Completing` door use different shapes. So 0016's
  "one shape for all three doors" is two shapes in code. Accept this explicitly at sign-off
  rather than reading leg B as met verbatim.
- NEEDS-HUMAN [impl] — **The teardown hint and the test env can point at the wrong store.**
  The hint at `crates/server/src/cli.rs:3173` says to run `wyrd custodian
  --reap-abandoned-uploads` with no `--data-dir` or backend, so the copy-pasted command runs
  against the default dir, not the store just fenced. Echo the `--data-dir` (and
  `--metadata-backend`, if given) the abort used. Separately, the subprocess helpers
  (`multipart_operator_abort.rs` `mpu_abort` / `reap_once`) inherit `WYRD_METADATA_BACKEND`,
  while `crates/server/tests/build_identity_startup_log.rs:137` already clears it. Do the same.
  (T4 raised both.)
- Note (no tag): **the rounds budget is sized for the `Open` door only.** `ABORT_ROUNDS =
  MAX_COMPLETE_ATTEMPTS + 1 = 4` (`crates/core/src/multipart/abort.rs:71`) is reasoned from
  Complete fences moving an `Open` session. On the new `Completing` door, every segment batch
  of a *live* completer also moves the session bytes (`complete.rs:1567-1572`), so aborting a
  live `Completing` session can spend all 4 rounds and exit 1 with `RoundsSpent`. That fails
  closed, a re-run works, and the target case (a stuck foreign-clocked session) has no live
  completer. The `ABORT_ROUNDS` doc should say this in one sentence.
- Note: **T4 items I judge overstated.** First, a rival writing identical `Aborting` bytes after
  an unknown outcome, so this run reports `Fenced` (`abort.rs:464`). The end state and the
  obligations are byte-identical, so only the audit line's attribution is wrong, and this is
  inherited from `abort_session`. Second, `--data-dir --bogus` swallowing a flag-shaped value
  (`cli.rs:3094`). That is the existing `ParsedArgs` convention: on redb it is refused (probed),
  and on networked backends `--data-dir` is ignored by every command, the one-shot included.
  Whether a destructive verb should refuse a `--data-dir` it can't apply, or a repeated
  `--data-dir` / `--metadata-backend` (last value wins today), is a CLI-convention call. It is
  not a defect this patch introduced.
- Note: **the `C4-diff-cov` "fail" is environmental.** `patch.diff` does not apply on
  `origin/main` because this is a stacked bundle (brief, Falsifiability). It is not evidence
  against the fix.

## On the verdict

`check-gates.json`'s C4-verify row ("red without the fix, green with it, 4 test(s) ran red")
is warranted: I confirmed GREEN and read RED's counts and failure points. The DST row the brief
required as an atomicity proof is warranted too: the cases go red under negation 2. Overall is
"fail" only because of the T4 batch review's 15 blocking items. Of those, I'd act on the test
gaps, the false "nothing written" message and the hint text, and record the rest as
convention decisions.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3102`: The new unknown-flag check runs after `ParsedArgs::parse` has consumed option-shaped values. For example, `mpu abort --upload-id ID --data-dir --gc-expired-pending --data-dir DIR` hides the unsupported flag as the first data-directory value; the second directory replaces it, and the abort proceeds. Reject missing values before consuming another option, and cover this case plus an ordinary unknown flag in the subprocess tests. The frozen mutation log also shows that removing `unknown_flag` entirely survives the tests.

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3202`: “Nothing written” is not guaranteed when `NoSuchUpload` follows a commit attempt. A fence can commit with an unknown outcome, then the custodian can drain and delete the session before the settling read. `fence_rounds` clears its doubt and returns the absent outcome (`crates/core/src/multipart/abort.rs:467`), so this new diagnostic denies a write that actually happened. Distinguish initial absence from disappearance after an ambiguous commit, or report only the observed absence without claiming no write; cover that interleaving.

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3173`: The actionable teardown command drops the selected `--data-dir` and `--metadata-backend`. After aborting in a nondefault directory, copying this command instead opens `wyrd-data`; if that contains another initialized, empty store, the one-shot can report satisfaction while the intended upload remains undrained. Preserve the resolved store selection in the suggested command, including safe quoting of directory paths.

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_operator_abort.rs:151` and `crates/server/tests/multipart_operator_abort.rs:164`: Both subprocess helpers inherit `WYRD_METADATA_BACKEND` without explicitly selecting redb, although every fixture and assertion uses the temporary redb store. An environment selecting TiKV/FDB makes these tests fail in a default build or contact the configured remote store in a feature-enabled build. Pass `--metadata-backend redb` to both commands so they necessarily exercise their fixtures.

No separate reuse, simplification, or efficiency findings: both doors reuse the existing teardown helpers and atomic fence builder. Validation used the frozen gate evidence: CI passed, and all four subprocess tests ran green with the patch and red on the base. No gates were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve uncertainty about who fenced and whether this invocation wrote anything—independent probes returned `Fenced` for a rival’s commit and `NoSuchUpload` after this invocation’s fence had landed and been reclaimed (`crates/core/src/multipart/abort.rs:464`; `crates/core/src/multipart/abort.rs:468`; `crates/server/src/cli.rs:3202`).
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art by affected path—the supplied target exposes only one synthetic commit and no remote/history audit, so previous rejection or duplicate work cannot be settled from this bundle (`review-prior-art.log:1`; `gate-logs/T4-batch-review.log:10`).
- [ ] T5 Judgment — Strengthen the regression oracle: parser mutants survive, subprocess fixtures inherit backend selection, and cancellation tests omit late commits and the second-obligation conflict case; these gaps conceal operator-visible failures (`gate-logs/C5-mutants.log:13`; `crates/server/tests/multipart_operator_abort.rs:151`; `crates/dst/tests/custodian.rs:8642`).
- [ ] Validation — fitness-to-purpose — Approve the interim host-access workflow and coordinated release with the reaper; assess the unexercised production late-commit case, whose assurance currently rests on the atomic-batch code and a cancellation model that cannot apply a dropped commit later (`brief.md:169`; `crates/dst/tests/support/mod.rs:344`; `crates/dst/tests/custodian.rs:8642`).
- [ ] **Test D doesn't pin grant term (ii) or the unknown-flag refusal; both C5 survivors are real gaps.** `crates/server/tests/multipart_operator_abort.rs:711-735` only repeats `--upload-id` on its own. Under the surviving mutant at `crates/server/src/cli.rs:3240` (`seen == name` → `!=`), the argv `--data-dir d1 --data-dir d2 --upload-id A --upload-id B` is **accepted**: `data-dir` enters `repeated` first, `upload-id` is never pushed, and the verb fences B. That breaks grant term (ii), and the suite stays green. Under the second survivor (`cli.rs:3272`, `unknown_flag` → `None`), `--clock-source logical` is silently accepted, although the brief's Scope ("no clock-source flag") and `06-runtime-view.md` say it is refused. The real code is correct (I probed both). Add those two argv cases to test D, each asserting exit 1 and nothing written.
- [ ] **The new second-key conflict path has no test.** `lost_any` (`crates/core/src/multipart/abort.rs:520-535`, used at `:491-498`) is new. Its "second obligation key taken" branch is reached only by the `Completing` door. No case seeds `retire:records:s:<id>:<E>` already present beside an unchanged `Completing@E` record and checks the answer: `ObligationKeyTaken` naming the *records* key, exit 1, nothing written. The existing Taken coverage only reaches the first key. (T4 raised the same gap.)
- [ ] **The CLI can say "nothing written" when the fence did write.** At `crates/core/src/multipart/abort.rs:464-469`, after a `CommitUnknownResult`, suppose the fence landed and the custodian finished the teardown before the one re-read. Then `doubt.settle(None)` clears the doubt, `absent()` answers `NoSuchUpload`, and `crates/server/src/cli.rs:3202` prints "is absent … nothing written", exit 1. The fence did write. This is unlikely, because the whole drain must finish inside one read's window. But the loop knows better: `Doubt::puts` is non-empty at that point. Report "possibly fenced and already torn down" instead of "absent; nothing written". (T4 raised it.)
- [x] **Leg B passes against `{session, all}`, not the criterion's `{session, parts}`.** The brief (leg B and Scope) and `0016:665` say `retire:bytes:{session, parts}`. The patch installs `{session, all}`, and the test asserts `PartScope::All` (`crates/server/tests/multipart_operator_abort.rs:601-602`). This is the restore fence's existing shape, a documented deviation for the value ceiling (#842) (`crates/core/src/multipart.rs:2759-2760`), which the brief's "reuse, don't rewrite" told Do to reuse. `all` is a superset, and it is sound because no part commits outside `Open`. I think it is the right call. But it departs from the literal criterion, and the patch's `RetirePayload` table edit (`crates/core/src/multipart.rs:4157-4158`) now records that the operator abort and the reaper's planned `Completing` door use different shapes. So 0016's "one shape for all three doors" is two shapes in code. Accept this explicitly at sign-off rather than reading leg B as met verbatim.
- [ ] **The teardown hint and the test env can point at the wrong store.** The hint at `crates/server/src/cli.rs:3173` says to run `wyrd custodian --reap-abandoned-uploads` with no `--data-dir` or backend, so the copy-pasted command runs against the default dir, not the store just fenced. Echo the `--data-dir` (and `--metadata-backend`, if given) the abort used. Separately, the subprocess helpers (`multipart_operator_abort.rs` `mpu_abort` / `reap_once`) inherit `WYRD_METADATA_BACKEND`, while `crates/server/tests/build_identity_startup_log.rs:137` already clears it. Do the same. (T4 raised both.)
- [ ] `crates/server/src/cli.rs:3102`: The new unknown-flag check runs after `ParsedArgs::parse` has consumed option-shaped values. For example, `mpu abort --upload-id ID --data-dir --gc-expired-pending --data-dir DIR` hides the unsupported flag as the first data-directory value; the second directory replaces it, and the abort proceeds. Reject missing values before consuming another option, and cover this case plus an ordinary unknown flag in the subprocess tests. The frozen mutation log also shows that removing `unknown_flag` entirely survives the tests.
- [ ] `crates/server/src/cli.rs:3202`: “Nothing written” is not guaranteed when `NoSuchUpload` follows a commit attempt. A fence can commit with an unknown outcome, then the custodian can drain and delete the session before the settling read. `fence_rounds` clears its doubt and returns the absent outcome (`crates/core/src/multipart/abort.rs:467`), so this new diagnostic denies a write that actually happened. Distinguish initial absence from disappearance after an ambiguous commit, or report only the observed absence without claiming no write; cover that interleaving.
- [ ] `crates/server/src/cli.rs:3173`: The actionable teardown command drops the selected `--data-dir` and `--metadata-backend`. After aborting in a nondefault directory, copying this command instead opens `wyrd-data`; if that contains another initialized, empty store, the one-shot can report satisfaction while the intended upload remains undrained. Preserve the resolved store selection in the suggested command, including safe quoting of directory paths.
- [ ] `crates/server/tests/multipart_operator_abort.rs:151` and `crates/server/tests/multipart_operator_abort.rs:164`: Both subprocess helpers inherit `WYRD_METADATA_BACKEND` without explicitly selecting redb, although every fixture and assertion uses the temporary redb store. An environment selecting TiKV/FDB makes these tests fail in a default build or contact the configured remote store in a feature-enabled build. Pass `--metadata-backend redb` to both commands so they necessarily exercise their fixtures.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- [ ] size backstop — this slice is behaving oversized: patch is 100 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): The core fence holds (one-batch abort, exact-attempt F18 reclaim, X57 — the adversary could not refute them); keep the design. Fix the implementation-level findings, all small: - CLI parsing: refuse a valued flag whose value is missing or option-shaped (`--data-dir --bogus`) before any store access; add subprocess cases for an unknown flag (`--clock-source logical`) and for repeated non-id flags alongside a repeated `--upload-id` (kills the two C5 survivors at cli.rs:3240 / :3272), each asserting exit 1 and nothing written. - Operator output: the teardown hint must echo the effective `--data-dir` (quoted) and `--metadata-backend`; after a CommitUnknownResult, a later absence must not be reported as "nothing written" (report "possibly fenced and already torn down"), and do not attribute a rival's identical fence to this invocation. - Tests: pass `--metadata-backend redb` (or clear WYRD_METADATA_BACKEND) in both subprocess helpers; add the Completing-door case where only the records obligation key is already taken (ObligationKeyTaken naming it, nothing written); add a DST schedule where an already-submitted fence commit lands after the operator gives up, and check reclamation afterwards. - Doc one sentence on ABORT_ROUNDS being sized for the Open door (a live completer can exhaust it; fails closed). Decided at sign-off: `{session, all}` is ACCEPTED for the operator-abort Completing door (leg B is met with `PartScope::All`). Fix the RetirePayload table at `crates/core/src/multipart.rs` so it says all three Completing→Aborting doors (restore, reaper W_session ceiling, operator abort) use `{session, all}` — not that the reaper uses `{session, parts}`. Do not edit 0016 here; the proposal update is tracked separately.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Human accepted `{session, all}` for the operator-abort Completing door (#842 value ceiling); update proposal 0016 (`0016:665`, "one shape for all three doors") to `{session, all}`, and fix the `multipart.rs` RetirePayload table — 1017's reaper ceiling uses `completing_teardown` too, so all three doors share it.
