- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:638`: The new bulk-delete budget test requires `OverBudget { received: 70_115 }`, but `read_whole` returns the cumulative bytes at the first frame that crosses the budget (`crates/validate/src/s3/answer.rs:617`). TCP reads can split this 70,115-byte response so that the crossing frame ends before the body's end; correct behavior then fails this assertion. Match `received > BUFFERED_BODY_BUDGET` (as the existing `refused_as` helper does), and assert `head.header("content-length") == Some("70115")` separately to pin the documented response size. This is a potential CI flake identified by inspection; the frozen gate run passed.

No additional correctness or reuse/simplification findings in this diff. Review used the read-only target and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->
