# Accept SDK-default checksum-trailer streaming PUTs

## Summary
**User impact:** uploading a file to the S3 gateway with a current, default-configured
AWS client — `aws s3 cp`, boto3, or a modern AWS SDK — fails with "403 Forbidden", even
though the credentials are correct. Modern clients switched their default upload format
to one that appends an integrity checksum at the end of the stream, and the gateway
rejected that format outright, so a stock client could not upload at all.

This PR teaches the gateway to fully consume and verify that checksum-trailer upload
format, so default-configured clients work — without loosening the gateway's rule that
it never accepts data it cannot completely check.

Reported in #505.

## What to look at
The change is confined to the S3 gateway crate: the streaming body decoder (which now
reads and validates the trailing checksum section) and the request-authentication layer
(which now admits the two trailer upload formats it previously refused). The object
write path is untouched.

To try it: run `cargo test -p wyrd-server --test s3_streaming_trailer` (an end-to-end
wire test over a real loopback listener), or point a default-configured `aws s3 cp` at
`wyrd s3` — on `main` it 403s; with this PR it round-trips.

## Root cause
`streaming_variant` (`crates/gateway-s3/src/sigv4.rs:510-515` on `main`) is a
deliberately closed set that maps `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` and
`STREAMING-UNSIGNED-PAYLOAD-TRAILER` to `None`, so `verify` refuses them as
`AuthError::Malformed`, which the wire layer maps to 403
(`crates/gateway-s3/src/lib.rs:514-519`). The refusal was correct at the time: the
`aws-chunked` decoder simply returns after the terminating zero-length chunk and can
neither consume nor reject trailer bytes (`crates/gateway-s3/src/streaming.rs:221-250`),
so admitting a `-TRAILER` sentinel would have accepted a framing the pipeline cannot
fully verify — the "no half-accept" invariant this codebase enforces (issue #364
carry-forward). Meanwhile, modern SDKs made exactly that framing their default.

## Fix
Make the trailer framing fully consumable and verified, then admit it:

- `streaming.rs` — the decoder consumes the trailer section after the zero-length chunk
  (`consume_trailer`, bounded by `MAX_TRAILER_SIZE`, fail-closed on malformed framing),
  verifies the trailer signature for the signed variant (`sign_trailer`, per AWS's
  published trailing-header string-to-sign), and validates the declared
  `x-amz-checksum-*` value against the bytes actually streamed — never
  consumed-and-trusted.
- `checksum.rs` (new) — `crc32` (in-tree table-driven IEEE CRC-32, the aws-cli/boto3
  default), `crc32c` (the workspace's already-vetted crate — no new external
  dependency), `sha256`, plus a canonical-only base64 decoder. Unrecognised algorithms
  (`crc64nvme` included) are refused before any body is read.
- `sigv4.rs` — admits the two `-TRAILER` sentinels and parses/validates the declaration
  headers up front: the consumed trailer name must match `x-amz-trailer`, and a declared
  `x-amz-decoded-content-length` must match the decoded byte count.
- `lib.rs` — a checksum mismatch maps to 400 `BadDigest` (distinct from a signature
  failure, which stays 403); the object is never committed on any refusal.

## Verification
- **Claim:** a `STREAMING-UNSIGNED-PAYLOAD-TRAILER` PUT and a
  `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` PUT, each declaring an
  `x-amz-checksum-{crc32,crc32c,sha256}` trailer, are accepted and round-trip
  byte-identical through GET; every unverifiable trailer is refused fail-closed.
- **Checked:** `crates/gateway-s3/src/sigv4.rs:510-515` on `main` — the closed set
  returning `None` for both `-TRAILER` sentinels; `crates/gateway-s3/src/streaming.rs:221-250`
  on `main` — the decoder returning after the zero chunk without consuming or rejecting
  trailer bytes; `crates/gateway-s3/src/lib.rs:514-519` on `main` — the 403 mapping a
  stock client hits.
- **Oracles:** trailer signing is pinned to AWS's own published worked example
  (`aws-c-auth` `sigv4_trailing_headers_signing_test`, chained off the module's existing
  pinned chunk-signing example); CRC-32 is pinned to the standard `"123456789"` check
  value and cross-checked against `zlib.crc32`; base64 against the RFC 4648 vectors.
- **Test:** `crates/server/tests/s3_streaming_trailer.rs` — built as a pure S3 client
  from primitives that exist on `main`, so it compiles unchanged on the base tree and
  fails there with real wire-level 403s (9 of 11 tests); passes 11/11 with the fix. It
  covers both sentinels, all three algorithms, a forged trailer signature (403, nothing
  stored), a tampered checksum (400 `BadDigest`, nothing stored), an unrecognised
  algorithm, bad base64, an undeclared trailer name, garbage after the trailer block,
  and a decoded-content-length mismatch. Existing streaming/SigV4 tests are unchanged
  and green; `fmt`/`clippy -D warnings`/workspace tests/`cargo deny` all pass.

Fixes #505
