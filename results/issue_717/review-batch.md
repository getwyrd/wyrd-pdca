# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/multipart.rs:2770` **BUG** (seen by 1 pass): `RetirePayload::checked_against_token` accepts a segment generation under both epoch `E` and `E+1`, so the same record-deletion obligation has two valid keys and can be duplicated or drained twice, violating the canonical single-identity property required for retirement records.
- [ ] `crates/core/src/metadata.rs:1562` **BUG** (seen by 1 pass): `PendingEntry` now accepts an owned-shaped value under a `pending:` key, so existing key-aware renewal/lease-guard paths can treat multipart staging as an ordinary lease and `renew_pending` can silently erase its `owner`/`staged` identity instead of rejecting the misfiled record.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
