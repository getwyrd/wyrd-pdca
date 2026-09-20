# PR description

## Summary
**User impact:** none yet, directly — this adds a building block, not a behavior
change. The multipart upload protocol already writes "this data can be reclaimed"
markers to storage (bytes to mark as orphaned, or records to delete, once an
upload attempt is superseded, aborted, or an old object generation is replaced).
Until now nothing in the codebase could read those markers back — the code that
names them existed, the code that understands what they mean did not. Left
unfixed, any later feature that tries to reclaim that space has nowhere to plug
in, and a bug in a hand-rolled reader would risk reclaiming the wrong upload's
data.

This PR adds the decoder for that value, plus the type that mints the
part-number lists it can contain, so a later change can safely drain these
markers.

## What to look at
The core new function is `decode_retire_obligation(key, bytes)` in
`crates/core/src/multipart.rs`. It's deliberately called with the *key* as well
as the raw bytes, because part of what makes a marker valid or invalid is
whether it agrees with the key it's stored under (e.g. a marker that only makes
sense for a whole upload session must not be filed under a key naming one part
of it). Anything the decoder can't reconcile comes back as one of the new,
named error variants near the top of the same file, rather than a generic
parse failure.

To exercise it, read `crates/core/tests/multipart_retire_obligation.rs`: it
hand-builds the exact JSON bytes for every shape the protocol is allowed to
write, decodes each one, and separately hand-builds every shape it's *not*
allowed to write and checks each is rejected with the right error.

## Root cause
The record grammar for these markers (proposal 0016) specifies which value
shapes are legal under which keys, but no code existed to read a value and
check it against those rules. Two format questions — whether a marker can
combine multiple pieces of information in one value, and whether a retired
object generation can name two different kinds of storage location at once —
had been left unresolved across several earlier attempts; this PR resolves
both in line with the actual on-disk data model (an object's storage is always
one of two shapes, never both at once), so a decoded value can't quietly
represent something the system never writes.

## Fix
Adds `PartNumberSet` (a compact, canonical way to store "these part numbers"),
`RetirePayload`/`RetireGeneration`/`RetiredMap` (the marker's value shapes),
nine new `RecordError` variants (one per way a stored value can disagree with
its key or with itself), and `decode_retire_obligation(key, bytes)`, which
returns the marker's mode, its key, and the validated value together so a
future caller can't have one without the others. Also extends the
architecture doc's one sentence describing this part of the key space, and
corrects a stale doc comment.

One known gap in the source spec this PR does not touch: proposal 0016's
value table (`0016:355`) reads as if a retired generation could carry both of
its two storage shapes at once; the actual data model does not allow that, and
this PR follows the data model. Fixing the proposal's wording is a separate,
tracked follow-up — this PR only corrects the code's own doc comment and cites
the lines the discrepancy is in.

## Verification
- **Claim:** every value shape the protocol's writers install decodes
  successfully under its own key.
  **Checked:** the ten writer-row shapes are each decoded from hand-built
  bytes in `crates/core/tests/multipart_retire_obligation.rs:257-427`, against
  the writer table transcribed in `crates/core/src/multipart.rs:2924-2939`.
  **Test:** `crates/core/tests/multipart_retire_obligation.rs` (new file) —
  does not exist pre-fix (the types and function it imports don't exist, so
  the crate fails to compile); passes post-fix, 28/28.

- **Claim:** a value shape no writer installs is rejected with a specific,
  named error, not silently accepted or given a generic message.
  **Checked:** `crates/core/src/multipart.rs:3245`
  (`decode_retire_obligation`) and its key-relation checks at
  `crates/core/src/multipart.rs:3109` (`checked_against_key`).
  **Test:** ten separate rejection tests, e.g.
  `crates/core/tests/multipart_retire_obligation.rs:495` (wrong storage mode
  for the key), `:544` and `:564` (marker doesn't match the key's scope),
  `:580` (generation identity mismatch), `:623` (segment epoch mismatch),
  `:677` (unsupported chunk encoding), `:697` (non-canonical part-number
  spelling), `:444` (a generation naming both storage shapes at once). Each
  was confirmed to isolate its own rule: removing the one production check
  behind it fails exactly that one test and no other (documented per-check in
  the review record).

- **Claim:** a generation marker names exactly one storage shape, never both
  or neither.
  **Checked:** `crates/core/src/multipart.rs:2675` (`RetiredMap`, the
  two-shape type) and `:2771` (the decode that enforces it).
  **Test:** `crates/core/tests/multipart_retire_obligation.rs:444` (both
  present, rejected) and `:467` (neither present, rejected).

- **Claim:** every accepted value re-encodes to the exact same bytes it was
  decoded from (needed because these markers are compared byte-for-byte
  elsewhere in the storage protocol).
  **Checked:** `crates/core/src/multipart.rs:3245` ends every decode by
  requiring the re-encoded bytes match the input exactly.
  **Test:** `crates/core/tests/multipart_retire_obligation.rs:729` (a
  differently-spelled-but-equivalent value is rejected precisely because it
  wouldn't re-encode identically).

Fixes #771
