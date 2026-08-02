# Design proposal — issue 508 / multipart-upload

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.

- **Slug:** multipart-upload
- **Kind:** enhancement (design proposal)
- **Goal:** the full S3 multipart-upload verb set — CreateMultipartUpload, UploadPart,
  CompleteMultipartUpload, AbortMultipartUpload, ListParts, ListMultipartUploads — so
  objects >5 GB become possible and default clients (aws-cli/boto3 auto-switch to
  multipart above ~8 MB) can upload large objects at all. Today every multipart form is
  hard-refused 501 via the subresource denylist.
- **Success criterion:** against the in-process loopback S3 gateway: a stock `aws-sdk-s3`
  client completes `create_multipart_upload` → `upload_part` × N (parts out of order, over
  the streaming wire path) → `complete_multipart_upload`, and a subsequent GET returns the
  assembled object **byte-identical** to the concatenated parts, with the Complete
  response's ETag in S3 multipart form (`"<md5-of-part-md5s>-N"`); completing with a wrong
  part list fails (`InvalidPart`/`InvalidPartOrder`) without publishing; `list_parts`
  reflects staged parts; `abort_multipart_upload` ends the session (subsequent
  UploadPart/Complete/ListParts → `NoSuchUpload`) and abort + the custodian's GC pass
  leave **no unreferenced staged fragments** — asserted through observables the harness
  actually has: the wire, plus the `FsChunkStore` tempdir the test owns (fragment-file
  count returns to its pre-upload baseline), NOT a store handle (`Gateway`'s metadata
  field is private and the store is moved into `Gateway::new` — adversary finding; if
  session-record hygiene needs direct assertion, build and seed the store BEFORE
  `Gateway::new`, as 507's brief describes). Asserted by
  `crates/server/tests/s3_multipart_upload.rs`, red on the wave base (object-scoped
  multipart verbs → 501; see Falsifiability for the per-leg red reasons).
- **Falsifiability:** RED is producible in-process on the wave base (the folded
  507+509+510 result): every OBJECT-scoped multipart leg (Create/UploadPart/Complete/
  Abort/ListParts) is refused 501 by `unsupported_subresource`
  (`crates/gateway-s3/src/lib.rs:328-366` — `uploads`/`uploadId`/`partNumber` at
  `lib.rs:330-332`, guard at `lib.rs:813-820`, all cited on main; 507/509/510 do not
  remove these entries). The BUCKET-scoped `GET /b?uploads` (ListMultipartUploads) leg's
  red reason differs: on the folded base it flows through 507's bucket-GET routing, which
  MUST preserve the subresource denylist for bucket paths (a 507 requirement, stated in
  its brief) — so it also reds as 501, not as a listing. The test must fail by ASSERTION,
  not compile error: drive everything over the wire (SDK/HTTP against the loopback
  gateway), import no new production symbol. Multi-GB bodies are NOT needed
  to earn the red/green — the harness's small chunk size (`with_chunk_size(8)`, peer
  `crates/server/tests/s3_object_metadata.rs:43-52`) makes modest parts span many chunks;
  the >5 GB/8 MB acceptance is an off-Check manual leg (see Verification posture). This
  bundle is wave≥1; `run-verify.sh` honours `$PDCA_VERIFY_BASE` (`_resolve_base_ref`,
  engine/scripts/run-verify.sh:186-192).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 507, 509, 510
- **Conflicts with:**
- **Ordering note:** scheduled LAST — encoded as explicit `Depends on` edges, NOT as
  conflict edges: the adversarial plan review showed the wave scheduler orients a bare
  conflict pair name-lower-first, which would have run 508 in wave 1 and re-based 509/510
  onto this rewrite. The dependency is deliberate sequencing (wave-fold ordering, docs 09),
  not a semantic build-on: 508 is the largest rewrite of the same
  `crates/gateway-s3/src/lib.rs` dispatch every other batch issue edits (removing three
  denylist entries and adding query-form routing for five verbs), so it builds on the
  folded result of 507/509/510 rather than forcing them to rebase onto it. Resulting
  wave order: [507] → [509] → [510] → [508]. (506/HeadObject left the batch: re-landed
  on main as PR #607, issue closed.) The ETag
  prerequisite (#503) is merged on main (commit 68403eb, ADR-0047). No genuine build-on
  dependency inside the batch.
- **Difficulty:** high
- **Scope:** the six verbs + their state, end-to-end: remove `uploads`/`uploadId`/
  `partNumber` from the denylist and route the query forms (`POST /b/k?uploads`,
  `PUT /b/k?partNumber=N&uploadId=U`, `POST /b/k?uploadId=U`, `DELETE /b/k?uploadId=U`,
  `GET /b/k?uploadId=U`, `GET /b?uploads`); a protocol-neutral multipart seam crossing
  `gateway-core` (no S3 vocabulary — ADR-0010); session + staged-part state in the
  `MetadataStore` under new disjoint key prefixes; UploadPart staging over the existing
  streaming write path (RS-encode to chunks now, part→chunk mapping held under the
  session); Complete assembling ordered parts into the object's chunk map in ONE metadata
  commit (CAS, concurrent-Complete-safe) returning the multipart ETag; Abort + custodian
  GC of orphaned/aborted staged parts; ListParts + ListMultipartUploads; the new routing
  must not be evadable via percent-encoded query keys (#491 — match the DECODED key form).
  / out of scope: fixing #491 for the REMAINING denylist entries (separate open issue —
  just don't extend the bug to the new routes); `x-amz-copy-source` UploadPartCopy;
  per-part `Content-MD5`/checksum trailers beyond what the existing streaming path
  verifies; multipart expiry/lifecycle policy (GC of ABANDONED-but-unaborted sessions
  beyond a custodian sweep hook is follow-up — state the chosen boundary in build-notes).
- **External dependencies:** none — base toolchain; the shipped test runs in-process on
  the loopback stack. The >5 GB / `aws s3 cp` auto-multipart acceptance is off-Check,
  via the AWS CLI against a deploy stack (registered doctor row
  "aws cli (S3 gateway round-trip)").
- **Test file:** crates/server/tests/s3_multipart_upload.rs   (NEW `*/tests/*.rs` file —
  `run-verify.sh --classify` dry-run confirms an added `crates/server/tests/*.rs` is the
  red→green discriminator)
- **Verification posture:** the binding criterion (round-trip, ETag form, abort/GC
  hygiene) is a flippable red→green at Check via the test file above — the default holds.
  Pre-declared deferred leg: the issue's headline acceptance "`aws s3 cp` of an 8+ GB file
  round-trips sha256-identical" is observable only off-Check against a real deploy stack;
  the multipart machinery it exercises IS built and exercised at Check by the SDK
  integration test (same verbs, same wire forms, smaller bodies). The maintainer confirms
  the large-object leg by hand at sign-off or post-merge; record it in §9.
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites: the denylist + guard to relax `crates/gateway-s3/src/lib.rs:328-366`,
  `:813-820` (its doc-comment names UploadPart's destructive fall-through — the reason
  routing must land WITH the denylist removal, atomically); the streaming PUT path
  UploadPart stages through `lib.rs:853-902` → `put_object_streaming`
  (`crates/server/src/lib.rs:283`); the commit primitive for Complete/Abort/session CAS is
  `WriteBatch::require`/`require_absent` (`crates/traits/src/lib.rs`, used per ADR-0046's
  record pattern); the custodian's orphan/GC machinery to hook staged-part reclamation
  into is `crates/custodian/` (`tests/gc.rs` shows the harness pattern); disjoint-prefix
  record design per ADR-0046 decisions 1–2 (`docs/design/adr/0046-…`); the SDK test
  client `crates/server/tests/s3_gateway_cluster.rs:98`.
- **Disposition hint:** new-feature

## Motivation

The single biggest S3-surface gap: PutObject's 5 GB ceiling makes larger objects
impossible, and default clients auto-switch to multipart above ~8 MB, so ordinary large
uploads fail today. Core to the 0.1-Alpha epic (milestone 16).

## Design

**State model (the issue's named design decision — decided here):** multipart state lives
in the **metadata keyspace**, not a dedicated store, following ADR-0046's record pattern —
new disjoint prefixes, keyed **collision-safely**: the bucket/key contain arbitrary bytes
including the delimiter, so the server-minted `upload-id` (opaque, unique) is the sole
scan/lookup component — `upload:{upload-id}` for the session record (which STORES the
bucket/key inside the JSON, never in the key) and `uploadpart:{upload-id}:{part-number}`
for each staged part's chunk map + part ETag/size (fixed-width part number so scan order
is numeric),
JSON-encoded like existing records, invisible by construction to the custodian's current
`inode:`/`pending:`/`orphan:` scans until explicitly wired. Rationale: the store's atomic
`commit` with preconditions is exactly what Complete needs (assemble N part chunk-maps
into the object's inode record in one CAS commit, precondition on the session record still
existing — a racing Abort or duplicate Complete loses cleanly with `Conflict`), and a
second storage system for transient state would add an ops surface Alpha doesn't need.

**UploadPart** streams through the existing write path (RS-encode to chunks as the bytes
arrive — never buffer a part), then commits the part record; re-uploading a part number
replaces the record, orphaning the previous chunks into the GC path. **Complete** verifies
the client's part list (numbers, ETags, all-but-last ≥5 MB per S3 — enforce or relax
deliberately and say which), assembles in part-number order, computes
`md5(concat(part-md5s))-N`, commits object inode + dirent + session deletion in one batch.
Note the part-ETag implication: S3 part ETags are MD5s and the composed ETag is
md5-of-md5s — the session must store per-part MD5 (computed while streaming), distinct
from the sha256-flavoured single-PUT ETag (ADR-0047); the wire layer returns the stored
part MD5 as UploadPart's ETag. **Abort** deletes session + part records in one batch and
routes staged chunks into the existing orphan/GC path so nothing is permanently stranded;
the shipped test asserts this through the store/custodian handles. **GC:** a custodian
hook reclaims chunks referenced only by deleted-session part records (crash between
staging and commit), mirroring the existing orphan-grace design.

**Part-size floor (decided, not open):** enforce S3's 5 MiB minimum for every part except
the last, validated at Complete (per S3, `EntityTooSmall`). The shipped test uses ≥5 MiB
parts (two parts suffice; in-memory redb + fs-tempdir handle 10 MiB comfortably, and the
SDK's auto-multipart part size is 8 MiB anyway) — do NOT weaken the floor to make small
test parts pass.

**Routing:** parse the query ONCE into a typed operation (decoded keys, so the #491
percent-encoding bypass class cannot recur on these routes), then dispatch; the denylist
keeps refusing everything still unimplemented. `ListMultipartUploads` (`GET /b?uploads`)
is bucket-scoped and plugs into the bucket-routing split 507 landed on this wave's base.

## Alternatives considered

- **A dedicated staging store** (filesystem spool of raw parts, RS-encode at Complete):
  simpler ETag math, but Complete then re-reads and re-encodes the whole object (hours for
  large uploads), needs its own crash-recovery/GC, and breaks the stream-don't-buffer
  invariant at the seam. Rejected.
- **Buffering parts in memory:** violates the 0015:789 OOM-cliff invariant. Rejected.
- **Sessions in coordination (etcd-style) state:** part maps are bulk metadata, not
  coordination; and the metadata store's CAS commit is the primitive Complete needs.

## Impact & compatibility

Three denylist entries become live routes (the guard's protective purpose — refusing
forms that would be destructively mishandled — is preserved by real routing). New record
prefixes in the metadata keyspace (no on-disk chunk-format change; conformance vectors
untouched). New trait surface in gateway-core (in-tree implementers updated in-change).
Custodian gains a reclamation input. No new dependency: Complete's small XML body
(`<CompleteMultipartUpload><Part>…`) is parsed by a minimal in-crate routine and
ListParts/ListMultipartUploads XML is string-built, as 507/509 do.

## Open questions

- Whether the maintainer wants the record-prefix/GC design captured as an ADR before
  merge (it is metadata-keyspace content, not on-disk format; this proposal is written to
  be sufficient) — flag at sign-off; reviewer will NEEDS-HUMAN any spec-adjacent call.
- `ListMultipartUploads` pagination: reuse 507's materialize-sort-slice pattern; confirm.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the reds and the advisory findings — the feature is close but not shippable as-is. Blocking reds to clear: - C4-ci gating FAIL: `cargo xtask ci` red on `typos` misspellings in changed text (e.g. crates/core/src/multipart.rs, crates/gateway-s3/src/lib.rs). CI must go green (fix the misspellings, or a deliberate typos.toml exception). - C2/C4 red→green shape: pre-fix leg is compile-red because the test calls the patch-added `Gateway::run_gc` (crates/server/tests/s3_multipart_upload.rs:514). The brief requires a wire-level ASSERTION-red on the base — reshape the test so it fails by assertion, not by referencing a new production symbol. Advisory issues to fix: - Destructive fall-through (data loss): malformed multipart queries now skip the guard and hit the plain verb — `PUT /b/k?partNumber=1` (no uploadId), non-numeric partNumber, `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads` overwrite the whole object; malformed DELETE deletes it. These were safe 501 on the base. After `multipart_object_op` returns None, refuse (501/400) any request still carrying a decoded uploads/uploadId/partNumber key instead of letting it reach the plain arm. Add malformed-query coverage to the test so this goes red without the guard. - UploadPart drops the signed-payload `x-amz-content-sha256` integrity check that plain PutObject enforces (gateway-s3/src/lib.rs:2251). Carry a ContentHash through the upload_part seam and reject a mismatch, matching the plain-PUT path. - Part-number range unvalidated: reject partNumber=0 and >10000 (InvalidArgument) at the routing boundary. - Pagination stubbed (IsTruncated=false hard-coded) — lower priority; brief lists it as an open question, but note it.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on §6 items 4-8. The feature core is accepted-shape (C1/C3/C5, T1-T3 pass; adversary could not refute red→green), but the brief needs re-versioning before the next build — its base/dependency assumptions and GC design section no longer match reality: - BASE / brief staleness (item 8): the brief's declared wave order [507]→[509]→[510]→[508] was not honored — 508 was built and verified on base ee801ec (507+509 only). 510 was accepted 2026-07-19 (PR #610), not dropped. Re-plan against the true base: fold PR #610 into pdca-integration/main first, update the brief's Depends-on/ordering/falsifiability sections to the folded base, and require re-verification there; 510 touches the same gateway-s3 dispatch this patch rewrites. - GC design (item 5): the brief's prescribed sweep ("reclaim chunks referenced only by deleted-session part records") was not implemented — the shipped hook treats every uploadpart: record as a live reference and sweeps none, so an Abort/Complete racing a concurrent UploadPart leaks a permanent, custodian-invisible part record (abort/complete CAS only the session; commit_part never bumps it). The revised brief must make this leg binding and testable: bump the session record in commit_part so the racing CAS loses, and/or the session-less-part sweep, with a red→green assertion for the race/orphan case. - Item 6: mint_upload_id's "128 bits of OS entropy" doc-claim is unwarranted (RandomState derives per-thread after one seed) — brief should state the required id-generation contract (real RNG draw per id, or an honest doc-comment). - Item 7: GET /bucket?uploads on an unknown bucket must answer 404 NoSuchBucket like the adjacent listing route, not 200 empty-list; the brief should name bucket-existence behavior for the bucket-scoped route and require the multipart-key + denylisted-subresource mix handling to be stated deliberately. - Item 1 (C4): the full cargo xtask ci gate (including cargo deny with a writable advisory database) reruns at the next Check; do not carry the provisional pass. - Item 4 (validation): the 8+ GB aws s3 cp deploy round-trip stays the human's off-Check leg, to be recorded in §9 on the next pass.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
