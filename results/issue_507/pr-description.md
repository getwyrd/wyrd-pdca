# PR description

## Summary
**User impact:** Browsing a bucket with any standard S3 client fails outright.
`aws s3 ls`, `aws s3 sync`, boto3, rclone, restic and s3fs all begin by asking
the server to list a bucket's contents, and the gateway refuses every such
request with a generic error — so while single objects can be read and written,
nothing can enumerate what a bucket holds.

This PR implements bucket listing: ListObjectsV2 (with prefix, delimiter,
pagination, start-after and URL encoding) plus a thin v1 ListObjects
compatibility shim, so the stock clients above can browse.

Reported in #507.

## What to look at
The change has three parts: a routing split in the S3 wire crate so a
bucket-only `GET` reaches a listing handler instead of the object-path guard
(bucket subresource requests like `?acl` still answer 501, and paths with an
empty bucket segment like `//bucket` are rejected); one pure function
(`compute_page`) that computes each page — prefix filter, delimiter rollups,
the combined max-keys slice, and the resume point — over a single sorted view;
and a protocol-neutral `list_container` seam so the core gateway trait stays
free of S3 vocabulary while `wyrd-server` supplies the sorted per-bucket scan.

To exercise it: `cargo test -p wyrd-server --test s3_list_objects` runs 24
wire-level tests over a real loopback listener, driven by the stock `aws-sdk-s3`
client (its paginator is the token-chaining oracle) and raw SigV4-signed HTTP
for the surfaces the SDK does not emit by default (`encoding-type`, the v1
shim, malformed parameters). Note: until CreateBucket (#511) writes
`bucket:{name}` markers, a live stack answers `NoSuchBucket` for every listing
— the tests seed markers directly, per the documented Alpha stance.

## Root cause
Request dispatch recognised only object paths `/{bucket}/{key}`:
`split_bucket_key` (`crates/gateway-s3/src/lib.rs:371-377` on `main`) returns
`None` for a bucket-only path, and the caller answers `400 InvalidRequest`
(`crates/gateway-s3/src/lib.rs:788-795`). Every bucket-scoped GET was therefore
rejected before any listing logic could exist.

## Fix
- **Routing** (`crates/gateway-s3/src/lib.rs:1463-1489`): partition paths into
  bucket-scoped (`/{bucket}`, `/{bucket}/`) vs object-scoped before the
  object-path guard; `bucket_scoped_path` (`lib.rs:426`) strips exactly one
  leading `/`, so an empty bucket segment (`//bucket`) is refused, and the
  bucket route consults the subresource denylist on percent-decoded query keys
  (`lib.rs:400`) so `GET /bucket?acl` — even percent-encoded — stays 501
  rather than being answered with a listing document.
- **Listing** (`lib.rs:653`): parses the v2/v1 query vocabulary; `compute_page`
  (`lib.rs:519`) applies the prefix filter, delimiter common-prefix rollups,
  the combined max-keys budget, and the resume point over the sorted key set.
  Two resume mechanisms stay distinct (`lib.rs:505`): the v2 token encodes the
  last raw consumed key, while v1's `NextMarker` names the last returned item
  (the common prefix for a rollup), and the whole-group collapse on
  resume==prefix applies only to the v1 marker path — a client-chosen v2
  `start-after` equal to a common prefix keeps the rollup, matching AWS's
  raw-keyspace semantics. A co-sent `continuation-token` overrides
  `start-after` (AWS precedence; the stock paginator resends both).
- **`encoding-type=url`** (`lib.rs:835`): render-time percent-encoding of
  `Key`/`Prefix`/`Delimiter`/`CommonPrefixes` and the `StartAfter`/`Marker`/
  `NextMarker` echoes (`a&b/c d` → `a%26b/c%20d`), with
  `<EncodingType>url</EncodingType>` emitted and the opaque continuation
  tokens left untouched; any other value answers `400 InvalidArgument`.
  botocore injects this parameter into every ListObjects request, so it is
  required by the exact clients this feature targets.
- **Seam** (`crates/gateway-core/src/lib.rs:227`,
  `crates/server/src/lib.rs:462`): `list_container` returns the container's
  complete, lexicographically sorted key set (`None` = no bucket record →
  `NoSuchBucket`; `Some(vec![])` = empty bucket → empty 200). The server
  implementation reads the `bucket:{name}` record first (ADR-0046), scans the
  per-bucket dirent prefix of the flat `{bucket}/{key}` encoding, and skips
  uncommitted inodes. XML is string-built — no new dependency.

## Verification
- **Claim:** the feature is real end-to-end, not test-shaped. **Checked:** on
  the merge base (`07d024472f3c30c77802cf7d813acf656ebbe7a0`) with only the new
  test file added, 23/24 tests fail by assertion (every bucket GET answers 400
  `InvalidRequest` — the pre-fix behaviour at
  `crates/gateway-s3/src/lib.rs:788-795` on `main`); with the change applied,
  24/24 pass. Independently re-reproduced in a scratch clone during review.
- **Claim:** correct listings — sorted `Contents` with real `Size`/`ETag`
  (ETag checked against an independently computed digest), prefix filtering,
  delimiter rollups. **Test:** `crates/server/tests/s3_list_objects.rs:266`,
  `:309`, `:328`.
- **Claim:** pagination returns every key exactly once, including under a
  delimiter with no double-emitted prefix, driven by the stock SDK paginator.
  **Test:** `s3_list_objects.rs:380`, `:631`; `max-keys=0` untruncated at
  `:601`; invalid token → 400 with the `<Code>InvalidArgument</Code>` body at
  `:455`.
- **Claim:** absent bucket ≠ empty bucket. **Test:** `s3_list_objects.rs:419`
  (404 `NoSuchBucket`), `:439` (empty 200 with a marker).
- **Claim:** `encoding-type=url` matches the botocore oracle on every encoded
  element (v2 and v1), tokens resume verbatim, and a non-`url` value is a 400
  asserted on the body code. **Test:** `s3_list_objects.rs:819`, `:840`,
  `:879`, `:935`, `:953` — raw signed HTTP, exact wire bytes.
- **Claim:** AWS resume semantics — v2 `start-after` equal to a common prefix
  still returns the rollup; a token overrides a co-sent `start-after`; v1
  `NextMarker` resumes without duplicates. **Test:** `s3_list_objects.rs:1002`,
  `:1042`, `:541`.
- **Claim:** routing stays fail-closed — bucket subresource GETs answer 501
  (even percent-encoded), and `//bucket` is rejected, not listed. **Test:**
  `s3_list_objects.rs:712`, `:1091`, and the unit test at
  `crates/gateway-s3/src/lib.rs:3015`.
- **Gate:** the full `cargo xtask ci` (fmt, clippy `-D warnings`, build, test,
  deny, conformance) passes on the patched tree.

Deferred, tracked for #511 (flagged during review, not reachable in this
change): CreateBucket must reject bucket names containing `/` before markers
become writable, and the end-to-end `aws s3 ls` / `aws s3 sync` round-trip
acceptance runs once #511 lands marker writes.

Fixes #507
