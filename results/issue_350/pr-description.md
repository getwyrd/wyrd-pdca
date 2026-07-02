# Backfill identity placement on pre-M3 committed chunks

## Summary

Clusters that predate M3 hold committed chunk records whose fragment
**placement was never written out** — the vector is empty, and each fragment is
only resolved implicitly at read time by falling back to its identity D-server.
That compatibility fallback is meant to be temporary, but today there is no way
to tell how many such records remain, and records that no other maintenance loop
ever touches never get upgraded — so the migration-only path stays load-bearing
with no route to retirement. This adds a custodian **backfill pass** that
rewrites each empty placement to its explicit full-length identity vector and
emits how many empty-placement records still remain, so the old population is
both drained and watchable as it drops to zero.

## What to look at

- `crates/custodian/src/backfill.rs` — the new pass. `reconcile()` is the whole
  story: scan committed records, classify each chunk's placement, fill the empty
  ones under a version-conditional commit, then emit the remaining count.
- To exercise it: `cargo test -p wyrd-custodian --test backfill` builds an
  in-memory metadata store, commits an inode whose chunk carries `placement:
  vec![]` (a pre-M3 record), runs a pass, and checks the record now carries an
  explicit identity vector and the remaining-count gauge reads zero.
- This pass rewrites metadata only — **no fragment moves**, and the read and
  write paths are untouched. A good first pass is confirming the fill is exactly
  the value the read path already resolves to (see Verification).

## Root cause

`ChunkRef::placed_dserver` resolves a missing placement entry to its identity
D-server (`crates/core/src/metadata.rs:119-124`), so a pre-M3 record with an
empty vector reads correctly but never records its placement explicitly.
Reconstruction and rebalance materialize the full vector when they happen to act
on a chunk, but the long tail of untouched committed records has nothing to drain
it and nothing to measure it — so the fallback cannot be safely removed.

## Fix

A new custodian pass scans every committed inode record and classifies each
chunk's placement through the shared classifier (`ChunkRef::checked_fragments`,
`crates/core/src/metadata.rs:174-185`): **empty** → materialize the explicit
full-length identity vector `(0..fragment_count())`; **already full-length** →
left untouched (idempotent); **malformed** (non-empty, wrong length) → left
exactly as committed and surfaced as an operator signal, never rewritten. Each
touched record is committed version-conditionally on its exact prior record —
the same compare-and-set the writers and other custodians already race through —
so a concurrent writer wins the race and the fill is retried on a later pass
instead of clobbering the winner. Every pass emits the count of empty-placement
records still remaining on the durability telemetry seam. Converting the read
fallback itself into a hard error is intentionally deferred to follow-up #363,
gated on that count reaching zero in production.

## Verification

- **Claim:** an empty-placement committed chunk is rewritten to its explicit
  full-length identity vector (`placement.len() == fragment_count()`,
  `placement[i] == i`), committed under one prior-record CAS.
  - **Checked:** the fill materializes exactly what the read path resolves to —
    `placed_dserver` returns `unwrap_or(index)` at
    `crates/core/src/metadata.rs:119-124`, and the pass fills `(0..n).map(...)`.
  - **Test:** `crates/custodian/tests/backfill.rs` —
    `backfills_identity_placement_for_an_empty_placement_committed_chunk`.
- **Claim:** a racing writer wins the compare-and-set; backfill does not clobber
  it and converges on a later pass.
  - **Checked:** the commit is version-conditional on the prior record, mirroring
    `rebalance.rs:evacuate_chunk` (`crates/custodian/src/rebalance.rs:286-287`).
  - **Test:** `a_racing_writer_wins_the_cas_and_backfill_retries_on_a_later_pass`.
- **Claim:** a malformed (non-empty, wrong-length) vector is never rewritten.
  - **Checked:** classification reuses the shared validator
    (`crates/core/src/metadata.rs:159-185`); the malformed arm only audits.
  - **Test:** `malformed_placement_is_never_rewritten`; idempotence of an
    already-explicit vector is covered by
    `already_explicit_full_length_placement_is_left_untouched`.
- **Claim:** the remaining empty-placement population is observable and reads
  zero once the store is covered.
  - **Checked:** emitted on the durability seam using the existing gauge idiom
    (`crates/custodian/src/rebalance.rs:318-321`).
  - **Test:** `emitted_remaining_count_reaches_zero_once_backfill_covers_the_store`
    reads the count back off the Prometheus surface and asserts `0`.
- The new test file fails to compile against the base (the module does not exist)
  and passes with the change applied; the full `cargo xtask ci` gate — fmt,
  clippy, build, `test --workspace`, deny, conformance, and the DST sweep —
  passes with the change applied.

## Notes for reviewers

- The pass exposes `reconcile()` as a directly-callable `pub` entry rather than
  wiring it into the shared `reconcile_step` dispatch
  (`crates/custodian/src/reconciliation.rs:65-106`). The issue marks that hosting
  illustrative; threading it through would touch ~72 positional call sites across
  unrelated test suites, so it is left to a mechanical follow-up. Confirm shipping
  the pass unscheduled is acceptable for this slice.
- Telemetry names (`backfill_placement_remaining`, `backfill_chunks_filled`,
  `backfill_malformed_placement`, `backfill_conflict`) follow the existing
  `<loop>_<noun>` convention and are a maintainer sign-off item.

Fixes [#350](https://github.com/getwyrd/wyrd/issues/350)
