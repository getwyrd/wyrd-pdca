## Summary
**User impact:** today an object can only be stored if its list of chunks fits in a single
metadata value, which caps it at 158 chunks — about 158 MiB at the default 1 MiB chunk size.
Large multipart uploads (the launch goal is objects over 10 GiB) cannot be completed. This PR
adds the piece that writes the larger, split form of that list safely. It changes nothing a
user sees yet: the multipart Complete in #658 is its first and only caller.

This PR adds one new `wyrd-core` module, `staged_publication`, that publishes a chunk list
either as a normal (flat) map or, when it is too big, as a split map written in several
bounded steps and then made live in one final step, so a reader never sees an object whose
map points at a missing piece.

## What to look at
- The new module `crates/core/src/staged_publication.rs`. Its top doc comment explains the
  rules it enforces and the two duties it leaves to its caller. Start with `publish`.
- The behaviour is easiest to follow in the new test file
  `crates/core/tests/segmented_publication.rs`. Each test is named after the property it
  checks (`a_…` round trip, `b_…` batch size, `c_…` refusals, `d_…` resume, `e_…` atomic
  flip, `f_…` rollback ordering).
- To try it: `cargo test -p wyrd-core --test segmented_publication`, and
  `cargo xtask dst` for the seeded fault scenario.
- Not in this PR, on purpose: the multipart session and its Complete verb (#658), the retry
  loop around the final step, installing `retire:` obligations, and deleting old segment
  records (the drain, #659). No edits to `metadata.rs`, proposal 0016, any ADR, or the lock
  file.

## Root cause
The segmented map shape (`ChunkMap::Segmented`, `seg:<nonce>:<epoch>:<i>` records) and the
shared resolver are on `main`, but only the read side exists: no code writes a segment record
or a segmented root. Publishing one cannot be a single batch (it exceeds the transaction
size limit), so it needs a staged write with a fence that keeps a rollback and the final flip
from both landing.

## Fix
`publish` (`crates/core/src/staged_publication.rs:445`) takes the target (fresh key or
superseding a prior inode record), the segment group `(nonce, epoch)`, the chunk list, the
object metadata, a fence record key, a per-batch fence contribution from the caller, a resume
cursor, optional byte/operation budgets (clamped to `MAX_BATCH_BYTES` / `MAX_BATCH_OPS`), and
the caller's contribution to the final batch.

- **Flat or split.** At most `MAX_MAP_CHUNKS` chunks: flat root, one batch. More: segments
  are filled by encoded bytes, not by a fixed count (`split_segments`, `:580`), and a plan
  over `MAX_ROOT_SEGMENTS` is refused up front. `planned_segments` (`:417`) exposes the
  count, which can exceed `⌈chunks / MAX_SEG_CHUNKS⌉` when chunk references are wide, so the
  caller can pin its cursor at the right value.
- **Segment phase.** Records are written in batches bounded by encoded bytes (including
  precondition keys and values) and by operation count (`split_batches`, `:627`;
  `batch_bytes` / `batch_ops`, `:688`, `:701`). New rows go in under `require_absent`; rows
  already durable and identical are pinned, not rewritten, so a recovery at the same epoch
  never double-writes.
- **Verify, then flip.** Before the flip the whole group range is read back, paged, and every
  planned record must be present and byte-identical (`check_durable`, `:1018`). The resume
  cursor is only where writing starts, never proof the earlier records exist. The flip is one
  batch: the fence, the root compare-and-swap that pins both inode and dirent on both the fresh
  and supersede paths, version `1` or `prior.version + 1`, `overwrite_modified` for the
  timestamp, and the caller's whole contribution (`flip_batch`, `:900`).
- **Refusals before any write.** Every contribution must pin the fence; the fence value a
  batch needs and leaves must depend only on the cursor; the flip must move it; it may never
  return to a value it left (checked over every cursor position, so a recovery gets the same
  answer as the first attempt); and no contribution may touch the inode, the dirent, or the
  group's `seg:` range (`check_contribution`, `:722`; `check_track`, `:816`;
  `check_flip_fence`, `:837`). Store faults, `Conflict` and `CommitUnknownResult` are passed
  back unchanged; the caller owns retry.
- **Caller duties, documented** (module doc, `:61-74`): a rollback must commit a fence move
  before it deletes any `seg:` record (the existing `SessionRecord::completing_teardown`,
  `crates/core/src/multipart.rs:2556-2582`, has this shape), and contributions must not touch
  committer-owned keys (also enforced).
- **Docs:** a staged-publication paragraph in
  `docs/design/architecture/08-crosscutting-concepts.md:91`, after the segmented write-side
  paragraph.

Known limit, for #658 to settle: the flat-or-split choice counts chunks against the fixed 158,
which is sized for the RS(6,3) reference width. With wider erasure settings a flat map can go
over the value budget, and the caps are compile-time constants while `KnobSet` makes them
per-deployment. Some boundary tests (an over-ceiling root value, `ChunkTooLarge`,
`VersionOverflow`, a byte batch exactly at its limit) are left as follow-ups; the code paths
for them are in place.

## Verification
- **Claim:** a split map round-trips, and a small one stays flat.
  **Checked:** `crates/core/tests/segmented_publication.rs:406` (fresh and supersede, resolved
  through `resolve_chunk_map`, `crates/core/src/metadata.rs:3218`, version 1 / `prior + 1`),
  `:631` (130 segments, past one 128-row read page), `:468` (wide references: planned count
  matches what is written, every segment within the value budget).
- **Claim:** every batch stays within the byte and operation budgets, and the root lands in
  exactly one commit after all segment commits.
  **Checked:** `:673` (bytes, counting preconditions), `:747` (operation limit binds where bytes
  do not).
- **Claim:** every refusal happens with zero commits and the store unchanged.
  **Checked:** `:788` (table of refusals, including a contribution that deletes `seg:…:0`, a
  flip that deletes an unrelated key without moving the fence, and over-budget flips), `:1060`
  / `:1070` (a stray or foreign row in the group's range).
- **Claim:** the check before the flip covers every segment, not only those below the resume
  cursor.
  **Checked:** `:1081` (D1, a record below the cursor missing or changed), `:1112` (D2, a
  record deleted after the last segment commit), `:1139` (D3, a fully durable range is not
  rewritten).
- **Claim:** the root and the caller's flip contribution commit together or not at all, and
  the flip pins inode and dirent.
  **Checked:** `:1170`, `:530`, `:580`.
- **Claim:** a rollback and the flip cannot both land, in either order; an older epoch's
  drain never touches a newer one; a fence cycle is refused.
  **Checked:** `:1335` (rollback first), `:1353` (flip first), `:1197`, `:1230`, `:1301`.
- **Claim:** under seeded commit-ambiguity faults with recovery and a rival rollback, at most
  one root names the group and any root that does resolves fully.
  **Checked:** `crates/dst/tests/segmented_publication.rs` under `cargo xtask dst`.
- **Test:** the module is new, so the test file cannot compile without it and there is no
  plain fails-before / passes-after run. Instead each guard was shown to matter: 24 separate
  temporary edits to the committer (for example, checking only below the resume cursor,
  batching by record count, splitting the root from the caller's batch, dropping the fence
  precondition from the flip, removing the inode or dirent pins) each turned a named test red,
  and the file was restored after each. `cargo mutants` on the diff: 151 caught, 18 missed
  (all off-by-one budget boundaries), 7 unviable. `cargo xtask ci` passes on the final tree
  (core `segmented_publication`: 21 passed; DST scenario: 3 passed).

Fixes #653
