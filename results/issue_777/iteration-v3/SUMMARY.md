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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 65 of 65 instrumentable changed lines executed (floor 80%); 65 of 202 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 12 mutants tested in 2m: 5 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.60s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #777 repair of chunks stored in `seg:` records passes independent red→green verification; scope acceptance and fitness-to-purpose remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Permanent refusal has a falsifiable exit condition: repaired placement, drained obligation, unchanged root, preserved competing writes, and full-value-ceiling refusal; `brief.md:21`, `brief.md:99`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the new test produced a compilable baseline with 10 failures and one pass, including both binding repair failures; `reviewer-red.log:5`, `crates/custodian/tests/segmented_map_repoint.rs:484`. |
| C3 Change | NEEDS-HUMAN | Approve the scope expansion or return it to Plan — the patch changes four files against the three-file budget and changes pre-existing flat-record overflow outcomes; the fourth-file edit removes an obsolete shared-plan initializer; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `crates/custodian/src/reconstruction.rs:685`, `crates/custodian/src/reconstruction.rs:1178`. |
| C4 Verification (red→green) | PASS | Restoring the fix passed all 11 new tests and the full custodian suite (257 passed, one ignored); local scanners passed, and frozen CI/coverage logs substantiate their green rows; `reviewer-green-suite.log:264`, `reviewer-measurements.log:8`, `gate-logs/C4-ci.log:3832`, `gate-logs/C4-diff-cov.log:52`. |
| C5 Causal adequacy | PASS | The formerly permanent refusal now reaches the existing placement primitive and one commit carrying both discharge and orphan evidence; no capability probe masks the cause, and binding tests observe the store; `crates/custodian/src/reconstruction.rs:1159`, `crates/custodian/src/reconstruction.rs:1198`, `crates/custodian/tests/segmented_map_repoint.rs:493`. |
| T1 Structure | PASS | The repair retains the metadata trait boundary and shared per-object snapshot, with record addressing delegated to the existing core primitive; no core or dependency change is introduced; `crates/custodian/src/reconstruction.rs:691`, `crates/custodian/src/reconstruction.rs:1159`. |
| T2 Shape | PASS | The patch fits the byte and semantic-line budgets: 84,430 bytes and 72 added production lines after excluding blanks, comments, and delimiter-only lines; the file-count exception is assigned to C3; `reviewer-measurements.log:1`, `reviewer-measurements.log:7`. |
| T3 Runtime | PASS | Exercised races preserve sibling writes, reject stale chunk/root plans, retain obligations on refusal, contain object damage once, and propagate backend faults; no new lifecycle clock is introduced; `crates/custodian/tests/segmented_map_repoint.rs:544`, `crates/custodian/tests/segmented_map_repoint.rs:590`, `crates/custodian/tests/segmented_map_repoint.rs:693`, `crates/custodian/tests/segmented_map_repoint.rs:846`. |
| T4 Contribution | PASS | Independent affected-path history covers all four files and all 356 closed/merged PRs, including rejected #647; contribution-document auditing is N/A until its required publish rerun; `reviewer-prior-art.log:1`, `reviewer-prior-art.log:24`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Earlier test gaps now have observable assertions for the displaced orphan identity, once-per-object containment, and move-side store errors; independent mutation testing found five caught and seven unviable mutants, with none surviving; `crates/custodian/tests/segmented_map_repoint.rs:519`, `crates/custodian/tests/segmented_map_repoint.rs:733`, `crates/custodian/tests/segmented_map_repoint.rs:858`, `reviewer-mutants.log:4`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated repair behavior for release and decide whether to trigger Tier-1 disk-fault/Tier-2 kill-and-reconstruct observation — this review exercised real repair code over in-memory stores and scripted races, not a deployed segmented-object lifecycle; `crates/custodian/tests/segmented_map_repoint.rs:8`, `crates/custodian/tests/segmented_map_repoint.rs:25`, `AGENTS.md:78`. |

No implementation defect was established. The scope decision is concrete: `reconstruction/staged.rs` loses one obsolete initializer because the shared `RepairPlan` no longer carries `chunk_index`; flat version exhaustion now returns containment instead of overflowing, and a flat chunk beyond an overflowing byte range is contained instead of repaired by index. Those outcomes are tested, but the brief's three-file boundary does not authorize them explicitly (`crates/custodian/tests/segmented_map_repoint.rs:816`, `crates/custodian/tests/segmented_map_repoint.rs:824`).

Source citations above are relative to `$PDCA_TARGET`; evidence logs and the brief are relative to this review directory. The target contains the prerequisite primitive and accepts the complete reverse patch check, so there is no stale-target caveat. After stash/pop and mutation testing, the tracked diff is byte-identical to its initial state and the new test's SHA-256 is unchanged (`reviewer-measurements.log:9`).

| Gate evidence | Verdict | Independently established basis |
|---|---|---|
| C4-ci | PASS | Read the frozen full log: real typos, docs lint/render with link audit, fmt, clippy, build, tests, dependency checks, conformance, statics, and DST complete; `gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3204`, `gate-logs/C4-ci.log:3832`. Locally reran the custodian suite, fmt, typos, docs lint, and machete. |
| C4-verify | PASS | Independently reproduced 10 failures/one pass before the fix and 11 passes after restoration; `reviewer-red.log:5`, `reviewer-green-suite.log:264`; consistent with `gate-logs/C4-verify.log:137`. |
| C4-diff-cov | PASS | Frozen log measures 65/65 instrumentable changed lines covered, with 137 other changed lines unscored; this is not a claim of coverage for every changed line; `gate-logs/C4-diff-cov.log:39`, `gate-logs/C4-diff-cov.log:52`. |
| C5-mutants | PASS | Independently reran `cargo mutants --in-place --in-diff ../patch.diff -p wyrd-custodian` with offline Cargo: five caught, seven unviable, zero missed; `reviewer-mutants.log:4`. This does not demonstrate every manually named primitive-pin negation, which lies outside this diff. |
| T4-batch-review | PASS | Frozen output reports zero blocking, zero recorded-rejected, and zero noise-dropped findings; only that reported result is established by this log; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | `pr-description.md` is absent by design at Check; substantive contribution auditing must rerun at publish; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Frozen log records successful clippy compilation of the TiKV crate and server selection arms; it is compile evidence, not a running TiKV deployment test; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`. |

All external tools named in `brief.md:132` have execution evidence in the frozen logs; none is substituted by a shim or a code-read claim in that evidence. Instance-scoped wrappers were adjudicated from their logs without treating their absence from this target as a defect. No `INTEGRATION.md` exists in the supplied target, so no additional human-only checklist could be derived from it.

Tracked deferrals remain settled: #698 key canonicalization, #723 stranded fragments, and #682 seeded move DST are not reopened. The prerequisite also explicitly tracks the living-architecture follow-up under #777 (`crates/core/src/metadata.rs:3238`); those documents are unchanged here, and that recorded deferral is not recast as a new verification failure.

### Advisory — adversary

# Adversarial review — issue #777 (reconstruction completes `seg:` repair through `repoint_chunk`)

Re-ran the proof on a scratch copy of `$PDCA_TARGET`: `segmented_map_repoint` is 11/11 green and the
rewritten `segmented_map_reconstruction` is 6/6 green. The frozen red log (`gate-logs/C4-verify.log`)
shows the base's real behaviour (`refused-segmented` rows, `Blocked`), so the red is earned against
base + #776 and the tests drive `reconcile_step`, not a copy of production. The fix holds for the
main cases. What follows is what I could break or what the evidence overstates.

- NEEDS-HUMAN [impl] — **The abort offset on a contained move is not tested.** `crates/custodian/src/reconstruction.rs:448-451`: I deleted `emit_aborted(plan.chunk_id);` at `:450` and ran the whole `wyrd-custodian` suite. Every test binary passed. With that mutant, a move that was contained (torn `seg:` record, version exhausted, …) still counts as a successful repair on the durability plane. The success figure `reconstruction_repaired − conflict − aborted − ceiling_refused` (`:386-393`) goes up by one for a repair that never happened. That is the "silent success" class in the rubric, and the comment at `:444-447` claims the opposite. C5's "0 missed" does not cover this line because cargo-mutants made no mutant for it. Fix: in `torn_under_the_move` (`crates/custodian/tests/segmented_map_repoint.rs:700-738`), assert that `monotonic_counter.reconstruction_aborted` ticks exactly `owed.len()` times. Smaller nit in the same place: the row `emit_aborted` prints (`reconstruction.rs:1425`) still says "could not place the rebuilt shard(s)", which is the wrong reason for a contained move.

- NEEDS-HUMAN [human] — **The #698 deferral now covers segmented objects, and the verdict got worse.** Concrete case: a segmented root stored at the non-canonical key `inode:01`, owed `CHUNK`. I ran 3 passes on each tree. Base: `Blocked` every pass, nothing written. Patched: `parse_inode_key` turns `inode:01` into 1 (`reconstruction.rs:651-652`), and the move pins `inode:1` (`crates/core/src/metadata.rs:3257-3258`), which does not exist. So the commit loses with `Conflict`, and the pass answers **`Satisfied`** every pass. The obligation stays queued forever, and a rebuilt fragment is rewritten to the free server each pass (the #723 leak). The brief gives non-canonical keys to #698, so under the rubric's deferral rule this does not block the patch. But #698 should record two things: segmented objects now hit it too, and their verdict changed from `Blocked` to `Satisfied`. An object stuck forever (the C-1 case) now sits behind a clean verdict.

- NEEDS-HUMAN [human] — **A ceiling refusal on a generation the root has already left pages a human.** The primitive's contract (`crates/core/src/metadata.rs:3134-3138`) says to confirm the generation is still current with a fresh resolve before escalating a `Refused`. The caller escalates directly (`reconstruction.rs:1174-1176` → `:452-455`). Concrete case: leg 5's padded `seg:` record, with the root overwritten by a flat record that no longer names `CHUNK` between the resolve and the move's own `get`. The pass answers `Blocked` and emits one `refused-ceiling` row saying "NEEDS-HUMAN: the object's record must shrink" (`:1448`), for a record the object no longer uses. The next pass answers `Satisfied` and drains. That is one false page per race. The base's flat arm had the same gap (it weighed the scan snapshot), so this carries existing behaviour forward. Decide whether a root re-read on `Refused` belongs in this slice.

- NEEDS-HUMAN [human] — **A docs deferral that points at this issue is now due and is not paid.** `crates/core/src/metadata.rs:3238-3240` (from #776) says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring and touches no doc. The brief keeps `metadata.rs` untouched and its 3-file budget has no room for docs, so the marker's "nothing calls this yet" is now false and stays false. A human should either budget the doc update here or point the deferral at a new issue.

- NEEDS-HUMAN [human] — **"11 test(s) ran red" (`check-gates.json`, C4-verify) overstates what was fixed.** Leg 10 (`segmented_map_repoint.rs:823-837`) is red on the base because the base *repairs* that flat chunk (red log: `left: Changed, right: Blocked`). The patch switches the flat arm from addressing by index to addressing by byte offset (`reconstruction.rs:669-690`), and as a side effect that object is now contained every pass and never repaired. The record is corrupt (chunk lengths sum past `u64`, size 8), so containment is defensible under ADR-0045 ("strict in maintenance paths"). But it is a flat-arm behaviour change the brief did not ask for, and its red is not evidence of a fixed defect. In the same way, leg 8's red is the base correctly draining a truly unreferenced obligation (`DELETED`). The patch freezes drains across the whole store while the `inode:x` object is owed a repair. Guard (b) requires that, but it is broader than the base's per-obligation refusal.

- Note, no action needed: the patch touches a 4th file, `crates/custodian/src/reconstruction/staged.rs:366` (one deleted line), outside the brief's 3-file budget. The edit is forced: removing the now-unread `RepairPlan::chunk_index` would otherwise trip clippy's dead-field warning. Other budgets are met: 81 added non-comment production lines (limit 100), `patch.diff` 84,430 B (limit 85 KB).

**Tried and could not refute:**
- **Leg 2 merge:** the race lands after the resolver's `scan_page` and before the move's `get`. Leg 11's base red ("the fault never fired") proves the move's read is the only `get` on a `seg:` key.
- **Two owed chunks in one `seg:` record:** both land in one pass (placements `[[0,2],[0,2]]`, queue empty, 2 orphan marks), as the rewritten comment at `:415-419` claims.
- **Drains after a move-time containment:** they are withheld, because the drain batch runs after the repair loop and is gated on `reading.incomplete` (`:477`).
- **Fragment writes on a failed move:** no answer other than `Prepared` writes a fragment (`:1172-1196`).
- **Store fault vs. object damage:** the split at `:1186-1189` is pinned by leg 11.
- **Once-per-object naming:** pinned by leg 7 (`Reading::contain`, `:219-225`).
- **Restart onto a live root:** using the resolved record rather than the scanned one only affects liveness. The root pin keeps a stale plan from landing either way.

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified.

Reviewed the diff against the read-only target source and supplied frozen gate evidence. Gates were not rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Approve the scope expansion or return it to Plan — the patch changes four files against the three-file budget and changes pre-existing flat-record overflow outcomes; the fourth-file edit removes an obsolete shared-plan initializer; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `crates/custodian/src/reconstruction.rs:685`, `crates/custodian/src/reconstruction.rs:1178`.
- [ ] Validation — fitness-to-purpose — Accept the demonstrated repair behavior for release and decide whether to trigger Tier-1 disk-fault/Tier-2 kill-and-reconstruct observation — this review exercised real repair code over in-memory stores and scripted races, not a deployed segmented-object lifecycle; `crates/custodian/tests/segmented_map_repoint.rs:8`, `crates/custodian/tests/segmented_map_repoint.rs:25`, `AGENTS.md:78`.
- [ ] **The abort offset on a contained move is not tested.** `crates/custodian/src/reconstruction.rs:448-451`: I deleted `emit_aborted(plan.chunk_id);` at `:450` and ran the whole `wyrd-custodian` suite. Every test binary passed. With that mutant, a move that was contained (torn `seg:` record, version exhausted, …) still counts as a successful repair on the durability plane. The success figure `reconstruction_repaired − conflict − aborted − ceiling_refused` (`:386-393`) goes up by one for a repair that never happened. That is the "silent success" class in the rubric, and the comment at `:444-447` claims the opposite. C5's "0 missed" does not cover this line because cargo-mutants made no mutant for it. Fix: in `torn_under_the_move` (`crates/custodian/tests/segmented_map_repoint.rs:700-738`), assert that `monotonic_counter.reconstruction_aborted` ticks exactly `owed.len()` times. Smaller nit in the same place: the row `emit_aborted` prints (`reconstruction.rs:1425`) still says "could not place the rebuilt shard(s)", which is the wrong reason for a contained move.
- [ ] **The #698 deferral now covers segmented objects, and the verdict got worse.** Concrete case: a segmented root stored at the non-canonical key `inode:01`, owed `CHUNK`. I ran 3 passes on each tree. Base: `Blocked` every pass, nothing written. Patched: `parse_inode_key` turns `inode:01` into 1 (`reconstruction.rs:651-652`), and the move pins `inode:1` (`crates/core/src/metadata.rs:3257-3258`), which does not exist. So the commit loses with `Conflict`, and the pass answers **`Satisfied`** every pass. The obligation stays queued forever, and a rebuilt fragment is rewritten to the free server each pass (the #723 leak). The brief gives non-canonical keys to #698, so under the rubric's deferral rule this does not block the patch. But #698 should record two things: segmented objects now hit it too, and their verdict changed from `Blocked` to `Satisfied`. An object stuck forever (the C-1 case) now sits behind a clean verdict.
- [ ] **A ceiling refusal on a generation the root has already left pages a human.** The primitive's contract (`crates/core/src/metadata.rs:3134-3138`) says to confirm the generation is still current with a fresh resolve before escalating a `Refused`. The caller escalates directly (`reconstruction.rs:1174-1176` → `:452-455`). Concrete case: leg 5's padded `seg:` record, with the root overwritten by a flat record that no longer names `CHUNK` between the resolve and the move's own `get`. The pass answers `Blocked` and emits one `refused-ceiling` row saying "NEEDS-HUMAN: the object's record must shrink" (`:1448`), for a record the object no longer uses. The next pass answers `Satisfied` and drains. That is one false page per race. The base's flat arm had the same gap (it weighed the scan snapshot), so this carries existing behaviour forward. Decide whether a root re-read on `Refused` belongs in this slice.
- [ ] **A docs deferral that points at this issue is now due and is not paid.** `crates/core/src/metadata.rs:3238-3240` (from #776) says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring and touches no doc. The brief keeps `metadata.rs` untouched and its 3-file budget has no room for docs, so the marker's "nothing calls this yet" is now false and stays false. A human should either budget the doc update here or point the deferral at a new issue.
- [ ] **"11 test(s) ran red" (`check-gates.json`, C4-verify) overstates what was fixed.** Leg 10 (`segmented_map_repoint.rs:823-837`) is red on the base because the base *repairs* that flat chunk (red log: `left: Changed, right: Blocked`). The patch switches the flat arm from addressing by index to addressing by byte offset (`reconstruction.rs:669-690`), and as a side effect that object is now contained every pass and never repaired. The record is corrupt (chunk lengths sum past `u64`, size 8), so containment is defensible under ADR-0045 ("strict in maintenance paths"). But it is a flat-arm behaviour change the brief did not ask for, and its red is not evidence of a fixed defect. In the same way, leg 8's red is the base correctly draining a truly unreferenced obligation (`DELETED`). The patch freezes drains across the whole store while the `inode:x` object is owed a repair. Guard (b) requires that, but it is broader than the base's per-obligation refusal.
- [ ] C3 Change — Decide whether to allow the fourth, mechanical companion-file edit — removing the shared plan field also changes the staged constructor, outside the explicit three-file budget; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `patch.diff:604`.
- [ ] **No seeded Tier-0 DST coverage for the new concurrent write path.** This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments; then commit the segment CAS, the obligation delete and the orphan marks together. The rubric (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path. Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating gate) on this finding three times. A human has to choose: accept a `// deferred: #722` marker, which this diff does not add yet, or widen the scope.
- [ ] **The docs-currency deferral lands on this issue, and this diff drops it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring. It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out both the doc update and a fix to the marker, whose "nothing calls this yet" is now false. After merge, the deferral points at a closed issue. The rubric makes docs currency a merge requirement, and its protocol says to raise the tracking issue when a deferral looks wrong. Here the tracking issue is this one. A human decides: add the doc update to this PR, or file a new issue and re-point the marker.
- [ ] C3 Change — Accept the one-line staged initializer cleanup as an exception to the three-file cap, or return the file budget to Plan — removing the shared plan field makes the actual footprint four files; `brief.md:124`, `patch.diff:654`, `crates/custodian/src/reconstruction/staged.rs:363`.
- [ ] **A segmented object under a non-canonical key (`inode:01`) now answers `Satisfied` every pass while its obligation never drains. The base answered `Blocked` and named the object.** `reconstruction.rs:651-652` takes `parse_inode_key("inode:01") = 1`. The move then pins `inode:1` (`core/src/metadata.rs:3257-3258`), which does not exist, so the CAS (compare-and-swap) fails every pass. `RepairOutcome::Conflict` is not a hole (`reconstruction.rs:490`), so the verdict is `Satisfied`. I probed this with `seed(&meta, b"inode:01", &records())` plus `owe(CHUNK)` over 3 passes:
- [ ] **There is no seeded Tier-0 DST coverage for the new concurrent write path, and no deferral marker for it.** This is the T4 gating failure (`gate-logs/T4-batch-review.log`: 3 blocking, all this point). The rubric requires DST coverage for a new concurrent path. The repo's own marker at `backfill.rs:217-218` says the segmented write path "and the seeded Tier-0 DST case belonging to it, land together". This patch lands that path in reconstruction with only scripted Tokio races. `crates/dst/tests/custodian.rs` covers only flat reconstruction. The brief assigns that file to #722 (rebalance) and does not say #722 owns segmented-reconstruction DST. A human needs to pick one: widen this slice to add the DST case, or add a `// deferred: #N` marker near `reconstruction.rs:1156` naming the issue that owns it. Either one makes the T4 finding settled under the reviewer protocol.
- [ ] **#776's docs deferral points at this issue, and this patch does not discharge it.** `core/src/metadata.rs:3238-3240` says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". After this merges, "nothing calls this yet" is false, and the docs-currency duty has no owner. The brief makes `metadata.rs` off-limits and caps the patch at 3 files, so the builder cannot fix this. A human should allow the doc and marker edit here or re-defer both to a named issue. I found no doc sentence that this change makes false outright; §6.3's "single atomic metadata mutation" still holds.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — **The abort offset on a contained move is not tested.** `crates/custodian/src/reconstruction.rs:448-451`: I deleted `emit_aborted(plan.chunk_id);` at `:450` and ran the whole `wyrd-custodian` suite. Every test binary passed. With that mutant, a move that was contained (torn `seg:` record, version exhausted, …) still counts as a successful repair on the durability plane. The success figure `reconstruction_repaired − conflict − aborted − ceiling_refused` (`:386-393`) goes up by one for a repair that never happened. That is the "silent success" class in the rubric, and the comment at `:444-447` claims the opposite. C5's "0 missed" does not cover this line because cargo-mutants made no mutant for it. Fix: in `torn_under_the_move` (`crates/custodian/tests/segmented_map_repoint.rs:700-738`), assert that `monotonic_counter.reconstruction_aborted` ticks exactly `owed.len()` times. Smaller nit in the same place: the row `emit_aborted` prints (`reconstruction.rs:1425`) still says "could not place the rebuilt shard(s)", which is the wrong reason for a contained move.. 12 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
