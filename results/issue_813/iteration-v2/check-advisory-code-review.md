No findings. This diff is clean on both advisory lenses: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified.

- Correctness: checked staged-before-committed reads, failure attribution, and drain protection at `crates/custodian/src/scrub.rs:123`, `crates/custodian/src/reconstruction.rs:210`, and `crates/custodian/src/reconstruction.rs:427`; reviewed the publication-race coverage at `crates/dst/tests/custodian.rs:2841`.
- Reuse and efficiency: the strict placement rule is shared at `crates/custodian/src/gc.rs:1460`; the part-only reader reuses the paged range walker at `crates/custodian/src/gc.rs:1607`; reconstruction retains its empty-queue shortcut at `crates/custodian/src/reconstruction.rs:207`.

Validation used the frozen gate logs: CI and the reconstruction handoff simulations passed; staged scrub had 11 green tests versus 9 failures and 2 passes on the base; diff coverage was 92.0%; mutation testing reported 15 caught and 16 unviable mutants. No builds or tests were rerun against the read-only target.
