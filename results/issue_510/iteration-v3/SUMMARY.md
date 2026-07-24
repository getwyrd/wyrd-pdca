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

Review of issue 510: add efficient single-range S3 GETs and GET/HEAD conditional request handling with correct HTTP status and validator semantics.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the wire and includes the anti-discard oracle; the affected behaviors are exercised at `crates/server/tests/s3_range_conditional.rs:280`. |
| C2 Reproduction (red pre-fix) | PASS | The retained integration test compiled on base and failed behaviorally in 9/10 cases (200 instead of 206/304/412/416), including the first range assertion at `crates/server/tests/s3_range_conditional.rs:298`. |
| C3 Change | FAIL | Conditional GET authorization must apply to the representation actually served — `head_object` is checked first, then a separate full/ranged resolve can serve an overwritten version whose ETag/date did not satisfy the request (`crates/gateway-s3/src/lib.rs:1675`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept incomplete gate reproduction — targeted red→green (9 behavioral failures→10 passes), fmt, affected-crate clippy, and 69 gateway tests were reproduced, but the asserted `./engine/xtask.sh ci` runner is absent from the supplied target, so its full deny/conformance gate could not be rerun (`crates/server/tests/s3_range_conditional.rs:280`). |
| C5 Causal adequacy | FAIL | The range seam makes headers and bytes one snapshot, but the preceding conditional decision remains a separate snapshot, so racing PUTs can bypass `If-Match`/date intent before `get_object_range` runs (`crates/gateway-s3/src/lib.rs:1707`). |
| T1 Structure | PASS | Protocol parsing stays in the S3 layer while byte-range resolution and chunk selection remain protocol-neutral at `crates/gateway-core/src/lib.rs:317` and `crates/server/src/lib.rs:401`. |
| T2 Shape | FAIL | The read seam must atomically bind preconditions to the selected representation — returning atomic range metadata/body alone cannot make the earlier `head_object` decision authoritative (`crates/gateway-s3/src/lib.rs:1681`). |
| T3 Runtime | FAIL | Under a concurrent overwrite, a matching stale `If-Match` can return 200/206 with fresh bytes and a different ETag, violating optimistic-concurrency/cache semantics despite ordinary loopback tests passing (`crates/gateway-s3/src/lib.rs:1694`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path collision risk is acceptable — merged/all-ref history was checked by the four affected paths, but this artifact-only checkout has no mechanically authoritative closed/rejected review history. |
| T5 Judgment | FAIL | The human must require conditional evaluation and body selection to share a version before accepting, because the current two-resolve path can authorize the wrong representation (`crates/gateway-s3/src/lib.rs:1676`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the feature is fit for real concurrent S3 traffic — loopback range/conditional behavior and covering-chunk reads pass at `crates/server/tests/s3_range_conditional.rs:597`, but no test exercises an overwrite between conditional evaluation and body resolve. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept incomplete gate reproduction — targeted red→green (9 behavioral failures→10 passes), fmt, affected-crate clippy, and 69 gateway tests were reproduced, but the asserted `./engine/xtask.sh ci` runner is absent from the supplied target, so its full deny/conformance gate could not be rerun (`crates/server/tests/s3_range_conditional.rs:280`).
- [ ] T4 Contribution — Decide whether affected-path collision risk is acceptable — merged/all-ref history was checked by the four affected paths, but this artifact-only checkout has no mechanically authoritative closed/rejected review history.
- [ ] Validation — fitness-to-purpose — Decide whether the feature is fit for real concurrent S3 traffic — loopback range/conditional behavior and covering-chunk reads pass at `crates/server/tests/s3_range_conditional.rs:597`, but no test exercises an overwrite between conditional evaluation and body resolve.
- [ ] **Obsolete-but-valid HTTP-dates fail OPEN on `If-Unmodified-Since`** —
- [ ] **Sign-off item 3 is closed for the range leg only; the conditional gate
- [ ] **HEAD advertises `Accept-Ranges: bytes` but ignores `Range`**

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
- Iteration delta (if iterating): Human sign-off (2026-07-20): the four iteration-2 carry-forward items are confirmed fixed and red→green holds; rejected to fix the three remaining §6 findings (adversary items 4–6): 1. `If-Unmodified-Since` fails OPEN for obsolete-but-valid HTTP-dates: `parse_http_date` (crates/gateway-s3/src/lib.rs:2084) accepts only the 29-char IMF-fixdate, and the ignore-on-unparse path (:2024-2030) then serves 200 where 412 is conformant (confirmed live with an RFC-850 date). RFC 9110 §5.6.7 requires recipients to accept all three HTTP-date formats — extend the parser to RFC-850 and asctime; keep ignore-on-unparse only for genuinely malformed dates. Add wire tests for both obsolete formats on IUS (412) and IMS. 2. Close the residual check-then-act window on conditionals: preconditions are evaluated against a `head_object` snapshot (:1676) while the body comes from a second resolve (:1694-1696 / :1707-1709), so `If-Match` can pass against v1 and a self-coherent v2 206 be served without a 412. Bind conditional evaluation and body selection to ONE resolve — the new `RangeRead` seam can express this: evaluate conditionals against `RangeRead.meta` from the single resolve and drop the stream on 304/412. The "a 304/412 costs no body work" trade is not worth piercing the If-Match fence. Add a deterministic version-skew test (the item-3 double pattern at gateway-s3 lib.rs:4136 already models a racing overwrite) asserting 412 when the precondition matched only the stale snapshot — this also discharges the Validation §6 item (no overwrite-between-eval-and-resolve test). 3. HEAD must honor `Range` now that it advertises `Accept-Ranges: bytes` (:1726-1728): mirror real S3 — a satisfiable range is reflected in `Content-Length` (with `Content-Range`), an unsatisfiable one answers 416. Confirmed live deviation: HEAD `bytes=8-15` → 200 CL=64; HEAD `bytes=999-` → 200 where real S3 answers 416. Add both HEAD cases to the wire test. Do NOT re-open: the four iteration-2 items (confirmed fixed, keep their tests), the iteration-2 dismissals (wildcard conditionals on pre-ADR-0047 records), and the brief-sanctioned out-of-scope set (multi-range, If-Range, multi-ETag lists, conditional PUT). The minor advisory observations (case-insensitive `BYTES=`, multi-tag If-Match list, pre-epoch clamp blind spot) remain advisory — do not chase them.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
