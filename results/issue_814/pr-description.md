## Summary
**User impact:** If a file part is still being uploaded (the upload has not been
completed yet) and one of its stored copies is lost — a disk failure, a server
going away — that part stayed under-protected indefinitely. The system knew a
copy was missing but never replaced it unless the client happened to finish the
upload. A rebuild only ever happened for already-completed uploads, so a slow or
long-running upload could sit one failure away from real data loss for as long as
it stayed in progress.

This change makes reconstruction rebuild and re-place the missing copy for an
in-progress upload's parts too, using the same safety rules (mark-before-write,
a deadline, and an all-or-nothing hand-off) the completed-upload repair path
already uses.

## What to look at
The new logic lives in `crates/custodian/src/reconstruction/staged.rs`. It plugs
into the existing repair pass (`crates/custodian/src/reconstruction.rs`): a part
that used to be recognized-but-untouched now gets assessed and repaired the same
way a completed upload's part does, unless the upload has since finished, aborted,
or the record can't be trusted.

To reproduce the original bug: start a multipart upload, commit one part, delete
one of its stored fragments, queue a repair, and run a reconstruction pass on
`main` — the repair obligation stays queued and nothing gets written. The new
test `crates/custodian/tests/staged_repair.rs` reproduces exactly this and many
of its edge cases (the process aborting mid-repair, a slow-but-legal write, a
concurrent garbage-collection race, and so on).

## Root cause
Reconstruction recognized that a staged (in-progress-upload) part still needed
its obligation kept, but the code path that actually rebuilds a fragment only
existed for committed (completed-upload) chunks (`reconstruction.rs:736-739`,
the `Assessment::Staged` arm on `main` @ `feb1e30`). There was no destination
write, no mark, and no adoption step for the staged case at all.

## Fix
Adds a `reconstruction::staged` module that gathers and verifies survivors the
same way the committed path does, then: pre-marks the destination durably before
writing to it; derives the write deadline from the pre-mark's own timestamp plus
the configured window (never the pass's start time, so a slow-but-legal earlier
write in the same move can't starve a later one's deadline); sends a move's
destination writes together rather than one after another; and commits the
result with one compare-and-swap pinned to the upload's session state, the prior
part bytes, the pre-mark, and the destination's drain status. Any failure at any
point leaves the repair obligation queued and the pre-mark standing — nothing is
ever written and then left unaccounted for.

## Verification
- **Claim:** a committed part's chunk in an in-progress (`Open`) upload that
  lost a fragment is rebuilt, and no interruption at any step of the rebuild
  strands a fragment (every written byte is either named by a record or covered
  by a mark garbage collection can act on).
  - **Checked:** `crates/custodian/src/reconstruction/staged.rs` (module added by
    this PR) — the pre-mark write (around `staged.rs:616-633`), the deadline
    derivation and grouped destination writes (`staged.rs:662-669`), and the
    single adoption compare-and-swap (`staged.rs:699-726`), all in this PR's
    diff against `main` @ `feb1e30`.
  - **Test:** `crates/custodian/tests/staged_repair.rs` (new) — fails on `main`
    (the obligation stays queued and nothing is written); passes with this fix.
    Covers the full rebuild, session-fence loss at each step, the pre-mark and
    deadline rules, a concurrent garbage-collection race against the adoption's
    two preconditions, and the cases where a staged part is correctly left kept
    rather than rebuilt.
- **Claim:** the rebuild never stalls or leaves a chunk permanently
  under-protected under any legal interleaving of the session lifecycle.
  - **Checked:** `crates/dst/tests/custodian.rs` (seeded property test, run
    under `cargo xtask ci`'s deterministic-simulation mode).
  - **Test:** `staged_replace_under_the_fence_strands_nothing` and
    `staged_replace_reaches_every_point_of_the_fence`, 50 seeds — new in this
    PR; not present on `main`.

Fixes #814
