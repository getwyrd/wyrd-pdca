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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.5% — 66 of 67 instrumentable changed lines executed (floor 80%); 67 of 210 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 13 mutants tested in 2m: 5 caught, 8 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.91s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #777: make reconstruction repair `seg:`-resident chunks and drain their obligations through #776’s placement primitive.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required repair, sibling merge, conflict, and full-ceiling outcomes are observable and distinguish repair from permanent refusal; `brief.md:11`, `brief.md:152`. |
| C2 Reproduction (red pre-fix) | PASS | The base compiles with the new tests retained and fails both binding repair legs: `Blocked` instead of `Changed`; 10/11 tests fail overall, with the ceiling leg passing; `reviewer-red.log:46`, `reviewer-red.log:72`, `reviewer-red.log:127`. |
| C3 Change | NEEDS-HUMAN | Accept the scope expansion or revise Plan — the three-file brief becomes four files, and shared-primitive adoption also changes flat-map overflow/version-exhaustion outcomes; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `crates/custodian/src/reconstruction.rs:685`, `crates/custodian/src/reconstruction.rs:1178`. |
| C4 Verification (red→green) | PASS | Independent red→green succeeds; 257 custodian tests pass and changed-line coverage is 66/67 (98.5%); the local dependency-audit lock failure is a host caveat, with passing frozen audit evidence; `reviewer-summary.log:3`, `reviewer-diff-coverage.log:1`, `gate-logs/C4-ci.log:3209`. |
| C5 Causal adequacy | PASS | The formerly permanent refusal now reaches an atomic placement/obligation/orphan commit, while races preserve the winner; no capability-probe or load-time symptom guard was introduced; `crates/custodian/src/reconstruction.rs:1159`, `crates/custodian/src/reconstruction.rs:1198`, `crates/custodian/tests/segmented_map_repoint.rs:529`. |
| T1 Structure | PASS | Placement pins and ceiling policy remain owned by the existing core primitive; the custodian adds its evidence to that same batch without changing core or another maintenance loop; `crates/custodian/src/reconstruction.rs:1172`, `crates/custodian/src/reconstruction.rs:1206`. |
| T2 Shape | PASS | The patch is 84,990 bytes and adds 85 nonblank, non-comment production lines, within the byte/line budgets; the separate file-count exception is the C3 decision; `reviewer-shape.log:1`, `brief.md:126`. |
| T3 Runtime | PASS | Move-time object damage blocks certification once per object, backend faults propagate, and rejected preparation writes no fragments; accounting offsets each contained repair; `crates/custodian/src/reconstruction.rs:448`, `crates/custodian/src/reconstruction.rs:1186`, `crates/custodian/tests/segmented_map_repoint.rs:683`, `crates/custodian/tests/segmented_map_repoint.rs:838`. |
| T4 Contribution | N/A | Publication artifacts are absent by design and receive their substantive audit at publish; affected-path merged and closed/rejected prior art was independently checked; `gate-logs/T4-contribution.log:10`, `reviewer-prior-art-summary.log:1`. |
| T5 Judgment | PASS | The carried-forward test gaps are closed: exact displaced orphan identity, one/two-obligation containment, backend-error propagation, and abort-counter offsets are asserted; independent mutation testing reproduces 5 caught/8 unviable, not 13 caught; `crates/custodian/tests/segmented_map_repoint.rs:505`, `crates/custodian/tests/segmented_map_repoint.rs:723`, `crates/custodian/tests/segmented_map_repoint.rs:838`, `reviewer-mutants.log:3`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated repair behavior as sufficient for this slice and decide deployment follow-up — the executable proof uses hand-seeded segmented records and scripted in-memory races, without a production segmented publisher or live disk-loss campaign; `crates/custodian/tests/segmented_map_repoint.rs:300`, `crates/custodian/tests/segmented_map_repoint.rs:392`, `AGENTS.md:78`. |

No implementation defect was established. The remaining decisions concern scope and fitness, not a failed repair or a missing compiler. Source citations above resolve under the supplied `PDCA_TARGET`; brief, patch, gate logs, and reviewer evidence resolve in this review directory. The target contains #776, compiles in both legs, and has no observed stale-base caveat.

**Scope needs an explicit disposition.** The fourth file loses only the obsolete `RepairPlan.chunk_index` initializer. The two flat-map changes replace a version-overflow panic and repair beyond an unaddressable byte offset with containment; their base failures are visible in `reviewer-red.log:26` and `reviewer-red.log:111`. These are reasonable consequences of sharing the primitive, but exceed the brief’s stated three-file boundary and deserve a scope decision rather than an automatic rebuild.

**Verification supports the behavior, with one local host limitation.** The production patch was stashed while retaining the new test, then restored. The restored tree passed the whole custodian suite, including all 11 new tests. `git apply --reverse --check ../patch.diff` subsequently succeeded, confirming that the reviewed patch remains intact (`reviewer-summary.log:7`). Legs 3–4 distinguish attempted repair from base refusal, not independently the correctness of the pins; leg 5 is intentionally green on the base.

| Frozen gate | Verdict | Re-derived evidence |
|-------------|---------|---------------------|
| C4-ci | PASS | The frozen log ends with all checks passed (`gate-logs/C4-ci.log:3830`), including real typos, docs rendering, machete, and all three deny invocations. Independently, fmt/clippy/build/workspace tests/docs/machete passed; 1,492 tests passed. Local `cargo-deny` stopped at a read-only advisory-database lock (`reviewer-ci.log:3129`), including on an offline retry. This is not a patch defect or evidence that the frozen audit was skipped. |
| C4-verify | PASS | Independent base: 1 pass/10 failures, exit 101; restored patch: all 11 pass (`reviewer-red.log:127`, `reviewer-green.log:157`, `reviewer-custodian.log:279`). |
| C4-diff-cov | PASS | Independent LCOV intersection reproduces 66/67 changed executable lines, 98.5%; only the existing unplaced-abort dispatch at `crates/custodian/src/reconstruction.rs:443` is missed by this test binary (`reviewer-diff-coverage.log:1`). |
| C5-mutants | PASS | Independent `cargo mutants -p wyrd-custodian --in-diff ../patch.diff` reproduces 13 mutants: 5 caught and 8 unviable (`reviewer-mutants.log:1`). This does not claim coverage of every conceivable manual negation. |
| T4-batch-review | PASS | The frozen wrapper output reports zero blocking, rejected, or noise findings; the supplied log contains the summary, not the underlying review bodies (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | The substantive contribution-artifact audit is deferred until publish, where it reruns (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | The frozen output shows actual compilation of both the TiKV crate and server feature selection; it establishes compile coverage, not a running TiKV service (`gate-logs/host-tikv.log:109`, `gate-logs/host-tikv.log:209`). |

Conformance vectors and the shared-global-state scanner also passed independently (`reviewer-conformance.log:1`, `reviewer-statics.log:3`). The independent DST run passed 80 tests with one ignored documentation test (`reviewer-summary.log:5`). The new move’s seeded DST property remains explicitly deferred to #682; that settled deferral is not reopened (`crates/custodian/src/reconstruction.rs:1157`). Neither are #698’s noncanonical-key handling, #702’s retired-read semantics, #723’s stranded fragments, or the chosen conflict-only `Satisfied` verdict.

**Prior art is mechanically checked, not left to the human.** All 356 closed PRs returned by the repository query were filtered by each of the four affected paths. Merged reconstruction history includes #706, #718, #770, #824 and #826; the staged file comes from #826. The matching closed-unmerged #647 was inspected: its recorded disposition requested a smaller change. No closed PR touched the new test path (`reviewer-prior-art-summary.log:3`, `reviewer-summary.log:6`). The publish-time contribution audit remains N/A independently of this completed investigation.

For fitness follow-up, observe the next Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns, as the standing rubric requests for reconstruction changes. Their success should not be represented as segmented-path evidence until they actually seed or publish segmented objects. The current tests can be rerun with `cargo test -p wyrd-custodian --test segmented_map_repoint`; the expected result is 11 passing tests. No `INTEGRATION.md` was present in the supplied target.

### Advisory — adversary

# Adversarial review — issue #777 (reconstruction completes `seg:` repair through `repoint_chunk`)

Bottom line: I could not break the main fix. Legs 1–2 go red on the base and green with the patch, for the right reasons, through `reconcile_step`. I re-ran both segmented test binaries in a scratch copy (17 tests, all pass) and ran my own attack tests and hand mutations against the whole `wyrd-custodian` suite. Three findings need a human decision. None of them is a correctness hole in the new segmented write path.

## Findings

- NEEDS-HUMAN [human] — **A segmented object under a non-canonical key goes from `Blocked` to a silent `Satisfied` forever.** `crates/custodian/src/reconstruction.rs:651-652` takes any key `parse_inode_key` accepts (`inode:01`, `inode:+1`). The move then pins and writes under the canonical `inode_key(id)` (`:1159-1171`), which does not exist, so the commit loses at `:1206-1210` every pass. Concrete case, run in scratch: a segmented root seeded at `inode:01` (the `records()` fixture), with chunk `0xA200` owed. **Base:** `Blocked` on 3 of 3 passes, nothing written. **Patch:** `Satisfied` on 3 of 3 passes, one `conflict` row per pass, the obligation is never drained, and a rebuilt fragment 1 is rewritten to the free server every pass with no reference and no orphan mark. The cause is #698 (open), and the brief forbids fixing `parse_inode_key` here. Before this patch, though, #698 only hit flat objects; segmented ones were refused and stayed visible as `Blocked`. Per the reviewer protocol ("raise the tracking issue instead"), the human should either note on #698 that it now also hides segmented objects behind `Satisfied`, or allow a narrow containment here (contain when `inode_key(id) != key`).

- NEEDS-HUMAN [human] — **A docs deferral that names #777 is left undischarged.** `crates/core/src/metadata.rs:3238-3240` reads: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring. It adds the only caller (`reconstruction.rs:1159`) but touches no architecture doc. That leaves the "nothing calls this yet" comment false. The brief's 3-file budget and its "`metadata.rs` untouched" rule make it impossible to discharge the deferral inside this bundle, and AGENTS.md treats docs currency as a merge requirement. The human should either widen the scope (the doc paragraph plus the stale comment) or re-point the deferral to a new issue before merge.

- NEEDS-HUMAN [human] — **The flat arm's behaviour changes for two corrupt-record cases the brief never asked about.** Legs 9 and 10 (`crates/custodian/tests/segmented_map_repoint.rs:810`, `:817`) are red on the base because the base flat arm behaves differently:
  - **Version `u64::MAX`:** the base panics with "attempt to add with overflow" in a debug build (C4-verify log) and wraps the version in release.
  - **Chunk lengths that overflow `u64`:** the base *repaired* the chunk and answered `Changed`.

  The patch contains both cases on every pass (`reconstruction.rs:685-690`, `:1178-1182`), which follows from #776's primitive addressing chunks by byte offset. Containment also sets `reading.incomplete`, so **every unrelated drain in the store is withheld** while such a record exists. Run in scratch: a flat record at version `u64::MAX` owing `0xA200`, plus an owed `0x0E00` that no record references, gives `Blocked` on both passes with the queue stuck at `[3584, 41472]` both times, even though the record was read in full and only its write is impossible. This is safe (no data loss), and leg 9 is clearly better than a panic or a wrapped version. But the brief says this child changes the flat arm "for nobody", and leg 10 turns a chunk the base repaired into one that only an operator can unstick. The human should confirm this is acceptable.

## Lower-impact notes (not routed; not worth a rebuild on their own)

- **The C5 result ("5 caught, 0 missed") overstates how well the tests pin the new code.** `cargo-mutants` does not mutate match-arm bodies, and two hand mutants survive the whole `wyrd-custodian` suite:
  - **(a) `reconstruction.rs:694`:** pinning the scanned `record` instead of `resolved.record`. I tested a resolve that restarts onto a superseding epoch-8 generation (the race fires after the resolver's first `seg:` page). The real code answers `Changed` and lands the repair in the new generation's `seg:1`. The mutant answers `Satisfied`: the conflict is recorded, the obligation stays queued, and a fragment is stranded. That costs one pass of liveness, and the root pin keeps it safe.
  - **(b) `:1177`:** reporting a prepare-time `Repoint::Conflict` as `Aborted`. This misattributes the counter (`aborted`/`unplaced` instead of `conflict`), because `assert_lost` (`segmented_map_repoint.rs:441`) never checks the conflict row.

  If another round happens anyway, add a restart leg and assert `reconstruction_conflict` in `assert_lost`.
- **The ceiling refusal is escalated without the fresh resolve the primitive's own contract asks for** (`metadata.rs:3134-3138`). `reconstruction.rs:452-455` turns any `Repoint::Refused` straight into `Blocked` plus a NEEDS-HUMAN audit row. If the root is superseded between resolve and move, that is a one-pass false alarm. The base flat arm had the same gap.
- **Scope:** the patch touches 4 files, against a budget of 3. The fourth is a forced one-line deletion in `crates/custodian/src/reconstruction/staged.rs:366` (`chunk_index`, after the field was dropped from `RepairPlan`). Size is within budget: 84,990 B, and about 85 added non-comment production lines (limit 100).

## What I tried and could not break

- **The red→green evidence is real** (`gate-logs/C4-verify.log`): 10 of 11 tests are red on the base, each for its stated reason. Leg 4 fails on "race never landed", leg 9 on the overflow panic, and leg 5 is green by construction. The tests drive `reconcile_step` and assert on the store, not on a copy of production. Leg 1 checks the exact orphan key, the placements, the rebuilt fragment, the unchanged root and the untouched decoy.
- **Addressing by summed byte offset is sound for segmented maps.** Segment tables must tile contiguously (`metadata.rs:1129-1135`), segment records cannot be empty (`:1385`), and segment spans are checked at decode.
- **`MalformedReplacement` cannot end the pass.** The placement always comes from `checked_fragments`, so it is always full length (`reconstruction.rs:822-825`).
- **The race and fault cases hold:**
  - A sibling edit is merged, and an edit to the planned chunk loses.
  - A root superseded after the resolve loses.
  - A record torn under the move is contained once per object, with an abort recorded for each dispatched repair.
  - A store fault under the move's `get` ends the pass.
  - Two owed chunks in one `seg:` record both land.
- **The CI gate is clean,** including `typos`, the docs render, the statics gate and the madsim DST suite (`gate-logs/C4-ci.log`).

### Advisory — code-review

No findings: the diff is clean on both requested lenses—introduced correctness defects and actionable reuse, simplification, or efficiency issues.

Reviewed byte-offset planning and containment (`crates/custodian/src/reconstruction.rs:669`), the shared repoint primitive and error handling (`crates/custodian/src/reconstruction.rs:1159`), atomic obligation/orphan updates (`crates/custodian/src/reconstruction.rs:1198`), and the new race fixtures (`crates/custodian/tests/segmented_map_repoint.rs:123`). All references were checked against `$PDCA_TARGET`.

Validation relied on the frozen gate logs: CI passed; all 11 new tests passed with the patch, with 10 failing when production was reverted; diff coverage was 98.5%; mutation testing reported 5 caught and 8 unviable mutants. No builds or tests were rerun, and the target source was not modified.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C3 Change — Accept the scope expansion or revise Plan — the three-file brief becomes four files, and shared-primitive adoption also changes flat-map overflow/version-exhaustion outcomes; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `crates/custodian/src/reconstruction.rs:685`, `crates/custodian/src/reconstruction.rs:1178`.
- [x] Validation — fitness-to-purpose — Accept the demonstrated repair behavior as sufficient for this slice and decide deployment follow-up — the executable proof uses hand-seeded segmented records and scripted in-memory races, without a production segmented publisher or live disk-loss campaign; `crates/custodian/tests/segmented_map_repoint.rs:300`, `crates/custodian/tests/segmented_map_repoint.rs:392`, `AGENTS.md:78`.
- [ ] **A segmented object under a non-canonical key goes from `Blocked` to a silent `Satisfied` forever.** `crates/custodian/src/reconstruction.rs:651-652` takes any key `parse_inode_key` accepts (`inode:01`, `inode:+1`). The move then pins and writes under the canonical `inode_key(id)` (`:1159-1171`), which does not exist, so the commit loses at `:1206-1210` every pass. Concrete case, run in scratch: a segmented root seeded at `inode:01` (the `records()` fixture), with chunk `0xA200` owed. **Base:** `Blocked` on 3 of 3 passes, nothing written. **Patch:** `Satisfied` on 3 of 3 passes, one `conflict` row per pass, the obligation is never drained, and a rebuilt fragment 1 is rewritten to the free server every pass with no reference and no orphan mark. The cause is #698 (open), and the brief forbids fixing `parse_inode_key` here. Before this patch, though, #698 only hit flat objects; segmented ones were refused and stayed visible as `Blocked`. Per the reviewer protocol ("raise the tracking issue instead"), the human should either note on #698 that it now also hides segmented objects behind `Satisfied`, or allow a narrow containment here (contain when `inode_key(id) != key`).
- [ ] **A docs deferral that names #777 is left undischarged.** `crates/core/src/metadata.rs:3238-3240` reads: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring. It adds the only caller (`reconstruction.rs:1159`) but touches no architecture doc. That leaves the "nothing calls this yet" comment false. The brief's 3-file budget and its "`metadata.rs` untouched" rule make it impossible to discharge the deferral inside this bundle, and AGENTS.md treats docs currency as a merge requirement. The human should either widen the scope (the doc paragraph plus the stale comment) or re-point the deferral to a new issue before merge.
- [x] **The flat arm's behaviour changes for two corrupt-record cases the brief never asked about.** Legs 9 and 10 (`crates/custodian/tests/segmented_map_repoint.rs:810`, `:817`) are red on the base because the base flat arm behaves differently:
- [x] C3 Change — Decide whether to allow the fourth, mechanical companion-file edit — removing the shared plan field also changes the staged constructor, outside the explicit three-file budget; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `patch.diff:604`.
- [x] **No seeded Tier-0 DST coverage for the new concurrent write path.** This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments; then commit the segment CAS, the obligation delete and the orphan marks together. The rubric (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path. Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating gate) on this finding three times. A human has to choose: accept a `// deferred: #722` marker, which this diff does not add yet, or widen the scope.
- [ ] **The docs-currency deferral lands on this issue, and this diff drops it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring. It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out both the doc update and a fix to the marker, whose "nothing calls this yet" is now false. After merge, the deferral points at a closed issue. The rubric makes docs currency a merge requirement, and its protocol says to raise the tracking issue when a deferral looks wrong. Here the tracking issue is this one. A human decides: add the doc update to this PR, or file a new issue and re-point the marker.
- [x] C3 Change — Accept the one-line staged initializer cleanup as an exception to the three-file cap, or return the file budget to Plan — removing the shared plan field makes the actual footprint four files; `brief.md:124`, `patch.diff:654`, `crates/custodian/src/reconstruction/staged.rs:363`.
- [ ] **A segmented object under a non-canonical key (`inode:01`) now answers `Satisfied` every pass while its obligation never drains. The base answered `Blocked` and named the object.** `reconstruction.rs:651-652` takes `parse_inode_key("inode:01") = 1`. The move then pins `inode:1` (`core/src/metadata.rs:3257-3258`), which does not exist, so the CAS (compare-and-swap) fails every pass. `RepairOutcome::Conflict` is not a hole (`reconstruction.rs:490`), so the verdict is `Satisfied`. I probed this with `seed(&meta, b"inode:01", &records())` plus `owe(CHUNK)` over 3 passes:
- [x] **There is no seeded Tier-0 DST coverage for the new concurrent write path, and no deferral marker for it.** This is the T4 gating failure (`gate-logs/T4-batch-review.log`: 3 blocking, all this point). The rubric requires DST coverage for a new concurrent path. The repo's own marker at `backfill.rs:217-218` says the segmented write path "and the seeded Tier-0 DST case belonging to it, land together". This patch lands that path in reconstruction with only scripted Tokio races. `crates/dst/tests/custodian.rs` covers only flat reconstruction. The brief assigns that file to #722 (rebalance) and does not say #722 owns segmented-reconstruction DST. A human needs to pick one: widen this slice to add the DST case, or add a `// deferred: #N` marker near `reconstruction.rs:1156` naming the issue that owns it. Either one makes the T4 finding settled under the reviewer protocol.
- [ ] **#776's docs deferral points at this issue, and this patch does not discharge it.** `core/src/metadata.rs:3238-3240` says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". After this merges, "nothing calls this yet" is false, and the docs-currency duty has no owner. The brief makes `metadata.rs` off-limits and caps the patch at 3 files, so the builder cannot fix this. A human should allow the doc and marker edit here or re-defer both to a named issue. I found no doc sentence that this change makes false outright; §6.3's "single atomic metadata mutation" still holds.
- [x] C3 Change — Approve the scope expansion or return it to Plan — the patch changes four files against the three-file budget and changes pre-existing flat-record overflow outcomes; the fourth-file edit removes an obsolete shared-plan initializer; `brief.md:124`, `crates/custodian/src/reconstruction/staged.rs:363`, `crates/custodian/src/reconstruction.rs:685`, `crates/custodian/src/reconstruction.rs:1178`.
- [ ] **The #698 deferral now covers segmented objects, and the verdict got worse.** Concrete case: a segmented root stored at the non-canonical key `inode:01`, owed `CHUNK`. I ran 3 passes on each tree. Base: `Blocked` every pass, nothing written. Patched: `parse_inode_key` turns `inode:01` into 1 (`reconstruction.rs:651-652`), and the move pins `inode:1` (`crates/core/src/metadata.rs:3257-3258`), which does not exist. So the commit loses with `Conflict`, and the pass answers **`Satisfied`** every pass. The obligation stays queued forever, and a rebuilt fragment is rewritten to the free server each pass (the #723 leak). The brief gives non-canonical keys to #698, so under the rubric's deferral rule this does not block the patch. But #698 should record two things: segmented objects now hit it too, and their verdict changed from `Blocked` to `Satisfied`. An object stuck forever (the C-1 case) now sits behind a clean verdict.
- [x] **A ceiling refusal on a generation the root has already left pages a human.** The primitive's contract (`crates/core/src/metadata.rs:3134-3138`) says to confirm the generation is still current with a fresh resolve before escalating a `Refused`. The caller escalates directly (`reconstruction.rs:1174-1176` → `:452-455`). Concrete case: leg 5's padded `seg:` record, with the root overwritten by a flat record that no longer names `CHUNK` between the resolve and the move's own `get`. The pass answers `Blocked` and emits one `refused-ceiling` row saying "NEEDS-HUMAN: the object's record must shrink" (`:1448`), for a record the object no longer uses. The next pass answers `Satisfied` and drains. That is one false page per race. The base's flat arm had the same gap (it weighed the scan snapshot), so this carries existing behaviour forward. Decide whether a root re-read on `Refused` belongs in this slice.
- [ ] **A docs deferral that points at this issue is now due and is not paid.** `crates/core/src/metadata.rs:3238-3240` (from #776) says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring and touches no doc. The brief keeps `metadata.rs` untouched and its 3-file budget has no room for docs, so the marker's "nothing calls this yet" is now false and stays false. A human should either budget the doc update here or point the deferral at a new issue.
- [x] **"11 test(s) ran red" (`check-gates.json`, C4-verify) overstates what was fixed.** Leg 10 (`segmented_map_repoint.rs:823-837`) is red on the base because the base *repairs* that flat chunk (red log: `left: Changed, right: Blocked`). The patch switches the flat arm from addressing by index to addressing by byte offset (`reconstruction.rs:669-690`), and as a side effect that object is now contained every pass and never repaired. The record is corrupt (chunk lengths sum past `u64`, size 8), so containment is defensible under ADR-0045 ("strict in maintenance paths"). But it is a flat-arm behaviour change the brief did not ask for, and its red is not evidence of a fixed defect. In the same way, leg 8's red is the base correctly draining a truly unreferenced obligation (`DELETED`). The patch freezes drains across the whole store while the `inode:x` object is owed a repair. Guard (b) requires that, but it is broader than the base's per-obligation refusal.

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
- Iteration delta (if iterating): Keep the fix as built; the 4th file (staged.rs one-line cleanup) and the flat-arm containment of corrupt records are accepted. Ceiling-refusal false alarm (item 206) and the DST deferral to #682 are accepted as-is. Two changes for the rebuild: 1. Non-canonical key guard: in reconstruction.rs, contain (report Blocked, do not attempt the move) when a segmented object's scanned key differs from inode_key(id), e.g. `inode:01`, so it no longer answers Satisfied forever with a never-drained obligation and a fragment rewritten each pass. Narrow guard only; do NOT fix parse_inode_key (#698 owns that). Add a test leg: segmented root at `inode:01`, owed chunk -> Blocked, nothing written, obligation still queued. 2. Discharge the docs deferral that names #777: update `docs/design/architecture/06-runtime-view.md` §6.3 and `docs/design/architecture/08-crosscutting-concepts.md` §8.7 to describe the custodian repairing seg:-resident chunks through repoint_chunk, and fix/remove the `deferred: #777` comment at crates/core/src/metadata.rs:3229-3231 ("nothing calls this yet" is now false). The human lifts the 3-file budget and the "metadata.rs untouched" fence for exactly these doc + comment edits.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Note on #698: segmented objects under a non-canonical key now reach the repair path too; #777 adds a narrow guard, but #698 still owns the real fix.
