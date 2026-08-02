# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/restore.rs:449` **BUG** (seen by 1 pass): Keying `Expected` solely by `ChunkId` merges distinct committed references with divergent schemes or placements, so one healthy placement can mask another restored object's misplaced or unreconstructible reference.
- [ ] `crates/custodian/src/restore.rs:461` **BUG** (seen by 1 pass): Regrouping placements solely by `ChunkId` merges distinct committed `ChunkRef`s after an ID collision, allowing one object’s surviving fragment to satisfy another object’s placement and potentially hiding an unreadable chunk behind a successful restore exit.
- [ ] `crates/server/src/cli.rs:1281` **TEST-GAP** (seen by 1 pass): No CLI-level test proves an unreadable-only restore prints the new diagnosis and exits non-zero; existing tests only exercise `RestoreReport::is_clean`, which this branch never calls.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
