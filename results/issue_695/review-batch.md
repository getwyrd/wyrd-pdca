# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/backfill.rs:112` **BUG** (seen by 1 pass): Every undecodable `inode:` row increments `incomplete` before `parse_inode_key` or committed-state validation, so malformed non-inode keys or corrupt non-committed records permanently block backfill certification even though the pass is scoped to committed inode objects.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
