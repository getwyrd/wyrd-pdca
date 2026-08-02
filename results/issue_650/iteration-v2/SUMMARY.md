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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 19 mutants tested in 14s: 4 caught, 15 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make GC and scrub resolve segmented chunk maps, fail closed without over-certifying incomplete reference sets, and continue past per-object damage.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The core contract is falsifiable through positive byte survival, non-certifying outcomes, continued healthy-object verification, and propagated store faults at `crates/custodian/tests/segmented_map_consumers.rs:419`, `crates/custodian/tests/segmented_map_consumers.rs:519`, `crates/custodian/tests/segmented_map_consumers.rs:638`, and `crates/custodian/tests/segmented_map_consumers.rs:817`. |
| C2 Reproduction (red pre-fix) | PASS | On the #649 base the added test target compiled and all 6 tests failed at runtime, including the positive segmented-object leg at `crates/custodian/tests/segmented_map_consumers.rs:419`; this was assertion/behavior red, not compile red. |
| C3 Change | NEEDS-HUMAN | Plan must decide whether drain-status attribution lands in #650 or remains deferred to #651 — the patch adds the public `PendingUnresolvable` contract at `crates/custodian/src/desired_state.rs:102` and a rebalance-surface test at `crates/custodian/tests/rebalance.rs:1498`, while the brief both excludes those surfaces and asks docs to promise the report-only behavior. |
| C4 Verification (red→green) | PASS | The same 6 tests passed patched, as did 34 focused tests, typos, docs lint/render, fmt, clippy, build, workspace tests, machete, all three deny walls, conformance, statics, 50-seed DST including `crates/dst/tests/custodian.rs:1732`, and 19 diff mutants; the wrapper-only advisory-lock red was a read-only-host caveat discharged with scratch-local cargo state. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must contain a root that becomes unreadable during resolver restart — a driven valid-root→unreadable-root race returned `Err(Store(...))` because `crates/core/src/metadata.rs:2517` emits a raw decode error that `crates/custodian/src/gc.rs:389` propagates instead of attributing to the object. |
| T1 Structure | PASS | The shared core resolver remains the single map-resolution seam at `crates/custodian/src/gc.rs:386`, while outcome precedence is centralized at `crates/custodian/src/reconciliation.rs:55`; no concrete-backend dependency was introduced. |
| T2 Shape | PASS | The diff is 11 files and approximately 1,016 added nonblank, noncomment semantic lines, within the 15-file and 1,500-line limits; the outcome aggregation stays localized at `crates/custodian/src/reconciliation.rs:47`. |
| T3 Runtime | FAIL | A concurrent supersede to unreadable root bytes aborts the whole maintenance step at `crates/custodian/src/gc.rs:389` instead of returning `Blocked`, so the promised per-object runtime containment is incomplete. |
| T4 Contribution | NEEDS-HUMAN | Contribution-review disposition is owed — affected-path merged and closed/unmerged history was independently checked and found #647 as the sole functionally relevant unmerged prior art, but the named `scripts/review-branch` and `scripts/pdca` tools are absent, so the reported 8 blocking findings and contribution-check result cannot be reproduced. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make the reverse-order aggregation test genuinely exercise `Changed`→`Blocked` — an explicit fixture check returned `Satisfied` for the supposed converging GC context at `crates/custodian/tests/segmented_map_consumers.rs:947`, so that leg currently proves only `Satisfied`→`Blocked`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must accept the operational policy of freezing reclamation fleet-wide while exposing a new blocked drain state — this trades cleanup availability for C-1 durability at `crates/custodian/src/gc.rs:304` and `crates/custodian/src/desired_state.rs:124`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Plan must decide whether drain-status attribution lands in #650 or remains deferred to #651 — the patch adds the public `PendingUnresolvable` contract at `crates/custodian/src/desired_state.rs:102` and a rebalance-surface test at `crates/custodian/tests/rebalance.rs:1498`, while the brief both excludes those surfaces and asks docs to promise the report-only behavior.
- [ ] C5 Causal adequacy — Rebuild must contain a root that becomes unreadable during resolver restart — a driven valid-root→unreadable-root race returned `Err(Store(...))` because `crates/core/src/metadata.rs:2517` emits a raw decode error that `crates/custodian/src/gc.rs:389` propagates instead of attributing to the object.
- [ ] T4 Contribution — Contribution-review disposition is owed — affected-path merged and closed/unmerged history was independently checked and found #647 as the sole functionally relevant unmerged prior art, but the named `scripts/review-branch` and `scripts/pdca` tools are absent, so the reported 8 blocking findings and contribution-check result cannot be reproduced.
- [ ] T5 Judgment — Rebuild must make the reverse-order aggregation test genuinely exercise `Changed`→`Blocked` — an explicit fixture check returned `Satisfied` for the supposed converging GC context at `crates/custodian/tests/segmented_map_consumers.rs:947`, so that leg currently proves only `Satisfied`→`Blocked`.
- [ ] Validation — fitness-to-purpose — Maintainers must accept the operational policy of freezing reclamation fleet-wide while exposing a new blocked drain state — this trades cleanup availability for C-1 durability at `crates/custodian/src/gc.rs:304` and `crates/custodian/src/desired_state.rs:124`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must contain a root that becomes unreadable during resolver restart — a driven valid-root→unreadable-root race returned `Err(Store(...))` because `crates/core/src/metadata.rs:2517` emits a raw decode error that `crates/custodian/src/gc.rs:389` propagates instead of attributing to the object.; T4 Contribution — Contribution-review disposition is owed — affected-path merged and closed/unmerged history was independently checked and found #647 as the sole functionally relevant unmerged prior art, but the named `scripts/review-branch` and `scripts/pdca` tools are absent, so the reported 8 blocking findings and contribution-check result cannot be reproduced.; T5 Judgment — Rebuild must make the reverse-order aggregation test genuinely exercise `Changed`→`Blocked` — an explicit fixture check returned `Satisfied` for the supposed converging GC context at `crates/custodian/tests/segmented_map_consumers.rs:947`, so that leg currently proves only `Satisfied`→`Blocked`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
