## Summary
**User impact:** Storage taken by multipart uploads that are aborted, partly
unused, re-uploaded or later replaced is never given back. The cluster keeps
those old bytes on disk for good, so space use only grows. Nothing in a released
build creates multipart sessions yet, so no deployed cluster sees this today, but
it would as soon as multipart uploads are switched on.

This PR makes the custodian (the background service that repairs and cleans up
storage) free that space safely: it marks the old data as garbage, waits the
normal grace period, and lets garbage collection delete it.

**Depends on #1066** (the record half of the same cleanup); please merge that first.

## What to look at
The new cleanup step lives in one new module of the custodian. It is called from
the existing reconcile pass, the same entry point the server's background loop
already uses, so there is no new task or setting. The main thing to check is the
order of operations: a fragment is always marked as garbage before the record
that pointed to it is deleted, and the step stops and reports to the audit log,
rather than guessing, whenever it cannot tell that going on is safe.

To try it: `cargo test -p wyrd-custodian --test multipart_retirement_marks`
drives real aborts, Completes and re-uploads against an in-memory store and a
fleet double, then runs the reconcile and garbage-collection passes until the
space is freed. `cargo xtask ci` also runs the seeded race test against garbage
collection.

## Root cause
Every multipart path that drops bytes (Abort, a Complete's unused parts, a part
re-upload, a superseded generation) installs a `retire:bytes:` obligation, and
GC keeps every fragment such an obligation names. #1066 drains the
`retire:records:` obligations only, so nothing ever marked these bytes as orphans,
and they stayed protected with their records for ever.

## Fix
- **New `retire:bytes:` mode** of the retirement leg
  (`crates/custodian/src/retirement/marks.rs`), covering every row shape:
  `{session}`, `{session, all}`, `{session, parts}`, `{parts}`, `{chunks}`, and
  `{generation}` both flat and segmented (a segmented one marks each `seg:`
  record's current placement, read at drain time). It walks in bounded pages with
  a resumable cursor, so a part or generation larger than one batch still makes
  progress every step.
- **Mark guard:** a mark that decodes as `reclaiming` is never replaced (arm 0),
  since GC deletes that key blind after destroying the fragment. Otherwise the
  three arms: same event is skipped, a different event is re-stamped under an
  exact-value precondition, and an absent mark is written under
  `require_absent`. If a re-stamp loses its precondition, the re-read treats a
  new `reclaiming` value as arm 0, never as a retry.
- **Stamp time:** the clock is read per step, after the obligation is read
  (`crates/custodian/src/retirement.rs:428`), not the pass's cached `now_millis`.
- **Typed, audited holds** (obligation and records left byte-identical):
  untrusted placement, unreadable record, unreadable mark, a session that still
  owns `sidx:` entries, and a `reclaiming` mark over a record the drain keeps.
  The last one never clears without an operator, and the docs say so.
- **Segments before parts:** for one session token, no `part:` record is deleted
  while the token's `retire:records:` obligation still owes `{seg}`. This keeps
  restore's check accurate between passes, and replaces the `deferred` note in
  `crates/custodian/src/restore.rs` with what the drain now guarantees.
- **GC sweep:** a drain mark over a position with no fragment is removed once its
  `retire:bytes:` obligation is gone (`crates/custodian/src/gc.rs:918`), so every
  mark the drain writes has a deleter. While the obligation exists the mark stays.
- **Docs:** `docs/design/architecture/06-runtime-view.md` (GC paragraphs) and
  `05-building-block-view.md` describe the new order, the grace start and the holds.

**Known limitation (accepted, to be tracked as a follow-up):** at a `reclaiming`
mark over a position no kept record names, the drain waits for GC with no time
limit (`marks.rs:396-399`, `:494`). If that fragment's server leaves the fleet
for good, GC never finishes the reclaim, so the obligation waits for ever. That
wait is counted and logged at debug level only, not audited.

## Verification
Line numbers are on this branch (main with #1066 applied).

- **Claim:** marks are written before the record that named them is deleted, and
  no mark ever names a chunk of the published object.
  **Checked:** `marks.rs:435` (deletes only once `evidenced && may_delete`).
  **Test:** `b_a_subset_complete_marks_the_unnamed_parts_before_deleting_their_records`
  (`multipart_retirement_marks.rs:855`); a recording store wrapper shows the batch order.
- **Claim:** each mark is stamped no earlier than the retirement it evidences, so
  grace runs in full.
  **Checked:** `retirement.rs:428`.
  **Test:** `t_marks_are_stamped_no_earlier_than_the_retirement_they_evidence` (`:925`)
  installs an obligation mid-pass after moving a test clock past grace. It fails
  with the cached pass time ("stamped 10000000, before its retirement at 10120000").
- **Claim:** a `reclaiming` mark is never overwritten, and the drain finishes
  once real GC passes delete it.
  **Checked:** `marks.rs:261` (arm 0), `:494` (wait).
  **Test:** `c_a_reclaiming_mark_is_never_replaced_and_the_drain_resumes_once_gc_deletes_it`
  (`:1034`), `c_the_three_arms_skip_restamp_and_write` (`:1174`), and the seeded race
  rows `retirement_restamp_races_gc_swap` / `retirement_restamp_reaches_both_winners`
  (`crates/dst/tests/custodian.rs:7117-7127`), which reach both winners.
- **Claim:** a `reclaiming` mark over a kept record is a stable, audited hold; no
  pass errors and later obligations still drain.
  **Checked:** `marks.rs:389`.
  **Test:** `h_a_reclaiming_mark_over_a_retained_record_is_an_audited_hold_and_nothing_moves` (`:1308`).
- **Claim:** every shape drains to empty and its fragments are freed after grace.
  **Test:** `d_a_reuploaded_parts_chunks_drain_to_completion` (`:1466`),
  `d_a_superseded_flat_generation_drains_to_completion` (`:1501`),
  `d_an_aborted_sessions_parts_drain_to_completion` (`:1554`),
  `s_a_part_larger_than_a_batch_advances_inside_the_entry_and_finishes` (`:1831`),
  `s_a_segmented_generation_marks_current_placements_and_drains_to_completion` (`:1883`),
  and the four `paging_*` tests (`:2407-2533`) for runs longer than one page.
- **Claim:** untrusted placements and unreadable marks are held, never marked
  from or deleted.
  **Checked:** `marks.rs:707`, `:402`.
  **Test:** `e_an_untrusted_placement_is_never_marked_from_or_deleted` (`:1595`),
  the three `e_an_untrusted_chunk_in_*` tests (`:1731-1749`),
  `c_a_mark_of_none_of_the_three_shapes_is_held_and_never_restamped` (`:1256`).
- **Claim:** a session obligation waits while the session still owns `sidx:` entries.
  **Checked:** `marks.rs:215-217`.
  **Test:** `r_a_session_obligation_waits_for_its_owned_residue` (`:1785`).
- **Claim:** restore stays accurate between drain steps.
  **Checked:** `marks.rs:187` (segments before parts), `restore.rs:1157-1164`.
  **Test:** `g_restore_stays_accurate_between_drain_steps` (`:2021`) runs the
  production restore check after every step.
- **Red/green:** all 26 tests in the new file fail on the #1066 base and pass with
  this change. Each leg was also shown red by undoing just its own rule (stamp
  time, arm 0, placement hold, residue hold, segments-first, mark-before-delete,
  `{chunks}` row). `cargo xtask ci` passes (2,486 tests, including both seeded
  race rows, typos and the docs build).

Fixes #1062
