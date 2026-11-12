# core: part commit — a compensation with an unknown outcome is settled or stays in doubt (657.2d)

> Last of the four slices of #927 (657.2). It builds on 657.2c. Citations were verified at `376b106` on 2026-11-01;
> re-resolve them on your base, which folds in 657.2a, 657.2b and 657.2c.
>
> **What is verified where.** On `376b106` nothing here can be reproduced: no part-commit code exists
> (`crates/core/src/multipart.rs:107-109`, `:153-155`), so `commit.rs` and the test file are absent. The
> first half of the defect is the stop #1027 (H4) and #1029 (`Scope`) specify, and it exists only on your
> base. The second half was reproduced against attempt 7's patched tree, which is not on any base
> (`results/issue_927/iteration-v7/SUMMARY.md:342`): "a commit is held, or answered unknown with
> `may_still_commit: false`, and never applied; the slot key is then erased with the session untouched.
> Actual result in both runs: `Ok(Committed)`, with no `part:` record and the owned entry still there",
> and for the compensation, "`Refused(OperationAborted)` with 0 obligations and the owned entry still
> there". The human's decision to fix it here is at `results/issue_927/split-proposal.md:131-134`: "fix
> it in 657.2d, where the store can still show the commit did not land. A re-upload with its slot erased
> stays a documented limit." **Re-run the `Repro instruction` on your base first and record the result in
> `build-notes.md`.** If it does not reproduce, say so and stop that leg.

- **Slug:** multipart-part-commit-settle-compensation
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.2c a compensation answered `CommitUnknownResult` returns the in-doubt error at once, even when
  its own obligation shows it landed. And, with only a key damaged, the own-commit proof can answer `Committed` for a
  commit that never landed. This slice settles a compensation's outcome from re-reads and closes that false answer where the
  store still shows it.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`.
  - **An unknown compensation is settled by re-reading** its own obligation, its own slot and the session (for an empty
    part, slot and session alone). Each attempt lost to a same-number rival, and the session stays `Open`. An in-doubt
    answer fails every run below except the one that names it:

    | The compensation | Non-empty part | Empty part |
    |---|---|---|
    | Lands, answered unknown | `Refused(OperationAborted)`, one send, one obligation, slot released once | `Refused(OperationAborted)`, one send, **no** obligation, slot released once |
    | Not landed, `may_still_commit: false` | Sent again and lands: `Refused(OperationAborted)`, one obligation, slot released once | The same, with **no** obligation |
    | Held, and a later round lands | `Refused(OperationAborted)`, one obligation, slot released once; the held batch, released, answers `Conflict` | The same, with **no** obligation |
    | Held, every later round `Conflict` until they run out | The in-doubt error with `may_still_commit: true` and `Stop::CompensationRoundsSpent`, no obligation yet; the held batch, released, lands: one obligation, slot released once | not run |
    | Every round unknown with `may_still_commit: false`, none applied | Exactly 4 sends, then `Stop::CompensationRoundsSpent`, claiming no release and carrying no `may_still_commit: true`: no obligation, slot still held | The same |
  - **The obligation re-read.** A compensation lands with an unknown outcome, then a client Abort runs before the re-read. Only
    the obligation can settle this: the answer is not in-doubt. A second run plants other bytes at the key and ends in
    `Stop::ObligationKeyTaken`, or `InDoubt` with `ObligationKeyTaken` as its cause under a held compensation.
  - **The epoch chase re-reads every obligation it sent:** the loser's first compensation is held; right after its slot read it
    lands and the session rolls back; the second round conflicts. The answer is `Refused(OperationAborted)`.
  - **A held commit beside an unknown empty compensation.** An empty re-upload over an empty part 1, clock fixed. The commit is
    held; an empty rival lands in the same millisecond. If a compensation is sent, the wrapper first lands the held commit, then
    answers it unknown. The log shows **no compensation batch** while the commit was unsettled; the answer is never
    `Refused(OperationAborted)`; at the end the slot is released once and no `retire:` key exists.
  - **A held compensation under a teardown.** An empty part loses to a rival, its compensation is held, then an Abort and a
    teardown run: the in-doubt error, never `Refused(OperationAborted)`.
  - **A slot gone under a moved session at the compensation's re-read:** the in-doubt error.
  - **Faults and damage under a held compensation.** A store fault at the next obligation, slot or session read, and undecodable
    slot or session bytes: `InDoubt { batch: Compensation, may_still_commit: true }`. Each fails if its doubt-keeping return is
    replaced with a plain stop.
  - **Damage only, no false success** (attempt 7's finding 3; fix decided by the human, 2026-11-01). A commit that required the key **absent** is
    held, or answered unknown with `may_still_commit: false`, and never applied; the slot key is erased with the session untouched.
    The answer is not `Committed`, because a landed commit leaves a record at that key and none is there: it is `Stop::Record` or the
    in-doubt error, with the doubt kept. A non-empty compensation, unknown and unapplied, with its slot erased: no obligation is
    there, so it is not `Refused(OperationAborted)`.
  - **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. Held and unknown compensations, the erased slot and
  each fault are forced by the wrapper. Finding 3 was reproduced against attempt 7's patched tree by independent probes. (E) is the
  gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). A compensation is answered as
  landed only on evidence that it landed, and `Refused(OperationAborted)` promises the attempt's chunks are named in an obligation.
  `Committed` is never answered when the store shows no such commit can have landed. A slot is released exactly once. **Sources:** the
  maintainer's rule 2026-07-25; 0016's refutation standard (`0016:2809-2813`); `0016:1651-1653`, `:1792`. SELF-TEST: it binds the
  compensation's re-read and the own-commit proof, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1029
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2c, whose settled commit is the only thing that lets a compensation be sent in doubt-free form.
  This is the last slice: #668, #669 and #670 are re-pointed here after `--accept`, because a production caller needs the whole
  commit. Intake: created by the split of #927 under the human's per-id cap override of 2026-11-01.
- **Difficulty:** low
- **Do model:** opus
- **Scope:** what a compensation's `CommitUnknownResult` means, and the damage-only false answers of the own-commit proof and the
  compensation. / out of scope:
  - a re-upload over a non-empty or empty prior with its slot erased: no read can tell that apart, since a landed commit and a rival
    both leave a record there. It stays a documented limit of the proof, and no legal writer does this;
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`; the gateway and any production caller;
    Complete, the drain, the reaper, DST.
- **Repro instruction:** On 657.2c's base, hold a commit that required the key absent, erase the slot key, and read the answer: `Committed`
  with no `part:` record (attempt 7, `commit.rs:1012-1018`).
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**.
- **Verification posture:** C4-verify reports `PASS (green-only)`, a pre-declared sign-off item. **Demonstrated reds (required, recorded in
  `build-notes.md`).** Each temporary edit fails the named leg:
  - an unknown compensation treated as "did not land" with `may_still_commit: true` → the unknown-compensation leg;
  - an empty part's unknown compensation returning in-doubt instead of settling on its slot and session → that leg's empty column;
  - the obligation re-read skipped, and its byte comparison made always true → the re-read leg;
  - the chase keeping only the latest obligation → the chase leg;
  - a compensation sent while a commit is held → the held-commit-beside leg;
  - the session check after an unknown compensation made always true → the teardown leg;
  - each doubt-keeping return replaced with a plain stop → the faults and damage leg;
  - the part-key read removed from the own-commit proof → the damage-only leg.
  **C5 (advisory) survivors** are listed in `build-notes.md`. **Review depth:** sign-off needs a confirmed high-effort review of the final patch.
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice. (b) #669 calls this commit, and this
  slice is its last prerequisite among the commit slices. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Start from attempt 7.** Do MAY open `results/issue_927/iteration-v7/patch.diff` as a reference (`compensate`, `commit.rs:1012-1018`, `:1290`).
    Its tests for this slice: `b_a_held_commit_beside_an_unknown_empty_compensation_never_answers_operation_aborted`, `c_an_unknown_compensation_outcome_is_settled_by_a_re_read`,
    `h_a_held_compensation_under_a_teardown_is_in_doubt`, `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes`,
    `h_the_epoch_chase_re_reads_every_obligation_it_sent`, `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read`,
    `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt`, `h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt`, the compensation
    half of `held_damage_at_a_re_read_keeps_the_doubt`, the unknown-outcome leg of `c4_empty_parts_never_install_an_empty_obligation`, and the held-compensation legs of
    `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
- **Prior-art check (triage cycles):** as 657.2a, on `origin/main` @ `376b106`, 2026-11-01. #927's attempts 1 to 7 are the source.
- **Disposition hint:** new-feature

## Design

**Re-read, in order:** the obligation (when there is one), the attempt's own slot, then the session. The obligation holding the bytes the
compensation put, or the slot gone while the session is still the bytes a doubtful compensation required, means it landed. That second test
is sound **only because no commit of this attempt is outstanding** when it compensates (657.2c): the slot compare-and-set shows that one of this
attempt's slot-releasing batches landed, not which one. For an empty part there is no obligation, so slot and session alone decide. With
`may_still_commit: true` the doubt is kept as for the commit, and a slot read made after the session move is needed before letting it go.

**The damage-only answers.** Add one read to the own-commit proof when the commit required the key **absent**: a landed commit leaves a record
there, and a `part:` record is removed only after the session leaves `Open`, so an absent key disproves it. For a non-empty compensation, a slot gone
with no obligation disproves the landing. In both, keep the doubt. An obligation's drain cannot make this a false success: at worst it costs an
in-doubt answer, which is an allowed availability cost.

## Alternatives considered

- **Leave the damage-only answers as they are.** No legal writer does this, but it is a false success, where the earlier damage findings were
  wrong refusals. Rejected by the human, 2026-11-01.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-11-01, revision pass): both findings hold. The brief now quotes the attempt 7 probe and the human's decision, separates what `376b106` shows from what only the prerequisite base can show, and has Do re-run the repro. The blanket "or in-doubt" oracle is replaced by a table of outcomes per scenario, with zero obligations for an empty part, one for a non-empty part, and a red that fails if empty settlement is bypassed.
