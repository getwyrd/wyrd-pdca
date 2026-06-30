# Schedule repair when a read hits a fragment a backend reports as corrupt

## Summary

When a client reads an object and one of its fragments is corrupt, the read
is supposed to leave behind a repair obligation so the corruption gets fixed
automatically. That only happened when the read decoded the bad bytes itself.
When a verifying backend — the on-disk store, or the gRPC client translating a
`DATA_LOSS` response — reported the corruption as a typed `IntegrityFault`
*instead of* returning the bad bytes, the read threw the signal away: a
single-fragment object failed with nothing queued, and an erasure-coded object
read back fine but the bad shard was never recorded for repair. In both cases
the corruption sat unrepaired. This change makes the read path treat that fault
as a corruption finding and enqueue the chunk for repair, matching what the
background scrubber already does.

## What to look at

- `crates/core/src/read.rs`, `read_chunk` — the two places a fragment is
  fetched: the `EcScheme::None` branch (single fragment) and the
  `EcScheme::ReedSolomon` branch (the `while let` loop over in-flight fetches).
  Both now inspect the fetch error and, when it is a corruption fault, push the
  chunk id onto the `corrupt` list (which becomes a queued repair at
  `read.rs:251`).
- The classifier is `wyrd_traits::is_integrity_fault`, called exactly as the
  scrubber calls it — no new behaviour invented, the read path is simply
  brought in line with the scrub path.
- To exercise it: read an object whose fragment fetch returns
  `Err(IntegrityFault { … })` (the shape `FsChunkStore` and the gRPC client
  produce on a checksum failure) and check the shared repair queue. The new
  tests in `crates/core/tests/read_repair.rs` do this with an in-memory store
  double that injects the fault for one fragment.

## Root cause

In `read_chunk` the `EcScheme::None` branch fetched the fragment with `.await?`
(`read.rs:136` on `main`), so any `Err` — including an `IntegrityFault` — short
-circuited the function before the corruption could be recorded. The
`EcScheme::ReedSolomon` branch used `if let Ok(Some(fragment)) = fetched`
(`read.rs:189` on `main`), which silently discards every `Err` variant, so an
`IntegrityFault` shard was indistinguishable from an absent one and was read
around without ever being enqueued.

## Fix

Both fetch sites now `match` on the result and apply the same
corruption-vs-transient split the scrubber uses (`crates/custodian/src/scrub.rs:102`
on `main`):

- A fetch that returns an `IntegrityFault` records the chunk as a repair
  obligation. For `EcScheme::None` it is recorded before the (unavoidable)
  error is surfaced; for `EcScheme::ReedSolomon` it is recorded and the read
  continues, reconstructing from the surviving shards.
- A transient / non-integrity error keeps its existing behaviour — propagated
  for `EcScheme::None`, dropped as absent for `EcScheme::ReedSolomon` — and is
  never recorded as corruption.

## Verification

- **Claim:** an `IntegrityFault` from a fragment fetch becomes a durable repair
  obligation, while a transient error does not.
  - **Checked:** `crates/core/src/read.rs` — the `EcScheme::None` and
    `EcScheme::ReedSolomon` arms in `read_chunk` push the chunk id only on the
    `is_integrity_fault` branch; the queued repair is written at `read.rs:251`
    (`repair::enqueue_repair(meta, chunk, "read")`).
  - **Checked:** the same classifier on the scrub path,
    `crates/custodian/src/scrub.rs:102` on `main`, confirming the read path now
    matches the established corruption-vs-transient contract.

- **Claim:** an `EcScheme::ReedSolomon` read still succeeds by reading around
  the faulted shard, and the chunk is queued.
  - **Test:** `crates/core/tests/read_repair.rs` —
    `ec_read_enqueues_integrity_fault_shard_for_repair_and_reconstructs`
    reconstructs the object from the two survivors and asserts the chunk is on
    the repair queue. Fails pre-fix (no queue entry), passes post-fix.

- **Claim:** an `EcScheme::None` read records the chunk before surfacing the
  unavoidable read error.
  - **Test:** `crates/core/tests/read_repair.rs` —
    `none_read_enqueues_integrity_fault_fragment_for_repair` asserts the read
    fails and the chunk is on the repair queue. Fails pre-fix, passes post-fix.

  Both tests inject the fault with an in-memory store double whose
  `get_fragment`/`get_fragment_at` returns `IntegrityFault` for one fragment —
  the same shape `FsChunkStore` and the gRPC client emit. The full
  `cargo xtask ci` gate passes with both regressions green.

Fixes #288
