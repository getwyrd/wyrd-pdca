<!-- pdca:split-proposal v1 -->
# Split proposal — issue 711

## Why this slice is oversized

#711 asks for **one primitive and its two callers**. The attempt archived in
`iteration-v1/` built exactly that and landed **7 files / 1595 added lines / 124 KB**
against the brief's hard **≤ 6 files / ≤ 450 semantic lines** budget and the driver's
100 KB size backstop. Sign-off returned it `iterate-plan` (2026-08-10) rather than
`iterate-do` because two Do attempts had already converged on the same oversized shape.

The 7th file was not drift. Each caller's conversion **invalidates that caller's own
refusal test**: `segmented_map_reconstruction.rs`'s
`an_obligation_inside_a_segmented_object_is_refused_never_discarded` (`:484`) and
`segmented_map_rebalance.rs`'s
`an_owed_segmented_evacuation_is_refused_once_and_mutates_nothing` (`:328`) both assert
the #697 / #696 placeholder this slice removes. Landing both callers in one bundle
therefore *forces* both edits — the brief allowed only one. **Splitting by caller
dissolves that by construction: one forced edit per child.**

And the corrected slice is **bigger**, not smaller. Check surfaced four substantive gaps
that all add code:

- **Adversary** — the `chunk == prior` equality (`metadata.rs:2920` in the attempt) is the
  primitive's *only* guard for the read→prepare window, and **nothing** exercises it: it
  was deleted outright and `cargo test -p wyrd-custodian -p wyrd-core` plus the full
  15-property DST campaign at `MADSIM_TEST_NUM=50` stayed green.
- **C5 / T2** — the primitive re-reads the segment and CASes *those* bytes, while three doc
  sites claim it pins "the exact bytes the resolve read". `ResolvedChunkMap`
  (`crates/core/src/metadata.rs:2294-2300`) exposes only `record` + flattened `chunks` and
  **cannot** hand a caller the segment bytes, so the claim was never implementable.
- **T3** — `plan_evacuations` (`crates/custodian/src/rebalance.rs:257`, `plans.push` at
  `:367`) emits one plan per *(object, chunk)* with no cross-object `ChunkId` dedup, so two
  committed objects naming one `ChunkId` drive two independent moves and two orphan marks.
  Reconstruction has no such gap — `sites: HashMap<ChunkId, Site>` (`reconstruction.rs:375`,
  guarded at `:528`) dedups per pass.
- **C5-mutants** — 17 missed, *all* inside `repoint_chunk` / `covers` / `chunk_at`, which
  shipped with **zero** `wyrd-core` unit tests against a module whose own convention is to
  pin such rules in-crate (child-1 #710's boundary test, `metadata.rs:2776-2780`).

## Why NOT the seam sign-off suggested

Sign-off proposed "separate the repoint primitive from its two callers". **A
primitive-only child cannot earn evidence**, verified against this instance's own gate:

- `run-verify.sh` reverts **production** files on the RED leg (`:469-476`), so a
  `crates/core/tests/*.rs` naming `repoint_chunk` fails to *compile* against the reverted
  base → `UNVERIFIABLE`, exit 77 (`:492-500`);
- an in-crate `#[cfg(test)]` test adds no `*/tests/*.rs`, so the gate takes the green-only
  branch (`:454-464`) and proves no red either.

Either way the child ships with no discriminating evidence — a Plan-blocking gap. **Split
along the caller axis instead**, carrying the primitive with its first consumer. Both
children then drive a genuine red through `reconcile_step`. Confirmed by dry-running the
gate's own classifier on synthetic patches of each child's file set:

```
child-1: ADDED_TEST crates/custodian/tests/segmented_map_repoint.rs
child-2: ADDED_TEST crates/custodian/tests/segmented_map_evacuate.rs
```

Two children, not three: a DST-only third child would cost a full cycle for a test-only PR.
It rides with child-2, which balances the halves (child-1 would otherwise be ~1150 added
lines against child-2's ~600).

## Wave sketch

**Two waves, strictly sequential.** `child-2` **`Depends on: child-1`** — it consumes
`repoint_chunk`, which child-1 authors, and cannot compile without it. This is a genuine
build-on dependency, not a file conflict: child-2 touches **no** file child-1 touches
(child-1 owns `metadata.rs` + `reconstruction.rs` + the two reconstruction-side tests;
child-2 owns `rebalance.rs` + the two rebalance-side tests + `dst/tests/custodian.rs`).

Under `wave_mode = "merge"` with `[driver].auto_merge = false` (INTEGRATION §2) the run
**STOPs after wave 0**: the human merges child-1's PR into `main`, then re-runs, and
child-2's `C4-verify` resolves `origin/main` — which by then genuinely contains the
primitive (`run-verify.sh:_resolve_base_ref`, precedence `$PDCA_BASE` → `$PDCA_VERIFY_BASE`
→ `$WYRD_VERIFY_BASE` → the brief's base → `origin/main`). Neither child sets `Onto branch`,
so the brief's base IS the verify base.

**Both children must additionally be kept out of #717's wave** — it inserts two fields into
`PendingEntry` (`metadata.rs:1528`) and edits `dst/tests/custodian.rs`, colliding with
child-1's and child-2's file sets respectively. `Conflicts with:` cannot say so here (the
proposal's ordering fields may only name sibling labels, `split.py:_validate_ordering`), so
`717` is added to each materialised brief's `Conflicts with:` line at acceptance — the same
repointing #717's own brief records having had done to it.

**No `Depends on (merged):` is needed.** Every external prerequisite of the parent is
already merged into `origin/main` at `92e1b4b`: #710 (the ceiling helpers) as PR #718,
#695/#696/#697 as PRs #704/#705/#706.

<!-- pdca:child child-1 -->
- **Slug:** segmented-repair-completes-through-repoint
- **Defect:** **A chunk whose `ChunkRef` lives in a `seg:` record can never be repaired.**
  #697 stopped reconstruction aborting on a segmented object, but it deliberately writes
  nothing: a repair obligation for a `seg:`-resident chunk is **refused and stays queued**,
  every pass, forever (`crates/custodian/src/reconstruction.rs:552` routes it to
  `Site::Refused`, `:609` answers `Assessment::Refused`). Nothing exits that state — the
  obligation is not drained (that would be data loss), and no code path can move the
  placement, because the only placement writer in the tree rebuilds an **inode** record:
  `repair_chunk` (`:829`) takes `object.prior.chunk_map.as_flat()` at `:894`, aborts if it
  is `None`, and CASes `inode:` at `:937-953`. It can address a
  `seg:<nonce>:<epoch>:<index>` record not at all. So a multipart-published object's
  redundancy decays untended, permanently.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Five legs:
  1. **BINDING, RED pre-fix — a `seg:`-resident under-replicated chunk is repaired.** Seed a
     committed **segmented** object (raw `seg:` records + a segmented root, never a
     committer) whose chunk has lost a fragment; enqueue its repair; run `reconcile_step`
     with a `ReconstructionContext`. Assert: the rebuilt fragment is on a healthy D server in
     a failure domain distinct from the survivors; the **`seg:` record's** `ChunkRef.placement`
     names it; the repair obligation is **drained** (`queued_repairs` no longer contains it);
     the pass answers `Changed`; and the **root** record's bytes are **unchanged** (a repoint
     rewrites the segment, never the root). Base behaviour: refused, obligation still queued,
     `seg:` bytes byte-identical → **red**.
  2. **BINDING, RED pre-fix — a concurrent rewrite of a DIFFERENT chunk in the same segment
     record is MERGED, not conflicted.** Same fixture, but a competing writer moves a
     *sibling* chunk's placement inside the same `seg:` record between the pass's resolve and
     its commit. Assert **both** survive: the repair lands (obligation drained, pass answers
     `Changed`) **and** the sibling's new placement is still in the record afterwards. Base:
     refused → **red**. This leg pins the design decision below and is the one that would go
     red if the primitive instead pinned the whole resolved record's bytes.
  3. **NOT independently red — the same chunk rewritten under the plan is a CONFLICT.** A
     competing writer moves **the planned chunk's own** placement between resolve and commit.
     Assert: **nothing at all is written** — the `seg:` record still holds exactly the
     competing writer's placement, byte for byte; the repair obligation is **still queued**;
     no orphan mark was published; the pass does not certify. Pre-fix the pass refuses, so
     this leg also passes on the base — it is **not** C4-verify evidence. It is the leg the
     **mutation** oracle needs: this is the sign-off's named requirement, and `build-notes.md`
     MUST record the named negation — *deleting the `chunk == prior` equality turns leg 3
     red* — demonstrated, not asserted. (Without the pin, a chunk matched on byte offset
     alone is rewritten onto freshly-read bytes and the competing writer's placement is
     silently reverted; the adversary reproduced exactly this.)
  4. **NOT independently red — a superseded root generation is a CONFLICT.** The root is
     flipped to a different generation between resolve and commit. Assert nothing is written
     and the obligation stays queued. `build-notes.md` records the named negation for the
     root precondition too.
  5. **NOT independently red — the ceiling refusal holds over a segment record.** A `seg:`
     record seeded just under `MAX_VALUE_BYTES` whose repoint would cross it: refused, record
     byte-identical, obligation queued, pass non-certifying. #710 established the rule for the
     flat arm and its `custodian/tests/placement_ceiling.rs` is on this base; this leg pins it
     for the segmented arm, which #710 could not.

  Legs **1 and 2 are the discriminating evidence**; 3, 4 and 5 pass pre-fix by construction
  and must not be counted as red. **Additionally**, `crates/core/src/metadata.rs` gains
  in-crate `#[cfg(test)]` unit tests for the two addressing helpers the primitive introduces
  (offset-plus-equality lookup, and segment coverage), mirroring the module's own convention
  at `metadata.rs:2776-2780` — the C5 residue was 17 missed mutants, all in this new code.
- **Falsifiability:** legs 1 and 2 go RED on the ordinary base — `origin/main` at `92e1b4b`,
  no special topology, no external service. The forbidden state is *reachable by seeding*: a
  segmented object is written as raw `seg:` records plus a segmented root (this build ships
  no producer of segmented maps, which is exactly why the fixture hand-writes them, as
  `crates/custodian/tests/segmented_map_restore.rs:387-431` already does). The failure is
  deterministic and present on every pass, so no seed sweep or race window is needed to
  observe it: `reconcile_step` answers with the obligation still queued and the `seg:` bytes
  untouched. Verified by dry-running the gate's classifier — the added
  `crates/custodian/tests/segmented_map_repoint.rs` is the discriminator, the gate runs
  `-p wyrd-custodian --test segmented_map_repoint`, and that file carries no `#![cfg(...)]`,
  so it is genuinely compiled and executed in both legs (`run-verify.sh:_crate_cfgs`,
  `:363-373`). Legs 3–5 are falsifiable only against the **mutation** oracle, which is why
  each carries a required named negation rather than a red claim.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost: every durable byte is, at every instant, protected by a record that names
  it *or* evidenced for reclamation, and every state has an actor that exits it in bounded
  time** (`docs/principles.md:137`, §6 row *Storage lifecycle / reclamation*, sourced to §5
  C-1 at `:109`; the maintainer's standing rule of 2026-07-25; `0016:2802-2813`;
  `crates/custodian/src/gc.rs:22-25`). A refused-forever repair obligation is a state with
  **no** actor that exits it. The invariant is restored only when the maintenance write path
  for a `seg:`-resident chunk **exists and the repair pass completes through it** — not when
  the refusal is made quieter, better-counted or better-explained. Guarding, annotating or
  re-classifying `metadata.rs` alone satisfies nothing here.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; M4's
  integration branch is merged and deleted, and every #635 slice to date landed on `main`
  directly. Base at authoring: `92e1b4b`.)
- **Ordering note:** **Wave 0 — no in-batch prerequisite.** Every external prerequisite is
  already **merged** into `origin/main`: #710 (the ceiling helper `flat_value_ceiling_crossed`,
  `metadata.rs:380`) as PR #718, and #695/#696/#697 as PRs #704/#705/#706 — verified with
  `git -C ../wyrd log --oneline origin/main` and `gh issue view`, all four CLOSED. So no
  `Depends on (merged):` is required; do not add one. **Never share a wave with #717** — it
  inserts `owner`/`staged` into `PendingEntry` at `metadata.rs:1528`, shifting every citation
  below that point in this child's largest file. (#717 is the terminal child of #692's
  2026-08-09 split, #715 → #716 → #717; #715 and #716 touch only `crates/core/src/multipart.rs`
  and their own new test files and share nothing with this child.) **Cite by symbol, not by
  number** where a citation sits below `:1528` in `metadata.rs`. child-2 stacks on this child
  and must not start until this child's PR is **merged** — with `auto_merge = false` the driver
  stops at the wave boundary and the human merges (INTEGRATION §2).
- **Difficulty:** high
- **Scope:** **the missing maintenance write path for a `seg:`-resident chunk, and
  reconstruction completing through it.**
  - `crates/core/src/metadata.rs` — the placement move: given a resolved generation, the byte
    offset of the chunk within the object, the `ChunkRef` the caller planned from, and the new
    placement, produce the compare-and-swap batch that lands the move in whichever record
    holds that `ChunkRef` — flat inode **or** segment record — plus the in-crate unit tests for
    its addressing helpers. It **hands the batch back** rather than committing: the caller adds
    its own evidence for the same move (the obligation delete, the orphan marks) and lands all
    of it in ONE mutation (`0005:277`, ADR-0015). Weigh the re-encoded record through **#710's**
    `flat_value_ceiling_crossed` (`metadata.rs:380`) — do not re-implement the guard and do not
    add a second ceiling constant.
  - **WHAT IT PINS — settled at Plan, do not re-derive.** `ResolvedChunkMap`
    (`metadata.rs:2294-2300`) carries only `record` + the flattened `chunks`; it **cannot**
    hand back per-segment bytes, so "pin the exact bytes the resolve read" is not
    implementable for the segmented arm and must not be claimed. The move pins **three**
    things: the **root generation's** bytes (a supersede always flips the root first,
    `0016:2452-2462`, so a repoint racing one loses its CAS); the **segment record's own
    freshly-read bytes**; and the **`ChunkRef` itself** — the chunk moved is the one that
    begins at the given offset **and equals** the reference the caller planned from, anything
    else is a conflict. **A concurrent edit to a *sibling* chunk in the same segment record is
    therefore MERGED, deliberately** — two repairs inside one multipart object must not
    serialise on the whole record — **while an edit to the planned chunk itself is a
    conflict.** Leg 2 pins the first half, leg 3 the second. Any prose Do writes about what the
    move pins must say this; the archived attempt's three doc sites saying otherwise are a
    **known defect to correct, not a spec to follow**.
  - `crates/custodian/src/reconstruction.rs` — the repair pass stops refusing a `seg:`-resident
    chunk (#697's placeholder at `:552` / `:609`) and completes the move. The placement change,
    the discharge of the repair obligation (`repair::repair_key` delete) and the orphan evidence
    for each displaced position stay **one batch** — do not split the batch to fit the new
    primitive; if the primitive's shape makes that awkward, change the primitive.
  - `crates/custodian/tests/segmented_map_reconstruction.rs` — leg 2
    (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`, `:484`) asserts the
    refusal this child removes and MUST be rewritten to assert the repair now lands. This is a
    **forced** edit, budgeted for, not drift.
  - **Constraints carried forward (blockers from #651 / #638 — these bound the shape, they do
    not name it):** duplicate chunk ids get one plan, not independent ones — keep it to the
    narrow rule, do **not** rebuild the cross-object claim-counting apparatus dropped at #651's
    replan. **Bounded memory:** pin the bytes of **one** record at a time; find the covering
    segment in the root's own table (the tiling is contiguous and checked at decode,
    `SegmentedMap::new`, `metadata.rs:870`), so no `seg:` range is walked and no other segment
    is decoded; do not retain the namespace's decoded chunks and do not deep-copy a segmented
    root into every plan. **A losing CAS does not retract already-published bytes** — settled,
    rejected 4× in #638 (`results/issue_638/review-rejected.md:15-16`); the refusal and conflict
    paths write **nothing at all**. Keep `commit_chunk_map`'s CAS idiom for the flat arm
    (`metadata.rs:1769-1797`: `version = prior.version + 1` and `..prior.clone()`, so ADR-0047
    object metadata is **preserved**); its own segmented refusal at `:1776-1780` **stays** —
    `commit_chunk_map` is not what this child changes.
  - **Budget:** ≤ **4** files — `core/src/metadata.rs`, `custodian/src/reconstruction.rs`,
    `custodian/tests/segmented_map_repoint.rs` (**new**), `custodian/tests/segmented_map_reconstruction.rs`
    — ≤ **250** added semantic lines of non-test code, and `patch.diff` ≤ **95 KB** (the
    driver's size backstop trips at 100 KB, and the parent's attempt hit 124 KB). A fifth file
    means the shape is wrong; in particular needing to edit `rebalance.rs`, `backfill.rs`,
    `restore.rs`, `gc.rs` or `desired_state.rs` means the scope has drifted: **STOP and hand
    back a proposed split.** Keep the new test lean by reusing the fixture shape cited below
    rather than re-authoring one.
  - **Out of scope:** the **drain / evacuation** caller and its tests, and the DST
    repoint-versus-supersede property (**child-2** — this child must leave
    `crates/custodian/src/rebalance.rs`, `crates/custodian/tests/segmented_map_rebalance.rs`
    and `crates/dst/tests/custodian.rs` untouched). The write-side ceiling helper itself
    (**#710**, merged — consume it). **The committer, the destination pre-mark, the drain fence,
    rollback and resume (#653).** Proposal 0016's full segment-repoint precondition set
    (`0016:669`) is `require(seg == prior)` + `require(inode == prior)` + `require(orphan:<P_new>
    == prior)` (the destination pre-mark) + `require_absent(desired:dserver:<S_new>)` (the drain
    fence); **this child ships only the first two.** That is the parent issue's own carve-out and
    a **pre-declared sign-off item, not a surprise NEEDS-HUMAN**: without the pre-mark a repoint
    that loses its CAS leaves the pre-written destination fragment unreferenced — exactly the
    behaviour the **flat** path already has and documents today
    (`crates/custodian/src/reconstruction.rs:931-935`: *"a crash here leaves only collectable
    garbage"*), reclaimed by GC's ordinary unreferenced sweep. This child **introduces no new
    stranding class**; it extends an existing, settled one to a second record shape. Do **not**
    implement the pre-mark or the fence here. Also out: the chunk-id floor (**#652**, merged);
    restore and `desired_state` (**#651**, merged); `gc.rs` / `scrub.rs` (**#650**, merged);
    `backfill.rs` (**#695**, merged — untouched here). The read side generally: no new resolving
    walk, no change to `resolve_chunk_map`, no change to the containment rule #695/#696/#697
    landed. Any new or edited ADR / spec / proposal (0016 is a **draft** and stays untouched);
    any conformance-vector change; any new dependency.
  - **KEEP THE DISCRIMINATOR ASSERTION-RED — HARD CONSTRAINT.** The new test MUST NOT name the
    primitive or any other symbol this patch introduces. The RED leg reverts production
    (`run-verify.sh:469-476`), so such a reference makes the target fail to **compile** and the
    gate reports UNVERIFIABLE (exit 77, `:492-500`) instead of a red. Drive everything through
    `reconcile_step` and observe the **store**. `MAX_VALUE_BYTES` is base-visible and may be
    named.
- **Repro instruction:** on the target checkout, read the binding commit with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` — `:894` takes
  `as_flat()` and aborts on `None`, `:937-953` CASes `inode:`; nothing addresses a `seg:`
  record. Then seed a committed segmented object (raw `seg:` records + a segmented root,
  per `crates/custodian/tests/segmented_map_restore.rs:387-431`) with a lost fragment, enqueue
  its repair, and run `reconcile_step` with a `ReconstructionContext`: the obligation is
  refused (`reconstruction.rs:552`, `:609`) and stays queued, every pass, forever.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/custodian/tests/segmented_map_repoint.rs` — a **NEW** file, not
  optional, completing the `segmented_map_*` family (`_consumers.rs` #650, `_restore.rs` #651,
  `_backfill.rs` #695, `_rebalance.rs` #696, `_reconstruction.rs` #697). This project's
  `C4-verify` earns its red **only** from an *added* `*/tests/*.rs` (`run-verify.sh:_added_files`
  + `_is_test_file`, `:97-98`); a test appended to an existing file makes the gate take the
  green-only branch (`:454-464`) and prove no red. Confirmed at Plan by dry-running
  `run-verify.sh --classify` over a synthetic patch of this child's exact file set: it returns
  `ADDED_TEST crates/custodian/tests/segmented_map_repoint.rs`, and because that is the only
  added test the gate runs `-p wyrd-custodian --test segmented_map_repoint` — so the edit to
  `segmented_map_reconstruction.rs` ships in addition and is covered by C4-ci, not by the
  discriminator. The in-crate `metadata.rs` unit tests are likewise C4-ci's, not the
  discriminator's.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice — mirror these peers rather than invent a shape:**
  `crates/core/src/metadata.rs:1769-1797` (`commit_chunk_map`, the flat CAS idiom — `version + 1`,
  `..prior.clone()`); `crates/custodian/src/reconstruction.rs:829-956` (`repair_chunk`, the
  binding commit being replaced, including the `repair::repair_key` delete and the
  `gc::orphan_key` puts that must stay **in the same batch** as the placement change, and the
  ceiling refusal at `:923-929` that must now run inside the primitive);
  `crates/core/src/metadata.rs:2294-2300` and `:2647-2660` (`ResolvedChunkMap` /
  `resolve_chunk_map` — what a caller actually holds after a resolve, and therefore what the
  move can and cannot pin); `crates/core/src/metadata.rs:1258-1330` (`seg_key` /
  `seg_range_prefix` / `parse_seg_key` — the only sanctioned way to address a segment record);
  `crates/core/src/metadata.rs:1127-1200` (`SegmentRecord::new` / `chunks()` / `byte_offset()`,
  the validating constructor) and `:2536-2552` (`decode_segment_record`);
  `crates/core/src/metadata.rs:2493-2500` (the resolver's read-side `MAX_VALUE_BYTES` refusal —
  the boundary a write must not cross) and `:2582-2589` (the root-table extent invariant a
  placement-only rewrite must preserve); `crates/core/src/metadata.rs:2776-2780` (#710's in-crate
  boundary test — the convention the new unit tests follow);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` (`seed_segmented` / `seed_damaged`:
  raw `seg:` + root seeding with a fixture self-check). **Salvage:** the archived attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a working
  primitive and a working reconstruction caller that passed C4-ci and C4-verify — reuse them,
  but (a) correct every doc site claiming the move pins "the exact bytes the resolve read", (b)
  add legs 2–5 and the in-crate unit tests, and (c) drop everything in the rebalance and DST
  files, which belong to child-2.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/core/src/metadata.rs` → most
  recently `d2609b2` (#710, the ceiling helper this consumes), `b083ec4` (#652), `11aa85f`
  (#650), `99c7fcf` (#649, the shared resolver — the premise this builds on), `3e05891` (#648,
  the segmented record shape). None implements a placement move in a `seg:` record.
  `git -C ../wyrd log origin/main -- crates/custodian/src/reconstruction.rs` → `1f871ce` (#697,
  the containment this completes), the repair loop (#144) and its fixes (#197 *"don't count
  aborted repairs as successes"*, PR #238; #346 identity-placement fallback; #348 malformed
  placement). No open PR touches these paths. **Closed/rejected:** PR **#647** (CLOSED
  2026-07-30, unmerged) is the un-split ancestor and contained a `repoint`-shaped write; it was
  closed for **size and reviewability**, not direction. Its custodian-local
  `crates/custodian/src/resolve.rs` has been superseded by the shared resolver — **do not
  reintroduce it.** Within the harness, `results/issue_638/review-rejected.md:15-16` records the
  standing, four-times-rejected rule that a losing/late write is **not** retracted; do not
  re-litigate it.
- **Disposition hint:** likely-fix
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** segmented-drain-evacuation-completes
- **Defect:** **A D-server decommission holding a `seg:`-resident fragment never
  converges.** #696 stopped rebalance aborting on a segmented object, but it deliberately
  writes nothing: an evacuation owed by a chunk whose `ChunkRef` lives in a `seg:` record is
  **refused and stays owed**, every pass, forever (`crates/custodian/src/rebalance.rs:352-355`
  — `scanned_flat` is `None`, so the chunk produces no plan and the object is counted refused
  at `:378-381`; the drain is never certified, `:187`). The operator's drain therefore never
  completes and the server can never be retired. Child-1 lands the placement move for
  whichever record holds a chunk; this child is the drain caller completing through it.
  Separately, `plan_evacuations` (`:257`, `plans.push` at `:367-376`) emits one plan per
  *(object, chunk)* with **no** cross-object `ChunkId` dedup — reconstruction has such a dedup
  (`reconstruction.rs:375`, guarded at `:528`), rebalance does not — so two committed objects
  naming one `ChunkId` drive two independent moves and two orphan marks over the same physical
  fragment.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_evacuate.rs`
  passes, driven **only** through symbols visible on the base (post-child-1) —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Four legs:
  1. **BINDING, RED pre-fix — a `seg:`-resident fragment is evacuated off a draining server.**
     Seed a committed **segmented** object (raw `seg:` records + a segmented root, never a
     committer); `set_lifecycle(.., Draining)` on the server holding one of its fragments; run
     `reconcile_step` with a `RebalanceContext`. Assert: the fragment is copied to a
     non-draining server in a failure domain distinct from the survivors; the **`seg:` record's**
     `ChunkRef.placement` names it and no longer names the draining server; the vacated position
     is orphan-marked; the pass answers `Changed`; and the **root** record's bytes are
     **unchanged**. Base: refused, placement unchanged → **red**.
  2. **BINDING, RED pre-fix — two committed objects sharing one `ChunkId` both end the pass
     naming only present fragments.** Seed two committed **segmented** objects whose maps both
     name the same `ChunkId`, with one of its positions on a draining server. Run the pass.
     Assert: **neither object's placement names the draining server** afterwards; **every
     fragment either object names is actually present** on the D server it names; and no
     fragment a committed map still names has been reclaimed. Base: refused, both still naming
     the draining server → **red**. This is the T3 finding from the parent's review, stated as
     the invariant rather than as a mechanism — the simplest satisfying shape mirrors
     reconstruction's per-pass `ChunkId` dedup (`reconstruction.rs:375`, `:528`), but the
     binding claim is the invariant, and whatever Do lands must record in `build-notes.md`
     which of the two it did and why.
  3. **NOT independently red — the ceiling refusal holds over a segment record on the drain
     arm.** A `seg:` record seeded just under `MAX_VALUE_BYTES` whose repoint would cross it:
     refused, record byte-identical, nothing copied, the drain **not** certified. Passes
     pre-fix (refused for the other reason) — not C4-verify evidence.
  4. **NOT independently red — a lost CAS writes nothing and retracts nothing.** A competing
     writer takes the record between the pass's resolve and its commit. Assert the `seg:`
     record holds exactly the competing writer's bytes, no orphan mark for this move was
     published, and the drain is not certified — and that the already-copied destination
     fragment is **left in place** as collectable garbage rather than deleted (settled and
     rejected 4× in #638, `results/issue_638/review-rejected.md:15-16`).

  Legs **1 and 2 are the discriminating evidence.** **Additionally**, the DST
  **repoint-versus-supersede** property ships in the **existing**
  `crates/dst/tests/custodian.rs` (a *new* `crates/dst/tests/*.rs` would be `#![cfg(madsim)]`
  and, as an added test file, would become the C4-verify discriminator and compile to nothing
  under the gate's invocation — `run-verify.sh:_crate_cfgs`, `:104-121`). Across the seed
  sweep, in **both** interleavings (the repoint commits before the racing write, and after it),
  a repoint whose pinned root generation **or** whose pinned segment bytes changed under it
  commits **nothing** — neither the placement nor any orphan mark — and the object is left
  naming a fragment that exists. **Scope this property honestly in its own doc comment:** it
  proves the two CAS **preconditions**, and `build-notes.md` must record the named negations
  (deleting either `require` turns it red at `MADSIM_TEST_NUM=50`). It does **not** reach the
  read→prepare window — the racing batch is applied inside the repoint's own `commit()`, i.e.
  strictly after the primitive's own read — and must not claim to; that window is covered by
  child-1's deterministic legs. C4-ci runs this property; it is **not** the C4-verify
  discriminator.
- **Falsifiability:** legs 1 and 2 go RED on this child's base — `origin/main` **after
  child-1's PR is merged** — with no special topology and no external service. The forbidden
  state is reachable by seeding: a segmented object is hand-written as raw `seg:` records plus
  a segmented root (this build ships no producer of segmented maps), and `set_lifecycle(..,
  Draining)` puts a server into the drain state deterministically; the refusal then occurs on
  every pass, so no seed sweep is needed to observe it. Verified by dry-running the gate's
  classifier over a synthetic patch of this child's exact file set: it returns
  `ADDED_TEST crates/custodian/tests/segmented_map_evacuate.rs` as the sole discriminator, so
  the gate runs `-p wyrd-custodian --test segmented_map_evacuate`; that file carries no
  `#![cfg(...)]` and is genuinely compiled and executed in both legs, and the `crates/dst`
  edit — which *is* `#![cfg(madsim)]` — is applied but **not** compiled by the discriminator
  run, so it can neither vacuously green nor false-red the gate. **Base precondition:** if
  child-1's PR is not merged when this child builds, `repoint_chunk` is absent and the whole
  patch fails to compile — that is why the wave boundary is a hard stop, not a preference.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost: every durable byte is, at every instant, protected by a record that names
  it *or* evidenced for reclamation, and every state has an actor that exits it in bounded
  time** (`docs/principles.md:137`, §6 row *Storage lifecycle / reclamation*, sourced to §5
  C-1 at `:109`; the maintainer's standing rule of 2026-07-25; `0016:2802-2813`;
  `crates/custodian/src/gc.rs:22-25`). A drain that can never certify is a state with **no**
  actor that exits it, and a fragment two objects reference is a durable byte whose protection
  must hold under *both* references at every instant. The invariant is restored only when the
  drain pass **completes the move** for a `seg:`-resident fragment and leaves every committed
  reference naming a fragment that exists — not when the refusal is better counted or the
  double-move is argued to be harmless.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; the base this child builds
  on is `main` **after child-1 has merged** — see Ordering note.)
- **Depends on:** child-1
- **Ordering note:** **Wave 1 — `Depends on: child-1`** is a genuine build-on dependency, not
  a file conflict: this child calls the placement primitive child-1 authors in
  `crates/core/src/metadata.rs` and cannot compile without it. The two children share **no
  file** — child-1 owns `metadata.rs`, `reconstruction.rs` and the two reconstruction-side
  tests; this child owns `rebalance.rs`, the two rebalance-side tests and
  `dst/tests/custodian.rs`. Under `wave_mode = "merge"` with `[driver].auto_merge = false`
  (INTEGRATION §2) the driver does **not** merge at the wave boundary — it **stops**, the
  human merges child-1's PR into `main`, and the run resumes; only then does the
  `origin/main` that `C4-verify` resolves genuinely contain the primitive
  (`run-verify.sh:_resolve_base_ref`). **If child-1 is not accepted and merged, hold this
  bundle** — do not rebuild it against a base without the primitive and do not let it absorb
  child-1's write path; that re-creates the un-splitting that closed PR #647. Every *external*
  prerequisite is already merged (#710 as PR #718; #695/#696/#697 as PRs #704/#705/#706), so
  no `Depends on (merged):` is needed. **Never share a wave with #717** — it edits
  `crates/dst/tests/custodian.rs`, which is in this child's file set (its substantive DST edit
  against this child's; #717's own brief already declares the reciprocal conflict). **Cite by
  symbol, not by number** — child-1 will have moved `reconstruction.rs`'s and `metadata.rs`'s
  line numbers before this child builds.
- **Difficulty:** medium
- **Scope:** **the drain pass completing the placement move for a `seg:`-resident fragment,
  and one committed reference set surviving a shared chunk's move.**
  - `crates/custodian/src/rebalance.rs` — the evacuation pass stops refusing a `seg:`-resident
    chunk (#696's placeholder at `:352-355`) and completes the move through **child-1's**
    primitive, exactly as child-1 converted the repair caller. The placement change and the
    orphan evidence for each vacated position stay **one batch** (`0005:298-299`, ADR-0015).
    Route the ceiling refusal through the primitive; do not keep a second copy of the check at
    `:522-528`, and do not add a second ceiling constant. Also close the shared-`ChunkId` gap
    the parent's review found in the pass's planning, in whatever shape satisfies leg 2's
    invariant.
  - `crates/custodian/tests/segmented_map_rebalance.rs` — leg 2
    (`an_owed_segmented_evacuation_is_refused_once_and_mutates_nothing`, `:328`) asserts the
    refusal this child removes and MUST be rewritten to assert the evacuation now lands. This
    is a **forced** edit, budgeted for, not drift.
  - `crates/dst/tests/custodian.rs` — the repoint-versus-supersede property, added to the
    **existing** file, scoped as the success criterion states.
  - **Constraints carried forward:** **bounded memory** — the pass already deep-copies
    `prior_chunks` into every plan (`rebalance.rs:369`, `O(chunks)` per plan); do not make it
    worse by deep-copying a segmented root into every plan (`O(chunks × segments)`), and do
    not retain the namespace's decoded chunks. **Do not rebuild the cross-object
    claim-counting apparatus dropped at #651's replan** — leg 2 is a narrow per-pass rule, not
    a reference counter. **A losing CAS does not retract already-published bytes** (#638,
    `results/issue_638/review-rejected.md:15-16`); the refusal and conflict paths write
    nothing at all.
  - **Budget:** ≤ **4** files — `custodian/src/rebalance.rs`,
    `custodian/tests/segmented_map_evacuate.rs` (**new**),
    `custodian/tests/segmented_map_rebalance.rs`, `dst/tests/custodian.rs` — ≤ **200** added
    semantic lines of non-test code, and `patch.diff` ≤ **95 KB**. A fifth file means the
    shape is wrong; in particular needing to edit `crates/core/src/metadata.rs`,
    `reconstruction.rs`, `backfill.rs`, `restore.rs`, `gc.rs` or `desired_state.rs` means the
    scope has drifted: **STOP and hand back a proposed split.** Needing to change the
    primitive specifically is the one signal worth reporting rather than working around —
    child-1 built it for two callers, and if the second does not fit, say so.
  - **Out of scope:** the placement primitive itself and its in-crate unit tests (**child-1** —
    consume them, do not author them, do not edit `crates/core/src/metadata.rs` at all); the
    repair caller and its tests (**child-1**). **The committer, the destination pre-mark, the
    drain fence, rollback and resume (#653)** — proposal 0016's full precondition set
    (`0016:669`) also requires `require(orphan:<P_new> == prior)` and
    `require_absent(desired:dserver:<S_new>)`, and this child, like child-1, ships **only** the
    two record preconditions. That is the parent issue's carve-out and a **pre-declared
    sign-off item**: without the pre-mark, a move that loses its CAS leaves the copied
    destination fragment unreferenced — exactly what the **flat** drain path already does and
    documents (`crates/custodian/src/rebalance.rs:530-531`, *"a crash here leaves only
    collectable garbage"*), reclaimed by GC's unreferenced sweep, whose committed-reference set
    is a hard safety gate (`crates/custodian/src/gc.rs:143-147`, `:190-193`). No new stranding
    class is introduced. Also out: `backfill.rs` (#695, merged), `gc.rs` / `scrub.rs` (#650,
    merged), restore and `desired_state` (#651, merged). The read side generally: no new
    resolving walk, no change to `resolve_chunk_map`, no change to the containment rule
    #695/#696/#697 landed. Any new or edited ADR / spec / proposal (0016 is a **draft** and
    stays untouched); any conformance-vector change; any new dependency.
  - **KEEP THE DISCRIMINATOR ASSERTION-RED — HARD CONSTRAINT.** The new test MUST NOT name any
    symbol this patch introduces, and must not name child-1's primitive either if child-1's
    merge is what makes it visible — drive everything through `reconcile_step` and observe the
    **store**. The RED leg reverts production (`run-verify.sh:469-476`); a reference to a
    symbol this patch adds makes the target fail to **compile** and the gate reports
    UNVERIFIABLE (exit 77, `:492-500`) instead of a red. `MAX_VALUE_BYTES` is base-visible and
    may be named.
- **Repro instruction:** on the target checkout, read the refusal with
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` — `:352-355`
  (`let Some(prior_chunks) = scanned_flat else { refused = true; continue; }`) and the
  binding commit at `:538-556`, which CASes `inode:` and can address no `seg:` record. Then
  seed a committed segmented object (raw `seg:` records + a segmented root, per
  `crates/custodian/tests/segmented_map_rebalance.rs:221-253`), `set_lifecycle` the server
  holding one of its fragments to `Draining`, and run `reconcile_step` with a
  `RebalanceContext`: the evacuation is refused, nothing is written, and the drain is never
  certified — every pass, forever. For the second defect, seed **two** committed objects whose
  maps name the same `ChunkId` and read `plan_evacuations` at `:318-376`: one plan is pushed
  per object, with no dedup between them.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/custodian/tests/segmented_map_evacuate.rs` — a **NEW** file, not
  optional. This project's `C4-verify` earns its red **only** from an *added* `*/tests/*.rs`
  (`run-verify.sh:97-98`); appending to `segmented_map_rebalance.rs` would make the gate take
  the green-only branch (`:454-464`) and prove no red. Confirmed at Plan by dry-running
  `run-verify.sh --classify` over a synthetic patch of this child's exact file set: the sole
  `ADDED_TEST` is this file, so the gate runs `-p wyrd-custodian --test segmented_map_evacuate`
  and reads its cfg gates off **this file only** — which is precisely why the DST property must
  go into the **existing** `crates/dst/tests/custodian.rs`: a new `crates/dst/tests/*.rs` would
  be a second `ADDED_TEST`, would put `--cfg madsim` on the run
  (`run-verify.sh:_crate_cfgs`, `:115-121`), and would change what the gate compiles. The edits
  to `segmented_map_rebalance.rs` and `dst/tests/custodian.rs` ship **in addition** and are
  covered by C4-ci.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice — mirror these peers rather than invent a shape:**
  `crates/custodian/src/reconstruction.rs` **as child-1 leaves it** — the repair caller's
  conversion to the primitive is the exact pattern this child repeats for the drain, including
  where the ceiling refusal and the orphan puts land; read it first;
  `crates/custodian/src/rebalance.rs:429-556` (`evacuate_chunk`, the binding commit being
  replaced, including the `gc::orphan_key` puts that must stay **in the same batch** as the
  placement change, and the ceiling refusal at `:522-528` the primitive now owns);
  `crates/custodian/src/rebalance.rs:257-381` (`plan_evacuations` — where the shared-`ChunkId`
  gap lives) against `crates/custodian/src/reconstruction.rs:375` and `:528`
  (`sites: HashMap<ChunkId, Site>`, the peer's per-pass dedup);
  `crates/custodian/src/gc.rs:143-147` and `:190-193` (the committed-reference safety gate —
  what actually protects a still-referenced position that carries a stale orphan mark);
  `crates/custodian/tests/segmented_map_rebalance.rs:221-253` (`seed_segmented`: raw `seg:` +
  root seeding) and `:297-311` (`assert_flat_evacuated`, the assertion shape to mirror);
  `crates/dst/tests/custodian.rs` (the existing seeded Tier-0 custodian properties, for the
  shape the new one must match). **Salvage:** the archived parent attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a working
  rebalance caller and a working `RaceAtRepoint` DST double that passed C4-ci — reuse them,
  but (a) correct the doc site claiming the move pins "the exact bytes the resolve read", (b)
  add legs 2 and 4 and re-scope the DST property's doc comment to what it actually proves, and
  (c) drop everything in `metadata.rs`, `reconstruction.rs` and
  `segmented_map_reconstruction.rs`, which belong to child-1.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/custodian/src/rebalance.rs`
  → `3829097` (#696, the containment this completes), the drain loop (#145) and its fixes
  (#346 identity-placement fallback; #348 malformed placement). `git -C ../wyrd log origin/main
  -- crates/dst/tests/custodian.rs` → the Tier-0 custodian property set; no repoint property
  exists. No open PR touches these paths. **Closed/rejected:** PR **#647** (CLOSED 2026-07-30,
  unmerged) is the un-split ancestor of the whole #682 family and contained a `repoint`-shaped
  drain write; it was closed for **size and reviewability**, not direction — do not reintroduce
  its custodian-local `crates/custodian/src/resolve.rs`, superseded by the shared resolver
  (#649). The parent bundle's own first attempt (`results/issue_711/iteration-v1/`) is
  rejected-for-size, **not** rejected-for-direction; its rebalance hunk is salvage, its file
  count is the thing that must not recur. Within the harness,
  `results/issue_638/review-rejected.md:15-16` records the standing, four-times-rejected rule
  that a losing/late write is **not** retracted; do not re-litigate it.
- **Disposition hint:** likely-fix
<!-- pdca:end child-2 -->
