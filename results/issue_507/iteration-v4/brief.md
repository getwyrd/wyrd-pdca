# Design proposal — issue 507 / list-objects-v2

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.

- **Slug:** list-objects-v2
- **Kind:** enhancement (design proposal)
- **Goal:** ListObjectsV2 (`GET /bucket?list-type=2`) with `prefix`, `delimiter`
  (common-prefixes), `max-keys`, and `continuation-token` pagination, plus a thin
  ListObjects v1 compat shim (`GET /bucket`), instead of today's 400 `InvalidRequest`.
  Unblocks `aws s3 ls`, `aws s3 sync`, rclone/restic/s3fs browsing.
- **Success criterion:** against the in-process loopback S3 gateway with objects stored
  under a bucket whose `bucket:{name}` marker exists in the `MetadataStore`: a signed
  bucket-scoped `GET /bucket?list-type=2` returns a well-formed `<ListBucketResult>` whose
  `<Contents>` are exactly the bucket's keys in lexicographic order (with correct `Size`
  and `ETag`); `prefix` filters correctly; `delimiter=/` rolls nested keys into
  `<CommonPrefixes>`; with `max-keys` smaller than the key count, pages chain via
  `IsTruncated`/`NextContinuationToken`/`continuation-token` until every key is returned
  exactly once; listing a bucket with NO marker record returns 404 `NoSuchBucket`; and a
  v1 `GET /bucket` returns a v1 `<ListBucketResult>` (`Marker`-based); an XML-special
  key (`&`, `<`, quote) round-trips correctly escaped; an empty bucket WITH a marker
  lists as empty 200 (not 404); an invalid `continuation-token` answers 400. Asserted by
  `crates/server/tests/s3_list_objects.rs`, red on the wave base (bucket-only GET → 400).
  The test drives the wire only (SDK/HTTP; no new production symbol imported), so the
  C4-verify red leg fails by assertion, not compile error.
- **Falsifiability:** RED is producible in-process: on the wave base the bucket-only path
  is rejected before any listing logic — `split_bucket_key`
  (`crates/gateway-s3/src/lib.rs:371-377`) returns `None` for `/bucket`, and the caller
  answers 400 `InvalidRequest` (`lib.rs:788-795`) — so every assertion in the new test
  fails. C4-verify (revert production, keep the ADDED test) reproduces it; this bundle is
  wave≥1 in the batch, and `run-verify.sh` honours the driver's `$PDCA_VERIFY_BASE`
  (folded integration branch) with the right precedence (`_resolve_base_ref`,
  engine/scripts/run-verify.sh:186-192), so the red/green legs run against the correct base.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 508, 510
- **Ordering note:** the bucket-model dependency named in the tracker is RESOLVED —
  ADR-0046 is Accepted on main (commit ea0ecf2, `docs/design/adr/0046-bucket-model-real-namespace.md`)
  and is normative for this issue. (506/HeadObject left the batch: re-landed on main as
  PR #607, issue closed.) All batch peers edit the same
  `crates/gateway-s3/src/lib.rs` dispatch → conflict edges with 508/510. 509
  (bulk DeleteObjects) declares `Depends on: 507` because it builds on the bucket-scoped
  routing split this slice introduces. #511 (bucket operations, writes the `bucket:{name}`
  marker) is open and OUTSIDE this batch — see Open questions.
- **Difficulty:** high
- **Scope:** route bucket-scoped GETs (relax the object-path guard for GET on `/bucket`),
  implement ListObjectsV2 + the v1 shim end-to-end: a protocol-neutral listing seam
  crossing `gateway-core` (S3 vocabulary stays in the S3 crate — ADR-0010 / ADR-0046
  decision 6), a `wyrd-server` implementation scanning the per-bucket dirent prefix of the
  flat `{bucket}/{key}` encoding, delimiter/common-prefix computation over the key
  remainder, deterministic ordering + pagination on top of the unordered `scan`, the
  `NoSuchBucket` record read, and hand-built XML emission (no new dependency). / out of
  scope: CreateBucket/HeadBucket/ListBuckets/DeleteBucket and writing `bucket:{name}`
  markers (#511); bucket-existence preconditions on object PUT/GET/DELETE (#511);
  `encoding-type=url`; `fetch-owner`/`start-after` MAY be omitted if stock aws-cli `ls`
  does not require them (state which in build-notes); changing `MetadataStore::scan`'s
  trait contract across backends (see Design).
- **External dependencies:** none — base toolchain; the test runs in-process on the
  redb-in-memory + fs-tempdir + loopback stack. **Marker seeding (corrected by adversarial
  review):** `Gateway`'s metadata field is private and the store is MOVED into
  `Gateway::new` (`crates/server/src/lib.rs:59-60`), so a post-construction
  `MetadataStore` handle does NOT exist — seed the `bucket:{name}` record by building the
  `RedbMetadataStore` first, `commit`ing the marker batch on it directly (it implements
  `MetadataStore`), and only THEN passing it to `Gateway::new`. Do not add a production
  accessor for the test. Off-Check manual acceptance (`aws s3 ls`, `aws s3 sync`)
  uses the AWS CLI, registered as doctor row "aws cli (S3 gateway round-trip)".
- **Test file:** crates/server/tests/s3_list_objects.rs   (NEW `*/tests/*.rs` file —
  `run-verify.sh --classify` dry-run confirms an added `crates/server/tests/*.rs` is the
  red→green discriminator)
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites to mirror: the loopback + SigV4 test harness
  `crates/server/tests/s3_object_metadata.rs:43-92` (`build_gateway`/`start_gateway`/
  `signed_headers`) and the stock-SDK client `crates/server/tests/s3_gateway_cluster.rs:98`
  (`sdk_client` — its `list_objects_v2()` paginator is the ideal oracle); the seam pattern
  to follow is `head_object` on `ObjectGateway` (`crates/gateway-core/src/lib.rs:139-177`,
  the `head_object` method at `:172`, and its `crates/server/src/lib.rs:266`-rooted impl,
  `head_object` at `:394` — on main since PR #607); the dirent keyspace and flat
  `{bucket}/{key}` encoding live in `crates/server/src/lib.rs` (the object-key composition
  is visible at the wire layer, `crates/gateway-s3/src/lib.rs:801-805`); `scan`'s contract
  (whole-prefix, order-unspecified, complete-or-`Err`) is `crates/traits/src/lib.rs:772-776`;
  `SCAN_CAP` is the const in `crates/traits/src/lib.rs:286` (re-exported redb `:46`,
  enforced in the redb `scan` at `crates/metadata-redb/src/lib.rs:119-125`).
- **Disposition hint:** new-feature

## Motivation

Without listing, every browsing client fails: `aws s3 ls`, `aws s3 sync`, rclone, restic,
s3fs. It is the largest single gap after multipart in the 0.1-Alpha S3 epic (milestone 16).

## Design

Normative baseline: **ADR-0046** (Accepted). Object keys stay flat `{bucket}/{key}`
dirents under ROOT at Alpha, so listing scans the per-bucket dirent prefix;
`delimiter`/common-prefixes are computed over the key remainder after the bucket segment;
listing an absent bucket answers `NoSuchBucket` — read the `bucket:{name}` record first
(a plain `get`).

Key decisions for Do:

1. **Routing.** `split_bucket_key` rejecting bucket-only paths is load-bearing for object
   verbs; do not blanket-relax it. Split dispatch into bucket-scoped vs object-scoped
   before the object-path guard (`crates/gateway-s3/src/lib.rs:788-795`), routing
   bucket-scoped GET to the listing handler. **The bucket path MUST still consult the
   subresource denylist** (`lib.rs:328-366`) before treating a bucket GET as a listing:
   only `list-type=2`, the v1 bare/`marker`/`prefix`/`delimiter`/`max-keys` forms, and
   benign params route to listing; `GET /bucket?acl` / `?policy` / `?versions` /
   `?location` / `?uploads` etc. stay 501 `NotImplemented` (adversarial finding: without
   this, `aws s3api get-bucket-acl` would receive a listing document, and 508's
   ListMultipartUploads red would be silently destroyed). Other bucket-scoped methods
   keep today's behaviour — #511 and 509 extend the split later.
2. **Seam.** `gateway-core` stays free of S3 vocabulary: a listing method speaking
   container/key terms. **Grouping and paging happen in ONE place — the wire layer**
   (codex finding: splitting them — server slices raw keys, wire groups — breaks S3's
   `max-keys`, which counts `Contents` + `CommonPrefixes` COMBINED, and can duplicate a
   common prefix across pages). So the seam returns the container's complete
   lexicographically-sorted key set with `(key, size, etag, modified)` — already
   materialized and `SCAN_CAP`-bounded, so returning it whole adds no new cost class —
   and the S3 layer computes delimiter groups, the combined `max-keys` slice, and the
   continuation token over that one sorted view.
3. **Pagination over an unordered, capped `scan`.** The primitive gap is explicitly this
   issue's scope (ADR-0046 consequences): at Alpha, materialize the per-bucket dirent scan
   (the dirent prefix of the flat encoding — derive it from how the object write path
   composes dirent keys in `crates/server/src/lib.rs`, do not invent a new shape), sort
   lexicographically, page wire-side per decision 2. The continuation token encodes the
   last item RETURNED — key or common prefix, both live in the same sort order (opaque,
   e.g. base64); an invalid/undecodable token answers 400 `InvalidArgument`, never a
   silent restart. `max-keys` defaults to 1000 and clamps to it. A bucket whose dirent
   count exceeds `SCAN_CAP` surfaces `scan`'s error mapped to an S3 500-class error rather
   than a silently truncated listing — the complete-or-`Err` contract
   (`crates/traits/src/lib.rs:772-776`) holds; a streaming/paged trait evolution is
   deferred (the #262 discussion is closed with the materialized-Vec stance).
4. **XML.** Emit `<ListBucketResult>` by string building (the workspace has no XML crate;
   adding one is a human-gated dependency decision — avoid it). XML-escape key names.
   V1 shim: same handler, `Marker`/`NextMarker` fields, no `list-type` param.
5. **Tests.** Seed `bucket:{name}` markers by committing them directly through the
   `MetadataStore` handle (`start_gateway_with_handle` pattern,
   `crates/server/tests/s3_http_wire.rs:70`); drive assertions with the stock
   `aws-sdk-s3` client where possible (its paginator exercises continuation-token
   chaining for real).

## Alternatives considered

- **Synthesized bucket existence** (bucket exists iff ≥1 dirent): foreclosed by ADR-0046
  decision 7.
- **A paged/streaming `scan` trait change now**: touches every metadata backend
  (redb/tikv/fdb/mem) in the same change as a new user-facing feature; ADR-0046 keeps the
  gap this issue's scope but the materialized-sort answer satisfies Alpha's bounds
  (`SCAN_CAP`) without the cross-backend blast radius.
- **An XML dependency (quick-xml)**: pulls a new license/dependency audit (ADR-0003) for
  what a string builder does adequately for emission.

## Impact & compatibility

Bucket-scoped GET changes from 400 to a real response — no client depends on the 400.
Until #511 lands CreateBucket, no production path writes `bucket:{name}` markers, so on a
live stack every listing answers `NoSuchBucket` until markers are backfilled (ADR-0046's
stated Alpha stance: development clusters backfill or reset). The `aws s3 ls` end-to-end
acceptance is therefore demonstrable off-Check only after a marker backfill or #511.

## Open questions

- **Sequencing vs #511** (pre-declared): ADR-0046 assigns the `NoSuchBucket` record-read
  to this issue while #511 (outside this batch) writes the markers. The record read is
  ADR-NORMATIVE and lands here (tests seed markers; a live stack backfills or waits for
  #511 — Impact section). The only open sequencing question is #511's timing, not whether
  the read ships.
- Whether `start-after` / `encoding-type` are needed for stock aws-cli `ls`/`sync` —
  Do should verify with the SDK test and state the result.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Human rejects this attempt pending fixes for all advisory findings (check-review.md + adversary). Carry forward: - max-keys=0 must match S3: return IsTruncated=false with no keys, never a truncated page without a resume token (clamp at crates/gateway-s3/src/lib.rs:529 admits 0; zero-budget branch at lib.rs:471-473 suppresses next_key). Add a test for max-keys=0 on a non-empty bucket. - Subresource denylist on the bucket route must match percent-DECODED query keys: GET /bucket?%61cl (or ?upload%73) currently bypasses the 501 guard at lib.rs:1173 and receives a listing; decode before matching (unsupported_subresource, lib.rs:363-367). - Close the centerpiece test gap: add a delimiter + max-keys chaining test exercising the group-consume/resume path under truncation (lib.rs:482-488), asserting no CommonPrefix is double-emitted across pages. - Do not silently ignore start-after / encoding-type: reject the unimplemented forms (400/501) until implemented — rclone/minio-go send encoding-type=url and URL-decode returned keys, corrupting keys like a%2Fb; start-after clients re-receive consumed keys. - Correct or temper the seam doc claim at crates/gateway-core/src/lib.rs:210 ("no new cost class"): list_container does one sequential read_inode per dirent (crates/server/src/lib.rs:495) and re-runs scan+reads+sort per page; batch the inode reads or document the N+1 as explicit, bounded Alpha debt. - Conformance nits: v1 emits NextMarker without a delimiter (lib.rs:727-729, test locks non-AWS behavior in) and any list-type other than 2 silently falls to the v1 shim (lib.rs:521) where AWS answers 400 InvalidArgument — align with AWS or state the deviation deliberately.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix all three adversary refutations and clear the gating C4-ci red: 1. C4-ci / typos: reword `mis-decoding` in the comment at crates/gateway-s3/src/lib.rs:556 (or record a deliberate typos.toml exception) so `cargo xtask ci` is green — the sole gate finding, caused by this patch. 2. Denylist gap: add `versioning`, `intelligent-tiering`, `ownershipControls`, `policyStatus`, `metadataTable` to UNSUPPORTED_SUBRESOURCES (crates/gateway-s3/src/lib.rs:330-361) so a bucket subresource GET answers 501, never a listing document; add a probe test (e.g. stock-SDK get_bucket_versioning → clean 501). 3. v1 NextMarker: emit the last *returned* item (the common prefix when a group was the last entry), not the last *consumed* raw key (render_list_v1 / lib.rs:770,800), and make the marker resume skip a whole group when the marker names a common prefix (resume filter at lib.rs:491) so a client resuming from a stored last-CommonPrefix never receives duplicates. Strengthen the test at crates/server/tests/s3_list_objects.rs:414-438 to assert the NextMarker VALUE, not just is_some(). Mainline is otherwise sound (red→green independently corroborated; pagination/token/encoding probes unrefuted) — keep the approach, fix only the above.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversary's landed refutation: a client-chosen resume point inside a delimiter group (v2 `start-after` or v1 `marker` — arbitrary keys, unlike server-issued tokens) makes the group's remaining keys silently invisible on every page. Reproduced end-to-end: bucket {a/1, a/2, b}, `?list-type=2&delimiter=/&start-after=a/1` returns Contents=[b], CommonPrefixes=[] — AWS returns CommonPrefixes=[a/], Contents=[b]; same for v1 `?delimiter=/&marker=a/1`. Cause: the group-skip predicate `resume_after.is_some_and(|r| cp.as_str() <= r)` (crates/gateway-s3/src/lib.rs:537) treats any resume value ≥ the common prefix as "group consumed"; that is only valid for the two server-issued values. Fix locally in `compute_page`: skip the whole group only when the resume point equals the common prefix or is ≥ the group's last raw key; otherwise filter the group's raw keys individually and emit the rollup if any survive. Add wire tests for start-after and v1 marker landing *inside* a delimiter group (the existing start-after test is flat/no-delimiter and cannot catch this). While in there, address the two minor advisory observations: soften/correct the "raw match is sufficient" comment on the object-path subresource fence, and either use or drop the unused `BucketRecord` struct (crates/core/src/metadata.rs:346).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversary's two landed findings; both verified as covered by NO other tracker issue (repo-wide search: zero hits for encoding-type; start-after hits unrelated), so neither can be punted — and epic #513's Alpha client bar (stock aws-cli + boto3 list working) plus #507's own tracker acceptance ("aws s3 ls / aws s3 sync work") make item 1 part of THIS issue's goal. The brief must change, hence iterate-plan: 1. Scope/Goal contradiction — pull `encoding-type=url` INTO scope. The brief scopes it out while its Goal names aws-cli/rclone; empirically (aws-cli 2.36.1 --debug) botocore auto-injects `encoding-type=url` into every ListObjects/V2 request, so the current 501 rejection (crates/gateway-s3/src/lib.rs:635-641) refuses every stock aws-cli / boto3 / rclone listing, and the in-code comment at lib.rs:634 claiming aws-cli does not send it is factually wrong. Fix is modest per the adversary: URL-encode Key/Prefix/CommonPrefixes/Delimiter (and StartAfter/Marker echoes) when encoding-type=url is requested and emit `<EncodingType>url</EncodingType>`. Add a wire test that sends encoding-type=url (the SDK suite never does — that is why C4 green missed this) and asserts a key like `a&b/c d` round-trips URL-encoded. Note #512's aws-cli/boto3 harness will deterministically re-expose this if skipped. 2. v2 `start-after` equal to a common prefix silently hides the whole group (residue of the iteration-3 refutation). Reproduced on the wire: bucket {a/1, a/2, b}, `?list-type=2&delimiter=/&start-after=a/` → Contents=[b], CommonPrefixes=[] — AWS returns CommonPrefixes=[a/]. Cause: the group-skip predicate at crates/gateway-s3/src/lib.rs:556-557 applies the `r == cp` collapse to client-chosen v2 start-after; that collapse is only valid for the server-issued v1 NextMarker resume. Fix: restrict the `r == cp` clause to the v1 marker path (a server-issued v2 token always satisfies `r >= last_raw`), keeping the v1 resume test at crates/server/tests/s3_list_objects.rs:414-471 green. Add a wire test for v2 start-after exactly equal to a common prefix (folder-marker workflow trigger). Minor, while in there: emit the `<StartAfter>` echo in render_list_v2 when the request carried start-after (noted, not pressed). Mainline is otherwise sound — red→green independently corroborated both legs (17/17), the iteration-3 inside-group resume fix holds, denylist/token/auth probes unrefuted. Keep the approach; amend the brief's scope and fix the two items above.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
