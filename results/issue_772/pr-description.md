## Summary
**User impact:** Multipart uploads need a per-chunk record that tracks which
upload owns a not-yet-committed chunk and where its erasure-coded pieces are
planned to go. That record shares its on-disk shape with the plain write
lease the server already stores for ordinary writes, and nothing stopped the
two from being confused. If they ever were, a multipart upload's ownership
of a chunk could be silently erased the next time its lease was renewed, or
the chunk could be reclaimed by routine cleanup as if the write had been
abandoned — corrupting or losing part of an in-progress upload with no error
raised anywhere.

This change adds the multipart staging record and closes that gap: every
place that reads or writes the plain-lease record now checks which "kind" of
record it's looking at, and refuses to treat one as the other. A record that
can't be read at all (corrupt, or torn between the two shapes) is skipped by
cleanup rather than deleted or silently rewritten, and cleanup still finishes
processing everything else. No multipart uploads use this record yet, so
this PR has no effect on current behavior — it's the safe foundation the
next multipart feature builds on.

## What to look at
- `crates/core/src/multipart.rs` — the new staging record (`OwnedEntry`,
  `StagedPlacement`) and its key-aware decoder (`decode_owned_entry`), which
  is the only supported way to read or build one.
- `crates/core/src/metadata.rs` — `PendingEntry` (the plain write-lease
  record) gains two optional fields and one shared decode function
  (`decode_pending_entry`) that every reader now goes through, so a
  misfiled staging record is rejected instead of silently accepted.
- `crates/core/src/write.rs` and `crates/custodian/src/gc.rs` — the two
  cleanup sweeps that scan write leases; both now skip and report a record
  they can't read as an ordinary lease, instead of aborting or reclaiming it.
- To exercise it: `cargo test -p wyrd-core --test multipart_owned_staging`
  and `cargo test -p wyrd-custodian --test gc` run the new coverage,
  including seeded random populations that mix ordinary, staged, torn and
  corrupt records in every possible scan order.

## Root cause
The two record kinds share one serialized shape by design, distinguished
only by which key prefix they're stored under (`pending:` vs `sidx:`). Every
existing reader decoded generically without checking that prefix, so a
value of the wrong shape under either prefix would decode successfully and
be acted on as if it were valid.

## Fix
Adds the staging record type with a public checked constructor (so code
outside this crate can build one safely), and a key-taking decoder that
validates the record's shape, its ownership against the key, and its
erasure-coding geometry. On the existing write-lease side, adds one decode
function per namespace that every reader (lease renewal, live-lease guards,
and both maintenance sweeps) now uses, so a value of the wrong shape is
rejected rather than silently accepted. The two maintenance sweeps classify
and skip anything they can't read, then report what they skipped, rather
than aborting the whole pass or reclaiming a record on doubt.

## Verification
- **Claim:** a valid staging record round-trips under its own key, and every
  torn, misfiled, or geometrically invalid value is rejected with a typed
  error at decode time.
  **Checked:** `crates/core/src/multipart.rs` (`decode_owned_entry`,
  `OwnedEntry`, `StagedPlacement`) and `crates/core/src/metadata.rs`
  (`decode_pending_entry`, `PendingEntry`) on `main`.
  **Test:** `crates/core/tests/multipart_owned_staging.rs` — new file;
  doesn't exist pre-fix (the types it exercises don't exist on `main`),
  passes post-fix (14 cases).
- **Claim:** every existing plain write-lease record decodes and re-encodes
  byte-for-byte identically to before this change, so today's behavior is
  unaffected.
  **Checked:** `crates/core/src/metadata.rs` (`PendingEntry`'s
  `skip_serializing_if` fields) on `main`.
  **Test:** `crates/core/tests/multipart_owned_staging.rs` (the
  serialization-identity cases) — asserts identity is checked, not assumed.
- **Claim:** a cleanup sweep that meets a record it can't read as an
  ordinary lease skips it (rather than reclaiming it or aborting), reports
  what it skipped, and still finishes reclaiming everything else — in any
  scan order, since storage doesn't guarantee an order.
  **Checked:** `crates/core/src/write.rs` (`sweep_expired_leases`) and
  `crates/custodian/src/gc.rs` (`expired_pending_chunks`) on `main`.
  **Test:** `crates/core/tests/multipart_owned_staging.rs` (seeded
  populations run over a real store) and `crates/custodian/tests/gc.rs`
  (`expired_lease_input_skips_what_it_cannot_read_under_every_scan_order`,
  32 seeds run in both a drawn order and its reverse) — fails pre-fix (the
  functions these tests call don't exist yet on `main`), passes post-fix.

Fixes #772
