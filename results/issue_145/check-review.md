# Check review — issue 145 / m3.7-rebalance-drain-decommission

Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`,
`check-gates.json` (build-notes deliberately withheld). Citations re-derived against
`$PDCA_TARGET = /home/eddie/wyrd/wyrd` (read-only) where the element grounds on existing
source; new files are cited against `patch.diff`.

## Headline finding — the patch base is ahead of the target (#144 ordering gate, UNRESOLVED)

`$PDCA_TARGET` is bare `main` and does **not** contain #144 (reconstruction, slice 6),
which the brief lists as a hard dependency ("Depends on: 144"; "must be COMPLETE first").
Concretely, on the target:

- `reconcile_step` is **5-arg** (`gc, scrub, now_millis`) at
  `crates/custodian/src/reconciliation.rs:60-66`, with the doc-comment "Reconstruction /
  rebalance (slices 6–7) are not yet dispatched" (`reconciliation.rs:59`). The patch's
  reconciliation hunk assumes a **6-arg** base (already carrying `reconstruction`) and a
  `use crate::reconstruction` import that does not exist on the target.
- `crates/core/src/placement.rs` has `select_distinct_domains` (`placement.rs:158`) but
  **no** `select_distinct_domains_excluding` and **no** `Topology::domain_of` — both
  called by the patch (`rebalance.rs` `evacuate_chunk`, `plan_evacuations`).
- `crate::gc::orphan_key` is module-**private** (`fn orphan_key`, `gc.rs:45`), not
  `pub(crate)`; the patch calls it cross-module from `rebalance.rs`.
- `crates/custodian/src/reconstruction.rs` and `crates/custodian/tests/reconstruction.rs`
  do not exist (target tests dir = `gc.rs scrub.rs skeleton.rs`); the patch edits
  `tests/reconstruction.rs`.

Consequence: the patch **cannot apply or compile against the current target**. The C4
gates in `check-gates.json` (xtask ci PASS, run-verify PASS) are therefore green only in
`$PDCA_WORKTREE` (the worktree base that carries #144, per the carry-forward's commit
`5fb905c`). This is exactly the carry-forward's flagged item: *"confirm #144 is on `main`
before the next sign-off."* It is **not** confirmed on the target — a human must clear it.
(This is not necessarily a defect of this patch — it is the documented dependency
ordering — but it blocks verification against the target and so blocks sign-off.)

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | Brief states binding success criteria (3 legs + invariant) each cited to proposal 0005 (`brief.md:42-74`, e.g. `0005:297-303`, `0005:351-352`, `0005:341-343`); spec is decomposed and unambiguous. |
| C2 — C2 Reproduction (red pre-fix) | PASS | `check-gates.json` run-verify oracle = PASS ("red without the fix, green with it"); test ships as its own new file `tests/rebalance.rs` (ADDED_TEST discriminator). Residual: build-notes withheld, so the documented assertion-level flip (test docs "skip the desired-state read", patch `tests/rebalance.rs:738-740`) is corroborated only by the gate, not re-read. |
| C3 — C3 Change | PASS | `patch.diff` is a coherent single slice: new `rebalance.rs` (327 ln) + `desired_state.rs` (150 ln), `placement.rs` `domain_utilization`/`excluding`, `reconcile_step` wiring, net-new tests. (Applicability to target — see §6.1.) |
| C4 — C4 Verification (red→green) | NEEDS-HUMAN | Gates green per `check-gates.json` (C4-ci, C4-verify) but only in `$PDCA_WORKTREE`; against `$PDCA_TARGET` the patch cannot compile (`select_distinct_domains_excluding`/`domain_of` absent in `placement.rs`; `gc::orphan_key` private at `gc.rs:45`; 5-arg `reconcile_step` at `reconciliation.rs:60-66`; `tests/reconstruction.rs` absent). Red→green is unverifiable on the target until #144 lands — see §6.1. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Always-human root-cause sign-off. On its face adequate: each move reuses the version-conditional commit-point-atomic re-place (copy fragment first, then one CAS `commit` repointing placement + orphaning the source — patch `rebalance.rs` `evacuate_chunk`), spread-wins via selector refusal→`Aborted`, and an intactness gate (`repair::fragment_intact`, target `repair.rs:53`). Human confirms root cause + scope. |
| T1 — T1 Structure | PASS | New dedicated `crates/custodian/tests/rebalance.rs` mirrors `gc.rs`/`scrub.rs` (the brief's named test file, `brief.md:111`); plus two unit tests in `placement.rs` `mod tests` (patch `placement.rs:63-97`). Correct placement. |
| T2 — T2 Shape | PASS | Assertions bind the criteria, not tautologies: distinct-domain evacuation + `version == 2` (exactly one commit), `ReconciliationStatus` Pending→Satisfied, spread-wins refusal leaves `version == 1` / placement untouched, per-domain metric `capacity_domain_utilization` read back, multi-fragment one-commit, lost-CAS leaves garbage + no `orphan:` record + `rebalance_conflict` metric (patch `tests/rebalance.rs` throughout). |
| T3 — T3 Runtime | PASS | `check-gates.json` C4-ci = PASS shows the suite builds and runs green (in the worktree). Same env caveat as C4/§6.1 — they do not run on the bare target. |
| T4 — T4 Contribution | PASS | Net-new coverage of every binding leg incl. both carry-forward-required additions: multi-fragment `evac.len() > 1` (`evacuates_two_drained_servers_of_one_chunk_in_a_single_commit`) and lost-CAS `Conflict` via a concurrency seam (`RacingMeta` / `a_racing_writer_loses_the_version_conditional_commit...`). Directly closes the iteration-1 gaps. |
| T5 — T5 Judgment | NEEDS-HUMAN | Always-human test-adequacy call. One item to weigh: the lost-CAS test asserts the pre-commit copy is "collectable garbage" but only checks the fragment is present and **no** `orphan:` record exists (patch `tests/rebalance.rs:1547-1554`); it does not exercise GC actually reclaiming an unreferenced-but-unorphaned fragment — human judges whether that gap matters here. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Option-A in-process demonstration through the real `reconcile_step`; the live operator-facing desired-state API + CLI are deferred to ADR-0013 (`brief.md:132-140`, `0005:355-356`). Whether in-process demonstration is fit-for-purpose for this slice is the human sign-off (carry-forward records this as an accepted Option-A boundary). |

## §6 — items the human must clear (each NEEDS-HUMAN above)

1. **C4 — #144 ordering gate (blocking).** Confirm #144 (reconstruction, slice 6) is
   merged on `main`/`$PDCA_TARGET` before sign-off. The target currently lacks
   `select_distinct_domains_excluding` and `Topology::domain_of` (`placement.rs`), a
   `pub(crate) gc::orphan_key` (`gc.rs:45` is private), the 6-arg `reconcile_step`
   (`reconciliation.rs:60-66` is 5-arg), and `tests/reconstruction.rs`. Until #144 is on
   the target, this patch does not apply/compile there and the green gates reflect only
   the worktree base.
2. **C5 — root-cause / approach sign-off.** Confirm the rebalance design (reuse the
   reconstruction atomic re-place; spread-wins; intactness gate; single-zone desired-state
   ledger) is the correct root-cause resolution and within scope (drain/decommission
   only; hot-spot rebalance out of scope, `brief.md:95-101`).
3. **T5 — test-adequacy judgment**, including the lost-CAS "collectable garbage"
   assertion noted in the table (GC reclamation of the unreferenced copy is not exercised).
4. **V — fitness-to-purpose** of the Option-A in-process demonstration with the operator
   API/CLI deferred (ADR-0013).
