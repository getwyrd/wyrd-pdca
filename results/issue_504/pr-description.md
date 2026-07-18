# PR description

## Summary
**User impact:** copying an object through the S3 gateway silently destroys the
destination. A copy request (`aws s3api copy-object`, or any SDK's CopyObject call)
returns success — but instead of copying, the gateway replaces the destination object
with an empty one. Anyone copying onto an existing key loses that object's data, with
no error to warn them.

This PR makes the gateway refuse copy requests outright — `501 NotImplemented`, with
the destination left untouched — until server-side copy is actually implemented.

Reported in #504.

## What to look at
A single early-return guard at the top of the S3 gateway's PUT handling: if the request
is a copy (it carries the copy-source header), refuse it before reading any of the
body. Ordinary uploads are unaffected.

To try it: against a running `wyrd s3` gateway, upload an object, then
`aws s3api copy-object --copy-source src/k --bucket dst --key k` onto it. Before this
change the command "succeeds" and the destination becomes empty; after it, the command
fails with `NotImplemented` and the destination still reads back its original bytes.

## Root cause
The gateway dispatches by HTTP method only and never read the `x-amz-copy-source`
header (zero mentions in `crates/gateway-s3/` before this change), so a CopyObject —
which is a `PUT` whose *body is empty* (the payload is the copy-source reference) —
fell through to the ordinary PutObject path. That path streamed the empty body into
the destination key and answered 200.

## Fix
Guard at the top of the `Method::PUT` arm, before any body byte is consumed
(`crates/gateway-s3/src/lib.rs:577-584`): if `x-amz-copy-source` is present on the
request headers, return `501 NotImplemented` (S3 error body, code `NotImplemented`)
via the existing `error_response` helper. This mirrors the subresource guard directly
above (`crates/gateway-s3/src/lib.rs:548-561`), which refuses `?uploadId`/`?tagging`
forms for the same reason: a request form the gateway does not implement is refused,
never silently mishandled. The header is checked on the raw request headers, so the
guard applies whether or not the client put it in its SigV4 signed-header set; SigV4
verification itself is unchanged.

Real server-side copy (resolve the source object, alias its chunk map, return the
source ETag) is the follow-up step of #504 and depends on the object-metadata model
(#503) — deliberately out of scope here. The guard itself is independent of the
in-flight #503/#505 work; the diff context merely overlaps the same PUT dispatch.

## Verification
- **Claim:** a SigV4-signed PUT carrying `x-amz-copy-source` is refused with
  `501`/`NotImplemented` and the destination object's prior content survives
  byte-identical; an ordinary PUT (no such header) still stores normally.
- **Checked:** `crates/gateway-s3/src/lib.rs:577-584` — the guard returns before the
  `content-type` read and before the body stream is constructed, so no byte of the
  refused request can reach the store; placement mirrors the subresource refusal at
  `crates/gateway-s3/src/lib.rs:548-561`.
- **Test:** `crates/server/tests/s3_copy_object_guard.rs` — fails pre-fix (the
  copy-form PUT returns 200 and a follow-up GET returns zero bytes), passes post-fix.
  It drives the production wire path end to end: a real loopback TCP listener served
  by `S3Gateway::serve`, requests signed with the production `sigv4::sign`, redb
  in-memory metadata plus an fs chunk store in a tempdir. Two cases:
  `copy_source_put_is_refused_and_destination_survives` (refusal + byte-identical
  destination) and `ordinary_put_without_copy_source_still_stores` (no regression of
  the normal upload path).
- **Gate:** `cargo xtask ci` (fmt `--check`, clippy `-D warnings`, build, full test
  suite incl. DST, `cargo deny`, conformance) — all checks passed.

Fixes #504
