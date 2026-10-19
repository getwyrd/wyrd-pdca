- NEEDS-HUMAN [impl] — `crates/server/tests/last_modified_monotonic.rs:183`: The new stored-value tests await real filesystem work without a deadline: `seed_file` directly awaits `write_fragments`, and their PUT/read calls at `:213`, `:224`, and `:260` are also outside a timeout. Unlike the wire-test setup, these callers never enclose the operations in `bounded`, so a stalled operation can leave the tests waiting indefinitely. Reuse the existing `bounded` helper (`:583`) around these call chains. This confirms the finding in the frozen `T4-batch-review` log.

No additional production-correctness, reuse/simplification, or efficiency findings in this diff. Reviewed the target source and frozen gate evidence; did not rerun builds or tests.

<!-- pdca:leaf-complete -->
