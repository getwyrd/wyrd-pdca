# PR description

## Summary
**User impact:** an object's chunk list currently has to fit inside a single
100 KB metadata value on the tightest backend. In practice that puts a hard
ceiling on how large an object can be, far below the multi-gigabyte object
sizes this project needs to support — a large upload would eventually hit a
wall that has nothing to do with available storage.

This PR adds the underlying record shape that lets a large object's chunk
list be split across multiple records instead of one, so a later change can
remove that ceiling. It does not yet change any runtime behavior: nothing
writes the new shape yet, and every existing record and code path keeps
working exactly as before.

## What to look at
The new type is `ChunkMap`, an enum with a `Flat` variant (today's inline
list, unchanged) and a `Segmented` variant (a pointer to a table of
segments, each stored separately). It lives in `crates/core/src/metadata.rs`
next to the existing `InodeRecord`. The easiest way to see the behavior is
the new test file `crates/core/tests/segmented_map_record.rs`, which
hand-builds both flat and segmented records as raw bytes and decodes them
through the existing `metadata::decode`/`encode` functions — no new
producer or consumer is wired up in this change.

## Root cause
`InodeRecord.chunk_map` was declared as a bare `Vec<ChunkRef>`, an inline
list serialized straight into the metadata record's JSON value, so the
whole chunk list for an object was bounded by that one value's size limit.

## Fix
Introduces `ChunkMap::{Flat, Segmented}`, discriminated by JSON type on the
wire so every pre-existing flat record still decodes and re-encodes
byte-for-byte identical to what it already was. Adds `SegmentedMap`,
`SegmentGroup`, `SegmentRef` and `SegmentRecord` with their decode-time
structural invariants (segment count agreement, contiguous non-overlapping
byte tiling spanning the object's declared size, non-empty segments, a
valid 32-hex group nonce), and the `seg:`/`seggrp:` key helpers used to
address a segment's own record. Every existing call site that reads
`.chunk_map` is migrated to fail closed (a typed error) rather than treat
an unresolvable segmented map as an empty chunk list.

## Verification
- **Claim:** a legacy flat record round-trips through decode/encode
  byte-identically, and a CAS commit against a store holding the original
  bytes still succeeds.
  - **Checked:** `crates/core/src/metadata.rs:268` (today's bare
    `Vec<ChunkRef>` field) and `:277-286` (the byte-identity/CAS rule this
    change extends to the segmented shape) on `main`.
  - **Test:** `crates/core/tests/segmented_map_record.rs` —
    `legacy_flat_record_round_trips_byte_identically` and
    `legacy_flat_record_cas_still_commits_against_the_original_bytes` — both
    fail to compile-time-relevant assertions on `main` (the segmented shape
    does not exist there) and pass with this change.

- **Claim:** a well-formed segmented root decodes, and each structural
  invariant has a raw-byte negative case that is rejected.
  - **Checked:** the invariants enforced at decode in this PR's
    `crates/core/src/metadata.rs` (segment count, index order, contiguous
    span, non-empty segment, 32-hex nonce).
  - **Test:** `crates/core/tests/segmented_map_record.rs` —
    `well_formed_segmented_root_decodes` plus one negative test per
    invariant (`segment_count_mismatching_segments_len_is_err`,
    `duplicate_segment_index_is_err`, `segment_index_gap_is_err`,
    `overlapping_segment_spans_is_err`, `non_monotonic_segment_span_is_err`,
    `an_empty_segmented_map_is_err`, `a_zero_byte_segment_is_err`,
    `non_hex_nonce_is_err`) — fail as assertions on `main` (raw segmented
    bytes don't decode there), pass with this change.

- **Claim:** a segmented root at the largest allowed segment count still
  fits inside the 100 000-byte value ceiling every backend enforces.
  - **Checked:** the capacity constants in this PR's
    `crates/core/src/metadata.rs` (`MAX_ROOT_SEGMENTS`, `MAX_VALUE_BYTES`,
    `MAX_ROOT_VALUE_BYTES`).
  - **Test:** `crates/core/tests/segmented_map_record.rs` measures
    `encode(...).len()` against an upper bound over every possible root at
    that segment count, not a single hand-built example.

Fixes #648
