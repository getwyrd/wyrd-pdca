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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 27 mutants tested in 2m: 21 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.65s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1010: recover multipart Complete from unknown commit outcomes without claiming another request's fence or losing a held commit; no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | G1–G6 distinguish landed, held, unapplied and unread outcomes, and the latest carry-forward explicitly requires damaged reads to preserve doubt; the acceptance contract is falsifiable (`brief.md:25`, `brief.md:173`). |
| C2 Reproduction (red pre-fix) | PASS | Keeping the patched tests while stashing the production changes reproduced 17 failures out of 85, including applied-flip recovery, segment resume and initial-fence retry (`reviewer-red.log:340`, `reviewer-red.log:350`, `reviewer-red.log:382`). |
| C3 Change | PASS | The change satisfies the approved recovery scope, including preserving doubt on decode failure, and updates the living architecture description (`crates/core/src/multipart/complete.rs:895`, `crates/core/src/multipart/complete.rs:1555`, `docs/design/architecture/05-building-block-view.md:252`). |
| C4 Verification (red→green) | PASS | Restoring the patch made all 85 multipart-complete tests and all 669 core tests pass; frozen full-CI evidence supports the broader gate, with the local gitlink limitation documented below (`reviewer-green.log:257`, `gate-logs/C4-ci.log:4841`). |
| C5 Causal adequacy | PASS | Recovery requires attributable publication or a definite fence success, while unresolved held commits prevent release; the byte-identical rival-fence test and all 21 viable mutants support these causal boundaries (`crates/core/src/multipart/complete.rs:963`, `crates/core/src/multipart/complete.rs:1545`, `crates/core/tests/multipart_complete.rs:5849`, `reviewer-mutants-lint-capped.log:4`). |
| T1 Structure | PASS | Recovery remains behind the narrow MetadataStore seam, with request-local doubt bookkeeping and the existing publication committer retaining responsibility for writes (`crates/core/src/multipart/complete.rs:1415`, `crates/core/src/multipart/complete.rs:1491`, `crates/core/src/multipart/complete.rs:1830`). |
| T2 Shape | PASS | The expanded scenarios address the approved test gaps, including held-commit read faults and flip-construction failure; the recorded size exception is settled, and independent formatting, spelling and documentation lint checks pass (`brief.md:173`, `crates/core/tests/multipart_complete.rs:6700`, `crates/core/tests/multipart_complete.rs:7065`). |
| T3 Runtime | PASS | Unknown outcomes consume bounded retry rounds, pending batches remain fenced by exact session bytes, and the caller-owned clock/deadline contract is preserved (`crates/core/src/multipart/complete.rs:246`, `crates/core/src/multipart/complete.rs:934`, `crates/core/src/multipart/complete.rs:1572`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; the mandatory publish-time contribcheck must judge the eventual PR description and commit message (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The latest damaged-read regressions exercise both fence and publication outcomes, affected-path prior art is checked, and recorded scope/deferral decisions remain settled (`crates/core/tests/multipart_complete.rs:6489`, `crates/core/tests/multipart_complete.rs:6544`, `reviewer-prior-art.txt:3`, `crates/core/src/multipart/complete.rs:795`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the observed success, in-doubt and refusal outcomes as meeting the intended multipart service behavior — automated state-transition evidence cannot decide fitness for the intended callers (`brief.md:25`, `crates/core/src/multipart/complete.rs:184`). |

The independent evidence supports the patch's recovery contract. In the disposable `$PDCA_TARGET`, I stashed only the two production Rust files and the architecture document, retaining the new tests, then ran `cargo test --locked -p wyrd-core --test multipart_complete`: 68 passed and 17 failed. After `git stash pop`, `cargo test --locked -p wyrd-core` passed all 669 tests, including all 85 multipart-complete tests. The restored diff is byte-identical to `patch.diff`, and no stash remains. Source citations above resolve against that restored target; input and evidence citations resolve in this review directory.

Mutation testing independently corroborates the frozen result. `cargo mutants --in-diff ../patch.diff -p wyrd-core --test-package wyrd-core`, restricted to the multipart-complete integration suite, tested 27 mutants: 16 caught and 11 unviable (`reviewer-mutants.log:4`). Five were unviable solely because generated replacements triggered unused-code lints; re-running exactly those with `--cap-lints true` caught all five (`reviewer-mutants-lint-capped.log:4`). The remaining six require nonexistent `Default` implementations. Thus 21 viable mutants were caught, with none missed, consistent with `gate-logs/C5-mutants.log:13`. The new checks concern protocol state and commit attribution; the capability-probe/load-time symptom-guard smell-test does not fire.

Verification has two explicit limits, neither a patch defect. The full local `cargo xtask ci` run exited 1 because the synthetic target's index lacks the `.github/advisory-db` gitlink: one xtask test fails and dependency auditing cannot resolve its pin (`reviewer-ci.log:3657`; `xtask/tests/advisory_db.rs:67`). The executed formatting, clippy, build, documentation render, conformance, statics and DST checks passed. For the full gate, I read the frozen log: the dependency audits report success at `gate-logs/C4-ci.log:4208` and `gate-logs/C4-ci.log:4211`, and the gate finishes successfully at `gate-logs/C4-ci.log:4841`. Separately, diff coverage was not measured because the patch does not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`); the brief already settles this stacked-base limitation. Neither limitation is represented as measured coverage or a wholly green local CI run.

The remaining checks are accounted for. The declared external dependency, `typos`, ran successfully independently and in the captured CI gate (`brief.md:92`, `gate-logs/C4-ci.log:11`). Both TiKV feature clippy commands passed independently (`reviewer-tikv.log:104`, `reviewer-tikv.log:203`). The instance-scoped batch-review wrapper was adjudicated from its frozen log, which reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The contribution-artifact check remains N/A until its required publish-time re-run.

Prior art was checked by all four affected paths against GitHub's default-branch commit history and the file lists of all 19 closed, unmerged PRs. The histories include the merged Complete fence (#1032) and Abort precedent (#909); none of those 19 rejected/closed PRs touches an affected path (`reviewer-prior-art.txt:3`). The standing rubric's clock, seam, decode-boundary, bounded-await and test-fidelity requirements were examined. The explicit DST deferral to #660 and the approved client-retry and size decisions are settled, not new findings.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

**Verdict:** I could not break the production logic. Every attack on fence ownership, the
held/unsettled bookkeeping, resume and release failed. One real mismatch is left: in the fence
phase, the in-doubt error can say a fence "may still be applied" right after a read has shown it
can't land. That is the safe direction (it over-warns, never under-warns), but it contradicts the
documented contract of a public field.

## Evidence re-run (scratch copy of `$PDCA_TARGET`, `cargo test -p wyrd-core --test multipart_complete`)

- **Green:** 85/85 pass on the patched tree.
- **Red:** with the patched test file over the pre-fix `crates/core/src/multipart/complete.rs`,
  17 of the 23 new legs fail: G1, G1-open, G2, G2-first, G2-refused, G2-settled, G3-fence, G4,
  G4-landed, G4-repin, G6, G6-spent, both G6-held legs that clear or refuse, G6-held no-check, and
  both G6-rolled-back legs. The 6 that pass pre-fix (G3 flip, G5, both G3-torn, G2-unread,
  G4-unbuilt) all expect the in-doubt error, which pre-fix code returned for every unknown
  outcome. They guard against recovering too eagerly, so passing pre-fix is expected. The C4-verify
  row is green-only (its log shows only the GREEN run), as the brief's posture (b) says. The red
  side above is now reproduced independently.
- **G5's red rests on mutations, so I ran two by hand** at `complete.rs:963-965`. (a) Treating the
  matching fence bytes as ownership (`break (opened, placed)`). (b) Dropping the check. Both turn
  G5 and the G0 applied-fence leg red.
- **The wrapper is faithful to a real store.** `Hold`/`Land`/`release_held` apply the held batch
  through the inner redb store's `commit` (`crates/core/tests/multipart_complete.rs:572-575`,
  `:620-627`). So a late batch re-checks its `require`s, and every "can no longer land" assertion
  is a real CAS (compare-and-swap) result, not a copy of production logic.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:963-965` returns the in-doubt error
  before `doubt.settle` runs. So a held first fence is reported with `may_still_commit: true` even
  when the read just found the session off the bytes that fence requires. Concrete case, with the
  test file's wrapper: script `[OnCommit::Hold, OnCommit::Unknown { applied: true,
  may_still_commit: false }]`, then `up.complete(&named(&PARTS))`. It returns
  `InDoubt { commit: Fence { epoch: 1 }, may_still_commit: true }`, and the text says "its fence at
  Completing@1 may still be applied". `up.store.release_held()` afterwards answers `Conflict`: the
  held fence requires `Open@0`, which never comes back. The same happens with
  `[Hold, OnCommit::land(OnCommit::Pass)]`: the held fence has already landed before the request
  returns, yet the error says it may still be applied after. This contradicts three places that
  say a held doubt lasts only "until a read finds the session off the bytes ... requires":
  - the field doc at `complete.rs:527-530`;
  - the module doc at `complete.rs:189-194`;
  - the living doc at `docs/design/architecture/05-building-block-view.md:252`.

  Abort keeps the rule exactly (`abort.rs:259-264`); Complete's publication phase also settles
  before it ends (`complete.rs:1560`). Safety is not affected: the G5 rule (never claim ownership)
  still returns the in-doubt error. But #670's same-gateway recovery reads this flag. Fix
  options:
  - report `false` when the read finds the fence's own bytes (any held fence's required `Open`
    bytes are then gone for good);
  - or narrow the three doc sentences to say a held outcome stays reported when the read finds the
    fence's own bytes.

  Either way, add the probe above as a regression.

## Attempted to refute; could not

- **Stale re-read after `may_still_commit: false`** (the fence landed, the read shows `Open@E`, so
  the check before the fence answers and leaves the fence standing). The store contract rules this
  out: one re-read settles an out-of-flight outcome (`crates/traits/src/lib.rs:240-243`).
- **Unknown outcome lost inside the committer.** `staged_publication.rs:457` and `:464` pass the
  store's error through `?` unchanged, so the downcast at `complete.rs:1500` sees
  `CommitUnknownResult`. It is never read as a definite failure that releases.
- **The pin rule for publication-round holds** (`complete.rs:1507`, `:1560`, `:1878-1894`). Within
  a round the committer stops at the first unknown outcome, so the held batch requires the bytes
  the last committed batch left. The read after it finds those bytes, or the bytes after them if
  it landed. Re-pinning on a second hold (`:1855-1859`) and clearing once a later round's batch
  moved the cursor both hold. Every case is either exact or conservative.
- **`unknown_flips` matching an older flip** (`complete.rs:1545-1550`). An earlier round's flip
  pins the target it read. If the target moved, that flip can't land. If it didn't, both rounds'
  `Completed` bytes are identical. So keeping every flip is harmless, and only flips of this owned
  fence can match.
- **`open_and_charged_since`** (`complete.rs:1007-1012`): reaper rollback; rollback followed by a
  rival's give-back; a rival's charged release; a torn or missing record. Each one answers
  `OperationAborted`, loops into the in-doubt error, or returns `NoSuchUpload`. A re-fence with the
  original stamp happens only after a give-back, which the human approved in iteration 2.
- **Answers given while a held fence is kept** (`complete.rs:897-933`). A held doubt survives
  `settle` only when the read is exactly the bytes it required. Those bytes passed every check in
  the round that sent it, and the optimization skips itself (`:927`).
- **Termination.** Each fence commit, unknown ones included, spends a round (`:934-937`). Each
  publication round, unknown ones included, counts against `R_PUBLISH` (`:1572`). The release stays
  bounded.
- **Citations:** `multipart.rs:2276-2285` (the `Completing` variant) and `abort.rs:259-273`
  resolve correctly on the patched tree.

## Note (no action asked)

- `complete.rs:1897-1905` (`Doubt::in_doubt`) always passes `fault_behind_fence: None`. So when a
  held doubt ends publication on a read fault, the fault is dropped from the error. Probe:
  `[Pass, arm(FailGets(bucket_key(BUCKET)), Hold)]` gives `InDoubt { Flip, true }` with no mention
  of the marker-read fault. The field's doc limits it to a fault that led to a release, so this is
  consistent. Carrying the cause would touch the error type, which belongs to child-1. I'm
  recording it, not raising it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. No introduced correctness bugs or actionable reuse, simplification, or efficiency issues were identified in this diff.

Reviewed the changed implementation and tests against the read-only source at `$PDCA_TARGET`, with the frozen gate evidence. Checks were not rerun; the brief's settled decisions and tracked deferrals were respected.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the observed success, in-doubt and refusal outcomes as meeting the intended multipart service behavior — automated state-transition evidence cannot decide fitness for the intended callers (`brief.md:25`, `crates/core/src/multipart/complete.rs:184`).
- [x] `crates/core/src/multipart/complete.rs:963-965` returns the in-doubt error before `doubt.settle` runs. So a held first fence is reported with `may_still_commit: true` even when the read just found the session off the bytes that fence requires. Concrete case, with the test file's wrapper: script `[OnCommit::Hold, OnCommit::Unknown { applied: true, may_still_commit: false }]`, then `up.complete(&named(&PARTS))`. It returns `InDoubt { commit: Fence { epoch: 1 }, may_still_commit: true }`, and the text says "its fence at Completing@1 may still be applied". `up.store.release_held()` afterwards answers `Conflict`: the held fence requires `Open@0`, which never comes back. The same happens with `[Hold, OnCommit::land(OnCommit::Pass)]`: the held fence has already landed before the request returns, yet the error says it may still be applied after. This contradicts three places that say a held doubt lasts only "until a read finds the session off the bytes ... requires":
- [x] size backstop — this slice is behaving oversized: patch is 171 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
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
- #1010 follow-up: fence phase returns InDoubt before doubt.settle (complete.rs:963-965), so may_still_commit can over-report true; #670 must not trust it unchecked — fix the code or narrow the three doc sentences, with the adversary's probe as a regression.
