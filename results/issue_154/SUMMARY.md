# Result — issue 154 / m2-7-followup-build-bench-repo-hygiene

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: each item addressed — Dockerfile pinned to `rust:1.96.0-bookworm`
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the seven enumerated hygiene items only. / **out of scope:** anything requiring

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

# Check review — issue 154 / m2-7-followup-build-bench-repo-hygiene

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md withheld). Citations re-derived
against the target source at `$PDCA_TARGET = /home/eddie/wyrd/wyrd` (read-only),
which carries the draft applied; the diff's own hunks corroborate.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:6-43` is a complete plan: 7 enumerated hygiene items, each with a cited path:line and a per-item Success criterion (`brief.md:28-33`); scope fenced to "the seven enumerated items only" (`brief.md:37-39`). |
| C2 — C2 Reproduction (red pre-fix) | N/A | Bundle is explicitly non-correctness ("none affects correctness today", `brief.md:8`); no pre-fix failing reproduction applies. The two behavioral items (2,4) are covered by net-new tests over net-new pure fns (`resolve_dserver_count`, `finalize_panic_safe`) that had no red baseline. `check-gates.json:15-22` C2 = none / no gate configured. |
| C3 — C3 Change | PASS | All 7 items present in target: Dockerfile `1.96.0` + `--locked` (`crates/chunkstore-grpc/tests/dserver/Dockerfile:14,19`); teardown rework (`xtask/src/main.rs:116-123,160-178`); bench doc (`crates/core/benches/throughput.rs:53-55`); count warning (`xtask/src/main.rs:103-106,135-149`); dependabot (`.github/dependabot.yml:1-22`); `results/` removed (`.dockerignore` no longer lists it, 7 lines); tier note (`docs/design/architecture/10-quality-risks-glossary.md:101`). |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json:32-39` — gating C4 `cargo xtask ci` (fmt/clippy/build/test/deny/conformance) = pass; overall = pass (`:3`). New unit tests run within that gate. (Gate-reported; not independently re-run here.) |
| C5 — C5 Causal adequacy | PASS | Each item attacks its cited cause: Dockerfile patch-pin removes the float/re-download + `--locked` matches `rust-toolchain.toml:4` (`1.96.0`); `finalize_panic_safe` (`xtask/src/main.rs:160-178`) wraps `body` in `catch_unwind`, routes a panic through `finish_integration` (capture→teardown, `:187-201`) as `Err`, then `resume_unwind` — composing with #150 rather than colliding (the iteration-1 carry-forward requirement, `brief.md:54`), verified present in target. |
| T1 — T1 Structure | PASS | Tests live in the existing `mod tests` (`xtask/src/main.rs:197+` per diff), reusing #150's `RefCell<Vec<&str>>` ordering-probe idiom; correct location, consistent style. |
| T2 — T2 Shape | PASS | Assertions bind the right invariants: count resolution unset/valid/boundary-2/rejected-with-warning (`patch.diff:204-229`), and finalize ordering panic→capture,teardown,resume / clean→teardown / err→capture,teardown,propagate (`patch.diff:237-302`) — encodes both #150 capture-before-teardown and #154 panic-safety + non-silent clamp. |
| T3 — T3 Runtime | PASS | Plain `#[test]` unit tests reference only in-crate symbols (`resolve_dserver_count`, `finalize_panic_safe`, `finish_integration`, `DSERVER_COUNT`), all confirmed in target; executed under the green `cargo xtask ci` gate (`check-gates.json:32-39`). Gate-derived, not re-run in this review. |
| T4 — T4 Contribution | PASS | Tests are regression-meaningful: the panic test fails if teardown reverts to the bare closure (unwind would skip capture/teardown or swallow the panic); the warning test fails if the clamp goes silent again. Pure-fn refactor was done specifically to make both unit-testable without a container. Items 1,3,5,6,7 are config/docs — untestable, inspection-confirmed per `brief.md:33`; appropriate. |
| T5 — T5 Judgment | PASS | Sound test/altitude balance: behavioral items extracted to pure functions and unit-tested; non-behavioral (docs/CI config) left to inspection rather than forced into brittle tests. No over- or under-testing observed. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human at sign-off. Human must confirm the bundle is the right thing to merge AND clear the #150 coordination: `brief.md:46-48` says item 2 reworks the same `run_integration` teardown as #150 — "land the teardown rework once and do not co-schedule the two in one concurrent wave." Target shows #150 already landed (`finish_integration` present), so composition is real, but the merge-ordering / no-co-schedule decision is the human's. |

## §6 — Human must clear

1. **Validation — fitness-to-purpose (V).** Confirm this maintenance bundle
   meets its purpose and is mergeable as drafted. Specifically clear the #150
   scheduling constraint (`brief.md:46-48`): the teardown rework and #150 touch
   the same code; the target already contains #150's `finish_integration`, so
   #154 composes on top of it — verify the intended landing order holds and the
   two are not co-scheduled in one concurrent wave.

## Notes (non-blocking)

- The `github-actions` dependabot ecosystem is justified: `.github/workflows/`
  exists with pinned actions (adr-immutability, ci, dco, docs, integration-nightly, …).
- `finalize_panic_safe` relies on unwinding panics (`catch_unwind`); a
  `panic=abort` profile would bypass it. Default test/run profile unwinds, so
  this is fine for the Tier-2 path — noted only for completeness.
- `DSERVER_COUNT = 9` confirmed at `xtask/src/main.rs:75`, matching the brief's
  cited default.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] V — Validation — fitness-to-purpose — Always-human at sign-off. Human must confirm the bundle is the right thing to merge AND clear the #150 coordination: `brief.md:46-48` says item 2 reworks the same `run_integration` teardown as #150 — "land the teardown rework once and do not co-schedule the two in one concurrent wave." Target shows #150 already landed (`finish_integration` present), so composition is real, but the merge-ordering / no-co-schedule decision is the human's.

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
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
