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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

Task under review: implement the complete S3 multipart-upload verb set, including streaming assembly, validation, listing, abort, and staged-fragment reclamation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is sufficiently decidable for Check: wire round-trip, composite ETag, invalid completion, abort, listing, and fragment hygiene all have observable outcomes exercised at `crates/server/tests/s3_multipart_upload.rs:422`. |
| C2 Reproduction (red pre-fix) | PASS | On base `ee801ec2`, the added test compiled and ran, then all 7 cases failed at runtime (six on expected multipart 501 responses and the encoded malformed query on an unsafe 200), grounding the assertion-red at `crates/server/tests/s3_multipart_upload.rs:438`. |
| C3 Change | PASS | The review decision is whether all six verbs and their lifecycle are covered without reopening destructive plain-verb fall-through; decoded, fail-closed classification precedes ordinary dispatch at `crates/gateway-s3/src/lib.rs:1581`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the full gate as provisional or rerun with a writable cargo advisory database — the dedicated suite independently flipped 0/7 red to 7/7 green at `crates/server/tests/s3_multipart_upload.rs:422`, but `cargo xtask ci` stopped at `cargo deny check` on the host's read-only advisory DB lock after earlier stages passed. |
| C5 Causal adequacy | PASS | The decision turns on removing the refusal/fall-through cause rather than masking it; typed multipart routing replaces the denylist path and refuses malformed forms before plain verbs at `crates/gateway-s3/src/lib.rs:1594`, with no capability-probe/runtime-guard smell. |
| T1 Structure | PASS | The protocol-neutral seam, metadata state machine, S3 adapter, server streaming implementation, and custodian reference scan preserve layer ownership; staged parts enter GC accounting at `crates/custodian/src/gc.rs:293`. |
| T2 Shape | PASS | The discriminator remains a production-symbol-independent SDK/HTTP test that compiled unchanged on the base and patch, while the multipart seam is exercised through public wire behavior at `crates/server/tests/s3_multipart_upload.rs:431`. |
| T3 Runtime | PASS | The independently run integration suite passed all 7 cases, including byte-identical assembly/ETag at `crates/server/tests/s3_multipart_upload.rs:589`, invalid completion, signed-hash refusal, pagination, abort/GC, and malformed-query safety. |
| T4 Contribution | NEEDS-HUMAN | Confirm no merged or closed/rejected work already covers each of the 9 affected paths — the supplied artifacts contain no affected-file prior-art result and the available `gh` is a shim, so contribution novelty cannot be mechanically settled here. |
| T5 Judgment | NEEDS-HUMAN | Decide whether the metadata-prefix/GC design needs an ADR before merge and whether the implemented pagination semantics are the desired contract — these spec-adjacent choices affect long-lived metadata and client interoperability (`crates/core/src/multipart.rs:292`, `crates/gateway-s3/src/lib.rs:1908`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Run `aws s3 cp` with an 8+ GB file against a deployed gateway, GET it back, and compare SHA-256 while observing multipart completion/cleanup — the in-process suite validates the same verbs only with modest bodies at `crates/server/tests/s3_multipart_upload.rs:427`, so real-deploy large-object fitness remains unexercised. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the full gate as provisional or rerun with a writable cargo advisory database — the dedicated suite independently flipped 0/7 red to 7/7 green at `crates/server/tests/s3_multipart_upload.rs:422`, but `cargo xtask ci` stopped at `cargo deny check` on the host's read-only advisory DB lock after earlier stages passed.
- [ ] T4 Contribution — Confirm no merged or closed/rejected work already covers each of the 9 affected paths — the supplied artifacts contain no affected-file prior-art result and the available `gh` is a shim, so contribution novelty cannot be mechanically settled here.
- [ ] T5 Judgment — Decide whether the metadata-prefix/GC design needs an ADR before merge and whether the implemented pagination semantics are the desired contract — these spec-adjacent choices affect long-lived metadata and client interoperability (`crates/core/src/multipart.rs:292`, `crates/gateway-s3/src/lib.rs:1908`).
- [ ] Validation — fitness-to-purpose — Run `aws s3 cp` with an 8+ GB file against a deployed gateway, GET it back, and compare SHA-256 while observing multipart completion/cleanup — the in-process suite validates the same verbs only with modest bodies at `crates/server/tests/s3_multipart_upload.rs:427`, so real-deploy large-object fitness remains unexercised.
- [ ] **Abort/Complete racing a concurrent UploadPart leaks a permanent,
- [ ] **`mint_upload_id`'s "128 bits of OS entropy" claim is unwarranted**
- [ ] **conformance nit:** `GET /bucket?uploads` on an unknown bucket
- [ ] **the verify base is missing declared dependency 510.** The brief declares

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected on §6 items 4-8. The feature core is accepted-shape (C1/C3/C5, T1-T3 pass; adversary could not refute red→green), but the brief needs re-versioning before the next build — its base/dependency assumptions and GC design section no longer match reality: - BASE / brief staleness (item 8): the brief's declared wave order [507]→[509]→[510]→[508] was not honored — 508 was built and verified on base ee801ec (507+509 only). 510 was accepted 2026-07-19 (PR #610), not dropped. Re-plan against the true base: fold PR #610 into pdca-integration/main first, update the brief's Depends-on/ordering/falsifiability sections to the folded base, and require re-verification there; 510 touches the same gateway-s3 dispatch this patch rewrites. - GC design (item 5): the brief's prescribed sweep ("reclaim chunks referenced only by deleted-session part records") was not implemented — the shipped hook treats every uploadpart: record as a live reference and sweeps none, so an Abort/Complete racing a concurrent UploadPart leaks a permanent, custodian-invisible part record (abort/complete CAS only the session; commit_part never bumps it). The revised brief must make this leg binding and testable: bump the session record in commit_part so the racing CAS loses, and/or the session-less-part sweep, with a red→green assertion for the race/orphan case. - Item 6: mint_upload_id's "128 bits of OS entropy" doc-claim is unwarranted (RandomState derives per-thread after one seed) — brief should state the required id-generation contract (real RNG draw per id, or an honest doc-comment). - Item 7: GET /bucket?uploads on an unknown bucket must answer 404 NoSuchBucket like the adjacent listing route, not 200 empty-list; the brief should name bucket-existence behavior for the bucket-scoped route and require the multipart-key + denylisted-subresource mix handling to be stated deliberately. - Item 1 (C4): the full cargo xtask ci gate (including cargo deny with a writable advisory database) reruns at the next Check; do not carry the provisional pass. - Item 4 (validation): the 8+ GB aws s3 cp deploy round-trip stays the human's off-Check leg, to be recorded in §9 on the next pass.
- By / date: Eduard Ralph / 2026-07-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
