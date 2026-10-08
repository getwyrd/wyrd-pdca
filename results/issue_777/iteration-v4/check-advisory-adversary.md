# Adversarial review — issue #777 (reconstruction completes `seg:` repair through `repoint_chunk`)

Bottom line: I could not break the main fix. Legs 1–2 go red on the base and green with the patch, for the right reasons, through `reconcile_step`. I re-ran both segmented test binaries in a scratch copy (17 tests, all pass) and ran my own attack tests and hand mutations against the whole `wyrd-custodian` suite. Three findings need a human decision. None of them is a correctness hole in the new segmented write path.

## Findings

- NEEDS-HUMAN [human] — **A segmented object under a non-canonical key goes from `Blocked` to a silent `Satisfied` forever.** `crates/custodian/src/reconstruction.rs:651-652` takes any key `parse_inode_key` accepts (`inode:01`, `inode:+1`). The move then pins and writes under the canonical `inode_key(id)` (`:1159-1171`), which does not exist, so the commit loses at `:1206-1210` every pass. Concrete case, run in scratch: a segmented root seeded at `inode:01` (the `records()` fixture), with chunk `0xA200` owed. **Base:** `Blocked` on 3 of 3 passes, nothing written. **Patch:** `Satisfied` on 3 of 3 passes, one `conflict` row per pass, the obligation is never drained, and a rebuilt fragment 1 is rewritten to the free server every pass with no reference and no orphan mark. The cause is #698 (open), and the brief forbids fixing `parse_inode_key` here. Before this patch, though, #698 only hit flat objects; segmented ones were refused and stayed visible as `Blocked`. Per the reviewer protocol ("raise the tracking issue instead"), the human should either note on #698 that it now also hides segmented objects behind `Satisfied`, or allow a narrow containment here (contain when `inode_key(id) != key`).

- NEEDS-HUMAN [human] — **A docs deferral that names #777 is left undischarged.** `crates/core/src/metadata.rs:3238-3240` reads: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring. It adds the only caller (`reconstruction.rs:1159`) but touches no architecture doc. That leaves the "nothing calls this yet" comment false. The brief's 3-file budget and its "`metadata.rs` untouched" rule make it impossible to discharge the deferral inside this bundle, and AGENTS.md treats docs currency as a merge requirement. The human should either widen the scope (the doc paragraph plus the stale comment) or re-point the deferral to a new issue before merge.

- NEEDS-HUMAN [human] — **The flat arm's behaviour changes for two corrupt-record cases the brief never asked about.** Legs 9 and 10 (`crates/custodian/tests/segmented_map_repoint.rs:810`, `:817`) are red on the base because the base flat arm behaves differently:
  - **Version `u64::MAX`:** the base panics with "attempt to add with overflow" in a debug build (C4-verify log) and wraps the version in release.
  - **Chunk lengths that overflow `u64`:** the base *repaired* the chunk and answered `Changed`.

  The patch contains both cases on every pass (`reconstruction.rs:685-690`, `:1178-1182`), which follows from #776's primitive addressing chunks by byte offset. Containment also sets `reading.incomplete`, so **every unrelated drain in the store is withheld** while such a record exists. Run in scratch: a flat record at version `u64::MAX` owing `0xA200`, plus an owed `0x0E00` that no record references, gives `Blocked` on both passes with the queue stuck at `[3584, 41472]` both times, even though the record was read in full and only its write is impossible. This is safe (no data loss), and leg 9 is clearly better than a panic or a wrapped version. But the brief says this child changes the flat arm "for nobody", and leg 10 turns a chunk the base repaired into one that only an operator can unstick. The human should confirm this is acceptable.

## Lower-impact notes (not routed; not worth a rebuild on their own)

- **The C5 result ("5 caught, 0 missed") overstates how well the tests pin the new code.** `cargo-mutants` does not mutate match-arm bodies, and two hand mutants survive the whole `wyrd-custodian` suite:
  - **(a) `reconstruction.rs:694`:** pinning the scanned `record` instead of `resolved.record`. I tested a resolve that restarts onto a superseding epoch-8 generation (the race fires after the resolver's first `seg:` page). The real code answers `Changed` and lands the repair in the new generation's `seg:1`. The mutant answers `Satisfied`: the conflict is recorded, the obligation stays queued, and a fragment is stranded. That costs one pass of liveness, and the root pin keeps it safe.
  - **(b) `:1177`:** reporting a prepare-time `Repoint::Conflict` as `Aborted`. This misattributes the counter (`aborted`/`unplaced` instead of `conflict`), because `assert_lost` (`segmented_map_repoint.rs:441`) never checks the conflict row.

  If another round happens anyway, add a restart leg and assert `reconstruction_conflict` in `assert_lost`.
- **The ceiling refusal is escalated without the fresh resolve the primitive's own contract asks for** (`metadata.rs:3134-3138`). `reconstruction.rs:452-455` turns any `Repoint::Refused` straight into `Blocked` plus a NEEDS-HUMAN audit row. If the root is superseded between resolve and move, that is a one-pass false alarm. The base flat arm had the same gap.
- **Scope:** the patch touches 4 files, against a budget of 3. The fourth is a forced one-line deletion in `crates/custodian/src/reconstruction/staged.rs:366` (`chunk_index`, after the field was dropped from `RepairPlan`). Size is within budget: 84,990 B, and about 85 added non-comment production lines (limit 100).

## What I tried and could not break

- **The red→green evidence is real** (`gate-logs/C4-verify.log`): 10 of 11 tests are red on the base, each for its stated reason. Leg 4 fails on "race never landed", leg 9 on the overflow panic, and leg 5 is green by construction. The tests drive `reconcile_step` and assert on the store, not on a copy of production. Leg 1 checks the exact orphan key, the placements, the rebuilt fragment, the unchanged root and the untouched decoy.
- **Addressing by summed byte offset is sound for segmented maps.** Segment tables must tile contiguously (`metadata.rs:1129-1135`), segment records cannot be empty (`:1385`), and segment spans are checked at decode.
- **`MalformedReplacement` cannot end the pass.** The placement always comes from `checked_fragments`, so it is always full length (`reconstruction.rs:822-825`).
- **The race and fault cases hold:**
  - A sibling edit is merged, and an edit to the planned chunk loses.
  - A root superseded after the resolve loses.
  - A record torn under the move is contained once per object, with an abort recorded for each dispatched repair.
  - A store fault under the move's `get` ends the pass.
  - Two owed chunks in one `seg:` record both land.
- **The CI gate is clean,** including `typos`, the docs render, the statics gate and the madsim DST suite (`gate-logs/C4-ci.log`).
