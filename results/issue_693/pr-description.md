# PR description

## Summary
**User impact:** a client retrying a multipart upload's Complete call (for example
after a dropped connection) could not be reliably told whether its retry matched the
upload that already finished, versus a different one uploaded under the same id later.
Until now there was also no single, well-defined way to compute a multipart object's
ETag, so nothing could answer "does this retry match what was already published."

This change adds that answer table and the two identity values it depends on: for
every multipart verb (upload a part, complete, abort, list parts, list uploads) and
every state an upload session can be in, there is now exactly one typed answer,
including the tricky case of a retried Complete matching or not matching what was
already recorded.

## What to look at
The core of the change is `crates/core/src/multipart.rs`: the `answer` function (and
the five per-verb functions it calls) is the whole verb x state answer table in one
place, and `multipart_etag` / `complete_fingerprint` are the two digest functions a
completed upload's identity is built from. The accompanying test,
`crates/core/tests/multipart_state_machine.rs`, is the easiest way to see the intent —
it enumerates every verb x state combination and checks each one, and includes several
cases that show what goes wrong if part numbers get silently reordered instead of
rejected. Run it with `cargo test -p wyrd-core --test multipart_state_machine`.

## Root cause
The design behind this (proposal 0016, decision 3) had a normative answer table and
digest formulas, but nothing implemented them yet: earlier work landed only the record
*types*, so a verb hitting the system had no function to consult and no ETag/fingerprint
to compare against.

## Fix
Adds the outcome/refusal enums, `Verb`, the per-state answer functions, and the total
`answer` dispatcher, plus `multipart_etag` (SHA-256 over the parts' raw digests,
`<hex>-<count>`) and `complete_fingerprint` (SHA-256 over each part's number and digest,
so a different assembly under the same upload id is distinguishable from an identical
retry). Both digest functions refuse an empty, out-of-order, or duplicate part list
rather than silently sorting it. `sha2` (already used elsewhere in the workspace) is
added as a dependency of `crates/core` to compute them.

Because a completed upload's ETag now needs to be handed back verbatim on a retry (the
individual part records it was built from may already be cleaned up by then), the
persisted `Completion.etag` field changes from a bare digest to the full composed ETag.
The architecture doc is updated in the same commit to describe this, per the project's
rule that a persisted-record change updates the living doc alongside it.

## Verification
- **Claim:** every verb x state combination (25 cells, including "no session record at
  all") has exactly one typed answer, with no case falling through to a generic error.
  **Checked:** `crates/core/src/multipart.rs:4303-4316` (the `answer` dispatcher) and its
  five per-verb functions, `crates/core/src/multipart.rs:4213-4292`.
  **Test:** `crates/core/tests/multipart_state_machine.rs:339`
  (`every_decision_3_cell_is_answered_for_an_identical_retry`) and `:870`
  (`outcome_enums_are_exhaustive`) — fails to compile pre-fix (the functions and types
  don't exist yet), passes post-fix (22/22 tests green).
- **Claim:** a retried Complete against an already-completed upload is told apart from a
  different assembly under the same id, using the whole recorded ETag.
  **Checked:** `crates/core/src/multipart.rs:4239-4255` (`complete_answer`'s tombstone
  branch) and `:3895-3903` (`complete_fingerprint`).
  **Test:** `crates/core/tests/multipart_state_machine.rs:422`
  (`a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag`) and
  `:586` (`complete_fingerprint_disagrees_on_the_same_digests_under_different_numbers`).
- **Claim:** an out-of-order or duplicate part list is refused, never silently sorted or
  de-duplicated.
  **Checked:** `crates/core/src/multipart.rs:3746-3766` (`canonical_named_parts`).
  **Test:** `crates/core/tests/multipart_state_machine.rs:532`
  (`multipart_etag_refuses_a_non_ascending_duplicate_or_empty_list`).
- **Claim:** the composed ETag's part-count suffix accepts exactly `[1, MAX_PART_NUMBER]`
  and rejects anything else (zero, too high, or malformed).
  **Checked:** `crates/core/src/multipart.rs:3793-3810` (`MultipartEtag::parse`).
  **Test:** `crates/core/tests/multipart_state_machine.rs:670`
  (`multipart_etag_parse_refuses_a_count_outside_the_range`).
- **Claim:** the record change (bare digest -> composed ETag) is reflected in both the
  stored-record round-trip test and the architecture doc.
  **Checked:** `docs/design/architecture/05-building-block-view.md:204`.
  **Test:** `crates/core/tests/multipart_session_records.rs:293`
  (`session_completed_round_trips`) and `:338` (`completion_round_trips_standalone`) —
  updated to expect the composed form; both pass post-fix.

Fixes #693
