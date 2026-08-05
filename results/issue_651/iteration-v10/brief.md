# Brief — issue 651 / restore-and-desired-state-contained-and-attributed

> Slice **4a of 7** of the #635 re-slicing (0016 decision 7(e)/(f)). Re-briefed **2026-08-03**
> after iteration 7 was returned to Plan. It is **NOT split again** — see § Why this is not a
> split, which also records what the round-7 numbers actually were. The rest of the old slice 4
> is **#681** (the resolving namespace walk for reconstruction / backfill / rebalance, read side)
> and **#682** (`repoint_chunk` + the record ceilings + the repair/evacuation write path).
> History and closed PR #647 are on https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** restore-and-desired-state-contained-and-attributed
- **Defect:** The two operator-facing surfaces that report **whether a reconciliation is
  complete** cannot survive — let alone describe — an object whose chunk map they could not
  read; and where they *can* read, they judge a chunk on evidence that may belong to a different
  object. #650 built the containment the first three need (`gc::ReferenceSet`) and deferred both
  surfaces here **by name, in its own code**.
  1. **Post-restore reconciliation fails the whole pass closed.** `reconcile_after_restore`'s
     *mark* half is already safe — it withholds every fragment while the reference set is
     incomplete (`gc::ReferenceSet::protects`, `restore.rs:239`) — but its *report* half re-reads
     every committed record through `committed_chunks` (`restore.rs:390`, called at `:326`),
     which `?`s out on `ChunkMap::Segmented` (`restore.rs:403-405`, under the comment at `:397`
     that names this very slice). So a store holding **one**
     segmented object, or one structurally unreadable committed root, returns `Err` and the
     operator command produces **no report at all**: not a stranded count, not the
     dangling/misplaced chunks of the objects it *could* read. One damaged object blanks the
     whole answer. #650 states the gap at `restore.rs:196`: *"deferred: #651 — the **contained**
     answer for this surface (report every object it could read, name the one it could not, and
     say the run is not certified …) belongs to the slice that owns restore."*
  2. **The drain surface cannot distinguish "not converged" from "I could not look".**
     `reconciliation_status` answers a bare `Pending` when the reference set is incomplete
     (`desired_state.rs:189`) — the same word it uses for a server that genuinely still holds
     referenced fragments. An operator watching a decommission stall has no way to learn *which*
     record is blocking it, so the stall is a state nothing exits. The tree already has the
     opposite pattern one level up: a malformed placement answers `PendingMalformed { chunks }`,
     naming the blockers in the answer itself (`desired_state.rs:101-104`, from merged #397).
     #650 states the gap at `desired_state.rs:183-187`.
  3. **The operator surface would print a hollow green.** `wyrd custodian` renders the report and
     exits non-zero only on `dangling` / `misplaced` (`crates/server/src/cli.rs:1196-1236`). An
     incomplete reading has no cell in that summary and no effect on the exit code — so once (1)
     stops erroring, a restore script checking the status code would record a run that could not
     read part of the store as a healthy one. That is precisely the failure mode the comment at
     `cli.rs:1230-1233` refuses for lost data — it even names "one whose chunks cannot be read",
     while the `if` below it at `:1234` does not test for that case.
  4. **Where it CAN read, the pass judges a chunk on evidence that may not be that chunk's.**
     Both halves reduce the fleet to a set keyed by `FragmentId` — `(chunk id, index)` — and a
     chunk id is **not** unique across objects after a restore: ids are minted from the inode
     counter the restore rewound (`crates/server/src/cli.rs`'s `chunk_id_minter`; the allocator
     floor that narrows reuse is #652), so two committed objects can carry the same id with
     different placements. This pass exists for exactly that store, and today it conflates them:
     - **the report half** counts a chunk as recoverable if bytes with that id exist *anywhere*
       in the fleet (`present_anywhere`, `restore.rs:320`, consumed at `:350-353`). Object A's
       healthy fragment therefore answers for object B's missing one: B is unreadable — the read
       path and the repair loop both fetch strictly from **its** placement — yet the verdict
       reads `misplaced`, "the bytes are one hop away", and a repair guided by it would copy A's
       bytes into B's placement. Same id, different data.
     - **the mark half** builds `canonical: HashMap<FragmentId, Vec<DServerId>>` over the whole
       fleet (`restore.rs:229`) and marks a copy collectable as soon as *any* holder of that id
       has it (`restore.rs:254-266`). With a colliding id, A's copy at A's placement satisfies
       that test, so a copy of B's fragment displaced to an unnamed server is marked — and GC
       then deletes what may be B's only copy. This is the data-losing leg of the same
       conflation, and it is why criterion (4) is not merely a reporting nicety.
- **Success criterion:** The added test target `crates/custodian/tests/segmented_map_restore.rs`
  passes, driven **only** through entries already visible on this slice's base
  (`wyrd_custodian::{reconcile_after_restore, RestoreReport, GcContext}`,
  `wyrd_custodian::desired_state::{reconciliation_status, ReconciliationStatus}` — everything
  else in `gc` is `pub(crate)` and unreachable from an integration test):
  1. **A segmented object no longer stops the pass.** `reconcile_after_restore` over a store
     seeded with a segmented object — raw `seg:` records plus a segmented root, **never** a
     committer — returns `Ok` (today: `Err`), with `RestoreReport::stranded_marked == 0`, and
     every fragment that object owns is still present on its D server afterwards.
  2. **A damaged object is contained, and the run is not certified.** Two scenarios, because one
     alone is passable vacuously:
     - **(2a) non-certification, with the incomplete reading as the SOLE cause.** One committed
       object whose chunk map cannot be read, in an otherwise **fully healthy** store — nothing
       dangling, nothing misplaced, nothing under-replicated, nothing to mark. The pass returns
       `Ok`, marks nothing (`stranded_marked == 0`), and `report.is_clean()` is **false**. Note
       `is_clean()` (`restore.rs:144`) is already false whenever any loss is reported, so a
       scenario carrying a loss would satisfy this clause **without the fix** — assert it on a
       store where the unreadable object is the only reason.
     - **(2b) containment — the damaged object does not starve the healthy ones.** The same
       unreadable object seeded **beside** a readable object that has a genuine loss: the pass
       still returns `Ok` and still reports that readable object's loss (`dangling` or
       `misplaced` names its chunk), and still marks nothing of the unreadable one.
  3. **The drain surface tells the two Pendings apart.** `reconciliation_status` over that same
     store attributes the blocking object — the operator can name the record to repair — instead
     of the unattributed `Pending` the base answers. Assert on the **audit/tracing seam**, the
     way #650's own fixture does (`assert_attributes_blocker`), so this leg needs no symbol the
     base lacks.
  4. **Evidence is attributable to the object that references it.** Over a store where two
     committed objects reference the **same chunk id** with different placements:
     - **(4a) report.** Object A's fragment present at A's own placement, object B's placement
       empty, no other copy in the fleet: the chunk is reported **`dangling`** — it is lost *for
       B*, and nothing in the fleet can be shown to be B's. It must **not** be reported
       `misplaced`, which tells the operator the bytes are recoverable one hop away and is the
       verdict a repair acts on. Base today: `misplaced == [id]`, `dangling` empty — so this
       assertion is red on the base and green with the fix.
     - **(4b) mark.** Same two objects, and B's fragment displaced to a D server **no** committed
       placement names: `stranded_marked == 0` and no `orphan:` record is written for it (today:
       A's copy at A's placement satisfies the `canonical` test and B's only copy is marked).
     - **(4c) no collision ⇒ no change.** A displaced fragment whose id **no** other object
       references still counts as its own chunk's evidence: `misplaced` + `displaced_kept`,
       exactly as `a_displaced_fragment_is_only_under_replicated_while_k_survive_at_the_placement`
       and `a_stranded_fragment_is_marked_so_gc_can_finally_reclaim_it` already pin on the base.
       Assert this too — a fix that made the pass conservative everywhere would pass (4a)/(4b)
       and break the pass's whole purpose.

  Criteria (2) and (4) are the binding ones. **All** of (2a), (2b), (4a), (4b), (4c) must ship:
  (2a) alone does not show the walk continues, (2b) alone does not show the run is
  non-certifying, (4a) without (4c) is satisfied by a pass that reports everything as lost, and
  (4b) is the only one that pins the data-losing leg. A version that only checks the call
  returned `Ok` proves none of them.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no dev-dependency, no
  network.
  - **Base.** This bundle has no `Onto branch` and no `stack-base` marker, so
    `run-verify.sh:_resolve_base_ref` falls through to the brief base → `origin/main`.
  - **Base prerequisite — SATISFIED, re-verified 2026-08-03 on `origin/main` = `d50f0ca`:**
    `metadata::resolve_chunk_map` (`core/src/metadata.rs`), `ReferenceSet::unresolvable`
    (`custodian/src/gc.rs:294`), `object_name` (`gc.rs:470`, `pub(crate)`),
    `referenced_fragments` (`gc.rs:360`, `pub(crate)`) and the #650 fixture
    `custodian/tests/segmented_map_consumers.rs` (1,327 lines) are all present. #648/#649/#650
    are merged (PR #672, PR #683). So the containment vocabulary this slice extends exists on the
    base and the criterion can be both expressed and made red.
  - **Discriminator — `--classify` dry-run performed at Plan** on this slice's expected file set
    (v7's, which this brief keeps): `./engine/scripts/run-verify.sh --classify` returns exactly
    `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs` plus `CRATE crates/custodian`
    and `CRATE crates/server`. One added test ⇒ the invocation is
    `cargo test -p wyrd-custodian --test segmented_map_restore`, so the reverted `crates/server`
    change cannot break the leg. The file does **not** exist on `origin/main` (checked).
  - **The RED leg** keeps that file and reverts `restore.rs`, `desired_state.rs`, `cli.rs` and
    every modified test file. On that tree `committed_chunks` still fails closed, so (1) and (2)
    fail on `Err`; (3) fails because no attribution is emitted; (4a) fails because the base
    answers `misplaced`; (4b) fails because the base marks the displaced copy.
  - **Evidence this is not theoretical:** iteration 7's C4-verify recorded **PASS — red without
    the fix, green with it** on criteria (1)–(3). Criterion (4) is new and its red is argued
    above from `restore.rs:320/350` and `:229/254` on the base.
  - No `#![cfg(...)]` on `crates/custodian/tests/*.rs`, so neither the vacuous `0 tests … ok`
    branch nor a compile-red-scored-as-pass can occur.
  - **Keep the discriminator assertion-red — hard constraint.** The file MUST NOT reference any
    symbol this patch introduces (no new `RestoreReport` field, no new `ReconciliationStatus`
    variant, no new helper): the RED leg reverts the production files, so such a reference makes
    the target fail to *compile* and the red degrades to "a symbol is missing" instead of "the
    behaviour was wrong". Coverage that genuinely needs a new public shape — match
    exhaustiveness, a new field's contents — ships in the **existing**
    `crates/custodian/tests/restore_reconcile.rs`, which `C4-ci` gates.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the surfaces that report whether a reconciliation
  is complete**. Sourced, not intuited — this slice is the §6 category row *Storage lifecycle /
  reclamation* (`docs/principles.md:137`), which names **restore** and **drain** explicitly and
  whose sources are §5 C-1 (`docs/principles.md:109`), the maintainer's rule of 2026-07-25,
  `0016:2802-2813`, and `gc.rs:22-25`. Over that category:
  - **A pass never reports a conclusion it could not reach.** "Complete", "clean" and "satisfied"
    are claims about a reading that *finished*; over an incomplete one they are the reclamation
    decision in report form — "you may decommission this box", "close the ticket" — and an
    operator will act on them.
  - **Containment is per object, and the answer still gets made.** One damaged record may not
    blank a fleet-wide status surface, nor withhold the losses of every object the pass *could*
    read. `Err` for the whole query is as wrong as `Satisfied`.
  - **An incomplete reading is attributed, not merely signalled.** An operator who cannot learn
    *which* record blocked the pass cannot repair it, so the stall is a state nothing exits —
    the same permanence C-1 forbids, arrived at through the report instead of through deletion.
  - **Evidence about a chunk must be attributable to the object that references it.** C-1's own
    words: every durable byte is at every instant *"protected by a record that names it **or**
    evidenced for reclamation"* (`docs/principles.md:137`). Bytes that another object's record
    names are neither — they do not evidence this chunk's recoverability, and they do not
    authorize reclaiming a copy of it. This is the rule `ReferenceSet::protection` already
    applies one level up (`gc.rs:306-334`): an unreadable map hides *which* chunks an object
    owns, so nothing in the fleet can be shown not to be one of them, and the stated trade is
    *"the cost is a leak until the object is repaired; the alternative is deleting a live
    object's bytes."* Ambiguous identity is the same shape of unknowing, one level down.
  - **A non-zero exit is part of the report.** A surface that prints a caveat and exits 0 has
    told the automation the run was healthy.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan 2026-08-03:
  `git -C ../wyrd rev-parse origin/main` → `d50f0ca`. INTEGRATION §2's default; Wyrd has no
  maintenance branches and the M4 integration branch is deleted. **Not** `pdca-integration/main`
  — that is the driver's run-scoped wave-fold branch, regenerated per run; its stale marker was
  what made v5 build on a tree that had lost #650, and it has been removed from this bundle.)
- **Conflicts with:** 681
- **Ordering note:** **`Depends on` is deliberately EMPTY now.** v7 carried `Depends on: 650`;
  #650 is merged to `origin/main` (PR #683) and verified present on the base above, so the
  prereq is in the base rather than in a wave — listing a COMPLETE bundle would only schedule an
  empty wave. `Conflicts with 681` stands: no dependency in either direction (681 owns
  reconstruction / backfill / rebalance; this one owns restore / `desired_state`), but both are
  likely to edit `crates/custodian/tests/segmented_map_consumers.rs` (#650's shared fixture,
  which this slice extends) and `crates/server/src/cli.rs`, so they must not be built blind on
  the same base. #682 depends on #681 and touches none of this slice's files.
- **Surfaces:** data
- **Difficulty:** high   (rated **up** from v7's `medium`, deliberately: three production files
  across two crates, a new public enum variant and a new public report field, a change to how the
  *mark* half decides what may be reclaimed — a diff-reviewer must hold the pass's two halves and
  the operator surface in view at once — and seven prior Do iterations on this subject. The
  higher tier is the safe default and routes the stronger backend and deeper review.)
- **Scope:** make both completeness-reporting surfaces answer **contained, attributed and
  attributable** — contained over a reference set with a hole in it, and drawn only on evidence
  belonging to the object being judged.
  `crates/custodian/src/restore.rs` — the report half survives an object it cannot read (reports
  every object it *could* read, records the ones it could not, and the run is not clean), and
  **both** halves judge a chunk only on evidence attributable to the object that references it:
  a chunk id that more than one committed object references is ambiguous, and authorizes neither
  a recoverability verdict for a reference whose own placement is empty nor a reclamation mark on
  any copy of it — while a chunk whose id no other object references keeps exactly today's
  displaced/stranded behaviour. `crates/custodian/src/desired_state.rs` — the drain status
  distinguishes "still holds referenced fragments" from "could not read object X", and names X.
  `crates/server/src/cli.rs` — the operator summary states an incomplete reading and the
  command's exit status reflects it. Plus their existing test files, the added discriminator, and
  the docs-currency paragraph.
  **Out of scope:** reconstruction, backfill and rebalance, and the resolving namespace walk they
  need (**#681**) — this slice adds **no** custodian-level walk and no `crate::resolve` module,
  and reads the reference set through `gc::referenced_fragments` exactly as the base does;
  `repoint_chunk` and the record ceilings (**#682**) — this slice writes **nothing** to a chunk
  map; the chunk-id allocator floor (**#652**) — this slice changes only how the pass *judges* a
  store that already contains reused ids, never how ids are minted; the committer, fence,
  rollback and resume (#653); **any edit to `gc.rs` or `scrub.rs`** — the fleet-wide-by-id
  displaced-tolerance rule that needs attribution exists **only** in `restore.rs` (verified: no
  `HashMap<FragmentId, …>` or displaced concept in either), so the invariant is restorable
  without touching #650's shared code; **no new report class or CLI cell for a colliding id** —
  the collision surfaces through the existing verdicts and the audit seam, and
  `RestoreReport::dangling` / `misplaced` keep their `Vec<ChunkId>` shape; **no owner attribution
  on the dangling/misplaced audit events** — under a colliding id a bare chunk id does not tell
  the operator *which* object is lost, which is a real gap, but the fix for it is #652 (stop the
  reuse) rather than a wider report schema here, and threading the owning `inode:` key through
  `Expected` / `emit_dangling` / `emit_misplaced` would enlarge a slice that has already been
  returned to Plan twice; any new/edited ADR / spec / proposal; any conformance-vector change.
- **Budget:** ≤ **950** added semantic lines (non-blank, non-comment), ≤ **8** files. Measured
  basis, not a guess: iteration 7 delivered **8 files / 1,282 added lines / ≈749 semantic** (410
  of them the discriminator fixture; ≈240 production), and criterion (4) adds roughly 40–60
  production lines plus its scenarios. If mid-build the tree exceeds this, STOP and hand back a
  proposed split rather than finishing.
- **Repro instruction:** on the base — `git -C ../wyrd show origin/main:crates/custodian/src/restore.rs`:
  `committed_chunks` at `:390` reaches `.as_flat().ok_or(SegmentedMapUnsupported)` at `:403-405`
  and is called unconditionally at `:326`, so seeding any `seg:`-backed committed root makes the
  whole pass return `Err` (defect 1). `:320` builds `present_anywhere` keyed by `FragmentId` and
  `:350-353` counts it as this chunk's evidence (defect 4, report half); `:229` builds
  `canonical` the same way and `:254-266` marks on it (defect 4, mark half). For defect (2),
  `crates/custodian/src/desired_state.rs:189` returns a bare `Pending` for a non-empty
  `ReferenceSet::unresolvable`. For defect (3), `crates/server/src/cli.rs:1196-1236` prints
  "post-restore reconciliation complete" and gates the exit code on `dangling`/`misplaced` only.
- **External dependencies:** `cargo-deny`, `cargo-machete`, `typos`, `docs-renderer` — all four are registered doctor.checks rows in pdca.toml, all four green on this host as of 2026-08-03. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dev-dependency, no DST leg.
  The first is named because it is **not** warn-skipped: cargo xtask ci's ADR-0003 dependency
  wall hard-fails the gating C4-ci row on a tool too old for the root-level --config form, which
  is exactly what cost iteration 7 its C4-ci verdict (0.19.9 on this host vs the >= 0.20 CI
  installs; fixed 2026-08-03 by reinstalling to 0.20.2, and now preflighted by a doctor row). The
  other three warn-skip locally but are enforced in host CI, and this slice edits docs.
- **Test file:** `crates/custodian/tests/segmented_map_restore.rs` — a **NEW** file, and this is
  not optional. `C4-verify` classifies its discriminator on an **added** `*/tests/*.rs`
  (`_added_files` keys on `--- /dev/null`), and `segmented_map_consumers.rs` /
  `restore_reconcile.rs` both already exist on this base — appending to either makes it a
  *modified* file and the gate takes the green-only branch, proving no red at all. Confirmed by
  the `--classify` dry-run recorded under Falsifiability. Updates to the existing per-pass test
  files may ship **in addition**; `C4-ci` covers them.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. No deferred or off-Check leg.
- **Citations expected:** cite `path:line` on the target branch for every change. Line numbers
  below were re-verified against `origin/main` at `d50f0ca` on 2026-08-03; still cite by symbol,
  not by number, if the base advances.
  **START FROM THE PREVIOUS ATTEMPT — do not rebuild from scratch.**
  `results/issue_651/iteration-v7/patch.diff` is this bundle's own iteration 7: 8 files, it
  passed **C4-verify** (red→green) and **T4-contribution**, and its only substantive blocker was
  the finding criterion (4) now settles. Its C4-ci red was a stale host tool, not the patch (see
  External dependencies). Take it as the base of this attempt and change what criterion (4)
  requires, plus the finding below. Salvage from `results/issue_651/sources/salvage.diff` (closed
  PR #647) only for fixture idiom; **do not** carry over anything touching reconstruction,
  backfill, rebalance, `repoint_chunk` or the record ceilings, and **do not** introduce a
  `crate::resolve` module — that is #681 and is the single most likely way this bundle fails
  again.
  **The one round-7 finding to FIX (not reject):** `crates/custodian/tests/restore_reconcile.rs:952`
  — v7's `one_object_s_healthy_copy_does_not_answer_for_another_object_s_missing_one` asserted
  `report.dangling.is_empty()` on the ground that "nothing is lost — the bytes are one hop away",
  which codifies precisely the cross-object conflation criterion (4a) forbids. That assertion
  must be **inverted**, not re-litigated: with a colliding id and B's placement empty, the chunk
  is `dangling`. This finding must **not** appear in `review-rejected.md`.
  **Peer callsites Do MAY open — mirror them rather than inventing a shape:**
  - `crates/custodian/src/gc.rs:265-294` (`ReferenceSet`, and its `unresolvable:
    BTreeMap<Vec<u8>, String>` keyed by **raw key bytes**, with the reason a rendered name is not
    injective), `:306-334` (`protection` / `protects` — **the model for criterion (4)**: a
    protection reason is exactly "this byte cannot be shown to be reclaimable", and its stated
    trade of a leak over a deletion is the one to copy), `:470` (`object_name`, which escapes
    rather than replaces), `:563` (`emit_unresolvable`, the audit seam), `:234-241`
    (`Reconciled::Blocked` — the non-certifying answer, and *why*).
  - `crates/custodian/src/desired_state.rs:91-104` — `PendingMalformed { chunks }`, the
    attribution-in-the-answer shape already in the tree (merged #397).
  - `crates/server/src/cli.rs:1196-1236` — the operator summary, its NEEDS-HUMAN paragraphs, and
    the exit-code comment at `:1230-1233` with its `if` at `:1234`.
  - `crates/custodian/src/restore.rs:250-266` — the displaced case's own comment, which spells
    out why the last copy of a referenced fragment must never be marked. Criterion (4b) is that
    same rule under a colliding id, where "the map's server DOES have it" stops being proof.
  Fixture idiom to mirror: `crates/custodian/tests/segmented_map_consumers.rs` (#650) —
  `MemMeta` / `MemDServer` / `PoisonedMeta`, `seed_segmented`, and the tracing capture
  (`capturing_dispatch`, `attributed_objects`, `assert_attributes_blocker`). Integration-test
  crates cannot import across files, so the discriminator carries its own **pruned** copy of only
  what criteria (1)–(4) need.
  Normative: 0016 decision 7(e) `:2393-2415`; `docs/principles.md:109` (§5 C-1) and `:137` (the
  §6 category row).
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — extend the
  containment paragraph #650 started with **the sentences this slice lands, and only those**:
  post-restore reconciliation reports every object it could read, names the ones it could not,
  and does not certify the run; a drain status distinguishes "still holds referenced fragments"
  from "a committed object could not be read", naming the blocker; and a chunk is judged only on
  fragments attributable to the object that references it, so a reused chunk id neither certifies
  another object's chunk nor authorizes reclaiming its copy. Nothing about repair, evacuation or
  staged publication — those are #681 / #682 / #653.
- **Prior-art check (triage cycles):** searched by affected file path (`restore.rs`,
  `desired_state.rs`, `cli.rs`) across merged history and open/closed PRs. **#647 is the only PR
  addressing this concern and it is CLOSED unmerged** — closed on reviewability, not correctness,
  hence a salvage source. Of the merged ones, three bind criteria here and none is superseded:
  **#555** established the `RestoreReport` / `stranded_marked` contract that criterion (1)
  extends; **#397** established `PendingMalformed`, the attribution shape criterion (3) mirrors;
  **#193** originated `ReconciliationStatus`. **#672 / #683** merged the #648 / #649 / #650
  prereqs onto `main`. No prior art routes restore's report half through the resolver, none
  attributes an *unresolvable* object on either surface, and none makes either half's evidence
  object-attributable — `git log -S present_anywhere` shows the fleet-wide set unchanged since it
  was introduced.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` / `MetadataStore` implementation owns the network bound, not the caller
  (`crates/traits/src/lib.rs:1000-1012`). (ii) *"a genuine store fault should be contained too"* —
  no: a store fault propagates (#650's
  `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed`); only a
  *record-level* read failure is contained. (iii) *"`Completed` releases its admission slot"* —
  withdrawn as unsatisfiable. Do MUST record each rejection in `review-rejected.md` **at every
  line the finding is reported at**, in the gate's `<file:line> | <CLASS> | <MATCH> | <reason>`
  format. The bundle's existing `review-rejected.md` is keyed to iteration 7 and its four scope
  declines (#681 / #682 / #652) still hold — but the `restore_reconcile.rs:952` finding is now
  **in scope and must be fixed**, per Citations above.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Why this is not a split (read before sizing this bundle again)

Iteration 7 was returned to Plan on the ground that the patch ran "roughly 3× over the brief's
own budget (~1,957–2,184 semantic lines / 12 files)". **Those numbers describe iteration 5, not
iteration 7.** They come from `deferred-findings.json` (`"through_round": 5`), written against the
pre-split patch whose 1,069-line discriminator was `segmented_map_repair.rs` — a file iteration 7
does not contain. Measured directly from `iteration-v7/patch.diff`:

| | iteration 7 | its budget |
|---|---|---|
| files | 8 | ≤ 8 |
| added lines | 1,282 | — |
| non-blank / non-comment added | ≈ 749 | ≤ 700 |

Per file: discriminator fixture 410 · `cli.rs` 139 · `restore_reconcile.rs` 85 · `restore.rs` 79
· `desired_state.rs` 22 · rest ≈ 14. For scale, #650's merged fixture
`segmented_map_consumers.rs` is 1,327 lines, `rebalance.rs` 1,497, `reconstruction.rs` 1,949 — a
628-line integration test is house idiom here.

A further split would separate ~79 lines of report containment from ~22 lines of drain
attribution, and each child would still need its own ~350-line fixture over the same seeded
store: more total lines, two more cycles, worse evidence. The two real blockers were a semantics
question nobody had decided (now criterion 4) and a stale host tool (now fixed and preflighted).
**If this bundle is over budget again, size it against `patch.diff`, not against
`deferred-findings.json`.**

## Carry-forward — what the seven prior iterations settled

- Iterations 1–5 ran on the **un-split** issue (restore + reconstruction + backfill + rebalance +
  `desired_state`), archived in `iteration-v1/` … `iteration-v5/`. Of iteration 5's seven
  blockers, **none is in this slice's scope**: two memory-blowup risks and two containment breaks
  in the shared namespace walk (→ #681), a duplicate-reference gap and an inflated success count
  in the repair/evacuation path (→ #682). Do not re-import that machinery.
- Iteration 6 (`iteration-v6/`) — C4-ci **pass**, C4-verify **pass**; failed T4 (3 blocking) and
  advisory C5 (7 missed mutants).
- Iteration 7 (`iteration-v7/`) — C4-verify **pass** (red→green), T4-contribution pass; C4-ci red
  on the stale `cargo-deny` (host, now fixed); C5 advisory 2 missed; **one** T4 blocking finding,
  `restore_reconcile.rs:952`, which criterion (4) now settles. The reviewer leaf crashed
  (`check-review.error.log`), so that round has no advisory review — it is not a verdict on the
  patch.
- Do NOT re-attempt the rejected approaches unchanged; satisfy the Success criterion above.

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Human note: the prior iteration-v1..v7 history predates this slice's split at Plan; this bundle is a fresh convergence attempt post-split, not evidence of an unbounded/oversized slice. The size overage (971 semantic lines vs. the brief's ~950 budget, and 116KB vs. the 100KB backstop) is slight, not the kind of overage that warrants sending the whole thing back to Plan — keep it as one iterate-do round. What to fix, corroborated independently by both the adversary review and the freshly re-run decorrelated reviewer (T3 Runtime: FAIL, T4 Contribution: FAIL): the ambiguity/containment rule in crates/custodian/src/restore.rs (~229, ~254, ~665-675) keys on "how many D servers are named" (a HashSet of placements) rather than "how many committed references exist" for a shared chunk id. Two committed objects that reference the same chunk id but happen to share a placement server defeat the ambiguity check entirely: the mark half still marks an extra unnamed copy collectable even though it may be the *other* object's only correct bytes (data loss via GC), and the report half can emit `dangling` for bytes that are actually present and safe to restage, or `misplaced` for bytes that are not actually recoverable at that placement. Rebuild the rule to be conservative on reference-count ambiguity (>1 committed reference under one chunk id), not holder-count, using `committed_chunks`'s one-entry-per-reference data that is already read. Also worth resolving in the rebuild, per the adversary review's second finding: the doc/brief premise that live committed objects can collide on chunk id via `chunk_id_minter` looks unsupported by the current minting code (inode id is packed into the id and inode ids don't repeat across live records) -- either name the actual reachable collision path, or fold this concern into #652 rather than carrying inert conservatism that produces false-LOST reports in the CLI. Also fix, per the adversary review's third/fourth findings: the CLI's DANGLING paragraph (crates/server/src/cli.rs:1284-1290) still unconditionally claims lost bytes are unrecoverable even though the new ambiguity-induced `dangling` can fire when bytes are present and restaging is actively harmful -- hedge that sentence; and criterion (2a)'s test justification cites `is_clean()`, which has no production caller (cmd_custodian exits on `restore_verdict`'s own `needs_human` predicate) -- fix the claim or make one predicate load-bearing.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 37 mutants tested in 2m: 3 missed, 18 caught, 16 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Size budget only nominally exceeded (128KB vs 100KB backstop, ~1,059 vs ≤950 lines) — accepted as-is, not a re-slice trigger. 651 was recently right-sized; some renewed convergence churn is expected. Give it 2-3 more rounds to converge before reconsidering iterate-plan. Carry forward ALL reviewer/adversary/mutant findings for the next Do round to address in place: - C5 causal adequacy (core gap): two committed objects sharing both a chunk ID AND a placement still produce a false-clean report — `anywhere = placed` credits one ownerless fragment to both claimants (restore.rs:484; segmented_map_restore.rs:712). This is the central attribution property the brief asks for — must be closed. - Adversary [human]: the doc-claimed "reachable collision path" (m4-first-deployment-blueprint.md:693-704) does not actually reach `CommittedChunks::ambiguous` (claims stays 1, never >1) — the scenario that IS reachable (dead object's fragment vs. one committed object with empty placement) still misclassifies as `misplaced` instead of `dangling`, which is the exact corruption criterion (4a) exists to prevent. Either name the actual reachable collision path or fold into #652 — this question is still open. - Adversary [impl]: an ambiguous chunk id that never becomes `dangling` is completely silent and the run is certified clean forever (`emit_ambiguous_evidence` only reachable from the `anywhere < k` arm; restore.rs:497-504). Emit `ambiguous-chunk-id` wherever ambiguity actually changed a decision, not only in the dangling arm. - Adversary [impl]: the ambiguity gate sits above the `already`-marked check (restore.rs:358-366 vs :395), so a mark an earlier run wrote is reported "kept" while GC still deletes it — a regression vs. origin/main's honest `already_marked: 1`. Move the gate below the already-marked check. - Adversary [impl]: `is_clean()` has no true-branch assertion anywhere in the tree (restore.rs:178) — `fn is_clean(&self) -> bool { false }` would pass the whole suite. Add a true-branch assertion (single healthy chunk -> is_clean() == true). - Adversary [impl]: two unasserted C5 mutant misses on lines this patch added — restore.rs:359 (`+= -> *=` on displaced_kept in the ambiguous arm) and restore.rs:503 (`- -> +` on the withheld count in emit_ambiguous_evidence) — pin both in the C4-ci-gated restore_reconcile.rs. - Adversary [impl]: UNREADABLE operator paragraph (cli.rs:1314-1320) asserts a fleet-wide fact ("nothing anywhere in the fleet was [marked]") that report.unresolvable (union of both walks) cannot support — derive the sentence from report.stranded_marked instead. - T4 batched rubric review: 3 blocking findings (review-branch) — resolve or address explicitly. - C5 mutants gate: 6 missed of 44 tested — close the gap (ties to the two items above plus any others surfaced). - T5 Judgment (open question, needs resolution alongside the fix, not deferred silently): decide whether duplicate committed IDs are a supported restore state here or belong to #652 — the shipped minter claims cross-object uniqueness but the documented rewind route doesn't clearly establish the second committed reference this rule requires (cli.rs:1796; m4-first-deployment-blueprint.md:694). - Validation / fitness-to-purpose (open question): decide, ideally by exercising a representative restored store, whether same-placement duplicates receiving a hollow green and the DANGLING summary potentially misdirecting recovery are acceptable for the actual restore topology. None of the above are being cleared in §6 here — this is an iterate-do disposition, §6 items remain open for the next round to address or for a future sign-off to clear explicitly. </content>
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 44 mutants tested in 2m: 6 missed, 20 caught, 18 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 10 — carry-forward (from the previous attempt)
- Sign-off rationale: Send back to Plan primarily to settle the architectural fork this bundle has now raised twice: whether the cross-object chunk-id ambiguity rule is a real, reachable hazard worth keeping as conservative-but-occasionally-wrong behavior, or whether it should fold into #652 (stop id reuse at the source) and this slice ship only the containment/attribution half it was originally briefed for. The diff's own docs now state that two *live* records cannot collide given the current minter/epoch/CopyObject-refusal design, and the adversarial review found that the only in-tree-reachable trigger of the new ambiguity rule (a single object whose chunk map lists one id twice) produces a FALSE "LOST" verdict — so the rule as shipped is inert against its intended target and actively wrong on the one case it can hit. That is a scope/design decision, not an implementation bug a rebuild can resolve on its own. Also feeding this: the size backstop (156KB, round 2 of a 2-round threshold, semantic lines 1,242 vs. the brief's own 950-line stop rule, climbing every round: 749->971->1,059->1,242) and a still-open real bug (mark-withdrawal skipped on the displaced shape, restore.rs:394-411, reproduced to cause GC to delete a live copy) that a re-plan should size correctly into whichever child slice ends up owning it, rather than patching again inside an oversized bundle. At re-plan: settle the ambiguity-rule question first (keep vs. fold into #652), then split the remaining containment/attribution work and the mark-withdrawal fix into properly-sized child briefs via `pdca split`.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v10/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
