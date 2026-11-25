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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 32 mutants tested in 3m: 18 caught, 11 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.45s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #1029's read-only settlement of unknown multipart commits, including preservation of earlier evidence across later read faults.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success distinguishes this attempt's landed commit from ambiguous histories; the carried-forward read-fault requirement is explicit and testable (`brief.md:22`, `brief.md:307`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, a durable part with one slot release still returned `InDoubt` instead of `Committed`; the behavior probe failed at an assertion, not compilation (`reviewer-red.log:12`). |
| C3 Change | PASS | The unknown-outcome branch returns directly through settlement, excluding any later retry or compensation that could reclaim or republish live chunks (`target/crates/core/src/multipart/commit.rs:1351`). |
| C4 Verification (red→green) | PASS | The identical probe passed after unstashing; restored code passed 642 core tests and independent checks, with full-workspace CI supported by frozen evidence (`reviewer-probe-green.log:8`, `reviewer-checks.log:10`, `gate-logs/C4-ci.log:4814`). |
| C5 Causal adequacy | PASS | Both new regressions fail when their respective evidence fold is removed, pinning the carried-forward defect; own-commit proof still requires a session read after the slot read (`reviewer-mutation-first-session.log:14`, `reviewer-mutation-absent-part.log:14`, `target/crates/core/src/multipart/commit.rs:920`). |
| T1 Structure | PASS | Settlement stays behind the existing metadata seam and separates ambiguous outcomes from definite-conflict compensation; no new dependency or production caller broadens the slice (`target/crates/core/src/multipart/commit.rs:876`, `target/crates/core/src/multipart/commit.rs:1361`). |
| T2 Shape | PASS | Errors retain indeterminate classification even when a later store fault ends settlement, and the living architecture describes the changed contract (`target/crates/core/src/multipart/commit.rs:567`, `target/docs/design/architecture/05-building-block-view.md:250`). |
| T3 Runtime | PASS | Settlement performs at most two four-read laps plus a missing-session slot check; it adds no clock source, task or write, and uses the store's bounded-operation contract (`target/crates/core/src/multipart/commit.rs:947`, `target/crates/core/src/multipart/commit.rs:858`, `target/crates/traits/src/lib.rs:1334`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must run at publish, so the deferred result requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Replay and torn-read histories exercise the safety boundary on real redb; prior art was checked by all affected paths, while accepted availability and size decisions remain settled (`target/crates/core/tests/multipart_upload_part_commit.rs:4734`, `target/crates/core/tests/multipart_upload_part_commit.rs:4815`, `reviewer-checks.log:18`, `brief.md:307`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the verified core-only outcome completes #1029 — production S3 behavior is intentionally outside this slice and remains #669 (`brief.md:175`, `target/docs/design/architecture/05-building-block-view.md:250`). |

The independent evidence supports the carried-forward fix and the broader settlement contract. The review checked protocol causality, adversarial test histories, error classification, runtime bounds and documentation against the target's standing rubric. No capability probe or load-time symptom guard was introduced.

- **Behavioral red→green:** a standalone probe used the unchanged base fixture, staged a real part, and verified durable ownership, exactly one release, one commit and no compensation before requiring `Committed`. It failed on the stashed base and passed on the restored patch for both store flags. Its source is retained in `pdca-reviewer-1029-scratch/pdca_reviewer_1029_reproduction.rs`; results are in `reviewer-red.log:12` and `reviewer-probe-green.log:8`.
- **Carried-forward causal regression:** independently removing `doubt.seen_session(...)` at `target/crates/core/src/multipart/commit.rs:912` made the moved-session/read-fault test fail. Removing the required-absent part update at `:904` made the rival/read-fault test fail. Both failed on the expected `may_still_commit` assertion at `target/crates/core/tests/multipart_upload_part_commit.rs:5258`. Restored tests also verify that each held batch subsequently conflicts (`:5275`).
- **Independent green checks:** all 642 core tests passed after restoration, including the 77 multipart commit tests and the cross-phase/replay histories. Core clippy across all targets, workspace formatting, spelling, docs lint and the 101-page render/link audit passed. The two declared external tools were actually exercised; no shim or missing-tool waiver underlies these results (`reviewer-checks.log:10`).

Frozen gates support a narrower claim than their aggregate labels. Full workspace CI reports success, including dependency checks and DST (`gate-logs/C4-ci.log:4162`, `:4435`, `:4814`); the TiKV row records successful feature compilation, not a live-cluster test (`gate-logs/host-tikv.log:2`, `:211`). The per-fix wrapper explicitly reports green-only (`gate-logs/C4-verify.log:161`); the independent behavioral probe supplies the missing red. Diff coverage was not measured because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`), consistent with the stacked prerequisite in `brief.md:152`; this is a base-state caveat, not a patch defect. The supplied target itself matches the stated stack base and applies the patch exactly.

The mutation gate's red status reports 18 caught, 11 unviable and three timed-out mutants, with no reported survivor. The timeouts alter loop progress or its continuation condition; they are not evidence of a surviving correctness defect (`gate-logs/C5-mutants.log:13`). The frozen batch-review log reports zero blocking findings but does not expose its individual passes (`gate-logs/T4-batch-review.log:10`); this review independently examined the final patch. Historical iteration-2 implementations were not supplied, so the independent red claims above concern the actual base and the two explicit mutations, not an asserted execution of that historical patch.

Prior art was checked by file path for all three changed files. GitHub's merged history returns `5e8aa24` for the implementation and test file, and 49 commits for the architecture document (`reviewer-merged-history.json:1`). All 19 closed, unmerged PRs had their changed-file lists checked; none touches an affected path (`reviewer-closed-history.json:1`). No additional prior-art decision remains open.

The final target diff is byte-for-byte identical to `patch.diff`; the temporary probe is outside the target test directory. Review artifacts and scratch outputs remain inside this harness-owned cwd for harness disposal.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 1029 (657.2c), iteration 4

**Verdict: I tried to refute the fix and could not.** I copied the patched tree into scratch, ran `cargo test -p wyrd-core --test multipart_upload_part_commit` (77 passed), then applied hand mutations and the two designs that earlier rounds rejected. Every one turned at least one test red, and the failures came from the right assertions. The one open item is about sign-off evidence, not the code.

## Refutation attempts that failed (the evidence holds)

- **R1 fold (this round's carry-forward)**, `crates/core/src/multipart/commit.rs:912`. With the first-session-read fold removed, `c_a_read_fault_after_the_session_moved_keeps_the_commit_unable_to_land` goes red, and it is the only test that does. So the pin the human asked for is real. (The carry-forward's `:922` is the iteration-3 line; the fold now sits at `:912`.) Removing the rival fold at `commit.rs:904` turns 5 tests red, including the new `c_a_read_fault_after_a_rival_where_none_was_required_...`. Removing the last-read fold at `:917` turns 5 red. Dropping `cannot_land` in `gone_in_doubt` (`:860`) turns `r_a_missing_session_behind_a_held_commit_is_in_doubt` red.
- **A false `Committed` from the own-commit proof**, `commit.rs:920`. Three mutations each go red: taking the session state from the read made *before* the slot read (7 tests: C2, H, read-gap, R), accepting a moved session (2 tests), and accepting "slot gone" alone (5 tests). Treating another attempt's slot as still held (`commit.rs:696`) fails C3′. Checking damage before the proof (`:926` ahead of `:920`) fails `r_damage_under_a_held_commit_keeps_the_doubt`.
- **Is "slot gone under `Open@E`" really only this attempt's doing?** Yes, in the code as it stands. The only `.delete(` of a `slot:` key in the repo is in the commit batch (`commit.rs:1075`) and the compensation batch (`commit.rs:1098`). `stage.rs` deletes nothing, abort only fences, and custodian `gc.rs`/`restore.rs` only read `part:` and never touch `slot:`.
- **Is `may_still_commit: false` ever reported while the batch could still land?** I found no case. The session fold relies on epochs only rising (`judge_session`, `commit.rs:649-670`). The rival fold relies on `part:` records being deleted only by the retirement drain, which the root flip arms after the session leaves `Open` (`0016:796`). No `part:` delete exists in code. The flag only ever falls from the store's value (`Doubt::of`, `commit.rs:797-802`).
- **The rejected designs, put back by hand.** (1) Sending the same batch again after `OutcomeUnknown` (iteration 2's `Judged::Again`): 17 tests red. Both X2 runs fail on "at most one of A's commit batches lands" (test file `:4864`), so they catch the real double landing, not just a batch count. (2) Compensating when the doubt ends with `may_still_commit: false`: 8 red, including `x_late_staging_that_rebuilds_a_landed_commits_slot_never_compensates` at "no compensation" (`:4766`).
- **Lap budget**, `commit.rs:317`. `SETTLE_LAPS = 1` turns 7 tests red, and `= 3` turns 2 red. The test file pins the read sequence exactly (`laps_of`, `MOST_LAPS` at test `:3916`), and the lap-2 `Committed` path is exercised (read-gap gaps 3–4).
- **Test fidelity.** Held and unknown batches land on the real redb store with their real preconditions (`release_held`, test `:520-533`). `assert_no_owed_chunk_is_published` (test `:4098`) scans every obligation and every `part:` record, so it is not a count that can pass while the property fails.
- **Gates.** `C4-ci` really ran typos, `lint_docs`, `render_site` and this test file (`gate-logs/C4-ci.log:11-19`, `:1319`). `C4-verify` is green-only and `C4-diff-cov` unmeasured, both declared in advance in the brief. My mutation runs above stand in for the per-fix red that `C4-verify` cannot isolate. `C5` has 0 missed. Its 3 timeouts (`commit.rs:887`, `:947`) are mutants that make the lap loop endless, and the tests catch them by hanging.

## Open item

- NEEDS-HUMAN [human] — The brief makes sign-off depend on "a confirmed high-effort review of the final patch" and notes that iteration 2's 14.85 s T4 run "could not be confirmed". This round's `T4-batch-review` row says `pass` after 26.75 s for three reviewers on a ~148 KB patch (`check-gates.json` T4 row; `gate-logs/T4-batch-review.log:10` is a single summary line with no findings detail). That timing does not show the deep, multi-pass review the brief and the rubric's *Definition of done* require. The human should confirm the review depth from `results/issue_1029/review-batch.md` before accepting the T4 row as that review. This is a process check, not a code defect.

## Notes (not findings)

- The second lap (`commit.rs:947`) re-reads right after the first, with no wait in between. It only catches a commit that lands within the time of four reads, so it buys little availability in production. It is still correct, since the flag stays honest either way.
- The `Committed` proof depends on the documented rule that no reaper deletes a slot while the session is `Open` (`0016:1625-1628`). That rule holds in today's code (see above). The planned idle fence moves the session in the same batch (`0016:665`), so it stays safe. A future reaper that broke the rule would produce a false `Committed`, and nothing in this slice would catch it. That is for the reaper's slice (#625), not this one.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. I found no correctness bugs introduced by this diff and no actionable reuse, simplification, or efficiency issues.

Reviewed the settling logic and terminal unknown-outcome branch (`crates/core/src/multipart/commit.rs:876`, `crates/core/src/multipart/commit.rs:1350`), the scripted interleavings and read-fault regressions (`crates/core/tests/multipart_upload_part_commit.rs:3971`, `crates/core/tests/multipart_upload_part_commit.rs:5231`), and the architecture update (`docs/design/architecture/05-building-block-view.md:250`). Citations were verified against `$PDCA_TARGET`; the review respects the brief's settled decisions and explicit deferrals.

Validation used source inspection and frozen gate evidence; tests were not rerun. C4-ci passed. C4-verify was green-only, diff coverage was not measured, and C5 reported 18 caught mutants, 11 unviable mutants, three timeouts, and no surviving mutants.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether the verified core-only outcome completes #1029 — production S3 behavior is intentionally outside this slice and remains #669 (`brief.md:175`, `target/docs/design/architecture/05-building-block-view.md:250`).
- [x] The brief makes sign-off depend on "a confirmed high-effort review of the final patch" and notes that iteration 2's 14.85 s T4 run "could not be confirmed". This round's `T4-batch-review` row says `pass` after 26.75 s for three reviewers on a ~148 KB patch (`check-gates.json` T4 row; `gate-logs/T4-batch-review.log:10` is a single summary line with no findings detail). That timing does not show the deep, multi-pass review the brief and the rubric's *Definition of done* require. The human should confirm the review depth from `results/issue_1029/review-batch.md` before accepting the T4 row as that review. This is a process check, not a code defect.
- [x] **Removing compensation does not make the retained retry rule safe.** `brief.md:25-30` and `brief.md:197-199` claim that requiring the same slot bytes means at most one commit lands. But the re-plan itself establishes that late staging can recreate those bytes (`brief.md:175-180`); it never repairs the retry proof. On the target, reserve can recreate the deleted slot (`crates/core/src/multipart/stage.rs:1296-1298`, `:1329-1333`), and held intent/renewal batches can restore the subsequent values (`stage.rs:2325-2336`, `:1620-1630`). This affects more than the release count: replay can publish chunks already owed to retirement.
- [x] size backstop — this slice is behaving oversized: patch is 144 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
