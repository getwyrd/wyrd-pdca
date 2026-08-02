# PR description

## Summary
**User impact:** An object stored in the new segmented layout could be
committed successfully but could never be read back — whole-object reads,
ranged reads, and streaming reads all failed with an unsupported-format
error, even though the object was live and intact. Anyone relying on that
storage layout would see a hard failure trying to fetch data that had been
written correctly.

This change adds the missing read support: a single shared resolver that
every read path now uses to turn a segmented object's metadata into its
ordered byte list, so these objects read back correctly through both the
core read path and the gateway.

## What to look at
The new resolver lives in `crates/core/src/metadata.rs`; the two entry
points are `resolve_chunk_map` (used by a caller that already has the
object's metadata in hand) and `resolve_current_chunk_map` (used when the
metadata must be re-read fresh). The core read functions
(`crates/core/src/read.rs`) and the two gateway read paths
(`crates/server/src/lib.rs`, whole-object/streaming and ranged) were
switched from "give up on this format" to "resolve through the shared
path." To exercise it: seed a segmented object directly (no producer for
this layout exists yet upstream) and read it back whole, and over a byte
range that crosses a segment boundary, through both the core reader and
the gateway — both should return the same bytes as an equivalent
non-segmented object.

## Root cause
The segmented storage layout was introduced with no code that could turn
it into a chunk list, so every reader took the pre-segmented shortcut and
failed on anything segmented. There was also no shared place for that
logic to live, which would have meant each reader re-deriving it
independently — the same kind of duplication that previously let a
garbage-collection pass delete fragments a live object still owned.

## Fix
Added one resolver used by every metadata-aware reader. It bounds the
work a segment table can demand of the reader (a table naming too many
segments is refused before any of it is read; each page of a segment's
own record range is capped at a fixed size the reader chooses, never a
size the table itself claims) and it resolves ambiguity through a single
retry rule: if the object's metadata moved on to a newer version mid-read,
the read restarts against that live version; if the object was deleted
mid-read, the read reports "no such object" rather than an unsettled or
partial result — including on the very last retry attempt, where an
earlier version of this fix mis-reported a deletion as an internal error
instead. The two affected read paths (core and gateway) now call this
resolver instead of failing outright.

## Verification
- **Claim:** A segmented object reads back byte-identical to its flat
  equivalent, whole-object and over a range spanning a segment boundary,
  through both the core read path and the gateway.
  **Checked:** `crates/core/src/read.rs:96-97` and
  `crates/server/src/lib.rs:364-365`, `:459-460` on this PR's base
  branch (which introduces the segmented storage layout this PR adds
  read support for, #648) — all three currently fail closed with
  `SegmentedMapUnsupported`.
  **Test:** `crates/core/tests/segmented_map_resolution.rs` and
  `crates/server/tests/segmented_object_read.rs` (new files) — both
  fail on the current base, pass with this patch.
- **Claim:** The work a read can be made to do is bounded by the reader,
  not by the object's own metadata (a table naming too many segments is
  refused unread; each page request is capped at a fixed size).
  **Checked:** resolver's request log assertions in
  `crates/core/tests/segmented_map_resolution.rs`.
  **Test:** same file, cases covering the ceiling refusal and the fixed
  page-size bound — fail pre-fix, pass post-fix.
- **Claim:** A deletion or a metadata update racing a read is always
  resolved to a definite outcome (restart onto the live version, or "no
  such object") and never to a torn or partial result — including when
  the deletion happens on the reader's last allowed retry.
  **Checked:** the resolve-retry logic in `crates/core/src/metadata.rs`.
  **Test:**
  `crates/core/tests/segmented_map_resolution.rs::a_delete_met_on_the_readers_last_attempt_is_no_such_object`
  — fails pre-fix (reports an internal "unsettled" error instead of "no
  such object"), passes post-fix.

Fixes #649
