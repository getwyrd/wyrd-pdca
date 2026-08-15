# Result — issue 691 / multipart-key-grammar

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: the NEW file `crates/core/tests/multipart_keys.rs` passes. Every
  leg is pure — no store, no async, no fixture beyond literals:
  1. **Round-trip + canonical rejection, per keyed class.** For each of `mpu:<id>`,
     `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`, `sidx:<id>:<part>:<chunk>`,
     `retire:bytes:<token>`, `retire:records:<token>`: `parse(key(x)) == x` over a table,
     AND each parser rejects with a typed error: a `+` sign, **every short width including
     the bare `7` and the padded-but-short `007`** (the carried-forward MUST-FIX the v2
     review found missing — `multipart_records.rs:162` in the archived attempt), an
     over-wide spelling, a non-decimal body, an empty upload id, an upload id containing
     `:`, a malformed/truncated key, a trailing component, and non-UTF-8 bytes. Two
     spellings of one record must never both parse.
  2. **Byte-lexicographic order equals numeric order** over a fixed-width series that
     crosses every digit-width boundary (1, 9, 10, 99, 100, …, max), for both the slot
     index and the part number — exactly the property `metadata.rs:270-273` states for
     `SEG_INDEX_WIDTH`.
  3. **No prefix is a prefix of another**, over the full set: the seven above plus the
     pre-existing `inode:`, `dirent:`, `pending:`, `bucket:`, `orphan:`
     (`metadata.rs:30-70`), `seg:`, `seggrp:` (`metadata.rs:293-300`) and
     `desired:dserver:` (`custodian/src/desired_state.rs:33`). The two named near-misses:
     `scan("mpu:")` must not return the `mpuctl` singleton, and `sidx:` must not be
     reachable from `scan("pending:")` (`0016:475-491`).
  4. **The `retire:` token grammar** distinguishes its two token forms (session-scoped vs
     generation-scoped, `0016:359-380`) at parse, round-trips both, and rejects a token of
     neither form.
  5. **The pinned format constants hold:** `PART_NUMBER_WIDTH == 6`,
     `MAX_PART_NUMBER == 999_999`, `SLOT_INDEX_WIDTH == 6`, `MAX_SLOT_INDEX == 999_999`,
     and each parser's width equals its constant.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: no live milestone
  integration branch; verified `git -C ../wyrd rev-parse origin/main` → `339da46`)
- Scope (one logical fix) / out of scope: ONE new module `crates/core/src/multipart.rs` (flat sibling of `metadata.rs`;
  the workspace has no directory modules) + one `pub mod multipart;` line in
  `crates/core/src/lib.rs`: `RecordError` (typed, `Display`, `Error`), the validated
  newtypes `UploadId` / `AttemptId` (token grammar via `TOKEN_HEX_LEN` =
  `metadata::SEG_NONCE_HEX_LEN`, `is_token` / `require_token`), `PartNumber` /
  `SlotIndex` (validating constructors + `Deserialize` that routes through them),
  `Digest` ([u8; 32], lowercase-hex `Serialize`/`Deserialize`, `hex_lower`), the prefix
  constants (`MPUCTL_KEY`, `MPU_PREFIX`, `SLOT_PREFIX`, `PART_PREFIX`, `PSUM_PREFIX`,
  `SIDX_PREFIX`, `RETIRE_BYTES_PREFIX`, `RETIRE_RECORDS_PREFIX`), `fixed_width_u32` /
  `canonical_decimal` / `split_key`, the key constructors + `*_range` prefixes + `parse_*`
  parsers for all seven keyed classes, `RetireMode` / `RetireToken` /
  `parse_retire_mode` / `parse_retire_key` / `retire_key` / `retire_session_range`.
  **Plan decisions pinned here, not Do's to revisit:** `PART_NUMBER_WIDTH = 6`
  (protocol-neutral format headroom — S3 caps at 10,000 per its wire protocol, Azure block
  blobs at 50,000 committed / 100,000 staged; the gateway seam is protocol-neutral by
  ADR-0046, so the *format* must clear every known protocol ceiling with margin while each
  protocol's cap is enforced at admission as *capacity*, mirroring `SEG_INDEX_WIDTH` vs
  `MAX_ROOT_SEGMENTS`, `metadata.rs:270-321`); `SLOT_INDEX_WIDTH = 6` (0016's clamp
  arithmetic reaches ≈524,288, `0016:1471`). Doc-comment both with this reasoning.
  / out of scope: every record **value** type, `encode_record` / `decode_record` and all
  validating value decoders (child-2's); the outcome enums, answer table and digests
  (child-3's — `Digest` the *type* is here, `sha2` the *dependency* is child-3's, so no
  `Cargo.toml` change in this child); every store round trip (#656–#659); the knob values
  (#655); S3 verbs/XML/status (#508); `crates/core/src/metadata.rs` untouched; no file
  beyond the three named.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 134 mutants tested in 2m: 50 caught, 84 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add Wyrd core's canonical multipart key grammar and validated identity types, including strict retirement-key decoding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Proposal 0016 fixes the namespaces, retirement-token forms, strict decode boundary, and disjointness needed to judge this slice without an unresolved design choice (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:333`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:358`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`). |
| C2 Reproduction (red pre-fix) | N/A | This is born-at-tier functionality with no pre-existing runtime symptom; stashing the patch reproduces criterion absence as `no test target named multipart_keys` rather than a behavioral failing test. |
| C3 Change | PASS | The required decision surface is complete: validated identities, pinned widths, disjoint key prefixes, canonical parsers/ranges, and both retirement-token forms all fail closed at decode (`crates/core/src/multipart.rs:164`, `crates/core/src/multipart.rs:255`, `crates/core/src/multipart.rs:471`, `crates/core/src/multipart.rs:822`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Human must accept criterion absence plus the two mandated negations as the born-at-tier red oracle — `engine/scripts/run-verify.sh` is unavailable and the base has no test target, while the patch passes all 21 focused tests and full `cargo xtask ci`, and tests catch both short `007` and separator-free `mpu` negations (`crates/core/tests/multipart_keys.rs:70`, `crates/core/tests/multipart_keys.rs:630`). |
| C5 Causal adequacy | PASS | Whole-key decoding now owns retirement-mode classification, so truncated and non-UTF-8 known-mode keys cannot pass a prefix-only guard; both negations fail and the independent 134-mutant run has 50 caught and 84 unviable with no survivor (`crates/core/src/multipart.rs:732`, `crates/core/tests/multipart_keys.rs:775`). |
| T1 Structure | PASS | The feature is a flat core-module sibling exported once and exercised by the required integration-test target, matching repository structure (`crates/core/src/lib.rs:13`, `crates/core/src/multipart.rs:1`, `crates/core/tests/multipart_keys.rs:1`). |
| T2 Shape | PASS | The exact three-file surface contains 1,144 non-blank, non-comment added lines, staying within the 1,150 semantic-line budget without a dependency or metadata-module change (`crates/core/src/lib.rs:13`, `crates/core/src/multipart.rs:1`, `crates/core/tests/multipart_keys.rs:1`). |
| T3 Runtime | N/A | This slice is intentionally pure and has no store I/O, async path, runtime topology, or production consumer to exercise (`crates/core/src/multipart.rs:5`, `crates/core/src/multipart.rs:55`). |
| T4 Contribution | NEEDS-HUMAN | Human must accept the supplied contribution and batched-review passes without their reports/tools — independent affected-path checks found no match in merged history or all 291 open/closed/merged PRs, but `scripts/review-branch` and `scripts/pdca` are unavailable, so those exact gates cannot be reproduced. |
| T5 Judgment | PASS | No remaining implementation-level judgment gap surfaced: strict malformed-key, canonicality, identity, ordering, range-isolation, and retirement-mode claims are asserted by value and survived mutation testing (`crates/core/tests/multipart_keys.rs:291`, `crates/core/tests/multipart_keys.rs:417`, `crates/core/tests/multipart_keys.rs:775`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this pure, currently unconsumed grammar is fit to become the durable key-space foundation for later multipart slices — any accepted alternate spelling or overlapping range would become a stored-format migration and reclamation/data-loss risk (`crates/core/src/multipart.rs:34`, `crates/core/src/multipart.rs:55`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Human must accept criterion absence plus the two mandated negations as the born-at-tier red oracle — `engine/scripts/run-verify.sh` is unavailable and the base has no test target, while the patch passes all 21 focused tests and full `cargo xtask ci`, and tests catch both short `007` and separator-free `mpu` negations (`crates/core/tests/multipart_keys.rs:70`, `crates/core/tests/multipart_keys.rs:630`).
- [x] T4 Contribution — Human must accept the supplied contribution and batched-review passes without their reports/tools — independent affected-path checks found no match in merged history or all 291 open/closed/merged PRs, but `scripts/review-branch` and `scripts/pdca` are unavailable, so those exact gates cannot be reproduced.
- [x] Validation — fitness-to-purpose — Human must decide whether this pure, currently unconsumed grammar is fit to become the durable key-space foundation for later multipart slices — any accepted alternate spelling or overlapping range would become a stored-format migration and reclamation/data-loss risk (`crates/core/src/multipart.rs:34`, `crates/core/src/multipart.rs:55`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
