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

Review of issue 510: add S3 byte-range GET/HEAD responses and ETag/date conditional handling while preserving version-coherent, covering-chunk-only reads.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete: single ranges, 206/416 framing, conditional precedence, malformed-range fallback, and chunk-read economy are independently traceable to the wire entry points at `crates/gateway-s3/src/lib.rs:1644` and the ranged storage seam at `crates/server/src/lib.rs:390`. |
| C2 Reproduction (red pre-fix) | PASS | With only `crates/server/tests/s3_range_conditional.rs` added to the unpatched target base, the suite failed 11/12 for the expected ignored-header behavior (for example the first 206 assertion at `crates/server/tests/s3_range_conditional.rs:300` observed 200), establishing a behavioral red rather than a compile failure. |
| C3 Change | PASS | The change stays within the required protocol/read seams and focused wire test: request dispatch delegates at `crates/gateway-s3/src/lib.rs:1621`, while the real gateway resolves metadata and covering chunks from one committed inode at `crates/server/src/lib.rs:406`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused red→green plus the completed fmt/clippy/build/workspace-test legs is sufficient for acceptance — the patched wire suite passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`, but `cargo xtask ci` could not independently complete `cargo deny check` because the sandbox could not lock the read-only Cargo advisory database. |
| C5 Causal adequacy | PASS | The implementation removes the ignored-header and whole-read causes through parsed protocol decisions and a chunk-aware range seam (`crates/gateway-s3/src/lib.rs:1663`, `crates/server/src/lib.rs:425`); no capability probe or in-capability runtime guard triggers the symptom-guard smell test. |
| T1 Structure | PASS | Protocol grammar and response policy remain in the S3 layer, while protocol-neutral range resolution/read behavior remains in gateway-core/server (`crates/gateway-s3/src/lib.rs:1983`, `crates/server/src/lib.rs:390`). |
| T2 Shape | PASS | The seam returns metadata and outcome together, so callers cannot accidentally frame bytes from a second version; the production implementation constructs both from one inode resolve at `crates/server/src/lib.rs:406`. |
| T3 Runtime | PASS | The in-process signed-wire suite passed all 12 tests, including covering-chunk economy at `crates/server/tests/s3_range_conditional.rs:705`, obsolete dates at `crates/server/tests/s3_range_conditional.rs:603`, and ranged HEAD at `crates/server/tests/s3_range_conditional.rs:653`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether local affected-path history is enough to exclude competing closed/rejected work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review record, so collision risk cannot be fully discharged. |
| T5 Judgment | PASS | The patch honors the previously owed decisions on atomic conditional/body selection and HEAD ranges: conditionals judge the same resolved metadata at `crates/gateway-s3/src/lib.rs:1690`, and HEAD resolves satisfiable/unsatisfiable ranges at `crates/gateway-s3/src/lib.rs:1765`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process S3 gateway evidence represents the intended Alpha client workload — signed loopback tests prove headers, bodies, conditionals, and chunk economy, but final product fitness remains a human acceptance judgment (`crates/server/tests/s3_range_conditional.rs:283`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether focused red→green plus the completed fmt/clippy/build/workspace-test legs is sufficient for acceptance — the patched wire suite passed 12/12 at `crates/server/tests/s3_range_conditional.rs:283`, but `cargo xtask ci` could not independently complete `cargo deny check` because the sandbox could not lock the read-only Cargo advisory database.
- [x] T4 Contribution — Decide whether local affected-path history is enough to exclude competing closed/rejected work — merged/all-ref history was checked for all four affected paths, but this checkout exposes no mechanically authoritative closed/rejected review record, so collision risk cannot be fully discharged.
- [x] Validation — fitness-to-purpose — Decide whether the in-process S3 gateway evidence represents the intended Alpha client workload — signed loopback tests prove headers, bodies, conditionals, and chunk economy, but final product fitness remains a human acceptance judgment (`crates/server/tests/s3_range_conditional.rs:283`).
- [x] **Stale doc-comment on `serve_get` describes the REJECTED design**:
- [x] **The brief's off-Check acceptance claim is not verifiable in this tree**: the

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_510 §6.4: file a bug (assign milestone 0.1 Alpha) — stale `serve_get` doc-comment at `crates/gateway-s3/src/lib.rs:1646-1649` describes the rejected iteration-3 head-then-read design; code is correct (one resolve, stream dropped on 304/412), doc misleads. Doc-only fix.
