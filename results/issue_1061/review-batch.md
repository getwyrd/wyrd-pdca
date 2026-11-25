# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/retirement.rs:351` **CONVENTION** (seen by 1 pass): The new drain awaits metadata commits, reads, and scans without a timeout or enclosing deadline, violating the bounded-await rule and allowing a stalled backend to block subsequent reconciliation passes indefinitely.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
