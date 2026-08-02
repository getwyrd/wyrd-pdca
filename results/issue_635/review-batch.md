# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/gc.rs:335` **BUG** (seen by 1 pass): Containing an unresolvable inode in a partial `ReferenceSet` lets `scrub::reconcile`, which never checks `unresolvable`, skip that object's fragments and incorrectly return `Satisfied`.
- [ ] `crates/core/src/metadata.rs:5162` **BUG** (seen by 1 pass): A syntactically valid but structurally undecodable `seg:` value such as `{}` is marked complete with no IDs, so `high_water_marks` contributes zero instead of the conservative ceiling and can under-report chunk IDs hidden by the damaged record.
- [ ] `crates/core/src/metadata.rs:2874` **BUG** (seen by 1 pass): Segment decode and bounds failures bypass the root re-read, so a request racing an overwrite can fail on corruption in the retired generation instead of resolving the healthy replacement.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
