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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 120 mutants tested in 3m: 24 missed, 20 caught, 76 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: introduce the byte-compatible flat/segmented chunk-map record shape, decode invariants, and segment-key helpers for issue #648, without yet adding segmented producers or resolvers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The slice boundary is decision-ready: durable shape/codec and fail-closed existing consumers now, producer/resolver later; the target states that boundary at crates/core/src/metadata.rs:260 and docs/design/architecture/08-crosscutting-concepts.md:85. |
| C2 Reproduction (red pre-fix) | PASS | Against base plus the added test, 9/11 checks passed and the two positive segmented assertions failed at crates/core/tests/segmented_map_record.rs:108 and crates/core/tests/segmented_map_record.rs:216, establishing an assertion red rather than a compile red. |
| C3 Change | PASS | The patch stays within the approved shape/codec, architecture paragraph, and mechanical `.into()`/`.as_flat()` migration; no segmented producer or resolver was introduced (crates/core/src/metadata.rs:672, crates/core/src/write.rs:274). |
| C4 Verification (red→green) | PASS | The same acceptance target flipped to 11/11 green, both co-located invariant tests passed at crates/core/src/metadata.rs:1780 and crates/core/src/metadata.rs:1795, and every `xtask ci` component—including real `typos` and docs render/link audit—passed after relocating cargo-deny's read-only global cache into reviewer scratch. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Bind the capacity test to the production constants in a patch-aware test—the current test hard-codes 512 and 100,000 at crates/core/tests/segmented_map_record.rs:200 and crates/core/tests/segmented_map_record.rs:222 while the values under judgment live at crates/core/src/metadata.rs:295 and crates/core/src/metadata.rs:300, so a ceiling drift can pass without proving the real `MAX_ROOT_SEGMENTS` shape fits. |
| T1 Structure | PASS | The semantic shape is co-located in metadata, the base-visible acceptance is a new integration target, and the persisted-field documentation landed with it (crates/core/src/metadata.rs:243, crates/core/tests/segmented_map_record.rs:1, docs/design/architecture/08-crosscutting-concepts.md:85). |
| T2 Shape | PASS | JSON type preserves the legacy array while the segmented object and private validating constructors reject malformed durable structure at decode (crates/core/src/metadata.rs:653, crates/core/src/metadata.rs:731, crates/core/src/metadata.rs:740). |
| T3 Runtime | PASS | Live writers remain flat and existing readers/maintenance paths return the typed unsupported-shape error rather than an empty ownership set (crates/core/src/write.rs:274, crates/core/src/read.rs:94, crates/custodian/src/gc.rs:263). |
| T4 Contribution | NEEDS-HUMAN | Decide the disposition of the asserted eight `scripts/review-branch --bundle` blockers—the tool and its finding output are absent from both the supplied artifacts and target, so the red row is provisional and could conceal release-relevant defects even though the independently runnable contribution/build checks pass. |
| T5 Judgment | NEEDS-HUMAN | Confirm that closed-unmerged [PR #647](https://github.com/getwyrd/wyrd/pull/647) was abandoned only for reviewability—the affected-path search found it as the sole segmentation prior art, but its GitHub body/comments record no closure rationale, so reusing its shape depends on an unrecorded disposition. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether landing the durable segmented-record ABI before any producer/resolver is fit for rollout—the flat path and shape are verified, but real greater-than-10-GiB publication/read remains intentionally deferred, so this slice proves representation rather than launch behavior (docs/design/architecture/08-crosscutting-concepts.md:85). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Bind the capacity test to the production constants in a patch-aware test—the current test hard-codes 512 and 100,000 at crates/core/tests/segmented_map_record.rs:200 and crates/core/tests/segmented_map_record.rs:222 while the values under judgment live at crates/core/src/metadata.rs:295 and crates/core/src/metadata.rs:300, so a ceiling drift can pass without proving the real `MAX_ROOT_SEGMENTS` shape fits.
- [ ] T4 Contribution — Decide the disposition of the asserted eight `scripts/review-branch --bundle` blockers—the tool and its finding output are absent from both the supplied artifacts and target, so the red row is provisional and could conceal release-relevant defects even though the independently runnable contribution/build checks pass.
- [ ] T5 Judgment — Confirm that closed-unmerged [PR #647](https://github.com/getwyrd/wyrd/pull/647) was abandoned only for reviewability—the affected-path search found it as the sole segmentation prior art, but its GitHub body/comments record no closure rationale, so reusing its shape depends on an unrecorded disposition.
- [ ] Validation — fitness-to-purpose — Decide whether landing the durable segmented-record ABI before any producer/resolver is fit for rollout—the flat path and shape are verified, but real greater-than-10-GiB publication/read remains intentionally deferred, so this slice proves representation rather than launch behavior (docs/design/architecture/08-crosscutting-concepts.md:85).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Bind the capacity test to the production constants in a patch-aware test—the current test hard-codes 512 and 100,000 at crates/core/tests/segmented_map_record.rs:200 and crates/core/tests/segmented_map_record.rs:222 while the values under judgment live at crates/core/src/metadata.rs:295 and crates/core/src/metadata.rs:300, so a ceiling drift can pass without proving the real `MAX_ROOT_SEGMENTS` shape fits.; T4 Contribution — Decide the disposition of the asserted eight `scripts/review-branch --bundle` blockers—the tool and its finding output are absent from both the supplied artifacts and target, so the red row is provisional and could conceal release-relevant defects even though the independently runnable contribution/build checks pass.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
