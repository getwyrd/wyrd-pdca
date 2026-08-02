# Brief — issue 649 / shared-segmented-map-resolver-and-read-paths

> Slice **2 of 6** of the #635 re-slicing (0016 decision 7(e)/(h)). History and the closed PR
> #647 are on the parent issue — https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** shared-segmented-map-resolver-and-read-paths
- **Defect:** After #648 a `chunk_map` can be `Segmented`, but **nothing can read one**. Every
  consumer still takes the inline list off the record (`crates/core/src/read.rs:92`,
  `crates/server/src/lib.rs:360`,`:446` on `origin/main`), so a segmented object is opaque to the
  whole-object read, the ranged read and every maintenance pass, and there is no shared
  resolution call at all (`git -C ../wyrd grep -n "resolve_chunk_map\|MapResolution" origin/main
  -- crates/` is empty) — so the way each consumer learns an object's chunks is about to be
  re-derived once per consumer, which 0016 decision 7(e) forbids.
- **Success criterion:** The two added test targets pass and bind the issue's acceptance, all
  through **base-visible** entry points (`wyrd_core::read::{read_object, read_path}` and
  `wyrd_gateway_core::ObjectGateway`) over directly-seeded raw `seg:` records:
  1. **Byte-identical reads.** A segmented object seeded as raw `seg:` records + a segmented root
     (never via a committer) reads back — whole-object and over a range that **spans a segment
     boundary** — byte-identical to the flat equivalent of the same payload.
  2. **The resolver is bounded.** Reading one object touches the root plus **only** the range
     `seg:<nonce>:<epoch>:` — a second group seeded in the store (different nonce, and the same
     nonce at a different epoch) is **never read**, asserted on an instrumented store double that
     records every key/prefix requested; and a root claiming more segments than the ceiling allows
     is **refused before any range read is performed**, asserted on the same double.
  3. **The resolver is total and never tears.** Both arms of the resolve-retry rule, asserted on
     the interleaving: root **moved on** (or gone) + a segment absent ⇒ the resolution is dropped
     as a concurrently-retired generation (the read restarts / answers no-such-key), never a torn
     half-map and never "this object owns no bytes"; root **unchanged** + a segment absent,
     undecodable, unnamed, or whose extents disagree with the root's table ⇒ **fail closed** with
     a typed error. Chunks are ordered by the **parsed index**, proved against a deliberately
     shuffling store double. A read driven from a stale (superseded) snapshot resolves against the
     **live** root.

  Also in the issue's acceptance and shipped here, but **not** a Check discriminator: the DST
  resolver-tear property, run by `cargo xtask ci` / `dst` (see *Verification posture*).
- **Falsifiability:** RED is an **assertion** red on base-visible symbols. `C4-verify` resets
  `../wyrd-verify` to this bundle's base — a wave>0 bundle gets
  `PDCA_VERIFY_BASE=origin/pdca-integration/main` (`src/pdca_harness/gates.py:371-389`), honoured
  ahead of the brief base (`run-verify.sh:186-192`), so the patch applies onto a tree already
  carrying #648. `--classify` dry-run on this file set confirms exactly two discriminators and no
  cfg-gated addition. The RED leg keeps both test files, reverts `crates/core/src/metadata.rs`,
  `crates/core/src/read.rs`, `crates/server/src/lib.rs` and removes the added
  `crates/custodian/src/resolve.rs`. On that tree the segmented root still **decodes** (#648's
  contribution, so the fixtures still build) but no read can resolve it — criteria (1)–(3) fail as
  assertions, and both files still compile because they import only base-visible symbols
  (`seg_key` / `SegmentRecord` come from #648, which is base for this slice; nothing from *this*
  patch). No dev-dependency is added. Plain Linux workspace, no topology, no cfg gate on the two
  files, so neither the vacuous `0 tests … ok` branch (`:383-389`,`:420-427`) nor a
  compile-red-scored-as-pass can occur.
  **Keep the DST property out of the discriminator set:** ship it by *modifying*
  `crates/dst/tests/custodian.rs` (`#![cfg(madsim)]`). A **new** `crates/dst/tests/*.rs` would
  join the added-test set and force `RUSTFLAGS=--cfg madsim` + 50 seeds onto the whole C4-verify
  invocation (`run-verify.sh:110-134`,`:347-366`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **how any process learns which durable bytes an object owns**:
  - **Resolution is total and single-sourced.** Exactly one way to turn a committed inode into its
    ordered chunk list, and every consumer goes through it. Two answers to "which bytes does this
    object own" is the condition under which one process protects a fragment another reclaims.
  - **An answer is never a quiet under-approximation.** A resolution that cannot complete is a
    typed error for **that object**, never an empty or partial list.
  - **Fail-closed is scoped to the object that failed** — one unreadable object must not end a
    read of any other; a store-wide refusal is the same availability loss by another route.
  - **The work a record can demand of a reader is bounded by the reader, not the record.** A root's
    own table may not set the budget spent on its behalf; a table past the ceiling is refused,
    unread.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- **Depends on:** 648
- **Conflicts with:** *(none — 651 and 652 also edit `crates/core/src/metadata.rs`, but both sit
  in later waves via the dependency chain)*
- **Ordering note:** Second of the serial chain — it consumes #648's `ChunkMap` / `SegmentRecord`
  types and `seg:` helpers and edits the same file, and #650 in turn calls this resolver.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** **the one shared resolver**, and the **read paths** routed through it — nothing that
  reclaims, repairs or publishes. `crates/core/src/metadata.rs` — the resolution result type, the
  bounded per-group range read with its ceiling check, the resolve-retry arbiter, the
  resolve-against-the-live-root entries, and the `ChunkMapError` variants they raise.
  `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` — the thin
  custodian-facing wrapper and the shared root-classification arm every `scan("inode:")` loop will
  use, **with its unit coverage in this slice**; its pass consumers are #650/#651.
  `crates/core/src/read.rs` — the byte-assembly entry takes an already-resolved chunk list; the
  placement-aware entries resolve first. `crates/server/src/lib.rs` — gateway whole-object /
  streaming and ranged reads route through the resolver; **move #647's co-located `#[cfg(test)]
  mod` gateway tests into the added integration file**, since co-located tests cannot earn a
  per-fix red here. **Caller-first:** every production symbol introduced here has a caller **in
  this slice** — the resolver is called by `read.rs` and the gateway, the wrapper by its own unit
  tests and (next wave) #650; this slice lands no behaviour flip and no producer of segmented
  maps. **Out of scope:** GC / scrub / `ReferenceSet` (#650); restore, reconstruction, backfill,
  rebalance, `desired_state`, `repoint_chunk` and the record-ceiling helpers (#651); the chunk-id
  floor (#652); the committer, fence, rollback and resume (#653); any new/edited ADR / spec /
  proposal; any conformance-vector change.
- **Budget:** ≤ ~1,500 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 15 files.
  Mechanical migration — **callsites of `read::read_object_from` gaining the resolved-chunk-list
  form (`read_object_chunks(chunks, &map, size)`)** across benches and existing test files — is
  counted separately and allowed on top; declare it as that pattern. Salvage production measures
  ~660 semantic lines (metadata.rs resolver ~320; read.rs + resolve.rs + custodian/lib.rs 278;
  server/src/lib.rs's **non-test** plumbing ~60 — its other ~440 added lines are the co-located
  test module being relocated); #647's full test bodies push this to ~1,900, so **prune the
  co-located resolver tests to the binding cases**. If mid-build the tree exceeds this, STOP and
  hand back a proposed split instead of finishing — an over-budget patch is iterate-to-Plan by
  default, not another Do round. The re-slicing's named fallback split is *read paths* out of
  *resolver*.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/core/src/read.rs` (`:92`, the
  inline-only assumption) and `.../crates/server/src/lib.rs` (`:360`,`:446`, the same assumption
  in the gateway). After #648, seed a segmented root plus its raw `seg:` records and read the
  object — it cannot be read.
- **External dependencies:** `typos`, `docs-renderer` — registered doctor.checks rows (ids
  `typos`, `docs-renderer`), named because this slice edits a living-architecture paragraph and
  `cargo xtask ci`'s prose gates warn-and-skip when those tools are absent locally
  (INTEGRATION §3). Nothing else beyond the base Rust toolchain — the DST property runs under the
  workspace's own madsim harness. No Docker, no protoc, no live backend, no new dev-dependency.
- **Test file:** crates/core/tests/segmented_map_resolution.rs, crates/server/tests/segmented_object_read.rs
  — two **NEW** files, both required (this project's C4 discriminator is an added `*/tests/*.rs`;
  an appended or co-located test degrades to the green-only branch, `run-verify.sh:392-402`).
  Both MUST import only symbols visible on this slice's base — for the core file
  `wyrd_core::read::{read_object, read_path}` plus `wyrd_core::metadata::{encode, decode,
  seg_key, SegmentRecord, InodeRecord, inode_key}`; for the server file
  `wyrd_gateway_core::ObjectGateway` (the streaming and ranged entries are **trait** methods, not
  inherent `pub fn`s — `crates/server/src/lib.rs:270`,`:344`,`:414`; the import idiom is already
  used at `crates/server/tests/s3_http_wire.rs:44` and `crates/server/tests/e2e.rs:85`). Nothing
  this patch adds may be imported, so criteria (2)–(3) are observed **through the read path plus
  an instrumented store double**, not by calling the resolver directly; the resolver's own
  unit-level cases are co-located in `metadata.rs` and covered by `C4-ci`. The DST property goes
  into the existing `crates/dst/tests/custodian.rs`.
- **Verification posture:** default for criteria (1)–(3) — assertion-red on the base, green with
  this patch, both at Check. One declared exception: **the DST resolver-tear property is not a
  Check discriminator** — it is built and exercised in this cycle by the gating `C4-ci`
  (`cargo xtask ci`, which runs `dst`), not by `C4-verify`, for the cfg/seed reason above. It is
  not deferred work: the property ships in this patch and runs in this cycle's gate set.
- **Citations expected:** cite `path:line` on the target branch for every change. **Salvage —
  extract and adapt from `results/issue_649/sources/salvage.diff` (this bundle, permitted input);
  do not re-derive settled code.** It carries #635's `metadata.rs`, `read.rs`, `resolve.rs`,
  `custodian/lib.rs`, `server/lib.rs`, `dst/tests/custodian.rs`, the docs file and the five server
  read tests. **Rework note, verbatim from the issue:** *"read.rs's co-located test seeds its
  segmented object via the deferred committer — rewrite it to seed raw `seg:` records (the server
  tests already do)."* Peers Do MAY open: `origin/main:crates/traits/src/lib.rs:275-324` (`scan` is
  complete-or-fail-loud at `SCAN_CAP`, which is why a group range must be **paged**),
  `:1037-1046` (the `scan_page` cursor/order clauses) and `:1084-1087` — the seam merged as #634 /
  PR #645, commit `18180a2`. Normative design: 0016 decision 7(e) `:2393-2415` and 7(h)
  `:2452-2471`.
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — **the resolver
  paragraph, and only that one**: one metadata value has a ceiling so a large map is segmented;
  resolving is the root plus one bounded range read ordered by parsed index; every consumer
  resolves through the same call; the fail-closed / retired-generation arms; resolve against the
  live root; fail-closed scoped to the object. The **containment** sentences belong to
  #650/#651 and staged publication to #653 — do not write them here.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs. `crates/core/src/read.rs`: 14 PRs — **#647 CLOSED unmerged** (the salvage
  source, closed on **reviewability, not correctness**) and 13 MERGED, none touching chunk-map
  resolution (#594, #564, #558, #534, #489, #448, #416). `crates/server/src/lib.rs`: 19 PRs —
  #647 CLOSED, the rest MERGED and unrelated (#621, #611/#610, #609, #607/#600, #594).
  `crates/custodian/src/resolve.rs` appears in **no** PR but #647 — it is a new file. No merged or
  rejected prior art for a shared chunk-map resolver.
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
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must bind the exact-ceiling and one-coordinate-only extent cases—surviving `>`→`>=` and `; T4 Contribution — The decision owed is whether the seven asserted batch-review blockers and closed/rejected prior art are settled—`scripts/review-branch` and closed-PR refs are absent from this artifact sandbox, so only merged history by affected path was independently checked.; T5 Judgment — Rebuild must assert the recorded metadata-get keys as well as scan prefixes—the double records `gets` at `crates/core/tests/segmented_map_resolution.rs:202`, but the bounded-access oracle inspects only prefixes at `crates/core/tests/segmented_map_resolution.rs:286`.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 56 mutants tested in 2m: 3 missed, 11 caught, 42 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the four asserted batch-review blockers are settled—the red wrapper row cannot be reproduced because `scripts/review-branch` and `scripts/pdca` are absent; merged history and all ten closed-unmerged PRs were independently intersected by every affected path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide whether to trust the reported batch-review/contribution gates—the target lacks `scripts/review-branch` and `scripts/pdca`, so those wrappers could not be rerun; the independent affected-path check did cover all 20 paths plus all six closed-unmerged PRs and found only acknowledged #647 or unrelated work.; T5 Judgment — Rebuild must assert that every recorded metadata `get` is the target inode key—the bounded-footprint oracle only forbids direct `seg:` gets at `crates/core/tests/segmented_map_resolution.rs:546`, so unrelated metadata reads would still pass.. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the three reported batch-review blockers are substantive—the required `scripts/review-branch --bundle` wrapper is absent, so its red row cannot be reproduced; the independent affected-path check covered merged history plus all ten closed-unmerged PRs and found only acknowledged #647 or unrelated dependency/docs work.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the reported batched-review and contribution greens are trustworthy—the target lacks `scripts/review-branch` and `scripts/pdca`, so those wrappers could not be reproduced; the independent check did cover all 20 affected paths, merged history, and all 10 closed-unmerged PRs, finding only acknowledged #647 or unrelated dependency/ADR work.. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on two grounds, both to be fixed in the next attempt: 1. Unresolved BUG finding (T4 batch review, crates/core/src/metadata.rs:2248): segment values are decoded without enforcing MAX_VALUE_BYTES or a per-segment chunk-size ceiling, so a structurally-valid but oversized segment record can force unbounded allocation despite MAX_ROOT_SEGMENTS bounding the count. Must be fixed (add the size ceiling) or explicitly recorded-rejected with justification in review-rejected.md — it was left untriaged in this bundle. 2. Unscoped Cargo.lock change: event-listener bumped 5.4.1->5.4.2 with no manifest change and no brief justification, and it does not even clear the RUSTSEC-2026-0221 advisory it appears to be reacting to. Drop this lockfile edit from the patch; it is out of scope for this slice. Note: the pre-existing base-tree RUSTSEC-2026-0221 advisory itself (independent of any lockfile bump) is accepted as out of scope for this fix and will be tracked in a separate issue (see SUMMARY.md §10) — it is not part of the rejection.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the merits of the T4 batch-review BUG findings (metadata.rs:2250-2251, review-batch.md), confirmed by direct code review: read_group_range's row-count ceiling (accounted > MAX_ROOT_SEGMENTS) bounds how many seg: rows a page can hold, but scan_page returns each page's values fully materialized before the per-row MAX_VALUE_BYTES check ever runs (the doc comment at metadata.rs itself concedes "scan_page is bounded in rows, never in bytes"). So a single retired/corrupted/ oversized seg: row is pulled entirely into memory before SegmentValueOverCeiling can fire, and MAX_ROOT_SEGMENTS oversized rows in one page multiply that — the resolver's claimed bound of MAX_ROOT_SEGMENTS x MAX_VALUE_BYTES is not actually enforced. This directly undercuts the brief's own acceptance criterion 2 ("the resolver is bounded"), and unlike the timeout/deadline findings in this same bundle, it was never argued through to an explicit, precedent-backed rejection — it's just an open, unchecked finding. The companion TEST-GAP finding (segmented_map_resolution.rs:553) independently confirms the current test can't actually prove values aren't over-materialized, since its store double delegates to the real backend before truncating. This is a plan-level question, not a build-level bug-fix: the next attempt should resolve whether the byte ceiling on a segment group's range read is the resolver's responsibility (requiring a genuinely page-and-byte-bounded read primitive, e.g. checking/capping value size as each row streams in rather than after scan_page returns the page) or the store/MetadataStore seam's responsibility (in which case that must be argued through and recorded with the same rigor as the standing timeout/deadline rejection in review-rejected.md, not left as an unresolved review finding). iterate-do was rejected in favor of iterate-plan because 7 prior build iterations patching symptoms within the same resolver shape have not converged — the diff has been growing rather than shrinking and this same boundedness gap survived multiple passes — indicating the slice/approach itself, not just the code, needs reconsideration.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
