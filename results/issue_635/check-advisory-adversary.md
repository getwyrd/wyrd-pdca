# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Inputs: `patch.diff`, `brief.md`, `check-gates.json`. All `path:line` below are on the target
worktree at `$PDCA_TARGET` (= `origin/main` @ `9120f7a` + this patch).

## The evidence — attacked, and it holds

I re-ran the asserted red→green rather than trusting the `C4-verify` row.

- **Green post-fix, on the production path**: `cargo test -p wyrd-custodian --test
  segmented_map_consumers` → 12/12 pass in the target worktree.
- **Red pre-fix, and it is an *assertion* red, not a build error** — the failure mode the brief
  warns about (`run-verify.sh`'s unconditional PASS after a build failure). I extracted
  `9120f7a` into scratch, dropped in *only*
  `crates/custodian/tests/segmented_map_consumers.rs:1`, and ran the same target: it **compiles**
  and **12/12 fail** with `panicked at …expect(…)` / `assert_eq!` messages
  (`Error("invalid type: map, expected a sequence", line: 1, column: 23)` propagating out of the
  real `reconcile_step` / `reconcile_after_restore` / `backfill::reconcile` / `high_water_marks`).
  So the red is genuine, the test names only base symbols, and the double is only at the
  `MetadataStore`/`ChunkStore` seams — the loops under test are the production ones.
- Not a tautology: legs assert **positive** observables (`ReconciliationStatus::Pending` for a
  holder, byte-identical `read_object`, fragment *counts* before/after a post-grace GC pass,
  byte-identical stored `inode:`/`seg:` bytes), not the absence of an error.

## Findings

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5753` (`ClassIds::Optional`), with
  `:5148` (`RecoveredIds::contribution`) and `:5078`: the chunk-id floor *under-approximates* for
  a corrupted **flat** root, which is the one direction the brief's containment table forbids
  ("Must be total … and it must **never under-approximate** the floor",
  `brief.md:527`).** Concrete failing input, executed against the patched crates:
  `inode:1 = {"size":8,"chunk_map":{"a":1},"state":"Committed","version":1}` (also reproduced with
  `"chunk_map":null`, and the same class covers a missing `chunk_map` field). The value is valid
  JSON, fails `InodeRecord` decode, carries **no `id` token**, so `raw_chunk_id_floor` reports
  `found == 0, complete == true`; `covers(Optional)` is then `true` and the record contributes
  **0**. Measured: `high_water_marks` → `Ok((max_inode = 1, max_chunk = 0))`, while the identical
  damage one record class down (`seg:… = {}`) correctly contributes `2^64 - 1` via
  `ClassIds::Required`. On the base the same store returned `Err(invalid type: map, expected a
  sequence)` — so this patch converts a fail-closed answer into a silently low floor for exactly
  the case `RecoveredIds::covers` was written to catch. The distinction is decidable from the
  bytes and the module already makes it three hundred lines up (`inode_chunk_map_fault` /
  `:2064` tests `map.is_object()`): an `inode:` value whose `chunk_map` does **not** claim the
  segmented shape is `Required`, not `Optional`. Fix at that boundary rather than widening
  `Optional`.

- **NEEDS-HUMAN [human] — `crates/server/src/lib.rs:124`: the entire chunk-floor recovery
  apparatus this patch adds has no production consumer, so the containment-table row it is built
  for (and the reviewer's likely reading of it) claims more than the code delivers.** `recover`
  is `let (max_inode, _max_chunk) = metadata::high_water_marks(...)` — the chunk floor is
  discarded, and it is the only caller outside tests (`grep high_water_marks` finds only
  `crates/server/src/lib.rs:124` plus doc/test references). The patch nonetheless adds
  `segment_chunk_floor`, `raw_chunk_id_floor`, `json_chunk_id_floor`, `scavenged_chunk_id_floor`,
  `RecoveredIds`, `ClassIds`, `ScannedId`, `torn_digit_escape`, `json_string_token`
  (`crates/core/src/metadata.rs:5063-5624`) plus a full `seg:`-namespace walk on the gateway
  startup path — several hundred lines of production code, and a large share of the surviving-C5
  mutant surface, computing a number nobody reads. Two consequences a human should adjudicate:
  the finding above is **latent rather than live** (its severity depends on whether #636/#508 will
  wire the floor), and the leg A(vii)(a) assertion's own rationale —
  `crates/custodian/tests/segmented_map_consumers.rs:1196` "must not fail the id floor the gateway
  starts from" — is true only for the *totality* half, not the *value* half. Either wire the floor
  or shrink the apparatus to what totality actually requires.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:163-168` and `:210-214`: with one
  unresolvable object, GC mislabels every fragment in the fleet as `skip reason="referenced"` and
  still returns `Reconciled::Satisfied`, which is the exact move the same patch calls forbidden
  for scrub.** `ReferenceSet::protects` (`:269`) now short-circuits `true` while
  `unresolvable` is non-empty, so a fragment that is in neither `placed` nor `malformed` takes the
  `else` arm at `:166` and is audited as *referenced* — an operator reading the durability seam is
  told GC verified a reference it never had. And `gc::reconcile` still answers `Satisfied`
  (`:210`) over an incomplete set, while `scrub::reconcile` was given
  `Reconciled::Blocked` for the identical condition with the rationale that `Satisfied` over an
  incomplete set is "a clean bill for part of the store" (`crates/custodian/src/scrub.rs:194`,
  `crates/custodian/src/reconciliation.rs:25-42`). Add an `"unresolvable"` skip reason and give
  GC the same `Blocked` answer — or say in `review-rejected.md` why GC is different.
  (Related, worth knowing when judging how much the new variant buys: the deployed run loop
  discards it — `crates/server/src/custodian.rs:519`, `:533`, `:610` all match `Ok(_) => {}` — so
  `Blocked` is observable only in tests today.)

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:646` / `:676`, reached once per
  queued repair at `:190-191`: the store-wide chunk lookup now costs one extra metadata round trip
  *per committed object per queued chunk*.** `find_chunk` scans `inode:` and, for every committed
  record, calls `resolve::homes_of` → `metadata::resolve_current_chunk_homes` → `store.get(root)`
  before it can compare ids; on the base the same loop decided from the scan snapshot with no
  further I/O. For a repair queue of `Q` chunks over `N` committed objects the pass goes from `Q`
  namespace scans to `Q × N` point reads (worst case; `N/2` expected per chunk), on the loop the
  deployed custodian drives every interval. The re-read is deliberate and load-bearing for
  *snapshot currency* (`crates/custodian/src/resolve.rs:27-44`), but hoisting it out of the
  per-chunk loop (resolve once per pass into an id→home index) would keep the currency guarantee
  at `N` reads instead of `Q × N`. This is the "O(N) maintenance-pass round-trip regression" §6
  item carried since iteration 11; I cannot see `review-rejected.md` from this leaf, so if it has
  already been recorded-declined with a tracker id, treat this as settled per
  `AGENTS.md:200-203`.

## Attempted and could not refute

- **The flat compatibility contract.** `ChunkMap::Serialize` (`crates/core/src/metadata.rs:1382`)
  delegates straight to the `Vec<ChunkRef>` array — no tag, no wrapper — and the leg A test
  asserts the stored flat `inode:` bytes are unchanged after every pass. I could not construct a
  legacy record whose decode→encode moves.
- **The publication's ordering and completeness rules.** I tried the iteration-5/7/8 failure
  shapes: a shorter same-epoch replan leaving an orphaned tail is refused
  (`PublicationTailStranded`, `:4414`); a flip over a partially written range is refused
  (`DurableRange::WholePlan`, `:4515`); resume trusts nothing (`verify_durable_range` walks the
  **whole** plan, `:4429`); every zero-I/O refusal is decided before the first write (`publish`,
  `:4550`); the fence cycle rule spans prefix + phase + flip (`:4256-4270`).
- **The v10/v13/v16 findings.** `plan_with` now refuses an empty placement (`:3709`);
  `read_group_range` **refuses** rather than clamps a `segment_count` past `MAX_ROOT_SEGMENTS`
  (`:2796`); `next_root_version` is checked at every version-advancing site (`:2346`); a `seg:`
  value of `{}` contributes the ceiling (`ClassIds::Required`, `:5073`); structural faults are
  classified as `ChunkMapError` at the one `decode` boundary (`:1920`) so containment cannot be
  spelled differently per consumer (`crate::resolve::contain` is the crate's only downcast,
  `crates/custodian/src/resolve.rs:273`).
- **Key/prefix aliasing.** `seg:` vs `seggrp:`, epoch `1` vs `11`, non-canonical epochs and
  off-width indices are all rejected or non-overlapping (`:1650-1691`).
