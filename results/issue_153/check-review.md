# Check review — issue 153 / docs-contributing-and-pr-issue-templates

Advisory, artifact-only review. Inputs: `patch.diff`, `brief.md`, `check-gates.json`
(build-notes.md withheld). Citations re-derived against the target source at
`$PDCA_TARGET` = `/home/eddie/wyrd/wyrd` (read-only; patch already applied there).

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:13-22` gives a concrete, deterministically-inspectable success criterion (named files exist + accurately describe the enforced rules, cross-checked vs `require-issue.yml`/`dco.yml`). Oracle is `brief.md`; criterion is specific, not vague. |
| C2 — C2 Reproduction (red pre-fix) | N/A | Net-new docs/process artifact — no executable pre-fix behaviour to drive red. Absence confirmed by prior-art (`brief.md:30-32`; target had no `CONTRIBUTING.md`/PR template/`ISSUE_TEMPLATE/`); `check-gates.json` C2 = "no gate configured". |
| C3 — C3 Change | PASS | `patch.diff` creates exactly the scoped files — `CONTRIBUTING.md`, `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/{bug_report,enhancement,config}.yml`; no out-of-scope or Rust file touched (matches scope at `brief.md:25-27`). |
| C4 — C4 Verification (red→green) | PASS | Gate `cargo xtask ci` ran green (`check-gates.json:33-39`) but is **supplementary** — the change touches no Rust, so the gate proves nothing about content. The load-bearing verification is content accuracy, re-derived here: DCO-every-commit (`dco.yml:37-49`), require-issue title/body→real-issue (`require-issue.yml:32-51`), `xtask ci` = fmt/clippy/build/test/deny/conformance (`ci.yml:4-5,109`), Tier-2 nightly & non-gating (`integration-nightly.yml:1-19`), local docker warn-and-skip (`xtask/src/main.rs:85-97`). All match. |
| C5 — C5 Causal adequacy | PASS | Root cause (rules CI-enforced but undocumented → learned by tripping a check) is uncontested; the patch documents each enforced rule accurately and every cross-reference resolves on target: ADR-0003 §1 (`docs/design/adr/0003-apache-2-license-and-dco.md`), ADR-0016 xtask single-source (`adr/0016-…:20,27`), ADR-0002 conformance vectors (`adr/0002-…:20`), `SECURITY.md`, `docs/governance/{CODE_OF_CONDUCT,GOVERNANCE}.md`, README status note (`README.md:12-16`). |
| T1 — T1 Structure | N/A | Docs/process change has no executable test surface; the brief's chosen oracle is deterministic inspection (`brief.md:16-22`) — no test artifact exists to assess structure. `check-gates.json` T1 = "no gate configured". |
| T2 — T2 Shape | N/A | No test artifact exists to assess shape. |
| T3 — T3 Runtime | N/A | No test artifact exists to run. |
| T4 — T4 Contribution | N/A | No test artifact exists to weigh for contribution. |
| T5 — T5 Judgment | PASS | "No automated test" is real and stated: the artifacts have no executable surface and sit outside `docs/`, so even the one markdown linter (`lint_docs.py`/`docs-check`) does not cover them (`brief.md:18-22`); deterministic inspection is the appropriate oracle. Residual risk worth noting: nothing guards CONTRIBUTING↔workflow drift, but the brief explicitly scopes that out. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Whether these documents genuinely de-friction newcomers (tone, completeness, helpfulness) is a human sign-off call, not mechanically derivable. `check-gates.json` V oracle = "human at sign-off". |

## §6 — Human items to clear

1. **Validation / fitness-to-purpose (V).** Confirm `CONTRIBUTING.md`, the PR
   template, and the issue templates actually serve newcomers well — accurate is
   verified (see C4/C5), *helpful* is the human's call.

## Notes (non-gating)

- Content accuracy held against all four workflows and the cited ADRs/files; I
  found no misstatement. Notable correctness detail done right: the PR doc keeps
  "closing keyword auto-closes" separate from "`Refs #N`/bare `#N` satisfy the
  *check*" — the `require-issue` regex `(#|issues/)[0-9]+` is keyword-agnostic
  (`require-issue.yml:35`), and GitHub only auto-closes on Closes/Fixes/Resolves,
  so the distinction is materially correct.
- C4's green gate is genuine but vacuous for this change (no Rust); the verdict
  rests on the inspection, not the gate. Flagged per "a green mechanical check is
  not a correctness verification."
- Drift risk (T5 residual): `CONTRIBUTING.md` paraphrases `*.yml` that can change
  independently with no CI tie-back. Out of scope here; candidate follow-up.
