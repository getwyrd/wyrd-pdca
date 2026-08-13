<!-- pdca:split-proposal v1 -->
# Split proposal — issue 682

## Why this slice is oversized

The brief names **two defects under one invariant**, and they are joined by nothing but the
invariant. C-1 is cited over the whole category ("the placement move that transfers a durable
byte's protection from one position to another"), which is true — but a shared *principle* is
not a shared *outcome*, and this is exactly the merge that closed PR #647 for size and
reviewability. The two halves have **disjoint red evidence**, which is the tell:

1. **A repair may grow a record past the value ceiling, and a record past it is permanently
   un-overwritable.** Verified on `origin/main` while writing this proposal:
   `MAX_VALUE_BYTES` has exactly one behavioural use, the *read*-side refusal at
   `metadata.rs:2465`; the rest are the constant (`:327`), the const assertion (`:354`) and doc
   references. **No write path checks it.** Alongside it sits the certification defect: at
   `crates/custodian/src/rebalance.rs:128` the arm is literally `EvacOutcome::Aborted => {}`,
   and the loop then answers `Reconciled::Satisfied` when nothing changed — telling an operator
   a box is safe to pull while a fragment still sits on it. Both of these are red against
   **today's** `origin/main` (`339da46`). Neither has anything to do with segmented records.
2. **A chunk that lives in a `seg:` record can never be repaired or evacuated.** Red only on a
   base that already contains #696 and #697 — the two passes must first *resolve* and *contain*
   a segmented object before there is a refusal for this slice to complete. The work is a new
   public primitive (`repoint_chunk`) in `crates/core/src/metadata.rs`, the workspace's most
   load-bearing module, with two record-class arms, exact-bytes pinning of both the root
   generation and the segment record, both custodian call-sites replaced at their **binding
   commit**, and a DST supersede-race property.

The brief itself already draws the seam and explains why, at *Falsifiability*: "**Leg (3) does
not depend on #696/#697 at all** — it is red against the current `origin/main` as well, because
the missing ceiling check is orthogonal to the segmented read side. That is deliberate: it
guarantees this bundle has at least one behavioural red even if the fold lands differently than
planned." A leg carried as *insurance* against a dependency landing late is a leg that wants to
be its own bundle. Split, it stops being insurance and becomes a slice that ships now.

The ordering falls out of the ceiling rule rather than out of tidiness. C-1 says a repair may
never make an object un-repairable — and the segmented arm has the **same** growth vector as the
flat one (`placement: Vec<DServerId>` re-encoded, ~19 bytes per moved entry — the multiplier is
however many fragments one repair moves).
So the guard must exist *before* a second record class starts moving placements through it, or
child-2 ships a fresh instance of the very defect child-1 closes.

A note on what is **not** a seam here, so the human can discount it: splitting the primitive
from its callers. `repoint_chunk` with no caller fixes no defect and earns no behavioural red —
it would be a patch, not a slice. Likewise the DST property and the duplicate-`ChunkId` rule are
properties *of* the segmented move; they have no standalone defect and stay with it.

Two children, not more. If child-2 is still judged oversized at its own Plan, the honest next
seam is **by caller** — `reconstruction.rs` (repair, brief leg 1) before `rebalance.rs`
(evacuation, brief leg 2), sharing the primitive, the second stacking on the first. That keeps
the brief's "one commit" rule intact, since the batch is per-caller. I do not propose it now:
it costs a third full cycle for a division the primitive itself may not need.

## Wave sketch

**Wave 1 — child-1, gated on the #696/#697 merges.** Its *red* is independent of them — legs
(1) and (2) are red on today's `origin/main` (`339da46`) — but its **diff** is not: the accepted,
unmerged PRs for #696 and #697 rewrite the two custodian passes wholesale, spanning the very
outcome `match`es child-1 edits (#696's diff even annotates the `Aborted => {}` arm with
"deferred: #682"). Built before those merge, child-1 conflicts at fold with two already-accepted
PRs. So child-1 carries `Depends on (merged): 696, 697` and builds the moment those PRs are
merged — in parallel with any bundle sharing no file (the parent names #655,
`crates/core/src/multipart.rs`, as exactly that). The reds survive the merge: #696 explicitly
defers the `Aborted` arm to #682, and neither PR adds a write-side ceiling check.

**Wave 2 — child-2, stacked on child-1.** `Depends on: child-1` for two independent reasons,
both load-bearing:

- **Build-on.** Child-2's segmented arm must be ceiling-guarded by construction, and the guard —
  the helpers in `metadata.rs`, and the `Refused` outcome that is distinguishable from a lost CAS
  — is child-1's deliverable. Built on a base without it, child-2 either re-invents the guard
  (two conflicting implementations of one rule) or ships the seg arm unguarded, reintroducing
  defect (1) in a second record class. The brief's own leg (4) is the assertion that the rule
  holds over segmented records — it *presupposes* the rule exists.
- **File collision.** Both children edit the same three production files — `core/src/metadata.rs`,
  `custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs` — and in the custodian pair they
  edit the *same region*: the outcome `match` and the binding commit sit within a few lines of each
  other (`rebalance.rs:125-130` vs `:296-330`). On one base they would collide on nearly every
  hunk. Under `wave_mode = "merge"` (pdca.toml:90) with `[driver].auto_merge = false`
  (pdca.toml:100; commit `8086d3a`) the driver does **not** merge at the wave boundary — it
  **stops**, the **human** merges child-1's PR, and re-runs; child-2 then builds and verifies on
  a tree that genuinely contains it. If child-2 is instead run in a separate later batch,
  hand-add child-1's real id to its `Depends on (merged)` first — the proposal cannot pre-write
  an id that does not exist yet.

Because the dependency already serialises them, no `Conflicts with:` edge is needed between the
two — the field is for pairs that must not share a wave despite neither needing the other's
outcome, and that is not this pair.

**How the external #696/#697 gate is machine-enforced.** The proposal-local `Depends on:` field
takes sibling labels only — `pdca split --accept` refuses tracker ids there (`split.py`,
`_validate_ordering`). The harness has a stricter field for exactly this case:
`- **Depends on (merged):** <id, …>` (`brief.py`, `depends_on_merged`), enforced by `flow` via
`merged.is_merged` — the bundle is held until the prerequisite's PR is **merged into the base**,
not merely COMPLETE. That field is not one of the split parser's `ORDERING_FIELDS`, so it passes
through `--accept` verbatim into the materialised briefs. **Both children carry it for 696, 697.**
The distinction is load-bearing: #696/#697 are already COMPLETE bundles with **unmerged** PRs, so
a plain `Depends on` would wave them through and the child would build on a base missing their
diffs. The parent's hold rule survives unchanged — if #696/#697 are not accepted and merged,
neither child builds, and neither absorbs their read side; that is precisely the un-splitting
that closed PR #647.

**Conflict with #692 (was: the #654 "rebase fact").** The parent's #654 story is stale: #654 was
itself SPLIT (`results/issue_654/close-disposition`), never lands, and its `PendingEntry`
two-field extension is carried by its child **#692** — unbuilt, and its brief already declares
`Conflicts with: 682` (`results/issue_692/brief.md:80`) over the shared files
`core/src/metadata.rs` and `dst/tests/custodian.rs`. Both children here edit `metadata.rs`
(child-2 also `dst/tests/custodian.rs`), so the conflict survives this split and must follow the
ids: **post-accept, repoint #692's `Conflicts with: 682` at the two new child ids** (682 closes
as split and the reference dangles). External ids cannot ride in this proposal's
`Conflicts with:` fields (same sibling-labels rule), so that is a named post-accept step for the
human — the mirror of the one #654's own proposal records in the reverse direction.

**Budget and ownership honesty.** The children's semantic-line budgets are ≤250 + ≤450 = the
parent's ≤700. The file union is **8** named files against the parent's 7: the extra file is
`custodian/tests/placement_ceiling.rs`, the second discriminator — a split produces one
C4-verify red per child by construction, so one added test file per child is the structural cost
of splitting, not scope growth. And the parent assigned the refused-outcome accounting and the
silent-`Aborted` settlement to the single repoint slice; this split deliberately reallocates
them to child-1 (they are flat-path behaviours with flat-path reds, and #696 deferred them to
#682, not to a segmented slice). Child-2 consumes them — the reallocation *is* the split, not a
contradiction of it.

<!-- pdca:child child-1 -->
- **Slug:** ceiling-refused-placement-writes-do-not-certify
- **Defect / goal:** Two write-path defects on the **flat** repair/evacuation path, both
  permanent-state costs C-1 rules out, both red on today's `origin/main` (`339da46`).
  1. **A repair may grow a record past the backend value ceiling, and a record past it is
     permanently un-overwritable.** Every mutation in `crates/core/src/metadata.rs` is
     `require(key, encode(prior))` + `put(key, encode(next))` — a full-value CAS. A record whose
     encoded bytes exceed `MAX_VALUE_BYTES` (100 000, `metadata.rs:327`) is refused by the
     tightest backend on the `put`, and thereafter **every** repair of that object fails. The
     tree already states the invariant at `metadata.rs:334-352` — *"a root that cannot be
     re-written is an object whose placement can never be repaired"* — and then checks it
     nowhere on a write path. Verified: `MAX_VALUE_BYTES` has exactly one behavioural use, the
     resolver's **read**-side refusal at `:2465`; the others are the constant (`:327`), the const
     assertion (`:354`) and doc references. A placement move is a real growth vector —
     `placement: Vec<DServerId>` re-encodes each moved entry, and a small id (`1`) replaced by a
     large one (`18446744073709551615`) adds ~19 bytes per moved entry (only the rebuilt
     positions are re-encoded — the multiplier is however many fragments one repair moves). A
     crossing repair therefore either commits an oversized record (a store without native
     enforcement — thereafter un-overwritable on the tightest backend) or surfaces as a raw
     backend `Err` indistinguishable from a transient fault (a store with it). In neither case
     is there a *refusal* — classified, persisting nothing, distinguishable from a lost CAS:
     capacity spent as durability.
  2. **A move that did not persist still certifies, and can inflate reported successes.** At
     `crates/custodian/src/rebalance.rs:128` the arm is `EvacOutcome::Aborted => {}` — silent —
     and the loop answers `Reconciled::Satisfied` when nothing changed, reporting a drain
     converged while a fragment still sits on the draining server. That tells an operator the box
     is safe to remove. `reconstruction.rs` already carries the offsetting seam from #197
     (`emit_repaired` at `:257` fires before the outcome match, offset by `emit_conflict` /
     `emit_aborted`, documented at `:714-715` as `repaired − conflict − aborted`); the new
     **refused** outcome this child introduces must join that identity rather than inflate it.

  These two defects are one rule — **a move that did not persist neither certifies nor counts** —
  and the weld is structural, not rhetorical: introducing the ceiling refusal *forces* the
  certification/accounting decision, because a `Refused` arm added beside a still-silent
  `Aborted => {}` would re-create defect (2) for the new outcome on the day it is born. Both land
  in the same crate-private outcome `match`es and the same documented accounting seam; a third
  cycle to separate them would buy two colliding PRs over the same arms.
- **Success criterion:** the NEW file `crates/custodian/tests/placement_ceiling.rs` passes,
  driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{inode_key, encode, decode, MAX_VALUE_BYTES, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Three legs:
  1. **A repoint that would cross the value ceiling is refused, not persisted.** Hand-seed a
     committed **flat** root whose encoded length is just under `MAX_VALUE_BYTES`, holding a chunk
     placed on small-id D servers, and arrange a repair whose new placement uses large `u64` ids
     so the re-encoded record crosses the ceiling. Assert: the record is **byte-identical**
     afterwards, the obligation **stays queued**, the pass does **not** answer `Satisfied`, and the
     refusal is named on the audit seam. Base behaviour: the oversized record is committed (the CAS
     has no ceiling check), so `get(inode_key)` returns bytes whose length **exceeds
     `MAX_VALUE_BYTES`** → **red**.
     **Assert the stored byte length, not a downstream un-repairability**: an in-memory
     `MetadataStore` double has no value ceiling and will happily hold the oversized value, so "the
     object is now un-repairable" is *not* observable through it. Do **not** copy the parent
     brief's optional two-phase demo ("commit once, then a second repair fails"): with a
     ceiling-enforcing double the *first* crossing write is already refused by the store, so that
     sequence demonstrates nothing — the two store models cannot share one narrative. The coherent
     supplementary leg, if wanted: over a ceiling-enforcing double, the base's crossing repair
     surfaces as a raw backend `Err` out of `reconcile_step` (unclassified, indistinguishable from
     a transient fault), while post-fix the pass returns cleanly with the obligation queued and
     the refusal named — assert that contrast. The binding assertion either way is the stored
     length over the unlimited double.
  2. **An evacuation that did not persist does not certify.** With `set_lifecycle(.., Draining)`
     on a server holding a fragment, arrange an evacuation that cannot proceed (refused by the
     ceiling, or aborted for want of a free distinct failure domain). Assert the fragment is still
     on the draining server and the pass MUST NOT answer `Satisfied` while `reconciliation_status`
     for that server is not converged. Base: the silent `Aborted => {}` arm lets the loop answer
     `Satisfied` → **red**. This leg **flips a deliberately pinned base assertion**:
     `crates/custodian/tests/rebalance.rs:940-975` seeds exactly this shape (no free distinct
     domain) and asserts `Satisfied` ("no move: collapsing the chunk's spread would violate
     durability"). Rewriting that pin is sanctioned — #696's brief defers the certification
     question to #682 — and is why `tests/rebalance.rs` is this child's **named** fifth file, not
     an option. No new public variant is needed: `Reconciled::Blocked` already exists on base
     (`crates/custodian/src/reconciliation.rs:44`, and it outranks `Changed`), so the
     discriminator asserts `!matches!(.., Satisfied)` with base symbols only.
  3. **A refused move is subtracted, never counted as a success.** Assert the documented
     `repaired − conflict − aborted` identity holds over a pass mixing one repaired, one refused
     and one aborted chunk — a refused repair must not inflate reported successes. The counters
     are observable through in-memory doubles: existing tests already read the
     `monotonic_counter`s back via the telemetry subscriber and compute exactly this identity
     (`crates/custodian/tests/reconstruction.rs:1939-1945`). (This leg is **not** independently
     discriminating: on the base the would-be-refused repair simply commits its oversized record
     and *correctly* counts as a success, so the identity holds there too — it is red only as a
     derivative of leg (1). It ships because it pins the accounting rule for the new outcome —
     the same discount this proposal applies to child-2's leg (3).)
  Legs (1) and (2) are the discriminating reds — both red against the current `origin/main`
  (`339da46`) without #696/#697, and both surviving their merge (#696 defers the `Aborted` arm to
  #682; neither PR adds a ceiling check). Leg (3) is binding but derivative.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly.)
- **Reproduction:** on the target checkout, `git -C ../wyrd grep -n MAX_VALUE_BYTES --
  crates/core/src/metadata.rs` returns the constant (`:327`), the const assertion (`:354`) and the
  resolver's read-side refusal (`:2465`) — **no write path checks it**; and
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` at `:125-130` shows
  `EvacOutcome::Aborted => {}` falling through to `Reconciled::Satisfied`. Seed a committed flat
  root just under 100 000 encoded bytes and run a repair that moves a placement onto large-id D
  servers: the oversized record commits.
- **Scope (one logical fix) / out of scope:** **one rule — a placement write that would not
  survive is refused, and a move that did not persist does not certify.**
  - `crates/core/src/metadata.rs` — the record-ceiling checks: a placement write whose re-encoded
    record would cross the backend value ceiling is **refused and not persisted**, and the refusal
    is distinguishable by the caller from a lost CAS (they mean different things to an obligation:
    one is "never retry this shape", the other is "retry next pass"). Carve out **only** the
    ceiling helpers the write path needs — **not** the committer around them. Enforce the ceiling
    that is actually normative for the record being written (`:324-354`, `MAX_VALUE_BYTES` /
    `MAX_ROOT_VALUE_BYTES` and the `const` assertion tying them); **do not invent a third
    constant**.
  - `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs` — the two
    binding commits check the ceiling before committing, and the new refused outcome joins the
    documented `repaired − conflict − aborted` accounting; the pre-existing silent
    `EvacOutcome::Aborted => {}` arm (`rebalance.rs:128`) is settled here — #696 deliberately left
    it to this work. The refusal path writes **nothing at all**; it does **not** retract bytes
    already published to a D server (settled, rejected 4× in #638,
    `results/issue_638/review-rejected.md:15-16`).
  - Keep the CAS idiom of `commit_chunk_map` (`metadata.rs:1741-1768`): `require(key,
    encode(prior))` + `put(key, encode(next))`, `version = prior.version + 1`, and `..prior.clone()`
    so ADR-0047 object metadata is **preserved** — a placement-maintenance commit must not move
    `Last-Modified` or drop the content type.
  - **Budget:** ≤ **250** added semantic lines (non-blank, non-comment, non-mechanical) —
    child-2's ≤450 makes the pair sum to the parent's ≤700 — and ≤ **5** files:
    `core/src/metadata.rs`, `custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs`,
    `custodian/tests/placement_ceiling.rs` (**new**), and `custodian/tests/rebalance.rs` (the
    pinned `Satisfied` assertion leg (2) rewrites — a **named** cost, not an option). A sixth
    file means the shape is wrong.
  - **Out of scope:** anything segmented — **no** `repoint_chunk`, no `seg:` record addressing, no
    change to `resolve_chunk_map` or any read side, no containment-rule change (#695/#696/#697).
    The committer, the destination pre-mark, the drain fence, rollback and resume (**#653**). The
    duplicate-`ChunkId` one-plan rule and the DST supersede property (child-2). `backfill.rs`
    (**#695**) — a backfill fill that would cross the ceiling is a real gap, but it is a *different*
    write path and belongs to whichever slice owns it next, not to a widened diff here.
    `restore.rs`, `gc.rs`, `scrub.rs`, `desired_state.rs`. Any new or edited ADR / spec / proposal
    (0016 is a **draft** and stays untouched); any conformance-vector change; any new dependency.
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** The new test MUST NOT name the
    ceiling helpers or any other symbol this patch introduces. The RED leg reverts production, so
    such a reference makes the target fail to **compile** and the gate reports UNVERIFIABLE
    (exit 77, `run-verify.sh:450`, `:500`) rather than a red. Drive everything through `reconcile_step`
    and observe the **store**. `MAX_VALUE_BYTES` is base-visible (`metadata.rs:327`) and may be
    named.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`,
  `cargo-mutants` — the five `[[doctor.checks]]` ids in `pdca.toml` (cite by **id**, not by line — the parent
  brief's line numbers were already stale at review, and the file churns). Named
  because the prose and dependency-wall legs warn-skip locally while CI enforces them
  (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with
  a message naming a flag rather than the stale tool. Nothing else beyond the base Rust toolchain:
  no Docker, no protoc, no live backend, no new dependency.
- **Test file:** `crates/custodian/tests/placement_ceiling.rs` — a **NEW** file, not optional.
  C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97`, `:98`);
  appending to an existing file makes the gate take the green-only branch (`run-verify.sh:454-464`)
  and prove no red. No `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` on the
  base, so neither zero-test guard trips (`run-verify.sh:445`, `:481`). Re-run
  `run-verify.sh --classify` at Plan to confirm the invocation.
- **Difficulty:** medium
- **Depends on (merged):** 696, 697
- **Ordering note:** **Wave 1.** Its RED depends on no other child and **not** on #696/#697 —
  the missing ceiling check is orthogonal to the segmented read side — but its **diff** does:
  the accepted, unmerged PRs for #696/#697 rewrite the two custodian passes across the very
  outcome `match`es this child edits (#696's diff annotates the `Aborted => {}` arm
  "deferred: #682"). Hence `Depends on (merged): 696, 697` above: `flow` holds this bundle until
  those PRs are merged into the base (`brief.py` `depends_on_merged`, enforced via
  `merged.is_merged`). The reds survive the merge — #696 leaves the `Aborted` arm silent and
  neither PR adds a ceiling check. Once merged, this builds in parallel with any bundle sharing
  no file (e.g. #655, `crates/core/src/multipart.rs`). **Conflict, not rebase fact:** the
  `PendingEntry` two-field extension formerly attributed to #654 now belongs to **#692** (#654
  was SPLIT and never lands); #692 is unbuilt and its brief declares `Conflicts with: 682` over
  `core/src/metadata.rs` — a file this child edits. Never share a wave with #692; post-accept,
  repoint that declaration at this child's real id (682 closes as split). **Cite by symbol, not
  by number**, in `metadata.rs`.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
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
- **Depends on:** child-1
- **Depends on (merged):** 696, 697
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
  precisely the un-splitting that closed PR #647. Same rule for child-1 (in-batch, the wave
  boundary stop covers it; if this child is ever run in a separate later batch, hand-add child-1's
  real id to `Depends on (merged)` first). **Never share a wave with #692** — it carries what was
  #654's `PendingEntry` extension into `metadata.rs` plus `dst/tests/custodian.rs`, both in this
  child's file set, and its brief declares `Conflicts with: 682`; post-accept, repoint that at
  this child's real id. **Cite by symbol, not by number** — the base WILL have advanced: #696/#697
  rewrite `rebalance.rs` and `reconstruction.rs`, child-1 edits both plus `metadata.rs`, and #692
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
<!-- pdca:end child-2 -->
