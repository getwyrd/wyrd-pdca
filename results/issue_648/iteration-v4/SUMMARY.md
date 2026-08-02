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

Review of issue #648’s record-shape slice: add byte-compatible flat/segmented chunk maps, decode-time invariants, and segment-key helpers without yet adding a segmented producer or resolver.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is decidable across legacy byte identity/CAS, raw-byte malformed cases, key grammar, and the encoded 100,000-byte ceiling, with the producer/resolver explicitly excluded. |
| C2 Reproduction (red pre-fix) | PASS | The base-visible test target compiled against the unpatched base and failed four assertions, including the positive segmented decode and production-capacity checks now at `crates/core/tests/segmented_map_record.rs:113` and `crates/core/tests/segmented_map_record.rs:365`. |
| C3 Change | PASS | The change stays within the record-shape slice: the two-shape value begins at `crates/core/src/metadata.rs:816`, segment records at `crates/core/src/metadata.rs:912`, strict key helpers at `crates/core/src/metadata.rs:1049`, and existing consumers only gain the required fail-closed flat adapter. |
| C4 Verification (red→green) | PASS | The assertion-red base became 14/14 green, the 14 co-located invariant tests passed from `crates/core/src/metadata.rs:2008`, and a final literal `cargo xtask ci` run passed every prose, build, test, deny, conformance, scanner, and DST step. |
| C5 Causal adequacy | PASS | The patch replaces the inline-only representation instead of probing around it, while malformed structures are made unrepresentable through `SegmentedMap::new` and decode routing at `crates/core/src/metadata.rs:670` and `crates/core/src/metadata.rs:796`. |
| T1 Structure | PASS | The durable shape remains owned by core metadata and callers depend on its typed seam; for example maintenance rejects the unresolved variant through `crates/custodian/src/gc.rs:263` without adding a concrete-backend dependency. |
| T2 Shape | PASS | JSON-type dispatch preserves the legacy array while the object form, cross-field span check, and canonical key parser are centralized at `crates/core/src/metadata.rs:863`, `crates/core/src/metadata.rs:1227`, and `crates/core/src/metadata.rs:1096`. |
| T3 Runtime | PASS | Real redb CAS tests cover legacy and segmented stored bytes at `crates/core/tests/segmented_map_record.rs:67` and `crates/core/tests/segmented_map_record.rs:147`, and the worst-case production-sized root is encoded and measured at `crates/core/tests/segmented_map_record.rs:365`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the unreported review blocker affects release — `check-gates.json` asserts one `review-branch --bundle` blocker, but its finding detail and both named scanner/contribution scripts are absent, so that result cannot be independently triaged despite the reproducible gates passing. |
| T5 Judgment | PASS | No test gap remains behind the advisory mutation red: each survivor only deletes a redundant `size: prior.size` that the same struct update restores via `..prior.clone()` at `crates/custodian/src/backfill.rs:140`, `crates/custodian/src/rebalance.rs:308`, and `crates/custodian/src/reconstruction.rs:596`; affected-path history found no merged competing implementation. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the shape should ship ahead of live segmented flow — publication is intentionally rejected at `crates/core/src/metadata.rs:1290` and reads fail closed at `crates/core/src/read.rs:94` until the dependent producer/resolver slices land, so sign-off accepts a staged API rather than an end-to-end large-object path. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the unreported review blocker affects release — `check-gates.json` asserts one `review-branch --bundle` blocker, but its finding detail and both named scanner/contribution scripts are absent, so that result cannot be independently triaged despite the reproducible gates passing.
- [ ] Validation — fitness-to-purpose — Decide whether the shape should ship ahead of live segmented flow — publication is intentionally rejected at `crates/core/src/metadata.rs:1290` and reads fail closed at `crates/core/src/read.rs:94` until the dependent producer/resolver slices land, so sign-off accepts a staged API rather than an end-to-end large-object path.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] external dependency: the whole slice builds and is exercised with the base
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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the unreported review blocker affects release — `check-gates.json` asserts one `review-branch --bundle` blocker, but its finding detail and both named scanner/contribution scripts are absent, so that result cannot be independently triaged despite the reproducible gates passing.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
