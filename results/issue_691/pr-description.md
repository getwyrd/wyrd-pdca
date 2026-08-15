# PR description

## Summary
**User impact:** None today — nothing in the codebase calls this module yet. This
lays the storage-key groundwork for upcoming S3 multipart-upload support, and it
closes a gap where a small helper that classifies a stored key by its prefix could
be handed a broken or truncated key and still answer "this is a valid key of mode
X" instead of refusing it — the kind of mistake that, once something depends on
that answer, quietly leads it to treat data it can't actually read as if it were
fine.

This PR adds a new module defining the canonical, validated format for every key
the multipart-upload feature will use, and makes that key-mode helper refuse any
key it cannot fully decode instead of matching on a prefix alone.

## What to look at
The new file `crates/core/src/multipart.rs` (the key formats and validated id
types) and its test `crates/core/tests/multipart_keys.rs` (the guarantees, checked
directly). Run `cargo test -p wyrd-core --test multipart_keys` to exercise them;
the "retirement" section of the test is the part this PR's fix touches — it
checks that a key naming a retirement obligation is only ever classified once it
is confirmed to be a complete, readable key.

## Root cause
The helper that reads a retirement key's disposal mode (`retire:bytes:…` vs.
`retire:records:…`) only checked the key's leading bytes, so a key with no token
after the prefix (`retire:bytes:`) or with a non-UTF-8 byte in the token
(`retire:bytes:\xff`) still reported a valid mode even though the key as a whole
names no real obligation. The public function callers would use,
`parse_retire_mode`, called that prefix-only helper directly, so it inherited the
same blind spot.

## Fix
`parse_retire_mode` now runs the full key decode (`parse_retire_key`) and keeps
only the mode half of the result, so it can report a mode only for a key that
decodes completely, token included. The prefix-only helper is kept private and is
used solely as the first step inside that full decode, never as a standalone
answer.

## Verification
- **Claim:** every key this module can construct round-trips through its parser,
  and no non-canonical or incomplete spelling of a key — including a retirement
  key with no token, or a non-UTF-8 one — is ever accepted.
- **Checked:** `crates/core/src/multipart.rs:764-766` — `parse_retire_mode`
  delegates to the whole-key decode rather than a prefix check; `:731-748` — the
  prefix-only classifier stays private and is used only as that decode's first
  step, never exposed on its own.
- **Test:** `crates/core/tests/multipart_keys.rs`
  (`cargo test -p wyrd-core --test multipart_keys`) — 21 tests pass, including
  rows for a retirement key with no token and one with a non-UTF-8 byte, and a
  check that both the mode-only helper and the full decode reject exactly the
  same set of malformed keys. This module is new, so there's no pre-existing
  binary to compare against; the fail-closed behavior was instead confirmed by
  temporarily reverting `parse_retire_mode` to its old prefix-only form, which
  turned two of these assertions red (a malformed key was wrongly accepted as a
  valid `Bytes`-mode key), then restoring the fix, which returns all 21 tests to
  green.

Fixes #691
