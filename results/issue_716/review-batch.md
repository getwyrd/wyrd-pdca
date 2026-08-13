# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/multipart.rs:1578` **CONVENTION** (seen by 1 pass): `SessionRecord` accepts reordered or whitespace-varied JSON that re-encodes to different bytes, violating the required decode→encode byte identity for a record used in whole-value CAS.

## Recorded rejections (triaged — not blocking)

- `crates/core/src/multipart.rs:1746` CONVENTION: `metadata::decode` accepts reordered or whitespace-varied JSON that re-encodes differently, violating the required byte-identical decode→encode property for records used in exact-byte CAS.
- `crates/core/src/multipart.rs:1746` CONVENTION: The decoder accepts reordered or whitespace-bearing JSON that re-encodes differently, violating the required decode→encode byte identity for records used in whole-record CAS.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
