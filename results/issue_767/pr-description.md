## Summary
**User impact:** listing a bucket and then checking a single object (HEAD) gave two different
modification times for the same object, up to almost a second apart, for nearly every object.
Backup and sync tools that remember the time from a listing and later re-check the object saw
a "changed" object on every pass, so they could not tell a real change from a formatting
difference.

This PR makes listings report modification times in whole seconds, the same precision the
`Last-Modified` header already uses and the same precision Amazon S3's own listings use. The
stored time is unchanged.

## What to look at
- The timestamp formatter used by bucket listings now always writes `.000Z` instead of the
  millisecond part. That is the whole behaviour change; the rest is a test and docs.
- To try it: store an object, then compare `GET /<bucket>?list-type=2` with
  `HEAD /<bucket>/<key>`. Before this change the listing shows e.g.
  `2023-11-14T22:13:20.750Z` while HEAD shows `Tue, 14 Nov 2023 22:13:20 GMT`. After it, the
  listing shows `2023-11-14T22:13:20.000Z`, the same second.
- **Is whole seconds right?** Yes, checked against the live service on 2026-10-14. Anonymous
  listings of two public AWS Open Data buckets in two regions returned 3,000 of 3,000
  `<LastModified>` values ending `.000Z`, under both ListObjectsV2 and v1 ListObjects, and a
  HEAD of a listed object agreed to the second. If S3 kept milliseconds, about 999 in 1,000
  would end in something else. Anyone can re-run it:
  `curl -s "https://noaa-goes16.s3.amazonaws.com/?list-type=2&max-keys=1000&prefix=ABI-L1b-RadC/2024/"`.

## Root cause
The listing renders `<LastModified>` through `iso8601`, which kept `epoch_millis % 1_000` as a
`.mmm` fraction, while GET/HEAD render `Last-Modified` through `http_date`, which drops
milliseconds because IMF-fixdate has one-second resolution (RFC 7231 §7.1.1.1). Both read the
same stored `modified`, so they disagreed for every value not on a second boundary.

## Fix
- `iso8601` (`crates/gateway-s3/src/lib.rs:634-648`) drops the millisecond remainder and
  writes a literal `.000Z`. It keeps the same `/ 1_000` truncation `http_date` uses, so both
  formatters name the same second for every stored `u64`. The year-9999 bound is unchanged.
- The rule ("every S3 surface reports `modified` at whole seconds; storage stays in
  milliseconds per ADR-0047") is written in the doc comments of both `iso8601`
  (`:620-629`) and `http_date` (`:3193-3197`), and in one sentence of the object-metadata
  paragraph of `docs/design/architecture/05-building-block-view.md:194-196`. Future surfaces
  that report a part or object time (for example ListParts, #508) get the same rule by reusing
  `iso8601`.
- Not changed: the stored precision (ordering across overwrites, #768, relies on
  milliseconds) and the conditional-request comparison, which already compares whole seconds
  (`crates/gateway-s3/src/lib.rs:2786`, `:2801`).

Rounding to the nearest second was ruled out: it would disagree with the header and with the
conditional-request comparison, which both truncate.

## Verification
- **Claim:** for an object stored at `1_700_000_000_750` ms, v1 and v2 listings render
  `2023-11-14T22:13:20.000Z`, HEAD and GET send `Tue, 14 Nov 2023 22:13:20 GMT`, and both
  values parse to the same instant. An object stored exactly on a second renders as before.
- **Checked:** `crates/gateway-s3/src/lib.rs:634-648` — `iso8601` has one caller,
  `render_contents` (`:1034`), shared by v1 and v2 listings. `http_date` has two callers,
  `apply_object_headers` (`:2499`, shared by GET, ranged GET and HEAD) and the 304 response
  (`:2607`). So every surface that reports the time today goes through one of the two
  formatters, and both now truncate the same way.
- **Checked:** unit test `listing_and_header_formatters_both_truncate_to_whole_seconds`
  (`crates/gateway-s3/src/lib.rs:4622-4656`) — `.000`, `.750`, `.999` and the last renderable
  millisecond in year 9999 give matching listing and header strings.
- **Test:** `crates/server/tests/s3_last_modified_agreement.rs` (new) — runs the real gateway
  on a loopback socket over a real store, with objects seeded at `…000_750` and `…000_000`,
  and sends signed raw HTTP for ListObjectsV2, ListObjects v1, HEAD and GET
  (`surfaces`, `:720-763`). `assert_agreement` (`:765-793`) checks the exact wire strings, then
  parses both with the AWS SDK's own date parsers (`aws_smithy_types::DateTime`, `:483-492`)
  and requires the same instant. With only the gateway change reverted, the off-second test
  fails with `left: "2023-11-14T22:13:20.750Z" right: "2023-11-14T22:13:20.000Z"`; with the
  change, all 4 tests pass. A separate test (`:500-514`) shows the SDK parser keeps `.750`, so
  the instant check cannot pass on the old code by accident. The response reader rejects
  incomplete or malformed HTTP framing (`:247-387`, covered by `:516-702`), and every network
  and store call has a 30 s deadline.
- **Full gate:** `cargo xtask ci` passes every stage except
  `cargo deny --all-features --config deny-all-features.toml check advisories`, which fails on
  `main` too: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, pulled in only
  through `tikv-client 0.4.0` behind the off-by-default `tikv` feature. This PR touches no
  `Cargo.toml`, `Cargo.lock` or `deny*.toml`. With those two advisories ignored locally (not
  part of this PR), the full gate passes, including conformance, statics, deploy-guard and the
  DST suite. The advisory decision belongs in its own PR.

Out of scope, noted for follow-up: the draft proposal
`docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463` still says listings keep
milliseconds and can differ from HEAD by up to 999 ms. That is stale after this change and
should be amended separately.

Fixes #767
