No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed all four changed files against `$PDCA_TARGET`, including evacuation planning (`crates/custodian/src/rebalance.rs:436`), the shared repoint and atomic orphan batch (`crates/custodian/src/rebalance.rs:590`), and the race assertions (`crates/dst/tests/custodian.rs:6132`). Validation used the frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->
