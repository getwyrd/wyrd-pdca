No additional actionable findings on correctness introduced by this diff or on reuse, simplification, and efficiency.

Reviewed the shared destination selection and pre-mark/adoption protocol (`crates/custodian/src/premark.rs:226`, `crates/custodian/src/premark.rs:456`, `crates/custodian/src/premark.rs:529`), both committed movers, the staged selector extraction, and the changed race/deadline tests against the target source and frozen gate evidence. Existing deferrals and the frozen T4 findings are not reopened or overridden by this advisory. No gates were rerun; diff coverage remains unmeasured as recorded.

<!-- pdca:leaf-complete -->
