# Batched review — 3 passes, union of findings

- [ ] `crates/validate/src/matrix.rs:889` **BUG** (seen by 1 pass): `expect_bytes` ignores `content_length`, so a GET declaring 15 bytes for a known 16-byte object becomes merely Unsettled if cut after a correct prefix, even though the response head already proves a size mismatch that must be fatal.
- [ ] `crates/validate/src/matrix.rs:1009` **BUG** (seen by 1 pass): GET classification ignores the declared Content-Length, so a response declaring 17 bytes for a known 16-byte object and cutting off after 16 correct bytes becomes Unsettled despite the header already proving a size mismatch; ranged and conditional-miss GETs have the same omission.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
