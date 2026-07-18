# HeadObject: answer HTTP HEAD with the object's metadata, not a 405

## Summary
**User impact:** Checking whether an object exists — the first thing almost every
S3 client does before a download or an upload — fails outright against the
gateway. `aws s3 cp` downloads abort on their preflight check, and SDK existence
checks report a "method not allowed" error (405) instead of a plain yes/no.
Anyone pointing a stock S3 client or the AWS CLI at the gateway hits this
immediately.

This PR teaches the gateway to answer HEAD requests: a stored object gets a 200
with its metadata headers and no body; a missing key gets a clean 404.

Reported in #506. Builds on the object-metadata model of #503 (PR #594,
commit 76dd913109db98a628dd24fac76bed3b25dfc780), whose headers a HEAD answers
with.

## What to look at
The change is one new HEAD arm in the S3 object dispatch, backed by a small
metadata-only lookup — it reads the object's stored record and never touches the
object's data, so a HEAD of a large object stays cheap. PUT, GET, and DELETE are
untouched.

To try it: run the gateway, upload an object, then
`aws s3api head-object --bucket <b> --key <k>` — you get a 200 with
Content-Length, ETag, Content-Type, and Last-Modified matching what a download
reports; the same command on a missing key gets a 404; and `aws s3 cp
s3://<b>/<k> out` (which previously failed on its HEAD preflight) now succeeds.

## Root cause
The object dispatch (`dispatch` in `crates/gateway-s3/src/lib.rs`) had arms only
for `Method::PUT`, `Method::GET`, and `Method::DELETE`, so every HEAD fell
through to the `_` fallback — on `main` today at
`crates/gateway-s3/src/lib.rs:625-631` — which answers
`405 MethodNotAllowed` ("only object PUT, GET, and DELETE are supported").
Nothing in the stack could resolve a key's metadata without also opening its
fragment stream, so there was no cheap primitive for a HEAD to answer from.

## Fix
Line references below are on this branch, which includes PR #594's metadata
model.

- `crates/gateway-core/src/lib.rs:62-83` — new `ObjectMeta` type: the four
  header-bearing fields a HEAD answers (`size`, `etag`, `content_type`,
  `modified`), deliberately its own body-less type rather than `ObjectRead` with
  the stream ignored, so a metadata-only caller can never conjure or leak a
  stream.
- `crates/gateway-core/src/lib.rs:167-172` — new `head_object` method on the
  `ObjectGateway` trait, beside `get_object_streaming`: metadata only, `None`
  for an absent key.
- `crates/server/src/lib.rs:373-388` — the concrete implementation: the same
  `read::committed_inode` resolution `get_object_streaming` performs before it
  streams (`crates/server/src/lib.rs:326-329`), with the streaming half
  (channel + spawned reader task) omitted.
- `crates/gateway-s3/src/lib.rs:697-742` — the `Method::HEAD` arm: 200 with
  `Content-Length` from the real size and the same
  `content-type`/`etag`/`last-modified` header helpers and fallbacks the GET arm
  uses, with an empty body; `Ok(None)` maps to `404 NoSuchKey` exactly as GET's
  does (`:689-694`). The `_` fallback message now names HEAD (`:753`).
- `crates/gateway-s3/src/lib.rs` — the three in-crate mock gateways gain the new
  trait method (compile-only edits; their existing tests all still pass).
- `crates/server/tests/s3_head_object.rs` — new regression test (see
  Verification).

No `Cargo.toml` changes; no changes to PUT/GET/DELETE code paths; no logging
changes (the access log already classifies HEAD responses as body-less,
`crates/gateway-s3/src/lib.rs:396-407`).

## Verification
- **Claim:** a signed `HEAD /bucket/key` of a stored object returns 200, an
  empty body, `Content-Length` equal to the object's real size, and
  `ETag`/`Content-Type`/`Last-Modified` identical to what a GET of the same
  object returns.
  **Checked:** `crates/server/tests/s3_head_object.rs:231-263` — the test PUTs
  through the real signed wire path, takes a GET response as the oracle, and
  asserts HEAD's status, empty body, and all four headers against it,
  header by header.
- **Claim:** a signed HEAD of an absent key returns 404, headers only.
  **Checked:** `crates/server/tests/s3_head_object.rs:282-291` — asserts 404 and
  an empty body on a never-written key.
- **Claim:** the lookup is metadata-only — no fragment read, no spawned reader
  task, for any object size.
  **Checked:** `crates/server/src/lib.rs:378-388` — `head_object` calls only
  `read::committed_inode` and maps the inode's fields; contrast
  `get_object_streaming`'s streaming half at `crates/server/src/lib.rs:341-363`,
  which it deliberately omits.
- **Claim:** PUT/GET/DELETE behaviour is unchanged.
  **Checked:** `crates/server/tests/s3_head_object.rs:296-358` interleaves HEAD
  with PUT/GET/DELETE and asserts each behaves exactly as without it; the
  pre-existing wire suite `crates/server/tests/s3_http_wire.rs` still passes
  19/19, and the gateway crate's own tests 58/58.
- **Test:** `crates/server/tests/s3_head_object.rs` — fails pre-fix (all three
  tests observe the 405 fallback) and passes post-fix (3/3), exercising the real
  loopback listener and signed requests, no mocks. Workspace fmt,
  clippy `-D warnings`, build, tests, cargo-machete, the cargo-deny audits,
  conformance vectors, and the deterministic-simulation suite are all green on
  the patched tree.

Fixes #506
