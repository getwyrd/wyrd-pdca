# Brief — issue 651 / repair-passes-through-resolver-with-containment

> Slice **4 of 6** of the #635 re-slicing (0016 decision 7(e)/(f)). History and the closed PR
> #647 are on the parent issue — https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** repair-passes-through-resolver-with-containment
- **Defect:** Two defects in the passes that **repair, move or rewrite** an object's chunks.
  1. **They cannot see a segmented object's chunks, and one of them would rewrite it blind.**
     Restore, reconstruction (`assess` / `find_chunk`), backfill, rebalance evacuation and
     `desired_state` all read the inline list off the record
     (`crates/custodian/src/reconstruction.rs:325`,`:613`; `.../backfill.rs:93`,`:111-124`,`:169`
     on `origin/main`). A repair obligation for a chunk living in a `seg:` record resolves to
     "referenced by no committed chunk map" and is silently drained, so staged redundancy decays
     untended; a drain of a server holding one never converges; and **backfill clones and rewrites
     `record.chunk_map` wholesale** (`backfill.rs:111-124`) — a blind rewrite of a map it never
     read. Restore is acute: it gates on `protects` (`.../restore.rs:91`,`:183`,`:222`), so its
     `stranded_marked` accounting (`restore.rs:104-145`,`:288`,`:299`) comes from a set missing
     the object's fragments.
  2. **`find_chunk` re-reads each object's root once per queued chunk — the finding still open at
     #647's close.** There `assess` calls `find_chunk` per queued chunk and `find_chunk` resolves
     **every** scanned object, so for segmented objects the deployed custodian's repair loop turns
     Q namespace scans into **Q × N point reads**. Correct, and unusable at fleet scale.
  Additionally `repoint_chunk` — the exact-bytes CAS a repair uses to move a fragment — must not
  grow a record past the value ceiling: every CAS is `require(key, encode(prior))`, so an
  oversized record is permanently un-overwritable.
- **Success criterion:** The added test target `crates/custodian/tests/segmented_map_repair.rs`
  passes and binds the issue's acceptance, driven through base-visible entries
  (`wyrd_custodian::{reconcile_step, reconcile_after_restore, RestoreReport}`):
  1. **`RestoreReport::stranded_marked == 0` across a full reconcile step over segmented objects**,
     seeded as raw `seg:` records + a segmented root (never a committer), and every fragment the
     object owns is still present.
  2. **A repoint respects the record ceilings.** Reconstruction repairs an under-replicated chunk
     whose `ChunkRef` lives in a `seg:` record (the obligation is **not** drained as
     unreferenced) and rebalance evacuates one off a draining server; a repoint that would push a
     record past the value ceiling is refused, not persisted.
  3. **Backfill either resolves a segmented record or skips it with a stated reason and an
     assertion, never rewriting it blind** — given a segmented record it leaves it
     **byte-identical**; a *fillable flat* record in the same store is still filled in the same
     pass.
  4. **Reconstruction resolves each object's root once per pass, not once per queued chunk** —
     with Q > 1 obligations over N objects, the resolutions performed in one pass are asserted to
     be **O(N), not O(Q × N)**, *counted* on an instrumented store double.

  The per-object containment legs the issue's What names (a damaged object does not starve the
  healthy ones; its obligation stays queued) ship as tests here too, but the four criteria above
  are what the slice is judged on.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols. `C4-verify` resets
  `../wyrd-verify` to this bundle's base — a wave>0 bundle gets
  `PDCA_VERIFY_BASE=origin/pdca-integration/main` (`src/pdca_harness/gates.py:371-389`), honoured
  ahead of the brief base (`run-verify.sh:186-192`), so the patch applies onto a tree already
  carrying #648–#650. `--classify` dry-run confirms the single discriminator
  `ADDED_TEST crates/custodian/tests/segmented_map_repair.rs`. The RED leg keeps it, reverts the
  five pass files and `crates/core/src/metadata.rs`, and runs
  `cargo test -p wyrd-custodian --test segmented_map_repair`. On that tree the resolver and the
  GC/scrub containment exist but these five passes do not use them, so criteria (1)–(3) fail on
  assertions (stranded fragments, a drained obligation, a blind rewrite) and (4) fails on the
  counted reads. The file imports only symbols visible on that base — never `repoint_chunk` or
  the ceiling helpers, which this patch adds; criterion (2)'s ceiling refusal is therefore
  observed **through a rebalance/reconstruction pass over an oversized record**, not by calling the
  helper. No dev-dependency is added. Plain Linux workspace over in-memory trait doubles, no
  topology, no cfg gate, so neither the vacuous `0 tests … ok` branch (`:383-389`,`:420-427`) nor
  a compile-red-scored-as-pass can occur.
  **Criterion (4) is the one to watch** — it is the easiest to write vacuously. It must assert a
  *counted* number of resolutions from the instrumented store, not the presence of a cache.
  **Keep the DST property out of the discriminator set:** add the repoint-versus-supersede
  property to the **existing** `crates/dst/tests/custodian.rs` (`#![cfg(madsim)]`); a new
  `crates/dst/tests/*.rs` would force `RUSTFLAGS=--cfg madsim` + 50 seeds onto the whole
  C4-verify invocation (`run-verify.sh:110-134`,`:347-366`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **passes that repair, move or rewrite the record that protects durable bytes**:
  - **A repair obligation is never retired on an incomplete reading** — a pass that could not read
    an object's map may not conclude a chunk belongs to no object; the obligation stays queued.
    Dropping it is redundancy that decays with nothing that will ever restore it.
  - **A pass never rewrites a record it did not read.** The two legal answers are *resolve it* or
    *decline it with a stated reason*, never *overwrite it*.
  - **No repair may leave a record the system can no longer overwrite** — a record grown past the
    value ceiling is a state nothing exits, not a capacity cost.
  - **Containment is per object, and the walk goes on** — a decommission still evacuates the
    servers the damaged object has nothing to do with, and a queued repair for a healthy chunk is
    still assessed; nothing of the unreadable object is filled, moved or reclaimed on the way past.
  - **The work one object can demand of a pass is bounded** — a repair loop costing
    (obligations × objects) is an availability failure the fleet inflicts on itself.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- **Depends on:** 650
- **Conflicts with:** *(none — #652 also edits `crates/core/src/metadata.rs` and already sits
  behind this slice in the chain)*
- **Ordering note:** Fourth of the serial chain, and **serial not parallel** with #650: restore
  calls `gc::referenced_fragments` and gates on `protects` (`restore.rs:91`,`:183`,`:222`), so it
  builds on #650's `ReferenceSet` and containment rule.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** route the **remaining maintenance consumers** through the resolver under the same
  per-object containment, and make a repoint safe to persist.
  `crates/custodian/src/restore.rs` — post-restore reconciliation resolves through the resolver;
  `stranded_marked` accounts for a segmented object's fragments.
  `crates/custodian/src/reconstruction.rs` — `assess` / `find_chunk` resolve **once per object per
  pass**; an obligation for an unresolvable object stays queued.
  `crates/custodian/src/backfill.rs` — a segmented record is handled **deliberately**: resolved,
  or skipped with a stated reason and an assertion (it rewrites the map, so a shape assumption is
  destructive here). `.../rebalance.rs`, `.../desired_state.rs` — evacuation and the drain-status
  surface, same containment. `crates/core/src/metadata.rs` — the record-ceiling checks the repoint
  CAS needs (~31 semantic lines) and `repoint_chunk`, the exact-bytes CAS that moves a chunk's
  placement in whichever record holds it. Plus their existing test files, the added file below,
  the central `reconcile_step` leg, and the DST repoint-versus-supersede property in the
  **existing** `crates/dst/tests/custodian.rs`.
  **Caller-first:** every production symbol introduced here has a caller **in this slice** —
  `repoint_chunk` is called by reconstruction and rebalance, the ceiling checks by
  `repoint_chunk`; this slice lands no behaviour flip and no producer of segmented maps. **Out of
  scope:** the chunk-id floor (#652); the committer, fence, rollback and resume (#653) — carve out
  **only** the record-ceiling helpers `repoint_chunk` needs, not the committer around them; any
  new/edited ADR / spec / proposal; any conformance-vector change.
- **Budget:** ≤ ~1,500 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 15 files.
  Mechanical migration — **pass callsites gaining the resolved-chunk-list form in place of
  `record.chunk_map` indexing** — is counted separately and allowed on top; declare it as that
  pattern. Salvage **production** is small (`restore.rs` +40, `reconstruction.rs` +59,
  `backfill.rs` +128, `rebalance.rs` +44, `desired_state.rs` +8 = **279 semantic lines**, plus ~31
  for the ceiling helpers); #647's full test files push this to ~2,000, so the whole budget risk
  is tests — **prune to the per-pass binding legs** for criteria (1)–(4). If mid-build the tree
  exceeds this, STOP and hand back a proposed split instead of finishing — an over-budget patch is
  iterate-to-Plan by default, not another Do round.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs`
  — `assess` at `:315` indexes `prior.chunk_map[chunk_index]` at `:325`, `find_chunk` at `:603`
  reads `.chunk_map` at `:613`; `.../backfill.rs` clones and rewrites at `:111-124`;
  `.../restore.rs` gates on `referenced.protects` at `:222` and counts `stranded_marked` at
  `:288`,`:299`. For defect (2), read the closed PR in `sources/salvage.diff`: `assess` calls
  `find_chunk` per queued chunk and `find_chunk` resolves every scanned object.
- **External dependencies:** `typos`, `docs-renderer` — registered doctor.checks rows (ids
  `typos`, `docs-renderer`), named because this slice edits a living-architecture paragraph and
  `cargo xtask ci`'s prose gates warn-and-skip when those tools are absent locally
  (INTEGRATION §3). Nothing else beyond the base Rust toolchain — the custodian loops run over the
  `traits`/`core` seams with in-memory doubles, and the DST property runs under the workspace's
  own madsim harness. No Docker, no protoc, no live backend, no new dev-dependency.
- **Test file:** `crates/custodian/tests/segmented_map_repair.rs` — a **NEW** file, and this is
  not optional: `segmented_map_consumers.rs` already exists on this slice's base (#650 added it),
  so appending to it makes it a **modified** file — not this project's C4 discriminator — and the
  gate would take the green-only branch (`run-verify.sh:392-402`) and prove no red at all. Import
  only symbols visible on this base (the `crates/custodian/tests/gc.rs:33-40` set plus
  `reconcile_after_restore` / `RestoreReport`), reusing #650's fixture helpers. The central
  `reconcile_step` leg and updates to the per-pass test files may ship **in addition** — `C4-ci`
  covers them.
- **Verification posture:** default for criteria (1)–(4) — assertion-red on the base (which
  carries #648–#650), green with this patch, both at Check. One declared exception: the **DST
  repoint-versus-supersede property** is not a Check discriminator; it is built and exercised in
  this cycle by the gating `C4-ci` (`cargo xtask ci`, which runs `dst`), not by `C4-verify`, for
  the cfg/seed reason above. It is not deferred work — it ships in this patch.
- **Citations expected:** cite `path:line` on the target branch for every change. **Salvage —
  extract and adapt from `results/issue_651/sources/salvage.diff` (this bundle, permitted input);
  do not re-derive settled code.** It carries #635's five pass files, `metadata.rs`, the five
  per-pass test files, the shared fixture, `dst/tests/custodian.rs` and the docs file. Take the
  containment shape from the pass regions but **do not carry over the per-queued-chunk resolve —
  that is defect (2)**; from `metadata.rs` take **only** `repoint_chunk` and the
  `check_record_ceilings` / `check_value_ceiling` helpers, not the committer around them; from the
  fixture take the reconstruction / rebalance / backfill and containment legs (the GC/scrub legs
  are #650's, the read-path leg #649's — do not pull them forward). Peers Do MAY open: **the
  containment peer already in the tree**, `origin/main:crates/custodian/src/desired_state.rs:82-98`,
  `:150-179` (`PendingMalformed`: attribute the blocker, name the chunks, keep answering) — every
  pass here contains a fault the same way; and the reference-set gate this composes with,
  `.../restore.rs:91`,`:183`,`:222`. Normative: 0016 decision 7(e) `:2393-2415`, 7(f)
  `:2416-2431`, the §1 `seg:` row's writer (2) `:354` (pre-mark → write → exact-bytes CAS).
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — extend the
  containment paragraph with **the repair/evacuation-walk sentences this slice lands, and only
  those**: a record a pass cannot read is attributed and skipped and the walk continues; a repair
  obligation whose chunk may live in it is **kept queued** rather than retired as referenced by
  nothing; a record that was never *committed* holds nothing back. Staged publication is #653's.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs. Across the five pass files (12/4/8/2/3 PRs respectively), **#647 is the only one
  touching this concern and it is CLOSED unmerged** — closed on **reviewability, not
  correctness**, which is why it is the salvage source; its `find_chunk` perf finding is what this
  slice fixes. Every other PR is MERGED and unrelated to chunk-map resolution, with two that bind
  criteria here: **#402** (backfill identity placement — the rewrite path defect (1) concerns) and
  **#555** (post-restore reconciliation, whose `stranded_marked` contract criterion (1) extends to
  segmented objects). No prior art routes these passes through a shared resolver, and none for
  `repoint_chunk`.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` implementation owns the network bound, not the caller; (ii) *retraction of
  already-published bytes* — rejected 4× in #638 on unchanged evidence; (iii) *"`Completed`
  releases its admission slot"* — withdrawn as unsatisfiable; a `Completed` tombstone **stays
  counted**; (iv) every settled decision named in the slice issue's body, in particular that
  backfill **skipping** a segmented record with a stated reason is an accepted answer — it is not
  a coverage gap to be closed by making backfill rewrite one. Do MUST record each rejection in
  `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make the evidence bind the claimed cause: the ceiling fixture seeds an already-oversize record and moves server 0→3, so it never exercises a legal-to-oversize transition, while 18 changed-logic mutants survive (`crates/custodian/tests/segmented_map_repair.rs:543`, `crates/custodian/tests/segmented_map_repair.rs:573`).; T4 Contribution — Human must inspect the 23 recorded batch-review blockers because `scripts/review-branch` and its raw report are unavailable here; affected-file PR checks did confirm #647 as the only closed-unmerged prior art and the contribution checker reports pass.; T5 Judgment — Rebuild must preserve a non-certifying outcome and add binding containment tests: unreadable records are skipped but both repair passes fall through to `Satisfied`; a direct probe observed `rebalance=Satisfied` and `reconstruction=Satisfied` (`crates/custodian/src/rebalance.rs:140`, `crates/custodian/src/reconstruction.rs:313`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 23 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 62 mutants tested in 87s: 18 missed, 5 caught, 39 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 23 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Human must adjudicate the 14 recorded batch-review blockers — `scripts/review-branch` and its raw report are unavailable here, although affected-path checks independently confirmed #647 as closed-unmerged and #402/#555 as merged prior art.; T5 Judgment — Rebuild must make a refused evacuation non-certifying — `Aborted` is discarded at `crates/custodian/src/rebalance.rs:149`, so the test reports `Satisfied` while the placement remains on the draining server (`crates/custodian/tests/segmented_map_repair.rs:663`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must bind the restart-to-noncommitted arm: flipping `!=` to `==` at `crates/core/src/metadata.rs:2846` survived the independently reproduced 129-mutant run, so that retirement case is untested.; T4 Contribution — Human must adjudicate the 11 reported blocking review findings and mechanically settle closed/rejected prior art: `scripts/review-branch` and its report are unavailable here, although affected-path local history confirms merged #402 and #555.; T5 Judgment — Rebuild must make malformed inode keys attributed blockers and add binding coverage—the current silent skip can let reconstruction classify an owned chunk as absent and drain its repair at `crates/custodian/src/resolve.rs:97`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 129 mutants tested in 3m: 1 missed, 42 caught, 86 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Human must adjudicate the batch review's eight opaque blockers because `scripts/review-branch` and its report are absent; affected-path PR inventory independently confirms #647 closed-unmerged and #402/#555 merged, but definition of done requires recorded dispositions (`AGENTS.md:206`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: patch.diff does not apply on origin/pdca-integration/main — the bundle is stale; rebase Do.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: The Do beat has now run 5 iterations on this slice and each pass still surfaces a fresh batch of blocking bugs (7 new ones on this run: two memory-blowup risks in the shared resolver/evacuation walk, two duplicate-reference/correctness gaps, a walk-abort-on-one- bad-object regression against the slice's own containment invariant, and an inflated success-count bug) — alongside the bundle running to ~2x the brief's line budget. That pattern (repeated iterations, each still yielding new blockers, growing size) suggests the brief is asking one Do pass to carry too much: it bundles restore, reconstruction, backfill, rebalance and desired-state onto one shared resolver/repoint path in a single slice. Before re-running Do again, replan: reconsider whether this should be split into smaller slices (e.g. shared resolver + one or two consumers first, the rest after), and re-examine whether the per-object containment invariant (walk continues past one damaged object) is actually being honored by the shared walk's error-handling design, since two of the new findings are exactly that invariant breaking under decode/malformed-root errors. Also resolve the #650 dependency mismatch (prerequisite not actually on origin/pdca-integration/main) before the next attempt builds on it again.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
