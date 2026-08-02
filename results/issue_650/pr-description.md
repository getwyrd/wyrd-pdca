# PR description

> One logical fix per PR.

## Summary
**User impact:** For an object stored in segments, garbage collection could delete bytes
that object still owns, while scrub silently skipped verifying them and still reported the
store healthy. Separately, if garbage collection could not even read one object's chunk
list, it went ahead and told operators the store had fully converged — even though it knew
its picture of the store was incomplete.

This change makes both maintenance passes read a segmented object's full chunk list, and
makes them refuse to report the store as converged or clean whenever any committed
object's chunk list could not be read, instead of guessing.

This is one slice of a larger, already-agreed re-slicing tracked on the parent issue
(getwyrd/wyrd#635); no tracker URL pattern is configured for this repo, so no separate
report link is included here beyond the closing reference below.

## What to look at
The core change is in `crates/custodian/src/gc.rs`'s `referenced_fragments` (the function
that builds the "do not delete this" set both garbage collection and scrub consult) and in
`crates/custodian/src/reconciliation.rs`'s `Reconciled` enum (the shared pass/fail/blocked
outcome type). To exercise it: seed a store with a segmented object (its chunk list split
across separate records) and run a garbage-collection pass past its grace window, or seed
an object whose committed record cannot be decoded at all and run a garbage-collection or
scrub pass over it — the added test file
(`crates/custodian/tests/segmented_map_consumers.rs`) does both, plus a case where one
damaged object sits next to a healthy one to confirm the healthy object is still handled.

## Root cause
`referenced_fragments` built its protected set purely from each committed record's inline
chunk list, so a segmented object's chunks — stored separately — never appeared in it, and
garbage collection could reclaim them as if they were unreferenced. Independently, when a
committed record could not be read at all, garbage collection withheld reclamation (safe)
but still returned `Reconciled::Satisfied` (dishonest) — reporting the store converged over
a reference set it knew was incomplete. Scrub already handled that second case correctly;
garbage collection did not, so the two passes disagreed about the same input.

## Fix
The reference build now resolves every committed record through the shared chunk-map
resolver, so a segmented object's chunks are included. A new `Reconciled::Blocked` outcome
is added: whenever the reference set is incomplete (some committed object's map could not
be read), both garbage collection and scrub return `Blocked` — never `Satisfied` or
`Changed` — reclaim nothing, and log the unreadable object so it can be found and repaired.
The reconciliation step now reports the least-certified outcome across every loop it runs,
so one blocked loop is never masked by another loop converging. One unreadable object no
longer halts the entire pass: it is attributed and skipped, and every other object in the
store is still protected, verified, and (where applicable) reclaimed. A genuine
store-access failure underneath the resolver — as opposed to one object's own unreadable
record — still propagates as an error, since that is not a single object's fault to
contain.

## Verification
- **Claim:** A segmented object's fragments survive a garbage-collection pass (run past its
  grace window) and a scrub pass, and a drain of a server holding one of its fragments
  correctly answers "pending" rather than falsely certifying it as unreferenced.
  - **Checked:** `crates/custodian/src/gc.rs:378-457` (reference build resolves segmented
    maps) and `crates/custodian/src/desired_state.rs:167-170` (drain status stays
    non-certifying over an incomplete set), both on the branch this PR targets.
  - **Test:** `crates/custodian/tests/segmented_map_consumers.rs` (leg 1) — fails against
    the pre-fix code (the fragments are deleted / the drain wrongly certifies), passes
    post-fix.
- **Claim:** With one unreadable committed object, both garbage collection and scrub return
  a non-certifying result and reclaim nothing of that object; the blocker is named in the
  audit log.
  - **Checked:** `crates/custodian/src/gc.rs:231-269` (outcome and audit emission) and
    `crates/custodian/src/scrub.rs:707-745` (identical outcome rule), plus the shared
    `Reconciled::Blocked` variant at `crates/custodian/src/reconciliation.rs:22-546`.
  - **Test:** `crates/custodian/tests/segmented_map_consumers.rs` (leg 2), with the
    positive `Reconciled::Blocked` assertions in
    `crates/custodian/tests/gc.rs:865-880` and `crates/custodian/tests/scrub.rs:959-971` —
    all fail pre-fix (compile against base symbols only, so the pre-fix build reports
    `Ok(Satisfied)` where the test expects otherwise), pass post-fix.
- **Claim:** One damaged or unreadable object does not stop the pass from covering the rest
  of the store; a genuine store-access fault (not one object's fault) still propagates.
  - **Checked:** `crates/custodian/src/gc.rs:398-447` (per-object containment vs.
    propagated store error).
  - **Test:** `crates/custodian/tests/segmented_map_consumers.rs` (legs covering both a
    damaged object beside a healthy one, and a genuine store fault during resolve) — fail
    pre-fix, pass post-fix.

Fixes #650
