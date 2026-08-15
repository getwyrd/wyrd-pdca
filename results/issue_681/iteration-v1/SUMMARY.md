# Result — issue 681 / passes-read-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The three maintenance passes that walk the committed namespace **themselves** still
  read the chunk map out of the inode record inline, so a **single** segmented object aborts the
  whole pass — and an object whose map cannot be read for any other reason ends the walk with an
  `Err` rather than being contained. Seven sites, each re-verified on `origin/main` at `339da46`
  with its enclosing function:

  | Site | Function | What it ends |
  |---|---|---|
  | `crates/custodian/src/reconstruction.rs:332` | `assess` | one obligation's assessment, and the pass (`?`) |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | the binding repair commit |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` | the whole `inode:` scan, for every obligation |
  | `crates/custodian/src/backfill.rs:99` | `reconcile` | the fill scan |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` | the remaining-placement gauge |
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` | the evacuation scan |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` | the binding evacuation commit |

  Each is `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`, and
  each `?`s out of a function whose error ends the pass. Consequences, all live on the base:
  1. **One segmented object disables repair, backfill and drain for the entire store.** Not for
     that object — for every object, because the error propagates out of the `scan(b"inode:")`
     loop. The three passes are the ones that *restore redundancy*; a store that has published a
     single multipart object therefore stops self-healing.
  2. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as *"referenced by no committed chunk
     map"* → `Assessment::Drain` (`reconstruction.rs:322-325`, returning at `:325`), and the obligation is deleted in
     the drain batch (`:271-277`). Today `find_chunk` errors before it can return `None`, so this
     is latent — but any implementation that makes the walk *skip* a segmented object instead of
     containing it silently discards the repair obligation for live, under-replicated data. That
     is the loss this slice must not introduce while removing the abort.
  3. **The deployed repair loop is Q namespace scans × N point reads.** `reconcile` calls `assess`
     once per queued obligation (`reconstruction.rs:185`), and `assess` calls `find_chunk`
     (`:322`), which scans the entire `inode:` namespace and decodes every record (`:620-644`).
     With the resolver wired in, each of those N objects also costs a bounded `seg:` range read —
     so the pass would go from Q×N decodes to Q×N *resolves*. This is the finding left open at
     #647's close and it is in scope here.
  4. **Containment is not per object.** #650 and #651 contain a damaged record in GC/scrub and in
     restore/drain (`gc.rs:360-450`, `restore.rs:621-688`); these three passes have no such rule.
     A record that will not `decode` ends the walk at `reconstruction.rs:625`, `backfill.rs:80`,
     `backfill.rs:174`, `rebalance.rs:148` — before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_passes.rs` passes,
  driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step, Custodian,
  FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::backfill::{reconcile, BackfillContext}`,
  `wyrd_custodian::desired_state::set_lifecycle`, `wyrd_core::repair::{enqueue_repair, repair_key,
  queued_repairs}`, `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}` — over in-memory
  `MetadataStore` / `ChunkStore` doubles. Five legs, each binding:
  1. **A segmented object no longer ends any of the three passes.** Over a store holding one
     healthy segmented object (raw `seg:` records + a segmented root, **never** a committer) beside
     one healthy flat object: `reconcile_step` with a `ReconstructionContext` **and** a
     `RebalanceContext` returns `Ok`, and `backfill::reconcile` returns `Ok` — today all three
     return `Err`. Assert **positively**, not merely "no error": the flat object's genuine work
     still happens in the same pass — a queued repair for an under-replicated **flat** chunk is
     assessed and repaired (its placement moved, its obligation drained), and a **fillable flat**
     record with an empty `placement` is still filled.
  2. **A repair obligation for a chunk in a `seg:` record is NOT drained.** Enqueue a repair for a
     chunk that lives in a segmented record, run the pass, and assert `queued_repairs` still
     contains that chunk id and the `seg:` record's bytes are **unchanged**. Then assert the pass
     did **not** certify: `!= Reconciled::Satisfied`. (Spell the negative as
     `!= Satisfied && != Changed` where the pass converged nothing, so the leg cannot be scored by
     a variant this patch introduces — `Reconciled::Blocked` **does** exist on the base
     (`reconciliation.rs:44`, #650), so naming it positively is also legal here; prefer naming it.)
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed a
     committed root whose map genuinely fails to resolve (a `SegmentRef` the root names whose `seg:`
     record was never written — assert in the fixture that `resolve_chunk_map` really errors, as
     `segmented_map_restore.rs:415-431` does) **first** in key order, beside a healthy flat object
     carrying real work. Assert the conjunction: each pass returns `Ok`, answers `Blocked` (never
     `Satisfied`), the healthy object's queued repair is still assessed **and** its fill still
     happens, and the unreadable object is **named** on the audit seam by its `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470`). A record whose own bytes will not `decode`
     is contained by the same rule.
  4. **A store fault under the resolve still ends the pass.** A metadata double that fails a `get`
     with a non-`ChunkMapError` error makes the pass return `Err` — a walk that cannot reach the
     store has no answer for any object, not one unreadable object. This is the other half of (3)
     and it is what the downcast rule at `gc.rs:402-416` exists for.
  5. **Reconstruction resolves each object's root once per pass — O(N), not O(Q×N).** Drive the
     pass with **Q ≥ 3** queued obligations over **N ≥ 3** committed objects on a *counted*
     instrumented `MetadataStore` double, and assert the number of `inode:` scans is **1** and the
     number of `seg:` range reads is **≤ N** (one per segmented object), independent of Q. Assert
     the count, not a timing. The same store must still produce the correct repairs, so the leg
     cannot be satisfied by a pass that stopped doing the work. **Scope the count to the
     reconstruction pass alone** — build the `ReconstructionContext` leg with no `GcContext` /
     `ScrubContext` / `RebalanceContext` beside it. The other loops each walk `inode:` themselves
     today and sharing one walk across passes is a much larger refactor that is **not** in scope;
     asserting a store-wide scan count would demand it by the back door.

  Legs (1), (2), (3) and (5) are the binding ones — each fails on the base for a *behavioural*
  reason, not a missing symbol.

  **Not in the discriminator, covered by C4-ci:** positive matches on any variant or field this
  patch introduces, and the per-pass regressions in `crates/custodian/tests/{reconstruction,
  backfill,rebalance}.rs`.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches and M4's integration branch is merged and deleted. #648/#649/#650/#651/#652
  all landed on `main` directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`. Not
  `pdca-integration/main` — that is the driver's run-scoped fold branch, and `wave_mode = "merge"`
  means a dependent builds on a genuinely merged `origin/main` anyway.)
- Scope (one logical fix) / out of scope: make the three passes that scan `inode:` **read every committed object through the
  shared resolver, contain what they cannot read, and refuse rather than abort** the writes this
  slice does not own.
  - `crates/custodian/src/reconstruction.rs` — the pass reads each committed object's chunk map
    **once per pass** and assesses its queued obligations against that reading, instead of
    re-reading the namespace per obligation. A chunk whose `ChunkRef` lives in a `seg:` record is
    **refused, not drained**: the obligation stays queued and the pass does not certify. A chunk in
    a flat record is repaired exactly as today.
  - `crates/custodian/src/backfill.rs` — a segmented record is left **byte-identical**, declined
    with a stated reason on the audit seam and counted on a gauge, while a fillable **flat** record
    in the same store is still filled in the same pass. The remaining-placement gauge stays
    correct over a store containing segmented objects.
  - `crates/custodian/src/rebalance.rs` — the evacuation scan resolves per object; a fragment whose
    chunk lives in a `seg:` record is **refused and stays on the draining server**, and the pass
    does not report the drain satisfied. A flat chunk is evacuated exactly as today.
  - All three: a record that will not `decode`, and a generation the resolver cannot read, are
    **contained per object** — named on the audit seam, the walk continues, the pass answers
    non-certifying. A fault that is not a `ChunkMapError` still propagates.

  **Constraints carried forward (blockers found on the old #651 — must not recur; these bound the
  shape, they do not name it):**
  - **Bounded memory.** A pass may retain work proportional to the **obligations it holds** and to
    **one object at a time** — never the whole namespace's decoded chunk list, never any segment's
    exact bytes, and never a per-chunk deep copy of a segmented root (that is O(chunks × segments)).
    This slice writes no segmented record, so it needs to pin **nothing**; pinning exact bytes is
    #682's.
  - **Bounded work.** Backfill's remaining-placement gauge must not cost a **second resolving walk**
    of the namespace: on a store of segmented objects that doubles every `seg:` range read for a
    number the pass has already seen. One resolving reading per pass.
  - **Containment on *any* read fault, not just a segmented shape.** A serde decode failure on a
    concurrently-replaced root must contain that object, not abort the walk.
  - **A duplicate committed chunk id is ambiguous**, repaired by neither reference, and **both**
    objects are named. Do **not** rebuild the cross-object claim-counting apparatus that was
    dropped at #651's replan — this is the narrow rule only: if the one reading finds two committed
    references to the same `ChunkId`, neither is repaired and both keys are named on the audit
    seam. No new report schema, no `ambiguous-*` verdict surface, no mark-withholding keyed on it.

  **Out of scope:**
  - **Any write to a segmented record.** `repoint_chunk`, the record ceilings, and the
    repair/evacuation write path for a `seg:`-resident chunk are **#682**. A refusal in this slice
    writes **nothing at all**.
  - Restore and `desired_state` (**#651**, merged): `restore.rs` and `desired_state.rs` are **not**
    edited here. The marker comment at `restore.rs:616` names this slice; leave it, or update only
    its wording if the shared walk genuinely subsumes `committed_chunks` — do **not** refactor
    restore into it.
  - `gc.rs` / `scrub.rs` (**#650**, merged) — untouched.
  - The chunk-id floor (**#652**, merged); the committer, fence, rollback and resume (**#653**).
  - The **pre-existing** question of whether an ordinary `EvacOutcome::Aborted` (no free domain, a
    missing fragment) should certify — that is listed in #682's carried-forward constraints and is
    its to settle. This slice makes only the refusal **it introduces** non-certifying.
  - Any new or edited ADR / spec / proposal; any conformance-vector change; any new CLI subcommand
    or report-schema change.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 60 mutants tested in 67s: 18 missed, 16 caught, 26 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make reconstruction, backfill, and rebalance resolve segmented chunk maps per object, contain unreadable records, and keep unsupported writes non-certifying.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable against base-visible resolver and non-certifying vocabulary, so containment, propagation, refusal, and O(N) work have observable outcomes (`crates/core/src/metadata.rs:2619`; `crates/custodian/src/reconciliation.rs:44`). |
| C2 Reproduction (red pre-fix) | PASS | The added discriminator compiled against the base and all five tests failed on behavioral assertions, establishing the whole-pass abort, obligation-drain risk, store-fault distinction, and Q scans before the fix (`crates/custodian/tests/segmented_map_passes.rs:529`). |
| C3 Change | PASS | The change addresses the specified read sites through the shared resolver while preserving non-`ChunkMapError` propagation, so the human need not choose between divergent per-pass read semantics (`crates/custodian/src/backfill.rs:115`; `crates/custodian/src/rebalance.rs:211`; `crates/custodian/src/reconstruction.rs:824`). |
| C4 Verification (red→green) | PASS | The five-test discriminator was red on reverted production and green patched; all CI components also passed with the real tools, including `cargo-deny` under a writable scratch cache and the DST seed sweep (`crates/custodian/tests/segmented_map_passes.rs:529`). |
| C5 Causal adequacy | PASS | The Q×N cause is removed by one obligation-indexed namespace reading and resolver use rather than a capability probe or symptom guard, preserving per-object containment without retaining segmented records for write (`crates/custodian/src/reconstruction.rs:156`; `crates/custodian/src/reconstruction.rs:795`). |
| T1 Structure | PASS | The four touched files stay inside the allocated custodian surface, reuse the shared metadata resolver and existing `Reconciled::Blocked`, and add no parallel resolver or dependency (`crates/custodian/tests/segmented_map_passes.rs:1`). |
| T2 Shape | PASS | The patch remains within the four-of-eight file allocation and approximately 843 non-comment, non-mechanical added lines, leaving the brief's size and scope boundaries intact (`crates/custodian/tests/segmented_map_passes.rs:38`). |
| T3 Runtime | PASS | Direct execution proves healthy flat work continues beside segmented and unreadable objects, while the counted reconstruction leg performs one inode scan and at most one segment-range read per segmented object (`crates/custodian/tests/segmented_map_passes.rs:703`; `crates/custodian/tests/segmented_map_passes.rs:973`). |
| T4 Contribution | NEEDS-HUMAN | The owner must reconcile the five reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently triaged; affected-path history and closed PR #647 were independently checked. |
| T5 Judgment | NEEDS-HUMAN [impl] | Decide whether the no-data-loss claim has adequate regression protection — all three mutations of the incomplete-reading drain guard survive, and no test directly drives segmented backfill refusal or segmented evacuation refusal, so a rebuild can add the missing cases (`crates/custodian/src/reconstruction.rs:430`; `crates/custodian/src/backfill.rs:159`; `crates/custodian/src/rebalance.rs:276`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether non-certifying refusal until #682 is operationally fit — in-memory red→green and DST evidence establish safety mechanics but not production cadence, audit usability, or tolerance for blocked segmented writes (`crates/custodian/src/reconstruction.rs:327`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — The owner must reconcile the five reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently triaged; affected-path history and closed PR #647 were independently checked.
- [ ] T5 Judgment — Decide whether the no-data-loss claim has adequate regression protection — all three mutations of the incomplete-reading drain guard survive, and no test directly drives segmented backfill refusal or segmented evacuation refusal, so a rebuild can add the missing cases (`crates/custodian/src/reconstruction.rs:430`; `crates/custodian/src/backfill.rs:159`; `crates/custodian/src/rebalance.rs:276`).
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether non-certifying refusal until #682 is operationally fit — in-memory red→green and DST evidence establish safety mechanics but not production cadence, audit usability, or tolerance for blocked segmented writes (`crates/custodian/src/reconstruction.rs:327`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — The owner must reconcile the five reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently triaged; affected-path history and closed PR #647 were independently checked.; T5 Judgment — Decide whether the no-data-loss claim has adequate regression protection — all three mutations of the incomplete-reading drain guard survive, and no test directly drives segmented backfill refusal or segmented evacuation refusal, so a rebuild can add the missing cases (`crates/custodian/src/reconstruction.rs:430`; `crates/custodian/src/backfill.rs:159`; `crates/custodian/src/rebalance.rs:276`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b.
- By / date: auto-iterate / 2026-08-05

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
