# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/multipart/store.rs:797` **BUG** (seen by 1 pass): A TiKV lock race can return `Conflict` while the session remains unchanged and the obligation is absent, so this branch incorrectly reports `ObligationKeyTaken` and can discard an earlier fence’s still-in-flight outcome instead of retrying with that doubt preserved.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
