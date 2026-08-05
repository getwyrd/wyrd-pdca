# Result — issue 651 / restore-and-desired-state-contained-and-attributed

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The two operator-facing surfaces that report **whether a reconciliation is
  complete** cannot survive — let alone describe — an object whose chunk map they could not
  read. #650 built the containment they need (`gc::ReferenceSet`) and deferred both here **by
  name, in its own code**.
  1. **Post-restore reconciliation fails the whole pass closed.** `reconcile_after_restore`'s
     *mark* half is already safe — it withholds every fragment while the reference set is
     incomplete (`gc::ReferenceSet::protects`, `restore.rs:239`) — but its *report* half re-reads
     every committed record through `committed_chunks` (called at `restore.rs:326`), which `?`s
     out on `ChunkMap::Segmented` (`restore.rs:390`, `:403-405`). So a store holding **one**
     segmented object, or one structurally
     unreadable committed root, returns `Err` and the operator command produces **no report at
     all**: not a stranded count, not the dangling/misplaced chunks of the objects it *could*
     read. One damaged object blanks the whole answer. #650 states the gap at `restore.rs:196`:
     *"deferred: #651 — the **contained** answer for this surface (report every object it could
     read, name the one it could not, and say the run is not certified …) belongs to the slice
     that owns restore."*
  2. **The drain surface cannot distinguish "not converged" from "I could not look".**
     `reconciliation_status` answers a bare `Pending` when the reference set is incomplete
     (`desired_state.rs:188-190`) — the same word it uses for a server that genuinely still holds
     referenced fragments. An operator watching a decommission stall has no way to learn *which*
     record is blocking it, so the stall is a state nothing exits. The tree already has the
     opposite pattern one level up: a malformed placement answers `PendingMalformed { chunks }`,
     naming the blockers in the answer itself (`desired_state.rs:101-104`, from merged #397).
     #650 states the gap at `desired_state.rs:183-187`: *"deferred: #651 — the **ATTRIBUTED**
     answer for this surface … lands with the slice that owns `desired_state`."*
  3. **The operator surface would print a hollow green.** `wyrd custodian` renders the report and
     exits non-zero only on `dangling` / `misplaced` (`crates/server/src/cli.rs:1196-1236`). An
     incomplete reading has no cell in that summary and no effect on the exit code — so once (1)
     stops erroring, a restore script checking the status code would record a run that could not
     read part of the store as a healthy one. That is precisely the failure mode the existing
     comment at `cli.rs:1230-1233` refuses for lost data — it even names "one whose chunks cannot
     be read", while the `if` below it at `:1234` does not test for that case.
- Success criterion: The added test target `crates/custodian/tests/segmented_map_restore.rs`
  passes, driven **only** through entries already visible on this slice's base
  (`wyrd_custodian::{reconcile_after_restore, RestoreReport, GcContext}`,
  `wyrd_custodian::desired_state::{reconciliation_status, ReconciliationStatus}`):
  1. **A segmented object no longer stops the pass.** `reconcile_after_restore` over a store
     seeded with a segmented object — raw `seg:` records plus a segmented root, **never** a
     committer — returns `Ok` (today: `Err`), with `RestoreReport::stranded_marked == 0`, and
     every fragment that object owns is still present on its D server afterwards.
  2. **A damaged object is contained, and the run is not certified.** Two scenarios, because one
     alone is passable vacuously:
     - **(2a) non-certification, with the incomplete reading as the SOLE cause.** One committed
       object whose chunk map cannot be read, in an otherwise **fully healthy** store — nothing
       dangling, nothing misplaced, nothing under-replicated, nothing to mark. The pass returns
       `Ok`, marks nothing (`stranded_marked == 0`), and `report.is_clean()` is **false**. Note
       `is_clean()` (`restore.rs:144`) is already false whenever any loss is reported, so a
       scenario carrying a loss would satisfy this clause **without the fix** — it must be
       asserted on a store where the unreadable object is the only reason.
     - **(2b) containment — the damaged object does not starve the healthy ones.** The same
       unreadable object seeded **beside** a readable object that has a genuine loss: the pass
       still returns `Ok` and still reports that readable object's loss (`dangling` or
       `misplaced` names its chunk), and still marks nothing of the unreadable one.
  3. **The drain surface tells the two Pendings apart.** `reconciliation_status` over that same
     store attributes the blocking object — the operator can name the record to repair — instead
     of the unattributed `Pending` the base answers. Assert this on the **audit/tracing seam**,
     the way #650's own fixture does (`assert_attributes_blocker`), so this leg needs no symbol
     the base lacks.

  Criterion (2) is the binding one — it is the whole point of the slice, and the one an
  incomplete fix passes vacuously. **Both** (2a) and (2b) must ship: (2a) alone does not show the
  walk continues, and (2b) alone does not show the run is non-certifying. A version that only
  checks the call returned `Ok` proves neither.
- Repo + branch target: getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`, matching the sandbox's
  `origin/main`. INTEGRATION §2's default — a milestone integration branch was considered and
  **dropped**: #648 landed on `main` directly (PR #672), so keeping the rest off `main` bought
  nothing, and each slice is individually gated and self-consistent. **Not**
  `pdca-integration/main`: that is the driver's run-scoped wave-fold branch, regenerated and
  force-pushed per run, and it silently outranks this field via the bundle's `stack-base` marker
  — which is why v5 built on a tree that had lost #650. That marker has been removed from this
  bundle.)
- Scope (one logical fix) / out of scope: make both completeness-reporting surfaces answer **contained and attributed** over a
  reference set that has a hole in it, instead of erroring or certifying.
  `crates/custodian/src/restore.rs` — the report half survives an object it cannot read: it
  reports every object it *could* read, records the ones it could not, and the pass is not
  clean. `crates/custodian/src/desired_state.rs` — the drain status distinguishes "still holds
  referenced fragments" from "could not read object X", and names X. `crates/server/src/cli.rs`
  (and `custodian.rs` if the report shape reaches it) — the operator summary states an
  incomplete reading and the command's exit status reflects it. Plus their existing test files,
  the added discriminator below, and the docs-currency paragraph.
  **Out of scope:** reconstruction, backfill and rebalance, and the resolving namespace walk they
  need (**#681**) — this slice adds **no** custodian-level walk and no `crate::resolve` module,
  and reads the reference set through `gc::referenced_fragments` exactly as the base does;
  `repoint_chunk` and the record ceilings (**#682**) — this slice writes **nothing** to a chunk
  map; the chunk-id floor (#652); the committer, fence, rollback and resume (#653); any
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 24 mutants tested in 8m: 7 missed, 3 caught, 14 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '-p', 'MemoryHigh=8G', '-p', 'MemoryMax=16G', '-p', 'MemorySwapMax=0', '-p', 'OOMPolicy=continue', '--', 'codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '-m', 'gpt-5.6-sol', '-c', 'model_reasoning_effort=xhigh', '--add-dir', '/home/eddie/wyrd/wyrd', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] leaf produced no usable verdict (needs a human) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
