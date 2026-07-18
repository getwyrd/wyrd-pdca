# Object metadata: ETag, Content-Type, and Last-Modified on the S3 surface

## Summary
**User impact:** Uploading an object through the S3 gateway returns no checksum, so
clients and SDKs that verify their uploads have nothing to check against — and every
download comes back as generic binary data with no modification date, no matter what
type the uploader declared. Files mis-render in browsers, sync tools cannot tell what
changed, and upload-integrity validation silently does nothing.

This PR makes the gateway store an object's checksum (ETag), its declared content
type, and its publication time alongside the data: the upload response now carries the
ETag, and a download returns the same ETag, the original content type, and a
`Last-Modified` date. The accompanying design record (ADR-0047) decides the metadata
model that HeadObject (#506), server-side copy (#504 step 2), and multipart will build
on. Decides and closes #503.

## What to look at
- The decision record first: `docs/design/adr/0047-object-metadata-model.md`. The one
  choice worth weighing is the ETag basis — the SHA-256 the write path already
  computes, treated as an opaque change token, rather than S3-classic MD5 (which would
  add a dependency and a second digest pass to serve only clients that violate ETag
  opacity).
- The metadata lives on the persisted object record and is committed in the same
  atomic step as the data, with one deliberate split: publishing content (create /
  overwrite) stamps fresh metadata; repairing or re-placing the *same* content
  preserves it, so maintenance never moves `Last-Modified`.
- Try it: `cargo test -p wyrd-server --test s3_object_metadata` runs a signed PUT/GET
  round trip over a real loopback listener; or bring up the gateway and do an
  `aws s3api put-object` / `get-object` with a `--content-type` and compare headers.

## Root cause
Nothing below the wire layer held object metadata, so the wire layer had nothing to
serve: on `main`, PutObject answers an empty `200` with no ETag
(`crates/gateway-s3/src/lib.rs:594-597`) and GetObject hardcodes
`content-type: application/octet-stream` (`crates/gateway-s3/src/lib.rs:603-611`).
The persisted record carries only `size`/`chunk_map`/`state`/`version`
(`crates/core/src/metadata.rs:235-244`), and the gateway seam returns only size plus
the body stream (`crates/gateway-core/src/lib.rs:45-50`).

## Fix
- `InodeRecord` gains three flat optional fields — `etag`, `content_type`, `modified`
  (epoch millis) — each `Option` + `#[serde(default)]`, so pre-existing records still
  decode and degrade to the old wire behaviour instead of erroring
  (`crates/core/src/metadata.rs`).
- Publication-vs-repair split: `commit_chunk_map_superseding{,_leased}` and
  `commit_create` stamp fresh metadata; the plain `commit_chunk_map` and the custodian
  backfill/rebalance/reconstruction commits preserve the stored trio via
  `..prior.clone()`.
- The seam stays transport-neutral: `ObjectGateway::put_object_streaming` takes the
  declared content type (`Option<String>`) and returns the committed ETag;
  `ObjectRead` carries `etag`/`content_type`/`modified` alongside `size`
  (`crates/gateway-core/src/lib.rs`). All paths remain streaming; the ETag reuses the
  SHA-256 already streamed through `HashingSource` (`crates/server/src/lib.rs`).
- Wire: the PUT arm passes the request's `Content-Type` down and answers with the
  S3-quoted `ETag`; the GET arm serves the stored content type (falling back to
  `application/octet-stream`), the `ETag`, and an RFC-7231 `Last-Modified` formatted
  by a small in-tree IMF-fixdate helper — no new dependency
  (`crates/gateway-s3/src/lib.rs`).
- Hardening: a malformed *stored* content type or etag (reachable via store corruption
  or out-of-band edits) degrades — default type / omitted header — instead of
  panicking the response builder and denying every read of that object.
- `docs/design/adr/0047-object-metadata-model.md` records the decisions; the ADR index
  (`docs/design/adr/README.md`) is updated.

## Verification
- **Claim:** a signed PutObject answers with the quoted lowercase-hex SHA-256 ETag,
  and a subsequent GetObject returns the *same* ETag, the PUT's declared
  Content-Type, and a valid RFC-7231 Last-Modified, round-tripped through a real
  metadata-store commit (redb + fs loopback stack).
  **Test:** `crates/server/tests/s3_object_metadata.rs` — the ETag oracle is an
  independent SHA-256 of the body computed in the test. Both tests fail on `main`
  (no ETag header on PUT; hardcoded `application/octet-stream` on GET —
  `crates/gateway-s3/src/lib.rs:594-611` on the target branch) and pass with this
  patch.
- **Claim:** an overwrite is a fresh publication — it stamps a new ETag/content
  type/Last-Modified, never the prior version's.
  **Test:** the second wire test (two PUTs of different content, independent digest
  oracle) plus unit tests in `crates/core/tests/mutation_regressions.rs` covering
  both superseding commits, including the leased path the wire PUT drives.
- **Claim:** repair paths preserve metadata — a placement-maintenance commit must not
  move Last-Modified or drop the ETag/content type.
  **Checked:** the preservation commits in `crates/core/src/metadata.rs` and
  `crates/custodian/src/{backfill,rebalance,reconstruction}.rs`.
  **Test:** seeded (non-vacuous) preservation tests in
  `crates/core/tests/mutation_regressions.rs`, `crates/custodian/tests/backfill.rs`,
  `crates/custodian/tests/rebalance.rs`, and
  `crates/custodian/tests/reconstruction.rs` — each seeds a record carrying the full
  trio before the maintenance commit fires.
- **Claim:** records written before this change still decode and serve.
  **Checked:** every new field is `Option` + `#[serde(default)]`; records are
  encoded once, centrally (`crates/core/src/metadata.rs:275-281` on the target
  branch), so the one compatibility rule covers all metadata backends.
- **Claim:** a malformed stored value degrades the GET, never panics it.
  **Test:** router-level tests in `crates/gateway-s3/src/lib.rs` drive a signed GET
  through the real router with a CR/LF-bearing stored content type / etag; each fails
  with the production `InvalidHeaderValue` panic when the guard is removed, and
  passes (200, full body, degraded header) with it.
- Whole gate: `cargo xtask ci` (fmt, clippy `-D warnings`, build, tests incl. DST,
  cargo-deny, conformance vectors) passes with the patch applied.

Fixes #503
