## Summary
**User impact:** an operator draining a storage server (to wipe or decommission it) could
be told the drain finished while a live, in-progress upload still had bytes sitting on
that server. Wiping the box at that point loses data: the upload later commits, or
publishes, referring to fragments that are gone.

This PR makes the drain-status check count an in-progress upload's bytes as still held,
so it no longer reports a server safe to wipe while one is in flight there.

## What to look at
The function that answers "is this server's drain finished" now also looks at
in-progress-upload records, not just fully committed data. The key file is
`crates/custodian/src/desired_state.rs`, function `reconciliation_status`. To exercise it,
run the new test file `crates/custodian/tests/staged_drain_status.rs` — it seeds a server
with only in-progress upload bytes, marks it draining, and checks the answer.

The server-rebalancing pass that evacuates data off a draining server is deliberately left
alone: it only ever moves fully committed data, never an upload's own in-progress records,
so it correctly does nothing for a server holding only in-progress bytes. That is now
documented and covered by a test, rather than left implicit.

## Root cause
`reconciliation_status` computed "drain satisfied" from committed placement records only.
An upload's staged bytes (a committed part's fragments, or a still-streaming part's
in-flight fragment, with no committed record yet at all) were invisible to it, so a server
holding only those bytes was certified satisfied.

## Fix
`reconciliation_status` now reads the existing `StagedSet` class
(`crate::gc::staged_fragments`) beside the committed reference set, staged before
committed (the same order `gc::reconcile` already uses, so an in-flight publication can't
slip between the two reads unseen). A staged fragment on the server now answers `Pending`.
An unreadable staged record blocks every drain (`PendingUnresolvable`); a staged record
that decodes but carries a placement of the wrong shape blocks every drain the way an
untrustworthy committed placement already does (`PendingMalformed`), sharing one sorted,
deduplicated list of chunk ids with the committed class. The rebalance pass itself is
unchanged (it scans only the committed namespace); a doc comment on `plan_evacuations`
records that staged bytes are out of its scan by construction, and the drain-status
sentence in `docs/design/architecture/06-runtime-view.md` is corrected to match. One peer
test (`staged_protection.rs`) had its guard narrowed to scrub only, since drain status no
longer answers identically with and without upload records — that is the intended change.

## Verification
- **Claim:** a draining server holding only an in-progress upload's staged fragment (either
  a committed part's fragment or an in-flight part's own fragment) answers `Pending`, not
  `Satisfied`.
  **Checked:** `crates/custodian/src/desired_state.rs:222-227` (staged class read
  before the committed one) and `:225-240` (both counted as held) on `main`.
  **Test:** `crates/custodian/tests/staged_drain_status.rs`,
  `an_in_flight_owned_fragment_holds_the_drain` and
  `a_committed_parts_fragment_holds_the_drain` — both fail (`Satisfied`, want `Pending`)
  before this fix and pass after it.
- **Claim:** a draining server holding none of the staged bytes in the store still drains
  normally (the fix doesn't over-block).
  **Checked:** `crates/custodian/src/desired_state.rs:225-240` on `main`.
  **Test:** `staged_drain_status.rs`, `a_server_holding_none_of_the_staged_bytes_still_drains`
  (green before and after — a guard) paired with
  `every_server_that_does_carry_staged_bytes_holds_its_own_drain` (fails before, passes
  after).
- **Claim:** the rebalance pass over a draining server holding only staged bytes writes and
  rewrites nothing, while the drain status for that same server still answers `Pending`.
  **Checked:** `crates/custodian/src/rebalance.rs:231-250` (doc comment on the scan
  boundary) on `main`.
  **Test:** `staged_drain_status.rs`,
  `rebalance_leaves_staged_bytes_alone_while_the_drain_stays_pending` — the `Pending` half
  fails before this fix and passes after; a control in the same test then adds committed
  data to the same server and shows the same pass does move that.
- **Claim:** a staged record that can't be read, or can be read but not trusted, blocks
  every drain and names the record to repair, mirroring how the committed class already
  behaves.
  **Checked:** `crates/custodian/src/desired_state.rs:181-215` (both classes merged into
  one blocked/attributed answer) on `main`.
  **Test:** `staged_drain_status.rs`,
  `a_staged_record_the_query_cannot_read_blocks_every_drain` and
  `a_staged_record_the_query_cannot_trust_blocks_every_drain` — both fail before this fix
  and pass after.

Fixes #808
