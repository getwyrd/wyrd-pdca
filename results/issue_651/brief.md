# Brief — issue 651 / restore-and-drain-report-contained-and-attributed

> Slice **4a of 7** of the #635 re-slicing (0016 decision 7(e)/(f)). **Re-scoped 2026-08-03**, after
> ten Do iterations, by dropping the *cross-object chunk-id ambiguity* apparatus the builder grew
> from round-6 review findings: never briefed, guards a state the shipped minters cannot produce,
> and its only reachable trigger reports a **healthy** object as LOST (§ Out of scope). Every
> unresolved blocking finding at v10 — `review-batch.md` ×3, T3, T2, two adversary NEEDS-HUMANs —
> lived in it. This ships the half the issue was opened for, which is the half #650 deferred here
> by name in its own code. History: https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** restore-and-drain-report-contained-and-attributed
- **Defect:** The two surfaces that report **whether a reconciliation is complete** cannot survive —
  let alone describe — an object whose chunk map they could not read.
  1. **Post-restore reconciliation fails the whole pass closed.** The *mark* half of
     `reconcile_after_restore` is already safe (it withholds every fragment while the reference set
     is incomplete — `gc::ReferenceSet::protects`, `gc.rs:331`). Its *report* half re-reads every
     committed record through `committed_chunks` (`restore.rs:326`), which `?`s out on a segmented
     map (`restore.rs:390`, `:403-405` `.as_flat().ok_or(SegmentedMapUnsupported)`). So a store
     holding **one** segmented object — or one unreadable committed root — returns `Err`, and the
     operator command produces **no report at all**: not a stranded count, not the dangling or
     misplaced chunks of the thousands of objects it *could* read. One damaged object blanks the
     whole answer. #650 marks the gap in-tree at `restore.rs:196`.
  2. **The drain surface cannot tell "not converged" from "I could not look."**
     `reconciliation_status` answers a bare `Pending` over an incomplete set
     (`desired_state.rs:188-190`) — the same word it uses for a server that genuinely still holds
     referenced fragments. An operator watching a decommission stall cannot learn *which* record
     blocks it, so the stall is a state nothing exits. The opposite pattern already sits one level
     up: `PendingMalformed { chunks }` names its blockers in the answer itself
     (`desired_state.rs:101-104`, #397). #650 marks the gap at `desired_state.rs:183-187`.
  3. **The operator surface would print a hollow green.** `wyrd custodian` exits non-zero only on
     `dangling` / `misplaced` (`cli.rs:1197-1237`). An incomplete reading has no cell in the summary
     and no effect on the exit code — so once (1) stops erroring, a restore script checking the
     status code records a run that could not read part of the store as healthy. The comment at
     `cli.rs:1230-1233` already refuses exactly this, naming *"one whose chunks cannot be read"*,
     while the `if` at `:1234` does not test for it.
- **Success criterion:** `crates/custodian/tests/segmented_map_restore.rs` passes, driven **only**
  through symbols visible on the base (`wyrd_custodian::{reconcile_after_restore, RestoreReport,
  GcContext}`, `wyrd_custodian::desired_state::{reconciliation_status, ReconciliationStatus}`):
  1. **A segmented object no longer stops the pass.** Over a store seeded with a segmented object
     (raw `seg:` records plus a segmented root, **never** a committer), `reconcile_after_restore`
     returns `Ok` (today: `Err`), `RestoreReport::stranded_marked == 0`, and every fragment that
     object owns is still on its D server afterwards.
  2. **A damaged object is contained, and the run is not certified.** Three legs, each closing a
     hole the others leave open — all three ship:
     - **(2a) non-certification, incomplete reading the SOLE cause.** One committed object whose
       chunk map cannot be read; **every other object healthy** — nothing dangling, misplaced or
       under-replicated among those the pass can read, and no stray fragment beyond the ones the
       unreadable object itself owns (withheld by `protects`, base behaviour from #650, and they
       must stay withheld). Pass returns `Ok`, `stranded_marked == 0`, and `report.is_clean()` is
       **false**. `is_clean()` (`restore.rs:144`) is already false whenever any loss is reported, so
       a scenario carrying a loss satisfies this clause **without the fix** — assert it on a store
       where the unreadable object is the only reason.
     - **(2b) containment — the damaged object does not starve the healthy ones.** The same
       unreadable object seeded **beside** a readable object with a genuine loss: the pass still
       returns `Ok`, still reports that object's loss (`dangling` or `misplaced` names its chunk),
       and still marks nothing of the unreadable one.
     - **(2c) marks and report rest on one reading.** Seed a metadata double serving a committed
       record **readable on the pass's first `inode:` read and unreadable on any later one**, plus a
       genuinely unreferenced fragment a complete reading would mark. Assert the **conjunction**:
       *the pass never both marks a fragment and reports an unreadable record* — `stranded_marked >
       0` and an unresolvable attribution on the audit seam must not both hold. Stated that way it
       is implementation-neutral: a pass reading the set **once** sees the record fine and marks the
       stray (passes); a pass reading twice but withholding marks while **either** reading found a
       hole marks nothing and names the record (passes); a pass reading twice and gating on the
       **first** reading marks the stray *and* reports the record unreadable (fails — the defect
       this leg exists to catch). Do **not** assert "marks nothing" unconditionally: that would fail
       the single-reading implementation, which is the better one.
  3. **Attribution, on the audit seam.** The restore pass names the object it could not read, and
     `reconciliation_status` over the (2a) store attributes the blocking object instead of answering
     a bare `Pending`. Assert both as #650's own fixture does — `assert_attributes_blocker`,
     `crates/custodian/tests/segmented_map_consumers.rs:353` — so this needs no symbol the base lacks.

  Criterion (2) is binding: it is the point of the slice and the one an incomplete fix passes
  vacuously. A version that only checks the call returned `Ok` proves none of it.

  **Not in the discriminator, covered by C4-ci:** anything needing a shape this patch introduces —
  the new `RestoreReport` field's contents, the new `ReconciliationStatus` variant, the CLI verdict
  and exit code. Those ship in `restore_reconcile.rs`, `segmented_map_consumers.rs` and `cli.rs`'s
  own `#[cfg(test)] mod tests`.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no dev-dependency, no Docker.
  - **Base.** No `Onto branch`; the stale `stack-base` marker is gone (verified absent), so
    `_resolve_base_ref` (`run-verify.sh:202-209`) falls through to the brief base. Confirmed by
    running `run-verify.sh --print-base` against this bundle at Plan → `origin/main`.
  - **Prerequisites SATISFIED**, verified on the checkout at `d50f0ca`: #648/#649/#650 are all on
    `origin/main` — `metadata::resolve_chunk_map` (`core/src/metadata.rs:2539`),
    `ReferenceSet::unresolvable` (`gc.rs:294`), `protects` (`gc.rs:331`), `object_name` (`gc.rs:470`,
    `pub(crate)`, reachable from `desired_state.rs`), `Reconciled::Blocked` (`reconciliation.rs:44`),
    and `tests/segmented_map_consumers.rs`. `GcContext` (`gc.rs:72`) is `pub` with `pub` fields, so
    an integration test can build one. The vocabulary this slice extends exists on the base.
  - **Discriminator classification — DRY-RUN at Plan, not assumed.** This project's C4-verify
    classifies on an **added** test file (`run-verify.sh:92` keys `_added_files` on `--- /dev/null`;
    `:93` `_is_test_file`). `run-verify.sh --classify` on a synthetic patch listing the expected file
    set returned exactly one `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs` (plus
    `CRATE crates/custodian`, `CRATE crates/server`). One added test ⇒ the invocation is `cargo test
    -p wyrd-custodian --test segmented_map_restore` (`run-verify.sh:317-322`), so the reverted
    `crates/server` change cannot break the leg.
  - **The RED leg** (`run-verify.sh:420-431`) keeps that one file and reverts `restore.rs`,
    `desired_state.rs`, `cli.rs` and **every modified test file**. There `committed_chunks` still
    fails closed, so (1), (2a), (2b), (2c) fail on `Err` and (3) fails for want of attribution.
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** It MUST NOT name any symbol this
    patch introduces (no new `RestoreReport` field, no new `ReconciliationStatus` variant, no new
    helper): the RED leg reverts the production files, so such a reference makes the target fail to
    *compile* and the red degrades from "the behaviour was wrong" to "a symbol is missing".
  - **No vacuous green.** No `#![cfg(...)]` on any `crates/custodian/tests/*.rs` (grepped on the
    base), so neither `0 tests … ok` guard can trip (`run-verify.sh:400` green leg, `:437` red leg —
    both exit 77 UNVERIFIABLE rather than invent a verdict).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the surfaces that report whether a reconciliation is
  complete**. Sourced, not intuited: `docs/principles.md:137` (§6 row *Storage lifecycle /
  reclamation*, which names **restore** and **drain** explicitly), sourced in turn to §5 C-1
  (`docs/principles.md:109`), the maintainer's rule of 2026-07-25, `0016:2802-2813`, `gc.rs:22-25`.
  Over that category:
  - **A pass never reports a conclusion it could not reach.** "Complete", "clean" and "satisfied"
    are claims about a reading that *finished*. Over an incomplete one they are the reclamation
    decision in report form — *"you may decommission this box"*, *"close the ticket"* — and an
    operator acts on them.
  - **Containment is per object, and the answer still gets made.** One damaged record may not blank
    a fleet-wide status surface, nor withhold the losses of every object the pass could read. `Err`
    for the whole query is as wrong as `Satisfied`.
  - **An incomplete reading is attributed, not merely signalled.** An operator who cannot learn
    which record blocked the pass cannot repair it — the stall becomes a state nothing exits, the
    same permanence C-1 forbids, reached through the report instead of through deletion.
  - **A conclusion and the reading it rests on are one.** No fragment is marked under a reading that
    found a hole, and the pass cannot report a record unreadable while its mark half acted as though
    the set were complete. Two readings that can disagree are two conclusions, and the operator is
    shown one of them.
  - **A non-zero exit is part of the report.** A surface that prints a caveat and exits 0 has told
    the automation the run was healthy.
- **Repo + branch target:** getwyrd/wyrd @ main   (per INTEGRATION §2: single slice, no live
  milestone integration branch — M4's is merged and deleted, and #648/#649/#650 landed on `main`
  directly. Verified `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`. **Not**
  `pdca-integration/main`, the driver's run-scoped wave-fold branch — that is what made v5 build on
  a tree missing #650, and `wave_mode` is `"merge"` now regardless.)
- **Depends on:** *(none — #650, its only code dependency, is merged to `main` as `11aa85f`)*
- **Conflicts with:** 681
- **Ordering note:** `Depends on` empty: the prerequisite (#650's `gc::ReferenceSet`) is already on
  the base, verified, so there is nothing to fold. `Conflicts with 681` — no dependency either way
  (#681 owns the resolving namespace walk for reconstruction / backfill / rebalance; this owns
  restore and `desired_state`), but both will edit `crates/custodian/tests/segmented_map_consumers.rs`,
  #650's shared containment fixture that each slice extends with its own consumer's legs — so they
  must not be built blind on the same base. (`rebalance.rs` is #681's alone; verified this slice
  does not need it.) #682 depends on #681 and touches none of these files.
- **Surfaces:** data
- **Difficulty:** medium   (three production files plus tests. Blast radius is **narrower than v10
  assumed**, and I measured rather than inherited the guess: `ReconciliationStatus` has no
  exhaustive `match` anywhere under `crates/*/src` — its only in-tree consumers are
  `desired_state.rs` itself and `assert_eq!` sites in tests — so a new variant forces no call-site
  churn; `RestoreReport` is constructed in one place. Not `low`, because the change spans two crates
  and the CLI's exit contract.)
- **Scope:** make both completeness-reporting surfaces answer **contained and attributed** over a
  reference set with a hole in it, instead of erroring or certifying.
  - `crates/custodian/src/restore.rs` — the report half survives an object it cannot read: it
    reports every object it *could* read, records the ones it could not, and the pass is not clean.
    Marks and report rest on one reading: no fragment is marked while any reading in the pass found
    a record it could not read.
  - `crates/custodian/src/desired_state.rs` — the drain status distinguishes "still holds referenced
    fragments" from "could not read object X", and names X. Where an unreadable record and a
    malformed placement are both present the answer ranks the unreadable one first, matching the
    base's existing check order (`:188` precedes `:198`); the operator repairs the named record and
    re-polls for the malformed ids. **Plan decision — settled, not Do's to revisit.**
  - `crates/server/src/cli.rs` — the summary states an incomplete reading and the exit status
    reflects it. Keep to the summary cell, one NEEDS-HUMAN paragraph and the exit code; the exit
    code must be **derived from the report itself**, not re-computed from the paragraphs, so the
    printed verdict and the status code cannot drift.

  **Out of scope:**
  - **The cross-object chunk-id ambiguity rule — DROPPED, not deferred; it must not come back.** No
    notion of "more than one committed object claims this chunk id", no claim-counting on
    `Expected` / `committed_chunks`, no `ambiguous-*` audit event, no mark-withholding keyed on it.
    Three reasons, all verified on the base at Plan: (i) the gateway mints `(random per-process
    epoch << 64) | seq` with the top bit set, so every id is ≥ 2^127 and two live records cannot
    collide — `crates/server/src/lib.rs:238-241`, with `:230-237` saying so outright, while the
    cluster path packs the inode; (ii) #652 **deleted** the chunk-id floor as dead code left by
    #487, so there is no "fold it into #652" left; (iii) the rule's only in-tree-reachable trigger —
    one record listing an id twice — made the command print *"chunk(s) are LOST"* and exit 1 over a
    **healthy** object, the very C-1 failure this brief exists to prevent, reached from the other
    side. Whether a hand-assembled or imported namespace with duplicate live ids is a supported
    restore state is a maintainer decision on its own issue, to be answered before any code guards it.
  - Reconstruction, backfill, rebalance and the resolving namespace walk they need (**#681**): this
    slice adds **no** custodian-level walk and no `crate::resolve` module, reading the reference set
    through `gc::referenced_fragments` exactly as the base does.
  - `repoint_chunk` and the record ceilings (**#682**): this slice writes **nothing** to a chunk map.
  - `gc.rs` and `scrub.rs` untouched. The committer, fence, rollback and resume (#653).
  - **No report-schema churn:** `dangling` / `misplaced` / `under_replicated` keep their `Vec<ChunkId>`
    shape. Threading the owning `inode:` key through `Expected` / `emit_dangling` / `emit_misplaced`
    is declined — it widens the schema for a gap that existed only under the dropped rule.
  - Any new/edited ADR, spec or proposal; any conformance-vector change; any new CLI subcommand
    (there is no drain CLI today and this slice adds none — `reconciliation_status` is a public
    library surface, and its attributed answer lands in the return value and the audit seam, as
    `PendingMalformed` has since #397).
- **Budget:** ≤ **700** added semantic lines (non-blank, non-comment, non-mechanical), ≤ **8** files.
  The eight are named, so the cap is an allocation rather than a race: `custodian/src/restore.rs`,
  `custodian/src/desired_state.rs`, `server/src/cli.rs`, `custodian/tests/segmented_map_restore.rs`
  (new), `custodian/tests/restore_reconcile.rs`, `custodian/tests/segmented_map_consumers.rs`,
  `docs/…/m4-first-deployment-blueprint.md`, `docs/…/06-runtime-view.md` (confirm-only, likely
  untouched — that is the headroom). A **ninth** file means the shape is wrong: STOP and hand back.
  `custodian/tests/rebalance.rs` in particular should not need editing (verified: its 15
  `ReconciliationStatus` sites are `assert_eq!`s, not exhaustive matches). The 700 is the original
  pre-creep budget and it is reachable — v10 measured ~1,201 semantic lines **with** the dropped
  apparatus and its four extra discriminator legs. The discriminator's fixture is the largest single
  item and the only real risk: prune it to what (1)–(3) need. If mid-build the tree exceeds this,
  **STOP and hand back a proposed split** rather than finishing. This bundle has spent ten
  iterations, four of them on scope that grew past its budget one "nominal overage" at a time; an
  over-budget patch is iterate-to-Plan, not another Do round.
- **Repro instruction:** read the three defect sites on the base with `git -C ../wyrd show
  origin/main:<file>` — the citations are in § Defect and were re-verified there. Seeding any
  `seg:`-backed committed root, or any record that will not decode, makes `reconcile_after_restore`
  return `Err` for the whole store; the same store makes `reconciliation_status` answer a bare
  `Pending`; and `wyrd custodian` exits 0 over both.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete` — the four doctor.checks ids (pdca.toml :696, :703, :733, :740), all OK on this host at Plan. Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool — that already cost this bundle a Do iteration. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dev-dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_restore.rs` — a **NEW** file, not optional.
  C4-verify classifies its discriminator on an **added** `*/tests/*.rs`, and
  `segmented_map_consumers.rs` / `restore_reconcile.rs` both already exist on this base (verified via
  `git ls-tree origin/main crates/custodian/tests/`) — appending to either makes it a *modified* file,
  the gate takes the green-only branch (`run-verify.sh:408-418`) and proves no red at all. Confirmed
  by the `--classify` dry-run above. Updates to the existing per-pass test files ship **in addition**;
  C4-ci covers them.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at Check.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line number
  in this brief was re-verified against `origin/main` at `d50f0ca` during the Plan verification pass;
  still cite by symbol, not number, if the base advances.
  **Peer callsites Do MAY open — this is a composition slice; mirror them rather than invent a shape:**
  - `crates/custodian/src/gc.rs:360-415` — `referenced_fragments`'s walk: decode failure contained
    per object (`unresolvable.insert(key, fault); continue`), resolve via
    `metadata::resolve_chunk_map`, and the **downcast rule** — `Ok(ChunkMapError)` is contained as
    *this record's* fault, any other error propagates because a store fault is not one object's.
    `committed_chunks` must contain by exactly this rule and no other.
  - `crates/custodian/src/gc.rs:155-165` — attribution emitted by the **consumer**, per object,
    **before** the fleet walk, so a later transient fault cannot cost the operator the record's name.
    Mirror the placement, not just the call.
  - `crates/custodian/src/gc.rs:234-240` — refusing to certify over a non-empty `unresolvable` while
    keeping everything the pass did accomplish. This is the report half's shape.
  - `crates/custodian/src/desired_state.rs:101-104` + `:198-203` — `PendingMalformed { chunks }`, the
    precedent for naming blockers **in the answer itself**. The new variant is its sibling.
  - `crates/custodian/tests/segmented_map_consumers.rs:329-360` — `capturing_dispatch`,
    `attributed_objects`, `assert_attributes_blocker`: how criterion (3) is asserted without naming a
    new symbol.
  - **Salvage:** `results/issue_651/iteration-v10/patch.diff` implements all of the above and was
    green at Check — its `desired_state.rs` hunk (~81 lines) is entirely in scope, as are its
    `restore.rs` containment/attribution hunks and four discriminator legs
    (`a_segmented_object_no_longer_stops_the_post_restore_pass`,
    `an_unreadable_object_is_contained_and_the_run_is_not_certified`,
    `an_unreadable_object_does_not_starve_the_objects_the_pass_could_read`,
    `a_drain_over_an_incomplete_reference_set_names_the_blocking_record`). Reuse it, but **strip
    every ambiguity / claims / `CommittedChunks::ambiguous` construct** and its four test legs, and
    add criterion (2c), which v10 lacked.
- **Docs-currency:** two small touches. `docs/design/architecture/06-runtime-view.md:31` already
  states this invariant in full (#650 wrote it, report-only-surface clause included) — confirm it
  reads true of restore and the drain status, and edit only if it does not.
  `docs/design/architecture/m4-first-deployment-blueprint.md:740` describes
  `wyrd custodian --reconcile-after-restore` for the operator; extend it with the INCOMPLETE outcome
  and the non-zero exit. **Claim only what the run can evidence** — a v10 finding (T2) was that the
  runbook asserted a fleet-wide fact the report could not carry; derive any such sentence from the
  report's own fields.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/custodian/src/restore.rs
  crates/custodian/src/desired_state.rs` → six commits; the most recent, `11aa85f` (#650),
  **creates** this slice's premise rather than duplicating it, and before it sit `fdacf02`/`5e1e7af`
  (#551, the pass itself) and `985867c` (#397, malformed placement). No open PR touches these paths.
  **Closed/rejected:** PR #647 (CLOSED 2026-07-30, unmerged) is the un-split ancestor — closed for
  size, not direction; its content is landing as the #635 slice sequence. No prior attempt at this
  containment was rejected on the merits.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 11 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild on the same brief; three findings from this round to address: 1. T3/T5 (blocking): attribution-order gap in reconcile_after_restore — an unresolvable object discovered while building `referenced` (restore.rs:257) is not written to the audit seam until after orphan_leases/pending_chunks/committed_chunks all succeed (restore.rs:272). If any of those later reads hits a genuine, unrelated store fault, the whole call returns Err and the already-known damaged-object name never reaches the operator. Brief explicitly cites gc.rs:155-165 as the pattern to mirror: emit attribution per object, before the fleet walk, so a later transient fault cannot cost the operator the record's name. Emit the already-known unresolvable name as soon as it is discovered (right after the `referenced` read), not batched with the later reads. Add a regression that forces an intervening read (orphan_leases/pending_chunks/ committed_chunks) to fail after an unresolvable object was already found, and assert the record's name still reached the audit trail. 2. T4 gate (blocking, likely mechanical): review-batch.md flags restore.rs:521 (`resolve_chunk_map(...).await` lacking a caller-side timeout) as a new blocking CONVENTION finding. This is the same concern already rejected 3x under the standing rule in review-rejected.md ("the MetadataStore implementation owns its own network bound, not the caller") at the old line numbers (:318, :330) — the code moved and the rejection was not re-recorded at the new line. Re-file the standing rejection at restore.rs:521 in review-rejected.md so the gate clears, unless the rebuild's line numbers shift again in which case re-locate it there. 3. Validation/fitness-to-purpose NEEDS-HUMAN was raised but not the reason for iterating — revisit at the next sign-off once (1) is fixed; not a rebuild item on its own.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v11/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 12 — carry-forward (from the previous attempt)
- Sign-off rationale: Human reviewed the four T4 batch-review blockers (cli.rs:1308/1279/1291 — NEEDS-HUMAN paragraphs print only a count, not the record names the drain/restore report promises to name; restore.rs:326 — possible mark-race for an object that commits/changes between the two namespace scans) and directs the next Do round to fix them rather than re-litigate. Size backstop noted (patch at 100KB, at the 100KB threshold) but explicitly NOT treated as a split trigger: the overage is minimal, so iterate-do (not iterate-plan) is the human's call — do not re-scope or split on this basis. T5 [impl] and Validation/fitness-to-purpose NEEDS-HUMAN items remain open; revisit at next sign-off once the T4 blockers are addressed.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v12/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 13 — carry-forward (from the previous attempt)
- Sign-off rationale: Not accepted yet, but the slice is sound and close. All correctness gates are green (C4-ci pass, C4-verify red->green 5/5, C5 38 mutants with no survivor), C5 causal adequacy and T1 both PASS. One gating item remains: T4. WHAT TO FIX — the only blocking work in this round: - The two T4 blockers in `review-batch.md` are both TEST-GAP: the new concurrent two-scan marking path can authorize GC-visible orphan marks but carries only Tokio / in-memory interleaving coverage, which does not satisfy the rule that new destructive or concurrent paths ship seeded Tier-0 DST coverage. Sites: `crates/custodian/src/restore.rs:296` and `crates/custodian/src/restore.rs:585`. - Add seeded Tier-0 DST coverage for that path. It must exercise the interleaving the Tokio doubles only approximate: the two namespace readings disagreeing, and the marking decision that rests on their reconciliation. The property to pin is the brief's own one-reading rule — no fragment is marked while any reading in the pass found a record it could not read. - Note the brief currently states "No DST leg" under External dependencies. That line is now stale against the shipped design: the two-scan path did not exist when it was written. Adding the DST leg is the correct resolution; the brief's Scope and Success criterion are otherwise unchanged and still binding. WHAT NOT TO CHASE — do not spend the round on these: - The advisory reviewer's T2 / T3 / T5 findings against `crates/server/src/cli.rs` (count-only `dangling` / `misplaced` paragraphs, "See the audit log for each chunk id", cli.rs:1282/1284/1290/1292, tests at :2720/:2755) are NOT defects in this patch. That text is verbatim pre-existing on `origin/main` — confirmed by reading `git show origin/main:crates/server/src/cli.rs`. The patch preserved the base wording and inserted the new UNREADABLE paragraph beside it. Report-schema churn is also explicitly out of scope in the brief. Record-reject these in `review-rejected.md` rather than rebuilding the CLI output shape. - The reviewer's T4 basis line attributes the two blockers to that same CLI text. It is wrong: the reviewer could not re-run `scripts/review-branch --bundle` (it says so in its own re-run notes) and substituted two self-found CLI findings for the gate's actual two because the counts matched. Work from `review-batch.md`, not from the reviewer's §5 table. - Do not reintroduce the cross-object chunk-id ambiguity rule. It was dropped at the v10 replan and must stay dropped. SIZE — not a re-slice trigger this round: The backstop fired at 121 KB / 2 rounds and pre-recommends `iterate-plan`. Declined deliberately. The v10 replan (3rd) already did its job: patch 159 KB -> 99 KB, C5 converted from chronically failing to reliably passing, T4 blockers 3 -> 1. Production code is ~708 added lines against the brief's 700-line budget — on budget; the byte count is dominated by ~1,000 lines of discriminator and CI-gated tests, which is the shape we want. Keep the 8-file allocation. Expect the DST leg to add test bytes; that is not overage in the sense the backstop means.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_651/review-b
- Full previous attempt preserved in `iteration-v13/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
