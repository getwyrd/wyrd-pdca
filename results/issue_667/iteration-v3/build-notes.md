# Build notes — issue #667 (508.2): multipart routing classifier, iteration 3

Base: getwyrd/wyrd `main` @ `376b106` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`). "base:N" = line on `376b106`. Lines cited as
`lib.rs:N` or `s3_multipart_routing.rs:N` are after `patch.diff`, in `crates/gateway-s3/src/` and `crates/server/tests/`.
Patch: 98516 bytes, 3 files. A copy of the test is in this bundle.

## What changed since iteration 2, and why

Check found no routing defect in iteration 2's multipart logic. The carry-forward named two things, and I fixed both. The rest of the patch is
iteration 2's, re-applied unchanged.

1. **Docs (the red T4 gate).** `docs/design/architecture/05-building-block-view.md:137` said "the S3 wire still refuses every
   multipart form with `501`", which this patch makes false. Line 137 now says the wire serves none of the verbs and points at a new paragraph,
   **S3 multipart routing (#667)**, at `:139`. It states each answer: `501` naming the operation, `501` for a part-ranged read, `400
   InvalidArgument` for an ill-formed shape, `400 InvalidBucketName` for the alias, the denylist winning in any spelling, and the
   decoded bulk-delete marker.
   - **This goes against the brief's Scope line** (`brief.md:94`, "docs (no API operation is added)"). I followed the carry-forward. The
     rubric's docs-currency rule covers a change that *alters* an API operation and calls that a merge requirement, and iteration 2 showed the gate
     stays red without it. No other living architecture doc makes this claim: I searched `docs/design/architecture` for "every multipart",
     "denylist", and `?delete`.
2. **The bulk-delete marker is now matched decoded (the adversary's finding).** `is_delete_subresource` (`lib.rs:698`) now takes the
   `DecodedQuery`. `dispatch` calls it with the query it already decoded (`lib.rs:1970`). `op_label` calls the same function on its own decode
   (`lib.rs:322`), and only for a POST on a bucket path, because of the guard's order. So `POST /b?d%65lete`, which SigV4 signs as
   `delete`, now runs DeleteObjects instead of answering `501`.

### The marker: why I took option (b), and what the human should check

`deferred-findings.json` called this a judgment call: (a) accept and narrow the invariant, or (b) decode the marker. Round 2 then routed it to the
builder as an implementation finding, with (b) as the stated fix and (a) as the fallback. I took (b) for these reasons:

- The brief's invariant says a form the gateway implements "is reachable by exactly one route, whatever spelling the client uses" (`brief.md:75-76`),
  and Scope lists bulk delete among the routes that read the classification (`brief.md:82-83`). Option (a) would mean rewriting the invariant.
  The brief names "the invariant to restore", so minimal diff is not the deciding point here. As it happens, (b) is also about the same size.
- It widens no authority. The request has passed SigV4 with `delete` as the signed key, and it enters the same handler with the same body checks.
  A client that sends it asked for a bulk delete.
- It cannot let a refused key through. `foreign_subresource_on_delete` already reads decoded keys, and leg 9 proves that a key beside an encoded
  marker is refused by name.

**Still worth the human's eye:** this does turn one `501` into a live destructive route, on purpose. `POST /b?d%65lete` deleted nothing on `main`
and now deletes what its body names, exactly as `POST /b?delete` does. If you would rather have (a), revert the `is_delete_subresource` change
and legs 8 and 9, then narrow the invariant's wording and the comment at `lib.rs:1950-1952`.

**Tests for it**, all in the patch:

- `an_encoded_bulk_delete_marker_runs_the_bulk_delete` (`s3_multipart_routing.rs:904`): `200`, a `<DeleteResult>`, and both objects gone.
- `an_encoded_bulk_delete_marker_keeps_every_refusal` (`s3_multipart_routing.rs:927`): four refused keys beside the encoded marker, plus `POST
  /b?d%2565lete` (signs as another key, `400`), `POST` and `DELETE /b/k?d%65lete` and `GET /b?d%65lete` (`501` naming `delete`). Objects are
  checked after every row.
- `op_label` rows (`lib.rs:5008`): encoded marker gives `delete`, double-encoded and object-path forms give `other`. The two router-level tests
  (`lib.rs:5707`, `:5765`) got matching rows, since `cargo mutants` only runs the mutated crate's own tests.
- `delete_body()` (`s3_multipart_routing.rs:856`) is factored out of leg 7 so the three legs share one body. Leg 7's rows are unchanged.

### Findings I did not act on

- **UploadPartCopy (the other `deferred-findings.json` item):** held for the human, as the carry-forward says. The comment on `multipart_response`
  (`lib.rs:2254-2257`) records that the UploadPart arm must refuse `x-amz-copy-source` itself.
- **Stale comment at `crates/server/tests/multipart_gateway_seam.rs:76-78`:** it cites the removed `unsupported_subresource`. The test still passes,
  and the adversary routed no action. The carry-forward widened Scope for the doc only, so I left it for #668.
- **`docs/design/proposals/draft/0017-blackbox-validation-tool.md:246-248`:** expects the #491 encoded-`partNumber` vector to answer `501`; here it
  answers `400`, as brief (1) requires. It is a draft, not a living doc.
- **Other keys the adversary probed** (`?uploadid=U`, `?uploadId%20=U`, `?x=1;uploadId=U`) still reach the plain verb. Each signs as a key other than the
  three selectors. The denylist comment says it is not a ban on all query strings, so I did not widen it. The human may read "every spelling" more broadly.

### New finding, outside this slice

Listing parameters are still matched on the raw key: `query_param` (`lib.rs:828`) compares the undecoded key,
and `list_objects` reads `prefix`, `list-type`, `max-keys` and others through it. So by code reading, a signed `GET /b?pref%69x=a` would ignore the
prefix and list the whole bucket. **I read this in the code and did not probe it over the wire.** It answers a read, loses no data, and has nothing
to do with the three multipart keys, so per the rubric's out-of-scope rule it needs its own issue. It is the same class of bug, so it probably
deserves one.

## The red→green record (this patch)

Runner: `PDCA_BUNDLE=… PDCA_LANE=1 ./engine/scripts/run-verify.sh` on the final `patch.diff`, exit 0: `PASS — red without the fix, green with it (11 test(s)
ran red)`. That summary overstates it, as Check noted last round. The actual counts:

- GREEN: 11 ran, 11 passed.
- RED (production reverted, test kept): **11 ran, 7 failed, 4 passed**. All 7 are assertion panics, 6 at `run`'s final `assert!` (with 2, 5, 6, 14, 7
  and 18 failing rows) and 1 at leg 8's status `assert_eq!`. Nothing failed to build. The 4 that pass: leg 4 and leg 7 (the regression guards) and
  the two harness tests, which touch no production code.
- What `main` answered on the RED leg:
  - `PUT /b/k?part%4Eumber=1` → **200**, and `k` then read back overwritten (HEAD 200 with a different ETag).
  - `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` → **204**, then `k` answered HEAD 404 and GET 404: **deleted**. This is the brief's "should
    answer 204", confirmed.
  - `DELETE /b/k?%75ploadId=<id>` → 204 and deletes `k`, for all three ids.
  - `POST /b/k?upload%73` → 405 MethodNotAllowed.
  - `POST /b?d%65lete` → 501 NotImplemented naming `delete`, both objects left in place.
  - `DELETE /b/k?d%65lete` → **204** and deletes `k`; `POST /b/k?d%65lete` → 405. (Same raw-match bug on the object route.)
  - Leg 2 (18 rows) and leg 6 (6 rows): 501 NotImplemented where 400 is required.
  - Leg 3: the plain-spelled rows answer 501 but the message names the key, not the operation.
  - Leg 5: 501, naming `partNumber` / `uploads` instead of `tagging` / `acl`.

### Refuting my own test (the three questions)

- **(a) Genuine red? Yes.** With production reverted by run-verify, 7 of 11 fail by assertion with the rows above. No build error: the file uses only
  base API (`serve_s3`, `Gateway`, `sigv4::{sign, format_amz_date, Credentials}`, `RedbMetadataStore`, `WriteBatch`) and no new dev-dependency. The two
  harness tests are green on both trees by design; iteration 2 proved them binding (removing the abort reddens the fixture test; the iteration 1
  parser accepted the torn rows).
- **(b) Production path? Yes.** Requests go over TCP to the real axum router, started by the production `serve_s3`, through `handle` → `dispatch` →
  `sigv4::verify` → `DecodedQuery::decode` → `is_delete_subresource` / `classify_query`. Nothing is mocked; the backend is redb in memory +
  `FsChunkStore` + `MemCoordination`. The `lib.rs` unit tests use a recording double, but through the same real router and verifier, and they add to
  the wire test rather than replace it.
- **(c) Fixture includes the fault? Yes.** The objects the destructive forms would hit are real and checked after every row; the RED leg shows them
  overwritten and deleted. Leg 6 also seeds `b/a/k`, the object the `/b%2Fa/k` alias names. Leg 7's control and leg 8 both end with real deletions
  (`404` on both objects), so the body would delete them if a refusal did not stop it.

## Brief inconsistencies — what I followed and why

1. **Docs.** See item 1 above: Scope excludes docs, the carry-forward and the rubric require this one. I edited the one false claim and added its
   replacement, nothing else.
2. **Falsifiability says "(3)'s malformed id … red on exact status (main answers 501 or a 2xx, never the required 400)".** But the Success criterion (3)
   requires `DELETE /b/k?uploadId=zz` to answer **501**, and Scope says the target check belongs to each verb child. `main` answers 501 for that row
   too, so it cannot red on status. I built to the Success criterion: 501. On `main` that row reds only on the message.
3. **Falsifiability says (5) is green on both trees**, but criterion (5) says the 501 names `tagging` / `acl`. `main` names the first multipart key instead,
   so with the naming asserted, leg 5 reds on `main`. Status and code are the same on both trees.
4. Leg 3's plain-spelled rows red on `main` only on the operation name; the brief's falsifiability does not list (3) as red apart from the malformed id.
   I asserted what the criterion states ("naming the operation").

## Decisions, and alternatives I ruled out

- **Decode once, not twice.** SigV4 canonicalisation decodes once, so the signed key is the once-decoded key. `?part%254Eumber=1` signs as the literal key
  `part%4Eumber`, which S3 also treats as an unknown parameter, so it stays a plain PUT. Pinned in a unit test and in leg 4, which covers #508 v5's
  double-decoding finding at the wire. The marker follows the same rule (`d%2565lete` is not the marker).
- **`op_label` decodes its own copy instead of sharing `dispatch`'s.** It runs in `handle`, before `dispatch` and before SigV4, and sharing would mean passing
  the decoded query through `dispatch`'s signature. Cost of the second decode: one `Vec` of strings, only for POSTs on bucket paths. Both call one predicate, so
  they cannot drift apart.
- **The denylist is now matched decoded on the object route too.** Brief (5) requires `PUT /b/k?partNumber=1&uploadId=U&t%61gging=1` to name `tagging`; with a
  raw object-route match that row would classify as UploadPart. Side effect, worth the human's eye: the documented residual at base:460-465 closes for every
  remaining denylist entry, not only the multipart ones. On `main`, `PUT /b/k?t%61gging=1` was a plain PUT that overwrote `k`, and `DELETE /b/k?t%61gging` deleted
  it; now both answer 501. The Scope line "the remaining denylist entries and both guards' behaviour for them" is out of scope; I read it as "do not add, remove or
  re-answer entries", since (5) cannot hold otherwise. Rejected alternative: decode the denylist only when a multipart key is present, about 5 more lines in
  `classify_query` (a second, raw `UNSUPPORTED_SUBRESOURCES` scan on the no-multipart path), which keeps a known overwrite/delete path open.
- **Alias check placement.** After the denylist, before InvalidArgument, on every multipart shape and on both routes, so `GET /b%2Fa?uploads` is refused 400
  InvalidBucketName too. `PartRead` is not alias-checked: it is a GetObject form, and plain GetObject does not refuse the alias today. Bulk delete on an alias
  bucket (`POST /b%2Fa?delete`) is unchanged from `main`, in either spelling of the marker.
- **Part number above `u32::MAX`** (canonical digits, e.g. `4294967296`) → 400 InvalidArgument at decode. The seam takes `u32`, and S3 answers InvalidArgument for any
  out-of-range part number.
- **Exotic methods** (PATCH etc.) carrying a multipart key → 400 InvalidArgument rather than the method table's 405. Either is a refusal; the brief does not specify.
- **GET with both `uploadId` and `partNumber`** → 400. HEAD with `uploadId` → 400 (no such S3 form).
- **Rejected: two route-specific enums** (object ops vs bucket ops). About 30 more lines for a property the single `multipart_response` already gives.
- **Rejected: inlining the six arms in `dispatch`.** The ListMultipartUploads arm would sit on both routes, or need an unreachable arm on one.
- **Rejected: echoing the upload id / part number in the 501 message.** Client-supplied text of up to the request-target limit in every error body, for no benefit here.
- **Rejected: factoring the chunked decoder into shared test support.** It would edit `multipart_gateway_seam.rs`, outside this slice. Cost of keeping a copy: about 55 lines.
- `MultipartOp`'s `upload_id` / `part_number` fields are only read through the derived `PartialEq` (tests) in this slice; the verb children read them.

## For the verb children (#668–#670)

- Replace your operation's arm in `multipart_response` (`lib.rs:2258`). It and `query_route_response` (`lib.rs:2229`) take only `request_id`, the op and the bucket
  today, so you will need to pass in `state`, the decoded key, headers, payload hash and body (and make them async).
- **UploadPartCopy** lands in the UploadPart arm, and the CopyObject refusal lives only in the plain PUT arm. #669 must refuse it in its own arm or a copy request
  would store an empty part.
- The 1024-byte key limit (`MAX_KEY_BYTES`) is enforced only in the plain PUT arm; Create (#668) should apply it as well.
- `partNumber` arrives canonical but unranged (0 and 10001 route to UploadPart); the seam's range check must answer them.
- #668 should update the new doc paragraph (`05-building-block-view.md:139`) as each verb lands.

## Size

98.5 KB against the brief's "about 45–60 KB", under the 100 KB backstop, and **close to it**. The growth from iteration 2's 85 KB is about 13 KB: the doc diff is about
6.6 KB (a long paragraph counts twice in a diff, plus the new one), and two wire legs and unit rows make up most of the rest. The wire test is about 40 KB and the `lib.rs` diff
about 51 KB, of which about 17 KB is unit tests. Those stay because `cargo mutants` runs only the mutated crate's tests, so the wire test cannot kill a `wyrd-gateway-s3`
mutant. A further iteration that adds much would pass the backstop.

## Gates I ran (in the worktree, on the final tree)

- `cargo fmt --all -- --check`: clean. `typos` on all three files: clean (the brief's one external dependency is present).
- `cargo clippy -p wyrd-gateway-s3 --all-targets` and `cargo clippy -p wyrd-server --test s3_multipart_routing`: clean under `warnings = "deny"`.
- `cargo test -p wyrd-gateway-s3`: 97 passed.
- S3 wire suites: `s3_multipart_routing` 11, `multipart_gateway_seam` 7, `s3_copy_object_guard` 2, `s3_delete_objects` 22, `s3_http_wire` 19, `s3_key_length_limit` 7,
  `s3_list_objects` 24: all passed. This round I did not re-run `s3_head_object`, `s3_object_metadata`, `s3_range_conditional` or `s3_server_version_header`; they passed in
  iteration 2 and Check's C4-ci runs them.
- `./engine/xtask.sh ci-dry-run` (runs the repo-hygiene guards for real): passed. `./engine/xtask.sh statics`: passed.
- `scripts/mutants-in-diff`: 58 tested: 56 caught, 0 missed, 2 unviable (the `Default::default()` stubs for types with no `Default`). It ran before the final comment-only
  edit to the test file's header, and it does not run that file.
- **Not run: the full `cargo xtask ci`.** Check's C4-ci runs it. `wyrd-gateway-s3`'s public API is unchanged.

Self-review against `## Review rubric & protocol`:

- **Docs currency:** now met (item 1).
- **Clock:** the one clock read is the existing test-file `SystemTime::now()` for SigV4 dates, with its stated exemption.
- **Crates and seams:** no new crate root, and no port, RPC, CLI flag or persisted field changes.
- **Await discipline:** every await in the new test is bounded, and the spawned server is aborted on drop.
- **Tier-0 DST:** I read "a new destructive path" as not triggered. The encoded marker enters the existing `delete_objects` handler; no new delete code or concurrency is added.
  A reviewer could read the wider entry as triggering it, and I would answer with this reasoning.
