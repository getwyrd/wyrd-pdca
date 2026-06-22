# Result — issue 145 / m3.7-rebalance-drain-decommission

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: Demonstrable at C4-verify, in-process over the trait stores
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single line, no
- Scope (one logical fix) / out of scope: the rebalance loop and its declarative drain/decommission hook plus the

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
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 145 / m3.7-rebalance-drain-decommission

> **Advisory, artifact-only, decorrelated.** Inputs: `patch.diff`, `brief.md`,
> `check-gates.json` (build-notes.md deliberately withheld). Citations re-derived
> against the read-only target `$PDCA_TARGET = /home/eddie/wyrd/wyrd`.

## Standing observation that colours several rows

The provided target is `main` **without #144 (reconstruction, slice 6)**:
`crates/custodian/src/lib.rs:23-27` has no `reconstruction` module, and
`crates/custodian/src/reconciliation.rs:60-66` declares `reconcile_step` with only
`gc`/`scrub` (no `reconstruction`, no `rebalance`). The patch's base clearly already
contains #144 (its `reconciliation.rs` hunk treats `reconstruction:` as an *unchanged
context* parameter; it edits `tests/reconstruction.rs`, a file absent on target).

Consequently the patch references three symbols that **do not exist on the bare target
and are not introduced by this patch** — they must come from #144:

- `wyrd_core::placement::select_distinct_domains_excluding` — target `placement.rs`
  exposes only `select_distinct_domains` (`placement.rs:158`).
- `Topology::domain_of` — no such method on target `Topology` (`placement.rs:79-116`).
- `crate::gc::orphan_key` — **private** `fn orphan_key` on target (`gc.rs:45`), not
  reachable from `rebalance.rs` as written; the patch does not re-export it.

This is consistent with the brief's declared `Depends on: 144` ("must be COMPLETE
first"), so it is an **ordering/integration gate for the human**, not a defect of this
slice. But it means the patch as supplied does **not** compile on the target I can see,
and the C4 gates in `check-gates.json` were necessarily run on a different base
(`$PDCA_WORKTREE`, with #144). I take those gate results as reported but cannot
re-derive them on target.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Binding criteria + invariant are explicit and decidable: brief.md success criterion (1)-(3) (`brief.md:42-61`) and invariant (`brief.md:62-74`), pointing at accepted proposal 0005 (`0005:297-303`, `0005:346-356`, `0005:341-343`). |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | No gate configured for C2 (`check-gates.json` C2 result=`none`). C4-verify reports red→green, but whether the pre-fix red is a *demonstrated assertion-level* flip (brief.md:124-128 promises a temporary spread/desired-state negation) versus resting-red on a net-new file that simply fails to compile is recorded only in the **withheld** build-notes.md — cannot confirm from my artifacts. |
| C3 — C3 Change | PASS | Coherent change mapping 1:1 to the three legs: rebalance loop `crates/custodian/src/rebalance.rs` (evacuate via one version-conditional `commit`, patch ll.538-559), declarative hook `crates/custodian/src/desired_state.rs`, capacity emission `rebalance.rs::emit_domain_utilization` + `placement.rs::domain_utilization`, wired into the fenced seam at `reconciliation.rs` dispatch (patch ll.658-666). Caveat: not buildable on bare target (see standing observation). |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json` C4-ci=`pass` ("xtask ci: all checks passed", gating) and C4-verify=`pass` ("red without the fix, green with it"). Taken as reported; necessarily run in `$PDCA_WORKTREE` on a base incl. #144, which I cannot independently reproduce on target. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Oracle is "reviewer + human sign-off" (`check-gates.json`). The local logic re-derives soundly — `reconciliation_status` returns `Satisfied` iff no committed placement references the drained server (`desired_state.rs:243-257`), distinct from the `Pending` "changed" moment — but the load-bearing distinct-domain re-place rests on `select_distinct_domains_excluding`/`domain_of` (patch `rebalance.rs:342,458,500`), which are **unseen on target** (#144-provided). Causal chain unverifiable end-to-end here. |
| T1 — T1 Structure | PASS | Net-new `crates/custodian/tests/rebalance.rs` mirrors the sibling `tests/gc.rs`/`tests/scrub.rs`/`tests/reconstruction.rs` layout (in-mem trait stores + `elect` helper + `#[tokio::test]`), plus unit tests added in `placement.rs` (`domain_utilization_sums_per_domain`, `excluding_drops_servers_and_their_utilization`, patch ll.63-97). |
| T2 — T2 Shape | PASS | Assertions are behavioural, not tautological: exact-one version bump (`record.version == 2`), atomic placement flip to a distinct domain (`vec![0,3,2]`, 3 distinct domains), orphan record present, read-back of original bytes, spread-wins refusal (version untouched + `Pending`), and per-domain metric series exported (patch `tests/rebalance.rs` ll.1031-1216). |
| T3 — T3 Runtime | PASS | Tests drive the **real** fenced `reconcile_step` in-process (patch `tests/rebalance.rs:1028,1142,1197`), not a test-only entry; runtime success is asserted by C4-ci=`pass`. Same base caveat as C4. |
| T4 — T4 Contribution | PASS | Genuine net-new coverage for born-here behaviour (rebalance loop, drain/decommission desired-state surface, per-failure-domain capacity), matching the brief's NET-NEW posture (`brief.md:117-124`); no pre-existing rebalance test to overlap. |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle "reviewer + human sign-off". Concrete judgment gap to weigh: tests exercise only **single-fragment** evacuation; the multi-fragment evac path (`evac.len() > 1`) and the lost-CAS `EvacOutcome::Conflict`/`emit_conflict` branch (patch `rebalance.rs:413,561-617`) are unexercised, and the in-process `MemMeta`/`Fleet` fidelity to production stores is a human call. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Whether the Option-A in-process demonstration (no deployed custodian runtime; operator-facing API + CLI deferred to ADR-0013, `brief.md:132-140`) actually satisfies the operator's "mark a server draining and have it evacuated" purpose is a fitness judgment for sign-off. |

## §6 — Items the human must clear

1. **Ordering/integration gate (cross-cutting, highest priority).** Confirm #144
   (reconstruction, slice 6) has landed on `main` and that it supplies
   `select_distinct_domains_excluding`, `Topology::domain_of`, and a `pub(crate)`/`pub`
   `gc::orphan_key` with the contracts this patch assumes. On the target as given the
   patch does not compile. (Surfaced by C3/C4/C5/T3 caveats.)

2. **C2 — pre-fix red.** Read the withheld `build-notes.md` and confirm a *demonstrated
   assertion-level* red was captured (the brief's temporary spread/desired-state
   negation, `brief.md:124-128`) rather than a compile-level/absence red on the new file.

3. **C5 — causal adequacy.** Sign off that the evacuation truly reuses the
   reconstruction's commit-point-atomic, version-conditional re-place and that
   `select_distinct_domains_excluding`'s contract preserves `n` distinct domains for the
   `evac.len() > 1` case — the load-bearing machinery is unseen on target.

4. **T5 — test judgment.** Decide whether single-fragment-only evacuation coverage is
   sufficient, given the uncovered multi-fragment and lost-CAS `Conflict` branches.

5. **V — fitness-to-purpose.** Judge whether the in-process, no-live-operator-API
   demonstration meets the operator-facing intent of the slice.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 — C2 Reproduction (red pre-fix) — No gate configured for C2 (`check-gates.json` C2 result=`none`). C4-verify reports red→green, but whether the pre-fix red is a *demonstrated assertion-level* flip (brief.md:124-128 promises a temporary spread/desired-state negation) versus resting-red on a net-new file that simply fails to compile is recorded only in the **withheld** build-notes.md — cannot confirm from my artifacts.
- [ ] C5 — C5 Causal adequacy — Oracle is "reviewer + human sign-off" (`check-gates.json`). The local logic re-derives soundly — `reconciliation_status` returns `Satisfied` iff no committed placement references the drained server (`desired_state.rs:243-257`), distinct from the `Pending` "changed" moment — but the load-bearing distinct-domain re-place rests on `select_distinct_domains_excluding`/`domain_of` (patch `rebalance.rs:342,458,500`), which are **unseen on target** (#144-provided). Causal chain unverifiable end-to-end here.
- [ ] T5 — T5 Judgment — Oracle "reviewer + human sign-off". Concrete judgment gap to weigh: tests exercise only **single-fragment** evacuation; the multi-fragment evac path (`evac.len() > 1`) and the lost-CAS `EvacOutcome::Conflict`/`emit_conflict` branch (patch `rebalance.rs:413,561-617`) are unexercised, and the in-process `MemMeta`/`Fleet` fidelity to production stores is a human call.
- [ ] V — Validation — fitness-to-purpose — Always-human. Whether the Option-A in-process demonstration (no deployed custodian runtime; operator-facing API + CLI deferred to ADR-0013, `brief.md:132-140`) actually satisfies the operator's "mark a server draining and have it evacuated" purpose is a fitness judgment for sign-off.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Design and approach are sound; the gap is test coverage of two reachable branches the current fixtures never construct. Rebuild must add both tests (be thorough): 1. Multi-fragment evacuation (`evac.len() > 1`): wider topology with >=2 servers marked draining that hold fragments of the SAME chunk, plus enough spare distinct domains to re-place each. Current tests use RS(2,1) one-fragment-per-server and drain a single server, so `evac` is always length 0 or 1. This test also closes the C5 claim that `select_distinct_domains_excluding` preserves `n` distinct domains for the multi-fragment case. 2. Lost-CAS `EvacOutcome::Conflict` / `emit_conflict`: inject a concurrent inode mutation between `plan_evacuations` (read) and `evacuate_chunk` (commit) so the `.require(prior)` precondition fails and the commit returns `CommitOutcome::Conflict`. This is NOT a fixture-size issue — it needs a concurrency seam (e.g. a MemMeta wrapper that bumps the inode version once before the commit). This branch is the slice's headline safety claim ("a racing writer loses rather than corrupts the record") and is currently asserted only in prose. Not defects, carried forward as context for the next reviewer: - C2 pre-fix red IS already demonstrated (build-notes: temporary `draining.clear()` flips `drains_a_d_server...` to left:Satisfied,right:Changed) — the reviewer flagged it only because build-notes were withheld. - V (fitness) is an accepted Option-A scope boundary: in-process demonstration, operator API/CLI deferred to ADR-0013 per proposal 0005 sequencing. - #144 ordering gate: build-notes claim #144 is merged on the worktree base (5fb905c) supplying `select_distinct_domains_excluding` / `Topology::domain_of` / `gc::orphan_key`; confirm it is on `main` before the next sign-off.
- By / date: Eduard Ralph / 2026-06-22

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
