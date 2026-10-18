No introduced production correctness bugs found. The frozen red→green evidence confirms that the new wire test detects the original fractional-second mismatch.

- NEEDS-HUMAN [impl] — Optional reuse/simplification at `crates/server/tests/s3_last_modified_agreement.rs:477`: the new calendar conversion and two date parsers duplicate functionality available in `aws-smithy-types`, already a direct dev-dependency (`crates/server/Cargo.toml:134`). Use its `DateTime` parsing for the ISO-8601 and HTTP-date strings while retaining the exact wire-string assertions at `crates/server/tests/s3_last_modified_agreement.rs:842`. This preserves an oracle independent of the production formatter and removes roughly 100 lines of custom date-parsing logic without adding a dependency.

<!-- pdca:leaf-complete -->
