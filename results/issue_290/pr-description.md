# Fix panic reading a corrupt inode with an oversized size

## Summary

Reading an object whose stored inode carries a corrupt or tampered size — for
example a huge value like `u64::MAX` backed by an empty or short chunk map —
could crash the read with a capacity-overflow panic, or trigger an allocation
proportional to that untrusted size (up to an out-of-memory abort), instead of
failing with a clean read error. This stops the read path from sizing its output
buffer from the inode's recorded size before that size has been validated
against the actual data.

## What to look at

- `crates/core/src/read.rs` — `read_object_collecting`. The buffer that
  reassembles an object used to be preallocated from `inode.size`; it now starts
  empty and grows only from bytes the chunk reads return.
- **To reproduce on `main`:** build a committed `InodeRecord` with
  `size: u64::MAX` and an empty `chunk_map`, then drive the read reassembly path.
  Pre-fix it panics with "capacity overflow" before the size check runs;
  post-fix it returns a typed size-mismatch error.

## Root cause

`read_object_collecting` allocated its output buffer with
`Vec::with_capacity(inode.size as usize)` before it had verified that the chunk
map could produce that many bytes. `inode.size` is untrusted metadata decoded
straight from stored JSON with no bound checks, so a corrupt value drove the
allocation directly — and the reassembled-vs-recorded size check only ran
afterward, too late to prevent the panic or oversized allocation.

## Fix

Drop the capacity hint entirely: the buffer starts as `Vec::new()` and grows
only from bytes each chunk read has already fetched and checksum-verified. The
recorded `inode.size` is no longer consulted until the existing equality check,
which surfaces any discrepancy as a typed size-mismatch error. A corrupt
oversized inode now reads through to that check and fails cleanly, without a
panic and without an allocation that scales with the untrusted field.

## Verification

- **Claim:** Reading a committed inode with a wildly oversized `inode.size`
  (e.g. `u64::MAX` with an empty chunk map) returns a clean typed error — no
  panic, and no allocation proportional to the untrusted size.
- **Checked:** `crates/core/src/read.rs:79` on `main` — the buffer is no longer
  preallocated from `inode.size`; it grows from chunk bytes and the size is
  validated at the mismatch check (`crates/core/src/read.rs:83-89`, returning
  `ReadError::SizeMismatch` defined at `crates/core/src/read.rs:307`).
- **Test:** `crates/core/src/read.rs` (tests module) —
  `oversized_inode_size_with_empty_chunk_map_errors_cleanly_not_panics` builds
  the `u64::MAX` / empty-`chunk_map` inode against a chunk store whose every
  fetch would `unreachable!()`, proving the empty map fails on the size check
  rather than reaching a fragment fetch. Fails pre-fix (capacity-overflow panic),
  passes post-fix (clean `Err`).

Reported and tracked in [#290](https://github.com/getwyrd/wyrd/issues/290).

Fixes #290
