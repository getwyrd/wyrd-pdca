# Adversarial review — issue 508 / multipart-upload (iteration 2)

Verdict: the red→green evidence is genuine and I could not refute the core fix; two
implementation-grade findings and one sequencing question below.

## Evidence re-run (attempted refutation — failed)

- Re-ran the proof myself at `$PDCA_TARGET` (scratch clone for the red leg, swept):
  **base + only the new test file** compiles clean (no new production symbol — the
  iteration-1 compile-red defect is fixed) and all 7 tests fail **by assertion** on the
  wire (501 `NotImplemented` / wrong status), incl. the base answering
  `PUT ?part%4Eumber=1` with **200** — i.e. the #491-encoded destructive fall-through was
  real on the base and the malformed-query leg genuinely reds. With the patch: 7/7 green
  (`crates/server/tests/s3_multipart_upload.rs`). Exercises the production path (stock
  `aws-sdk-s3` + raw signed HTTP against the loopback gateway; no parallel re-implementation).
- Attacked the ETag oracle: the pinned known answers (`s3_multipart_upload.rs:935-939`,
  `PART1_MD5`/`PART2_MD5`/`COMPOSITE_ETAG`) match an independent CPython/OpenSSL MD5 I
  computed here, and the `crates/core/src/md5.rs:196-236` vectors match RFC 1321 + an
  independent 130-byte digest. The server is not checking itself.
- Attempted to refute: key-grammar injection via a client-supplied `uploadId` containing
  `:` (gated — every part-key path first requires a live session, `crates/server/src/lib.rs:592-596,676-680,776-794`, and minted ids are 32-hex); GC reclaiming a live upload's
  staged fragments (the reference-set safety gate at `crates/custodian/src/gc.rs:163`
  protects them; the test's restore-pass leg would red without the hook); non-chunk-aligned
  part boundaries corrupting GET (read path concatenates per-`ChunkRef` `len`,
  `crates/core/src/read.rs:92-95`; no Range GET on this floor). Could not.

## Findings

- NEEDS-HUMAN [impl] — **Abort/Complete racing a concurrent UploadPart leaks a permanent,
  custodian-invisible part record.** `multipart::abort` (`crates/core/src/multipart.rs:495-515`)
  and `complete` (`:543-601`) CAS only on the **session** bytes, and `commit_part` (`:451`)
  never mutates the session — so an UploadPart that commits between Abort/Complete's
  `list_parts` scan and their batch commit satisfies both preconditions in either order.
  Concrete case: `DELETE ?uploadId=U` racing `PUT ?partNumber=3&uploadId=U` (a client
  cancelling a parallel upload) leaves `uploadpart:U:0000000003` behind with no session;
  the GC hook (`crates/custodian/src/gc.rs:301-329`) then keeps its ~5 MiB of fragments in
  the reference set **forever** — unreachable by Abort/ListParts (`NoSuchUpload`), never
  marked stranded by `reconcile_after_restore` (they are "referenced"). The brief's Design
  §GC prescribed exactly the missing piece: "a custodian hook reclaims chunks referenced
  only by **deleted-session** part records" — the implemented hook does the opposite
  (treats every `uploadpart:` record as a reference, sweeps none). Milder sibling: Complete
  racing a re-upload of a listed part number deletes the new record without orphaning its
  chunks (restore-pass-reclaimable leak; no corruption — the safety gate protects the
  published chunks). Fix is iterable: bump the session record in `commit_part` so the
  Abort/Complete session CAS loses the race, and/or add the brief's session-less-part sweep.
- NEEDS-HUMAN [impl] — **`mint_upload_id`'s "128 bits of OS entropy" claim is unwarranted**
  (`crates/server/src/lib.rs:829-843`). `RandomState::new()` draws OS entropy once per
  thread; each subsequent construction derives from a per-thread counter, so successive
  upload ids on one worker thread are a deterministic SipHash-1-3 stream from one seed —
  correlated outputs of a hasher std explicitly does not warrant as cryptographic, not
  fresh entropy per id. Exposure is bounded (every request is SigV4-authenticated;
  the id only gates cross-session interference), but either mix a real RNG draw per id or
  correct the doc-comment so the guarantee isn't overstated at sign-off.
- NEEDS-HUMAN [impl] — **conformance nit:** `GET /bucket?uploads` on an unknown bucket
  answers `200` with an empty `<ListMultipartUploadsResult>` (intercept at
  `crates/gateway-s3/src/lib.rs:1519-1532` runs before any bucket-existence check;
  `list_multipart_uploads` in `crates/server/src/lib.rs:808-822` never consults
  `list_container`), while the adjacent listing route answers `404 NoSuchBucket`
  (`crates/gateway-s3/src/lib.rs:792-798`). Real S3 answers `NoSuchBucket` for both.
  Related nit, same class: a multipart key mixed with a still-denylisted subresource
  (`GET /b?uploads&acl`, `POST /b/k?uploadId=U&partNumber=1`) routes to the multipart verb
  ignoring the extra key, where the base refused 501 — harmless (no destructive
  fall-through) but worth a deliberate line in build-notes.
- NEEDS-HUMAN — **the verify base is missing declared dependency 510.** The brief declares
  `Depends on: 507, 509, 510` and wave order `[507]→[509]→[510]→[508]`, but the target
  worktree HEAD is `ee801ec "pdca-integrate: issue_509"` (507 via PR #609, then 509 — no
  510 fold). My red/green reproduction and the C4 gates therefore attest a base without
  510. If 510 is still pending, this dispatch rewrite lands before it — the exact rebase
  the plan's ordering note tried to avoid; if 510 was dropped (as 506 was), the brief's
  dependency line is stale. Needs the human's knowledge of 510's fate; verdict on
  sequencing is provisional, not a defect in the code.

## Carry-forward audit (iteration 1 rejections — all verified addressed)

Typos gate: C4-ci gating PASS. Assertion-red shape: reproduced (above). Destructive
malformed-query fall-through: three-way classifier + wire test, red on base (I observed the
base 200). Signed-payload integrity on UploadPart: carried through the seam and tested.
Part-number range: enforced 1..=10000 as `InvalidArgument`. Pagination: `IsTruncated`
computed, markers real, tested over the SDK.
