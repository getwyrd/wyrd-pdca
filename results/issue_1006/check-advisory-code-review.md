No findings. The diff is clean on both advisory lenses: no introduced correctness bugs, misleading tests, or actionable reuse, simplification, or efficiency issues found.

Reviewed the required-row checks and helpers in `crates/validate/src/matrix.rs:1020` and their tests in `crates/validate/tests/capability_matrix_required.rs:382`, grounded on `$PDCA_TARGET`.

Validation uses the frozen gate evidence; no builds were rerun. It records 42 new tests and 30 existing matrix tests passing, 39 new tests failing with production reverted, 100% instrumentable diff coverage, and 75 mutants caught with 3 unviable and none surviving.

<!-- pdca:leaf-complete -->
