- **Slug:** repoint-chunk-segmented-placement-moves
- **Defect / goal:** **A chunk that lives in a `seg:` record can never be repaired or evacuated.**
  #695/#696/#697 stop the three maintenance passes aborting on a segmented object, but they
  deliberately write nothing: a repair obligation or a drain evacuation for a `seg:`-resident chunk
  is **refused and stays queued**, every pass, forever. Nothing exits that state — the obligation is
  not drained (which would be data loss), and no code path can move the placement, because the only
  placement writers in the tree rebuild an **inode** record: `reconstruction::repair_chunk` builds
  `plan.prior.chunk_map.as_flat()?.to_vec()` and CASes the inode
  (`crates/custodian/src/reconstruction.rs:578-612`), and `rebalance::evacuate_chunk` does the same
  (`crates/custodian/src/rebalance.rs:296-330`). Neither can address a
  `seg:<nonce>:<epoch>:<index>` record at all. So a multipart-published object's redundancy decays
  untended and a D-server decommission holding one of its fragments never converges. Both are
  permanent states, which C-1 rules out as costs (`docs/principles.md:137` §6 row *Storage
  lifecycle / reclamation*, sourced to §5 C-1 at `:109`; the maintainer's standing rule of
  2026-07-25; `0016:2802-2813`; `gc.rs:22-25`).
  Give the repair and evacuation passes an **exact-bytes placement move** that works in whichever
  record holds the chunk, and switch both callers onto it.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs` passes,
  driven **only** through symbols visible on the base (post-child-1, post-#696/#697) —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, ReconstructionContext, RebalanceContext,
  Reconciled}`, `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle,
  reconciliation_status, ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Four legs:
  1. **A `seg:`-resident under-replicated chunk is repaired.** Seed a committed **segmented**
     object (raw `seg:` records + a segmented root, never a committer) whose chunk has lost a
     fragment, enqueue its repair, run `reconcile_step` with a `ReconstructionContext`. Assert: the
     rebuilt fragment is on a healthy D server in a distinct failure domain; the `seg:` record's
     `ChunkRef.placement` now names it; the repair obligation is **drained** (`queued_repairs` no
     longer contains it); the pass answers `Changed`; and the **root** record's bytes are unchanged
     except as the move itself requires. Base behaviour: refused, obligation still queued, `seg:`
     bytes unchanged → **red**.
  2. **A `seg:`-resident fragment is evacuated off a draining server.** Same fixture shape with
     `set_lifecycle(.., Draining)` on the server holding a fragment; run `reconcile_step` with a
     `RebalanceContext`. Assert the fragment is copied to a non-draining server in a distinct
     domain, the `seg:` record names it, the vacated position is orphan-marked, and the pass answers
     `Changed`. Base: refused, placement unchanged → **red**.
  3. **The ceiling refusal holds over a segmented record.** A `seg:` record seeded just under
     `MAX_VALUE_BYTES` whose repoint would cross it: refused, record byte-identical, obligation
     queued, pass non-certifying. (This leg is **not** independently red — pre-fix the move is
     refused for the *other* reason, and child-1 already established the rule. It ships because it
     pins the rule for the segmented arm, which child-1 cannot; **do not count it as discriminating
     evidence**.)
  4. **Two committed references to the same `ChunkId` get one plan, not independent ones.** Seed
     two committed objects whose maps both name the same `ChunkId`, with a repair queued for it.
     Assert the pass does not repoint or overwrite the same `FragmentId`s twice and does not orphan
     copies the other object still references — neither object is left naming a fragment that was
     reclaimed.
  Legs (1), (2) and (4) are binding. **Additionally**, the DST **repoint-versus-supersede** property
  ships in the **existing** `crates/dst/tests/custodian.rs` (a new `crates/dst/tests/*.rs` would put
  `#![cfg(madsim)]` on the C4-verify invocation and change what the gate compiles): a repoint whose
  pinned root generation **or** segment bytes changed under it commits **nothing** — neither the
  placement nor any orphan mark — and the object is left naming a fragment that exists. Assert it
  across the seed sweep, in **both** interleavings (repoint wins before the supersede's inode CAS;
  repoint loses after it). C4-ci runs it; it is **not** the C4-verify discriminator.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly.)
- **Reproduction:** on the target checkout, read the two binding commits with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` (`:578-612`) and
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` (`:296-330`) — both rebuild an
  **inode** record and can address no `seg:` record. Seed a committed segmented object (raw `seg:`
  records + a segmented root) with a lost fragment, enqueue its repair, and run `reconcile_step`:
  the obligation is refused and stays queued, every pass, forever.
- **Scope (one logical fix) / out of scope:** **one primitive and its two callers.**
  - `crates/core/src/metadata.rs` — `repoint_chunk`: move one chunk's placement in the record that
    holds its `ChunkRef` — flat inode **or** segment record. Both arms pin the **exact bytes the
    resolve read**: the root generation, and additionally, for a segmented map, the segment record.
    A stale-generation write is a `Conflict`, never a silent overwrite. The refusal and the conflict
    paths write **nothing at all**. Route both arms through **child-1's** ceiling helpers — do not
    re-implement the guard, and do not add a second ceiling constant.
  - `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs` — the two
    callers stop refusing a `seg:`-resident chunk (#696's and #697's placeholder) and complete the
    move through the new primitive. The placement change, the discharge of the repair obligation,
    and the orphan evidence for each displaced position stay **one commit** — do not split the batch
    to fit the new primitive; if the primitive's shape makes that awkward, change the primitive.
  - **The seam child-1 provides is stable by test, not by promise:**
    `custodian/tests/placement_ceiling.rs` is on this child's base and C4-ci runs it, so replacing
    the binding commits with `repoint_chunk` cannot silently regress the refusal, certification or
    accounting rules. The rework this child does to child-1's two caller functions is inherent to
    the parent's own design (the parent replaced those same commits) and bounded to them; the
    outcome enums are crate-private (`reconstruction.rs`, `rebalance.rs`), so no public API
    churns.
  - `crates/dst/tests/custodian.rs` — the repoint-versus-supersede property, added to the
    **existing** file.
  - **Constraints carried forward (blockers found on the old #651 — must not recur; these bound the
    shape, they do not name it):**
    - **Duplicate chunk ids get one plan, not independent ones.** Keep this to the narrow rule; do
      **not** rebuild the cross-object claim-counting apparatus dropped at #651's replan.
    - **Bounded memory.** The move pins the bytes of **one** record at a time. Do not retain the
      namespace's decoded chunks, and do not deep-copy a segmented root into every plan
      (O(chunks × segments)).
    - **A losing CAS does not retract already-published bytes** — settled, rejected 4× in #638
      (`results/issue_638/review-rejected.md:15-16`). The refusal path writes nothing at all.
    - Keep the CAS idiom of `commit_chunk_map` (`metadata.rs:1741-1768`) for the flat arm —
      `version = prior.version + 1` and `..prior.clone()` so ADR-0047 object metadata is
      **preserved**. Its own segmented refusal at `:1748-1753` **stays**; `commit_chunk_map` is not
      what this slice changes.
  - **Budget:** ≤ **450** added semantic lines (non-blank, non-comment, non-mechanical), ≤ **6**
    files: `core/src/metadata.rs`, `custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs`,
    `custodian/tests/segmented_map_repoint.rs` (**new**), `dst/tests/custodian.rs`, and at most one
    of `custodian/tests/{reconstruction,rebalance}.rs`. A seventh file means the shape is wrong — in
    particular, needing to edit `backfill.rs`, `restore.rs`, `gc.rs` or `desired_state.rs` means the
    scope has drifted: STOP and hand back a proposed split.
  - **Out of scope:** the write-side ceiling helpers themselves and the outcome accounting
    (**child-1** — consume them, do not author them). **The committer, the destination pre-mark, the
    drain fence, rollback and resume (#653).** Proposal 0016's full segment-repoint precondition set
    (`0016:669`) is `require(seg == prior)` + `require(inode == prior)` + `require(orphan:<P_new> ==
    prior)` (the destination pre-mark) + `require_absent(desired:dserver:<S_new>)` (the drain
    fence), bounded by `W_repoint`. **This slice ships only the first two.** That is the issue's own
    carve-out and it is a **pre-declared sign-off item, not a surprise NEEDS-HUMAN**: without the
    pre-mark, a repoint that loses its CAS leaves the pre-written destination fragment unreferenced
    — which is exactly the behaviour the **flat** path already has and documents today
    (`crates/custodian/src/reconstruction.rs:610-614`, `crates/custodian/src/rebalance.rs:325-329`:
    *"the rebuilt fragments are collectable garbage"*), reclaimed by GC's ordinary unreferenced
    sweep. So this slice **introduces no new stranding class**; it extends an existing, settled one
    to a second record shape. Do **not** implement the pre-mark or the fence here.
    Also out: the chunk-id floor (**#652**, merged); restore and `desired_state` (**#651**, merged);
    `gc.rs` / `scrub.rs` (**#650**, merged); `backfill.rs` (**#695** — this slice does not touch it,
    which is exactly why it does not depend on that child). The read side generally: no new
    resolving walk, no change to `resolve_chunk_map`, no change to the containment rule
    #695/#696/#697 land. Any new or edited ADR / spec / proposal (0016 is a **draft** and stays
    untouched); any conformance-vector change; any new dependency.
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** The new test MUST NOT name
    `repoint_chunk` or any other symbol this patch introduces. The RED leg reverts production, so
    such a reference makes the target fail to **compile** and the gate reports UNVERIFIABLE
    (exit 77, `run-verify.sh:450`, `:500`) rather than a red. Drive everything through `reconcile_step`
    and observe the **store**. `MAX_VALUE_BYTES` is base-visible and may be named.
  - **Peer callsites Do MAY open — this is a composition slice; mirror them rather than invent a
    shape:** `crates/core/src/metadata.rs:1741-1768` (`commit_chunk_map`, the flat CAS idiom);
    `crates/custodian/src/reconstruction.rs:578-612` and `crates/custodian/src/rebalance.rs:296-330`
    (the two binding commits being replaced, including the `repair::repair_key` delete and the
    `gc::orphan_key` puts that must stay **in the same batch** as the placement change);
    `crates/core/src/metadata.rs:2619-2650` (`resolve_chunk_map` / `ResolvedChunkMap` — what a
    caller holds after a resolve, and therefore what "the exact bytes the resolve read" can mean);
    `crates/core/src/metadata.rs:1230-1300` (`seg_key` / `seg_range_prefix` / `parse_seg_key`, the
    only sanctioned way to address a segment record);
    `crates/custodian/tests/segmented_map_restore.rs:387-431` (`seed_segmented` / `seed_damaged`:
    raw `seg:` + root seeding with a fixture self-check); `crates/dst/tests/custodian.rs` (the
    existing seeded Tier-0 custodian properties, for the shape the new one must match).
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`,
  `cargo-mutants` — the five `[[doctor.checks]]` ids in `pdca.toml` (cite by **id**, not by line — the parent
  brief's line numbers were already stale at review, and the file churns). Named
  because the prose and dependency-wall legs warn-skip locally while CI enforces them
  (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with
  a message naming a flag rather than the stale tool. The DST leg needs no external tool — cargo
  xtask ci supplies the madsim cfg and the seed sweep itself. Nothing else beyond the base Rust
  toolchain: no Docker, no protoc, no live backend, no new dependency.
- **Test file:** `crates/custodian/tests/segmented_map_repoint.rs` — a **NEW** file, not optional,
  completing the `segmented_map_*` family (`_consumers.rs` #650, `_restore.rs` #651, and the three
  per-pass files `_backfill.rs` #695 / `_rebalance.rs` #696 / `_reconstruction.rs` #697). C4-verify
  earns its red only from an **added** `*/tests/*.rs`; appending to an existing file makes the gate
  take the green-only branch (`run-verify.sh:454-464`) and prove no red. The DST property in
  `crates/dst/tests/custodian.rs` and updates to `tests/{reconstruction,rebalance}.rs` ship **in
  addition**; C4-ci covers them, and the modified DST file is **not** compiled by the discriminator
  run (no `--cfg madsim` is imposed on the gate, `run-verify.sh:112-120`). Re-run
  `run-verify.sh --classify` at Plan to confirm the invocation.
- **Difficulty:** high
- **Depends on:** 710
- **Depends on (merged):** 696, 697
- **Conflicts with:** 717
- **Ordering note:** **Wave 2 in this proposal, and additionally gated on tracker issues #696 and
  #697** (rebalance and reconstruction containment) — **not** #695 (backfill), which this slice does
  not touch. Those are proposal-external, so they ride in the `Depends on (merged):` field above —
  the split parser passes it through verbatim (it is not one of its ordering fields) and `flow`
  enforces it via `merged.is_merged`, holding this bundle until their PRs are **merged into the
  base**. Plain `Depends on` would not: both are already COMPLETE bundles with unmerged PRs. This
  slice **completes** the refusal path #696/#697 introduce: its callers are the two sites they
  leave refusing, and it edits the same two production files, so building on a base without them
  would collide on every hunk. Under `wave_mode = "merge"` (pdca.toml:90) with
  `[driver].auto_merge = false` the driver does **not** merge at the wave boundary — it stops and
  the **human** merges the wave's PRs, then re-runs; once merged, the ref C4-verify resolves **is**
  the base the PR opens against and genuinely contains them (INTEGRATION §2, "How `C4-verify`
  resolves the base"). **If #696/#697 are not accepted and merged, hold this bundle** — do not
  rebuild it against a base without them and do not let it absorb their read side; that is
  precisely the un-splitting that closed PR #647. Same rule for child-1 = **#710** (in-batch, the
  wave boundary stop covers it; if this bundle is ever run in a separate later batch, hand-add
  `710` to `Depends on (merged)` first). **Never share a wave with #717** — it carries what was
  #654's, then #692's, `PendingEntry` extension into `metadata.rs` plus `dst/tests/custodian.rs`,
  both in this child's file set, and its brief declares `Conflicts with: 710, 711`. (#692 was
  itself SPLIT on 2026-08-09 into #715 → #716 → #717; only the terminal child **#717** touches
  `metadata.rs`/`dst/tests/custodian.rs`. **#715 and #716 share nothing with this child** —
  they touch only `crates/core/src/multipart.rs` and their own new test files — so they MAY run
  in the same wave. Lineage: the declaration was repointed from `682` at that split's
  acceptance, 2026-08-08, and from `692` to `717` at this one's, 2026-08-09.) **Cite by symbol,
  not by number** — the base WILL have advanced: #696/#697
  rewrite `rebalance.rs` and `reconstruction.rs`, child-1 edits both plus `metadata.rs`, and #717
  (if it lands first) inserts two fields into `PendingEntry` at `metadata.rs:1528`, shifting every
  citation below it. The constants at `:322`–`:354` sit above the insertion and are unaffected.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/core/src/metadata.rs` → most
  recently `b083ec4` (#652), `11aa85f` (#650), `99c7fcf` (#649, the shared resolver — the premise
  this builds on), `bbdb7c5` (#648 follow-up), `3e05891` (#648, the segmented record shape). None
  implements a placement move in a `seg:` record. `git -C ../wyrd log origin/main --
  crates/custodian/src/{reconstruction,rebalance}.rs` → the repair/evac loops (#144/#145) and their
  fixes (#197 *"don't count aborted repairs as successes"*, PR #238; #346 identity-placement
  fallback; #348 malformed placement). No open PR touches these paths. **Closed/rejected:** PR
  **#647** (CLOSED 2026-07-30, unmerged) is the un-split ancestor and contained a `repoint`-shaped
  write; it was closed for **size and reviewability**, not direction. Its custodian-local
  `crates/custodian/src/resolve.rs` has been superseded by the shared resolver — **do not
  reintroduce it.** Within the harness, `results/issue_638/review-rejected.md:15-16` records the
  standing, four-times-rejected rule that a losing/late write is **not** retracted; do not
  re-litigate it.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized for one Do attempt: 7 files against the brief's hard 6-file budget, both mutually-exclusive optional test files edited instead of one, and 124 KB against the 100 KB size backstop (which itself recommended iterate-plan). Two build attempts already converged on this same oversized shape rather than a smaller one, so a further iterate-do would likely repeat it. Additionally, the adversary review found a real, unaddressed gap — the chunk == prior race guard (the only defence against a read/prepare race during repair) has zero test coverage; deleting it left the whole suite green. Return to Plan to author a split (pdca split 711) that separates the repoint primitive from its two callers / the DST property, and have the split brief require a leg that proves the chunk == prior guard.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 36 mutants tested in 83s: 17 missed, 6 caught, 13 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_711/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
