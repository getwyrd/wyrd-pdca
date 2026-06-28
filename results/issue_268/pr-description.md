# Reconstruct remote dead-sector reads as block-read faults

## Summary

A dead-sector read failure (POSIX `EIO`) on a fragment stored on a networked
D server was indistinguishable, on the client, from a transient network blip —
so reconstruction retried the permanently-unreadable fragment forever instead
of reading around it and rebuilding from the surviving fragments. Read-around
already worked for local fs-backed stores; it never reached the networked path,
which is the dominant production transport. This change makes a block-layer read
fault keep its identity across the gRPC wire so the remote path behaves exactly
like the local one.

## What to look at

- `crates/chunkstore-grpc/src/server.rs` — the `get_fragment` error mapping: a
  new branch sends a block-layer read fault as `FAILED_PRECONDITION` instead of
  flattening it to `INTERNAL`.
- `crates/chunkstore-grpc/src/client.rs` — `classify_get_status`: the matching
  branch reconstructs `FAILED_PRECONDITION` into the new `BlockReadFault` seam
  type.
- `crates/traits/src/lib.rs` — the new `BlockReadFault` type and the
  `is_block_read_fault` predicate; this is the crux. `BlockReadFault` is
  deliberately **not** an integrity fault, and its `source()` exposes a synthetic
  `EIO` so the existing reconstruction classifier recognises it unchanged.
- To exercise it: stand up a gRPC client against a D-server service whose inner
  store returns `io::Error::from_raw_os_error(5)`, fetch a fragment, and inspect
  the client-side error — see `crates/chunkstore-grpc/tests/read_fault_seam.rs`,
  which drives this over a real in-process tonic channel.

## Root cause

The server mapped only an integrity fault to `DATA_LOSS` and flattened every
other failure — including an `io::Error` with `raw_os_error() == Some(5)` — to
`INTERNAL`; the client reconstructed a typed fault only for `DATA_LOSS` and
turned everything else into a transport error with no `io::Error` in its source
chain. The reconstruction classifier walks that chain looking for an `EIO`, found
nothing, and so classified a real remote dead sector as transient and retried it.

## Fix

Introduce a third, distinct fault category on the seam instead of overloading the
two that exist. A block-layer read fault now travels as `FAILED_PRECONDITION`
(distinct from `DATA_LOSS` for corruption and from the transient codes) and is
reconstructed client-side into a new `BlockReadFault`. That type is permanent —
its `source()` exposes a synthetic `EIO`, so the existing chain-walking classifier
in reconstruction treats it as a read-around without any consumer-side change —
but it is not an integrity fault, so scrub does not record a dead sector as a
corruption finding; it falls through to the same branch a local `EIO` takes. A
generic non-`EIO` server error still arrives as a transport error and keeps its
retry behaviour. The errno-5 closure is defined once on the seam
(`is_block_read_fault`) so the server, client, and reconstruction classifier
agree; the local and remote classification paths are otherwise untouched.

## Verification

- **Claim:** A remote `EIO` read fault is classified as a permanent block-read
  fault (read around it, do not retry).
  - **Checked:** `crates/chunkstore-grpc/src/server.rs:83-88` and
    `crates/chunkstore-grpc/src/client.rs:25-33` on `main` — pre-fix the server
    sends `INTERNAL` for a non-integrity error and the client wraps it as a
    transport error, which `crates/custodian/src/reconstruction.rs:327-349`
    (`is_permanent_read_fault` / `is_block_read_fault`) cannot recognise because
    no `io::Error(EIO)` is in the source chain.
  - **Test:** `crates/chunkstore-grpc/tests/read_fault_seam.rs` —
    `remote_eio_is_block_read_fault_not_corruption_over_grpc` asserts
    `is_block_read_fault(err)` is true over the wire. Fails pre-fix, passes
    post-fix.

- **Claim:** A remote `EIO` read fault is **not** a corruption finding — scrub
  treats it the same as a local `EIO`.
  - **Checked:** `crates/custodian/src/scrub.rs:102-108` on `main` — only
    `is_integrity_fault` enters the `emit_corruption` / repair-enqueue branch;
    everything else hits `Err(e) => return Err(e)` (the local-`EIO` path).
    `BlockReadFault` is not an integrity fault, so it takes that fall-through
    unchanged.
  - **Test:** same test asserts `is_integrity_fault(err)` is false over the wire.
    Fails pre-fix (the prior approach reused the corruption carrier), passes
    post-fix.

- **Claim:** A genuinely transient remote fault keeps its existing transport-error
  classification (no spurious re-placement).
  - **Checked:** `crates/chunkstore-grpc/src/client.rs:25-33` on `main` — the
    catch-all arm still produces a transport error.
  - **Test:** `remote_generic_error_stays_transient_transport_error_over_grpc`
    asserts a non-`EIO`, non-integrity error is neither a block-read fault nor an
    integrity fault and arrives as a `TransportError`. Passes post-fix.

- **Whole gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test, deny,
  conformance) exits 0.

Fixes #268
