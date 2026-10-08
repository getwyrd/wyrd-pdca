No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in this diff.

Reviewed the atomic fence and both collision guards (`crates/custodian/src/restore.rs:824`), repeat-pass obligation validation and paging (`crates/custodian/src/restore.rs:935`), teardown construction (`crates/core/src/multipart.rs:2291`), regression coverage (`crates/custodian/tests/restore_completing_fence.rs:394`), and operator reporting (`crates/server/src/cli.rs:1402`). All source citations were checked against `$PDCA_TARGET`.

Validation uses the frozen gate evidence; no builds or tests were rerun. CI passed, all seven new tests failed before the fix and passed after it, and C5 reported 8 caught and 28 unviable mutants with none missed. Diff coverage was not measured because the patch did not apply to `origin/main`. The explicit #659 and #843 deferrals remain settled for this review.
