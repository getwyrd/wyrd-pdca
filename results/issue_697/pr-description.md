# PR description

## Summary
**User impact:** When a store held even one file that had been written as a
multipart ("segmented") object, the background job that repairs
under-replicated data stopped working for the *entire* store — not just for
that one file. Every other file waiting on a repair was silently left
under-replicated, with no error surfaced beyond a failed maintenance pass, so
an operator had no signal that redundancy was not being restored.

This PR makes reconstruction read every object the way the store's other
maintenance jobs (garbage collection, scrub, restore) already do: it no
longer aborts on the first multipart object it meets. A repair it genuinely
cannot perform yet (because the target lives inside a multipart object) is
now refused and reported, rather than silently dropped or used to kill the
whole pass.

Reported via issue #697 (no public tracker URL configured for this project).

## What to look at
- `crates/custodian/src/reconstruction.rs` — the repair pass's `reconcile`
  function and its new `read_committed` helper, which now scan the committed
  object namespace once per pass instead of once per queued repair.
- Try it by seeding a store with one multipart object and one ordinary
  (unrelated) under-replicated object, then running a reconstruction pass:
  before this change the pass errors out and nothing gets repaired; after,
  the ordinary object is repaired and the multipart one is reported as
  refused rather than blocking anything else.

## Root cause
Three call sites in `reconstruction.rs` read a committed object's chunk map
directly out of the inode record and propagated `?` on
`ChunkMapError::SegmentedMapUnsupported` the instant they met a multipart
object, ending the entire pass rather than just that one object's handling.
The same code also rescanned the whole object namespace once per queued
repair instead of once per pass.

## Fix
Reconstruction now performs one reading of the committed namespace per pass,
through the same resolver every other maintenance loop shares
(`metadata::resolve_chunk_map`). A record that cannot be decoded or resolved
is contained and named on the audit log, and the walk continues — mirroring
the containment rule already used by garbage collection. A repair whose
target lives in a multipart record is refused (nothing is written, the
repair stays queued) rather than being treated as "nothing to repair" or
aborting the pass. The pass only reports success over the reading it
actually completed: if anything was unreadable or refused, it reports
"blocked" instead of claiming the store is fully repaired.

## Verification
- **Claim:** one multipart object no longer stops repair of any other object
  in the store, and per-object faults are named rather than aborting the
  whole pass.
  **Checked:** `crates/custodian/src/reconstruction.rs:164-168` (one reading
  per non-empty pass), `:477-487` (per-object containment, mirroring
  `gc.rs:402-416`), `:581` / `:533-539` (refusal counted and named once per
  object, not once per chunk).
  **Test:** `crates/custodian/tests/segmented_map_reconstruction.rs` (new) —
  6 legs driving the real fenced entry point `reconcile_step`; with
  production reverted, 5 of 6 fail behaviourally against the base's
  `SegmentedMapUnsupported` abort (leg 6 is a declared regression guard, not
  a base red); with the fix applied, all 6 pass.
- **Claim:** the committed namespace is scanned once per pass, not once per
  queued repair.
  **Checked:** `crates/custodian/src/reconstruction.rs:164-168` (`read_committed`
  called once, only when the queue is non-empty).
  **Test:** `crates/custodian/tests/segmented_map_reconstruction.rs`, the
  scan-count leg — queues 3+ obligations across 3+ objects and asserts
  exactly one `scan(b"inode:")` call, versus the base's one call per queued
  obligation.
- **Claim:** the existing repair classifications and their gauge accounting
  are unchanged for ordinary (non-multipart) objects.
  **Test:** `crates/custodian/tests/reconstruction.rs` — the pre-existing
  suite, left unmodified and still green.

Fixes #697
