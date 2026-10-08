# Build notes — #777: reconstruction completes the `seg:` repair through #776's primitive

Target: getwyrd/wyrd @ main, worktree HEAD `4bda59c` (the integration branch; it already holds
#776 as merge `36f006d`, PR #844, so `metadata::repoint_chunk` is on the base). Line numbers
below are **post-patch** in `crates/custodian/src/reconstruction.rs` unless marked `base:`
(`git show HEAD:…`).

## What changed and why

The base routed a `seg:`-resident obligation to `Site::Refused` (base: `reconstruction.rs:701`)
and answered `Assessment::Refused` (base: `:783`), so the obligation stayed queued every pass
and the pass answered `Blocked`, forever. That is the C-1 state. The fix removes the refusal and
sends **every** committed repair, flat or segmented, through `metadata::repoint_chunk`
(`crates/core/src/metadata.rs:3241`):

- `read_committed` now records, for each owed chunk, a `Site { object, byte_offset, chunk_ref }`
  (`:574`) for both shapes. `byte_offset` is summed over the resolved chunk list, which is the
  address the primitive finds the chunk by (`chunk_at`, `metadata.rs:3368`). The held generation
  is `resolved.record` (`Object.prior`, `:552`), the root the chunks were actually resolved
  from, so the root pin matches the plan even when a segmented resolve restarted.
- `repair_chunk` (`:1142-1176`) keeps the peer's order: select targets → abort if a target is
  outside the fleet → **prepare the move** (reads only, writes nothing) → write fragments →
  add `repair_key` delete + one orphan mark per displaced position to the batch the primitive
  handed back → one `commit`. The base's inline record build and its ceiling check
  (base: `:1143-1178`) are gone; the ceiling refusal now comes from inside the primitive
  (`weighed`, `metadata.rs:3340`), as the brief asked. The flat arm's output is unchanged:
  the primitive builds the same `..prior.clone()` record with `version + 1`, and pins the same
  `require(inode_key, encode(prior))`.
- `Repoint` answers map as: `Prepared` → commit; `Refused` → `RepairOutcome::Refused` (a hole,
  `Blocked`); `Conflict` → `RepairOutcome::Conflict` (not a hole, so `Satisfied` when it is the
  only outcome, as decided 2026-08-19); `VersionExhausted` and a typed `ChunkMapError` from the
  move's re-read → new `RepairOutcome::Contained(String)` (`:1057`), which `reconcile` handles
  with `reading.contain(...)` + `emit_aborted` (`:448`). Any other `Err` (a store fault)
  still ends the pass.
- `hole` (`:493`) loses only the `!reading.refused.is_empty()` term. Conflicts were not added.

### Regression guards from the brief

- **(a) once per OBJECT.** `Reading::contain` (`:541`) now de-duplicates by key through
  `contained: BTreeSet<Vec<u8>>` (`:535`), so an object the move finds unusable under two
  queued chunks is named and counted once. The refusal set it replaces is gone because the
  refusal is gone. The ceiling refusal stays once per chunk, which is the flat arm's existing
  behaviour (`emit_ceiling_refused` per plan) and not something this slice changes.
  **Not covered by a test in this patch:** reaching the move's `Contained` arm needs a torn or
  absent `seg:` record *between* the resolve and the move (the resolver would contain it
  first otherwise). The new file's `MemMeta` can do that, but a sixth test would change the
  brief's "5 tests ran" count, so I left it out. The parent's salvage has that exact leg
  (`results/issue_721/segmented_map_repoint.rs`, `a_torn_segment_record_is_contained_once_per_object_never_re_planned`)
  if the human wants it added.
- **(b) no new silent skip.** The base's flat `(Some(_), None) => continue` is kept, decided off
  the scanned record's shape exactly as before (`:655`, #698). A **segmented** record under an
  unparseable key used to be refused (kept, `Blocked`); a plain `continue` for it would have been
  a new silent skip that lets its owed chunks look unreferenced and get drained. It is now
  contained instead (`:676`), which sets `reading.incomplete` and so withholds every drain.
  `parse_inode_key` itself is untouched.

### Files and budget

| file | + | − |
|---|---|---|
| `crates/custodian/src/reconstruction.rs` | 169 | 201 |
| `crates/custodian/src/reconstruction/staged.rs` | 0 | 1 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | 50 | 51 |
| `crates/custodian/tests/segmented_map_repoint.rs` (new) | 586 | 0 |

Added non-comment, non-blank production lines: **75** (budget ≤ 100). Production is net −32
lines. `patch.diff` is **67.6 KB** (budget ≤ 85 KB).

**Budget overrun, flagged: a 4th file.** `RepairPlan.chunk_index` was read only by the
committed arm's inline record build, which this patch deletes. Left in place it is a field
that is written (by `staged.rs:369`) but never read, which is a `dead_code` warning and fails
`clippy -D warnings`. So the field goes, and with it the one line `chunk_index: site.index,`
at base `crates/custodian/src/reconstruction/staged.rs:369`. That is a one-line deletion, no
behaviour change (`staged` still reads `site.index` at `:285` and `:355`). The alternatives were
worse: `#[allow(dead_code)]` on a field nothing reads, or keeping the index path alive just to
read it.

## Test: `crates/custodian/tests/segmented_map_repoint.rs` (new, no `#![cfg]`)

Drives only `reconcile_step` and asserts on the store. Imports only base symbols
(`MAX_VALUE_BYTES`, not `MAX_ROOT_VALUE_BYTES`); it does not call `repoint_chunk`.

Race hooks, per the brief: `AfterSegmentPage` fires inside `scan_page` for a `seg:` prefix,
**after** the page is built from the pre-race bytes and before it is returned, so the resolver
plans from old bytes and the move's `get` sees the new ones. `IntoCommit` fires at the top of
`commit`, after the resolve. Each racing leg asserts `meta.raced()`, so it cannot pass because
the race never happened.

1. `a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record` — `Changed`;
   segment 1 placements `[[0,2],[0,1]]` (only the owed chunk moved); fragment on server 2 in a
   different domain; queue empty; one orphan mark; root and segment 0 byte-identical.
2. `a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged` — `Changed`;
   `[[0,2],[0,7]]`: repair and racer both survive; queue empty.
3. `a_racing_move_of_the_planned_chunk_itself_is_a_conflict` — segment 1 holds the racer's
   bytes exactly; root unchanged; still queued; no orphan; `Satisfied`. No assertion about the
   destination fragment (#723).
4. `a_superseded_root_generation_makes_the_repair_lose` — root flipped into `commit`; segment
   1 unchanged; still queued; no orphan; root equals the competing bytes; `Satisfied`.
5. `a_repoint_one_byte_past_the_value_ceiling_is_refused` — segment 1 is seeded programmatically
   so the repointed record encodes to exactly `MAX_VALUE_BYTES + 1` (the growth from a one-digit
   id to `u64::MAX` is measured, not assumed; seeded size is `< MAX_VALUE_BYTES`). Record
   byte-identical, still queued, `Blocked`.

`segmented_map_reconstruction.rs`: the forced edit.
`an_obligation_inside_a_segmented_object_is_refused_never_discarded` becomes
`…_is_repaired_never_discarded`: both chunks (one per segment) are repaired **in one pass**
(each move pins only the root, unchanged, and its own record), each `seg:` record names `[0,2]`,
the queue is empty, the root is byte-identical, the backlog gauge reads 2. To do that, the
survivor-fragment loop was lifted out of `seed`'s `UnderReplicated` arm into `survivors()`, and
the now-unused `MemMeta::records()` was removed (it would warn).

## Red → green (run in the worktree, `timeout`-bounded `cargo test`, the command `run-verify.sh` runs)

I did not run `engine/scripts/run-verify.sh` itself: it runs `git worktree add -B pdca-verify…`
against the **host** repo, which is outside the roots I may write to. Instead I did its red leg
by hand in the cycle worktree: copy the two production files aside (to
`$PDCA_SCRATCH/pdca-builder-777-redleg`), `git checkout HEAD --` them, run the tests, copy them
back, and check `git diff` is byte-identical to `patch.diff` (it was). C4-verify at Check is the
official measurement.

- **Green (fix applied):** `segmented_map_repoint` 5 passed / 0 failed;
  `segmented_map_reconstruction` 6 passed / 0 failed; whole `-p wyrd-custodian` suite: no
  failures.
- **Red (production reverted, tests kept):** `segmented_map_repoint`: **5 ran, 4 failed** —
  leg 1 `left: Blocked right: Changed`; leg 2 `left: Blocked right: Changed`; leg 3
  `left: Blocked right: Satisfied`; leg 4 failed at `assert!(meta.raced())` (the base never
  reaches `commit`, so the root flip never fires); leg 5 ok. `segmented_map_reconstruction`:
  the rewritten leg 2 fails, the other 5 pass.

Leg 4's base red is on "the race never landed", not on the `Satisfied` line. It is the same
coarse "refused vs attempted" signal the brief describes, and like leg 3 it is **not** evidence
that the pins are right. That evidence is the mutation run below.

## Named negations (mutation oracle), demonstrated

Each mutation was applied to `crates/core/src/metadata.rs` (not part of the patch), the new
test file was run, and the file was restored with `git checkout`. Each one turns exactly its own
leg red and leaves the other four green:

| leg | mutation | result |
|---|---|---|
| 3 | `chunk_at` (`metadata.rs:3374`): `chunk == prior` → `chunk.id == prior.id` | leg 3 FAILED, 4 passed |
| 4 | segmented arm (`metadata.rs:3325`): `root_pin.require(key, bytes)` → `WriteBatch::new().require(key, bytes)` (drop the root pin) | leg 4 FAILED, 4 passed |
| 5 | segmented arm: `weighed(…)` → `Repoint::Prepared(… .put(key, encode(&next)))` (drop the ceiling weigh) | leg 5 FAILED, 4 passed |

Leg 2's negation (pin the bytes the resolve saw instead of the move's fresh read) has no
one-line form inside the primitive; leg 2 is binding-red on the base anyway. On the caller side,
widening `hole` to include conflicts would turn legs 3 and 4 red on their `Satisfied` line.

What leg 5 does **not** pin: that the bound is `MAX_VALUE_BYTES` and not `MAX_ROOT_VALUE_BYTES`.
A V/2 bound would also refuse this record. An "admitted at exactly `MAX_VALUE_BYTES`" case would
pin it through the pass, but it is red on the base (the base refuses every segmented repair), so
it would break the brief's "leg 5 green by construction". The two sides of the bound are pinned
in core by `the_value_ceiling_admits_the_boundary_and_refuses_only_past_it`
(`metadata.rs`, `#[cfg(test)] mod tests`). If the human wants it through the pass as well, it
is a second `padded(...)` seed at `MAX_VALUE_BYTES - growth` asserting `Changed`.

## Refute-your-own-test

- **(a) Genuine red?** Yes. With `reconstruction.rs` and `staged.rs` reverted to `HEAD` and the
  tests kept: 5 ran, 4 failed (legs 1–4), as listed above. The rewritten reconstruction leg also
  goes red.
- **(b) Production path?** Yes. Every leg calls the public `reconcile_step` with a real
  `ReconstructionContext`; the pass runs the production `read_committed` → `assess` →
  `repair_chunk` → `metadata::repoint_chunk` → `MetadataStore::commit`. Fragments are real
  `erasure::encode` + `encode_ec_fragment` bytes, so the production verify passes. The only
  doubles are the in-memory `MetadataStore` / `ChunkStore` the brief names; the race hooks sit
  on the seam methods (`scan_page`, `commit`) and do not change their answers.
- **(c) Fixture includes the fault?** Yes. The lost server 1 is in the committed placement and
  in neither the fleet nor the topology, so the fragment is really missing; the racing writes
  are real encoded `SegmentRecord`/`InodeRecord` bytes that actually land (asserted by
  `raced()`); the ceiling fixture really encodes to within the bound and really crosses it after
  the repoint.

## Things for the human

- **Stale deferral note in core.** `crates/core/src/metadata.rs:3238-3240` says "deferred: #777
  — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7)
  … nothing calls this yet. It moves with the custodian wiring in #777." After this patch
  something does call it. The brief forbids touching `metadata.rs` and caps the patch at the
  custodian files, so I changed neither that comment nor the docs. I searched both docs: no
  sentence says reconstruction refuses segmented objects, so nothing there is now false. The
  rubric's docs-currency rule covers ports, API operations, RPCs, CLI flags and persisted fields,
  and this patch adds none. But the #776 note explicitly hands the doc update to this issue, so
  either a follow-up adds one sentence to §6.3 ("the repair loop moves a chunk's placement in
  whichever record holds it, the flat root or one segment record, pinned on the root generation,
  that record and the chunk's own reference") and drops the deferral, or the deferral is
  re-pointed to a new issue.
- **#698 interaction (not fixed, by instruction).** A segmented root under a noncanonical key
  such as `inode:01` parses to inode 1, so its move pins `inode:1`, which does not exist or holds
  something else. The move then loses every pass: `Conflict`, the obligation stays queued, and a
  pass with only that answers `Satisfied`. On the base it was refused (`Blocked`). The flat arm
  has had this exact behaviour all along, and the brief puts noncanonical keys in #698. Worth
  saying on #698 that it now covers segmented objects too.
- **Flat arm now addresses by offset + equality, not index.** Same records written, same pins;
  `the_committed_namespace_is_read_once_per_pass` (flat, 8 chunks, 4 owed) still passes
  unchanged, including the "second repair in one object loses the CAS" behaviour.
- **One behaviour is now better than the flat arm:** two owed chunks in *different* segments of
  one object both land in one pass (the move never rewrites the root), where two in one flat
  object take two passes. Two in the *same* segment also both land, because the second move
  re-reads the record after the first committed and merges.

## Gates run locally

- `cargo fmt --all`: clean, no further changes.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean.
- `typos` on the three changed custodian files: clean.
- `cargo test --no-fail-fast -p wyrd-custodian`: all pass.
- `cargo xtask ci` (the whole gate, in the cycle worktree with the patch applied):
  **`xtask ci: all checks passed`, rc=0**. The prose gates ran rather than skipped (`typos`,
  `lint_docs: OK`, `render_site --check` wrote 99 pages).
