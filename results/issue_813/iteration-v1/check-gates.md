# Check gates — issue_813

**Overall (gating): fail**

The Check 5/5/1: 5 correctness · 5 conformance · 1 validation.

## Correctness (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| C1 Spec | none | brief.md | — | — | no |
| C2 Reproduction (red pre-fix) | none | (no gate configured) | — | — | no |
| C3 Change | none | patch.diff | — | — | no |
| C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) | pass | ./engine/xtask.sh ci | C4-ci | xtask ci: all checks passed | yes |
| C4 per-fix red->green: this patch's test red pre-fix, green post-fix | pass | ./engine/scripts/run-verify.sh | C4-verify | run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red). | no |
| C4 diff coverage: changed lines executed by the patch's tests | fail | ./engine/scripts/run-diff-cov.sh | C4-diff-cov | diff coverage 78.6% — 88 of 112 instrumentable changed lines executed (below the 80% floor); 112 of 432 changed lines we | no |
| C5 surviving mutants on the bundle diff (cargo mutants --in-diff) | pass | scripts/mutants-in-diff | C5-mutants | 26 mutants tested in 4m: 14 caught, 12 unviable | no |

## Conformance (5)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| T1 Structure | none | (no gate configured) | — | — | no |
| T2 Shape | none | (no gate configured) | — | — | no |
| T3 Runtime | none | (no gate configured) | — | — | no |
| T4 batched multi-pass rubric review (3x codex, union, triaged) | fail | scripts/review-branch --bundle | T4-batch-review | review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b | yes |
| T4 contribution artifacts complete (user-impact opener + tracker id in both) | deferred | scripts/pdca contribcheck | T4-contribution | pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish | yes |
| T4 tikv feature compiles (crate + server selection arms) | pass | WYRD_TIKV_TOOLCHAIN=1 sh -c 'cargo clippy -p wyrd-metadata-tikv --features tikv --tests && cargo clippy -p wyrd-server --features tikv,etcd --tests' | host-tikv |     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.97s | yes |
| T5 Judgment | none | reviewer + human sign-off | — | — | no |

## Validation (1)

| Check | Result | Oracle | Rule | Evidence | Gating |
|---|---|---|---|---|---|
| Validation — fitness-to-purpose | none | human at sign-off | — | — | no |
