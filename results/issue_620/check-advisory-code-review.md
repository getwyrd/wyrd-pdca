- No correctness findings introduced by this diff. The PUT-only guards validate the decoded bucket and UTF-8 key length before payload dispatch (`crates/gateway-s3/src/lib.rs:1853`, `crates/gateway-s3/src/lib.rs:1864`). The new tests check backend calls, stored state, alternate payload forms, and deletion through the existing body-cap boundary (`crates/gateway-s3/src/lib.rs:4890`, `crates/server/tests/s3_key_length_limit.rs:611`, `crates/server/tests/s3_key_length_limit.rs:708`).
- No actionable reuse, simplification, or efficiency findings. The implementation reuses decoded components, and the wire tests use existing SigV4 signing and URI-encoding helpers (`crates/gateway-s3/src/lib.rs:1798`, `crates/server/tests/s3_key_length_limit.rs:341`, `crates/server/tests/s3_key_length_limit.rs:408`).

Validation evidence reviewed: frozen CI passed; all seven wire tests passed with the fix and four failed when production changes were reverted; all five tested mutants were caught. No checks were rerun or target files modified.

<!-- pdca:leaf-complete -->
