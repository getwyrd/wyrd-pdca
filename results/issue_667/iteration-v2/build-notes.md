# Build notes — issue #667 (508.2): multipart routing classifier, iteration 2

Base: getwyrd/wyrd `main` @ `376b106` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`). "base:N" = line on `376b106`. Lines cited as
`lib.rs:N` or `s3_multipart_routing.rs:N` are after `patch.diff`, in `crates/gateway-s3/src/` and `crates/server/tests/`.
Patch: 84989 bytes, 2 files. A copy of the test is in this bundle.

## What changed since iteration 1, and why

The routing design from iteration 1 stands: Check found no production routing defect. The carry-forward named two test-harness
defects, one optional cleanup and the mutation survivors.

1. **Response parser accepted torn chunked framing (C5, T4; 3 of the 4 batch findings).** `decode_chunked` returned at the zero-size
   chunk, so `1\r\nx\r\n0\r\n` and an unterminated trailer passed. It now follows the rules of
   `multipart_gateway_seam.rs:575`: hex-only chunk sizes, the trailer section, the final CRLF, nothing after it
   (`s3_multipart_routing.rs` `decode_chunked`). `parse_response` got the same tightening: bad or repeated `content-length`,
   non-`chunked` or repeated `transfer-encoding`, both framings at once, and no framing at all are now errors. So are a bad
   HTTP version and bytes after a HEAD, `204` or `304`. Before, a response with no framing was read to EOF, which was the same
   truncation gap in another form. New test: `the_response_parser_refuses_torn_framing`, 21 torn inputs and 6 whole ones.
2. **Fixture detached its server (T3, T5, T4; the 4th batch finding).** `Served` now keeps the `JoinHandle` and aborts it in `Drop`,
   as `s3_key_length_limit.rs:118` does. New test: `dropping_the_fixture_stops_its_server`. It watches a `Weak` to the backend go
   to zero strong handles. I chose that over "connect must be refused" because a freed ephemeral port can be rebound by a test
   running in parallel, which would make it flaky. I removed the abort by hand and the test went red ("the server stops once its
   fixture is dropped: Elapsed"), then restored it.
3. **Classifier cleanup (code-review, optional).** `classify_query` now fills fixed slots in one pass (`lib.rs:603`), with no
   per-key `Vec`s and no separate `present` list; `DecodedQuery::values` is gone. A repeat returns as soon as it is seen. The key
   list is built only in the ill-formed fallback arm. Wire behaviour is unchanged except the wording of the repeat message
   ("appears more than once", no count).
4. **Mutation survivors (C5, advisory; 17 in iteration 1).** `cargo mutants` runs only the mutated crate's tests, so the wire test
   in `wyrd-server` could not kill anything in `wyrd-gateway-s3`.
   - `multipart_shapes` is **deleted**, not tested. It accounted for 8 survivors and only produced help text in a `400` message,
     and the brief specifies only that answer's status and code. Testing it would have pinned prose. The `400` message still
     names the keys, method and path. This removes about 18 production lines.
   - Two router-level unit tests in `lib.rs` (`the_router_answers_each_query_route_and_reaches_no_write`,
     `the_router_keeps_the_plain_verbs_for_every_non_multipart_query`) drive the real router and SigV4 against the existing
     `RecordingGateway` double, for the name, selector, `query_route_response`, alias-guard, `multipart_not_implemented` and
     `dispatch` `&&` survivors. They also prove that a refused route reaches neither `put` nor `delete` on the seam.
   Scan result (`scripts/mutants-in-diff`): **50 tested: 48 caught, 0 missed, 2 unviable**. The unviable two are `Default::default()`
   stubs for types with no `Default`. The scan ran on the final `lib.rs`; only the test file's status-line check changed afterwards, and
   the scan does not run that file.
5. **Small extras from self-review.** Leg 4 now checks every seeded object after each plain request (the brief says "after every
   request"; it used to check once at the end) and reads back the once-decoded PUT's body. Leg 2 has the brief's
   `?uploadId=U&uploadId=V` on PUT too. The bucket-marker commit in `seed_bucket` is now bounded. The `multipart_response` doc no
   longer says the verb children change "nothing else", and it records that the UploadPart arm must refuse `x-amz-copy-source`
   itself (a comment only, no behaviour change).

**Not addressed, on purpose:** the two items in `deferred-findings.json` (the raw `?delete` marker, and whether to refuse UploadPartCopy
now) are held for the human, as the carry-forward says. Neither is touched here beyond the comment in item 5.

## The red→green record (this patch)

Runner: `PDCA_BUNDLE=… PDCA_LANE=1 ./engine/scripts/run-verify.sh` on the final `patch.diff`, exit 0: `PASS — red without the fix,
green with it (9 test(s) ran red)`. That summary overstates it, as Check noted last round. The actual counts:

- GREEN: 9 ran, 9 passed.
- RED (production reverted, test kept): **9 ran, 5 failed, 4 passed**. All 5 failures are assertion panics at
  `s3_multipart_routing.rs:306` (`run`'s final `assert!`), with 2, 5, 14, 6 and 18 failing rows. Nothing failed to build. The 4 that
  pass: leg 4 and leg 7 (the regression guards) and the two harness tests, which touch no production code.
- What `main` answered on the RED leg:
  - `PUT /b/k?part%4Eumber=1` → **200**, and `k` then read back overwritten (HEAD 200 with a different ETag).
  - `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` → **204**, then `k` answered HEAD 404 and GET 404: **deleted**. This
    is the brief's "should answer 204", confirmed.
  - `DELETE /b/k?%75ploadId=<id>` → 204 and deletes `k`, for all three ids.
  - `POST /b/k?upload%73` → 405 MethodNotAllowed.
  - Leg 2 (18 rows) and leg 6 (6 rows): 501 NotImplemented where 400 is required.
  - Leg 3: the plain-spelled rows answer 501 but the message names the key, not the operation.
  - Leg 5: 501, naming `partNumber` / `uploads` instead of `tagging` / `acl`.

### Refuting my own test (the three questions)

- **(a) Genuine red? Yes.** With production reverted by run-verify, 5 of 9 fail by assertion with the rows above. No build error: the
  file uses only base API (`serve_s3`, `Gateway`, `sigv4::{sign, format_amz_date, Credentials}`, `RedbMetadataStore`, `WriteBatch`) and
  no new dev-dependency. The two new harness tests are green on both trees by design; I proved the fixture test binds by removing
  the abort, and the parser test binds by its torn rows, which the iteration 1 parser accepted (`reviewer-evidence/probes.log`).
- **(b) Production path? Yes.** Requests go over TCP to the real axum router, started by the production `serve_s3`, through `handle` →
  `dispatch` → `sigv4::verify` → `DecodedQuery::decode` → `classify_query` → `query_route_response`. Nothing is mocked; the backend is
  redb in memory + `FsChunkStore` + `MemCoordination`. The new `lib.rs` unit tests use a recording double, but through the same real
  router and verifier, and they supplement the wire test rather than replace it.
- **(c) Fixture includes the fault? Yes.** The object the destructive forms would hit is real and checked after every row; the RED leg
  shows it overwritten and deleted. Leg 6 also seeds `b/a/k`, the object the `/b%2Fa/k` alias names. Leg 7 ends with a control: the
  same `<Delete>` body without a multipart key answers 200 and both objects then answer 404, so each refusal withheld a real delete.

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
- **Rejected: factoring the chunked decoder into shared test support** (code-review suggested it). It would edit
  `multipart_gateway_seam.rs`, and the brief limits this slice to `lib.rs` and the new test. Cost of keeping a copy: about 55 lines.

## For the verb children (#668–#670)

- Replace your operation's arm in `multipart_response` (`lib.rs:2260`). It and
  `query_route_response` take only `request_id`, the op and the bucket today, so you will need to
  thread `state`, the decoded key, headers, payload hash and body through (and make them async).
- **UploadPartCopy** (`x-amz-copy-source` on an UploadPart shape) lands in the UploadPart arm. The
  CopyObject refusal lives only in the plain PUT arm (`lib.rs:2053`), so #669 must refuse it
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
- Size: 85.0 KB against the brief's "about 45–60 KB" (under the 100 KB backstop). The wire test is about 36 KB and the
  `lib.rs` diff about 48 KB, of which about 17 KB is unit tests. Those stay because `cargo mutants` runs only the mutated crate's
  tests, so the wire test cannot kill a `wyrd-gateway-s3` mutant, and they cover shapes the wire test does not (PATCH,
  HEAD+`uploadId`, bucket POST/PUT/DELETE, `u32` overflow, the single-decode rule on values).
- `docs/design/proposals/draft/0017-blackbox-validation-tool.md:246-248` expects the #491 encoded-`partNumber` vector to answer
  `501`. Here `PUT ?part%4Eumber=1` without `uploadId` answers `400 InvalidArgument`, as brief (1) requires. A draft doc; flagged by
  the adversary last round, left alone.
- Main code locations: `DecodedQuery` `lib.rs:474`, `classify_query` `lib.rs:603`, `foreign_subresource_on_delete` `lib.rs:781`,
  `query_route_response` `lib.rs:2231`, `multipart_response` `lib.rs:2260`.

## Gates I ran (in the worktree, on the final tree)

- `cargo fmt --all -- --check`: clean. `typos` on both files: clean (the brief's one external dependency is present).
- `cargo clippy -p wyrd-gateway-s3 --all-targets` and `cargo clippy -p wyrd-server --test s3_multipart_routing`: clean under
  `warnings = "deny"`.
- `cargo test -p wyrd-gateway-s3`: 97 passed.
- S3 wire suites: `s3_multipart_routing` 9, `multipart_gateway_seam` 7, `s3_copy_object_guard` 2, `s3_delete_objects` 22,
  `s3_head_object` 3, `s3_http_wire` 19, `s3_key_length_limit` 7, `s3_list_objects` 24, `s3_object_metadata` 2,
  `s3_range_conditional` 17, `s3_server_version_header` 1: all passed.
- `./engine/xtask.sh ci-dry-run` (runs the repo-hygiene guards for real): passed. `./engine/xtask.sh statics`: passed.
- `scripts/mutants-in-diff`: 48 caught, 0 missed, 2 unviable (item 4 above).
- **Not run: the full `cargo xtask ci`.** Check's C4-ci runs it. Nothing outside `wyrd-gateway-s3` and the new server test target
  changed, and `wyrd-gateway-s3`'s public API is unchanged.

Self-review against `## Review rubric & protocol`: the one clock read is the existing test-file `SystemTime::now()` for SigV4 dates,
with its stated exemption; no new crate root; no port, operation, RPC, CLI flag or persisted field changes. Every await in the new
test is bounded, and the spawned server is aborted on drop.
