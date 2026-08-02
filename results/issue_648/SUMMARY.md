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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #648: add the flat/segmented chunk-map record shape, decode-time invariants, and segment-key helpers while preserving legacy bytes and keeping unsupported consumers fail-closed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The slice has measurable compatibility, malformed-input, capacity, and fail-closed boundaries, so the implementation scope is determinate at `crates/core/src/metadata.rs:240`. |
| C2 Reproduction (red pre-fix) | PASS | An isolated base-plus-test checkout compiled and failed 6 of 16 tests as assertions at the positive segmented-decode and capacity cases in `crates/core/tests/segmented_map_record.rs:119` and `crates/core/tests/segmented_map_record.rs:546`. |
| C3 Change | PASS | The patch stays on the declared data surface: the centralized shape/codec and required fail-closed migrations are paired with only the requested architecture update at `crates/core/src/metadata.rs:804` and `docs/design/architecture/08-crosscutting-concepts.md:85`. |
| C4 Verification (red→green) | PASS | The isolated full patch passes all 16 focused tests and all 42 core unit tests, and `cargo xtask ci` passes typos, docs lint/render, fmt, clippy, build, workspace tests, all dependency-wall checks, conformance, guards, and madsim DST; the binding cases begin at `crates/core/tests/segmented_map_record.rs:60`, `crates/core/tests/segmented_map_record.rs:119`, and `crates/core/tests/segmented_map_record.rs:546`. |
| C5 Causal adequacy | PASS | JSON-type discrimination removes the inline-only shape without changing flat bytes, while decode validation prevents half-resolvable maps from becoming values at `crates/core/src/metadata.rs:807` and `crates/core/src/metadata.rs:1240`; the three mutation survivors are behaviorally equivalent inherited-size deletions. |
| T1 Structure | PASS | One `ChunkMap` abstraction owns shape selection and consumers use its flat accessor rather than duplicating parsing, keeping the compatibility boundary centralized at `crates/core/src/metadata.rs:804` and `crates/core/src/read.rs:94`. |
| T2 Shape | PASS | Private validated segmented-map and segment-record internals plus strict wire structs make malformed states unrepresentable while retaining the bare-array flat form at `crates/core/src/metadata.rs:665`, `crates/core/src/metadata.rs:769`, and `crates/core/src/metadata.rs:911`. |
| T3 Runtime | PASS | With no segmented producer yet, publication and destructive consumers reject the unsupported shape before durable mutation, while the fully rerun suite preserves current flat behavior at `crates/core/src/metadata.rs:1290` and `crates/core/src/metadata.rs:1521`. |
| T4 Contribution | NEEDS-HUMAN | Accept the two reported contribution/scanner passes without independent reproduction — `scripts/review-branch`, `scripts/pdca`, and the detailed scanner output are absent from both the supplied artifacts and target, so release-relevant findings and artifact completeness cannot be re-triaged. |
| T5 Judgment | PASS | The only mutation survivors delete explicit size fields already inherited identically at `crates/custodian/src/backfill.rs:132`, `crates/custodian/src/rebalance.rs:300`, and `crates/custodian/src/reconstruction.rs:588`, and the affected-path prior-art check confirms no rejected competing implementation. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a shape-only first slice is fit to land — it cannot create or resolve segmented objects until the later chain, so it does not remove the live object-size ceiling by itself despite preserving current behavior at `crates/core/src/metadata.rs:1276` and `docs/design/architecture/08-crosscutting-concepts.md:85`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Accept the two reported contribution/scanner passes without independent reproduction — `scripts/review-branch`, `scripts/pdca`, and the detailed scanner output are absent from both the supplied artifacts and target, so release-relevant findings and artifact completeness cannot be re-triaged.
- [x] Validation — fitness-to-purpose — Decide whether a shape-only first slice is fit to land — it cannot create or resolve segmented objects until the later chain, so it does not remove the live object-size ceiling by itself despite preserving current behavior at `crates/core/src/metadata.rs:1276` and `docs/design/architecture/08-crosscutting-concepts.md:85`.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101 — rerun at sign-off: clean worktree off origin/main@9120f7a, patch.diff applied, `cargo test --workspace --exclude wyrd-dst` green (0 failed) end-to-end. Recorded as transient/flaky at Check-run time, not reproducible.
- [x] T5 Judgment — Confirm that closed-unmerged [PR #647](https://github.com/getwyrd/wyrd/pull/647) was abandoned only for reviewability—the affected-path search found it as the sole segmentation prior art, but its GitHub body/comments record no closure rationale, so reusing its shape depends on an unrecorded disposition.
- [x] external dependency: the whole slice builds and is exercised with the base — confirmed via the C4 rerun above (clean apply + build + test against origin/main@9120f7a).

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
