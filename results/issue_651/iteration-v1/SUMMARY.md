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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 62 mutants tested in 87s: 18 missed, 5 caught, 39 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 23 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #651: route repair, evacuation, restore, desired-state, and backfill maintenance over segmented chunk maps with containment, bounded resolution work, and ceiling-safe repoints.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is decision-ready: remaining maintenance passes must resolve segmented maps with per-object containment, safe repoints, deliberate backfill behavior, and O(N) reconstruction work. |
| C2 Reproduction (red pre-fix) | PASS | An isolated base-only build compiled and all six discriminator tests failed pre-fix; the same six pass with the patch (`crates/custodian/tests/segmented_map_repair.rs:278`). |
| C3 Change | PASS | The patch stays on the declared maintenance/data surfaces and centralizes home resolution plus repoint CAS in the core metadata seam (`crates/core/src/metadata.rs:2738`, `crates/core/src/metadata.rs:2805`). |
| C4 Verification (red→green) | PASS | Independent clean reruns proved six-red→six-green and a full `cargo xtask ci` pass with real typos, docs render, deny, conformance, and 50-seed DST, including the supersede race (`crates/dst/tests/custodian.rs:1996`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must make the evidence bind the claimed cause: the ceiling fixture seeds an already-oversize record and moves server 0→3, so it never exercises a legal-to-oversize transition, while 18 changed-logic mutants survive (`crates/custodian/tests/segmented_map_repair.rs:543`, `crates/custodian/tests/segmented_map_repair.rs:573`). |
| T1 Structure | PASS | Dependency direction remains narrow: custodian consumers use the core resolver/CAS over existing trait seams, with no new backend coupling (`crates/custodian/src/rebalance.rs:178`, `crates/custodian/src/reconstruction.rs:692`). |
| T2 Shape | PASS | Scope and size fit the brief: 10 files and approximately 1,157 nonblank/noncomment added lines are within the 15-file/~1,500-line budget, with no forbidden ADR/spec/vector change. |
| T3 Runtime | FAIL | Fleet-scale memory is quadratic in chunks per record: every homed chunk clones its entire segment record, then reconstruction clones the resolved root per chunk (`crates/core/src/metadata.rs:2722`, `crates/custodian/src/reconstruction.rs:698`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect the 23 recorded batch-review blockers because `scripts/review-branch` and its raw report are unavailable here; affected-file PR checks did confirm #647 as the only closed-unmerged prior art and the contribution checker reports pass. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must preserve a non-certifying outcome and add binding containment tests: unreadable records are skipped but both repair passes fall through to `Satisfied`; a direct probe observed `rebalance=Satisfied` and `reconstruction=Satisfied` (`crates/custodian/src/rebalance.rs:140`, `crates/custodian/src/reconstruction.rs:313`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether to trigger Tier-1 disk-fault and Tier-2 kill-and-reconstruct before sign-off; both real-environment scenarios remain unrun and the repo requires an explicit follow-up judgment for custodian/reconstruction durability changes (`AGENTS.md:78`, `AGENTS.md:81`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must make the evidence bind the claimed cause: the ceiling fixture seeds an already-oversize record and moves server 0→3, so it never exercises a legal-to-oversize transition, while 18 changed-logic mutants survive (`crates/custodian/tests/segmented_map_repair.rs:543`, `crates/custodian/tests/segmented_map_repair.rs:573`).
- [ ] T4 Contribution — Human must inspect the 23 recorded batch-review blockers because `scripts/review-branch` and its raw report are unavailable here; affected-file PR checks did confirm #647 as the only closed-unmerged prior art and the contribution checker reports pass.
- [ ] T5 Judgment — Rebuild must preserve a non-certifying outcome and add binding containment tests: unreadable records are skipped but both repair passes fall through to `Satisfied`; a direct probe observed `rebalance=Satisfied` and `reconstruction=Satisfied` (`crates/custodian/src/rebalance.rs:140`, `crates/custodian/src/reconstruction.rs:313`).
- [ ] Validation — fitness-to-purpose — Human must decide whether to trigger Tier-1 disk-fault and Tier-2 kill-and-reconstruct before sign-off; both real-environment scenarios remain unrun and the repo requires an explicit follow-up judgment for custodian/reconstruction durability changes (`AGENTS.md:78`, `AGENTS.md:81`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 23 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make the evidence bind the claimed cause: the ceiling fixture seeds an already-oversize record and moves server 0→3, so it never exercises a legal-to-oversize transition, while 18 changed-logic mutants survive (`crates/custodian/tests/segmented_map_repair.rs:543`, `crates/custodian/tests/segmented_map_repair.rs:573`).; T4 Contribution — Human must inspect the 23 recorded batch-review blockers because `scripts/review-branch` and its raw report are unavailable here; affected-file PR checks did confirm #647 as the only closed-unmerged prior art and the contribution checker reports pass.; T5 Judgment — Rebuild must preserve a non-certifying outcome and add binding containment tests: unreadable records are skipped but both repair passes fall through to `Satisfied`; a direct probe observed `rebalance=Satisfied` and `reconstruction=Satisfied` (`crates/custodian/src/rebalance.rs:140`, `crates/custodian/src/reconstruction.rs:313`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 23 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
