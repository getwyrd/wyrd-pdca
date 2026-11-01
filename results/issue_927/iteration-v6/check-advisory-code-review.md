- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:590`: The slot-ownership guard takes the attempt ID from the batch's own `expected` bytes and uses that ID to select `last_slot`. If attempts A and B have both used the same index through the wrapper, an erroneous batch from A requiring B's latest bytes passes this assertion; the delete check only requires a precondition to exist. Thus the guard does not establish the advertised invariant that a batch requires its caller's own slot bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and check that the guard rejects a batch carrying another recorded attempt's bytes. This is a test-validation gap, not a demonstrated production defect.

No production correctness defects or actionable reuse, simplification, or efficiency findings identified in this diff. Validation evidence was reviewed from the frozen gate logs; tests were not rerun.

<!-- pdca:leaf-complete -->
