# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/restore.rs:668` **BUG** (seen by 1 pass): A malformed committed placement is silently skipped and never added to any `RestoreReport` finding, so `is_clean()` and the CLI can return success even though the maintenance pass could not validate or read that chunk.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
