# PR description

## Summary
**User impact:** When a disk sector holding one piece of an object goes bad, reads
still succeed — the system rebuilds the object from the surviving pieces — but the
damage is then quietly forgotten. Nothing schedules the broken piece for repair, so
the object keeps running with less protection than intended until a background
integrity sweep eventually stumbles over the same bad sector. Until then the cluster
carries avoidable risk of data loss that no queue or dashboard shows.

This PR makes the read path itself report the damage: a read that works around a
permanently unreadable piece now also puts that piece on the repair queue, marked
distinctly from data corruption.

## What to look at
The change is one new error-handling arm in the erasure-coded read path
(`crates/core/src/read.rs`), plus the plumbing to carry the finding to the two
places the read path already enqueues repairs. To try it, run the new regression
test:

```
cargo test -p wyrd-core --test read_block_fault_repair
```

It simulates a three-fragment RS(2,1) object where one fragment's storage backend
reports a dead sector: the read still returns the correct bytes, and the repair
queue now holds the affected chunk.

## Root cause
The RS fan-out's error handling records a typed integrity fault as corrupt and
enqueues it (`crates/core/src/read.rs:362-365` on `main`), but every other fetch
error — including a permanent `BlockReadFault` — falls into the final catch-all,
is emitted as `FaultClass::Transient` telemetry only, and is enqueued nowhere
(`read.rs:379`; the arm's own comment deferred the question to #431). Yet
`wyrd_traits::BlockReadFault` is documented permanent damage — retrying the same
fetch cannot help (`crates/traits/src/lib.rs:164-199`) — and the custodian already
classifies it as a permanent read fault on the rebuild side
(`crates/custodian/src/reconstruction.rs:475`).

## Fix
A dedicated match arm ahead of the transient catch-all recognises the fault through
`wyrd_traits::is_block_read_fault` (`crates/traits/src/lib.rs:339` — the single
decision point for permanence, not re-derived inline), reads around it exactly as
any other excluded shard, and collects the chunk on a new `block_fault` finding list
threaded alongside the existing `corrupt` one. Both drain onto the same shared
repair queue via `repair::enqueue_repair` (`crates/core/src/repair.rs:78`), but
block faults record `detected_by = "read-block-fault"` and bump a new
`FaultClass::BlockFault` counter — a dead sector is not checksum corruption, so the
corruption-specific counters and the `"read"` reason are untouched. All other
non-integrity errors (timeouts, unavailable) remain transient and un-enqueued.

## Verification
- **Claim:** a foreground RS read that encounters a block-layer read fault on one
  shard (while ≥ k others remain readable) still returns the correct bytes AND lands
  the chunk on the shared repair queue with a non-corruption `detected_by` reason,
  without incrementing the corruption-specific fault signals.
- **Checked:** `crates/core/src/read.rs:362-380` on `main` — pre-fix, the block
  fault falls into the final transient arm and no obligation is recorded;
  `crates/core/src/repair.rs:78-98` on `main` — the enqueue seam and queue read-back
  the fix and test use, shared with the scrub producer.
- **Test:** `crates/core/tests/read_block_fault_repair.rs` — fails pre-fix (the
  queue-content assertion: `left: []`, `right: [2970417687]`) and passes post-fix.
  It also reads the repair entry's value back through the `MetadataStore` and
  asserts the recorded reason is `"read-block-fault"`, distinct from the corruption
  producers' `"read"`.
- `cargo fmt -- --check` and `cargo clippy -p wyrd-core --tests` are clean on the
  changed crate.

Fixes #431
