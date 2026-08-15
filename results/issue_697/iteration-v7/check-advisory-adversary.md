# Adversarial review — issue #697 (advisory, non-gating)

Toolchain was available (`cargo 1.96.0`); the red→green was re-run end to end in a scratch
copy of `$PDCA_TARGET`, and `cargo mutants --in-diff` was reproduced. No verdict below is
provisional for want of tools.

## What I could confirm (so the refutations below are not noise)

- **Red→green is real, and on the production path.** Base `339da46`'s
  `crates/custodian/src/reconstruction.rs` + the new test → **6 of 7 legs fail
  behaviourally** (`SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }`,
  and leg 7's `left: ([[0,2],[0,1],[0,1]], 2)`); patched → **7/7 green**. No compile
  break, so the red is behavioural, not the exit-77 degradation the brief warned about.
  Every leg drives `reconcile_step` (`crates/custodian/tests/segmented_map_reconstruction.rs:1042`),
  the real fenced entry, over the real `reconstruction::reconcile` — not a re-implementation.
- `cargo test -p wyrd-custodian` (all 16 binaries) and `-p wyrd-server --test custodian_day_one`
  are green with the patch; `crates/custodian/tests/reconstruction.rs` is untouched and still 15/15.
- `cargo mutants --in-diff` reproduces exactly: **20 mutants, 13 caught, 7 unviable, 0 missed**.
  A wider run over `reconstruction.rs`'s touched functions surfaces 9 missed mutants, but all
  of them sit on lines the diff never touched (`:213`, `:225`, `:231`, `:693`, `:711`, `:857`)
  — pre-existing debt, not this patch's.
- The containment core (`reconstruction.rs:472-511`) is byte-for-byte the `gc.rs:365-416` /
  `restore.rs:625-657` shape, decode-before-`state`, `Ok(None)` skipped, `ChunkMapError`
  downcast contained and everything else propagated. The emitters
  (`reconstruction.rs:1029-1069`) mirror `gc.rs:562-571` / `restore.rs:826-835` field for field.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:498-535` (leg 2)
  does not bind "a refusal blocks nothing but itself", and I broke the fix without turning a
  single test red.** Insert, immediately above `crates/custodian/src/reconstruction.rs:294`
  (`let mut changed = false;`), the four lines
  `if !reading.refused.is_empty() { return Ok(Reconciled::Blocked); }` — i.e. "a refusal means
  the pass cannot certify, so don't bother writing". All **7 legs stay green**, the whole
  `cargo test -p wyrd-custodian` suite stays green (89 tests), and `-p wyrd-server --test
  custodian_day_one` stays green (15 tests). That tree stops **every flat repair and every
  drain store-wide** the moment one multipart object owes a chunk — the exact defect this slice
  exists to remove, wearing a different hat. The hole is a fixture-compression artefact: leg 1
  (`:465`) has flat work but no refusal, leg 2 (`:498`) has a refusal but *only* the segmented
  object beside it, so nothing in the bundle ever puts the two in one pass. The brief's own
  §Scope answer rule 1 claims this property is "Bound by legs 1 and 2" — it is not.
  Cheap fix: add `seed(&meta, &d0, 2, UnderReplicated { chunks: &FLAT[..1] })` +
  `enqueue(&meta, &[FLAT[0], DELETED])` to leg 2 and assert the flat repoint landed and
  `DELETED` was drained, exactly as leg 3 does for the *incomplete* case.

- **NEEDS-HUMAN [human] — the test file is 59% over the brief's hard raw ceiling and 83% over
  its semantic one, and the brief pre-declared that as a stop condition, not a stretch.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:1` — **731 raw lines** against a
  budget of **≤ 460 raw / ≤ 280 semantic** (513 semantic by a non-blank/non-comment count).
  brief.md §Budget: *"a test file past 460 raw means the shape is wrong: **STOP and hand back
  rather than finish**."* The production side is 161 added semantic lines against ≤ 160 — that
  one is counting-method noise and I would not raise it alone; the test file is not. This is
  the same over-run signature the brief's own §Ordering note says got rounds 1–5 rejected, and
  it is not something a reviewer may wave through as "the legs needed it": seven legs shipped
  where six were briefed (see next bullet), which accounts for part of the overrun.

- **NEEDS-HUMAN [human] — the intra-pass generation chaining at
  `crates/custodian/src/reconstruction.rs:308-316` and `:893` is unbriefed scope, and it
  falsifies the brief's stated basis for shipping no seeded Tier-0 DST leg.** The patch keeps a
  `HashMap<usize, InodeRecord>` of records *this pass wrote* and conditions each object's second
  and later repairs on them (`:310` `repaired.get(&plan.object).unwrap_or(plan.prior)`; `:893`
  `.require(inode_key.clone(), metadata::encode(prior))`). Three problems, none of which the
  builder can settle alone:
  1. brief.md §Verification posture justifies "no seeded Tier-0 DST case ships in this child, and
     none is owed" with *"Every write it performs is on a flat record resolved by borrow from the
     generation the scan returned … committed under the base's own **unmodified**
     version-conditional CAS (`:598-608`)"*. After this patch that is no longer true for writes
     2..Q inside one object: the CAS precondition is a record synthesized in memory, never read
     back from the store. The rubric's *Test fidelity* class ("a new destructive or concurrent
     path lands with seeded Tier-0 DST coverage") now has a surface it did not have when the
     posture was written.
  2. brief.md §Scope, the #698 carve-out, pins `repair_chunk:598-601` to stay **byte-identical**
     and `RepairPlan` to keep its base field `prior: InodeRecord`. Base `:600` was
     `.require(inode_key.clone(), metadata::encode(&plan.prior))`; the patch changed it, and
     `RepairPlan.prior` became `&'a InodeRecord` (`reconstruction.rs:125`). Not #698's fix, but
     not the frozen line either.
  3. Leg 7 (`:699`) exists only to bind this mechanism, and its base red
     (`([[0,2],[0,1],[0,1]], 2)` — I reproduced it) demonstrates a *pre-existing, unbriefed*
     defect: on `origin/main` a pass repairs only the first obligation per object and the rest
     lose their CAS. Real, and worth fixing — but it is not #697's defect, it is not in the
     brief's six legs, and shipping it here means part of this bundle's red→green evidence is
     about a different bug. A human should say whether it lands here or as its own issue.
  I could not turn the chaining into a *wrong* answer: I walked the CAS through same-pass
  supersede, external racer, `Conflict` and `Aborted` first-repair, plans re-ordered by
  `repair_priority`, and two keys parsing to one `InodeId` — every path either commits the
  right bytes or fails closed to `Conflict` with the obligation queued. The objection is scope
  and evidence, not correctness.

## Attempted and could not refute

- **Over-containment.** I could not find an error class the patch swallows that it should
  propagate: the downcast at `reconstruction.rs:504-511` is `gc.rs:402-416` verbatim, and leg 5
  (`:638`) genuinely injects on `scan_page(b"seg:")` — the resolver's read — not on
  `scan(b"inode:")`, so it is a real over-containment guard and not a tautology.
- **Silent drain.** Every route into `drain_only` passes the one `!reading.incomplete` gate at
  `:333`; there is no second drain site to drift. Leg 3 (`:543`) binds it and would go red if
  the gate were dropped.
- **Index/shape confusion.** `resolved.chunks` for a flat snapshot is `Cow::Borrowed` of the
  record's own list (`crates/core/src/metadata.rs:2585`), so `FlatSite::index` addresses the very
  list `repair_chunk` mutates; a segmented snapshot's restart-onto-a-newer-root can never reach a
  write because write-eligibility is read off the **scanned** `record.chunk_map`
  (`reconstruction.rs:507-511`), exactly as §Scope demands.
- **Memory / complexity regression.** `Reading::objects` holds one record per object *owed* a
  repair; base held one full `InodeRecord` clone per *obligation* inside `RepairPlan`, so this is
  strictly less. The per-pass Θ(S) `seg:` resolves for objects nothing is owed on are real but
  explicitly blessed ("≤ S") by the brief.
- **Duplicate `ChunkId` / non-canonical `inode:` keys.** Both reachable only through #700 / #698,
  both carved out, and neither is made worse: first-committed-reference-in-key-order is preserved
  (`:527`) and a key that will not parse is skipped exactly where the base's `find_chunk` skipped
  it.
- **Audit vocabulary drift.** `unresolvable-chunk-map` / `reconstruction_unresolvable_records` and
  `refused-segmented` / `reconstruction_refused_records` match the pinned set exactly; no docs
  table anywhere in the repo enumerates the sibling counters, so no docs-currency debt is owed.

## On the gate rows

- `check-gates.json:48` reports C4-verify as *"red without the fix, green with it (**7** test(s)
  ran red)"*. Only **6** go red on the base; leg 6 (`:667`, empty queue) is green there by
  design and the brief says so. If that string is meant as "7 tests went red" it overstates the
  evidence by one leg; if it means "7 tests ran in the red leg" it is fine. Worth one word from
  whoever owns `run-verify.sh`, not a refutation of the fix.
- `check-gates.json:94` — `T4 batched multi-pass rubric review` is **gating and `fail`** (3
  blocking), and `overall` is `fail`. Nothing in this advisory should be read as clearing that.
