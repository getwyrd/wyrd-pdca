# Build notes — issue 510 / range-conditional-get

Target branch: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE` off the folded base
`0fed4c8 pdca-integrate: issue_507` (507 already landed on the base, so the brief's cited
line numbers had shifted — the GET/HEAD arms are at `crates/gateway-s3/src/lib.rs:1615`/`:1674`
on this base, not `:909`/`:968`).

## What the fix does

Two ordered gates in the GET/HEAD arms — **conditionals first** (they can answer 304/412 with
no read), then **range** — plus `Accept-Ranges: bytes` on every object read.

### 1. The seam (protocol-neutral byte offsets, no HTTP vocabulary)

`crates/gateway-core/src/lib.rs:211` — a new `ObjectGateway::get_object_range(self: Arc<Self>,
key, offset, len) -> Result<Option<ObjectStream>>`. The wire layer resolves the HTTP `Range`
against the object size and hands the seam already-clamped byte offsets (`offset < size`,
`offset + len <= size`); the seam names no `Range`/`Content-Range`/suffix vocabulary.

**Open question resolved (seam shape).** The brief left "separate `get_object_range` method vs a
range parameter on `get_object_streaming`" to Do. I chose a **separate method with a
`Ok(None)` default**, mirroring two existing precedents on this same trait:
- the peer `head_object` chose a separate method (issue #506);
- `list_container` (`crates/gateway-core/src/lib.rs:227`) added a **default `Ok(None)`** so the
  crate's own wire-layer test doubles need no bespoke impl.
A range *parameter* on `get_object_streaming` would have forced every callsite of that method
**and** all 6 implementers (`server/src/lib.rs:266`; the 5 test doubles at
`gateway-s3/src/lib.rs:2298,2588,3139` and `server/tests/request_capacity_planes.rs:183,250`) to
change signature. The default-`None` separate method changes only the trait + the one real
`Gateway`; the 5 doubles compile untouched (and never receive a ranged request). The real
`Gateway` **overrides** it (`server/src/lib.rs:399`), so a shipped ranged GET never hits the
default — a double that inherited it would answer `NoSuchKey`, which is only ever reached by code
that never sends a Range.

### 2. Server implementation — only the covering chunks

`crates/server/src/lib.rs:399` — `get_object_range` resolves the committed inode (the same
`read::committed_inode` lookup `get_object_streaming` uses), walks the chunk map **once**
computing each chunk's byte span, collects only the chunks overlapping `[offset, offset+len)`
with an intra-chunk trim (`skip`/`take`), and streams them over the same bounded channel +
`in_current_span` reader task as `get_object_streaming`. A chunk entirely outside the span is
never fetched — that is what the anti-discard oracle proves. The trim clamps defensively
(`hi = (skip+take).min(bytes.len())`) so a short chunk read degrades to a short body (which the
wire's declared span length flags as a truncation) rather than panicking a slice.

### 3. Wire layer — 206 / 416 / 304 / 412 / Accept-Ranges

`crates/gateway-s3/src/lib.rs`:
- GET/HEAD arms are now thin calls to `serve_get` (`:1649`) / `serve_head` (`:1732`), keeping the
  dispatch a verb table (peer `list_objects`).
- `serve_get`: plain GET (no conditional, no honoured range) keeps the **single** streaming
  lookup, unchanged but for `Accept-Ranges`. A conditional-or-range request resolves metadata
  first via `head_object` (metadata only — **no chunk read, no reader task**, so a 304/412/416
  costs no data read and the anti-discard oracle sees only the covering-chunk fetches), runs the
  conditional gate, then either 416s, streams the covering span, or serves the full object.
- `partial_content_response` (`:1832`) declares `Content-Length = len` (the **span**, per the
  brief's note that a full-size length would log every ranged GET as truncated by the access
  wrapper) and `Content-Range: bytes {offset}-{last}/{size}`.
- `range_not_satisfiable` (`:1888`) answers 416 with `Content-Range: bytes */{size}`
  (S3 `InvalidRange`).
- Conditionals: `evaluate_conditionals` (`:2058`) applies RFC 9110 §13.2.2 **precedence**
  (If-Match > If-Unmodified-Since; If-None-Match > If-Modified-Since) with **S3 comparison
  semantics** — `etag_matches` (`:2099`) is exact opaque equality + `*`, weak/multi-list out of
  scope; `parse_http_date` (`:2114`) parses IMF-fixdate to seconds (the inverse of the existing
  `http_date` emitter, sharing a new `days_from_civil` at `:2153`) and dates compare at second
  resolution (`m / 1_000`). A record with no stored etag fails a specific If-Match; no stored
  `modified` ignores the date conditionals; an unparseable date is ignored — degrade safely.
- `parse_range` (`:1937`): only a single `bytes=` range. **Open question resolved (malformed):**
  multi-range, non-`bytes`, and syntactically malformed values all parse to `None` → the caller
  serves the full 200 (mirroring real S3), asserted in
  `out_of_scope_and_unranged_gets_answer_full_200_with_accept_ranges`.

## Alternative rejected — with its cost

**Wire-side slicing of the full stream** (read the whole object over `get_object_streaming`, then
discard out-of-range bytes in the wire layer). Rejected on two grounds:
1. It defeats range for large objects — a `bytes=0-0` GET of a 1 GiB object would read all 1 GiB.
2. It is **exactly the failure the brief's anti-discard oracle exists to catch.** Concretely, the
   `narrow_range_fetches_only_the_covering_chunks` test would then see `distinct_chunks() == 8`
   (the whole 8-chunk object fetched) instead of `1`. I verified this is not academic: on the
   **base** (Range ignored → full 200 → whole object streamed) that assertion reports `8`, which
   is the same count a wire-side-slice implementation would produce. The seam-carried range is the
   only design that makes the oracle green.

The extra cost of the chosen design is a **second metadata point-read** on the ranged/conditional
path (`head_object` then `get_object_range`/`get_object_streaming`) — a metadata lookup, **not** a
chunk read. The plain-GET hot path is untouched (one lookup). This is the minimum needed to run
the conditional/range gates before body work, which the design mandates.

## Test — `crates/server/tests/s3_range_conditional.rs`

Drives the **production** wire path over a real loopback S3 listener (the `s3_object_metadata.rs`
harness: signed requests, raw HTTP so exact status/headers/body are asserted). No new production
symbol is imported — the test uses only existing types (`Gateway`, `FsChunkStore`, `EcScheme`,
`ChunkStore`/`PlacementChunkStore`), so it compiles on the base and fails by **assertion**, not a
compile error (satisfies the C4-verify red leg). The counting `CountingChunkStore` wraps the real
`FsChunkStore` and records every `get_fragment` chunk id (the `request_capacity_planes.rs:665`
`GateStore` pattern); with `with_durability(EcScheme::None)` each chunk read is exactly one
counted fetch.

Harness choice: I used the **raw-wire** harness (`s3_object_metadata.rs`) rather than the
aws-sdk-s3 client the brief cited as an option (`s3_gateway_cluster.rs:98`). Both drive the same
production wire path; the raw wire gives precise control over the many header combinations (the
three `Range` forms, `If-*`, malformed/multi-range) and lets me read exact status codes/headers —
and it keeps the unit import-light (no SDK/display dependency), so a headless runner cannot crash
on load.

7 tests: ranged 206 (closed/spanning/open/suffix/clamped + body slice + Content-Range +
span Content-Length + Accept-Ranges), 416 + `bytes */size`, out-of-scope/malformed → 200,
If-None-Match → 304 on GET **and** HEAD, If-Match mismatch → 412, If-Modified-Since → 304 /
If-Unmodified-Since → 412, and the anti-discard oracle.

## Refuting my own test (forced, recorded)

- **(a) Genuine red?** YES. I reverted all three production files (`git stash push` of
  gateway-core/gateway-s3/server) and ran the test on the bare base: **7/7 failed by assertion**
  (206→got 200, 416→got 200, 304/412→got 200, Accept-Ranges absent, and the oracle's
  `distinct_chunks == 1`→got 8). Restored via `git stash pop`; green again. Every assertion binds
  a distinct behaviour of the fix.
- **(b) Production path?** YES. The test signs real requests to a real `S3Gateway::serve` loopback
  listener composed over the real `Gateway<RedbMetadataStore, _, MemCoordination>` — the same
  `serve_s3` composition the CLI runs. `get_object_range`/`evaluate_conditionals`/`parse_range`
  are the shipped production code, not a copy. The counting store *wraps* the real `FsChunkStore`
  (delegates every op) — it observes, it does not replace, the production read.
- **(c) Fixture includes the fault?** YES. The anti-discard oracle uses a genuine many-chunk
  object (64 bytes / 8 chunks via `with_chunk_size(8)` + `EcScheme::None`) and asserts the narrow
  `bytes=8-15` GET touches **exactly the one covering chunk** — measured by the fragments the
  production read path *actually fetched* through the wrapped real store, with a full-GET baseline
  in the same test proving all 8 are otherwise fetched. The "fault" a wire-side-slice
  implementation exhibits (fetching all 8) is exactly what the assertion is built to catch, and
  the base demonstrates that count.

## Gates run locally (worktree)

- `cargo fmt -- --check`: clean (formatter applied to touched files).
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings`: clean.
- New test red→green: proven both directions (above).
- No regressions: `wyrd-gateway-s3` (67 unit tests), `s3_http_wire` (19), `s3_object_metadata`
  (2), `request_capacity_planes` (6 — the real GET-path metrics/access-log test) all pass, so the
  GET/HEAD arm refactor did not change the access-log/metrics finish path.

## No NEEDS-HUMAN

The entire success criterion is exercised headlessly in-process (206/Content-Range/Content-Length/
body, suffix/open/clamped, 416, Accept-Ranges, 304 on GET+HEAD, 412, date 304/412, multi-range
200, anti-discard oracle). No irreducibly-GUI/IO behaviour, no external dependency beyond the base
toolchain. The brief's off-Check manual `aws s3api get-object --range` acceptance is already a
registered doctor row and is not required to prove the criterion; no new external dependency was
needed.
