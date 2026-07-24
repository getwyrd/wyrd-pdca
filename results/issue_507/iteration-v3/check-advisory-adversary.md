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
