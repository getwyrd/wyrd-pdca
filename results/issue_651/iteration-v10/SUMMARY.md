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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 51 mutants tested in 2m: 30 caught, 21 unviable

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

Review of issue #651: make post-restore and drain-completeness reporting contained and attributed, and prevent cross-object chunk-id evidence from authorizing false recovery or reclamation conclusions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives falsifiable outcomes for segmented-map containment, named blockers, non-zero operator status, and per-object evidence, so the intended behavior is reviewable. |
| C2 Reproduction (red pre-fix) | PASS | On the clean base the added discriminator compiled and ran 8 tests, with 7 behavior-assertion failures (not missing-symbol failures), including the restore error and unsafe collision cases at `crates/custodian/tests/segmented_map_restore.rs:439`. |
| C3 Change | NEEDS-HUMAN | Decide whether to waive the brief's STOP threshold — the patch stays at 8 files but measures about 1,201 non-blank/non-comment additions against the 950-line budget, materially beyond the prior 1,059-line waiver. |
| C4 Verification (red→green) | PASS | Restoring the patch made all 8 discriminator tests green; format, clippy, build, workspace tests, docs lint/render, typos, machete, all three deny checks, conformance, statics, and the 50-seed DST suite also passed, with the deny cache relocated only to satisfy the sandbox. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether duplicate live chunk IDs in imported/hand-assembled namespaces are a supported restore state here — shipped allocators keep live IDs unique, while the reachable rewind case is one live claimant plus dead bytes and remains deferred to #652 (`docs/design/architecture/m4-first-deployment-blueprint.md:697`). |
| T1 Structure | PASS | The change stays within the eight named files, adds no forbidden `gc.rs`/`scrub.rs`/resolver surface, and keeps the new integration test as the required base-visible discriminator. |
| T2 Shape | FAIL | The operator runbook must describe the two-scan result truthfully — it guarantees nothing fleet-wide was marked while an unreadable object remains, but the CLI explicitly handles marks written when only the second scan becomes unreadable (`docs/design/architecture/m4-first-deployment-blueprint.md:617`, `crates/server/src/cli.rs:1337`). |
| T3 Runtime | FAIL | An ambiguous off-placement copy with no canonical copy exits as “kept” at `crates/custodian/src/restore.rs:394` before the stale-orphan withdrawal at `crates/custodian/src/restore.rs:418`; a focused test left the deletion mark intact, so GC can reclaim the supposedly kept byte. |
| T4 Contribution | NEEDS-HUMAN | Re-run the unavailable `scripts/review-branch` tool before accepting its reported three findings; affected-path merged history and closed PR #647 were rechecked, but the batch review itself could not be independently enumerated in this sandbox. |
| T5 Judgment | NEEDS-HUMAN [impl] | The tests must cover a stale mark when no canonical copy exists — the shipped stale-mark case keeps a canonical copy, bypasses the early displaced return, and therefore overstates withdrawal coverage (`crates/custodian/tests/segmented_map_restore.rs:883`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide fitness on a representative restore: stop writers/custodians, restore metadata to V while retaining current D-server bytes, run the documented `wyrd custodian --reconcile-after-restore ...`, require attributed INCOMPLETE/non-zero output, and verify no orphan mark remains on any ambiguous kept copy; the in-memory seam tests cannot establish that operational outcome. |

### Advisory — adversary

# Adversarial review — issue 651 (advisory, non-gating)

Re-ran the asserted red→green in a scratch clone (`cargo test -p wyrd-custodian --test
segmented_map_restore`): **8/8 green with the patch**, and with `restore.rs` /
`desired_state.rs` / `cli.rs` / both modified test files reverted to `origin/main` the
discriminator **compiles and fails 7/8 on assertions** (the 8th, criterion 4c, is the
"no-collision ⇒ no change" leg and is meant to be green on base). The evidence is real, it
drives the production entry points (`reconcile_after_restore`, `reconciliation_status`), and
it is assertion-red rather than compile-red. The attacks below are on the fix, not the proof.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:394-411`: the **mark withdrawal is
  skipped on the displaced shape**, so the data-losing leg the patch claims to close is still
  open. The `if let Some(holders)` arm `continue`s at `:410` *before* the already-marked
  withdrawal at `:418-441` ever runs, so an ambiguous-id copy that arrives already carrying an
  `orphan:` record keeps it whenever **no** claimant's placement holds the bytes. Concrete
  case, reproduced: two committed objects claim chunk id `0xBB00` with placements `d0`/`d1`,
  neither holds the fragment, the only copy is on unnamed server `d8` and carries a mark an
  earlier run wrote → `RestoreReport { already_marked: 0, displaced_kept: 1, dangling:
  [0xBB00, 0xBB00] }` and `orphan:` **still present**; running `gc::reconcile` immediately
  after (same `GcContext`, past the grace window) **deletes the fragment** — the only copy of
  both claimants — because `gc::ReferenceSet::protection` (`gc.rs:306-318`) has no
  ambiguity clause. That directly refutes `restore.rs:263-264` ("neither marks a copy of the
  id **nor leaves one carrying a mark an earlier run wrote**"), `docs/design/architecture/
  06-runtime-view.md:31` ("a reclamation mark an earlier pass left on one is **withdrawn**
  rather than left for collection") and `m4-first-deployment-blueprint.md:716`. It also breaks
  the `already_marked` contract at `restore.rs:117-123` ("It is still counted here, because it
  still *arrived* carrying a mark"): the fragment is counted as `displaced_kept` and
  `already_marked` stays 0. The test that is supposed to pin this,
  `crates/custodian/tests/segmented_map_restore.rs:882-933`, seeds a copy at `d0` **as well**,
  which is exactly the one arm that reaches the withdrawal — delete `d0.put(...)` from it and
  it goes red on the patched tree. Fix: move the ambiguity/withdrawal decision below the
  already-marked check for the displaced arm too (or hoist the `already` lookup above the
  `canonical` branch).

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:645-647` + `:726`: ambiguity is keyed
  on the number of committed **references**, not on the number of committed **objects**, so a
  single object whose chunk map lists one id twice (two identical/deduped chunks — an import
  artifact exactly as plausible as the cross-object collision this rule targets) is declared
  lost. Reproduced: one committed record with `chunk_map = [ChunkRef{id:0xCC00,..},
  ChunkRef{id:0xCC00,..}]`, its fragment present at its own placement, healthy store →
  `dangling: [0xCC00, 0xCC00]`, `is_clean() == false`, and `restore_verdict` (`cli.rs:1262`)
  prints "2 chunk(s) are LOST" and exits non-zero. Nothing here is unattributable — there is
  only **one** claimant object, so the bytes provably belong to it, and the invariant the brief
  states ("evidence must be attributable to the object that references it") is *satisfied*, not
  violated. The audit line the operator is sent to is wrong in the same way:
  `restore.rs:922` says "claimed by more than one committed **object**" while `claims` counted
  references inside one record. Fix: count distinct owning `inode:` keys per chunk id (the walk
  at `:687-748` already has the key in hand), not `ChunkRef`s. Secondary: `report.dangling`
  now carries one entry **per reference**, so `dangling.len()` in the summary and the
  NEEDS-HUMAN paragraph counts a single chunk id twice.

- NEEDS-HUMAN [human] — `docs/design/architecture/m4-first-deployment-blueprint.md:695-716`
  and `crates/custodian/src/restore.rs:244-254`: the open question the iteration-9 sign-off
  carried forward ("name the actual reachable collision path or fold into #652") is now
  *answered in the diff's own docs* — "Two *live* records still cannot collide", the CLI minter
  packs the inode and the gateway draws a random epoch, and CopyObject is refused
  (`crates/gateway-s3/src/lib.rs:1725-1730`), so no shipped writer can produce two committed
  claimants of one id — yet the rule ships anyway, and its verdict is the most severe one the
  command has (LOST + exit 1). Combined with the previous bullet, the only in-tree-reachable
  trigger of the new rule produces a **false** LOST. This is the architectural/fitness call the
  human deferred twice: keep the inert conservatism (accepting that a corrupt or imported record
  is now reported as data loss rather than as a corrupt record), or fold the whole ambiguity
  rule into #652 and ship only the containment/attribution half this slice was briefed for.

- NEEDS-HUMAN [human] — size against the brief's own stop rule: the diff is **8 files** (the
  cap) and **1,242 semantic added lines** (non-blank, non-comment; 1,203 excluding docs) against
  a `≤ 950` budget whose brief says "If mid-build the tree exceeds this, STOP and hand back a
  proposed split rather than finishing". The trend across rounds is v7 ≈749 → v8 ≈971 → v9
  ≈1,059 → now ≈1,242, i.e. each accepted "nominal" overage has grown; `crates/server/src/cli.rs`
  alone added 232 semantic lines (+315 raw) for a slice whose CLI scope was one summary cell and
  the exit code. Two prior sign-offs accepted this as not a re-slice trigger; at +31% it is no
  longer nominal and deserves an explicit decision rather than a third silent pass.

- Minor (no adjudication needed) — `crates/custodian/src/desired_state.rs:225-246`:
  `PendingUnresolvable` returns before the `PendingMalformed` check, so on a store carrying both
  blockers the malformed chunk ids are withheld from the answer. Consistent with how
  `PendingMalformed` already ranks under `Pending`, and the operator re-polls after repairing the
  named record, so it is a ranking choice rather than an unattributed stall — noted only because
  the slice's stated invariant is that a refusal names *what* to repair.

## Attacked and could not refute

- The red→green itself: re-ran both legs (above). The discriminator names no symbol this patch
  introduces, so the base leg is genuinely assertion-red, and every leg calls the production
  functions over the real `metadata::resolve_chunk_map` / `gc::referenced_fragments` path — no
  parallel re-implementation, no mock of the defect.
- `is_clean()` now has a true-branch assertion (`crates/custodian/tests/restore_reconcile.rs:280`),
  so the "`fn is_clean(&self) -> bool { false }` passes everything" hole from the last round is
  closed; `restore_verdict` derives the exit code from `report.needs_human()` itself
  (`crates/server/src/cli.rs:1360`) and `cli.rs:2720-2790` pins each finding one at a time.
- The claim ordering in `committed_chunks` (`restore.rs:722-729`) — counting a reference *before*
  the malformed-placement skip — is right, and `a_malformed_placement_still_claims_its_chunk_id`
  pins it; making it fail requires counting after the skip.
- `emit_ambiguous_evidence` is now reachable from every ambiguous verdict (`restore.rs:569-581`),
  not only the `anywhere < k` arm, so the "silent forever, certified clean" path from the previous
  round is gone.
- Scope: `gc.rs` and `scrub.rs` are untouched, no `crate::resolve` module, nothing written to a
  chunk map, `RestoreReport::dangling` / `misplaced` keep their `Vec<ChunkId>` shape — the
  out-of-scope list holds.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether to waive the brief's STOP threshold — the patch stays at 8 files but measures about 1,201 non-blank/non-comment additions against the 950-line budget, materially beyond the prior 1,059-line waiver.
- [ ] C5 Causal adequacy — Decide whether duplicate live chunk IDs in imported/hand-assembled namespaces are a supported restore state here — shipped allocators keep live IDs unique, while the reachable rewind case is one live claimant plus dead bytes and remains deferred to #652 (`docs/design/architecture/m4-first-deployment-blueprint.md:697`).
- [ ] T4 Contribution — Re-run the unavailable `scripts/review-branch` tool before accepting its reported three findings; affected-path merged history and closed PR #647 were rechecked, but the batch review itself could not be independently enumerated in this sandbox.
- [ ] T5 Judgment — The tests must cover a stale mark when no canonical copy exists — the shipped stale-mark case keeps a canonical copy, bypasses the early displaced return, and therefore overstates withdrawal coverage (`crates/custodian/tests/segmented_map_restore.rs:883`).
- [ ] Validation — fitness-to-purpose — Decide fitness on a representative restore: stop writers/custodians, restore metadata to V while retaining current D-server bytes, run the documented `wyrd custodian --reconcile-after-restore ...`, require attributed INCOMPLETE/non-zero output, and verify no orphan mark remains on any ambiguous kept copy; the in-memory seam tests cannot establish that operational outcome.
- [ ] `crates/custodian/src/restore.rs:394-411`: the **mark withdrawal is
- [ ] `crates/custodian/src/restore.rs:645-647` + `:726`: ambiguity is keyed
- [ ] `docs/design/architecture/m4-first-deployment-blueprint.md:695-716`
- [ ] size against the brief's own stop rule: the diff is **8 files** (the
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 156 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Send back to Plan primarily to settle the architectural fork this bundle has now raised twice: whether the cross-object chunk-id ambiguity rule is a real, reachable hazard worth keeping as conservative-but-occasionally-wrong behavior, or whether it should fold into #652 (stop id reuse at the source) and this slice ship only the containment/attribution half it was originally briefed for. The diff's own docs now state that two *live* records cannot collide given the current minter/epoch/CopyObject-refusal design, and the adversarial review found that the only in-tree-reachable trigger of the new ambiguity rule (a single object whose chunk map lists one id twice) produces a FALSE "LOST" verdict — so the rule as shipped is inert against its intended target and actively wrong on the one case it can hit. That is a scope/design decision, not an implementation bug a rebuild can resolve on its own. Also feeding this: the size backstop (156KB, round 2 of a 2-round threshold, semantic lines 1,242 vs. the brief's own 950-line stop rule, climbing every round: 749->971->1,059->1,242) and a still-open real bug (mark-withdrawal skipped on the displaced shape, restore.rs:394-411, reproduced to cause GC to delete a live copy) that a re-plan should size correctly into whichever child slice ends up owning it, rather than patching again inside an oversized bundle. At re-plan: settle the ambiguity-rule question first (keep vs. fold into #652), then split the remaining containment/attribution work and the mark-withdrawal fix into properly-sized child briefs via `pdca split`.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
