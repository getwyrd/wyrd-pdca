# PR description

One logical fix per PR.

## Summary
**User impact:** if a piece of a multipart upload rots or goes missing while it
is still "staged" (uploaded but not yet finalized — this can last hours), the
system does not notice. Two background health checks silently ignore staged
data: one never inspects it for damage, and the other actively throws away the
one record proving a piece is missing, so a later health report can claim
everything is fine when a copy is actually gone.

This change makes both checks look at staged data: one now inspects it for
damage, and the other keeps the "this piece is missing a copy" record instead
of discarding it, until the upload is finished and the data becomes permanent
(rebuilding a lost staged copy outright is left to a follow-up change).

## What to look at
- `crates/custodian/src/scrub.rs` — the integrity-check loop. It now reads a
  staged upload's committed part records (`staged_committed_parts`, defined in
  `crates/custodian/src/gc.rs:1593`) and checks those fragments too, using
  whatever redundancy scheme each record declares, before checking the
  already-finalized data. If a committed record also claims the same piece,
  the committed one always wins.
- `crates/custodian/src/reconstruction.rs` — the loop that rebuilds redundancy
  after a loss. It now reads the staged records before deciding a piece has
  been deleted, and holds off if a staged record still references it.
- To reproduce by hand: seed a multipart upload with a committed part record,
  flip one bit in one of its stored fragments, and run a scrub pass — on the
  old code nothing is queued for repair; on this branch it is queued. Enqueue
  that piece as needing repair and run a reconstruction pass — on the old code
  the record disappears with nothing fixed; on this branch it stays queued and
  the pass reports it cannot fully certify the store yet.

## Root cause
Scrub only ever walked the reference set built from finalized ("committed")
data (`crate::gc::referenced_fragments`), so a fragment named only by a staged
upload's part record was never fetched or checked. Reconstruction resolved an
obligation only against that same committed data; finding none for a staged
piece, it concluded the piece had been deleted and deleted the repair record
along with it, even though the piece was still mid-upload and genuinely
missing a copy.

## Fix
Scrub reads a new staged-only class, `StagedPartSet` (built by
`staged_committed_parts`, `crates/custodian/src/gc.rs:1593`), before the
committed reference set, and checks those fragments with the redundancy
scheme each part record carries (`crates/custodian/src/scrub.rs:142-163` reads
it and reports damage; the fetch loop folds it in around
`crates/custodian/src/scrub.rs:220-260`, kept out whenever a committed record
already names the piece, `scrub.rs:236-256`). A staged placement that is
empty or the wrong length is reported rather than silently accepted or
guessed at, and the pass answers "cannot certify" for it instead of "all
clear" (`scrub.rs:326-333`).

Reconstruction reads the same staged classes first
(`crates/custodian/src/reconstruction.rs:214-227`) and, when a piece has no
committed record but a staged one still names or holds it, keeps the repair
record queued instead of deleting it (`reconstruction.rs:736-742`,
`Assessment::Staged` at `reconstruction.rs:712`). Once a piece is finalized,
its committed record decides everything exactly as before, whatever a
leftover staged record says (`reconstruction.rs:938-941` unchanged). An empty
repair queue still reads nothing at all — no cost added to the common case.

The operator-facing log lines for a damaged staged record now name the actual
record's key, not the piece's ID under the finalized-data wording, so an
operator is pointed at something that actually exists
(`crates/custodian/src/gc.rs:1423`, mirrored for scrub's own reader).

## Verification
- **Claim:** a damaged or missing fragment named only by a staged upload's
  part record is caught by scrub and kept as an open repair by
  reconstruction, instead of being silently dropped.
  **Checked:** `crates/custodian/src/scrub.rs:142-163` (staged read, ahead of
  the committed one) and `crates/custodian/src/reconstruction.rs:214-227`,
  `:736-742` on the target branch.
  **Test:** `crates/custodian/tests/staged_scrub.rs` (new file) and the
  staged legs in `crates/custodian/tests/staged_protection.rs`. Both fail
  against the unmodified code (13 of 18 assertions in the new file fail) and
  pass with this change.
- **Claim:** once a piece is finalized, repair and cleanup behave exactly as
  they did before, even if a leftover staged record still names it.
  **Checked:** `crates/custodian/src/reconstruction.rs:938-941`; regression
  test `crates/custodian/tests/reconstruction.rs:386-388`.
- **Claim:** scrub's staged-session listing does not stop after the first
  page of results, and staged damage is still reported even when a later
  read fails.
  **Checked:** `crates/custodian/src/gc.rs:1593-1622` (paging) and
  `crates/custodian/src/scrub.rs:142-163` (report-before-later-read
  ordering).
  **Test:** `crates/custodian/tests/staged_scrub.rs:1004`
  (`scrub_checks_committed_parts_of_sessions_past_the_first_page`) and
  `crates/custodian/tests/staged_scrub.rs:936`
  (`staged_damage_is_named_even_when_the_committed_read_then_faults`). Both
  fail without the corresponding line and pass with it.
- **Claim:** the fix compiles and passes the project's full check suite,
  including the simulation-tested (`--cfg madsim`) crate.
  **Checked:** `cargo xtask ci` — full run green, including workspace tests,
  clippy, and the `dst` simulation crate.

Fixes #813
