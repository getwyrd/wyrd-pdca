# Adversarial review — issue #776 (`repoint_chunk`, seg-record placement move)

**Bottom line: I tried to refute the fix and could not break its core.** Every guard the brief names goes red when removed. The three findings below are minor. None shows a wrong write, a lost write, or a batch that lands when it should not.

## What I re-ran (toolchain present, scratch copy of `$PDCA_TARGET`)

- The 16 new `metadata::placement_move` tests pass on the patched tree, matching `gate-logs/C4-ci.log:871-908`.
- `C4-verify` is green-only, as `brief.md` declares (`gate-logs/C4-verify.log`, "PASS (green-only)"). So I did not rely on it. `C5-mutants` reports 36 mutants, 27 caught, 9 unviable, 0 missed, but the log does not name them. To fill that gap I applied 13 hand mutations that `cargo mutants` does not generate. 11 were caught, including all four negations the brief names:
  - drop `&& chunk == prior` (`crates/core/src/metadata.rs:3350`): 5 tests go red (sibling/planned-chunk conflict, zero-length boundary, `chunk_at` unit, flat conflict, seeded campaign).
  - drop the root pin in the segmented arm (`:3301`): superseded-root test and seeded campaign go red.
  - drop the segment pin (`:3301`): sibling-edit test and seeded campaign go red.
  - disable the `bytes.len() > MAX_VALUE_BYTES` guard (`:3272`): over-ceiling-row test goes red.
  - drop the `byte_offset` half of the bounds check, drop the `retired_or` call (`:3309`), turn the zero-length `continue` into `Conflict`, swap `checked_add` for `saturating_add` in `chunk_at`, disable the placement-length check (`:3224`), skip `weighed` in the segmented arm, drop the flat version bump: each caught.
  - Survivor 1: deleting `if at > byte_offset { break; }` in `chunk_at`. This is an equivalent mutant. `at` never decreases, so the break only ends the loop early. Not a finding.
  - Survivor 2: forcing `state: InodeState::Committed` in the flat arm. Finding 1 below.
- Other attacks that did not land:
  - Duplicate identical `ChunkRef`s in one map, which would defeat the equality pin. Chunk ids are minted unique (`crates/server/src/lib.rs:255`), and CopyObject / UploadPartCopy are refused (`crates/gateway-s3/src/lib.rs:1713-1730`), so no conforming writer can produce two in one map.
  - A zero-length segment making the "at most two records read" claim false. It is refused at decode (`metadata.rs:1152`, `:1401`).
  - `SegmentRecord::new` failing after a placement-only edit. It runs the same checks as decode (`:1385-1435`), so it cannot fail.
  - A root table over `MAX_ROOT_SEGMENTS`. The move does not refuse it, which matches the documented rule that the capacity guard applies only where a table becomes work (`:534-539`).
  - Any `MAX_ROOT_VALUE_BYTES` comparison in the diff. None exists; it appears only in doc comments.
- The previous round's 8 blocking findings are addressed and pinned by tests:
  - over-ceiling segment row: `:4893-4922`, and the mutation above is caught.
  - malformed replacement placement: `:4925-4952`.
  - overflow in `chunk_at`: `:4996-4997` now queries at `u64::MAX`, and the `saturating_add` mutant is caught.
  - seeded Tier-0 campaign: `:5002-5235`. It drives the production resolver and redb, and it catches the pin removals.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3194` says the flat arm leaves "`state` left as it was", but no test pins it. Every test builds a `Committed` generation (`flat_root`, patch test helper). Concrete case: change the flat arm's `InodeRecord { .. }` to add `state: InodeState::Committed` — exactly the peer idiom at `metadata.rs:2183` that the brief says to mirror — and all 16 `placement_move` tests still pass. I ran this. Under that change, a move on a `Pending` generation would publish it as `Committed`. Current callers skip non-committed records (`crates/custodian/src/reconstruction.rs:633`), so the risk is low. The fix is small: add a `Pending` flat generation to `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata` (`:4556`) and assert the state is unchanged. Or refuse a non-`Committed` generation outright.
- NEEDS-HUMAN [impl] — The two new `ChunkMapError` variants break that enum's own doc contract. `crates/core/src/metadata.rs:609-613` says every variant is "a structural violation of the segmented chunk-map shape", raised at decode or at a site that met an unwired `Segmented` map. `VersionExhausted` (`:848`) is neither, and `ReplacementPlacementMalformed` (`:857`) is a bad argument from the caller, not an object fault. This matters for #777: every custodian consumer downcasts `ChunkMapError` to mean "this object is unreadable, contain it and keep walking" (`crates/custodian/src/reconstruction.rs:639-645`, and the same pattern at `gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A planner bug that passes a wrong-length placement would then be filed as per-object corruption instead of surfacing as a bug. Fix inside the same file: update the enum doc to admit write-side refusals, or return the placement refusal as its own type (`MalformedPlacement` at `:460` already has the same `{expected, actual}` shape).
- NEEDS-HUMAN [human] — In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.

## On the reviewer's verdict

- `check-gates.json` row `C4-verify` = `pass` is green-only. It proves nothing per fix, and the brief says so up front. It is not a hidden rationalization.
- The `C5-mutants` claim of zero survivors holds for what `cargo mutants` generates. My hand mutations reproduce the brief's four named negations. The one gap is the unpinned `state` behaviour in finding 1.
- I found no claim in `brief.md` or `check-gates.json` that the evidence does not support.
