# Result — issue 508 / multipart-upload

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the full S3 multipart-upload verb set — CreateMultipartUpload, UploadPart,
- Success criterion: against the in-process loopback S3 gateway: a stock `aws-sdk-s3`
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the six verbs + their state, end-to-end: remove `uploads`/`uploadId`/

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue 508: implement the full S3 multipart-upload verb set, staged-part lifecycle, atomic completion, and abort/GC reclamation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is decision-complete for wire behavior, multipart ETag, 5 MiB non-final-part floor, atomic publication, and abort/GC observables exercised by `crates/server/tests/s3_multipart_upload.rs:170`. |
| C2 Reproduction (red pre-fix) | FAIL | The required assertion-red discriminator is not met: applying only the declared new test and its test dependency to the target base fails to compile because the test calls the patch-added `Gateway::run_gc` (`crates/server/tests/s3_multipart_upload.rs:514`), so the pre-fix failure is not the specified wire-level assertion failure. |
| C3 Change | PASS | The scoped behavior is implemented end-to-end at the protocol-neutral seam, including session creation (`crates/server/src/lib.rs:588`), atomic completion (`crates/server/src/lib.rs:692`), abort (`crates/server/src/lib.rs:783`), and both listing surfaces (`crates/server/src/lib.rs:799`). |
| C4 Verification (red→green) | FAIL | The focused patched suite is green (4/4), but the mandatory CI result independently remains red: `typos` exits 2 on changed text including `crates/core/src/multipart.rs:497` and `crates/gateway-s3/src/lib.rs:2151`; moreover the independently reconstructed pre-fix leg is compile-red, not assertion-red (`crates/server/tests/s3_multipart_upload.rs:514`). |
| C5 Causal adequacy | PASS | The change removes the unsupported-routing cause and supplies real multipart state/assembly/reclamation rather than adding a capability probe or symptom guard; staged fragments are explicitly included in GC reachability (`crates/custodian/src/gc.rs:301`). |
| T1 Structure | PASS | The focused integration target compiles with the patch, and the protocol-neutral implementation remains in core/server while S3 query and XML handling remain in `crates/gateway-s3/src/lib.rs:2110`. |
| T2 Shape | PASS | Decoded query classification covers the new object forms without extending the encoded-key bypass class (`crates/gateway-s3/src/lib.rs:2138`), and completion validates strict ordering, ETags, and the non-final size floor (`crates/core/src/multipart.rs:451`). |
| T3 Runtime | PASS | Direct execution of the patched integration target passed all four runtime tests, including byte-identical assembly, invalid completion, abort plus GC, and upload listing (`crates/server/tests/s3_multipart_upload.rs:173`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether overlapping closed/rejected work exists for the 11 affected paths — merged local history was checked by affected path and showed no prior multipart implementation, but closed/rejected review history is unavailable locally, so duplication risk cannot be mechanically discharged. |
| T5 Judgment | NEEDS-HUMAN | Decide whether the metadata-prefix/GC design requires an ADR and whether unshown ListMultipartUploads pagination is acceptable — these open design choices affect persistence governance and client compatibility beyond the four exercised cases (`crates/server/src/lib.rs:820`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the implementation is fit for real large-object use by running `aws s3 cp` of an 8+ GB file against a deployed gateway and comparing source/download SHA-256 — Check exercised the same verbs only with small in-process bodies (`crates/server/tests/s3_multipart_upload.rs:178`). |

### Advisory — adversary

# Adversarial review — issue 508 / multipart-upload

Attacked the red→green evidence, the fix, and the reviewer's verdict. The shipped
`s3_multipart_upload.rs` legs (round-trip byte-identity, composite ETag vs an independent
`md-5` oracle, InvalidPart-without-publish, abort+GC fragment reclamation, ListMultipartUploads)
are genuine wire-driven assertions over the production `Gateway` — I could **not** refute those
happy paths, and the MD5 primitive is pinned to RFC-1321 vectors + an external oracle. The
refutations below land on paths the test never drives.

- **NEEDS-HUMAN [impl] — Destructive fall-through the patch's own doc-comment claims is impossible.**
  `crates/gateway-s3/src/lib.rs:340` removes `uploads`/`uploadId`/`partNumber` from
  `UNSUPPORTED_SUBRESOURCES`, and `unsupported_subresource` (`:393`) matches only that list.
  Routing only *classifies* a multipart op when the query is **well-formed**: `multipart_object_op`
  (`:2149-2160`) returns `None` for any PUT unless **both** `uploadId` **and** a *u32-parseable*
  `partNumber` are present, and DELETE/GET require `uploadId` (`:2163`). So these authenticated
  requests now skip the guard (`:1646`) and hit the plain-verb arms (`PUT` streaming overwrite
  `:1700`, `DELETE` object delete `:1840`) — each was a safe `501` on the wave base:
    - `PUT /b/k?partNumber=1` (no `uploadId`) → **plain PUT overwrites the whole object** with the
      part body — the exact "`PUT …?partNumber=N` falls through to a plain PUT and destructively
      overwrites the whole object" the comment at `:338-339` / `:1573-1574` says can "never" happen.
    - `PUT /b/k?partNumber=abc&uploadId=U` (non-numeric part) → `.parse::<u32>()` fails → `None` →
      same destructive overwrite.
    - `PUT /b/k?uploadId=U` (no `partNumber`) and `PUT /b/k?uploads` → overwrite.
    - `DELETE /b/k?partNumber=1` or `DELETE /b/k?uploads` (no `uploadId`) → **deletes the object**.
  The shipped test only issues well-formed SDK calls, so it never goes red on this. Fix is local:
  after `multipart_object_op` returns `None`, refuse (501/400) any request still carrying a decoded
  `uploads`/`uploadId`/`partNumber` key rather than letting it reach the plain verb.

- **NEEDS-HUMAN — UploadPart drops the signed-payload integrity check PutObject performs.**
  `crates/gateway-s3/src/lib.rs:2251` handles a single-shot `PayloadHash::Signed(_)` by **discarding**
  the signed `x-amz-content-sha256` (`_`) and staging `raw` unverified. The plain PUT path instead
  passes `ContentHash::Expected(hex)` (`:1712-1721`) and rejects a mismatch as `PayloadMismatch`
  (`:2889`). The `MultipartGateway::upload_part` seam (`crates/gateway-core/src/lib.rs:1093`,
  impl `crates/server/src/lib.rs:2062`) carries no `ContentHash`, so a signed part whose bytes are
  altered in transit is staged and folded into the completed object undetected — the composite
  ETag is `md5`-of-what-was-*received*, so it cannot catch it either. The inline comment
  ("Content-MD5 … out of scope") conflates per-part Content-MD5 with the `x-amz-content-sha256`
  verification the brief's own integrity primitives call for; this is a seam-contract/scope call,
  not obviously the brief's declared out-of-scope item. (The `Streaming` arm is fine — per-chunk
  signatures are verified in `streaming::decode`.)

- **NEEDS-HUMAN [impl] — Part-number range is unvalidated at the routing boundary.**
  `multipart_object_op` (`crates/gateway-s3/src/lib.rs:2152`) accepts any `u32` part number, so
  `partNumber=0` and `partNumber>10000` are staged (S3 requires `1..=10000`, `InvalidArgument`
  otherwise). A `partNumber=0` part can be UploadPart-staged but is then permanently unreachable at Complete:
  `assemble` seeds `last = 0` and rejects `r.part_number <= last` as `InvalidPartOrder`
  (`crates/core/src/multipart.rs:715-720`), so its chunks only ever leave via Abort/GC. Minor
  conformance + a small orphan-until-abort footprint; not on the binding criterion.

- **Pagination stubbed (flagged, low).** `render_list_parts_result` / `render_list_uploads_result`
  hard-code `<IsTruncated>false</IsTruncated>` (`crates/gateway-s3/src/lib.rs:1678,1706`) and the
  server materializes the full set (`crates/server/src/lib.rs:2253,2266`). An upload with >1000
  parts, or a bucket with many sessions, is returned whole with `IsTruncated=false` — the brief
  itself lists ListMultipartUploads pagination as an open question, so noting rather than pressing.

## Could not refute
- The composite-ETag math, out-of-order assembly, and byte-identity are checked against an
  **independent** RustCrypto `md-5` oracle over the real wire — no tautology, no mocked defect.
- Abort + `run_gc` reclamation is asserted through the `FsChunkStore` tempdir the test owns
  (`after_upload > baseline`, `after_abort > baseline`, `after_gc == baseline`) — a real observable,
  not a store-handle peek; the reclamation is genuine, not eager deletion.
- Complete's one-batch CAS on the session record (`crates/core/src/multipart.rs:569`) makes a
  racing Abort / duplicate Complete lose with `Conflict`; I could not construct a double-publish.

## Toolchain note
Did not re-run the heavy in-process SDK integration test (multi-GB `target/` rebuild); the
`C4-verify` gate already records red→green PASS and all findings above are static, wire-shape /
control-flow arguments that do not depend on re-execution. The gating `C4-ci` red is a `typos`
lint failure (separate deterministic gate), not adjudicated here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether overlapping closed/rejected work exists for the 11 affected paths — merged local history was checked by affected path and showed no prior multipart implementation, but closed/rejected review history is unavailable locally, so duplication risk cannot be mechanically discharged.
- [ ] T5 Judgment — Decide whether the metadata-prefix/GC design requires an ADR and whether unshown ListMultipartUploads pagination is acceptable — these open design choices affect persistence governance and client compatibility beyond the four exercised cases (`crates/server/src/lib.rs:820`).
- [ ] Validation — fitness-to-purpose — Decide whether the implementation is fit for real large-object use by running `aws s3 cp` of an 8+ GB file against a deployed gateway and comparing source/download SHA-256 — Check exercised the same verbs only with small in-process bodies (`crates/server/tests/s3_multipart_upload.rs:178`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the reds and the advisory findings — the feature is close but not shippable as-is. Blocking reds to clear: - C4-ci gating FAIL: `cargo xtask ci` red on `typos` misspellings in changed text (e.g. crates/core/src/multipart.rs, crates/gateway-s3/src/lib.rs). CI must go green (fix the misspellings, or a deliberate typos.toml exception). - C2/C4 red→green shape: pre-fix leg is compile-red because the test calls the patch-added `Gateway::run_gc` (crates/server/tests/s3_multipart_upload.rs:514). The brief requires a wire-level ASSERTION-red on the base — reshape the test so it fails by assertion, not by referencing a new production symbol. Advisory issues to fix: - Destructive fall-through (data loss): malformed multipart queries now skip the guard and hit the plain verb — `PUT /b/k?partNumber=1` (no uploadId), non-numeric partNumber, `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads` overwrite the whole object; malformed DELETE deletes it. These were safe 501 on the base. After `multipart_object_op` returns None, refuse (501/400) any request still carrying a decoded uploads/uploadId/partNumber key instead of letting it reach the plain arm. Add malformed-query coverage to the test so this goes red without the guard. - UploadPart drops the signed-payload `x-amz-content-sha256` integrity check that plain PutObject enforces (gateway-s3/src/lib.rs:2251). Carry a ContentHash through the upload_part seam and reject a mismatch, matching the plain-PUT path. - Part-number range unvalidated: reject partNumber=0 and >10000 (InvalidArgument) at the routing boundary. - Pagination stubbed (IsTruncated=false hard-coded) — lower priority; brief lists it as an open question, but note it.
- By / date: Eduard Ralph / 2026-07-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
