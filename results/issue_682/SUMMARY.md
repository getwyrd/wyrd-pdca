# Result — issue 682 / repoint-chunk-ceiling-safe-placement-moves

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Two permanent states, both of which C-1 rules out as costs.
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
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_repoint.rs` passes,
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
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- Scope: give the repair and evacuation passes a **ceiling-safe, exact-bytes placement move**
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
