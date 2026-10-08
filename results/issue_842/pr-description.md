## Summary
**User impact:** after an operator restores the metadata store from an older image, an
upload that was in the middle of finishing when that image was taken can come back to
life, even if it was cancelled since. Its data may already have been cleaned up, but
nothing stops it from publishing an object that points at data that no longer exists, and
the bookkeeping records its finish attempt wrote are never cleaned up. No client can
create multipart uploads yet, so nobody hits this today; this closes the hole before they
can.

This PR makes the post-restore check close those uploads too, record who owes the cleanup
of both their data and their finish-attempt records, and name any upload it could not
close cleanly so an operator can deal with it.

This PR is stacked on #846, #847 and #851 (the earlier parts of the same restore work);
its diff reads cleanly once those land.

## What to look at
- The post-restore step that already closes open uploads (#851) now also closes uploads
  that were mid-finish. It does so in one write that closes the upload and records both
  cleanups, so none of the three can happen without the others.
- **A deliberate departure from the design proposal.** The proposal says the data cleanup
  record should list the upload's part numbers. For an upload with many scattered parts,
  that list is larger than the store accepts, and FoundationDB would refuse the write on
  every run. This PR records "all parts of this upload" instead, which owes the same data
  because no part can be added once an upload starts finishing. That choice was made
  before implementation and is pinned by a test with 10,000 scattered parts.
- What happens when an upload can't be closed cleanly: a cleanup record already exists,
  its finish-attempt records are damaged, or the epoch counter is at its maximum. Each case
  is named for a human, never overwritten. A second run names the same problems again
  instead of reporting all clear.
- To try it: seed a mid-finish upload session with its nonce, two committed parts and one
  segment record, run the post-restore pass, and check that the session is now aborting
  and both cleanup records exist. `crates/custodian/tests/restore_completing_fence.rs`
  does exactly this, plus the failure cases.

## Root cause
After #851 the restore fence handled only `Open` sessions; a decodable `Completing@E`
session was named as unsettled and left as is, so it could still publish over reclaimed
bytes (0016 D-B, `0016:717-728`), and its `seg:<nonce>:<E>:*` records had no deleter
anywhere (X57, `0016:880`). The `Completing → Aborting` row 0016 specifies (`0016:665`)
uses a `{session, parts}` bytes payload that, for a sparse part set, exceeds
`MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`): 10,000 alternating part numbers
encode to 128,916 bytes against 100,000.

## Fix
- **Core** (`crates/core/src/multipart.rs`): `SessionRecord::completing_teardown`
  (`:2291`) builds, from a `Completing@E` record, the session at `Aborting@E+1`,
  `retire:bytes:s:<id>:<E>` = `{session, all}`, and `retire:records:s:<id>:<E>` =
  `{seg: (nonce, E)}`. The records payload comes from the private
  `RetireObligation::attempt_segments` (`:3665`), which takes the token epoch from the
  group, so key and payload always agree. It returns `None` below `Completing` or at
  `u64::MAX` (no wrap, no panic). The retire rows table gives the reason for
  `{session, all}` (`:3301`).
- **The fence** (`crates/custodian/src/restore.rs`): `plan_fence` (`:886`) picks the
  writes per state. `fence_session` (`:801`) commits one `WriteBatch`:
  `require(mpu:<id>, bytes as read)`, `require_absent` on **every** obligation key, the
  session put, and both obligation puts. On `Conflict` it re-reads each obligation key
  and names the first one taken.
- **Checking the attempt's records**: after the commit, `check_attempt` (`:971`) pages
  the attempt's `seg:` range and, only if it holds anything, the session's `part:` range
  (`part_chunks`, `:1019`). An undecodable segment, a stray key, or a chunk no part holds
  names the session in the new `RestoreReport::segments_unaccounted`, and on the audit
  seam as `session-segments-unaccounted`.
- **Re-runs** (`recheck_fenced`, `:935`): for an `Aborting@E'` session it reads
  `retire:records:s:<id>:<E'-1>`. If it owes exactly the session's own group and nothing
  else (`:957`), the range is checked. If it is undecodable or owes anything else, the
  obligation is named. If it is absent, the range must be empty or the session is named.
- **CLI** (`crates/server/src/cli.rs`): the verdict counts the new list (`:1282`) and the
  NEEDS-HUMAN paragraph names each session with its first bad record and why
  (`:1402-1416`). `unsettled_causes` loses its `Completing` slot.
- **#851's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be
  fenced" `Completing@3` fixture is now `Completing@u64::MAX`, since a plain
  `Completing@3` is fenced now.
- **Docs**: the fence paragraph in `06-runtime-view.md` §6.5 (`:65`) and step 7 of the m4
  blueprint (`:600-644`) now cover `Completing` sessions.

## Verification
Line numbers are on this branch (main + #846 + #847 + #851 + this patch). Tests are in
`crates/custodian/tests/restore_completing_fence.rs` unless noted.

- **Claim:** a `Completing@E` session is fenced in one commit with both obligations, each
  decoding against its key; the records obligation is installed even when
  `segments_written` is 0.
  - **Checked:** `restore.rs:801` (`fence_session`), `:886` (`plan_fence`),
    `multipart.rs:2291` (`completing_teardown`), `:3665` (`attempt_segments`).
  - **Test:** `a_completing_session_is_fenced_with_its_segments_deleter` (`:388`).
- **Claim:** the fence lands whole or not at all.
  - **Test:** `a_failed_completing_fence_commit_leaves_none_of_its_writes` (`:411`). It
    fails the commit carrying each of the three writes in turn.
- **Claim:** neither obligation overwrites one already there.
  - **Checked:** `restore.rs` `fence_session`, `require_absent` on each key.
  - **Test:** `neither_obligation_overwrites_one_already_there` (`:438`), with one arm per
    key (bytes and records), each beside an uncollided session that is still fenced.
- **Claim:** every value the fence writes fits the store's value ceiling, whatever the part
  count.
  - **Test:** `a_sparse_completing_session_is_fenced_inside_the_value_ceiling` (`:477`).
    Its store double refuses any value over `MAX_VALUE_BYTES` (and the test first proves
    that with a one-byte-over probe). 10,000 parts numbered 1, 3, …, 19,999, with a segment
    naming the chunk of part 19,999 so `part:` paging is exercised.
- **Claim:** what cannot be fenced cleanly is never passed off as done (no nonce, a chunk
  no part holds, an undecodable segment, a stray key, `u64::MAX`).
  - **Checked:** `restore.rs:971` (`check_attempt`), `:1019` (`part_chunks`).
  - **Test:** `what_cannot_be_fenced_cleanly_is_never_passed_off_as_done` (`:560`).
    Includes a two-chunk segment whose second chunk is the bad one, a bad record at index
    512 (past the first page), and one audit event per named session. Every test session's
    nonce differs from its upload id, so the two cannot be confused.
- **Claim:** a second run leaves the store byte-identical and still names what needs a
  human, including an obligation that is missing, undecodable, owes another group, owes
  only parts, or owes the own group plus parts.
  - **Checked:** `restore.rs:935-960` (`recheck_fenced`, the guard at `:957`).
  - **Test:** `a_second_pass_is_idempotent_and_still_names_what_needs_a_human` (`:591`).
- **Claim:** the fence still runs after Pass 3, and a fence fault never hides the other
  verdicts.
  - **Test:** `a_completing_fence_fault_never_hides_the_pass_verdicts` (`:676`).
- **Claim:** `completing_teardown` returns a teardown only for `Completing` below
  `u64::MAX`, and both keys match their payloads.
  - **Test:** `only_a_completing_session_has_one_and_both_obligations_match_their_keys`
    (`multipart.rs:5197`).
- **Red → green:** with the `src` changes reverted and the tests kept, all 7 tests in
  `restore_completing_fence.rs` compile and fail by assertion. With the fix, all 7 pass.
  A mutant that drops the `parts().is_none()` guard fails the re-run test.
- **Whole gate:** `cargo xtask ci` passes (typos, docs render and link audit, fmt,
  clippy, workspace tests, the statics gate, and the DST crate under `--cfg madsim`).

**Follow-ups (not in this PR):**
- DST coverage of the fence is #843.
- A re-run does not name an already-fenced session (`Open` or `Completing`) whose
  `retire:bytes` obligation is missing: #855.
- No test pins `part_chunks` skipping an unreadable `part:` record
  (`restore.rs:1017-1027`); a mutant that turns the skip into an error survives today.
- 0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`)
  still specify `{session, parts}` and hit the same value ceiling. Those writers belong to
  #656 and #659; this PR does not edit 0016.

Fixes #842
