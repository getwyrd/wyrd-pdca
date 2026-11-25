## Summary
**User impact:** a client that uploads a large file in parts (S3 multipart upload) has
no way yet to finish it. Asking to complete the upload can lock the upload for
completion, but the parts never become a readable object, so the file never shows up
under its key.

This PR adds the core step that finishes an upload: it joins the named parts, in part
order, into one object, publishes it, and marks the upload completed, all in one
atomic write. It covers objects up to one flat chunk map. Larger objects get a clear
"not supported yet" error until segmented publication lands (#1009). Nothing calls this
in production yet; the S3 endpoint comes with #670.

Second of the five slices of #658. It builds on the fence phase from #1007 (PR #1032)
and on the part commit from #1027 (PR #1041), both on `main`.

## What to look at
- **The new entry point**, `complete_upload` in the multipart complete module, and the
  publish loop behind it. That is where a lost race is retried, where a deleted bucket
  is caught, and where the upload is unlocked again when publishing cannot succeed.
- **The one publishing write.** Besides the object itself, it records which staged
  bytes are now unused and can be reclaimed later. The rule to check: nothing that was
  just published is ever marked for reclaim, and every staged byte stays accounted for.
- **To try it:** `cargo test -p wyrd-core --test multipart_complete`. The new tests
  stage parts out of order through the real part upload, complete them, and read the
  object back byte for byte. Others race an overwrite, delete the bucket, or make the
  store fail at the moment of publishing.

## Root cause
Before this change the complete path stopped after taking the fence (the lock that
freezes the upload's part set for one Complete). No code published the frozen parts,
recorded the completion, or arranged for the leftover staged bytes to be reclaimed.

## Fix
- `complete_upload` (`crates/core/src/multipart/complete.rs:1165`) runs the fence phase,
  then `publish_flat` (`:1178`). That joins the named parts' chunk lists in part order
  and publishes through the existing staged-publication committer
  (`crates/core/src/staged_publication.rs:445`). The committer already pins the dirent
  and inode (`:917-927`) and applies `overwrite_modified` (`:885`).
- The flip's own part of the batch (`FlipPlan::flip`, `complete.rs:1383`):
  - moves the session `Completing@E` → `Completed@E+1` (`SessionRecord::completed`,
    `crates/core/src/multipart.rs:2702`), with the ETag and the request fingerprint;
  - installs `retire:records:` for the published parts only, and `retire:bytes:` for
    any staged part the list left out (`multipart.rs:4224`, `:4232`);
  - installs `retire:bytes:` for a superseded generation only if it has chunks
    (`superseded`, `complete.rs:1485`; `multipart.rs:4260`). A zero-byte prior owns
    nothing, and an empty obligation would not decode;
  - each obligation sits under `require_absent`, and the batch pins the exact `bucket:`
    marker bytes validation read (`complete.rs:1431`).
- **Lost flip:** re-read the session, the target and the bucket marker
  (`complete.rs:1258-1282`). If the session moved, answer from its new state. If the
  marker is gone or changed, release and answer `NoSuchBucket`. Otherwise retry with the
  version recomputed from the re-read prior, up to `R_PUBLISH`, then release and answer
  `OperationAborted`.
- **Refused flip:** a definite store error releases the fence once (`give_up`, `:1295`)
  and stops with that error as the cause. If the release fails too, the error says the
  fence still stands. **Unknown flip outcome:** return the in-doubt error and send
  nothing more (`:1245`). Recovery from that is #1010.
- **Flat only, for now:** `Admit::Flat` (`complete.rs:688`) refuses a list past
  `MAX_MAP_CHUNKS` chunks. It refuses before the fence with no commit (`:777`), or
  behind it with one release when only the frozen set shows the excess (`:825-839`).
  Both it and `Stop::SegmentedUnsupported` are marked `deferred: #1009`.
  `fence_complete` keeps its behaviour (`Admit::Any`).
- One stamp, `PublishRequest::completed_at_millis`, sets both the completion time and
  the root's `modified`. It comes from the session's clock, which the fence already
  checked. This is written up in the module doc (`complete.rs:143-150`).
- The architecture doc gets a paragraph on the new function
  (`docs/design/architecture/05-building-block-view.md:254`).

Beyond the design, and argued in the code: the session is re-read after a lost flip,
so a moved session is answered right away rather than after more doomed retries. A
segmented prior is retired by its segment group instead of being refused.

Follow-up to note: the flip writes the bucket pin by hand (`complete.rs:1429-1431`, and
the re-read at `:1273-1281`) rather than calling `require_bucket`
(`crates/core/src/multipart/create.rs:698-705`). When #511 adds the bucket-deletion
fence, it needs to cover this caller too.

## Verification
Line numbers are on `main` with this patch applied. The touched files are the same on
`main` as on the base the patch was built and tested on.

- **Claim:** parts staged out of order, including a non-final part that is not a whole
  number of chunks, publish as one flat object that reads back byte for byte. The ETag
  and completion match `multipart_etag`, and a repeat answers `AlreadyCompleted`.
  - **Checked:** `complete.rs:1178`, `:1383`
  - **Test:** `crates/core/tests/multipart_complete.rs:3330`
- **Claim:** after rejected attempts (bad part numbers, replaced parts), a valid list
  still publishes.
  - **Test:** `multipart_complete.rs:3425`. The replacement is a real part upload
    landing during the fence commit.
- **Claim:** a subset Complete marks only the published parts' records for retirement,
  and the left-out part's bytes and records for reclaim. No published chunk is ever
  marked.
  - **Checked:** `multipart.rs:4224`, `:4232`
  - **Test:** `multipart_complete.rs:3490`
- **Claim:** the version comes from the re-read prior after a lost flip, and running out
  of retries releases the fence and leaves the session `Open` with `attempts` restored.
  - **Checked:** `complete.rs:1258-1282`
  - **Test:** `multipart_complete.rs:3562`, `:3650`
- **Claim:** a zero-byte prior gets no generation obligation.
  - **Checked:** `complete.rs:1485`
  - **Test:** `multipart_complete.rs:3723`
- **Claim:** a bucket removed, or removed and recreated, just before the flip answers
  `NoSuchBucket` and leaves no object.
  - **Checked:** `complete.rs:1431`
  - **Test:** `multipart_complete.rs:3763`
- **Claim:** a flip the store refuses for good releases the fence once and stops with
  that error.
  - **Checked:** `complete.rs:1295`
  - **Test:** `multipart_complete.rs:4026`
- **Claim:** an unknown flip outcome (applied or not) returns the in-doubt error and
  sends nothing more.
  - **Checked:** `complete.rs:1245`
  - **Test:** `multipart_complete.rs:4093`
- **Claim:** a list past one flat map stops with zero commits before the fence, or with
  one release behind it.
  - **Checked:** `complete.rs:688`, `:777`, `:825-839`
  - **Test:** `multipart_complete.rs:4140`, `:4179`
- **Fails before, passes after:** without the fix the new tests do not compile, because
  `complete_upload` does not exist. So each rule was proven on its own instead. 25
  single-rule mutations of the production code all turned at least one test red, and
  none of them failed to compile. Examples: retiring every staged part's records instead
  of only the published ones fails the subset test. Freezing the version at fence time
  fails the lost-flip test. Dropping the bucket pin fails the removed-bucket test.
  Treating an unknown outcome as a conflict fails the unknown-outcome test.
- **Whole gate:** `cargo xtask ci` passes, including the `--cfg madsim` DST build and
  tests. `multipart_complete` runs 59 tests: the 43 from #1007 plus 16 new.
  `typos` and the docs lint and render check are clean.

Fixes #1008
