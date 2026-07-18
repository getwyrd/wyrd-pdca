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

Review of issue #503: persist object ETag, Content-Type, and Last-Modified metadata and round-trip it through the S3 PUT/GET wire path while preserving it across repairs and refreshing it on overwrite.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the real signed HTTP/redb loopback seam, including fresh overwrite metadata and repair preservation (`crates/server/tests/s3_object_metadata.rs:206`, `crates/core/tests/mutation_regressions.rs:342`). |
| C2 Reproduction (red pre-fix) | PASS | On the base, both copied wire tests failed at PUT because no ETag header existed (`crates/server/tests/s3_object_metadata.rs:225`, `crates/server/tests/s3_object_metadata.rs:295`). |
| C3 Change | PASS | The public record/seam and wire response cover the required persisted-to-wire path, with optional fields for old records and explicit PUT response/date rendering (`crates/core/src/metadata.rs:270`, `crates/gateway-core/src/lib.rs:53`, `crates/gateway-s3/src/lib.rs:707`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the independently reproduced red→green plus passing fmt/clippy/build/workspace tests despite `cargo deny` being blocked by the host's read-only advisory lock — the asserted full CI green could not be completely reproduced (`crates/server/tests/s3_object_metadata.rs:206`). |
| C5 Causal adequacy | PASS | The change removes the missing persisted-metadata cause across publication, read, and repair paths; no capability probe or runtime guard masks an eager/load-time failure (`crates/core/src/metadata.rs:520`, `crates/core/src/metadata.rs:562`). |
| T1 Structure | PASS | The responsibility split remains record/commit in core, neutral streaming seam in gateway-core, and header formatting in gateway-s3 (`crates/core/src/metadata.rs:298`, `crates/gateway-core/src/lib.rs:53`, `crates/gateway-s3/src/lib.rs:727`). |
| T2 Shape | PASS | Flat optional persisted fields preserve old-record decoding while the public read shape exposes exactly the three metadata values required downstream (`crates/core/src/metadata.rs:270`, `crates/gateway-core/src/lib.rs:53`). |
| T3 Runtime | PASS | Real loopback tests passed for initial PUT/GET and overwrite freshness, and repair/backfill/rebalance/reconstruction preservation tests passed in the workspace run (`crates/server/tests/s3_object_metadata.rs:206`, `crates/server/tests/s3_object_metadata.rs:281`, `crates/custodian/tests/reconstruction.rs:444`). |
| T4 Contribution | PASS | Affected-path `git log --all` found no merged object-metadata implementation, and the brief records the affected-path closed/rejected-work check as previously human-cleared; this contribution is not duplicate prior art (`docs/design/adr/README.md:61`). |
| T5 Judgment | NEEDS-HUMAN | Accept ADR-0047's flat inode model and opaque SHA-256 ETag as the project architecture decision — its Accepted status governs later HeadObject, copy, and multipart work (`docs/design/adr/README.md:61`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether SHA-256-as-opaque-ETag and the exercised in-process redb wire topology match intended client compatibility and production purpose — automated tests establish behavior, not product fitness (`crates/server/tests/s3_object_metadata.rs:206`). |

### Advisory — adversary

# Adversarial review — issue #503 (object-metadata-model), iteration 2

Verdict: the core fix stands — I re-ran the green proof locally and verified the red
structurally against the base — but the carry-forward's overwrite-freshness demand is only
**half**-tested, and the patch introduces one new panic edge on the GET path.

## Findings

- NEEDS-HUMAN [impl] — **`Last-Modified` overwrite-freshness is untested — carry-forward
  item 2 is only half-satisfied.** The sign-off rationale demanded "a second PUT of new
  content must stamp a NEW ETag/**Last-Modified**". The shipped wire test
  (`crates/server/tests/s3_object_metadata.rs:346-352`) asserts ETag and Content-Type
  freshness but only shape-validates `Last-Modified` with `is_imf_fixdate` — it cannot
  assert freshness because both PUTs land within the same wall-clock second. No unit test
  covers it either: the only `modified` assertions in the tree are the repair-*preservation*
  tests (where the stale value IS the expected value). Concrete failing case: regress
  `crates/core/src/metadata.rs:581` (or the leased twin at `:641`) from
  `modified: meta.modified` to `modified: prior.modified` — the prior value is
  `Some(first-PUT millis)`, still a valid IMF-fixdate, so **every gate stays green** while
  overwritten objects serve the first publication's `Last-Modified` forever. Fix by
  iterating: add a unit test on `commit_chunk_map_superseding{,_leased}` seeding a prior
  record with a distinct `modified` and asserting the commit stamps `meta.modified`
  (mirror of the preservation test at `crates/core/tests/mutation_regressions.rs:320-364`).

- NEEDS-HUMAN [impl] — **new panic edge: a stored `content_type` that is not a valid HTTP
  header value makes every wire GET of that object panic instead of degrading.** The GET
  arm now feeds a persisted string into the response builder and still unwraps with
  `.expect("streaming response is always valid")`
  (`crates/gateway-s3/src/lib.rs:659`; header set at `:645-650`) — an expectation that was
  true when the headers were constants, and is no longer true in general. The S3 wire PUT
  is guarded (`to_str().ok()` at `lib.rs:1042-1046` keeps values visible-ASCII), but the
  seam is not: `ObjectGateway::put_object_streaming` accepts an arbitrary
  `Option<String>` (`crates/gateway-core/src/lib.rs:119-128`) and `crates/server`
  commits it verbatim (`crates/server/src/lib.rs:310-314`). Concrete failing case:
  `put_object_streaming(key, src, ContentHash::Unverified, Some("text/plain\u{7f}".into()))`
  commits fine; every subsequent wire GET of that key panics the connection task. #504
  (server-side copy) and #506 (HeadObject) will call this seam next. Cheap hardening:
  fall back to `application/octet-stream` when `HeaderValue::from_str` fails, or document
  + enforce the constraint at the seam.

- NEEDS-HUMAN — **ADR-0047 acceptance** (pre-declared, expected — not a defect):
  `docs/design/adr/0047-object-metadata-model.md:4` ships `status: Accepted` before the
  maintainer — the accepting authority — has signed off, and the index row
  (`docs/design/adr/README.md:61`) already lists it Accepted. Frontmatter shape matches
  the ADR-0046 peer; content matches the brief's decided points (SHA-256 opaque ETag,
  flat optional fields, repair-preserves). Human confirms acceptance at sign-off per the
  brief's pre-declared item.

## Refutation attempts that failed (evidence the fix holds)

- **Red→green evidence**: re-ran the shipped tests green at `$PDCA_TARGET`
  (`cargo test -p wyrd-server --test s3_object_metadata` → 2 passed;
  `-p wyrd-core --test mutation_regressions commit_chunk_map_preserves` → 1 passed).
  Red verified structurally against base `HEAD` (0b01454): the base PUT arm answers
  `Ok(()) => empty_response(StatusCode::OK)` and GET hardcodes
  `application/octet-stream` (base `crates/gateway-s3/src/lib.rs:594-611`), and the new
  test file uses only base-era APIs (`sigv4::sign`/`format_amz_date` pre-exist; `sha2` is
  already a `[dependencies]` entry of `wyrd-server`, Cargo.toml:89) — so it compiles on
  base and fails on the ETag assertion, a genuine assertion-red, not a compile-error red.
- **Tautology check**: the ETag oracle is an independent SHA-256 computed in-test
  (`s3_object_metadata.rs:243-249`), not an echo of the server's value; a wire layer
  returning an arbitrary string fails. The test drives the real loopback listener
  (redb + fs tempdir), not a double.
- **Carry-forward item 1 (repair preservation)**: the four new preservation tests seed a
  non-`None` trio and drive the real commits (`mutation_regressions.rs:320`,
  `custodian/tests/backfill.rs:496`, `rebalance.rs:730`, `reconstruction.rs:842`) — each
  asserts the repair *fired* (`Reconciled::Changed` / version bump) before asserting
  preservation, so none is vacuous. `..prior.clone()` regressing to `..Default::default()`
  is now caught. Could not refute.
- **In-tree date formatter**: cross-checked `civil_from_days` + weekday math
  (`gateway-s3/src/lib.rs:727-772`) against Python `datetime` over 200k random instants
  up to year 9999 — zero mismatches; epoch-millis `u64` keeps `days` non-negative so the
  `(days%7+4)%7` weekday is exact. Could not refute.
- **ETag freshness / stale-content-type on overwrite**: wire test 2 asserts both against
  independent oracles after a real overwrite through `commit_chunk_map_superseding_leased`
  (the production path via `commit_written` → `commit_overwrite`). Could not refute.

## Non-blocking observation

- The buffered `put_object` / CLI path (`crates/server/src/lib.rs:158-166`,
  `cli.rs:1432`) commits `ObjectMeta::default()` — a CLI overwrite of a wire-PUT object
  drops its ETag/Content-Type/Last-Modified to `None`. This is the documented degrade
  path (`crates/core/src/write.rs:63-69`) and stale metadata over new bytes would be
  worse, so not filed as a defect; noting it so #506 (HeadObject) doesn't assume the trio
  is always present on committed objects.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the independently reproduced red→green plus passing fmt/clippy/build/workspace tests despite `cargo deny` being blocked by the host's read-only advisory lock — the asserted full CI green could not be completely reproduced (`crates/server/tests/s3_object_metadata.rs:206`).
- [ ] T5 Judgment — Accept ADR-0047's flat inode model and opaque SHA-256 ETag as the project architecture decision — its Accepted status governs later HeadObject, copy, and multipart work (`docs/design/adr/README.md:61`).
- [ ] Validation — fitness-to-purpose — Decide whether SHA-256-as-opaque-ETag and the exercised in-process redb wire topology match intended client compatibility and production purpose — automated tests establish behavior, not product fitness (`crates/server/tests/s3_object_metadata.rs:206`).
- [ ] **`Last-Modified` overwrite-freshness is untested — carry-forward
- [ ] **new panic edge: a stored `content_type` that is not a valid HTTP
- [ ] **ADR-0047 acceptance** (pre-declared, expected — not a defect):

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
- Iteration delta (if iterating): Rejected on the two adversarial-review findings; the implementation and design stand — do not redesign, fix exactly these: 1) Last-Modified overwrite-freshness is still untested (carry-forward item 2 half-met): the wire test only shape-validates the header (both PUTs land in the same second) and no unit test asserts freshness — regressing `modified: meta.modified` to `modified: prior.modified` in `commit_chunk_map_superseding{,_leased}` (crates/core/src/metadata.rs:581 / :641) keeps every gate green. Add a unit test seeding a prior record with a distinct `modified` and asserting the superseding commit stamps the NEW `meta.modified` (mirror the preservation test at crates/core/tests/mutation_regressions.rs:320-364). 2) New panic edge on GET: a stored `content_type` that is not a valid HTTP header value makes every wire GET of that object panic via the `.expect("streaming response is always valid")` unwrap (crates/gateway-s3/src/lib.rs:659; header set at :645-650) — the seam (`ObjectGateway::put_object_streaming`, gateway-core/src/lib.rs:119-128) accepts arbitrary strings and server commits them verbatim. Harden: fall back to `application/octet-stream` when `HeaderValue::from_str` fails (or document + enforce the constraint at the seam). #504/#506 call this seam next. Context already settled by the human (do not revisit): T5/ADR-0047 decisions (SHA-256 opaque ETag, flat record) approved; T4 prior-art cleared; carry-forward item 1 (repair preservation) now satisfied and its four tests must be kept; reviewer's partial CI rerun (cargo deny host lock) is an environment issue, not rebuild scope.
- By / date: Eduard Ralph / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- CLI/buffered `put_object` path commits default metadata — a CLI overwrite drops a wire-PUT object's ETag/Content-Type/Last-Modified trio (documented degrade path); #506 HeadObject must not assume the trio is present.
- Reviewer host's `cargo deny` blocked by read-only advisory-db lock for the second iteration running — recurring reviewer-environment gap, fix at the process/tooling level.
