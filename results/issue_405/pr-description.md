# Add a networked S3 client that records real-time operation history

## Summary
**User impact:** Wyrd promises that when a file is overwritten again and again,
everyone sees those writes land in one consistent order — a read never returns an
older value once a newer one has been saved, and never returns a half-written one.
Today that promise is only asserted: nothing actually exercises the live system and
confirms it holds, so a consistency bug of exactly this kind could slip through
unnoticed.

This PR adds a reusable client that drives writes, reads, and deletes against the
storage service over a real network connection and records a timestamped log of every
operation — giving the consistency checks something concrete to verify against. It is
part of the storage-consistency verification work tracked in #329.

## What to look at
Two new files carry the change: a small client module and one integration test that
uses it. The test starts the storage service on a local port, overwrites the same
object v1 → v2 → v3 with a read after each write (then a delete and a final read), and
confirms the recorded history is complete, correctly ordered in real time, and never
shows a version going backwards. Exercise it with:

```
cargo test -p wyrd-server --test consistency_observable
```

## Root cause
Wyrd's object PUT/GET/DELETE was reachable only in-process (`crates/server/src/lib.rs`);
only the fragment-level gRPC store was networked. The S3 HTTP wire surface now exists
(#448) and `crates/server/tests/s3_http_wire.rs` drives it by hand over a `TcpStream`,
but it records no reusable, real-time-ordered history — so there was no client able to
produce the per-operation, file-as-register history the consistency model (ADR-0041
decision 1) needs to check that a file's writes are linearizable at its home zone
(ADR-0015 guarantee 2).

## Fix
Add `ObservableS3Client` in `crates/server/src/consistency_observable.rs`, exported at
`crates/server/src/lib.rs:17`. It signs each request with the production
`wyrd_gateway_s3::sigv4::sign`, issues PUT/GET/DELETE over a fresh `TcpStream` against
the live listener (mirroring the `s3_http_wire.rs` driving composition), and records an
`OpRecord` per op: kind, key, observed version, HTTP status, and the `[start, end]`
wall-clock span. The wire floor returns no version/ETag header
(`crates/gateway-s3/src/lib.rs:40`), so the register version is carried as the object's
own bytes — a PUT of version *n* writes the decimal digits of *n*, a GET decodes them
back — meaning every operation genuinely commits and re-reads through the gateway with
no wire-surface change. `History` exposes `well_formed()` (non-empty, and every span
non-reversed) and `versions_monotone_per_key()` (no stale or torn read). The verdict
engine and the multi-node partition testing that consume this history are separate,
later slices under #329 and are out of scope here.

## Verification
- **Claim:** A networked client drives overwriting PUT/GET/DELETE over a real loopback
  listener and records, per operation, a `[start, end]` real-time span and the observed
  version — yielding a non-vacuous, well-formed, version-monotone register history.
- **Checked:** `crates/server/src/consistency_observable.rs:158,177,200` — PUT/GET/DELETE
  each stamp `start`/`end` around the real round trip and record the observed version;
  `crates/server/src/consistency_observable.rs:98,106` — `well_formed()` /
  `versions_monotone_per_key()` enforce non-emptiness, non-reversed spans, and no version
  regression; exported at `crates/server/src/lib.rs:17`.
- **Test:** `crates/server/tests/consistency_observable.rs:68`
  (`observable_records_a_nonvacuous_wellformed_register_history`) drives v1 → v2 → v3 with
  interleaved reads plus a delete against a live loopback gateway and asserts the recorded
  8-op history is complete (`:136`), well-formed (`:153`), and version-monotone (`:160`).
  It fails to compile before this change (`error[E0432]: unresolved import
  wyrd_server::consistency_observable` — there is no client to construct) and passes
  after; the full `cargo xtask ci` suite is green.

Fixes #405
