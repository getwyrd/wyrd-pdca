No patch-introduced correctness bugs or hot-path efficiency regressions found.

- NEEDS-HUMAN [impl] — Low-priority reuse: `crates/server/tests/multipart_gateway_seam.rs:506` and `crates/server/tests/multipart_gateway_seam.rs:574` add near-copies of the response parser and chunk decoder at `crates/server/tests/s3_chunk_size_flag.rs:294` and `crates/server/tests/s3_chunk_size_flag.rs:357`. The copies already differ in repeated-transfer-encoding rejection, checked chunk-size arithmetic, and trailer validation. Extract a shared test-support parser for these two suites, retaining the new malformed-response fixtures, so framing fixes and regression coverage apply to both callers. This is an advisory cleanup, not a correctness blocker.

<!-- pdca:leaf-complete -->
