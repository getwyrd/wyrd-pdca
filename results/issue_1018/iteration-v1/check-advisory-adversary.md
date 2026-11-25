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
