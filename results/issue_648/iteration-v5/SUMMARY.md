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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review: add the backward-compatible flat/segmented chunk-map record shape, structural decode invariants, key helpers, and fail-closed caller migration needed to remove the inline metadata value-size ceiling.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance is bounded to the record codec, raw-byte invariants, capacity, and mechanical fail-closed migration, with producer/resolver behavior explicitly left to later slices; the target surface starts at `crates/core/src/metadata.rs:243`. |
| C2 Reproduction (red pre-fix) | PASS | The independent base run compiled and ran all 14 acceptance tests, then failed four segmented assertions—not compilation—including well-formed decode and capacity at `crates/core/tests/segmented_map_record.rs:119` and `crates/core/tests/segmented_map_record.rs:475`. |
| C3 Change | PASS | The non-mechanical diff stays within the declared shape/codec/test/docs slice: JSON-type variants begin at `crates/core/src/metadata.rs:804`, strict key parsing at `crates/core/src/metadata.rs:1093`, and the single living-architecture update at `docs/design/architecture/08-crosscutting-concepts.md:85`. |
| C4 Verification (red→green) | PASS | Independent GREEN was 14/14 acceptance plus 14/14 co-located invariant tests; typos, docs lint/render, fmt, clippy, build, workspace tests, dependency audits, conformance, statics, orchestrator coverage, and DST passed after redirecting cargo-deny's read-only home lock, grounding the acceptance at `crates/core/tests/segmented_map_record.rs:60`. |
| C5 Causal adequacy | PASS | The change replaces the inline-only representation and rejects malformed stored structure at construction/decode (`crates/core/src/metadata.rs:670`); the three reproduced mutation survivors are equivalent explicit-`size` deletions restored unchanged by each literal's `..prior.clone()` at `crates/custodian/src/backfill.rs:133`, `crates/custodian/src/rebalance.rs:301`, and `crates/custodian/src/reconstruction.rs:589`. |
| T1 Structure | PASS | The stored grammar remains in core metadata and dependent crates use its typed error without reversing dependency direction, as shown at `crates/core/src/read.rs:94` and `crates/custodian/src/gc.rs:263`. |
| T2 Shape | PASS | Flat maps remain bare arrays while segmented maps are objects through the paired serializer/visitor at `crates/core/src/metadata.rs:863` and `crates/core/src/metadata.rs:872`, preserving the legacy wire shape. |
| T3 Runtime | PASS | Current producers remain flat, while a segmented value encountered before resolver/publication support fails closed rather than becoming an empty chunk list at `crates/core/src/read.rs:94` and `crates/core/src/metadata.rs:1290`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the two undisclosed `review-branch --bundle` blockers or the unverified contribution artifact affect release—the named scanner/contribution scripts and blocker details are absent, so their reported results cannot be independently reproduced or triaged. |
| T5 Judgment | PASS | The shape-ahead seam is coherent and no contested guard is introduced: raw stored bytes exercise both legacy and segmented CAS behavior at `crates/core/tests/segmented_map_record.rs:73` and `crates/core/tests/segmented_map_record.rs:154`, and the affected-path prior-art record covers merged history plus the closed salvage work. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a shape-only, fail-closed first slice is fit to anchor the >10 GiB delivery chain before producer/resolver slices land—compatibility and capacity are proven here, but end-to-end segmented publication/read remains intentionally outside this patch (`docs/design/architecture/08-crosscutting-concepts.md:85`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the two undisclosed `review-branch --bundle` blockers or the unverified contribution artifact affect release—the named scanner/contribution scripts and blocker details are absent, so their reported results cannot be independently reproduced or triaged.
- [ ] Validation — fitness-to-purpose — Decide whether a shape-only, fail-closed first slice is fit to anchor the >10 GiB delivery chain before producer/resolver slices land—compatibility and capacity are proven here, but end-to-end segmented publication/read remains intentionally outside this patch (`docs/design/architecture/08-crosscutting-concepts.md:85`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the two undisclosed `review-branch --bundle` blockers or the unverified contribution artifact affect release—the named scanner/contribution scripts and blocker details are absent, so their reported results cannot be independently reproduced or triaged.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
