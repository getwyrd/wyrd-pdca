# PR description

## Summary
**User impact:** Nothing yet writes these records in production, but the
in-flight state of a multipart upload — which upload is targeting which
object, which parts are reserved and committed, and where each part's data
lives — had no validated on-disk shape at all. Without this change, a
corrupted or hand-edited stored value (for example, one part naming a
different object than the upload session it belongs to, or a part
reservation that is already expired the moment it is written) would be
silently accepted as valid instead of being rejected, letting one
upload's data end up attributed to the wrong object or a live upload lose
its reserved storage out from under it.

This change adds validation for those records at the point they are read
back out of storage, so a malformed or inconsistent stored value is
refused with a specific, typed error instead of being trusted.

## What to look at
- `crates/core/src/multipart.rs` — the new record types (`SessionRecord`,
  `SlotRecord`, `PartRecord`, `PartSummary`, and their supporting types)
  and their `decode_*` functions. Each type checks its own fields for
  internal consistency the moment it is decoded from stored bytes, rather
  than trusting whatever JSON happens to parse.
- To exercise it without a running store: `cargo test -p wyrd-core --test
  multipart_session_records` decodes hand-written byte strings representing
  both valid and deliberately torn records and asserts each is accepted or
  rejected as expected.
- `docs/design/architecture/05-building-block-view.md` gets one paragraph
  describing the four new persisted key shapes this adds.

## Root cause
These record types (and their decoders) did not exist yet — this is new
functionality, not a bug fix to existing behavior. The records have no
live writer until a later change wires up the store round trips, so the
risk is about what a future or malformed writer could produce reaching
this code unvalidated once that happens.

## Fix
Adds `SessionRecord`/`SessionState`/`PublishTarget`/`Completion` (the
`mpu:` value), `SlotRecord` (`slot:`), and `PartRecord`/`PartSummary`
(`part:`/`psum:`), each validating inside its own `Deserialize` impl and
decoded/encoded through the existing generic `metadata::encode`/`decode`
codec (no new envelope). Chunk references inside a part are read through a
closed wire mirror local to this module, so an unsupported erasure scheme,
an omitted placement list, or an unknown field inside a chunk is rejected
rather than silently defaulted or dropped — while a placement list whose
*length* disagrees with the scheme's fragment count is still accepted, per
the project's contextual-vs-structural validation boundary.

## Verification
- **Claim:** every landed record type round-trips `encode`/`decode`.
  **Checked:** `crates/core/src/multipart.rs:1640-1990` (the record
  structs) on `main`. **Test:** `crates/core/tests/multipart_session_records.rs`
  round-trip tests (e.g. `session_open_round_trips`, `slot_round_trips`,
  `part_summary_round_trips`) — pass post-fix; the whole file fails to
  compile pre-fix since the types it imports do not exist yet.
- **Claim:** a session whose `publish_target` names a different
  bucket/object, or a different fence epoch, than the session's own is
  rejected. **Checked:** `crates/core/src/multipart.rs:1705-1743` (`impl
  TryFrom<SessionRecordWire> for SessionRecord`) on `main`. **Test:**
  `leg_1c_publish_target_key_mismatch_is_rejected`,
  `leg_1c_epoch_publish_target_epoch_mismatch_is_rejected` — fail (assert
  wrong error / value decodes) with the check removed, pass with it in
  place.
- **Claim:** a part's chunk carries only an erasure scheme the coder
  supports, and its declared length matches the checked (overflow-safe)
  sum of its chunks. **Checked:** `crates/core/src/multipart.rs:1853-1912`
  and `:2044-2071` on `main`. **Test:**
  `leg_1i_chunk_scheme_unsupported_is_rejected`,
  `leg_1k_part_length_mismatch_is_rejected`.
- **Claim:** a chunk's placement-list *length* disagreeing with its
  scheme's fragment count still decodes (deliberately not enforced).
  **Checked:** `crates/core/src/multipart.rs:1914-1946` (`ChunkRefWire`) on
  `main`. **Test:** `leg_1i_chunk_ref_wrong_placement_length_still_decodes`.
- **Claim:** a slot reservation whose lease has already expired when
  written is rejected. **Checked:** `crates/core/src/multipart.rs:1811-1832`
  on `main`. **Test:** `leg_1i_slot_lease_already_lapsed_is_rejected`.
- **Claim:** an unknown field anywhere in a stored record — including
  inside a nested chunk or its scheme — is a decode error, never silently
  dropped, so decode-then-re-encode always reproduces the stored bytes
  exactly (required for the compare-and-swap these records are stored
  under). **Checked:** `crates/core/src/multipart.rs:1914-1955` (`ChunkRefWire`
  / `EcSchemeWire`) on `main`. **Test:** `leg_1m_unknown_field_is_rejected`,
  `leg_1m_unknown_field_in_nested_chunk_is_rejected`,
  `leg_1m_unknown_field_in_nested_scheme_is_rejected`,
  `part_chunk_omitted_placement_is_rejected`.
- **Overall:** `cargo test -p wyrd-core --test multipart_session_records`
  — 27 passed, 0 failed on the patched tree; the same file fails to
  compile against unpatched `main` (the types it imports don't exist),
  which is the pre-fix red for this new-functionality change.

Fixes #716
