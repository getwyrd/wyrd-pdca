# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/restore.rs:916` **BUG** (seen by 1 pass): Skipping `Aborting` sessions loses a previous pass’s residue finding, so rerunning after an incomplete `Completing` teardown can certify `mpufence` complete without repairing anything, and restoring missing parts as instructed does not add them to the already-installed explicit retirement set.
- [ ] `crates/custodian/src/restore.rs:916` **BUG** (seen by 1 pass): A pass that fences a Completing session with residue discards its publish target and records the blocker only in memory, so an unchanged rerun skips the now-Aborting session and marks the new generation complete despite the unresolved teardown.
- [ ] `crates/custodian/src/restore.rs:916` **BUG** (seen by 1 pass): A session fenced with uncovered segment chunks becomes `Aborting` and is skipped on every retry, so rerunning without repairing anything clears the residue finding and certifies `mpufence` complete while the incomplete retirement obligations remain unchanged.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
