# Result — issue 507 / list-objects-v2

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: ListObjectsV2 (`GET /bucket?list-type=2`) with `prefix`, `delimiter`
- Success criterion: against the in-process loopback S3 gateway with objects stored
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: route bucket-scoped GETs (relax the object-path guard for GET on `/bucket`),

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

Review of issue #507: implement S3 ListObjectsV2 and a ListObjects v1 compatibility path with filtering, grouping, pagination, bucket-existence, and XML/error semantics.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is concrete and falsifiable across routing, ordering, metadata, pagination, errors, v1 compatibility, and XML escaping; the human has no unresolved semantic choice within the stated feature boundary (`crates/server/tests/s3_list_objects.rs:1`). |
| C2 Reproduction (red pre-fix) | PASS | In an isolated committed-base clone with only the added test, all 14 cases failed at the expected bucket-only routing boundary with `400 InvalidRequest`, grounding the claimed pre-fix symptom (`crates/server/tests/s3_list_objects.rs:158`). |
| C3 Change | PASS | The change stays within the specified metadata record, neutral gateway seam, S3 wire handling, server implementation, and wire-only integration-test surfaces; no unrelated behavior or dependency expansion was found (`crates/gateway-core/src/lib.rs:194`). |
| C4 Verification (red→green) | FAIL | Although the isolated test moved from 0/14 red to 14/14 green, required `cargo xtask ci` stops at `typos`: `mis-decoding` is rejected as a misspelling, so the patch is not gate-green (`crates/gateway-s3/src/lib.rs:556`). |
| C5 Causal adequacy | PASS | The patch removes the bucket-only routing rejection and supplies the listing path itself; no capability probe or runtime guard papering over an eager/load-time cause was added (`crates/gateway-s3/src/lib.rs:1237`). |
| T1 Structure | PASS | Protocol-neutral listing data remains in gateway-core while S3 grouping, pagination, tokens, and XML remain in gateway-s3, preserving the project seam boundary (`crates/gateway-core/src/lib.rs:194`). |
| T2 Shape | PASS | The new integration test drives the signed loopback SDK/HTTP boundary without importing a new production listing symbol, so it tests observable wire shape rather than implementation structure (`crates/server/tests/s3_list_objects.rs:1`). |
| T3 Runtime | PASS | The applied target ran all 14 in-process runtime cases successfully, including zero-budget pagination, delimiter chaining, encoded subresources, start-after, invalid tokens, and v1 marker behavior (`crates/server/tests/s3_list_objects.rs:444`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the local affected-path history plus the brief's rejected-iteration carry-forward adequately discharges prior art — merged history showed no equivalent listing implementation, but closed/rejected remote work cannot be mechanically settled from the supplied artifacts, which matters for duplicate or superseded contribution risk (`crates/gateway-s3/src/lib.rs:539`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether shipping listing before #511/backfill is acceptable — production has no marker writer yet, so live listings return `NoSuchBucket` despite the feature being correct in marker-seeded tests, which matters to immediate user utility (`crates/server/src/lib.rs:463`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process SDK evidence is sufficient for the promised client interoperability — after fixing CI, seed/backfill `bucket:<name>`, launch the gateway, run `aws s3 ls s3://<bucket>` and `aws s3 sync <dir> s3://<bucket>`, and confirm complete, non-duplicated listings because real AWS CLI acceptance was not exercised (`crates/server/tests/s3_list_objects.rs:140`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 507 / list-objects-v2 (iteration 2)

Adversarial pass. The red→green evidence was re-run **independently** (scratch clone at
wave base `07d0244` + the ADDED test only): all 14 tests fail by **assertion** (400
`InvalidRequest` off the wire, compile succeeds — no new production symbol imported), and
all 14 pass on the patched tree. The proof drives the production path end-to-end via a
stock `aws-sdk-s3` client over a real loopback listener; it is not a parallel
re-implementation, and the ETag oracle is an independent SHA-256. The C4-verify PASS in
`check-gates.json` is corroborated, not merely trusted. All six iteration-1
carry-forwards are genuinely addressed (max-keys=0, decoded denylist, delimiter+max-keys
chaining under truncation, start-after/encoding-type, tempered seam-cost doc,
list-type≠2 → 400 / NextMarker gated on delimiter). Three refutations still landed:

- NEEDS-HUMAN [impl] — **Denylist gap: `GET /bucket?versioning` is silently answered with a
  listing document** — the exact failure class the brief's routing decision 1 forbids.
  `UNSUPPORTED_SUBRESOURCES` (`crates/gateway-s3/src/lib.rs:330-361`) lists `versions` but
  not `versioning`, so the bucket route's fence (`lib.rs:1247`) passes it through to
  `list_objects`. Empirically demonstrated with a scratch probe: stock-SDK
  `get_bucket_versioning()` against the patched tree receives `<ListBucketResult>` and dies
  with `XmlDecodeError: expected VersioningConfiguration but got ListBucketResult` instead
  of a clean 501 — and `aws s3 sync`-adjacent tooling (rclone, boto-based scripts) does
  probe versioning. Same gap for `intelligent-tiering`, `ownershipControls`,
  `policyStatus`, `metadataTable`. Fix is a few list entries; add a probe test.
- NEEDS-HUMAN [impl] — **The gating C4-ci red is caused by this patch, not the
  environment**: `typos` flags `mis-decoding` in the new comment at
  `crates/gateway-s3/src/lib.rs:556` (verified by running `typos` on the target: the sole
  finding, `556:81 mis → miss/mist`). One-word reword (or a `typos.toml` exception)
  un-reds the only failed deterministic gate; nothing deeper hides behind it.
- NEEDS-HUMAN [impl] — **v1 `NextMarker` emits the wrong value and the marker resume can
  double-emit a group across implementations.** `render_list_v1` emits `page.next_key` —
  the last *consumed* raw key (`lib.rs:770,800`, e.g. `a/1`) — where AWS's `NextMarker` is
  the last item *returned* (the common prefix `a/`). Correspondingly, the resume filter
  `o.key > marker` (`lib.rs:491`) treats a marker naming a common prefix as an ordinary
  key: concrete case — keys `{a/1,a/2,b/1}`, `GET /bucket?delimiter=/&marker=a/` re-emits
  `CommonPrefixes a/` where AWS (whose own NextMarker would be `a/`, and which cannot
  infinite-loop on it) skips the whole group. Self-chaining with *our* markers works
  (proven by the tests), but a client resuming from a stored last-CommonPrefix — the
  documented AWS v1 pattern — receives duplicates. The test
  `crates/server/tests/s3_list_objects.rs:414-438` asserts only `next_marker().is_some()`,
  never its value, so it locks the deviation in unnoticed.

Attempted and could NOT refute:
- Pagination correctness under truncation with delimiter (group-consume/resume,
  `lib.rs:465-535`): probed max-keys=1 chaining, group-at-budget-boundary, resume exactly
  at end-of-list — token = last *consumed* key makes mid-group resume unreachable for a
  conforming v2 client; no double-emit found (test at `s3_list_objects.rs:466` is real).
- Token robustness: empty `continuation-token=` → `base64_decode` rejects empty input
  (`crates/gateway-s3/src/checksum.rs:180`) → 400, not a silent restart; non-canonical
  base64 rejected; percent-encoded `+`/`=` in tokens decode correctly.
- Percent-encoded subresource dodge on the bucket route (`?%61cl`, `?upload%73`) — closed
  by `unsupported_subresource_decoded` (`lib.rs:380`).
- Bucket/`foobar` prefix spill — fenced by the trailing `/` in the scan prefix
  (`crates/server/src/lib.rs:473`); pending/torn inodes correctly excluded (`:492-497`).
- Note (pre-existing, not this diff's defect): a percent-encoded `/` in the *bucket*
  segment (`GET /bu%2Fcket`) decodes to `bu/cket` and aliases into bucket `bu`'s dirent
  namespace; listing merely makes the existing flat-encoding aliasing observable. The
  bucket-record read fences the listing itself (404). Belongs with #511 name validation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the local affected-path history plus the brief's rejected-iteration carry-forward adequately discharges prior art — merged history showed no equivalent listing implementation, but closed/rejected remote work cannot be mechanically settled from the supplied artifacts, which matters for duplicate or superseded contribution risk (`crates/gateway-s3/src/lib.rs:539`).
- [ ] T5 Judgment — Decide whether shipping listing before #511/backfill is acceptable — production has no marker writer yet, so live listings return `NoSuchBucket` despite the feature being correct in marker-seeded tests, which matters to immediate user utility (`crates/server/src/lib.rs:463`).
- [ ] Validation — fitness-to-purpose — Decide whether the in-process SDK evidence is sufficient for the promised client interoperability — after fixing CI, seed/backfill `bucket:<name>`, launch the gateway, run `aws s3 ls s3://<bucket>` and `aws s3 sync <dir> s3://<bucket>`, and confirm complete, non-duplicated listings because real AWS CLI acceptance was not exercised (`crates/server/tests/s3_list_objects.rs:140`).
- [ ] **Denylist gap: `GET /bucket?versioning` is silently answered with a
- [ ] **The gating C4-ci red is caused by this patch, not the
- [ ] **v1 `NextMarker` emits the wrong value and the marker resume can
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
- Iteration delta (if iterating): Fix all three adversary refutations and clear the gating C4-ci red: 1. C4-ci / typos: reword `mis-decoding` in the comment at crates/gateway-s3/src/lib.rs:556 (or record a deliberate typos.toml exception) so `cargo xtask ci` is green — the sole gate finding, caused by this patch. 2. Denylist gap: add `versioning`, `intelligent-tiering`, `ownershipControls`, `policyStatus`, `metadataTable` to UNSUPPORTED_SUBRESOURCES (crates/gateway-s3/src/lib.rs:330-361) so a bucket subresource GET answers 501, never a listing document; add a probe test (e.g. stock-SDK get_bucket_versioning → clean 501). 3. v1 NextMarker: emit the last *returned* item (the common prefix when a group was the last entry), not the last *consumed* raw key (render_list_v1 / lib.rs:770,800), and make the marker resume skip a whole group when the marker names a common prefix (resume filter at lib.rs:491) so a client resuming from a stored last-CommonPrefix never receives duplicates. Strengthen the test at crates/server/tests/s3_list_objects.rs:414-438 to assert the NextMarker VALUE, not just is_some(). Mainline is otherwise sound (red→green independently corroborated; pagination/token/encoding probes unrefuted) — keep the approach, fix only the above.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
