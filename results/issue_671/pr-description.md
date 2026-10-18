## Summary
**User impact:** an S3 client that uploads a single object larger than the gateway can
store is not told so up front. On FoundationDB it uploads every byte and only then gets an
error. On redb the upload "succeeds" and stores an object larger than the storage format is
sized for. The limit is 158 × `--chunk-size`: 158 MiB at the default 1 MiB, about 2.47 GiB at
the 16 MiB maximum.

This PR makes the gateway refuse such an upload with `400 EntityTooLarge` as soon as it
reads the request headers, before any of the body is sent or stored. To make that check
possible, a PUT must now declare its length, as S3 itself requires; one that does not gets
`411 MissingContentLength`.

## What to look at
- **The limit is now real and stated.** One PUT can store at most 158 × `--chunk-size`
  bytes. The `wyrd s3 --chunk-size` help text and the M4 blueprint say so in numbers. They
  give multipart's larger limits as future ones, because multipart still answers
  `501 NotImplemented` on this gateway. Until it ships, the way to accept bigger objects is
  a larger `--chunk-size`.
- **Who sees a change.** Uploads within the limit that declare their length (every AWS SDK
  and the AWS CLI) behave as before. Changes:
  - an oversize upload is refused at once;
  - an upload with no declared length gets `411`;
  - a streamed upload that sends more or fewer bytes than it declared is refused.
- **The chunk size stays a deployment setting.** The issue first proposed picking a chunk
  size per upload. That was dropped: it would quietly turn `--chunk-size` (#848) into a
  minimum, and reaching S3's 5 GiB that way needs about 32 MiB chunks, above the 16 MiB
  maximum.
- **Try it:** start `wyrd s3 --chunk-size 1048576`, then `curl -X PUT` an object with
  `Content-Length: 165675009` (one byte over 158 MiB) and send no body. You get `400
  EntityTooLarge` right away instead of the server waiting for the body.
- **Depends on other open work.** This branch is built on the changes for #666, #686 and
  #767, and touches the same PUT handler as #620 and #768. Whichever lands first, the other
  needs a rebase. Check the order of the header-time refusals against #620's once both are on
  the same handler.

## Root cause
A single PUT is one flat chunk map, and a flat map is budgeted for at most `MAX_MAP_CHUNKS`
= 158 references. No code path compared a PUT against that. On top of that, the gateway
could not check it from the headers: it read `x-amz-decoded-content-length` only for the
`-TRAILER` streaming forms (and treated it as optional there), and it compared the declared
length with the streamed bytes only after the whole body had arrived.

## Fix
- **One limit, defined in core.** `max_single_put_bytes(c)` sits beside `max_part_bytes(c)`.
  Both come from one private `one_value_bytes(c)`, and compile-time asserts tie both map
  caps to the same per-value capacity, so a single PUT and a multipart part can never
  disagree about what one value holds.
- **The declared length crosses the gateway seam.** `ObjectGateway::put_object_streaming`
  gains a `declared_len` argument, and `GatewayError` gains `EntityTooLarge { declared, limit }`
  and `LengthMismatch { declared }`. The server checks the limit itself, before it reads the
  body. It also wraps the body so it cannot run past, or stop short of, what was declared. So
  any front door gets the limit, not just S3. The buffered `Gateway::put_object` runs the same
  check before it plans any chunk.
- **S3 wire.** The PUT handler reads the declared length from the headers:
  `x-amz-decoded-content-length` for every `aws-chunked` form, `Content-Length` otherwise.
  Both are parsed as strict digits only. It answers `411` when there is none (a
  `Transfer-Encoding` body with no `Content-Length` counts as none). The `aws-chunked` decoder
  now refuses a chunk *on its header* if it would pass the declared length, checks for a
  short body on every streaming form, and starts reading the body only when first polled, so
  a request refused from its headers never has its body read.
- **Compile-time pins.** `158 × MAX_CHUNK_SIZE` stays under S3's 5 GiB, so no runtime 5 GiB
  check is needed. Raising `MAX_CHUNK_SIZE` past about 33 MiB fails the build until one is
  added. The multipart object limit stays above 10 GiB even at the smallest chunk size.
- **Docs.** The `--chunk-size` help, the M4 blueprint comment, the building-block view's
  description of the gateway seam, and draft proposal 0016's single-PUT text (the carve-out,
  rows X50/X102/X107, the accepted-cost rows and summaries) now describe this behaviour.
- **Existing tests, migrated.** The streaming test harnesses in `s3_http_wire.rs` and
  `s3_streaming_trailer.rs` now send `x-amz-decoded-content-length`, as the SDKs do. Tests
  whose objects were above their 8-byte fixture's new 1,264-byte limit were shrunk. They still
  span several chunks and several wire frames, and each refusal test is still refused for its
  own reason (tampered chunk, mismatched checksum, …), not for its size. The `validate`
  round trip sizes its gateway's chunk from its payload, so it stays one PUT for any
  `tcp_wmem` (the Linux socket send-buffer setting it reads), and fails with a clear message
  if it cannot.

**Known gaps, left for follow-ups:**
- `wyrd put` (the CLI) writes through `write::write_new_object` and does not go through this
  check. `wyrd put --chunk-size 8` of a 1,265-byte file still stores 159 chunks. The doc
  comment on `admit_single_put` says "every single-PUT path", which is true of the gateway's
  paths, not of the CLI. Moving the check into core's `write_new_object` would close it.
- `LengthMismatch` maps to `400 IncompleteBody`, but over HTTP a plain body shorter than its
  `Content-Length` is cut off by hyper first and answers `500`. The upload is still refused
  and nothing is stored.
- No test drives `put_object_streaming` directly with a body longer than `declared_len`. Over
  S3 the decoder or hyper catches that first. The server-side guard is correct when probed by
  hand but not pinned by a test.
- Draft proposal 0017 still describes the old per-PUT chunk-size plan (`:291-292`, `:313`,
  `:323`, `:326`).

## Verification
Line numbers are on this PR's branch.

- **Claim:** a PUT at exactly 158 × chunk size is stored and reads back byte for byte.
  - **Checked:** `crates/server/src/lib.rs:291-297`. `admit_single_put` refuses only
    `declared > limit`.
  - **Test:** `crates/server/tests/s3_put_object_ceiling.rs:359`
    (`a_put_at_the_ceiling_round_trips`, 1,264 bytes on an 8-byte chunk).
- **Claim:** one byte over is refused `400 EntityTooLarge`, and nothing is stored (HEAD
  `404`, no chunk on disk).
  - **Checked:** `crates/gateway-s3/src/lib.rs:3587-3591`. The refusal names the limit in
    bytes and does not mention multipart.
  - **Test:** `s3_put_object_ceiling.rs:383`.
- **Claim:** the refusal comes from the headers, not from reading the body.
  - **Checked:** `crates/server/src/lib.rs:417`. The check runs before the body stream is
    polled. The PUT handler reads the declared length at `crates/gateway-s3/src/lib.rs:1834-1864`.
  - **Test:** `:418` declares `Content-Length: 2147483648`, sends nothing, and is answered
    within the time limit. `:437` does the same for a streamed upload whose
    `x-amz-decoded-content-length` is 1,265.
- **Claim:** a PUT with no declared length is refused `411 MissingContentLength`, and nothing
  is stored.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1852`, `:1957` (`declared_content_length`), and
    `crates/gateway-s3/src/sigv4.rs:590`, where the decoded length is now read for every
    streaming form.
  - **Tests:** `:460` (plain streamed), `:479` (trailer form), `:513`
    (`Transfer-Encoding: chunked`).
- **Claim:** a streamed upload is held to its declared length while it streams.
  - **Checked:** `crates/gateway-s3/src/streaming.rs:369` refuses on the chunk header, and
    `:581` (`check_decoded_length`) catches a short body. On the server side,
    `DeclaredLengthSource` at `crates/server/src/lib.rs:921` has unit tests at `:1078-1100`.
  - **Tests:** `:539` declares 1,000 bytes and streams 1,265: refused, HEAD `404`. `:564`
    streams fewer bytes than declared: refused.
- **Claim:** the buffered `Gateway::put_object` is bounded by the same limit.
  - **Checked:** `crates/server/src/lib.rs:258`. The check runs before `plan_write`.
  - **Test:** `:582`. 1,264 bytes is stored. 1,265 returns `Err`, the chunk count is
    unchanged, and `get_object` returns `None`.
- **Claim:** the single-PUT limit and the part limit come from one rule.
  - **Checked:** `crates/core/src/multipart.rs:4784-4808` (`max_part_bytes`,
    `max_single_put_bytes`, `one_value_bytes`, and the two asserts). The 5 GiB and 10 GiB pins
    are at `crates/server/src/lib.rs:108-119`.
- **Regression test:** `crates/server/tests/s3_put_object_ceiling.rs`, 11 tests. With the
  fix reverted, 9 fail. Two pass on both trees, as expected: the at-limit round trip, which is
  legal before and after, and a check that the test's own request signer matches the
  gateway's. With the fix, all 11 pass. The wire tests go through the real loopback listener,
  SigV4 verification, the `aws-chunked` decoder and the gateway, on redb with a real
  filesystem chunk store, with no test doubles.
- **Whole gate:** fmt, clippy, the full workspace test suite, `xtask conformance` and
  `xtask dst` all pass. `cargo xtask ci` stops at `cargo deny … check advisories` on two new
  RustSec advisories for `rustls 0.21.12`, which comes in through `tikv-client 0.4.0`. That
  lockfile entry is unchanged by this PR, so `main` fails the same check. This PR's only
  lockfile change adds `wyrd-core` as a dev-dependency of `wyrd-validate`.

Fixes #671
