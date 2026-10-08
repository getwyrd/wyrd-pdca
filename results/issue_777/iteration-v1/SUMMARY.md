# Result — issue 777 / reconstruction-completes-seg-repair-through-primitive

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **A chunk whose `ChunkRef` lives in a `seg:` record is never repaired.**
  #697 stopped reconstruction aborting on a segmented object but deliberately writes
  nothing: the repair obligation is routed to `Site::Refused`
  (`crates/custodian/src/reconstruction.rs:552`) and answered `Assessment::Refused`
  (`:609`), every pass, forever — the obligation is not drained (data loss) and, until
  #776, nothing could move the placement. With #776 merged the primitive **exists but
  nothing calls it**: a multipart-published object's redundancy still decays untended,
  permanently. That is the C-1 violation this bundle exists to close — see **Invariant to
  restore**.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base *after #776 merges* —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, ReconstructionContext,
  Reconciled}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, inode_key, encode, decode, MAX_VALUE_BYTES,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord, ChunkRef,
  EcScheme}` (**`MAX_VALUE_BYTES`, not `MAX_ROOT_VALUE_BYTES`** — leg 5 pins the FULL ceiling;
  reversed by the human 2026-08-19, see #776's leg 5 for the warrant)
  — over in-memory `MetadataStore` / `ChunkStore` doubles. Five legs, the parent's, with its
  two corrections kept:
  1. **BINDING, RED pre-fix** — a `seg:`-resident under-replicated chunk is repaired:
     rebuilt fragment on a healthy D server in a distinct failure domain; the **`seg:`
     record's** `ChunkRef.placement` names it; obligation **drained**; pass answers
     `Changed`; **root bytes unchanged**. Base: refused, queued, byte-identical → red.
  2. **BINDING, RED pre-fix** — a concurrent rewrite of a *sibling* chunk in the same
     `seg:` record is **MERGED**: repair lands **and** the sibling's new placement
     survives. Base: refused → red. This is the leg that goes red if the primitive had
     pinned whole-record bytes instead of the three pins.
  3. **Not independently red** — the *planned* chunk rewritten under the plan is a
     **CONFLICT**: no repair-owned METADATA written (the `seg:` record holds exactly the
     competing writer's placement, byte for byte; root untouched; obligation **still queued**;
     no orphan mark published). Do **not** assert the destination fragment's absence and do
     **not** delete it — production writes fragments before the commit (`:931-935`); the
     stranded fragment is the tracked leak **#723**, inherited as-is (DECIDED at the parent's
     Plan 2026-08-10 — do not re-open, do not implement 0016's X47 pre-mark or drain fence,
     do not reword the two "collectable garbage" comments; #723 owns them).
     **DECIDED BY THE HUMAN 2026-08-19 — the carried-forward "may the pass still certify?"
     question is CLOSED, and this leg now asserts the verdict as well as the store: the pass
     MUST answer `Reconciled::Satisfied`.** That is the flat arm's own answer for a
     conflict-only pass and this child changes it for nobody: `RepairOutcome::Conflict` emits a
     counter and nothing more (`reconstruction.rs:311-318`), and the pass's `hole` covers an
     incomplete reading, a segmented refusal and a ceiling refusal — **not** conflicts
     (`:341-357`). Rationale for keeping it: a lost CAS is a *retry*, not a dead end — nothing
     repair-owned was written, the obligation is still queued, and the next pass re-plans onto
     the winner's bytes, so C-1's "an actor that exits it in bounded time" holds. **Do NOT
     widen `hole` to include conflicts:** that would change the flat arm's long-standing
     behaviour and its existing tests — a different slice with a different blast radius.
     Asserting it here pins the decision instead of leaving it accidental.
  4. **Not independently red** — a superseded root generation is a **CONFLICT**: no
     *repair-owned* metadata written (the leg's own setup writes the competing root, so no
     blanket no-write assertion), obligation still queued, and — same decision, same reason —
     the pass answers **`Reconciled::Satisfied`**.
  5. **Not independently red** — the ceiling refusal holds over a segment record at the
     **full `MAX_VALUE_BYTES`** bound (#776's segmented-arm check, exercised through the whole
     pass): refused, record byte-identical, obligation queued, and the pass answers
     **`Reconciled::Blocked`** — a ceiling refusal *is* a hole (`ceiling_refused`,
     `reconstruction.rs:315-318`, `:341-357`), unlike a conflict. Seed the record just under
     `MAX_VALUE_BYTES` programmatically, never as a byte literal (the 85 KB `patch.diff`
     budget). **Bound reversed from V/2 by the human 2026-08-19** — warrant in #776's leg 5;
     do not restore V/2 and do not name `MAX_ROOT_VALUE_BYTES` here.
  **The race window is reachable deterministically — hook the RIGHT read.** The resolver reads
  the group's `seg:` range with **`scan_page`**, never `get` (`read_group_range`,
  `metadata.rs:2452-2461`, docstring `:2417-2425`); the move's own read is the **only** `get`
  on a `seg:` key. Apply the racing batch **after returning the `scan_page` page**
  (equivalently: on the first `get` of that key, before answering it). Counting `get`s and
  injecting after the first return does NOT work — it quietly inverts leg 2. Leg 4's root flip
  goes on the way into `commit`, after resolve. This gap sank the parent (`RaceAtRepoint`
  injected inside `commit()`) — do not reproduce that shape.
  **Two REGRESSION GUARDS carried from iteration 3 — read the scoping carefully, it was
  corrected at Plan.** Both name defects the *previous attempt introduced by restructuring*,
  not defects on the base; neither licenses fixing the base's own pre-existing behaviour.
  (a) **Refusal containment stays once per OBJECT, not per obligation** — a segmented object
  with two queued chunks in one `seg:` record must produce ONE refusal tick. The base gets
  this right via the `reading.refused` set (`reconstruction.rs:546-552`); iteration 3 lost it
  while restructuring. Preserve it. (b) **Introduce no new silent skip:** if this child
  restructures the committed-object walk, an unattributable object must not fall out of it
  without setting `reading.incomplete` — otherwise an obligation can be drained for a chunk a
  committed map still references. **BUT the base's own `(Some(_), None) => continue`
  (`:511-514`) is deliberate and stays:** its comment assigns it to **#698** ("#698 owns the
  fix"), which is OPEN. Do **not** fix #698's `parse_inode_key` canonicalisation here — that
  is the noncanonical-`inode:01` bug the iteration-4 batch review also raised, and it belongs
  to #698, not this slice.
  **Verification posture:** legs 1–2 are the C4-verify red→green. **Leg 5** passes on the base
  by construction. **Legs 3–4 are now *coarse*-red** — the added `Satisfied` assertion fails on
  the base, which answers `Blocked` from the segmented refusal — but that red only distinguishes
  "refused" from "attempted"; it is **not** evidence the CAS pins are right, so do not present
  it as such. The pins behind legs 3–5 stay bound to the mutation oracle, each with its named
  negation *demonstrated* in `build-notes.md`. Mechanics and the hard constraint on keeping the red
  compilable: see **Falsifiability** below.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the repair pass stops refusing a `seg:`-resident chunk and completes the move
  through **#776's primitive**. The placement change, the obligation discharge
  (`repair::repair_key` delete) and the orphan evidence per displaced position stay **ONE
  batch** — the caller adds its evidence to the batch the primitive hands back
  (`0005:277`, ADR-0015); if the primitive's shape makes that awkward, the finding goes
  back to #776, this child does not fork the batch. Mirror `repair_chunk` and the seeding
  fixture (see **Citations expected**). Duplicate chunk ids get one plan, not independent
  ones — the narrow rule only, no cross-object claim-counting (#651's replan). **A losing
  CAS does not retract already-published bytes** — settled, rejected 4×
  (`results/issue_638/review-rejected.md:15-16`); refusal and conflict paths write no
  repair-owned metadata at all. **Salvage:** the reconstruction caller in
  `results/issue_711/iteration-v1/patch.diff` (harness repo) passed C4-ci and C4-verify —
  reuse it minus everything in the rebalance and DST files (#722's). This bundle's own
  `iteration-v4/patch.diff` is the most refined prior attempt.
  **Untouched:** `crates/core/src/metadata.rs` entirely (#776 owns it — if this child
  needs a core edit, the split is wrong: STOP and hand back); `rebalance.rs`, `backfill.rs`,
  `restore.rs`, `gc.rs`, `desired_state.rs`; `custodian/tests/segmented_map_rebalance.rs` and
  `crates/dst/tests/custodian.rs` (#722); the read side (`resolve_chunk_map`, the
  #695/#696/#697 containment rule); **`parse_inode_key`'s noncanonical-key handling — #698
  (OPEN) owns it, and the base comment at `:513` says so**; any ADR/spec/proposal, conformance
  vector, or new
  dependency. **Forced edit, budgeted:** `custodian/tests/segmented_map_reconstruction.rs:489`
  (`:484` was stale; re-verified at `a801997` — cite the symbol, not the number)
  (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`) asserts the refusal
  this child removes and is rewritten to assert the repair lands.
  **Budget:** 3 files — `custodian/src/reconstruction.rs`,
  `custodian/tests/segmented_map_repoint.rs` (**new**),
  `custodian/tests/segmented_map_reconstruction.rs` — ≤ 100 added semantic non-test lines,
  `patch.diff` **≤ 85 KB**. (Budget calibrated at Plan against the parent's v4 patch: the
  custodian half of it measured **~74 KB**, so the 60 KB first proposed was unachievable and
  would have failed its own budget on day one. 85 KB leaves headroom and still sits well under
  the 95 KB cap the parent's 107 KB blew.) Downstream #722's ordering is in the **Ordering
  note**: its real prerequisite is #776, not this child.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 71.0% — 44 of 62 instrumentable changed lines executed (below the 80% floor); 62 of 169 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 13 mutants tested in 2m: 6 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.08s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #777: restore reconstruction of under-replicated `seg:` chunks — red→green is confirmed, but coverage and an orphan-identity assertion need attention; scope and prior-art decisions remain advisory.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The permanent refusal has observable recovery criteria, with conflict and ceiling outcomes explicitly settled; `brief.md:11`, `brief.md:37`, `brief.md:53`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing both production edits leaves five runnable tests: four fail and the ceiling leg passes; the binding repair and sibling-merge legs fail on `Blocked`; `reviewer-verification.log:53`, `crates/custodian/tests/segmented_map_repoint.rs:393`. |
| C3 Change | NEEDS-HUMAN | Decide whether to allow the fourth, mechanical companion-file edit — removing the shared plan field also changes the staged constructor, outside the explicit three-file budget; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `patch.diff:604`. |
| C4 Verification (red→green) | FAIL | Red→green independently passes, but independent LLVM coverage reproduces 44/62 changed executable lines (71.0%), below 80%, including untested new containment/error branches; `reviewer-diff-coverage.log:1`, `gate-logs/C4-diff-cov.log:64`, `crates/custodian/src/reconstruction.rs:448`. |
| C5 Causal adequacy | PASS | Binding tests demonstrate an exit from the permanent refusal and preservation of a concurrent sibling edit; the cause is removed without a capability probe or fallback guard; `crates/custodian/tests/segmented_map_repoint.rs:398`, `crates/custodian/tests/segmented_map_repoint.rs:452`. |
| T1 Structure | PASS | Placement, obligation discharge, and displaced-fragment evidence retain one conditional commit through the existing core seam; no alternate metadata writer or dependency is introduced; `crates/custodian/src/reconstruction.rs:1142`, `crates/custodian/src/reconstruction.rs:1176`. |
| T2 Shape | PASS | The 67,593-byte patch fits the 85 KB limit and the production additions fit the semantic-line budget; the file-count exception is isolated in C3; `brief.md:124`, `patch.diff:1`. |
| T3 Runtime | PASS | The patched target compiles, the five new tests and six existing segmented reconstruction tests pass, and the injected races reach the relevant read/commit boundaries; `reviewer-verification.log:88`, `crates/custodian/tests/segmented_map_repoint.rs:104`, `crates/custodian/tests/segmented_map_reconstruction.rs:491`. |
| T4 Contribution | NEEDS-HUMAN | Confirm affected-path merged and closed/rejected prior art for all four files — the brief records merged reconstruction history and archived attempts, while the supplied target exposes only one synthetic base commit; `brief.md:173`, `reviewer-prior-art.log:1`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Assert the exact displaced orphan identity — checking only one mark also passes for a mark naming the survivor or rebuilt destination, so the test does not establish its claimed reclamation evidence; `crates/custodian/tests/segmented_map_repoint.rs:421`, `AGENTS.md:175`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated recovery behavior meets the intended deployment's durability needs — the focused evidence uses in-memory stores; Tier-1 disk-fault and Tier-2 kill/reconstruct observation is warranted for rollout; `crates/custodian/tests/segmented_map_repoint.rs:60`, `AGENTS.md:81`. |

Source citations beginning with `crates/` or `AGENTS.md` refer to `$PDCA_TARGET` (`target/`). The target contains #776's primitive and compiled in both legs; no stale-target or missing-toolchain caveat applies. Production changes were restored after the red leg and mutation run; all four file hashes match `patch.diff` (`reviewer-restoration.log:1`).

The actionable test defects are narrow:

- **Coverage:** the independently measured misses match the frozen gate exactly: reconstruction lines 448–453, 542–544, 656, 680–681, 1157–1159, and 1164–1166. Exercise the newly introduced move-time containment and error paths, including two obligations in one damaged object, to check one audit event, retained obligations, and withheld certification. `reviewer-diff-coverage.log:1`; `crates/custodian/src/reconstruction.rs:1157`.
- **Orphan identity:** replace the count-only assertion with the exact key for `(LOST, FragmentId { chunk: CHUNK, index: 1 })`, and retain the absence-of-orphans assertions for losing races. The current production code derives the mark from the displaced placement; this is a test defect, not an observed incorrect deletion. `crates/custodian/src/reconstruction.rs:1121`; `crates/custodian/tests/segmented_map_repoint.rs:369`.

The frozen gate results were adjudicated as follows:

- **C4-ci — PASS from captured evidence:** the log shows actual spelling, docs lint/render, format, Clippy, build/test, dependency scans, conformance, and DST checks, ending successfully. Independent format, changed-file spelling, docs lint, and custodian Clippy also pass. `gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3198`, `gate-logs/C4-ci.log:3825`; `reviewer-scanners.log:1`; `reviewer-clippy.log`.
- **C4-verify — PASS independently reproduced:** five tests execute in each leg; four fail pre-fix and all pass post-fix. Leg 4's red stops at “the race never landed,” so it is coarse refusal evidence, not proof of the root pin. `reviewer-verification.log:19`; `gate-logs/C4-verify.log:32`.
- **C4-diff-cov — FAIL independently reproduced:** 44/62, 71.0%, with the same 18 missed lines. This is a test-coverage failure, not a host fault. `reviewer-diff-coverage.log:2`.
- **C5-mutants — PASS independently reproduced:** 13 tested, six caught, seven unviable, matching the frozen evidence. All candidates are in custodian code; this aggregate does not itself demonstrate the named negations of #776's core pins/ceiling. `reviewer-mutants.log:3`; `gate-logs/C5-mutants.log:10`; `reviewer-mutants-list.log:1`.
- **T4-batch-review — one distinct test finding retained:** its two orphan-count findings are the same T5 defect. Its three DST findings concern work explicitly assigned to #722 and excluded here, so they are settled under the standing deferral protocol rather than re-raised. `gate-logs/T4-batch-review.log:10`; `brief.md:111`; `brief.md:116`; `AGENTS.md:200`.
- **T4-contribution — N/A:** publishing artifacts are absent by design; the substantive contribution audit must rerun at publish. `gate-logs/T4-contribution.log:10`.
- **host-tikv — PASS from captured evidence:** the log shows real feature-enabled checks of the metadata adapter and server, ending successfully. `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`.

The brief's external tools were exercised in the captured gate evidence; no alias, shim, or skipped-tool substitute is shown. The in-memory fixture is the brief's specified test environment, not evidence of a real-cluster run. The conflict-only `Satisfied` decision and tracked work in #698, #723, and #722 remain settled. No implementation changes are proposed for those deferred issues.

### Advisory — adversary

# Adversarial review — issue #777 (segmented repair through `repoint_chunk`)

Short verdict: I could not break the production change. The red→green reproduces, the tests go
through the real `reconcile_step` path, and the named negations for legs 3–5 hold. The weak spot
is the **new containment path** the patch adds: no test runs it, and I showed that removing it
leaves every shipped test green. Two further items need a human scope call (DST coverage, the
docs deferral).

## Findings

- NEEDS-HUMAN [impl] — **The new move-time containment arm has no test at all.**
  `crates/custodian/src/reconstruction.rs:448-454` (with `:1157-1167` and `Reading::contain`'s
  once-per-object dedupe at `:541-546`) is how a typed `ChunkMapError` from the move becomes
  `Blocked` rather than a clean pass. C4-diff-cov lists all of these lines as MISS. C5 reports
  "0 missed" only because cargo-mutants made no mutant for an `if let` body. I disabled the
  containment by hand (the `if let Target::Committed(site)` at `:449` never matches) and ran
  the **whole** `wyrd-custodian` suite (27 test binaries, including the 5 new legs and the
  rewritten `segmented_map_reconstruction.rs` leg 2): **everything still passed.** What that
  mutant does: if a `seg:` record becomes undecodable between the resolve and the move while
  the root still names it, the pass answers `Satisfied` instead of `Blocked`, which tells the
  operator redundancy is fine when it is not (C-1). The current code handles this correctly.
  My attack test (arm `Race::AfterSegmentPage` with bytes `{not a segment record` at `seg:…:1`)
  gets `Blocked`, the obligation stays queued, the record is byte-identical, the root is
  untouched and no orphan mark is written. Fix: add that leg to `segmented_map_repoint.rs`, and
  add a two-obligations-in-one-object variant that checks exactly one `unresolvable` row for
  `inode:1`. That covers guard (a), "once per OBJECT", on the path that replaced the refusal.

- NEEDS-HUMAN [impl] — **Leg 1 checks the orphan mark by count only**
  (`crates/custodian/tests/segmented_map_repoint.rs:420-424`, `orphans(&meta).await.len() == 1`).
  The rubric's *Absent or unsupported entries* class names count-based assertions, and the
  T4 batch review raised this twice. The production code is right: I checked, and the single
  key is `orphan:1:41472:1` = `metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })`.
  But a bug that marked the survivor or the destination would still pass this assertion.
  Assert equality on that key instead. `orphan_key` is a base symbol, so the red leg still
  compiles.

- NEEDS-HUMAN [human] — **No seeded Tier-0 DST coverage for the new concurrent write path.**
  This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare
  the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments;
  then commit the segment CAS, the obligation delete and the orphan marks together. The rubric
  (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path.
  Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has
  segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented
  reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating
  gate) on this finding three times. A human has to choose: accept a `// deferred: #722`
  marker, which this diff does not add yet, or widen the scope.

- NEEDS-HUMAN [human] — **The docs-currency deferral lands on this issue, and this diff drops
  it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living
  architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing
  calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring.
  It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out
  both the doc update and a fix to the marker, whose "nothing calls this yet" is now false.
  After merge, the deferral points at a closed issue. The rubric makes docs currency a merge
  requirement, and its protocol says to raise the tracking issue when a deferral looks wrong.
  Here the tracking issue is this one. A human decides: add the doc update to this PR, or
  file a new issue and re-point the marker.

- NEEDS-HUMAN [impl] — **Comments the patch made false and left in place.**
  `reconstruction.rs:316` still says "Like the `seg:` refusal", but that refusal is gone.
  `reconstruction.rs:410-419` still says each commit is "conditioned on the generation THE SCAN
  returned" and that "a second obligation inside the same object still loses the CAS it always
  lost". Neither holds for segmented objects any more: the snapshot is now the *resolved*
  generation, and two owed chunks in the **same** `seg:` record both land in one pass. My
  attack test got `Changed`, placements `[[0,2],[0,2]]`, an empty queue and 2 orphans, and
  `segmented_map_reconstruction.rs:490` asserts the same across segments. Also, the new test's
  module doc (`segmented_map_repoint.rs`, "Legs 3–4 fail on the base only on their `Satisfied`
  verdict") is wrong for leg 4. On the base, leg 4 fails at `assert!(meta.raced())` (`:357`,
  "the race never landed"), because the base never commits. Harmless, but the doc should say so.

## Informational (not tagged: settled, or too minor to spend a rebuild)

- **#698 (settled, not raised against this diff):** a segmented root stored under a
  non-canonical key such as `inode:01` now fails silently. I seeded one: on the **base** every
  pass answers `Blocked`. **Patched**, every pass answers `Satisfied`, the obligation stays
  queued forever, and each pass writes a rebuilt fragment to the free server that is then
  stranded. That happens because `parse_inode_key` (`reconstruction.rs:650`) maps the key to
  `1`, and the root pin (`metadata.rs:3257-3258`) CASes against `inode:1`, which does not
  exist. The flat arm has always behaved this way, the writer is unreachable today, and the
  brief assigns this to #698. Suggest noting on #698 that the segmented arm now shares the
  flat arm's silent failure and lost the `Blocked` signal the refusal gave it.
- `Repoint::Refused`'s contract (`metadata.rs:3134-3138`) asks the caller to confirm the
  generation is still current before escalating. The caller escalates straight away
  (`reconstruction.rs:455-458`: ceiling counter plus `Blocked`). Worst case: one spurious
  `reconstruction_ceiling_refused` tick and one `Blocked` pass when a supersede lands between
  the resolve and the move. The base flat arm did the same.
- The file budget said 3; the patch touches 4. The `staged.rs` change is a one-line removal
  forced by dropping `RepairPlan::chunk_index`; keeping the field would be dead code under
  clippy `-D warnings`.

## Attempted refutations that did not land

- **Red→green, re-run myself** (scratch copy, base+#776 production, the new test kept):
  4 failed and 1 passed (leg 5) on the base; 5 passed with the patch. This matches the
  C4-verify log. The tests go through `reconcile_step` and assert on the store, not on a
  parallel copy of the logic.
- **Named negations** (a mutation that removes one pin; the named leg must then fail), each
  applied to `metadata.rs` in scratch: comparing only the chunk id in `chunk_at` (`:3374`)
  → leg 3 fails; dropping the root pin in the segmented arm (`:3325`) → leg 4 fails;
  skipping `weighed` in the segmented arm → leg 5 fails. So the "bound by named negation"
  claims hold.
- **Supersede onto a flat generation mid-resolve** (a flat root at `inode:1`, v2, raced in
  after the segment page): the resolve restarts, and the move repoints the *live* flat root
  (v3, CHUNK → `[0,2]`, queue drained). The switch of `Object::prior` to
  `ResolvedChunkMap::record` (`reconstruction.rs:683-689`) is correct.
- **Guard (b), no new silent skip:** a segmented object whose key does not parse is contained,
  not skipped (`reconstruction.rs:676-682`), and the base's flat `continue` (`:655-657`) is
  kept, as the brief requires.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_repoint.rs:421`: The success case checks only that one orphan mark exists. It still passes if repair marks the survivor, the rebuilt destination, or another fragment instead of the displaced position. Compare the complete returned key list with the expected orphan key for `(LOST, FragmentId { chunk: CHUNK, index: 1 })`; this proves the identity claimed by the assertion and excludes extra marks.

No additional introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified. Reviewed against the read-only target and supplied frozen gate evidence; no builds rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether to allow the fourth, mechanical companion-file edit — removing the shared plan field also changes the staged constructor, outside the explicit three-file budget; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `patch.diff:604`.
- [ ] T4 Contribution — Confirm affected-path merged and closed/rejected prior art for all four files — the brief records merged reconstruction history and archived attempts, while the supplied target exposes only one synthetic base commit; `brief.md:173`, `reviewer-prior-art.log:1`.
- [ ] T5 Judgment — Assert the exact displaced orphan identity — checking only one mark also passes for a mark naming the survivor or rebuilt destination, so the test does not establish its claimed reclamation evidence; `crates/custodian/tests/segmented_map_repoint.rs:421`, `AGENTS.md:175`.
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated recovery behavior meets the intended deployment's durability needs — the focused evidence uses in-memory stores; Tier-1 disk-fault and Tier-2 kill/reconstruct observation is warranted for rollout; `crates/custodian/tests/segmented_map_repoint.rs:60`, `AGENTS.md:81`.
- [ ] **The new move-time containment arm has no test at all.** `crates/custodian/src/reconstruction.rs:448-454` (with `:1157-1167` and `Reading::contain`'s once-per-object dedupe at `:541-546`) is how a typed `ChunkMapError` from the move becomes `Blocked` rather than a clean pass. C4-diff-cov lists all of these lines as MISS. C5 reports "0 missed" only because cargo-mutants made no mutant for an `if let` body. I disabled the containment by hand (the `if let Target::Committed(site)` at `:449` never matches) and ran the **whole** `wyrd-custodian` suite (27 test binaries, including the 5 new legs and the rewritten `segmented_map_reconstruction.rs` leg 2): **everything still passed.** What that mutant does: if a `seg:` record becomes undecodable between the resolve and the move while the root still names it, the pass answers `Satisfied` instead of `Blocked`, which tells the operator redundancy is fine when it is not (C-1). The current code handles this correctly. My attack test (arm `Race::AfterSegmentPage` with bytes `{not a segment record` at `seg:…:1`) gets `Blocked`, the obligation stays queued, the record is byte-identical, the root is untouched and no orphan mark is written. Fix: add that leg to `segmented_map_repoint.rs`, and add a two-obligations-in-one-object variant that checks exactly one `unresolvable` row for `inode:1`. That covers guard (a), "once per OBJECT", on the path that replaced the refusal.
- [ ] **Leg 1 checks the orphan mark by count only** (`crates/custodian/tests/segmented_map_repoint.rs:420-424`, `orphans(&meta).await.len() == 1`). The rubric's *Absent or unsupported entries* class names count-based assertions, and the T4 batch review raised this twice. The production code is right: I checked, and the single key is `orphan:1:41472:1` = `metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })`. But a bug that marked the survivor or the destination would still pass this assertion. Assert equality on that key instead. `orphan_key` is a base symbol, so the red leg still compiles.
- [ ] **No seeded Tier-0 DST coverage for the new concurrent write path.** This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments; then commit the segment CAS, the obligation delete and the orphan marks together. The rubric (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path. Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating gate) on this finding three times. A human has to choose: accept a `// deferred: #722` marker, which this diff does not add yet, or widen the scope.
- [ ] **The docs-currency deferral lands on this issue, and this diff drops it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring. It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out both the doc update and a fix to the marker, whose "nothing calls this yet" is now false. After merge, the deferral points at a closed issue. The rubric makes docs currency a merge requirement, and its protocol says to raise the tracking issue when a deferral looks wrong. Here the tracking issue is this one. A human decides: add the doc update to this PR, or file a new issue and re-point the marker.
- [ ] **Comments the patch made false and left in place.** `reconstruction.rs:316` still says "Like the `seg:` refusal", but that refusal is gone. `reconstruction.rs:410-419` still says each commit is "conditioned on the generation THE SCAN returned" and that "a second obligation inside the same object still loses the CAS it always lost". Neither holds for segmented objects any more: the snapshot is now the *resolved* generation, and two owed chunks in the **same** `seg:` record both land in one pass. My attack test got `Changed`, placements `[[0,2],[0,2]]`, an empty queue and 2 orphans, and `segmented_map_reconstruction.rs:490` asserts the same across segments. Also, the new test's module doc (`segmented_map_repoint.rs`, "Legs 3–4 fail on the base only on their `Satisfied` verdict") is wrong for leg 4. On the base, leg 4 fails at `assert!(meta.raced())` (`:357`, "the race never landed"), because the base never commits. Harmless, but the doc should say so.
- [ ] `crates/custodian/tests/segmented_map_repoint.rs:421`: The success case checks only that one orphan mark exists. It still passes if repair marks the survivor, the rebuilt destination, or another fragment instead of the displaced position. Compare the complete returned key list with the expected orphan key for `(LOST, FragmentId { chunk: CHUNK, index: 1 })`; this proves the identity claimed by the assertion and excludes extra marks.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — Confirm affected-path merged and closed/rejected prior art for all four files — the brief records merged reconstruction history and archived attempts, while the supplied target exposes only one synthetic base commit; `brief.md:173`, `reviewer-prior-art.log:1`.; T5 Judgment — Assert the exact displaced orphan identity — checking only one mark also passes for a mark naming the survivor or rebuilt destination, so the test does not establish its claimed reclamation evidence; `crates/custodian/tests/segmented_map_repoint.rs:421`, `AGENTS.md:175`.; **The new move-time containment arm has no test at all.** `crates/custodian/src/reconstruction.rs:448-454` (with `:1157-1167` and `Reading::contain`'s once-per-object dedupe at `:541-546`) is how a typed `ChunkMapError` from the move becomes `Blocked` rather than a clean pass. C4-diff-cov lists all of these lines as MISS. C5 reports "0 missed" only because cargo-mutants made no mutant for an `if let` body. I disabled the containment by hand (the `if let Target::Committed(site)` at `:449` never matches) and ran the **whole** `wyrd-custodian` suite (27 test binaries, including the 5 new legs and the rewritten `segmented_map_reconstruction.rs` leg 2): **everything still passed.** What that mutant does: if a `seg:` record becomes undecodable between the resolve and the move while the root still names it, the pass answers `Satisfied` instead of `Blocked`, which tells the operator redundancy is fine when it is not (C-1). The current code handles this correctly. My attack test (arm `Race::AfterSegmentPage` with bytes `{not a segment record` at `seg:…:1`) gets `Blocked`, the obligation stays queued, the record is byte-identical, the root is untouched and no orphan mark is written. Fix: add that leg to `segmented_map_repoint.rs`, and add a two-obligations-in-one-object variant that checks exactly one `unresolvable` row for `inode:1`. That covers guard (a), "once per OBJECT", on the path that replaced the refusal.; **Leg 1 checks the orphan mark by count only** (`crates/custodian/tests/segmented_map_repoint.rs:420-424`, `orphans(&meta).await.len() == 1`). The rubric's *Absent or unsupported entries* class names count-based assertions, and the T4 batch review raised this twice. The production code is right: I checked, and the single key is `orphan:1:41472:1` = `metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })`. But a bug that marked the survivor or the destination would still pass this assertion. Assert equality on that key instead. `orphan_key` is a base symbol, so the red leg still compiles.; **Comments the patch made false and left in place.** `reconstruction.rs:316` still says "Like the `seg:` refusal", but that refusal is gone. `reconstruction.rs:410-419` still says each commit is "conditioned on the generation THE SCAN returned" and that "a second obligation inside the same object still loses the CAS it always lost". Neither holds for segmented objects any more: the snapshot is now the *resolved* generation, and two owed chunks in the **same** `seg:` record both land in one pass. My attack test got `Changed`, placements `[[0,2],[0,2]]`, an empty queue and 2 orphans, and `segmented_map_reconstruction.rs:490` asserts the same across segments. Also, the new test's module doc (`segmented_map_repoint.rs`, "Legs 3–4 fail on the base only on their `Satisfied` verdict") is wrong for leg 4. On the base, leg 4 fails at `assert!(meta.raced())` (`:357`, "the race never landed"), because the base never commits. Harmless, but the doc should say so.; `crates/custodian/tests/segmented_map_repoint.rs:421`: The success case checks only that one orphan mark exists. It still passes if repair marks the survivor, the rebuilt destination, or another fragment instead of the displaced position. Compare the complete returned key list with the expected orphan key for `(LOST, FragmentId { chunk: CHUNK, index: 1 })`; this proves the identity claimed by the assertion and excludes extra marks.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
