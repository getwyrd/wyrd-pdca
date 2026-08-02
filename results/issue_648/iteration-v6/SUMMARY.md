# Result — issue 648 / chunkmap-flat-segmented-record-shape

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  `InodeRecord.chunk_map` is a bare inline `Vec<ChunkRef>`
  (`crates/core/src/metadata.rs:268` on `origin/main`), so an object's whole chunk list must fit
  one metadata value — 100 KB on the tightest backend
  (`crates/metadata-fdb/tests/contention.rs:142`), a hard object-size ceiling far below the
  >10 GiB launch requirement. The `ChunkMap::Flat | Segmented` shape, the `seg:` / `seggrp:`
  record classes and their key helpers do not exist in the tree at all
  (`git -C ../wyrd grep -n "enum ChunkMap\|SegmentRecord\|SEG_PREFIX" origin/main -- crates/` is
  empty). This slice lands **only** the shape, its decode-time invariants and its key helpers.
- Success criterion:
  The added test target `crates/core/tests/segmented_map_record.rs` passes
  and binds the issue's acceptance, all through **base-visible** API (`metadata::{encode, decode,
  InodeRecord}`) over raw stored bytes:
  1. **Legacy round-trips byte-identically, and CAS still commits.** For a hand-authored legacy
     `inode:` value in exactly the shape `origin/main` emits (JSON-array `chunk_map`;
     `etag`/`content_type`/`modified` absent), `encode(&decode::<InodeRecord>(bytes)?)` equals
     `bytes` byte-for-byte, and a `require(key, encode(prior))` CAS over that pre-existing record
     **commits** against a store holding the original bytes.
  2. **A well-formed segmented root decodes**, and **each decode invariant has its raw-byte
     negative case** that is `Err`: `segment_count != segments.len()`, duplicate index, index
     gap, non-monotonic/overlapping byte spans (contiguous tiling spanning exactly `size`),
     non-32-hex nonce.
  3. **A segmented root stays inside the value ceiling** — a root holding `MAX_ROOT_SEGMENTS`
     segments encodes to ≤ 100 000 bytes, asserted on `encode(...).len()`, i.e. measured in
     encoded bytes.

  The two invariants that cannot be reached without patch-added symbols — a **wrong-width `seg:`
  key index** and a `SegmentRecord` whose chunk lengths do not sum (checked, not wrapping) to its
  declared span — ship as raw-byte negative cases in the **co-located** `metadata.rs` tests, which
  `C4-ci` runs. Supplementary, not binding: `cargo xtask ci` green including the prose gates.
- Repo + branch target:
  getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- Scope (one logical fix) / out of scope:
  the segmented **record shape and its codec**, and nothing that reads or writes one.
  `crates/core/src/metadata.rs` — the `ChunkMap` two-variant value, `SegmentedMap` /
  `SegmentGroup` / `SegmentRef`, `SegmentRecord`, the `ChunkMapError` variants **this slice's
  invariants raise**, the `seg:` / `seggrp:` key helpers and their parser (fixed-width
  zero-padded index, so byte-lexicographic order equals index order), the capacity constants, and
  an encode/decode path that leaves a flat record byte-identical; plus the mechanical migration
  below. **Caller-first:** this slice lands no behaviour flip and no producer — nothing publishes
  a segmented map until #653 → #658; every existing `.chunk_map` site must therefore treat the
  `Segmented` variant as a typed error, not an empty list. **Out of scope:** the resolver and its
  consumers (#649–#651), the chunk-id floor (#652), the staged-publication committer (#653), any
  new/edited ADR / spec / proposal (0016 §(a) names this an ADR-graduation candidate — that is the
  architecture board's, INTEGRATION §2/§4), and any conformance-vector change.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: introduce a byte-compatible flat/segmented `InodeRecord` chunk-map shape, strict decode invariants and key helpers, while interim consumers fail closed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives falsifiable byte-identity, decode-invariant, encoded-ceiling, dependency, scope, and staged-production criteria, matching the persisted-shape contract documented at `docs/design/architecture/08-crosscutting-concepts.md:85`. |
| C2 Reproduction (red pre-fix) | PASS | An independent base-plus-added-test run compiled and produced the required assertion red—10 passed and 4 failed—at the segmented positive/capacity cases in `crates/core/tests/segmented_map_record.rs:120` and `crates/core/tests/segmented_map_record.rs:476`. |
| C3 Change | NEEDS-HUMAN | Decide whether approximately 1,640 nonblank, non-comment, nonmechanical additions are within the brief's `≤ ~1,500` budget—the overage is concentrated in the shape/tests beginning at `crates/core/src/metadata.rs:243` and `crates/core/tests/segmented_map_record.rs:1`, and the brief makes an over-budget result a Plan re-entry. |
| C4 Verification (red→green) | NEEDS-HUMAN | Re-run or accept CI on a host with a writable Cargo advisory database—the independent red→14/14 green transition and every preceding `cargo xtask ci` stage passed, but `cargo deny check` could not lock the sandbox-read-only advisory DB, so the supplied full-gate green remains provisional (`crates/core/tests/segmented_map_record.rs:60`). |
| C5 Causal adequacy | PASS | The 127-mutant rerun reproduced three survivors, all equivalent deletions of an explicit `size` immediately inherited unchanged through `..clone()` at `crates/custodian/src/backfill.rs:133`, `crates/custodian/src/rebalance.rs:301`, and `crates/custodian/src/reconstruction.rs:589`; no symptom-guard smell or semantic test gap remains. |
| T1 Structure | PASS | Record types, validation, codec, and key grammar are cohesively colocated from `crates/core/src/metadata.rs:243`, while the persisted-shape architecture is updated at `docs/design/architecture/08-crosscutting-concepts.md:85`. |
| T2 Shape | PASS | JSON-type discrimination preserves the legacy array, constructor-routed validation rejects malformed roots/segment records, and strict key parsing is centralized at `crates/core/src/metadata.rs:670`, `crates/core/src/metadata.rs:863`, `crates/core/src/metadata.rs:918`, and `crates/core/src/metadata.rs:1096`. |
| T3 Runtime | PASS | Existing production writes remain flat and segmented inputs fail closed before destructive work; the real GC pass proves no fragment is reclaimed at `crates/custodian/tests/gc.rs:806`, and the read path returns the typed refusal at `crates/core/src/read.rs:91`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the one undisclosed batch-review blocker is release-relevant—the target lacks both `scripts/review-branch` and `scripts/pdca`, and no finding detail is supplied, so the scanner red and contribution green cannot be independently triaged or reproduced; the affected-path prior-art query itself confirmed closed-unmerged #647 as the only earlier segmentation work. |
| T5 Judgment | PASS | The evidence exercises stored bytes through the production codec and a real CAS store, while the capacity proof measures re-encoded roots and a conservative all-table upper bound at `crates/core/tests/segmented_map_record.rs:454` and `crates/core/tests/segmented_map_record.rs:475`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether landing the intentionally decode-only segmented half before the #649 resolver and #653 publisher is fit for staged delivery—it protects current data and bounds the root, but this slice cannot itself prove the eventual >10 GiB end-to-end path (`docs/design/architecture/08-crosscutting-concepts.md:85`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether approximately 1,640 nonblank, non-comment, nonmechanical additions are within the brief's `≤ ~1,500` budget—the overage is concentrated in the shape/tests beginning at `crates/core/src/metadata.rs:243` and `crates/core/tests/segmented_map_record.rs:1`, and the brief makes an over-budget result a Plan re-entry.
- [ ] C4 Verification (red→green) — Re-run or accept CI on a host with a writable Cargo advisory database—the independent red→14/14 green transition and every preceding `cargo xtask ci` stage passed, but `cargo deny check` could not lock the sandbox-read-only advisory DB, so the supplied full-gate green remains provisional (`crates/core/tests/segmented_map_record.rs:60`).
- [ ] T4 Contribution — Decide whether the one undisclosed batch-review blocker is release-relevant—the target lacks both `scripts/review-branch` and `scripts/pdca`, and no finding detail is supplied, so the scanner red and contribution green cannot be independently triaged or reproduced; the affected-path prior-art query itself confirmed closed-unmerged #647 as the only earlier segmentation work.
- [ ] Validation — fitness-to-purpose — Decide whether landing the intentionally decode-only segmented half before the #649 resolver and #653 publisher is fit for staged delivery—it protects current data and bounds the root, but this slice cannot itself prove the eventual >10 GiB end-to-end path (`docs/design/architecture/08-crosscutting-concepts.md:85`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] T5 Judgment — Confirm that closed-unmerged [PR #647](https://github.com/getwyrd/wyrd/pull/647) was abandoned only for reviewability—the affected-path search found it as the sole segmentation prior art, but its GitHub body/comments record no closure rationale, so reusing its shape depends on an unrecorded disposition.
- [ ] external dependency: the whole slice builds and is exercised with the base

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
- Iteration delta (if iterating): Rejecting on the T4 batched-review TEST-GAP finding: add raw-byte decode test coverage for empty segmented maps (NoSegments) and zero-byte SegmentRefs (EmptySegment), per crates/core/tests/segmented_map_record.rs:111, so removing those guards would be caught. Other §6 items (C3 budget overage, C4 deny-check environment gap, T4 contribution tooling visibility, staged-delivery fitness, T5 PR #647 provenance) are not blocking — leave those as-is / not needing rework this round.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
