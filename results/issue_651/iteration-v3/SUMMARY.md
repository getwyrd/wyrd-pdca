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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 129 mutants tested in 3m: 1 missed, 42 caught, 86 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #651: route repair, move, and rewrite passes through segmented-map resolution with per-object containment, bounded reconstruction reads, and ceiling-safe repoints.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Human acceptance is decidable: the brief gives four observable criteria, an assertion-red discriminator, the containment invariant, dependencies, and an explicit scope budget. |
| C2 Reproduction (red pre-fix) | PASS | Fresh prerequisite-base execution produced 7/7 behavioral failures across restore, reconstruction, rebalance, backfill, containment, and counted resolution; the discriminator is enumerated at `crates/custodian/tests/segmented_map_repair.rs:17`. |
| C3 Change | NEEDS-HUMAN | Human must decide the mechanical-migration carve-out: the patch has 2,973 insertions and approximately 1,957 nonblank/noncomment lines against the brief's approximately 1,500 semantic-line budget, while its 12 files remain below the 15-file cap. |
| C4 Verification (red→green) | PASS | The same 7 tests went red→green, and typos, docs lint/render, fmt, clippy, build/tests, machete, deny, conformance, statics, and the 50-seed DST suite independently passed; the exact gate wrapper only hit host-side advisory-lock/network refresh faults. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must bind the restart-to-noncommitted arm: flipping `!=` to `==` at `crates/core/src/metadata.rs:2846` survived the independently reproduced 129-mutant run, so that retirement case is untested. |
| T1 Structure | PASS | The shared maintenance walk stays behind the core/traits seam and is private to custodian at `crates/custodian/src/resolve.rs:35` and `crates/custodian/src/lib.rs:30`; no dependency-direction or crate-root convention is violated. |
| T2 Shape | FAIL | A decoded committed record whose `inode:` suffix is nonnumeric is silently omitted at `crates/custodian/src/resolve.rs:97`; a scratch probe observed backfill return `Satisfied` with it untouched, so an incomplete walk is shaped as complete. |
| T3 Runtime | FAIL | Backfill resolves the entire namespace at `crates/custodian/src/backfill.rs:77` and again at `crates/custodian/src/backfill.rs:208`, doubling object point/range reads despite the one-resolution-per-object contract at `docs/design/architecture/06-runtime-view.md:33`. |
| T4 Contribution | NEEDS-HUMAN | Human must adjudicate the 11 reported blocking review findings and mechanically settle closed/rejected prior art: `scripts/review-branch` and its report are unavailable here, although affected-path local history confirms merged #402 and #555. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make malformed inode keys attributed blockers and add binding coverage—the current silent skip can let reconstruction classify an owned chunk as absent and drain its repair at `crates/custodian/src/resolve.rs:97`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide deployment fitness after the silent-certification and duplicate-resolution defects are rebuilt and the 11 opaque review blockers are adjudicated, because false convergence can retire repair work or authorize decommission. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Human must decide the mechanical-migration carve-out: the patch has 2,973 insertions and approximately 1,957 nonblank/noncomment lines against the brief's approximately 1,500 semantic-line budget, while its 12 files remain below the 15-file cap.
- [ ] C5 Causal adequacy — Rebuild must bind the restart-to-noncommitted arm: flipping `!=` to `==` at `crates/core/src/metadata.rs:2846` survived the independently reproduced 129-mutant run, so that retirement case is untested.
- [ ] T4 Contribution — Human must adjudicate the 11 reported blocking review findings and mechanically settle closed/rejected prior art: `scripts/review-branch` and its report are unavailable here, although affected-path local history confirms merged #402 and #555.
- [ ] T5 Judgment — Rebuild must make malformed inode keys attributed blockers and add binding coverage—the current silent skip can let reconstruction classify an owned chunk as absent and drain its repair at `crates/custodian/src/resolve.rs:97`.
- [ ] Validation — fitness-to-purpose — Human must decide deployment fitness after the silent-certification and duplicate-resolution defects are rebuilt and the 11 opaque review blockers are adjudicated, because false convergence can retire repair work or authorize decommission.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- [ ] C3 Change — Human must choose a split or grant a budget exception — the patch has 2,184 rough nonblank/noncomment additions against the brief's approximately 1,500-line ceiling, including the 1,069-line discriminator (`crates/custodian/tests/segmented_map_repair.rs:1`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must bind the restart-to-noncommitted arm: flipping `!=` to `==` at `crates/core/src/metadata.rs:2846` survived the independently reproduced 129-mutant run, so that retirement case is untested.; T4 Contribution — Human must adjudicate the 11 reported blocking review findings and mechanically settle closed/rejected prior art: `scripts/review-branch` and its report are unavailable here, although affected-path local history confirms merged #402 and #555.; T5 Judgment — Rebuild must make malformed inode keys attributed blockers and add binding coverage—the current silent skip can let reconstruction classify an owned chunk as absent and drain its repair at `crates/custodian/src/resolve.rs:97`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
