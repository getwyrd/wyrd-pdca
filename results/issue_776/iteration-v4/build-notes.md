# Build notes — #776 seg-record placement-move primitive (iteration 4)

Target: getwyrd/wyrd @ main (worktree base `243241e`, the same base as iteration 3). One
file changed: `crates/core/src/metadata.rs`. `patch.diff` is **49 324 bytes** (budget
50 KB; iteration 3 was 49 615). Line numbers are in the patched file unless marked "main".
The brief's own numbers (`:380`, `:870`, `:1776`, …) come from an older main and have
moved, so I cite by symbol too.

Production semantic lines added (not blank, not comment/doc, before the test module):
**144** (budget ≤170). The production code is **byte-identical to iteration 3**. I checked
this by diffing the two patches up to `mod placement_move`: the only differences are the
git blob hash on the `index` line and the test hunk's line count.

## What this iteration changed

Iteration 3 (`iteration-v3/patch.diff`) is the base. Its review was clean except for one
adversary finding marked `[impl]`. I fixed it. The five `[human]` findings deferred to
sign-off are **not** changed here (see "Left for the human").

### The finding: brief item 2 was not pinned at a segment boundary

The adversary changed the `segment_may_hold` call's length argument from `prior.len` to
`0` (`metadata.rs:3267`). With that change, a non-empty chunk that *starts* a segment also
makes the move read and decode the *previous* segment. All 16 tests stayed green. So a
healthy chunk could become unrepairable because its neighbour is damaged
(`SegmentRecordUndecodable { index: 0 }`), which is what brief item 2 ("no other segment
decoded") forbids. `cargo mutants` never swaps a call-site argument, so its "0 missed" did
not cover this.

The cause: the test that puts garbage in segment 0,
`segmented_arm_rewrites_only_the_covering_segment_and_never_the_root`, only moved `c()` at
offset 8. That offset is inside segment 1 and never touches segment 0's edge. The other
test that moved `b()` at offset 5 (`segmented_arm_addresses_a_chunk_at_a_segment_start`)
had a *healthy* segment 0, so reading it did no visible harm.

The fix (test only, `metadata.rs:4624-4653`):

- The garbage-segment-0 test now makes two moves: `c()` at 8, then `b()` at 5
  (`:4635-4640`). Offset 5 is segment 1's first byte and exactly where segment 0 ends. It
  asserts that both moves land in segment 1 (`:4642-4645`), the root bytes are unchanged
  (`:4646`), and segment 0 is still the same garbage bytes (`:4647-4652`).
- `segmented_arm_addresses_a_chunk_at_a_segment_start` is **removed**. It is now a strict
  subset of the merged test: the same `b()`-at-5 move, the same segment-1 result, and a
  weaker check on segment 0 (healthy and unchanged, not garbage and unread). Merging, rather
  than adding a third move to the old test, also keeps the patch under budget. The patch
  shrank by 291 bytes instead of growing.

Test count: 15 (iteration 3 had 16; two were merged into one).

### Other call-site argument swaps, checked on purpose

The finding was a class (argument swaps that `cargo mutants` never generates), so I tried
every call-site argument in the segmented arm that a swap could plausibly break. All go red
on the final file:

| # | Negation | Tests that went red |
|---|---|---|
| M1 | `segment_may_hold(.., prior.len)` → `0` (`:3267`) — **the adversary's mutant** | 1: **`segmented_arm_rewrites_only_the_covering_segment_…`** |
| M2 | same argument → `1` (never admits a trailing empty chunk) | 2: `a_zero_length_chunk_…`, `seeded_moves_race_…` |
| M3 | `chunk_at(record.chunks(), within, ..)` → `byte_offset` (`:3301`; `let _ = within;` added so it builds) | 6 |
| M4 | `SegmentRecord::new(chunks, segment.byte_offset)` → `0` (`:3306`) | 4 |
| M7 | flat `chunk_at(chunks, byte_offset, ..)` → `0` (`:3244`) | 4 |
| M8 | `seg_key(group, segment.index)` → `seg_key(group, 0)` (`:3271`) | 7 |
| M14 | segment `put` aimed at `root_key` instead of `key` (`:3309`) | 5 |

M3's first attempt did not compile (the workspace denies unused variables). I caught that
because the harness printed no result line, re-ran it in a lint-clean form, and made the
harness report "DID NOT BUILD/RUN" loudly from then on.

## Verification posture

Green-only under C4-verify by design, as the brief says: no new `tests/*.rs`, all tests
in-crate `#[cfg(test)]`. The binding oracle is `cargo mutants` plus the named negations,
all re-run on the **final** file because the test set changed.

### Named negations (applied one at a time, run, then restored)

Harness: `$PDCA_SCRATCH/pdca-builder-776-neg4/run.sh`. It copies the known-good file,
applies one whole-file perl substitution (and fails loudly if nothing changed), runs
`cargo test -p wyrd-core --lib placement_move` under `timeout 1500`, reports a failed
build, prints the failing tests, restores, and `cmp`s against the good copy. Every run
ended "restored", and the worktree file is byte-identical to the good copy afterwards.
Log: `negations.log` in that directory.

| # | Negation | Tests that went red |
|---|---|---|
| N1 | `chunk == prior` → `(chunk == prior \|\| black_box(true))` (`:3357`) | 5, incl. **`a_sibling_edit_…` (same-chunk conflict leg)** |
| N2 | root pin dropped (`:3241` → `WriteBatch::new()`) | 3, incl. **`a_superseded_root_fails_…`** |
| N3 | segment-bytes pin dropped (`:3308`) | 2: `a_sibling_edit_…`, `seeded_moves_race_…` |
| N4 | oversize-row guard off (`:3279`, `&& black_box(false)`) | 1: `a_segment_row_over_the_ceiling_…` |
| N7 | version advance unchecked (`wrapping_add(1).checked_add(0)`, `:3247`) | 1: **`flat_arm_refuses_a_version_it_cannot_advance`** |
| N8 | output ceiling off (`black_box(None::<usize>)` in `weighed`, `:3324`) | 2: **both ceiling tests** |
| N9 | `retired_or` skipped (`:3316`) | 2: `a_damaged_segment_…`, `a_segment_row_over_the_ceiling_…` |
| N10 | flat arm adds `state: InodeState::Committed` (iteration 2's finding) | 1: `flat_arm_moves_the_placement_…` |

The brief's four named negations are N1 (equality → same-chunk conflict test red), N2
(root pin → superseded-root test red), N8 (ceiling comparison → ceiling tests red; the `>`
itself lives in the pre-existing `flat_value_ceiling_crossed`, so the negation removes this
diff's use of it) and N7 (unchecked version → exhaustion test red).

### cargo mutants

`cargo mutants --in-diff <final diff> --no-shuffle -p wyrd-core --test-package wyrd-core
--output $PDCA_SCRATCH/pdca-builder-776-mutants4 -- --lib` (cargo-mutants 27.1.0):
**36 mutants, 27 caught, 0 missed, 0 timeout, 9 unviable**, in 75 s.

The 9 unviable mutants fail to compile under the workspace's `warnings = "deny"` lints or
need a `Default` that does not exist. I re-applied every one that can exist in a lint-clean
form (`if black_box(true) { return X; }` at the top of the body) on the final file. Every
one went **red**:

| Unviable mutant | Hand-applied result |
|---|---|
| `segment_may_hold → true` | 3 failed |
| `segment_may_hold → false` | 10 failed |
| `chunk_at → None` | 12 failed |
| `chunk_at → Some(0)` | 7 failed |
| `chunk_at → Some(1)` | 7 failed |
| delete field `version` (as `version: version - 1`) | 2 failed |
| `MalformedReplacement::fmt → Ok(())` (as `let _ = (self, f); Ok(())`) | 1 failed |
| `repoint_chunk → Ok(Default::default())`, `weighed → Default::default()` | cannot exist: `Repoint` has no `Default` |

## Refute-your-own-test

- **(a) Genuine red?** Yes, per negation rather than per revert. The defect is an absent
  API, so reverting the patch removes the symbols and the tests stop compiling, as the
  brief says. Each piece of behaviour was negated on its own on the final file and went
  red: 8 named negations, 7 call-site argument swaps (including the adversary's exact
  mutant, M1, which now fails `segmented_arm_rewrites_only_the_covering_segment_…`), 7
  hand-applied unviable mutants, and the `cargo mutants` run has 0 missed.
- **(b) Production path?** Yes. Every test calls the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at` directly. Batches are applied through
  `MetadataStore::commit` on the real redb backend (`RedbMetadataStore::in_memory()`). The
  seeded campaign plans through the production resolver `resolve_current_chunk_map`.
- **(c) Fixture includes the fault?** Yes. For this iteration's finding the fault is a
  damaged neighbour segment: segment 0 holds `b"not a segment"` while the root still names
  the generation, and the move targets the chunk sitting exactly on segment 0's end. Under
  M1 that fixture makes the move fail with `SegmentRecordUndecodable { index: 0 }`. The
  other faults carried from iteration 3 are still in place: a `Pending` generation, a
  version at `u64::MAX`, wrong-length replacements in both arms, a V+1-byte live row, a
  V-byte row that must still be read, a sum that overflows `u64`, and real concurrent
  moves, root overwrites, root deletes and reclaimed segments over 256 seeds. "Nothing
  written" is always a full-store `scan(b"")` compared before and after.

## Gates run

- `cargo fmt -p wyrd-core`, then `cargo fmt --all --check`: clean.
- `cargo test -p wyrd-core --lib placement_move` under `timeout 1500`: 15/15 pass.
- `./engine/xtask.sh ci` (= `cargo xtask ci`, the C4-ci gate) with `PDCA_WORKTREE` set to
  the worktree, on the final file: **exit 0, "xtask ci: all checks passed"**. Every step
  ran: typos, docs lint and render (link audit OK), gitlink-guard, unsafe-guard,
  `cargo fmt --all -- --check`, workspace clippy/build/test (all 15 `placement_move` tests
  among them), cargo-machete, the three cargo-deny checks, conformance, statics,
  deploy-guard, and the madsim `wyrd-dst` leg. The cargo-deny warnings in the log
  (`license-not-encountered` "ISC", `advisory-not-detected` RUSTSEC-2026-0253) come from
  existing entries in `deny.toml` / `deny-all-features.toml`, not from this diff. The
  `custodian_gc.rs` hang that iteration 3 saw once did not recur. Log:
  `$PDCA_SCRATCH/pdca-builder-776-ci5.log`.
- The bundle's `patch.diff` is byte-identical to `git diff` in the worktree (checked with
  `cmp`).
- The external dependencies the brief lists were all exercised: `typos`, the docs
  renderer, `cargo-deny`, `cargo-machete` (inside `xtask ci`) and `cargo-mutants` (above).
  No NEEDS-HUMAN external dependency.

## Left for the human (deferred `[human]` findings, not changed here)

The driver deferred these to sign-off. I did not change them, since each is a judgment call
and the carry-forward says they are not addressed here.

- **T4 "seeded Tier-0 DST coverage"**: the patch carries an in-crate seeded campaign
  (`seeded_moves_race_each_other_and_the_roots_retirement`, `:5232`, driving `campaign` at
  `:5069`). Whether a madsim `crates/dst` scenario is also needed belongs with #777, where
  the primitive gets its first caller.
- **Flat-arm `Conflict` on a miss no retry can fix**: a `chunk_at` miss in the flat arm is
  a pure function of the caller's arguments, yet returns `Repoint::Conflict` ("re-plan next
  pass"). Brief item 7 allows "conflict/`Blocked`". If #777 needs a distinct outcome,
  `MalformedReplacement` shows the shape (a call-fault error that is not a `ChunkMapError`).
- **`Repoint::Refused` / `VersionExhausted` for a generation the root has already left**:
  the doc (`:3125`) calls `Refused` "not transient", but neither outcome checks that the
  root still names the generation. Two fixes: one extra root read before returning either,
  or a doc line telling the caller to confirm liveness before escalating. Rare in practice
  (a non-conforming row within ~19 bytes of V, plus a race with retirement).
- **T5 prior-art check**: the reviewer's sandbox could not corroborate Plan's
  affected-path history check.
- **Docs currency**: this adds a public library function, an outcome enum and an error
  struct in `wyrd-core`. There is no port, gateway/API operation, RPC, CLI flag or
  persisted field, and the brief limits the change to one file, so I did not touch the
  living architecture doc.

Scratch used: `$PDCA_SCRATCH/pdca-builder-776-{neg4,mutants4,ci5.log}` plus earlier
iterations' leftovers. The CI run built into the worktree's own `target/`. Left for the
harness to clean up, per the builder rule against rm-style commands.
