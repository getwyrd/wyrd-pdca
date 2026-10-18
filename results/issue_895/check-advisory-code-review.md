No findings on either advisory lens: no introduced correctness defect or actionable reuse, simplification, or efficiency issue identified.

- Correctness: `crates/core/tests/multipart_abort.rs:1112` forces a held unknown commit, an unapplied conflict, and a failed conflict re-read, reaching `crates/core/src/multipart/abort.rs:337`. It asserts the retained indeterminate outcome, then releases the held batch after Abort returns and verifies the fence, obligation, and unchanged admission count. Returning plain `Stopped` on that error arm would fail this test.
- Reuse: `crates/core/src/multipart/abort.rs:77` centralizes fence construction, and `crates/custodian/src/restore.rs:1055` uses it with the existing preconditions and write order preserved. No further actionable simplification found.

Validation used the frozen gate evidence, without rerunning builds: 24 core tests and 6 DST tests passed; mutation testing reported 18 caught and 4 unviable. Baseline red remains unverifiable because the new API is absent on the base; diff coverage was not measured because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->
