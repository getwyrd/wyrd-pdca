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

Review of issue #503: persist object ETag, Content-Type, and Last-Modified metadata and surface it consistently through S3 PUT/GET, including overwrite freshness and repair preservation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the signed loopback wire and persisted-record seams, including backward compatibility and repair/overwrite semantics (`brief.md`; `crates/core/src/metadata.rs:262`). |
| C2 Reproduction (red pre-fix) | PASS | In an attributable scratch clone, the shipped wire test failed on the base because both PUT cases lacked ETag, then passed 2/2 with the patch (`crates/server/tests/s3_object_metadata.rs:205`). |
| C3 Change | PASS | The review decision is whether metadata is committed atomically and survives the correct lifecycle transitions; create/overwrite stamp it and repair preserves it (`crates/core/src/metadata.rs:529`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the otherwise green red→green, workspace tests, fmt/clippy/build, and wire coverage despite `cargo deny check` being unreproducible because its advisory DB lock is read-only; this matters because the complete asserted CI gate was not independently discharged (`crates/server/tests/s3_object_metadata.rs:205`). |
| C5 Causal adequacy | PASS | The change removes the missing persisted-model/wire cause rather than adding a capability probe; distinct-time overwrite tests kill stale-publication regressions on both commit paths (`crates/core/tests/mutation_regressions.rs:397`). |
| T1 Structure | PASS | The architectural decision is kept at the neutral gateway seam and persisted core record, with HTTP rendering confined to the S3 layer (`crates/gateway-core/src/lib.rs:118`). |
| T2 Shape | PASS | Public implementers and test doubles type-check under the widened streaming seam, while old JSON records remain decodable through optional serde-default fields (`crates/core/src/metadata.rs:262`). |
| T3 Runtime | PASS | Real loopback redb/fs PUT→GET and overwrite flows passed, and malformed stored Content-Type was directly exercised to return 200 with the octet-stream fallback instead of panicking (`crates/gateway-s3/src/lib.rs:2007`). |
| T4 Contribution | PASS | Affected-path `git log --all` checks across the core/gateway/server/ADR files found no earlier object-metadata implementation or deleted/rejected equivalent; the human-cleared prior-art conclusion remains supported. |
| T5 Judgment | NEEDS-HUMAN | Maintainer must accept the new ADR's flat optional record model and opaque SHA-256 ETag because Accepted ADRs are project-defined human-only decisions with downstream compatibility impact (`docs/design/adr/0047-object-metadata-model.md:29`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether opaque SHA-256 ETags and second-granularity Last-Modified behavior meet real client expectations; this product-compatibility judgment is not settled by the passing in-process wire oracle (`docs/design/adr/0047-object-metadata-model.md:67`). |

### Advisory — adversary

# Adversarial review — issue #503 / object-metadata-model (iteration 3)

Skeptic's pass. I re-ran the red→green proof independently in a scratch copy of the
target (full worktree + cache copied to `$PDCA_SCRATCH`, cleaned up after) and planted
the exact mutants the iteration-2 rejection named. Findings:

- NEEDS-HUMAN — ADR-0047 ships in-slice with frontmatter `status: Accepted`
  (`docs/design/adr/0047-object-metadata-model.md:4`, indexed in
  `docs/design/adr/README.md:61`). This is the **pre-declared project-defined human-only
  sign-off item** the brief announces (Scope: "the maintainer's sign-off IS the accepting
  authority"); routing it here is expected, not a defect. Content matches the brief's
  settled decisions (SHA-256 opaque ETag, flat optional fields, repair-preserves split) —
  nothing in the ADR contradicts what the maintainer already approved at Plan.

- NEEDS-HUMAN — residual asymmetry in the panic-hardening the iteration-2 rejection
  demanded: the GET arm now degrades a malformed stored `content_type`
  (`crates/gateway-s3/src/lib.rs:642`, `content_type_header` at `:729`) but still passes
  the stored `etag` **unguarded** into the response builder
  (`crates/gateway-s3/src/lib.rs:650`, `quote_etag`), so a stored `etag` containing a
  non-header byte (e.g. CR/LF) panics every GET of that object at the same
  `.expect("streaming response is always valid")` (`:657`). Concrete case: an
  `InodeRecord` whose JSON `etag` field is `"x\r\ny"` — decode is liberal
  (ADR-0045 parse-don't-validate), so it reaches the builder. **Honest severity
  assessment:** reachability is materially LOWER than the content_type case that forced
  iteration 3 — `etag` is only ever *server-computed* lowercase hex
  (`crates/server/src/lib.rs:299,311`); no seam caller can inject it (the seam *returns*
  the etag, it does not accept one), so only store corruption / out-of-band edits hit
  this. Adjudicate: require the one-line symmetric fallback now, or defer to #506
  (HeadObject reads the same fields next). I deliberately do NOT mark this `[impl]` —
  the human scoped iteration 3 to exactly two items and an auto-rebuild for a
  corruption-only edge may not be worth the cycle.

- (non-blocking observation) `Last-Modified` **value** correctness is untested: the wire
  tests validate only IMF-fixdate *shape* (`crates/server/tests/s3_object_metadata.rs`,
  `is_imf_fixdate`) and both PUTs run at "now", so a wrong-but-well-formed date (e.g. a
  month-index slip in `crates/gateway-s3/src/lib.rs:739` `http_date` /
  `civil_from_days`) would pass every gate. I cross-checked the shipped algorithm
  against Python's datetime over 200,005 sampled epoch values including boundary dates:
  **zero mismatches** — the in-tree formatter is correct today; only the regression
  guard is absent. A unit test pinning e.g. `http_date(784_111_777_000) == "Sun, 06 Nov
  1994 08:49:37 GMT"` would close it; fine to fold into #506 rather than rebuild.

## Attempted refutations that FAILED (the fix survived)

- **Red→green evidence is genuine.** Reversed the patch (keeping only the shipped test
  file) in the scratch copy: both tests in `crates/server/tests/s3_object_metadata.rs`
  compile against the base and fail for the RIGHT reason — "a PutObject response must
  carry an ETag header … pre-fix it has none" (`s3_object_metadata.rs:225`, `:295`) —
  through the real loopback listener → sigv4 → redb + fs-tempdir stack, not a parallel
  re-implementation. Re-applied: 2/2 green. The ETag oracle is an independent SHA-256
  computed in-test (`s3_object_metadata.rs:263` `sha256_hex`), so an echo-anything wire
  layer cannot pass; not a tautology.
- **Carry-forward item 1 (Last-Modified overwrite freshness) really lands.** Planted the
  named mutant — `modified: meta.modified` → `modified: prior.modified` at
  `crates/core/src/metadata.rs:581` and `:641` — and both new unit tests failed
  (`crates/core/tests/mutation_regressions.rs:433`, `:510`); reverted, both pass. The
  leased variant covers the path the wire PUT actually drives
  (`commit_written` → `commit_overwrite` → superseding_leased, `crates/server/src/lib.rs:184`).
- **Carry-forward item 2 (content-type panic) really lands.** Reintroduced the
  pre-hardening raw pass-through at `crates/gateway-s3/src/lib.rs:642`: the new
  router-level test panicked at the production `.expect` (`lib.rs:657`) exactly as the
  rejection described; with the shipped `content_type_header` it degrades to
  `application/octet-stream` and still serves body + ETag. The test drives the real
  signed router dispatch, not a stand-in.
- **Repair-preservation tests (iteration-1 item, "must be kept") are present and green**
  and are non-vacuous (they seed a non-`None` trio): `mutation_regressions.rs`
  (`commit_chunk_map_preserves_object_metadata_across_a_repair`),
  `crates/custodian/tests/backfill.rs`, `rebalance.rs`, `reconstruction.rs` — 8 + 5 +
  10 + 15 tests re-run green in the scratch copy.
- **No new dependency smuggled in:** zero `Cargo.toml` hunks in the patch; the HTTP-date
  formatter is in-tree as the brief directed, and `sha2`/`tower`/`tempfile` were already
  workspace deps of the touched crates.
- Also probed and could not break: overwrite-via-plain-`ObjectMeta::default()` paths
  (CLI writes commit all-`None` metadata → wire degrades to pre-metadata behaviour, per
  design, and a CLI overwrite clearing a stale S3 ETag is the *correct* direction);
  empty-body PUT (digest of empty content still a valid ETag); the unsigned
  `content-type` header ride-along (axum's `HeaderValue` already bars CR/LF from the
  wire, so the verbatim-commit path cannot be poisoned by an HTTP client).

**Verdict:** could not refute the fix or the gate evidence; both prior rejection items
are demonstrably closed (mutants killed). The two NEEDS-HUMAN bullets above are the
pre-declared ADR acceptance and one low-reachability symmetry residue for adjudication.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the otherwise green red→green, workspace tests, fmt/clippy/build, and wire coverage despite `cargo deny check` being unreproducible because its advisory DB lock is read-only; this matters because the complete asserted CI gate was not independently discharged (`crates/server/tests/s3_object_metadata.rs:205`).
- [ ] T5 Judgment — Maintainer must accept the new ADR's flat optional record model and opaque SHA-256 ETag because Accepted ADRs are project-defined human-only decisions with downstream compatibility impact (`docs/design/adr/0047-object-metadata-model.md:29`).
- [ ] Validation — fitness-to-purpose — Decide whether opaque SHA-256 ETags and second-granularity Last-Modified behavior meet real client expectations; this product-compatibility judgment is not settled by the passing in-process wire oracle (`docs/design/adr/0047-object-metadata-model.md:67`).
- [ ] ADR-0047 ships in-slice with frontmatter `status: Accepted`
- [ ] residual asymmetry in the panic-hardening the iteration-2 rejection

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
- Iteration delta (if iterating): Rejected solely on the adversary's one genuinely new finding — the residual panic-hardening asymmetry; the implementation and design stand, do not redesign. Fix exactly this: the GET arm degrades a malformed stored `content_type` (crates/gateway-s3/src/lib.rs:642, `content_type_header` at :729) but still passes the stored `etag` UNGUARDED into the response builder (crates/gateway-s3/src/lib.rs:650, `quote_etag`), so a stored etag containing a non-header byte (e.g. CR/LF — reachable via store corruption / out-of-band edits, since decode is liberal per ADR-0045) panics every GET of that object at the `.expect("streaming response is always valid")` (:657). Apply the symmetric fallback: when the quoted etag is not a valid header value, omit the ETag header (degrade, never panic), and add a router-level test mirroring the malformed-content_type one (seed a stored etag with a CR/LF byte, assert 200 + body served without panic). Keep all existing tests — the iteration-1/2 carry-forward tests are verified non-vacuous (mutants killed) and must remain. Optional, non-blocking: a unit test pinning an http_date value (e.g. http_date(784_111_777_000) == "Sun, 06 Nov 1994 08:49:37 GMT") may ride along or fold into #506. Context already settled by the human (do not revisit): T5/ADR-0047 decisions (SHA-256 opaque ETag, flat optional record, status Accepted in-slice) approved; T4 prior-art cleared; the reviewer's `cargo deny` scratch-clone advisory-DB lock is an environment issue, not rebuild scope.
- By / date: Eduard Ralph / 2026-07-18

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
