## Summary
**User impact:** anyone using the validator to check an S3 endpoint can only test writing, reading and deleting a single object. It cannot check whether the endpoint answers correctly when asked for an object's details, part of an object, an object only if it has changed, or a list of objects. Even with those requests added through the AWS library alone, a list cut off halfway, a reply that stalls after a success status, or a "not modified" reply could not be reported as what the server really sent.

This PR adds those read requests to the validator's S3 client, and each one reports the server's answer exactly as it arrived. Nothing but tests calls them yet; the planned smoke run and the remaining request types will build on them.

## What to look at
The main idea: the client records the reply's status and headers as soon as they arrive, then reads the body itself. If the body then goes wrong, the report shows both the success status and the failure. A list is returned complete or not at all. The client reports answers and does not judge them.

To try it: `cargo test -p wyrd-validate --test s3_client_smoke_verbs`. It needs no Docker and no network beyond `127.0.0.1`.

The patch is large (about 183 KB), but most of it is the new test file. A good reading order is the reply capture, then the requests, then the list parsing.

## Root cause
The client only had `put_object`, `get_object` and `delete_object` (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main`). The SDK deserializes responses itself, and it accepts cut-off XML and can replace an unparseable header, so its output cannot show what was on the wire.

## Fix
- **Capture (`s3/answer.rs`):** a per-call interceptor records the head at `read_after_transmit`, before the SDK's later hook can change headers, then takes the body. `read_whole` reads the body under `BUFFERED_BODY_BUDGET` and the body deadlines. Failures are typed (`BodyFailure`) and kept beside the head.
- **Verbs (`s3/verbs.rs`):** `head_object`, `get_object_range`, `get_object_if_none_match`, `list_objects_v2` and `list_objects_v1`. A 2xx GET body still streams. The two GET verbs turn off the SDK's response checksum handling for the call, so the body is read as sent and no checksum is requested.
- **Listings (`s3/document.rs`):** pages are read strictly, and a field that is repeated, holds markup or is missing where S3 always sends it is refused. Following pages stops at `MAX_LISTING_PAGES`, and a later bad page discards the earlier ones.
- **Shared code (`s3/response.rs`, `s3.rs`):** `whole_document` and `object_length` are extracted so the new and existing paths use the same rules. The existing verbs' behaviour is unchanged, and `Framing::of` is untouched.
- **Replies with no body:** `framing_of_no_body` accepts a `Transfer-Encoding` alone on a `304` to a GET or on a response to a HEAD, as RFC 9112 §6.1 allows. The framing that may not be sent stays refused: both headers together, or a `204` with a `Transfer-Encoding`.

**Known limits and follow-ups**
- A head that hyper itself rejects (for example `Content-Length: abc` on a listing page or a GET) comes back as `S3Error::NoResponse` with the status lost, because hyper's error carries no status. The same header on a HEAD keeps its status, so the two reports differ. This already applies to `get_object`, and separating the cases means changing `s3/wire.rs:130-136`, which the existing verbs share. The doc comments at `answer.rs:34-35`, `:176-178` and `:309-310` and `error.rs:41-42` overstate the guarantee here; #956 is to correct them.
- Production handled three cases correctly when they were tried by hand, but no test here pins them: the deadlines on a ranged or conditional GET's 2xx body, the early header capture when `Expires` is invalid, and an empty `ListObjectsV2` continuation token. #956 is to add those tests.
- A `304` that declares a `Content-Length` is recognised by matching the SDK's error text (`answer.rs:606`), because the error type is private. If that text changes, the `304` reads as a failure rather than a success, and a test goes red.
- The existing `get_object` keeps the checksum handling the new GET verbs turn off. It is unchanged here.
- These verbs are not yet in the architecture doc; that is deferred to #956 (`verbs.rs:42`).

## Verification
Paths are under `crates/validate/`. Line numbers are for this patch applied to `main` at `376b106`. Tests are in `tests/s3_client_smoke_verbs.rs`.

| Claim | Checked in | Test |
|---|---|---|
| Every read verb works against a real in-process gateway: size and `ETag`, a bare 404, a 206 with exact bytes, 304 and 200, all keys across pages | `src/s3/verbs.rs:44`, `:62`, `:86`, `:115`, `:149` | `:282`, `:325`, `:363`, `:437` |
| A 200 survives a body that stalls, trickles, is cut or is not a listing | `src/s3/answer.rs:527`, `src/s3/verbs.rs:287` | `:783` |
| Unreadable pages are refused, never returned in part, and a page of exactly the budget is read | `src/s3/document.rs:93`, `src/s3/response.rs:462` | `:1029`, `:1258`, `:1300`, `:2695` |
| Continuation is followed and bounded, and every header line is kept | `src/s3/document.rs:21`, `:125-140`; `src/s3/answer.rs:457` | `:1349`, `:1406`, `:1450`, `:1529`, `:1577` |
| Replies with no body are not refused for legal framing, and forbidden framing stays refused | `src/s3/answer.rs:499` | `:1647`, `:1694`, `:1783`, `:1843` |
| An S3 error through each new verb keeps its status, headers, code and message | `src/s3/answer.rs:333` | `:2528` |
| GET bodies are read as sent, with no checksum requested | `src/s3/verbs.rs:34` | `:2262`, `:2321` |
| A 2xx GET whose framing is unusable or whose body is cut keeps its status | `src/s3/verbs.rs:257` | `:2075`, `:2130`, `:2182` |
| The existing verbs are unchanged | `tests/s3_client_roundtrip.rs`, `tests/s3_client_upload_peers.rs`, `tests/s3_client_nonconforming_responses.rs`, all unmodified | all pass |

**Fails before, passes after:** with production reverted, the test file does not compile, because the verbs do not exist on `main`. That makes a plain before/after run impossible, so the failures were shown with single edits to production instead. Each edit compiled and turned named assertions red:
- returning only the timeout when a body stalls after a 2xx
- accepting a cut listing
- stopping after the first page (v1 and v2 separately)
- reporting every error as `NoBody`
- refusing a `304`'s framing
- ignoring each rule in the listing refusals
- dropping ETags

**Other checks:**
- `cargo xtask ci` passes.
- Changed-line coverage is 95.2% (573 of 602 instrumentable lines).
- `cargo mutants --in-diff` tested 113 mutants: 78 caught, 35 unviable, 0 missed. That tool has no mutation for the listing refusal rules, so the hand edits above are the evidence for those.

Fixes #955
