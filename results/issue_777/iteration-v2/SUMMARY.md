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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (10 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.5% — 64 of 65 instrumentable changed lines executed (floor 80%); 65 of 199 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 12 mutants tested in 2m: 5 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.02s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #777 fix restores repair of `seg:`-resident chunks through #776’s placement primitive and passes independent red→green verification; its fourth-file scope exception still needs sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required outcome distinguishes completed repair, retryable conflict, and permanent refusal, with explicit concurrency windows and settled exclusions; `brief.md:21`, `brief.md:37`, `brief.md:61`, `brief.md:99`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing tracked changes while retaining the added test produced a compiling baseline with 9 failures and 1 pass; repair and sibling-merge assertions independently reproduce the defect; `reviewer-rerun.log:150`, `reviewer-rerun.log:261`. |
| C3 Change | NEEDS-HUMAN | Accept the one-line staged initializer cleanup as an exception to the three-file cap, or return the file budget to Plan — removing the shared plan field makes the actual footprint four files; `brief.md:124`, `patch.diff:654`, `crates/custodian/src/reconstruction/staged.rs:363`. |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 10 discriminator tests, and a separate clean-cache run passed all 256 custodian tests; full-workspace completion is supported by the frozen CI log, subject to the rerun limitation below; `reviewer-rerun.log:300`, `reviewer-clean-green.log:571`, `gate-logs/C4-ci.log:3829`. |
| C5 Causal adequacy | PASS | The formerly permanent refusal now exits through the real placement commit and obligation discharge; the added checks classify corrupt records rather than probe an assumed capability, so the symptom-guard trigger does not apply; `crates/custodian/src/reconstruction.rs:1156`, `crates/custodian/src/reconstruction.rs:1195`, `crates/custodian/tests/segmented_map_repoint.rs:474`. |
| T1 Structure | PASS | Placement, obligation deletion, and displaced-position evidence retain one conditional batch through the existing core/trait seam, preserving atomicity and the segmented root; `crates/custodian/src/reconstruction.rs:1169`, `crates/custodian/src/reconstruction.rs:1195`, `crates/custodian/tests/segmented_map_repoint.rs:496`. |
| T2 Shape | PASS | The 81,189-byte patch fits the 85 KB limit; 69 added noncomment code-bearing production lines fit the semantic-line budget, with the separate file-count exception recorded under C3; `brief.md:126`, `patch.diff:1`. |
| T3 Runtime | PASS | Tests exercise sibling merge, both conflict cases, the full value ceiling, and once-per-object containment while preserving queued work; the change adds no clock source or spawned task; `crates/custodian/tests/segmented_map_repoint.rs:520`, `crates/custodian/tests/segmented_map_repoint.rs:628`, `crates/custodian/tests/segmented_map_repoint.rs:676`, `reviewer-clean-green.log:407`. |
| T4 Contribution | PASS | All four affected paths were checked against merged history and every closed-unmerged PR; the batch review’s three DST findings belong to the explicit #722 deferral, and the publish-artifact audit is N/A at Check; `reviewer-prior-art-paths.log:2`, `reviewer-prior-art-paths.log:185`, `brief.md:114`, `AGENTS.md:200`. |
| T5 Judgment | PASS | Independent executable negations show that the tests detect weakened chunk/root pins, a removed ceiling, lost containment, and wrong orphan identity; each counted failure reached an assertion rather than merely failing compilation; `reviewer-negations.log:2`, `reviewer-negations.log:31`, `reviewer-negations.log:60`, `reviewer-negations.log:107`, `reviewer-negations.log:136`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether restored segmented repair with retryable conflicts and explicit containment satisfies C-1 for the intended release — executable seam tests establish behavior, while operational fitness remains the sign-off decision; `brief.md:152`, `crates/custodian/tests/segmented_map_repoint.rs:469`. |

No implementation defect was found. The scope decision is narrow: `staged.rs` loses only the obsolete `chunk_index` initializer after that field is removed from the shared `RepairPlan`. The brief still budgets three files. Its other exclusions remain intact, including core metadata, rebalance, the resolver, #698 canonicalisation, and #723 stranded-fragment handling.

The safety assertions discriminate the relevant failures. In a separate scratch copy, weakening the planned-reference equality made leg 3 fail; removing the root pin made leg 4 fail; removing the ceiling check made leg 5 fail. Disabling move-time containment produced `Satisfied` instead of `Blocked`, and changing the displaced orphan’s server ID failed the exact-key assertion. The first containment mutant was unviable because of `unused_mut`; only the corrected, compiling mutant counts as evidence (`reviewer-negations.log:89`, `reviewer-negations.log:136`). Legs 3–4’s baseline failures remain coarse evidence of attempting the move, and leg 5 remains green on the baseline, as the brief requires.

The frozen gate results and their review dispositions are:

| Gate | Recorded result / disposition | Evidence |
|------|-------------------------------|----------|
| C4-ci | PASS; full completion comes from the captured gate run. | The log shows real typos/docs rendering, workspace checks, cargo-machete, cargo-deny, conformance and DST execution; `gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3203`, `gate-logs/C4-ci.log:3487`, `gate-logs/C4-ci.log:3829`. |
| C4-verify | PASS; independently reproduced. | 10 tests ran in both legs: baseline 1 passed / 9 failed, patched 10 passed; `reviewer-rerun.log:261`, `reviewer-rerun.log:300`. |
| C4-diff-cov | PASS from captured evidence. | 64/65 instrumentable changed lines executed, 98.5%; the uncovered line propagates a non-ChunkMapError store fault; `gate-logs/C4-diff-cov.log:50`, `crates/custodian/src/reconstruction.rs:1185`. |
| C5-mutants | PASS from captured evidence, supplemented by independent named negations. | 5 caught and 7 unviable, not 12 caught; `gate-logs/C5-mutants.log:13`, `reviewer-negations.log:136`. |
| T4-batch-review | FAIL as recorded; all three findings declined as the same tracked, out-of-scope DST request. | `gate-logs/T4-batch-review.log:10`; the brief assigns the DST file to #722 (`brief.md:114`), and #722 explicitly carries the repoint-versus-supersede property (`reviewer-deferral-summary.log:3`). The reviewer does not change the deterministic gate result. |
| T4-contribution | N/A — contribution artifacts are drafted after Check and the substantive audit reruns at publish. | `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS from captured compiler output; no claim of live TiKV integration. | Both requested feature selections completed clippy; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`. |

The independent full-CI attempt passed the prose checks, formatting, workspace clippy and build, but stalled in the unchanged server `custodian_gc` test binary with seven tests reporting more than 60 seconds. I interrupted that run; all 10 tests in that binary then passed with `--test-threads=1` (`reviewer-ci.log:2217`, `reviewer-checks.log:358`). This is an unresolved parallel-run limitation, not evidence of a patch defect. Statics and conformance were also rerun successfully (`reviewer-checks.log:348`). An intermediate run reused the mutation build cache and repeated the containment mutant’s failures; it is superseded by the fresh-cache, fully passing custodian run (`reviewer-clean-green.log:1`, `reviewer-clean-green.log:571`). The captured gate evidence shows the brief’s named external tools running, with no reported skip.

Prior art is mechanically checked by affected path: main history contains 19 reconstruction commits, 2 staged-reconstruction commits, 3 existing-test commits, and no earlier new-test file. Of 356 closed PRs, all 19 unmerged PRs were scanned by changed path; only ancestor #647 overlaps (`reviewer-prior-art-paths.log:2`, `reviewer-prior-art-paths.log:30`, `reviewer-prior-art-paths.log:40`, `reviewer-prior-art-paths.log:185`). The patch uses the shared primitive/resolver rather than reviving that ancestor’s private resolver. Existing #722 and #723 deferrals remain settled. Observing the existing Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns is warranted for this durability change, as `AGENTS.md:78` requests; neither was run here or treated as a new merge gate.

All source citations refer to the supplied `$PDCA_TARGET`; brief, patch, gate-log and reviewer-log citations refer to this review directory. The supplied target contains #776, the patch reverse-check succeeds, and its original patch state was restored.

### Advisory — adversary

# Adversarial review — issue #777 (advisory)

**Bottom line:** the red→green holds up, and the main claim stands: a `seg:`-resident chunk is now repaired through #776's `repoint_chunk`, and the obligation drains in the same batch. I found one concrete input where the fix turns a loud refusal into a permanent `Satisfied`, one untested error arm that a hand mutant shows is not pinned, and two scope items a human has to settle. T4 still fails (gating) on the DST point below.

## What I re-ran (scratch copy of `$PDCA_TARGET`, cargo 1.96)

- Fix applied: `segmented_map_repoint` 10/10 green; `segmented_map_reconstruction` 6/6 green.
- Production files reverted (`reconstruction.rs`, `staged.rs`), test kept: 9 of 10 red. Leg 5 is green, as the brief predicts. The rewritten `an_obligation_inside_a_segmented_object_is_repaired_never_discarded` also goes red. This matches `gate-logs/C4-verify.log` line for line.
- The tests drive the real `reconcile_step` and assert on the store. They do not use a parallel copy of production. The leg-2 race hook sits where the brief requires: `segmented_map_repoint.rs:112-123` builds the page first and only then fires the race, and `get` never fires it.

## Findings

- NEEDS-HUMAN [human] — **A segmented object under a non-canonical key (`inode:01`) now answers `Satisfied` every pass while its obligation never drains. The base answered `Blocked` and named the object.** `reconstruction.rs:651-652` takes `parse_inode_key("inode:01") = 1`. The move then pins `inode:1` (`core/src/metadata.rs:3257-3258`), which does not exist, so the CAS (compare-and-swap) fails every pass. `RepairOutcome::Conflict` is not a hole (`reconstruction.rs:490`), so the verdict is `Satisfied`. I probed this with `seed(&meta, b"inode:01", &records())` plus `owe(CHUNK)` over 3 passes:
  - Patched: `[Satisfied, Satisfied, Satisfied]`. The obligation stays queued, nothing names `inode:01`, and a rebuilt fragment is written to the free server each pass with no mark (the #723 leak).
  - Base: `[Blocked, Blocked, Blocked]`, with an audit row naming `inode:01`.

  This breaks leg 3's stated reason for `Satisfied` ("the next pass re-plans onto the winner's bytes"). For this input the retry never succeeds. The brief gives non-canonical keys to #698 and the code comment at `reconstruction.rs:550-555` says so too, so by the rubric this belongs on #698, not as an in-PR fix. A human should either record on #698 that segmented objects went from `Blocked` to "Satisfied forever", or allow a narrow guard here: contain a segmented object whose key does not round-trip (`inode_key(id) != key`), the same way the `(None, None)` arm at `:659` and `:679-684` already does. That guard would not touch `parse_inode_key`.
- NEEDS-HUMAN [impl] — **The move's store-fault arm is untested, and a mutant there survives the whole `wyrd-custodian` suite.** At `reconstruction.rs:1185`, `Err(err) => return Err(err)` is the diff-cov MISS. I changed it to `return Ok(contained(err.to_string()))` and ran `cargo test -p wyrd-custodian`: every test binary passed. With that mutant, a metadata-store outage during the move's `seg:` read is reported as "object `inode:N` is unreadable" and the pass continues. That is the exact confusion `06-runtime-view.md:29` says a maintenance pass must avoid ("this object is unreadable" vs "the store is failing"). C5's "0 missed" does not cover this: cargo-mutants made no mutant for the arm. Fix: add the move-side twin of `segmented_map_reconstruction.rs:673` (`a_store_fault_under_the_resolver_ends_the_pass_after_the_name_is_out`). Make `MemMeta::get` return `Err` for a `seg:` key, then assert the pass returns `Err`, nothing drains, and no `unresolvable-chunk-map` row names the object.
- NEEDS-HUMAN [human] — **There is no seeded Tier-0 DST coverage for the new concurrent write path, and no deferral marker for it.** This is the T4 gating failure (`gate-logs/T4-batch-review.log`: 3 blocking, all this point). The rubric requires DST coverage for a new concurrent path. The repo's own marker at `backfill.rs:217-218` says the segmented write path "and the seeded Tier-0 DST case belonging to it, land together". This patch lands that path in reconstruction with only scripted Tokio races. `crates/dst/tests/custodian.rs` covers only flat reconstruction. The brief assigns that file to #722 (rebalance) and does not say #722 owns segmented-reconstruction DST. A human needs to pick one: widen this slice to add the DST case, or add a `// deferred: #N` marker near `reconstruction.rs:1156` naming the issue that owns it. Either one makes the T4 finding settled under the reviewer protocol.
- NEEDS-HUMAN [human] — **#776's docs deferral points at this issue, and this patch does not discharge it.** `core/src/metadata.rs:3238-3240` says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". After this merges, "nothing calls this yet" is false, and the docs-currency duty has no owner. The brief makes `metadata.rs` off-limits and caps the patch at 3 files, so the builder cannot fix this. A human should allow the doc and marker edit here or re-defer both to a named issue. I found no doc sentence that this change makes false outright; §6.3's "single atomic metadata mutation" still holds.
- Informational, not a defect — **4 files, against a 3-file budget.** `reconstruction/staged.rs:366-369` loses `chunk_index: site.index` because `RepairPlan.chunk_index` was removed. It is a forced one-line edit and harmless. Keeping the field would have stayed in budget. The other limits hold: 81 added non-comment production lines (limit 100) and an 81,189-byte `patch.diff` (limit 85 KB).

## Attempted and could not refute

- **Byte-offset addressing.** I traced the offset the walk computes (`reconstruction.rs:669-672`) against `segment_may_hold` and `chunk_at` (`metadata.rs:3355-3380`). The cases were: a chunk in the second segment at a non-zero offset, a zero-length chunk at a segment's end, one at a segment's start, and an empty segment. In every case the move picks the same reference the walk did. For a flat map, offset plus equality picks the same index the base did.
- **Pins.** Leg 3 (equality pin) and leg 4 (root pin) each go red if their pin is removed: the repair would land on the racer's bytes or on a superseded root. Leg 2 goes red if the move pinned the bytes the resolver saw instead of the bytes it re-reads.
- **Drains over a move-time containment.** These are withheld. `Contained` sets `reading.incomplete` before the drain commit at `:477`, which runs after the repair loop. Once-per-object accounting holds for two obligations in one torn record (leg 7).
- **Writes before refusal.** Nothing is written ahead of a refusal, conflict-at-prepare, or containment: `put_fragment` runs only after `Repoint::Prepared` (`:1169-1193`).
- **Flat-arm changes.** A record at `u64::MAX` and chunk lengths that overflow are now contained rather than repaired. On the base the first panicked in debug builds and would wrap in release; the second was repaired by index despite the corrupt record. Both are named and `Blocked`. I don't count that as a refutation.

### Advisory — code-review

No additional findings in either advisory lens: no introduced correctness bug or actionable reuse, simplification, or efficiency issue found.

Reviewed the resolved-generation snapshot and byte-offset handling (`crates/custodian/src/reconstruction.rs:669`), move-time containment (`crates/custodian/src/reconstruction.rs:448`), and shared-primitive integration with atomic placement, obligation deletion, and orphan evidence (`crates/custodian/src/reconstruction.rs:1156`). The race fixtures exercise the intended read/commit windows, and the success test now checks the exact displaced orphan identity (`crates/custodian/tests/segmented_map_repoint.rs:496`).

Validation relied on the frozen gate logs; tests were not rerun and the target was not modified. The existing T4 finding about seeded DST coverage remains recorded in the gate evidence; this advisory does not override it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Accept the one-line staged initializer cleanup as an exception to the three-file cap, or return the file budget to Plan — removing the shared plan field makes the actual footprint four files; `brief.md:124`, `patch.diff:654`, `crates/custodian/src/reconstruction/staged.rs:363`.
- [ ] Validation — fitness-to-purpose — Decide whether restored segmented repair with retryable conflicts and explicit containment satisfies C-1 for the intended release — executable seam tests establish behavior, while operational fitness remains the sign-off decision; `brief.md:152`, `crates/custodian/tests/segmented_map_repoint.rs:469`.
- [ ] **A segmented object under a non-canonical key (`inode:01`) now answers `Satisfied` every pass while its obligation never drains. The base answered `Blocked` and named the object.** `reconstruction.rs:651-652` takes `parse_inode_key("inode:01") = 1`. The move then pins `inode:1` (`core/src/metadata.rs:3257-3258`), which does not exist, so the CAS (compare-and-swap) fails every pass. `RepairOutcome::Conflict` is not a hole (`reconstruction.rs:490`), so the verdict is `Satisfied`. I probed this with `seed(&meta, b"inode:01", &records())` plus `owe(CHUNK)` over 3 passes:
- [ ] **The move's store-fault arm is untested, and a mutant there survives the whole `wyrd-custodian` suite.** At `reconstruction.rs:1185`, `Err(err) => return Err(err)` is the diff-cov MISS. I changed it to `return Ok(contained(err.to_string()))` and ran `cargo test -p wyrd-custodian`: every test binary passed. With that mutant, a metadata-store outage during the move's `seg:` read is reported as "object `inode:N` is unreadable" and the pass continues. That is the exact confusion `06-runtime-view.md:29` says a maintenance pass must avoid ("this object is unreadable" vs "the store is failing"). C5's "0 missed" does not cover this: cargo-mutants made no mutant for the arm. Fix: add the move-side twin of `segmented_map_reconstruction.rs:673` (`a_store_fault_under_the_resolver_ends_the_pass_after_the_name_is_out`). Make `MemMeta::get` return `Err` for a `seg:` key, then assert the pass returns `Err`, nothing drains, and no `unresolvable-chunk-map` row names the object.
- [ ] **There is no seeded Tier-0 DST coverage for the new concurrent write path, and no deferral marker for it.** This is the T4 gating failure (`gate-logs/T4-batch-review.log`: 3 blocking, all this point). The rubric requires DST coverage for a new concurrent path. The repo's own marker at `backfill.rs:217-218` says the segmented write path "and the seeded Tier-0 DST case belonging to it, land together". This patch lands that path in reconstruction with only scripted Tokio races. `crates/dst/tests/custodian.rs` covers only flat reconstruction. The brief assigns that file to #722 (rebalance) and does not say #722 owns segmented-reconstruction DST. A human needs to pick one: widen this slice to add the DST case, or add a `// deferred: #N` marker near `reconstruction.rs:1156` naming the issue that owns it. Either one makes the T4 finding settled under the reviewer protocol.
- [ ] **#776's docs deferral points at this issue, and this patch does not discharge it.** `core/src/metadata.rs:3238-3240` says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". After this merges, "nothing calls this yet" is false, and the docs-currency duty has no owner. The brief makes `metadata.rs` off-limits and caps the patch at 3 files, so the builder cannot fix this. A human should allow the doc and marker edit here or re-defer both to a named issue. I found no doc sentence that this change makes false outright; §6.3's "single atomic metadata mutation" still holds.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- [ ] C3 Change — Decide whether to allow the fourth, mechanical companion-file edit — removing the shared plan field also changes the staged constructor, outside the explicit three-file budget; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `patch.diff:604`.
- [ ] **No seeded Tier-0 DST coverage for the new concurrent write path.** This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments; then commit the segment CAS, the obligation delete and the orphan marks together. The rubric (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path. Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating gate) on this finding three times. A human has to choose: accept a `// deferred: #722` marker, which this diff does not add yet, or widen the scope.
- [ ] **The docs-currency deferral lands on this issue, and this diff drops it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring. It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out both the doc update and a fix to the marker, whose "nothing calls this yet" is now false. After merge, the deferral points at a closed issue. The rubric makes docs currency a merge requirement, and its protocol says to raise the tracking issue when a deferral looks wrong. Here the tracking issue is this one. A human decides: add the doc update to this PR, or file a new issue and re-point the marker.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — **The move's store-fault arm is untested, and a mutant there survives the whole `wyrd-custodian` suite.** At `reconstruction.rs:1185`, `Err(err) => return Err(err)` is the diff-cov MISS. I changed it to `return Ok(contained(err.to_string()))` and ran `cargo test -p wyrd-custodian`: every test binary passed. With that mutant, a metadata-store outage during the move's `seg:` read is reported as "object `inode:N` is unreadable" and the pass continues. That is the exact confusion `06-runtime-view.md:29` says a maintenance pass must avoid ("this object is unreadable" vs "the store is failing"). C5's "0 missed" does not cover this: cargo-mutants made no mutant for the arm. Fix: add the move-side twin of `segmented_map_reconstruction.rs:673` (`a_store_fault_under_the_resolver_ends_the_pass_after_the_name_is_out`). Make `MemMeta::get` return `Err` for a `seg:` key, then assert the pass returns `Err`, nothing drains, and no `unresolvable-chunk-map` row names the object.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
