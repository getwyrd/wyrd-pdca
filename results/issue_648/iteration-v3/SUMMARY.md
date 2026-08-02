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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 129 mutants tested in 3m: 3 missed, 46 caught, 80 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #648: add the flat/segmented chunk-map record shape, decode invariants, key helpers, legacy wire/CAS compatibility, and fail-closed caller migration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The bounded shape-only slice has falsifiable acceptance for legacy identity/CAS, malformed raw bytes, and production-bound capacity at crates/core/tests/segmented_map_record.rs:53, crates/core/tests/segmented_map_record.rs:112, and crates/core/tests/segmented_map_record.rs:239. |
| C2 Reproduction (red pre-fix) | PASS | On base 9120f7a with only the added test, the target compiled and ran 11 tests, with the segmented positive case and production-capacity case failing at crates/core/tests/segmented_map_record.rs:115 and crates/core/tests/segmented_map_record.rs:223 while the nine base-valid cases passed. |
| C3 Change | PASS | The semantic change stays on the declared record/codec and fail-closed migration surfaces at crates/core/src/metadata.rs:753, crates/core/src/read.rs:94, and docs/design/architecture/08-crosscutting-concepts.md:85; approximately 1,463 non-comment added lines across 42 files fit the explicit semantic-plus-mechanical budget. |
| C4 Verification (red→green) | PASS | The same scratch checkout flipped from two pre-fix test failures to 11/11 acceptance and 12/12 co-located invariant tests green, and scratch-local `cargo xtask ci` completed all checks including real typos/docs render, deny, conformance, statics, and DST; the binding cases are at crates/core/tests/segmented_map_record.rs:53, crates/core/tests/segmented_map_record.rs:112, and crates/core/tests/segmented_map_record.rs:239. |
| C5 Causal adequacy | PASS | The representation change removes the inline-vector ceiling directly at crates/core/src/metadata.rs:753 with no capability-probe smell; the reproduced 3/129 missed mutants are equivalent because deleting each explicit `size` still inherits that same value at crates/custodian/src/backfill.rs:132, crates/custodian/src/rebalance.rs:300, and crates/custodian/src/reconstruction.rs:588. |
| T1 Structure | PASS | Private fields plus validating constructors keep malformed group, map, and segment-record states out of values at crates/core/src/metadata.rs:542, crates/core/src/metadata.rs:607, and crates/core/src/metadata.rs:855. |
| T2 Shape | PASS | Array/object dispatch preserves the legacy array encoding while object decode routes through structural validation, and the segment-key grammar is strict at crates/core/src/metadata.rs:800, crates/core/src/metadata.rs:809, and crates/core/src/metadata.rs:1028. |
| T3 Runtime | PASS | Real-redb legacy CAS, raw-byte negative cases, capacity measurement, workspace tests, and DST passed; unresolved segmented data fails closed on read and GC before being treated as empty at crates/core/src/read.rs:94 and crates/custodian/src/gc.rs:263. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the five reported review blockers or the asserted contribution pass affect release — `scripts/review-branch`, its finding details, and `scripts/pdca contribcheck` are absent from both the supplied artifacts and target, so neither result can be independently reproduced or triaged. |
| T5 Judgment | PASS | Deep rubric review found no remaining implementation concern, and the affected-path prior-art search found only #647 closed unmerged for PR size, with no merged segmentation implementation; the durable-write guard also refuses premature segmented publication at crates/core/src/metadata.rs:1221. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether to sign off this shape-only first slice ahead of its resolver and publisher — the build deliberately refuses segmented publication and consumption at crates/core/src/metadata.rs:1207 and crates/core/src/read.rs:94, so acceptance validates compatibility and invariants but does not yet deliver usable large-object segmentation. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the five reported review blockers or the asserted contribution pass affect release — `scripts/review-branch`, its finding details, and `scripts/pdca contribcheck` are absent from both the supplied artifacts and target, so neither result can be independently reproduced or triaged.
- [ ] Validation — fitness-to-purpose — Decide whether to sign off this shape-only first slice ahead of its resolver and publisher — the build deliberately refuses segmented publication and consumption at crates/core/src/metadata.rs:1207 and crates/core/src/read.rs:94, so acceptance validates compatibility and invariants but does not yet deliver usable large-object segmentation.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] T5 Judgment — Confirm that closed-unmerged [PR #647](https://github.com/getwyrd/wyrd/pull/647) was abandoned only for reviewability—the affected-path search found it as the sole segmentation prior art, but its GitHub body/comments record no closure rationale, so reusing its shape depends on an unrecorded disposition.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the five reported review blockers or the asserted contribution pass affect release — `scripts/review-branch`, its finding details, and `scripts/pdca contribcheck` are absent from both the supplied artifacts and target, so neither result can be independently reproduced or triaged.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
