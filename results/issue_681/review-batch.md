# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/rebalance.rs:412` **BUG** (seen by 1 pass): `resolve_chunk_map` may restart onto a newer generation, so `chunk_index` can describe that generation while `prior_bytes` still contain the scanned flat map, making this unchecked indexing panic before the stale CAS can reject the plan.
- [ ] `crates/custodian/src/reconstruction.rs:659` **BUG** (seen by 1 pass): A resolver restart can produce a `FlatSite` whose `chunk_index` belongs to the live generation but whose `prior_bytes` belong to the stale scanned generation, so indexing `prior_chunks` can panic instead of safely aborting on CAS conflict.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
