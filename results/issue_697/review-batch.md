# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/tests/segmented_map_reconstruction.rs:712` **TEST-GAP** (seen by 1 pass): The suite never exercises concurrent root supersession during segmented resolution, leaving the new restart/refusal behavior and its no-stale-write guarantee unverified by seeded Tier-0 DST coverage.
- [ ] `crates/custodian/src/reconstruction.rs:476` **TEST-GAP** (seen by 1 pass): The new scan/resolve snapshot path explicitly relies on correctness under concurrent root replacement, but adds no seeded Tier-0 DST regression exercising that race as required for a new concurrent path.

## Recorded rejections (triaged — not blocking)

- `crates/custodian/src/reconstruction.rs:474` CONVENTION: The new resolver await performs external metadata I/O without a caller-visible timeout or cancellation bound, violating the rubric’s requirement that every await on external work be bounded.
- `crates/custodian/src/reconstruction.rs:474` CONVENTION: The newly introduced metadata-resolver await is not bounded by a caller-side timeout, violating the required fail-closed await discipline.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
