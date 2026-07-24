# Result — issue 510 / range-conditional-get

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: GetObject honors `Range: bytes=a-b` (206 Partial Content, `Content-Range`,
- Success criterion: against the in-process loopback S3 gateway with a stored
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical change with two legs. (1) **Range**: parse single-range

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue 510: add coherent S3 byte-range GET/HEAD handling and conditional-read preconditions, including the iteration-3 date, atomicity, and ranged-HEAD corrections.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete for single ranges, malformed/multi-range fallback, conditional precedence, and S3 comparison semantics in `brief.md`; the affected wire entry points ground at `crates/gateway-s3/src/lib.rs:1621` and `crates/gateway-s3/src/lib.rs:1629`. |
| C2 Reproduction (red pre-fix) | PASS | On base `0fed4c8`, applying only the wire test compiled and produced the intended symptom: 11/12 assertions failed because requests remained 200 instead of 206/304/412/416, grounded by the oracle at `crates/server/tests/s3_range_conditional.rs:283`. |
| C3 Change | PASS | The change stays within the protocol seam, S3 dispatch, server read implementation, and one focused wire suite; the single-snapshot contract is grounded at `crates/gateway-core/src/lib.rs:110` and the production override at `crates/server/src/lib.rs:401`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the independently confirmed focused red→green despite the complete `cargo xtask ci` rerun ending at `cargo deny check`: the host could not acquire the read-only Cargo advisory DB lock, so dependency-policy verification remains provisional; the patched wire suite itself passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`. |
| C5 Causal adequacy | PASS | The fix removes the header-blind/full-read cause and binds precondition evaluation to the same resolved version as the body, rather than adding a capability probe or symptom guard; the causal boundary is grounded at `crates/gateway-s3/src/lib.rs:1672` and `crates/gateway-s3/src/lib.rs:1690`. |
| T1 Structure | PASS | Protocol-neutral range math and read outcomes remain in gateway-core while HTTP status/header mapping remains in gateway-s3, preserving the repository's narrow-seam boundary at `crates/gateway-core/src/lib.rs:84` and `crates/gateway-s3/src/lib.rs:1643`. |
| T2 Shape | PASS | The public seam expresses metadata plus satisfiable/unsatisfiable range outcome from one resolve, which is the minimum shape needed to prevent mixed-version framing at `crates/gateway-core/src/lib.rs:110` and `crates/gateway-core/src/lib.rs:341`. |
| T3 Runtime | PASS | The in-process loopback suite passed all 12 cases, including covering-chunk fetch count, obsolete dates, and ranged HEAD; the anti-discard runtime oracle is grounded at `crates/server/tests/s3_range_conditional.rs:711`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review history, so collision risk cannot be fully discharged. |
| T5 Judgment | PASS | The prior human direction to trade unused stream resolution for an intact conditional fence is implemented and regression-bound for stale-versus-served versions at `crates/gateway-s3/src/lib.rs:1672` and `crates/gateway-s3/src/lib.rs:4350`; no new ambiguous scope choice is introduced. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the loopback evidence is sufficient for real-client acceptance — run `aws s3api get-object --endpoint-url <gateway> --bucket <bucket> --key <key> --range bytes=8-15 <output>` and confirm 206-equivalent metadata plus byte-identical output, because the brief explicitly leaves AWS CLI round-trip off Check. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite the complete `cargo xtask ci` rerun ending at `cargo deny check`: the host could not acquire the read-only Cargo advisory DB lock, so dependency-policy verification remains provisional; the patched wire suite itself passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`.
- [ ] T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review history, so collision risk cannot be fully discharged.
- [ ] Validation — fitness-to-purpose — Decide whether the loopback evidence is sufficient for real-client acceptance — run `aws s3api get-object --endpoint-url <gateway> --bucket <bucket> --key <key> --range bytes=8-15 <output>` and confirm 206-equivalent metadata plus byte-identical output, because the brief explicitly leaves AWS CLI round-trip off Check.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite the complete `cargo xtask ci` rerun ending at `cargo deny check`: the host could not acquire the read-only Cargo advisory DB lock, so dependency-policy verification remains provisional; the patched wire suite itself passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`.; T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review history, so collision risk cannot be fully discharged.
- By / date: auto-iterate / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
