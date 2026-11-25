## Summary
**User impact:** a client that uploads a large file in parts (S3 multipart upload)
cannot finish the upload once the file is bigger than what one flat chunk map holds
(158 chunks). Asking to complete it is refused with a "not supported yet" error, so
large files uploaded in parts never show up under their key.

This PR lets the complete step publish those large uploads too, as a segmented
object (its chunk list split across several records), and keeps the retry budget
honest when a completion attempt already wrote some of those records and then fails.
Nothing calls this in production yet; the S3 endpoint comes with #670.

Third of the five slices of #658. It builds on #1008 (PR #1050), which is not on
`main` yet; please review and merge that one first. It was built and tested on
`main` plus #1008 and #1028 (PR #1051).

## What to look at
- **The publish loop** behind `complete_upload` in the multipart complete module. It
  no longer stops at one flat map. It first writes the chunk list in segment records,
  in as many writes as needed, then flips the object into place. Each segment write
  also records how far the upload has got, so a retry picks up where the last round
  stopped instead of starting over.
- **The release when an attempt fails.** The rule to check: an attempt that wrote no
  segment gets its retry back, as before. An attempt that wrote segments keeps its
  retry spent and leaves a cleanup record naming exactly the segments it wrote. So
  every written segment is owned either by the published object or by a cleanup
  record, and the retry cap limits how much leftover one upload can create.
- **To try it:** `cargo test -p wyrd-core --test multipart_complete`. The new tests
  complete uploads past the flat limit and read them back byte for byte. Others make
  the publish lose or fail after segments have landed, delete the bucket mid-publish,
  make the store unsure whether a write landed, and force a retry that resumes in the
  middle of the segment list.

## Root cause
The previous slice published only flat objects and refused, by design, any part list
past `MAX_MAP_CHUNKS` chunks (`Stop::SegmentedUnsupported`, marked `deferred: #1009`).
The committer already supported a segment phase, but Complete had no caller for it and
no rule for what a failed attempt owes once its segments are durable.

## Fix
Line numbers are on the tested base (`main` + #1008 + #1028) with this patch applied.

- **Segmented publication.** `publish_held` (was `publish_flat`,
  `crates/core/src/multipart/complete.rs:1287`) gives the committer
  (`crates/core/src/staged_publication.rs:445`) a real segment contribution:
  - each segment batch requires the session at `Completing@E+1` with
    `segments_written = first_index` and moves it to `written` (`complete.rs:1325`).
    The per-cursor record comes from the new `SessionRecord::with_segments_written`
    (`crates/core/src/multipart.rs:2717`), which refuses a cursor past the decode
    bound (`:2726`);
  - the group is the attempt's own, from `attempt_segment_group` (`multipart.rs:2598`),
    taken once when the fence is minted (`Placed::mint`, `complete.rs:393`);
  - `resume_from` is the cursor this Complete last read (`complete.rs:1350`), so a
    later round pins the segments already written instead of writing them again.
- **Segment count from the committer.** The flip requires the session at
  `staged_publication::planned_segments` (`staged_publication.rs:417`), computed once
  in `FlipPlan::read` (`complete.rs:1494`) and pinned by the flip (`:1571`). It is
  never re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉`; wide chunk references can push the
  real count above that.
- **The flat-only refusal is gone.** `Stop::SegmentedUnsupported` and the `Admit` enum
  are deleted. `fence_complete` now holds the fence body (`complete.rs:790`).
- **Release and charging.** `Placed` (`complete.rs:375`) mints both releases before
  the fence is sent, so no fence is placed that could not be released. `OwnFence`
  (`:1026`) carries the cursor it last read, and `release_batch` (`:1074`):
  - cursor 0 (`:1076`): requires the fence's bytes and gives the attempt back. This is
    the old release, byte for byte;
  - cursor > 0: requires the session at that cursor, keeps `attempts` charged, and
    installs `retire:records:` `{seg: (nonce, E+1)}` under `require_absent`, through
    the existing `RetireObligation::attempt_segments`.

  Every release requires the session's exact bytes at the cursor it assumes, so a
  wrong guess fails as a conflict and can never land as a release without the
  obligation. After a lost round the cursor is re-read through `OwnFence::follow`
  (`:1057`, used at `:1397`). After a round the store refuses for good, one read is
  added before the release (`:1378`); if that read fails, a conflicting release whose
  re-read shows this same fence further along is re-sent from there (`:1171`).
- **Unknown outcomes.** The committer does not say which batch an unknown outcome
  came from. A round with segment batches to send reports the new
  `DoubtfulCommit::Publication { epoch }` (`complete.rs:501`); a flip-only round
  reports `Flip` (`:1340`). Nothing is sent after either. Recovery is #1010.
- **Docs.** The `complete_upload` paragraph in
  `docs/design/architecture/05-building-block-view.md:254` replaces "segmented
  publication is not supported yet" with the segmented path, the cursor, the charging
  rule and the release's obligation.

Known limits, all tracked: a published segmented object cannot be deleted or
overwritten until the retirement drain lands (#659,
`crates/core/src/metadata.rs:2148-2151`, `:2188-2193`); that is harmless while nothing
in production publishes one. One rule has no dedicated segmented test yet: a segmented
attempt whose segment batches all lose without landing must give its retry back. The
code handles it (the cursor stays 0, so the cursor-0 release is taken), and a scratch
test confirmed it, but the permanent test is planned for #1010.

## Verification
Test lines are in `crates/core/tests/multipart_complete.rs`.

- **Claim:** a part list past one flat map publishes a segmented root that names the
  attempt's group and reads back byte for byte; the session's cursor matches the
  committer's segment count.
  - **Checked:** `complete.rs:1287`, `:1325`, `:1494`
  - **Test:** `:4658`
- **Claim:** exactly `MAX_MAP_CHUNKS` chunks stays flat; one chunk more is segmented.
  This replaces the old "refused past the flat limit" tests.
  - **Test:** `:4779`
- **Claim:** a fence that wrote segments stays charged, and after
  `MAX_COMPLETE_ATTEMPTS` (`multipart.rs:5629`) such attempts the next Complete is
  refused with zero commits. Each release left a decodable `retire:records:`
  obligation for its own group.
  - **Checked:** `complete.rs:1074-1076`
  - **Test:** `:4838`
- **Claim:** a bucket removed after the segments land, or a flip the store refuses for
  good, releases with the segment obligation and stays charged.
  - **Checked:** `complete.rs:1171`, `:1378`
  - **Test:** `:4932`, `:4981`
- **Claim:** an unknown segment-batch outcome (applied or not) returns the in-doubt
  error and sends nothing more; in a resumed round with only the flip left, the error
  names the flip.
  - **Checked:** `complete.rs:501`, `:1340`
  - **Test:** `:5043`, `:5108`
- **Claim:** when a later segment batch loses after an earlier one landed, the next
  round resumes mid-list from the cursor; when it is refused for good, the release
  follows the cursor to that mid-list position.
  - **Checked:** `complete.rs:1350`, `:1057`
  - **Test:** `:5185`, `:5290` (two real segment batches, asserted from the commit
    log)
- **Claim:** the cursor can move up to the decode bound and never past it.
  - **Checked:** `multipart.rs:2726`
  - **Test:** `multipart.rs:6184` (unit test)
- **Fails before, passes after:** with the production change reverted, all 9 new
  segmented tests fail with the old "segmented publication unsupported" refusal; the
  56 older tests pass. Single-rule mutations each turn their test red: giving the
  attempt back on every release fails `:4838`; dropping the segment obligation fails
  `:4838` and `:4932`; re-deriving the segment count fails `:4658`; treating an unknown
  outcome as a conflict fails `:5043`; restarting from cursor 0 on a mid-list retry
  fails `:5185`. `cargo mutants` on the diff: 45 caught, 12 unviable, 1 missed. The
  miss is `cursor >= self.cursor` → `true` (`complete.rs:1062`), which is equivalent:
  no writer ever moves the cursor backwards.
- **Whole gate:** `cargo xtask ci` passes, including the `--cfg madsim` DST build and
  tests. `multipart_complete` runs 65 tests in about 2 s. `typos` and the docs lint
  and render check are clean.

Fixes #1009
