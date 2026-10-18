# Batched review — 3 passes, union of findings

- [ ] `crates/server/src/version/derivation.rs:205` **BUG** (seen by 1 pass): Unicode trimming silently turns a valid Git tag such as `v1.2.3\u{00A0}` into `1.2.3`, bypassing exotic-tag rejection and allowing different commits to advertise the same identity instead of using the SHA fallback.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
