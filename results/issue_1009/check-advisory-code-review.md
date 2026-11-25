No findings. The diff is clean on both advisory lenses: introduced correctness bugs (including test fidelity), and actionable reuse, simplification, or efficiency issues.

- Checked cursor advancement and resume, charged release obligations, and immediate termination on unknown outcomes (`crates/core/src/multipart/complete.rs:1074`, `crates/core/src/multipart/complete.rs:1325`, `crates/core/src/multipart/complete.rs:1350`, `crates/core/src/multipart/complete.rs:1361`). The implementation reuses the committer's segment planner and the existing retirement-obligation constructor.
- The added tests exercise intermediate-cursor resume and release, resumed flip-only unknown outcomes, and the cursor's decode boundary (`crates/core/tests/multipart_complete.rs:5185`, `crates/core/tests/multipart_complete.rs:5290`, `crates/core/tests/multipart_complete.rs:5108`, `crates/core/src/multipart.rs:6184`).

Validation used the frozen gate evidence; no builds were rerun. CI and core tests passed. Diff coverage was not measured because the patch did not apply to origin/main. The sole surviving mutant at `crates/core/src/multipart/complete.rs:1062` removes the backward-cursor guard; no supported writer moves this fence's cursor backward, consistent with the brief's recorded disposition.

<!-- pdca:leaf-complete -->
