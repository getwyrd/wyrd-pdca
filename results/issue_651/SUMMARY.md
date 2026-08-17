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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 38 mutants tested in 2m: 24 caught, 14 unviable

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

Reviewing issue #651: make restore and drain reconciliation contain and attribute unreadable chunk maps without certifying an incomplete reading or licensing unsafe marks.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision boundary is falsifiable: contain per-object read failures, attribute both completeness surfaces, keep marks/report coherent, and derive automation status from the report. |
| C2 Reproduction (red pre-fix) | PASS | On base `d50f0ca`, all five discriminator cases assertion-failed on the old behavior without relying on patch-only symbols; the same cases are grounded at `crates/custodian/tests/segmented_map_restore.rs:461`. |
| C3 Change | PASS | The scoped behavior is implemented at the reporting, drain-status, and operator boundaries without reintroducing dropped ambiguity machinery (`crates/custodian/src/restore.rs:265`, `crates/custodian/src/desired_state.rs:181`, `crates/server/src/cli.rs:1256`). |
| C4 Verification (red→green) | PASS | Fresh scratch reruns produced 0/5 red then 5/5 green, and typos/docs/fmt/clippy/build/tests/deny/machete/conformance/statics plus the 50-seed DST campaign passed (`crates/dst/tests/custodian.rs:2217`). |
| C5 Causal adequacy | PASS | The cause is reconciled rather than probed around: either read's hole withholds all marks and second-read-only references are protected before marking (`crates/custodian/src/restore.rs:283`, `crates/custodian/src/restore.rs:305`, `crates/custodian/src/restore.rs:364`). |
| T1 Structure | PASS | The change stays on existing custodian/server/DST seams and reuses the shared chunk-map resolver; no dependency, clock, unsafe, or shared-global boundary is added (`crates/custodian/src/restore.rs:590`). |
| T2 Shape | FAIL | The settled eight-file/STOP allocation remains binding after required DST coverage, but the patch touches nine paths; keeping both the DST addition and optional runtime-view expansion requires dropping one path or returning to Plan (`crates/dst/tests/custodian.rs:1727`, `docs/design/architecture/06-runtime-view.md:31`). |
| T3 Runtime | PASS | The seeded simulator demonstrably reaches the two-reading divergence window and the operator exit follows the report's human-action verdict (`crates/dst/tests/custodian.rs:2111`, `crates/server/src/cli.rs:1196`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect the reported one batch-review blocker and confirm closed/rejected prior art by affected path — `scripts/review-branch --bundle`, `scripts/pdca contribcheck`, their logs, and closed-work state are absent here, so the supplied red/pass claims remain provisional despite rechecking merged path history. |
| T5 Judgment | PASS | Code read, assertion red→green, 38-mutant coverage, and seeded DST support the behavioral judgment; the remaining defect is the mechanical path allocation, not an unresolved causal or architectural choice (`crates/custodian/src/restore.rs:220`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether partial-but-attributed restore/drain answers and non-zero automation semantics are fit for real operations — deterministic evidence proves the mechanics, not the operational policy (`docs/design/architecture/m4-first-deployment-blueprint.md:596`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Human must inspect the reported one batch-review blocker and confirm closed/rejected prior art by affected path — `scripts/review-branch --bundle`, `scripts/pdca contribcheck`, their logs, and closed-work state are absent here, so the supplied red/pass claims remain provisional despite rechecking merged path history.
- [x] Validation — fitness-to-purpose — Maintainers must decide whether partial-but-attributed restore/drain answers and non-zero automation semantics are fit for real operations — deterministic evidence proves the mechanics, not the operational policy (`docs/design/architecture/m4-first-deployment-blueprint.md:596`).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- [x] size backstop — this slice is behaving oversized: patch is 141 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: discontinued
- Iteration delta (if iterating):
- By / date: unknown / 2026-08-16

## 10. Act candidates (hints for the next Act review)
- File a separate bug (0.1 Alpha milestone): `crates/custodian/src/restore.rs`'s `committed_chunks` silently skips a malformed committed placement with no report field and no effect on `is_clean()` — pre-existing base behavior (unchanged by this patch), out of scope for #651; T4 finding at `restore.rs:668` recorded-rejected on this basis rather than fixed here.
