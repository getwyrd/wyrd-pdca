# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/metadata.rs:1674` **CONVENTION** (seen by 1 pass): `decode_pending_entry` accepts noncanonical or unknown-field spellings (for example `"owner":null`), so `renew_pending` can CAS successfully and silently rewrite/drop fields, violating serialization identity.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
