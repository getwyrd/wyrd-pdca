# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/multipart.rs:2776` **BUG** (seen by 1 pass): Rejecting a generation containing both `chunks` and `segments` contradicts proposal 0016’s normative `chunks?` plus `segments` shape, so valid retirement obligations cannot be decoded.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
