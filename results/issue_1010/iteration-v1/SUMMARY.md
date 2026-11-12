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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 20 mutants tested in 3m: 2 missed, 10 caught, 6 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.34s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1010, recovery of multipart Complete after unknown commit outcomes: red→green is confirmed; two regression gaps need builder iteration, and the rollback-to-Open interpretation needs human approval.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | G1–G6 define observable publication, ownership and retry outcomes; the separate Open-rollback interpretation is recorded under T5 (brief.md:26; brief.md:117). |
| C2 Reproduction (red pre-fix) | PASS | Stashing only the implementation while retaining the new tests produced five expected failures: G1, G1-open, G2, G4 and G6; 64 tests passed (reviewer-red.log:248; reviewer-red.log:284). |
| C3 Change | PASS | Recovery stays within the existing fence/publication protocol and updates its living architecture description; no persisted format or public error arm changes (crates/core/src/multipart/complete.rs:939; crates/core/src/multipart/complete.rs:1464; docs/design/architecture/05-building-block-view.md:252). |
| C4 Verification (red→green) | PASS | Restoring the patch produced 69/69 integration passes and the full-core baseline passed 653 tests; frozen CI and TiKV logs show successful runs, while diff coverage remains unmeasured because its remote base lacks prerequisites (reviewer-green.log:76; mutants.out/log/baseline.log:1; gate-logs/C4-ci.log:4829; gate-logs/C4-diff-cov.log:10). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add regressions for refusal while an initial fence may still land and termination after doubt is settled — removing either protection survives the full core suite, leaving safety and recovery behavior unprotected (crates/core/src/multipart/complete.rs:912; crates/core/src/multipart/complete.rs:1842; reviewer-mutants.log:3; reviewer-settle.log:3). |
| T1 Structure | PASS | Recovery preserves the MetadataStore seam, exact-byte transaction preconditions and decode boundaries; it introduces no backend dependency or shared mutable global (crates/core/src/multipart/complete.rs:884; crates/core/src/multipart/complete.rs:923; crates/core/src/multipart/complete.rs:1416). |
| T2 Shape | PASS | Private doubt bookkeeping and a shared termination helper keep the existing API and record shapes; formatting, spelling and documentation lint passed independently (crates/core/src/multipart/complete.rs:1565; crates/core/src/multipart/complete.rs:1795; docs/design/architecture/05-building-block-view.md:254). |
| T3 Runtime | PASS | Retries and retained flip candidates are bounded by existing round limits, with caller-supplied clock stamps and store-contract deadlines; the held-flip and rival-fence tests passed (crates/core/src/multipart/complete.rs:237; crates/core/src/multipart/complete.rs:1351; crates/core/tests/multipart_complete.rs:5453; crates/core/tests/multipart_complete.rs:5718). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent until publish, where their substantive audit must rerun; the separate affected-path prior-art audit completed (gate-logs/T4-contribution.log:10; reviewer-prior-art.log:1). |
| T5 Judgment | NEEDS-HUMAN | Approve interpreting “Open ⇒ answer as a fresh Complete” as OperationAborted followed by a fresh client request — this avoids reusing an expired fence timestamp but changes who performs recovery (brief.md:132; crates/core/src/multipart/complete.rs:220; crates/core/tests/multipart_complete.rs:5398). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated core recovery and the cost of leaving ambiguous initial fences charged satisfy this slice's purpose — the tests exercise a real store, but this API has no production caller yet (brief.md:125; crates/core/src/multipart/complete.rs:208; crates/core/src/multipart/complete.rs:255). |

The two C5 findings concern missing regression protection; this review found no demonstrated failure of the unmutated implementation. Source citations above and below refer to `$PDCA_TARGET`; evidence-file citations refer to this review directory. The target contained the prerequisite slices and compiled successfully. Its final diff matches `patch.diff` byte for byte, and the review stash was restored.

1. **Protect the held initial fence against an early refusal.** `Doubt::holds → false` survived all core tests (`reviewer-mutants.log:3`). The existing G6 held-fence case keeps the bucket and parts valid, so running validation again is indistinguishable from skipping it (`crates/core/tests/multipart_complete.rs:5865`). Add a case that removes the bucket while holding the first fence, then makes subsequent fence commits conflict. Require InDoubt and prove the held fence can still land afterwards. Without the protection at `crates/core/src/multipart/complete.rs:912`, validation can return NoSuchBucket while that fence remains in flight, leaving a later Completing session behind a definitive refusal.
2. **Protect the transition from unknown to settled.** Replacing `Doubt::settle` with a no-op also survived all core tests (`reviewer-settle.log:3`). Add an unapplied flip with `may_still_commit: false`, a successful unchanged session read, conflicts for the remaining publication rounds, and a successful release. Require OperationAborted with the session Open. The current code clears doubt and permits that release (`crates/core/src/multipart/complete.rs:1525`; `crates/core/src/multipart/complete.rs:1572`); the mutant returns InDoubt and leaves the fence standing. Existing G1/G4/G6 success paths can return before demonstrating this distinction. The first mutation run rejected this mutant only for an unused parameter; rerunning with `--cap-lints true` made it viable and confirmed the missing test.

The independent evidence supports the advertised recovery cases. `git stash push -- crates/core/src/multipart/complete.rs`, followed by `cargo test --offline --locked -p wyrd-core --test multipart_complete`, produced the red result; `git stash pop` followed by the same command produced 69 passes. G3 and G5 already passed before the fix because the previous unconditional InDoubt response conservatively satisfied them; their pre-fix passes do not establish recovery. The mutation runs also executed the entire `wyrd-core` suite, with 653 baseline passes. Independent `git diff --check`, `typos`, `python3 docs/publishing/tools/lint_docs.py` and `cargo fmt --all -- --check` exited successfully. The brief's external dependency, `typos`, was exercised directly and also appears in `gate-logs/C4-ci.log:11`; no missing-tool substitution is involved.

The frozen gate evidence has two distinct limits. C4-verify explicitly reports green-only (`gate-logs/C4-verify.log:161`); the independent stash/pop experiment supplies its missing red leg. Diff coverage never ran because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`); remote file history still contains only the first Complete slice, while this disposable target includes the prerequisite publication slices (`reviewer-prior-art.log:4`). This is a base-ordering caveat, not a compile or application defect in the supplied target. No coverage percentage is established. The instance-scoped wrappers were adjudicated from their frozen logs: full CI reports success after fmt, clippy, build, tests, dependency checks and DST (`gate-logs/C4-ci.log:4829`), both TiKV feature compilations finish (`gate-logs/host-tikv.log:112`; `gate-logs/host-tikv.log:211`), and the batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The contribution row remains deferred to publish, not a missing-evidence escalation.

Prior art was checked by all four affected paths, including default-branch histories and the file lists of all 19 returned closed/unmerged PRs; none of those rejected PRs touched these exact paths (`reviewer-prior-art.log:3`; `reviewer-prior-art.log:7`). Abort's two kinds of doubt provide the relevant existing contract (`crates/core/src/multipart/abort.rs:259`). The tracked DST deferral to #660 is settled under the standing rubric (`crates/core/src/multipart/complete.rs:783`). The symptom-guard smell test does not fire: the new checks distinguish transaction uncertainty and ownership, rather than probing an assumed capability or masking a load-time side effect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

**Bottom line:** I could not break the production logic. Every input I built gets the right answer from the fixed code. The weak spot is the tests: most of the new doubt bookkeeping (`Doubt` in `complete.rs`) can be mutated without any leg going red, and C5's two "missed" mutants are only part of that gap. The toolchain was available, so this verdict is not provisional.

## Evidence re-run (scratch copy of `$PDCA_TARGET`, `cargo test -p wyrd-core`)

- **Green:** all 10 G-legs pass on the patched tree.
- **Red:** with the pre-fix `crates/core/src/multipart/complete.rs` and `multipart.rs` put back under the new test file, G1, G1-open, G2, G4 and G6 fail. G0-fence, G3 and G5 still pass, which is expected: they guard against an over-eager fix, and the old code already returned the in-doubt error there. The brief's reds for them are mutant reds. I confirmed both. Turning the matching-bytes check at `complete.rs:947-951` into `break` (taking the bytes as ownership) fails G0-fence and G5. Letting `end_publication` (`complete.rs:1572`) release despite a doubt fails G2 and G3.
- **No flakiness:** G2, G5 and G6 passed 40 of 40 repeat runs. B can't open G5's `OpenOnGet(mpu:)` gate early, because between its fence and its flip it reads only `part:`, `psum:` and bucket keys (`multipart.rs:1254-1256`).

## Findings

- NEEDS-HUMAN [impl] — **Fence-phase doubt handling is untested.** Three mutants survive the whole `wyrd-core` suite. I wrote a probe leg for each one; the probe passes on the fix and fails on the mutant:
  - `Doubt::holds -> false` (`complete.rs:1833`, used at `:912`). Case: `OnCommit::write_first(delete bucket, OnCommit::Hold)` on the first fence. The fix sends the fence again; it lands, the check behind it rejects, the release goes out, and the answer is `NoSuchBucket` after 3 commits, with `release_held()` then answering `Conflict`. The mutant answers `NoSuchBucket` after 1 commit, and `release_held()` then lands a `Completing@1` that no answer names. That is exactly the "answer while a fence may still land" bug that `:908-911` says it prevents.
  - The fence `stop` closure ignoring the doubt (`complete.rs:867`, read failure at `:943`). Case: `arm(FailGets(mpu:), Unknown{applied:false, may_still_commit:true})` on the fence. The fix returns `InDoubt{Fence{1}, may_still_commit:true}`. The mutant returns `Stopped{NotPlaced}`, telling the caller no fence landed while one may still land. G3 covers a failed re-read only after a **flip**, never after a fence.
  - No `doubt.settle` at `complete.rs:877`. Case: `[Unknown{applied:false, may_still_commit:false}]`, then every later commit `ConflictUnmoved`. The fix ends in `Stopped{NotPlaced, FenceRoundsSpent}`. The mutant ends in-doubt.
- NEEDS-HUMAN [impl] — **Publication-phase doubt handling is partly untested.** Three more mutants survive; my probes kill each one:
  - Deleting the doubt check before a definite refusal (`complete.rs:1481-1483`). Case: `[Pass, Hold, Fail{transient:true}]`. The fix returns `InDoubt{Flip, true}` with 3 commits. The mutant sends a release (4 commits) and returns `Stopped`.
  - Letting the unsettled outcome outrank the held one in `Doubt::in_doubt` (`complete.rs:1862-1867`), which goes against the brief's Scope rule "the first `may_still_commit: true` outcome is kept". Case: `[Pass, Hold, arm(FailGets(mpu:), Unknown{applied:false, may_still_commit:false})]`. The fix reports `may_still_commit: true`. The mutant reports `false`, which under `crates/traits/src/lib.rs:242-246` tells the caller one re-read settles it.
  - `Doubt::settle -> ()`, or no `doubt.settle` at `complete.rs:1525`. Case (flat): `[Pass, Unknown{applied:false, may_still_commit:false}, ConflictUnmoved, ConflictUnmoved, Pass]`. The fix releases and answers `Refused(OperationAborted)`. The mutant ends in-doubt and leaves the fence for the reaper.
- NEEDS-HUMAN [impl] — **Two pin rules still have no killing case, even with my probes.** Both need wrapper support the suite lacks:
  - Clearing a pinned held doubt (`complete.rs:1847`; mutant `kept = true`). This is the "until a read finds the session moved" half of the brief's invariant. Case: in the wide fixture, the segment batch is held, the held batch lands before the next round (a script step that calls `release_held` from inside the plan), the read finds cursor c1 ≠ the pinned c0, and then the bucket is deleted. The correct answer is a release plus `NoSuchBucket`; the mutant ends in-doubt.
  - Re-pinning on a second held outcome (`complete.rs:1826`; mutant keeps the old pin). Case: in the deep two-batch fixture, round 1 batch 1 is `Hold`; in round 2, batch 1 `Pass` and batch 2 `Unknown{applied:false, may_still_commit:true}`, then the bucket is gone. The fix stays in-doubt with the pin moved to c1. The mutant clears the doubt at c1 and sends a release while batch 2 may still land.
- NEEDS-HUMAN [human] — **The `Open` answer after an unknown publication outcome departs from the brief's wording.** The brief's Design says "`Open` ⇒ answer as a fresh Complete", and `0016:951-953` says "a fresh attempt must re-fence from `Open`". The patch answers `Refused(OperationAborted)` and never re-fences (`complete.rs:220-224`). The new G1-open leg (`crates/core/tests/multipart_complete.rs:5398`), which the brief did not ask for, locks that behavior in. `0016:2544` (X6: "the client retries") and the stale-stamp argument support the patch's reading, so I think it's defensible. But the reviewer should have flagged it, not passed it silently. A related inconsistency: after an unknown **fence** outcome, the fence loop (`complete.rs:874-951`) does fence again from a newer `Open` with the same `fenced_at_millis`. If that unknown fence was this request's own and the reaper rolled it back, the stamp is just as stale. This only happens if a commit stalls past `W_completing` once #625 exists, so it is low priority.
- NEEDS-HUMAN [impl] — **The gate rows are weaker than `overall: pass` suggests.** `C4-diff-cov` never ran: "patch.diff does not apply on origin/main — the bundle is stale" (`gate-logs/C4-diff-cov.log`). Rebase and re-run it; the untested `Doubt` branches above are exactly what it would have shown. `C4-verify`'s "test is co-located with the fix (no separate */tests/*.rs)" is wrong, because the legs live in `crates/core/tests/multipart_complete.rs`. The red→green claim holds only because I reproduced it by hand above. `AGENTS.md` asks that surviving mutants be inspected when a change touches correctness logic, and the 0-blocking T4 row does not mention C5's two survivors.

## Attempted and could not break

- **A byte-identical rival fence (G5), both orders.** A never takes the matching bytes as its own, and B publishes or is refused as the brief requires.
- **A held fence plus a retried fence.** A `Committed` retry requires `Open@E`, which the held fence would have used up, so the retry's success proves ownership and the held fence can no longer land. G6 shows this with `release_held() → Conflict`.
- **The `unknown_flips` byte match (`complete.rs:1509-1514`).** The committer merges the flip contribution unchanged (`staged_publication.rs:928`, `:949-955`), so the `Completed` bytes a read finds are exactly the recorded ones. A different version gives different bytes and a different publication.
- **A held segment batch landing between the read and the retry.** Both require the session's exact bytes, and the segment rows sit under `require_absent`, so only one can land. The next read moves the cursor and clears the pin.
- **DST coverage.** Not raised: it is deferred to #660 by the in-code `// deferred: #660` marker (`complete.rs:783`), which the rubric treats as settled.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:5878`: G6 keeps validation successful while the initial fence is held, so it does not verify that recovery skips pre-fence validation. The frozen `gate-logs/C5-mutants.log` confirms that replacing `Doubt::holds` with `false` at `crates/core/src/multipart/complete.rs:1834` survives. Add a held initial fence whose scripted step deletes the bucket, followed by conflicting retries. Assert `InDoubt` and show the held fence can still land afterward. Otherwise this regression can return definitive `NoSuchBucket` while leaving a fence able to land after the response.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:5632`: G4 ends in a definite successful publication, which bypasses the doubt bookkeeping; it therefore does not verify that a successful reread clears an out-of-flight unknown outcome. The frozen mutation log confirms that removing `Doubt::settle` at `crates/core/src/multipart/complete.rs:1841` survives. Add an unapplied unknown flip with `may_still_commit: false`, a successful reread, conflicting remaining publication rounds, and a successful release. Assert `Refused(OperationAborted)` and an `Open` session; the surviving regression instead returns `InDoubt` and strands the fence until the reaper.

No introduced production logic defect or actionable reuse, simplification, or efficiency issue identified. Findings are advisory regression-test gaps, based on target-source inspection and frozen gate evidence; gates were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add regressions for refusal while an initial fence may still land and termination after doubt is settled — removing either protection survives the full core suite, leaving safety and recovery behavior unprotected (crates/core/src/multipart/complete.rs:912; crates/core/src/multipart/complete.rs:1842; reviewer-mutants.log:3; reviewer-settle.log:3).
- [x] T5 Judgment — Approve interpreting “Open ⇒ answer as a fresh Complete” as OperationAborted followed by a fresh client request — this avoids reusing an expired fence timestamp but changes who performs recovery (brief.md:132; crates/core/src/multipart/complete.rs:220; crates/core/tests/multipart_complete.rs:5398).
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated core recovery and the cost of leaving ambiguous initial fences charged satisfy this slice's purpose — the tests exercise a real store, but this API has no production caller yet (brief.md:125; crates/core/src/multipart/complete.rs:208; crates/core/src/multipart/complete.rs:255).
- [ ] **Fence-phase doubt handling is untested.** Three mutants survive the whole `wyrd-core` suite. I wrote a probe leg for each one; the probe passes on the fix and fails on the mutant:
- [ ] **Publication-phase doubt handling is partly untested.** Three more mutants survive; my probes kill each one:
- [ ] **Two pin rules still have no killing case, even with my probes.** Both need wrapper support the suite lacks:
- [x] **The `Open` answer after an unknown publication outcome departs from the brief's wording.** The brief's Design says "`Open` ⇒ answer as a fresh Complete", and `0016:951-953` says "a fresh attempt must re-fence from `Open`". The patch answers `Refused(OperationAborted)` and never re-fences (`complete.rs:220-224`). The new G1-open leg (`crates/core/tests/multipart_complete.rs:5398`), which the brief did not ask for, locks that behavior in. `0016:2544` (X6: "the client retries") and the stale-stamp argument support the patch's reading, so I think it's defensible. But the reviewer should have flagged it, not passed it silently. A related inconsistency: after an unknown **fence** outcome, the fence loop (`complete.rs:874-951`) does fence again from a newer `Open` with the same `fenced_at_millis`. If that unknown fence was this request's own and the reaper rolled it back, the stamp is just as stale. This only happens if a commit stalls past `W_completing` once #625 exists, so it is low priority.
- [x] **The gate rows are weaker than `overall: pass` suggests.** `C4-diff-cov` never ran: "patch.diff does not apply on origin/main — the bundle is stale" (`gate-logs/C4-diff-cov.log`). Rebase and re-run it; the untested `Doubt` branches above are exactly what it would have shown. `C4-verify`'s "test is co-located with the fix (no separate */tests/*.rs)" is wrong, because the legs live in `crates/core/tests/multipart_complete.rs`. The red→green claim holds only because I reproduced it by hand above. `AGENTS.md` asks that surviving mutants be inspected when a change touches correctness logic, and the 0-blocking T4 row does not mention C5's two survivors.
- [ ] `crates/core/tests/multipart_complete.rs:5878`: G6 keeps validation successful while the initial fence is held, so it does not verify that recovery skips pre-fence validation. The frozen `gate-logs/C5-mutants.log` confirms that replacing `Doubt::holds` with `false` at `crates/core/src/multipart/complete.rs:1834` survives. Add a held initial fence whose scripted step deletes the bucket, followed by conflicting retries. Assert `InDoubt` and show the held fence can still land afterward. Otherwise this regression can return definitive `NoSuchBucket` while leaving a fence able to land after the response.
- [ ] `crates/core/tests/multipart_complete.rs:5632`: G4 ends in a definite successful publication, which bypasses the doubt bookkeeping; it therefore does not verify that a successful reread clears an out-of-flight unknown outcome. The frozen mutation log confirms that removing `Doubt::settle` at `crates/core/src/multipart/complete.rs:1841` survives. Add an unapplied unknown flip with `may_still_commit: false`, a successful reread, conflicting remaining publication rounds, and a successful release. Assert `Refused(OperationAborted)` and an `Open` session; the surviving regression instead returns `InDoubt` and strands the fence until the reaper.
- [x] size backstop — this slice is behaving oversized: patch is 109 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): The production logic holds; close the test gaps. The design (brief) stands. - Kill every surviving mutant the reviewers found, using their probe cases as regressions: fence phase — Doubt::holds -> false (complete.rs ~:912/:1834: held first fence + bucket deleted + conflicting retries -> InDoubt, held fence still lands after); fence `stop` closure ignoring the doubt (~:867/:943: failed mpu: read after an unknown fence with may_still_commit true -> InDoubt{Fence, true}, not Stopped{NotPlaced}); no doubt.settle at ~:877 (unapplied unknown, may_still_commit false, then conflicts -> Stopped{NotPlaced, FenceRoundsSpent}). publication phase — doubt check before a definite refusal (~:1481-1483: [Pass, Hold, Fail{transient}] -> InDoubt{Flip, true}, no release); Doubt::in_doubt ordering (~:1862-1867: first may_still_commit:true outcome is kept); Doubt::settle no-op / no settle at ~:1525 (unapplied unknown flip, unchanged reread, conflicts, release -> Refused(OperationAborted), session Open). - Add the wrapper support needed for the two pin rules and test them: clearing a pinned held doubt once a read finds the cursor moved (~:1847), and re-pinning on a second held outcome (~:1826). - Human decisions at sign-off: refusing with OperationAborted when the reread finds Open (client retries, no re-fence with a stale stamp) is approved, including the G1-open leg; gate-row weakness (diff-cov / green-only verify) is a stacked-base artifact, cleared; the size backstop (109 KB) is overridden — do not split.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
