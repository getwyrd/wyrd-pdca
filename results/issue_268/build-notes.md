# Build notes — issue 268 / grpc-block-read-fault-distinct-wire-category

## Root cause (2 sentences)

On `main`, `server.rs:83-89` maps every non-`IntegrityFault` error from `get_fragment` — including `io::Error(EIO)` from a dead sector — to `Status::internal`; `client.rs:25-33` maps `INTERNAL` to `TransportError::Rpc`, which carries no `io::Error` in its source chain. `reconstruction.rs`'s private `is_block_read_fault` therefore finds nothing and `is_permanent_read_fault` returns false, so a remote dead-sector fault is misclassified as transient and retried forever instead of being read around.

## Invariant to restore

Three fault categories — corruption (`IntegrityFault`), block-layer read fault (`EIO` / dead sector), and transient (unreachable / timeout) — must be mutually distinguishable at every consumer (reconstruction `assess`, scrub) across the gRPC wire seam. The v1 fix collapsed dead-sector onto corruption (both used `Code::DataLoss` → `IntegrityFault`), which made scrub `emit_corruption` for a dead-sector fault — wrong. This fix introduces a THIRD wire code + seam type.

## Design chosen

### Wire mechanism
- `DATA_LOSS` (15): integrity/corruption fault → `IntegrityFault` (unchanged)
- `FAILED_PRECONDITION` (9): block-layer read fault → `BlockReadFault` (new)
- `INTERNAL` (13): everything else → `TransportError` (unchanged)

`FAILED_PRECONDITION` was selected because gRPC's own documentation describes it as appropriate when an operation should NOT be retried until the system state is fixed — semantically matches a dead sector that needs device-level repair, not a transient retry. It is clearly distinct from `DATA_LOSS` (corruption) and the transient codes (`UNAVAILABLE`, `DEADLINE_EXCEEDED`).

### `BlockReadFault` seam type (new, `crates/traits/src/lib.rs`)

Parallel to `IntegrityFault`, with two key properties:
1. It is NOT an `IntegrityFault` → `is_integrity_fault` returns false → scrub's `emit_corruption` branch is NOT taken → the local-EIO behavior (`scrub.rs:108 Err(e) => return Err(e)`) is preserved.
2. Its `source()` returns a synthetic `io::Error::from_raw_os_error(5)` (EIO) → `reconstruction.rs`'s private `is_block_read_fault` chain-walker finds it → `is_permanent_read_fault` returns true → reconstruction reads around it.

This is the key design insight: the existing chain-walker in `reconstruction.rs:338-348` already handles `BlockReadFault` via the source chain **without any change to reconstruction.rs or scrub.rs**. No consumer needed touching.

Also added `wyrd_traits::is_block_read_fault` (public) — the single decision point for the EIO closure (errno-5 only, per #251 §6 item 2). The server calls this to detect EIO from the inner store rather than re-deriving the check inline.

### What was NOT changed
- `crates/custodian/src/reconstruction.rs` — the private `is_block_read_fault` chain-walker already handles `BlockReadFault` via `source()` → `io::Error(EIO)`. No change.
- `crates/custodian/src/scrub.rs` — `is_integrity_fault(BlockReadFault) == false` already falls through to `Err(e) => return Err(e)`. No change.
- Local/fs classification, `is_block_read_fault`'s errno closure — explicitly out of scope per brief.
- `IntegrityFault` / corruption semantics — unchanged.

## Alternatives considered and rejected

### Alternative: use `Code::DataLoss` with a typed proto detail to distinguish
**Rejected.** This is the v1 approach (collapsed onto `IntegrityFault`), explicitly out of scope. Adding a proto detail field to distinguish would also require proto schema changes (a new dependency not justified for a status-code-level classification).

**Cost:** proto schema change (~30 lines across proto + build); interpretation logic at every consumer (~10 lines); fragile string/field parsing. Much heavier than a distinct `Status` code.

### Alternative: encode distinction in the message string prefix
**Rejected.** Parsing `Status::message()` for fault category is brittle; any message-logging middleware could strip or modify it. A `Status` code is a stable, first-class gRPC concept.

**Cost:** ~5 lines, but introduces a hidden protocol between server and client that the type system cannot enforce. Less safe, no size advantage.

### Alternative: update `reconstruction.rs::is_block_read_fault` to also check `BlockReadFault` directly (delegating to `wyrd_traits::is_block_read_fault`)
**Considered.** Not strictly needed because `BlockReadFault::source()` exposes `io::Error(EIO)` and the existing chain-walker finds it. Touching reconstruction.rs would also brush against the "out of scope: changing the local/fs classification" boundary.

**Verdict:** Leave reconstruction.rs untouched; the source-chain approach is correct and the brief says not to touch local-path classification.

## Test

`crates/chunkstore-grpc/tests/read_fault_seam.rs` — two tests over a real in-process tonic client↔server channel (same transport as `round_trip.rs`):

1. `remote_eio_is_block_read_fault_not_corruption_over_grpc`: `EioStore` inner store returns `io::Error::from_raw_os_error(5)`; asserts `is_block_read_fault(err) == true` (part a) AND `is_integrity_fault(err) == false` (part b).
2. `remote_generic_error_stays_transient_transport_error_over_grpc`: `GenericErrStore` returns a non-EIO `io::Error`; asserts NOT block-read-fault, NOT integrity-fault, IS `TransportError` (third-category invariant).

Pre-fix: both tests compile against the patched seam type but the EIO test fails because `is_block_read_fault` is not yet in `wyrd_traits` (compilation error → red). Post-fix: `cargo xtask ci` exits 0.

## Citations

- `crates/traits/src/lib.rs:64-81` — updated `IntegrityFault` seam-doc (three-category contract)
- `crates/traits/src/lib.rs:119+` — new `BLOCK_READ_FAULT_ERRNO`, `BlockReadFault`, `is_block_read_fault`
- `crates/chunkstore-grpc/src/server.rs:83-89` — added `is_block_read_fault` → `FAILED_PRECONDITION` branch
- `crates/chunkstore-grpc/src/client.rs:25-33` — added `FAILED_PRECONDITION` → `BlockReadFault::new` branch
- `crates/custodian/src/reconstruction.rs:327-349` — `is_permanent_read_fault` / `is_block_read_fault` chain-walker: NO change; works via `BlockReadFault::source()` → `io::Error(EIO)`
- `crates/custodian/src/scrub.rs:102-108` — `is_integrity_fault` branch + fallthrough `Err(e)`: NO change; `BlockReadFault` falls to `Err(e) => return Err(e)` automatically
