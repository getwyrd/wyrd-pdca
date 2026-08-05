# Result — issue 651 / restore-and-desired-state-contained-and-attributed

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The two operator-facing surfaces that report **whether a reconciliation is
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
- Success criterion: The added test target `crates/custodian/tests/segmented_map_restore.rs`
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
- Repo + branch target: getwyrd/wyrd @ main   (resolved and verified at Plan 2026-08-03:
  `git -C ../wyrd rev-parse origin/main` → `d50f0ca`. INTEGRATION §2's default; Wyrd has no
  maintenance branches and the M4 integration branch is deleted. **Not** `pdca-integration/main`
  — that is the driver's run-scoped wave-fold branch, regenerated per run; its stale marker was
  what made v5 build on a tree that had lost #650, and it has been removed from this bundle.)
- Scope (one logical fix) / out of scope: make both completeness-reporting surfaces answer **contained, attributed and
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 44 mutants tested in 2m: 6 missed, 20 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make post-restore reconciliation and drain status contain and attribute unreadable objects, reject cross-object chunk evidence, and surface incomplete runs to operators.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives falsifiable outcomes for segmented maps, sole-cause non-certification, continued reporting, drain attribution, both shared-ID halves, and the non-collision control. |
| C2 Reproduction (red pre-fix) | PASS | With only the added discriminator on the base, 6 of 7 tests fail on behavioral assertions while the non-collision control stays green (`crates/custodian/tests/segmented_map_restore.rs:435`). |
| C3 Change | NEEDS-HUMAN | Decide whether to accept or reslice the current 8-file patch: 1,851 additions mechanically count to about 1,059 nonblank/non-comment lines and 131,233 bytes, beyond the brief's ≤950-line budget and carried 100-KB backstop — scope control matters before another rebuild. |
| C4 Verification (red→green) | PASS | Restoring the patch makes all 7 discriminator tests green, and fmt, clippy, build, workspace tests, docs lint/render, typos, machete, all dependency-wall graphs, conformance, statics, and the 50-seed DST pass; the initial cargo-deny lock error was reproduced as a read-only-Cargo-home host fault and cleared with a writable advisory-db copy (`crates/custodian/tests/segmented_map_restore.rs:704`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the shared-placement case: `anywhere = placed` credits one ownerless fragment to both same-ID claimants, so the central attribution rule still has a missed case and its same-placement test leaves the named placement empty (`crates/custodian/src/restore.rs:484`; `crates/custodian/tests/segmented_map_restore.rs:712`). |
| T1 Structure | PASS | The change remains within the scoped custodian/server/tests/living-doc boundaries, adds no dependency, and centralizes the command decision in one report-to-verdict seam (`crates/server/src/cli.rs:1262`). |
| T2 Shape | PASS | The public answer names unreadable objects separately from ordinary pending state and exposes one report predicate for the CLI, so callers can distinguish containment from convergence (`crates/custodian/src/desired_state.rs:119`; `crates/custodian/src/restore.rs:192`). |
| T3 Runtime | FAIL | A scratch regression with two same-ID objects sharing one populated placement returns a completely clean report because `placed` counts the same fragment for both claimants (`crates/custodian/src/restore.rs:465`; `crates/custodian/src/restore.rs:489`). |
| T4 Contribution | FAIL | The driver review harness is unavailable in this artifact-only checkout, but an independent scratch assertion confirms the first CLI summary still says every DANGLING map's bytes “were already reclaimed,” contradicting its ambiguous-ID case; the shipped test checks only the later paragraph (`crates/server/src/cli.rs:1264`; `crates/server/src/cli.rs:2804`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether duplicate committed IDs are a supported restore state here or belong to #652: the shipped minter claims cross-object uniqueness, while the documented rewind route describes stale fragment IDs but does not establish the second committed reference this rule requires — the boundary determines whether this is necessary protection or inert false-LOST behavior (`crates/server/src/cli.rs:1796`; `docs/design/architecture/m4-first-deployment-blueprint.md:694`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the operator behavior is fit for the actual restore topology after exercising a representative restored store: same-placement duplicates currently receive a hollow green and the remaining DANGLING summary can send recovery toward the wrong cause, which directly affects decommission and backup decisions (`crates/custodian/src/restore.rs:489`; `crates/server/src/cli.rs:1266`). |

### Advisory — adversary

# Adversarial review — issue 651 (advisory, non-gating)

Evidence re-run independently at `$PDCA_TARGET`, not taken on the bundle's word:

* **Green post-fix** — `cargo test -p wyrd-custodian --test segmented_map_restore` → 7/7 pass.
* **Red pre-fix** — in a scratch copy with `restore.rs`, `desired_state.rs`, `cli.rs` and both
  modified test files reverted to `origin/main` (d50f0ca) and only the discriminator kept:
  **6 of 7 fail, all on assertions/behaviour, none on a compile error** — `SegmentedMapUnsupported`
  for (1)/(2a)/(2b), an empty audit capture for (3), `misplaced:[45312]` vs `dangling` for (4a),
  `stranded_marked: 1` for (4b). The 7th (`..._no_other_object_claims_...`, criterion 4c) passes on
  base *by design*. The discriminator drives `wyrd_custodian::reconcile_after_restore` /
  `desired_state::reconciliation_status` — the real entry points, not a parallel re-implementation.
  So the C4-verify row is **not** refutable, and the red is for the right reason.

The findings below are all against the *fix*, not the evidence.

- **NEEDS-HUMAN [human] — the collision path the patch now documents as reachable cannot produce
  the state the patch keys on, and the one it *does* produce is still uncovered.**
  `docs/design/architecture/m4-first-deployment-blueprint.md:693-704` replaces the old
  "chunk ids are random, a reused inode cannot collide" claim with: the CLI path mints
  `(inode << 64) | seq`, so "a reused inode *does* re-mint the ids of the post-*V* file that held
  it. Post-restore reconciliation does not assume otherwise — … where the restored namespace shows
  one chunk id claimed by more than one committed object it withholds both the 'restage it' verdict
  and any reclamation mark." The post-*V* file is precisely the record the restore **removed**, so
  it is not a committed claimant; that mechanism yields `claims == 1`, and `CommittedChunks::ambiguous`
  (`crates/custodian/src/restore.rs:562`) never fires. Executed on the target: one committed object
  at `d0` with an empty placement, plus the dead object's same-id fragment on an unnamed server →
  `RestoreReport { dangling: [], misplaced: [91], displaced_kept: 1 }`, and `restore_verdict`
  (`crates/server/src/cli.rs:1303-1309`) then prints *"Restage those fragments onto the placed D
  servers … Do NOT go to a backup: the data is here"* — i.e. write the dead file's bytes under the
  live file's map. That is exactly the corruption criterion (4a) exists to prevent, in the one
  scenario the diff itself now advertises as reachable. Iteration 8's carry-forward asked to
  *"either name the actual reachable collision path, or fold this concern into #652"*; the path
  named does not reach `claims > 1`, so that question is still open and is a scope call
  (widen the oracle beyond committed references, or drop the doc's coverage claim and defer to #652).

- **NEEDS-HUMAN [impl] — an ambiguous chunk id that never becomes `dangling` is completely silent,
  and the run is certified clean.** `emit_ambiguous_evidence` is reachable only from inside the
  `anywhere < k` arm (`crates/custodian/src/restore.rs:497-504`); the mark half's withhold at
  `restore.rs:358-366` emits `emit_displaced` instead. Executed on the target: two committed objects
  claiming chunk `71`, both healthy at their shared placement `d0`, plus one genuine stray copy on
  `d9` → `RestoreReport { stranded_marked: 0, displaced_kept: 1, dangling: [], misplaced: [],
  unresolvable: [] }`, `is_clean() == true`, `needs_human() == false`. So the CLI prints
  "post-restore reconciliation **complete**" and exits **0**, on every re-run forever, while the pass
  has permanently stopped reclaiming that id — and the only signal emitted is `emit_displaced`, whose
  text (`restore.rs:795-800`) ends *"The placement is stale, not the data; repair repoints it"*, an
  instruction that is wrong here (nothing is stale; the id is duplicated). The brief's own invariant is
  "a pass never reports a conclusion it could not reach" and "an incomplete reading is attributed, not
  merely signalled" — the withheld reclamation is a conclusion it could not reach, and it is neither
  attributed nor allowed to affect the verdict. Fix is cheap and needs no new report class (the brief
  declines one): emit `ambiguous-chunk-id` wherever the ambiguity actually changed a decision, not only
  in the dangling arm.

- **NEEDS-HUMAN [impl] — the ambiguity gate is placed above the `already`-marked check, so a mark an
  earlier run wrote is reported as "kept" while GC still deletes it.** `restore.rs:358-366` `continue`s
  before `already.contains_key(&(dserver, frag))` at `restore.rs:395`, and this pass never deletes an
  `orphan:` record. Executed on the target: chunk `72` claimed by two committed objects, a stray copy on
  `d9`, and an `orphan:` record for `(9, frag(72,0))` already on disk → `displaced_kept: 1`,
  **`already_marked: 0`**, the orphan record still present, `is_clean() == true`. `gc::reconcile`
  (`crates/custodian/src/gc.rs:191-214`) gates only on `ReferenceSet::protection`, which has no
  ambiguity clause, so it reclaims that fragment as soon as the grace window elapses — the (4b)
  data-loss leg, reached through a mark written before the second claimant existed (exactly the m4
  inode-reuse timeline) rather than by this run. Note this is a **regression introduced by the diff**:
  on `origin/main` the same store reports `already_marked: 1` (honest), because the fragment fell
  through to `:395`. Minimum fix: move the ambiguity gate below the `already` check so the count stays
  truthful, and decide whether an ambiguous id's stale mark should be retracted (a mark is an
  authorization to delete, and `emit_displaced` claims the fragment is "kept, never marked").

- **NEEDS-HUMAN [impl] — `is_clean()` has no true-branch assertion anywhere in the tree, so
  criterion (2a)'s central claim is one-sided.** `crates/custodian/src/restore.rs:178`. Every use in the
  repo is negative (`segmented_map_restore.rs:556`, `restore_reconcile.rs:729`, `:881`,
  `cli.rs:2749`'s `!(human && report.is_clean())`); `fn is_clean(&self) -> bool { false }` passes the
  entire suite — which is precisely the C5 row's `MISSED restore.rs:179:9: replace
  RestoreReport::is_clean -> bool with false` (and `:179:30 == → !=`). The bundle asserts
  "an incomplete reading is never clean" without ever asserting "a complete, healthy reading **is**".
  Verified on the target that the true branch is reachable and correct (a single healthy chunk →
  `is_clean() == true`), so the assertion costs one line.

- **NEEDS-HUMAN [impl] — the two remaining C5 misses are both on lines this patch added, and both are
  genuinely unasserted.** (a) `restore.rs:359` `report.displaced_kept += 1` in the ambiguous arm —
  `+= → *=` survives, i.e. no test pins the counter on the ambiguity path (criterion (4b) asserts only
  `stranded_marked == 0` and the absent `orphan:` record). (b) `restore.rs:503`
  `emit_ambiguous_evidence(chunk, committed.claims(chunk), by_id_alone - anywhere)` — `- → +` survives:
  `segmented_map_restore.rs:773-776` greps only for `"action":"ambiguous-chunk-id"` and the chunk hex,
  never for `claims` or `withheld`, yet `withheld` is the number that tells the operator the bytes are
  still on disk and that an older backup may be unnecessary. Both are pinnable in the C4-ci-gated
  `restore_reconcile.rs` without touching the discriminator's compile-safety constraint.

- **NEEDS-HUMAN [impl] — the UNREADABLE operator paragraph asserts a fleet-wide fact the report field
  cannot carry.** `crates/server/src/cli.rs:1314-1320` states unconditionally *"Nothing of theirs was
  marked — and nothing anywhere in the fleet was"*. But `report.unresolvable` is the **union** of both
  walks (`restore.rs:296` → `name_unresolvable`, `restore.rs:684-701`), while marking is gated only on
  `referenced.protects` (`restore.rs:339`), i.e. on the *first* walk's `unresolvable`. The second walk
  (`committed_chunks`, `restore.rs:290`) is a separate read of the same records, and `restore.rs:399-402`
  explicitly contemplates running this pass against a live cluster — a `seg:` record removed between the
  two reads lands in `committed.unresolvable` only, leaving the mark gate open. The command then prints
  "nothing anywhere in the fleet was [marked]" on one line and a non-zero `stranded_marked` on the line
  above it. Derive the sentence from `report.stranded_marked` (or from a flag set by the walk that
  actually gated), rather than asserting it.

## Attempted and could not refute

- **Arithmetic underflow at `restore.rs:503`** (`by_id_alone - anywhere`, with `anywhere == placed`
  under ambiguity): both counts filter the *same* `expected.frags` vector and
  `present.contains(&(d,f)) ⟹ present_anywhere.contains(&f)`, so `placed ≤ by_id_alone` elementwise.
  No panic path.
- **Containment dead-code claim** — I expected `referenced_fragments` at `restore.rs:278` to `?` out on
  an undecodable record before `committed_chunks` could contain it. It does not: `gc.rs:378-385` already
  contains decode failures identically. The two walks' decode/resolve/contain arms are line-for-line
  equivalent.
- **The discriminator smuggling in a new symbol** (which would degrade the red to "a symbol is missing"):
  it does not — `segmented_map_restore.rs` names no new field or variant, and the shape assertions on
  `RestoreReport::unresolvable` / `ReconciliationStatus::PendingUnresolvable` correctly live in the
  C4-ci-gated `restore_reconcile.rs:872` and `segmented_map_consumers.rs:719-731`. The reverted-tree run
  above confirms the red is behavioural, not a compile error.
- **`PendingUnresolvable` shadowing `PendingMalformed`** (`desired_state.rs:225-246` ranks unresolvable
  first, so a store with both loses the malformed chunk ids from the answer until the record is repaired):
  iterative but never certifying, and the same fail-safe ordering `PendingMalformed` itself uses. Not a
  defect.
- **`CopyObject`-style legitimate chunk-map sharing making every copied object ambiguous**: not reachable —
  `crates/gateway-s3/src/lib.rs:1725-1730` refuses `x-amz-copy-source` with 501.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether to accept or reslice the current 8-file patch: 1,851 additions mechanically count to about 1,059 nonblank/non-comment lines and 131,233 bytes, beyond the brief's ≤950-line budget and carried 100-KB backstop — scope control matters before another rebuild.
- [ ] C5 Causal adequacy — Rebuild the shared-placement case: `anywhere = placed` credits one ownerless fragment to both same-ID claimants, so the central attribution rule still has a missed case and its same-placement test leaves the named placement empty (`crates/custodian/src/restore.rs:484`; `crates/custodian/tests/segmented_map_restore.rs:712`).
- [ ] T5 Judgment — Decide whether duplicate committed IDs are a supported restore state here or belong to #652: the shipped minter claims cross-object uniqueness, while the documented rewind route describes stale fragment IDs but does not establish the second committed reference this rule requires — the boundary determines whether this is necessary protection or inert false-LOST behavior (`crates/server/src/cli.rs:1796`; `docs/design/architecture/m4-first-deployment-blueprint.md:694`).
- [ ] Validation — fitness-to-purpose — Decide whether the operator behavior is fit for the actual restore topology after exercising a representative restored store: same-placement duplicates currently receive a hollow green and the remaining DANGLING summary can send recovery toward the wrong cause, which directly affects decommission and backup decisions (`crates/custodian/src/restore.rs:489`; `crates/server/src/cli.rs:1266`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 128 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Size budget only nominally exceeded (128KB vs 100KB backstop, ~1,059 vs ≤950 lines) — accepted as-is, not a re-slice trigger. 651 was recently right-sized; some renewed convergence churn is expected. Give it 2-3 more rounds to converge before reconsidering iterate-plan. Carry forward ALL reviewer/adversary/mutant findings for the next Do round to address in place: - C5 causal adequacy (core gap): two committed objects sharing both a chunk ID AND a placement still produce a false-clean report — `anywhere = placed` credits one ownerless fragment to both claimants (restore.rs:484; segmented_map_restore.rs:712). This is the central attribution property the brief asks for — must be closed. - Adversary [human]: the doc-claimed "reachable collision path" (m4-first-deployment-blueprint.md:693-704) does not actually reach `CommittedChunks::ambiguous` (claims stays 1, never >1) — the scenario that IS reachable (dead object's fragment vs. one committed object with empty placement) still misclassifies as `misplaced` instead of `dangling`, which is the exact corruption criterion (4a) exists to prevent. Either name the actual reachable collision path or fold into #652 — this question is still open. - Adversary [impl]: an ambiguous chunk id that never becomes `dangling` is completely silent and the run is certified clean forever (`emit_ambiguous_evidence` only reachable from the `anywhere < k` arm; restore.rs:497-504). Emit `ambiguous-chunk-id` wherever ambiguity actually changed a decision, not only in the dangling arm. - Adversary [impl]: the ambiguity gate sits above the `already`-marked check (restore.rs:358-366 vs :395), so a mark an earlier run wrote is reported "kept" while GC still deletes it — a regression vs. origin/main's honest `already_marked: 1`. Move the gate below the already-marked check. - Adversary [impl]: `is_clean()` has no true-branch assertion anywhere in the tree (restore.rs:178) — `fn is_clean(&self) -> bool { false }` would pass the whole suite. Add a true-branch assertion (single healthy chunk -> is_clean() == true). - Adversary [impl]: two unasserted C5 mutant misses on lines this patch added — restore.rs:359 (`+= -> *=` on displaced_kept in the ambiguous arm) and restore.rs:503 (`- -> +` on the withheld count in emit_ambiguous_evidence) — pin both in the C4-ci-gated restore_reconcile.rs. - Adversary [impl]: UNREADABLE operator paragraph (cli.rs:1314-1320) asserts a fleet-wide fact ("nothing anywhere in the fleet was [marked]") that report.unresolvable (union of both walks) cannot support — derive the sentence from report.stranded_marked instead. - T4 batched rubric review: 3 blocking findings (review-branch) — resolve or address explicitly. - C5 mutants gate: 6 missed of 44 tested — close the gap (ties to the two items above plus any others surfaced). - T5 Judgment (open question, needs resolution alongside the fix, not deferred silently): decide whether duplicate committed IDs are a supported restore state here or belong to #652 — the shipped minter claims cross-object uniqueness but the documented rewind route doesn't clearly establish the second committed reference this rule requires (cli.rs:1796; m4-first-deployment-blueprint.md:694). - Validation / fitness-to-purpose (open question): decide, ideally by exercising a representative restored store, whether same-placement duplicates receiving a hollow green and the DANGLING summary potentially misdirecting recovery are acceptable for the actual restore topology. None of the above are being cleared in §6 here — this is an iterate-do disposition, §6 items remain open for the next round to address or for a future sign-off to clear explicitly. </content>
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
