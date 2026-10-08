# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/reconstruction.rs:541` **BUG** (seen by 1 pass): `parse_inode_key` accepts noncanonical numeric forms such as `inode:01`/`inode:+1`, so the repair targets canonical `inode:1`, loses its CAS, leaves the obligation queued, yet the pass can incorrectly return `Satisfied` instead of containing the unattributable row.
- [ ] `crates/core/src/metadata.rs:2816` **TEST-GAP** (seen by 1 pass): The new concurrent segmented-repoint path has only deterministic unit/test-double race tests, but the rubric requires seeded Tier-0 DST coverage for every new concurrent path.
- [ ] `crates/core/src/metadata.rs:2826` **TEST-GAP** (seen by 1 pass): This new concurrent maintenance/CAS path has only unit and manually injected race tests, but the rubric requires seeded Tier-0 DST coverage for new concurrent paths.
- [ ] `crates/core/src/metadata.rs:2894` **CONVENTION** (seen by 1 pass): The new `MetadataStore::get` await is unbounded, so a stalled backend can hang reconstruction in violation of the required external-await timeout discipline.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
