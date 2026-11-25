# Batched review — 3 passes, union of findings

- [ ] `crates/custodian/src/premark.rs:391` **BUG** (seen by 1 pass): The new fallback search admits servers absent from the fleet, so after a destination mark rules out the first choice, an unavailable server can displace a usable fallback and leave the move blocked on every retry.
- [ ] `crates/custodian/src/rebalance.rs:2855` **TEST-GAP** (seen by 1 pass): Holding only the second copy lets a serial write loop pass the mixed-outcome test, so the test does not verify concurrent dispatch.
- [ ] `crates/custodian/tests/move_destination_marks.rs:2411` **TEST-GAP** (seen by 1 pass): Holding only the second write lets a serial write loop pass the mixed-outcome test, so the test does not verify concurrent dispatch.
- [ ] `crates/custodian/tests/move_destination_marks.rs:2411` **TEST-GAP** (seen by 1 pass): Holding only the second write lets a sequential implementation pass this test, so the test does not verify that both writes are sent together.
- [ ] `crates/custodian/src/rebalance.rs:2855` **TEST-GAP** (seen by 1 pass): Holding only the second copy lets a sequential implementation pass this test, so the test does not verify that both copies are sent together.
- [ ] `crates/custodian/src/premark.rs:588` **CONVENTION** (seen by 1 pass): A malformed vacated mark is silently treated like a valid reclaiming mark, allowing adoption to report success and drain the repair obligation while the fragment remains under evidence GC cannot interpret.

## Recorded rejections (triaged — not blocking)

- `crates/custodian/src/premark.rs:390` BUG: After an orphan mark rules out the first destination, the fallback can select a server outside the active fleet and stop the move even when a later live server has a usable position, leaving the repair queued while reconstruction can still return `Satisfied`.
- `crates/custodian/src/premark.rs:550` BUG: The adoption CAS can publish a rebuilt fragment that an earlier GC delete still targets, because a retry can remove the `reclaiming` mark and allow the position to be reused while the original filesystem delete remains in flight.
- `crates/custodian/src/premark.rs:390` BUG: After a destination mark rules out the initial pick, the matcher can select a topology member absent from the live fleet and abort the move even when another live destination is available.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale (a decision follows its finding to the nearest matching line when the line shifts) — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
