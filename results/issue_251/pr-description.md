# Read around a placed fragment's permanent read fault during repair

## Summary

A single disk with a dead sector could stall repair for an entire cluster. When
the custodian assesses a chunk for repair it reads every already-placed fragment;
previously any read error from one fragment aborted the whole assessment, so one D
server returning a block-layer `EIO` halted reconciliation for *every* chunk on the
shared repair queue — and a disk that goes bad after its data has landed could
never be repaired. This change reads around a permanent read fault: the faulted
fragment is treated as missing and the chunk is rebuilt from its surviving shards,
while a transient (healthy-server) error is still propagated for retry rather than
mistaken for permanent loss.

## What to look at

- `crates/custodian/src/reconstruction.rs`, function `assess` — the per-fragment
  fetch now classifies a fault instead of propagating it with `?`. Two small helpers
  draw the line: `is_permanent_read_fault` and `is_block_read_fault`.
- To exercise it: run `cargo test -p wyrd-custodian --test reconstruction`. The new
  cases place a chunk, swap one server for a store whose reads fail, and assert the
  chunk is still reconstructed for a permanent fault and *not* re-placed for a
  transient one.

## Root cause

`assess` fetched each placed fragment with `store.get_fragment(frag).await?`, and the
bare `?` propagated any non-`NotFound` error out of the per-chunk assessment, aborting
the shared reconciliation pass. The read path and the scrub loop already tolerate
exactly this kind of fault — reading the unreadable fragment around and rebuilding
from the survivors — but reconstruction did not.

## Fix

Replace the bare `?` with a classify-at-the-seam match: a permanent durability fault
(a corruption/integrity fault, or a block-layer `EIO` where the device cannot return
the bytes) is treated as a missing shard and read around; a transient fault is
returned so the retry policy decides. `is_block_read_fault` walks the error's
`source()` chain so an `EIO` is found whether the backend surfaces it directly or
wraps it inside its own error type. No seam type or on-disk format is changed; scrub
and the read path are untouched.

## Verification

- **Claim:** A placed fragment whose store returns a permanent `EIO` read fault is read
  around — `assess` rebuilds from the surviving shards and reports the chunk repairable
  — instead of propagating the error and aborting the pass.
  - **Checked:** `crates/custodian/src/reconstruction.rs:246` on `main` is the pre-fix
    `store.get_fragment(frag).await?` that propagates the fault; the precedents it now
    mirrors are `crates/core/src/read.rs:189` (the read path admits only `Ok(Some(_))`)
    and `crates/custodian/src/scrub.rs:102` (classify-and-continue on an integrity fault,
    propagate otherwise).
  - **Test:** `crates/custodian/tests/reconstruction.rs` —
    `reads_around_a_depth0_permanent_read_fault_on_a_placed_fragment` and
    `reads_around_a_wrapped_permanent_read_fault_on_a_placed_fragment` fail pre-fix
    (the `EIO` propagates and the chunk is never repaired) and pass post-fix.

- **Claim:** The `EIO` shape the tests inject matches what a real faulting disk produces,
  so the fix is not a no-op against production.
  - **Checked:** `crates/chunkstore-fs/src/lib.rs:241` on `main` — a non-`NotFound`
    `fs::read` error is surfaced as `Err(e.into())`, boxing the `io::Error` directly, so
    a real dead-sector `EIO` reaches `assess` with `raw_os_error() == Some(5)` reachable
    (depth 0). The wrapped fixture additionally drives the `source()` walk at depth 1, so
    a backend that boxes the fault inside its own error type is covered too.

- **Claim:** A transient (healthy-server) fault is not converted into permanent fragment
  loss or a re-placement — it is propagated for retry, the fragment stays put, and the
  repair obligation stays queued.
  - **Checked:** the permanent-vs-transient distinction is the durability-seam contract
    documented at `crates/traits/src/lib.rs` (the `is_integrity_fault` classifier at
    `:107`); the fix preserves it at this consumer rather than swallowing the error.
  - **Test:** `crates/custodian/tests/reconstruction.rs` —
    `a_transient_fault_is_not_turned_into_a_spurious_re_placement` passes post-fix and
    guards against an over-broad swallow (it would fail if a transient error were treated
    as a loss: the placement, version, and queued obligation all stay unchanged).

Fixes #251
