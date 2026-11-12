# Result — issue 1010 / multipart-complete-unknown-outcomes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 to child-3, any `CommitUnknownResult` on a commit of Complete's
  returns the in-doubt error. That is safe, but coarse in two ways:
  - a flip that **landed** and was answered unknown makes Complete return an error for an object
    it published;
  - an unknown segment write or initial fence that did not land stops a Complete that could
    resume or retry.

  0016 requires the publisher to re-read and recover (`0016:942-964`). Two traps must stay closed:
  - an unchanged re-read after `may_still_commit: true` does not prove the held batch will not
    land (`crates/traits/src/lib.rs:240-247`);
  - after an unknown **initial** fence, a re-read showing `Completing@E+1` with the proposed
    bytes is not proof of ownership. A `Completing` record names no request
    (`multipart.rs:2186-2194`).
- Success criterion: New legs appended to `crates/core/tests/multipart_complete.rs`. The
  wrapper gains a **hold** step: it keeps a batch with `may_still_commit: true` and lands it later
  (the shape of `crates/core/tests/multipart_abort.rs:961-980`). The G0 cases for the flip, the
  segment batch and the unapplied initial fence are replaced by G1, G4 and G6. The G0 cases for
  the applied fence, the release and the failed read stay.
  **(G1)** The wrapper applies the flip and answers `CommitUnknownResult`. Complete answers
  `Published` once. The inode version advanced once, and exactly one set of obligations exists.
  **(G2) A held flip.** The wrapper holds the flip (`may_still_commit: true`) and answers every
  later commit `Conflict` without applying it, until the rounds run out. Complete returns the
  in-doubt error. Released afterwards, the held flip lands: the object is published once, with
  one set of obligations.
  **(G3) A failed re-read.** After an unknown flip outcome, the re-read of `mpu:<id>` fails.
  Complete returns the in-doubt error, never an answer.
  **(G4) An unknown segment write.** A segment batch answers `CommitUnknownResult {
  may_still_commit: false }`, applied in one run and not in another. Complete re-reads, resumes
  from `segments_written` at the same epoch, writes no `seg:` record twice, and publishes once.
  **(G5) An unknown INITIAL fence never proves ownership.** With the clock fixed, Complete A
  (parts 1, 2) and Complete B (parts 1, 3) both read `Open@E` and build byte-identical fences.
  - **Run 1:** the wrapper holds A's fence, lets B's commit (a definite success), then answers A's
    `CommitUnknownResult { may_still_commit: false }` unapplied.
  - **Run 2:** the wrapper applies A's fence and answers it unknown, then B's fence conflicts.

  In both runs, A's re-read finds `Completing@E+1` with exactly its proposed bytes, and A returns
  the in-doubt error with no `seg:` record and no flip. In run 1, B publishes parts 1 + 3 once.
  In run 2, B answers `Refused(OperationAborted)`, nothing is published, and the session stays
  `Completing@E+1` for the reaper (#625).
  **(G6) An unknown initial fence that did not land is retried.** The first fence answers
  `CommitUnknownResult { may_still_commit: false }` unapplied, and the re-read finds `Open@E`
  unchanged. Complete retries the fence (bounded), gets a definite success, and publishes once.
  Every earlier leg still passes.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the re-read, resume, retry and settling logic after a `CommitUnknownResult` on the initial
    fence, segment batches, flip and release;
  - the in-doubt error's doubt bookkeeping (the first `may_still_commit: true` outcome is kept,
    as `abort.rs` does);
  - the wrapper's hold step.

  Out of scope:
  - any change to the error type's arms (child-1 owns them);
  - any record shape change (none is needed);
  - the value ceiling (child-5);
  - the reaper (#625), the gateway's same-request recovery (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 26 mutants tested in 2m: 20 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.64s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #1010: safely recover multipart Complete after unknown commit outcomes; no builder-fixable defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | G1–G6 define observable recovery and ownership guarantees; the latest approved rollback rule resolves the original retry ambiguity (brief.md:25; brief.md:159). |
| C2 Reproduction (red pre-fix) | PASS | Stashing only the production fix while retaining the new tests produces 16 assertion failures, including applied-flip recovery and initial-fence retry; these are behavioral failures, not build faults (reviewer-red.log:351). |
| C3 Change | PASS | The four-file patch stays within recovery, regression tests and living documentation; charged rollback now prevents reuse of a potentially expired fence timestamp (target/crates/core/src/multipart/complete.rs:966; target/docs/design/architecture/05-building-block-view.md:252). |
| C4 Verification (red→green) | PASS | Restoring the identical patch makes all 80 multipart Complete tests pass; the full core run passes 664 tests, with independent clippy and scanners also green; diff coverage remains explicitly unmeasured (reviewer-green.log:252; reviewer-clippy.log:142; gate-logs/C4-diff-cov.log:11). |
| C5 Causal adequacy | PASS | Recovery preserves the distinction between uncertain ownership and owned publication, retaining held outcomes across failed reads and later errors; no capability-probe or load-time workaround was introduced (target/crates/core/src/multipart/complete.rs:959; target/crates/core/src/multipart/complete.rs:1503; target/crates/core/src/multipart/complete.rs:1873). |
| T1 Structure | PASS | Recovery remains behind the existing MetadataStore seam, with local doubt bookkeeping and one publication-ending helper; no dependency-direction or shared-global-state change is needed (target/crates/core/src/multipart/complete.rs:1597; target/crates/core/src/multipart/complete.rs:1831; reviewer-statics.log:3). |
| T2 Shape | PASS | Scope and size follow the recorded sign-off; the extra cases exercise requested rollback and segmented-publication behavior, and no persisted schema changes (brief.md:159; target/crates/core/tests/multipart_complete.rs:5435; target/crates/core/tests/multipart_complete.rs:6196). |
| T3 Runtime | PASS | Recovery spends the existing bounded fence/publication rounds; held batches prevent premature release, and store awaits retain the backend termination contract (target/crates/core/src/multipart/complete.rs:931; target/crates/core/src/multipart/complete.rs:1569; target/crates/traits/src/lib.rs:1334). |
| T4 Contribution | N/A | Contribution artifacts are deliberately drafted after Check; their substantive audit must rerun at publish, so there is no missing-artifact decision to escalate (gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | The requested regressions now distinguish charged rollback from a rival's give-back; independent mutation testing catches all 20 viable mutants, and affected-path history plus all 19 closed/unmerged PR file lists reveal no rejected overlapping work (target/crates/core/tests/multipart_complete.rs:6294; reviewer-mutants-capped.log:3; reviewer-prior-art.log:14). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge whether the demonstrated G1–G6 behavior is sufficient for this core-only feature — passing contract tests cannot make the product acceptance decision (brief.md:25; brief.md:103). |

The independent evidence supports the recovery contract. `git stash push` removed only the two production-source changes, preserving the patched integration tests: `cargo test --offline -p wyrd-core --test multipart_complete` returned 101 with 64 passing and 16 failing tests. After `git stash pop`, `cargo test --offline -p wyrd-core` passed all 664 tests, including all 80 Complete tests. The final source diff is byte-identical to the original disposable target diff. Evidence: reviewer-red.log:369; reviewer-green.log:252.

Mutation testing also reproduces the frozen result: 26 tested, 20 caught, six unviable, no survivors or timeouts (reviewer-mutants-capped.log:3; gate-logs/C5-mutants.log:13). An initial run left five additional mutants unviable because removed bodies triggered unused-variable errors; rerunning with `--cap-lints true` made those mutants executable and the tests caught them. The G1 wide-object case checks publication, exact object contents and segment reachability; the charged-rollback cases prohibit a second fence while the give-back case permits one (target/crates/core/tests/multipart_complete.rs:5435; target/crates/core/tests/multipart_complete.rs:6196; target/crates/core/tests/multipart_complete.rs:6294).

Independent validators passed: `typos`, `cargo fmt --all -- --check`, documentation lint, core clippy with all targets, conformance, statics, and the real repository guards within `cargo xtask ci-dry-run`. The dry-run's workspace cargo commands were only printed; they are not claimed as executed. The brief's external `typos` dependency was exercised successfully. Evidence: reviewer-docs.log:1; reviewer-conformance.log:1; reviewer-statics.log:3; reviewer-ci-dry-run.log:16.

Frozen gate evidence supplies the remaining checks:

- **C4-ci — PASS:** the captured full run includes formatting, clippy, build, workspace tests, dependency checks, conformance and DST, ending with “all checks passed” (gate-logs/C4-ci.log:4836). The instance-scoped wrapper is not in this target, and the disposable snapshot's index omits `.github/advisory-db`, so a faithful full CI rerun is unavailable here. This is a snapshot limitation, not a patch defect.
- **C4-verify — PASS with limited original evidence:** the frozen log is green-only (gate-logs/C4-verify.log:161); the independent stash/restoration run above supplies the missing behavioral red→green.
- **C4-diff-cov — recorded FAIL, coverage unmeasured:** the log reports failure to apply the stacked patch on `origin/main`, not a coverage percentage (gate-logs/C4-diff-cov.log:10). This limitation was already accepted at sign-off (brief.md:166); it is not re-raised as a patch failure.
- **T4 batch review and TiKV compilation — PASS from logs:** the batch summary reports zero blocking findings; the feature-gated compilation finishes successfully (gate-logs/T4-batch-review.log:10; gate-logs/host-tikv.log:211). These are frozen results, not independent reruns.
- **T4 contribution — N/A:** the publish-time audit remains mandatory (gate-logs/T4-contribution.log:10).

Prior art was checked by all four affected paths against merged default-branch history, then against the changed-file lists of all 19 closed, unmerged PRs; every query succeeded and none of those PRs touched the paths (reviewer-prior-art.log:1; reviewer-prior-art.log:14). Existing deferrals for the reaper, S3 wiring and seeded DST remain settled; the review does not reopen them (brief.md:86; target/crates/core/src/multipart/complete.rs:792).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

**Bottom line:** I could not break the production logic. The red→green proof holds when re-run. Two test and contract gaps remain, both fixable by the builder. All `path:line` refs are to `$PDCA_TARGET` (the patched tree).

## Evidence, re-run

- **Red→green confirmed.** I ran the patched `crates/core/tests/multipart_complete.rs` against the pre-fix `complete.rs` (base `0b1143d`): 16 of 80 tests fail (G1, G1-open, G2 ×4, G3-fence, G4 ×3, G6 ×6). With the fix, all 80 pass. G3 (flip) and G5 pass on the old code too, because the old code returned the in-doubt error at once. Mutants guard those two instead: dropping the own-bytes check at `crates/core/src/multipart/complete.rs:959` is caught by G5 and G0-fence, and releasing on a failed re-read at `:1538` is caught by G3 and G2-first. The tests drive `complete_upload` / `fence_complete` against a real redb store through the `Logged` wrapper (`crates/core/tests/multipart_complete.rs:542-731`), so they exercise the production path, not a copy.
- **Hand mutants (15).** 11 were caught. 4 survived (finding 1). `cargo mutants` (C5) never tried those 4, because it does not swap one call for another.

## Findings

- NEEDS-HUMAN [impl] — **Four of the new "stay in doubt" branches have no test.** Reverting any one of them to its pre-patch form still passes all 80 tests:
  - `crates/core/src/multipart/complete.rs:945`: a re-sent fence is refused for good while an earlier fence is held. Reverted to `not_placed(..)`, it returns `Stopped{NotPlaced}`, but the held fence can still land. **Probe:** script `[Hold, Fail{transient: true}]`. Expect `InDoubt{Fence{1}, may_still_commit: true}` and 2 commits; afterwards `release_held()` returns `Committed`.
  - `:1562`: the bucket-marker read fails after a held flip. Reverted to `release(..)`, it sends a release that races the held flip and ends `Stopped{Released}`. **Probe:** `[Pass, arm(FailGets(bucket_key(BUCKET)), Hold)]`. Expect `InDoubt{Flip{1}, true}` and 2 commits (no release); then `clear_read_faults()`, `release_held()` lands, and `assert_published_once` holds.
  - `:1577`: the target's dirent read fails after a held flip. Same failure as `:1562`. **Probe:** the same script with `FailGets(dirent_key(ROOT, "photos/object-1"))`.
  - `:1468`: building the flip fails after a held batch, e.g. a target re-read that finds a prior at `u64::MAX`. Same class; harder to set up (a held segment batch, then an overwrite with a max-version prior).

  I ran the three probes: each passed on the patch and failed on its mutant. These branches carry the module doc's own rules: no release while a batch may still land, and never `Stopped` while in doubt (`complete.rs:190-194`, `:239-241`). Each deserves a regression test, like the G2-refused leg the carry-forward asked for at `:1513`.
- NEEDS-HUMAN [impl] — **A session read that does not decode settles an unknown outcome, so the request ends `Stopped` even when the unknown commit landed.**
  - **Fence phase:** `doubt.settle` runs at `complete.rs:888`, before the decode at `:895-896`. Script `[write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))]` returns `Stopped{fence: NotPlaced, cause: Record(..)}` (probe run here). `NotPlaced` is documented as "No fence commit of this Complete landed" (`:590-591`), which is false here.
  - **Publication phase:** `:1549-1555` sends the same unreadable session read to `moved()` (`:1327-1332`). `[Pass, write_after(Unknown{applied: true, may_still_commit: false}, torn)]` returns `Stopped{Gone}` while the object is published (the dirent is bound to inode 500, probe run here).

  Both contradict "never `CompleteError::Stopped`" while a doubt stands (`:190-194`). It takes a session record that does not decode (corruption, or a newer record format during a rolling upgrade), so severity is low. The fix is to return `doubt.in_doubt(..)` when the read after an unknown outcome does not decode, before settling.
- Note, not a defect: every in-doubt error the new code returns has `fault_behind_fence: None` (`complete.rs:1513-1514`, `:1604-1605`). So a held flip followed by, say, a damaged target (`Stop::Target`) reports only "in doubt", and the cause is lost to the operator. This matches the field's wording ("when that is why it tried" a release, `:534-538`), since no release is sent.
- Note on the gate rows: the C5 row in `check-gates.json` ("26 mutants … 20 caught, 6 unviable") is accurate, but narrower than "no surviving mutants" suggests (see finding 1). I did not re-raise the C4-diff-cov row; the human already cleared it as a stacked-base artifact.

## Attempted to refute; could not

- **Byte-identical fence trap.** A rival's fence with the same bytes is never taken as ownership (`:959-961`). A rival that has moved past cursor 0 gets `OperationAborted` through `answer_without_fence`, which is correct: an unproven fence of this call never writes segments. A held fence cannot land once a re-sent fence of the same call commits, because both require the same `Open@E` bytes and the epoch only moves forward (`:972-973`).
- **Held pin set to bytes "at or past" the held batch** (`:1557`, `Doubt::settle` `:1873-1892`). It can keep a doubt that has in fact landed (a false in-doubt, so the client waits for the reaper). It can never clear one that may still land: only this Complete moves the cursor, and every later batch requires the pinned bytes. That is a deliberate, safe trade-off, not a defect.
- **`unknown_flips`** (`:1460`, `:1541-1546`). Only this fence's flips can put those `Completed` bytes. An entry for a flip that was never sent (a round whose segment batch was the unknown one) is harmless.
- **`open_and_charged_since`** (`:1003-1008`) checked against give-back releases, a charged release after segments, and a rollback followed by a rival's give-back. It is sound, provided #625's rollback keeps the attempt charged even at cursor 0. `0016:661` supports that, and the iteration-2 human decision settled it.
- **Bounds.** Unknown rounds count against `FENCE_ROUNDS` and `R_PUBLISH`. No new unbounded loop or await.
- **Rubric.** `CommitUnknownResult` is never treated as `Conflict` (Transactions class). Docs currency is met (`docs/design/architecture/05-building-block-view.md:252-254`). The DST deferral to #660 is settled (`complete.rs:792`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found in the fence retry/rollback handling (`crates/core/src/multipart/complete.rs:937`), publication recovery (`crates/core/src/multipart/complete.rs:1487`), or doubt bookkeeping (`crates/core/src/multipart/complete.rs:1842`). The changed tests exercise ownership ambiguity, held commits, cursor recovery, and the charged rollback case (`crates/core/tests/multipart_complete.rs:5435`, `crates/core/tests/multipart_complete.rs:5838`, `crates/core/tests/multipart_complete.rs:6196`, `crates/core/tests/multipart_complete.rs:6726`).
- No actionable reuse, simplification, or efficiency findings. Recovery uses the existing cursor/resume and release helpers, with bounded rounds (`crates/core/src/multipart/complete.rs:1482`, `crates/core/src/multipart/complete.rs:1569`, `crates/core/src/multipart/complete.rs:1597`).

Validation evidence: frozen CI and core tests passed; mutation testing reported 20 caught and 6 unviable mutants, with none surviving. No local build was run. The diff-coverage limitation is already settled in the brief.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Judge whether the demonstrated G1–G6 behavior is sufficient for this core-only feature — passing contract tests cannot make the product acceptance decision (brief.md:25; brief.md:103).
- [ ] **Four of the new "stay in doubt" branches have no test.** Reverting any one of them to its pre-patch form still passes all 80 tests:
- [ ] **A session read that does not decode settles an unknown outcome, so the request ends `Stopped` even when the unknown commit landed.**
- [ ] size backstop — this slice is behaving oversized: patch is 154 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Production logic and design stand; close the two adversary findings from v3, nothing else. - Fix: a session read that does not decode after an unknown outcome must not settle the doubt. Return doubt.in_doubt(..) before doubt.settle when the re-read fails to decode — fence phase (complete.rs ~:888, settle runs before the decode at ~:895-896) and publication phase (~:1549-1555, the torn read sent to moved()). Regressions: [write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))] -> InDoubt, not Stopped{NotPlaced, Record}; [Pass, write_after(Unknown{applied: true, may_still_commit: false}, torn)] -> InDoubt, not Stopped{Gone}. - Add regression tests for the untested "stay in doubt" branches, using the adversary's probes: ~:945 [Hold, Fail{transient: true}] -> InDoubt{Fence{1}, true}, 2 commits, release_held() commits; ~:1562 [Pass, arm(FailGets(bucket_key(BUCKET)), Hold)] -> InDoubt{Flip{1}, true}, no release, then clear_read_faults + release_held lands and assert_published_once; ~:1577 same with FailGets(dirent_key(ROOT, "photos/object-1")); ~:1468 (flip build fails after a held batch, e.g. prior at u64::MAX) if it can be set up. - Size: iterate-do chosen over a split; growth should be the tests above and the small decode fix only.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
