# Brief — issue 682 / repoint-chunk-ceiling-safe-placement-moves

> Slice **4c of 7** of the #635 re-slicing (proposal 0016 decisions 2 and 7). The **write**
> primitive the repair and evacuation passes need once #696/#697 have taught them to read: move a
> fragment's placement in whichever record holds the chunk's `ChunkRef`, without ever growing that
> record past the ceiling that would make it permanently un-overwritable.
> **Depends on #696 (rebalance) + #697 (reconstruction)** — the two passes that call this must
> already resolve and contain. *(#681 was SPLIT on 2026-08-06 into #695 backfill / #696 rebalance /
> #697 reconstruction; this slice touches only the latter two, so it does NOT depend on #695.)*
> Tracker: https://github.com/getwyrd/wyrd/issues/682.

- **Slug:** repoint-chunk-ceiling-safe-placement-moves
- **Defect:** Two permanent states, both of which C-1 rules out as costs.
  1. **A chunk that lives in a `seg:` record can never be repaired or evacuated.** #695/#696/#697
     stop the three maintenance passes aborting on a segmented object, but they deliberately write
     nothing:
     a repair obligation or a drain evacuation for a `seg:`-resident chunk is **refused and stays
     queued**, every pass, forever. Nothing exits that state — the obligation is not drained (which
     would be data loss), and no code path can move the placement, because the only placement
     writers in the tree rebuild an **inode** record: `reconstruction::repair_chunk` builds
     `plan.prior.chunk_map.as_flat()?.to_vec()` and CASes the inode
     (`crates/custodian/src/reconstruction.rs:578-612`), and `rebalance::evacuate_chunk` does the
     same (`crates/custodian/src/rebalance.rs:255-330`). Neither can address a `seg:<nonce>:<epoch>:<index>`
     record at all. So a multipart-published object's redundancy decays untended and a D-server
     decommission holding one of its fragments never converges.
  2. **A repair may grow a record past the backend value ceiling, and a record past it is
     permanently un-overwritable.** Every mutation in `crates/core/src/metadata.rs` is
     `require(key, encode(prior))` + `put(key, encode(next))` — a full-value CAS. A record whose
     encoded bytes exceed `MAX_VALUE_BYTES` (100 000, `metadata.rs:327`) is refused by the tightest
     backend on the `put`, and thereafter **every** repair of that object fails: *"a root that
     cannot be re-written is an object whose placement can never be repaired"* — the tree already
     says so at `metadata.rs:334-352`, and then does not check it anywhere on the repair path. A
     placement move is a real growth vector: `placement: Vec<DServerId>` re-encodes each moved
     entry, and a small id (`1`) replaced by a large one (`18446744073709551615`) adds ~19 bytes per
     fragment, ×9 fragments per RS(6,3) chunk. There is **no** ceiling check in
     `reconstruction::repair_chunk`, `rebalance::evacuate_chunk` or `backfill::reconcile` today
     (grepped: `MAX_VALUE_BYTES` has exactly three **code** uses — `metadata.rs:327` the definition,
     `:354` the const assertion, `:2465` the resolver's read-side refusal; every other hit is a doc
     reference. None is on a write path). A repair that crosses the
     ceiling therefore succeeds once and bricks the object's future repairs — capacity spent as
     durability.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs` passes,
  driven **only** through symbols visible on the base (post-#696/#697) — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, inode_key, encode, decode, MAX_VALUE_BYTES, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord, ChunkRef, EcScheme}` — over
  in-memory `MetadataStore` / `ChunkStore` doubles. Five legs:
  1. **A `seg:`-resident under-replicated chunk is repaired.** Seed a committed **segmented** object
     (raw `seg:` records + a segmented root, never a committer) whose chunk has lost a fragment,
     enqueue its repair, run `reconcile_step` with a `ReconstructionContext`. Assert: the rebuilt
     fragment is on a healthy D server in a distinct failure domain; the `seg:` record's
     `ChunkRef.placement` now names it; the repair obligation is **drained**
     (`queued_repairs` no longer contains it); the pass answers `Changed`; and the **root** record's
     bytes are unchanged except as the move itself requires. Base behaviour: refused, obligation
     still queued, `seg:` bytes unchanged → **red**.
  2. **A `seg:`-resident fragment is evacuated off a draining server.** Same fixture shape with
     `set_lifecycle(.., Draining)` on the server holding a fragment; run `reconcile_step` with a
     `RebalanceContext`. Assert the fragment is copied to a non-draining server in a distinct
     domain, the `seg:` record names it, the vacated position is orphan-marked, and the pass answers
     `Changed`. Base: refused, placement unchanged → **red**.
  3. **A repoint that would cross the value ceiling is refused, not persisted — over a FLAT
     record.** This is the leg that is red on the base for a *behavioural* reason today, with no
     dependency on (1)/(2): hand-seed a committed **flat** root whose encoded length is just under
     `MAX_VALUE_BYTES`, holding a chunk placed on small-id D servers, and arrange a repair whose new
     placement uses large `u64` ids so the re-encoded record crosses the ceiling. Assert: the record
     is **byte-identical** afterwards, the obligation **stays queued**, the pass does **not** answer
     `Satisfied`, and the refusal is named on the audit seam. Base behaviour: the oversized record
     is committed (the CAS has no ceiling check), so `get(inode_key)` returns bytes whose length
     **exceeds `MAX_VALUE_BYTES`** → **red**. **Assert the stored byte length, not a downstream
     un-repairability**: an in-memory `MetadataStore` double has no value ceiling and will happily
     hold the oversized value, so "the object is now un-repairable" is *not* observable through it.
     If the leg wants to show the consequence too, give the double an explicit ceiling — a `put`
     over `MAX_VALUE_BYTES` returns the backend's refusal — and assert a **second** ordinary repair
     of that object then fails pre-fix and succeeds post-fix. That is the stronger shape and it
     models the real backend; the binding assertion either way is the stored length.
  4. **The same refusal over a segmented record.** A `seg:` record seeded just under the ceiling
     whose repoint would cross it: refused, record byte-identical, obligation queued, pass
     non-certifying. (This leg is **not** independently red on the base — pre-fix the move is
     refused for the *other* reason. It ships because it pins the post-fix rule for the segmented
     arm, which (3) cannot; do not count it as discriminating evidence.)
  5. **A refused or failed move is subtracted, never certified.** Two conjunctions:
     - an evacuation that does not persist (refused by the ceiling, or aborted for want of a free
       distinct domain) leaves the fragment on the draining server, and the pass MUST NOT answer
       `Satisfied` while `reconciliation_status` for that server is not converged;
     - the documented `repaired − conflict − aborted` accounting must not let a **refused** repair
       inflate reported successes: assert the emitted success identity over a pass mixing one
       repaired, one refused and one aborted chunk.
  6. **Two committed references to the same `ChunkId` get one plan, not independent ones.** Seed two
     committed objects whose maps both name the same `ChunkId`, with a repair queued for it. Assert
     the pass does not repoint or overwrite the same `FragmentId`s twice and does not orphan copies
     the other object still references — neither object is left naming a fragment that was
     reclaimed.

  Legs (1), (2), (3) and (5) are binding. **Additionally**, the DST **repoint-versus-supersede**
  property ships in the **existing** `crates/dst/tests/custodian.rs` (a new `crates/dst/tests/*.rs`
  would put `#![cfg(madsim)]` on the C4-verify invocation and change what the gate compiles): a
  repoint whose pinned root generation **or** segment bytes changed under it commits **nothing** —
  neither the placement nor any orphan mark — and the object is left naming a fragment that exists.
  Assert it across the seed sweep, in **both** interleavings (repoint wins before the supersede's
  inode CAS; repoint loses after it). C4-ci runs it; it is not the C4-verify discriminator.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency.
  - **Base.** `PDCA_BUNDLE=results/issue_682 ./engine/scripts/run-verify.sh --print-base` →
    `origin/main` (run at Plan). Under `wave_mode = "merge"` (pdca.toml:90) the PRs for #696 and
    #697 are merged into `origin/main` before this wave builds, so the ref C4-verify resolves **is**
    the base the PR opens against and **does** contain them (INTEGRATION §2, "How `C4-verify`
    resolves the base").
    No `Onto branch`, no `stack-base` marker in the bundle.
  - **Leg (3) does not depend on #696/#697 at all** — it is red against the *current* `origin/main`
    (`339da46`) as well, because the missing ceiling check is orthogonal to the segmented read side.
    That is deliberate: it guarantees this bundle has at least one behavioural red even if the fold
    lands differently than planned.
  - **Discriminator classification — DRY-RUN at Plan, not assumed.** C4-verify earns its red only
    from an **added** `*/tests/*.rs` (`run-verify.sh:97`, `:98`). `run-verify.sh --classify` on a
    synthetic patch listing the expected file set (`crates/core/src/metadata.rs`,
    `crates/custodian/src/{reconstruction,rebalance}.rs`, an added
    `crates/custodian/tests/…​.rs`, a modified `crates/dst/tests/custodian.rs`) returned exactly one
    `ADDED_TEST` plus `CRATE crates/core`, `CRATE crates/custodian`, `CRATE crates/dst` — so the
    invocation is `cargo test -p wyrd-custodian --test segmented_map_repoint`, the modified DST file
    is **not** compiled by the discriminator run (no `--cfg madsim` is imposed on the gate,
    `run-verify.sh:112-120`), and the RED leg reverts `metadata.rs` and both custodian sources while
    keeping the one added test.
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** It MUST NOT name `repoint_chunk`,
    the ceiling helpers, or any other symbol this patch introduces. The RED leg reverts production,
    so such a reference makes the target fail to **compile** and the gate reports UNVERIFIABLE
    (exit 77, `run-verify.sh:487-497`) rather than a red. Drive everything through `reconcile_step`
    / `backfill::reconcile` and observe the **store**. `MAX_VALUE_BYTES` is base-visible
    (`metadata.rs:327`) and may be named.
  - **No vacuous green.** No `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]`
    (grepped on the base), so neither zero-test guard trips (`run-verify.sh:445`, `:481`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the placement move that transfers a durable byte's
  protection from one position to another**. Sourced, not intuited: `docs/principles.md:137` (§6 row
  *Storage lifecycle / reclamation*), sourced to §5 C-1 (`docs/principles.md:109`), the maintainer's
  standing rule of 2026-07-25, `0016:2802-2813`, `gc.rs:22-25`. Over that category:
  - **Every chunk a committed object references is repairable, whatever record shape holds it.**
    "This chunk's `ChunkRef` lives in a record class the repair path cannot address" is a state
    nothing exits: the obligation is never dischargeable and the redundancy never returns.
  - **A repair may never make an object un-repairable.** A record grown past the value ceiling can
    never be CAS-overwritten again, so its placement can never be moved again — a permanent state
    reached *by the very operation meant to restore durability*. It is refused, not persisted;
    capacity is a tradeable cost, permanence is not.
  - **A move is one atomic transfer of protection.** The placement change, the discharge of the
    obligation that licensed it, and the evidence for the vacated position are one commit. Split
    across two, either the byte is unprotected and unevidenced, or the obligation is discharged
    without the move.
  - **A losing move writes nothing.** A refusal or a lost CAS leaves the store exactly as it found
    it. It does **not** retract bytes already published to a D server — those are collectable
    garbage on GC's own terms, which is settled and was rejected four times over in #638
    (`results/issue_638/review-rejected.md:15-16`) — and it does not leave a half-applied record.
  - **A pass that refused or failed a move does not certify.** Reporting `Satisfied` while a
    fragment still sits on a draining server tells an operator the box is safe to remove.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- **Depends on:** 696, 697
- **Conflicts with:** *(none beyond the dependencies above)*
- **Ordering note:** **Wave 1.** `Depends on 681` is a genuine build-on dependency in both
  directions that matter: (a) this slice **completes** the refusal path #681 introduces — its
  callers are the two sites #681 leaves refusing — and (b) it edits
  `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs`, the same two
  production files #681 rewrites, so building the two on one base would collide on every hunk.
  Under `wave_mode = "merge"` #681's PR is merged to `origin/main` at the wave boundary, so this
  bundle builds and verifies on a tree that genuinely contains it. **If #681 is not accepted, hold
  this bundle** — do not rebuild it against a base without #681 and do not let it absorb #681's read
  side; that is precisely the un-splitting that closed PR #647. The other wave-1 bundle is #655,
  which shares no file with this one (`crates/core/src/multipart.rs` vs
  `crates/core/src/metadata.rs`), so the two build in parallel safely. Note the wave-0 bundle #654
  makes a **two-field** addition to `PendingEntry` in `crates/core/src/metadata.rs` — a different
  region of the file, landed and merged before this wave builds, so it is a rebase fact, not a
  conflict; cite by symbol rather than by line in that file.
- **Surfaces:** data
- **Difficulty:** high   (a new public primitive in `crates/core/src/metadata.rs` — the workspace's
  most load-bearing module — plus ceiling helpers, plus both custodian call-sites replaced at their
  **binding commit**, plus a DST property. Cross-crate, and every one of those commits is the point
  at which a durable byte's protection transfers. A diff-reviewer must hold the CAS shape, the two
  record classes, the outcome accounting and the supersede race in view at once.)
- **Scope:** give the repair and evacuation passes a **ceiling-safe, exact-bytes placement move**
  that works in whichever record holds the chunk, and switch both callers onto it.
  - `crates/core/src/metadata.rs` — `repoint_chunk`: move one chunk's placement in the record that
    holds its `ChunkRef` — flat inode **or** segment record. Both arms pin the **exact bytes the
    resolve read**: the root generation, and additionally, for a segmented map, the segment record.
    A stale-generation write is a `Conflict`, never a silent overwrite. The refusal and the
    conflict paths write **nothing at all**.
  - `crates/core/src/metadata.rs` — the record-ceiling checks: a repoint whose re-encoded record
    would cross the backend value ceiling is **refused and not persisted**, and the refusal is
    distinguishable by the caller from a lost CAS (they mean different things to an obligation:
    one is "never retry this shape", the other is "retry next pass"). Carve out **only** the ceiling
    helpers `repoint_chunk` needs — not the committer around them.
  - `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs` — the two
    callers stop refusing a `seg:`-resident chunk (#696's and #697's placeholder) and complete the move
    through the new primitive. The placement change, the discharge of the repair obligation, and
    the orphan evidence for each displaced position stay **one commit** — do not split the batch to
    fit the new primitive; if the primitive's shape makes that awkward, change the primitive.
  - `crates/dst/tests/custodian.rs` — the repoint-versus-supersede property, added to the
    **existing** file.

  **Constraints carried forward (blockers found on the old #651 — must not recur; these bound the
  shape, they do not name it):**
  - **Refused outcomes are subtracted from the success count.** The documented
    `repaired − conflict − aborted` calculation must not let a refused repair inflate reported
    successes, and every failed evacuation is non-certifying: an `Aborted` that leaves the placement
    on a draining server must not report `Satisfied` while the drain status is not converged. This
    is where the **pre-existing** silent `EvacOutcome::Aborted => {}` arm
    (`crates/custodian/src/rebalance.rs:128`) is settled — #696 deliberately left it to this slice.
  - **Duplicate chunk ids get one plan, not independent ones.** Two committed references to the same
    `ChunkId` must not repoint or overwrite the same `FragmentId`s and orphan copies the other
    object still references. Keep this to the narrow rule; do **not** rebuild the cross-object
    claim-counting apparatus dropped at #651's replan.
  - **Bounded memory.** The move pins the bytes of **one** record at a time. Do not retain the
    namespace's decoded chunks, and do not deep-copy a segmented root into every plan
    (O(chunks × segments)).
  - **A losing CAS does not retract already-published bytes** — settled, rejected 4× in #638
    (`results/issue_638/review-rejected.md:15-16`). The refusal path writes nothing at all.

  **Out of scope:**
  - **The committer, the destination pre-mark, the drain fence, rollback and resume (#653).**
    Proposal 0016's full segment-repoint precondition set (`0016:669`) is
    `require(seg == prior)` + `require(inode == prior)` + `require(orphan:<P_new> == prior)` (the
    destination pre-mark) + `require_absent(desired:dserver:<S_new>)` (the drain fence), bounded by
    `W_repoint`. **This slice ships only the first two.** That is the issue's own carve-out and it
    is a **pre-declared sign-off item, not a surprise NEEDS-HUMAN**: without the pre-mark, a repoint
    that loses its CAS leaves the pre-written destination fragment unreferenced — which is exactly
    the behaviour the **flat** path already has and documents today
    (`crates/custodian/src/reconstruction.rs:610-614`, `crates/custodian/src/rebalance.rs:325-329`:
    *"the rebuilt fragments are collectable garbage"*), reclaimed by GC's ordinary unreferenced
    sweep. So this slice **introduces no new stranding class**; it extends an existing, settled one
    to a second record shape. The pre-mark and the drain fence tighten it for the multipart-era
    races and are #653's. Do **not** implement them here.
  - The chunk-id floor (**#652**, merged); restore and `desired_state` (**#651**, merged);
    `gc.rs` / `scrub.rs` (**#650**, merged); `backfill.rs` (**#695** — this slice does not touch it,
    which is exactly why it does not depend on that child;
    a backfill fill that would cross the ceiling is a real gap, but it is a *different* write path
    and belongs to whichever slice owns it next, not to a widened diff here).
  - The read side generally: no new resolving walk, no change to `resolve_chunk_map`, no change to
    the containment rule #695/#696/#697 land.
  - Any new or edited ADR / spec / proposal (0016 is a **draft** proposal and stays untouched); any
    conformance-vector change; any new dependency.
- **Budget:** ≤ **700** added semantic lines (non-blank, non-comment, non-mechanical), ≤ **7**
  files, named so the cap is an allocation rather than a race: `core/src/metadata.rs`,
  `custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs`,
  `custodian/tests/segmented_map_repoint.rs` (**new**), `dst/tests/custodian.rs`,
  `custodian/tests/reconstruction.rs`, `custodian/tests/rebalance.rs`. An **eighth** file means the
  shape is wrong — in particular, needing to edit `backfill.rs`, `restore.rs`, `gc.rs` or
  `desired_state.rs` means the scope has drifted: STOP and hand back a proposed split.
- **Repro instruction:** on the target checkout, read the two binding commits with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` (`:578-612`) and
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` (`:296-330`) — both rebuild an
  **inode** record and can address no `seg:` record. For the ceiling: `git -C ../wyrd grep -n
  MAX_VALUE_BYTES -- crates/core/src/metadata.rs` returns exactly `:327` (the constant), `:354` (the
  const assertion) and `:2465` (the resolver's read-side refusal) — no write path checks it. Seed a
  committed flat root just under 100 000 encoded bytes and run a repair that moves a placement onto
  large-id D servers: the oversized record commits.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — the five doctor.checks ids (pdca.toml :696, :703, :711, :733, :740), all OK on this host at Plan (scripts/pdca doctor). Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool. The DST leg needs no external tool — cargo xtask ci supplies the madsim cfg and the seed sweep itself. Nothing else beyond the base Rust toolchain: no Docker, no protoc, no live backend, no new dependency.
- **Test file:** `crates/custodian/tests/segmented_map_repoint.rs` — a **NEW** file, not optional,
  completing the `segmented_map_*` family (`_consumers.rs` #650, `_restore.rs` #651, and the three
  per-pass files `_backfill.rs` #695 / `_rebalance.rs` #696 / `_reconstruction.rs` #697 — note the
  single `_passes.rs` this brief previously named was never built; #681 was split before Do). C4-verify earns its red only from an **added** `*/tests/*.rs`; appending to an existing
  file makes the gate take the green-only branch (`run-verify.sh:454-464`) and prove no red.
  Confirmed by the `--classify` dry-run above. The DST property in `crates/dst/tests/custodian.rs`
  and updates to `tests/{reconstruction,rebalance}.rs` ship **in addition**; C4-ci covers them.
- **Verification posture:** default for the C4-verify discriminator — assertion-red on the base,
  green with this patch, both at Check. The **DST** property is a supplementary leg run by C4-ci's
  `cargo xtask ci` (which includes the seeded sweep, INTEGRATION §3); it is built and exercised at
  Check, not deferred.
- **Production reach:** the live repair and evacuation paths traverse this seam at Check — the
  discriminator drives `reconcile_step`, not a stand-in. The one boundary to declare is the
  precondition set above: `repoint_chunk` pins the root generation and the segment bytes, and does
  **not** yet carry 0016's destination pre-mark or drain fence (#653). The consequence is bounded
  and pre-existing: a lost CAS leaves an unreferenced destination fragment for GC's ordinary sweep,
  identically to the flat path today.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line number
  in this brief was verified against `origin/main` at `339da46` during the Plan verification pass.
  **Cite by symbol, not by number — the base WILL have advanced when this bundle builds:** #697
  and #696 (both wave 0) rewrite `reconstruction.rs` and `rebalance.rs` respectively, and #654 (wave 0) inserts two
  fields into `PendingEntry` at `crates/core/src/metadata.rs:1528`, shifting every citation in that
  file **below** it (`:1741`, `:2465`, `:2619`, `:2652`) by a few lines. The constants at `:322`–
  `:354` sit above the insertion and are unaffected.
  **Peer callsites Do MAY open — this is a composition slice; mirror them rather than invent a
  shape:**
  - `crates/core/src/metadata.rs:1741-1768` — `commit_chunk_map`: the exact CAS idiom a repoint's
    **flat** arm must keep — `require(key, encode(prior))` + `put(key, encode(next))`,
    `version = prior.version + 1`, and `..prior.clone()` so ADR-0047 object metadata is
    **preserved** (a placement-maintenance commit must not move `Last-Modified` or drop the content
    type). Note its own segmented refusal at `:1748-1753` stays — `commit_chunk_map` is not what
    this slice changes.
  - `crates/custodian/src/reconstruction.rs:578-612` and
    `crates/custodian/src/rebalance.rs:296-330` — the two binding commits being replaced, including
    the `repair::repair_key` delete and the `gc::orphan_key` puts that must stay **in the same
    batch** as the placement change.
  - `crates/core/src/metadata.rs:324-354` — `MAX_VALUE_BYTES` / `MAX_ROOT_VALUE_BYTES` and the
    `const` assertion tying them, plus the doc comment at `:337-352` that already states the
    invariant this slice enforces (*"a root that cannot be re-written is an object whose placement
    can never be repaired"*). Enforce the ceiling that is actually normative for the record being
    written; do not invent a third constant.
  - `crates/core/src/metadata.rs:2619-2650` — `resolve_chunk_map` / `ResolvedChunkMap`: what a
    caller holds after a resolve, and therefore what "the exact bytes the resolve read" can mean.
    `crates/core/src/metadata.rs:1230-1300` — `seg_key` / `seg_range_prefix` / `parse_seg_key`, the
    only sanctioned way to address a segment record.
  - `crates/custodian/tests/segmented_map_restore.rs:387-431` — `seed_segmented` / `seed_damaged`:
    raw `seg:` + root seeding with a fixture self-check. `crates/dst/tests/custodian.rs` — the
    existing seeded Tier-0 custodian properties, for the shape the new one must match.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/core/src/metadata.rs` → most
  recently `b083ec4` (#652), `11aa85f` (#650), `99c7fcf` (#649, the shared resolver — the premise
  this builds on), `bbdb7c5` (#648 follow-up), `3e05891` (#648, the segmented record shape). None
  implements a placement move in a `seg:` record; none adds a write-side ceiling check.
  `git -C ../wyrd log origin/main -- crates/custodian/src/{reconstruction,rebalance}.rs` → the
  repair/evac loops (#144/#145) and their fixes (#197 *"don't count aborted repairs as successes"*,
  PR #238 — directly relevant to constraint (1); #346 identity-placement fallback; #348 malformed
  placement). No open PR touches these paths (`gh pr list --state open` → empty). **Closed/rejected:**
  PR **#647** (CLOSED 2026-07-30, unmerged) is the un-split ancestor and contained a `repoint`-shaped
  write; it was closed for **size and reviewability**, not direction. Its custodian-local
  `crates/custodian/src/resolve.rs` has been superseded by the shared resolver — do not reintroduce
  it. Within the harness, `results/issue_638/review-rejected.md:15-16` records the standing,
  four-times-rejected rule that a losing/late write is **not** retracted; do not re-litigate it.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
