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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (13 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.6% — 69 of 70 instrumentable changed lines executed (floor 80%); 70 of 222 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 16 mutants tested in 2m: 6 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #777: restore repairs of under-replicated chunks in `seg:` records through the existing placement primitive; implementation checks pass, with release fitness reserved for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The latest accepted scope defines observable repair, conflict, containment, and generation-restart outcomes; `brief.md:218`, `crates/custodian/tests/segmented_map_repoint.rs:929`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix reproduces the permanent refusal: 13 tests run, 12 fail and the ceiling case passes; `reviewer-red.log:148`, `crates/custodian/tests/segmented_map_repoint.rs:504`. |
| C3 Change | PASS | All seven affected files fit the explicit scope/budget overrides and latest three requests; core changes are documentation only; `brief.md:209`, `brief.md:218`, `crates/core/src/metadata.rs:3238`. |
| C4 Verification (red→green) | PASS | Independent restoration yields 13/13 regression tests and 259 custodian tests passing; frozen full CI passes, while the reviewer CI has the host caveat below; `reviewer-evidence.log:5`, `gate-logs/C4-ci.log:3832`. |
| C5 Causal adequacy | PASS | A successful repair now ends the refused-forever state without losing concurrent sibling edits; root/reference/ceiling negations each fail their intended assertion, and no optional-capability probe masks a load-time cause; `crates/custodian/src/reconstruction.rs:1167`, `reviewer-named-mutants.log:9`. |
| T1 Structure | PASS | CAS and value limits remain owned by the shared primitive; repair obligation and orphan evidence join its single batch through the existing trait seam; `crates/custodian/src/reconstruction.rs:1180`, `crates/custodian/src/reconstruction.rs:1206`. |
| T2 Shape | PASS | Shared resolved snapshots and first-reference selection preserve one plan per chunk without copying an entire object per obligation; `crates/custodian/src/reconstruction.rs:684`, `crates/custodian/src/reconstruction.rs:699`. |
| T3 Runtime | PASS | Unusable records withhold certification once per object, backend faults terminate the pass, and existing clock/write-order semantics are preserved; `crates/custodian/src/reconstruction.rs:448`, `crates/custodian/src/reconstruction.rs:1194`, `crates/custodian/src/reconstruction.rs:1210`. |
| T4 Contribution | PASS | Merged history and closed/rejected work were checked by all seven affected paths; overlapping closed PR #647 required the smaller split followed here; publication-artifact gate is N/A until publish; `reviewer-prior-art.json:2`, `reviewer-pr647-comments.json:4`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The latest assertions distinguish a stale resolved generation and a conflict misreported as an abort; both named mutants are independently caught, and orphan identity is exact; `crates/custodian/tests/segmented_map_repoint.rs:491`, `crates/custodian/tests/segmented_map_repoint.rs:539`, `reviewer-named-mutants.log:3`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the restored redundancy behavior and its Changed/Satisfied/Blocked operator contract for release — executable checks establish mechanics, while deployment fitness remains the owner's decision; `docs/design/architecture/06-runtime-view.md:39`, `crates/custodian/tests/segmented_map_repoint.rs:504`. |

No new implementation defect was found. Source citations above ground on the supplied `$PDCA_TARGET`; it contains the prerequisite primitive and the patch, and its complete patch passes a reverse-application check. The tracked fix was restored byte-for-byte after the red leg. `reviewer-evidence.log:1` records the target and rerun commands.

The evidence supports the repair and its race protections. Independently rerunning `cargo mutants --in-diff` produced **6 caught, 10 unviable, 0 missed**; unviable mutations are not counted as tested protections (`reviewer-mutants.log:19`). Five additional named mutations each compiled and failed the intended regression: wrong resolved root, conflict relabelled as abort, omitted root pin, omitted chunk-reference pin, and omitted value ceiling. The initial equality-pin mutation failed compilation for an unused parameter; a viable form preserving parameter use then failed the assertion, and restoring the source returned all 13 tests to green (`reviewer-named-mutants.log:17`). The base's coarse-red conflict cases are not used as proof of the individual CAS pins.

The reviewer CI limitation is environmental, and the frozen log supplies the remaining gate evidence. With Rust 1.96.0, typos, docs lint/render/link audit, structural guards, formatting, workspace clippy/build/tests, and cargo-machete passed independently. CI then stopped because cargo-deny could not exclusively lock `/home/eddie/.cargo/advisory-dbs/db.lock`, a read-only path (`reviewer-ci-pinned.log:3220`). The frozen CI log actually shows cargo-deny succeeding (`gate-logs/C4-ci.log:3224`, `gate-logs/C4-ci.log:3238`) and the full pipeline passing (`gate-logs/C4-ci.log:3832`); this is not a patch failure or an unexercised dependency. Conformance and the statics scanner also passed independently (`reviewer-conformance.log:1`, `reviewer-statics.log:3`). The frozen coverage log reports 69/70 instrumentable changed lines covered, with the miss at the existing generic abort dispatch (`gate-logs/C4-diff-cov.log:480`); the frozen TiKV log records actual successful crate and server feature builds (`gate-logs/host-tikv.log:209`). The instance-scoped coverage and batch-review wrappers were adjudicated from their captured logs; batch review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`).

T4-contribution is **N/A**: `pr-description.md` is absent by design and the substantive artifact check must rerun at publish (`gate-logs/T4-contribution.log:10`). Prior art was checked against merged history for each affected path and the file lists of all 19 unmerged closed PRs, drawn from 356 closed PRs. Only #647 overlaps; its recorded disposition asks for a smaller change (`reviewer-prior-art.json:321`, `reviewer-pr647-comments.json:4`). The accepted scope expansions, conflict verdict, and tracked #682/#698/#723 deferrals remain settled (`brief.md:218`). Tier-1 disk-fault and Tier-2 kill-reconstruct campaigns warrant deployment observation via `cargo xtask disk-faults` and `cargo xtask kill-reconstruct`; they were not run here and are not represented as passing. No `INTEGRATION.md` exists in the supplied target.

### Advisory — adversary

# Adversarial review — issue #777 (iteration 6)

**Verdict: I could not refute the fix.** I re-ran the proof, ran three hand-made mutants, and
ran six attack tests of my own against a scratch copy of `$PDCA_TARGET`. The production change
held every time. Nothing below is gating, and nothing needs a rebuild. The three notes at the
end are small, and two of them are pre-existing or belong to #776.

## Evidence: re-run, and checked that it uses the production path

- `crates/custodian/tests/segmented_map_repoint.rs` passes 13/13 at the target, and the rewritten
  `segmented_map_reconstruction.rs` passes 6/6. The red leg in `gate-logs/C4-verify.log`
  matches the module doc at `segmented_map_repoint.rs:14-27`: 12 of 13 tests fail on the base.
  Leg 3 fails on `Blocked` vs `Satisfied` (`:486`), leg 4 fails on "the race never landed"
  (`:470`), leg 9 fails on the base's own `u64` overflow panic at the old `reconstruction.rs:1157`,
  and leg 5 is green, as expected.
- Every leg drives `reconcile_step` (`segmented_map_repoint.rs:432`), so none of them uses a
  separate copy of the logic. The test double's `commit` (`:132-147`) checks each precondition
  byte for byte, so a CAS cannot pass by accident. The race hooks fire after the `scan_page`
  page (`:126-128`), as the brief requires, and each race leg asserts `meta.raced()`, so a leg
  cannot pass if its race never happened.
- Attempted: a test that passes for the wrong reason. Leg 1 (`:530-536`) now checks the exact
  orphan key, so a mark on the survivor or the destination would fail it. Leg 6/7
  (`:745-765`) checks the abort count and reason, and `assert_lost` (`:491-494`) checks
  conflict=1 and aborted=0. Could not refute.

## Hand-made mutants (cargo-mutants made only 6 viable mutants, so C5 alone says little)

- Removed the once-per-object dedupe in `Reading::contain` (`reconstruction.rs:547-549`).
  Leg 7 fails at `segmented_map_repoint.rs:749`. **Caught.**
- Changed `(None, None) => None` to `continue` at `reconstruction.rs:667`, which silently skips
  an unparsable segmented object. Leg 8 fails at `:794`. **Caught.**
- Built `prior` from the scanned `record.clone()` instead of `resolved.record`
  (`reconstruction.rs:702`). Leg 13 fails at `:940`. **Caught.**

## Attack tests (scratch only, not shipped) — trying to break the fix

- **Restart onto a FLAT live root** (a segmented→flat supersede lands during the resolve). The
  pass answers `Changed`, the flat root goes to version 3 with `CHUNK` on `[0, FREE]`, and the
  queue is empty. `prior = resolved.record` (`reconstruction.rs:702`) handles the shape change
  correctly.
- **One object, two owed chunks: the seg 0 chunk is healthy, the seg 1 record gets torn under
  the move.** The pass answers `Blocked`. The seg 0 chunk is repaired and drained, the object
  is named once (`(1,1,true)`), there is one abort, and `CHUNK` stays queued. Guard (b) holds:
  the move's containment sets `reading.incomplete` before the drain gate at `:477`.
- **Two owed chunks in the same `seg:` record, no race.** Both land in one pass (placements
  `[[0,2],[0,2]]`, 2 orphan marks), which matches the comment at `reconstruction.rs:415-419`.
- **Next pass after leg 3's lost race.** The pass answers `Changed` and the record ends up
  `[[0,2],[0,1]]` with `orphan:7:41472:1`. So "a lost CAS is a retry" holds: the next pass
  re-plans onto the winner's placement (C-1 holds).
- **Sibling edit that lands AFTER the move's own re-read** (on the way into `commit`). The pass
  answers `Satisfied` with one conflict, and the next pass answers `Changed` and keeps the
  sibling's placement. This is safe; see the doc note below.

## Notes (advisory, not routed)

- `docs/design/architecture/06-runtime-view.md:40` says "so a concurrent move of a *sibling*
  chunk in the same segment record is merged". That is only true when the sibling edit lands
  **before** the move's re-read. The attack test above shows a counter-case: a sibling edit
  that lands between the re-read and the commit makes the repair lose (one conflict, one
  stranded rebuilt fragment, #723) and retry next pass. `crates/core/src/metadata.rs:3208-3209`
  states this correctly ("one that lands after it fails the CAS"). The comment at
  `reconstruction.rs:1154-1155` is close enough because it says why ("as IT re-reads it").
  This is a one-clause fix to the doc. I'm not routing it because it is not worth a review
  round on its own (the rubric's definition of done).
- Attempted and not raised: a segmented root whose stored bytes are not the canonical
  `encode(decode(bytes))`. For example, a stray leading space would do it. `InodeRecordWire`
  (`metadata.rs:1659-1671`) does not deny unknown fields, so a root written by a newer build
  would also do it. Such a root loses the root pin every pass: `Satisfied`, the obligation
  never drains, and one fragment is rewritten each pass. I confirmed this over 3 passes in
  scratch. It is the same symptom the human closed for the `inode:01` key. But the flat arm on
  the base had the identical `require(inode_key, encode(&object.prior))` pin, the pin is #776's
  (`metadata.rs:3257`), and no writer in this build emits such bytes (one encoder, with
  `skip_serializing_if`; `08-crosscutting-concepts.md` §8.7 already treats round-trip identity
  as a system-wide rule). So it is not this diff's defect.
- `reconstruction.rs:443` is the only line diff-coverage reports as missed. It is the new
  `"unplaced"` reason label on the abort taken when the selector picks a server outside the
  fleet. No test checks that label, so swapping it for `"unresolvable-chunk-map"` would go
  unnoticed. The cost is cosmetic (the abort counter is right either way), so I'm not routing it.

## Reviewer-verdict check

- No claim in `check-gates.json` looks unwarranted. "13 test(s) ran red" counts tests that
  ran, not tests that failed (12 failed), as the brief warns. T4 shows 0 blocking, and that is
  consistent with what I found. C5's "0 missed" covers only 6 viable mutants, so it is weak
  evidence on its own. The hand-made mutants above cover the gap for the new lines.

### Advisory — code-review

- **Correctness:** No introduced bugs or actionable test defects found. Reviewed generation and chunk pins, race handling, containment, and atomic repair evidence against the target source (`crates/custodian/src/reconstruction.rs:702`, `crates/custodian/src/reconstruction.rs:1167`, `crates/custodian/src/reconstruction.rs:1206`; `crates/custodian/tests/segmented_map_repoint.rs:469`, `crates/custodian/tests/segmented_map_repoint.rs:929`).
- **Reuse / simplification / efficiency:** No actionable findings. Reconstruction reuses `metadata::repoint_chunk` and retains one shared object snapshot per pass (`crates/custodian/src/reconstruction.rs:699`, `crates/custodian/src/reconstruction.rs:1167`).

Validation: reviewed the supplied frozen gate logs; CI passed, all 13 new tests passed, diff coverage was 98.6%, and mutation testing reported 6 caught and 10 unviable mutants. No builds were rerun or target files modified. Settled deferrals were not reopened.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the restored redundancy behavior and its Changed/Satisfied/Blocked operator contract for release — executable checks establish mechanics, while deployment fitness remains the owner's decision; `docs/design/architecture/06-runtime-view.md:39`, `crates/custodian/tests/segmented_map_repoint.rs:504`.

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
