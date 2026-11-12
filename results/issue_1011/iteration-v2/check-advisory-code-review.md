No findings on either advisory lens: introduced correctness bugs, or actionable reuse, simplification and efficiency issues.

- Correctness: both validation passes measure the stored obligation values, admit equality at the ceiling, and pass the frozen set's obligations to publication (`crates/core/src/multipart/complete.rs:961`, `crates/core/src/multipart/complete.rs:1011`, `crates/core/src/multipart/complete.rs:1192`, `crates/core/src/multipart/complete.rs:1836`). The added unnamed-set regression checks zero commits and unchanged session bytes (`crates/core/tests/multipart_complete_value_ceiling.rs:676`).
- Reuse: part-set construction uses the existing canonical constructors, and sizing and writes share the same payload encoder (`crates/core/src/multipart/complete.rs:1171`, `crates/core/src/multipart.rs:4382`).

Validation evidence was inspected, not rerun: CI passed; all nine new tests passed, with four refusal cases failing when the production fix was reverted; mutation testing reported nine caught and seven unviable mutants. Diff coverage was not measured because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->
