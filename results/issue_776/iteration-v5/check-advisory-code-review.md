No additional advisory findings: the diff is clean under the introduced-correctness and reuse/simplification/efficiency lenses.

Reviewed the placement primitive (`crates/core/src/metadata.rs:3228`), addressing helpers (`crates/core/src/metadata.rs:3342`, `crates/core/src/metadata.rs:3355`), and regression/seeded tests (`crates/core/src/metadata.rs:4437`). Grounding used only the supplied target source. Validation relied on frozen evidence, without rerunning builds: CI passed; mutation testing reported 27 caught, 9 unviable, and no missed mutants; per-fix verification was green-only as planned.

The existing T4 docs-currency finding concerning the new public operation (`crates/core/src/metadata.rs:3228`) remains recorded in `gate-logs/T4-batch-review.log`; this advisory does not clear that gate or duplicate its finding.
