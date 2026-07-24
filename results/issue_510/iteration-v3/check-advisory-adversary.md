# check-advisory-adversary.md — issue 510 / range-conditional-get (iteration 3)

Skeptic's pass. I re-ran the red→green proof myself and probed the fix with boundary
inputs against a live loopback build (scratch clone, since removed). Verdict summary:
the evidence holds; three scope-level deviations need human adjudication; no
implementation defect found that a rebuild should chase.

## The evidence — attempted to refute; could not

- Re-ran the asserted red→green independently (fresh scratch build, not `run-verify.sh`):
  **green** = patched sources, `cargo test -p wyrd-server --test s3_range_conditional`
  → 10/10 pass; **red** = reverse-applied patch + the test file → the file **compiles on
  the base** and **9/10 fail by assertion** (200 where 206/304/412/416 was demanded) —
  exactly the fail-by-assertion red the brief required. The test drives the production
  path (real TCP listener → SigV4 → `dispatch` → `serve_get`/`serve_head` → real
  `Gateway` → real `FsChunkStore`), not a parallel re-implementation; the
  anti-wire-side-discard oracle counts real fragment fetches and flips 8→1 chunks for
  `bytes=8-15` (crates/server/tests/s3_range_conditional.rs:611-641).
- One test is green-on-base by design: `invalid_conditional_date_is_ignored_not_misparsed`
  (crates/server/tests/s3_range_conditional.rs:489) asserts the ignore-side (200), which
  the header-blind base trivially satisfies. It is a legitimate negative control guarding
  `days_from_civil`'s calendar validation, but it contributes nothing to the red
  discriminator — noted so nobody counts it as red evidence.
- The two seam unit tests for sign-off items 3 & 4 (`ranged_206_is_framed_from_the_range_seam…`,
  `a_non_overriding_gateway_serves_ranges…`, crates/gateway-s3/src/lib.rs:4136, :3991) pass
  and drive the real router; verified the item-4 default double genuinely omits
  `get_object_range` (crates/gateway-s3/src/lib.rs:3984).
- Boundary probes against the patched server all behaved: `bytes=0-0` / `bytes=63-63` →
  correct single-byte 206s; `If-None-Match: *` → 304; Range + matching `If-None-Match` →
  304 wins over 206; empty object `bytes=0-` → 416 `bytes */0`; `bytes=+8-+15`, tab-infixed
  and signed-suffix forms → 200 (sign-off item 2 holds); pre-epoch IUS → 412 (item 1 holds).

## Findings for human adjudication

- NEEDS-HUMAN — **Obsolete-but-valid HTTP-dates fail OPEN on `If-Unmodified-Since`** —
  confirmed live: `If-Unmodified-Since: Sunday, 06-Nov-94 08:49:37 GMT` (RFC-850 form)
  against an object modified in 2026 answers **200**, where the conformant answer is 412.
  `parse_http_date` (crates/gateway-s3/src/lib.rs:2084, doc :2080-2083) parses only the
  29-char IMF-fixdate; RFC 9110 §5.6.7 says a recipient **MUST accept all three**
  HTTP-date formats, and the ignore-on-unparse at :2024-2030 then serves the object. This
  is the *same fail-open inversion class* the iteration-2 sign-off ordered fixed for
  pre-epoch dates (item 1: "for IUS, 'ignore' inverts the answer") — but the brief's
  Design section itself sanctioned IMF-fixdate-only (brief.md:107-109), so the quarrel is
  with the brief's scope, not the builder's execution. Human call: accept the scope cut
  (stock SDKs send IMF-fixdate only) and record it, or extend the parser.
- NEEDS-HUMAN — **Sign-off item 3 is closed for the range leg only; the conditional gate
  still evaluates on a separate resolve.** `serve_get` evaluates preconditions off
  `head_object` (crates/gateway-s3/src/lib.rs:1676) and then fetches the body from a
  second resolve (:1694-1696 unranged, :1707-1709 ranged). Concrete case: client sends
  `If-Match: "v1-etag"` + `Range: bytes=0-99`; a PUT lands between :1676 and :1708; the
  precondition passed against v1 but a 206 of **v2** is served — self-coherently framed
  (v2 ETag/size, so the cache-poisoning vector the sign-off named IS closed, and the item-3
  unit test at :4181 proves it), yet the If-Match fence is pierced without a 412. The new
  `RangeRead` seam could express the fully atomic form (evaluate conditionals against
  `RangeRead.meta` from the single resolve, drop the stream on 304/412); the patch traded
  that for "a 304/412 costs no body work" (doc :1646-1649). Whether the residual
  check-then-act window satisfies the sign-off's "atomic conditional+ranged read"
  parenthetical is a design judgment, not a mechanical fix — hence no [impl].
- NEEDS-HUMAN — **HEAD advertises `Accept-Ranges: bytes` but ignores `Range`**
  (crates/gateway-s3/src/lib.rs:1726-1728, decided in-code as out of scope). Real S3
  HeadObject honours Range: a satisfiable range is reflected in `Content-Length`, an
  unsatisfiable one answers **416**. Confirmed live: HEAD `bytes=8-15` → 200 CL=64;
  HEAD `bytes=999-` → 200 (real S3: 416). The brief's success criterion never asked for
  ranged HEAD, so this is a scope/fitness decision (Alpha clients may not care), but the
  newly-advertised `Accept-Ranges` on HEAD makes the deviation observable.

## Minor observations (advisory only, no adjudication needed)

- `Range: BYTES=8-15` → full 200 (crates/gateway-s3/src/lib.rs:1914 matches `bytes=`
  case-sensitively); RFC 9110 range-unit names are case-insensitive. Safe degrade
  (200, never a wrong 206); S3 parity unverified here.
- `If-Match: "deadbeef", "<true-etag>"` (multi-tag list) → 412 despite containing the
  current tag (crates/gateway-s3/src/lib.rs:2063-2077). Brief-sanctioned out of scope,
  and it fails **closed** for If-Match / falls back to a full 200 for If-None-Match —
  both safe directions.
- `evaluate_conditionals`' pre-epoch clamp has an unreachable blind spot: an object with
  stored `modified` in [0,1000) ms would compare equal to a clamped pre-epoch IUS and
  serve 200 (crates/gateway-s3/src/lib.rs:2027); no real write path produces such a
  timestamp.

## Verdict

Attempted to refute the red→green evidence (re-ran both legs from scratch), the
anti-discard oracle (counted real fragment fetches), the sign-off carry-forward items
1–4 (all four confirmed fixed, items 3/4 additionally via their unit tests), and the
range/conditional boundary behavior (11 live probes): **could not refute the fix within
the brief's stated scope**. The three NEEDS-HUMAN items above are scope-boundary
deviations the brief either sanctioned or never addressed — none is a defect a rebuild
should chase without a human decision first.
