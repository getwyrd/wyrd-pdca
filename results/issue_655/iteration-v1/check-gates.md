# Check gates — issue_655

**Overall (gating): fail**

The Check 5/5/1: 5 correctness · 5 conformance · 1 validation.

## Correctness (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| C1 Spec | none | brief.md | — | — | no |
| C2 Reproduction (red pre-fix) | none | (no gate configured) | — | — | no |
| C3 Change | none | patch.diff | — | — | no |
| C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) | pass | ./engine/xtask.sh ci | C4-ci | xtask ci: all checks passed | yes |
| C4 per-fix red->green: this patch's test red pre-fix, green post-fix | unverifiable | ./engine/scripts/run-verify.sh | C4-verify |                why this slice has no isolable red (the cargo output is above). | no |
| C4 diff coverage: changed lines executed by the patch's tests | pass | ./engine/scripts/run-diff-cov.sh | C4-diff-cov | diff coverage 96.2% — 125 of 130 instrumentable changed lines executed (floor 80%); 130 of 480 changed lines were instru | no |
| C5 surviving mutants on the bundle diff (cargo mutants --in-diff) | fail | scripts/mutants-in-diff | C5-mutants | 123 mutants tested in 5m: 21 missed, 89 caught, 13 unviable | no |

## Conformance (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| T1 Structure | none | (no gate configured) | — | — | no |
| T2 Shape | none | (no gate configured) | — | — | no |
| T3 Runtime | none | (no gate configured) | — | — | no |
| T4 batched multi-pass rubric review (3x codex, union, triaged) | fail | scripts/review-branch --bundle | T4-batch-review | review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_655/review- | yes |
| T4 contribution artifacts complete (user-impact opener + tracker id in both) | deferred | scripts/pdca contribcheck | T4-contribution | pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish | yes |
| T4 tikv feature compiles (crate + server selection arms) | pass | WYRD_TIKV_TOOLCHAIN=1 sh -c 'cargo clippy -p wyrd-metadata-tikv --features tikv --tests && cargo clippy -p wyrd-server --features tikv,etcd --tests' | host-tikv |     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s | yes |
| T5 Judgment | none | reviewer + human sign-off | — | — | no |

## Validation (1)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| Validation — fitness-to-purpose | none | human at sign-off | — | — | no |
