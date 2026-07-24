# Honor Range and conditional headers on S3 GET/HEAD

## Summary
**User impact:** Anyone downloading from the S3 gateway got the *entire*
object every time. A `Range: bytes=…` request — the mechanism behind
resumable downloads, media seeking, and "fetch just the first few bytes"
tooling — was ignored and answered with a full 200, and the conditional
requests used for HTTP caching and optimistic concurrency
(`If-None-Match`, `If-Match`, `If-Modified-Since`, `If-Unmodified-Since`)
had no effect at all, so caches could never revalidate cheaply and
concurrent writers had no way to guard an overwrite.

This change makes GetObject and HeadObject honor those standard HTTP
headers: partial content (206, or 416 when the range can't be met) for
ranges, and 304 / 412 for the conditional preconditions, with
`Accept-Ranges: bytes` advertised on every object read.

Reported in #510.

## What to look at
The two request handlers are the S3 GET and HEAD paths — `serve_get` and
`serve_head` in `crates/gateway-s3/src/lib.rs`; the range math and the
covering-chunk read live in the gateway-core seam and the server storage
path. Against a stored object on the loopback gateway:

- `GET` with `Range: bytes=0-15` → `206` with `Content-Range: bytes
  0-15/<size>` and a 16-byte body;
- `GET` with `Range: bytes=999999-` on a smaller object → `416`;
- `GET`/`HEAD` with `If-None-Match: "<current-etag>"` → `304`;
- `GET`/`HEAD` with `If-Match: "<wrong-etag>"` → `412`.

## Root cause
The GET handler read no request headers — it unconditionally streamed the
whole object with a `200` — and the HEAD handler evaluated no
preconditions, so `Range` and every `If-*` header were silently dropped.

## Fix
GET and HEAD now parse the `Range` header and evaluate the conditional
preconditions before any body work. A satisfiable range returns a `206`
whose `Content-Length` is the span (not the object size) and whose
`Content-Range` names the served slice; an unsatisfiable one returns `416`
with `Content-Range: bytes */<size>`. Conditionals are compared against the
stored ETag / modification time using RFC 9110 precedence and S3's
exact-ETag comparison semantics, answering `304` or `412`. The range read
crosses the storage seam as byte offsets, so only the chunks overlapping
the span are fetched — never the whole object discarded after the fact —
and it takes its metadata and its bytes from one object snapshot, so a
`206`'s headers and body always describe the same version. Every object
GET/HEAD now advertises `Accept-Ranges: bytes`.

## Verification
- **Claim:** A signed `GET` with `Range: bytes=a-b` returns `206` with
  `Content-Range: bytes a-b/<size>`, `Content-Length: b-a+1`, and a body
  byte-identical to that slice; the suffix/open forms behave the same, an
  unsatisfiable range is `416`, and an unranged GET carries
  `Accept-Ranges: bytes`.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1654` (`serve_get` — 206/416
    framing and `Accept-Ranges`), against the target branch.
  - **Test:** `crates/server/tests/s3_range_conditional.rs` — the ranged
    and out-of-scope/malformed-range cases (from `:354`).
- **Claim:** A narrow range of a many-chunk object fetches only the
  covering chunks, not the whole object.
  - **Checked:** `crates/server/src/lib.rs:401` (`get_object_range` walks
    the chunk map — covering selection at `:431`, fragment fetch at `:454`).
  - **Test:** `narrow_range_fetches_only_the_covering_chunks` at
    `crates/server/tests/s3_range_conditional.rs:706`, which wraps the real
    chunk store in a counting shim and asserts a `bytes=8-15` read touches
    only the covering chunks while a full GET touches all eight.
- **Claim:** `If-None-Match` / `If-Modified-Since` answer `304` and
  `If-Match` / `If-Unmodified-Since` answer `412`, on both GET and HEAD,
  with RFC 9110 precedence; obsolete HTTP-date formats and a pre-epoch
  `If-Unmodified-Since` resolve correctly.
  - **Checked:** `crates/gateway-s3/src/lib.rs:2079`
    (`evaluate_conditionals` — precedence + S3 comparison) and `:2161`
    (`parse_http_date` — all three RFC 9110 date formats).
  - **Test:** `pre_epoch_if_unmodified_since_fires_412_not_ignored` (`:523`)
    and `obsolete_http_date_formats_are_honored_on_conditionals` (`:603`) in
    `crates/server/tests/s3_range_conditional.rs`.
- **Claim:** HEAD honors `Range` now that it advertises range support —
  satisfiable → body-less `206`, unsatisfiable → `416`.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1749` (`serve_head`).
  - **Test:** `head_honors_range_with_206_and_416` at
    `crates/server/tests/s3_range_conditional.rs:653`.
- **Claim:** A `206`'s headers and body always name the same object
  version, even under a racing overwrite.
  - **Checked:** `crates/server/src/lib.rs:406` — the metadata and chunk
    references come from one committed-inode snapshot, and chunk references
    are content-addressed, so a racing overwrite cannot substitute bytes.
  - **Test:** a version-skew seam guard drives the real router against a
    gateway reporting divergent versions and asserts a stale-snapshot
    `If-Match` still `412`s on both the ranged and unranged paths.
- **Regression trail:** `crates/server/tests/s3_range_conditional.rs`
  drives real signed requests through the loopback S3 gateway. On the
  current code it fails (ranges and conditionals ignored → `200` where
  `206` / `304` / `412` / `416` are expected); with this change it passes.

Fixes #510
