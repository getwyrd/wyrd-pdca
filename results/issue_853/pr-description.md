## Summary
**User impact:** `wyrd-validate` can report a broken or misbehaving S3 endpoint as healthy.
When a server sends a response no real S3 server would send (an empty error, an HTML error
page, a half-written error, an object download that stops early), the validator's client
often reports it as a normal answer, or as a smaller but complete object. It also reads
some responses fully into memory with no limit, so one large junk error can grow the
validator to hundreds of megabytes. Because the validator exists to judge a deployment, a
wrong fact here becomes a wrong verdict.

This PR makes the client strict: a response that is not a whole, correctly framed S3
response is reported as unreadable (or, for a download, as a body error), the same way on
GET, PUT and DELETE. Error bodies are capped at 64 KiB as they arrive. Normal downloads of
any size are not affected, and chunked downloads, which the client used to refuse, now
work.

**Depends on #931** (payload derivation, #744). This branch is built on top of it and its
test uses that PR's `sha2` dependency, so #931 should merge first.

## What to look at
- **The crux:** the client now reads S3 error bodies itself instead of trusting the SDK,
  checks how each response marks its end, and counts response bytes against a budget as
  they arrive. To see what the server sent *after* a response ended, it uses a small HTTP
  connector of its own instead of the SDK's.
- **Try it:** `cargo test -p wyrd-validate --test s3_client_nonconforming_responses`. No
  Docker, no network: each test serves exact bytes from a loopback socket and calls the
  real client. For example, a `404` with `Content-Length: 0` used to come back as error code
  `NotFound`; it now comes back as "no body".
- **Trade-off to weigh:** every request now uses a fresh TCP connection, not only PUTs. The
  existing `TIME_WAIT` cap (about 470 requests a second per client/gateway address pair on
  Linux defaults) now covers GET and DELETE too. This is documented in `s3.rs` and the
  building-block view.

## Root cause
The client trusted the AWS SDK's reading of every response. The SDK fills in a code for a
bodiless 404, accepts cut-off or trailing XML, checks body length on `GetObject` only, and
for every response it does not stream (every error, every PUT or DELETE response) reads the
whole body before any client code runs. Its HTTP connector also hides the connection, so
bytes past a response's end could not be seen.

## Fix
- `crates/validate/src/s3/response.rs` (new): `ResponseGuard`, an SDK interceptor added to
  every call, records a GET success's framing and refuses framing that cannot mark an end
  (both `Transfer-Encoding` and `Content-Length`, a coding other than exactly `chunked`, a
  `204` that frames a body, a `Transfer-Encoding` on a PUT/DELETE `304`). It wraps every
  buffered body in a counter: 64 KiB for an error (`BUFFERED_BODY_BUDGET`), zero for a PUT
  or DELETE success. `error_document` reads an error body with `roxmltree`: one whole
  `<Error>`, at most 16 elements deep, text-only `<Code>`/`<Message>`, nothing after it but
  whitespace and comments.
- `crates/validate/src/s3/wire.rs` (new): `WireClient`, a plain-HTTP connector on hyper's
  HTTP/1.1 client connection, one per request. When a body ends it takes the connection
  back and fails the body (`Overrun`) if anything followed. A chunked trailer section is
  dropped, never handed to the SDK. Otherwise the SDK's checksum validator panics on it.
- `crates/validate/src/s3.rs`: uses the new connector and guard; `get_object` accepts a
  correct `Content-Length` or complete chunked framing; `classify` maps error bodies through
  `error_document` and keeps the SDK's diagnostic when it cannot read a success. All
  `// deferred: #853` markers are removed.
- Dependencies: `aws-smithy-http-client` is replaced by `aws-smithy-runtime-api`, `hyper`
  and `hyper-util`, which were already in the shipped graph through it. `roxmltree` moves
  from dev to normal. Net effect on `cargo tree -p wyrd-validate -e normal`: 13 crates out,
  none in.
- Docs: the `s3.rs` module doc and `05-building-block-view.md` describe the strict reading
  and the budget.

## Verification
Line numbers are on this branch.

- **Claim:** an empty error body is "no body", never a code the server did not send.
  **Checked:** `crates/validate/src/s3.rs:339-405` (`classify`) and
  `crates/validate/src/s3/response.rs:363` (`error_document`).
  **Test:** `a_bodiless_error_response_is_no_body_never_a_code_the_body_did_not_carry`
  (`tests/s3_client_nonconforming_responses.rs:875`), on GET, PUT and DELETE.
- **Claim:** HTML, cut-off, trailed, nested or non-text error documents are unreadable.
  **Checked:** `response.rs:363-442` (depth limit at `:442`).
  **Test:** `:961`, `:1003`, `:1074`, `:1166`.
- **Claim:** a body whose length or framing is wrong, or that is followed by extra bytes,
  gets the same verdict on all three operations.
  **Checked:** `response.rs:236-262` (framing refusal), `wire.rs:302-350`
  (`WholeResponse`, end-of-body check).
  **Test:** `:1234`, `:1333`, `:1410`, `:1446`, `:2102`, `:2139`.
- **Claim:** buffered bodies are capped at 64 KiB as they arrive, even with no declared
  length; exactly 64 KiB is still read.
  **Checked:** `response.rs:47`, `:263-273`, `:283-293`.
  **Test:** `an_error_document_at_the_budget_is_read_and_one_byte_more_is_unreadable`
  (`:1754`); `:1881` and `:1890` stream an endless body and assert the endpoint wrote at most
  the budget plus a stated slack (bytes written at the endpoint, not process memory).
- **Claim:** a valid GET is not cut short. A GET four times the budget, and a complete
  chunked GET, come back byte-identical; a close-delimited or cut-short GET is a body error.
  **Checked:** `s3.rs:281-320` (`get_object`).
  **Test:** `:1914`, `:1930`, `:1968`, `:2003`, `:2018`.
- **Claim:** a success the SDK cannot read (for example a bad `Last-Modified`) is unreadable,
  with the real status, request id and the SDK's diagnostic.
  **Checked:** `s3.rs:363-368`. **Test:** `:1490`.
- **Red/green:** with the production change reverted and the test kept, 20 of 28 tests
  fail; with it, 28 of 28 pass. `cargo fmt`, `clippy -D warnings` and `typos` are clean,
  and `cargo xtask ci` is green.

## Known limits, left for follow-ups
Found in review and kept out of this PR's scope:
- A `503` with a mismatched `x-amz-checksum-*` header is unreadable on GET but
  `Service(SlowDown)` on PUT and DELETE, which breaks the "same on all three" rule for that
  one case.
- The 64 KiB budget counts decoded body bytes, not bytes on the wire. A peer that pads
  chunk-size lines can make the client read far more framing than the budget, so the
  docs' "cut off at the budget" is not exact for that case.
- A GET answered with a 2xx other than `200` (`203`, `204`, `206`) is accepted as the whole
  object.

Fixes #853
