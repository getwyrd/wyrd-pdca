# Check gates — issue_250

**Overall (gating): pass**

The Check 5/5/1: 5 correctness · 5 conformance · 1 validation.

## Correctness (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| C1 Spec | none | brief.md | — | — | no |
| C2 Reproduction (red pre-fix) | none | (no gate configured) | — | — | no |
| C3 Change | none | patch.diff | — | — | no |
| C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) | pass | ./engine/xtask.sh ci | C4-ci | xtask ci: all checks passed | yes |
| C4 per-fix red->green: this patch's test red pre-fix, green post-fix | fail | ./engine/scripts/run-verify.sh | C4-verify | run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red). | no |
| C5 Causal adequacy | none | reviewer + human sign-off | — | — | no |

## Conformance (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| T1 Structure | none | (no gate configured) | — | — | no |
| T2 Shape | none | (no gate configured) | — | — | no |
| T3 Runtime | none | (no gate configured) | — | — | no |
| T4 Contribution | none | (no gate configured) | — | — | no |
| T5 Judgment | none | reviewer + human sign-off | — | — | no |

## Validation (1)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| Validation — fitness-to-purpose | none | human at sign-off | — | — | no |
