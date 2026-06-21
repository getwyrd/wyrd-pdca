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

**Mode:** advisory, artifact-only, decorrelated from the builder. Inputs seen:
`brief.md`, `patch.diff`, `check-gates.json` (build-notes.md withheld by design).

**Grounding source.** `$PDCA_TARGET` could not be read directly (env access is
permission-blocked in this sandbox). The target checkout `getwyrd/wyrd @ main` is
attached as a working directory at `/home/eddie/wyrd/wyrd` and already carries the
patch; every path:line below was re-derived against that source **and** cross-checked
against `patch.diff` — the two agree, so the verdicts stand on `patch.diff` alone even
if the attached tree is not the intended `$PDCA_TARGET`. No other checkouts were searched.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | All 7 enumerated brief items are addressed and match the success criterion: Dockerfile→`rust:1.96.0-bookworm`+`--locked` (`Dockerfile:14,19`), panic-safe teardown (`xtask/src/main.rs:114-121,154-170`), bench doc corrected (`crates/core/benches/throughput.rs:51-55`), `WYRD_DSERVER_COUNT` warns (`xtask/src/main.rs:103-106,133-147`), `.github/dependabot.yml` cargo+github-actions, `.dockerignore` `results/` removed, tier cross-map note (`docs/.../10-quality-risks-glossary.md:101`). |
| C2 — C2 Reproduction (red pre-fix) | N/A | Brief declares a no-behavior-change hygiene bundle ("none affects correctness today"); no pre-existing defect to reproduce red. The added unit tests target the *newly extracted* pure fns (`resolve_dserver_count`, `with_teardown`), which did not exist pre-patch, so they are regression tests, not a red-pre-fix repro. check-gates C2 oracle = "(no gate configured)". |
| C3 — C3 Change | PASS | `patch.diff` is substantive, on-point, and confined to the seven hygiene surfaces (build/CI/bench/docs); no data-path change, matching declared scope. |
| C4 — C4 Verification (red→green) | PASS | check-gates.json C4-ci (gating=true) reports `cargo xtask ci` "all checks passed" — fmt/clippy/build/test/deny/conformance. Note: this is a mechanical gate; it verifies the workspace compiles and the new unit tests pass, **not** per-item correctness of the doc/config changes (covered under C5/inspection). New Rust read clean and compiles in shape (`xtask/src/main.rs:133-170,186-260`). |
| C5 — C5 Causal adequacy | PASS | Each fix targets the named mechanism, not a symptom: floating tag → exact patch pin matching `rust-toolchain.toml:4`; unpinned resolve → `--locked`; panic unwinds past teardown → Drop-guard `with_teardown` (Rust Drop runs during unwind; `xtask/src/main.rs:159-169`); silent clamp → explicit warning on rejected value (`:138-145`). Item 3 fixes a false doc claim by stating the truth (dropped `JoinHandle` detaches, not aborts) — the brief explicitly permits "corrected (or servers aborted)". Root cause is not contested. |
| T1 — T1 Structure | PASS | Tests live in an idiomatic `#[cfg(test)] mod tests` in the file under test (`xtask/src/main.rs:186-260`), using `super::*` and std primitives; correct placement for unit tests of crate-private fns. |
| T2 — T2 Shape | PASS | Assertions are meaningful and case-covering: unset→default-silent, valid+boundary `2`→silent, rejected set `{0,1,garbage,""}`→default+warning naming the var, and teardown on clean-return / error / panic-unwind (`xtask/src/main.rs:194-259`). |
| T3 — T3 Runtime | PASS | The test suite runs and is green via the C4 `cargo xtask ci` gate (test stage included), per check-gates.json C4-ci=pass. Could not independently re-execute (no build env / read-only). |
| T4 — T4 Contribution | PASS | Tests would catch real regressions on the two behavioral items: reverting to a silent clamp (item 4) or a non-Drop teardown (item 2) fails `dserver_count_rejects_unusable_values_with_a_warning` / `with_teardown_runs_teardown_when_the_body_panics`. Items 1,3,5,6,7 are config/doc with no automated test — acceptable (no meaningful runtime to assert; verified by inspection). |
| T5 — T5 Judgment | PASS | Sound, proportionate testing strategy: extracting the env-parse and teardown logic into pure fns to make panic-safety and the warning path unit-testable *without Docker* is the right call; leaving Dockerfile/dependabot/dockerignore/docs to inspection is appropriate. No over- or under-testing observed. Not contested. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. A maintainer must confirm the bundle as a whole serves the #117 follow-up intent. Specifically flag for sign-off: (a) the **#150 scheduling conflict** — item 2 reworks the same `run_integration` teardown (`xtask/src/main.rs:114-121`) as #150's log-capture change; brief says "land the teardown rework once / do not co-schedule" — a human must confirm merge ordering; (b) item 3 leaves servers *detached* rather than aborted-on-drop (a permitted choice, but a judgment); (c) the `github-actions` dependabot ecosystem assumes `.github/workflows/` exists — confirm CI surface. |

## §6 — Items the human must clear

1. **Validation fitness-to-purpose (V → NEEDS-HUMAN).** Confirm the hygiene bundle
   meets maintainer intent for the #117 follow-up, and in particular:
   - **#150 coordination:** the item-2 teardown rework collides with #150's
     log-capture change on the same `run_integration` teardown
     (`xtask/src/main.rs:114-121`). The brief's scheduling note says to land the
     teardown rework once and not co-schedule the two — a human must verify which
     PR carries it and the merge order.
   - **Item 3 design choice:** the bench doc was corrected to describe detachment
     rather than changing servers to abort-on-drop. Both were allowed by the brief;
     confirm the chosen disposition is acceptable.
   - **dependabot github-actions ecosystem:** verify a `.github/workflows/` surface
     actually exists for that ecosystem entry to act on (otherwise it is inert, not
     wrong).

## Reviewer notes (non-blocking)

- Drop-guard correctness re-verified: `with_teardown` runs teardown on the normal
  return path and during a panic unwind, then the panic resumes; `compose_down` is
  best-effort (`let _ = …`) so a double-panic abort is not a realistic risk.
- Patch is delivered as one combined diff; the brief allowed several focused commits.
  Commit granularity is not derivable from a diff and is not a correctness concern.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] V — Validation — fitness-to-purpose — Always-human. A maintainer must confirm the bundle as a whole serves the #117 follow-up intent. Specifically flag for sign-off: (a) the **#150 scheduling conflict** — item 2 reworks the same `run_integration` teardown (`xtask/src/main.rs:114-121`) as #150's log-capture change; brief says "land the teardown rework once / do not co-schedule" — a human must confirm merge ordering; (b) item 3 leaves servers *detached* rather than aborted-on-drop (a permitted choice, but a judgment); (c) the `github-actions` dependabot ecosystem assumes `.github/workflows/` exists — confirm CI surface.

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
- Iteration delta (if iterating): Rebase item-2 teardown rework onto #150's shape. #150 lands first and introduces `finish_integration` (capture-logs-before-teardown) on the same `run_integration` teardown (xtask/src/main.rs:114-121) that #154's `with_teardown` Drop-guard reworks. Rebuild #154's panic-safe teardown on top of #150's `finish_integration` so the two compose instead of colliding — i.e. preserve both capture-before-teardown (#150) and teardown-on-panic-unwind (#154). The other six hygiene items reviewed clean and are unaffected; only the teardown item needs reworking against #150.
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
