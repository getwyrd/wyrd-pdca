# Build notes — issue #722, iteration 2 (segmented drain evacuation completes)

Target: getwyrd/wyrd @ main, base `0fba76c` (worktree `$PDCA_WORKTREE`). #721's primitive is on
this base: `metadata::repoint_chunk` (`crates/core/src/metadata.rs:3240`, commit `842f48e`) and
the peer's root re-check (`659ed31`, `crates/custodian/src/reconstruction.rs:1189-1193`). Line
numbers below are on the patched tree unless marked "base".

Four files, the brief's budget. Non-test change: **105** added non-blank, non-comment lines in
`rebalance.rs` (cap 200). `patch.diff` is **87 KB** (cap 95 KB). No edit to `metadata.rs`,
`reconstruction.rs` or any other out-of-scope file.

## What this iteration changed, finding by finding

I started from the iteration-1 patch (its move, its tests, its DST property) and fixed what
the carry-forward block names. The rejected parts are not resubmitted unchanged.

### 1. Offset overflow became a conflict every pass (T3 FAIL, T5, review-batch BUG)

Iteration 1 summed the chunk offset with `saturating_add`. A flat map (no length check at
decode) with lengths `[u64::MAX, 1, drained]` planned the third chunk at `u64::MAX`, which the
primitive's checked walk (`metadata.rs:3367-3379`) can never reach, so every pass logged an
unnamed `conflict`.

Fix, mirroring the peer (`reconstruction.rs:673-698`): the offset is now `Option<u64>` built
with `checked_add` (`rebalance.rs:382-386`). When a chunk that owes a move has no address, the
object is **contained** — named once with "the chunk lengths overflow the object's byte range"
— and the walk breaks out of that object (`rebalance.rs:418-424`). Leg 10 pins it, including
the "healthy object later in key order still drains" half the reviewer asked for.

Behaviour against the base, stated plainly: the base moved that chunk (it planned by index,
not offset). The patched pass names the object and does not move it. The primitive addresses by
offset and cannot reach the chunk, and editing the primitive is out of scope (brief: a fifth
file means the shape is wrong). A flat map whose lengths sum past `u64` cannot describe a real
object (`size` is a `u64`), so naming it for an operator is the right signal, and it is what
the peer already does for the same case.

### 2. Segmented root under a non-canonical key (`inode:03`)

The scan parses `inode:03` to id 3, but the move pins `inode:3`. Iteration 1 copied the
fragment and lost the CAS every pass, unnamed. Fix: the peer's guard
(`reconstruction.rs:652-660`), as `canonical = scanned record is flat || key ==
inode_key(id)` (`rebalance.rs:374-375`). On the first owed chunk of a non-canonical segmented
row the object is contained and named by its own key (`rebalance.rs:413-417`). A flat row under
such a key is left alone, as it is in the peer (#698 owns it, and the base behaves the same
there). Unparseable keys still `continue` as on the base (`rebalance.rs:336-338`, #698). Leg 11
pins it.

### 3. Contained fault named once per chunk instead of once per object

`EvacScan::withheld: bool` is replaced by `contained: BTreeSet<Vec<u8>>` (`rebalance.rs:254`)
and one helper, `contain` (`rebalance.rs:260-264`), that emits `emit_unresolvable` only on the
first insert of a key, mirroring the peer's `Reading::contain` (`reconstruction.rs:538-543`).
All four scan-time sites (decode failure, resolver failure, non-canonical, overflow) and the
work loop's `Contained` arm (`rebalance.rs:189-193`) go through it, so a scan-time containment
and a move-time one for the same object also share one name. Certification reads
`!scan.contained.is_empty() || unmoved` (`rebalance.rs:206`). Legs 6, 7, 10 and 11 assert
exactly one event and one counter tick (`assert_contained_once`,
`segmented_map_evacuate.rs:205-214`).

Rejected alternative: keep `withheld: bool` and add a second set used only by the work loop.
That is about the same size (+1 field, +1 set, the two existing sites untouched instead of
-2/+1 lines each) but leaves a double name possible when the scan contains an object for
overflow after planning its earlier chunks, and a move of one of those chunks then contains it
again (e.g. a flat object at `version == u64::MAX`). One set closes that case and matches the
peer.

### 4. The drain's own prepare-time handling had no test (C4 diff-cov 75.4%, C5 two survivors)

New legs 5–9 in `segmented_map_evacuate.rs` drive each prepare-time answer through the
drain. The double gets one new seam, `AtMoveRead` (`segmented_map_evacuate.rs:89-94`): a
write or a store fault applied on the first `seg:` `get`. The resolver reads `seg:` records
only with `scan_page` (`metadata.rs:2853-2855`), so a `seg:` `get` during a pass is the move's
own re-read (`metadata.rs:3288`). Each leg asserts the seam fired (`fx.fired()`), so none
passes by never reaching the move.

- Leg 5 — ceiling refusal on a **superseded** root → `conflict`, never `refused-ceiling`
  (`:633-656`). This kills both iteration-1 survivors (`!still_current(..)` guard → `false`,
  and `still_current` → `Ok(true)`, both at `rebalance.rs:606` / `:662`).
- Leg 6 — `VersionExhausted` on a flat record → contained, named once (`:663`).
- Leg 7 — torn `seg:` record under the move → contained, named once (`:691`).
- Leg 8 — store fault on the move's `get` → the pass ends with `Err` carrying it (`:724`).
- Leg 9 — the planned chunk edited before the move reads it → `Repoint::Conflict` at prepare
  time, nothing copied (unlike leg 4's commit-time loss, which leaves the copy) (`:752`).

These legs live in the **new** file, not `segmented_map_rebalance.rs`, because the diff
coverage gate measures under the patch's own added test only (`run-diff-cov.sh:54`, "the same
`-p <pkg> --test <name>`"). In `segmented_map_rebalance.rs` they would not count.

### 5. DST comment claimed coverage it did not have

`crates/dst/tests/custodian.rs:5921-5930` now says the property does not exercise the drain's
handling of a prepare-time answer, and names where that is covered: the primitive's re-read by
`segmented_map_repoint.rs` (reconstruction caller) and the drain's handling by legs 5–9 of
`segmented_map_evacuate.rs`.

### 6. Nit — stale doc on `reconcile`

`rebalance.rs:142-144` no longer says `Blocked` covers "an evacuation it may not perform"; it
cites `EvacScan::contained`. I also restored a base comment iteration 1 had deleted for no
reason (the `deferred: #682` discharge note on the `Aborted` arm, `rebalance.rs:198-201`).

### Carried forward from iteration 1 unchanged (reviewed, still correct)

- The refusal is gone (base `rebalance.rs:346-355`, base `:378-381`, base `emit_refused`).
- `EvacObject` (`rebalance.rs:105-111`): the root is cloned once per object that owes a move
  (`rebalance.rs:436-442`), never per plan, and the resolved chunk list is not kept. This meets
  the bounded-memory constraint and is smaller than the base, which deep-copied the whole flat
  list into every plan (base `rebalance.rs:389`).
- Per-(object, chunk) planning kept, no cross-object dedup (doc at `rebalance.rs:304-307`).
- The move is `metadata::repoint_chunk` (`rebalance.rs:590-598`); the orphan marks join the
  returned batch (`rebalance.rs:639-644`), so placement and orphan evidence are one commit. The
  hand-built `InodeRecord`, the local ceiling check (base `:519-547`) and the hand-built batch
  (base `:557-560`) are removed: one ceiling check, in the primitive (`metadata.rs:3338-3347`).
- Verdict mapping mirrors the peer (`reconstruction.rs:1181-1210`): `rebalance.rs:599-629`,
  `still_current` at `:662-665`.
- The comment on the move does not repeat the salvage's wrong claim that the move "pins the
  exact bytes the resolve read"; it says the `seg:` record is pinned "as the move itself
  re-reads it" (`rebalance.rs:578-580`).
- Not touched, per the brief: the "collectable garbage" wording (module doc `:26`, the commit
  conflict comment `:652-653`, `emit_conflict`), owned by getwyrd/wyrd#723. My own new text
  says "left in place; nothing is retracted".

## Deviation from the brief — leg 3 is keyed to `MAX_VALUE_BYTES`, not V/2 (unchanged)

The merged primitive weighs a `seg:` record against the full `MAX_VALUE_BYTES`
(`metadata.rs:3218-3219`, `:3338-3347` → `flat_value_ceiling_crossed`, `metadata.rs:605`),
not `MAX_ROOT_VALUE_BYTES` (`metadata.rs:574`). A record seeded just under 50 000 would be
repointed and committed (the iteration-1 reviewer reproduced a 49 999 → 50 018 byte commit).
Leg 3 therefore seeds exactly `MAX_VALUE_BYTES` and the move widens one placement entry by 19
bytes, a real crossing of the ceiling the primitive applies; the fixture asserts the exact
length (`padded_segment`). Making it V/2 would need either a second ceiling in `rebalance.rs`
(forbidden by the brief) or a primitive change (#721's file). This is already held in
`deferred-findings.json` for the human; I did not re-argue it.

## Red → green evidence

Runner: the project has no per-test xtask entry. `run-verify.sh` runs
`cargo test -p wyrd-custodian --test segmented_map_evacuate` in a shared checkout; I ran the
same command in my worktree under `timeout 600`.

- **Green (fix applied):** 11 passed, 0 failed.
- **Red (base `rebalance.rs` via `git show HEAD:…`, tests kept):** 11 ran, **10 failed**. Only
  leg 3 passed (refused for the segmented reason, as the brief predicts). Legs 1 and 2 fail on
  `Blocked` vs `Changed` (`:405`, `:462` at the time of the run); leg 4 on "the racer never
  fired"; leg 6 panics in the base at `rebalance.rs:525` (`version + 1` overflow); legs 10 and
  11 find no containment event `(0, 0)` vs `(1, 1)`; legs 5, 7, 8, 9 never reach a move.
  **The C4-verify count is now 11 ran / 10 failing, not the 4 / 3 the brief's posture
  predicts.** Legs 1 and 2 are still the discriminating evidence for the brief's success
  criterion; the added legs exist because the iteration-1 review required them.
- **Against iteration 1's `rebalance.rs` (tests kept):** 4 failed — leg 6 and leg 7 `(2, 2)`
  events vs `(1, 1)` (named per chunk), leg 10 and leg 11 `(0, 0)` (overflow and non-canonical
  not contained). Legs 5, 8, 9 pass there; they cover arms iteration 1 had but did not test.
- `segmented_map_rebalance`: 5/5. Full `cargo test -p wyrd-custodian`: all green.
  `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean. `cargo fmt --all
  --check`: clean. `typos` on all four files: clean (one hit, "mis-moves", fixed).
- DST property 19 at `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`: green.
- **Full gate** `./engine/xtask.sh ci` on the final worktree: `xtask ci: all checks passed`,
  exit 0 (log shows `segmented_map_evacuate` and the madsim `drain_repoint_versus_supersede…`
  test running).
- **C5 (same script, `scripts/mutants-in-diff`):** 18 mutants, **15 caught, 0 missed**, 3
  unviable. Iteration 1 had 2 missed.
- **C4 diff coverage (measured locally):** `cargo llvm-cov -p wyrd-custodian --test
  segmented_map_evacuate`, scored with `run-diff-cov.sh --changed-lines` / `--score`: **80 of 82**
  instrumentable changed lines executed (97.6%, floor 80%). The two misses are
  `rebalance.rs:329` and `:356`, the existing decode / resolver containment sites now routed
  through `contain`; `segmented_map_rebalance.rs` leg 3 runs them. I did not run the gate's own
  wrapper because it builds in a shared `../wyrd-cov` checkout outside my roots.

### Named negations (mutation oracle) — run, then `metadata.rs` restored with `git checkout`

| Negation in `crates/core/src/metadata.rs` | Test | Result |
|---|---|---|
| root `require` deleted: `WriteBatch::new().require(root_key…)` → `WriteBatch::new()` (`:3257`) | DST property 19, 50 seeds | **red** at `custodian.rs:6163` "a lost move still rewrote the record (drained 0, racer Supersede, move first: false)"; custodian legs green |
| `seg:` `require` deleted: `root_pin.require(key.clone(), bytes)` → `root_pin` (`:3324`) | DST property 19, 50 seeds | **red** at `custodian.rs:6167` "a lost move still marked a fragment for reclamation (drained 1, racer Rewrite, move first: false)" |
| same | `segmented_map_evacuate` | leg 4 **red** ("a lost move does not certify", `:595`); others green |
| ceiling check deleted: `match flat_value_ceiling_crossed(&next)` → `match None::<usize>` (`:3340`) | `segmented_map_evacuate` | leg 3 **red** (`:535`, `Changed`), leg 5 **red** (`:655`, "a refusal copied"); DST green |

Leg 4: I found no fixture seam that lands the competing write before the fix and still proves a
lost CAS after it. Before the fix no plan exists, so no commit happens for the racer to beat.
Leg 4's conflict property rests on the mutation row above and on DST property 19, as the brief
allows.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With `rebalance.rs` reverted to the base and the test file kept,
  10 of 11 legs fail (output summarised above); legs 1 and 2 fail on the refusal itself
  (`Blocked` vs `Changed`). With iteration 1's `rebalance.rs`, the four legs aimed at its
  defects fail. Leg 3 is green on the base by design and bound instead by the ceiling negation.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` →
  `rebalance::reconcile` → `plan_evacuations` → `evacuate_chunk` → `metadata::repoint_chunk`.
  The doubles only store bytes, check preconditions, and inject one write or fault at a named
  seam. No production logic is copied into the test. The test names no symbol this patch adds.
- **(c) Fixture includes the fault?** Yes. The draining server is in the fleet and holds the
  real fragment. Segmented objects are raw `seg:` records plus a segmented root, the shape the
  base refused. Leg 2 really shares one `ChunkId`. Leg 3's record is exactly at the ceiling
  (asserted). Legs 4, 5, 7, 8 and 9 assert their injected race or fault actually fired. Leg 10's
  lengths really overflow (`u64::MAX + 1`), leg 11's root really sits at `inode:03`.

## Prior art (T4 asked for it)

`git log origin/main -- crates/custodian/src/rebalance.rs`: `c40948a` (staged bytes in drain
status), `9470de5` (fragment-write deadline), `d2609b2` (#710 ceiling refusal), `3829097` (#696,
the containment this completes), `3e05891` (segmented shape). `segmented_map_rebalance.rs`:
`3829097`, `9470de5`. `crates/dst/tests/custodian.rs`: Tier-0 properties, latest `fc7eecc`; no
drain repoint property exists. `segmented_map_evacuate.rs`: no history on any branch. #721's
commits `842f48e` and `659ed31` are ancestors of `origin/main`. Closed PR #647 and the
iteration-1 attempt are covered by the brief; nothing here reintroduces `resolve.rs`.

## Things for the human at sign-off

1. **Leg 3 V/2 deviation** (above) — already in `deferred-findings.json`.
2. **C4-verify count** is 11 ran / 10 failing on the base, not 4 / 3. Legs 1 and 2 remain the
   discriminating evidence.
3. **Docs currency.** No port, API operation, RPC, CLI flag or persisted field changes.
   `docs/design/architecture/08-crosscutting-concepts.md:89` names only the reconstruction pass
   as a caller of `repoint_chunk`; still true, now incomplete. A fifth file breaks the budget,
   so I left it. Suggested follow-up sentence: "The rebalance pass moves a draining server's
   fragment the same way, adding the orphan marks to the same batch."
4. **Stale deferral in the peer.** `reconstruction.rs:1166-1167` says "deferred: #682 — the
   seeded Tier-0 DST property for this move (repoint versus supersede)". Property 19 is that
   property, through the drain caller. `reconstruction.rs` is #721's file; I did not edit it.
   Same for the primitive's doc, `metadata.rs:3238` ("The custodian's reconstruction pass is
   the caller").
5. **Telemetry removed.** The `rebalance_refused_records` counter and the `refused-segmented`
   audit action go away with the refusal. `git grep` finds no doc, dashboard or alert using
   them (only a negative assertion in `segmented_map_rebalance.rs:384`).
6. **Inherited leak (getwyrd/wyrd#723), not new.** A lost CAS at commit leaves the copied
   destination fragment unreferenced and unmarked. The flat path already does this; this patch
   extends it to `seg:` records, as the brief decided. Leg 4 asserts the copy is left in place,
   not retracted. Prepare-time losses (leg 9) copy nothing.
7. **Behaviour shifts on the flat path**, both from routing through the primitive: a flat
   record at `version == u64::MAX` is now contained and named (the base panicked in debug /
   wrapped in release on `version + 1`, base `rebalance.rs:525`); a flat map whose lengths
   overflow `u64` is contained and named instead of having its chunk moved by index. `state`
   is carried over rather than re-stamped `Committed` (the scan plans committed records only,
   so the value is the same).
