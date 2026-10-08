No additional findings in either advisory lens: no introduced correctness bug or actionable reuse, simplification, or efficiency issue found.

Reviewed the resolved-generation snapshot and byte-offset handling (`crates/custodian/src/reconstruction.rs:669`), move-time containment (`crates/custodian/src/reconstruction.rs:448`), and shared-primitive integration with atomic placement, obligation deletion, and orphan evidence (`crates/custodian/src/reconstruction.rs:1156`). The race fixtures exercise the intended read/commit windows, and the success test now checks the exact displaced orphan identity (`crates/custodian/tests/segmented_map_repoint.rs:496`).

Validation relied on the frozen gate logs; tests were not rerun and the target was not modified. The existing T4 finding about seeded DST coverage remains recorded in the gate evidence; this advisory does not override it.
