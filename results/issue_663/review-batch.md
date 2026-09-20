# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/scrub.rs:180` **BUG** (seen by 1 pass): Staged placements on servers absent from `ctx.fleet` are added here but never visited or enqueued, so the deployed loop’s removal of unreachable servers leaves their staged chunks under-replicated indefinitely without repair obligations.
- [ ] `crates/custodian/src/scrub.rs:101` **CONVENTION** (seen by 1 pass): This adds staged-part scrubbing and reconstruction without the required living architecture update, leaving docs/design/architecture/06-runtime-view.md:80 incorrectly stating that scrub reads committed references only.
- [ ] `crates/custodian/src/reconstruction.rs:108` **CONVENTION** (seen by 1 pass): The new staged reconstruction API and scrub behavior require a living architecture update, but none is included and `docs/design/architecture/06-runtime-view.md:80` still explicitly says scrub reads committed references only.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
