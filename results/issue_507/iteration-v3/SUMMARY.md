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

Review of issue #507: implement bucket-scoped S3 ListObjectsV2 and v1 listings with filtering, grouping, pagination, bucket-existence, and XML/error compatibility.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-ready: wire-visible success and failure cases, Alpha scan bound, and deferred live bucket creation are explicit in `brief.md:9`. |
| C2 Reproduction (red pre-fix) | PASS | A clean committed-base clone with only the added wire test compiled and ran, then failed 0/15 with bucket GET returning `400 InvalidRequest`, grounding the pre-fix symptom at `crates/server/tests/s3_list_objects.rs:139`. |
| C3 Change | PASS | The change stays within the declared cross-crate seam, routing, storage scan, and wire-test surfaces; bucket listing dispatch is isolated from object paths at `crates/gateway-s3/src/lib.rs:1283`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept independently confirmed 0/15→15/15 red→green plus fmt/clippy/build/tests/typos/docs/machete green without a completed advisory audit — `cargo deny check` alone could not acquire its read-only host lock, so the full gate is provisional (`crates/server/tests/s3_list_objects.rs:139`). |
| C5 Causal adequacy | PASS | The former bucket-path rejection is replaced by a distinct bucket route and real listing implementation, not a capability probe or guard around the old failure (`crates/gateway-s3/src/lib.rs:1278`). |
| T1 Structure | PASS | Protocol-neutral listing data remains in gateway-core while S3 grouping and pagination remain in the wire crate, preserving the intended boundary (`crates/gateway-core/src/lib.rs:83`). |
| T2 Shape | PASS | The combined page model exposes separate consumed-key and returned-item resume points, which is the shape needed for v2 tokens and v1 delimiter markers (`crates/gateway-s3/src/lib.rs:459`). |
| T3 Runtime | PASS | The in-process stock-SDK suite passed all 15 runtime cases, including sorted metadata, prefix/delimiter, chained pages, escaping, absent/empty buckets, invalid tokens, and v1 resume (`crates/server/tests/s3_list_objects.rs:139`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether prior art is adequately cleared — affected-path merged history was inspected and showed no earlier ListObjects implementation, but closed/rejected work was not mechanically available, so duplication risk is not fully discharged (`crates/gateway-s3/src/lib.rs:588`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether Alpha may ship listing before #511/backfill makes bucket markers available in live stacks — otherwise the correct marker fence makes every production listing return `NoSuchBucket` (`crates/server/src/lib.rs:462`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether real-client browsing is fit for release — after seeding/backfilling `bucket:<name>` (or landing #511), run the gateway and verify `aws --endpoint-url http://HOST:PORT s3 ls s3://BUCKET` and `aws --endpoint-url http://HOST:PORT s3 sync SRC s3://BUCKET/PREFIX`; the automated SDK wire coverage is green but AWS CLI behavior was not exercised (`crates/server/tests/s3_list_objects.rs:83`). |

### Advisory — adversary

# Adversarial review — issue #507 (list-objects-v2), iteration 3

Verdict: the red→green evidence is **genuine** (independently reproduced), but one
refutation of the fix **landed**, demonstrated end-to-end against the patched gateway.

## Refutation that landed

- NEEDS-HUMAN [impl] — **A client-chosen resume point inside a delimiter group makes the
  group's remaining keys invisible.** The group-skip predicate
  `resume_after.is_some_and(|r| cp.as_str() <= r)` (`crates/gateway-s3/src/lib.rs:537`)
  treats *any* resume value ≥ the common prefix as "group already consumed". That is
  correct for the two values the server itself issues (v2 token = the group's last raw
  key; v1 `NextMarker` = the common prefix), but `start-after`
  (`crates/gateway-s3/src/lib.rs:682`) and the v1 `marker` are **arbitrary client-chosen
  keys**. Concrete failing case, run against the patched gateway over the wire with the
  stock SDK: bucket `{a/1, a/2, b}`, `GET /bucket?list-type=2&delimiter=/&start-after=a/1`
  → this patch returns `Contents=[b]`, `CommonPrefixes=[]`; AWS applies `start-after` to
  the raw keyspace *before* rollup, so `a/2 > a/1` survives and rolls up →
  `CommonPrefixes=[a/], Contents=[b]`. Same divergence for v1
  `?delimiter=/&marker=a/1` (probe output: `cps=[] keys=["b"]` for both). Net effect:
  `a/2` is neither listed nor represented by a rollup on *any* page — silent data
  invisibility, the exact failure class the carry-forward's start-after item was meant to
  close ("clients re-receive consumed keys" was fixed by creating "clients never see
  unconsumed keys"). The patch's own `start-after` test
  (`crates/server/tests/s3_list_objects.rs:558-582`) only exercises a flat, no-delimiter
  bucket, so it cannot catch this. Fix is local to `compute_page`: skip the whole group
  only when the resume point equals the common prefix or is ≥ the group's last raw key;
  otherwise filter the group's raw keys individually and emit the rollup if any survive.

## Refutations attempted that failed (evidence corroborated)

- Re-ran the red leg myself (base production files + the added test, scratch checkout):
  compiles clean, **15/15 fail by assertion** (no compile-error red). Re-ran the green
  leg: **15/15 pass**. The test drives the shipping HTTP surface through a stock
  `aws-sdk-s3` client over a real loopback listener — no parallel re-implementation, no
  mocked-away defect; the SDK's own paginator is the chaining oracle and the ETag oracle
  is an independent SHA-256. The `C4-verify: pass` claim in check-gates.json is warranted.
- Empty continuation token (`continuation-token=`, a value the server never issues): probed
  → server answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:668-675`), not a
  silent restart. Could not refute.
- Percent-encoded bucket subresource (`?%61cl`, `?upload%73`) and the five bucket-spelled
  subresources (`versioning` etc.): fence holds (`unsupported_subresource_decoded`,
  `crates/gateway-s3/src/lib.rs:382-395`, denylist `:328-371`), locked by the
  `get_bucket_versioning` probe test. Could not refute.
- `max-keys=0`, delimiter+max-keys chaining under truncation (the prior centerpiece gap),
  v1 `NextMarker` value = common prefix with whole-group resume skip: all now locked by
  wire-level tests that I confirmed go red on base. Could not refute.
- `typos` and `cargo fmt --check` on the patched tree: clean — partial independent
  corroboration of the gating `C4-ci: pass` (full `xtask ci` not re-run here; the gate is
  deterministic and green).

## Minor observations (no adjudication demanded)

- The object path deliberately keeps **raw** subresource matching (`unsupported_subresource`,
  `crates/gateway-s3/src/lib.rs:377-378`) with the new claim that a raw match "is
  sufficient" there. A doubly-encoded `PUT /b/k?part%4Eumber=1&upload%49d=x` bypasses the
  fence and executes as a plain destructive object PUT. Only a deliberately-encoding,
  fully-credentialed client can trigger it (real SDKs never encode these keys, and such a
  client could plain-PUT anyway), so I do not press it — but the comment's claim is
  stronger than the code warrants.
- `BucketRecord` (`crates/core/src/metadata.rs:346`) is added but referenced by no
  production code in this patch — `list_container` checks marker *presence* only
  (`crates/server/src/lib.rs:461-466`) and the test seeds hand-written JSON
  (`crates/server/tests/s3_list_objects.rs:59-63`) that nothing validates against the
  struct. Speculative API for #511; drift risk if #511's shape differs, but harmless here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept independently confirmed 0/15→15/15 red→green plus fmt/clippy/build/tests/typos/docs/machete green without a completed advisory audit — `cargo deny check` alone could not acquire its read-only host lock, so the full gate is provisional (`crates/server/tests/s3_list_objects.rs:139`).
- [ ] T4 Contribution — Decide whether prior art is adequately cleared — affected-path merged history was inspected and showed no earlier ListObjects implementation, but closed/rejected work was not mechanically available, so duplication risk is not fully discharged (`crates/gateway-s3/src/lib.rs:588`).
- [ ] T5 Judgment — Decide whether Alpha may ship listing before #511/backfill makes bucket markers available in live stacks — otherwise the correct marker fence makes every production listing return `NoSuchBucket` (`crates/server/src/lib.rs:462`).
- [ ] Validation — fitness-to-purpose — Decide whether real-client browsing is fit for release — after seeding/backfilling `bucket:<name>` (or landing #511), run the gateway and verify `aws --endpoint-url http://HOST:PORT s3 ls s3://BUCKET` and `aws --endpoint-url http://HOST:PORT s3 sync SRC s3://BUCKET/PREFIX`; the automated SDK wire coverage is green but AWS CLI behavior was not exercised (`crates/server/tests/s3_list_objects.rs:83`).
- [ ] **A client-chosen resume point inside a delimiter group makes the

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the adversary's landed refutation: a client-chosen resume point inside a delimiter group (v2 `start-after` or v1 `marker` — arbitrary keys, unlike server-issued tokens) makes the group's remaining keys silently invisible on every page. Reproduced end-to-end: bucket {a/1, a/2, b}, `?list-type=2&delimiter=/&start-after=a/1` returns Contents=[b], CommonPrefixes=[] — AWS returns CommonPrefixes=[a/], Contents=[b]; same for v1 `?delimiter=/&marker=a/1`. Cause: the group-skip predicate `resume_after.is_some_and(|r| cp.as_str() <= r)` (crates/gateway-s3/src/lib.rs:537) treats any resume value ≥ the common prefix as "group consumed"; that is only valid for the two server-issued values. Fix locally in `compute_page`: skip the whole group only when the resume point equals the common prefix or is ≥ the group's last raw key; otherwise filter the group's raw keys individually and emit the rollup if any survive. Add wire tests for start-after and v1 marker landing *inside* a delimiter group (the existing start-after test is flat/no-delimiter and cannot catch this). While in there, address the two minor advisory observations: soften/correct the "raw match is sufficient" comment on the object-path subresource fence, and either use or drop the unused `BucketRecord` struct (crates/core/src/metadata.rs:346).
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
