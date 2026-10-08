# Batched review — 3 passes, union of findings

- [ ] `xtask/src/repo_guard.rs:751` **BUG** (seen by 1 pass): Resolve targets missing from `packages` are silently named by opaque package ID via `unwrap_or`, so malformed/incomplete metadata can hide a forbidden `wyrd-*` dependency and let this explicitly fail-closed guard pass.
- [ ] `xtask/src/repo_guard.rs:729` **BUG** (seen by 1 pass): Package entries missing an `id` or `name` are silently skipped, so a reachable forbidden package with an opaque ID and no readable name can produce a clean result instead of failing closed.
- [ ] `xtask/src/repo_guard.rs:730` **BUG** (seen by 1 pass): Malformed `packages` entries missing `id` or `name` are silently skipped, allowing a reached forbidden package to fall back to its opaque ID and evade the prefix check instead of failing closed.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
