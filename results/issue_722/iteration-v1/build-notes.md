# Build notes — issue #722 (segmented drain evacuation completes)

Target: getwyrd/wyrd @ main, base `0fba76c` (worktree `$PDCA_WORKTREE`). #721's primitive
`metadata::repoint_chunk` is present on this base (`crates/core/src/metadata.rs:3240`), and
reconstruction already calls it (`crates/custodian/src/reconstruction.rs:1168`), so the wave
precondition holds. Line numbers below are on the patched tree unless marked "base".

## What changed

Four files, exactly the brief's budget. Non-test change: 82 added non-blank, non-comment lines
in `rebalance.rs` (cap 200). `patch.diff` is 67 KB (cap 95 KB).

### `crates/custodian/src/rebalance.rs`

- **Refusal removed.** Base `rebalance.rs:333-374` (`scanned_flat` / `let Some(prior_chunks) =
  scanned_flat else { refused = true; continue; }`) and base `:397-400` (`emit_refused`) are
  gone, along with `emit_refused` itself (base `:651-669`). A `seg:`-resident chunk now gets a
  plan like a flat one.
- **Plan shape.** `EvacPlan` no longer carries `prior: InodeRecord` + `prior_chunks:
  Vec<ChunkRef>` + `chunk_index` per plan. It carries an index into a per-object snapshot
  (`EvacObject`, `:105`), the chunk's `byte_offset` and its `prior: ChunkRef`. The root record is
  cloned once per object that owes a move (`:402-408`), never per plan, and the resolved chunk
  list is not retained. That meets the brief's bounded-memory constraint, and is *better* than
  the base for flat objects (base deep-copied the whole chunk list into every plan, base `:389`).
- **Byte offset.** Summed over the resolved list as the walk goes (`:360-363`), saturating. A
  flat map and a segmented one are both byte tilings, so `repoint_chunk`'s `chunk_at` finds the
  chunk by offset + equality in either shape.
- **Per-(object, chunk) planning kept.** No cross-object dedup (brief: withdrawn). Doc comment on
  `plan_evacuations` says why (`:288-291`).
- **The move.** `evacuate_chunk` now calls `metadata::repoint_chunk` (`:555-562`) and adds the
  orphan marks to the returned batch (`:604-609`), so placement + orphan evidence are one batch.
  The hand-built flat `InodeRecord` + local `flat_value_ceiling_crossed` check (base `:519-547`)
  and the hand-built `require/put` batch (base `:557-560`) are removed — the ceiling check now
  lives only in the primitive (`metadata.rs:3339-3347`), no second copy, no second value.
- **Verdict mapping mirrors the peer** (`reconstruction.rs:1181-1210`): `Prepared` → continue;
  `Refused`/`VersionExhausted` on a generation the root has left → `Conflict` (via
  `still_current`, `:627`, a copy of the peer's `reconstruction.rs:1238`); `Refused` →
  `EvacOutcome::Refused`; `VersionExhausted` and a typed `ChunkMapError` → new
  `EvacOutcome::Contained` (`:457`), named on the seam through the existing
  `emit_unresolvable` (`:188-191`); any other error ends the pass. `Contained` is not
  `Committed`, so `persisted()` withholds certification with no extra code.
- Doc fixes: module doc invariant list (`:35`, `:51-62`), `EvacScan::withheld`,
  `plan_evacuations` generation paragraph, `EvacOutcome::Conflict`. The salvage's wrong claim
  that the move "pins the exact bytes the resolve read" is **not** carried over: the comment at
  `:540-549` says the move pins the root generation the scan resolved, the `seg:` record **as
  the move itself re-reads it**, and the chunk reference.
- **Not touched (per brief):** the "collectable garbage" comments (`:26`, `:615-616`, the
  `emit_conflict` doc) — owned by getwyrd/wyrd#723. In my own new text I wrote "left in place;
  nothing is retracted", never "collectable garbage".

### `crates/custodian/tests/segmented_map_evacuate.rs` (new — the C4-verify discriminator)

Four legs through `reconcile_step` over in-memory doubles; observes only the store. Names no
symbol this patch adds. Besides the brief's list it names `wyrd_core::metadata::orphan_key`,
`InodeId`, `InodeState`, `SegmentRecord::byte_offset/byte_len`, `wyrd_core::placement::Topology`
and `wyrd_chunk_format::{EcSchemeType, FragmentHeader}` — all on the base, none touched here.

Fixture: RS(2,1) chunk on servers 0/1/2 (domains A/B/C), server `u64::MAX` alone in domain D,
server 0 draining. The only domain distinct from the survivors is D, so the leg-1 assertion
"copied to a non-draining server in a failure domain distinct from the survivors" is an exact
placement check: `[u64::MAX, 1, 2]`.

### `crates/custodian/tests/segmented_map_rebalance.rs`

Leg 2 (`an_owed_segmented_evacuation_is_refused_once_and_mutates_nothing`, base `:333`) asserted
the refusal; rewritten as `an_owed_segmented_evacuation_lands_in_the_seg_records`: both owed
`seg:` records repointed to `TARGET`, bytes copied, each vacated position orphan-marked, root and
the untouched segment byte-identical, two `evacuate` audit lines, no `refused-segmented`. The
header doc notes the rewrite.

### `crates/dst/tests/custodian.rs`

Property 19, `prop_drain_repoint_versus_supersede_is_all_or_nothing`, added to the existing file
and registered in both the per-seed campaign and `committed_regression_seeds_stay_green`.

It drives the **drain** (`RebalanceContext`), not the repair loop. The salvage
(`results/issue_711/iteration-v1/patch.diff`) drove reconstruction; I adapted its
`RaceAtRepoint` store (renamed `RaceAtDrainMove`) and its arms, because this child's new path is
the drain and the rubric wants a new concurrent path covered by seeded Tier-0 DST. Seed picks:
the draining server (0..3), the racer (`Supersede` = new flat root CAS'd on the root;
`Rewrite` = competing writer CASes the same `seg:` record), and the order. Declared outcomes:
move-first → placement + exactly one orphan mark, `Changed`, racer `Committed` (Supersede) or
`Conflict` (Rewrite); racer-first → `seg:` bytes = racer's, no orphan mark, `Blocked`. Both: the
racer fired, and the live generation names only fragments that exist. The doc comment states
what it proves (the two CAS preconditions) and what it does not (the read→prepare window — the
racing batch is applied inside the move's own `commit()`, after the primitive's read).

## Deviation from the brief — leg 3 is keyed to `MAX_VALUE_BYTES`, not V/2

The brief says #721 settled that a `seg:` record is weighed at V/2 (`MAX_ROOT_VALUE_BYTES`,
50 000) and asks leg 3 to be keyed there. **The merged primitive does the opposite:**
`repoint_chunk` weighs both arms through `flat_value_ceiling_crossed` (`metadata.rs:3339-3347`),
which is `encoded.len() > MAX_VALUE_BYTES` (`metadata.rs:605-607`), and its doc says so
explicitly (`metadata.rs:593-600`: "The segmented arm of `repoint_chunk` weighs its record here
too … A placement move never re-encodes a segmented root, so `MAX_ROOT_VALUE_BYTES` bounds only
the publication that writes a segment table"). A leg seeded just under 50 000 would be repointed
and committed by the base primitive — the leg would fail, and the only way to make it pass would
be a second ceiling value in `rebalance.rs`, which the brief forbids.

So leg 3 seeds a `seg:` record of **exactly** `MAX_VALUE_BYTES` (admissible: the resolver reads
up to it, `>` refuses) and the move widens one placement entry from `0` to `u64::MAX` (+19
bytes) — a real crossing of the ceiling the primitive actually applies. The fixture asserts the
seeded length is exact (`padded_segment`). **Sign-off item:** confirm this reading of #721; if
the intended rule really is V/2 for `seg:` records, that is a change to the primitive (#721's
file), not to this caller.

## Red → green evidence

Runner note: the project has no per-test xtask subcommand, and `engine/scripts/run-verify.sh`
works in a shared `../wyrd-verify` checkout with an unbounded `cargo test`. I ran the same
`cargo test -p wyrd-custodian --test segmented_map_evacuate` it would run, in my worktree,
wrapped in `timeout`.

- **Green (fix applied):** 4 passed, 0 failed.
- **Red (base `rebalance.rs` restored via `git show HEAD:…`, test kept):** 4 ran, **3 failed**
  — leg 1 (`left: Blocked, right: Changed` "the move landed"), leg 2 (`Blocked` vs `Changed`
  "both moves landed"), leg 4 ("the racer never fired" — the ordinary reason the brief
  predicts); leg 3 passed (refused for the segmented reason). Matches the brief's expected base
  shape exactly.
- `segmented_map_rebalance`: 5/5 green. Full `cargo test -p wyrd-custodian`: all green.
  `cargo clippy -p wyrd-custodian --all-targets -D warnings`: clean. `cargo fmt --all --check`:
  clean. `typos` on all four files: clean.
- DST property 19 at `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50`: green.
- Full `cargo xtask ci` via `./engine/xtask.sh ci` on the patched worktree (fmt, clippy, build,
  test incl. DST, deny, conformance): `xtask ci: all checks passed`, exit 0.

### Named negations (mutation oracle), each run then reverted with `git checkout` of `metadata.rs`

The two custodian rows were re-run after `cargo fmt` so the cited lines are current.

| Negation (in `crates/core/src/metadata.rs`) | Test | Result |
|---|---|---|
| ceiling check removed: `match flat_value_ceiling_crossed(&next)` → `match None::<usize>` (`:3340`) | `segmented_map_evacuate` | leg 3 **red** (`segmented_map_evacuate.rs:451-455`, "a refusal wrote metadata"); legs 1, 2, 4 green |
| `seg:` precondition removed: `root_pin.require(key.clone(), bytes)` → `root_pin` (`:3324`) | `segmented_map_evacuate` | leg 4 **red** (`segmented_map_evacuate.rs:507-511`, "the competing write was overwritten"); others green |
| root precondition removed: `WriteBatch::new().require(root_key…)` → `WriteBatch::new()` (`:3257`) | DST property 19, 50 seeds | **red** (`custodian.rs:6159-6162`, "a lost move still rewrote the record" — the Supersede/racer-first arm) |
| `seg:` precondition removed (as above) | DST property 19, 50 seeds | **red** (`custodian.rs:6163-6166`, "a lost move still marked a fragment for reclamation" — the Rewrite/racer-first arm) |

Leg 4: I did not find a fixture seam that lands the competing write pre-fix *and* proves a lost
CAS post-fix — pre-fix no plan exists, so there is no commit for the racer to precede. Leg 4's
conflict property rests on the mutation row above and on DST property 19, as the brief allows.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With `rebalance.rs` reverted to base and the test kept, legs 1 and 2
  fail on `Blocked` vs `Changed` (the refusal), leg 4 fails because no move reached a commit.
  Run output quoted above.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` →
  `rebalance::reconcile` → `plan_evacuations` → `evacuate_chunk` → `metadata::repoint_chunk`,
  and the store doubles only store bytes and check preconditions (the same shape as the existing
  `segmented_map_rebalance.rs` double). No production logic is copied into the test. The DST
  property drives the same entry with `RebalanceContext`.
- **(c) Fixture includes the fault?** Yes. The draining server is in the fleet and holds the real
  fragment; the segmented object is raw `seg:` records + a segmented root, which is the shape the
  base refused; leg 2 really shares one `ChunkId` between two objects; leg 3's record is exactly
  at the ceiling and the move really crosses it (fixture asserts the length); leg 4's racer is
  asserted to have fired (`racer.is_none()` after the pass), so the conflict path is exercised,
  not skipped.

## Things for the human at sign-off

1. **Leg 3 V/2 deviation** (above) — the brief's premise does not match the merged primitive.
2. **Docs currency.** No port, API operation, RPC, CLI flag or persisted field changes, so the
   rubric's merge requirement does not trigger. But `docs/design/architecture/08-crosscutting-concepts.md:89`
   names only the reconstruction pass as a caller of `repoint_chunk`; it is still true, just
   incomplete now that the drain uses it too. Updating it would be a fifth file, which the brief
   says means the shape is wrong, so I left it. Suggested one-line follow-up: "The rebalance pass
   moves a draining server's fragment the same way, adding the orphan marks to the same batch."
3. **Stale deferral in the peer.** `reconstruction.rs:1166-1167` says "deferred: #682 — the
   seeded Tier-0 DST property for this move (repoint versus supersede)". Property 19 is that
   property, driven through the drain caller of the same primitive. I did not edit
   `reconstruction.rs` (#721's file, out of scope); the marker can be updated or retired there.
4. **Telemetry removed.** The `rebalance_refused_records` counter and `refused-segmented` audit
   action go away with the refusal; `git grep` finds no dashboard, alert or doc that references
   either.
5. **Inherited leak (getwyrd/wyrd#723), not new.** A lost CAS leaves the copied destination
   fragment unreferenced and unmarked — the flat path already does this; this patch extends it to
   `seg:` records, as the brief decided. Leg 4 asserts the copy is left in place, not retracted.
6. Behaviour shift on the flat path (intended, from adopting the primitive): the version is now
   advanced **checked** (`u64::MAX` → `Contained`, not a wrap), and `state` is carried over rather
   than re-stamped `Committed` (the scan only plans committed records, so this is the same value).
