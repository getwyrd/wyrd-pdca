# Batched review — 3 passes, union of findings

- [ ] `crates/core/src/multipart.rs:1041` **BUG** (seen by 1 pass): `Budget::new` admits `max_part_chunks` values that exceed the settled value-size/operation-envelope bounds, so a supposedly valid maximal part can produce an unsplittable commit that no backend can execute.
- [ ] `crates/core/src/multipart.rs:2024` **BUG** (seen by 1 pass): The session-token arm accepts every session-scoped payload without checking the token’s optional part/attempt suffix, so a whole-session `Session`/`Parts` obligation can decode under a per-part token (or `Chunks` under a session-wide token) and make the drain act on the wrong scope.
- [ ] `crates/core/src/multipart.rs:1074` **BUG** (seen by 1 pass): The `sidx:` scan bound counts only in-flight chunks and ignores up to `max_staged_chunks` committed staging entries, so an accepted profile can exceed `SCAN_CAP / 2` and make teardown’s single scan silently incomplete.
- [ ] `crates/core/src/multipart.rs:1027` **CONVENTION** (seen by 1 pass): `Budget::new` enforces only the lower bound on `max_staged_chunks`, admitting profiles above the proposal’s settled `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` publishable ceiling.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
