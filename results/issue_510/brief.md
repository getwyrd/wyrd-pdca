# Design proposal — issue 510 / range-conditional-get

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.

- **Slug:** range-conditional-get
- **Kind:** enhancement (design proposal)
- **Goal:** GetObject honors `Range: bytes=a-b` (206 Partial Content, `Content-Range`,
  only the requested span streamed, `Accept-Ranges: bytes` advertised) and GET/HEAD honor
  the conditional headers `If-Match`/`If-None-Match` (vs the stored ETag) and
  `If-Modified-Since`/`If-Unmodified-Since` (vs Last-Modified) with correct 304/412
  answers. Today Range is ignored (always a full 200) and conditionals do nothing.
- **Success criterion:** against the in-process loopback S3 gateway with a stored
  multi-chunk object: a signed `GET` with `Range: bytes=a-b` returns 206 with
  `Content-Range: bytes a-b/<size>`, `Content-Length: b-a+1`, and a body byte-identical
  to that slice of the original (also suffix `bytes=-N` and open `bytes=a-` forms); an
  unsatisfiable range returns 416 with `Content-Range: bytes */<size>`; unranged GET now
  carries `Accept-Ranges: bytes`; `If-None-Match` with the object's ETag returns 304 on
  GET and HEAD; `If-Match` with a non-matching ETag returns 412;
  `If-Modified-Since`/`If-Unmodified-Since` answer 304/412 (S3 comparison semantics — see
  Scope); and — the anti-wire-side-discard oracle (adversarial finding: without it, an
  implementation that streams the WHOLE object and discards out-of-range bytes passes
  every other assertion byte-identically) — a narrow ranged GET of a many-chunk object
  (`with_chunk_size(8)`) fetches **only the covering chunks**, asserted via a counting
  `ChunkStore` wrapper the test defines (wrap `FsChunkStore`, count `get_fragment` calls;
  the in-test `ChunkStore` impl pattern is
  `crates/server/tests/request_capacity_planes.rs:665-672`, `GateStore`). Asserted by
  `crates/server/tests/s3_range_conditional.rs`, red on the wave base (full 200 for every
  ranged request; conditionals ignored). The test drives the wire only (no new production
  symbol imported), so the C4-verify red leg fails by assertion, not compile error.
- **Falsifiability:** RED is producible in-process: on the base the GET arm
  (`crates/gateway-s3/src/lib.rs:909-959`) reads no request header at all — it
  unconditionally streams the whole object with a 200 — and the HEAD arm (`lib.rs:968-1006`,
  on main since PR #607) evaluates no conditionals either, so the new test's
  206/304/412/416 assertions all fail for the RIGHT reason (headers ignored, not a 405).
  If waves place this bundle after 507/508/509, `run-verify.sh` honours the driver's
  `$PDCA_VERIFY_BASE` (`_resolve_base_ref`, engine/scripts/run-verify.sh:186-192), so the
  red/green legs run on the correct folded base.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 507, 509
- **Ordering note:** the former in-batch dependency on 506 is RESOLVED OUTSIDE the batch:
  HeadObject was re-landed on `main` as PR #607 (merged 2026-07-19; issue #506 closed), so
  the HEAD arm and the `head_object`/`ObjectMeta` seam this slice extends are on the
  brief's base already. The ETag prerequisite (#503) is likewise merged (commit 68403eb,
  ADR-0047). Conflict edges with 507/509 (same dispatch file); 508 orders itself after
  this bundle via its own `Depends on`, giving wave order [507] → [509] → [510] → [508].
- **Difficulty:** high
- **Scope:** one logical change with two legs. (1) **Range**: parse single-range
  `bytes=a-b` / `bytes=a-` / `bytes=-N`; extend the read seam so only the covering chunks
  are fetched and the span sliced out — a ranged read on `ObjectGateway`
  (`crates/gateway-core/src/lib.rs:162-165`) implemented in the server read path
  (`crates/server/src/lib.rs:340`), never by streaming the whole object and discarding
  bytes wire-side; 206/`Content-Range`/416 wiring; `Accept-Ranges: bytes` on GET/HEAD.
  (2) **Conditionals**: evaluate `If-Match`/`If-None-Match`/`If-Modified-Since`/
  `If-Unmodified-Since` against the stored ETag/modified metadata BEFORE any body work,
  on GET and HEAD, with RFC 9110 §13.2.2 PRECEDENCE (If-Match > If-Unmodified-Since;
  If-None-Match > If-Modified-Since) but **S3 comparison semantics, not full RFC**
  (codex finding — S3 itself deviates): exact opaque ETag equality plus `*` support;
  weak comparators and multi-ETag lists are out of scope (stock aws clients send a single
  value); date comparison truncates the stored epoch-millis to SECONDS (IMF-fixdate has
  second resolution). 304 responses carry the ETag/Last-Modified headers.
  / out of scope: multi-range requests (`bytes=a-b,c-d`) and syntactically malformed
  `Range` values — both answer the full 200 exactly as real S3 does (decided; assert the
  multi-range case in the test); `Range` combined with
  conditional-range `If-Range` (omit at Alpha); conditional PUT (`If-None-Match: *` write
  fencing — a write-path concern, not this read slice); CopyObject conditionals.
- **External dependencies:** none — base toolchain; in-process test on the loopback stack
  (aws-sdk-s3 dev-dependency already present; `aws s3api get-object --range` is off-Check
  manual acceptance via the AWS CLI, registered as doctor row
  "aws cli (S3 gateway round-trip)").
- **Test file:** crates/server/tests/s3_range_conditional.rs   (NEW `*/tests/*.rs` file —
  `run-verify.sh --classify` dry-run confirms an added `crates/server/tests/*.rs` is the
  red→green discriminator)
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites: the GET arm and its header construction
  `crates/gateway-s3/src/lib.rs:909-959` (note `lib.rs:931` sets `content-length` from
  the full size — a 206 must declare the SPAN length or every ranged GET is logged as
  truncated by the access wrapper, `lib.rs:475-489` region — note #608 also added method-keyed RED
  metrics emitted at response-finish time; a 206/304/412 flows through the same finish path); the multi-chunk read path to extend
  is `get_object_streaming` `crates/server/src/lib.rs:340` (small `with_chunk_size(8)`
  test objects span many chunks — the harness pattern at
  `crates/server/tests/s3_object_metadata.rs:43-52`); the metadata-only lookup for
  conditionals/HEAD is the `head_object`/`ObjectMeta` seam
  (`crates/gateway-core/src/lib.rs:67-77`, `:172`; impl `crates/server/src/lib.rs:394`;
  wire arm `crates/gateway-s3/src/lib.rs:968-1006` — on main since PR #607); the SDK client
  for ranged/conditional oracles is `crates/server/tests/s3_gateway_cluster.rs:98`.
- **Disposition hint:** new-feature

## Motivation

Range backs resumable downloads, media streaming, and range-based tools; conditionals back
HTTP caching and optimistic concurrency. Both are pure read-side features the metadata
model (#503, merged) already provides the data for. P1 of the 0.1-Alpha S3 epic.

## Design

Two ordered gates in the GET/HEAD arms: conditionals first (they can answer 304/412 with
no read at all, from the metadata the arm already resolves), then range resolution. The
ranged read crosses the seam as byte offsets (`offset`,`len` or an inclusive range) —
protocol-neutral, no HTTP vocabulary in gateway-core — and the server implementation maps
the span onto the chunk map: fetch only chunks overlapping `[a,b]`, trim the first/last.
The truncation-detection invariant (declared length == streamed length, issue #364
carry-forward) must hold for the SPAN: a 206 declares `b-a+1` and the access-log wrapper's
`declared` accounting then works unchanged. ETag comparison is by exact opaque value
(strong comparison; the stored ETag is the content digest, ADR-0047 — weak comparators
are out of scope). Date parsing: IMF-fixdate, mirroring `http_date` emission
(`crates/gateway-s3/src/lib.rs:939-947` region); an unparseable date header is ignored
(RFC 9110). A record with no stored ETag/modified (pre-ADR-0047) fails `If-Match`
(no current entity-tag) and ignores date conditionals — degrade safely, never panic.

## Alternatives considered

- **Wire-side slicing of the full stream** (read everything, discard out-of-range bytes):
  defeats the point of range for large objects and violates the stream-don't-buffer
  spirit; rejected — the seam carries the range.
- **Implementing `If-Range`/multi-range now**: S3 clients in the Alpha bar (aws-cli,
  boto3) issue single ranges; the added surface isn't paid for.

## Impact & compatibility

`Accept-Ranges: bytes` newly advertised; a previously-ignored `Range` header now changes
the response (that is the point — no current client can depend on the old behaviour
correctly). One new/extended trait method on `ObjectGateway` (in-tree implementers updated
in-change). No on-disk change, no new dependency.

## Open questions

- Seam shape: a separate `get_object_range` method vs an optional range parameter on
  `get_object_streaming` — Do decides (the peer `head_object` chose a separate method for
  cost-shape reasons; either is acceptable if all implementers stay honest).
- Exact S3 behaviour for a syntactically invalid `Range` header (ignore vs 416) — mirror
  real S3 (ignore malformed → 200) and assert it.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked by affected path, but this checkout contains no mechanically authoritative closed/rejected review history, so collision risk remains.; **The gating C4-ci red (typos, exit 2) is caused by this patch's own; **An invalid HTTP-date is mis-parsed, not ignored — the code; **`If-Match` accepts a weak entity-tag as a match.**; **`Range: bytes=-0` answers 200; real S3 answers 416; **Untested success-relevant behavior: the 304's cache validators.**; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Human sign-off (2026-07-20): the range/conditional feature works and red→green is independently confirmed, but four confirmed findings must be fixed before accept: 1. Pre-epoch `If-Unmodified-Since` must fire 412, not be silently ignored — clamp a pre-1970 IMF-fixdate to epoch 0 in `parse_http_date` instead of failing the parse (crates/gateway-s3/src/lib.rs:2133, tail :2160-2166); for IUS, "ignore" inverts the answer. 2. A `+`-signed range spec (`bytes=+8-+15`) must be treated as malformed → full 200 (the brief's decided behavior), not honoured as 206 — reject any non-ASCII-digit byte in the two range positions before parsing (crates/gateway-s3/src/lib.rs:1938-1959, drop the interior-whitespace `trim()` tolerance at :1949), and add this shape to the malformed-forms test set (crates/server/tests/s3_range_conditional.rs:373-379). 3. Close the TOCTOU window on ranged GET: headers come from one resolve (`head_object`) and body bytes from a second (`get_object_range`), so a racing PUT can emit a version-mixed 206 that poisons ETag-keyed caches — reshape the seam so meta+stream come from ONE inode resolve (a bare `ObjectStream` return cannot express an atomic conditional+ranged read). 4. Remove the `get_object_range` trait-default `Ok(None)` landmine — a non-overriding gateway answers a ranged GET of an EXISTING object with 404 after advertising `Accept-Ranges: bytes` — use a correctness-preserving default (seam-side full-read + slice) or no default at all (crates/gateway-core/src/lib.rs:211-224; wire mapping crates/gateway-s3/src/lib.rs:1717-1719). Explicitly dismissed at sign-off — do NOT re-open: wildcard `If-Match`/`If-None-Match` behavior on pre-ADR-0047 records without stored ETags (all current objects carry SHA-256 ETags; legacy-only concern, recorded as a §10 Act candidate).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Human sign-off (2026-07-20): the four iteration-2 carry-forward items are confirmed fixed and red→green holds; rejected to fix the three remaining §6 findings (adversary items 4–6): 1. `If-Unmodified-Since` fails OPEN for obsolete-but-valid HTTP-dates: `parse_http_date` (crates/gateway-s3/src/lib.rs:2084) accepts only the 29-char IMF-fixdate, and the ignore-on-unparse path (:2024-2030) then serves 200 where 412 is conformant (confirmed live with an RFC-850 date). RFC 9110 §5.6.7 requires recipients to accept all three HTTP-date formats — extend the parser to RFC-850 and asctime; keep ignore-on-unparse only for genuinely malformed dates. Add wire tests for both obsolete formats on IUS (412) and IMS. 2. Close the residual check-then-act window on conditionals: preconditions are evaluated against a `head_object` snapshot (:1676) while the body comes from a second resolve (:1694-1696 / :1707-1709), so `If-Match` can pass against v1 and a self-coherent v2 206 be served without a 412. Bind conditional evaluation and body selection to ONE resolve — the new `RangeRead` seam can express this: evaluate conditionals against `RangeRead.meta` from the single resolve and drop the stream on 304/412. The "a 304/412 costs no body work" trade is not worth piercing the If-Match fence. Add a deterministic version-skew test (the item-3 double pattern at gateway-s3 lib.rs:4136 already models a racing overwrite) asserting 412 when the precondition matched only the stale snapshot — this also discharges the Validation §6 item (no overwrite-between-eval-and-resolve test). 3. HEAD must honor `Range` now that it advertises `Accept-Ranges: bytes` (:1726-1728): mirror real S3 — a satisfiable range is reflected in `Content-Length` (with `Content-Range`), an unsatisfiable one answers 416. Confirmed live deviation: HEAD `bytes=8-15` → 200 CL=64; HEAD `bytes=999-` → 200 where real S3 answers 416. Add both HEAD cases to the wire test. Do NOT re-open: the four iteration-2 items (confirmed fixed, keep their tests), the iteration-2 dismissals (wildcard conditionals on pre-ADR-0047 records), and the brief-sanctioned out-of-scope set (multi-range, If-Range, multi-ETag lists, conditional PUT). The minor advisory observations (case-insensitive `BYTES=`, multi-tag If-Match list, pre-epoch clamp blind spot) remain advisory — do not chase them.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite the complete `cargo xtask ci` rerun ending at `cargo deny check`: the host could not acquire the read-only Cargo advisory DB lock, so dependency-policy verification remains provisional; the patched wire suite itself passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`.; T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review history, so collision risk cannot be fully discharged.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
