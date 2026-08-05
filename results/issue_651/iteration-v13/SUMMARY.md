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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #651: contain and attribute unreadable chunk maps in restore/drain reporting while preserving fail-closed marking and an actionable non-zero operator verdict.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the observable safety outcomes, ordering, scope boundary, and base-visible discriminator, so the required implementation and operator decisions are unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | All five discriminator tests execute and fail against the base behavior, then pass with the patch; the cases begin at `crates/custodian/tests/segmented_map_restore.rs:461`. |
| C3 Change | PASS | The patch stays on the allocated restore, drain-status, CLI, test, and living-doc surfaces, and does not reintroduce the expressly dropped cross-object ambiguity apparatus. |
| C4 Verification (red→green) | PASS | Independent replay produced 5/5 red then 5/5 green, a clean `cargo xtask ci`, and 38/38 non-surviving mutants for the changed code; the discriminator's one-reading assertion is at `crates/custodian/tests/segmented_map_restore.rs:678`. |
| C5 Causal adequacy | PASS | Per-object faults are contained and both namespace readings are reconciled before any mark is authorized, addressing the whole-pass error and split-reading cause without a capability probe or symptom guard (`crates/custodian/src/restore.rs:277`, `crates/custodian/src/restore.rs:293`, `crates/custodian/src/restore.rs:358`). |
| T1 Structure | PASS | The change uses exactly the eight-file allocation and the added integration-test crate root carries the required unsafe prohibition (`crates/custodian/tests/segmented_map_restore.rs:48`); the iteration-12 size backstop was explicitly settled by the human. |
| T2 Shape | FAIL | The operator-facing dangling and misplaced paragraphs collapse existing chunk-ID vectors to counts and require an audit-log lookup, so their output shape omits the identifiers needed for repair (`crates/server/src/cli.rs:1282`, `crates/server/src/cli.rs:1290`). |
| T3 Runtime | FAIL | A terminal-only restore operator cannot identify which lost or misplaced chunks require action when the log collector is unavailable, despite the same actionability rule being enforced for unreadable records (`crates/server/src/cli.rs:1284`, `crates/server/src/cli.rs:1292`). |
| T4 Contribution | FAIL | Contribution artifacts and affected-path prior art are complete, but the red rubric result's two remaining blockers are independently grounded in the count-only dangling and misplaced output (`crates/server/src/cli.rs:1282`, `crates/server/src/cli.rs:1290`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must render each dangling/misplaced chunk ID inline and assert both identifiers; the current test creates IDs 1 and 2 but checks only generic status/text coupling, so the omission can recur (`crates/server/src/cli.rs:2720`, `crates/server/src/cli.rs:2755`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the rebuilt terminal report plus non-zero status is actionable enough to operate restore/decommission safely without depending on an audit collector — this is the production fitness boundary. |

Re-run notes:

- Target state was readable at `d50f0ca`, and reverse-apply checking confirmed that `patch.diff` exactly matches the applied target state.
- The base replay ran all five added tests and all five failed on behavior (not missing symbols); the patched replay ran the same five and all passed.
- `cargo xtask ci` completed with all checks passed after relocating Cargo's advisory-database lock to the named writable scratch directory; `typos`, docs lint/render, `cargo-machete`, and `cargo deny check` were all genuinely exercised. The first read-only Cargo-home lock error was a host caveat, not a patch defect.
- Mutation replay selected the asserted 38 diff mutants and completed with 24 caught and 14 unviable, with no survivor.
- Affected-path prior art was checked across merged `origin/main` history, all closed/unmerged pull requests, and all open pull requests: PR #647 is the sole closed/unmerged predecessor touching these files, and no open pull request overlaps them.
- The external `scripts/review-branch --bundle` orchestrator was not present in the bundle or target and could not be rerun; its two underlying source defects were nevertheless independently confirmed above rather than copied from the asserted gate result.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Rebuild must render each dangling/misplaced chunk ID inline and assert both identifiers; the current test creates IDs 1 and 2 but checks only generic status/text coupling, so the omission can recur (`crates/server/src/cli.rs:2720`, `crates/server/src/cli.rs:2755`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the rebuilt terminal report plus non-zero status is actionable enough to operate restore/decommission safely without depending on an audit collector — this is the production fitness boundary.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 118 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Not accepted yet, but the slice is sound and close. All correctness gates are green (C4-ci pass, C4-verify red->green 5/5, C5 38 mutants with no survivor), C5 causal adequacy and T1 both PASS. One gating item remains: T4. WHAT TO FIX — the only blocking work in this round: - The two T4 blockers in `review-batch.md` are both TEST-GAP: the new concurrent two-scan marking path can authorize GC-visible orphan marks but carries only Tokio / in-memory interleaving coverage, which does not satisfy the rule that new destructive or concurrent paths ship seeded Tier-0 DST coverage. Sites: `crates/custodian/src/restore.rs:296` and `crates/custodian/src/restore.rs:585`. - Add seeded Tier-0 DST coverage for that path. It must exercise the interleaving the Tokio doubles only approximate: the two namespace readings disagreeing, and the marking decision that rests on their reconciliation. The property to pin is the brief's own one-reading rule — no fragment is marked while any reading in the pass found a record it could not read. - Note the brief currently states "No DST leg" under External dependencies. That line is now stale against the shipped design: the two-scan path did not exist when it was written. Adding the DST leg is the correct resolution; the brief's Scope and Success criterion are otherwise unchanged and still binding. WHAT NOT TO CHASE — do not spend the round on these: - The advisory reviewer's T2 / T3 / T5 findings against `crates/server/src/cli.rs` (count-only `dangling` / `misplaced` paragraphs, "See the audit log for each chunk id", cli.rs:1282/1284/1290/1292, tests at :2720/:2755) are NOT defects in this patch. That text is verbatim pre-existing on `origin/main` — confirmed by reading `git show origin/main:crates/server/src/cli.rs`. The patch preserved the base wording and inserted the new UNREADABLE paragraph beside it. Report-schema churn is also explicitly out of scope in the brief. Record-reject these in `review-rejected.md` rather than rebuilding the CLI output shape. - The reviewer's T4 basis line attributes the two blockers to that same CLI text. It is wrong: the reviewer could not re-run `scripts/review-branch --bundle` (it says so in its own re-run notes) and substituted two self-found CLI findings for the gate's actual two because the counts matched. Work from `review-batch.md`, not from the reviewer's §5 table. - Do not reintroduce the cross-object chunk-id ambiguity rule. It was dropped at the v10 replan and must stay dropped. SIZE — not a re-slice trigger this round: The backstop fired at 121 KB / 2 rounds and pre-recommends `iterate-plan`. Declined deliberately. The v10 replan (3rd) already did its job: patch 159 KB -> 99 KB, C5 converted from chronically failing to reliably passing, T4 blockers 3 -> 1. Production code is ~708 added lines against the brief's 700-line budget — on budget; the byte count is dominated by ~1,000 lines of discriminator and CI-gated tests, which is the shape we want. Keep the 8-file allocation. Expect the DST leg to add test bytes; that is not overage in the sense the backstop means.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Reviewer leaf must hard-fail, not narrate, when a gate it is asked to corroborate cannot be re-run: here `scripts/review-branch --bundle` was unavailable, and the reviewer substituted two self-found CLI findings for the gate's two actual blockers (Tier-0 DST gaps in `review-batch.md`) because the counts matched.
- §6 evidence strings are truncated mid-path (`-> .../results/issue_651/review-b`), hiding the findings file exactly when it matters: the gating T4 item never names `review-batch.md`, so the real blockers are unreadable from the summary a human signs off on.
- Reviewer verdicts on changed operator-facing output must be taken against a base diff: T2/T3/T5 here charged this patch with `cli.rs` count-only dangling/misplaced text that is verbatim pre-existing on `origin/main`, and the one error propagated into four verdicts, reading as convergence.
- Sign-off leaf's read-set should include `review-batch.md` and `size-signal.json`, not just `SUMMARY.md` / `check-gates.md` / `check-review.md`: reading only the summary artifacts is compliant as prompted today, yet the primary artifacts contradicted the summary on both the actual blockers and the replan count.
