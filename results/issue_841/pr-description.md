## Summary
**User impact:** after an operator restores the metadata store from an older image, an
upload that was cancelled after that image was taken can come back to life as "open". Its
data may already have been cleaned up, but nothing stops a retried "finish upload" from
publishing an object that points at data that no longer exists. No client can create
multipart uploads yet, so nobody hits this today; this closes the hole before they can.

This PR makes the post-restore check close every upload the restored image still holds
open, record who owes the cleanup of its data, and name any upload it could not close so
an operator can deal with it.

This PR is stacked on #846 and #847 (the two earlier parts of the same restore work); its
diff reads cleanly once those land.

## What to look at
- The new step at the end of the post-restore pass that closes open uploads, and the one
  write that both closes an upload and records its cleanup, so neither can happen without
  the other.
- What happens when that write can't go through: the upload changed in the meantime, the
  cleanup record already exists, or the store fails. Each case is named, never overwritten
  or retried blind.
- To try it: seed an open upload session with one committed part, run the post-restore
  pass, and check that the session is now aborting and a cleanup record exists for it.
  `crates/custodian/tests/restore_open_fence.rs` does exactly this, plus the failure cases.

## Root cause
`reconcile_after_restore` read upload session records only by key, to protect their staged
data, and never decoded or wrote them. So a session the restored image held `Open` stayed
`Open`, with no fence and no retire obligation owing its parts (0016 D-B, decision 1.4).

## Fix
- **The fence** (`crates/custodian/src/restore.rs`): after Pass 3, `fence_open_sessions`
  re-lists `mpu:` in bounded pages and calls `fence_session` on each. For an `Open@E`
  session it commits one `WriteBatch`: `require(mpu:<id>, bytes as read)`,
  `require_absent(retire:bytes:s:<id>:<E>)`, put the session as `Aborting@E+1`, and put the
  obligation `{session, all}`.
  - `Committed` → counted in `sessions_fenced`, audited `session-fenced`.
  - `Conflict` → the session and obligation key are re-read once, and the session is named
    `ChangedUnderPass`, `ObligationKeyTaken` or `LostConflict`. No retry.
  - `Err` (including `CommitUnknownResult`) → audited and returned; never read as a
    conflict.
- **What it can't fence is named**: an undecodable value, `Open` at `u64::MAX`, and
  `Completing` (its fence also has to retire `seg:` records; that's #842). These are left
  byte-identical and go in `sessions_unsettled`, which `needs_human()` counts.
- **Verdicts survive a fence fault**: the summary is emitted before the error is returned,
  reads INCOMPLETE, and has `clean=false`, `needs_human=true`.
- **Writer API in `crates/core/src/multipart.rs`**: `SessionRecord::open_teardown` returns
  an `OpenTeardown` (session + `RetireObligation`). `RetireObligation` has private fields
  and one private constructor that builds key and payload together, so they can't disagree.
  The "no writer-side constructor" notes and the retire rows table are updated.
- **CLI** (`crates/server/src/cli.rs`): `restore_verdict` counts fenced sessions and prints
  a NEEDS-HUMAN paragraph for unfenced ones through `named_records` (unchanged bound: first
  20 by name, the rest as a count, per cause).
- **Docs**: a fence paragraph in `06-runtime-view.md` §6.5, and step 7 of the m4
  blueprint, including the old "never decodes an `mpu:` value" claim.
- `gc.rs`: only `staged_page` widened to `pub(crate)`.

## Verification
Line numbers are on this branch (main + #846 + #847 + this patch).

- **Claim:** an `Open` session is fenced whole, in one commit with an obligation owing
  every part.
  - **Checked:** `crates/custodian/src/restore.rs:738-790` (`fence_session`),
    `crates/core/src/multipart.rs:2264` (`open_teardown`), `:3584-3620`
    (`RetireObligation`).
  - **Test:** `an_open_session_is_fenced_whole` (`restore_open_fence.rs:489`). The
    obligation is decoded through `decode_retire_obligation` against its key.
- **Claim:** the fence lands whole or not at all, and an unknown commit result is never
  treated as a conflict.
  - **Test:** `a_failed_fence_commit_leaves_neither_write` (`:532`). It fails the commit on
    the session write in one run and on the obligation write in another.
- **Claim:** it never fences blind or overwrites an obligation.
  - **Checked:** `restore.rs:754-761` (the `require` / `require_absent` preconditions),
    `:768-781` (the conflict re-read).
  - **Tests:** `a_session_that_changes_under_the_pass_is_not_fenced_blind` (`:566`) and
    `the_fence_never_overwrites_an_obligation` (`:611`).
- **Claim:** what it can't fence is left untouched and named.
  - **Test:** `what_the_pass_cannot_fence_is_named_never_passed_off_as_done` (`:687`).
- **Claim:** a second run changes nothing.
  - **Test:** `a_second_pass_is_idempotent` (`:730`). The whole store is byte-identical
    after the second run.
- **Claim:** a fence fault never hides the pass's other verdicts and is never reported
  clean.
  - **Checked:** `restore.rs:714-716` (fence after Pass 3, summary before `?`),
    `:1320` (`clean = fence_finished && report.is_clean()`).
  - **Tests:** `a_fence_fault_never_hides_the_pass_verdicts` (`:772`) and
    `a_fence_fault_on_an_otherwise_clean_store_is_never_certified_clean` (`:816`).
- **Claim:** sessions across several listing pages are all fenced.
  - **Test:** `sessions_listed_across_pages_are_all_fenced` (`:840`).
- **Claim:** `open_teardown` returns a teardown only for `Open` below `u64::MAX`, and its key
  and payload agree.
  - **Test:** `only_an_open_session_has_a_teardown_and_its_obligation_matches_its_key`
    (`multipart.rs:5084`).
- **Claim:** the CLI reports both counts and stays within its naming bound.
  - **Tests:** `restore_verdict_counts_fenced_sessions_and_names_the_ones_it_could_not_fence`
    (`cli.rs:3123`) and `restore_verdict_names_unfenced_sessions_and_counts_the_ones_it_cannot_fit`
    (`cli.rs:3172`, 21 sessions). The agreement test is extended to the new paragraph.
- **Red → green:** with the `src` changes reverted and the tests kept, all 9 tests in
  `restore_open_fence.rs` compile and fail by assertion. With the fix, all 9 pass.
- **Whole gate:** `cargo xtask ci` passes. `staged_protection` is 36/36 and
  `restore_staged_report` is 2/2. The restore DST properties in
  `crates/dst/tests/custodian.rs` are untouched and pass.
- **Existing tests changed by design:**
  - The undecodable-session leg in `staged_protection.rs` (~`:1725-1800`) keeps its
    protection checks. It now expects the session to be named once, as `session-unsettled`.
  - `restore_staged_report.rs`'s session builder gains `segment_nonce`.

**Follow-ups:**
- DST coverage of the fence is #843.
- The `Completing` fence is #842.
- One known weak spot: `restore_staged_report.rs`'s
  `an_untrusted_staged_record_is_named_needs_no_human_and_is_not_a_clean_bill` now passes
  partly because its `Open` sessions get fenced, which by itself makes `is_clean()` false.
  It should seed those sessions `Aborting`, or assert `sessions_fenced: 2`, so it pins
  `staged_untrusted` again.

Fixes #841
