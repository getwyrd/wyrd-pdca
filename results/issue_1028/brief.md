# core: part commit — a commit that lost is classified and compensated (657.2b)

> Second of the four slices of #927 (657.2). It builds on 657.2a, whose commit stops on any `Conflict`
> with the slot held. Read 0016's commit classification (`0016:1584-1653`) and the **Live-session
> compensation** and **Terminal session delete** rows (`0016:673`, `:674`). Citations were verified at
> `376b106` on 2026-11-01; re-resolve them on your base, which folds in 657.2a.

- **Slug:** multipart-part-commit-classify-conflict
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.2a a commit or compensation answered `Conflict` stops and leaves its slot held.
  Under a live session nothing else frees that slot before `W_session`, because the reaper does not
  walk an `Open` session (`0016:1600-1620`). This slice classifies a `Conflict` from a re-read:
  the session fenced, the session rolled back to a newer `Open@E'`, a same-number rival, or a benign
  retry. It compensates where the session is live, chases a further rollback, and bounds every loop.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`, with the
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
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. The wrapper forces every
  red leg: the rollback and Abort are written by hook, (T)'s delete lands by hook between two reads, and
  rounds are answered `Conflict` by script. Attempt 7's finding 1 was reproduced against its patched tree by
  independent probes. (E) is the gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). A
  commit that lost to a live session releases its slot and names its own chunks in an obligation, in one
  batch, because no reaper will. An `Open` session is never answered `NoSuchUpload`. A record that does not
  decode, or an epoch that did not rise, is never evidence of a rival or a fence (ADR-0045). A slot read made
  before the read that found the change proves nothing. **Sources:** the maintainer's rule 2026-07-25;
  0016's refutation standard (`0016:2809-2813`); the classification (`0016:1584-1653`); ADR-0045. SELF-TEST:
  it binds the commit's and the compensation's re-reads, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1027
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2a: this slice replaces its `Conflict` stops, in its function and test
  file. 657.2c builds on this slice's round. Intake: created by the split of #927 under the human's per-id
  cap override of 2026-11-01.
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** what a `Conflict` means for a commit and for a compensation: the re-read, the classification,
  the retry, the epoch chase, the loop bounds, and the wider doc of `Refusal::OperationAborted`, with no new
  variant. / out of scope:
  - any batch answered `CommitUnknownResult`: 657.2a's in-doubt stop stays until 657.2c and 657.2d;
  - slots left held when a Complete fence rolls back (decided at sign-off, iteration 2: accept until
    `W_session`, raise against 0016, do not change it here);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`; the
    gateway and any production caller; Complete, the drain, the reaper, DST.
- **Repro instruction:** n/a (new functionality). On 657.2a's base a commit answered `Conflict` stops with
  the slot held, and that stop has a test.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**, on the harness 657.2a shipped.
- **Verification posture:** C4-verify reports `PASS (green-only)`, because no test file is added
  (`run-verify.sh --classify`), so a pre-declared sign-off item. **Demonstrated reds (required, recorded in
  `build-notes.md`).** Each temporary edit fails the named leg:
  - a slot read made before the session read taken as final, with no further slot read → (T), commit and compensation;
  - a missing session record always read as a terminal delete → (T)'s still-held leg;
  - another attempt's slot counted as gone behind a missing session → (T)'s foreign-slot leg;
  - compensating on any `Conflict` without the re-read → (B)'s fenced leg;
  - the rolled-back `Open@E'` sent to the reaper → (B)'s rollback leg;
  - the rollback or rival loser answering any refusal other than `OperationAborted` → (B);
  - a damaged part read as a rival → (R); a non-`Open` record at an unrisen epoch read as a fence → (R);
  - `require_absent` removed from the obligation key → (O);
  - a loop counter that never reaches its bound → (L); `COMPENSATION_ROUNDS` lowered by one → (L)'s rival-first chase run;
  - the lease check skipped on a later round → (L).
  **C5 (advisory) survivors** are listed in `build-notes.md`. **Review depth:** sign-off needs a confirmed
  high-effort review of the final patch; a clean T4 row at default settings is weak evidence.
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice.
  (b) The S3 verb lands with #669 after 657.2d. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the classification:** #895's Abort, `crates/core/src/multipart/abort.rs:282`.
  - **Start from attempt 7.** Do MAY open `results/issue_927/iteration-v7/patch.diff` as a reference; its
    round reads the part, the session, the slot, then the session again, and this brief overrides it on (T).
    Use that read order: 657.2c needs it. Its tests for this slice:
    `b_a_fenced_session_answers_no_such_upload_and_writes_nothing`, `b_a_rolled_back_session_is_compensated_under_its_newer_epoch`,
    `b_a_same_part_loser_compensates_its_own_chunks`, `c_every_loop_ends_after_exactly_its_bound`,
    `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part`, `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates`,
    `g_a_slot_lost_or_a_damaged_record_at_a_re_read_stops_without_a_release`, `g_a_settled_stops_compensation_chases_stops_or_reports_itself`,
    `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see`, `g_a_damaged_part_record_at_the_re_read_is_a_record_stop_not_a_rival`,
    `g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence`, `g_a_prior_part_record_gone_under_the_same_session_is_damage`,
    `g_a_damaged_part_record_under_a_moved_session_is_a_record_stop`, the loser leg of `c4_empty_parts_never_install_an_empty_obligation`,
    the stop legs of `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate`, and the no-doubt legs of
    `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
  - **The doc to widen:** `Refusal::OperationAborted` (`multipart.rs:4557-4559`), which says only "fenced into `Completing`".
- **Prior-art check (triage cycles):** as 657.2a, on `origin/main` @ `376b106`, 2026-11-01. No merged or open
  work classifies a part commit. #927's attempts 1 to 7 are the source.
- **Disposition hint:** new-feature

## Design

**After a `Conflict`, classify from a re-read** (`0016:1597-1647`). Each round reads the part key, the
session, the attempt's own slot, then the session again. Only the last session read decides the answer.

| The session (last read) | The part key | Answer |
|---|---|---|
| still `Open@E` | what the commit requires | a conflict no read explains (on TiKV, a lock another writer held): retry the same batch |
| still `Open@E` | changed: a same-number rival won, or a repair rewrote the prior | compensate under `Open@E`; `Refused(OperationAborted)` |
| back at a newer `Open@E'` | any | compensate under `Open@E'`; `Refused(OperationAborted)` |
| not `Open`, at a higher epoch (a fence) | any | `Refused(NoSuchUpload)`; the residue is the reaper's |
| gone, and a slot read made **after** it found the key **absent** | any | `Refused(NoSuchUpload)`; the delete took them in one batch (`0016:674`) |
| gone, and that later slot read found any record there, this attempt's or another's | any | `Stop::Record(SessionRecordMissing)`: damage |

**A session read is evidence only once judged.** It has **moved** when the record decodes at a higher epoch
than the bytes the batch required, whether `Open` (a rollback) or not (a fence). Every fence and rollback
bumps the epoch (`0016:364`, `:704-708`). Bytes that do not decode, and any record under other bytes at the
same or a lower epoch, whatever its state, are damage. At an earlier read damage is passed over; at the
round's last read it stops the attempt.

**A slot found gone with nothing in doubt** means it was lost: `SlotLost`, unless a moved or vanished session
explains it, as in the table. "Gone" means absent **or** holding another attempt's `attempt_id`
(`multipart.rs:2796`). The table's missing-session rows do not use that wider meaning: they test the key itself.
Every batch requires the bytes **this attempt last wrote**, never bytes re-read from the key.

**The answer for a lost commit on a live session** is exactly `Refused(OperationAborted)`: a conflicting
operation intervened and the client may retry the part (ratified at sign-off). Widen that variant's doc to
name a rival, a rollback and a repair rewriting the prior. Add no new `Refusal`. #669 maps it to the wire.

**The epoch chase.** A compensation that lost to a newer `Open` epoch is sent again under that epoch, up to
`COMPENSATION_ROUNDS` (`MAX_COMPLETE_ATTEMPTS + 1`, `multipart.rs:5279`). One that finds the session no longer
`Open` stops, the residue the reaper's. **Empty chunk lists:** no obligation, and the slot release and `sidx:`
deletes stay in the same batch.

## Alternatives considered

- **Compensate on any non-`Committed` outcome.** It orphan-marks a part that exists (`0016:1792`). Rejected.
- **A retry against a repaired part's new record.** Rejected at sign-off, iteration 4.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-11-01, revision pass): both findings hold. The terminal branch now needs the slot key absent, and a foreign slot behind a missing session is a damage stop at both re-reads. The chase now spends all four compensations in a rival-first run, matching attempt 7's second run, so lowering the bound fails it.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: (1) Fix (O) for an EMPTY re-upload over a non-empty prior: on the rounds-spent path (commit.rs:1028/:820) read the commit's retire key and stop ObligationKeyTaken if occupied; add the empty-attempt leg to the (O) test (multipart_upload_part_commit.rs:3538). (2) Add fenced-session (Abort) legs to the slot test (:3488) at both the commit's and the compensation's re-read — foreign slot -> NoSuchUpload, own-id rewritten slot -> SlotLost — so the commit.rs:569 `==`->`!=` mutant dies. (3) Reword the "most a session can see" claim in the chase test name/comment (rejected Completes give their attempt back, commit.rs:235-238). Do NOT add DST: the T4-batch-review blocker is a settled deferral to #660 (commit.rs:191-192). Do NOT re-split: prod code is ~33 KB.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  (1) Fix (O) for an EMPTY re-upload over a non-empty prior: on the rounds-spent path (commit.rs:1028/:820) read the commit's retire key and stop ObligationKeyTaken if occupied; add the empty-attempt leg to the (O) test (multipart_upload_part_commit.rs:3538).
  (2) Add fenced-session (Abort) legs to the slot test (:3488) at both the commit's and the compensation's re-read — foreign slot -> NoSuchUpload, own-id rewritten slot -> SlotLost — so the commit.rs:569 `==`->`!=` mutant dies.
  (3) Reword the "most a session can see" claim in the chase test name/comment (rejected Completes give their attempt back, commit.rs:235-238).
  Do NOT add DST: the T4-batch-review blocker is a settled deferral to #660 (commit.rs:191-192). Do NOT re-split: prod code is ~33 KB.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 34 mutants tested in 3m: 1 missed, 20 caught, 11 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1028/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
