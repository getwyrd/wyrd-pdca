# Build notes — issue 268 / grpc-block-read-fault-wire-signal

## Root cause

**`crates/chunkstore-grpc/src/server.rs:83-89`** — the `get_fragment` handler
checked `wyrd_traits::is_integrity_fault` and sent `Code::DataLoss` only for
checksum failures. A block-layer read fault (`EIO` / errno 5) from the inner
store did not match, so it fell to `Status::internal(e.to_string())`.

**`crates/chunkstore-grpc/src/client.rs:25-34`** — `classify_get_status` mapped
only `DataLoss` → `IntegrityFault`; every other code → `TransportError`. An
`INTERNAL` status became `TransportError::Rpc`. No `io::Error(EIO)` survived in
the error chain.

**`crates/custodian/src/reconstruction.rs` (pre-fix)** — `is_permanent_read_fault`
checked `wyrd_traits::is_integrity_fault(err) || is_block_read_fault(err)`. The
local `is_block_read_fault` walked the chain for `io::Error(raw_os_error == 5)`.
But the `TransportError::Rpc` wrapping a tonic `Status(INTERNAL)` carries no
`io::Error`, so `is_permanent_read_fault` returned `false` for remote EIO.
Result: `assess` hit the `Err(e) => return Err(e)` arm and propagated the error
to the retry policy instead of reading around the dead sector.

## Fix

Four source files + one new test + one Cargo.toml entry.

### 1. `crates/traits/src/lib.rs` (+49 lines after line 116)

Added:
- `const EIO: i32 = 5;` (private) — POSIX errno, defined once at the seam rather
  than per-consumer, as the brief requested.
- `pub fn is_block_read_fault(...)` — walks the error chain for `io::Error(EIO)`.
  Moving this from `reconstruction.rs` to `wyrd_traits` makes it the single source
  of truth for the block-read-fault closure (`errno-5`), accessible to both the
  server-side detection and the reconstruction loop.
- `pub fn is_permanent_read_fault(...)` — `is_integrity_fault || is_block_read_fault`.
  The combined predicate the reconstruction loop branches on is now part of the
  public seam contract (ADR-0010), not a private function in `custodian`. This makes
  it available to the integration test without depending on `wyrd-custodian`.

### 2. `crates/chunkstore-grpc/src/server.rs:83-89` (the bug site)

Changed:
```rust
// before
if wyrd_traits::is_integrity_fault(e.as_ref()) {

// after
if wyrd_traits::is_integrity_fault(e.as_ref())
    || wyrd_traits::is_block_read_fault(e.as_ref())
{
```

`DataLoss` now covers both permanent shapes. The client's existing
`DataLoss` → `IntegrityFault` mapping makes `is_permanent_read_fault` return
`true` on the client side without any client logic change.

### 3. `crates/chunkstore-grpc/src/client.rs:18-25` (comment only)

Updated the doc comment of `classify_get_status` to reflect that `DataLoss` now
covers block-read faults too, not only integrity faults. No logic change.

### 4. `crates/custodian/src/reconstruction.rs` (-45 lines, +4 lines)

- Removed private `const EIO`, `fn is_block_read_fault`, `fn is_permanent_read_fault`
  (moved to `wyrd_traits`).
- Changed call site from `is_permanent_read_fault(e.as_ref())` to
  `wyrd_traits::is_permanent_read_fault(e.as_ref())`.

### 5. `crates/chunkstore-grpc/Cargo.toml` (+3 lines)

Added `async-trait.workspace = true` to `[dev-dependencies]` so the proc macro
is available when compiling the `read_fault_seam.rs` integration test binary
(which uses `#[async_trait]` to implement `ChunkStore` for the fault-injecting
fakes).

## Wire contract decision

Both permanent shapes — integrity fault (checksum fail) and block-read fault
(EIO / dead sector) — travel as `Code::DataLoss`. The client side already mapped
`DataLoss` → `IntegrityFault`; no client change needed. This means a remote EIO
and a remote checksum failure arrive as the same error type on the client, which
is functionally correct: `assess` treats both as "read around it and rebuild."
The `IntegrityFault.detail` string will say "Os { code: 5, kind: … }" for EIO
rather than a checksum reason, distinguishable from log inspection though not
from the type. The brief explicitly says "the mechanism is ILLUSTRATIVE; the
binding condition is the permanent-vs-transient classification surviving the wire
round-trip" — this satisfies that binding condition.

Alternative considered: introduce a dedicated `BlockReadFault` type in
`wyrd_traits` and use a second `Status` code (e.g. `FailedPrecondition`). Cost:
~+80 lines (new error type + Display/Error impls + second branch in server + second
branch in client), plus a new public API type that doesn't need to exist because
no consumer distinguishes EIO from integrity at the read-around decision point.
Ruled out as disproportionate; the brief explicitly says the mechanism is
illustrative.

## Test: `crates/chunkstore-grpc/tests/read_fault_seam.rs`

Two tests over a real tonic client↔server in-process channel:

1. `remote_eio_block_read_fault_is_classified_permanent_over_grpc` — the primary
   regression test. `EioStore` returns `io::Error::from_raw_os_error(5)`. Pre-fix:
   fails to COMPILE (references `wyrd_traits::is_permanent_read_fault` which didn't
   exist) → non-zero exit, correctly RED. Post-fix: `DataLoss` → `IntegrityFault`,
   `is_permanent_read_fault` returns `true`, `TransportError` absent → GREEN.

2. `remote_generic_error_stays_transient_over_grpc` — invariant guard. A non-EIO
   error stays `INTERNAL` → `TransportError::Rpc` → `is_permanent_read_fault` false.
   Prevents false positives (overloaded servers misclassified as dead sectors).

Red→green verified:
- Pre-fix (stash reverts all source changes, test file kept): compile error on
  `wyrd_traits::is_permanent_read_fault` not found → exit 1 (RED).
- Post-fix: both tests pass in 0.00s → GREEN.
- Full `cargo xtask ci` (fmt, clippy, build, test, DST, deny, conformance):
  all checks passed. Two clippy/fmt iterations needed (trailing blank line in
  reconstruction.rs; `io::Error::other` spelling + rustfmt line-break).
