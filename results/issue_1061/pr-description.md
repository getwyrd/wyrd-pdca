## Summary
**User impact:** every multipart upload that completes, or whose completion is
rolled back, leaves bookkeeping records behind in the metadata store, and
nothing ever removes them. The store grows with every upload and never shrinks
back. Clients cannot start multipart uploads on `main` yet (that arrives with
#508), so no live deployment hits this today. This change lands the cleanup
before that door opens.

This PR makes the custodian (the background service that repairs and cleans up
the store) remove those leftover records in small, bounded batches on every
pass, so they go away on their own without slowing anything else down.

This is the first of two parts. It removes the leftover *records* only. The
second part (#1062) handles the leftover *data bytes*, whose cleanup entries
this change leaves exactly as they are.

## What to look at
- **The new cleanup step**: `crates/custodian/src/retirement.rs` (new file).
  It walks the list of pending cleanups, takes one small step on each, saves
  how far it got, and moves on. The step is cheap, it resumes on the next pass,
  and one broken entry cannot stop the rest.
- **Where it is wired in**: the custodian's regular repair pass runs it after
  reconstruction.
- **Saved progress**: each cleanup entry now carries an optional "how far
  along" marker. Entries without one are byte-for-byte unchanged.
- **To try it**: `cargo test -p wyrd-custodian --test multipart_retirement_drain`
  completes real uploads against an in-memory store, runs the custodian's
  normal pass, and checks the records are gone and nothing else changed. The
  file also covers a ledger far larger than one pass, a store that returns
  short pages, entries that cannot be processed, and a restore that happens
  half-way through a cleanup.

## Root cause
Completing a multipart upload, or rolling back a completion attempt, installs
`retire:records:` obligations in the `retire:` ledger, naming the published
parts' `part:`/`psum:` records or the attempt's `seg:` records. No code on
`main` drained them; only restore's re-check ever read them.

## Fix
- **Dispatch.** `reconcile_step` runs a new retirement leg on the
  reconstruction input, after reconstruction (not on GC's input, which must
  leave `retire:` alone on GC-only passes).
- **One step per visit.** Read the obligation, decode it with its cursor,
  delete the next batch, and put the obligation back with its advanced cursor,
  all in one commit guarded by `require(retire:… == prior value)`. The step
  that deletes the last record deletes the obligation instead. A lost
  precondition re-reads and re-plans. Batches stay within `MAX_BATCH_BYTES` and
  `MAX_BATCH_OPS`, preconditions counted. Records mode never writes an
  `orphan:` record.
- **`{parts}`**: per member, delete `part:<id>:<n>` and `psum:<id>:<n>`. Finding
  the next member of a sparse part set is a binary search over its runs, not a
  rescan from the first run.
- **`{seg}`**: delete the group's `seg:` records page by page. The payload names
  only the group's nonce and epoch, not where its range ends, so the component
  is finished only when a read of the whole range from its start finds it
  empty. A page the store cut short (`next` is set) is followed, never taken
  for the end.
- **`{parts, seg}`**: both components in order. Each component boundary is a
  commit boundary, and the obligation goes only with the last component's last
  record.
- **Cursor.** An optional component of `RetirePayload`, omitted when absent.
  Decode refuses, with a typed error, any cursor it can judge from the payload:
  on a component the payload lacks, past the end of a list the payload holds,
  or past `MAX_SEGMENT_INDEX`. Its widest encoding (69 bytes) is reserved: a
  Complete now refuses an obligation over `MAX_VALUE_BYTES - 69`, where #1011
  refused one over `MAX_VALUE_BYTES`.
- **Per-pass bounds.** At most 64 obligations visited, 70 store reads and 17
  commits per pass, whatever the ledger's size. Obligations that cannot
  progress still count as visits. The walk saves its position outside
  `retire:` (`custodian:retire-cursor`, the same pattern as GC's orphan-walk
  bookmark), resumes there next pass, and laps to the head at the end.
- **Holds.** An obligation that will not decode, has an unusable cursor, whose
  advanced value would cross `MAX_VALUE_BYTES`, or whose commit the store
  refuses is left byte-identical, with a typed outcome and an audit event. The
  over-size value is measured first and never sent: FoundationDB refuses it
  for good (2103), and sending it would fail the pass every time.
- **What a pass reports.** `Satisfied` only when the pass read the whole ledger
  from head to end and found no records obligation in it. A pass that resumed
  part-way, or spent its steps on lost preconditions, reports `Partial`.
- **Docs.** `06-runtime-view.md` describes the drain, the cursor, the
  short-page rule and the downgrade note; `05-building-block-view.md` no longer
  says the drain has not arrived.
- **Dev-dependency.** `wyrd-custodian` gains `wyrd-metadata-redb` as a
  dev-dependency (one `Cargo.lock` line) so its tests run over a real store.

**Downgrade.** A build from before this change cannot decode an obligation
that carries a cursor. Its GC never decodes obligations (presence alone
protects), so no bytes are at risk; its restore check reports such a value as
undecodable. Downgrading is safe for data and noisy in restore reports.

## Verification
Line numbers are on `main` with this patch applied.

- **Claim:** the deployed custodian runs the drain on every pass.
  - **Checked:** `crates/custodian/src/reconciliation.rs:161-170` calls
    `retirement::reconcile` inside the reconstruction branch of
    `reconcile_step`; its production caller is `reconcile_pass`,
    `crates/server/src/custodian.rs:397`. Every test drives the drain only
    through `reconcile_step` with the contexts that caller passes.
  - **Test:** `b_rec_a_completes_published_part_records_are_drained_and_nothing_else_is_touched`
    (`crates/custodian/tests/multipart_retirement_drain.rs:647`): after a real
    Complete, no `part:`/`psum:` of a published part remains, no `orphan:` was
    written, the object reads back byte-identical, and the unnamed parts and
    their `retire:bytes:` obligation are unchanged.
- **Claim:** obligations drain to empty in bounded batches, resume from the
  stored cursor, and are deleted only with their last record.
  - **Checked:** `parts_step` and `seg_step`, `retirement.rs:464`, `:515`.
  - **Test:** `c_a_sparse_parts_obligation_drains_to_empty_from_its_stored_cursor`
    (`:916`, 4,001+ non-adjacent parts), `c_a_seg_obligation_past_one_batch_drains_to_empty`
    (`:956`), `c_a_seg_obligation_drains_to_empty_on_a_store_that_cuts_pages_short`
    (`:988`, redb's own `with_scan_cap`), `c_a_combined_obligation_crosses_components_at_a_commit_boundary`
    (`:1027`, resumed from fresh contexts exactly at the boundary).
- **Claim:** a pass's work is bounded independently of the ledger, and
  obligations that cannot progress do not starve the rest.
  - **Checked:** bounds at `retirement.rs:113-138`; pinned to literal values by
    `the_built_bounds_are_the_promised_ones` (`:1085`).
  - **Test:** `l1_a_productive_backlog_drains_in_key_order_within_fixed_per_pass_bounds`
    (`:1172`), `l1_a_seg_backlog_reads_within_fixed_per_pass_bounds` (`:1222`),
    `l1_short_ledger_pages_make_the_page_bound_end_a_pass` (`:1270`),
    `l2_a_held_prefix_neither_breaks_the_bounds_nor_starves_what_follows`
    (`:1296`). Each pass is checked against reads, commits and visits counted
    by a recording store wrapper.
- **Claim:** an advanced value past `MAX_VALUE_BYTES` is never sent, holds only
  its own obligation, and a Complete leaves room for the cursor.
  - **Checked:** `retirement.rs:421-427` (measured before the commit);
    `RETIRE_CURSOR_MAX_BYTES` and `RETIRE_VALUE_CEILING`,
    `crates/core/src/multipart.rs:4065`, `:4079`; the Complete check,
    `crates/core/src/multipart/complete.rs:1196-1203`.
  - **Test:** `w_an_obligation_whose_cursor_cannot_fit_is_held_and_the_next_one_drains`
    (`multipart_retirement_drain.rs:1482`), under a wrapper that refuses any put
    over `MAX_VALUE_BYTES` as FoundationDB does: no pass returns `Err`, the
    later obligation drains, and one landing exactly at the ceiling still
    drains. Complete's tighter ceiling:
    `crates/core/tests/multipart_complete_value_ceiling.rs`.
- **Claim:** a pass reports `Satisfied` only when it saw the whole ledger owing
  nothing.
  - **Checked:** `retirement.rs:293-302`.
  - **Test:** `s_a_pass_that_resumed_part_way_does_not_certify_the_ledger`
    (`:1372`), `s_a_pass_whose_every_step_lost_its_precondition_does_not_certify`
    (`:1412`, a real second writer moving the obligation before each commit).
- **Claim:** restore never flags a session whose segment cleanup is mid-way.
  - **Test:** `g_rec_a_restore_fenced_attempt_is_drained_with_restore_naming_nothing_between_steps`
    (`:1604`): an attempt fenced by the production restore fence is drained one
    step at a time, and the production `reconcile_after_restore` names nothing
    between steps.
- **Claim:** the cursor codec is backward compatible and refuses what it can
  judge.
  - **Checked:** `RetireCursor` and `decode_retire_obligation`,
    `crates/core/src/multipart.rs:3999`, `:4540`.
  - **Test:** `crates/core/tests/multipart_retire_cursor.rs` (no-cursor bytes
    unchanged, exact round trip, typed refusals, reserved room = widest
    accepted cursor). The `{seg}` cursor past the last record is a hold, not a
    finish: `k_a_seg_cursor_past_the_last_record_with_records_below_it_is_held`
    (`multipart_retirement_drain.rs:1686`).
- **Red before the fix.** The new test file cannot compile on `main` (no
  `retirement` module, no redb dev-dependency), so there is no plain
  before/after run. Instead, each guarantee was broken on purpose and its test
  shown to fail, then the code restored: dispatch made unreachable, cursor
  ignored, obligation deleted after its first batch or its first component,
  `{seg}` finished without the whole-range read or on a short page, step and
  page counters not decremented, held obligations not counted, walk always
  restarted at the head, the ceiling check removed or off by one, and a
  resumed pass treated as having seen the whole ledger. All 18 broke their
  test.
- **Gate.** `cargo xtask ci` passes (fmt, clippy, workspace tests, deny,
  conformance, docs, DST sweep). 93% of the changed lines that can be
  instrumented run under the new tests.

**Known gaps.**
- The part set's next-member lookup (`crates/core/src/multipart.rs:3517-3518`)
  is checked for correct results only (`members_from_starts_at_the_first_member_at_or_after_its_position`,
  `crates/core/tests/multipart_retire_cursor.rs:495`). No test would fail if it
  went back to a linear scan from the first run; a timing-based test is left
  as follow-up.
- Store calls in the drain have no timeout of their own. That follows the
  store interface's contract (`crates/traits/src/lib.rs:1333-1342`: every
  backend bounds its own waiting) and matches GC, reconstruction and scrub.

Fixes #1061
