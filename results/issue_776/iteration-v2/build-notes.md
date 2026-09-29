# Build notes — #776 seg-record placement-move primitive (iteration 2)

Target: getwyrd/wyrd @ main (worktree base `243241e`). One file changed:
`crates/core/src/metadata.rs`. `patch.diff` is **49 337 bytes** (budget 50 KB). Line
numbers below are in the patched file. The brief's own numbers (`:380`, `:870`, `:1776`, …)
come from an older main and have moved, so I cite by symbol too.

Production semantic lines added (not blank, not comment/doc, outside the test module):
**145** (budget ≤170). Iteration 1 was 122. The 23 extra lines are the new error variant
and its `Display` arm, the placement-length check, and the oversize-row guard.

## What this iteration changed, finding by finding

Iteration 1's patch (`iteration-v1/patch.diff`) is the base. The review raised 8 gating
rows in 4 classes. Every one is **fixed**. None is recorded as rejected, so there is no
`review-rejected.md`.

### 1. Oversize live `seg:` row was decoded and could be rewritten (BUG ×2, adversary, code-review)

`repoint_chunk` read the segment and decoded it with no size check. The resolver refuses
such a row first (`read_group_range`, `metadata.rs:2903`: `value.len() > MAX_VALUE_BYTES`
→ `SegmentValueOverCeiling`). So a row the read side calls corrupt could be "repaired" by
the write side.

Fix: one match-guard arm ahead of the decode, `metadata.rs:3271-3278`:
`Some(bytes) if bytes.len() > MAX_VALUE_BYTES => ChunkMapError::SegmentValueOverCeiling {..}`.
It flows into the existing `retired_or` call (`:3309`), exactly as the resolver's anomaly
does. So a live generation gets `Err(SegmentValueOverCeiling)`, and a retired one gets
`Repoint::Conflict`. Same comparison, same `>` boundary, same full `MAX_VALUE_BYTES`
ceiling as the resolver. No V/2 appears.

The `# Anomalies` doc on `repoint_chunk` now lists this shape (`:3202-3209`).

Tests:
- `a_segment_row_over_the_ceiling_is_never_decoded_nor_rewritten` (`:4893`). A one-segment
  root whose row is exactly V+1 bytes, built by the existing `padded` helper (no byte
  literal). The move would **shrink** it under V (`[u64::MAX]` → `[0]`), which is the
  reviewer's probe. Live: `Err(SegmentValueOverCeiling { index: 0, bytes: V+1, ceiling: V })`
  and the whole store is byte-identical. Then the root flips to a flat generation:
  `Ok(Conflict)`, store byte-identical.
- `segmented_arm_weighs_its_record_against_the_full_value_ceiling` (`:4860`) gained a third
  leg (`:4884-4889`). The row stored there is now exactly V bytes. A second same-width move
  (`[u64::MAX]` → `[u64::MAX - 1]`) reads it, prepares, commits, and leaves it at exactly
  V. That pins "a row exactly at the ceiling is read, not refused", so a `>=` slip is caught.

### 2. Replacement placement never checked (CONVENTION ×3, adversary, code-review)

Fix: `metadata.rs:3223-3230`. Before anything is read, `placement.len()` must equal
`prior.fragment_count()`. Otherwise it returns the new typed
`ChunkMapError::ReplacementPlacementMalformed { expected, actual }` (`:852-862`, `Display`
at `:981-984`).

I made it **stricter than `placement_is_valid`** (`:426`), which admits the empty vector
for pre-M3 records. For a move, empty is refused too. A move says where every fragment now
lives. An empty vector would be identity-filled on read (`placed_dserver`) to servers 0..n,
where the fragments are not. The adversary's probe committed exactly that. The code-review
note asked only for "nonempty wrong-length" to be refused; the adversary asked for
`len == fragment_count()`. I took the stricter rule because the empty case is the one that
silently points reads at the wrong servers.

Why a `ChunkMapError` variant and not `MalformedPlacement` (`:460`): that struct has no
`Display`/`Error` impl, and its doc defines it as "non-empty but of a length other than
`fragment_count`". An empty replacement would not match that doc. A variant costs 11 lines
plus a 4-line `Display` arm, and it stays in the one typed error family the future caller
(#777) already classifies as object-local.

Test: `a_replacement_placement_must_name_every_fragment` (`:4925`). An RS(2,1) chunk on
`[10, 11, 12]`, run through **both arms** (flat root, and segment 1 of a segmented root).
`[]`, `[20]`, `[20, 21]` and `[20, 21, 22, 23]` each give
`Err(ReplacementPlacementMalformed { expected: 3, actual })` with the store byte-identical.
`[20, 21, 22]` prepares and commits.

### 3. `chunk_at` overflow test passed for the wrong reason (adversary, code-review)

The old assertion queried offset 0, so the loop left through `at > byte_offset` and never
reached the overflowing add. Fixed in `chunk_at_needs_both_the_offset_and_the_reference`
(`:4993-4997`). It now queries at `u64::MAX`: chunk 2 begins exactly there (`Some(1)`), and
`c()` would begin one byte past it, which is the overflowing `checked_add` → `None`. The
reviewer's hand mutant (`saturating_add`) now goes red (N6 below). I also corrected the comment.

### 4. No seeded Tier-0 coverage of the concurrent path (TEST-GAP ×3; T5)

Iteration 1's adversary called this a human scope call (it is in `deferred-findings.json`).
The code-review note pointed out that `wyrd-testkit` is already a `wyrd-core`
dev-dependency (`crates/core/Cargo.toml`, "Seeded property tests reuse the deterministic
simulator"), so it fits the one-file scope. I took that route rather than handing it back.
That settles the deferred item in scope: the human no longer needs to choose.

"Seeded Tier 0" here follows the repo's own in-crate shape: `wyrd_testkit::Sim` drawing
every choice from a seed, over the production redb store, with the seed in every failure
message. That is the pattern of `erasure.rs`'s `seeded_random_data_and_subsets_round_trip`
and of `crates/core/tests/multipart_owned_staging.rs:668`, which calls itself "seeded
Tier 0 (ADR-0009)".

Test: `seeded_moves_race_each_other_and_the_roots_retirement` (`:5228`), driving
`campaign` (`:5065`) over seeds 0..256. Per seed, `Sim` draws:
- the shape (flat or segmented), 1–3 segments of 1–3 chunks, lengths 0–3 (so empty chunks
  land on segment boundaries), and scheme None or RS(2,1);
- 2–5 movers, each going plan → prepare → commit. Plan uses the **production resolver**
  `resolve_current_chunk_map`, and the test asserts it returns every move that has landed
  so far. Prepare is `repoint_chunk`. Commit is `MetadataStore::commit` of the batch;
- the root's fate: it stays, it is overwritten by a new flat generation, or it is deleted.
  After that, a segmented generation's records are reclaimed in one batch;
- the interleaving: which ready actor steps next.

Each step is judged against `World` (`:5002`), a model that knows nothing of the batch's
pins. It knows only which records have been written since a mover planned or prepared:
- a prepare writes nothing (full-store `scan` before and after), and it answers `Conflict`
  exactly when (segmented) the planned chunk has moved or the records were reclaimed.
  Otherwise it answers `Prepared`;
- a commit lands exactly when the root is untouched since the plan **and** the record since
  the prepare. So a sibling's move that lands before the read is merged. A losing batch
  leaves the store byte-identical;
- after every step the store holds exactly the model: the root, and every segment's chunks.

Every server id drawn is new (`fresh`, `:5035`), so record bytes never repeat. That means
no CAS can pass on an A-B-A, and the model's "written since" is exact.

It also counts how often each race ran, and asserts that each count is above zero, so a
drift in the generator cannot make the test pass vacuously. Measured over the 256 seeds:
**18** sibling moves merged, **68** stale prepares, **142** commits lost to another move,
**186** commits lost to the retirement. The whole run takes about 1.1 s.

Interleaving is at call granularity (plan / prepare / commit). That is the granularity at
which this primitive can race. It writes nothing, and its batch is judged only at commit,
by what it pinned. So a concurrent write "during" the prepare is the same as one just
before or just after it. I did not wrap the store to inject writes between the reads
inside `repoint_chunk`. That would need a full `MetadataStore` test double in this file,
and a prediction model that tracks reads inside the call, for no race the call-level
schedule does not already produce.

Not done: a madsim (`crates/dst`) scenario. That is a second crate, and the brief says a
second file means the shape is wrong. The madsim leg belongs with #777, where the primitive
gets a caller whose tasks can actually run concurrently.

## Unchanged from iteration 1 (still holds)

- Flat arm = `commit_chunk_map`'s idiom (the fn is now at `metadata.rs:2167`, untouched,
  including its segmented refusal): `..generation.clone()`, next version via `checked_add`
  → `VersionExhausted`, `require(inode, encode(prior)) + put(inode, encode(next))`.
- Segmented arm finds candidates in the root's own table (`segment_may_hold`, `:3331`). There
  is no `seg:` range walk, at most two `get`s, and the root is never `put`.
- The three pins, as settled: root generation bytes; the segment record's bytes **as read
  fresh here** (the docs say plainly that this is not the resolve's bytes); and the `ChunkRef`
  itself (`chunk_at`, `:3344`).
- Both arms are weighed by `flat_value_ceiling_crossed` (full V) in `weighed` (`:3316`). No
  `MAX_ROOT_VALUE_BYTES` comparison in the diff; the one added mention is doc text (`:600`).
- Untouched: `commit_chunk_map`, the resolver and the read side, every custodian file,
  proposal 0016, conformance vectors. No new dependency.

## Verification posture

As the brief states, this is green-only under C4-verify by design. There is no new
`tests/*.rs`; every test is in-crate `#[cfg(test)]`. The binding oracle is `cargo mutants`
plus the named negations below.

### Named negations (applied one at a time, run, then restored)

Harness: `$PDCA_SCRATCH/pdca-builder-776-neg2/run.sh`. It copies the known-good file, applies
one perl substitution on one line, runs `cargo test -p wyrd-core --lib placement_move`
under `timeout 1500`, prints the failures, restores, and `cmp`s the file against the good
copy. Every run ended with "restored".

| # | Negation (patched-file line) | Tests that went red |
|---|---|---|
| N1 | drop `chunk == prior` (`:3350`, `\|\| black_box(true)`) | 5: `chunk_at_needs_…`, `flat_arm_conflicts_…`, `a_zero_length_chunk_…`, `a_sibling_edit_…` (**same-chunk conflict leg**), **`seeded_moves_race_…`** |
| N2 | drop the root-generation pin (`:3232`) | 3: `flat_arm_conflicts_…`, **`a_superseded_root_fails_…`**, **`seeded_moves_race_…`** |
| N3 | drop the segment-bytes pin (`:3301`) | 2: `a_sibling_edit_…`, **`seeded_moves_race_…`** |
| N4 | drop the oversize-row guard (`:3272`) | 1: `a_segment_row_over_the_ceiling_…` |
| N5 | drop the placement-length check (`:3224`) | 1: `a_replacement_placement_must_name_every_fragment` |
| N6 | `checked_add(..)?` → `saturating_add` in `chunk_at` (`:3353`) | 1: `chunk_at_needs_both_…` (the reviewer's surviving mutant) |
| N7 | version advance unchecked (`.wrapping_add(1).checked_add(0)`, `:3241`) | 1: `flat_arm_refuses_a_version_it_cannot_advance` |
| N8 | drop the output ceiling (`match None::<usize>…` in `weighed`, `:3317`) | 2: both ceiling tests |
| N9 | skip `retired_or` (anomaly → plain `Conflict`, `:3309`) | 2: `a_damaged_segment_…`, `a_segment_row_over_the_ceiling_…` |

The seeded campaign goes red on its own for all three pins (N1, N2, N3). So it binds the
concurrency properties, not only the hand-sequenced tests.

### cargo mutants

`cargo mutants --in-diff patch.diff -p wyrd-core --test-package wyrd-core -- --lib`
(cargo-mutants 27.1.0, run on the final `patch.diff`):
**36 mutants, 27 caught, 0 missed, 0 timeout, 9 unviable**, in 76 s. Iteration 1 had 30
mutants (21 caught). The 6 new ones are on the new code: `!= → ==` on the placement check,
and the oversize guard `→ true`, `→ false`, `> → ==`, `> → <`, `> → >=`. All 6 were caught.
`>=` is caught by the new exactly-V read leg.

The 9 unviable mutants fail to compile under the workspace's `warnings = "deny"` lints
(unreachable code / unused). They are the same 9 as in iteration 1. I re-applied each one
that can exist, in a lint-clean form (`if std::hint::black_box(true) { return X; }` at the
top of the body), through the same harness. Every one went **red**:

| Unviable mutant | Hand-applied result |
|---|---|
| `segment_may_hold → true` | 3 failed (incl. the helper test and the campaign) |
| `segment_may_hold → false` | 11 failed |
| `chunk_at → None` | 13 failed |
| `chunk_at → Some(0)` | 7 failed |
| `chunk_at → Some(1)` | 7 failed |
| delete field `version` (`version: version - 1`, i.e. not advanced) | 2 failed (flat-arm test and the campaign) |
| `ChunkMapError::fmt → Ok(())` | caught by an existing test, `multipart_retire_obligation::a_component_spelled_outside_its_grammar_is_rejected` (full `cargo test -p wyrd-core`) |
| `repoint_chunk → Ok(Default::default())`, `weighed → Default::default()` | cannot exist: `Repoint` has no `Default` |

The new `Display` arms' exact text is not asserted by any test. `cargo mutants` does not
make per-arm mutants, and the text is diagnostics only.

## Refute-your-own-test

- **(a) Genuine red?** Yes, per negation rather than per revert. The defect is an absent
  API, so reverting the patch removes the symbols and the tests stop compiling, as the brief
  says. Each piece of behaviour was negated on its own and went red: nine named negations
  (table above) plus the mutation run.
- **(b) Production path?** Yes. Every test calls the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at` directly. Batches are applied through
  `MetadataStore::commit` on the real redb backend (`RedbMetadataStore::in_memory()`). The
  campaign plans through the production resolver `resolve_current_chunk_map`. The campaign's
  `World` is an oracle of *outcomes* (which records were written since), not a copy of the
  primitive: it never builds a batch or chooses a pin.
- **(c) Fixture includes the fault?** Yes. The faults are in the store itself: a V+1-byte
  live row whose move would shrink it; a V-byte row that must still be read; wrong-length
  replacements in both arms; offset `u64::MAX` against a list whose sum overflows; and, in
  the campaign, real concurrent moves to the same and sibling chunks, root overwrites,
  root deletes, and reclaimed segment records, drawn over 256 seeds. The tally shows each
  race actually happened (18 / 68 / 142 / 186). "Nothing written" is always a full-store
  `scan(b"")` compared before and after.

## Gates run

- `./engine/xtask.sh ci` (= `cargo xtask ci`, the project's C4-ci gate) with
  `PDCA_WORKTREE` set to the worktree: **exit 0, "xtask ci: all checks passed"**. Every step
  ran: typos, docs lint and render, gitlink-guard, unsafe-guard, `cargo fmt --all --check`,
  workspace clippy/build/test, cargo-machete, all three cargo-deny checks, statics,
  deploy-guard, and the madsim `wyrd-dst` clippy and test. All 16 `placement_move` tests
  pass inside it. Log: `$PDCA_SCRATCH/pdca-builder-776-ci2.log`.
- `cargo fmt -p wyrd-core` was applied before the gate, so the patch is what the target's
  formatter produces.
- While iterating I used `cargo test -p wyrd-core --lib placement_move` under
  `timeout 1500`. xtask has no scoped test subcommand. 16/16 pass.
- The final `patch.diff` is byte-identical to `git diff` in the worktree (checked with
  `cmp`) after all the negation runs, and it reverse-applies cleanly.
- All the brief's external dependencies were actually exercised: `typos`, the docs
  renderer, `cargo-deny` and `cargo-machete` (all inside `xtask ci`), and `cargo-mutants`
  (above). No NEEDS-HUMAN external dependency.

## Things for the human to know

- **The deferred T4/T5 "seeded Tier-0" item is now handled in scope** (see §4), so the
  human choice recorded in `deferred-findings.json` should no longer be needed. If you
  consider only a madsim `crates/dst` scenario to count as Tier-0 here, that is a second
  crate, and it belongs with #777.
- **Docs currency rubric.** This adds a public library function and two error variants in
  `wyrd-core`. It adds no port, gateway/API operation, RPC, CLI flag or persisted field,
  and the brief limits the change to one file, so I did not touch the living architecture
  doc. If you read "API operation" to include core library entry points, the doc update
  belongs with #777, where the primitive gets its first caller.
- The `ChunkMapError` enum doc (`:609-613`) says every variant is raised at decode or at an
  unwired site. That was already out of date on main (the resolver variants are raised at
  resolve), and the two new variants add to it. I left it alone because it is outside this
  change.
- The existing doc on `flat_value_ceiling_crossed` still cites a stale `(:2493)` for the
  resolver's `>` refusal (it is now `read_group_range`, `:2903`). That is outside the
  sentences I changed, so I left it.
- Scratch used: `$PDCA_SCRATCH/pdca-builder-776-{target,neg2,mutants2,u0.diff,added2.txt}`,
  plus iteration 1's leftovers. Left for the harness to clean up, per the builder rule
  against rm-style commands.
