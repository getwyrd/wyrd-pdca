## Summary
**User impact:** when the metadata store loses track of whether a part upload was saved
(for example, a TiKV commit that timed out on the client side), the upload request fails
with an "outcome unknown" error, even when the part was in fact saved. The client has to
upload the part again, and the slot the first attempt used stays taken until the upload
ends.

This PR makes the request check what happened before answering: it reports success when
the reads prove that this request's own write landed, and otherwise keeps the "outcome
unknown" error, now with a clear statement of whether the write could still land. It never
sends a second write after an unknown outcome, so it cannot save a part twice or delete
the data of a part that was saved.

No production caller uses this code yet: it takes effect once the S3 `UploadPart` verb is
wired (#669). This PR builds on #1028 (PR #1051), which must merge first.

## What to look at
- The new settling step in the multipart part commit: what it reads after an unknown
  outcome, and the one condition under which it answers success.
- The rule that nothing is written after an unknown outcome, and why (see Root cause).
- To try it: `cargo test -p wyrd-core --test multipart_upload_part_commit`. The test
  wrapper holds or fails individual store writes and reads, and replays them later on a
  real redb store, so each race below is run, not simulated.

## Root cause
A part commit answered `CommitUnknownResult` returned `InDoubt` at once with no read
(`crates/core/src/multipart/commit.rs:1065` before this change, marked
`// deferred: #1029`), so a commit that had landed was still reported in doubt. Settling it
is harder than it looks: after a commit lands, every condition it checked can be rebuilt.
A late reserve, intent and renewal of the same attempt put the slot back at the exact bytes
the commit required, and a same-number empty rival writes the empty prior's exact bytes
again. So "my slot is still held" or "the part key is unchanged" proves nothing, and
resending the batch, or compensating, can land the part twice or schedule a saved part's
chunks for deletion.

## Fix
- `settle` (`commit.rs:876-952`) runs instead of the immediate in-doubt stop
  (`commit.rs:1350-1356`). It makes at most `SETTLE_LAPS` (two) laps of four reads: the
  part key, the session, the attempt's own slot, the session again.
- It answers `Committed` only on the own-commit proof (`commit.rs:918-924`): the slot is
  gone and the session read made **after** the slot read still finds the `Open@E` bytes
  the commit required. While the session is `Open`, only this attempt's own commit deletes
  its slot, so no other writer and no late batch of this attempt can produce it. The part
  key is never taken as proof: a `PartRecord` carries no attempt id.
- Everything else ends in `InDoubt`, which now carries `may_still_commit` and the cause
  that ended the reads. The flag starts at the store's report and only falls: a session
  at a newer epoch, a deleted session with its slot, or a rival record where the commit
  required the key absent means the commit cannot land any more.
- Each read is folded into that state as it returns (`Doubt::read`, `commit.rs:832-844`),
  so a read fault later in a lap keeps what earlier reads proved (`commit.rs:904`, `:912`).
- No batch follows an unknown outcome: no resend and no compensation. This keeps the store
  contract that a `WriteBatch` is not retried (`crates/traits/src/lib.rs:240-257`). The
  resend after a definite `Conflict` from #1028 is unchanged, since a `Conflict` round never
  lands.
- The cost is availability: a commit that did not land, or landed and was then rebuilt
  over, answers in doubt and its slot stays held until the session ends.
- Docs: the module doc (`commit.rs:155-170`) and
  `docs/design/architecture/05-building-block-view.md:250` describe the new behaviour.

## Verification
Line numbers are on `main` with #1028 merged and this patch applied.

- **Claim:** an unknown outcome on a commit that landed answers `Committed`, with the slot
  released once.
  **Checked:** `commit.rs:918-924`. **Test:**
  `b_a_landed_commit_answered_unknown_is_committed` (test file `:4305`), for both values of
  `may_still_commit`; also `c3_a_landed_commit_then_a_rival_is_committed` (`:5032`).
- **Claim:** after an unknown outcome the attempt sends exactly one commit batch and no
  compensation. **Checked:** `commit.rs:1350-1356`. **Test:**
  `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` (`:2610`).
- **Claim:** late staging that rebuilds a landed commit's slot never leads to a
  compensation. **Test:** `x_late_staging_that_rebuilds_a_landed_commits_slot_never_compensates`
  (`:4734`).
- **Claim:** a replayed history never lands the same commit twice or publishes chunks owed
  to retirement. **Test:** `x2_a_replay_after_an_applied_commit_never_lands_it_twice`
  (`:4893`) and `x2_b_replay_after_a_held_commit_never_lands_it_twice` (`:4899`). Putting
  back a resend after an unknown outcome turns 17 tests red, including both of these on
  "at most one commit batch lands".
- **Claim:** matching bytes are never taken as proof, and a torn read never answers
  `Committed`. **Test:** `c1_an_empty_part_is_never_committed_on_its_bytes` (`:4913`),
  `c2_a_torn_read_never_answers_committed` (`:4993`).
- **Claim:** a read fault keeps what earlier reads proved. **Checked:** `commit.rs:904`,
  `:912`. **Test:** `c_a_read_fault_after_the_session_moved_keeps_the_commit_unable_to_land`
  (`:5298`) and `c_a_read_fault_after_a_rival_where_none_was_required_keeps_the_commit_unable_to_land`
  (`:5305`) assert `may_still_commit: false` (`:5258`), then release the held batch and
  check it answers `Conflict` (`:5275`). Removing the fold at `:912` turns only the first
  red; removing the fold at `:904` turns the second red.
- **Red before, green after:** with the change reverted (in doubt at once, no reads), 26 of
  the 77 tests in `crates/core/tests/multipart_upload_part_commit.rs` fail, including the
  landed, C3, cross-phase and one-batch cases. All 77 pass with the change.
- **Mutation testing:** `cargo mutants --in-diff` on this diff: 18 caught, 11 unviable,
  3 timeouts (mutants that make the read loop endless, caught by the tests hanging),
  0 missed.
- **Whole gate:** `cargo xtask ci` passes on the patched tree, including `typos`, the docs
  lint and render, clippy, every workspace test and `cargo deny`.

Fixes #1029
