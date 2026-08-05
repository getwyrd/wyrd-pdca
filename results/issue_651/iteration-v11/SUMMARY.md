# Result — issue 651 / restore-and-drain-report-contained-and-attributed

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The two surfaces that report **whether a reconciliation is complete** cannot survive —
  let alone describe — an object whose chunk map they could not read.
  1. **Post-restore reconciliation fails the whole pass closed.** The *mark* half of
     `reconcile_after_restore` is already safe (it withholds every fragment while the reference set
     is incomplete — `gc::ReferenceSet::protects`, `gc.rs:331`). Its *report* half re-reads every
     committed record through `committed_chunks` (`restore.rs:326`), which `?`s out on a segmented
     map (`restore.rs:390`, `:403-405` `.as_flat().ok_or(SegmentedMapUnsupported)`). So a store
     holding **one** segmented object — or one unreadable committed root — returns `Err`, and the
     operator command produces **no report at all**: not a stranded count, not the dangling or
     misplaced chunks of the thousands of objects it *could* read. One damaged object blanks the
     whole answer. #650 marks the gap in-tree at `restore.rs:196`.
  2. **The drain surface cannot tell "not converged" from "I could not look."**
     `reconciliation_status` answers a bare `Pending` over an incomplete set
     (`desired_state.rs:188-190`) — the same word it uses for a server that genuinely still holds
     referenced fragments. An operator watching a decommission stall cannot learn *which* record
     blocks it, so the stall is a state nothing exits. The opposite pattern already sits one level
     up: `PendingMalformed { chunks }` names its blockers in the answer itself
     (`desired_state.rs:101-104`, #397). #650 marks the gap at `desired_state.rs:183-187`.
  3. **The operator surface would print a hollow green.** `wyrd custodian` exits non-zero only on
     `dangling` / `misplaced` (`cli.rs:1197-1237`). An incomplete reading has no cell in the summary
     and no effect on the exit code — so once (1) stops erroring, a restore script checking the
     status code records a run that could not read part of the store as healthy. The comment at
     `cli.rs:1230-1233` already refuses exactly this, naming *"one whose chunks cannot be read"*,
     while the `if` at `:1234` does not test for it.
- Success criterion: `crates/custodian/tests/segmented_map_restore.rs` passes, driven **only**
  through symbols visible on the base (`wyrd_custodian::{reconcile_after_restore, RestoreReport,
  GcContext}`, `wyrd_custodian::desired_state::{reconciliation_status, ReconciliationStatus}`):
  1. **A segmented object no longer stops the pass.** Over a store seeded with a segmented object
     (raw `seg:` records plus a segmented root, **never** a committer), `reconcile_after_restore`
     returns `Ok` (today: `Err`), `RestoreReport::stranded_marked == 0`, and every fragment that
     object owns is still on its D server afterwards.
  2. **A damaged object is contained, and the run is not certified.** Three legs, each closing a
     hole the others leave open — all three ship:
     - **(2a) non-certification, incomplete reading the SOLE cause.** One committed object whose
       chunk map cannot be read; **every other object healthy** — nothing dangling, misplaced or
       under-replicated among those the pass can read, and no stray fragment beyond the ones the
       unreadable object itself owns (withheld by `protects`, base behaviour from #650, and they
       must stay withheld). Pass returns `Ok`, `stranded_marked == 0`, and `report.is_clean()` is
       **false**. `is_clean()` (`restore.rs:144`) is already false whenever any loss is reported, so
       a scenario carrying a loss satisfies this clause **without the fix** — assert it on a store
       where the unreadable object is the only reason.
     - **(2b) containment — the damaged object does not starve the healthy ones.** The same
       unreadable object seeded **beside** a readable object with a genuine loss: the pass still
       returns `Ok`, still reports that object's loss (`dangling` or `misplaced` names its chunk),
       and still marks nothing of the unreadable one.
     - **(2c) marks and report rest on one reading.** Seed a metadata double serving a committed
       record **readable on the pass's first `inode:` read and unreadable on any later one**, plus a
       genuinely unreferenced fragment a complete reading would mark. Assert the **conjunction**:
       *the pass never both marks a fragment and reports an unreadable record* — `stranded_marked >
       0` and an unresolvable attribution on the audit seam must not both hold. Stated that way it
       is implementation-neutral: a pass reading the set **once** sees the record fine and marks the
       stray (passes); a pass reading twice but withholding marks while **either** reading found a
       hole marks nothing and names the record (passes); a pass reading twice and gating on the
       **first** reading marks the stray *and* reports the record unreadable (fails — the defect
       this leg exists to catch). Do **not** assert "marks nothing" unconditionally: that would fail
       the single-reading implementation, which is the better one.
  3. **Attribution, on the audit seam.** The restore pass names the object it could not read, and
     `reconciliation_status` over the (2a) store attributes the blocking object instead of answering
     a bare `Pending`. Assert both as #650's own fixture does — `assert_attributes_blocker`,
     `crates/custodian/tests/segmented_map_consumers.rs:353` — so this needs no symbol the base lacks.

  Criterion (2) is binding: it is the point of the slice and the one an incomplete fix passes
  vacuously. A version that only checks the call returned `Ok` proves none of it.

  **Not in the discriminator, covered by C4-ci:** anything needing a shape this patch introduces —
  the new `RestoreReport` field's contents, the new `ReconciliationStatus` variant, the CLI verdict
  and exit code. Those ship in `restore_reconcile.rs`, `segmented_map_consumers.rs` and `cli.rs`'s
  own `#[cfg(test)] mod tests`.
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2: single slice, no live
  milestone integration branch — M4's is merged and deleted, and #648/#649/#650 landed on `main`
  directly. Verified `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`. **Not**
  `pdca-integration/main`, the driver's run-scoped wave-fold branch — that is what made v5 build on
  a tree missing #650, and `wave_mode` is `"merge"` now regardless.)
- Scope (one logical fix) / out of scope: make both completeness-reporting surfaces answer **contained and attributed** over a
  reference set with a hole in it, instead of erroring or certifying.
  - `crates/custodian/src/restore.rs` — the report half survives an object it cannot read: it
    reports every object it *could* read, records the ones it could not, and the pass is not clean.
    Marks and report rest on one reading: no fragment is marked while any reading in the pass found
    a record it could not read.
  - `crates/custodian/src/desired_state.rs` — the drain status distinguishes "still holds referenced
    fragments" from "could not read object X", and names X. Where an unreadable record and a
    malformed placement are both present the answer ranks the unreadable one first, matching the
    base's existing check order (`:188` precedes `:198`); the operator repairs the named record and
    re-polls for the malformed ids. **Plan decision — settled, not Do's to revisit.**
  - `crates/server/src/cli.rs` — the summary states an incomplete reading and the exit status
    reflects it. Keep to the summary cell, one NEEDS-HUMAN paragraph and the exit code; the exit
    code must be **derived from the report itself**, not re-computed from the paragraphs, so the
    printed verdict and the status code cannot drift.

  **Out of scope:**
  - **The cross-object chunk-id ambiguity rule — DROPPED, not deferred; it must not come back.** No
    notion of "more than one committed object claims this chunk id", no claim-counting on
    `Expected` / `committed_chunks`, no `ambiguous-*` audit event, no mark-withholding keyed on it.
    Three reasons, all verified on the base at Plan: (i) the gateway mints `(random per-process
    epoch << 64) | seq` with the top bit set, so every id is ≥ 2^127 and two live records cannot
    collide — `crates/server/src/lib.rs:238-241`, with `:230-237` saying so outright, while the
    cluster path packs the inode; (ii) #652 **deleted** the chunk-id floor as dead code left by
    #487, so there is no "fold it into #652" left; (iii) the rule's only in-tree-reachable trigger —
    one record listing an id twice — made the command print *"chunk(s) are LOST"* and exit 1 over a
    **healthy** object, the very C-1 failure this brief exists to prevent, reached from the other
    side. Whether a hand-assembled or imported namespace with duplicate live ids is a supported
    restore state is a maintainer decision on its own issue, to be answered before any code guards it.
  - Reconstruction, backfill, rebalance and the resolving namespace walk they need (**#681**): this
    slice adds **no** custodian-level walk and no `crate::resolve` module, reading the reference set
    through `gc::referenced_fragments` exactly as the base does.
  - `repoint_chunk` and the record ceilings (**#682**): this slice writes **nothing** to a chunk map.
  - `gc.rs` and `scrub.rs` untouched. The committer, fence, rollback and resume (#653).
  - **No report-schema churn:** `dangling` / `misplaced` / `under_replicated` keep their `Vec<ChunkId>`
    shape. Threading the owning `inode:` key through `Expected` / `emit_dangling` / `emit_misplaced`
    is declined — it widens the schema for a gap that existed only under the dropped rule.
  - Any new/edited ADR, spec or proposal; any conformance-vector change; any new CLI subcommand
    (there is no drain CLI today and this slice adds none — `reconciliation_status` is a public
    library surface, and its attributed answer lands in the return value and the audit seam, as
    `PendingMalformed` has since #397).

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 31 mutants tested in 83s: 19 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #651: keep restore and drain reconciliation contained, attributed, and non-certifying when a committed chunk map cannot be read.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes two explicit completeness surfaces, binds containment/non-certification/attribution and one-reading behavior, and settles ordering, exclusions, budget, dependencies, and an assertion-red discriminator. |
| C2 Reproduction (red pre-fix) | PASS | With the implementation and modified tests stashed, all four base-visible discriminator cases failed behaviorally—three on `SegmentedMapUnsupported` and one on the later decode error—at `crates/custodian/tests/segmented_map_restore.rs:436` and `crates/custodian/tests/segmented_map_restore.rs:619`. |
| C3 Change | PASS | The patch stays within the settled restore, drain-status, CLI, test, and living-doc surfaces; typed per-object containment begins at `crates/custodian/src/restore.rs:508`, and no dropped ambiguity apparatus or unrelated maintenance walk returns. |
| C4 Verification (red→green) | PASS | The discriminator changed from 0/4 to 4/4 with the fix restored, and an independent exact `cargo xtask ci` run passed typos, docs render, fmt, clippy, build/tests, all three deny walls, conformance, statics/deploy guards, and 50-seed DST; the discriminator starts at `crates/custodian/tests/segmented_map_restore.rs:436`. |
| C5 Causal adequacy | PASS | The fatal per-object report read is transformed into typed containment and continued traversal at `crates/custodian/src/restore.rs:508`, while the union gates marking at `crates/custodian/src/restore.rs:268`; no capability probe or symptom guard is introduced, and all 31 in-diff mutants were caught or unviable. |
| T1 Structure | PASS | Exactly the eight allocated files change, the new discriminator is a standalone unsafe-forbidden integration test at `crates/custodian/tests/segmented_map_restore.rs:45`, and existing trait/module boundaries remain intact. |
| T2 Shape | PASS | The eight-file cap is met and a conservative count leaves 623 additions after excluding only blank/comment and clearly mechanical import/attribute/delimiter lines, below the 700 semantic-line cap; report vectors retain their existing shape at `crates/custodian/src/restore.rs:130`. |
| T3 Runtime | FAIL | An unreadable object discovered at `crates/custodian/src/restore.rs:257` is not attributed until `crates/custodian/src/restore.rs:272`; failures in the intervening orphan, pending, or second namespace reads at `:258-266` still return without the repairable record name. |
| T4 Contribution | FAIL | Affected-path merged history, the latest 100 closed PRs, and open PRs show only closed unmerged ancestor #647 and no competing open work, but the unresolved attribution-order defect at `crates/custodian/src/restore.rs:257` means the required deep review is not yet discharged. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must emit first-read attribution before any later fallible read and add a regression forcing an intervening metadata-scan failure—otherwise a restore error can still erase the operator's route to the damaged record (`crates/custodian/src/restore.rs:257`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer must decide whether the partial-report, fleet-wide mark withholding, attributed drain status, and non-zero CLI outcome are operationally fit for restore/decommission workflows—this determines whether operators can safely act on the new INCOMPLETE result. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Rebuild must emit first-read attribution before any later fallible read and add a regression forcing an intervening metadata-scan failure—otherwise a restore error can still erase the operator's route to the damaged record (`crates/custodian/src/restore.rs:257`).
- [ ] Validation — fitness-to-purpose — Maintainer must decide whether the partial-report, fleet-wide mark withholding, attributed drain status, and non-zero CLI outcome are operationally fit for restore/decommission workflows—this determines whether operators can safely act on the new INCOMPLETE result.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b

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
- Iteration delta (if iterating): Rebuild on the same brief; three findings from this round to address: 1. T3/T5 (blocking): attribution-order gap in reconcile_after_restore — an unresolvable object discovered while building `referenced` (restore.rs:257) is not written to the audit seam until after orphan_leases/pending_chunks/committed_chunks all succeed (restore.rs:272). If any of those later reads hits a genuine, unrelated store fault, the whole call returns Err and the already-known damaged-object name never reaches the operator. Brief explicitly cites gc.rs:155-165 as the pattern to mirror: emit attribution per object, before the fleet walk, so a later transient fault cannot cost the operator the record's name. Emit the already-known unresolvable name as soon as it is discovered (right after the `referenced` read), not batched with the later reads. Add a regression that forces an intervening read (orphan_leases/pending_chunks/ committed_chunks) to fail after an unresolvable object was already found, and assert the record's name still reached the audit trail. 2. T4 gate (blocking, likely mechanical): review-batch.md flags restore.rs:521 (`resolve_chunk_map(...).await` lacking a caller-side timeout) as a new blocking CONVENTION finding. This is the same concern already rejected 3x under the standing rule in review-rejected.md ("the MetadataStore implementation owns its own network bound, not the caller") at the old line numbers (:318, :330) — the code moved and the rejection was not re-recorded at the new line. Re-file the standing rejection at restore.rs:521 in review-rejected.md so the gate clears, unless the rebuild's line numbers shift again in which case re-locate it there. 3. Validation/fitness-to-purpose NEEDS-HUMAN was raised but not the reason for iterating — revisit at the next sign-off once (1) is fixed; not a rebuild item on its own.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
