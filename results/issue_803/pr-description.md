# PR description

## Summary
**User impact:** a large object uploaded in parts could silently lose data. If
garbage collection or a restore ran while an upload was still in progress (or
had just finished but not yet published), the bytes of a completed part, or
of a part still being streamed, could be deleted or marked for deletion —
even though the upload was not done and no error was ever shown to the user.

This change makes garbage collection and restore treat an in-progress
upload's bytes as protected, the same way they already protect a finished
object's bytes, so routine maintenance can no longer eat a live upload.

## What to look at
- `crates/custodian/src/gc.rs` — the new `staged_fragments` reader and
  `StagedSet` type, and the two lines in `reconcile` that now consult it
  before reclaiming or certifying anything.
- `crates/custodian/src/restore.rs` — the same reader used before the
  post-restore pass marks anything stranded.
- `crates/server/src/cli.rs` — the operator-facing summary text, updated so
  it tells a human whether an unreadable record is a staged upload record or
  a committed object.
- To exercise it: seed an in-memory store with an open multipart upload (one
  committed part record, one in-flight staging entry) with fragments marked
  stale past the grace window, then run a GC pass — before this change the
  fragments are reclaimed; after, they survive. The new test file
  `crates/custodian/tests/staged_protection.rs` does exactly this, across
  every upload state and both maintenance passes.

## Root cause
Garbage collection's "what is still needed" set was built only from
committed chunk maps (`crates/custodian/src/gc.rs:383-413`, scanned at
`:478-573`). A committed part's fragments (`part:`) and an in-flight upload's
owned fragments (`sidx:`) were in no protected set, so GC reclaimed one as
soon as it carried a stale mark, and restore's stranded-fragment check
(`crates/custodian/src/restore.rs:385`, `:435-438`) had the same blind spot.

## Fix
Adds a second, disjoint protection class (`StagedSet`) built by reading each
upload's own bounded key ranges — never a namespace-wide scan — in the order
that keeps a chunk covered across a part commit or a publication landing
mid-read. Only the two passes that delete or mark bytes read it; garbage
collection's read order and its report of an unreadable committed object are
unchanged, and scrub / drain-status keep reading committed chunk maps only,
so their behaviour and cost are unchanged. A staged record this reading
cannot parse blocks both passes fleet-wide (matching the existing rule for an
unreadable committed object); one whose placement cannot be trusted holds its
whole chunk; a store fault fails the pass. The operator summary and the
runbook now say "staged multipart record" rather than folding it into
"committed object."

## Verification
- **Claim:** GC never reclaims, and restore never marks, a fragment named by
  an in-progress upload's committed-part or owned-staging record, in any
  upload state.
  **Checked:** `crates/custodian/src/gc.rs:383-413` and `:478-573` (the prior
  reference set, committed-only) against the new class read at
  `crates/custodian/src/gc.rs:641-905`; `crates/custodian/src/restore.rs:385`
  and `:435-438` (the prior mark gate) against the same reader used at
  `crates/custodian/src/restore.rs:340-360`.
  **Test:** `crates/custodian/tests/staged_protection.rs` — 24 of 26 cases
  fail by assertion against current `main` and pass with this change; the
  remaining two are guards (already green on `main`, would fail against a
  design that shares this read with scrub/drain-status).

- **Claim:** the reads are bounded per upload and never scan the whole
  `part:`/`sidx:` namespace.
  **Checked:** `crates/custodian/src/gc.rs:335-397` (bounded page walk).
  **Test:** `gc_and_restore_never_scan_a_whole_staged_namespace` in the same
  file — a guard, green against `main` and this change alike.

- **Claim:** scrub and the drain-status query are unaffected — same answers,
  same cost, no exposure to an upload record's damage or read faults.
  **Checked:** the scrub/drain-status entry points are unchanged in this
  diff.
  **Test:** `scrub_and_drain_status_do_not_read_upload_records` — a guard,
  green against `main` and this change alike; it would fail against a design
  that shared the new read across all four consumers.

- **Claim:** the operator-facing verdict names a staged record distinctly
  from a committed object.
  **Checked:** `crates/server/src/cli.rs:1256-1380` (prior text hard-coded
  "committed object(s)" for every unreadable record).
  **Test:** a new case in `crates/server/src/cli.rs`'s own test module,
  beside the existing `restore_verdict` tests — fails against current `main`
  (prints "committed object(s)" for a staged record) and passes with this
  change.

Fixes #803
