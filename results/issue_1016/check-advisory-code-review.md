- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_reaper_deployed.rs:1288`: The new usage test inherits `RUST_LOG`, unlike the role tests that pin their log level. With a malformed value such as `RUST_LOG='nonsense=;;'`, logging initialization rejects the environment before printing usage (`crates/server/src/cli.rs:472`, `crates/server/src/logging.rs:205`); the expected exit status still matches, but every usage-text assertion fails despite correct usage. Clear `RUST_LOG` on this child command. Low priority; confirmed by source inspection, not a new execution.

No additional introduced production correctness bugs or actionable reuse/simplification/efficiency findings identified. Reviewed the frozen gate evidence; no target files were changed and no gates were rerun. The brief's settled decisions were not reopened.

<!-- pdca:leaf-complete -->
