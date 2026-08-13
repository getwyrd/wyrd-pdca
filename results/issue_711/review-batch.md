# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/metadata.rs:2866` **BUG** (seen by 1 pass): A missing segment is unconditionally returned as a transient conflict without checking whether the root still names it, silently retrying permanent metadata loss instead of surfacing the required absent-entry error.
- [ ] `crates/core/src/metadata.rs:2829` **BUG** (seen by 1 pass): Incrementing an untrusted persisted `u64` version with `+ 1` can panic in checked builds or wrap to zero in release when the version is `u64::MAX`.
- [ ] `crates/core/src/metadata.rs:2869` **BUG** (seen by 1 pass): The segmented write path decodes the freshly fetched record but never revalidates its offset and length against the root’s `SegmentRef`, so a concurrently replaced contextually invalid segment can be repointed and committed instead of producing a conflict.
- [ ] `crates/core/src/metadata.rs:2853` **CONVENTION** (seen by 1 pass): The newly added `store.get(&key).await` performs external metadata work without any timeout or other fail-closed bound, violating the repository’s await-discipline requirement.
- [ ] `crates/core/src/metadata.rs:2869` **CONVENTION** (seen by 1 pass): Collapsing a structurally invalid segment record into `Repoint::Conflict` violates the metadata-validation rule that decode failures surface as errors and can hide permanent corruption indefinitely.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
