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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 134 mutants tested in 3m: 5 missed, 46 caught, 83 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #691: add the multipart protocol's canonical key grammar and validated identity types to `wyrd-core`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The settled contract gives a complete decision surface—disjoint namespaces, strict token forms, and decode-time canonicality—so implementation can be judged without inventing policy (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:333`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed the base exits 101 because no `multipart_keys` target exists; after restoration both binding negations fail the intended assertions, demonstrating that the born-at-tier criterion is non-vacuous (`crates/core/tests/multipart_keys.rs:57`). |
| C3 Change | PASS | The patch implements the scoped validated identities, disjoint constants, range constructors, and strict parsers without store I/O or an out-of-scope dependency change (`crates/core/src/multipart.rs:152`). |
| C4 Verification (red→green) | PASS | Independent reruns produced absence-red, 16/16 patched tests green, both named negations red, and every `cargo xtask ci` leg green; the initial `cargo-deny` global-lock error disappeared with its home moved to scratch and was a host caveat (`crates/core/tests/multipart_keys.rs:14`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must add a digit-bearing `Digest::from_hex` round trip and cover `AttemptId::as_str`—five independently reproduced survivors show the claimed validated-identity evidence can miss broken numeric-nibble decoding and a broken public accessor (`crates/core/tests/multipart_keys.rs:110`). |
| T1 Structure | PASS | The dependency direction and flat-module convention remain intact: one sibling module is exported from the existing crate root (`crates/core/src/lib.rs:13`). |
| T2 Shape | PASS | The change is confined to the three authorized files and approximately 952 nonblank, non-comment added lines, below the 1,150 semantic-line ceiling (`crates/core/src/multipart.rs:1`). |
| T3 Runtime | N/A | This slice deliberately has no production consumer, store access, async path, clock, service, or manual/visual runtime outcome to assess (`crates/core/src/multipart.rs:5`). |
| T4 Contribution | NEEDS-HUMAN | Human must determine whether the unavailable `scripts/review-branch --bundle` blocker is the same as C5—the supplied gate summary reports one blocker but omits its report/tool, so a distinct contribution finding cannot be ruled out; affected-path prior art itself was mechanically clear. |
| T5 Judgment | PASS | No additional architecture, scope, capability-probe, or external-dependency judgment remains beyond the concrete rebuild-routable C5 coverage defect (`crates/core/src/multipart.rs:474`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this grammar is fit to freeze as the stored-format seam—later slices will persist these widths and prefixes, so an overlooked spelling or overlap becomes a migration and reclamation risk (`crates/core/src/multipart.rs:247`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must add a digit-bearing `Digest::from_hex` round trip and cover `AttemptId::as_str`—five independently reproduced survivors show the claimed validated-identity evidence can miss broken numeric-nibble decoding and a broken public accessor (`crates/core/tests/multipart_keys.rs:110`).
- [ ] T4 Contribution — Human must determine whether the unavailable `scripts/review-branch --bundle` blocker is the same as C5—the supplied gate summary reports one blocker but omits its report/tool, so a distinct contribution finding cannot be ruled out; affected-path prior art itself was mechanically clear.
- [ ] Validation — fitness-to-purpose — Human must decide whether this grammar is fit to freeze as the stored-format seam—later slices will persist these widths and prefixes, so an overlooked spelling or overlap becomes a migration and reclamation risk (`crates/core/src/multipart.rs:247`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add a digit-bearing `Digest::from_hex` round trip and cover `AttemptId::as_str`—five independently reproduced survivors show the claimed validated-identity evidence can miss broken numeric-nibble decoding and a broken public accessor (`crates/core/tests/multipart_keys.rs:110`).; T4 Contribution — Human must determine whether the unavailable `scripts/review-branch --bundle` blocker is the same as C5—the supplied gate summary reports one blocker but omits its report/tool, so a distinct contribution finding cannot be ruled out; affected-path prior art itself was mechanically clear.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
