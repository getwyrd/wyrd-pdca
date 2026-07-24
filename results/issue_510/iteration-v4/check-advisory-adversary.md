# Adversarial review — issue 510 / range-conditional-get (iteration 4)

Skeptic's pass. I attempted to refute the red→green evidence, the seam atomicity, the
range/date parsers, and the finish-path interactions. The fix survived every substantive
attack; what remains are two conformance nits below, neither in the class the iteration-3
sign-off told the builder to chase.

## Evidence re-run (independent, not the gate's word)

- **Green leg re-run and held**: copied the target source to scratch and ran
  `cargo test -p wyrd-server --test s3_range_conditional -p wyrd-gateway-s3 --lib` — all 12
  integration tests pass (plus the three new in-crate seam tests compile and pass with the
  fix). **Red leg re-run and held**: reverted only the three production files to `HEAD`
  (base `0fed4c8`, keeping the test file) — **11 of 12 tests fail by assertion on the
  production wire path** (`left: 200, right: 206` at
  `crates/server/tests/s3_range_conditional.rs:738`, etc.), not by compile error and not via
  a parallel re-implementation: the test drives a real loopback listener through
  `S3Gateway::serve`. The `C4-verify: PASS` claim in `check-gates.json` is warranted.
- The one test green-on-base (`invalid_conditional_date_is_ignored_not_misparsed`,
  crates/server/tests/s3_range_conditional.rs:492) asserts a
  no-behavior-change guard (200 before and after) — acceptable inside an otherwise-red
  file, noted for evidence hygiene only.
- **Anti-wire-side-discard oracle is real**: the counting store wraps the real
  `FsChunkStore` and the server override walks only covering chunks
  (crates/server/src/lib.rs:401 region, `covering` selection with first/last trim);
  confirmed live — `bytes=8-15` of the 8-chunk object touches exactly 1 chunk, the full
  GET touches 8. Not a tautology: on the base the same oracle sees the full 200.

## Refutation attempts that did NOT land

- **TOCTOU / version-skew (carry-forward items 2–3)**: `VersionSkewGateway` unit tests
  (crates/gateway-s3/src/lib.rs:4243 region) drive the REAL router and assert the 206's
  `Content-Range`/`ETag`/body all come from the `get_object_range` resolve, and a
  stale-head `If-Match` 412s on both ranged and unranged paths — with a positive control.
  `serve_get` (crates/gateway-s3/src/lib.rs:1654) never consults `head_object` on GET;
  I found no remaining two-resolve path on a body-carrying response.
- **Range math boundaries**: probed `bytes=-0` (416, resolve_byte_range
  crates/gateway-core/src/lib.rs:144 treats `n==0`/`size==0` as unsatisfiable),
  `bytes=60-999` clamp, `bytes=0-` (206 whole object, matches real S3), suffix > size,
  `u64`-overflow positions (malformed → 200), `+`-signed and interior-space specs
  (digits_or_empty, crates/gateway-s3/src/lib.rs:2021, rejects any non-digit byte).
  Could not construct a range that is honoured when it must be ignored, or vice versa.
- **Access-log/RED-metrics finish path**: `finish_response`
  (crates/gateway-s3/src/lib.rs:1289-1290) classifies 304 and every HEAD as body-less
  (recorded complete), and a GET 206 declares the SPAN length so declared==streamed holds —
  the patch's claims about #364/#608 interactions check out against the target source.
- **SigV4 with a signed `Range` header** (what aws-cli actually sends, unlike the test's
  unsigned headers): verification reconstructs the canonical request from the client's own
  `SignedHeaders` list with actual header values (crates/gateway-s3/src/sigv4.rs:439-472),
  so a signed `range`/`if-*` header verifies; the test's unsigned-header shortcut does not
  hide a 403.
- **Date semantics**: pre-epoch clamp (ymd_hms_to_epoch, crates/gateway-s3/src/lib.rs:2260,
  `total.max(0)`), impossible-calendar-date rejection (`30 Feb` → ignored), second-truncation
  compare, RFC-850 year pivot, If-Match strong / If-None-Match weak comparison split
  (crates/gateway-s3/src/lib.rs:2129) — all behave as documented; each has a wire test.

## Findings (advisory nits — none block; none in the sign-off's do-not-chase list verbatim, but same class)

- `parse_rfc850_date` (crates/gateway-s3/src/lib.rs:2197-2199) discards the weekday token
  unvalidated: `If-Unmodified-Since: Blursday, 01-Jan-90 00:00:00 GMT` (or an empty
  weekday, `", 01-Jan-90 …"`) parses as valid and fires 412, where RFC 9110 §13.1.4 would
  ignore the malformed date (→ 200). Failure direction matches the parser's deliberate
  liberality elsewhere; vanishingly rare input; advisory only.
- `Conditionals::from_headers` / the range read (crates/gateway-s3/src/lib.rs:1668, :1762,
  :2045) use `headers.get(..)` — the FIRST of repeated header lines. Two
  `Range:` lines (semantically a multi-range set, which the brief decides must answer 200)
  instead honour the first as a 206. Marginal: no stock S3 client sends repeated `Range`
  lines; advisory only.

## Verdict

Attempted to refute the red→green proof, the single-resolve atomicity, the anti-discard
oracle, the range grammar/boundary math, the HTTP-date parsers, and the finish-path
accounting; **could not**. The reviewer's pass verdict and both C4 rows in
`check-gates.json` are consistent with what I reproduced independently. No NEEDS-HUMAN
items from this pass.
