## Summary

Background scrub is supposed to be the safety net that catches bit rot: when
a stored fragment's bytes go bad, scrub should queue its chunk for
reconstruction, count the corruption on the durability metrics, and keep
checking the rest. Today it does the opposite — the **first** corrupt
fragment aborts the whole pass, so the rot is never queued for repair, every
fragment behind it goes unchecked, and the metrics report all-green while
data quietly decays below its redundancy floor. This change makes scrub
classify a corrupt fragment as a repair obligation and continue, restoring
the durability guarantee for both the on-disk and the networked D server.

## What to look at

- `crates/custodian/src/scrub.rs` — the fetch in `reconcile` is now a `match`:
  the crux of the fix. A corrupt fragment is enqueued and the loop continues;
  a transient fault still propagates.
- `crates/traits/src/lib.rs` — the small shared `IntegrityFault` type and the
  `is_integrity_fault` classifier that let scrub tell "bad bytes" from
  "backend unreachable" without depending on any concrete store.
- `crates/chunkstore-grpc/src/{server.rs,client.rs}` — how that distinction
  survives the network seam (corruption travels as `DATA_LOSS`, reconstructed
  client-side).
- To exercise it: populate an `FsChunkStore` with several referenced
  fragments, flip a byte in one that is **not** last in iteration order, and
  run a scrub pass — see `crates/custodian/tests/scrub.rs`.

## Root cause

`FsChunkStore::get_fragment` verifies on read and returns `Err` for a corrupt
or misfiled fragment, but scrub fetched it with `let Some(bytes) =
store.get_fragment(frag).await? else …` — the `?` propagated that `Err` out
of the whole pass *before* the corruption branch could run. The deeper gap:
the error contract carried no way to distinguish corruption (retrying cannot
help — repair and continue) from a transient fault (retry), so scrub could
not safely do anything but bail.

## Fix

A single typed `IntegrityFault` is added to the shared trait crate — the one
crate every backend and consumer already depend on — so the same type is
produced everywhere and recognised everywhere via `is_integrity_fault`. The
on-disk store maps a read-verify failure to it; the gRPC server carries it as
`DATA_LOSS` (distinct from a transient status) and the client reconstructs
the same type, so a networked store classifies identically to a local one.
Scrub's fetch becomes a `match`: an integrity fault is counted and enqueued
for reconstruction and the pass continues; any other error propagates to the
retry policy. On the same seam, a malformed-fragment PUT now returns
`INVALID_ARGUMENT` instead of `INTERNAL`, so callers stop retrying bytes that
can never verify. The read path is deliberately untouched (its concrete error
type is unchanged; the chunk-id recheck there is tracked separately).

## Verification

- **Claim:** A corrupt referenced fragment is enqueued for reconstruction, a
  corruption metric is emitted, and the pass continues over the remaining
  fragments — it never aborts.
  - **Checked:** `crates/custodian/src/scrub.rs:70` on `main` — the pre-fix
    `store.get_fragment(frag).await?` propagated a verifying backend's `Err`
    out of `reconcile` before the corruption branch at `:79-85` could run.
  - **Test:** `crates/custodian/tests/scrub.rs` —
    `fschunkstore_corruption_is_enqueued_and_the_pass_continues` populates a
    real `FsChunkStore` with three referenced fragments, rots **two**, and
    asserts the pass returns `Changed`, **both** rotten chunks are enqueued
    (order-independent proof it continued past the first), and the corruption
    + coverage metrics emit. Pre-fix the pass returns the aborting `Err` and
    the test panics; post-fix it passes.

- **Claim:** Corruption is distinguishable from a transient fault at scrub's
  decision point, including across the gRPC seam.
  - **Checked:** `crates/chunkstore-grpc/src/server.rs:73` and
    `crates/chunkstore-grpc/src/client.rs:72` on `main` — both mapped every
    store error uniformly (`Status::internal` / `TransportError`), so
    corruption and a transient fault were indistinguishable. The fix adds the
    `DATA_LOSS` path and reconstructs `IntegrityFault` client-side.
  - **Test:** `crates/chunkstore-grpc/tests/round_trip.rs` —
    `get_of_a_rotten_fragment_is_an_integrity_fault_over_grpc` asserts a rotted
    fragment surfaces as an integrity fault and **not** a `TransportError`;
    `crates/custodian/tests/scrub.rs::scrub_propagates_a_transient_get_fault_without_enqueuing`
    asserts a transient fault propagates and enqueues nothing.

- **Claim:** A malformed-fragment PUT is a client fault, not a server-internal
  one that invites futile retries.
  - **Checked:** `crates/chunkstore-grpc/src/server.rs:59` on `main` — a PUT
    verify failure became `Status::internal`. The fix routes an integrity
    fault to `INVALID_ARGUMENT`.
  - **Test:** `crates/chunkstore-grpc/tests/round_trip.rs` —
    `put_of_a_malformed_fragment_is_invalid_argument_over_grpc`.

- **Whole-tree gate:** `cargo xtask ci` (fmt `--check`, clippy `-D warnings`,
  build, full test suite incl. DST, `cargo deny`, conformance) passes.

Fixes #207
