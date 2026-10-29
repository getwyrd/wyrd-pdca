- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:889`: `expect_bytes` ignores `response.content_length`. For a known 16-byte object, a GET declaring 17 bytes and then cutting off after the 16 correct bytes returns `Unsettled`; declaring 15 bytes and cutting off after eight correct bytes does too. The head already establishes a size mismatch, so these should be fatal `integrity`, making the run FAIL instead of INCONCLUSIVE. Whole, ranged, and non-matching conditional GETs share this omission. Check any present length against `want.len()` before handling the cut, preserving `None` for chunked responses, and add row/run regressions for both shorter and longer declarations. This consolidates the two equivalent frozen batch-review findings.

No additional correctness, reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->
