# Adversarial review — issue 508 / multipart-upload (iteration 6)

Method: static attack on the applied tree at `$PDCA_TARGET` (patch is in the working tree).
I did **not** re-run `cargo`/`run-verify.sh` in place — the target is read-only and its
`target/` cache is 87 GB; every finding below is grounded in the source as it stands, and each
names the request that breaks it. Gate rows C4-ci / C4-verify are taken as given.

## Refutations

- **NEEDS-HUMAN [impl] — `UploadPartCopy` is now silently served as a 0-byte `UploadPart`.**
  `crates/gateway-s3/src/lib.rs:1938` intercepts *every* multipart form **before** the
  `x-amz-copy-source` guard at `:1998`, and the UploadPart arm (`:2335-2374`) reads only
  `content-type` off the head. So `PUT /b/k?partNumber=1&uploadId=<32hex>` carrying
  `x-amz-copy-source: /src/key` (an empty body by definition) stages a **zero-byte part**,
  answers `200` with `ETag: "e3b0c442…"` (the SHA-256 of nothing) and no `CopyPartResult` body.
  Concrete failing case: `aws s3api upload-part-copy` + `complete-multipart-upload` naming that
  single part — `publish_fenced` skips the `MIN_PART_BYTES` check for the final part
  (`crates/server/src/multipart.rs:840`), so Complete answers `200` and the destination object
  is published **empty**. On `origin/main` this form was a clean `501` (`partNumber` was on the
  denylist, `crates/gateway-s3/src/lib.rs:343-345` pre-patch), and the patch's own comment at
  `:1986-1997` states the rule it just broke ("refuse a form we do not implement rather than
  silently overwrite with zero bytes"). `x-amz-copy-source` is declared out of scope by the
  brief — out of scope means *refused*, not *mishandled*. Neither new test file mentions
  `x-amz-copy-source`, so nothing would have gone red. Fix is one guard beside `:1938`.

- **NEEDS-HUMAN — any recorded D-server drain turns every `UploadPart` into `404 NoSuchUpload`
  fleet-wide.** Every staging batch carries `require_absent(desired:dserver:<S>)` for each
  server in the placement (`crates/core/src/multipart.rs:1489-1491`), and the placement is the
  **identity vector** `0..fragments.len()` — always servers `0..=8` at the deployed RS(6,3)
  (`crates/core/src/write.rs:905`). A `Conflict` on that batch is reported as
  `WriteError::StagingFenced` (`crates/core/src/write.rs:931-938`) and mapped to
  `MultipartError::NoSuchUpload` (`crates/server/src/multipart.rs:453-456`) → `404`. Concrete
  case, reachable inside the patch's own fixture: in
  `crates/server/tests/s3_multipart_lifecycle.rs:604` (`set_lifecycle(&meta, 0, Draining)`),
  issue **any** further `upload_part` — it answers `404 NoSuchUpload` while ordinary
  `PutObject` (which carries no drain fence) keeps succeeding, i.e. an operator maintenance
  drain silently disables all multipart uploads and tells every client its upload does not
  exist (aws-cli treats 404 as fatal and abandons the transfer). 0016 specifies the other
  behaviour — the fence failure "**re-plans against the fresh `Topology::excluding(draining)`**"
  (`0016:658`, X59 at `0016:2588`) — and no re-plan exists here. Flagged non-`[impl]` because
  M0 has no placement selector to re-plan against: the choice between implementing exclusion,
  answering `503 SlowDown`, or scoping the fence is a design call, not an iteration.

- **NEEDS-HUMAN [impl] — a same-part-number retry that races the original answers `404
  NoSuchUpload`.** `commit_part_batch` preconditions `require_absent(part:<id>:<n>)` when it saw
  no prior (`crates/core/src/multipart.rs:1565`), and `upload_part` maps *every* commit
  `Conflict` to `NoSuchUpload` (`crates/server/src/multipart.rs:570-572`). Concrete case: two
  concurrent `upload_part(part_number = 1)` calls on an `Open` session (the shape an SDK
  produces when it times out a slow part and retries while the first is still streaming) — the
  loser gets `404 NoSuchUpload` for a session that is demonstrably still open; real S3 answers
  `200` to both, last writer wins. The sequential re-upload is covered
  (`s3_multipart_upload.rs:1344`), the concurrent one is not — `concurrent_part_uploads_do_not_
  conflict_with_each_other` (`:1650`) drives **distinct** part numbers only, so this would not
  have gone red. A `Conflict` on the `part:` precondition alone should not be spelled
  "your upload is gone".

- **NEEDS-HUMAN [impl] — the "second Abort is 204" assertion is a race, not an observation.**
  `crates/server/tests/s3_multipart_upload.rs:646-666` writes the held part's head + half body
  and then *immediately* issues the first `DELETE`; the first Abort spawns the whole-namespace
  drain (`crates/server/src/multipart.rs:163-179`), whose `teardown_sessions` deletes the `mpu:`
  record as soon as `list_owned_staging` reads empty (`crates/core/src/multipart.rs:2827`). If
  the held part has not yet committed its first `sidx:` entry when that pass runs, the session
  is gone and the second Abort answers `404 NoSuchUpload`, failing at `:663`. The sibling test
  gets this right — `s3_multipart_lifecycle.rs:594-597` polls `poll_until(sidx …, true)` before
  proceeding. Same fix here: poll for `sidx:{upload_id}:` before the first Abort.

- **NEEDS-HUMAN [impl] — stale derived value in a load-bearing comment.**
  `crates/core/src/multipart.rs:124` documents `MAX_STAGED_CHUNKS` as `51_480`; with this
  build's measured 320-byte chunk-ref it is `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS = 312 × 156 =
  48_672`. Trivial to fix, but this is exactly the class the module itself calls out at `:60-63`
  (0016's quoted 302 B "under-counts by 18 bytes"), and the next reader sizing a knob off the
  comment inherits a 6 % over-estimate of the publishable ceiling.

## Attacked and could not refute

- The three iteration-5 carry-forwards are genuinely closed, not papered over:
  `drain_records`' paged branch now sets `complete = false` on a truncated derivation and
  `commit_units` refuses to delete the obligation (`crates/core/src/multipart.rs:2641-2662`,
  regression test at `:3285`); the `Completed` tombstone gives its admission slot back through
  `release_admission_batch` (`:2069`, `teardown_sessions` arm at `:2840-2857`); and the
  lengthless `aws-chunked` PUT is bounded at staging time by `MAX_MAP_CHUNKS`
  (`crates/server/src/lib.rs:558`, `crates/core/src/write.rs:579`) with malformed
  `x-amz-decoded-content-length` failing closed (`crates/gateway-s3/src/lib.rs:2142-2159`).
- The percent-encoding fence: I could not find a spelling that reaches a plain object verb —
  `decoded_query_key` (`crates/gateway-s3/src/lib.rs:515`) decodes keys once, valueless keys
  included, and `foreign_subresource_on_multipart` (`:545`) re-applies the whole denylist to
  every non-marker key, so `?partNumber=1&uploadId=…&t%61gging=1` still refuses 501.
- The Complete-retry fingerprint: the quote-trimming asymmetry between the tombstone arm
  (`crates/server/src/multipart.rs:595-601`, untrimmed) and the publish arm (`:848-853`,
  trimmed) is **not** exploitable over HTTP — `parse_complete_body`
  (`crates/gateway-s3/src/lib.rs:2602-2611`) strips the quotes before either sees the value.
  It would bite a second protocol front-end on the same seam; not this diff's defect.
- Segmented maps are resolved through one shared resolver for every maintenance consumer
  (`crates/custodian/src/gc.rs:388-405`), and both `unlink` (`crates/core/src/metadata.rs:623`)
  and the superseding commit (`:740`) route a segmented prior through the O(1) generation
  obligation — the iteration-4 finding 1 leak is closed.
