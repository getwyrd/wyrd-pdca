- No new correctness findings in this diff. Reviewed version selection (`crates/validate/src/matrix.rs:505`), unsupported and required classification (`crates/validate/src/matrix.rs:924`, `crates/validate/src/matrix.rs:1054`), bulk-delete evidence folding (`crates/validate/src/matrix.rs:1213`), and run verdicts (`crates/validate/src/matrix.rs:1397`), including their regression coverage in `crates/validate/tests/capability_matrix.rs:368`.
- No actionable reuse, simplification, or efficiency findings. The shared status, body, probe, and result-folding helpers cover the repeated checks (`crates/validate/src/matrix.rs:962`, `crates/validate/src/matrix.rs:999`, `crates/validate/src/matrix.rs:1013`, `crates/validate/src/matrix.rs:1187`).

Validation evidence: the frozen logs report 53 passing matrix tests, 88.6% diff coverage, and 128 caught / 30 unviable mutants with none surviving. Full CI remains failed on dependency advisories outside this diff; this advisory does not override that gate or the design questions carried forward in the brief. Target source was read only; no builds were rerun.

<!-- pdca:leaf-complete -->
