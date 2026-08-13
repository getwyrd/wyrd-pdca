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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 133 mutants tested in 2m: 50 caught, 83 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review task: add Wyrd's canonical multipart key grammar and validated identity types for issue #691, with strict decode-time rejection and pure focused tests.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative key space, token grammar, and decode boundary provide a determinate oracle for this slice (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:333`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:358`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`). |
| C2 Reproduction (red pre-fix) | PASS | The stashed base independently exited 101 because `multipart_keys` did not exist, and each required injected fault independently turned its named applied-patch test red (`crates/core/tests/multipart_keys.rs:75`, `crates/core/tests/multipart_keys.rs:631`). |
| C3 Change | PASS | The requested durable-format boundary is complete without pulling later record values or store I/O into this slice, preserving strict validation where identities and keys enter the system (`crates/core/src/multipart.rs:34`, `crates/core/src/multipart.rs:467`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Human must accept criterion absence plus the two mandated negations as the born-at-tier red oracle — `engine/scripts/run-verify.sh` is absent, while the applied patch passed 21/21 focused tests and the complete `cargo xtask ci` gate (`crates/core/tests/multipart_keys.rs:20`). |
| C5 Causal adequacy | PASS | The evidence directly exercises canonical decode boundaries rather than adding a capability probe or symptom guard; 133 in-diff mutants yielded 50 caught, 83 unviable, and no survivors (`crates/core/src/multipart.rs:504`, `crates/core/src/multipart.rs:522`). |
| T1 Structure | PASS | A flat core-module sibling is the repository's established ownership boundary, and the crate root exposes exactly that module (`crates/core/src/lib.rs:13`, `crates/core/src/multipart.rs:1`). |
| T2 Shape | PASS | Scope remains reviewable at exactly three authorized files and a conservative 1,130 added nonblank/noncomment lines, below the 1,150-line budget (`crates/core/src/multipart.rs:1`, `crates/core/tests/multipart_keys.rs:1`). |
| T3 Runtime | N/A | This slice intentionally has no store call, async path, production consumer, or external topology, so there is no runtime behavior beyond the independently exercised pure functions (`crates/core/src/multipart.rs:5`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect or accept the two unprovided batch-review blockers and contribution packaging — `scripts/review-branch`, `scripts/pdca contribcheck`, their report, and contribution artifacts are absent, although merged and closed/rejected affected-path prior art was mechanically clear. |
| T5 Judgment | PASS | Independent parser-boundary review, strict-adversity tests, and the zero-survivor mutation campaign found no remaining implementation judgment defect (`crates/core/src/multipart.rs:535`, `crates/core/src/multipart.rs:805`, `crates/core/tests/multipart_keys.rs:417`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this grammar is fit to become the durable foundation for later multipart slices — a wrong canonical spelling or overlapping range would become a migration and reclamation risk once records are persisted (`crates/core/src/multipart.rs:43`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Human must accept criterion absence plus the two mandated negations as the born-at-tier red oracle — `engine/scripts/run-verify.sh` is absent, while the applied patch passed 21/21 focused tests and the complete `cargo xtask ci` gate (`crates/core/tests/multipart_keys.rs:20`).
- [ ] T4 Contribution — Human must inspect or accept the two unprovided batch-review blockers and contribution packaging — `scripts/review-branch`, `scripts/pdca contribcheck`, their report, and contribution artifacts are absent, although merged and closed/rejected affected-path prior art was mechanically clear.
- [ ] Validation — fitness-to-purpose — Human must decide whether this grammar is fit to become the durable foundation for later multipart slices — a wrong canonical spelling or overlapping range would become a migration and reclamation risk once records are persisted (`crates/core/src/multipart.rs:43`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Fix `parse_retire_mode` (crates/core/src/multipart.rs:712/729): it accepts truncated or non-UTF-8 keys such as `retire:bytes:` and `retire:bytes:\xff` on prefix match alone, contradicting its own doc-comment's fail-closed promise. Validate the token following the prefix rather than dispatching on the prefix; add the truncated and non-UTF-8 cases to the retire-grammar legs of crates/core/tests/multipart_keys.rs. Both T4 review passes raised this independently and it is the only blocking finding; `review-rejected.md` covers a different, older docs-currency finding and does NOT disposition this one. Nothing is broken end-to-end today — the full decode path (`parse_retire_key`) still rejects these downstream — but a caller that dispatches on `parse_retire_mode` alone (the store slices, #656-#659) would be misled, and this module is a durable format foundation, so the fix belongs here rather than in its consumers. The §6 size backstop's `iterate-plan` recommendation (2 rounds spent, threshold 2) was considered and is explicitly overridden: the finding is bug-shaped and localized to one function, not scope-shaped. Note for the next Check: this is the second iterate-do at the backstop threshold, so if the rebuild again returns fresh implementation-level parser defects elsewhere in the grammar, that is the signal the slicing — not the implementation — is the problem, and the next disposition should be `iterate-plan`. Procedural note: an identical `iterate-do` was decided for this bundle at 12:43 but the driver never recorded it (§9 empty, decision file left unconsumed); this decision is that one re-issued after review, not a new judgement. See issue_681 §10 for the driver defect. This decision was reaffirmed unchanged in the 2026-08-06 sign-off session after a §10 Act candidate was appended (skip/split driver behavior, unrelated to this disposition).
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- **Bug:** After `pdca split <id> --accept` marks a parent bundle's `CLOSE_MARKER=split`, `driver._close_class()` skips the Do+review leaves for the parent unconditionally on every subsequent pass — worth reviewing whether that's the intended terminal state for the parent or should be more visibly distinguished from an ordinary `iterate-plan` re-open, since a human re-driving the parent can mistake the skip for a driver defect (see issue_691 sign-off session, 2026-08-06).
