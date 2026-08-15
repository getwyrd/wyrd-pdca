# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Method: rebuilt the workspace in scratch, independently reproduced the red→green
(`git show HEAD:crates/custodian/src/{reconstruction,backfill,rebalance}.rs` over the new
test → **5/5 fail**, behavioural reds, not compile errors; with the patch → **5/5 pass**),
then hand-mutated each production guard the brief calls binding and re-ran the **whole**
`wyrd-custodian` suite to see which guards the suite can actually feel. Scratch removed.

The evidence is real: the discriminator is assertion-red on the base and exercises the
production path (`reconcile_step` / `backfill::reconcile`, in-memory trait doubles, no
parallel re-implementation). What it does **not** do is defend most of the guards the patch
adds. Four of the brief's binding rules can be deleted from the shipped code without a
single test in the crate turning red — verified, not suspected.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:430`: the
  incomplete-reading drain guard is unevidenced; deleting it reintroduces the exact data
  loss the brief exists to prevent, and the suite stays green.** I replaced
  `None if index.unreadable == 0 => Drain` with an unconditional `None => Drain` (dropping
  the `REFUSED_INCOMPLETE` arm entirely) and ran `cargo test -p wyrd-custodian`: **every
  test passed, including all five discriminator legs.** That mutant is the brief's §Defect
  consequence 2 — "a repair obligation ... drained as if the chunk were deleted" — and the
  §Invariant "a repair obligation ... is never discarded for want of a reading". Concrete
  missing case (I wrote it; it passes on the patched tree, so it is a test gap, not a code
  bug): `seed_undecodable(&meta)` + `enqueue_repair(&meta, REPAIR_CHUNK, "scrub")` with
  **no** record referencing `REPAIR_CHUNK` → assert `queued_repairs` still contains it and
  the pass answers `Blocked`. Leg (2) (`segmented_map_passes.rs:634`) only covers the
  *found-and-refused* case; the *not-found-over-a-holed-reading* case — the one that
  reaches the `drain_only` delete batch at `reconstruction.rs:319-325` — is untested.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:163` and
  `crates/custodian/src/rebalance.rs:236`: the two "refuse rather than write a segmented
  record" guards are dead under test; removing them turns both passes into silent
  segmented-root corrupters and nothing goes red.** I rewrote both `writable` matches so a
  segmented record is treated as writable (`(_, Some(inode_id)) => Ok(inode_id)`) and ran
  the full crate suite: **all tests passed.** With that mutation, `backfill.rs:195` and
  `rebalance.rs:388` build `chunk_map: next_chunk_map.into()`, and
  `impl From<Vec<ChunkRef>> for ChunkMap` (`crates/core/src/metadata.rs:1036-1042`) yields
  `ChunkMap::Flat` — i.e. both passes would **replace a segmented root with a flat one**,
  orphaning its `seg:` records: precisely the "**a refusal in this slice writes nothing at
  all**" the brief puts out of scope. The reason the branches are unreachable is fixture
  choice, not implementation: every seeded segmented chunk is
  `single_copy_ref(chunk, dserver)` with a **non-empty** placement
  (`segmented_map_passes.rs:419`) on server `0`, while the draining server in the only
  rebalance-driving leg is `IDLE_DRAINING = 9` (`:320`, `:553`) — so `to_fill` is always
  empty in backfill and `evac` is always empty in rebalance. Two concrete cases that would
  cover them (both pass on the patched tree — again a test defect, not a code defect):
  (a) `seed_segmented(&meta, SEGMENTED_INODE, &[(SEG_QUEUED, EVAC_DRAINING)], 1)` +
  `set_lifecycle(EVAC_DRAINING, Draining)` → assert `meta.snapshot()` unchanged,
  `Reconciled::Blocked`, and `"action":"refused"` on the rebalance seam; (b) a
  `SegmentRecord::new(vec![rs_ref(SEG_QUEUED, vec![])], 0)` → assert byte-identity,
  `Blocked`, and `"action":"declined"` on the backfill seam. The brief allocated
  `crates/custodian/tests/{backfill,rebalance}.rs` for exactly these positive regressions
  ("Not in the discriminator, covered by C4-ci"); neither file is in the diff.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:909`: the
  duplicate-chunk-id ambiguity rule is unevidenced.** I replaced the `Entry::Occupied` merge
  with a plain `slot.insert(site)` — so a chunk claimed by two committed maps is repaired
  against whichever reference the scan met **last**, and neither object is named — and the
  full crate suite passed. The brief lists this as a carried-forward constraint ("**A
  duplicate committed chunk id is ambiguous**, repaired by neither reference, and **both**
  objects are named"). A two-line fixture (two `seed_flat` objects sharing one `ChunkId`,
  one queued repair) would bind it: assert the obligation survives, both placements are
  untouched, and the audit line carries `"reason":"ambiguous-chunk-id"` with both names.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:170` / `:216` / `:256`: the
  remaining-placement gauge over a store with segmented objects is asserted nowhere.** The
  brief's scope says "The remaining-placement gauge stays correct over a store containing
  segmented objects", and the patch changed the gauge from a post-pass namespace re-count
  to an in-walk accumulator. The only gauge assertion in the repo
  (`crates/custodian/tests/backfill_telemetry.rs:207`) asserts **0** after a fully-covering
  flat pass — which the accumulator satisfies trivially — and the new test file never
  mentions the gauge at all. Deleting `remaining += to_fill.len() as u64` at `:170` (the
  declined-fill contribution) or `:216` (the lost-CAS contribution) is therefore invisible,
  and the operator's drain-to-zero signal would read 0 over a store that still holds an
  un-fillable population. Concrete case: one segmented object holding an empty-placement
  seg chunk → the gauge must read 1, not 0.

- **NEEDS-HUMAN [human] — `crates/custodian/src/rebalance.rs:276-283` vs
  `crates/custodian/src/desired_state.rs:225-246`: the refusal this slice introduces does
  not reach the operator-facing drain surface, so the C-1 stall is relabelled rather than
  closed.** Verified end to end: a segmented object whose chunk's fragment sits on a
  `Draining` server, ten `reconcile_step` passes — each returns `Reconciled::Blocked`, the
  fragment never moves, and `wyrd_custodian::reconciliation_status(&meta, server)` answers a
  bare **`Pending`**. That is the answer this repo's own comment at `desired_state.rs:206-214`
  says means "an evacuation is running and will finish", and which it calls the C-1
  permanence "reached through the report instead of through a deletion" when it isn't —
  `referenced_fragments` *can* resolve the segmented map, so `genuinely_holds` short-circuits
  at `:195` before the `PendingUnresolvable` attribution at `:225` is ever reached. The
  `Blocked` the pass does return is the only signal, and the deployed loop discards it
  (`crates/server/src/custodian.rs:531-546` matches on `Ok(_)`). `desired_state.rs` is
  explicitly out of this slice's scope, so this is a scope call for a human: either #682
  carries the attribution (it is not in the #682 constraints the brief quotes — those name
  `EvacOutcome::Aborted`, not the segmented refusal) or a tracking issue is opened. Not a
  regression against the base (where rebalance `Err`ed fleet-wide), but the slice's stated
  invariant "a pass that refused work does not certify" is satisfied only at a value nobody
  in production reads.

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:170-174`: reconstruction
  answers `Satisfied` over unreadable records whenever the repair queue is empty — which is
  the steady state.** Verified: a store holding both `seed_undecodable` and `seed_damaged`
  records with an empty queue → `Reconciled::Satisfied`, `inode_scans() == 0`, and **no**
  `unresolvable-chunk-map` line emitted; backfill and rebalance answer `Blocked` and name
  both records over the identical store. Leg (3) only ever runs the pass with an obligation
  in hand (`segmented_map_passes.rs:721`-ff), so the discriminator cannot see this. The
  builder documented the trade-off in-line and it is defensible (with no obligations the
  pass reads nothing, so nothing is incomplete), but it means the "redundancy is not whole"
  signal switches **off** exactly when the backlog drains, while the damaged record is still
  there — and the brief's invariant reads "saying `Satisfied` there tells an operator ...
  redundancy is whole". A human should decide whether that is the intended contract or
  whether an unreadable record must keep reconstruction non-certifying.

## Attacks that failed (could not refute)

- **The write path reading a different record than the CAS prior.** Tried to construct a
  case where `resolved.record` and the scan's `record` disagree in reconstruction
  (`reconstruction.rs:857`, which tests the *scan* record's shape, while backfill/rebalance
  test `resolved.record`). Cannot happen in the direction that matters: a flat map resolves
  with **no** re-read (`crates/core/src/metadata.rs:2584-2586`), so `Site::Flat` always
  carries the record it was decided on; the only divergence (segmented root superseded by a
  flat one mid-resolve) makes reconstruction *refuse*, which is the conservative side.
- **Two obligations in one object as a new convergence regression** (the second repair's CAS
  losing to the first now that priors are read once per pass). The base assessed **every**
  obligation before any repair too (`git show HEAD:crates/custodian/src/reconstruction.rs`,
  the `for chunk in queue { match assess(...) }` loop preceding the repair loop), so the
  stale prior is pre-existing, not introduced here.
- **A *false* ambiguity from one object referencing a chunk id twice.**
  `chunk_id_minter` (`crates/server/src/cli.rs:1964-1971`) packs the inode in the high 64
  bits and a per-object sequence in the low, so no writer mints a repeat within or across
  objects; the `REFUSED_AMBIGUOUS` merge cannot fire on a well-formed store.
- **Leg (4) as a tautology.** It is not: pre-fix the passes fail the same read with
  `SegmentedMapUnsupported`, and the leg asserts the *injected* fault text
  (`segmented_map_passes.rs:290-292`), so `expect_err` alone would not have earned the red.
- **The docs-currency confirm.** `docs/design/architecture/06-runtime-view.md:29` still
  reads true — `crates/core/src/read.rs:96`, `metadata.rs:1480`, `:1749`, `:1872` still
  refuse a segmented map outright — so the "confirm-only, edit only if false" instruction was
  correctly answered by not editing.
- **Containment-shape conformance.** The decode-then-resolve-then-downcast block in all
  three passes is byte-for-byte the rule at `crates/custodian/src/gc.rs:365-416`, including
  containing a decode failure *before* the `state != Committed` check and propagating a
  non-`ChunkMapError`. No divergence found.
