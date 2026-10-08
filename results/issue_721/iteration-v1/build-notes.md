# Build notes — #721 · segmented-repair-completes-through-repoint

*(Withheld from the reviewer; written for the human at sign-off.)*

Everything below is against the cycle worktree `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l1`, `origin/main` @ `a801997`; the brief was authored at
`92e1b4b`, and every citation it makes still resolves — `metadata.rs:352/380/2493/2776` are
unchanged, `reconstruction.rs` moved only by the #638 `put_fragment(..., deadline)` signature).

## 1. What the change is

Four files, exactly the brief's set.

| file | what |
| --- | --- |
| `crates/core/src/metadata.rs` | the placement move: `Repoint`, `repoint_chunk`, `covers`, `chunk_at`, `segment_value_ceiling_crossed`, plus in-crate `#[cfg(test)]` tests for the addressing helpers and the segment budget |
| `crates/custodian/src/reconstruction.rs` | the repair pass completes through it: `Site`/`Object` lose the flat-only shape, `Assessment::Refused` / `Reading::refused` / `emit_refused` are gone, `repair_chunk` prepares the move instead of hand-rolling an inode CAS |
| `crates/custodian/tests/segmented_map_repoint.rs` | **new**, the discriminator (5 legs) |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | the forced edit: that file's own leg 2 asserted the refusal this child removes |

Budget: **156** added semantic lines of non-test production code (215 counted over both
production files minus the 59 in the new `#[cfg(test)]` block) against a cap of 250;
`patch.diff` is **94 957 bytes** against a cap of 95 KB. No fifth file, so no split is owed.

### The primitive, and what it pins

`repoint_chunk(store, inode, generation, byte_offset, prior, placement) -> Repoint`
(`crates/core/src/metadata.rs:2742-2959` in the patched tree). It **hands the batch back**;
the caller adds the obligation delete and the orphan marks and commits once
(`reconstruction.rs:898-935`).

It pins **three** things, and the docs say exactly that (the archived attempt's three doc
sites claiming it pins "the exact bytes the resolve read" were corrected, per the brief —
`ResolvedChunkMap` carries only `record` + the flattened `chunks`, so per-segment bytes are
not available to pin and the claim was never implementable):

1. the **root generation's** bytes (a supersede flips the root first, `0016:2452-2462`);
2. the **segment record's own freshly-read bytes** — the `get` this call makes, not the
   resolve's `scan_page`;
3. the **`ChunkRef` itself** — the chunk moved begins at `byte_offset` **and equals** `prior`.

(2)+(3) together are what makes a sibling's concurrent move **merge** and the planned chunk's
own move **conflict**. That is the whole design decision, and legs 2 and 3 pin the two halves.

### The ceiling: V/2 for a segment record

As decided at Plan (not re-derived here): the flat arm keeps #710's `flat_value_ceiling_crossed`
(V) untouched; the segmented arm weighs the re-encoded **segment** record against
`MAX_ROOT_VALUE_BYTES` (V/2) through a new *named* helper over the **existing** constant — a
second name, not a second value. `metadata.rs:384-407` records both the evidence for (a
conforming publication never writes a `seg:` value above V/2, `0016:1462-1467`) and the
evidence against (the resolver refuses a stored row only above V, `:2493`) and states why the
latter is a containment bound rather than a licence for a maintenance write.

## 2. Salvage, and what was dropped

The brief points at `results/issue_711/iteration-v1/patch.diff`. Its `metadata.rs` primitive
and its `reconstruction.rs` caller were reused as the starting shape and then changed:

* every doc site claiming the move pins "the exact bytes the resolve read" rewritten (module
  doc, `Repoint::Prepared`, `repoint_chunk`'s `# What it pins`, `repair_chunk`'s comment);
* the segmented arm's ceiling moved from `flat_value_ceiling_crossed` (V) to the new V/2 helper;
* `rebalance.rs`, `segmented_map_rebalance.rs` and `crates/dst/tests/custodian.rs` dropped
  entirely (#722's);
* legs 2–5 and the in-crate unit tests added (the archived attempt had only the leg-1 shape,
  and its DST double applied the racing batch *inside* the repoint's own `commit()` — strictly
  after the primitive's read, so it could not reach the window at all; that shape is not
  reproduced here).

## 3. Reaching the read→prepare window (the gap that sank the parent)

The two reads are on **different `MetadataStore` methods**: the resolver reads the group's
range with `scan_page` (`read_group_range`, `metadata.rs:2452-2461`), and the move's `get` is
the only `get` anyone performs on a `seg:` key. The double therefore fires the racing batch
**after answering the `seg:` page** (`segmented_map_repoint.rs`, `MemMeta::scan_page`), which
is after the plan's read and before the move's. Counting `get`s would have landed the race
*after* the move captured its CAS bytes and quietly inverted leg 2 (the sibling edit would
have conflicted instead of merging). Leg 4's root flip fires on the way **into** `commit`, so
the resolve has completed and the pass does not simply restart onto the new generation.

Both windows carry a fixture self-check (`meta.raced()`), so a leg cannot pass because the
race silently never happened — except leg 4, where the base never reaches a commit at all
(see §5).

## 4. Named negations — demonstrated, not asserted

Each run is `cargo test -p wyrd-custodian --test segmented_map_repoint` with the single edit
applied to the patched tree, then reverted. All three were run; the production file was
restored from a scratch copy after each.

**(a) Delete the `chunk == prior` equality from `chunk_at` → leg 3 goes RED.**

```
test a_racing_move_of_the_planned_chunk_itself_is_a_conflict_that_writes_nothing ... FAILED
assertion `left == right` failed: the record still holds EXACTLY the competing writer's
placement, byte for byte ...
  left:  ... "id":41472, ... "placement":[0,2] ...      <- the repair's, silently reverting
 right:  ... "id":41472, ... "placement":[0,7] ...      <- the competing writer's
test result: FAILED. 4 passed; 1 failed
```

This is exactly the defect the adversary reproduced on the parent: matched on byte offset
alone, the chunk is rewritten onto freshly-read bytes and the other writer's placement is lost.

**(b) Delete the root-generation `require` from the segmented arm → leg 4 goes RED.**

```
test a_superseded_root_generation_makes_the_repair_lose_without_writing_its_own_metadata ... FAILED
assertion `left == right` failed: the placement did not move ...
  left:  ... "placement":[0,2] ...    <- repointed into a retired generation
 right:  ... "placement":[0,1] ...
test result: FAILED. 4 passed; 1 failed
```

**(c) Widen the segment budget from V/2 to the full value ceiling → leg 5 goes RED.**

```
test a_repoint_that_would_outgrow_a_segment_records_budget_is_refused ... FAILED
50019 bytes stored, past the budget a publication writes a segment record under
test result: FAILED. 4 passed; 1 failed
```

50 019 = the seeded 50 000 plus the 19 bytes a one-digit id re-encoded as `u64::MAX` costs —
i.e. the fixture really is at the boundary and the refusal really is the V/2 one. (This
mutant also reddens the in-crate unit test `the_segment_ceiling_is_the_publication_budget_…`.)

## 5. Two phrasings that are deliberate, and worth your eye at sign-off

**Legs 3 and 4 assert `outcome != Reconciled::Changed`, not "answers `Blocked`".** The brief
words both as "the pass does not certify". Post-fix a lost race answers `Satisfied`: `hole` is
`reading.incomplete || ceiling_refused`, and a conflict is neither — that is the **base's own,
unchanged** semantics for a flat lost CAS (`reconstruction.rs:314-323`, `:348-350`), and
changing it would be a behaviour change to the flat path this slice has no mandate for.
Pre-fix the same legs answer `Blocked` (the refusal). So the assertion that is both binding and
true in both worlds is "it never reports a change it did not make". Leg 5, where the refusal
*does* open a hole, asserts `Blocked` outright — in both worlds.

**Leg 4 is vacuous on the base** and cannot be made otherwise: pre-fix the pass never commits,
so the armed root flip never fires and `meta.raced()` cannot be asserted. Post-fix the leg only
passes *because* the flip fired (if it did not, the repair would land and the leg would fail),
and negation (b) shows it is binding. Flagged here rather than papered over.

## 6. The forced edit, and one fixture change inside it

`segmented_map_reconstruction.rs`'s leg 2 asserted the refusal (`:484` on the base) and now
asserts the repair lands, including that **both** obligations inside the one segmented object
land in the *same* pass — they sit in different `seg:` records and share only the root, which a
segmented repoint never rewrites.

One extra change in that file that is *not* cosmetic: its `Seed::Segmented` arm never wrote the
surviving fragments, because on the base a segmented chunk was refused before any fragment was
fetched. With the refusal gone those chunks assess for real and would have been classified
`Unrepairable` (data loss) — a fixture that silently stopped exercising what the leg claims. The
`survivors` helper now seeds fragment 0 for both shapes, exactly as the flat arm always did.
`MemMeta::records()` went with the refusal assertion that was its only caller.

## 7. Scope held / deliberately not done

* **No DST leg.** The rubric asks a new concurrent path to land with seeded Tier-0 DST
  coverage; the brief assigns the repoint-versus-supersede DST property to **#722** and forbids
  touching `crates/dst/tests/custodian.rs`, `rebalance.rs` and `segmented_map_rebalance.rs`. I
  ran `cargo xtask dst` anyway — green, nothing there asserted the old refusal.
* **No pre-mark, no drain fence** (0016 X47): the two remaining preconditions are out of scope,
  and the stranded-destination-fragment leak they close is the pre-existing, tracked
  getwyrd/wyrd#723. Not re-argued here, and the two "collectable garbage" comments #723 owns
  were left alone. One consequence worth noting: because the primitive prepares the batch
  **before** the fragment writes, a leg-3/leg-4 conflict here writes no fragment either — this
  slice's conflict path is strictly quieter than the flat one's, never louder.
* **No docs change.** No port, API operation, RPC, CLI flag or persisted field moves; a repoint
  writes the same `inode:`/`seg:` shapes. The two closest peers — #697 (`1f871ce`) and #710
  (`d2609b2`, which likewise added a public helper to `core::metadata`) — touched no docs
  either, and `docs/design/architecture/08-crosscutting-concepts.md:85` makes no claim this
  change falsifies.
* `commit_chunk_map` and its segmented refusal (`metadata.rs:1801-1805`) untouched, as the
  brief requires; the flat arm reuses its CAS idiom (`version + 1`, `..prior.clone()`), so
  ADR-0047 object metadata is preserved.
* `cargo doc` reports two new `private_intra_doc_links` (public docs naming
  `segment_value_ceiling_crossed`). It is **not** a gate here and the base already carries
  ~15 of the same class in this file (`InodeRecordWire`, `live_lease_guards`, `parse_inode_key`,
  …), so the patch follows the file's existing convention rather than inventing an exception.

## 8. Alternatives ruled out (with their cost)

* **Pin the whole resolved segment record's bytes** (the simplest "CAS on what I read"). Not
  implementable — `ResolvedChunkMap` (`metadata.rs:2294-2300`) has no per-segment bytes — and,
  if faked by re-reading and pinning at resolve time, it makes leg 2 red: two repairs inside one
  multipart object serialise on the record, so a busy object's second obligation loses its CAS
  every pass. Rejected on behaviour, not size.
* **Address the chunk by its index in the resolved list** (what the base's `chunk_index` did).
  Costs nothing in lines and is wrong across a segment boundary: the index into the flattened
  list is not the index into the segment record. Byte offset + equality is the same two fields
  the plan already carries.
* **Let the caller commit the placement, then the evidence.** Two commits instead of one:
  +1 `commit` call and a window in which a fragment has moved and nothing records where it
  went — ADR-0015 / `0005:277` forbid it. The hand-the-batch-back shape costs 1 line at the
  callsite (`batch = batch.delete(...)`).
* **Reuse `flat_value_ceiling_crossed` for the segment arm** (the archived attempt's choice):
  0 added lines, and it writes records in the 50 001..100 000 band that no publication could
  produce and no re-publication reproduce. The new helper is **6 lines** of code (the rest is
  the decision record) over the existing constant.
* **Widen `commit_chunk_map` to handle segments** instead of adding a primitive: it is the
  *publication* commit point, refuses a segmented prior deliberately (`metadata.rs:1794-1805`),
  and would need the orphan/state semantics a repair must not have. Explicitly out of scope.

## 9. Refutation of my own test (the three forced questions)

**(a) Genuine red?** Yes — actually reverted and re-run, twice. By hand
(`git checkout -- crates/core/src/metadata.rs crates/custodian/src/reconstruction.rs`, test
kept) and then through the project's own gate:

```
$ PDCA_BUNDLE=results/issue_721 ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_repoint (fix applied)
  test result: ok. 5 passed; 0 failed
run-verify.sh: RED — ... (production reverted, test kept)
  a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record --- FAILED (Blocked, want Changed)
  a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged   --- FAILED (Blocked, want Changed)
  test result: FAILED. 3 passed; 2 failed
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

Read the summary line as the brief says: **5 tests ran** in the red leg, **2 failed**. Legs 3,
4 and 5 pass pre-fix by construction and are **not** C4-verify evidence — they are bound by the
three negations in §4.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point — over `ReconstructionContext`, and asserts against the **store**. No
symbol this patch introduces is named anywhere in the test (that is also what keeps the red leg
a red rather than a compile error / exit 77); the primitive is reached only because production
calls it. Fragments are real shards through `erasure::encode` + `encode_ec_fragment`, so the
loop's identity and checksum verify actually pass and the rebuild is genuine.

**(c) Fixture includes the fault?** Yes. The object is a **committed segmented** one — raw
`seg:` records plus a segmented root, hand-written because this build ships no producer — and
`seed` asserts it genuinely resolves before any leg runs. The repaired chunk sits in the
**second** segment at a non-zero object offset, so the addressing really crosses a boundary the
resolved list hides; the lost D server (`LOST = 1`) is in neither the fleet nor the topology, so
the missing fragment is a real loss and not a curated-away one; the racing writer's batch really
lands (`meta.raced()` asserts it for legs 2 and 3); and leg 5's record is padded to *measured*
50 000 bytes rather than a hard-coded chunk count.

## 10. Gates run here

* `./engine/xtask.sh ci` → **all checks passed** (fmt, clippy `-D warnings` on all targets,
  build, workspace tests, deny, machete, typos, docs lint + render, statics/unsafe/gitlink
  guards, conformance).
* `./engine/xtask.sh dst` → green.
* `./engine/scripts/run-verify.sh` → **PASS** (see §9a).
* `cargo fmt --all` clean, so the target's own commit hooks have nothing to reject.
