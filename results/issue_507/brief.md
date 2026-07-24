# Design proposal — issue 507 / list-objects-v2 (iteration-5 plan amendment)

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.
> This is the AMENDED brief after the iteration-4 iterate-plan sign-off: the
> approach is accepted ("mainline sound, keep it"); two scope-level defects must
> be fixed. Start from the preserved previous attempt — see §Iteration-5 basis.

- **Slug:** list-objects-v2
- **Kind:** enhancement (design proposal)
- **Goal:** ListObjectsV2 (`GET /bucket?list-type=2`) with `prefix`, `delimiter`
  (common-prefixes), `max-keys`, `start-after`, `continuation-token` pagination, and
  `encoding-type=url`, plus a thin ListObjects v1 compat shim (`GET /bucket`,
  `marker`-based), instead of today's 400 `InvalidRequest`. Unblocks `aws s3 ls`,
  `aws s3 sync`, boto3, rclone/restic/s3fs browsing — NOTE: botocore auto-injects
  `encoding-type=url` into every ListObjects/V2 request (verified empirically,
  aws-cli 2.36.1 `--debug`), so `encoding-type=url` is load-bearing for the stock
  clients this Goal names, not an optional extra.
- **Success criterion:** against the in-process loopback S3 gateway with objects stored
  under a bucket whose `bucket:{name}` marker exists in the `MetadataStore`: a signed
  bucket-scoped `GET /bucket?list-type=2` returns a well-formed `<ListBucketResult>` whose
  `<Contents>` are exactly the bucket's keys in lexicographic order (with correct `Size`
  and `ETag`); `prefix` filters correctly; `delimiter=/` rolls nested keys into
  `<CommonPrefixes>`; with `max-keys` smaller than the key count, pages chain via
  `IsTruncated`/`NextContinuationToken`/`continuation-token` until every key is returned
  exactly once (including under `delimiter`, with no common prefix double-emitted);
  `max-keys=0` returns an empty untruncated page; listing a bucket with NO marker record
  returns 404 `NoSuchBucket`; an empty bucket WITH a marker lists as empty 200 (not 404);
  an invalid `continuation-token` answers 400; a v1 `GET /bucket` returns a v1
  `<ListBucketResult>` (`Marker`/`NextMarker`-based, `NextMarker` = the last RETURNED
  item, resumable without duplicates); an XML-special key (`&`, `<`, quote) round-trips
  correctly escaped; **`encoding-type=url` is implemented**: the response carries
  `<EncodingType>url</EncodingType>` and URL-encoded `Key`/`Prefix`/`Delimiter`/
  `CommonPrefixes` (and `StartAfter`/`Marker`/`NextMarker` echoes) such that a key like
  `a&b/c d` returns as `a%26b/c%20d` (v2 and v1), with `ContinuationToken`/
  `NextContinuationToken` untouched (a returned token resumes verbatim), while an
  `encoding-type` value other than `url` answers 400 `InvalidArgument` — asserted on the
  `<Code>InvalidArgument</Code>` BODY, not the status alone (the base itself answers 400
  `InvalidRequest` to every bucket GET, so a status-only assertion is vacuously green on
  the red leg); when `continuation-token` and `start-after` are BOTH sent (the stock
  paginator resends `StartAfter` with every token), the token wins and `start-after` is
  ignored (AWS semantics); **client-chosen resume points respect raw-key
  semantics**: v2 `start-after` (or v1 `marker`) landing strictly INSIDE a delimiter
  group still emits the group's rollup when keys survive, and v2 `start-after` EXACTLY
  EQUAL to a common prefix (`start-after=a/`) still returns that `<CommonPrefixes>` entry
  (AWS applies start-after to the raw keyspace before rollup) — while the server-issued
  v1 `NextMarker` resume (the common prefix itself) skips the consumed group without
  duplicates; bucket subresource GETs (`?acl`, `?versioning`, `?policy`, …) still answer
  501, matched on percent-DECODED query keys. Asserted by
  `crates/server/tests/s3_list_objects.rs`, red on the base (bucket-only GET → 400).
  The test drives the wire only (SDK/HTTP; no new production symbol imported), so the
  C4-verify red leg fails by assertion, not compile error.
- **Falsifiability:** RED is producible in-process: on the base the bucket-only path
  is rejected before any listing logic — `split_bucket_key`
  (`crates/gateway-s3/src/lib.rs:371-377`) returns `None` for `/bucket`, and the caller
  answers 400 `InvalidRequest` (`lib.rs:788-795`) — so every assertion in the new test
  fails. C4-verify (revert production, keep the ADDED test) reproduces it. **Base
  correction (batch adversary, plan-review-adversary-507-510.md):** this bundle is
  **wave 0** of the 507–510 batch — the deterministic scheduler (recomputed from the
  batch briefs' dep/conflict edges, `waves.py:140-196`) yields
  `[507], [509], [510], [508]`; 507 is wave 0 under any reading — so its verify base is
  the brief's branch target
  (`origin/main`, currently 07d0244), NOT a folded `$PDCA_VERIFY_BASE` (that applies to
  508/509/510). The v4 brief's "wave≥1" claim was wrong; nothing else changes:
  `run-verify.sh` falls back to the brief base when the driver exports no verify base
  (`_resolve_base_ref`, engine/scripts/run-verify.sh:186-192). Red was independently
  corroborated in all four prior iterations (v4: 17/17 red, 17/17 green).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 508, 510
- **Ordering note:** unchanged from v4 — ADR-0046 (Accepted on main, commit ea0ecf2,
  `docs/design/adr/0046-bucket-model-real-namespace.md`) is normative. All batch peers
  edit the same `crates/gateway-s3/src/lib.rs` dispatch → conflict edges with 508/510;
  509 declares `Depends on: 507` (builds on this slice's bucket-scoped routing split).
  Scheduler places 507 in wave 0 (adversary-verified). #511 (bucket operations, writes
  the `bucket:{name}` marker) stays outside this batch — see Open questions.
- **Difficulty:** high
- **Scope:** route bucket-scoped GETs (relax the object-path guard for GET on `/bucket`),
  implement ListObjectsV2 + the v1 shim end-to-end: a protocol-neutral listing seam
  crossing `gateway-core` (S3 vocabulary stays in the S3 crate — ADR-0010 / ADR-0046
  decision 6), a `wyrd-server` implementation scanning the per-bucket dirent prefix of the
  flat `{bucket}/{key}` encoding, delimiter/common-prefix computation over the key
  remainder, deterministic ordering + pagination on top of the unordered `scan`, the
  `NoSuchBucket` record read, hand-built XML emission (no new dependency), `start-after`
  with raw-keyspace semantics, **and `encoding-type=url` (moved INTO scope by the
  iteration-4 sign-off — the previous 501 rejection refuses every stock aws-cli / boto3 /
  rclone listing because botocore always sends it)**. / out of scope:
  CreateBucket/HeadBucket/ListBuckets/DeleteBucket and writing `bucket:{name}` markers
  (#511); bucket-existence preconditions on object PUT/GET/DELETE (#511); `fetch-owner`
  MAY be omitted if the stock SDK paginator does not require it (v4 omitted it and passed
  the SDK suite — state the status quo in build-notes); changing `MetadataStore::scan`'s
  trait contract across backends (see Design).
- **External dependencies:** none — base toolchain; the test runs in-process on the
  redb-in-memory + fs-tempdir + loopback stack. **Marker seeding (adversary-corrected,
  implemented in v4):** `Gateway`'s metadata field is private
  (`crates/server/src/lib.rs:59-61`) and the store is MOVED into `Gateway::new`
  (`crates/server/src/lib.rs:91-94`), so seed the `bucket:{name}` record by
  building the `RedbMetadataStore` first, `commit`ing the marker batch on it directly, and
  only THEN passing it to `Gateway::new` (the v4 test's `seed_bucket`/`start_gateway`
  pattern). Do not add a production accessor for the test. Off-Check manual acceptance
  (`aws s3 ls`, `aws s3 sync`) uses the AWS CLI, registered as doctor row
  "aws cli (S3 gateway round-trip)".
- **Test file:** crates/server/tests/s3_list_objects.rs   (NEW `*/tests/*.rs` file —
  `run-verify.sh --classify` dry-run confirms an added `crates/server/tests/*.rs` is the
  red→green discriminator)
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites to mirror: the loopback + SigV4 test harness
  `crates/server/tests/s3_object_metadata.rs:43-92` (`build_gateway`/`start_gateway`/
  `signed_headers`) and the stock-SDK client `crates/server/tests/s3_gateway_cluster.rs:98`
  (`sdk_client` — its `list_objects_v2()` paginator is the ideal oracle; NOTE the
  paginator does not inject `encoding-type` by DEFAULT — unlike botocore — which is why
  four green C4 rounds missed finding 1; the SDK can send it via `.encoding_type(...)`,
  but the encoding-type tests must drive raw signed HTTP as the stronger oracle, signing
  the canonical query — see Delta 1); the seam pattern is `head_object` on
  `ObjectGateway` (`crates/gateway-core/src/lib.rs:139-177`, method at `:172`, impl
  `crates/server/src/lib.rs:394`); the dirent keyspace and flat `{bucket}/{key}` encoding
  live in `crates/server/src/lib.rs` (object-key composition visible at the wire layer,
  `crates/gateway-s3/src/lib.rs:801-805`); `scan`'s contract (whole-prefix,
  order-unspecified, complete-or-`Err`) is `crates/traits/src/lib.rs:772-776`; `SCAN_CAP`
  is the const in `crates/traits/src/lib.rs:286` (enforced in the redb `scan` at
  `crates/metadata-redb/src/lib.rs:119-125`). For the two amendment deltas, the defect
  sites in the PREVIOUS attempt: the `encoding-type` 501 rejection + factually-wrong
  comment ("Stock aws-cli ls/sync do not send it") in `list_objects`
  (results/issue_507/iteration-v4/patch.diff, hunk adding `lib.rs` ~post-apply :634-641),
  and the group-consumed predicate `r == cp.as_str() || r >= last_raw` in `compute_page`
  (same patch, post-apply `lib.rs` ~:556-557).
- **Disposition hint:** new-feature

## Motivation

Without listing, every browsing client fails: `aws s3 ls`, `aws s3 sync`, rclone, restic,
s3fs. It is the largest single gap after multipart in the 0.1-Alpha S3 epic (milestone 16).
Four Do/Check iterations converged on a sound mainline (v4: 17/17 red→green, denylist/
token/auth adversary probes unrefuted); sign-off rejected on two scope-level defects that
this amended brief pulls into the contract.

## Iteration-5 basis — START FROM THE PREVIOUS ATTEMPT

The full previous attempt is preserved in `iteration-v4/` (`patch.diff`,
`build-notes.md`, check artifacts; earlier rounds in `iteration-v1..v3/`). The sign-off
verdict: **keep the approach — the mainline is sound**. Re-apply `iteration-v4/patch.diff`
as the starting point (it targets main @ 07d0244, the current base) and make ONLY the
changes below plus their tests. Do NOT regress the accumulated iteration-1..4 fixes, all
already present in v4 and locked by its 17 tests: max-keys=0 empty-untruncated;
percent-DECODED subresource denylist matching (incl. `versioning`,
`intelligent-tiering`, `ownershipControls`, `policyStatus`, `metadataTable`);
delimiter+max-keys chaining without double-emitted prefixes; v1 `NextMarker` = last
RETURNED item (common prefix) with duplicate-free group resume; inside-group client
resume rolls up survivors; `list-type` ≠ 2 → 400 `InvalidArgument`; typos-clean comments
(`cargo xtask ci` runs `typos` — iteration-2 lesson: "mis-decoding" tripped it).

**Delta 1 — implement `encoding-type=url` (scope change).** Replace the 501 rejection
(v4 `list_objects`, post-apply `lib.rs:634-641`) with the real behaviour:
`encoding-type=url` on v2 AND v1 returns `<EncodingType>url</EncodingType>` and
URL-encodes the returned `Key`, `Prefix`, `Delimiter`, `CommonPrefixes`→`Prefix`, and
`StartAfter` (v2) / `Marker`+`NextMarker` (v1) echo values; `NextContinuationToken`/
`ContinuationToken` are opaque and stay untouched. An `encoding-type` value other than
`url` answers 400 `InvalidArgument` (AWS behaviour). Encoding is RENDER-TIME ONLY —
filtering, grouping, resume comparison, and token payloads all operate on raw keys.
Encoding shape: percent-encode the UTF-8 bytes; the ONLY characters left literal are the
unreserved set `A-Za-z0-9`, `-`, `_`, `.`, `~`, plus `/`; space is NOT literal — it
encodes as `%20`, never `+` (botocore URL-decodes the values; `a&b/c d` → `a%26b/c%20d`
is the oracle). Delete the factually-wrong comment
claiming stock aws-cli does not send it. Replace the now-obsolete v4 test
`list_v2_encoding_type_is_rejected_not_silently_ignored` with raw-signed-HTTP wire tests
(the SDK paginator does not inject `encoding-type` by default — unlike botocore — which
is why four green C4 rounds missed this; the SDK CAN send it via `.encoding_type(...)`,
as v4's own test did, but raw HTTP is mandated as the stronger oracle: it asserts exact
wire bytes with no SDK decode layer in between): v2 and
v1 requests carrying `encoding-type=url` MUST assert EVERY encoded response element, not
just `Key` — with an encodable prefix/delimiter/resume value in play, lock the encoded
form of `Prefix`, `Delimiter`, `CommonPrefixes`→`Prefix`, `StartAfter` (v2) and
`Marker`/`NextMarker` (v1), assert the `<EncodingType>url</EncodingType>` element, AND
prove `NextContinuationToken` stays raw/opaque by resuming a truncated encoded listing
with the returned token verbatim; one test asserts `encoding-type=broken` → 400 whose
assertion MUST include the `<Code>InvalidArgument</Code>` body, not the status alone —
the base answers 400 (`InvalidRequest`) to every bucket GET, so a status-only assertion
is vacuously green on the C4-verify red leg (v4's invalid-token test already asserts the
body code — mirror it). SigV4 trap on the raw-HTTP path: the peer harness helper signs an
EMPTY query (`sign(method, path, "", …)`, `s3_object_metadata.rs:81-83`) — every new wire
test carries a query string, so pass the canonical (sorted, encoded) query as `sign`'s
query argument or the GREEN leg 403s on signature mismatch. Note #512's
aws-cli/boto3 harness will deterministically re-expose any skip here.

**Delta 2 — restrict the `r == cp` group-collapse to the v1 marker resume.** In
`compute_page` (v4 post-apply `lib.rs:556-557`), the predicate
`resume_after.is_some_and(|r| r == cp.as_str() || r >= last_raw)` collapses a whole
delimiter group whenever the resume value EQUALS the common prefix — but that equality
collapse is only valid for the server-issued v1 `NextMarker` (which names the CP by
design). A CLIENT-chosen v2 `start-after` exactly equal to a common prefix
(`start-after=a/`, the folder-marker workflow) must NOT collapse the group: AWS applies
start-after to the raw keyspace before rollup, so bucket `{a/1, a/2, b}` with
`?list-type=2&delimiter=/&start-after=a/` returns CommonPrefixes=[a/], Contents=[b] —
v4 returns CommonPrefixes=[] (reproduced on the wire at sign-off). Fix: pass a
resume-kind discriminator into `compute_page` and apply the `r == cp` clause ONLY on the
v1 `marker` path; the v2 resume relies on per-key filtering (`> r`) plus `r >= last_raw`
(a server-issued v2 token always encodes the last raw key, so it always satisfies
`>= last_raw` — no v2 behaviour is lost). Precedence when BOTH v2 resume params arrive
(adversary finding b — the stock SDK paginator resends `StartAfter` alongside every
`continuation-token`, so this is the NORMAL flow, not an edge): the token WINS and
`start-after` is ignored, per AWS semantics — a `max(token, start-after)` blend is wrong
(keys `{a,b,c,d}`, page 1 `start-after=a&max-keys=1` → token(b); page 2
`token(b)&start-after=d` must return `c`, not nothing). v4's first-page-only guard
(`start-after` consulted only when no token is present) already encodes this — keep it
explicit, and let the SDK-paginator-with-`start_after` test exercise it. Keep the v1
resume test (`list_v1_next_marker_is_common_prefix_and_resumes_without_double_emit`)
green. Add a wire test: v2 `start-after` exactly equal to a common prefix still emits
the rollup.

**Minor (SHOULD, from sign-off):** emit the `<StartAfter>` echo in `render_list_v2`
when the request carried `start-after` (URL-encoded under `encoding-type=url`).

## Design

Normative baseline: **ADR-0046** (Accepted). Object keys stay flat `{bucket}/{key}`
dirents under ROOT at Alpha, so listing scans the per-bucket dirent prefix;
`delimiter`/common-prefixes are computed over the key remainder after the bucket segment;
listing an absent bucket answers `NoSuchBucket` — read the `bucket:{name}` record first
(a plain `get`).

Key decisions (all implemented in v4; restated as the standing contract):

1. **Routing.** `split_bucket_key` rejecting bucket-only paths is load-bearing for object
   verbs; do not blanket-relax it. Split dispatch into bucket-scoped vs object-scoped
   before the object-path guard (base `crates/gateway-s3/src/lib.rs:788-795`), routing
   bucket-scoped GET to the listing handler. The bucket path MUST still consult the
   subresource denylist (base `lib.rs:328-366`, extended per iteration 2) on
   percent-DECODED query keys before treating a bucket GET as a listing; `GET /bucket?acl`
   etc. stay 501. Other bucket-scoped methods keep today's behaviour — #511 and 509
   extend the split later.
2. **Seam.** `gateway-core` stays free of S3 vocabulary. Grouping and paging happen in
   ONE place — the wire layer (S3's `max-keys` counts `Contents` + `CommonPrefixes`
   combined). The seam returns the container's complete lexicographically-sorted key set
   with `(key, size, etag, modified)`; the S3 layer computes delimiter groups, the
   combined `max-keys` slice, resume filtering, the continuation token — and, new,
   the render-time `encoding-type=url` projection.
3. **Pagination over an unordered, capped `scan`.** Materialize the per-bucket dirent
   scan, sort lexicographically, page wire-side. Two resume values exist and MUST stay
   distinct (v4's `next_key`/`next_marker` split): the v2 token payload is the last raw
   CONSUMED key; the v1 `NextMarker` is the last RETURNED item (the common prefix for a
   rollup). The `r == cp` group-collapse belongs to the v1 marker path only (Delta 2).
   Invalid/undecodable token → 400; `max-keys` defaults to 1000 and clamps; a bucket
   exceeding `SCAN_CAP` surfaces `scan`'s complete-or-`Err` contract
   (`crates/traits/src/lib.rs:772-776`) as an S3 500-class error, never silent truncation.
4. **XML.** `<ListBucketResult>` by string building (no XML crate — a new dependency is a
   human-gated decision). XML-escape everything; under `encoding-type=url`, URL-encode
   per Delta 1 THEN XML-escape (URL-encoding leaves no XML-specials in practice, but the
   escape stays as the outer invariant).
5. **Tests.** Seed `bucket:{name}` markers store-first (External dependencies field);
   drive assertions with the stock `aws-sdk-s3` client where it covers the surface (its
   paginator exercises token chaining for real) and raw signed HTTP where it does not
   (`encoding-type`, v1 shim, malformed params).

## Alternatives considered

- **Synthesized bucket existence** (bucket exists iff ≥1 dirent): foreclosed by ADR-0046
  decision 7.
- **A paged/streaming `scan` trait change now**: touches every metadata backend in the
  same change as a user-facing feature; the materialized-sort answer satisfies Alpha's
  `SCAN_CAP` bounds without the cross-backend blast radius.
- **An XML dependency (quick-xml)**: pulls a new license/dependency audit (ADR-0003) for
  what a string builder does adequately.
- **Keeping the `encoding-type` 501 rejection** (v4's stance): refuted at sign-off —
  botocore sends it unconditionally, so the rejection refuses the exact clients the Goal
  names; a repo-wide tracker search confirmed NO other issue owns `encoding-type`, so it
  cannot be punted.

## Impact & compatibility

Bucket-scoped GET changes from 400 to a real response — no client depends on the 400.
Until #511 lands CreateBucket, no production path writes `bucket:{name}` markers, so on a
live stack every listing answers `NoSuchBucket` until markers are backfilled (ADR-0046's
stated Alpha stance). The `aws s3 ls` end-to-end acceptance is demonstrable off-Check
only after a marker backfill or #511 — but with Delta 1, a backfilled stack now actually
works with stock aws-cli, which v4 did not.

## Open questions

- **Sequencing vs #511** (pre-declared): ADR-0046 assigns the `NoSuchBucket` record-read
  to this issue while #511 (outside this batch) writes the markers. The record read is
  ADR-normative and lands here; a live stack backfills or waits for #511.
- `fetch-owner`: v4 omitted it and the SDK suite passed; Do states the status quo in
  build-notes. (`start-after` and `encoding-type` are no longer open — both are in scope.)

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Mainline accepted — do NOT change the approach or regress the 23-test suite. One targeted defect to fix (adversary finding, §6 item 4): `bucket_scoped_path` (crates/gateway-s3/src/lib.rs:427-436) uses `trim_start_matches('/')`, which strips ALL leading slashes, so the `Some(_) => None` "empty bucket segment (`//…`) → neither" arm is unreachable and its comment is false; a signed `GET //bucket?list-type=2` answers 200 instead of an error. Fix is a one-liner: use a single `strip_prefix('/')` (making `//bucket` reject as intended) or delete the dead arm and correct the comment — and add a wire test locking the chosen behaviour. Already resolved at this sign-off (do not re-litigate): the off-Check aws-cli/boto3 acceptance and its marker external-dependency are explicitly deferred until at least #511 lands bucket-marker writes (§6 items 3 and 5 ticked). The C4 cargo-deny independent-reproduction and T4 closed-PR-overlap items were not grounds for iteration and remain for the next sign-off.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
