# Result — issue 650 / gc-scrub-through-resolver-fail-closed-containment

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  Two defects in the **reference set** GC and scrub both gate on.
  1. **It cannot see a segmented object's chunks.** `referenced_fragments` builds the protected
     set from each committed record's inline chunk list (`crates/custodian/src/gc.rs:251` on
     `origin/main`), so a segmented object's fragments are simply **absent from it** — and GC's
     safety gate (`gc.rs:159`; the loop's load-bearing invariant, whose violation is named
     *silent corruption*, at `gc.rs:22-25`) would pass them to `delete_fragment`. Scrub reads the
     same set (`scrub.rs:43`,`:75`) and would never verify them while still reporting the store
     clean.
  2. **An incomplete reference set still certifies — the finding still open at #647's close.** In
     the closed PR `ReferenceSet::protects` short-circuits `true` while the set is incomplete
     (correct for *reclamation*, and why nothing is deleted), but `gc_step` then audits **every**
     otherwise-unprotected fleet fragment as `skip reason="referenced"` and still returns
     `Reconciled::Satisfied` — it reports the store converged while holding a set it knows is
     partial. Scrub's twin **was** fixed in #647 (it returns `Blocked` for the identical
     condition); the GC side was not. Two passes reading one set answer differently about the
     same store.
- Success criterion:
  The added test target `crates/custodian/tests/segmented_map_consumers.rs`
  passes and binds the issue's acceptance, driven through the real fenced control point
  `wyrd_custodian::reconcile_step` (base-visible):
  1. **A segmented object's fragments survive GC + scrub, asserted positively.** After a GC pass
     **past the grace window** and a scrub pass over an object seeded as raw `seg:` records + a
     segmented root, every fragment it owns is still present on every D server that held it, **and**
     a drain of a server holding one answers `ReconciliationStatus::Pending` — the positive
     observable, because "GC deleted nothing" also passes when GC did nothing at all.
  2. **With one unresolvable inode, GC and scrub never certify and reclaim nothing.** Both steps
     return `Ok(_)` and **not** `Reconciled::Satisfied`, and nothing of that object is reclaimed;
     the blocker is attributed on the audit seam naming the object. (The positive
     `Reconciled::Blocked` match ships in the appended `crates/custodian/tests/{gc,scrub}.rs`
     legs, which `C4-ci` runs — see *Test file* for why the discriminator asserts the
     base-expressible half.)
  3. **One damaged object does not end the walk** — with that object present the reference build
     still completes over the rest of the store, a healthy object's fragments are still protected
     and still verified. A store-access fault (not one object's fault) still propagates.
- Repo + branch target:
  getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- Scope (one logical fix) / out of scope:
  route the **reference build** through the resolver and make **certification** honest
  in both passes that read it. `crates/custodian/src/gc.rs` — the reference build resolves a
  committed map through #649's resolver by bounded `seg:` ranges; `ReferenceSet` carries the
  objects it could **not** resolve; `gc_step`'s outcome reflects that incompleteness instead of
  reporting convergence. `crates/custodian/src/scrub.rs` — inherits the same set and the same
  outcome rule; the two passes' answers must be identical for identical input.
  `crates/custodian/src/reconciliation.rs` — the outcome type gains the "cannot certify" answer
  and the rule for combining outcomes across loops. Plus their existing test files and the added
  fixture. **Caller-first:** every production symbol introduced here has a caller **in this
  slice** — the incompleteness field is read by `protects` and by both steps' outcome; this slice
  lands no behaviour flip and no producer of segmented maps. **Out of scope:** restore,
  reconstruction, backfill, rebalance and `desired_state` (#651) — do not route them here even
  though they will share this fixture; the record-ceiling helpers and `repoint_chunk` (#651); the
  chunk-id floor (#652); the committer (#653); any new/edited ADR / spec / proposal; any
  conformance-vector change.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 30 mutants tested in 31s: 6 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #650: make GC and scrub resolve segmented chunk maps, fail closed on incomplete reference sets, and contain per-object map faults without falsely certifying the store.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Ownership must be decided: the brief requires a report-only blocker that names the object while also reserving `desired_state` attribution to #651, so acceptance depends on which scope statement controls. |
| C2 Reproduction (red pre-fix) | FAIL | The documented pre-fix leg is compile-red, not assertion-red: reverting only GC, scrub, and reconciliation leaves `crates/custodian/src/desired_state.rs:188` reading the reverted-away `ReferenceSet::unresolvable` field (E0609), so the claimed behavior is never exercised. |
| C3 Change | NEEDS-HUMAN | Scope disposition is owed: the candidate returns generic `Pending` yet explicitly defers attributed drain status to #651 at `crates/custodian/src/desired_state.rs:183`; decide whether this slice should omit, retain, or complete that surface because operator repairability depends on it. |
| C4 Verification (red→green) | FAIL | Green is independently confirmed by 8/8 focused tests, the complete `cargo xtask ci`, and 30 diff mutants, but the required assertion-red is absent because the pre-fix build stops at `crates/custodian/src/desired_state.rs:188`. |
| C5 Causal adequacy | PASS | The protected-set builder resolves each committed map and distinguishes typed object faults from store faults at `crates/custodian/src/gc.rs:402`, while an incomplete set withholds reclamation at `crates/custodian/src/gc.rs:311`, addressing the causal reference gap without a capability probe. |
| T1 Structure | PASS | The 12-file change is within the 15-file limit and approximately 1,284 added non-comment semantic lines are within budget; the shared outcome contract is centralized at `crates/custodian/src/reconciliation.rs:20`. |
| T2 Shape | PASS | `Reconciled::Blocked` makes non-certification explicit and least-certified aggregation preserves it across loop order at `crates/custodian/src/reconciliation.rs:55`, avoiding silent-success and absent-entry rubric defects. |
| T3 Runtime | PASS | The patched target passes the focused eight-test suite and the full gate, including the 50-seed DST property rooted at `crates/dst/tests/custodian.rs:1775`; typos, docs render, deny, conformance, statics, and workspace tests all ran. |
| T4 Contribution | NEEDS-HUMAN | Contribution disposition is owed: affected-path merged/open/closed history found prerequisite PRs #672/#675 and closed predecessor #647, but the asserted `scripts/review-branch --bundle` and `scripts/pdca contribcheck` tools/artifacts are absent, so their pass rows cannot be independently reproduced. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must restore falsifiable pre-fix coverage: the discriminator's stated red leg fails compilation at `crates/custodian/src/desired_state.rs:188`, so it does not test the behavior it claims. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Operational sign-off is owed: decide whether fleet-wide reclamation stall plus generic drain `Pending` until repair is acceptable, because `crates/custodian/src/gc.rs:311` preserves bytes at the cost of unbounded garbage retention and weaker status attribution. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C1 Spec — Ownership must be decided: the brief requires a report-only blocker that names the object while also reserving `desired_state` attribution to #651, so acceptance depends on which scope statement controls. — CLEARED at sign-off (human): the narrow reading controls — attribution/naming the blocked object belongs to #651; this slice is scaffolding and only needs to fail closed (never falsely certify) without adding new public API in `desired_state`/`restore`, matching what's actually shipped (generic `Pending` + `// deferred: #651` marker).
- [x] C3 Change — Scope disposition is owed: the candidate returns generic `Pending` yet explicitly defers attributed drain status to #651 at `crates/custodian/src/desired_state.rs:183`; decide whether this slice should omit, retain, or complete that surface because operator repairability depends on it. — CLEARED at sign-off (human): same decision as C1 above — omit the attributed surface here, retain only the non-certifying guard; attribution is #651's to complete.
- [x] T4 Contribution — Contribution disposition is owed: affected-path merged/open/closed history found prerequisite PRs #672/#675 and closed predecessor #647, but the asserted `scripts/review-branch --bundle` and `scripts/pdca contribcheck` tools/artifacts are absent, so their pass rows cannot be independently reproduced. — CLEARED at sign-off (human): PDCA-side tooling unreachable from the reviewer's artifact-only sandbox, not a patch defect; affected-path history independently re-run with plain `gh` and quoted in build-notes §1.
- [x] T5 Judgment — Rebuild must restore falsifiable pre-fix coverage: the discriminator's stated red leg fails compilation at `crates/custodian/src/desired_state.rs:188`, so it does not test the behavior it claims. — CLEARED at sign-off: reviewer error. `run-verify.sh`'s RED leg reverts every patch-touched production file (including `desired_state.rs`), not a hand-picked subset, so no orphaned field reference / compile error is possible; gate output (build-notes §5a) shows 8 real assertion/`expect` panics.
- [x] Validation — fitness-to-purpose — Operational sign-off is owed: decide whether fleet-wide reclamation stall plus generic drain `Pending` until repair is acceptable, because `crates/custodian/src/gc.rs:311` preserves bytes at the cost of unbounded garbage retention and weaker status attribution. — CLEARED at sign-off (human): accepted — leak-until-repaired (never reclaiming a byte while any object is unresolvable) is the correct trade against silently deleting live data; matches the brief's stated invariant.
- [x] C3 Change — Plan must decide whether drain-status attribution lands in #650 or remains deferred to #651 — the patch adds the public `PendingUnresolvable` contract at `crates/custodian/src/desired_state.rs:102` and a rebalance-surface test at `crates/custodian/tests/rebalance.rs:1498`, while the brief both excludes those surfaces and asks docs to promise the report-only behavior. — CLEARED at sign-off: stale, carried from iteration 3. Iteration 4 removed `PendingUnresolvable` and the `rebalance.rs` leg; verified against this bundle's `patch.diff` — neither string nor file appears anywhere in it.
- [x] C3 Change — Scope ownership must be decided — `PendingUnresolvable` and its rebalance coverage pull the brief's explicitly deferred desired-state surface into #650, changing the public status API and #651 boundary (`crates/custodian/src/desired_state.rs:102`). — CLEARED at sign-off: stale, same reason as above — verified not present in `patch.diff`.
- [x] C1 Spec — Landing order is owed — issue #650 consumes #649's resolver at `crates/custodian/src/gc.rs:402`, while prerequisite issues #648/#649 and PRs #672/#675 remain open; sign-off must keep this patch stacked until they land on `main`. — CLEARED at sign-off (human): acceptable — the harness's wave-stacking mechanism (`stack-base` file present in this bundle; publish opens the PR with `--base` set to the folded integration branch, not bare `main`) structurally flags and enforces the landing order on the PR itself; no `Onto branch` override needed.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
