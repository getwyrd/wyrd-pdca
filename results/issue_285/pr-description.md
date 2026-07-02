# Reject invalid stored EC schemes at read time instead of panicking

## Summary
Reading an object whose stored erasure-coding scheme is corrupt or tampered —
for example an inode that records zero data shards — crashes the process instead
of returning a read error, and a scheme recording zero parity shards can even
return bytes for a layout no valid write could ever have produced. This change
validates the stored scheme at the read and reconstruct boundaries before it is
used, so an invalid record fails as a clean, typed read error.

## What to look at
- `crates/core/src/read.rs` — the `EcScheme::ReedSolomon` arm of `read_chunk`:
  the new guard runs *before* any fragment fan-out, returning
  `ReadError::InvalidEcScheme` for a scheme the coder does not support.
- `crates/core/src/erasure.rs` — the new `supported(k, m)` predicate (a thin
  wrapper over the reed-solomon coder's own support check) and the matching guard
  at the top of `reconstruct`, which returns `ErasureError::InvalidScheme`.
- To exercise: construct a `ReedSolomon { k: 0, m }` (or `{ k, m: 0 }`) scheme and
  drive a read/reconstruct. On the current `main` this panics or returns bytes;
  with this change it returns a typed error. The added tests reproduce both cases.

## Root cause
`read_chunk` cast the stored `k`/`m` straight to `usize` and never re-validated
them, though they originate from untrusted inode metadata. With `k == 0` the
`available.len() < k` guard is `0 < 0` (false), so `reconstruct` fell through to
index shard `0` of a possibly-empty slice and panicked; with `m == 0` and all `k`
data shards present, reconstruction never consulted the coder at all and returned
bytes for a scheme the encoder could never have produced.

## Fix
Add `erasure::supported(k, m)`, which delegates to the reed-solomon coder's own
support check — the single source of truth for which schemes are encodable — and
call it at both boundaries before any shard indexing or fragment fetch: in
`reconstruct` (returning `ErasureError::InvalidScheme`) and in `read_chunk`'s
Reed-Solomon arm (returning `ReadError::InvalidEcScheme`). This rejects every
unsupported scheme — `k == 0`, `m == 0`, and any other unsupported `k`/`m` pair —
not just the zero-data-shard case, matching the rule the command-line parser
already enforces on input.

## Verification
- **Claim:** invalid EC parameters read back from stored metadata yield a clean
  `Err`, never a panic or out-of-bounds index.
- **Checked:** `crates/core/src/erasure.rs` — `supported` delegates to the coder's
  `ReedSolomonDecoder::supports`, which rejects `k == 0`, `m == 0`, and
  over-limit pairs; the guard at the top of `reconstruct` returns before the
  `available.len() < k` check and the `available[0]` index that panicked pre-fix.
- **Checked:** `crates/core/src/read.rs` — the `EcScheme::ReedSolomon` arm rejects
  an unsupported stored scheme before computing `n = k + m` or firing any
  `get_fragment_at`, so no fetch is issued for a bad record. This upholds the read
  path's documented "never bad data" contract (`crates/core/src/read.rs:8-16`).
- **Test:** `crates/core/src/erasure.rs` (tests) — `reconstruct(0, …)` panics
  pre-fix, returns `InvalidScheme` post-fix; `m == 0` with all `k` shards present
  returns bytes pre-fix, returns `InvalidScheme` post-fix; a guard-rail test
  confirms legitimate schemes still round-trip.
- **Test:** `crates/core/src/read.rs` (tests) — `read_chunk` against a stored
  `k == 0` scheme panics pre-fix and against `m == 0` silently proceeds pre-fix;
  both return `ReadError::InvalidEcScheme` post-fix without firing a fetch. The
  full `cargo xtask ci` gate (fmt, clippy, build, test, deny, conformance) passes.

Fixes [#285](https://github.com/getwyrd/wyrd/issues/285)
