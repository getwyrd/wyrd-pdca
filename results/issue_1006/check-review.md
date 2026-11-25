Review #1006: complete required-row grading and written-key 404 classification so a fully proved `pre-0.1` capability run can pass.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The closed evidence table, failure precedence, and unfit-ask rules make acceptance falsifiable within the pure-classifier scope (`brief.md:24`, `brief.md:85`, `crates/validate/src/matrix.rs:999`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing only production while retaining the new tests produced 39 assertion failures and 3 passes after successful compilation; the missing behavior is independently reproduced (`reviewer-cargo-red.log:3`, `reviewer-cargo-red.log:326`). |
| C3 Change | PASS | The two-file patch stays within scope; public declarations, the prerequisite test file, and the verdict fold remain unchanged, preserving the prerequisite's contract (`reviewer-scope.log:1`, `reviewer-scope.log:4`, `crates/validate/src/matrix.rs:1020`). |
| C4 Verification (red→green) | PASS | Restoring production passes all 42 new and 30 prerequisite tests; frozen CI passes, while the independent full-CI rerun has the snapshot gitlink caveat explained below (`reviewer-cargo-restored.log:38`, `reviewer-cargo-restored.log:86`, `gate-logs/C4-ci.log:4686`). |
| C5 Causal adequacy | PASS | Fatal facts survive incomplete observations, and a clean retry cannot erase an unanswered/truncated probe; regression cases exercise both delete rows and the full verdict (`crates/validate/src/matrix.rs:1391`, `crates/validate/tests/capability_matrix_required.rs:896`). |
| T1 Structure | PASS | The existing message-order evidence fold remains the single verdict authority, keeping row checks subordinate to the specified precedence (`crates/validate/src/matrix.rs:864`, `crates/validate/src/matrix.rs:1020`). |
| T2 Shape | PASS | The existing observation/API surface suffices; strict range parsing and exact document membership prevent malformed answers from passing without broadening the contract (`crates/validate/src/matrix.rs:1342`, `crates/validate/src/matrix.rs:1361`, `crates/validate/src/matrix.rs:1462`). |
| T3 Runtime | PASS | Classification performs finite work over recorded inputs, with range bounds checked before slicing and no new clock, I/O, shared mutable state, or task lifecycle (`crates/validate/src/matrix.rs:1054`, `crates/validate/src/matrix.rs:1246`, `crates/validate/src/matrix.rs:1391`). |
| T4 Contribution | N/A | Commit/PR drafts are intentionally absent during Check; the substantive contribution audit is required at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Path-based prior art finds only the prerequisite matrix implementation and no overlap among 19 closed-unmerged PRs; isolated wrong-status regressions address the carried test weakness (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:214`, `crates/validate/tests/capability_matrix_required.rs:1143`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated pure-classifier contract is sufficient for the #939 handoff — the runner and wire-to-observation mapping remain assigned to #939, so this slice does not establish deployed smoke fitness (`brief.md:172`, `brief.md:191`, `crates/validate/src/lib.rs:10`). |

No builder-fixable defect was found. The independent red→green run, source review, and path-based prior-art check support this advisory judgment. Fitness-to-purpose remains a human decision.

Source paths above are relative to `$PDCA_TARGET`; evidence-log paths are relative to this report. The supplied patch reverses cleanly against that target, and the production change was restored after the red run (`reviewer-scope.log:3`, `reviewer-stash-restore.log:12`).

The independent reproduction used `cargo test --offline -p wyrd-validate --test capability_matrix_required --test capability_matrix`, with scratch/build outputs inside this review directory. `git stash push -- crates/validate/src/matrix.rs` retained the new test for the red run; `git stash pop` restored the fix. No production or test edits were made. Standalone `typos`, formatting, and whitespace checks passed; the full-CI rerun also executed `typos`, satisfying the brief's named external dependency (`reviewer-ci.log:2`, `brief.md:184`).

The independent `cargo xtask ci` rerun exited 1 with exactly two failing checks, both caused by the snapshot gitlink omission; the remaining CI stages, including DST, completed successfully (`reviewer-ci.log:4241`, `reviewer-ci.log:4231`). The environment caveat is: the disposable base and index both omit `.github/advisory-db`'s gitlink. Consequently `xtask/tests/advisory_db.rs:66` fails and the dependency-wall pin step cannot run (`reviewer-host-caveat.log:1`, `reviewer-ci.log:3595`, `reviewer-ci.log:3614`). The patch changes neither that index entry nor the dependency configuration. The frozen gate records the actual pin fetch, successful dependency scans, and completed CI (`gate-logs/C4-ci.log:4039`, `gate-logs/C4-ci.log:4053`, `gate-logs/C4-ci.log:4686`). This snapshot limitation is not a C4 patch defect or evidence that the original gate skipped its dependency checks.

The instance-scoped coverage, mutation, and batch-review wrappers were adjudicated from their captured logs:

- **C4-verify PASS:** the frozen run agrees with the independent reproduction: 39 failures before the fix, 42 passes after (`gate-logs/C4-verify.log:336`, `gate-logs/C4-verify.log:14`).
- **C4-diff-cov PASS:** 333/333 instrumentable changed lines executed; 146 other changed lines are explicitly unscored. The inspected diff introduces no conditional-compilation exclusion (`gate-logs/C4-diff-cov.log:78`, `gate-logs/C4-diff-cov.log:91`).
- **C5-mutants PASS:** the captured campaign reports 75 caught and 3 unviable mutants, with no survivors (`gate-logs/C5-mutants.log:13`).
- **T4-batch-review PASS as reported:** its captured summary reports zero blocking findings. Detailed reviewer reasoning is not in that output; this report supplies an independent source review (`gate-logs/T4-batch-review.log:10`).
- **host-tikv PASS:** the captured output completes both requested feature compilations; it makes no claim of a live TiKV test (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).
- **T4-contribution N/A:** the publish-time audit remains due by design (`gate-logs/T4-contribution.log:10`).

The source review checked header/body precedence, unfit asks, duplicate delete requests, probe identity, incomplete retries, and grammar boundaries. The new wrong-status fixtures isolate the status check instead of obtaining the same class from malformed bodies (`crates/validate/tests/capability_matrix_required.rs:1143`). The added guards validate recorded inputs; none probes an optional capability or masks a load-time cause. The gateway citation now grounds at its actual bulk-delete contract (`crates/gateway-s3/src/lib.rs:2396`, `crates/gateway-s3/src/lib.rs:2548`).

The prior-art investigation queried merged history by both affected file paths and inspected changed-file lists for all 19 closed-unmerged PRs. Matrix history contains only [the prerequisite implementation](https://github.com/getwyrd/wyrd/commit/a4cdce13c2c94cb740203c056793d0991c9ec268); the new test path has no merged history, and none of those PRs overlaps either affected path (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:9`, `reviewer-prior-art.json:214`).

<!-- pdca:leaf-complete -->
