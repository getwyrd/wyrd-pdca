# Result — issue 651 / repair-passes-through-resolver-with-containment

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  Two defects in the passes that **repair, move or rewrite** an object's chunks.
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
- Success criterion:
  The added test target `crates/custodian/tests/segmented_map_repair.rs`
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
- Repo + branch target:
  getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- Scope (one logical fix) / out of scope:
  route the **remaining maintenance consumers** through the resolver under the same
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 96 mutants tested in 82s: 32 caught, 64 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make repair, restore, backfill, rebalance, and drain-status passes safely resolve segmented chunk maps while bounding reconstruction reads and refusing oversize repoints.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is decidable across restore protection, segmented repair/evacuation, deliberate backfill refusal, and counted O(N) resolution evidence (`crates/custodian/tests/segmented_map_repair.rs:15`). |
| C2 Reproduction (red pre-fix) | PASS | The retained discriminator compiled on the issue-650 base and all 8 tests failed on assertions, including the stranded-fragment verdict at `crates/custodian/tests/segmented_map_repair.rs:313`. |
| C3 Change | NEEDS-HUMAN | Human must choose a split or grant a budget exception — the patch has 2,184 rough nonblank/noncomment additions against the brief's approximately 1,500-line ceiling, including the 1,069-line discriminator (`crates/custodian/tests/segmented_map_repair.rs:1`). |
| C4 Verification (red→green) | PASS | The same 8 tests turned green, and independent typos, docs render, fmt, clippy, build, workspace tests, three dependency-wall checks, conformance, statics, and 50-seed DST reruns passed (`crates/dst/tests/custodian.rs:2049`). |
| C5 Causal adequacy | PASS | The evidence binds both formerly weak causes: the fixture proves a legal-to-oversize transition and the store double asserts exactly N segment reads (`crates/custodian/tests/segmented_map_repair.rs:623`, `crates/custodian/tests/segmented_map_repair.rs:859`); 96 rerun mutants left no survivors. |
| T1 Structure | PASS | Resolver-with-homes and exact-record repoint logic are centralized at the metadata boundary and consumed by both moving passes (`crates/core/src/metadata.rs:2671`). |
| T2 Shape | PASS | Segmented moves CAS both the carrying segment's exact bytes and the resolved root, preserving the generation boundary without new dependency or format surface (`crates/core/src/metadata.rs:2977`). |
| T3 Runtime | PASS | In-memory pass tests and the seeded repoint-versus-supersede simulation exercised successful, conflicting, refused, and reclaimable-copy outcomes (`crates/dst/tests/custodian.rs:1715`). |
| T4 Contribution | NEEDS-HUMAN | Human must adjudicate the 14 recorded batch-review blockers — `scripts/review-branch` and its raw report are unavailable here, although affected-path checks independently confirmed #647 as closed-unmerged and #402/#555 as merged prior art. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make a refused evacuation non-certifying — `Aborted` is discarded at `crates/custodian/src/rebalance.rs:149`, so the test reports `Satisfied` while the placement remains on the draining server (`crates/custodian/tests/segmented_map_repair.rs:663`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the operator experience for a permanently refused drain is acceptable — data remains safe, but the current reconciliation signal can claim convergence instead of exposing the blocked evacuation (`crates/custodian/src/rebalance.rs:482`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Human must choose a split or grant a budget exception — the patch has 2,184 rough nonblank/noncomment additions against the brief's approximately 1,500-line ceiling, including the 1,069-line discriminator (`crates/custodian/tests/segmented_map_repair.rs:1`).
- [ ] T4 Contribution — Human must adjudicate the 14 recorded batch-review blockers — `scripts/review-branch` and its raw report are unavailable here, although affected-path checks independently confirmed #647 as closed-unmerged and #402/#555 as merged prior art.
- [ ] T5 Judgment — Rebuild must make a refused evacuation non-certifying — `Aborted` is discarded at `crates/custodian/src/rebalance.rs:149`, so the test reports `Satisfied` while the placement remains on the draining server (`crates/custodian/tests/segmented_map_repair.rs:663`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the operator experience for a permanently refused drain is acceptable — data remains safe, but the current reconciliation signal can claim convergence instead of exposing the blocked evacuation (`crates/custodian/src/rebalance.rs:482`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Human must adjudicate the 14 recorded batch-review blockers — `scripts/review-branch` and its raw report are unavailable here, although affected-path checks independently confirmed #647 as closed-unmerged and #402/#555 as merged prior art.; T5 Judgment — Rebuild must make a refused evacuation non-certifying — `Aborted` is discarded at `crates/custodian/src/rebalance.rs:149`, so the test reports `Satisfied` while the placement remains on the draining server (`crates/custodian/tests/segmented_map_repair.rs:663`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
