## Summary
**User impact:** a multipart upload that is aborted never fully goes away. Each one keeps
a slot in the store's limit on open uploads, so after enough aborted uploads every new
multipart upload is refused with `503`, and stays refused. An aborted upload that left a
damaged record behind also keeps all of that chunk's stored bytes, for good.

This PR lets the custodian finish ended uploads: it cleans up what an aborted (or, later,
completed) upload left behind, then deletes the upload and gives its slot back, exactly
once.

No multipart upload can be created in production yet (that arrives with #508), so nobody
hits this today. Without this change, the first deployment with multipart enabled would.

**Stacking:** this builds on the retirement drain from #1061 and #1062 and should merge
after them.

## What to look at
- **The new step in the custodian's cleanup pass.** For each upload that has left the
  open state it does three things. It walks the upload's leftover in-flight staging
  entries. It retires damaged records by chunk id, only when the custodian can see every
  configured storage server. It ends the upload in one atomic batch once nothing is left.
- **When records may be deleted.** Nothing is deleted before the bytes it points at are
  marked for reclamation. Damaged records are only retired for aborted uploads, and only
  when every configured server is reachable. A completed upload's record is kept, so a
  retried Complete still succeeds.
- **To try it:** `cargo test -p wyrd-custodian --test multipart_session_terminal` (43
  tests). Each one builds real uploads with the production Create, Abort and Complete,
  then runs the deployed reconcile step pass by pass. The server-level check is
  `cargo test -p wyrd-server --test custodian_gc
  deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`.

## Root cause
After the retirement drain empties an ended upload's obligations, nothing deletes the
upload: its `mpu:` record, `slot:` records, owned `sidx:` residue and `mpuctl` admission
count all stay. The drain also holds an untrusted staged record (a wrong-length placement,
or an `sidx:` value that will not decode), and GC protects every fragment of its chunk, so
the upload can never reach an exit.

## Fix
- **Owned `sidx:` walk** (`crates/custodian/src/retirement/sessions.rs`). For `Aborting`
  and `Completed` sessions it marks every position of each entry's staged placement, then
  deletes the entry, in budgeted batches. A per-session bookmark
  (`custodian:owned-walk:<id>`) lets a wide entry resume, and a scan position
  (`custodian:owned-scan:<id>`) moves past a page of held entries. The bookmark is only
  put while the session record is unchanged (`sessions.rs:850-854`). A step that marks
  and deletes nothing costs no step budget, so held sessions cannot starve the others.
- **Retirement by chunk id** (`crates/custodian/src/retirement.rs`,
  `retirement/marks.rs`). This applies to `Aborting` sessions only. A `part:` or `sidx:`
  record whose placement fails validation, or an `sidx:` value that will not decode (its
  chunk taken from its key, as GC does), is retired in two steps. First, every fragment of
  that chunk found in the fleet's listings is marked `orphan:`, plus every position a
  decodable entry names itself, so a late write there is covered too. Then the record is
  deleted. This runs only when `fleet.len() == operator_fleet_size`, the deployed GC gate,
  mirrored; otherwise it is a typed `partial-fleet` hold. An undecodable `part:` names no
  chunk, so it is held and audited as needing a human.
- **Terminal delete** (`sessions.rs:917-990`). This runs only when the session's `sidx:`
  range was seen empty this pass and no session-scoped `retire:` obligation is left. One
  batch requires the session record's exact bytes and the `mpuctl` ledger as read. It
  deletes the session, its `slot:` records, the walk's own records, and the `seggrp:`
  marker if no `seg:` record names the nonce. It then CASes `mpuctl` down by one.
- **Tombstone input** on the drain context (`Tombstone::{Retain, Elapsed}`,
  `retirement.rs:390-396`). A `Completed` session is deleted only under `Elapsed`. The
  deployed caller passes `Retain` (`crates/server/src/custodian.rs:580`) until #625
  measures the window. This is one switch per pass, not per session, so #625 will need to
  turn it into a per-session check before it can pass `Elapsed`.
- **Docs:** `docs/design/architecture/06-runtime-view.md:92` and
  `05-building-block-view.md` describe the terminal delete, the kept tombstone, the
  whole-fleet rule and the late-write residual.

**Known residual (by design):** a write that lands after the listing, at a position a
damaged `sidx:` entry does not name, has no mark and no record. GC keeps it for good: it
is never lost, but nothing reclaims it automatically. The alternative, holding such entries
for operator repair, would keep the whole chunk and the admission slot instead.

Out of scope here: seeded DST coverage of this leg (#660) and the `W_tombstone` clock
(#625).

## Verification
Line numbers are on this PR's branch (`main` plus #1061/#1062 plus this change).

- **Claim:** an aborted session walks its residue and is deleted once, with `mpuctl` down
  by exactly one and no `slot:` record left.
  **Checked:** `crates/custodian/src/retirement/sessions.rs:917-990` (terminal batch, both
  preconditions); dispatched from `reconcile_step` at
  `crates/custodian/src/reconciliation.rs:167-171`.
  **Test:** `d_an_aborted_session_walks_its_residue_and_ends_once` (`:1113`).
- **Claim:** two custodians racing the terminal delete release the slot once.
  **Checked:** `sessions.rs:950`, `:978` (`require` on the session record and on `mpuctl`).
  **Test:** `d_two_drainers_racing_the_terminal_delete_release_the_slot_once` (`:1398`).
- **Claim:** the `seggrp:` marker is deleted only when no segment adopted it.
  **Test:** `d_an_adopted_groups_marker_outlives_the_session` (`:1182`).
- **Claim:** the terminal delete waits for an empty `sidx:` range and no remaining
  obligation.
  **Checked:** `sessions.rs:932-940`.
  **Test:** `d_the_terminal_delete_waits_for_an_empty_owned_range` (`:1290`).
- **Claim:** a completed upload keeps its record under the deployed input, a retried
  Complete still answers `AlreadyCompleted`, and the record is deleted once under `Elapsed`.
  **Checked:** `sessions.rs:927`; `crates/server/src/custodian.rs:580`.
  **Test:** `d_a_completed_session_keeps_its_tombstone_until_the_window_passes` (`:1212`).
- **Claim:** damaged records of an aborted upload are retired by chunk id (mark, then
  delete), and their fragments are gone after GC grace.
  **Checked:** `crates/custodian/src/retirement/sessions.rs:425-440` (wrong-length and
  undecodable `sidx:`), `crates/custodian/src/retirement/marks.rs` (untrusted `part:`).
  **Tests:** `e1_…` (`:1586`), `e2_…` (`:1596`, including the late write at a named
  position), `e3_…` (`:1608`, including the stated residual).
- **Claim:** this only happens on a whole-fleet pass. A server missing at startup, or
  unreachable this pass, leaves the record and every fragment in place until it is back.
  **Checked:** `crates/custodian/src/retirement.rs:413-425`;
  `crates/server/src/custodian.rs:577`.
  **Tests:** `e_a_part_record_waits_for_a_server_left_out_at_startup` (`:1743`) and the
  three siblings (`:1749`-`:1766`); in the server crate,
  `deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`
  (`crates/server/tests/custodian_gc.rs:1380`).
- **Claim:** open uploads and committed objects are untouched, and an undecodable `part:`
  is held for a human with the session kept.
  **Tests:** `e_an_open_sessions_untrusted_records_are_kept_whole` (`:1629`),
  `e_an_undecodable_part_record_needs_a_human_and_the_session_stays` (`:1780`),
  `e_a_completed_sessions_untrusted_entries_are_held_past_its_window` (`:1827`).
- **Claim:** the walk always makes progress. Wide entries resume, held entries do not hide
  later ones, held sessions do not starve others, and a late custodian cannot recreate the
  bookmark of a deleted session.
  **Tests:** `d_sessions_past_one_page_and_one_pass_of_steps_all_end` (`:2143`),
  `d_a_trusted_entry_behind_three_held_part_way_retires` (`:2934`),
  `d_sixteen_sessions_held_past_one_step_spend_no_step` (`:3689`),
  `d_no_bookmark_is_put_once_another_drainer_has_ended_the_session` (`:3373`).

All tests listed above are in `crates/custodian/tests/multipart_session_terminal.rs` (new)
unless another file is named. This is new behaviour, so the new test file does not compile
without the change: there is no pre-fix red at the test level. Each guard was checked
instead by removing it and confirming tests go red. That covers the empty-range gate, the
tombstone check, the `mpuctl` precondition, mark-before-delete, the whole-fleet gate,
marking an entry's own positions, the bookmark's session precondition and the step-charge
rule. `cargo xtask ci` passes on the branch.

Fixes #1013
