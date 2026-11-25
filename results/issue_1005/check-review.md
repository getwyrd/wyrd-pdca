Review of #1005: add the pure `wyrd-validate` capability-matrix core, version selection, observation contract and run verdict; no confirmed patch defect was found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance rules distinguish fatal evidence from incomplete evidence and explicitly reserve required-row checks for child-2 (`brief.md:47`, `brief.md:154`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: independently stashing production while retaining the test produces an unresolved module import, not a behavioral red, as already accepted in the brief (`reviewer-red.log:259`, `brief.md:182`). |
| C3 Change | PASS | The four changed files stay within the agreed module/test/draft-document scope; required-row checking remains deferred without falsely passing an exercise (`crates/validate/src/lib.rs:24`, `crates/validate/src/matrix.rs:998`). |
| C4 Verification (red→green) | PASS | The restored patch independently passes all 30 tests, and all four stipulated rule-removal cases produce assertion failures; baseline compilation failure is not counted as behavioral red (`reviewer-final-green.log:39`, evidence below). |
| C5 Causal adequacy | PASS | Incomplete evidence cannot hide a fatal fact: settlement checks fatal findings across message stages before accepting an unproved result; no capability-probe workaround was introduced (`crates/validate/src/matrix.rs:863`, `crates/validate/tests/capability_matrix.rs:475`). |
| T1 Structure | PASS | The pure observation boundary admits only the existing `ErrorCode` client type and contains the fields child-2 needs, avoiding a dependency on client execution (`crates/validate/src/matrix.rs:27`, `crates/validate/src/matrix.rs:550`, `crates/validate/src/matrix.rs:663`). |
| T2 Shape | PASS | Version and entity-tag parsing reject malformed tokens, profiles reject duplicate operations, and formatting/spelling checks pass (`crates/validate/src/matrix.rs:221`, `crates/validate/tests/capability_matrix.rs:862`, `crates/validate/tests/capability_matrix.rs:1010`). |
| T3 Runtime | PASS | Classification and folding need no external runtime state; independent tests cover wrong statuses, cut bodies, absent answers and repeated exercises without claiming unsupported coverage (`crates/validate/tests/capability_matrix.rs:530`, `crates/validate/tests/capability_matrix.rs:1163`). |
| T4 Contribution | PASS | Path-based prior art is confirmed: all 19 closed-unmerged PRs and 24 merges since the brief's cutoff have no overlap; earlier cited merged work was checked by filenames, and no PR is open (`reviewer-prior-art.json:2`); contribution artifacts remain N/A until publish. |
| T5 Judgment | PASS | The agreed pre-0.1 retention trade-off is stated with its false-failure cost, and the second child's checks remain explicitly deferred; neither decision needs reopening (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `crates/validate/src/matrix.rs:998`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the observation contract as a useful foundation for child-2 and #939 — fixed-observation tests establish this slice's logic, while suitability for the eventual scenario runner remains the human sign-off decision (`brief.md:188`, `crates/validate/src/matrix.rs:660`). |

Source citations beginning `crates/` or `docs/` were checked in `$PDCA_TARGET`; other citations name evidence in this bundle. The target was readable and matched the patch. Both new files were restored byte-for-byte, and reverse-application checking succeeds (`reviewer-restoration.log:5`).

Independent falsifiability evidence covers every case required by `brief.md:185`:

- Suppressing success recognition fails the unexpected-support assertion at `crates/validate/tests/capability_matrix.rs:710` (`reviewer-mutants.log:3`).
- Accepting every error code fails the wrong-code assertion at `crates/validate/tests/capability_matrix.rs:614` (`reviewer-code-mutant.log:3`). This mutant was initially unviable under denied unused-variable warnings; rerunning with cargo-mutants' lint cap produced an actual assertion failure.
- Disabling the unknown-build branch makes `0.0.0+git.unknown` parse as a release and fails at `crates/validate/tests/capability_matrix.rs:854` (`reviewer-unknown-mutant.log:14`).
- Weakening the inconclusive condition incorrectly produces PASS with an unsettled required row and fails at `crates/validate/tests/capability_matrix.rs:1115` (`reviewer-mutants.log:4`).

The final unmutated suite passes under Rust 1.96.0. Independent rustfmt, typos, documentation lint and rendering also pass; rendering produced 101 pages with a successful link audit (`reviewer-docs-render.log:2`). Thus both named external dependencies were actually exercised, with no shim or missing-tool substitution.

The instance-scoped gate wrappers were adjudicated from their frozen captured output; full workspace CI, coverage and the complete mutation campaign are not claimed as independent reruns.

| Gate | Verdict | Basis |
|------|---------|-------|
| C4-ci | PASS | Captured CI runs the matrix's 30 tests and finishes with all checks passed (`gate-logs/C4-ci.log:3236`, `gate-logs/C4-ci.log:4526`). |
| C4-verify | N/A | Exit 77 reflects the agreed new-API limitation: green executes, while the pre-fix test cannot import the module; independently reproduced (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:16`). |
| C4-diff-cov | PASS | Captured instrumentation reports 390/436 changed executable lines covered, 89.4%, above the 80% floor (`gate-logs/C4-diff-cov.log:162`). |
| C5-mutants | PASS | Captured campaign reports 89 caught and 27 unviable mutants, with no survivors; independent targeted assertion failures supplement it (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | PASS | Captured review reports zero blocking findings; this advisory review separately examined the patch against the standing rubric (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Contribution artifacts are absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Both requested feature compilation commands finish successfully in the captured log (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). |

<!-- pdca:leaf-complete -->
