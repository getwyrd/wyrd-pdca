# Batched review — 3 passes, union of findings

- [ ] `crates/server/build.rs:53` **BUG** (seen by 1 pass): Converting `std::env::var("WYRD_VERSION")` with `.ok()` silently treats a non-UTF-8 override as absent, allowing an explicitly invalid identity to be replaced by Git or the fallback instead of rejecting the build as promised.
- [ ] `crates/server/build.rs:53` **BUG** (seen by 1 pass): Calling `.ok()` silently treats a non-UTF-8 `WYRD_VERSION` as unset, baking a Git-derived or fallback identity instead of rejecting the invalid explicit override; distinguish `VarError::NotUnicode` from `NotPresent`.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
