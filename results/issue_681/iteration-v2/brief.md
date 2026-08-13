# Brief — issue 681 / passes-read-through-resolver-contained

> Slice **4b of 7** of the #635 re-slicing (proposal 0016 decision 7(e)). The **read side only**:
> the three maintenance passes that scan `inode:` themselves — reconstruction, backfill,
> rebalance — stop failing closed on a segmented object and start containing what they cannot
> read. Siblings: **#650** (GC/scrub, merged `11aa85f`), **#651** (restore + drain report, merged
> `8decc93`), **#682** (4c — `repoint_chunk` + the record ceilings, depends on this).
> Tracker: https://github.com/getwyrd/wyrd/issues/681.

- **Slug:** passes-read-through-resolver-contained
- **Defect:** The three maintenance passes that walk the committed namespace **themselves** still
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
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_passes.rs` passes,
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, no DST leg required for the discriminator.
  - **Base.** `PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh --print-base` →
    `origin/main` (run at Plan). No `Onto branch`, no `stack-base` marker in the bundle.
  - **Prerequisites SATISFIED**, verified on the checkout at `339da46` (`main == origin/main`,
    0 commits either way): `metadata::resolve_chunk_map` (`core/src/metadata.rs:2619`),
    `resolve_current_chunk_map` (`:2652`), `ChunkMapError` (`:469`), `Reconciled::Blocked`
    (`custodian/src/reconciliation.rs:44`) and `Reconciled::least_certified` (`:48`),
    `gc::referenced_fragments` (`gc.rs:360`) and `gc::object_name` (`gc.rs:470`),
    `repair::{enqueue_repair,queued_repairs,repair_key}` (`core/src/repair.rs:138`,`:151`,`:32`),
    the seeding vocabulary `SegmentGroup`/`SegmentRecord`/`SegmentRef`/`SegmentedMap`/`seg_key`.
    `ReconstructionContext` (`reconstruction.rs:71`) and `RebalanceContext` (`rebalance.rs:71`)
    are `pub` with `pub` fields, and `backfill::reconcile` is `pub` (`backfill.rs:76`) — an
    integration test can drive all three.
  - **Discriminator classification — DRY-RUN at Plan, not assumed.** This project's C4-verify
    classifies its red leg on an **added** `*/tests/*.rs` (`run-verify.sh:97` `_added_files`,
    `:98` `_is_test_file`). `run-verify.sh --classify` on a synthetic patch listing the expected
    file set returned exactly `ADDED_TEST crates/custodian/tests/segmented_map_passes.rs` +
    `CRATE crates/custodian`, so the invocation is `cargo test -p wyrd-custodian --test
    segmented_map_passes` (`run-verify.sh:363-372`) and the RED leg reverts the three production
    files and every modified test file while keeping that one (`:466-476`).
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** It MUST NOT name a symbol this
    patch introduces (no new enum variant, no new field, no new helper, no new `pub` fn): the RED
    leg reverts production, so such a reference makes the target fail to **compile** and the red
    degrades from "the behaviour was wrong" to "a symbol is missing" (`run-verify.sh:487-497`
    reports that as UNVERIFIABLE, exit 77, not a red).
  - **No vacuous green.** No `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]`
    (grepped on the base), so neither zero-test guard can trip (`run-verify.sh:445` green leg,
    `:481` red leg).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the maintenance passes that restore redundancy and
  execute a drain**. Sourced, not intuited: `docs/principles.md:137` (§6 row *Storage lifecycle /
  reclamation* — "any brief in which a durable byte's protection is created, transferred or
  lifted"), sourced in turn to §5 C-1 (`docs/principles.md:109`), the maintainer's standing rule of
  2026-07-25, `0016:2802-2813`, and `gc.rs:22-25`. Over that category:
  - **A pass reads every committed object the same way.** Redundancy restoration is not a service a
    store may lose by publishing one large object. A consumer that cannot resolve is a consumer
    that cannot protect, and "unsupported record shape" is not a bounded state — nothing exits it.
  - **A repair obligation is discharged or kept; it is never discarded for want of a reading.**
    "I could not read the map" and "no committed map references this chunk" are different facts,
    and only the second permits draining the obligation. Collapsing them deletes the last record
    that says live data is under-replicated.
  - **Containment is per object, and the answer still gets made.** One damaged record may not cost
    every healthy object its repair, its fill or its evacuation; and `Err` for the whole pass is as
    wrong as `Satisfied`. A fault that is **not** one object's — the store failing underneath —
    still ends the pass, because a walk that cannot reach the store has no answer for anything.
  - **A pass that refused work does not certify.** A refused repair or evacuation, and an
    incomplete reading, both mean the pass is answering over less than the store; saying
    `Satisfied` there tells an operator a decommission is safe or that redundancy is whole.
  - **The work a pass costs is bounded by the obligations it holds, not by the product of the
    obligations and the namespace.** A repair loop whose cost is Q×N resolves is one that stops
    converging as a store grows — the permanence C-1 forbids, reached through the scheduler.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches and M4's integration branch is merged and deleted. #648/#649/#650/#651/#652
  all landed on `main` directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`. Not
  `pdca-integration/main` — that is the driver's run-scoped fold branch, and `wave_mode = "merge"`
  means a dependent builds on a genuinely merged `origin/main` anyway.)
- **Depends on:** *(none — #650 is merged as `11aa85f`, #651 as `8decc93`, both on `origin/main`)*
- **Conflicts with:** *(none in this batch)*
- **Ordering note:** **Wave 0.** Nothing to fold: the only code prerequisites (#649/#650's shared
  resolver and `Reconciled::Blocked`; #651's containment precedent) are already on the base and
  were verified there at Plan. **#682 depends on this** and is scheduled into wave 1 — it edits
  `reconstruction.rs` and `rebalance.rs` again and needs this slice's refusal path to exist before
  it can complete the move. #654 is the other wave-0 bundle and shares no file with this one
  (`crates/core/src/{lib.rs,multipart.rs}` + `Cargo.toml` vs `crates/custodian/src/*`), so the two
  build in parallel safely.
- **Surfaces:** data
- **Difficulty:** high   (three production files, seven call-sites, and a restructure of
  reconstruction's assessment loop from per-obligation namespace scans to one namespace walk — plus
  a change to what each pass's `Reconciled` answer may claim, which `reconcile_step`'s
  `least_certified` fold and every consumer of the drain surface read. A diff-reviewer must hold
  three loops, the resolver's typed-error contract, the containment/downcast rule and the
  complexity property in view at once.)
- **Scope:** make the three passes that scan `inode:` **read every committed object through the
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
- **Budget:** ≤ **900** added semantic lines (non-blank, non-comment, non-mechanical), ≤ **8**
  files. The eight, named so the cap is an allocation rather than a race:
  `custodian/src/reconstruction.rs`, `custodian/src/backfill.rs`, `custodian/src/rebalance.rs`,
  `custodian/tests/segmented_map_passes.rs` (**new**), `custodian/tests/reconstruction.rs`,
  `custodian/tests/backfill.rs`, `custodian/tests/rebalance.rs`,
  `docs/design/architecture/06-runtime-view.md` (confirm-only — that is the headroom). A **ninth**
  file means the shape is wrong: STOP and hand back a proposed split rather than finishing. The
  discriminator's fixture is the largest single item and the only real risk — prune it to what legs
  (1)–(5) need, and reuse the seeding shape rather than inventing one.
- **Repro instruction:** on the target checkout, read the seven sites with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` (and `backfill.rs`,
  `rebalance.rs`) at the lines tabulated under § Defect. Seeding **any** `seg:`-backed committed
  root — or any committed record that will not decode — makes `reconstruction::reconcile`,
  `backfill::reconcile` and `rebalance::reconcile` all return `Err` for the whole store; the
  fixture shape is `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — the five doctor.checks ids (pdca.toml :696, :703, :711, :733, :740), all OK on this host at Plan (scripts/pdca doctor). Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool. Nothing else beyond the base Rust toolchain: the passes run over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_passes.rs` — a **NEW** file, not optional,
  and the name completes the family (`segmented_map_consumers.rs` #650, `segmented_map_restore.rs`
  #651). C4-verify earns its red only from an **added** `*/tests/*.rs`; appending to either existing
  file makes it a *modified* file, the gate takes the green-only branch (`run-verify.sh:454-464`)
  and proves no red at all. Confirmed by the `--classify` dry-run above. Updates to
  `tests/{reconstruction,backfill,rebalance}.rs` ship **in addition**; C4-ci covers them.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line number
  in this brief was re-verified against `origin/main` at `339da46` during the Plan verification
  pass; still cite by symbol, not number, if the base advances.
  **Peer callsites Do MAY open — this is a composition slice; mirror them rather than invent a
  shape:**
  - `crates/custodian/src/gc.rs:360-450` — `referenced_fragments`: the canonical walk. Decode
    failure contained per object (`unresolvable.insert(key, fault); continue`), resolve via
    `metadata::resolve_chunk_map`, `Ok(None)` skipped as "no live committed generation", and the
    **downcast rule** at `:402-416` — `Ok(ChunkMapError)` is contained as *this record's* fault,
    any other error propagates because a store fault is not one object's. Contain by exactly this
    rule and no other.
  - `crates/custodian/src/restore.rs:621-688` — `committed_chunks`: the same shape applied a second
    time by #651, with the deferral note to this slice at `:616`. It shows the per-object
    granularity a *consumer* needs, as distinct from GC's fleet-wide protection set.
  - `crates/custodian/src/gc.rs:155-165` — attribution emitted by the **consumer**, per object,
    **before** the rest of the pass, so a later transient fault cannot cost the operator the
    record's name. Mirror the placement, not just the call.
  - `crates/custodian/src/reconciliation.rs:44` + `:48-60` — `Reconciled::Blocked` and
    `least_certified`: the existing vocabulary for "ran over everything it could read and refuses
    to certify the rest". Reuse it; do not invent a parallel outcome.
  - `crates/custodian/tests/segmented_map_restore.rs:387-431` — `seed_segmented` / `seed_damaged`:
    raw `seg:` + root seeding, **never** a committer, with the fixture asserting the fault is real.
    `crates/custodian/tests/segmented_map_consumers.rs:80-120` — the `BTreeMap`-backed `MemMeta`
    whose ordering makes "the damaged record is met FIRST" a property of the fixture rather than
    luck. Both are what leg (3) needs.
  - `crates/custodian/src/reconstruction.rs:186-232` — the existing per-obligation assessment loop
    and its gauge accounting (`under_replicated`, `Unreachable`, `Blocked`, `Unrepairable`,
    `Malformed`). The rework must preserve every one of those classifications and the rule that a
    never-repaired condition stays **off** the repairable-backlog gauge.
- **Docs-currency:** one confirm-only touch. `docs/design/architecture/06-runtime-view.md:29-31`
  already states the containment rule fleet-wide, including *"a consumer that has not yet adopted
  it refuses a segmented map outright"*. Confirm that sentence still reads true once these three
  passes have adopted the resolver (it does for `commit_chunk_map`, `read.rs:96` and
  `high_water_marks`, which still refuse), and edit **only** if it does not. Claim nothing the pass
  cannot evidence.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/custodian/src/{reconstruction,
  backfill,rebalance}.rs` → 8 commits; the most recent touching this behaviour are `3e05891` (#648,
  the segmented record shape — which **created** these seven fail-closed sites rather than
  duplicating this work), `0c97685` (#430, fragment identity), `5f2f79f` (#397/#348, classify
  placement before scheme) and `fddb448` (#350, backfill). No open PR touches these paths (`gh pr
  list --state open` → empty). **Closed/rejected:** PR **#647** (`enhancement/635-segmented-chunk-map`,
  CLOSED 2026-07-30, unmerged) is the un-split ancestor — it touched all three of these files plus a
  custodian-local `crates/custodian/src/resolve.rs`. It was closed for **size and reviewability**,
  not direction, and its content is landing as this slice sequence; note that its custodian-local
  resolver has been **superseded** by the shared `metadata::resolve_chunk_map` (#649), so do not
  reintroduce `crates/custodian/src/resolve.rs`. No prior attempt at this containment was rejected
  on the merits.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — The owner must reconcile the five reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently triaged; affected-path history and closed PR #647 were independently checked.; T5 Judgment — Decide whether the no-data-loss claim has adequate regression protection — all three mutations of the incomplete-reading drain guard survive, and no test directly drives segmented backfill refusal or segmented evacuation refusal, so a rebuild can add the missing cases (`crates/custodian/src/reconstruction.rs:430`; `crates/custodian/src/backfill.rs:159`; `crates/custodian/src/rebalance.rs:276`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 60 mutants tested in 67s: 18 missed, 16 caught, 26 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized (113 KB patch vs 100 KB threshold; ~1,177 added semantic lines vs ~900 budget) and not converging: 2 build rounds with a flat (not shrinking) impl-finding count, plus T4 batched-review still gating with 4 unresolved findings. Re-split at Plan rather than another Do round — per docs/2026-07-31-oversized-slices-report.md, over-budget slices don't converge with more Do iterations. Carry forward into the split: - noncanonical inode-key parsing bugs (backfill.rs:174, rebalance.rs:259, reconstruction.rs:880) that can cause writable/CAS races on non-canonical keys. - rebalance's per-chunk refusal logging floods the audit seam (should aggregate per object like backfill's emit_declined). - transient store fault can suppress the "refused" attribution the brief required. - open judgment calls for the next Plan to settle explicitly: duplicate ChunkId within a single record (base used to repair it; brief's rule targets cross-object duplicates) and whether an idle pass with an empty repair queue should be allowed to report Satisfied over an unreadable object in the store.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
