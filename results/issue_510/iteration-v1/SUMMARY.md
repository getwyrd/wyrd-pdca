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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 
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
| C1 Spec | PASS | The acceptance contract fixes the supported range forms, conditional precedence, S3 comparison semantics, and no-whole-object-read requirement in `brief.md:10`. |
| C2 Reproduction (red pre-fix) | PASS | Independently retaining the wire test while restoring the three production files produced 0/7 passing tests, with the expected 200 responses and absent `Accept-Ranges` at `crates/server/tests/s3_range_conditional.rs:291`. |
| C3 Change | FAIL | Wildcard preconditions must turn on existence, not availability of stored ETag metadata; `If-Match: *` wrongly returns 412 and `If-None-Match: *` wrongly returns 200 for an existing pre-ADR-0047 object because `etag_matches` returns `stored.is_some()` at `crates/gateway-s3/src/lib.rs:2101`. |
| C4 Verification (red→green) | FAIL | The focused loopback test independently transitions from 0/7 to 7/7, but the required `typos` gate reproducibly rejects four patch-added spellings, including `unparseable` at `crates/gateway-s3/src/lib.rs:2056`. |
| C5 Causal adequacy | PASS | The fix removes the ignored-header cause at the wire path and performs bounded reads at the storage seam; the covering-chunk selection at `crates/server/src/lib.rs:408` is exercised by the one-of-eight chunk oracle at `crates/server/tests/s3_range_conditional.rs:499`. |
| T1 Structure | PASS | Protocol parsing remains in the S3 layer while the core/server seam accepts only offsets and lengths, preserving layer ownership at `crates/server/src/lib.rs:399`. |
| T2 Shape | PASS | The separate ranged-read seam has a bounded stream result and leaves the existing full-read path intact, matching the requested cost shape at `crates/server/src/lib.rs:399`. |
| T3 Runtime | PASS | The applied target completed `cargo test -p wyrd-server --test s3_range_conditional` with 7/7 passing, including cross-chunk slicing and the covering-chunk count at `crates/server/tests/s3_range_conditional.rs:274`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked by affected path, but this checkout contains no mechanically authoritative closed/rejected review history, so collision risk remains. |
| T5 Judgment | FAIL | Decide and test the promised `*` semantics for metadata-less existing objects before acceptance — the current tests cover only concrete ETags at `crates/server/tests/s3_range_conditional.rs:390`, so the legacy-object regression is unguarded. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the behavior meets real-client needs — after the two defects are fixed, run `aws s3api get-object --endpoint-url <gateway> --bucket wyrd-bucket --key <multi-chunk-key> --range bytes=8-15 <output>` and confirm 206 headers plus exact slice bytes, because the in-process oracle does not exercise the registered AWS CLI acceptance path. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked by affected path, but this checkout contains no mechanically authoritative closed/rejected review history, so collision risk remains.
- [ ] Validation — fitness-to-purpose — Decide whether the behavior meets real-client needs — after the two defects are fixed, run `aws s3api get-object --endpoint-url <gateway> --bucket wyrd-bucket --key <multi-chunk-key> --range bytes=8-15 <output>` and confirm 206 headers plus exact slice bytes, because the in-process oracle does not exercise the registered AWS CLI acceptance path.
- [ ] **The gating C4-ci red (typos, exit 2) is caused by this patch's own
- [ ] **An invalid HTTP-date is mis-parsed, not ignored — the code
- [ ] **`If-Match` accepts a weak entity-tag as a match.**
- [ ] **`Range: bytes=-0` answers 200; real S3 answers 416
- [ ] **Untested success-relevant behavior: the 304's cache validators.**
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — T4 Contribution — Decide whether affected-path prior art is clear of closed/rejected competing work — merged/all-ref history was checked by affected path, but this checkout contains no mechanically authoritative closed/rejected review history, so collision risk remains.; **The gating C4-ci red (typos, exit 2) is caused by this patch's own; **An invalid HTTP-date is mis-parsed, not ignored — the code; **`If-Match` accepts a weak entity-tag as a match.**; **`Range: bytes=-0` answers 200; real S3 answers 416; **Untested success-relevant behavior: the 304's cache validators.**; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- By / date: auto-iterate / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
