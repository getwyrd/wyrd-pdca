# Result — issue 503 / object-metadata-model

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the gateway stores and returns object metadata beyond byte size: an `ETag`
- Success criterion: through the real wire path, a signed `PutObject` response
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical change: the object-metadata model on the inode record + its

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

Task under review: issue #503 adds persisted object metadata and exposes ETag, Content-Type, and Last-Modified across the real S3 PUT/GET wire path.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is concrete and falsifiable: the same persisted digest and declared type must cross PUT, the metadata commit, and GET, with a valid HTTP date (`crates/server/tests/s3_object_metadata.rs:202`). |
| C2 Reproduction (red pre-fix) | PASS | A clean-base scratch run with only the new test failed at the intended missing-PUT-ETag assertion, establishing the stated pre-fix symptom (`crates/server/tests/s3_object_metadata.rs:224`). |
| C3 Change | PASS | The change stays within the declared model, neutral gateway seam, wire surface, compatibility call sites, test, and ADR; publication metadata is atomic while repair preserves it (`crates/core/src/metadata.rs:520`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the independently confirmed focused red→green plus successful fmt/clippy/build/workspace tests is sufficient — the full `cargo xtask ci` rerun could not complete because `cargo deny` could not lock the host's read-only Cargo advisory DB, so the asserted deny leg remains provisional (`crates/server/tests/s3_object_metadata.rs:205`). |
| C5 Causal adequacy | PASS | The remedy changes the persisted publication model rather than probing or guarding an optional runtime capability, and repair explicitly retains the recorded values (`crates/core/src/metadata.rs:527`). |
| T1 Structure | PASS | The architectural ownership is coherent: protocol-neutral values live in the gateway/core seams and HTTP quoting/date rendering remains in the S3 adapter (`crates/gateway-s3/src/lib.rs:703`). |
| T2 Shape | PASS | Backward compatibility turns on absent JSON fields decoding to `None`; all three persisted fields are optional and serde-defaulted (`crates/core/src/metadata.rs:265`). |
| T3 Runtime | PASS | The patched real loopback/redb/fs test passed and observed byte-identical GET plus matching ETag, declared Content-Type, and formatted Last-Modified (`crates/server/tests/s3_object_metadata.rs:231`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected work contains overlapping prior art — `git log --all` by every affected production path found no object-metadata commit, but the available refs do not mechanically cover closed/rejected work (`crates/gateway-s3/src/lib.rs:558`). |
| T5 Judgment | NEEDS-HUMAN | The accepting authority must approve ADR-0047's SHA-256 opaque-token and flat persisted-record decisions because the brief declares the new ADR a project-defined human-only item (`docs/design/adr/0047-object-metadata-model.md:29`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether SHA-256-as-opaque-ETag provides the intended SDK compatibility despite clients that assume single-part ETag equals MD5 — this determines whether the feature solves the motivating interoperability problem (`docs/design/adr/0047-object-metadata-model.md:67`). |

### Advisory — adversary

# Adversarial review — issue #503 (object-metadata-model)

## Evidence attack: red→green independently re-run — could not refute

Re-executed the proof myself in a scratch copy of `$PDCA_TARGET` (patched tree, then the
patch reverse-applied with only the new test kept):

- **GREEN** (patched): `cargo test -p wyrd-server --test s3_object_metadata` → 1 passed.
- **RED** (base + test): compiles **cleanly** against the base API and fails at exactly the
  asserted point — `panicked at crates/server/tests/s3_object_metadata.rs:225: a PutObject
  response must carry an ETag header (ADR-0047); pre-fix it has none`. The red is
  behavioral, not a compile artifact.
- The test drives the **production path** (real TCP loopback listener, real SigV4, redb +
  fs-tempdir stack, 8-byte chunks so the body spans chunks) and its ETag oracle is an
  **independent** SHA-256 computed in the test (`s3_object_metadata.rs:1172-1179`,
  `:1238`) — not an echo of the server's value, so a wire layer inventing a token fails.
  The IMF-fixdate validator (`:1184-1224`) is a strict shape check, not a substring match.
- Attempted to refute via: unsigned `content-type` slipping past SigV4 (no — the verifier
  honors the client-declared `SignedHeaders` list, `crates/gateway-s3/src/sigv4.rs:413-446`,
  and the in-tree signer signs `host;x-amz-content-sha256;x-amz-date` only, `sigv4.rs:594`,
  matching how the harness sends it); struct-update precedence dropping metadata in
  `commit_chunk_map` (no — listed fields win, `..prior.clone()` fills only the metadata
  trio, `crates/core/src/metadata.rs:527-536`); `http_date`/`civil_from_days` arithmetic
  (hand-checked against the RFC-7231 exemplar `Sun, 06 Nov 1994 08:49:37 GMT` including the
  weekday offset — correct, `crates/gateway-s3/src/lib.rs:869-908`); commit atomicity
  (metadata is stamped on the plan before `commit_written`, landing in the same CAS batch
  as the chunk map, `crates/server/src/lib.rs:310-315`). **Could not refute.**

## Fix attacks — findings

- NEEDS-HUMAN [impl] — The brief's **load-bearing** repair-preservation invariant
  ("a repair must not move `Last-Modified` or drop the content type", brief Design §"Which
  commits set metadata") is implemented in four places (`crates/core/src/metadata.rs:535`,
  `crates/custodian/src/backfill.rs:127`, `crates/custodian/src/rebalance.rs:285`,
  `crates/custodian/src/reconstruction.rs:576` — the `..prior.clone()` lines) but has
  **zero test coverage**: every custodian/core test seeds records via `..Default::default()`
  (all-`None` metadata), so preservation is vacuously true. Concrete failing case: delete
  any one of those `..prior.clone()` lines and every gate in `check-gates.json` still
  passes — the load-bearing half of ADR-0047 is unguarded. A small test (seed a record with
  `etag`/`content_type`/`modified` set, run a repair/backfill commit, assert the trio
  survives) closes it.
- NEEDS-HUMAN [impl] — No **overwrite** test: the freshness half of the same invariant
  (a second PUT stamps a NEW ETag/Last-Modified, `crates/core/src/metadata.rs:579-581`)
  is likewise untested — the shipped test does exactly one PUT + one GET. Concrete failing
  case: replace `meta.etag.clone()` with `prior.etag.clone()` in
  `commit_chunk_map_superseding` and all gates stay green while GET serves a stale ETag
  for rewritten content.
- The buffered/CLI write path commits **no metadata**: `Gateway::put_object`
  (`crates/server/src/lib.rs:158-166`, called from `cli.rs:1432`) leaves
  `WritePlan::object_meta` at its all-`None` default, so a CLI **overwrite of an
  S3-written object erases** its stored ETag/content-type/modified. This degrades to
  `None` (no header) rather than serving a stale ETag — the correct failure direction, and
  explicitly sanctioned by the `WritePlan::object_meta` doc comment
  (`crates/core/src/write.rs:63-69`) — but note the CLI path buffers the whole body and
  could trivially compute the digest; left as observed asymmetry, not a defect.
- No test decodes an **old-format record** (pre-metadata JSON) at the wire — the
  `#[serde(default)]` degrade path (brief: "never to an error") is trivially correct by
  construction but unexercised; minor, subsumed by the two [impl] bullets above if a
  seeded-record test lands.
- Cosmetic: the in-crate test doubles return `Ok(String::new())` for
  `put_object_streaming` (`crates/gateway-s3/src/lib.rs:1186`, `:1470`), so their PUT
  responses carry `etag: ""` — test-only, no production effect.

## Verdict attacks

- NEEDS-HUMAN — ADR-0047 (`docs/design/adr/0047-object-metadata-model.md`) ships with
  `status: Accepted` before the maintainer's sign-off, and the ADR + README index change is
  a project-defined human-only item. This is **pre-declared in the brief** (Scope: "the
  reviewer WILL route it to §6"), so it is expected, not a defect — but acceptance of the
  ADR (including the SHA-256-not-MD5 ETag decision and `Last-Modified` = commit time) is
  the human's call, not the gates'.
- `check-gates.json` C4-verify's "red without the fix, green with it" claim: **verified
  independently above; warranted.** No rationalization found in the gate rows — the only
  unguarded claims are the two untested invariants filed under [impl].

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether the independently confirmed focused red→green plus successful fmt/clippy/build/workspace tests is sufficient — the full `cargo xtask ci` rerun could not complete because `cargo deny` could not lock the host's read-only Cargo advisory DB, so the asserted deny leg remains provisional (`crates/server/tests/s3_object_metadata.rs:205`).
- [x] T4 Contribution — Decide whether closed/rejected work contains overlapping prior art — `git log --all` by every affected production path found no object-metadata commit, but the available refs do not mechanically cover closed/rejected work (`crates/gateway-s3/src/lib.rs:558`).
- [x] T5 Judgment — The accepting authority must approve ADR-0047's SHA-256 opaque-token and flat persisted-record decisions because the brief declares the new ADR a project-defined human-only item (`docs/design/adr/0047-object-metadata-model.md:29`).
- [ ] Validation — fitness-to-purpose — Decide whether SHA-256-as-opaque-ETag provides the intended SDK compatibility despite clients that assume single-part ETag equals MD5 — this determines whether the feature solves the motivating interoperability problem (`docs/design/adr/0047-object-metadata-model.md:67`).
- [ ] The brief's **load-bearing** repair-preservation invariant
- [ ] No **overwrite** test: the freshness half of the same invariant
- [ ] ADR-0047 (`docs/design/adr/0047-object-metadata-model.md`) ships with

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
- Iteration delta (if iterating): Rejected only for the two adversary-flagged test-coverage gaps on ADR-0047's load-bearing invariants; the implementation itself stands — do not redesign it, add the missing tests: 1) Repair-preservation: seed a record with etag/content_type/modified set, run a repair/backfill commit, and assert the metadata trio survives — guards the `..prior.clone()` preservation lines (crates/core/src/metadata.rs:535, crates/custodian/src/backfill.rs:127, rebalance.rs:285, reconstruction.rs:576), currently vacuously true because all existing tests seed all-None metadata. 2) Overwrite freshness: a second PUT of new content must stamp a NEW ETag/Last-Modified (commit_chunk_map_superseding, crates/core/src/metadata.rs:579-581) — the shipped test does exactly one PUT + one GET, so a stale-ETag regression would pass every gate. Context already settled by the human (do not revisit): T4 prior-art cleared; T5 ADR-0047 decisions (SHA-256 opaque ETag, flat record) approved. The reviewer's partial CI rerun (cargo deny env issue) and the MD5-compat question are recorded as §10 Act candidates / a foundations tracking issue, not rebuild scope.
- By / date: Eduard Ralph / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_503: reviewer environment could not rerun the full `cargo xtask ci` (cargo deny failed to lock the host's read-only Cargo advisory DB) — the deny leg of the independent rerun stayed provisional; fix the reviewer host/env so the full CI rerun is reproducible.
- issue_503: file a tracking issue, assigned to foundations, on SHA-256-as-opaque-ETag SDK compatibility — whether clients that assume single-part ETag == MD5 undermine the motivating interoperability goal (ADR-0047 §validation).
