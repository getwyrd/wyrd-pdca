# Brief — issue 648 / chunkmap-flat-segmented-record-shape

> Slice **1 of 6** of the #635 re-slicing (0016 decision 7(a)). History and the closed PR #647
> are on the parent issue — https://github.com/getwyrd/wyrd/issues/635 — do not re-read it here.

- **Slug:** chunkmap-flat-segmented-record-shape
- **Defect:** `InodeRecord.chunk_map` is a bare inline `Vec<ChunkRef>`
  (`crates/core/src/metadata.rs:268` on `origin/main`), so an object's whole chunk list must fit
  one metadata value — 100 KB on the tightest backend
  (`crates/metadata-fdb/tests/contention.rs:142`), a hard object-size ceiling far below the
  >10 GiB launch requirement. The `ChunkMap::Flat | Segmented` shape, the `seg:` / `seggrp:`
  record classes and their key helpers do not exist in the tree at all
  (`git -C ../wyrd grep -n "enum ChunkMap\|SegmentRecord\|SEG_PREFIX" origin/main -- crates/` is
  empty). This slice lands **only** the shape, its decode-time invariants and its key helpers.
- **Success criterion:** The added test target `crates/core/tests/segmented_map_record.rs` passes
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on the C4 variant this
  project actually has (`engine/scripts/run-verify.sh` discriminates on an **added**
  `*/tests/*.rs`, `:92-93`; `--classify` dry-run on this file set confirms
  `ADDED_TEST crates/core/tests/segmented_map_record.rs`). The RED leg reverts
  `crates/core/src/metadata.rs`, keeps the test, and runs
  `cargo test -p wyrd-core --test segmented_map_record`. On `origin/main` `chunk_map` is a
  `Vec<ChunkRef>`, so a segmented root's raw bytes fail `decode::<InodeRecord>` — criterion (2)'s
  "decodes" assertion and criterion (3) both **fail as assertions**, and the file still compiles
  because it imports nothing this patch adds. Environment: plain Linux Rust workspace, no
  topology, no cfg gate on `crates/core/tests/*.rs`, **no dev-dependency added by this patch**
  (`wyrd-metadata-redb` and `wyrd-traits` are already `crates/core` dev-deps, used by
  `crates/core/tests/placement_record.rs:29-33`), so neither the vacuous `0 tests … ok` branch
  (`:383-389`, `:420-427`) nor a compile-red-scored-as-pass can occur.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **the representation of which durable bytes an object owns**:
  - **A stored record's meaning may not change under it.** Every metadata CAS is
    `require(key, encode(prior))` compared byte-for-byte against the stored value
    (`crates/core/src/metadata.rs:277-286`,`:559`,`:605`,`:665`), so an encoding that gains a tag,
    wrapper or `null` turns every overwrite, backfill, reconstruction and rebalance of every
    pre-existing object into a permanent `Conflict` — a state nothing exits. Decode→encode is the
    identity on every byte sequence this system already wrote.
  - **A malformed record is an error, never a value a consumer could half-resolve** (ADR-0045,
    parse-don't-validate). A half-decoded map under-reports the bytes an object owns.
  - **A consumer that meets a shape it cannot resolve fails closed for that object** — never "this
    object owns no chunks". An empty answer is indistinguishable from a zero-length object and is
    how a live object's fragments become unreferenced.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- **Depends on:** *(none — first slice of the chain)*
- **Conflicts with:** *(none — 649/651/652 also edit `crates/core/src/metadata.rs`, but each
  depends on this slice, so the wave order already separates them)*
- **Ordering note:** First of the serial chain 648 → 649 → 650 → 651 → 652, because every later
  slice consumes the `ChunkMap`/`SegmentRecord` types and `seg:` helpers landed here.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** the segmented **record shape and its codec**, and nothing that reads or writes one.
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
- **Budget:** ≤ ~1,500 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 15 files.
  Mechanical migration — **construction sites gaining `.into()` / `ChunkMap::Flat(..)` and read
  sites gaining `.as_flat()`**, 43 files across 6 crates on `origin/main` — is counted separately
  and allowed on top; declare it as that pattern. The salvage shape+codec region measures ~990
  semantic lines, so the headroom is for **pruned** tests. If mid-build the tree exceeds this,
  STOP and hand back a proposed split instead of finishing — an over-budget patch is
  iterate-to-Plan by default, not another Do round.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/core/src/metadata.rs` —
  `chunk_map: Vec<ChunkRef>` at `:268`, `commit_chunk_map` at `:540-544`, the CAS/byte-identity
  precedent (ADR-0047 `skip_serializing_if`) at `:277-286`. The gap is the absence of the
  segmented shape, not a runtime failure.
- **External dependencies:** `typos`, `docs-renderer` — registered doctor.checks rows (ids
  `typos`, `docs-renderer`), named because this slice ships a living-architecture paragraph and
  `cargo xtask ci`'s prose gates warn-and-skip when those tools are absent locally
  (INTEGRATION §3). Nothing else beyond the base Rust toolchain: no Docker, no protoc, no live
  backend, and no new dev-dependency.
- **Test file:** `crates/core/tests/segmented_map_record.rs` — a **NEW** file (this project's C4
  discriminator is an added `*/tests/*.rs`; an appended or co-located test degrades to the
  green-only branch, `run-verify.sh:392-402`). It MUST import only symbols visible on
  `origin/main` — `wyrd_core::metadata::{encode, decode, InodeRecord, ChunkRef, EcScheme,
  InodeState, inode_key}`, `wyrd_traits::WriteBatch`, `wyrd_metadata_redb` — and express every
  segmented case as **raw bytes** through `decode::<InodeRecord>`, so the red leg is an assertion
  failure and not a compile error. Co-located `metadata.rs` tests carry the two invariants that
  need patch-added symbols.
- **Verification posture:** default for criteria (2)–(3) — assertion-red pre-fix, green post-fix
  at Check. Criterion (1) is a **property of the changed codec that is trivially true on the
  base**, so it cannot flip: Do MUST instead record a *demonstrated* red in `build-notes.md` —
  with the patch applied, temporarily serialize `ChunkMap` as a tagged enum, show criterion (1)'s
  legacy-CAS assertion fail, revert. Everything claimed here is built and exercised at Check;
  nothing is deferred off-Check.
- **Production reach:** **Declared.** This is a record-shape slice of the same class as #654, so
  the seam-ahead rule applies: (a) the `Segmented` half is honoured only by hand-authored raw
  record bytes in the test file — the **live** path is the `Flat` half, byte-identical to today,
  which is criterion (1); (b) production wiring lands in **#653** (staged-publication committer)
  and its caller **#658**; (c) the double is load-bearing, not scaffolding — the decode invariants
  run on real stored-byte sequences and criterion (3) measures real encoded output.
  `SegmentRecord` and the `seg:` / `seggrp:` helpers have no production caller until #649–#653;
  they are in scope only because the issue's What names them, and they must not grow beyond what
  criteria (2)–(3) exercise.
- **Citations expected:** cite `path:line` on the target branch for every change. **Salvage —
  extract and adapt from `results/issue_648/sources/salvage.diff` (this bundle, permitted input);
  do not re-derive settled code.** It carries #635's `crates/core/src/metadata.rs`, the docs
  paragraph, and the 21 mechanical-ripple files. Your region of `metadata.rs` in that diff is the
  shape+codec block — constants, `ChunkMapError` (take **only** the variants your invariants
  raise; the resolver/publication variants belong to #649/#653), `SegmentGroup`/`SegmentRef`/
  `SegmentedMap`/`ChunkMap`/`SegmentRecord`, the `seg:`/`seggrp:` helpers, `InodeRecord` + its
  wire form, and `encode`/`decode`/the structural-fault helpers. Two peers to mirror, which Do MAY
  open: `origin/main:crates/core/src/metadata.rs:277-286` (the byte-identity/CAS rule already
  enforced for ADR-0047's optional fields — the segmented variants must obey the same rule, and
  the salvage does it by discriminating on JSON **type**, array vs object), and the salvage's
  `impl From<Vec<ChunkRef>> for ChunkMap`, which is what keeps the 43-file ripple one line per
  site. Normative design: `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
  decision 7(a) `:2314-2331` and the §1 record table `:350`,`:354`.
- **Docs-currency:** `docs/design/architecture/08-crosscutting-concepts.md` — **the record-shape
  paragraph, and only that one.** The resolver/containment paragraphs belong to #649–#651 and
  staged publication to #653; do not write them here.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs (`gh pr list --state all --limit 200`, matched on `files[].path`).
  `crates/core/src/metadata.rs` appears in 10 PRs: **#647 is the only one touching this concern,
  and it is CLOSED unmerged** (2026-07-30, head `enhancement/635-segmented-chunk-map`) — closed on
  **reviewability, not correctness**, which is why it is the salvage source and not a rejected
  design. The other nine are MERGED and unrelated to chunk-map shape (#609, #594, #565, #489,
  #448, #397, #361, …). No merged PR implements segmentation; none rejects it.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` implementation owns the network bound, not the caller; (ii) *retraction of
  already-published bytes* — rejected 4× in #638 on unchanged evidence; (iii) *"`Completed`
  releases its admission slot"* — withdrawn as unsatisfiable; a `Completed` tombstone **stays
  counted**; (iv) every settled decision named in the slice issue's body. Do MUST record each
  rejection in `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Bind the capacity test to the production constants in a patch-aware test—the current test hard-codes 512 and 100,000 at crates/core/tests/segmented_map_record.rs:200 and crates/core/tests/segmented_map_record.rs:222 while the values under judgment live at crates/core/src/metadata.rs:295 and crates/core/src/metadata.rs:300, so a ceiling drift can pass without proving the real `MAX_ROOT_SEGMENTS` shape fits.; T4 Contribution — Decide the disposition of the asserted eight `scripts/review-branch --bundle` blockers—the tool and its finding output are absent from both the supplied artifacts and target, so the red row is provisional and could conceal release-relevant defects even though the independently runnable contribution/build checks pass.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 120 mutants tested in 3m: 24 missed, 20 caught, 76 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the two asserted `review-branch --bundle` blockers are release-relevant—the scanner and its detailed findings are absent from both the supplied artifacts and target, so that gating red cannot be reproduced or triaged independently.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 129 mutants tested in 3m: 3 missed, 46 caught, 80 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the five reported review blockers or the asserted contribution pass affect release — `scripts/review-branch`, its finding details, and `scripts/pdca contribcheck` are absent from both the supplied artifacts and target, so neither result can be independently reproduced or triaged.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 129 mutants tested in 3m: 3 missed, 46 caught, 80 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the unreported review blocker affects release — `check-gates.json` asserts one `review-branch --bundle` blocker, but its finding detail and both named scanner/contribution scripts are absent, so that result cannot be independently triaged despite the reproducible gates passing.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the two undisclosed `review-branch --bundle` blockers or the unverified contribution artifact affect release—the named scanner/contribution scripts and blocker details are absent, so their reported results cannot be independently reproduced or triaged.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejecting on the T4 batched-review TEST-GAP finding: add raw-byte decode test coverage for empty segmented maps (NoSegments) and zero-byte SegmentRefs (EmptySegment), per crates/core/tests/segmented_map_record.rs:111, so removing those guards would be caught. Other §6 items (C3 budget overage, C4 deny-check environment gap, T4 contribution tooling visibility, staged-delivery fitness, T5 PR #647 provenance) are not blocking — leave those as-is / not needing rework this round.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
