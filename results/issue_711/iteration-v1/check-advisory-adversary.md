# Adversarial review — issue #711 (`repoint-chunk-segmented-placement-moves`)

Advisory only; nothing here gates. Every citation is against the target working tree at
`$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Red→green was re-run from a throwaway
clone of the base commit `92e1b4b`, not from the bundle's own logs.

## Findings

- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:2920`: the `chunk == prior` equality —
  the primitive's *only* guard for the read→prepare window, and the one its own doc calls "the
  pin that turns a map rewritten under the plan into a `Repoint::Conflict` instead of a silent
  overwrite of somebody else's newer placement" (`:2909-2912`) — is exercised by **nothing** in
  this bundle. I deleted it outright (`chunk_at` matching by byte offset alone, `prior` unused)
  and re-ran `cargo test -p wyrd-custodian -p wyrd-core` **and** the full 15-property DST
  campaign at the gate's own `MADSIM_TEST_NUM=50`: **all green**, including
  `segmented_map_repoint.rs`, `placement_ceiling.rs`, and
  `repoint_versus_supersede_commits_all_or_nothing`.
  Concrete failing case the pin is the sole defence against: a repair resolves segmented object
  `O`, chunk `C` at offset 0 with `placement [0,1]`, fragment 1 lost, and plans `new_placement
  [0,2]`. Before `repoint_chunk`'s own re-read at `:2866`, another writer moves fragment 0 to
  server 5, leaving the `seg:` record at `[5,1]`. With the pin: `chunk_at` → `None` →
  `Repoint::Conflict`, nothing written. Without it: index 0 matches on offset alone, the CAS at
  `:2894` is pinned to the *freshly read* `[5,1]` bytes so it **succeeds**, and the record is
  rewritten to `[0,2]` — silently reverting the other writer and re-pointing fragment 0 at a
  server that no longer holds it. Add a leg that seeds a `seg:` record whose chunk at the plan's
  offset differs from `prior` and asserts `Repoint::Conflict` behaviour through the store
  (record byte-identical, obligation still queued, no orphan mark).
- **NEEDS-HUMAN [impl]** — `crates/dst/tests/custodian.rs:2207-2226` is *structurally* unable to
  reach that window, so the brief's claim that the DST leg proves "a repoint whose pinned root
  generation **or** segment bytes changed under it commits nothing" holds only for changes that
  land **after** the prepare. `RaceAtRepoint::commit` takes `self.pending` and applies the racing
  batch **inside the repoint's own `commit()` call** — i.e. strictly after
  `metadata::repoint_chunk` has already done its `store.get(&key)` at `metadata.rs:2866`. Both
  interleaving flags (`repoint_first`) only reorder *two commits*; neither mutates the record in
  the read→prepare gap. I confirmed the two CAS preconditions themselves **are** genuinely
  proven — deleting `.require(root_key, root_bytes)` (`:2893`) and deleting
  `.require(key.clone(), bytes)` (`:2894`) each turn the DST property red at 50 seeds — so the
  gap is exactly and only the reference pin.
- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:2784` ("**What it pins — the exact bytes
  the resolve read**"), echoed at `crates/custodian/src/reconstruction.rs:877` and
  `crates/custodian/src/rebalance.rs:516` and in the brief's scope text, is inaccurate for the
  segmented arm: `:2866` performs a **second, later** `store.get` and `:2894` pins *those* bytes,
  not the ones `resolve_chunk_map` read. That is defensible (it merges a concurrent edit to a
  neighbouring chunk instead of losing its CAS), but it is a different design from the one the
  brief specified, and it is what shifts the whole weight of the window onto the untested
  `:2920` pin. Either pin the resolve's bytes as written, or correct the three doc sites and land
  the test above.
- **NEEDS-HUMAN [impl]** — C5's "17 missed" is not the noise a non-gating mutation row usually
  is, and `AGENTS.md:72-74` explicitly says to inspect survivors "when the change touches
  correctness logic". **All 17 sit inside code this patch introduced** (`repoint_chunk`,
  `covers`, `chunk_at` — `mutants.out/missed.txt`). Two caveats in the fix's favour, which I
  verified: (a) the run scoped every `crates/core` mutant to `--package=wyrd-core@0.0.0` alone
  (`mutants.out/log/crates__core__src__metadata.rs_line_2902_col_77.log`), so it never ran the
  custodian tests that actually drive the primitive; (b) re-running two of them (`covers` `<`→`<=`
  at `:2902`, `chunk_at` `>`→`>=` at `:2917`) against the full custodian suite **kills** both, via
  `segmented_map_reconstruction.rs`'s two-segment fixture. So 17 overstates the gap — but the
  residue is real: `repoint_chunk`/`covers`/`chunk_at` ship with **zero** `wyrd-core` unit tests,
  against a module whose own convention (child-1's boundary test at `:2978-2994`) is to pin
  exactly these rules in-crate.
- **NEEDS-HUMAN [human]** — the patch touches **7** files against the brief's hard "**≤ 6** files"
  budget, and the two test files it edits (`crates/custodian/tests/segmented_map_rebalance.rs`,
  `crates/custodian/tests/segmented_map_reconstruction.rs`) are **not** the pair the brief
  allowed ("at most one of `custodian/tests/{reconstruction,rebalance}.rs`"). The brief's own
  stop rule reads "A seventh file means the shape is wrong … STOP and hand back a proposed
  split"; Do proceeded instead. Mitigation a human should weigh: both edits are *forced* — leg 2
  of each file asserted the #696/#697 refusal that this slice removes, so C4-ci could not stay
  green without them — and none of the named drift files (`backfill.rs`, `restore.rs`, `gc.rs`,
  `desired_state.rs`) is touched. This is a ratify-or-split call, not a code defect.
- `check-gates.json:48` reports C4-verify as "4 test(s) ran red". Only **3** of the 4 legs are
  actually red on the base — `a_repoint_that_would_cross_the_ceiling_over_a_segment_record_is_refused`
  passes pre-fix, exactly as the brief predicted ("do not count it as discriminating evidence",
  brief.md:43-44). The gate line is a test *count*, not a failure count; do not read it as four
  discriminating legs. (Advisory note, no action.)

## Refutations attempted that did not land

I could not break the following, and each was a real attempt, not a skim:

- **The red→green itself.** Cloned base `92e1b4b`, added only the new test file: 3/4 fail
  (`Blocked` vs `Changed`), leg 3 passes. Applied `patch.diff`: 4/4 green, plus
  `segmented_map_{reconstruction,rebalance}.rs` and `placement_ceiling.rs` all green. The test
  drives the real `reconcile_step` over in-memory doubles and reads the store back — no parallel
  re-implementation, no mocked-away defect, and it names no symbol this patch introduces.
- **Serialization identity of the CAS precondition.** `require(root_key, encode(generation))`
  re-encodes a *decoded* record — the rubric's named "serialization identity" class. Refuted:
  `InodeRecord`'s `skip_serializing_if = "Option::is_none"` on `etag`/`content_type`/`modified`
  (`metadata.rs:1409`, `:1415`, `:1421`) makes decode→encode the identity on legacy bytes, and
  that comment says so for precisely this reason.
- **`SegmentRecord::new` re-deriving `byte_len` desynchronising the root table.** Refuted:
  `byte_len == sum(chunk.len)` is a decode invariant (`:1180-1188`) and the resolver rejects a
  record whose extent disagrees with the root's `SegmentRef` (`:2585`), so a placement-only
  rewrite cannot move the span.
- **The flat arm no longer forcing `state: InodeState::Committed`.** Refuted:
  `resolve_current_chunk_map:2694` returns `Ok(None)` for a non-`Committed` root and both callers
  filter the scanned record on `Committed` (`reconstruction.rs:481`, `rebalance.rs:290`), so
  `prior.state` is always `Committed` and a repoint can never publish or un-publish.
- **Multi-segment addressing (`covers` picking the wrong segment).** Refuted: the two-segment
  fixture in `segmented_map_reconstruction.rs` leg 2 kills the boundary mutants, as I verified.
- **`?` escaping from `seg_key` (`:2860`) or `SegmentRecord::new` (`:2883`) and ending a whole
  pass.** Refuted: both failure conditions are decode invariants of a record already in hand.
- **Orphan-marking a position a second object still references** (the brief's leg-4 hazard, and
  the case where `rebalance::plan_evacuations` emits independent plans per object for one
  `ChunkId` with no dedup, `rebalance.rs:342-399`). Refuted by GC itself: `gc.rs:146` /
  `:190-193` treat the committed reference set as a hard safety gate — a fragment any committed
  chunk map still names is never reclaimed regardless of its `orphan:` mark. A stale mark on a
  still-referenced position is inert, not a deletion. The leg-4 property therefore holds for the
  evacuation caller too, by a different mechanism than the repair caller's `sites` dedup.
- **Zero-length chunks, duplicate `ChunkRef`s at one offset, and `checked_add` overflow in
  `chunk_at`.** Walked each by hand; the offset-plus-equality rule disambiguates correctly and
  overflow degrades to `Conflict`.
