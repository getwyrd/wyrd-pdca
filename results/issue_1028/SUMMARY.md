# Result — issue 1028 / multipart-part-commit-classify-conflict

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.2a a commit or compensation answered `Conflict` stops and leaves its slot held.
  Under a live session nothing else frees that slot before `W_session`, because the reaper does not
  walk an `Open` session (`0016:1600-1620`). This slice classifies a `Conflict` from a re-read:
  the session fenced, the session rolled back to a newer `Open@E'`, a same-number rival, or a benign
  retry. It compensates where the session is live, chases a further rollback, and bounds every loop.
- Success criterion: Appended to `crates/core/tests/multipart_upload_part_commit.rs`, with the
  wrapper extended by **an action between two reads**. In every leg nothing is in doubt: no batch of the
  attempt answered unknown.
  **(B) A lost commit is classified, never compensated blind** (`0016:1584-1653`).
  - The session fenced to `Aborting` before the commit: `Refused(NoSuchUpload)`, no write (the residue
    is the reaper's).
  - **The session rolled back to `Open@E'` (`E' > E`)** between the last renewal and the commit: the attempt
    compensates under `require(mpu == Open@E')` (`0016:1607-1620`), one obligation for its own chunks,
    its slot released and its `sidx:` entries deleted in one batch. It answers exactly
    `Refused(OperationAborted)`, never `Committed` and never `NoSuchUpload`, because the session is `Open`.
  - A same-part rival committed first: the loser commits one obligation for its **own** chunks (it
    decodes under `decode_retire_obligation`, `multipart.rs:3954`), releases its slot, deletes its
    `sidx:` entries, and answers exactly `Refused(OperationAborted)`. An empty loser writes no obligation.
  - A plain repair changed the part with nothing in doubt: compensate and answer
    `Refused(OperationAborted)`. No retry against the new record (decided at sign-off, iteration 4).
  - A conflict no read explains (the part as required, the session `Open@E`, the slot held): the same
    batch is retried.
  **(T) A terminal delete is not damage** (attempt 7's finding 1). The commit answers `Conflict`; the
  reads find the part, the session (`Aborting`) and the slot (held); **then** the terminal delete lands,
  removing the session, slots and owned entries in one batch (`0016:674`), and the last session read finds
  nothing. The answer is `Refused(NoSuchUpload)`, never `Stop::Record(SessionRecordMissing)`. The same
  for a compensation when the delete lands right after its slot read. Attempt 7 pinned the wrong answer
  (`multipart_upload_part_commit.rs:4687-4688`, "no second lap of reads"): that must not come across.
  **Any record at the slot key** on a slot read made after the session was found gone is damage,
  `Stop::Record(SessionRecordMissing)`, with no compensation: this attempt's bytes (decided at sign-off,
  iteration 6) **or another attempt's**. The delete takes every surviving slot in its one batch
  (`0016:674`) and a reservation requires the session's `Open` bytes (`stage.rs:1325-1329`), so nothing
  legal leaves a slot behind a missing session. Run the foreign-slot case at the commit's re-read and at
  the compensation's.
  **(R) Damage, with nothing in doubt, is a record stop, never a rival or a fence.** No compensation in
  any of these. Bytes at the part key that do not decode. A prior part record gone while the session is
  still `Open@E` (`RecordError::PartRecordMissing`). A session record that does not decode. A record under
  other bytes at the same or a lower epoch, `Open` or not (`SessionEpochNotRisen`). A damaged part record
  under a moved session. A slot rewritten or lost: `Stop::SlotLost`, with no release claimed.
  **(O) A taken obligation key.** Other bytes planted at the obligation's key, then a re-upload over a
  non-empty prior: those bytes and the prior are kept, ending in `Stop::ObligationKeyTaken`.
  **(L) Bounds and the lease.**
  - Every loop (commit rounds, compensation rounds, the epoch chase) has a stated maximum. With a wrapper
    answering every batch `Conflict` unapplied, each ends in a typed error after **exactly** that many
    commits. The test compares against a **number written in the test**, never the imported constant.
  - **The epoch chase's real bound:** a same-part rival beats the commit, which spends no rollback. Then
    three rollbacks, `MAX_COMPLETE_ATTEMPTS`, the most a session can see, each beat one compensation.
    The fourth compensation lands, and the log shows exactly 4. Only this run needs the whole budget.
    A second run shows the chase when a rollback beats the commit: one rollback there and two against
    compensations, and the third compensation lands.
  - During a compensation: a session found fenced answers `Refused(NoSuchUpload)` with no further write;
    a compensation that spends its rounds returns its own typed error, which claims no release.
  - `Conflict` rounds separated by slow reads take the lease below the margin before a later round: no
    further commit is sent and the attempt compensates. The answer is a typed stop, never `Committed`.
  - Commit rounds spent with nothing in doubt compensate.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: what a `Conflict` means for a commit and for a compensation: the re-read, the classification,
  the retry, the epoch chase, the loop bounds, and the wider doc of `Refusal::OperationAborted`, with no new
  variant. / out of scope:
  - any batch answered `CommitUnknownResult`: 657.2a's in-doubt stop stays until 657.2c and 657.2d;
  - slots left held when a Complete fence rolls back (decided at sign-off, iteration 2: accept until
    `W_session`, raise against 0016, do not change it here);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`; the
    gateway and any production caller; Complete, the drain, the reaper, DST.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 2m: 24 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1028/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.67s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1028: no implementation defect found in classifying lost multipart part commits and compensating live-session losers through bounded retries and terminal-delete races.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The no-doubt boundary gives falsifiable answers for live losers, fences, damage and exhaustion, preserving usable sessions and reclamation ownership; brief.md:14 and target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:1597. |
| C2 Reproduction (red pre-fix) | PASS | The old implementation demonstrably strands the attempted operation at its conflict stop: all 11 compatible behavioral tests fail after stashing the patch, including rollback and compensation; reviewer-evidence/replay-red.log:23 and :110. |
| C3 Change | PASS | The four-file change stays within the agreed conflict-classification slice, with matching error vocabulary and architecture documentation; target/crates/core/src/multipart/commit.rs:959, target/crates/core/src/multipart.rs:4755 and target/docs/design/architecture/05-building-block-view.md:250. |
| C4 Verification (red→green) | PASS | Restoring the identical patch turns all 11 behavioral failures green; the submitted suite passes 47/47, with 214/217 instrumented added production lines covered; reviewer-evidence/replay-green.log:18, reviewer-evidence/green.log:224 and reviewer-evidence/diff-coverage.txt:1. |
| C5 Causal adequacy | PASS | Live losers release their own resources without retiring the winner's chunks; terminal deletion is distinguished from surviving-slot damage at both re-reads, and mutation checks found no survivor; target/crates/core/src/multipart/commit.rs:629, :840 and :959; reviewer-evidence/mutants-lint-capped.log:3. |
| T1 Structure | PASS | Ownership remains at the narrow metadata seam: compensation requires the attempt's last-written slot bytes and atomically records its reclamation obligation, preventing another attempt's slot from being released; target/crates/core/src/multipart/commit.rs:802. |
| T2 Shape | PASS | The public refusal meaning and living architecture agree with the client-retry behavior, including empty losers and repaired priors; target/crates/core/src/multipart.rs:4755, target/docs/design/architecture/05-building-block-view.md:250 and target/crates/core/tests/multipart_upload_part_commit.rs:3035. |
| T3 Runtime | PASS | Commit and compensation budgets terminate after exactly four sends, later commits retain the same lease clock, and unknown outcomes stop further writes; target/crates/core/src/multipart/commit.rs:885, :1039, :1065 and :1075; target/crates/core/tests/multipart_upload_part_commit.rs:3676. |
| T4 Contribution | N/A | Contribution text is drafted after Check; its substantive audit is owed to the mandatory publish-time rerun, not human clearance now; gate-logs/T4-contribution.log:10. |
| T5 Judgment | PASS | The carry-forward empty-upload collision, fenced foreign/rewritten-slot distinction and charged-rollback budget are covered; path-based prior-art checks found no competing merged or rejected implementation, and the #660 deferral remains settled; target/crates/core/tests/multipart_upload_part_commit.rs:3524, :3607 and :3758; reviewer-evidence/prior-art-summary.txt:1. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve this core-only increment and prerequisite-first delivery — #1027's PR #1041 is still open, and production UploadPart wiring is reserved for #669, so passing core races cannot establish end-user fitness; reviewer-evidence/prerequisite-pr.json:1 and target/crates/core/src/multipart/commit.rs:196. |

The independent red→green is behavioral, not a compilation failure. Eleven submitted tests and their fixture were copied into a temporary test target using only pre-existing APIs; their assertions were unchanged, with unused fixture items allowed. With the entire tracked patch stashed, they compiled and failed on `CommitConflict`/`CompensationConflict` and the missing release. After `git stash pop`, all eleven passed. The temporary test is retained as reviewer evidence outside the target, and the target's final diff is byte-identical to its initial diff. The original 47-test suite also passed under coverage instrumentation. Evidence: reviewer-evidence/replay-red.log:20, reviewer-evidence/replay-green.log:18, reviewer-evidence/coverage.log:225 and reviewer-evidence/pdca_reviewer_1028_replay.rs:1.

Mutation testing independently corroborates the frozen result. `cargo mutants --in-diff patch.diff -p wyrd-core` tested 36 mutations: 19 caught and 17 unviable. Five were unviable only because of unused-item diagnostics; re-running those five with `--cap-lints=true` caught all five. Thus 24 mutations were caught by tests, 12 were intrinsically unviable, and none survived. The previously problematic slot-identity comparison and reduced compensation budget were caught. Evidence: reviewer-evidence/mutants-rerun.log:3, reviewer-evidence/mutants-lint-capped.log:3 and gate-logs/C5-mutants.log:13. Independent diff coverage was 4/4 added instrumented lines in multipart.rs and 210/213 in commit.rs; uncovered lines were 620, 643 and 865, including the two new read-error returns. This measures execution, not exhaustive interleavings.

The frozen gates support their stated outcomes, with explicit limits:

- **C4-ci / host-tikv: PASS from captured execution.** The CI log records spelling, docs lint/render, formatting, clippy, build/tests and conformance, ending in success; the TiKV log records actual compilation of both feature selections. This reviewer independently re-ran spelling, docs lint and the 101-page render/link audit successfully. Local rustfmt/clippy components are absent, so full CI and TiKV clippy were adjudicated from their frozen logs rather than claimed as fresh runs. Evidence: gate-logs/C4-ci.log:11, :30 and :4770; gate-logs/host-tikv.log:112 and :211; reviewer-evidence/fmt.log:1; reviewer-evidence/docs-lint.log:1; reviewer-evidence/docs-render.log:2.
- **C4-verify: frozen green-only, independently strengthened above.** The frozen log explicitly disclaims a pre-fix red at gate-logs/C4-verify.log:161. The independent stash/replay supplies that missing behavioral comparison.
- **C4-diff-cov: frozen base-state caveat.** Its remote-base apply failure at gate-logs/C4-diff-cov.log:10 did not measure coverage and is not a patch defect. The supplied target accepts the reverse patch check and compiles; the independent coverage run measures the patched target directly. The prerequisite is the still-open [PR #1041](https://github.com/getwyrd/wyrd/pull/1041), consistent with the base-ordering limitation.
- **T4-batch-review: PASS as recorded, limited to its summary.** gate-logs/T4-batch-review.log:10 reports zero blockers but contains no detailed review reasoning. This review separately traced the protocol state table, atomic preconditions, error/doubt paths, lease ownership, bounds and test oracles; it does not infer review depth from that summary. Contribution artifacts remain N/A until publish, as recorded in the table.

Prior art was checked by all four affected paths against merged history, and by actual file lists for all 19 closed, unmerged PRs. Neither new commit-code/test path has merged history; the existing multipart module has 18 history entries and the architecture document 46. None of the 19 rejected/closed PRs overlaps an affected path. The open part-commit PR is the declared prerequisite #1027, not a competing classifier. Evidence and query details are retained in reviewer-evidence/prior-art-summary.txt:1 and reviewer-evidence/prior-art-path-check.txt:1. The source's tracked DST deferral to #660 is settled under target/AGENTS.md:215; it is not re-raised here.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1028 (657.2b, classify a lost part commit)

**Bottom line:** I could not break the fix's behaviour. The patched tree passes all 47 tests in `crates/core/tests/multipart_upload_part_commit.rs` (cargo 1.96, offline, on a scratch copy of `$PDCA_TARGET`). I then ran 37 deliberate code breaks against `crates/core/src/multipart/commit.rs`. Every red the brief asks for showed up as a failing test. Four breaks survived. Each one points at a branch no test checks. Probe tests show the patched code answers those cases correctly, so these are test gaps, not wrong answers.

## Findings

- NEEDS-HUMAN [impl] — **A compensation that finds its own obligation key taken has no behaviour test.** The branch is `crates/core/src/multipart/commit.rs:863-865`, and the module doc's table promises it at `commit.rs:119`. Break: change `obligation_key_free(store, attempt, obligation).await?;` at `commit.rs:864` to `let _ = obligation_key_free(...).await;`. All 47 tests still pass. A concrete case that tells the two apart: a same-part loser (`losing_to_a_rival`) with bytes planted at `obligation_key(&loser, 0)`. The patched code answers `Stop::ObligationKeyTaken` after one compensation. The broken code answers `CompensationRoundsSpent { rounds: 4 }` after four. The only test that touches this read checks the read *order* (`multipart_upload_part_commit.rs:3125-3133`), not what the code does with the result. The (O) test (`:3607`) stops before any compensation is sent. C5 missed this because cargo-mutants never drops a `?` result. Fix: add that leg, plus a chase variant (one rollback, key planted under epoch 2). I ran both as a probe: they pass on the patch and fail on the break.
- NEEDS-HUMAN [impl] — **"Rolled back, and the slot is gone" is untested at both re-reads.** The arms are `commit.rs:989` (commit) and `commit.rs:875-877` (compensation), both `(SessionSeen::Open(_), SlotSeen::Gone) => SlotLost`. Two breaks survive. One makes the commit compensate under the newer epoch. The other makes the compensation chase the newer epoch. In both, the attempt still ends in `SlotLost`, but only after one extra batch that the module doc (`commit.rs:104-110`) says is never sent. The slot test (`multipart_upload_part_commit.rs:3524-3595`) covers only a `live` session and a `fenced` one, never a rolled-back one. Fix: add a `rolled back` session to that test. My probe (`rig.rollback()` plus the slot erased before the batch, at each re-read) passes on the patch and fails on both breaks.
- NEEDS-HUMAN [human] — **A fenced session with its slot missing or held by another attempt is answered as a clean `NoSuchUpload`, not as damage.** The arms are `commit.rs:993` and `:879`, with the doc at `commit.rs:108-110`. The iteration-1 sign-off asked for exactly this ("foreign slot -> NoSuchUpload"), so the builder followed instructions. But 0016 says a slot is removed only by its own commit or compensation, or together with the session at the terminal delete (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:578`, `:1683-1684`), and `abort.rs` writes no slot. The slot read (`commit.rs:976`) comes before the last session read (`:977`). So if that last read still finds a fenced session, the session also existed when the slot was read, and a missing or foreign slot then has no legal cause. That is the same argument the patch uses to call a slot behind a *missing* session damage (`commit.rs:89-95`). The code is strict in that case and lenient for a fenced session. The impact is small: nothing is written either way, and the client gets a refusal instead of a record stop. A human should confirm the sign-off decision still stands given `0016:578`. If it does, nothing changes.
- NEEDS-HUMAN [human] — **The T4 "pass" is weak evidence.** The `T4-batch-review` row in `check-gates.json` reports "3x codex" with 0 blocking findings in **22.41 s** (`gate-logs/T4-batch-review.log`) over a 2,341-line patch. That is too fast for three deep passes; the result was probably cached or run at default settings. The brief (`brief.md:111-112`) says a clean T4 row at default settings is weak evidence and that sign-off needs a confirmed high-effort review of the final patch. Confirm that review happened before accepting.

## The evidence, re-checked

- The `C4-verify` "pass" is green-only. The test lives in an existing file, so the gate could not isolate a red (`gate-logs/C4-verify.log`). I did that check myself. Each red the brief lists fails the named test:
  - a pre-read slot taken as final fails `t_a_terminal_delete_between_the_{commits,compensations}_reads…`;
  - treating every missing session as a terminal delete fails `t_a_missing_session_with_the_slot_still_held…`;
  - counting a foreign slot as gone fails `t_another_attempts_slot…`;
  - compensating blind fails 16 tests, including `b_a_fenced…`;
  - sending a rollback to the reaper fails `b_a_rolled_back…`;
  - a loser answering `NoSuchUpload` fails `b_a_same_part_loser…`;
  - a damaged part read as a rival fails `r_a_damaged_part…`;
  - an unrisen non-`Open` epoch read as a fence fails `r_a_damaged_session…`;
  - dropping `require_absent` on either obligation key fails `o_…` and `assert_compensated`;
  - unbounded counters, `COMPENSATION_ROUNDS - 1` and `COMMIT_ROUNDS - 1` fail the `l_…` tests;
  - skipping the later-round lease check fails `l_a_lease_used_up_between_rounds…`.

  The tests call the production `commit_part` over a real redb store. They do not test a copy of the logic.
- The `C5-mutants` row ("24 caught, 0 missed") overstates the coverage. The first finding above is a break it never generated.
- `C4-diff-cov` measured nothing: the patch does not apply on `origin/main`, the same as in iteration 1. That is probably because 657.2a is not on main yet, but this checkout has no remote refs, so I could not confirm it. If 657.2a has landed in a different form, the base needs a refresh (the rubric's stacked-branch rule).
- Minor, untagged: `commit.rs:1037-1038` and `:191-192` promise that every round sends the same bytes (the stamp is taken once). But the test that checks this (`multipart_upload_part_commit.rs:3067-3092`) runs with a frozen clock. A break that re-stamps every round passes; adding `tick_reads` to that leg would catch it.
- Minor, untagged: `SessionRecordMissing.required` is only ever asserted at epoch 0 (`:3243`, `:3255`, `:3301`). Hard-coding `required: 0` at `commit.rs:640` passes.

## Tried and could not break

- **Re-read order and first vs. last session read:** the order of both re-reads, and using the first session read where the last one should decide.
- **Epoch judgment:** `<=` changed to `<` in `judge_session` is caught. So are a damaged part under every session state, and `PartRecordMissing` raised only when the session is unchanged.
- **The epoch chase:** it moves both the required session and the obligation key's epoch.
- **Arm order:** `Rewritten` is checked before `Left` at both re-reads.
- **Obligation key before compensating:** the key is read before the rounds-spent and lease-short compensations, including for an empty re-upload.
- **Clock:** only `StageClock` is read (ADR-0009).
- **Awaits:** every new await is a store call, which the store trait promises will terminate (`crates/traits/src/lib.rs:1333-1336`).
- **Docs:** `05-building-block-view.md` is updated.
- **Error class:** the new stops classify the same way as `abort.rs:220`.
- **Test fidelity:** the test's rollback is a text edit (attempts +1), not `complete.rs`'s release (attempt given back, `complete.rs:529-530`). Classification reads only the epoch and the state, so this changes nothing.
- **Out of scope:** DST coverage is a settled deferral (#660), so I did not raise it.

All runs used a scratch copy under `$PDCA_SCRATCH`, which I deleted afterwards.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bug or actionable reuse, simplification, or efficiency issue identified in this diff. Reviewed conflict classification, terminal-delete read ordering, compensation and epoch chasing, lease checks, loop bounds, and the scripted tests against `$PDCA_TARGET`.

Validation used the frozen gate evidence; tests were not rerun. Full CI passed, and mutation testing reported 24 caught and 12 unviable mutants, with none surviving. Verification was green-only; diff coverage was unavailable because the patch did not apply to the gate's `origin/main`.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Approve this core-only increment and prerequisite-first delivery — #1027's PR #1041 is still open, and production UploadPart wiring is reserved for #669, so passing core races cannot establish end-user fitness; reviewer-evidence/prerequisite-pr.json:1 and target/crates/core/src/multipart/commit.rs:196.
- [x] **A compensation that finds its own obligation key taken has no behaviour test.** The branch is `crates/core/src/multipart/commit.rs:863-865`, and the module doc's table promises it at `commit.rs:119`. Break: change `obligation_key_free(store, attempt, obligation).await?;` at `commit.rs:864` to `let _ = obligation_key_free(...).await;`. All 47 tests still pass. A concrete case that tells the two apart: a same-part loser (`losing_to_a_rival`) with bytes planted at `obligation_key(&loser, 0)`. The patched code answers `Stop::ObligationKeyTaken` after one compensation. The broken code answers `CompensationRoundsSpent { rounds: 4 }` after four. The only test that touches this read checks the read *order* (`multipart_upload_part_commit.rs:3125-3133`), not what the code does with the result. The (O) test (`:3607`) stops before any compensation is sent. C5 missed this because cargo-mutants never drops a `?` result. Fix: add that leg, plus a chase variant (one rollback, key planted under epoch 2). I ran both as a probe: they pass on the patch and fail on the break.
- [x] **"Rolled back, and the slot is gone" is untested at both re-reads.** The arms are `commit.rs:989` (commit) and `commit.rs:875-877` (compensation), both `(SessionSeen::Open(_), SlotSeen::Gone) => SlotLost`. Two breaks survive. One makes the commit compensate under the newer epoch. The other makes the compensation chase the newer epoch. In both, the attempt still ends in `SlotLost`, but only after one extra batch that the module doc (`commit.rs:104-110`) says is never sent. The slot test (`multipart_upload_part_commit.rs:3524-3595`) covers only a `live` session and a `fenced` one, never a rolled-back one. Fix: add a `rolled back` session to that test. My probe (`rig.rollback()` plus the slot erased before the batch, at each re-read) passes on the patch and fails on both breaks.
- [x] **A fenced session with its slot missing or held by another attempt is answered as a clean `NoSuchUpload`, not as damage.** The arms are `commit.rs:993` and `:879`, with the doc at `commit.rs:108-110`. The iteration-1 sign-off asked for exactly this ("foreign slot -> NoSuchUpload"), so the builder followed instructions. But 0016 says a slot is removed only by its own commit or compensation, or together with the session at the terminal delete (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:578`, `:1683-1684`), and `abort.rs` writes no slot. The slot read (`commit.rs:976`) comes before the last session read (`:977`). So if that last read still finds a fenced session, the session also existed when the slot was read, and a missing or foreign slot then has no legal cause. That is the same argument the patch uses to call a slot behind a *missing* session damage (`commit.rs:89-95`). The code is strict in that case and lenient for a fenced session. The impact is small: nothing is written either way, and the client gets a refusal instead of a record stop. A human should confirm the sign-off decision still stands given `0016:578`. If it does, nothing changes.
- [x] **The T4 "pass" is weak evidence.** The `T4-batch-review` row in `check-gates.json` reports "3x codex" with 0 blocking findings in **22.41 s** (`gate-logs/T4-batch-review.log`) over a 2,341-line patch. That is too fast for three deep passes; the result was probably cached or run at default settings. The brief (`brief.md:111-112`) says a clean T4 row at default settings is weak evidence and that sign-off needs a confirmed high-effort review of the final patch. Confirm that review happened before accepting.
- [x] **A foreign slot can be mistaken for proof of terminal deletion.** `brief.md:137-138` treats a missing session plus a subsequently “gone” slot as `NoSuchUpload`, but `brief.md:146-148` defines “gone” as absent **or holding another attempt's `attempt_id`**. That admits this damaged state: the session read returns absent, then the slot read returns a record owned by another attempt. The target protocol deletes all surviving slots with the session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:674`), and a new reservation requires the session's `Open` bytes (`crates/core/src/multipart/stage.rs:12`, `:1325-1329`). A foreign slot still present after the missing-session read therefore does not establish a legitimate terminal delete. Distinguish “no longer owned by this attempt” from actual key absence in the terminal branch; specify a damage stop for a surviving foreign slot and cover it in both commit and compensation tests.
- [x] **The prescribed chase test cannot demonstrate the prescribed bound mutation.** `brief.md:52-53` schedules one rollback against the commit and only two against compensations, followed by successful compensation: two failed compensation attempts plus one successful attempt totals **three**. Yet `brief.md:154-155` sets `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1`, which is **four** on this target (`crates/core/src/multipart.rs:5279`), and `brief.md:101` requires lowering that bound by one to fail this chase leg. A bound of three still permits the entire stated schedule. Revise the fixture to enter compensation before spending any of the three rollback opportunities, then defeat three compensations before the fourth lands, or revise the claimed mutation evidence. Keep the all-Conflict exact-count test separate: it can detect the smaller budget, but does not make this chase schedule exercise a fourth compensation.
- [x] size backstop — this slice is behaving oversized: patch is 127 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- issue_1028: add a behaviour test for a compensation that finds its own obligation key taken (commit.rs:864 — dropping the `?` survives all 47 tests).
- issue_1028: add a rolled-back-session leg to the slot test at both re-reads (commit.rs:989, :875 — `Open(_)`+`Gone` arms untested).
