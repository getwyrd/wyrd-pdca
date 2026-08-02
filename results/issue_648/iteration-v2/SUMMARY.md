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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add the flat-or-segmented chunk-map record shape, decode-time invariants, canonical segment keys, and fail-closed caller migration without changing legacy stored bytes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable at the codec seam—legacy identity, malformed-record rejection, and the 100,000-byte root ceiling are directly observable without requiring the later resolver or producer (`crates/core/tests/segmented_map_record.rs:52`). |
| C2 Reproduction (red pre-fix) | PASS | Base plus only the added test compiled and ran 11 tests: 9 passed while the well-formed segmented decode and production-capacity assertions failed, grounding the missing behavior at `crates/core/tests/segmented_map_record.rs:112` and `crates/core/tests/segmented_map_record.rs:239`. |
| C3 Change | PASS | The patch remains within the record/codec, typed fail-closed migration, and one living-architecture paragraph; no resolver or segmented producer entered this slice (`crates/core/src/metadata.rs:260`). |
| C4 Verification (red→green) | PASS | The exact patch changed the two intended assertion reds to 11/11 green, and the independently rerun formatting, clippy, build, workspace tests, dependency walls, conformance, statics, docs, typos, and 50-seed DST checks passed; the read-only global advisory-cache fault was discharged with a writable scratch cache (`crates/core/tests/segmented_map_record.rs:112`). |
| C5 Causal adequacy | PASS | JSON-type discrimination removes the single-value representation cause while malformed segmented values are rejected during deserialization; no capability probe or symptom guard was added (`crates/core/src/metadata.rs:788`). |
| T1 Structure | PASS | The new durable-shape types, errors, codec, and key grammar are co-located at the metadata boundary, with downstream crates depending only on the existing core seam (`crates/core/src/metadata.rs:243`). |
| T2 Shape | PASS | Flat maps retain their bare-array encoding while segmented maps use a strict object shape and canonical fixed-width keys, preserving CAS identity and unambiguous ordering (`crates/core/src/metadata.rs:779`, `crates/core/src/metadata.rs:1007`). |
| T3 Runtime | PASS | Every current production consumer fails with a typed error on an unresolved segmented map instead of observing an empty object, preserving live-fragment ownership until the resolver lands (`crates/core/src/read.rs:94`, `crates/custodian/src/gc.rs:263`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the two asserted `review-branch --bundle` blockers are release-relevant—the scanner and its detailed findings are absent from both the supplied artifacts and target, so that gating red cannot be reproduced or triaged independently. |
| T5 Judgment | PASS | The fresh 129-mutant run's only three survivors delete explicit `size: prior.size` fields that `..prior.clone()` restores identically, so they are behavior-equivalent rather than untested correctness branches (`crates/custodian/src/backfill.rs:132`, `crates/custodian/src/rebalance.rs:300`, `crates/custodian/src/reconstruction.rs:588`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this record shape and its staged six-slice rollout are architecturally sufficient for the >10 GiB launch target—automation proves byte identity, invariants, and capacity, but not end-to-end product fitness before later resolver/publication slices land (`docs/design/architecture/08-crosscutting-concepts.md:85`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the two asserted `review-branch --bundle` blockers are release-relevant—the scanner and its detailed findings are absent from both the supplied artifacts and target, so that gating red cannot be reproduced or triaged independently.
- [ ] Validation — fitness-to-purpose — Decide whether this record shape and its staged six-slice rollout are architecturally sufficient for the >10 GiB launch target—automation proves byte identity, invariants, and capacity, but not end-to-end product fitness before later resolver/publication slices land (`docs/design/architecture/08-crosscutting-concepts.md:85`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the two asserted `review-branch --bundle` blockers are release-relevant—the scanner and its detailed findings are absent from both the supplied artifacts and target, so that gating red cannot be reproduced or triaged independently.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
