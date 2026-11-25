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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 34 mutants tested in 3m: 1 missed, 20 caught, 11 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1028/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.83s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1028’s multipart commit-conflict classification and compensation: independently reproduced red→green, with one regression-test gap requiring builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract distinguishes live-session compensation, fencing, terminal deletion and damage, with explicit bounds and unknown outcomes excluded; brief.md:15, brief.md:36, brief.md:86. |
| C2 Reproduction (red pre-fix) | PASS | Eight extracted behavior tests compiled and failed on the stashed pre-fix tree with `CommitConflict`, establishing the missing classification behavior rather than an API compilation failure; reviewer-red.log:20, reviewer-red.log:85. |
| C3 Change | PASS | The four-file change stays within conflict classification and its tests/docs, preserves unknown-outcome stops and adds no production caller; crates/core/src/multipart/commit.rs:1018, crates/core/src/multipart/commit.rs:1142, docs/design/architecture/05-building-block-view.md:250. |
| C4 Verification (red→green) | PASS | The same eight tests passed after stash-pop; all 47 shipped tests and independent format/spelling/docs checks passed; full CI is supported by its frozen log, while diff coverage remains unmeasured; reviewer-restored-green.log:15, reviewer-final-green.log:54, reviewer-checks.log:7, gate-logs/C4-ci.log:4766. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add fenced-session regressions distinguishing foreign slots from rewritten own slots at both re-reads — reversing identity classification silently swaps damage and refusal while all 47 shipped tests pass; crates/core/tests/multipart_upload_part_commit.rs:3488, crates/core/src/multipart/commit.rs:569, reviewer-mutant-slot.log:16. |
| T1 Structure | PASS | Classification and compensation remain behind the existing metadata and clock seams, with atomic batches requiring the attempt’s own slot bytes; crates/core/src/multipart/commit.rs:768, crates/core/src/multipart/commit.rs:913. |
| T2 Shape | PASS | The public refusal meaning and living architecture agree, and independent formatting, spelling and docs validation passed; crates/core/src/multipart.rs:4755, docs/design/architecture/05-building-block-view.md:250, reviewer-checks.log:8. |
| T3 Runtime | PASS | Four-round commit/compensation limits, per-round lease checks on the staging clock and bounded backend operations constrain retries; exact-count and lease tests passed; crates/core/src/multipart/commit.rs:229, crates/core/src/multipart/commit.rs:241, crates/core/src/multipart/commit.rs:1034, crates/traits/src/lib.rs:1334, crates/core/tests/multipart_upload_part_commit.rs:3572. |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art by all four affected file paths — the available history is one synthetic base commit, and the brief’s merged/open claim supplies neither path queries nor closed-work evidence; reviewer-prior-art.log:1, brief.md:130. |
| T5 Judgment | PASS | The patch addresses the lost-commit cause without a capability-probe workaround; existing DST and unknown-outcome deferrals remain settled under the review protocol, with no production wiring added; AGENTS.md:215, crates/core/src/multipart/commit.rs:191, crates/core/src/multipart/commit.rs:826, crates/core/src/multipart/commit.rs:1018. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that releasing live-session losers with `OperationAborted`, while leaving fenced residue to the reaper, meets the intended client/reclamation contract for this core-only slice — production S3 behavior is deliberately outside this verification; brief.md:113, crates/core/src/multipart.rs:4755. |

One test gap is demonstrated; no production-logic defect was demonstrated in the reviewed scope. The existing slot-damage test exercises only a live session, where both `Gone` and `Rewritten` produce `SlotLost` (`crates/core/tests/multipart_upload_part_commit.rs:3483`). Consequently it cannot detect swapping those classifications. A fenced session distinguishes them: a foreign slot permits `NoSuchUpload`, whereas this attempt’s rewritten slot reports `SlotLost` (`crates/core/src/multipart/commit.rs:846`, `crates/core/src/multipart/commit.rs:943`).

- Independent reproduction: changing only `==` to `!=` at `crates/core/src/multipart/commit.rs:569` leaves all 47 shipped tests green (`reviewer-mutant-existing.log:54`). Four reviewer probes covering commit/compensation × foreign/rewritten slot pass on the submitted implementation (`reviewer-slot-green.log:11`) and all fail under that mutation (`reviewer-mutant-slot.log:45`). The mutation therefore changes observable error semantics; it is not equivalent.
- Builder action: retain these four outcome distinctions in the committed regression suite, asserting no further batch or release after classification and preservation of the slot bytes. The executable probe source is preserved as `reviewer-executed-probe.rs`; its final four tests contain the counterexamples. The submitted implementation already passes them.
- Scope of the finding: this is missing regression protection for the new classifier. Seeded DST coverage is a separate, explicitly settled deferral to #660 and is not requested again here.

The frozen gates support the following conclusions. Their wrapper paths are instance-scoped and absent from this target (`reviewer-checks.log:1`); their logs were read rather than treating that absence as a defect. Full workspace CI and the TiKV feature builds were not independently rerun. The table distinguishes their recorded results from this advisory assessment; `check-gates.json` remains unchanged.

| Gate | Recorded result | Advisory assessment and evidence |
|------|-----------------|----------------------------------|
| C4-ci | pass | PASS on frozen evidence: actual typos, docs render/link audit, fmt, clippy, build, tests, dependency checks and DST complete successfully; gate-logs/C4-ci.log:11, gate-logs/C4-ci.log:4135, gate-logs/C4-ci.log:4387, gate-logs/C4-ci.log:4766. |
| C4-verify | pass | Its log proves green-only, not red→green (gate-logs/C4-verify.log:161). Independent stash/run/pop/run supplies eight compiled behavioral reds and eight greens; reviewer-red.log:85, reviewer-restored-green.log:15. |
| C4-diff-cov | fail | N/A as a coverage measurement: the gate stopped because the patch would not apply to origin/main; no changed-line coverage was measured (gate-logs/C4-diff-cov.log:10). This is a base-state caveat for a dependent slice, not a demonstrated patch/compile defect; the supplied target compiled and passed. |
| C5-mutants | fail | One non-equivalent survivor confirmed independently and routed to C5 above; the log also records 20 caught, 11 unviable and two timeout mutants, not three surviving behaviors; gate-logs/C5-mutants.log:13. |
| T4-batch-review | fail | The logged DST finding is rejected as already deferred: crates/core/src/multipart/commit.rs:191 names #660, and AGENTS.md:215 explicitly settles such findings. The original gate failure remains recorded; gate-logs/T4-batch-review.log:10. |
| T4-contribution | deferred | N/A: contribution artifacts are drafted later; the substantive audit must rerun at publish, as gate-logs/T4-contribution.log:10 states. No human clearance is owed for this deferral. |
| host-tikv | pass | PASS on frozen compile evidence for the requested crate/server feature checks; both commands finish successfully, without claiming a live TiKV runtime exercise; gate-logs/host-tikv.log:2, gate-logs/host-tikv.log:112, gate-logs/host-tikv.log:211. |

The independent red→green run isolates eight unchanged test bodies and their shared harness from the submitted test file, with only unused-helper/import warnings allowed in the extracted test crate. They cover fencing, rollback, rival, repair, unexplained conflict, both terminal-delete entry paths and the epoch chase. All eight fail at the pre-fix commit stop; this establishes the missing behavior, not independent pre-fix reachability of every later compensation branch. The original 47-test suite passes after restoration (`reviewer-final-green.log:54`). The extracted source and logs remain in this review sandbox. No builder rationale was consulted.

The declared external dependencies were exercised: typos exited successfully, docs lint reported OK, and the real renderer fetched Mermaid, rendered 101 pages and passed its link audit (`reviewer-checks.log:9`, `reviewer-docs-render.log:1`). No unmet external dependency is inferred from the already deferred DST work. No target `INTEGRATION.md` was supplied (`reviewer-checks.log:6`).

Prior-art confirmation remains a human decision because the supplied evidence cannot establish it. A local history query naming `crates/core/src/multipart/commit.rs`, `crates/core/src/multipart.rs`, `crates/core/tests/multipart_upload_part_commit.rs` and `docs/design/architecture/05-building-block-view.md` returns only synthetic commit `580d9c2`; there are no remotes (`reviewer-prior-art.log:1`). The brief’s historical merged/open statement does not establish the required closed/rejected check. No other checkout was inspected.

All source citations above resolve on the supplied `$PDCA_TARGET`; input and reviewer-log citations resolve in this sandbox. The target supports the patch and is not stale for the independent tests. Stash/pop and the mutation trial restored its tracked diff byte-for-byte; the reviewer test was moved back into the review sandbox, leaving only the original four-file patch (`reviewer-checks.log:12`). This report is advisory and does not override deterministic gate results.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1028 (657.2b, part commit: classify a lost commit)

Short version: the classification logic held up against every legal history I could build, and the
red→green evidence is real (I re-ran it, see the last section). I found two missed cases the
builder can fix, one gating T4 blocker that the rubric says is already settled, and one overclaim
in the brief.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:569` (`judge_slot`'s `attempt_id`
  compare): C5's one MISSED mutant (`==` → `!=`) really is untested. With it applied in a scratch
  copy, all 47 tests in `crates/core/tests/multipart_upload_part_commit.rs` still pass. The mutant
  swaps `Rewritten` and `Gone`, and the two only lead to different answers under a **fenced**
  session (`commit.rs:846-850`, `:943-947`). The module doc fixes both answers: a slot rewritten
  under this attempt's id is `SlotLost` "under any session record", and a fenced session "answers
  its refusal whatever else its slot holds". But `r_a_slot_lost_or_rewritten_at_a_re_read_stops_without_a_release`
  (tests `:3488`) only runs under a live session. Concrete case: real Abort plus `rewritten_slot`
  planted before the commit gives `SlotLost` on the patch and `Refused(NoSuchUpload)` under the
  mutant; Abort plus `foreign_slot` gives `NoSuchUpload` on the patch and `SlotLost` under the
  mutant. Both probes go red under the mutant. Fix: add a fenced ("abort") leg to that test, at
  the commit's re-read and at the compensation's.

- NEEDS-HUMAN [impl] — brief (O) does not hold for an **empty** re-upload over a non-empty prior.
  `commit_batch` puts `require_absent` on the retire key whenever the prior has chunks
  (`commit.rs:749-751`, built at `:1004`), but `lost_commit` (`commit.rs:913-966`) never reads that
  key. So a taken key looks like "a conflict no read explains", and all 4 commit rounds are spent on
  it. For an attempt with chunks, the compensation that follows trips on the same key and ends
  `ObligationKeyTaken`; that is the only case `o_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part`
  (tests `:3538`) covers. For an empty attempt, `obligation_for` returns `None`
  (`commit.rs:719-720`), so the compensation has no `require_absent`, lands, and the answer is
  `Stopped { CommitRoundsSpent { rounds: 4 }, slot_released: true }`. Scratch probe output:
  `cause: CommitRoundsSpent { rounds: 4 }, slot_released: true`, commits=4, compensations=1,
  planted bytes and prior kept. The damage at the key is never reported; the message says "nothing
  a read could see to explain it". That is the rubric's "silent skip" class, and the brief's (O)
  says "ending in `Stop::ObligationKeyTaken`" for any re-upload over a non-empty prior. A fix that
  keeps the brief's four-read round order (657.2c needs it): when `retire.is_some()`, read the
  retire key once on the rounds-spent path (`commit.rs:1028-1030`) before compensating, and stop
  `ObligationKeyTaken` if it holds a record. Add the empty-attempt leg to the (O) test.

- NEEDS-HUMAN [human] — the gating T4-batch-review row fails on one blocker
  (`multipart_upload_part_commit.rs:2950`, "only scripted Tokio coverage … requires seeded Tier-0
  DST coverage"). That re-raises a deferral the rubric treats as settled: `commit.rs:191-192`
  already carries `// deferred: #660 — seeded Tier-0 DST coverage of the commit and compensation
  races (a commit against a rival, an Abort, a Complete's rollback, the reaper)`. The marker
  existed before this patch (it is in the base `HEAD`), and the brief's scope puts DST out of
  scope. Under "Deferrals are settled", this blocker should be recorded as rejected with that
  reason, not fixed in this PR. A human has to record that, because the row gates.

- NEEDS-HUMAN [human] — overclaim in the brief and the test it asked for. Brief (L) and
  `l_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` (tests
  `:3647`) say three rollbacks (`MAX_COMPLETE_ATTEMPTS`) is "the most a session can see". The
  patch's own doc at `commit.rs:235-238` says otherwise: a Complete rejected behind its fence gives
  its attempt back (#1007), so rejected Completes can move the session any number of times.
  Concrete case: four rejected Completes (each a fence plus release, `attempts` unchanged) landing
  between a rival-loser's compensation rounds end in `CompensationRoundsSpent`, with the slot held
  under an `Open` session until `W_session`. The code is honest ("a budget for the work, not for
  the answer"), and the leaked slot falls in the class the brief already accepted (slots held
  after a Complete rollback, sign-off iteration 2). So this is about wording, not behaviour: decide
  whether the test name and the brief's "real bound" claim stand. Low priority.

## The evidence, re-checked (not findings)

- C4-verify proved only green ("PASS (green-only)"), and C4-diff-cov never ran: the bundle does
  not apply on `origin/main`, which lacks #1027. So I rebuilt the target in scratch. The suite
  passes 47/47, and I re-ran nine of the brief's demonstrated reds. Each failed the named tests:
  `COMPENSATION_ROUNDS` lowered by one (both `l_` loop tests); the earlier slot read taken as final
  (both `t_a_terminal_delete_between_…`); a missing session always read as a terminal delete
  (`t_a_missing_session_…`, `t_another_attempts_slot_…`); the lease check skipped on later rounds;
  a non-`Open` record at an unrisen epoch read as a fence; a rollback answered `NoSuchUpload` at
  the commit and in the chase; compensating on any `Conflict`; a damaged part read as a rival.
- The tests drive production `commit_part`. The rival's commit also goes through `commit_part` on
  the inner store, and Abort is the real `abort_session`. One parallel construction: the rollback
  is a text edit of the session record (`roll_back`, tests `:1370`), not Complete's
  `fence_released`. That is enough for this slice, because only the epoch and the state are judged.
- The two TIMEOUT mutants (`commit.rs:822`, `:1009`, `+=` → `*=`) leave the round counter at 0,
  so the loop never ends. The leg's 60 s `LEG_TIMEOUT` (tests `:90`) would fail them; cargo-mutants'
  20 s cap simply cut them first. Not a gap.
- I tried to break these and could not. "Other bytes at an unrisen epoch is damage": every `mpu:`
  writer bumps the epoch (`multipart.rs:2630`, `:2654`, `:2682`, `:2712`). "A gone or rewritten
  slot under a live session is `SlotLost`": only `stage.rs` writes `slot:`, and Complete's release
  (`complete.rs:875-910`) does not touch slots. Also checked: the (T) extra slot read after a
  missing session, the order of judgment (damage before rival), and the stated bound on all three
  loops.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1028` (also `:820`): An empty re-upload over a non-empty prior misses an occupied retirement-obligation key. Its commit requires that key absent, but the conflict reads never inspect it. After four conflicts, compensation derives its obligation from the empty attempt, sends only the slot release, and succeeds; the result is `CommitRoundsSpent` with `slot_released = true`, rather than the required `ObligationKeyTaken`. The prior and planted bytes survive, but the corruption is misclassified as retry exhaustion. Check the commit's obligation key independently of whether compensation needs an obligation, and extend `crates/core/tests/multipart_upload_part_commit.rs:3538` with an empty replacement.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:3488`: The new foreign-slot and rewritten-slot cases run only under an `Open` session, where both classifications return `SlotLost`. They therefore cannot detect reversing the attempt-ID comparison at `crates/core/src/multipart/commit.rs:569`; the frozen C5 log confirms that mutant survives. Add fenced-session cases for both commit and compensation re-reads: a foreign slot should yield `NoSuchUpload`, while changed bytes carrying this attempt's ID should yield `SlotLost`, with no further writes. This checks the distinction where it changes the observable answer.

No additional reuse, simplification, or efficiency findings. Review used the supplied diff and frozen gate evidence plus read-only target-source inspection; no tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add fenced-session regressions distinguishing foreign slots from rewritten own slots at both re-reads — reversing identity classification silently swaps damage and refusal while all 47 shipped tests pass; crates/core/tests/multipart_upload_part_commit.rs:3488, crates/core/src/multipart/commit.rs:569, reviewer-mutant-slot.log:16.
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art by all four affected file paths — the available history is one synthetic base commit, and the brief’s merged/open claim supplies neither path queries nor closed-work evidence; reviewer-prior-art.log:1, brief.md:130.
- [ ] Validation — fitness-to-purpose — Confirm that releasing live-session losers with `OperationAborted`, while leaving fenced residue to the reaper, meets the intended client/reclamation contract for this core-only slice — production S3 behavior is deliberately outside this verification; brief.md:113, crates/core/src/multipart.rs:4755.
- [ ] `crates/core/src/multipart/commit.rs:569` (`judge_slot`'s `attempt_id` compare): C5's one MISSED mutant (`==` → `!=`) really is untested. With it applied in a scratch copy, all 47 tests in `crates/core/tests/multipart_upload_part_commit.rs` still pass. The mutant swaps `Rewritten` and `Gone`, and the two only lead to different answers under a **fenced** session (`commit.rs:846-850`, `:943-947`). The module doc fixes both answers: a slot rewritten under this attempt's id is `SlotLost` "under any session record", and a fenced session "answers its refusal whatever else its slot holds". But `r_a_slot_lost_or_rewritten_at_a_re_read_stops_without_a_release` (tests `:3488`) only runs under a live session. Concrete case: real Abort plus `rewritten_slot` planted before the commit gives `SlotLost` on the patch and `Refused(NoSuchUpload)` under the mutant; Abort plus `foreign_slot` gives `NoSuchUpload` on the patch and `SlotLost` under the mutant. Both probes go red under the mutant. Fix: add a fenced ("abort") leg to that test, at the commit's re-read and at the compensation's.
- [ ] brief (O) does not hold for an **empty** re-upload over a non-empty prior. `commit_batch` puts `require_absent` on the retire key whenever the prior has chunks (`commit.rs:749-751`, built at `:1004`), but `lost_commit` (`commit.rs:913-966`) never reads that key. So a taken key looks like "a conflict no read explains", and all 4 commit rounds are spent on it. For an attempt with chunks, the compensation that follows trips on the same key and ends `ObligationKeyTaken`; that is the only case `o_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` (tests `:3538`) covers. For an empty attempt, `obligation_for` returns `None` (`commit.rs:719-720`), so the compensation has no `require_absent`, lands, and the answer is `Stopped { CommitRoundsSpent { rounds: 4 }, slot_released: true }`. Scratch probe output: `cause: CommitRoundsSpent { rounds: 4 }, slot_released: true`, commits=4, compensations=1, planted bytes and prior kept. The damage at the key is never reported; the message says "nothing a read could see to explain it". That is the rubric's "silent skip" class, and the brief's (O) says "ending in `Stop::ObligationKeyTaken`" for any re-upload over a non-empty prior. A fix that keeps the brief's four-read round order (657.2c needs it): when `retire.is_some()`, read the retire key once on the rounds-spent path (`commit.rs:1028-1030`) before compensating, and stop `ObligationKeyTaken` if it holds a record. Add the empty-attempt leg to the (O) test.
- [ ] the gating T4-batch-review row fails on one blocker (`multipart_upload_part_commit.rs:2950`, "only scripted Tokio coverage … requires seeded Tier-0 DST coverage"). That re-raises a deferral the rubric treats as settled: `commit.rs:191-192` already carries `// deferred: #660 — seeded Tier-0 DST coverage of the commit and compensation races (a commit against a rival, an Abort, a Complete's rollback, the reaper)`. The marker existed before this patch (it is in the base `HEAD`), and the brief's scope puts DST out of scope. Under "Deferrals are settled", this blocker should be recorded as rejected with that reason, not fixed in this PR. A human has to record that, because the row gates.
- [ ] overclaim in the brief and the test it asked for. Brief (L) and `l_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` (tests `:3647`) say three rollbacks (`MAX_COMPLETE_ATTEMPTS`) is "the most a session can see". The patch's own doc at `commit.rs:235-238` says otherwise: a Complete rejected behind its fence gives its attempt back (#1007), so rejected Completes can move the session any number of times. Concrete case: four rejected Completes (each a fence plus release, `attempts` unchanged) landing between a rival-loser's compensation rounds end in `CompensationRoundsSpent`, with the slot held under an `Open` session until `W_session`. The code is honest ("a budget for the work, not for the answer"), and the leaked slot falls in the class the brief already accepted (slots held after a Complete rollback, sign-off iteration 2). So this is about wording, not behaviour: decide whether the test name and the brief's "real bound" claim stand. Low priority.
- [ ] `crates/core/src/multipart/commit.rs:1028` (also `:820`): An empty re-upload over a non-empty prior misses an occupied retirement-obligation key. Its commit requires that key absent, but the conflict reads never inspect it. After four conflicts, compensation derives its obligation from the empty attempt, sends only the slot release, and succeeds; the result is `CommitRoundsSpent` with `slot_released = true`, rather than the required `ObligationKeyTaken`. The prior and planted bytes survive, but the corruption is misclassified as retry exhaustion. Check the commit's obligation key independently of whether compensation needs an obligation, and extend `crates/core/tests/multipart_upload_part_commit.rs:3538` with an empty replacement.
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:3488`: The new foreign-slot and rewritten-slot cases run only under an `Open` session, where both classifications return `SlotLost`. They therefore cannot detect reversing the attempt-ID comparison at `crates/core/src/multipart/commit.rs:569`; the frozen C5 log confirms that mutant survives. Add fenced-session cases for both commit and compensation re-reads: a foreign slot should yield `NoSuchUpload`, while changed bytes carrying this attempt's ID should yield `SlotLost`, with no further writes. This checks the distinction where it changes the observable answer.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1028/review-
- [ ] **A foreign slot can be mistaken for proof of terminal deletion.** `brief.md:137-138` treats a missing session plus a subsequently “gone” slot as `NoSuchUpload`, but `brief.md:146-148` defines “gone” as absent **or holding another attempt's `attempt_id`**. That admits this damaged state: the session read returns absent, then the slot read returns a record owned by another attempt. The target protocol deletes all surviving slots with the session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:674`), and a new reservation requires the session's `Open` bytes (`crates/core/src/multipart/stage.rs:12`, `:1325-1329`). A foreign slot still present after the missing-session read therefore does not establish a legitimate terminal delete. Distinguish “no longer owned by this attempt” from actual key absence in the terminal branch; specify a damage stop for a surviving foreign slot and cover it in both commit and compensation tests.
- [ ] **The prescribed chase test cannot demonstrate the prescribed bound mutation.** `brief.md:52-53` schedules one rollback against the commit and only two against compensations, followed by successful compensation: two failed compensation attempts plus one successful attempt totals **three**. Yet `brief.md:154-155` sets `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1`, which is **four** on this target (`crates/core/src/multipart.rs:5279`), and `brief.md:101` requires lowering that bound by one to fail this chase leg. A bound of three still permits the entire stated schedule. Revise the fixture to enter compensation before spending any of the three rollback opportunities, then defeat three compensations before the fourth lands, or revise the claimed mutation evidence. Keep the all-Conflict exact-count test separate: it can detect the smaller budget, but does not make this chase schedule exercise a fourth compensation.
- [ ] size backstop — this slice is behaving oversized: patch is 116 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): (1) Fix (O) for an EMPTY re-upload over a non-empty prior: on the rounds-spent path (commit.rs:1028/:820) read the commit's retire key and stop ObligationKeyTaken if occupied; add the empty-attempt leg to the (O) test (multipart_upload_part_commit.rs:3538). (2) Add fenced-session (Abort) legs to the slot test (:3488) at both the commit's and the compensation's re-read — foreign slot -> NoSuchUpload, own-id rewritten slot -> SlotLost — so the commit.rs:569 `==`->`!=` mutant dies. (3) Reword the "most a session can see" claim in the chase test name/comment (rejected Completes give their attempt back, commit.rs:235-238). Do NOT add DST: the T4-batch-review blocker is a settled deferral to #660 (commit.rs:191-192). Do NOT re-split: prod code is ~33 KB.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
