# Result — issue 1029 / multipart-part-commit-settle-unknown

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.2b a commit answered `CommitUnknownResult` returns the in-doubt error at once, even
  when a re-read could show that its own commit landed, or that it did not and never can. A client then
  sees an in-doubt answer for a part that committed, and a slot stays held. This slice settles the outcome
  from re-reads where the evidence allows it, and otherwise keeps the doubt.
- Success criterion: Appended to `crates/core/tests/multipart_upload_part_commit.rs`.
  **(B) Settling.**
  - A commit that lands but answers unknown: `Committed`, slot released exactly once.
  - A re-upload that did not land (`may_still_commit: false`): the re-read finds the **prior** part. Never
    `Committed` for it; it ends with its own part committed.
  - **A held batch** (`may_still_commit: true`, landed later): no `Committed` and no compensation on that read
    alone. Every later batch requires its own slot at the bytes the held batch requires, so at most one
    lands. Three runs: (i) released before the next batch; (ii) released after the attempt has returned; (iii)
    every later batch `Conflict` unapplied until the rounds run out, then released. In every run exactly one of
    {its own part, its compensation} is durable at the end, the slot is released once, and `Committed` is
    answered only if its own part landed. (iii) returns the in-doubt error keeping the flag.
  - A same-number rival with this attempt's commit not applied, where the commit required the key **absent**
    (run once per `may_still_commit` value): a slot read made **after** the read that found the rival finds the
    slot still held. Never `Committed`; it compensates and answers `Refused(OperationAborted)`.
  - **A changed prior settles nothing** (decided at sign-off, iteration 4). A held re-upload over a non-empty
    prior that loses to a rival answers the in-doubt error with no compensation. A held re-upload while a repair
    moves the prior away and back, in real re-place batches or their exact shape: no compensation while the
    commit may land, never a false `Refused(OperationAborted)`, slot released once. A rival over an empty prior
    settles nothing, because those bytes can come back.
  - **The read-gap leg.** A non-empty commit is held. At **each** gap between the classifying reads the wrapper
    runs one of four histories. *Landed, then a rival:* no compensation, `Committed`, slot released once.
    *Landed, then a rollback:* no compensation, the in-doubt error, never `Refused(OperationAborted)`. *Not
    landed, a rival* (the commit required the key absent) and *not landed, a rollback:* the attempt reaches a
    slot read made after it saw the change, then compensates and answers `Refused(OperationAborted)`; the held
    batch, released later, answers `Conflict`.
  **(C) Regressions.**
  - **C1.** With a fixed clock, an empty part 1 commits. An empty re-upload in the same millisecond and epoch
    answers unknown, **not applied** (once per flag). Its bytes equal the prior's, and the answer must not be
    `Committed` on them. On return the slot is released or covered by the reported doubt, and the held-slot count
    equals the live attempts (`0016:1779`). Same for an empty rival in the same millisecond.
  - **C2, a torn read.** A commit held and never applied. At **each** read gap the wrapper runs a real Abort and
    deletes the session's `sidx:` and `slot:` records. Never `Committed`; `NoSuchUpload` or the in-doubt error.
  - **C3.** A commit **lands**, answers unknown with `may_still_commit: false`, and a rival replaces the part
    before the re-read: `Committed`. **C3′:** then a third attempt reserves the freed index, so the slot key holds
    another `attempt_id`: still `Committed`, and that attempt's slot is byte-for-byte unchanged.
  - Every commit round answered unknown with `may_still_commit: false`, not applied: it compensates and returns
    `Stopped(CommitRoundsSpent)` reporting the release.
  - **The kept doubt is the round that may still land:** round 1 held, round 2 unknown with `false`, then a read
    faults. The answer keeps `may_still_commit: true`.
  - A slot rewritten under the attempt's own id during a held commit: `InDoubt(SlotLost)`.
  - A slot gone under a moved session: the in-doubt error, never `Committed`, never a compensation.
  **(F) The lease.** A commit in doubt when the lease runs short: no further commit or compensation, the in-doubt
  error. **Delayed after dispatch:** a commit sent with the margin, held past the lease's expiry. (i) It lands before
  the re-read: `Committed`. (ii) It lands after return: the in-doubt error; the part ends durable. (iii) A test-local
  batch in the idle fence's shape (`0016:1859-1863`) lands first: the held batch answers `Conflict`, no `part:`
  exists, the answer is `NoSuchUpload` or in-doubt. (iv) The held batch lands, then that fence: it answers
  `Conflict`, the session stays `Open@E`. No compensation in any of them.
  **(H) A held commit that lands, then a rollback,** both right after the slot read. The next slot read finds the
  slot gone under a moved session, which proves nothing about whose batch removed it: the in-doubt error, no
  compensation, the slot released once, and the attempt's own part durable. This is the read-gap leg's *landed, then a
  rollback* at one named gap. **If the commit has not landed** when the rollback does, that is *not landed, a rollback*,
  and the attempt compensates and answers `Refused(OperationAborted)`.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a
  record at the same or a lower epoch, `Open` or not, and a missing `mpu:` with any record at the slot key: `InDoubt` keeping
  `may_still_commit`. Repairing the record mid-round must not produce `Refused(OperationAborted)` or a compensation,
  and a repaired record lets the held batch land.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: what a commit's `CommitUnknownResult` means: the own-commit proof, settling, the kept doubt, and retrying
  the same batch. / out of scope:
  - a **compensation's** unknown outcome: 657.2a's in-doubt stop stays until 657.2d;
  - the damage-only false `Committed` with the slot erased (657.2d);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs, including the wording at `0016:1727-1728`; `Cargo.lock`
    and `deny*.toml`; the gateway and any production caller; Complete, the drain, the reaper, DST.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 37 mutants tested in 3m: 1 missed, 26 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.54s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1029: settle unknown multipart part commits from evidence of the attempt's own outcome, retaining doubt when that evidence is insufficient — no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The own-attempt proof and compensation prohibition define a falsifiable safety boundary across rival, rollback, damage and lease histories (`brief.md:74`, `brief.md:145`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, the landed-unknown and unapplied-reupload regressions both failed on the unexpected in-doubt answer, after successful compilation (`review-red.log:9`, `review-red.log:19`; target tests at `crates/core/tests/multipart_upload_part_commit.rs:4160`, `:4190`). |
| C3 Change | PASS | The three-file change fits the authorized settlement slice and updates its public error documentation and living architecture (`crates/core/src/multipart/commit.rs:378`, `docs/design/architecture/05-building-block-view.md:250`; `brief.md:87`). |
| C4 Verification (red→green) | PASS | Restoring the patch made the identical eight-test probe pass; independently, all 632 core tests passed, including 67 commit tests; full CI is supported by its frozen log, while diff coverage remains unmeasured (`review-probe-green.log:17`, `review-checks.log:8`, `review-green.log:244`, `gate-logs/C4-ci.log:4804`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | Settlement requires both irreversible loss of a precondition and a later held-slot read; success requires slot loss followed by the original session, preventing the specified false success and premature compensation (`crates/core/src/multipart/commit.rs:1183`, `:1204`, `:1217`, `:1237`). |
| T1 Structure | PASS | The attempt-local doubt state and narrow metadata seam preserve the architecture's dependency direction; runtime and backend composition stay outside the decision logic (`crates/core/src/multipart/commit.rs:768`, `:1167`, `:1271`). |
| T2 Shape | PASS | Error classification remains indeterminate for uncertain writes, the architecture documents the changed contract, and formatting, spelling and docs lint pass (`crates/core/src/multipart/commit.rs:543`, `docs/design/architecture/05-building-block-view.md:250`, `review-checks.log:3`). |
| T3 Runtime | PASS | Four dispatch rounds, at most one extra read cycle, and the attempt's single clock bound work without treating lease expiry as outcome evidence; metadata calls inherit the terminating backend contract (`crates/core/src/multipart/commit.rs:299`, `:1332`, `:1351`, `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit is owed at publish and is not waived (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The sole mutation survivor changes an unreachable fallback, not observable behavior; affected-path merged history and all 19 closed/unmerged PR file lists reveal no conflicting rejected work (`crates/core/src/multipart/commit.rs:1332`, `review-mutants.log:3`, `review-prior-art.json:1`, `review-closed-pr-files.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated settlement behavior fulfills #1029's intended multipart lifecycle — the real-store histories establish this slice's behavior, while production S3 use belongs to #669 (`brief.md:114`, `crates/core/tests/multipart_upload_part_commit.rs:4582`). |

The implementation is supported by independent behavioral reproduction, adversarial store histories, and review of the temporal proof. No builder-fixable concern emerged from the logic, test-fidelity, or standing-rubric passes. Source citations above resolve in `$PDCA_TARGET`; brief, log and review-evidence citations resolve in this review directory.

- **Behavior reproduced independently.** I copied the base test harness into a temporary integration target, appended the two unmodified B regressions, stashed the supplied patch, ran the probe, restored the patch, and ran the identical probe again. Six existing B tests passed on both trees; the two new tests failed before the fix and passed afterward (`review-red.log:34`, `review-probe-green.log:17`). The temporary probe is preserved as `pdca-reviewer-1029-probe.rs` outside the target. The final `cargo test --offline --locked -p wyrd-core` passed 632 tests. The original target diff was restored byte-for-byte (`review-checks.log:7`).
- **Safety assertions exercise the forbidden histories.** The wrapper delays real redb batches and releases them through the real precondition checks (`crates/core/tests/multipart_upload_part_commit.rs:522`); its read hooks preserve the value observed before the competing action (`:706`). The tests force every classification gap (`:4582`), identical empty-part bytes (`:4683`), slot reuse (`:4799`), delayed expiry (`:5003`), rollback after landing (`:5074`), and damaged/repaired records (`:5133`, `:5332`). A changed prior cannot discharge doubt because staged reconstruction can restore its bytes (`crates/custodian/src/reconstruction/staged.rs:723`). No capability probe or load-time symptom guard was introduced. Seeded DST coverage is explicitly deferred to #660 (`crates/core/src/multipart/commit.rs:258`); that settled deferral is not re-raised.
- **Mutation and provenance evidence have explicit limits.** The independent scan tested the same 37 mutants: initially 22 caught, 14 unviable, one missed (`review-mutants.log:4`). Four were rejected only by warning-denial; rerunning those with `--cap-lints true` caught all four, yielding the frozen scan's effective 26 caught / 10 unviable / one survivor (`review-lint-mutants.log:3`, `gate-logs/C5-mutants.log:14`). The survivor replaces `ReadAgain if send` with `ReadAgain if true`. Entering the read-only lap means the previous session read already marked every outstanding round unable to land; no batch can add doubt on that lap. Its held-slot read clears all doubt, while gone, rewritten or damaged slots return through other branches. A second `ReadAgain` is therefore unreachable (`crates/core/src/multipart/commit.rs:1204`, `:1213`, `:1237`, `:1316`).

Full workspace CI and TiKV feature compilation are adjudicated from the supplied instance-scoped gate logs, not claimed as independently rerun (`gate-logs/C4-ci.log:4804`, `gate-logs/host-tikv.log:211`). The named external dependencies were exercised: the CI log shows `typos` and successful docs rendering/link audit (`gate-logs/C4-ci.log:11`, `:16`); spelling and docs lint also passed independently. The frozen batch review reports zero findings (`gate-logs/T4-batch-review.log:10`); it does not expose the underlying individual reviews.

The coverage failure is a base-state caveat, not a demonstrated patch defect: it stopped before measurement because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`), consistent with the declared #1028 dependency (`brief.md:81`). This disposable target contains the prerequisite and the applied patch; reverse-application validation and independent compilation succeed (`review-checks.log:1`, `review-core.log:1`). No coverage percentage is asserted.

Prior art was checked by all three affected paths against GitHub's merged history and closed PR search, then against the actual changed-file lists of all 19 closed, unmerged PRs. The commit implementation and tests trace to [PR #1041](https://github.com/getwyrd/wyrd/pull/1041), the earlier #1027 slice; none of those 19 rejected/closed changes touches any of this patch's three paths. Queries and results are recorded in `review-prior-art.json:1` and `review-closed-pr-files.json:1`. The brief's correction to the earlier #927 design is respected (`brief.md:134`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1029 (657.2c), a part commit answered `CommitUnknownResult`

Verdict: I tried to break the settle logic and could not. I did find one real test gap: five of the six places that keep the doubt on a store fault have no test, so a regression there would ship green. Toolchain was available; everything below was re-run on a scratch copy of `$PDCA_TARGET`.

## Findings

- NEEDS-HUMAN [impl] — **Five doubt-keeping store-fault sites have no test.** At `crates/core/src/multipart/commit.rs:1325` (a definite failure of a later round), `:1197` (first session read), `:1202` (slot read), `:1211` (last session read) and `:741` (`session_gone`'s slot re-read), I replaced `doubt.end(upload_id, Stop::Store(fault))` with `stopped(upload_id, Stop::Store(fault))`, one site at a time. All 67 tests in `crates/core/tests/multipart_upload_part_commit.rs` still pass for each one. Only the part-read site (`commit.rs:1179`) is covered, by `c_the_kept_doubt_is_the_round_that_may_still_land` (`multipart_upload_part_commit.rs:4871`). The only commit `Act::fail()` in the file (`:2352`) fails round 1, when nothing is in doubt. **Concrete failing case:** round 1 `Act::hold()`, round 2 `Act::fail()`. The patched code answers `InDoubt { may_still_commit: true, cause: Store }`; I checked this with a throwaway test. The mutant answers `Stopped { cause: Store("scripted store fault"), slot_released: false }`, and `release_held()` then lands round 1 as `Committed`. `classify` (`crates/traits/src/lib.rs:786`) would read that mutant error as Terminal (or Transient for a `TransientFault`) instead of Indeterminate. So the caller hears "nothing happened" while the batch lands. The production code is right today; the tests just don't pin it. Fix: add the hold-then-fail leg, plus a read fault under a held round at each of the other four positions, asserting `InDoubt`, `may_still_commit: true` and no compensation.

## The evidence, re-checked

- **The red→green proof is real, but the gate didn't show it.** `check-gates.json`'s C4-verify row says `PASS (green-only)` because "the test is co-located with the fix (no separate */tests/*.rs)". That reason is wrong: the test is `crates/core/tests/multipart_upload_part_commit.rs`, appended, not added, so the script presumably skipped it. I ran the red myself. With the old `commit.rs` and the new tests, the build fails with 34 compile errors (the new tests use `InDoubt::cause` and `Stop::SessionMoved`). With an API-only shim on the old code (the field and two variants added, old logic kept), 23 of the 67 tests fail, all of them new #1029 legs. The patched tree passes 67 of 67. The tests drive the production `commit_part` over a real redb store. The wrapper releases held batches through `inner.commit` (`multipart_upload_part_commit.rs:522-535`), so preconditions are re-checked when the batch lands; this is not a parallel copy of production.
- **C4-diff-cov never ran.** Its log says "patch.diff does not apply on origin/main": the bundle is stacked on #1028 (`Depends on: 1028`). So changed-line coverage was never measured. My mutation pass (below) partly fills that hole. Re-run diff coverage once #1028 merges and the branch is rebased.
- **The C5 survivor at `commit.rs:1332:34` (`ReadAgain if send` → `true`) is an equivalent mutant (no test can kill it).** A no-send lap only follows a last session read that called `cannot_land()` (`:1213` → `:785`), so every round has `may_land == false`. The next slot read that finds the slot held then empties the doubt via `slot_held()` (`:1206`), and `ReadAgain` needs `doubt.any()` (`:1237`). So the arm at `:1338` is unreachable, and the mutant changes nothing.

## Refutation attempts that failed

- **The own-commit proof** (`commit.rs:1217`). I looked for any writer other than this attempt's commit that could clear slot `k` while `mpu` still holds the `Open@E` bytes. Reserve requires the slot absent (`crates/core/src/multipart/stage.rs:1332`). Abort, Complete and the idle fence move `mpu` first. The attempt never renews after quiesce. The epoch only rises, so a later `Same` read means the session was `Open@E` at the slot read too. I found no other writer. The damage-only slot erasure is out of scope (657.2d).
- **The settle order.** I traced *landed / not landed* × *rival / rollback* at each of the three read gaps. Evidence found after the slot read (`:1213`) only counts at the next slot read. A mutant that lets it settle at the same slot read is caught by `h_a_commit_held_while_a_rollback_lands_is_in_doubt` and `b_read_gap_…`.
- **`may_still_commit` is never `true` where the store said `false`.** `may_land` starts from the store's flag (`:774`) and only ever goes to `false`.
- **29 distinct mutations of my own** (19 targeted, plus each of the 11 doubt-keeping error sites in turn; one overlaps). They include: kept doubt via `.last()`; `Left` ignored in `session_seen`; `cannot_land` dropped in `session_gone`; `ReadAgain` → `Again`; the `ReadAgain` arm removed; the `doubt.any()` guard dropped from the proof; damage checked before the proof; the first session read ignored; a rival that settles nothing; a changed prior that settles; `slot_held` clearing every round; the lease-short and rounds-spent ends compensating; the moved-and-gone arm removed; the flag forced `true`; and the damage-keeping sites at `:735`, `:1221`, `:1223`, `:1224`, `:1229`, `:1248`. All caught except the five store-fault sites in the finding above.
- **Rubric checks.** No new clock read: the lease check reuses the attempt's clock (`:1351`). `source()` keeps `CommitUnknownResult` ahead of the stop cause, so a fault never hides the doubt. Docs currency is met at `docs/design/architecture/05-building-block-view.md:250`. No production caller was added.
- **Not raised:** DST coverage for this path. The brief scopes DST out and the path has no production caller.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No actionable findings: the diff is clean on both advisory lenses—introduced correctness bugs and reuse, simplification, or efficiency issues.

The C5 survivor at `crates/core/src/multipart/commit.rs:1332` is equivalent on reachable paths: `ReadAgain` marks every outstanding round unable to land, sends no batch, and the next held-slot read clears those rounds; other slot outcomes terminate. The guarded fallback cannot be reached again.

Reviewed the implementation, scripted interleavings, and frozen gate evidence against the target source. CI passed; verification was green-only and diff coverage was unavailable. No builds or tests were rerun during this read-only review.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated settlement behavior fulfills #1029's intended multipart lifecycle — the real-store histories establish this slice's behavior, while production S3 use belongs to #669 (`brief.md:114`, `crates/core/tests/multipart_upload_part_commit.rs:4582`).
- [ ] **Five doubt-keeping store-fault sites have no test.** At `crates/core/src/multipart/commit.rs:1325` (a definite failure of a later round), `:1197` (first session read), `:1202` (slot read), `:1211` (last session read) and `:741` (`session_gone`'s slot re-read), I replaced `doubt.end(upload_id, Stop::Store(fault))` with `stopped(upload_id, Stop::Store(fault))`, one site at a time. All 67 tests in `crates/core/tests/multipart_upload_part_commit.rs` still pass for each one. Only the part-read site (`commit.rs:1179`) is covered, by `c_the_kept_doubt_is_the_round_that_may_still_land` (`multipart_upload_part_commit.rs:4871`). The only commit `Act::fail()` in the file (`:2352`) fails round 1, when nothing is in doubt. **Concrete failing case:** round 1 `Act::hold()`, round 2 `Act::fail()`. The patched code answers `InDoubt { may_still_commit: true, cause: Store }`; I checked this with a throwaway test. The mutant answers `Stopped { cause: Store("scripted store fault"), slot_released: false }`, and `release_held()` then lands round 1 as `Committed`. `classify` (`crates/traits/src/lib.rs:786`) would read that mutant error as Terminal (or Transient for a `TransientFault`) instead of Indeterminate. So the caller hears "nothing happened" while the batch lands. The production code is right today; the tests just don't pin it. Fix: add the hold-then-fail leg, plus a read fault under a held round at each of the other four positions, asserting `InDoubt`, `may_still_commit: true` and no compensation.
- [ ] **The rollback criteria require opposite outcomes for the same history.** `brief.md:33-38` requires a held, unapplied commit interrupted by a rollback at *each* read gap to reach a later slot read, compensate, and return `Refused(OperationAborted)`. But H (`brief.md:61`) requires a held commit interrupted by a rollback immediately after the slot read to return in doubt without compensation. With the slot unchanged, reads succeeding, and lease/round budget remaining, that history satisfies both descriptions. The design itself requires the extra slot read (`brief.md:154-156`); the cited target peer likewise re-reads after detecting the session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise H to specify what prevents settlement—such as exhaustion before a confirming read, a failed confirming read, or the commit landing before the rollback—or align its expected result with the read-gap criterion. Otherwise the two required tests can demand incompatible behavior.
- [ ] size backstop — this slice is behaving oversized: patch is 146 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Close the test gap found by the adversary review; the production logic is judged correct, so do not change it. - Add tests pinning the doubt-keeping store-fault sites in crates/core/src/multipart/commit.rs: :1325 (a definite failure of a later round), :1197 (first session read), :1202 (slot read), :1211 (last session read), :741 (session_gone's slot re-read). Today replacing `doubt.end(upload_id, Stop::Store(fault))` with `stopped(...)` at any of them leaves all 67 tests green. - Start with the concrete leg: round 1 Act::hold(), round 2 Act::fail() -> InDoubt { may_still_commit: true, cause: Store }, no compensation; then a read fault under a held round at each of the other four positions, asserting InDoubt, may_still_commit: true and no compensation. - Size backstop (146 KB > 100 KB) is overridden by the human: the slice is one outcome and its size is test volume, so do NOT split; keep the patch otherwise as is.
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
