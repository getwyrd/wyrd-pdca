## Summary
**User impact:** an S3 client could lose data by sending what it meant as a multipart upload request. If the request's multipart parameter name was percent-encoded, the gateway did not recognise it, ran the request as an ordinary upload or delete, and reported success. The object was overwritten or deleted. Only a client with valid credentials can send such a request, and common SDKs do not spell parameters this way.

This PR makes the gateway recognise those parameters however they are percent-encoded, so these requests are refused and the object is left alone. It does not add multipart upload support: valid multipart requests still answer `501`.

This is the first of four parts of #508. The other three (#668, #669, #670) add the multipart operations on top of this routing.

## What to look at
The change is one decision made in one place: the gateway decodes the query once, the same way signature checking does, and picks the route from the decoded parameter names.

To try it, run `cargo test -p wyrd-server --test s3_multipart_routing`. The test sends signed requests over a real connection to a running gateway and checks after each one that the stored objects are unchanged.

Four behaviour changes deserve a reviewer's judgment:

| Request | Before | After |
|---|---|---|
| Malformed multipart request, such as `PUT /b/k?partNumber=1` with no upload id | `501` | `400 InvalidArgument` |
| Encoded form of an unsupported feature, such as `PUT /b/k?t%61gging=1` | overwrote `k` | `501` |
| Multipart request with `/` in the decoded bucket name, such as `POST /b%2Fa/k?uploads` | `501` | `400 InvalidBucketName` |
| Encoded bulk delete, `POST /b?d%65lete` | `501`, deleted nothing | runs the bulk delete, as `POST /b?delete` does |

The last row is the only one that makes a request do more than it did. See **Known limits** for one case it widens.

## Root cause
The object route refused multipart parameters by comparing the raw query text (`unsupported_subresource`, `crates/gateway-s3/src/lib.rs:466`, called at `:1842`), while SigV4 signature checking decodes each parameter once before verifying it. So `part%4Eumber` passed the signature check as a valid request but missed the refusal, and the method table below it dispatches on the HTTP method alone.

## Fix
Line numbers in this section are on the patched tree.

- `DecodedQuery` (`crates/gateway-s3/src/lib.rs:473`) holds the query decoded exactly once, built right after signature checking (`:1951`). Decoding twice would be wrong the other way: `part%254Eumber` signs as the literal key `part%4Eumber`, so it stays a plain PUT.
- `classify_query` (`:602`) is the single place a denylisted or multipart key picks a route, used by the bucket route (`:1993`) and the object route (`:2031`). A denylisted key wins first. Then a repeated multipart key is refused, `partNumber` must be canonical digits (`parse_part_number`, `:580`), and the keys present must match exactly one operation on its own method and path.
- `uploads`, `uploadId` and `partNumber` leave the denylist. Bulk delete now refuses them by name (`foreign_subresource_on_delete`), since it read that same list and runs before the classifier.
- `multipart_response` has one arm per operation, so #668–#670 can each replace their own. The numeric range of `partNumber` is left to the existing `upload_part` contract, and each operation still checks its own upload id.
- `docs/design/architecture/05-building-block-view.md` now describes the `501` / `400` split. `crates/server/tests/multipart_gateway_seam.rs` changes in comments only.

## Verification
Base lines are on `main` at `376b106`.

- **Claim:** encoded spellings no longer overwrite or delete an object.
  - **Checked:** `crates/gateway-s3/src/lib.rs:466` and `:1842` on `main` match raw keys. Without the fix, `PUT /b/k?part%4Eumber=1` answered `200` and read back overwritten, and `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` answered `204`, then `404` on HEAD and GET.
  - **Test:** `percent_encoded_spellings_classify_as_their_decoded_key` (`crates/server/tests/s3_multipart_routing.rs:617`).
- **Claim:** malformed multipart requests answer `400` and never reach a plain verb or a listing.
  - **Checked:** without the fix, each of the 18 malformed requests answered `501`.
  - **Test:** `ill_formed_multipart_shapes_answer_400_invalid_argument` (`:643`).
- **Claim:** each well-formed operation answers `501` naming the operation, and `GET` / `HEAD` with only `partNumber` stays `501`.
  - **Test:** `well_formed_multipart_shapes_answer_501_naming_the_operation` (`:703`) and `valid_non_multipart_forms_are_unaffected` (`:730`).
- **Claim:** a denylisted key wins in any spelling, and a bucket name containing `/` is refused on every multipart route.
  - **Test:** `a_denylisted_subresource_wins_in_any_spelling` (`:792`) and `a_bucket_alias_is_refused_on_every_multipart_route` (`:818`).
- **Claim:** bulk delete refuses multipart keys, and its encoded marker behaves like the plain one.
  - **Checked:** `foreign_subresource_on_delete` at `crates/gateway-s3/src/lib.rs:578` on `main` reads the denylist these keys leave.
  - **Test:** `bulk_delete_keeps_refusing_a_multipart_selector` (`:858`), `an_encoded_bulk_delete_marker_runs_the_bulk_delete` (`:896`) and `an_encoded_bulk_delete_marker_keeps_every_refusal` (`:919`).

**Before and after:** with production code reverted and the test file kept, 7 of the 11 tests fail by assertion. The other 4 pass on both trees: two regression guards and two tests of the test harness itself. With the fix, all 11 pass. Nothing is mocked: requests go through `serve_s3`, signature checking and the real `Gateway`.

**Wider checks:** `cargo xtask ci` passed on its second run. The first run failed on the known flaky test `a_bodyless_response_is_recorded_complete_not_aborted` (#988), which fails at about the same rate with and without this patch (6 of 500 runs each). Every changed line that coverage could measure ran under the tests (392 of 392), and mutation testing caught 56 of 58 mutants, with the other 2 unviable.

## Known limits
None of these is fixed here. The first is widened by this patch; the other two behave the same as on `main`.

- **Bucket name containing `/` on bulk delete (#1021):** `POST /b%2Fa?delete` already deletes `b/a/k` on `main`. With this patch, `POST /b%2Fa?d%65lete` does too, where it used to answer `501`. It is a second spelling of an existing problem, and no test covers it.
- **Different letter case (#1022):** `DELETE /b/k?UPLOADID=U` still deletes `k`, and `PUT /b/k?uploadid=U&partnumber=1` still overwrites it. These sign as different keys, so this patch does not treat them as multipart. We have not compared this with how S3 itself behaves. The new paragraph in `05-building-block-view.md` says "no spelling", which holds only for percent-encoded spellings, so a reviewer may want that wording tightened.
- **Encoded listing parameters (#1023):** `GET /b?pref%69x=zzz` ignores the prefix and lists the whole bucket.

#491 stays open for a maintainer to close or narrow: the object route is fixed here, and the listing case is tracked in #1023.

**For #669:** a `PUT` with `x-amz-copy-source` on the upload-part route answers `501` today. The upload-part operation must refuse that header itself, or a copy request would store its empty body as a part.

Fixes #667
