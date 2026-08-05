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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — PASS on confirm — first run failed transiently: xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit stat
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 37 mutants tested in 2m: 3 missed, 18 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '-p', 'MemoryHigh=8G', '-p', 'MemoryMax=16G', '-p', 'MemorySwapMax=0', '-p', 'OOMPolicy=continue', '--', 'codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '-m', 'gpt-5.6-sol', '-c', 'model_reasoning_effort=xhigh', '--add-dir', '/home/eddie/wyrd/wyrd.pdca-wt-l0', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — adversary

# Adversarial review — issue 651 (advisory; never gates)

Method: re-ran the asserted red→green in a scratch copy of `$PDCA_TARGET`
(`pdca-adversary-651-collide`, since removed) with `CARGO_TARGET_DIR` in scratch, and probed the
fix with three new scenarios driven through the **production** entry point
(`wyrd_custodian::reconcile_after_restore`). Toolchain was available; nothing below is a
"could not reproduce".

**The evidence holds.** With the patch, all 7 legs of `crates/custodian/tests/segmented_map_restore.rs`
pass; with `restore.rs` / `desired_state.rs` / `cli.rs` reverted to `origin/main` and the test file
kept, **6 of 7 fail on assertions** (not on a missing symbol — the file compiles against the base),
and (4c) passes both ways as the intended non-regression guard. `C4-verify`'s "red without the fix,
green with it" is real and the discriminator exercises production, not a mirror.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:665-675` (and `:303-306`, `:339-342`): the
  ambiguity test keys on *how many D servers are named*, not on *how many committed references exist*,
  so a colliding chunk id whose two objects share a placement defeats criterion (4) entirely — both
  legs, including the data-losing one.** `referenced.placed` is a `HashSet` (`crates/custodian/src/gc.rs:267`),
  so two committed references naming the same server collapse to `canonical[frag] == [d]`; then
  `holders.iter().all(...)` at `:340-341` is identical to the base's `any`, and `attributable`'s
  `holders.iter().all(|&d| d == dserver)` at `:674` returns `true`. Concrete failing store, **run on the
  patched tree**: `inode:2` and `inode:3` both committed with `ChunkRef { id: C, scheme: None, placement: vec![0] }`;
  D server 0 holds `(C,0)`; D server 8 — named by no placement — holds object 3's only copy.
  Observed: `RestoreReport { stranded_marked: 1, .. }` **and an `orphan:` record written for `(8, (C,0))`** —
  i.e. GC will delete the second object's only copy on the next grace window, which is criterion (4b)
  verbatim ("no `orphan:` record is written for it"). Second run, same two objects with server 0 empty
  and the only bytes on server 8: `misplaced: [C, C]`, `dangling: []` — the "the bytes are one hop away,
  restage them" verdict criterion (4a) forbids, emitted for **both** references. This is not the exotic
  shape: the M0–M2 identity route places fragment `index` on D server `index`
  (`crates/core/src/write.rs:80`, `crates/core/src/placement.rs:5`) and an empty `placement` vector decodes
  to that same identity fallback (`crates/custodian/src/gc.rs:418-426`), so two objects sharing an id
  normally name the **same** servers; the divergent placement both fixtures seed
  (`crates/custodian/tests/segmented_map_restore.rs:701-702`, `crates/custodian/tests/restore_reconcile.rs:934-935`)
  is the *narrower* case. The brief's own scope sentence is broader than the code — "a chunk id that more
  than one committed object references is ambiguous, and authorizes neither a recoverability verdict …
  nor a reclamation mark on **any** copy of it" — and the information needed is already in hand:
  `committed_chunks` (`restore.rs:545`) returns **one entry per reference**, so `>1` reference under one
  chunk id is derivable without a second read.

- **NEEDS-HUMAN [human] — the premise the whole of criterion (4) rests on is not supported by the code it
  cites, and the patch pays a real false-`LOST` price for it (`crates/custodian/src/restore.rs:231-240`).**
  The doc comment (and the brief) say ids "are minted from the inode counter the restore rewound … so two
  committed objects can carry the same id". But `chunk_id_minter` packs the **inode id** into the high 64
  bits (`crates/server/src/cli.rs:1788-1797`), and the inode id *is* the record key
  (`crates/core/src/metadata.rs:34`) — so two *live committed* records cannot share an id by that route;
  the gateway path mints per-process epochs ≥ 2^127 (`crates/server/src/lib.rs:229-241`), the in-process
  path resumes above every committed id (`crates/core/src/metadata.rs:2073-2093`), and
  `seed_next_inode_floor` (`crates/server/src/cli.rs:1758-1765`) raises the counter past every committed
  inode at gateway start. The fixtures create the shape by writing two raw `inode:` records
  (`segmented_map_restore.rs:701-702`), which does not demonstrate reachability. This is a squeeze, not a
  quibble: **either** the collision is reachable by a path nobody has named — and then finding 1 is a live
  data-loss gap on the commonest form of it — **or** it is not, and the new conservatism is inert code that
  still bills the operator. Measured cost, run on the patched tree: two committed objects sharing id `C`
  with *divergent* placements and the bytes genuinely displaced to an unnamed server (`displaced_kept: 1`,
  bytes on disk) now reports `dangling: [C, C]`, where the base reported `misplaced` — the CLI prints
  "**2 chunk(s) are LOST** … no reconstruction can rebuild them" for one chunk id whose bytes it just
  counted as kept. A human should name the reachable path (or re-scope criterion 4 to #652) rather than let
  the round-7 reviewer's "criterion (4) settled" stand on the brief's assertion alone.

- **NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:1284-1290`: the operator-facing DANGLING paragraph now
  states a cause that the same patch concedes is false.** It still says, unconditionally, "Restoring past a
  delete resurrects the map after GC took the bytes; no reconstruction can rebuild them" — but the new
  ambiguity-induced `dangling` (`restore.rs:462-473`) fires precisely when the bytes **do** exist, and
  restaging them is actively harmful. This patch already hedged the *runbook*
  (`docs/design/architecture/m4-first-deployment-blueprint.md`: "Usually you restored past a delete …
  Do not restage those"), so the diff concedes the story changed; the operator who reads only stderr — the
  surface this slice exists to fix — gets the old story, and the only place the difference is stated is the
  audit log (`emit_ambiguous_evidence`). This is not the new CLI *cell* the brief declines out of scope;
  it is the accuracy of a sentence the patch moved and reprinted. One hedging clause fixes it.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_restore.rs:554-559`: criterion (2a)'s binding
  assertion is justified by a coupling that does not exist.** The leg reads "`is_clean` is what the operator
  command exits on"; it is not — `cmd_custodian` exits on `restore_verdict(&report).needs_human`
  (`crates/server/src/cli.rs:1203`, `:1329-1333`), an independent predicate that deliberately ignores
  `stranded_marked` / `under_replicated`. `RestoreReport::is_clean` (`crates/custodian/src/restore.rs:173-180`)
  has **no production caller at all** (grep: only tests), so criterion (2a) is pinned on a predicate nothing
  reads, while `restore_verdict`'s own doc (`cli.rs:1250-1257`) claims the findings "are judged by the same
  predicate that prints them". Net effect: a future report field added to `is_clean()` will never reach the
  exit code. Fix the claim, or make one predicate load-bearing.

## Attempted and could not refute

- Tried to make the mark half delete *more* than the base: `all` implies `any` whenever
  `canonical.get(&frag)` is `Some` (non-empty by construction, `restore.rs:303-306`), so the mark leg is
  strictly more conservative; no new deletion path exists.
- Tried to underflow `by_id_alone - anywhere` (`restore.rs:472`): every element counted in `anywhere` is
  counted in `by_id_alone` (`present ⊆ present_anywhere`, `restore.rs:408`), so the subtraction cannot wrap.
- Tried to make `committed_chunks` (`restore.rs:545-600`) diverge from `gc::referenced_fragments`
  (`gc.rs:360-455`) so that `canonical` would lack an entry `attributable` needs (turning a healthy chunk
  `dangling`): the two walk the same prefix with the same decode-contain / resolve-contain /
  `checked_fragments` treatment, so `(dserver, frag) ∈ placed` for every `Expected::frags` entry.
- Tried to make the containment swallow a genuine store fault (the standing rejection (ii)):
  `restore.rs:578-590` downcasts to `ChunkMapError` and re-raises everything else, mirroring `gc.rs:405-415`.
- Tried an exhaustiveness/silent-fallthrough break from the new public `ReconciliationStatus::PendingUnresolvable`
  (`desired_state.rs:119-124`): no `match` on the enum exists outside tests, and `reconciliation_status` has
  no production caller today, so nothing degrades it to a `_` arm.
- `assert_attributes_blocker` (`segmented_map_restore.rs:244-258`) proves target, action and inode with three
  independent `contains` over the whole capture, so it would also pass if the three came from three different
  events; in these fixtures they do coincide on the one `unresolvable-chunk-map` line, so it is a durability
  nit rather than a false green — not raised as a finding.
- `check-gates.json` records `C4-ci` as `attempts: ["fail","pass"], flaky: true` with a truncated reason
  (`cargo test --workspace --exclude wyrd-dst` failed on the first run). Not refutable from the inputs here
  and not counted against the patch — but worth noting that the first failure is unattributed, and this
  bundle adds tracing-capture-dependent tests, a known flake class in this repo (issue #214, cited at
  `segmented_map_restore.rs:208-222`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] leaf produced no usable verdict (needs a human) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
- [ ] size backstop — this slice is behaving oversized: patch is 116 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human note: the prior iteration-v1..v7 history predates this slice's split at Plan; this bundle is a fresh convergence attempt post-split, not evidence of an unbounded/oversized slice. The size overage (971 semantic lines vs. the brief's ~950 budget, and 116KB vs. the 100KB backstop) is slight, not the kind of overage that warrants sending the whole thing back to Plan — keep it as one iterate-do round. What to fix, corroborated independently by both the adversary review and the freshly re-run decorrelated reviewer (T3 Runtime: FAIL, T4 Contribution: FAIL): the ambiguity/containment rule in crates/custodian/src/restore.rs (~229, ~254, ~665-675) keys on "how many D servers are named" (a HashSet of placements) rather than "how many committed references exist" for a shared chunk id. Two committed objects that reference the same chunk id but happen to share a placement server defeat the ambiguity check entirely: the mark half still marks an extra unnamed copy collectable even though it may be the *other* object's only correct bytes (data loss via GC), and the report half can emit `dangling` for bytes that are actually present and safe to restage, or `misplaced` for bytes that are not actually recoverable at that placement. Rebuild the rule to be conservative on reference-count ambiguity (>1 committed reference under one chunk id), not holder-count, using `committed_chunks`'s one-entry-per-reference data that is already read. Also worth resolving in the rebuild, per the adversary review's second finding: the doc/brief premise that live committed objects can collide on chunk id via `chunk_id_minter` looks unsupported by the current minting code (inode id is packed into the id and inode ids don't repeat across live records) -- either name the actual reachable collision path, or fold this concern into #652 rather than carrying inert conservatism that produces false-LOST reports in the CLI. Also fix, per the adversary review's third/fourth findings: the CLI's DANGLING paragraph (crates/server/src/cli.rs:1284-1290) still unconditionally claims lost bytes are unrecoverable even though the new ambiguity-induced `dangling` can fire when bytes are present and restaging is actively harmful -- hedge that sentence; and criterion (2a)'s test justification cites `is_clean()`, which has no production caller (cmd_custodian exits on `restore_verdict`'s own `needs_human` predicate) -- fix the claim or make one predicate load-bearing.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
