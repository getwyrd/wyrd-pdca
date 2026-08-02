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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 154 mutants tested in 3m: 56 caught, 98 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #651: make repair, evacuation, restore, backfill, and desired-state maintenance resolve segmented chunk maps with per-object containment and ceiling-safe repoints.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is explicit and observable across restore safety, segmented repair/evacuation, byte-identical backfill, bounded resolution work, and containment (`crates/custodian/tests/segmented_map_repair.rs:17`). |
| C2 Reproduction (red pre-fix) | PASS | On the local #650 integration commit with only the discriminator added, all 12 tests compiled and failed on behavior; this establishes the required pre-fix red independently (`crates/custodian/tests/segmented_map_repair.rs:417`). |
| C3 Change | NEEDS-HUMAN | Plan must accept this bundle-size exception or split/reduce it — the artifact adds about 2,780 nonblank/noncomment lines against a roughly 1,500-line budget, with the binding test alone reaching 1,772 lines (`crates/custodian/tests/segmented_map_repair.rs:1772`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Require a composed rerun after prerequisite #650 advances the target — current green evidence covers 12 discriminator tests and all CI components on the stale #649-shaped target, whose GC still explicitly rejects segmented maps (`crates/custodian/src/gc.rs:260`). |
| C5 Causal adequacy | PASS | The change removes the cause rather than probing around it: consumers share one per-pass resolver and repoints pin exact root plus segment bytes; all 154 changed-logic mutants were caught or unviable (`crates/custodian/src/resolve.rs:77`, `crates/core/src/metadata.rs:2946`). |
| T1 Structure | PASS | Shared walking and a single repoint builder keep ownership centralized, so maintenance consumers do not grow independent segmented-map parsers or mutation paths (`crates/custodian/src/resolve.rs:77`, `crates/core/src/metadata.rs:2977`). |
| T2 Shape | PASS | The data/API shape carries each chunk's exact record home and prior bytes and exposes a non-certifying loop outcome, which makes stale-generation writes and incomplete walks representable (`crates/core/src/metadata.rs:2773`, `crates/custodian/src/reconciliation.rs:25`). |
| T3 Runtime | PASS | The applied artifact passed the 12-test discriminator, workspace tests, three dependency-wall scans, conformance/statics, and the 50-seed DST race suite (`crates/dst/tests/custodian.rs:1504`). |
| T4 Contribution | NEEDS-HUMAN | Human must adjudicate the seven unavailable batch-review blockers and mechanically settle exhaustive closed/rejected path history — #647 was confirmed closed/unmerged and #402/#555 merged, but the missing report prevents the recorded dispositions required by the repo (`AGENTS.md:206`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make every failed evacuation non-certifying: `EvacOutcome::Aborted` is discarded, while the standing no-spare-domain test expects `Satisfied` even though drain status remains `Pending` (`crates/custodian/src/rebalance.rs:154`, `crates/custodian/tests/rebalance.rs:966`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the composed #650/#651 behavior, after fixing aborted-evacuation certification, is operationally safe for recovery and decommission authorization because those actions rely on the pass's claimed completeness (`docs/design/architecture/06-runtime-view.md:31`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Plan must accept this bundle-size exception or split/reduce it — the artifact adds about 2,780 nonblank/noncomment lines against a roughly 1,500-line budget, with the binding test alone reaching 1,772 lines (`crates/custodian/tests/segmented_map_repair.rs:1772`).
- [ ] C4 Verification (red→green) — Require a composed rerun after prerequisite #650 advances the target — current green evidence covers 12 discriminator tests and all CI components on the stale #649-shaped target, whose GC still explicitly rejects segmented maps (`crates/custodian/src/gc.rs:260`).
- [ ] T4 Contribution — Human must adjudicate the seven unavailable batch-review blockers and mechanically settle exhaustive closed/rejected path history — #647 was confirmed closed/unmerged and #402/#555 merged, but the missing report prevents the recorded dispositions required by the repo (`AGENTS.md:206`).
- [ ] T5 Judgment — Rebuild must make every failed evacuation non-certifying: `EvacOutcome::Aborted` is discarded, while the standing no-spare-domain test expects `Satisfied` even though drain status remains `Pending` (`crates/custodian/src/rebalance.rs:154`, `crates/custodian/tests/rebalance.rs:966`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the composed #650/#651 behavior, after fixing aborted-evacuation certification, is operationally safe for recovery and decommission authorization because those actions rely on the pass's claimed completeness (`docs/design/architecture/06-runtime-view.md:31`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] external dependency: prerequisite slice #650 (PR #676) is absent from origin/pdca-integration/main — the wave base was force-pushed back to #649 + a RUSTSEC bump, so this patch could not build on the ReferenceSet containment the brief's "Depends on: 650" assumed, and carries ~95 lines of its own equivalent instead; the two must be reconciled before both land.
- [ ] C3 Change — Human must choose a split or grant a budget exception — the patch has 2,184 rough nonblank/noncomment additions against the brief's approximately 1,500-line ceiling, including the 1,069-line discriminator (`crates/custodian/tests/segmented_map_repair.rs:1`).
- [ ] C3 Change — Human must decide the mechanical-migration carve-out: the patch has 2,973 insertions and approximately 1,957 nonblank/noncomment lines against the brief's approximately 1,500 semantic-line budget, while its 12 files remain below the 15-file cap.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): The Do beat has now run 5 iterations on this slice and each pass still surfaces a fresh batch of blocking bugs (7 new ones on this run: two memory-blowup risks in the shared resolver/evacuation walk, two duplicate-reference/correctness gaps, a walk-abort-on-one- bad-object regression against the slice's own containment invariant, and an inflated success-count bug) — alongside the bundle running to ~2x the brief's line budget. That pattern (repeated iterations, each still yielding new blockers, growing size) suggests the brief is asking one Do pass to carry too much: it bundles restore, reconstruction, backfill, rebalance and desired-state onto one shared resolver/repoint path in a single slice. Before re-running Do again, replan: reconsider whether this should be split into smaller slices (e.g. shared resolver + one or two consumers first, the rest after), and re-examine whether the per-object containment invariant (walk continues past one damaged object) is actually being honored by the shared walk's error-handling design, since two of the new findings are exactly that invariant breaking under decode/malformed-root errors. Also resolve the #650 dependency mismatch (prerequisite not actually on origin/pdca-integration/main) before the next attempt builds on it again.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
