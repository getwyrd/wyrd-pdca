No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed the placement primitive (`crates/core/src/metadata.rs:3215`), addressing helpers (`crates/core/src/metadata.rs:3331`, `crates/core/src/metadata.rs:3344`), and regression tests including the seeded race campaign (`crates/core/src/metadata.rs:5228`). All five diff hunks match the read-only target source.

Validation evidence comes from the frozen gate logs: CI passed, diff coverage was 99.1%, and mutation testing reported 27 caught, 9 unviable, and no surviving mutants. C4-verify was green-only as declared in the brief. Tests were not rerun during this advisory review.
