## Summary
**User impact:** the S3 gateway accepts and stores an object under a key of any length.
Real S3 refuses keys longer than 1,024 bytes, so a client could create objects here that
it could never create on S3, and that S3 tools may fail to copy, sync or migrate. A second
spelling of the same request (an encoded `/` in the bucket name) slipped part of the key
past any length check as well.

This PR makes object PUT refuse a key over 1,024 bytes with S3's own `KeyTooLongError`,
and refuse a bucket name containing `/`. Reading and deleting are unchanged, so objects
that already have long keys stay removable.

## What to look at
- The two new refusals in the object PUT handler of the S3 gateway. They run on the
  request headers, before any of the body is read, and before the code splits on how the
  body is sent, so they apply equally to plain signed uploads, the AWS SDK's default
  chunked uploads, and unsigned uploads.
- What is deliberately *not* limited: bulk delete. A key longer than about 64 KiB can't be
  put in a DELETE URL at all (the HTTP layer answers `414`), so bulk delete is the only S3
  way to remove one. Refusing long keys there would leave such objects stuck. The
  rationale is written up in the architecture doc and in the doc comments.

To try it: start the gateway, create bucket `b`, then
`PUT /b/<'a' × 1025>` → before: `200` and stored; after: `400 KeyTooLongError` and HEAD
`404`. `PUT /b%2Fa/<'x' × 1023>` → before: `200`, stored as `b/a/xxx…`; after:
`400 InvalidBucketName`.

## Root cause
No key-length check existed anywhere in `gateway-s3`, `gateway-core` or `server`: the PUT
arm percent-decodes bucket and key, joins them into one flat key and streams the body into
the backend. Because the path is split on the first literal `/` *before* decoding, a `%2F`
in the bucket segment moves bytes from the key into the bucket without changing the stored
name.

## Fix
- `crates/gateway-s3/src/lib.rs`: new `MAX_KEY_BYTES = 1024`, with a doc comment that says
  it is enforced only where an object is created. The PUT arm keeps the decoded bucket and
  key apart, then refuses a bucket containing `/` (`400 InvalidBucketName`) and a key over
  `MAX_KEY_BYTES` (`400 KeyTooLongError`). Both checks sit next to the existing CopyObject
  refusal and use the same `error_response` helper.
- The `MAX_DELETE_BODY_BYTES` doc comment no longer leaves open whether bulk delete has a
  per-key limit: the 8 MiB body cap is the only bound on one `<Key>`, on purpose. The
  parser itself is unchanged.
- `docs/design/architecture/05-building-block-view.md`: one paragraph on the limit, where
  it applies, why bulk delete has no per-key bound, and the remaining gap (a key over the
  8 MiB body cap, which only the in-process `Gateway::put_object` can write, had no S3 way
  out before and still has none; #893 tracks a native key limit).
- Tests: a new wire-level suite and four in-crate tests (below).

Out of scope: CopyObject (still `501`), multipart (#508 applies the same rule when it
lands), the `wyrd put --key` CLI and a native limit (#893), and the 8 MiB cap value.

## Verification
- **Claim:** a PUT of a key over 1,024 bytes is refused and nothing is stored.
  **Checked:** `crates/gateway-s3/src/lib.rs:1864-1871` (the check), `:489-502`
  (`MAX_KEY_BYTES`). **Test:** `crates/server/tests/s3_key_length_limit.rs:466`
  (`put_of_an_over_limit_key_is_refused_and_stores_nothing`) — `400 KeyTooLongError`,
  then HEAD `404` and the key missing from a listing.
- **Claim:** the limit counts UTF-8 bytes of the decoded key, not characters, wire bytes,
  or `bucket/key`. **Test:** `s3_key_length_limit.rs:498`
  (`limit_counts_utf8_bytes_of_the_decoded_key_alone`) — 512 × `é` (1,024 bytes) is
  stored, 342 × `€` (1,026 bytes) is refused, a 1,024-byte key sent fully percent-encoded
  is stored.
- **Claim:** `%2F` in the bucket cannot be used to get round the limit.
  **Checked:** `lib.rs:1853-1860`. **Test:** `s3_key_length_limit.rs:687`
  (`put_with_a_slash_in_the_decoded_bucket_is_refused`) — `PUT /b%2Fa/<'x' × 1023>` is
  `400 InvalidBucketName`, and HEAD `/b/a/<'x' × 1023>` is `404`.
- **Claim:** both refusals hold for every upload form, not just signed single-shot bodies.
  **Checked:** both checks come before the `match payload` dispatch (`lib.rs:1924`;
  `Streaming` `:1931`, `Signed` `:1946`, `Unsigned` `:1959`). **Test:**
  `s3_key_length_limit.rs:708` (`refusals_hold_for_aws_chunked_and_unsigned_payload_puts`)
  — for `aws-chunked` and `UNSIGNED-PAYLOAD`: a 1,024-byte key is stored, a 1,025-byte key
  and the `%2F` bucket are refused and absent afterwards. Moving the checks into the
  `Signed` arm only makes this test fail.
- **Claim:** no delete path gains a length refusal. **Tests:**
  `s3_key_length_limit.rs:535` — one DeleteObjects removes `b/ok`, a 1,025-byte key and a
  70,000-byte key (past the `414` limit) with no `<Error>` row;
  `:611` — a key of 8,388,563 bytes that fills the 8 MiB body exactly is deleted;
  `:662` — single-object DELETE still removes an over-limit object (HEAD `200` → DELETE
  `204` → HEAD `404`). These pass before and after, by design: they guard against a future
  bulk-delete bound.
- **In-crate tests** (`crates/gateway-s3/src/lib.rs` `mod tests`):
  `an_object_put_refuses_an_over_limit_key_before_the_write_seam`,
  `an_object_put_refuses_an_over_limit_key_in_every_payload_form` (also checks that a
  refused PUT never reaches the backend write), and the two bulk-delete tests.
- **Red/green:** with the `lib.rs` and doc changes reverted and the new test file kept,
  `cargo test -p wyrd-server --test s3_key_length_limit` gives 3 passed, 4 failed (the
  over-limit PUT, UTF-8 byte count, `%2F` bucket and payload-form tests); with the fix,
  7 passed. `cargo xtask ci` passes on the branch.

Fixes #620
