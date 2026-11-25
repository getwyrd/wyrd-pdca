# Result — issue 1029 / multipart-part-commit-settle-unknown

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.2b a commit answered `CommitUnknownResult` returns the in-doubt error at once, with no
  read (`crates/core/src/multipart/commit.rs:1065`, `// deferred: #1029`), even when a re-read could show that
  its own commit landed. A client then sees an in-doubt answer for a part that committed. This slice answers
  `Committed` when a re-read proves this attempt's own commit landed, and otherwise keeps the doubt. After an
  unknown outcome it sends **nothing more**: no compensation and no second commit. The store holds no evidence
  that such a commit did **not** land that a late staging batch of the same attempt, or a rival writing the same
  bytes, cannot recreate. So a compensation could orphan a landed part, and a resent commit could land twice.
- Success criterion: Appended to `crates/core/tests/multipart_upload_part_commit.rs`. **Across every leg below
  in which a commit round was answered unknown, the attempt sends no further batch after that round (the wrapper
  records no second commit batch and no compensation batch from it), and the answer is never
  `Refused(OperationAborted)`.**
  **(B) Settling.**
  - A commit that lands but answers unknown (once per `may_still_commit` value): `Committed`, slot released exactly once.
  - A re-upload that did not land (`may_still_commit: false`): the re-read finds the **prior** part. Never `Committed`
    on it: the in-doubt error with `may_still_commit: false`, the slot still held, one commit batch. (This is the
    availability cost the *Plan review* section names.)
  - **A held batch** (`may_still_commit: true`, landed later): no `Committed` on a read that finds the slot held. Two
    runs: (i) released before the re-read: `Committed`; (ii) released after return: the in-doubt error keeping
    `may_still_commit: true`, then the held batch lands. In both runs there is one commit batch, it releases the slot
    once, and the attempt's own part is durable.
  - **A same-number rival**, this attempt's commit not applied, the commit having required the key **absent** (once per
    `may_still_commit` value): never `Committed`; the in-doubt error with `may_still_commit: false`; the slot still held.
    In the held run, the held batch released later answers `Conflict`.
  - **A changed prior settles nothing.** A held re-upload over a non-empty prior that loses to a rival: the in-doubt
    error keeping `may_still_commit: true`. A held re-upload while a repair moves the prior away and back, in real
    re-place batches or their exact shape: the in-doubt error keeping `may_still_commit: true`; the held batch, released
    once the prior is back, lands: own part durable, slot released once. A rival over an empty prior: the in-doubt error.
  - **The read-gap leg.** A non-empty commit is held. At **each** gap between the classifying reads the wrapper runs
    one of four histories. *Landed, then a rival:* `Committed`, slot released once. *Landed, then a rollback:* the
    in-doubt error. *Not landed, then a rival* (the commit required the key absent) and *not landed, then a rollback:*
    the in-doubt error, the slot still held, and the held batch, released later, answers `Conflict`.
  **(X) The cross-phase leg** (iteration 2's rejection; it must fail on iteration 2's patch). Fixed clock. `Reserve`,
  `Intent` and the quiesce `Renewal` are each scripted `[hold, pass]`; one chunk is staged; the commit applies but
  answers unknown (once per `may_still_commit` value). Before the commit's re-read, the three held batches are released
  in that order. The test asserts each answers `Committed`, and that the slot then holds exactly the bytes the commit
  required and the owned entry is back, so the history is the real one. The answer is the in-doubt error; no `retire:`
  obligation names the attempt's own chunks; the attempt's own part is durable.
  **(X2) The replay leg** (the plan review's finding; it must fail on iteration 2's patch). Fixed clock and `Open@E`. An
  empty part 1, P, commits first. A is a non-empty re-upload of part 1 staged with (X)'s `[hold, pass]` reserve, intent
  and renewal. B is a same-number **empty** attempt in the same millisecond and epoch, so the part B writes is
  byte-for-byte P. Two runs:
  - **(a) Applied, before the re-read** (once per `may_still_commit` value). A's commit applies but answers unknown. At
    the gap before A's re-read, A's three held staging batches are released in order, then B commits over A's part,
    installing a `retire:` obligation, keyed by B's attempt, that owes A's chunks. The test asserts each of those four
    batches answers `Committed`, and that the part key then holds P's exact bytes and A's slot the exact bytes A's commit
    required, so the history is the real one. A's answer is the in-doubt error.
  - **(b) Held, then released after return.** A's commit is scripted held (`may_still_commit: true`). After A returns,
    the test releases A's held staging batches in order, then commits B, then releases A's held commit.
  In both runs: the wrapper records **exactly one** commit batch from A; at most one of A's commit batches lands; and at
  the end no `part:` record names a chunk that a `retire:` obligation owes.
  **(C) Regressions.**
  - **C1.** With a fixed clock, an empty part 1 commits. An empty re-upload in the same millisecond and epoch answers
    unknown, **not applied** (once per flag). Its bytes equal the prior's: never `Committed` on those bytes; the in-doubt
    error, the slot still held. Same-millisecond empty rival, the commit having required the key absent, not applied:
    the in-doubt error. On return, every slot still held belongs to an attempt that is live or returned the in-doubt
    error (`0016:1779`).
  - **C2, a torn read.** A commit held and never applied. At **each** read gap the wrapper runs a real Abort and deletes
    the session's `sidx:` and `slot:` records. Never `Committed`; `NoSuchUpload` or the in-doubt error.
  - **C3.** A commit **lands**, answers unknown with `may_still_commit: false`, and a rival replaces the part before the
    re-read: `Committed`. **C3′:** then a third attempt reserves the freed index, so the slot key holds another
    `attempt_id`: still `Committed`, and that attempt's slot is byte-for-byte unchanged.
  - **One batch, then reads.** For each `(applied, may_still_commit)` pair: exactly one commit batch and no compensation;
    applied: `Committed`; not applied: the in-doubt error keeping the flag, the slot still held. This reworks 657.2b's
    `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` (stack base, test file `:2581`) and keeps its
    `no retry` assertion.
  - **A conflict, then a held round.** Round 1 answers a `Conflict` that no read explains (657.2b sends the same batch
    again), round 2 is held, then a read faults: the in-doubt error keeping `may_still_commit: true`, no third batch; the
    held round then lands once.
  - **Read faults keep the doubt.** A read fault under a held commit at **each** read of the re-read (the part read, the
    first session read, the slot read, the last session read, and the slot re-read behind a missing session): the
    in-doubt error, `may_still_commit: true`, the fault as its cause. In each, the held batch is then released and lands:
    own part, one release.
  - A slot rewritten under the attempt's own id during a held commit: `InDoubt(SlotLost)`.
  - A slot gone under a moved session: the in-doubt error, never `Committed`.
  **(F) The lease, delayed after dispatch.** A commit sent with the margin, held past the lease's expiry. (i) It lands
  before the re-read: `Committed`. (ii) It lands after return: the in-doubt error; the part ends durable. (iii) A
  test-local batch in the idle fence's shape (`0016:1859-1863`) lands first: the held batch answers `Conflict`, no
  `part:` exists, the answer is `NoSuchUpload` or in-doubt. (iv) The held batch lands, then that fence: it answers
  `Conflict`, the session stays `Open@E`.
  **(H) A held commit and a rollback, both right after the slot read.** If the commit lands first: the next slot read finds
  the slot gone under a moved session, which proves nothing about whose batch removed it: the in-doubt error, slot released
  once, own part durable. If the rollback lands first: the in-doubt error, the slot still held, and the held batch,
  released later, answers `Conflict`.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a record at
  the same or a lower epoch, `Open` or not, and a missing `mpu:` with any record at the slot key: `InDoubt` keeping
  `may_still_commit`. Repairing the record mid-round must not produce a compensation, and a repaired record lets the held
  batch land.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: what a commit's `CommitUnknownResult` means: `Committed` on proof that the attempt's own commit landed, and
  otherwise the kept doubt; no further batch (no second commit, no compensation) after any unknown outcome; the living
  architecture doc kept current. / out of scope:
  - releasing the slot of a commit that provably did not land, **or sending its batch again**: both need a "did not
    land" proof, or a mark a landed commit leaves, that no late staging batch and no same-bytes rival can forge. That is
    a staging or record change (deferred, not filed: alpha intake is closed);
  - 657.2b's resend after a definite `Conflict` that no read explains: unchanged (a `Conflict` round never lands);
  - a **compensation's** unknown outcome: 657.2a's in-doubt stop stays until #1030;
  - the damage-only false `Committed` with the slot erased (#1030);
  - any change in `stage.rs`; proposal 0016 and the ADRs, including the wording at `0016:1727-1728`; `Cargo.lock` and
    `deny*.toml`; the gateway and any production caller; Complete, the drain, the reaper, DST.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 31 mutants tested in 3m: 16 caught, 12 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.41s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1029: settle an unknown multipart part commit from reads without another batch; the main behavior passes red→green, but one confirmed implementation defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted contract distinguishes proof of this attempt's commit from mere part presence, while forbidding further batches after an unknown outcome; `brief.md:254`, `crates/core/src/multipart/commit.rs:116`. |
| C2 Reproduction (red pre-fix) | PASS | An independently compiled probe finds the part durable but receives `InDoubt` on the stashed base, establishing the missing success classification; `reviewer-red.log:10`, `crates/core/tests/multipart_upload_part_commit.rs:4305`. |
| C3 Change | PASS | The three changed paths stay within the commit, its existing test suite and living architecture; the behavior and availability cost are documented together; `crates/core/src/multipart/commit.rs:1357`, `docs/design/architecture/05-building-block-view.md:250`. |
| C4 Verification (red→green) | PASS | The same probe passes after exact patch restoration, and all 640 core tests pass independently; the uncovered fault combination is adjudicated under C5, not hidden by these greens; `reviewer-redgreen-restored.log:9`, `reviewer-core.log:1`, `gate-logs/C4-ci.log:4812`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve irreversible evidence across a later read fault and cover that combination — otherwise callers are told a batch can still land after successful reads proved it cannot; R1, `crates/core/src/multipart/commit.rs:905`, `reviewer-probes-patched.log:26`. |
| T1 Structure | PASS | Settlement stays behind the existing metadata trait and shares the classification reads without adding a backend or runtime dependency; `crates/core/src/multipart/commit.rs:848`, `crates/core/src/multipart/commit.rs:894`. |
| T2 Shape | PASS | The bounded settlement helper and explicit error cause fit the existing API, with living documentation updated and independent formatting/spelling checks passing; `crates/core/src/multipart/commit.rs:393`, `docs/design/architecture/05-building-block-view.md:250`. |
| T3 Runtime | PASS | Unknown outcomes exit the write loop into bounded reads, preserving the no-retry/no-compensation safety rule and the existing clock lifecycle; `crates/core/src/multipart/commit.rs:316`, `crates/core/src/multipart/commit.rs:955`, `crates/core/src/multipart/commit.rs:1358`. |
| T4 Contribution | FAIL | One substantive review finding still needs resolution: all three frozen findings describe R1, independently reproduced here; affected-path prior-art checks are clear, and contribution-artifact validation is deferred to publish; `gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:1`. |
| T5 Judgment | PASS | The already accepted availability tradeoff does not need reopening; no capability probe masks an eager cause, and the tracked DST/compensation deferrals remain settled; `brief.md:249`, `crates/core/src/multipart/commit.rs:265`, `crates/core/src/multipart/commit.rs:1151`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected library-only slice is fit for its intended integration — store-level tests establish protocol behavior, while the production S3 caller remains #669; `brief.md:175`, `crates/core/src/multipart/commit.rs:260`. |

R1 — **Preserve successful observations when a subsequent read fails (P2).** `read_lap` returns only an error if any of its four reads fails (`crates/core/src/multipart/commit.rs:848`). Consequently, `settle` returns at line 907 before applying the part observation at line 915 or the first session observation at line 922. This violates the public flag's promise that no read has shown the batch unable to land (`crates/core/src/multipart/commit.rs:398`). It overstates possible future writes; the reproduced cases do not resend, compensate or corrupt a part.

The scratch probe exercised both histories against the real redb store and the submitted wrapper:

- Hold the commit, advance the session to a higher epoch, successfully read that session, then fail the slot read.
- Hold a commit requiring an absent part, commit a same-number rival, successfully read that part, then fail the first session read.

Both return `may_still_commit: true`; releasing each held batch returns `Conflict`. Both retain one slot and send exactly one commit and zero compensations (`reviewer-probes-patched.log:26`, `reviewer-probes-patched.log:28`). The existing fault test deliberately keeps the held batch landable and therefore does not cover either combination (`crates/core/tests/multipart_upload_part_commit.rs:5147`). Preserve the validated observations before later awaits, or carry partial observations with the read error, while retaining `InDoubt` and the read fault as its cause. Add regression coverage for both histories. This consolidates the three frozen batch-review findings into one actionable issue.

Reproduce R1 from this review directory:

```sh
TMPDIR="$PWD/pdca-reviewer-1029-tmp" CARGO_TARGET_DIR="$PWD/pdca-reviewer-1029-build" \
  cargo test --offline --locked --manifest-path pdca-reviewer-1029-probes/Cargo.toml \
  --test reviewer reviewer_irreversible_evidence_survives_later_read_fault -- --exact --nocapture
```

Verification evidence supports the main feature, with the following limits:

- **Independent execution:** `cargo test -p wyrd-core` passed 640 tests, including all 75 multipart commit tests. A separate probe compiled against both versions: stashing the whole patch produced an assertion failure on the missing `Committed` answer, and restoring it produced success for both `may_still_commit` values. The target's diff was byte-identical before and after restoration. `cargo fmt --all -- --check`, `typos`, docs lint, docs render/link audit, and `git diff --check` passed; rendering produced 101 pages (`reviewer-docs-render.log:2`). The brief's `typos` and `docs-renderer` dependencies were exercised.
- **Frozen host evidence:** instance-scoped wrapper gates were adjudicated from their supplied logs, not rerun or inferred from their summary status. C4-ci ends with all checks passed (`gate-logs/C4-ci.log:4812`); host-tikv records both feature compilations, ending successfully (`gate-logs/host-tikv.log:211`). C4-verify itself was explicitly green-only (`gate-logs/C4-verify.log:161`); the independent probe supplies a genuine red→green for the main feature, not every mutation claimed in the brief.
- **Coverage and mutations:** C4-diff-cov measured no coverage because its origin/main base lacks the prerequisite patch (`gate-logs/C4-diff-cov.log:10`). The supplied target contains the correct stacked base and compiles; the coverage failure is an ordering caveat, not a patch defect. C5-mutants reports 16 caught, 12 unviable and three timeouts, with no reported surviving mutant (`gate-logs/C5-mutants.log:11`). The timed-out mutations alter the settlement loop's progress/bound; that log does not establish an unmutated runtime failure or complete mutation verification.
- **Prior art and deferrals:** independently queried main-branch history for all three affected paths and inspected file lists for all 19 closed, unmerged PRs; none touches those paths (`reviewer-prior-art.log:1`). The core implementation and test history both identify merged #1027. Seeded DST remains explicitly deferred to #660, and compensation settlement to #1030; neither is re-raised. **T4-contribution: N/A** — its artifacts are intentionally absent at Check and the substantive audit runs at publish (`gate-logs/T4-contribution.log:10`).

Source citations above are grounded in the supplied `$PDCA_TARGET`; review and gate evidence paths are relative to this review directory. No product source was edited, and all reviewer-created probes, logs and build artifacts remain inside the harness-owned directory.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1029 (657.2c), settling a part commit answered unknown

**Bottom line.** I could not refute the core of the fix. Once a commit round is answered unknown, the patch sends nothing more. It answers `Committed` only when a lap finds the slot gone and the session read after that still holds the exact `Open@E` bytes. I could not build a history that gives a false `Committed`. The red→green evidence is real: putting back iteration 2's resend or compensation turns (X) and (X2) red on the C-1 property itself, not just on a count. Two findings remain. Both are about the `may_still_commit` flag, not about safety. The first one is the same issue the gating T4 batch review flagged, and I reproduced it.

## Findings

- NEEDS-HUMAN [impl] — **A read fault throws away a lap's earlier evidence, so `may_still_commit` says `true` for a batch that can no longer land.** `read_lap` (`crates/core/src/multipart/commit.rs:848-857`) returns early on the first failed `get`, and `settle` maps that error straight to `doubt.end(...)` (`commit.rs:905-907`). The part read and first session read that already came back are never folded into the doubt (the folds at `commit.rs:915-925` only run on a full lap). Concrete failing case, run as a probe test on the patched tree: a held commit (`may_still_commit: true`); at gap 1 (after the part read) the session rolls back to a newer `Open` epoch, then reads of the slot key fault. The answer is `InDoubt { may_still_commit: true, cause: Store("scripted read fault") }`, and releasing the held batch afterwards answers `Conflict`, so it could never land. A second case gives the same result: a first upload (part key required absent), a rival's part lands before the part read, then the first session read faults. This breaks the field's own contract at `commit.rs:398-401` ("no read since showed it unable to land") and the doc line "says whether the commit may still land" (`docs/design/architecture/05-building-block-view.md:250`). The error points the safe way for C-1 today, so this is not a safety bug. But it is the T4-batch-review gating red (`commit.rs:907/911/927` in its log), and #1030 will build on this flag. Fix: fold each read into `Doubt` as it is made, or return the reads made before the fault. Then add the moved-session-then-fault and rival-then-fault cases next to `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`crates/core/tests/multipart_upload_part_commit.rs:5147`).

- NEEDS-HUMAN [impl] — **No test covers the first session read's effect on the flag.** Deleting `doubt.seen_session(&judge_session(lap.first.as_ref(), session));` (`commit.rs:922`) leaves all 75 tests green. It changes the answer on a reachable (damage) history. Probe: a held commit; rollback at gap 1; session record erased at gap 3. As shipped this answers `may_still_commit: false, cause: Record(SessionRecordMissing)`; with line 922 removed it answers `true`, and nothing notices. Low severity on its own. Once the first finding's fix folds reads as they are made, though, this line decides the flag on a fault after a moved first read, so the test written for that fix should pin it too.

## Refutation attempts that failed (evidence for the fix)

- **The red→green proof.** C4-verify is only `PASS (green-only)`, so I ran the reds myself on a scratch copy of `$PDCA_TARGET`. Patched tree: 75/75 green. Then one deliberate breakage at a time to `commit.rs`:
  - The same batch resent after an `OutcomeUnknown` doubt (iteration 2's `Judged::Again`): 17 red. Both `x2_a_*` and `x2_b_*` fail on "at most one of A's commit batches lands" (test `:4864`), so they fail on the double landing, not on a batch count.
  - A compensation after an `OutcomeUnknown` doubt: (X) and 7 more red.
  - A compensation after any doubt: the `h_*` legs and the moved-session leg red.
  - The proof read from the first session read: C2 and the read-gap rollback leg red.
  - Slot "gone" taken as absent-only: C3′ red.
  - `Rewritten` taken as gone: the slot-rewritten leg red.
  - "A changed part is there" taken as `Committed`: 10 red.
  - `SETTLE_LAPS = 1`: 7 red, including the read-gap landed-then-rival leg, which needs lap 2 to answer `Committed`.
  - The damage check moved before the proof, or the `PartRecordMissing` arm dropped: (R) red.
  - The rival-part `cannot_land` dropped: 4 red.
  - `gone_in_doubt` keeping the flag: `r_a_missing_session_*` red.
- **Production path, not a mirror.** The legs drive `commit_part` against a real redb store. A held batch is re-checked against that store when it is released (`multipart_upload_part_commit.rs:520-533`), so a held commit's preconditions are judged at landing time, not at send time.
- **A false `Committed`.** The proof needs nothing but this attempt's own batch to delete its slot under `Open@E`. Only `commit_batch` (`commit.rs:1083`) and `compensation_batch` (`commit.rs:1106`) delete a `slot:` key. Nothing in `stage.rs`, `abort.rs`, `complete.rs` or the custodian does. No compensation comes before the unknown round, because `settle` is only reached from `commit_rounds` (`commit.rs:1357-1366`) before any `refuse`/`unexplained_stop`. A Complete release goes to `Open@E+2` (`complete.rs:650-651`), so the `Open@E` bytes never come back. Another attempt can only put its id at the slot key under `require_absent`, which needs this attempt's slot gone first. I could not build a history where a lap sees slot-gone plus `Open@E` without this attempt's own commit having landed.
- **A false "cannot land".** I checked each place that lowers the flag: the store said false; the session at a higher epoch (an epoch only rises); a part record where the commit required absence (`0016:351`); session gone with the slot key absent. Each one removes a precondition of the single batch for good, short of damage being repaired. I found no case that reports `false` for a batch that can still land.
- **C5 timeouts** (`commit.rs:904`, `:955`): all three mutants turn `settle` into an endless loop. The 20 s mutant timeout kills them, so they count as caught, not as survivors.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P2: Preserve successful observations when a later settling read fails.** In `crates/core/src/multipart/commit.rs:855`, a slot-read failure discards the already-read part and session; `crates/core/src/multipart/commit.rs:907` then returns the unchanged doubt before those observations are classified. For example, hold a commit with `may_still_commit: true`, roll the session forward, let the first session read observe the higher epoch, then fail the slot read. The response incorrectly retains `may_still_commit: true` although that epoch permanently prevents the batch from landing. A rival part observed where absence was required has the same problem. Preserve and classify successful observations before returning the read fault, while retaining `InDoubt` and sending no further batch. Extend `crates/core/tests/multipart_upload_part_commit.rs:5147` with these combinations; its current fault cases all leave the held batch able to land. This consolidates the three equivalent findings in the frozen T4 log.

No additional correctness findings or actionable reuse, simplification, or efficiency findings in this diff. Reviewed against the read-only target and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve irreversible evidence across a later read fault and cover that combination — otherwise callers are told a batch can still land after successful reads proved it cannot; R1, `crates/core/src/multipart/commit.rs:905`, `reviewer-probes-patched.log:26`.
- [ ] Validation — fitness-to-purpose — Decide whether the corrected library-only slice is fit for its intended integration — store-level tests establish protocol behavior, while the production S3 caller remains #669; `brief.md:175`, `crates/core/src/multipart/commit.rs:260`.
- [ ] **A read fault throws away a lap's earlier evidence, so `may_still_commit` says `true` for a batch that can no longer land.** `read_lap` (`crates/core/src/multipart/commit.rs:848-857`) returns early on the first failed `get`, and `settle` maps that error straight to `doubt.end(...)` (`commit.rs:905-907`). The part read and first session read that already came back are never folded into the doubt (the folds at `commit.rs:915-925` only run on a full lap). Concrete failing case, run as a probe test on the patched tree: a held commit (`may_still_commit: true`); at gap 1 (after the part read) the session rolls back to a newer `Open` epoch, then reads of the slot key fault. The answer is `InDoubt { may_still_commit: true, cause: Store("scripted read fault") }`, and releasing the held batch afterwards answers `Conflict`, so it could never land. A second case gives the same result: a first upload (part key required absent), a rival's part lands before the part read, then the first session read faults. This breaks the field's own contract at `commit.rs:398-401` ("no read since showed it unable to land") and the doc line "says whether the commit may still land" (`docs/design/architecture/05-building-block-view.md:250`). The error points the safe way for C-1 today, so this is not a safety bug. But it is the T4-batch-review gating red (`commit.rs:907/911/927` in its log), and #1030 will build on this flag. Fix: fold each read into `Doubt` as it is made, or return the reads made before the fault. Then add the moved-session-then-fault and rival-then-fault cases next to `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`crates/core/tests/multipart_upload_part_commit.rs:5147`).
- [ ] **No test covers the first session read's effect on the flag.** Deleting `doubt.seen_session(&judge_session(lap.first.as_ref(), session));` (`commit.rs:922`) leaves all 75 tests green. It changes the answer on a reachable (damage) history. Probe: a held commit; rollback at gap 1; session record erased at gap 3. As shipped this answers `may_still_commit: false, cause: Record(SessionRecordMissing)`; with line 922 removed it answers `true`, and nothing notices. Low severity on its own. Once the first finding's fix folds reads as they are made, though, this line decides the flag on a fault after a moved first read, so the test written for that fix should pin it too.
- [ ] **P2: Preserve successful observations when a later settling read fails.** In `crates/core/src/multipart/commit.rs:855`, a slot-read failure discards the already-read part and session; `crates/core/src/multipart/commit.rs:907` then returns the unchanged doubt before those observations are classified. For example, hold a commit with `may_still_commit: true`, roll the session forward, let the first session read observe the higher epoch, then fail the slot read. The response incorrectly retains `may_still_commit: true` although that epoch permanently prevents the batch from landing. A rival part observed where absence was required has the same problem. Preserve and classify successful observations before returning the read fault, while retaining `InDoubt` and sending no further batch. Extend `crates/core/tests/multipart_upload_part_commit.rs:5147` with these combinations; its current fault cases all leave the held batch able to land. This consolidates the three equivalent findings in the frozen T4 log.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- [ ] **Removing compensation does not make the retained retry rule safe.** `brief.md:25-30` and `brief.md:197-199` claim that requiring the same slot bytes means at most one commit lands. But the re-plan itself establishes that late staging can recreate those bytes (`brief.md:175-180`); it never repairs the retry proof. On the target, reserve can recreate the deleted slot (`crates/core/src/multipart/stage.rs:1296-1298`, `:1329-1333`), and held intent/renewal batches can restore the subsequent values (`stage.rs:2325-2336`, `:1620-1630`). This affects more than the release count: replay can publish chunks already owed to retirement.
- [ ] size backstop — this slice is behaving oversized: patch is 140 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Fix R1 only; size backstop overridden by the human (the bulk is the required test suite, the slice is not two outcomes — do not split). Fold each settle read into Doubt as it returns (or return the partial reads with the error), so a later read fault keeps what earlier reads proved: `may_still_commit` must go false once a moved session or a required-absent rival has been seen, still answering InDoubt with the read fault as its cause and sending no further batch. Add regression legs next to `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (test file :5147): (1) held commit, session moved to a higher epoch, first session read sees it, then the slot read faults; (2) held commit requiring the part absent, rival part lands, part read sees it, then the first session read faults. Each must answer may_still_commit: false, and the held batch released afterwards answers Conflict. Pin `commit.rs:922` (the first-session-read fold): removing it must turn a test red. Keep everything else as is. The plan-review retry finding is already resolved by the brief (no resend after an unknown answer, covered by X2).
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
