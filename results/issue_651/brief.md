# Brief — issue 651 / restore-and-desired-state-contained-and-attributed

> Slice **4a of 7** of the #635 re-slicing (0016 decision 7(e)/(f)). This issue was **re-scoped
> on 2026-08-02** after five Do iterations were sent back to Plan: it carried restore,
> reconstruction, backfill, rebalance *and* `desired_state` onto one shared resolver/repoint
> path, and every round surfaced a fresh batch of blockers at ~2.8× the line budget (4,202 added
> lines / 13 files). The other two thirds are now **#681** (the resolving namespace walk for
> reconstruction / backfill / rebalance, read side) and **#682** (`repoint_chunk` + the record
> ceilings + the repair/evacuation write path). History and closed PR #647 are on
> https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** restore-and-desired-state-contained-and-attributed
- **Defect:** The two operator-facing surfaces that report **whether a reconciliation is
  complete** cannot survive — let alone describe — an object whose chunk map they could not
  read. #650 built the containment they need (`gc::ReferenceSet`) and deferred both here **by
  name, in its own code**.
  1. **Post-restore reconciliation fails the whole pass closed.** `reconcile_after_restore`'s
     *mark* half is already safe — it withholds every fragment while the reference set is
     incomplete (`gc::ReferenceSet::protects`, `restore.rs:239`) — but its *report* half re-reads
     every committed record through `committed_chunks` (called at `restore.rs:326`), which `?`s
     out on `ChunkMap::Segmented` (`restore.rs:390`, `:403-405`). So a store holding **one**
     segmented object, or one structurally
     unreadable committed root, returns `Err` and the operator command produces **no report at
     all**: not a stranded count, not the dangling/misplaced chunks of the objects it *could*
     read. One damaged object blanks the whole answer. #650 states the gap at `restore.rs:196`:
     *"deferred: #651 — the **contained** answer for this surface (report every object it could
     read, name the one it could not, and say the run is not certified …) belongs to the slice
     that owns restore."*
  2. **The drain surface cannot distinguish "not converged" from "I could not look".**
     `reconciliation_status` answers a bare `Pending` when the reference set is incomplete
     (`desired_state.rs:188-190`) — the same word it uses for a server that genuinely still holds
     referenced fragments. An operator watching a decommission stall has no way to learn *which*
     record is blocking it, so the stall is a state nothing exits. The tree already has the
     opposite pattern one level up: a malformed placement answers `PendingMalformed { chunks }`,
     naming the blockers in the answer itself (`desired_state.rs:101-104`, from merged #397).
     #650 states the gap at `desired_state.rs:183-187`: *"deferred: #651 — the **ATTRIBUTED**
     answer for this surface … lands with the slice that owns `desired_state`."*
  3. **The operator surface would print a hollow green.** `wyrd custodian` renders the report and
     exits non-zero only on `dangling` / `misplaced` (`crates/server/src/cli.rs:1196-1236`). An
     incomplete reading has no cell in that summary and no effect on the exit code — so once (1)
     stops erroring, a restore script checking the status code would record a run that could not
     read part of the store as a healthy one. That is precisely the failure mode the existing
     comment at `cli.rs:1230-1233` refuses for lost data — it even names "one whose chunks cannot
     be read", while the `if` below it at `:1234` does not test for that case.
- **Success criterion:** The added test target `crates/custodian/tests/segmented_map_restore.rs`
  passes, driven **only** through entries already visible on this slice's base
  (`wyrd_custodian::{reconcile_after_restore, RestoreReport, GcContext}`,
  `wyrd_custodian::desired_state::{reconciliation_status, ReconciliationStatus}`):
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
       scenario carrying a loss would satisfy this clause **without the fix** — it must be
       asserted on a store where the unreadable object is the only reason.
     - **(2b) containment — the damaged object does not starve the healthy ones.** The same
       unreadable object seeded **beside** a readable object that has a genuine loss: the pass
       still returns `Ok` and still reports that readable object's loss (`dangling` or
       `misplaced` names its chunk), and still marks nothing of the unreadable one.
  3. **The drain surface tells the two Pendings apart.** `reconciliation_status` over that same
     store attributes the blocking object — the operator can name the record to repair — instead
     of the unattributed `Pending` the base answers. Assert this on the **audit/tracing seam**,
     the way #650's own fixture does (`assert_attributes_blocker`), so this leg needs no symbol
     the base lacks.

  Criterion (2) is the binding one — it is the whole point of the slice, and the one an
  incomplete fix passes vacuously. **Both** (2a) and (2b) must ship: (2a) alone does not show the
  walk continues, and (2b) alone does not show the run is non-certifying. A version that only
  checks the call returned `Ok` proves neither.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no dev-dependency.
  - **Base.** This bundle has no `Onto branch` and its stale `stack-base` marker has been
    removed, so `run-verify.sh:_resolve_base_ref` falls through to the brief base →
    `origin/main` (`run-verify.sh:186-192`).
  - **Base prerequisite — SATISFIED 2026-08-02.** `origin/main` (`d50f0ca`) now carries
    **#648** (PR #672), **#649** and **#650** (PR #683, which replayed them off the driver's
    force-pushed `pdca-integration/main` wave branch). Verified on the base: `resolve_chunk_map`
    in `core/src/metadata.rs`, `ReferenceSet::unresolvable` in `custodian/src/gc.rs`, and
    `custodian/tests/segmented_map_consumers.rs` all present. So the containment vocabulary this
    slice extends exists on the base, and the criterion can be both expressed and made red.
  - **Discriminator.** `run-verify.sh --classify` was dry-run at Plan on the expected file set
    and returns exactly one: `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs`
    (plus `CRATE crates/custodian`, `CRATE crates/server`). With one added test the invocation is
    `cargo test -p wyrd-custodian --test segmented_map_restore` (`run-verify.sh:301-311`), so the
    reverted `crates/server` change cannot break the leg.
  - **The RED leg** keeps that file and reverts `restore.rs`, `desired_state.rs`, `cli.rs` and
    every modified test file (`run-verify.sh:404-412`). On that tree `committed_chunks` still
    fails closed, so criteria (1) and (2) fail on `Err`, and (3) fails because no attribution is
    emitted.
  - **Keep the discriminator assertion-red — this is a hard constraint.** The file MUST NOT
    reference any symbol this patch introduces (no new `RestoreReport` field, no new
    `ReconciliationStatus` variant, no new helper): the RED leg reverts the production files, so
    such a reference makes the target fail to *compile* and the red degrades to "a symbol is
    missing" instead of "the behaviour was wrong". Any coverage that genuinely needs a new public
    shape — match-exhaustiveness, the field's own contents — ships in the **existing**
    `crates/custodian/tests/restore_reconcile.rs` / `rebalance.rs`, which `C4-ci` gates.
  - No `#![cfg(...)]` on `crates/custodian/tests/*.rs`, so neither the vacuous `0 tests … ok`
    branch (`run-verify.sh:383-389`, `:420-427`) nor a compile-red-scored-as-pass can occur.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the surfaces that report whether a reconciliation
  is complete**. Sourced, not intuited — this slice falls squarely in the §6 category row
  *Storage lifecycle / reclamation* (`docs/principles.md:137`), which names **restore** and
  **drain** explicitly and whose own sources are §5 C-1 (`docs/principles.md:109`), the
  maintainer's rule of 2026-07-25, `0016:2802-2813`, and `gc.rs:22-25`. Over that category:
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
  - **A non-zero exit is part of the report.** A surface that prints a caveat and exits 0 has
    told the automation the run was healthy.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`, matching the sandbox's
  `origin/main`. INTEGRATION §2's default — a milestone integration branch was considered and
  **dropped**: #648 landed on `main` directly (PR #672), so keeping the rest off `main` bought
  nothing, and each slice is individually gated and self-consistent. **Not**
  `pdca-integration/main`: that is the driver's run-scoped wave-fold branch, regenerated and
  force-pushed per run, and it silently outranks this field via the bundle's `stack-base` marker
  — which is why v5 built on a tree that had lost #650. That marker has been removed from this
  bundle.)
- **Depends on:** 650
- **Conflicts with:** 681
- **Ordering note:** `Depends on 650` — this slice's entire subject is the two consumers of the
  `gc::ReferenceSet` #650 built, and #650's code defers both here by name (`restore.rs:196`,
  `desired_state.rs:183`). `Conflicts with 681` — no dependency in either direction (681 owns
  reconstruction / backfill / rebalance, this one owns restore / desired_state), but both are
  likely to edit `crates/custodian/tests/rebalance.rs` — #681 because it changes rebalance, this
  slice because `ReconciliationStatus` is matched there at 15 sites — so they must not be built
  blind on the same base. #682 depends on #681 and touches neither of
  this slice's files.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** make both completeness-reporting surfaces answer **contained and attributed** over a
  reference set that has a hole in it, instead of erroring or certifying.
  `crates/custodian/src/restore.rs` — the report half survives an object it cannot read: it
  reports every object it *could* read, records the ones it could not, and the pass is not
  clean. `crates/custodian/src/desired_state.rs` — the drain status distinguishes "still holds
  referenced fragments" from "could not read object X", and names X. `crates/server/src/cli.rs`
  (and `custodian.rs` if the report shape reaches it) — the operator summary states an
  incomplete reading and the command's exit status reflects it. Plus their existing test files,
  the added discriminator below, and the docs-currency paragraph.
  **Out of scope:** reconstruction, backfill and rebalance, and the resolving namespace walk they
  need (**#681**) — this slice adds **no** custodian-level walk and no `crate::resolve` module,
  and reads the reference set through `gc::referenced_fragments` exactly as the base does;
  `repoint_chunk` and the record ceilings (**#682**) — this slice writes **nothing** to a chunk
  map; the chunk-id floor (#652); the committer, fence, rollback and resume (#653); any
  new/edited ADR / spec / proposal; any conformance-vector change.
- **Budget:** ≤ **700** added semantic lines (non-blank, non-comment, non-mechanical), ≤ **8**
  files — deliberately far tighter than the ~1,500 / 15 the un-split issue carried, and it is the
  point of the split. Salvage production is ~130 lines (see Citations); the discriminator's own
  fixture is the only real risk — **prune it to what criteria (1)–(3) need** and no more. If
  mid-build the tree exceeds this, STOP and hand back a proposed split rather than finishing: an
  over-budget patch is iterate-to-Plan by default, not another Do round.
- **Repro instruction:** on the base,
  `git -C ../wyrd show origin/main:crates/custodian/src/restore.rs` (once #650 is on `main`) —
  `committed_chunks` at `:390` reaches `.as_flat().ok_or(SegmentedMapUnsupported)` at `:403-405`,
  and `reconcile_after_restore` calls it unconditionally at `:326`, so seeding any `seg:`-backed
  committed root makes the whole pass return `Err`. For defect (2),
  `crates/custodian/src/desired_state.rs:188-190` returns a bare `Pending` for a non-empty
  `ReferenceSet::unresolvable`. For defect (3), `crates/server/src/cli.rs:1196-1236` prints
  "post-restore reconciliation complete" and gates the exit code on `dangling`/`misplaced` only.
  Until #650 is merged into the target branch these line numbers read off
  `origin/fix/650-gc-scrub-through-resolver-fail-closed-containment`, which is the same content.
- **External dependencies:** `typos`, `docs-renderer` — both registered as doctor.checks rows in
  pdca.toml (ids "typos" at :421 and "docs-renderer" at :428), named because this slice edits a
  living-architecture paragraph and `cargo xtask ci`'s prose gates warn-and-skip locally (INTEGRATION
  §3). Nothing else beyond the base Rust toolchain: the pass runs over the `traits`/`core` seams
  with in-memory doubles. No Docker, no protoc, no live backend, no new dev-dependency, and no
  DST leg in this slice.
- **Test file:** `crates/custodian/tests/segmented_map_restore.rs` — a **NEW** file, and this is
  not optional. `C4-verify` classifies its discriminator on an **added** `*/tests/*.rs`
  (`run-verify.sh:92-93`, `_added_files` keys on `--- /dev/null`), and
  `crates/custodian/tests/segmented_map_consumers.rs` and `restore_reconcile.rs` both already
  exist on this base — appending to either makes it a *modified* file, and the gate takes the
  green-only branch (`run-verify.sh:392-402`) and proves no red at all. Verified by the
  `--classify` dry-run recorded under Falsifiability. Updates to the existing per-pass test files
  may ship **in addition**; `C4-ci` covers them.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. No deferred or off-Check leg.
- **Citations expected:** cite `path:line` on the target branch for every change.
  **Line numbers below were re-verified against `origin/main` at `d50f0ca`** — every one
  resolves to the symbol named. Two base advances have been checked since they were first read:
  PR #683 (the #649/#650 replay) touched only `core/src/metadata.rs` and `server/src/lib.rs`, and
  the three Dependabot merges (#678/#679/#680) touched only `.github/workflows/*` and
  `Cargo.lock` — so nothing in `custodian/` or `server/src/cli.rs` has moved. Still cite by
  symbol, not by number, if the base advances again.
  **Peer callsites Do MAY open — this is a composition slice, mirror them rather than inventing
  a shape:**
  - `crates/custodian/src/gc.rs:265-294` (`ReferenceSet`, and its `unresolvable:
    BTreeMap<Vec<u8>, String>` keyed by **raw key bytes** — with the reason that keying by a
    rendered name is not injective), `:306-334` (`protection` / `protects`), `:470`
    (`object_name`, which escapes rather than replaces), `:563` (`emit_unresolvable`, the audit
    seam), and `:234-241` (`Reconciled::Blocked` — the non-certifying answer, and *why*). This is
    the containment vocabulary this slice extends; **reuse it, do not re-derive it**.
  - `crates/custodian/src/desired_state.rs:91-104` — `PendingMalformed { chunks }`, the
    attribution-in-the-answer shape already in the tree (merged #397). The unresolvable case is
    the same rule one level up.
  - `crates/server/src/cli.rs:1196-1236` — the operator summary, its NEEDS-HUMAN paragraphs, and
    the exit-code comment at `:1230-1233` and its `if` at `:1234` — which already spell out why a
    caveat plus exit 0 is a hollow green, and already say "or one whose chunks cannot be read".
  **Salvage — permitted inputs, both in this bundle:**
  `results/issue_651/iteration-v5/patch.diff` carries the most-reviewed prior draft of exactly
  these two files (`crates/custodian/src/restore.rs`, `crates/custodian/src/desired_state.rs`,
  ~130 semantic lines through five review rounds). **Take the shape — the
  `PendingUnresolvable { objects }` variant, the report field, the doc prose — but re-point every
  callsite:** those hunks are written against a `crate::resolve` module (`homed_objects`,
  `protected_fragments`, `MaintenanceWalk`, `resolve::object_name`) that belongs to **#681 and
  does not exist here**. On this base the same values come from `gc::referenced_fragments` /
  `gc::ReferenceSet` / `gc::object_name`. Pulling `crate::resolve` in would re-create #681 inside
  this slice and blow the budget — that is the single most likely way this bundle fails again.
  `results/issue_651/sources/salvage.diff` (closed PR #647) is the fallback for the same regions
  and for fixture idiom. **Do not** carry over anything touching reconstruction, backfill,
  rebalance, `repoint_chunk` or the record ceilings.
  Fixture idiom to mirror: `crates/custodian/tests/segmented_map_consumers.rs` (#650) —
  `MemMeta` / `MemDServer` / `PoisonedMeta`, `seed_segmented`, and the tracing capture
  (`capturing_dispatch`, `attributed_objects`, `assert_attributes_blocker`). Integration-test
  crates cannot import across files, so the discriminator carries its own **pruned** copy of only
  what criteria (1)–(3) need.
  Normative: 0016 decision 7(e) `:2393-2415` (verified: that line is decision 7(e)'s heading —
  every maintenance consumer resolves the segmented shape in **bounded** work, reading the root
  and, if `Segmented`, only the bounded `seg:<nonce>:<epoch>:` range, never a global `seg:` scan);
  `docs/principles.md:109` (§5 C-1) and `:137` (the §6 category row).
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — extend the
  containment paragraph #650 started with **the two reporting sentences this slice lands, and
  only those**: post-restore reconciliation reports every object it could read, names the ones it
  could not, and does not certify the run; and a drain status distinguishes "still holds
  referenced fragments" from "a committed object could not be read", naming the blocker. Nothing
  about repair, evacuation or staged publication — those are #681 / #682 / #653.
- **Prior-art check (triage cycles):** searched by affected file path (`restore.rs`,
  `desired_state.rs`) across merged history and open/closed PRs. Six PRs touch them.
  **#647 is the only one addressing this concern and it is CLOSED unmerged** — closed on
  reviewability, not correctness, which is why it is a salvage source. Of the merged ones, two
  bind criteria here and neither is superseded: **#555** established the `RestoreReport` /
  `stranded_marked` contract that criterion (1) extends to segmented and unreadable objects, and
  **#397** established `PendingMalformed`, the attribution shape criterion (3) mirrors one level
  up; **#193** originated `ReconciliationStatus`. **#672 / #676** are the in-flight prereqs
  (#648 / #650). No prior art routes restore's report half through the resolver, and none
  attributes an *unresolvable* object on either surface.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` / `MetadataStore` implementation owns the network bound, not the caller
  (`crates/traits/src/lib.rs:1000-1012`); no custodian await carries one, and this slice does not
  start. (ii) *"a genuine store fault should be contained too"* — no: a store fault propagates
  (`#650`'s `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed`); only a
  *record-level* read failure is contained. (iii) *"`Completed` releases its admission slot"* —
  withdrawn as unsatisfiable. Do MUST record each rejection in `review-rejected.md` **at every
  line the finding is reported at**, in the gate's `<file:line> | <CLASS> | <MATCH> | <reason>`
  format.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Carry-forward — why this brief is narrow

Five Do iterations on the un-split issue were rejected; the archived attempts are in
`iteration-v1/` … `iteration-v5/`. The pattern was not a builder failure — each round fixed its
findings and earned new ones, because one bundle carried three separable kinds of work. Of
iteration 5's seven blockers, **none is in this slice's scope**: two memory-blowup risks and two
containment breaks in the shared namespace walk (→ #681), a duplicate-reference gap and an
inflated success count in the repair/evacuation path (→ #682). They are recorded as named
must-not-recur constraints on those two issues. Do not re-import that machinery here.
