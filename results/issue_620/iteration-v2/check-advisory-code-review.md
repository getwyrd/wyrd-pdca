No introduced correctness bugs found in this diff. One low-priority reuse/efficiency finding:

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:4625` and `crates/server/tests/s3_key_length_limit.rs:136`: The new `path_encode` and `encode` helpers both duplicate the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`), including its unreserved set, slash escaping, and uppercase UTF-8 byte escapes. Reuse that helper to remove both copies and avoid their temporary `String` allocation per input byte. Keep `encode_all`, whose deliberate escaping of unreserved bytes exercises a different case. This is an optional test-code cleanup, not a correctness defect.

<!-- pdca:leaf-complete -->
