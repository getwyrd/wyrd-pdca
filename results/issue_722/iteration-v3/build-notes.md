# Build notes — issue #722, iteration 3 (segmented drain evacuation completes)

Target: getwyrd/wyrd @ main, base `0fba76c` (worktree `$PDCA_WORKTREE`). #721's primitive is on
this base: `metadata::repoint_chunk` (`crates/core/src/metadata.rs:3240`, commit `842f48e`) and
the peer's root re-check (`659ed31`). Line numbers below are on the patched tree unless marked
"base".

Four files, the brief's budget. Non-test change: **105** added non-blank, non-comment lines in
`rebalance.rs` (cap 200), unchanged from iteration 2. `patch.diff` is **90.6 KB** (cap 95 KB).
No edit to `metadata.rs`, `reconstruction.rs` or any other out-of-scope file.

## What iteration 3 changed

Iteration 2 passed every gate; the round-2 carry-forward names one implementation-level gap.
I re-applied iteration 2's patch unchanged and added only test code. **`rebalance.rs`, the DST
property and `segmented_map_rebalance.rs` are byte-identical to iteration 2.**

### 1. Nothing tested which generation the move pins (the carry-forward's finding)

`rebalance.rs:298-302` says every move is conditioned on "the generation the resolve ANSWERED
FROM", and `rebalance.rs:439` builds `EvacObject::prior` from `resolved.record`
(`rebalance.rs:364-367`), not from the scan's own snapshot. The adversary's hand mutation
`prior: record.clone()` passed the whole custodian suite.

New **leg 12**, `a_root_superseded_mid_resolve_is_moved_on_the_successor`
(`crates/custodian/tests/segmented_map_evacuate.rs:874-910`), the shape the adversary proposed:

- The double gets one new seam, `MemMeta::after_seg_page` (`segmented_map_evacuate.rs:108-110`):
  a `(key, bytes)` write applied once, right after the first `seg:` page is answered
  (`:132-140`). The resolver reads a group's `seg:` range only through `scan_page`
  (`metadata.rs:2853-2855`), then re-reads the root (`metadata.rs:2992`), so this lands
  exactly between the resolver's page read and its root re-read.
- The leg seeds root R1 (group epoch 3) and, ahead of time, the `seg:` record of a successor
  group (epoch 33) naming the same drained chunk — the root moves last, as in a real
  supersede. The hook installs R2 (version 1, naming the epoch-33 group) at `inode:3`. The
  resolver's re-read sees a different group, answers `Superseded` (`metadata.rs:2757-2761`),
  and restarts onto R2 (`metadata.rs:3051`, `:3074-3103`).
- Asserts: the hook fired; the pass answers `Changed`; R2's `seg:` record names
  `[FREE, S1, S2]`; R2's root is byte-identical (the move never rewrites a segmented root);
  R1's retired `seg:` record is untouched; the fragment is on `FREE`; the vacated position is
  orphan-marked; the drain status is `Satisfied`.

Fixture refactor to support it: `seed_segmented` now calls a new `seed_group(epoch, segments,
version)` (`:305-327`) that writes the `seg:` records and returns the root without writing it,
and `seed_root` uses a new free function `root(..)` (`:401-409`). Every older leg seeds exactly
what it did before (same epochs, same bytes), and all of them still pass.

**Mutant check (run).** With `rebalance.rs:439` changed to
`prior: { let _ = &prior; record.clone() }` (the `let _` keeps the build free of an unused-variable
error, since the workspace builds with `-D warnings`), leg 12 is **red** at
`segmented_map_evacuate.rs:900`: `left: Blocked, right: Changed`. The other 11 legs stay green,
so leg 12 is the only test that binds this property. Restored afterwards (copy of the patched
file kept under `$PDCA_SCRATCH`, then copied back; `git diff` confirmed).

I did not add a second leg for a *flat* successor. The mutant does the same thing there (pins
R1, loses the root CAS), and one leg is enough to bind the line.

### 2. Leg 2's doc comment argued for the rejected dedup

The adversary flagged that leg 2's doc said an object left naming the drained server "would
name a fragment GC is free to reclaim". That is false: GC never reclaims a fragment a committed
map names (`crates/custodian/src/gc.rs:143-147`, `:190-193`), which is the brief's own reason (a)
for withdrawing cross-object dedup. The comment now says the cost is that the drain never
certifies, and says why it is not a byte-loss risk (`segmented_map_evacuate.rs:473-477`).

### 3. Module doc

The file's top comment now lists leg 12 and says legs 5–12 are red on the base
(`segmented_map_evacuate.rs:28-34`).

## Carried forward from iterations 1–2 unchanged (reviewed, still correct)

- The segmented refusal is gone (base `rebalance.rs:346-355`, base `:378-381`, base
  `emit_refused`). The move is `metadata::repoint_chunk` (`rebalance.rs:590-598`); the orphan
  marks join the returned batch (`rebalance.rs:640-645`), so placement and orphan evidence are
  one commit. The hand-built `InodeRecord`, the local ceiling check (base `:519-547`) and the
  hand-built batch (base `:557-560`) are removed: one ceiling check, in the primitive.
- `EvacObject` (`rebalance.rs:105-111`): the root is cloned once per object that owes a move
  (`rebalance.rs:436-442`), never per plan; the resolved chunk list is not kept. Smaller than
  the base, which deep-copied the whole flat list into every plan (base `rebalance.rs:389`).
- Per-(object, chunk) planning kept, no cross-object dedup (doc at `rebalance.rs:304-307`).
- Offset carried as `Option<u64>` with `checked_add` (`rebalance.rs:382-385`); an
  unaddressable owed chunk contains the object, named once (`rebalance.rs:418-424`, leg 10).
- Non-canonical segmented root key contained (`rebalance.rs:374-375`, `:413-417`, leg 11). The
  guard reads the scanned record's shape, as the peer does (`reconstruction.rs:652-659`).
- One `contained: BTreeSet<Vec<u8>>` and one `contain` helper (`rebalance.rs:254`, `:260-264`):
  each object is named once per pass, at scan time or move time (legs 6, 7, 10, 11).
- Verdict mapping mirrors the peer: `rebalance.rs:599-629`, `still_current` at `:662-665`
  (legs 5–9).
- The move's comment does not repeat the salvage's wrong claim that it "pins the exact bytes
  the resolve read"; it says the `seg:` record is pinned "as the move itself re-reads it"
  (`rebalance.rs:578-580`).
- Not touched, per the brief: the "collectable garbage" wording (module doc `:26`, the commit
  conflict comment `:652-653`, `emit_conflict`), owned by getwyrd/wyrd#723.

## Deviation from the brief — leg 3 is keyed to `MAX_VALUE_BYTES`, not V/2 (unchanged)

The merged primitive weighs a `seg:` record against the full `MAX_VALUE_BYTES`
(`metadata.rs:3218-3219`, `:3338-3347` → `flat_value_ceiling_crossed`), not
`MAX_ROOT_VALUE_BYTES`. A record seeded just under 50 000 would be repointed and committed (both
iteration reviewers reproduced a 49 999 → 50 018 byte commit). Leg 3 seeds exactly
`MAX_VALUE_BYTES`, and the move widens one placement entry by 19 bytes, a real crossing of the
ceiling the primitive applies (`padded_segment` asserts the exact length). Keying it to V/2
would need a second ceiling in `rebalance.rs` (forbidden by the brief) or a primitive change
(#721's file). Already in `deferred-findings.json` for the human; I did not re-argue it.

## Red → green evidence

Runner: there is no per-test xtask entry. `run-verify.sh` runs
`cargo test -p wyrd-custodian --test segmented_map_evacuate`; I ran the same command in my
worktree under `timeout 900`.

- **Green (fix applied):** 12 passed, 0 failed.
- **Red (base `rebalance.rs` via `git show HEAD:…`, tests kept):** 12 ran, **11 failed**, only
  leg 3 passed (refused for the segmented reason, as the brief predicts). Legs 1 and 2 fail on
  their `Changed` assertion (`segmented_map_evacuate.rs:434`, `:492`: the base answers
  `Blocked`), which is the refusal itself. Leg 12 fails at `:900` the same way. Leg 4 fails on
  "the racer never fired" (`:619`), the ordinary reason the brief describes. Leg 6 panics in
  the base at `rebalance.rs:525` (`version + 1` overflow). Legs 10 and 11 fail in
  `assert_contained_once` (`:221`). Legs 5, 7, 8, 9 never reach a move.
  **The C4-verify count will be 12 ran / 11 failing, not the 4 / 3 the brief's posture
  predicts.** Legs 1 and 2 are still the discriminating evidence for the success criterion;
  legs 5–12 exist because the iteration-1 and iteration-2 reviews required them.
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-custodian --all-targets -- -D
  warnings`: clean. `typos` on all four files: clean.
- **Full gate** `./engine/xtask.sh ci` on the final worktree: `xtask ci: all checks passed`,
  exit 0. The log shows `a_root_superseded_mid_resolve_is_moved_on_the_successor ... ok` and
  the madsim `drain_repoint_versus_supersede_is_all_or_nothing ... ok`.
- **Not re-run this iteration:** C5 (`scripts/mutants-in-diff`) and the diff-coverage gate.
  `rebalance.rs` is byte-identical to iteration 2, where C5 was 15 caught / 0 missed / 3
  unviable and diff coverage 80 of 82 lines. The reviewer's mutant is a struct-field expression
  change that cargo-mutants does not generate, which is why it survived C5; I ran it by hand
  (table below).

### Named negations (mutation oracle)

The DST property, `metadata.rs` and `rebalance.rs` are byte-identical to iteration 2, so the
DST rows below are iteration 2's runs and still describe this patch. I re-ran the custodian
rows on this iteration's test file because its fixture code changed.

| Negation | Test | Result |
|---|---|---|
| root `require` deleted in `repoint_chunk` (`metadata.rs:3257`) | DST property 19, `MADSIM_TEST_NUM=50` | **red** at `crates/dst/tests/custodian.rs:6163` "a lost move still rewrote the record" (iteration 2 run) |
| `seg:` `require` deleted (`metadata.rs:3324`) | DST property 19, 50 seeds | **red** at `custodian.rs:6167` "a lost move still marked a fragment for reclamation" (iteration 2 run) |
| same: `root_pin.require(key.clone(), bytes)` → `{ let _ = bytes; root_pin }` | `segmented_map_evacuate` | leg 4 **red** at `:623` "a lost move does not certify"; other 11 green (this iteration) |
| ceiling check deleted: `match flat_value_ceiling_crossed(&next)` → `match None::<usize>` (`metadata.rs:3340`) | `segmented_map_evacuate` | leg 3 **red** at `:563` (`Changed`, "the drain is not certified"), leg 5 **red** at `:683` "a refusal copied"; other 10 green (this iteration) |
| `rebalance.rs:439` pins the scan snapshot (`record.clone()`) | `segmented_map_evacuate` | leg 12 **red** at `:900` (`Blocked` vs `Changed`); other 11 green (this iteration) |

Each `metadata.rs` negation was restored with `git checkout crates/core/src/metadata.rs` (the
patch does not touch that file); `git diff --stat` afterwards shows only the four patch files,
and `patch.diff` was re-compared byte-for-byte against the worktree (`cmp`: identical). Both
iteration reviewers also reproduced the two DST negations independently.

Leg 4: I found no fixture seam that lands the competing write before the fix and still proves a
lost CAS after it. Before the fix no plan exists, so no commit happens for the racer to beat.
Leg 4's conflict property rests on the `seg:` `require` negation and on DST property 19, as the
brief allows.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With `rebalance.rs` reverted to the base and the test file kept,
  11 of 12 legs fail; legs 1 and 2 fail on the refusal itself (`Blocked` vs `Changed`). With
  only the reviewer's mutant applied, leg 12 fails and the rest pass. Leg 3 is green on the base
  by design and is bound by the ceiling negation instead.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` →
  `rebalance::reconcile` → `plan_evacuations` → `metadata::resolve_chunk_map` → `evacuate_chunk`
  → `metadata::repoint_chunk`. The doubles only store bytes, check preconditions, and inject one
  write or fault at a named seam. Leg 12's supersede goes through the real resolver's restart.
  The test names no symbol this patch adds.
- **(c) Fixture includes the fault?** Yes. The draining server is in the fleet and holds the
  real fragment. Segmented objects are raw `seg:` records plus a segmented root, the shape the
  base refused. Leg 2 really shares one `ChunkId`. Leg 12's supersede really lands between the
  page read and the root re-read (asserted: the hook fired), and R2 really names a different
  group, so the resolver must restart. Legs 4, 5, 7, 8 and 9 assert their injected race or fault
  fired.

## Prior art

`git log origin/main -- crates/custodian/src/rebalance.rs`: `c40948a`, `9470de5`, `d2609b2`
(#710), `3829097` (#696, the containment this completes), `3e05891`.
`segmented_map_rebalance.rs`: `3829097`, `9470de5`. `crates/dst/tests/custodian.rs`: Tier-0
properties, latest `fc7eecc`; no drain repoint property exists. `segmented_map_evacuate.rs`: no
history. #721's commits `842f48e` and `659ed31` are ancestors of `origin/main`. Closed PR #647
and the iteration-1 attempt are covered by the brief; nothing here reintroduces `resolve.rs`.

## Things for the human at sign-off

1. **Leg 3 V/2 deviation** (above) — already in `deferred-findings.json`.
2. **C4-verify count** is 12 ran / 11 failing on the base, not 4 / 3. Legs 1 and 2 remain the
   discriminating evidence.
3. **Docs currency.** No port, API operation, RPC, CLI flag or persisted field changes.
   `docs/design/architecture/08-crosscutting-concepts.md:89` names only the reconstruction pass
   as a caller of `repoint_chunk`; still true, now incomplete. A fifth file breaks the budget,
   so I left it. Suggested follow-up sentence: "The rebalance pass moves a draining server's
   fragment the same way, adding the orphan marks to the same batch."
4. **Stale deferral in the peer.** `reconstruction.rs:1166-1167` says "deferred: #682 — the
   seeded Tier-0 DST property for this move (repoint versus supersede)". Property 19 is that
   property, through the drain caller. `reconstruction.rs` is #721's file; I did not edit it.
5. **Telemetry removed.** The `rebalance_refused_records` counter and the `refused-segmented`
   audit action go away with the refusal. `git grep` finds no doc, dashboard or alert using
   them.
6. **Inherited leak (getwyrd/wyrd#723), not new.** A lost CAS at commit leaves the copied
   destination fragment unreferenced and unmarked. The flat path already does this; this patch
   extends it to `seg:` records, as the brief decided. Leg 4 asserts the copy is left in place,
   not retracted. Prepare-time losses (leg 9) copy nothing.
7. **Behaviour shifts on the flat path**, both from routing through the primitive: a flat
   record at `version == u64::MAX` is now contained and named (the base panicked in debug /
   wrapped in release on `version + 1`, base `rebalance.rs:525`); a flat map whose lengths
   overflow `u64` is contained and named instead of having its chunk moved by index.
