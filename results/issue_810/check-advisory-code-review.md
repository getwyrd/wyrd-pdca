No findings. The diff is clean under both advisory lenses: introduced correctness bugs (including test fidelity), and actionable reuse, simplification, or efficiency issues.

Reviewed the generation's opening and completion preconditions and error propagation (`crates/custodian/src/restore.rs:901`, `crates/custodian/src/restore.rs:946`), durable residue rechecks, codec boundaries, and operator reporting. The added tests cover opening conflicts (`crates/custodian/tests/restore_fence_generation.rs:922`) and the generation returned by a real pass (`crates/custodian/tests/restore_open_fence.rs:869`).

Validation used the frozen evidence; no builds or tests were rerun. CI passed, and all 12 regression tests passed post-fix and failed by assertion on the base. Diff coverage was not measured because the patch did not apply to origin/main. The sole surviving mutant changes the pre-existing batching condition at `crates/custodian/src/restore.rs:798`; it is not an introduced defect.
