- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: The exact-budget success case checks only `entries.len() == 2`. Two copies of `list/a`, or entries with incorrect sizes or ETags, would pass this assertion. Compare the returned entries with the fixture's expected `list/a` and `list/b`, each size 7 and the supplied ETag, to satisfy the rubric's requirement for property assertions rather than count-only checks.

No additional production correctness bugs or actionable reuse, simplification, or efficiency findings identified. Validation relied on the frozen gate evidence; tests were not rerun against the read-only target.

<!-- pdca:leaf-complete -->
