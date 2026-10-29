- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:840`: `classify_unsupported` treats every cut body as unsettled, including `HeadBucket`, whose declared 400 response must have no body. A 400 with `Body::Cut { arrived: Arrived::Count(1), .. }` therefore produces INCONCLUSIVE instead of fatal `protocol`, even though the received byte already proves the answer wrong. Route `DeclaredCode::NoBody` through the existing `expect_no_body` helper (`crates/validate/src/matrix.rs:945`) before the generic cut handling. Extend `head_bucket_passes_on_a_400_with_no_body_only` (`crates/validate/tests/capability_matrix.rs:462`) with both count and byte cut observations: nonempty must fail, zero received must remain unsettled. This consolidates the three duplicate findings in the frozen T4 batch log.

No additional correctness or reuse/simplification/efficiency findings.

<!-- pdca:leaf-complete -->
