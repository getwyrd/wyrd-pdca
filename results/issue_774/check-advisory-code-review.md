No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found within this diff.

Reviewed argument parsing (`crates/validate/src/args.rs:122`), credential selection and redaction (`crates/validate/src/access_keys.rs:78`, `crates/validate/src/access_keys.rs:139`), output failure handling (`crates/validate/src/lib.rs:93`), and their integration tests against the read-only target. The prior implementation findings are addressed.

Validation evidence: the frozen logs record `cargo xtask ci` passing, all 19 CLI tests passing, 223/223 instrumentable changed lines covered, and 42 mutants tested (26 caught, 16 unviable). Tests were not rerun in this read-only review.
