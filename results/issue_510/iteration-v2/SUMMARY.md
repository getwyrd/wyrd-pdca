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

Review of issue 510: add byte-range GETs and GET/HEAD conditional-request handling to the S3 gateway while reading only covering chunks.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives executable wire outcomes, precedence rules, range edge cases, and a covering-chunk oracle; the required behavior is decision-complete. |
| C2 Reproduction (red pre-fix) | PASS | In an isolated base clone with only the new test applied, 8/9 cases failed on ignored Range/conditional headers (for example expected 206 but got 200 at `crates/server/tests/s3_range_conditional.rs:291`). |
| C3 Change | FAIL | Wildcard preconditions must turn on representation existence, but the object is already known to exist and `*` instead returns `stored.is_some()`, so legacy objects without ETags incorrectly yield 412 for `If-Match: *` and 200 for `If-None-Match: *` (`crates/gateway-s3/src/lib.rs:2114`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the independently confirmed targeted red→green is sufficient despite the host-blocked final policy scan — all 9 targeted tests pass at `crates/server/tests/s3_range_conditional.rs:285`, and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not acquire the read-only advisory DB lock. |
| C5 Causal adequacy | FAIL | The implementation removes the ignored-header cause and bounds chunk reads, but the required `*` conditional semantics remain incomplete for pre-ADR-0047 objects because matching is coupled to ETag availability (`crates/gateway-s3/src/lib.rs:2114`). |
| T1 Structure | PASS | The protocol-neutral range seam and wire-layer resolution preserve the gateway boundary, while the server maps spans to overlapping chunks (`crates/gateway-core/src/lib.rs:211`; `crates/server/src/lib.rs:399`). |
| T2 Shape | PASS | The new range method, extracted GET/HEAD handlers, and focused wire test fit existing trait/dispatch/test shapes without a new dependency (`crates/gateway-core/src/lib.rs:211`; `crates/gateway-s3/src/lib.rs:1620`). |
| T3 Runtime | PASS | The in-process wire suite observed correct 206/304/412/416 responses and the narrow-range oracle observed exactly one covering chunk fetched (`crates/server/tests/s3_range_conditional.rs:576`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether collision risk is acceptable — merged/all-ref history was checked by each affected path, but this checkout has no mechanically authoritative record of closed/rejected work for those paths. |
| T5 Judgment | FAIL | Shipping wildcard support that changes behavior based on whether an existing legacy record happens to carry an ETag violates the stated compatibility behavior and needs correction or an explicit scope decision (`crates/gateway-s3/src/lib.rs:2109`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the feature is fit for real S3 clients after fixing wildcard semantics — the in-process signed-wire evidence is strong, but the reserved AWS CLI round-trip acceptance remains manual. |

### Advisory — adversary

# check-advisory-adversary.md — issue 510 / range-conditional-get

- **Evidence attack — attempted, could not refute.** Independently re-ran the red→green proof in scratch (fresh copy of `$PDCA_TARGET`, `cargo test --test s3_range_conditional`): with the patch, 9/9 pass; with the three production files reverted to base (`git show HEAD:`) and the test kept, 8/9 fail **by assertion** (200 where 206/304/412/416 was asserted; `Accept-Ranges` absent) — exactly the claimed red. The test drives the production path (loopback TCP, SigV4-signed requests, real `Gateway` over `FsChunkStore`), and the anti-wire-side-discard oracle counts real fragment reads: the `PlacementChunkStore::get_fragment_at` default does delegate to `get_fragment` (crates/traits/src/lib.rs:641-643), so the counting wrapper observes the actual read path. `check-gates.json` C4-verify PASS is warranted. (One test, `invalid_conditional_date_is_ignored_not_misparsed`, is green on the base too — it guards the iteration-1 misparse regression, not the red; acceptable.)

- NEEDS-HUMAN [impl] — **A valid pre-epoch `If-Unmodified-Since` is ignored instead of firing 412.** `parse_http_date` (crates/gateway-s3/src/lib.rs:2133, tail at :2160-2166) maps any pre-1970 IMF-fixdate to `None` via the failed `u64::try_from`, and `evaluate_conditionals` (crates/gateway-s3/src/lib.rs:2072-2079) treats `None` as "ignore the conditional". Probe confirmed: `If-Unmodified-Since: Fri, 01 Jan 1960 00:00:00 GMT` on a freshly-PUT object answers **200**; RFC 9110/S3 semantics require **412** (the object was modified strictly after that valid date). The doc comment ("a pre-epoch instant … is simply not satisfiable and yields `None`") rationalizes the wrong branch — for IUS, "ignore" inverts the answer. Fix: clamp a pre-epoch instant to 0 (or return i64) instead of failing the parse.

- NEEDS-HUMAN [impl] — **A `+`-signed range is honoured as 206; the brief decided malformed → 200 "exactly as real S3 does".** `parse_range` (crates/gateway-s3/src/lib.rs:1938, arms at :1952-1959) parses via `u64::from_str`, which accepts a leading `+`; RFC 9110's grammar is DIGIT-only, so `bytes=+8-+15` is malformed. Probe confirmed: it answers **206** with `Content-Range: bytes 8-15/64` instead of the full 200. The test's malformed-forms set (crates/server/tests/s3_range_conditional.rs:373-379) omits this shape, so nothing guards it. Fix: reject any non-ASCII-digit byte in the two positions before parsing (also removes the undocumented interior-whitespace tolerance from the `trim()`s at :1949).

- NEEDS-HUMAN — **The ranged 206 can mix object versions under a racing overwrite (TOCTOU between the two resolves).** `serve_get` resolves metadata once (`head_object`, crates/gateway-s3/src/lib.rs:1682) and then the bytes from a *second* resolve (`get_object_range`, :1715; the server impl re-resolves the inode at crates/server/src/lib.rs:405). A PUT landing between the two yields a 206 whose `ETag`/`Last-Modified`/`Content-Range` total describe the OLD object but whose body bytes come from the NEW one — a version-mixed response real S3 cannot emit, and one that poisons any ETag-keyed client cache. The same window lets a passed `If-Match` (evaluated at :1690) stream a different object than the one the precondition fenced (the conditional-pass full-GET at :1697-1706 at least serves self-consistent headers from its own resolve). The seam shape (`get_object_range` returns a bare `ObjectStream`, no metadata) cannot express an atomic conditional+ranged read — whether this window is acceptable at Alpha, or the seam should return meta+stream from one inode resolve, is an architectural call.

- NEEDS-HUMAN — **The trait default `get_object_range → Ok(None)` makes a non-overriding gateway answer a ranged GET of an EXISTING object with 404.** crates/gateway-core/src/lib.rs:211-224 defaults to `Ok(None)`, and the wire maps `None` to `NoSuchKey` (crates/gateway-s3/src/lib.rs:1717-1719) — after the same gateway advertised `Accept-Ranges: bytes` on the plain 200 (apply_object_headers, :1799). `None` conflates "no such key" with "no ranged-read support"; the cited `list_container` precedent (crates/gateway-core/src/lib.rs:262-268) has no such ambiguity (no container concept ≈ not found). The doc acknowledges the landmine but ships it; a correctness-preserving default (seam-side full-read+slice, or no default at all) is the alternative. API-contract judgment for the human.

- **Minor, unprefixed:** (a) `etag_matches` (crates/gateway-s3/src/lib.rs:2112-2117) keys `*` on `stored.is_some()`, so against a pre-ADR-0047 record (no stored ETag) `If-None-Match: *` serves 200 (RFC/S3: 304 — a representation exists) and `If-Match: *` answers 412 (RFC: serve) — legacy-records-only, arguably covered by the design's "degrade safely" but it extends the *specific-tag* rule to `*`. (b) `serve_head` (crates/gateway-s3/src/lib.rs:1732-1737) ignores `Range` entirely, while real S3 HeadObject honours it (416 / part-sized `Content-Length`); within the brief's GET-only Range scope, noted for fitness-to-purpose only.

- **Verdict attack — attempted, could not land elsewhere.** Checked the anti-discard oracle for tautology (it counts the real store's fragment reads, not a mock; a stream-then-discard impl fetches ≥2 chunks for `bytes=8-15` and fails), the 304 validators, the `bytes=-0` → 416 and weak-`If-Match` → 412 carry-forwards (all asserted and green), and `finish_response`'s 304/HEAD body-less classification (crates/gateway-s3/src/lib.rs:1288-1302 — the patch's claim holds). The two `[impl]` bullets above are the only concrete failing cases found; the two plain NEEDS-HUMAN items are contract/concurrency judgments, not gate failures.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether the independently confirmed targeted red→green is sufficient despite the host-blocked final policy scan — all 9 targeted tests pass at `crates/server/tests/s3_range_conditional.rs:285`, and fmt/clippy/build/workspace tests passed, but `cargo deny check` could not acquire the read-only advisory DB lock.
- [ ] T4 Contribution — Decide whether collision risk is acceptable — merged/all-ref history was checked by each affected path, but this checkout has no mechanically authoritative record of closed/rejected work for those paths.
- [ ] Validation — fitness-to-purpose — Decide whether the feature is fit for real S3 clients after fixing wildcard semantics — the in-process signed-wire evidence is strong, but the reserved AWS CLI round-trip acceptance remains manual.
- [ ] **A valid pre-epoch `If-Unmodified-Since` is ignored instead of firing 412.** `parse_http_date` (crates/gateway-s3/src/lib.rs:2133, tail at :2160-2166) maps any pre-1970 IMF-fixdate to `None` via the failed `u64::try_from`, and `evaluate_conditionals` (crates/gateway-s3/src/lib.rs:2072-2079) treats `None` as "ignore the conditional". Probe confirmed: `If-Unmodified-Since: Fri, 01 Jan 1960 00:00:00 GMT` on a freshly-PUT object answers **200**; RFC 9110/S3 semantics require **412** (the object was modified strictly after that valid date). The doc comment ("a pre-epoch instant … is simply not satisfiable and yields `None`") rationalizes the wrong branch — for IUS, "ignore" inverts the answer. Fix: clamp a pre-epoch instant to 0 (or return i64) instead of failing the parse.
- [ ] **A `+`-signed range is honoured as 206; the brief decided malformed → 200 "exactly as real S3 does".** `parse_range` (crates/gateway-s3/src/lib.rs:1938, arms at :1952-1959) parses via `u64::from_str`, which accepts a leading `+`; RFC 9110's grammar is DIGIT-only, so `bytes=+8-+15` is malformed. Probe confirmed: it answers **206** with `Content-Range: bytes 8-15/64` instead of the full 200. The test's malformed-forms set (crates/server/tests/s3_range_conditional.rs:373-379) omits this shape, so nothing guards it. Fix: reject any non-ASCII-digit byte in the two positions before parsing (also removes the undocumented interior-whitespace tolerance from the `trim()`s at :1949).
- [ ] **The ranged 206 can mix object versions under a racing overwrite (TOCTOU between the two resolves).** `serve_get` resolves metadata once (`head_object`, crates/gateway-s3/src/lib.rs:1682) and then the bytes from a *second* resolve (`get_object_range`, :1715; the server impl re-resolves the inode at crates/server/src/lib.rs:405). A PUT landing between the two yields a 206 whose `ETag`/`Last-Modified`/`Content-Range` total describe the OLD object but whose body bytes come from the NEW one — a version-mixed response real S3 cannot emit, and one that poisons any ETag-keyed client cache. The same window lets a passed `If-Match` (evaluated at :1690) stream a different object than the one the precondition fenced (the conditional-pass full-GET at :1697-1706 at least serves self-consistent headers from its own resolve). The seam shape (`get_object_range` returns a bare `ObjectStream`, no metadata) cannot express an atomic conditional+ranged read — whether this window is acceptable at Alpha, or the seam should return meta+stream from one inode resolve, is an architectural call.
- [ ] **The trait default `get_object_range → Ok(None)` makes a non-overriding gateway answer a ranged GET of an EXISTING object with 404.** crates/gateway-core/src/lib.rs:211-224 defaults to `Ok(None)`, and the wire maps `None` to `NoSuchKey` (crates/gateway-s3/src/lib.rs:1717-1719) — after the same gateway advertised `Accept-Ranges: bytes` on the plain 200 (apply_object_headers, :1799). `None` conflates "no such key" with "no ranged-read support"; the cited `list_container` precedent (crates/gateway-core/src/lib.rs:262-268) has no such ambiguity (no container concept ≈ not found). The doc acknowledges the landmine but ships it; a correctness-preserving default (seam-side full-read+slice, or no default at all) is the alternative. API-contract judgment for the human.

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
- Iteration delta (if iterating): Human sign-off (2026-07-20): the range/conditional feature works and red→green is independently confirmed, but four confirmed findings must be fixed before accept: 1. Pre-epoch `If-Unmodified-Since` must fire 412, not be silently ignored — clamp a pre-1970 IMF-fixdate to epoch 0 in `parse_http_date` instead of failing the parse (crates/gateway-s3/src/lib.rs:2133, tail :2160-2166); for IUS, "ignore" inverts the answer. 2. A `+`-signed range spec (`bytes=+8-+15`) must be treated as malformed → full 200 (the brief's decided behavior), not honoured as 206 — reject any non-ASCII-digit byte in the two range positions before parsing (crates/gateway-s3/src/lib.rs:1938-1959, drop the interior-whitespace `trim()` tolerance at :1949), and add this shape to the malformed-forms test set (crates/server/tests/s3_range_conditional.rs:373-379). 3. Close the TOCTOU window on ranged GET: headers come from one resolve (`head_object`) and body bytes from a second (`get_object_range`), so a racing PUT can emit a version-mixed 206 that poisons ETag-keyed caches — reshape the seam so meta+stream come from ONE inode resolve (a bare `ObjectStream` return cannot express an atomic conditional+ranged read). 4. Remove the `get_object_range` trait-default `Ok(None)` landmine — a non-overriding gateway answers a ranged GET of an EXISTING object with 404 after advertising `Accept-Ranges: bytes` — use a correctness-preserving default (seam-side full-read + slice) or no default at all (crates/gateway-core/src/lib.rs:211-224; wire mapping crates/gateway-s3/src/lib.rs:1717-1719). Explicitly dismissed at sign-off — do NOT re-open: wildcard `If-Match`/`If-None-Match` behavior on pre-ADR-0047 records without stored ETags (all current objects carry SHA-256 ETags; legacy-only concern, recorded as a §10 Act candidate).
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_510: wildcard `If-Match`/`If-None-Match` semantics on pre-ADR-0047 records without stored ETags — dismissed at sign-off (all current objects carry SHA-256 ETags; legacy-only); revisit only if a migration story for pre-ETag records appears.
