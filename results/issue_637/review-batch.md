# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/restore.rs:743` **BUG** (seen by 1 pass): A `Completing` session is fenced using whatever `part:` keys happen to be present, so a missing restored part record silently produces an incomplete teardown obligation and the restore generation can still be marked complete instead of reporting the session unfenced.
- [ ] `crates/custodian/src/reconstruction.rs:1209` **BUG** (seen by 1 pass): A staged re-place still commits when the vacated source has an undecodable orphan mark, leaving the old fragment without valid reclamation evidence; the move must abort rather than merely log the fault.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
