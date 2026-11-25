# Build notes — issue #667 (508.2): multipart routing classifier

Base: getwyrd/wyrd `main` @ `376b106` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l2`, `origin/main`
was the same commit when the verify ran). "base:N" = line on `376b106`; "patched:N" = line after
`patch.diff`. Patch: 72,840 bytes, 2 files (`crates/gateway-s3/src/lib.rs` modified,
`crates/server/tests/s3_multipart_routing.rs` new). A copy of the test is in this bundle.

## What the change is

The object route refused multipart keys with a RAW key match (`unsupported_subresource`,
base:466-472, called at base:1842), while SigV4 decodes each key and value once and re-encodes
them (`crates/gateway-s3/src/sigv4.rs:248-268`). So the key the client signed and the key the
router matched could differ. I replaced the per-route matching with one decode and one classifier
that every route reads:

- `DecodedQuery` (patched:465-509): splits on `&` and the first `=`, then percent-decodes key and
  value exactly once with the same `percent_decode_utf8` the path uses (base:3526). A key with no
  `=` gets the empty value, as in the canonical query. Built once in `dispatch` right after SigV4
  (patched:1974-1979).
- `classify_query` (patched:599-690) returns `None` (the target's plain verb) or a `QueryRoute`
  (patched:564-582), in this order:
  1. a denylisted key, decoded, wins: `Unsupported(key)` → 501 naming it;
  2. no multipart key → `None`;
  3. any of `uploads` / `uploadId` / `partNumber` present more than once (counted on DECODED keys,
     so `uploadId=U&upload%49d=V` is a repeat) → `IllFormed` → 400 InvalidArgument;
  4. `partNumber` must pass `parse_part_number` (patched:584-597): ASCII digits only, no sign, no
     leading zero (a lone `0` is canonical), fits `u32`. Range `1..=10000` is NOT checked (brief
     scope; the seam checks it first, `crates/gateway-core/src/lib.rs:725-734`);
  5. exact shape table: object POST `uploads` → Create; object PUT `partNumber`+`uploadId` →
     UploadPart; object POST `uploadId` → Complete; object DELETE `uploadId` → Abort; object GET
     `uploadId` → ListParts; object GET/HEAD `partNumber` alone → `PartRead` (part-ranged
     GetObject/HeadObject, 501); bucket GET `uploads` → ListUploads. Anything else carrying a
     multipart key → `IllFormed` (400) with a message naming the keys, method and what that method
     takes (`multipart_shapes`, patched:692-710).
- `MultipartOp` (patched:521-562) carries the parsed `upload_id: String` / `part_number: u32`.
  The upload id is the decoded value, unchecked (the seam contract says validating it is the
  implementer's, gateway-core:725-734), so well-formed, unknown and malformed ids all route to the
  verb's arm.
- `query_route_response` (patched:2246-2278) is the one place the non-plain answers live, for both
  routes. For `IllFormed` and `Multipart` it first refuses a decoded bucket containing `/` with
  `400 InvalidBucketName` (the alias rule, brief (6)); `Unsupported` and `PartRead` are not
  alias-checked. `multipart_response` (patched:2280-2291) has one arm per operation, each calling
  `multipart_not_implemented` (patched:2293-2306), so #668–#670 each replace one arm.
  `invalid_bucket_name` (patched:2308-2317) is shared with the plain PUT arm (patched:2099-2101,
  was base:1887-1894; same status, code and message).
- Denylist: `uploads`, `uploadId`, `partNumber` removed (base:411-413), and in the same change
  `foreign_subresource_on_delete` (patched:790-810, was base:565-585) refuses `MULTIPART_KEYS`
  explicitly, reading the shared `DecodedQuery` (brief scope: "only together with an explicit
  bulk-delete refusal").
- Removed: `unsupported_subresource` (raw, base:455-472) and `unsupported_subresource_decoded`
  (base:550-563); both callers now read the classifier (bucket route patched:2012-2023, object
  route patched:2053-2061). `is_delete_subresource` (patched:712-727) stays a RAW match on purpose
  (see decisions).
- The 501 text for a well-formed operation is "the `<key>` S3 subresource selects <Operation>,
  which is not implemented". It keeps the phrase "the `<key>` S3 subresource" because the existing
  `crates/server/tests/multipart_gateway_seam.rs:444` checks it, and it names the operation as
  brief (3) requires. That seam test (7 tests) still passes, including its counter that no
  multipart form enters the seam.
- Unit tests in lib.rs (patched:5494-5720) replace the old raw-match test (base:5208-5224) with
  six classifier tests: denylist in any spelling, each well-formed shape (with decoded values,
  the single-decode rule, `zz` id, `0`/`10001`), part-ranged reads, 33 ill-formed shapes,
  `parse_part_number` edge cases, and bulk-delete refusals.

## The test and the red→green record

`crates/server/tests/s3_multipart_routing.rs` (708 lines): 7 `#[tokio::test]` legs, one per brief
item. Each starts a fresh gateway through `wyrd_server::cli::serve_s3` (cli.rs:2658) over a real
`Gateway` (in-memory redb, `FsChunkStore` on a tempdir, `MemCoordination`), seeds the bucket marker
the way `s3_list_objects.rs:82` does, stores `k` with a plain signed PUT, and drives hand-signed raw
requests (harness salvaged from `results/issue_508/s3_multipart_upload.rs:211-332`, rewritten:
stricter response parsing, a 30 s bound on every exchange). Every row asserts exact status + exact
S3 code, plus the `<Message>` text where the brief says what a 501 names. After every row, every
seeded object must answer HEAD 200 with the ETag its seeding PUT returned and GET its exact bytes.
PUT rows carry a distinct CLOBBER body so an overwrite is visible. A leg collects all failing rows
and fails once with the full list; a changed object is put back so one failure does not cascade.

Runner: the project's C4-verify script, `PDCA_BUNDLE=… PDCA_LANE=2 ./engine/scripts/run-verify.sh`
(lane-2 verify worktree, base `origin/main` = `376b106`), run four times as the patch settled; the
last run was on the final 72,840-byte `patch.diff` (later edits were comments and one no-op hoist).
Every run: `PASS — red without the fix, green with it (7 test(s) ran red)`, same counts each time.

- GREEN: 7 ran, 7 passed.
- RED (production reverted, test kept): **7 ran, 5 failed, 2 passed**. All 5 failures are
  assertion panics from `run`'s final `assert!` (s3_multipart_routing.rs, the `failures.is_empty()`
  check); nothing failed to build. Passed: leg 4 (`valid_non_multipart_forms_are_unaffected`) and
  leg 7 (`bulk_delete_keeps_refusing_a_multipart_selector`), the regression guards.
- What `main` answered on the RED leg:
  - `PUT /b/k?part%4Eumber=1` → **200**, then `k` read back as the 60-byte CLOBBER body
    (overwritten). This is the parent brief's reproduction, confirmed.
  - `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` → **204**, then `k` answered HEAD
    404 and GET 404: **deleted**. The brief asked me to confirm this form; it is confirmed.
  - `DELETE /b/k?%75ploadId=<id>` → 204 and deletes `k` too, for all three ids (another spelling).
  - `POST /b/k?upload%73` → 405 MethodNotAllowed.
  - Leg 2 (16 rows) and leg 6 (6 rows): 501 NotImplemented where 400 is required.
  - Leg 3: the plain-spelled rows answer 501 NotImplemented but the message names the key, not the
    operation (14 failing rows including the `%75ploadId` deletions).
  - Leg 5: 501 NotImplemented, but naming `partNumber` / `uploads` instead of `tagging` / `acl`.

### Refuting my own test (the three questions)

- **(a) Genuine red? Yes.** With the production change reverted (run-verify's RED leg, run 4 on
  the final patch) 5 of 7 tests fail, by assertion, with the rows listed above. No failure is a
  build error: the file uses only base API (`serve_s3`, `Gateway`, `sigv4::{sign,
  format_amz_date, Credentials}`, `RedbMetadataStore`, `WriteBatch`) and no new dev-dependency.
- **(b) Production path? Yes.** Requests go over TCP to the real axum router built by
  `S3Gateway::serve`, launched by the production `serve_s3` (which also runs `Gateway::recover`),
  through `handle` → `dispatch` → `sigv4::verify` → `DecodedQuery::decode` → `classify_query` →
  `query_route_response`. Nothing is mocked; the backend is the composition `wyrd s3` runs for
  redb+mem+FS, with redb in memory.
- **(c) Fixture includes the fault? Yes.** The object the destructive forms would hit is real and
  checked after every row: the RED leg shows it overwritten and deleted. Leg 6 also seeds `b/a/k`,
  the object the `/b%2Fa/k` alias names. Leg 7 ends with a control: the same `<Delete>` body
  without a multipart key answers 200 and both `k` and `k2` then answer 404, so each refusal in
  that leg withheld a real deletion.

## Brief inconsistencies — what I followed and why

1. **Falsifiability says "(3)'s malformed id … red on exact status (main answers 501 or a 2xx,
   never the required 400)".** But the Success criterion (3) requires `DELETE /b/k?uploadId=zz` to
   answer **501**, and Scope says the target check belongs to each verb child. `main` answers 501
   for that row too (raw match on `uploadId`), so it cannot red on status. I built to the Success
   criterion: 501. On `main` that row reds only on the message (it names `uploadId`, not
   AbortMultipartUpload). If the planner meant a malformed id to be 400 here, that is a brief
   change, not something I could infer against the explicit criterion.
2. **Falsifiability says (5) is green on both trees**, but criterion (5) says the 501 names
   `tagging` / `acl`. `main` names the first multipart key instead (`partNumber`, `uploads`), so
   with the naming asserted, leg 5 reds on `main`. Status and code are the same on both trees.
3. Leg 3's plain-spelled rows red on `main` only on the operation name; the brief's falsifiability
   does not list (3) as red at all apart from the malformed id. Again, I asserted what the
   criterion states ("naming the operation").

## Decisions, and alternatives I ruled out

- **Decode once, not twice.** SigV4 canonicalisation decodes once (sigv4.rs:255-259), so the signed
  key is the once-decoded key. `?part%254Eumber=1` signs as the literal key `part%4Eumber`, which
  S3 also treats as an unknown parameter, so it stays a plain PUT. Pinned in a unit test and in
  leg 4 (`PUT /b/decoded-once?part%254Eumber=1` → 200), which covers #508 v5's double-decoding
  finding at the wire.
- **The denylist is now matched decoded on the object route too.** Brief (5) requires
  `PUT /b/k?partNumber=1&uploadId=U&t%61gging=1` to name `tagging`; with a raw object-route match
  that row would classify as UploadPart and name `partNumber`. Decoding is also what "decode the
  query once into a typed classification that every route reads" says. Side effect, worth the
  human's eye: the documented residual at base:460-465 closes for every remaining denylist entry,
  not only the multipart ones. On `main`, `PUT /b/k?t%61gging=1` was a plain PUT that overwrote
  `k`, and `DELETE /b/k?t%61gging` deleted it; now both answer 501. The Scope line "the remaining
  denylist entries and both guards' behaviour for them" is out of scope; I read it as "do not add,
  remove or re-answer entries", since (5) cannot hold otherwise. Rejected alternative: decode the
  denylist only when a multipart key is present. That is about 5 more lines in `classify_query`
  (a second, raw `UNSUPPORTED_SUBRESOURCES` scan on the no-multipart path) and it keeps a known
  overwrite/delete path open.
- **`is_delete_subresource` stays raw.** Making it decoded would change which requests reach bulk
  delete AND the `op_label` RED-metrics predicate that must mirror it (base:308-325, the
  `op_label_attributes_bulk_delete_to_the_delete_series` test). An encoded `POST /b?d%65lete` was
  refused 501 on `main` by the bucket route's decoded denylist and still is (now via the
  classifier). No behaviour change, so I left it.
- **Alias check placement.** After the denylist, before InvalidArgument, on every multipart shape
  (well-formed or ill-formed) and on both routes, so `GET /b%2Fa?uploads` (ListMultipartUploads on
  an alias bucket) is refused 400 InvalidBucketName too. The brief's (6) list has only the five
  object forms, but its rule says "on every multipart route"; I added that bucket row to leg 6.
  `PartRead` is not alias-checked: it is a GetObject form, and plain GetObject does not refuse the
  alias today.
- **Part number above `u32::MAX`** (canonical digits, e.g. `4294967296`) → 400 InvalidArgument at
  decode. The seam takes `u32` and its out-of-range arm carries the number "as the caller sent
  it", so it cannot be passed on. S3 answers InvalidArgument for any out-of-range part number, so
  the wire answer matches what #669 will give a `u32` out of range.
- **Exotic methods** (PATCH etc.) carrying a multipart key → 400 InvalidArgument rather than the
  method table's 405. The classification is total over multipart keys, so no such request reaches
  the method table at all. Either is a refusal; the brief does not specify.
- **GET with both `uploadId` and `partNumber`** → 400 (ambiguous between ListParts and part-ranged
  GetObject). HEAD with `uploadId` → 400 (no such S3 form).
- **Rejected: two route-specific enums** (object ops vs bucket ops) for per-route exhaustive
  matching. About 30 more lines (two enums, two classifier entry points, the shared refusal
  variants duplicated) for a property the single `multipart_response` already gives: each
  operation's arm exists once, and the classifier never yields an object op on the bucket route.
- **Rejected: inlining the six arms in `dispatch`.** The ListMultipartUploads arm would then sit
  on both routes, or need an unreachable arm on one.
- **Rejected: echoing the upload id / part number in the 501 message.** Client-supplied text of up
  to the request-target limit in every error body, for no benefit in this slice.
- `MultipartOp`'s `upload_id` / `part_number` fields are only read through the derived
  `PartialEq` (tests) in this slice; rustc does not warn, and the verb children read them.

## For the verb children (#668–#670)

- Replace your operation's arm in `multipart_response` (patched:2280-2291). It and
  `query_route_response` take only `request_id`, the op and the bucket today, so you will need to
  thread `state`, the decoded key, headers, payload hash and body through (and make them async).
- **UploadPartCopy** (`x-amz-copy-source` on an UploadPart shape) lands in the UploadPart arm. The
  CopyObject refusal lives only in the plain PUT arm (patched:2077-2084), so #669 must refuse it
  in its own arm or a copy request would store an empty part.
- The 1024-byte key limit (`MAX_KEY_BYTES`) is enforced only in the plain PUT arm; Create (#668)
  should apply it as well.
- `partNumber` arrives canonical but unranged (0 and 10001 route to UploadPart); the seam's range
  check must answer them.

## Out of scope, noted for the human

- `crates/server/tests/multipart_gateway_seam.rs:76-78` says the named subresource is "the FIRST
  denylisted query key … (`unsupported_subresource`)". That function is gone; the test still passes
  because the 501 names each operation's selector key, which is the same key for all six forms.
  The comment is stale; the brief limits me to `lib.rs` and the new test.
- `docs/design/architecture/05-building-block-view.md:137` says "the S3 wire still refuses every
  multipart form with `501`". Still true for the six well-formed operations; ill-formed shapes now
  answer 400 and the alias 400 InvalidBucketName. The brief puts docs out of scope (no API
  operation is added); the rubric's docs-currency rule triggers on ports, API operations, RPCs,
  CLI flags and persisted fields, none of which change. #668 rewrites that sentence when it wires
  the first verb; the human may prefer to touch it now.
- Size: 72.8 KB against the brief's "about 45–60 KB" (under the 100 KB backstop). lib.rs is
  about 43 KB of it, about 12 KB of which is the unit tests that replace the old raw-match test;
  the wire test is about 29 KB. I kept the unit tests: they cover shapes the wire test does not (PATCH,
  HEAD+`uploadId`, bucket POST/PUT/DELETE, `u32` overflow, the single-decode rule on values).

## Gates I ran (in the worktree, on the final tree)

- `cargo fmt --all -- --check`: clean (the formatter changed only my two files).
- `typos` on both files: clean (typos-cli 1.48.0; the brief's one external dependency is present).
- `cargo clippy -p wyrd-gateway-s3 --all-targets` and `cargo clippy -p wyrd-server --test
  s3_multipart_routing`: clean under the workspace's `warnings = "deny"`.
- `cargo test -p wyrd-gateway-s3`: 95 passed.
- Existing S3 wire suites, plus the new one: `multipart_gateway_seam` 7, `s3_copy_object_guard` 2,
  `s3_delete_objects` 22, `s3_head_object` 3, `s3_http_wire` 19, `s3_key_length_limit` 7,
  `s3_list_objects` 24, `s3_object_metadata` 2, `s3_range_conditional` 17,
  `s3_server_version_header` 1, `s3_multipart_routing` 7 — all passed.
- `./engine/xtask.sh ci-dry-run` (runs the gitlink, unsafe-forbid and blackbox guards for real):
  passed. `./engine/xtask.sh statics`: passed.
- `cargo doc -p wyrd-gateway-s3 --no-deps --document-private-items`: 7 link errors, all
  pre-existing on `main` (e.g. `Gateway::put_object_streaming`, `signing_key`); none involve an
  item this patch adds or links. (Not a CI gate here; checked for hygiene.)
- **Not run: the full `cargo xtask ci`.** This leaf runs under a 16 GiB memory cap and was already
  at about 8 GB with the build cache; a full workspace clippy/build/test plus DST risked the OOM
  kill. Check's C4-ci runs it. Nothing outside `wyrd-gateway-s3` and the new server test target
  changed, and `wyrd-gateway-s3`'s public API is unchanged.
