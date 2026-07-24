# Adversarial review — issue 510 / range-conditional-get

Verdict on the evidence: **could not refute the red→green proof.** I re-ran it
independently in a scratch clone of `$PDCA_TARGET` @ `0fed4c8`: with only the new test file
present, all 7 tests fail **by assertion** (200 where 206/304/412/416 is asserted — not a
compile error); with `patch.diff` applied, all 7 pass. The test drives the production wire
path (raw TCP + SigV4 against the real `Gateway`/`FsChunkStore` stack), not a parallel
re-implementation. I also attacked the anti-wire-side-discard oracle and found it sound:
`head_object` is metadata-only (`crates/server/src/lib.rs:467-477`, no fragment read), and a
stream-then-discard cheat must fetch chunk 0 to skip bytes 0–7, so the `distinct == 1`
assertion genuinely discriminates. Boundary attempts on `resolve_range` (`u64::MAX` end,
zero-byte object, clamped forms) did not break it, and the 206/304 access-log invariant
holds (`finish_response` classifies 304 body-less at `crates/gateway-s3/src/lib.rs:1288-1289`;
a 206 declares the span length). `check-gates.json`'s C4-verify PASS claim is warranted.

The findings below are conformance defects **verified by live probe** against the fixed
build, plus the cause of the one gating red.

- NEEDS-HUMAN [impl] — **The gating C4-ci red (typos, exit 2) is caused by this patch's own
  new comments**, not the environment: `unparseable` at
  `crates/gateway-s3/src/lib.rs:2006`, `:2056`, `:2112` and `mis-parsed` at `:2152`
  (reproduced with `typos-cli 1.48.0` in the target worktree). Reword to
  "unparsable"/"misparsed" (or record a deliberate `typos.toml` exception) and the gate
  goes green; nothing else in `xtask ci`'s red is attributable to this diff's prose.

- NEEDS-HUMAN [impl] — **An invalid HTTP-date is mis-parsed, not ignored — the code
  contradicts its own documented contract and RFC 9110 §13.1.4.**
  `days_from_civil` (`crates/gateway-s3/src/lib.rs:2153-2154`) accepts `day <= 31` for
  *every* month, so `If-Unmodified-Since: Mon, 30 Feb 2026 00:00:00 GMT` parses as
  ≈2026-03-02 instead of returning `None`. Verified live: that request answers **412**
  where the RFC mandates "ignore an invalid date" → 200 — and the doc at `lib.rs:2152`
  explicitly claims "a malformed date is ignored rather than mis-parsed". Concrete fix:
  validate day against the month's real length (leap-year Feb included) in
  `days_from_civil`.

- NEEDS-HUMAN [impl] — **`If-Match` accepts a weak entity-tag as a match.**
  `etag_matches` strips `W/` before comparing (`crates/gateway-s3/src/lib.rs:2104`), so
  `If-Match: W/"<stored-etag>"` answers **200** (verified live); RFC 9110 §13.1.1 mandates
  *strong* comparison for If-Match → 412. The brief scoped weak comparators out of
  *support*, but actively matching them on the one precondition where the RFC forbids it is
  different. Severity low (stock SDKs never send weak tags); fix is to refuse the `W/`
  prefix on the If-Match leg only.

- NEEDS-HUMAN [impl] — **`Range: bytes=-0` answers 200; real S3 answers 416
  `InvalidRange`.** `parse_range` maps suffix-length 0 to `None` → full 200
  (`crates/gateway-s3/src/lib.rs:1948`, verified live). `-0` is *grammatically valid* but
  unsatisfiable (RFC 9110 §14.1.1/§15.5.17), so it falls on the 416 side of the brief's
  "mirror real S3" line, not the malformed→200 side. One-line change in `parse_range` /
  `resolve_range` plus a test case.

- NEEDS-HUMAN [impl] — **Untested success-relevant behavior: the 304's cache validators.**
  The brief's Design requires "304 responses carry the ETag/Last-Modified headers"; the
  implementation does (`crates/gateway-s3/src/lib.rs:1860-1875`), but both 304 tests assert
  only status + empty body (`crates/server/tests/s3_range_conditional.rs:401-406`,
  `:447-452`). A regression dropping the validators — breaking every client's cache
  revalidation loop — stays green. Two `header_value(...)` assertions close the gap.

- Observation (brief-conformant, no action forced): HEAD ignores `Range`
  (`crates/gateway-s3/src/lib.rs:1730`) while real S3 HeadObject honours it (part-size
  probes via `aws s3api head-object --range` get 206 framing). The brief's goal scoped
  range to GetObject only, so this follows the brief; recorded so the divergence is a
  known, chosen one.

Attempted and could **not** refute: the red→green evidence, the production-path claim, the
anti-discard oracle, integer-boundary handling in `resolve_range`/`partial_content_response`,
the head_object→get_object_range race (degrades to a detectable truncation via the
defensive slice clamp, `crates/server/src/lib.rs:445-447`, never a panic), and the
declared==streamed access-log invariant for 206/304.
