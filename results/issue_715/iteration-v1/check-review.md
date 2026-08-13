Review of issue #715’s multipart budget-admission record codec, validating bounds, derived admission limit, and torn-record handling.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Resolve Plan scope before landing — this slice adds persisted `mpuctl` fields, but the binding rubric requires a same-PR living-architecture update while the brief excludes docs, so compliance changes scope (`AGENTS.md:154`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared born-at-tier exception — the clean base exits 101 because the named test target is absent, so it supplies no behavioral red against which to judge the implementation. |
| C3 Change | PASS | The requested codec/profile change remains confined to its two named files and adds 420 non-comment lines, within the 550-line semantic budget (`crates/core/src/multipart.rs:879`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept green without a behavioral base red — the 10-test green and all six one-check negations reproduce, but the pre-fix leg proves only test-target absence (`crates/core/tests/multipart_budget_admission.rs:7`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild with an encoding-derived ceiling — actual JSON for a legal-width 32 MiB RS(6,3) `ChunkRef` is 303 bytes, so 165 refs total 50,161 bytes before part fields despite the claimed 302-byte upper bound (`crates/core/src/multipart.rs:919`). |
| T1 Structure | PASS | The patch touches exactly the requested module and new integration test, with no dependency, documentation, or unrelated source changes. |
| T2 Shape | FAIL | Public `max_sessions` and `profile` fields let callers construct and pass a contradictory `AdmissionRecord` to the public encoder, so the structural invariant is not preserved by the value shape (`crates/core/src/multipart.rs:901`, `crates/core/src/multipart.rs:1233`). |
| T3 Runtime | N/A | No live writer or reader consumes these records in this slice; only pure codec and arithmetic paths have production code (`crates/core/src/multipart.rs:879`). |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected prior art by both affected paths — merged history and open-PR files were mechanically checked, but the archived #654/#692 attempt file lists are unavailable in the permitted artifacts, so duplication cannot be ruled out. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild tests with independent numeric oracles for `U_ref` and `MAX_SESSIONS` — 25/46 mutants survive, including every `U_ref` operator and both quotient alternatives, so wrong admission arithmetic remains green (`crates/core/src/multipart.rs:977`, `crates/core/src/multipart.rs:1181`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the record/profile semantics are fit for downstream admission and reaper integration — this dormant slice has no live-store exercise, so operational suitability is not yet established (`crates/core/src/multipart.rs:879`). |
