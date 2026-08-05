# PR description

## Summary
**User impact:** After a restore, running `wyrd custodian
--reconcile-after-restore` could silently produce **no report at all** the
moment a single object's chunk map could not be read — not even the counts
for the thousands of other, healthy objects it could otherwise read — so an
operator had nothing to act on. Separately, an operator watching a drain /
decommission stall over the same kind of damaged object saw only a plain
"not converged yet" answer, indistinguishable from a server that genuinely
still holds live data, with no way to learn which record was blocking it
and no way for the stall to ever end.

This change makes both surfaces contain the damaged object instead of
erroring out or going quiet: they keep reporting everything they *can*
read, name the object(s) they could not, and refuse to call the run clean
or the drain satisfied until that record is repaired. The command's exit
code now reflects that directly, so a restore script can no longer read an
incomplete run as healthy.

## What to look at
- `wyrd custodian --reconcile-after-restore` (`crates/server/src/cli.rs`,
  `restore_verdict`): the printed summary and exit code for a store
  containing one unreadable committed object alongside healthy ones.
- The restore pass itself (`crates/custodian/src/restore.rs`,
  `reconcile_after_restore`): it now returns a report naming the
  unreadable object(s) instead of erroring, and never marks a fragment
  collectable while any reading of the committed namespace found a hole.
- The drain status (`crates/custodian/src/desired_state.rs`,
  `reconciliation_status`): the new `PendingUnresolvable { objects }`
  answer, which names the blocking record instead of a bare `Pending`.

## Root cause
The restore report re-read every committed record itself and propagated
the first decode/segmented-map error it hit, so one damaged object ended
the whole pass. The drain status folded "reference set incomplete" into
the same `Pending` answer used for a genuinely unconverged server, so the
blocker was never named in the response.

## Fix
`reconcile_after_restore`'s report half now reads the committed namespace
through the same resolver GC and scrub already use, containing a
per-object decode/resolve failure instead of failing the whole pass;
it names every unresolvable object on `RestoreReport::unresolvable` and
refuses to call the run clean. Marking is authorized only when both of the
pass's two readings of the committed namespace agree the set is complete,
so a record that changes between them can never license an unsafe mark.
`reconciliation_status` gains `ReconciliationStatus::PendingUnresolvable`,
naming the blocking objects the same way `PendingMalformed` already names
malformed chunk ids. The CLI's exit code is derived directly from the
report's own `needs_human()` predicate rather than a second, hand-kept
condition, so the printed verdict and the exit status cannot drift apart.

## Verification
- **Claim:** a committed object whose chunk map cannot be read no longer
  stops or blanks the restore report, and the run is not certified clean.
  **Checked:** `crates/custodian/src/restore.rs:220` (updated function
  docs) and `:265` (the report half reads through the shared resolver and
  contains a per-object failure instead of propagating it) on the target
  branch.
  **Test:** `crates/custodian/tests/segmented_map_restore.rs:461` (new
  discriminator, red on base / green with the fix) and
  `crates/custodian/tests/restore_reconcile.rs::every_unreadable_committed_record_is_named_and_stops_the_run_being_certified`
  — both fail pre-fix (`Err` / no report) and pass post-fix.
- **Claim:** the pass never marks a fragment while either of its two
  readings of the committed namespace found an unreadable record, so a
  record that changes mid-pass can't license an unsafe mark.
  **Checked:** `crates/custodian/src/restore.rs:283` (either read's hole
  withholds every mark), `:305` (the second read's own protections), and
  `:364` (the combined mark gate) on the target branch.
  **Test:**
  `crates/custodian/tests/restore_reconcile.rs::an_object_committed_between_the_two_readings_is_never_marked`
  plus the seeded, simulator-driven
  `crates/dst/tests/custodian.rs::restore_two_readings_never_license_a_mark`
  (`custodian.rs:2217`, 50 seeds, reaching the divergence window pinned at
  `custodian.rs:2111`) — fails pre-fix, passes post-fix.
- **Claim:** a drain stall over an unreadable committed object names the
  blocking record instead of answering a bare "not converged".
  **Checked:** `crates/custodian/src/desired_state.rs:181` (the new
  `PendingUnresolvable` branch, ranked the same as the existing
  `PendingMalformed` check) on the target branch.
  **Test:**
  `crates/custodian/tests/segmented_map_consumers.rs::one_unreadable_committed_inode_blocks_every_certifying_answer_and_reclaims_nothing`
  — fails pre-fix (asserted a bare `Pending`), passes post-fix (asserts the
  named `PendingUnresolvable`).
- **Claim:** the operator command's exit code can no longer say "healthy"
  over a run that could not fully read the store.
  **Checked:** `crates/server/src/cli.rs:1196` (the restore command's exit
  path) and `:1256` (`restore_verdict`, exit status derived from
  `RestoreReport::needs_human()` rather than re-computed) on the target
  branch.
  **Test:**
  `crates/server/src/cli.rs::tests::restore_needs_human_agrees_with_every_paragraph_it_prints`
  — pins the exit status to each finding individually so a future field
  can't be added to the report without also reaching the exit code.

Fixes #651
