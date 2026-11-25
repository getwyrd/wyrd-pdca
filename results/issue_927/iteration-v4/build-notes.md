# Build notes, #927 attempt 4 (Iteration 3 carry-forward)

## What this attempt is

I started from `iteration-v3/patch.diff`. The brief's Citations section names v1, but the carry-forward's
line numbers are on v3's tree and it says "keep the classification design, only the items below change".
From `iteration-v3/` I opened that patch, its `build-notes.md` and `check-gates.md`. I did not read its
reviews. Line numbers below are on the patched tree, base `376b106`. `patch.diff` is 285,945 bytes against
the 100 KB signal; the human's override says do not split. `crates/core/tests/multipart_upload_part_commit.rs`
is 197,598 bytes of that, and `crates/core/src/multipart/commit.rs` is 59,201.

## The changes against the carry-forward

**Item 1, decode each session read.** `crates/core/src/multipart/commit.rs:510` (`Doubt::session_seen`) now
asks `judge_session` (`:618`) instead of comparing bytes, so there is one rule for what "moved" means. It
is called at `:912`, `:930` and `:1179`. Moved means: a record that decodes and is not `Open`, a record at a
higher epoch, or no record. Damage leaves `may_land` as it was, so the doubt and `may_still_commit` survive.
The rule is in the module doc at `commit.rs:85-93`, and `InDoubt::may_still_commit`'s doc is corrected.

The history that mattered: a held commit over an empty prior, a rival, and a session damaged at the first
read and repaired by the last. Under the old code that answered `Refused(OperationAborted)` and compensated
while the held commit could still land. The red table's first "it3-1" row shows that exact wrong answer.

**Item 2, a missing prior part.** `commit.rs:900`: a prior that has gone no longer calls `cannot_land`.
`commit.rs:976-982`: with the slot held and the session still `Open@E`, it returns
`RecordError::PartRecordMissing` (`crates/core/src/multipart.rs:382`, `Display` at `:734`) through `torn`, so
it keeps any doubt and sends no compensation. Under a session that moved, that session's row answers.

**Item 3, the tests.**
- `:490`, now `commit.rs:503`: `g_every_commit_round_out_of_flight_compensates_and_reports_the_release`
  (test file `:4425`).
- `:579`, now `commit.rs:596`: `b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost`
  (`:4452`).
- `:598`, now `commit.rs:620`: `g_a_session_with_no_record_answers_no_such_upload` (`:4493`).

**Item 1's tests.** `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` (`:4033`, each read gap, two
kinds of damage, then repair and land), `b_a_session_damaged_and_repaired_mid_round_settles_nothing` (`:4089`)
and `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` (`:4191`). Item 2's test:
`g_a_prior_part_record_gone_under_the_same_session_is_damage` (`:4302`).

**Item 4, `:1187`: deleted.** It could not be reached. A read that finds the session left `Open` marks every
round unable to land, and the next lap's held-slot read then empties the doubt.

My first replacement simply dropped the branch and looped on `doubt.any()`. That passed every test, but
`cargo mutants` then showed four timeouts, because a loop that sends nothing and depends only on that logic
can spin forever when the logic breaks. So `commit.rs:1226` is now `if send && doubt.any()`, which ends the
read-only lap by its structure, with no unreachable arm. The rerun has no timeouts.

## Judgement calls for you

1. **No session record counts as moved.** The carry-forward's sentence reads "only a record that decodes and
   is not `Open`, or is at a higher epoch". Taken alone, that would make an absent record settle nothing. I kept
   absent as moved because item 3 calls it a row ("session gone"), `upload_part_answer(None)` already answers
   `NoSuchUpload`, and only a terminal delete removes the record. If you meant absent to keep the doubt too, it
   is a two-line change at `commit.rs:620`, and the row's test would change with it.
2. **A new record error for an `Open` record whose epoch has not risen.**
   `RecordError::SessionEpochNotRisen` (`multipart.rs:389`). The carry-forward says only a higher epoch counts
   as moved, which leaves same-or-lower-epoch `Open` bytes as damage. `judge_session` used to call any other
   `Open` record "newer", so it would compensate under those bytes. Leg:
   `g_an_open_session_whose_epoch_has_not_risen_is_damage_not_a_rollback` (`:4240`).
3. **A missing empty prior is damage too.** The carry-forward names the non-empty case. The empty one reached
   the same wrong answer at the same line, so I fixed both, and the leg runs both.
4. **A missing prior keeps `may_still_commit: true`.** The commit's `require` fails while the record is gone,
   but restoring it lets the batch land, and the leg shows that. This matches the choice attempt 3 made for a
   damaged `part:` record.

Neither new `RecordError` variant is persisted, so no doc or wire change follows. Outside `wyrd-core`, the type's
name appears only in two doc comments, and the workspace builds with the new variants. The sign-off's other decisions stand unchanged:
held slots after a rollback, the dispatch-only lease promise, `Refused(OperationAborted)`.

## `custodian_gc`

Two full `cargo xtask ci` runs on this attempt did not stall, and the final one ran it in 0.17 s (10 passed). I
did not look into it again. Attempt 3's finding still stands and has no issue filed: a deadlock in third-party
code, on `main` too, with the stack `tracing_core::callsite::register_dispatch` holding `LOCKED_DISPATCHERS` while
it drops a subscriber whose `SdkMeterProvider` logs through `tracing` from `Drop`
(`iteration-v3/build-notes.md`, "Why `custodian_gc` stalled"). A stall with that stack at C4-ci is that bug and
a re-run is fair. Any other stack is not that bug. **Whether to file an issue is still your decision.**

## (E) the whole gate

`./engine/xtask.sh ci` on the final tree: **exit 0, "xtask ci: all checks passed"**. The log shows `lint_docs: OK`,
`render_site: wrote 101 page(s)`, `link audit OK`, `custodian_gc` 10 passed, `multipart_upload_part_commit` 52
passed, `advisories ok, bans ok, licenses ok, sources ok`, the all-features advisories, `xtask conformance: 5
valid + 6 invalid vectors pass`, and no failed test binary among the 223 `test result: ok` lines. No source
changed after that run; the temporary edits since were restored and checked by SHA-256.

`patch.diff` matches the tree byte for byte, and `git apply --check` passes against `376b106` in a scratch index.
`cargo fmt --all -- --check` and `typos` pass. No pre-commit hook is installed in the checkout's hooks directory,
so `cargo xtask ci`'s fmt and prose steps are the commit-readiness check.

## Self-refutation

**(a) Genuine red? Yes.** The module does not exist on the base, so the file cannot compile there, and C4-verify
will report UNVERIFIABLE (exit 77), the pre-declared sign-off item. What binds is the table below: **66 runs of a
temporary edit against a named leg, all 66 red by assertion or panic on one run over the final tree.** Three
edits first failed to compile (`commit_stopped` and `required` left unused, and a `mut` left unneeded under the
repo's deny-warnings). I corrected those edits before this final run, so none of the 66 is a compile error.

**(b) Production path? Yes.** Every leg calls `commit_part` or `upload_part` over
`RedbMetadataStore::in_memory()`, with sessions from the real Create, parts from the real `stage_part`, and real
Aborts. The wrapper changes answers and timing only and passes applied batches to the real store. Three things
stand in for code that does not exist yet, as the brief asks: `roll_back` for a Complete (#658), and test-local
teardown and idle-fence batches for #659 and #625.

**(c) Fixture includes the fault? Yes.** The damage is real bytes in the store, planted by hook at the moment it
matters. The missing part is a real delete, the rewritten slot is real bytes under the attempt's own id, and the
erased session is a real delete. The held batches are real and are landed against the real store, which is how
the legs show the repair lets them apply.

## Demonstrated reds

Each row: the temporary edit, the leg, the first failure. Each edit was made in the tree, the leg was run with
`cargo test -p wyrd-core --test multipart_upload_part_commit <leg> -- --exact`, and the files were restored.
Rows are in the brief's order, then the earlier carry-forward's ("it2-"), then this one's ("it3-").

| Edit | Leg | First failure |
|---|---|---|
| compensating on CommitUnknownResult without re-reading | `b_a_landed_commit_answered_unknown_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| after an unknown outcome, a part being there taken as Committed | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | assertion `left == right` failed: it committed again |
| part bytes alone taken as proof of the attempt's own commit | `c1_an_empty_part_is_never_committed_on_its_bytes` | assertion `left == right` failed: a Committed attempt holds no slot |
| the session's state taken from a read before the slot read, never re-checked | `c2_a_torn_read_never_answers_committed` | assertion `left == right` failed: Ok(Committed { part_number: PartNumber(1), digest: Digest([14, 217, 101, 146 |
| the possibly-landed commit dropped from its doubt before classification | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| the rolled-back Open@E' sent to the reaper instead of compensated | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| an unknown compensation outcome treated as not landed with may_still_commit: true | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: Compensati |
| a commit-loop counter that never reaches its bound | `c_every_loop_ends_after_exactly_its_bound` | the commit never stopped committing (201 batches) |
| a compensation-loop counter that never reaches its bound | `c_every_loop_ends_after_exactly_its_bound` | the commit never stopped committing (201 batches) |
| my slot is gone read as the slot key is absent (ignoring attempt_id) | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("…1" |
| a compensation sent while the attempt's commit is held and unsettled | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: SlotLost { |
| the rival-record branch checked before the own-commit proof | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| an empty compensation or re-upload that still writes a {chunks} obligation | `c4_empty_parts_never_install_an_empty_obligation` | every obligation decodes against its own key: RetireObligationOwesNothing { component: "payload" } |
| the same-part loser answering a refusal other than OperationAborted | `b_a_same_part_loser_compensates_its_own_chunks` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| the rollback loser answering a refusal other than OperationAborted | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| the lease coverage check removed | `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates` | a part commit was sent with 3002 ms left on its lease |
| the lease coverage check removed | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | a part commit was sent with 4999 ms left on its lease |
| the lease coverage check off by one, too strict | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| the lease coverage check off by one, too loose | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | a part commit was sent with 4999 ms left on its lease |
| a commit round allowed while a commit is in doubt and the lease is short | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | a part commit was sent with 4999 ms left on its lease |
| a compensation sent while a commit is in doubt and the lease is short | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: LeaseShort |
| a lease past expiry taken as proof that a held commit did not land | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` | assertion `left == right` failed: run ii |
| a moved session seen after the last slot read counted as settling the commit | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: LandedThenRollback at gap 3 |
| a rival seen after the last slot read counted as settling the commit | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| settling never allowed after a precondition dies | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("…1" |
| a settled commit stop that returns without compensating | `g_settled_stops_never_fill_the_slot_cap` | assertion `left == right` failed: multipart part commit: upload …1: all 4 part c |
| a settled renewal stop that returns without compensating | `g_a_settled_renewal_stop_compensates_and_keeps_its_error` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| a settled renewal stop that returns without compensating | `g_settled_stops_never_fill_the_slot_cap` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| a compensation after SlotLost | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | expected a renewal error, got Err(Stopped { upload_id: UploadId("…1"), cause: Sl |
| a compensation after a renewal InDoubt | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| the over-limit check in the commit removed | `d_a_part_past_the_per_part_bound_is_refused_and_compensated` | assertion `left == right` failed: Ok(Committed { part_number: PartNumber(1), digest: Digest([150, 52, 238, 16, |
| the session check after an unknown compensation made always true | `h_a_held_compensation_under_a_teardown_is_in_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| the obligation re-read skipped | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("…1" |
| the obligation byte comparison made always true | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | expected Stopped, got Ok(Refused(OperationAborted)) |
| COMPENSATION_ROUNDS lowered by one | `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| UploadPartError::source returning None | `h_a_composed_error_keeps_its_class_and_its_message` | assertion `left == right` failed: multipart part commit: upload …1 is in doubt:  |
| it2-1: a damaged part record read as a rival | `g_a_damaged_part_record_at_the_re_read_is_a_record_stop_not_a_rival` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it2-1: a damaged part record read as a rival | `b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| it2-1: damage counted as settling a held commit | `b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: Record(Mal |
| it2-2: can_recur forced to false | `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| it2-2: the ceiling's exclusion of the part's own prior disabled | `d_the_cumulative_ceiling_leaves_out_the_parts_own_prior` | assertion `left == right` failed: Ok(Refused(EntityTooLarge { chunks: 80897, limit: 80896 })) |
| it2-2: require_absent on the re-upload's obligation key removed | `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` | expected Stopped, got Ok(Committed { part_number: PartNumber(1), digest: Digest([159, 16, 142, 203, 132, 37, 1 |
| it2-2: the doubt search replaced with .last() | `b_the_kept_doubt_is_the_round_that_may_still_land` | round 1 may still land |
| it2-2: the epoch chase keeping only the latest obligation | `h_the_epoch_chase_re_reads_every_obligation_it_sent` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("…1" |
| it2-3: the commit's slot-gone, session-fenced arm answering SlotLost | `g_a_slot_gone_under_a_moved_session_at_the_commits_re_read` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| it2-3: the commit's slot-gone, newer-Open arm answering a refusal | `g_a_slot_gone_under_a_moved_session_at_the_commits_re_read` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it2-3: refuse() answering its own refusal under a fenced session | `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read` | assertion `left == right` failed: Ok(Refused(OperationAborted)) |
| it2-3: the compensation's slot-gone, session-fenced arm answering SlotLost | `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| the session the commit requires read before the quiesce renewal adopted a rollback | `b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch` | assertion `left == right` failed: Ok(Refused(OperationAborted)) |
| it3-1: session_seen counting any other bytes as moved (the finding) | `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: Record(Mal |
| it3-1: session_seen counting any other bytes as moved (the finding) | `b_a_session_damaged_and_repaired_mid_round_settles_nothing` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| it3-1: session_seen counting any other bytes as moved (the finding) | `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` | torn, 64 bytes: damage settles nothing |
| it3-1: an Open record with an unrisen epoch counted as a rollback | `g_an_open_session_whose_epoch_has_not_risen_is_damage_not_a_rollback` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it3-1: an Open record with an unrisen epoch counted as a rollback | `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| it3-1: an Open record with an unrisen epoch counted as a rollback | `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| it3-1: the same epoch under other bytes counted as a rollback | `g_an_open_session_whose_epoch_has_not_risen_is_damage_not_a_rollback` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it3-2: the missing-record check removed | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it3-2: a missing prior counted as settling a held commit | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | expected InDoubt, got Err(Stopped { upload_id: UploadId("…1"), cause: Record(Par |
| it3-2: a missing prior called damage under a moved session too | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| it3-2: a missing empty prior read as a rival | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | expected Stopped, got Ok(Refused(OperationAborted)) |
| it3-3: an out-of-flight round kept as may still land (:490) | `g_every_commit_round_out_of_flight_compensates_and_reports_the_release` | expected Stopped, got Err(InDoubt(InDoubt { upload_id: UploadId("…1"), batch: Co |
| it3-3: a slot under the attempt's own id read as gone (:579) | `b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost` | expected InDoubt, got Ok(Committed { part_number: PartNumber(1), digest: Digest([45, 71, 142, 148, 103, 155, 1 |
| it3-3: no session record called damage (:598) | `g_a_session_with_no_record_answers_no_such_upload` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("…1"), cause |
| it3-3: no session record answered another refusal (:598) | `g_a_session_with_no_record_answers_no_such_upload` | assertion `left == right` failed: Ok(Refused(OperationAborted)) |
| it3-4: the compensation's second slot read skipped (the path left where :1187 was) | `g_a_session_with_no_record_answers_no_such_upload` | expected InDoubt, got Ok(Refused(NoSuchUpload)) |
| it3-4: a second lap of reads with nothing in doubt | `g_a_session_with_no_record_answers_no_such_upload` | assertion `left == right` failed |

Notes on specific rows:
- "A lease past expiry" clears the doubt only at a held-slot read, so run (i) still passes and the leg fails in the
  runs where a compensation appears, as the brief describes.
- "A compensation sent while the commit is held" runs against (H)'s leg, as the brief directs.
- "A rival seen after the last slot read" adds a part re-read after the slot read to make that history reachable.
  I ran it again alone to read the whole message: it fails at `committed_as` in the `LandedThenRival` arm, with
  `SlotLost` where `Committed` is required.
- The brief's "`EntryLost`" row cannot come out of `StagedAttempt::renew`; the unit test
  `a_stopped_renewal_compensates_only_on_the_rows_that_say_so` in `commit.rs` holds it.

## Slot bytes, by invariant rather than by red

Both builders take the slot bytes from the attempt's own state: `commit.rs:681` and `:710`, through
`StagedAttempt::slot_bytes`. The wrapper checks every batch of every leg at test file `:577`, and the dispatch
margin on every commit batch at `:606`.

## C5 mutants

`scripts/mutants-in-diff`, on this patch and final tree: **83 tested, 68 caught, 15 unviable, 0 missed, 0
timeouts.** There are no survivors, so attempt 1's six answer-changing survivors stay dead, and its two `lossy` and
one `Display` survivors are killed by the message assertions.

The mutant the brief lists as a survivor, a builder that re-reads the slot bytes from the key, is not generated by
cargo-mutants. By hand it is unreachable under the classification, for the brief's reason: a conforming attempt
recognises its own commit in C3′ and returns before any later batch is built, and no conforming path sends a batch
after another attempt holds the index.

The 15 unviable mutants are `Default::default()` replacements for return types with no `Default`
(`crates/core/src/multipart.rs:3061`, `:3085`, `:4135`; `commit.rs:433`, `:474`, `:544`, `:564`, `:589`, `:619`, `:756`,
`:1039`, `:1063`, `:1081`, `:1108`, `:1254`). They do not compile, so they say nothing about the tests.

## Rubric self-review

- **One clock:** no new clock read in this attempt. The lease check and the part's stamp still read the caller's
  `StageClock`, and the module doc states the owner.
- **Metadata validation boundaries:** items 1 and 2 are this rule at a re-read. Damage is an error, never a value, and
  never evidence of a moved session.
- **Serialization identity:** no record shape changed. `assert_values_decode` checks every batch.
- **Absent or unsupported entries:** an absent part under the live session is now an explicit error, not a silent
  reading as a rival. An absent session is the explicit `NoSuchUpload` row.
- **Transactions:** an unknown outcome is never read as a conflict, and damage no longer drops a doubt.
- **Await discipline:** every loop is bounded and counted against a number written in the test. The read-only lap is
  now bounded by its structure (item 4). Each store call is bounded by the store's contract. No helper tasks.
- **Test fidelity:** seeded Tier-0 DST coverage is deferred to #660 with the in-code `// deferred: #660` marker
  (`commit.rs`, after the module doc), as the brief puts it out of scope.
- **Docs currency:** the architecture paragraph (`docs/design/architecture/05-building-block-view.md:248`) is in the
  diff and unchanged from attempt 3. This attempt adds no port, operation, flag or persisted field.
- **`#![forbid(unsafe_code)]`:** no new crate root; the test file carries it.

## For sign-off

1. C4-verify will report UNVERIFIABLE (exit 77), pre-declared by the brief.
2. The four judgement calls above. Number 1 is the one where I read past the carry-forward's literal wording.
3. The `custodian_gc` deadlock still needs your decision on an issue. It is on `main` and I have not filed anything.
4. The 0016 wording on the lease, and on held slots after a rollback, are still yours to raise there. I did not edit
   0016.
