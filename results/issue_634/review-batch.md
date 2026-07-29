# Batched review — 3 passes, union of findings

- [ ] `crates/dst/tests/support/mod.rs:233` **BUG** (seen by 1 pass): `scan_capped` collects the entire matching namespace before checking the cap, so an oversized scan can exhaust simulator memory instead of failing after `cap + 1` rows like production backends.
- [ ] `crates/metadata-fdb/src/lib.rs:1940` **CONVENTION** (seen by 1 pass): The non-retryable and retry-exhaustion exits return while `trx` is still live without best-effort cancellation, violating the repository’s rollback-before-early-return rule.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
