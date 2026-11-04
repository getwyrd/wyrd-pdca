## Summary
**User impact:** anyone using the validator to check an S3 endpoint cannot test deleting many objects in one request, and cannot ask the endpoint about operations it may not support (copying an object, multipart uploads, creating or deleting buckets). Worse, the AWS library quietly reroutes or refuses some bucket names before sending anything. A check that deletes a bucket must use a name S3 forbids, so that it can never delete a real bucket. That safety rule only works if the name reaches the endpoint exactly as typed, and today it does not.

This PR adds bulk delete and one request per possibly-unsupported operation to the validator's S3 client. It also makes every request go to the configured endpoint with the bucket name exactly as given. Nothing but tests calls the new requests yet. The planned smoke run (#939) will use them.

## What to look at
There are three ideas:
- **Bulk delete** reports every row the server sent: which keys were deleted, which failed and why, which requested keys got no row, and which rows name a key that was not requested. The client reports these and does not judge them.
- **One request per unsupported operation** ("probe"). Each reports the status and S3 error code. A success status is kept even if the body after it stalls, is cut off or is garbage.
- **Own endpoint resolution.** The AWS library's rules for choosing where a request goes are replaced by a simple rule: the configured endpoint, then the bucket name as one path segment. Names the library would treat specially (S3 Express, Outposts aliases, `arn:`) are sent like any other name.

To try it: `cargo test -p wyrd-validate --test s3_client_smoke_verbs`. It needs no Docker and no network beyond `127.0.0.1`.

The patch is large (about 200 KB), but most of it is the test file (about 2,700 added lines). A good reading order is the endpoint resolver in `s3.rs`, then `delete_objects` and `probe` in `s3/verbs.rs`, then the delete-result parsing in `s3/document.rs`.

## Root cause
The client had no `DeleteObjects` and no way to send the operations Wyrd's gateway does not support. It also used the SDK's endpoint rules (`aws-sdk-s3` `config/endpoint.rs`), which read the bucket name. A name ending `--x-s3` or `--xa-s3` is treated as S3 Express (signed for another service, sent virtual-host style or refused). A name ending `--op-s3` is resolved to an `amazonaws.com` URL whatever the endpoint. A name starting `arn:` is refused path-style. In each case the request the caller asked for is not the one that reaches the endpoint, or nothing is sent at all.

## Fix
- **Endpoint resolution (`s3.rs`):** `PathStyle` replaces the SDK's rules for every request, existing verbs included. The URL is the endpoint's scheme, authority and path, then `/` and the bucket as one percent-encoded segment (`path_segment`). The request is signed SigV4 for `s3` in the configured region. The two changes this makes to existing verbs are named in its doc: an endpoint path is joined to the bucket with one `/`, and the region is signed as given (`aws-global` is no longer rewritten to `us-east-1`, and a region that is not a DNS label is sent rather than refused).
- **Bulk delete (`s3/verbs.rs`, `s3/document.rs`):** `delete_objects` sends one non-quiet `DeleteObjects`. `delete_rows` reads the `<DeleteResult>` strictly, and `DeleteResult` exposes the deleted keys, the `<Error>` rows (`KeyError`: key, code, message), `keys_without_a_row` and `rows_not_requested`. A result that is cut short, followed by junk, past `Content-Length` or past `BUFFERED_BODY_BUDGET` is refused beside its 200. Unknown children and processing instructions are refused too.
- **Probes (`s3/verbs.rs`, `s3/answer.rs`):** `Probe` has one variant per operation: `CopyObject`, `CreateMultipartUpload`, `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads`, `GetBucketLocation`, `CreateBucket`, `DeleteBucket`, `HeadBucket` and `ListBuckets`. `S3Client::probe` reports through the existing reply-capture core. A new `SuccessBody` says whether a 2xx must carry no body or one whole document under a given root, so a 200 with the wrong body is reported as unreadable, not as a success. A `200` whose body is an S3 `<Error>` document (S3 does this for `CopyObject` and `CompleteMultipartUpload`) keeps the 200 and reports the code and message.
- **Not sent vs no answer:** a request the SDK cannot build (an empty or blank bucket name, or a URL longer than an `http` URI allows) is `RequestNotBuilt`, and nothing is sent. An overlong key or upload id is refused before the SDK is called. A request that reached the connector and got no answer is classified as the existing verbs classify it: `Timeout { Connect }`, `Timeout { Operation }`, or `NoResponse`.
- **Docs:** the `validate` paragraph of `docs/design/architecture/05-building-block-view.md` now describes #955's verbs and this PR's. The doc comments in `s3.rs`, `response.rs` and `document.rs` were checked sentence by sentence against the tests, so none claims more than a test shows.

**Known limits**
- **Bulk delete size.** The delete result is read whole under the 64 KiB budget, and the server has already deleted the keys by the time an over-budget result is found. With no failures, 1,000 keys of 32 bytes fit and 1,000 keys of 40 bytes do not (70,115 bytes, every row lost). Error rows are larger, so 1,000 keys of 32 bytes fit with 41 `InternalError` rows and not with 42. The `delete_objects` doc states these numbers, and the caller has to size its requests.
- **Keys holding a carriage return.** The gateway writes a row's key without escaping a CR (`crates/gateway-s3/src/lib.rs:3803`), and XML reads a raw CR or CRLF as LF. Such a key is deleted, but its row comes back with an LF, so it is reported under both `keys_without_a_row` and `rows_not_requested`. A test pins this. When the gateway is fixed, that test fails on purpose and one clause of the `document.rs` doc should go.
- `head_object` still panics on a key long enough to overflow the URI. That is #955's verb and is out of scope here.
- No test uses `~` in a bucket name, so removing it from `path_segment`'s unreserved set would go unnoticed (`crates/validate/src/s3.rs:549`).

## Verification
Paths are under `crates/validate/`. Line numbers are for this patch applied to `main` at `fd64faf`. Tests are in `tests/s3_client_smoke_verbs.rs`.

| Claim | Checked in | Test |
|---|---|---|
| Bulk delete against the real gateway: every key under `<Deleted>`, no error row, keys then gone (HEAD 404) | `src/s3/verbs.rs:361`, `src/s3/document.rs:243` | `:569` |
| A mixed result is reported row by row, in document order, with each error row's key, code and message | `src/s3/document.rs:85`, `:150` | `:3102` |
| Requested keys with no row, and rows naming no requested key, are reported, not judged | `src/s3/document.rs:112`, `:121` | `:3184` |
| An unreadable delete result is refused beside its 200, never partial; one of exactly the budget is read | `src/s3/document.rs:243` | `:2677` |
| The documented size limits hold: 41 error rows fit, 42 do not | `src/s3/verbs.rs:347-359` | `:746` |
| Every probe reaches the gateway and returns its status and code (501 `NotImplemented`, 400 `InvalidRequest`, `HeadBucket` 400 with no body) | `src/s3/verbs.rs:69`, `:404` | `:1191` |
| Each probe sends exactly one request with its own method, path, operation parameters (with the values given) and `x-amz-copy-source`, and none of another's, so a wrong operation cannot pass by hitting the gateway's shared 501 branch | `src/s3/verbs.rs:404` | `:1225` (via `Relay`, `:851`) |
| Bucket names, including `--x-s3`, `--xa-s3`, `--op-s3`, `arn:` and names ending `-`, arrive as one path segment on an IP endpoint and a host-name endpoint | `src/s3.rs:492-549` | `:1300` |
| An endpoint path comes before the bucket on every request, existing verbs included | `src/s3.rs:509` | `:1375` |
| The region is signed as given | `src/s3.rs:440-491` (doc) | `:1782` |
| A request that cannot be built is `RequestNotBuilt` and nothing is sent | `src/s3/verbs.rs:605` | `:1466`, `:1582` |
| A probe that got no answer is `NoResponse`; under each deadline it is `Timeout { Connect }` or `Timeout { Operation }` | `src/s3.rs:361` (`classify`) | `:1723`, `:1847`, `:1868` |
| A probe's 200 survives a body that stalls, trickles, is cut or is not its success document | `src/s3/answer.rs:336` (`SuccessBody`) | `:2378` |
| Each probe's correct success is a success, checked against the request it answered | `src/s3/answer.rs:344` (`SuccessBody::read`) | `:4810` |
| A probe body over the 64 KiB budget is refused beside its 200; one of exactly the budget is read | `src/s3/verbs.rs:404` | `:4894` |
| A `304` with a chunked body is refused for a probe that is not a GET or HEAD, and for a bulk delete | `src/s3/response.rs` | `:4987`, `:5040` |
| A `200` carrying an S3 `<Error>` document keeps the 200 and reports its code and message | `src/s3/answer.rs` | `:5076` |
| A key holding a CR is deleted, and its row comes back with an LF | `src/s3/document.rs:75-83` | `:614` |
| The existing verbs still pass | `tests/s3_client_roundtrip.rs`, `tests/s3_client_upload_peers.rs`, `tests/s3_client_nonconforming_responses.rs`, all unmodified | all pass |

**Fails before, passes after:** with the production change reverted, the test file does not compile, because the verbs do not exist on `main`. So the failures were shown with single edits to production instead. Each edit compiled and turned the named tests red:
- putting the SDK's own endpoint rules back (bucket-name routing)
- dropping a delete result's `<Error>` rows (mixed result)
- reporting a request refused before sending as `NoResponse` (not-built)
- returning only the timeout when a probe's body stalls after a 2xx (200 survives)
- turning a probe's expired deadline into `NoResponse` (deadline tests)
- sending `UploadPart` with a different `uploadId` from the one the probe named (request checks)
- turning a probe's over-budget body into `NoResponse` (budget test, all 14 over-budget legs)
- normalising line endings in a requested key before the lookup (both CR tests)

**Other checks:**
- `cargo xtask ci` passes.
- Changed-line coverage is 97.9% (280 of 286 instrumentable lines).
- `cargo mutants --in-diff` tested 64 mutants: 47 caught, 14 unviable, 0 missed, 3 timeouts. The timeouts are in `tests/s3_client_nonconforming_responses.rs`, which this PR does not touch.

Fixes #956
