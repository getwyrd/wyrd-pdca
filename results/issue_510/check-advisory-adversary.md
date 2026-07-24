# Adversarial review — issue 510 / range-conditional-get (iteration 5)

Skeptic's pass. I re-ran the red→green proof myself in a scratch clone of `$PDCA_TARGET`
(sources reverted to HEAD for the red leg, test file kept), and attacked the fix's evidence,
edge cases, and seam contracts. Verdict: **could not refute the fix**; two annotations below.

## Refutation attempts that FAILED (the fix survived)

- **Re-ran the red→green proof independently.** Green: with the patch, `cargo test -p
  wyrd-server --test s3_range_conditional` passes 12/12. Red: with the three production files
  reverted to HEAD (`git checkout --`) and only the new test kept, **11/12 fail by assertion**
  (206→200, 412→200, 304→200, 416→200, counting-oracle 8 chunks vs 1) — not by compile error,
  exactly as the brief demands. The test drives the production path (real loopback listener →
  `S3Gateway::serve` → `dispatch` → `serve_get`/`serve_head`), not a parallel re-implementation.
  The `check-gates.json` C4-verify PASS claim is independently confirmed.
- **Tried to defeat the anti-wire-side-discard oracle.** The counting `ChunkStore` wraps the real
  `FsChunkStore` under `EcScheme::None`/`with_chunk_size(8)`; `crates/server/src/lib.rs:433-445`
  walks the chunk map with exact boundaries (`chunk_end <= offset` / `chunk_start >= end` — I
  checked the off-by-one candidates at chunk-aligned `bytes=8-15` and straddling `bytes=6-17`;
  both exact). A stream-then-discard implementation cannot pass `distinct_chunks() == 1`.
- **Tried the signed-Range attack.** The wire test sends `Range`/`if-*` unsigned, but a real
  SDK/CLI signs them — a verifier limited to a fixed header set would 403 real clients while the
  test passes. Refuted: `sigv4.rs:439-472` rebuilds the canonical request generically from the
  client's declared `SignedHeaders` list, so signed `range`/`if-none-match` verify correctly.
- **Tried to reopen the iteration-3 TOCTOU.** `serve_get` (gateway-s3 `lib.rs:1680-1739`)
  evaluates conditionals against the meta of the SAME resolve that yields the body on both the
  ranged (`get_object_range`) and unranged (`get_object_streaming`) paths; the
  `VersionSkewGateway` unit tests (gateway-s3 `lib.rs:4210-4515` region) drive the real router and
  would catch a regression to a separate `head_object` snapshot. The server override
  (`crates/server/src/lib.rs:401-482`) takes meta and chunk refs from one `committed_inode`
  snapshot; chunk refs are content-addressed, so a racing overwrite cannot substitute bytes.
- **Attacked the seam default and the range/date parsers on boundaries.** `resolve_byte_range`
  (gateway-core): `bytes=-0`→416, zero-byte object→416, clamp past end, `a==size-1` OK;
  `slice_object_stream` skip/take state machine correct incl. mid-stream error forward and
  source-chunk-boundary crossings; `parse_range`/`digits_or_empty` reject `+`-signed, interior
  whitespace, `b<a`, `bytes=-`, u64 overflow → all fall on the malformed→200 side per the decided
  behavior. Date parsers: `30 Feb` rejected (leap-year table), pre-epoch clamped to 0 (IUS fires
  412 — confirmed live), RFC-850 pivot at 70, asctime space-padded day, non-ASCII rejected before
  byte-indexing (no panic path found).
- **Checked the #608 finish-path interaction the brief flags.** `finish_response`
  (gateway-s3 `lib.rs:1289-1303`) classifies HEAD and 304 as body-less/complete (no bogus
  `aborted`/`truncated` rows for the new HEAD-206/304 shapes); a GET 206 declares the SPAN length
  so the declared==streamed truncation accounting holds; 412/416 count as 4xx RED errors exactly
  like the pre-existing 404. No metrics/access-log regression found.
- **Precedence/semantics probes.** If-Match > If-Unmodified-Since and If-None-Match >
  If-Modified-Since (RFC 9110 §13.2.2) hold at `evaluate_conditionals` (gateway-s3
  `lib.rs:2084-2113`); conditionals-before-range on GET and HEAD; If-Match absent-ETag fails
  closed; weak `W/` tag refused on If-Match only. No inverting input found.

## Findings

- NEEDS-HUMAN [impl] — **Stale doc-comment on `serve_get` describes the REJECTED design**:
  `crates/gateway-s3/src/lib.rs:1646-1649` says preconditions run "off a metadata-only
  [`ObjectGateway::head_object`] resolve … skipped entirely when the request carries none, so a
  plain or purely-ranged GET pays no extra metadata round-trip" — that is the iteration-3
  head-then-read shape the sign-off rejected (carry-forward item 2). The code (and the correct
  comment at `lib.rs:1672-1679`) does the opposite: one resolve, stream dropped on 304/412. A
  maintainer trusting the header doc could "optimize" the TOCTOU back in. Doc-only fix; behavior
  is correct.
- NEEDS-HUMAN — **The brief's off-Check acceptance claim is not verifiable in this tree**: the
  brief states `aws s3api get-object --range` manual acceptance "is registered as doctor row
  'aws cli (S3 gateway round-trip)'", but no such row exists anywhere in this checkout (only the
  FDB doctor, `xtask/src/fdb_doctor.rs`). If that registry lives in the PDCA engine (not visible
  here) this is moot; if it was owed by this bundle, nothing in the patch delivers it. Human
  scope call — the in-Check success criterion is unaffected either way.

## Notes (not findings)

- `invalid_conditional_date_is_ignored_not_misparsed` is the one test green on the base (it
  asserts the 200 the base also serves); it is a deliberate anti-misparse guard, and the file as
  a whole is a valid red-leg discriminator (11/12 red) — no evidence defect.
- Brief-sanctioned out-of-scope items were NOT re-filed per the iteration-3 sign-off's explicit
  dismissals: If-Range (a resuming client mixing versions gets a 206 where real S3 would answer
  200), multi-ETag If-None-Match lists (a list containing the real ETag serves 200 where S3
  304s), case-insensitive `BYTES=`, conditional PUT.
