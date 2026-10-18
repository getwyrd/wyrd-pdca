# Result — issue 722 / segmented-drain-evacuation-completes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **A D-server decommission holding a `seg:`-resident fragment never
  converges.** #696 stopped rebalance aborting on a segmented object, but it deliberately
  writes nothing: an evacuation owed by a chunk whose `ChunkRef` lives in a `seg:` record is
  **refused and stays owed**, every pass, forever (`crates/custodian/src/rebalance.rs:352-355`
  — `scanned_flat` is `None`, so the chunk produces no plan and the object is counted refused
  at `:378-381`; the drain is never certified, `:185-190`). The operator's drain therefore never
  completes and the server can never be retired. Sibling slice #721 lands the placement move for
  whichever record holds a chunk; this child is the drain caller completing through it.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_evacuate.rs`
  passes, driven **only** through symbols visible on the base (post-#721) —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Four legs:
  1. **BINDING, RED pre-fix — a `seg:`-resident fragment is evacuated off a draining server.**
     Seed a committed **segmented** object (raw `seg:` records + a segmented root, never a
     committer); `set_lifecycle(.., Draining)` on the server holding one of its fragments; run
     `reconcile_step` with a `RebalanceContext`. Assert: the fragment is copied to a
     non-draining server in a failure domain distinct from the survivors; the **`seg:` record's**
     `ChunkRef.placement` names it and no longer names the draining server; the vacated position
     is orphan-marked; the pass answers `Changed`; and the **root** record's bytes are
     **unchanged**. Base: refused, placement unchanged → **red**.
  2. **BINDING, RED pre-fix — EVERY committed reference to a shared chunk is repointed, so the
     drain converges.** Seed **two** committed **segmented** objects whose maps both name the
     same `ChunkId`, one of whose positions is on the draining server. Run the pass. Assert:
     **neither object's placement names the draining server** afterwards, and **every fragment
     either object names is present** on the D server it names. Base: refused, both still
     naming the draining server → **red**.
     **Read this as a convergence property, not a dedup requirement.** The target already
     plans per *(object, chunk)* (`rebalance.rs:264`, `:318`, `:367-376`), which repoints both
     references and satisfies this leg once the segmented refusal is gone; that shape is
     **acceptable and is the expected outcome**. What the leg forbids is the opposite error —
     borrowing reconstruction's **first-committed-reference-wins** dedup
     (`reconstruction.rs:524-529`), which would repoint only the first object and leave the
     second naming a server that no longer holds the fragment. Do **not** add cross-object
     dedup here (see Out of scope).
  3. **NOT independently red — the ceiling refusal holds over a segment record on the drain
     arm, at the V/2 bound.** A `seg:` record seeded just under **V/2**
     (`MAX_ROOT_VALUE_BYTES`, `50_000`, `metadata.rs:352`) whose repoint would cross it:
     refused, record byte-identical, nothing copied, the drain **not** certified. Key the
     fixture to **V/2, not `MAX_VALUE_BYTES`** — #721 settles that a `seg:` record is weighed
     at V/2, so a record seeded just under the full `MAX_VALUE_BYTES` would already be above
     the write ceiling and would test refusal of an inadmissible record instead of a crossing.
     Passes pre-fix (refused for the other reason) — not C4-verify evidence. Its named
     negation is **deletion of the ceiling guard**, run against this custodian test; it is NOT
     one of the two `require` deletions, which belong to the DST property below.
  4. **RED pre-fix, but for the WRONG reason — a lost CAS writes no metadata and retracts
     nothing.** A competing writer takes the record between the pass's resolve and its commit.
     Assert the `seg:` record holds exactly the competing writer's bytes, no orphan mark for
     this move was published, and the drain is not certified — and that the already-copied
     destination fragment is **left in place, not deleted** (retracting a published write is
     the rule #638 rejected 4×). Call it "left in place", **never "collectable garbage"**: an
     unreferenced, unmarked fragment is a permanent leak (getwyrd/wyrd#723), not garbage — see
     the stranding note in Out of scope, and do not let the two halves of this brief disagree.
     **Read its base behaviour honestly.** Pre-fix the segmented refusal produces no plan at
     all (`rebalance.rs:346-355`), so the work loop never calls `evacuate_chunk` (`:158-160`)
     and the injected racer never fires: the leg fails on the base for the *ordinary* reason —
     no evacuation happened — not because it caught a CAS race. So it is **not** evidence of
     the conflict property, and the base shape is **three** failing legs (1, 2, 4), not two.
     Do not claim a two-failure base. If a fixture seam can be found that lands the competing
     write pre-fix *and* still proves a lost CAS post-fix, say so in `build-notes.md`;
     otherwise record that leg 4's conflict property rests on the mutation oracle and the DST
     property, exactly as #721's legs 3-4 do.

  Legs **1 and 2 are the discriminating evidence.** **Additionally**, the DST
  **repoint-versus-supersede** property ships in the **existing**
  `crates/dst/tests/custodian.rs` (a *new* `crates/dst/tests/*.rs` would be `#![cfg(madsim)]`
  and, as an added test file, would become the C4-verify discriminator and compile to nothing
  under the gate's invocation — `run-verify.sh:_crate_cfgs`, `:104-121`). Across the seed
  sweep, with a **distinct declared outcome per interleaving** — the first draft said "commits
  nothing" for both, which is incoherent: a later write cannot retroactively unmake an earlier
  successful conditional commit. **(i) The racing write lands FIRST:** the repoint's `require`
  loses at commit, so it writes **nothing** — neither the placement nor any orphan mark — and
  the object still names its pre-move fragment, which exists. **(ii) The repoint's CAS lands
  FIRST:** it commits in full — placement *and* orphan evidence together, never one without
  the other — and the racing supersede then either loses its own CAS or targets a different
  key. The property asserted across both is **all-or-nothing per attempt**, plus the standing
  invariant that the object is never left naming a fragment that does not exist. **Scope this property honestly in its own doc comment:** it
  proves the two CAS **preconditions**, and `build-notes.md` must record the named negations
  (deleting either `require` turns it red at `MADSIM_TEST_NUM=50`). It does **not** reach the
  read→prepare window — the racing batch is applied inside the repoint's own `commit()`, i.e.
  strictly after the primitive's own read — and must not claim to; that window is covered by
  #721's deterministic legs. C4-ci runs this property; it is **not** the C4-verify
  discriminator.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2; the base this child builds
  on is `main` **after #721 has merged** — see Ordering note.)
- Scope: **the drain pass completing the placement move for a `seg:`-resident fragment.**
  One logical fix — the second defect the first draft carried has been withdrawn, see Out of
  scope.
  - `crates/custodian/src/rebalance.rs` — the evacuation pass stops refusing a `seg:`-resident
    chunk (#696's placeholder at `:352-355`) and completes the move through **#721's**
    primitive, exactly as #721 converted the repair caller. The placement change and the
    orphan evidence for each vacated position stay **one batch** (`0005:298-299`, ADR-0015).
    Route the ceiling refusal through the primitive (which weighs a `seg:` record at **V/2**
    and a flat record at `MAX_VALUE_BYTES` — #721 settles that); do not keep a second copy of
    the check at `:522-528`, and do not introduce a second ceiling value. **Leave
    `plan_evacuations`' per-(object, chunk) planning shape alone** beyond what removing the
    refusal requires.
  - `crates/custodian/tests/segmented_map_rebalance.rs` — **that file's own** second leg
    (`an_owed_segmented_evacuation_is_refused_once_and_mutates_nothing`, `:328` — not to be
    confused with the success criterion's leg 2 above) asserts the
    refusal this child removes and MUST be rewritten to assert the evacuation now lands. This
    is a **forced** edit, budgeted for, not drift.
  - `crates/dst/tests/custodian.rs` — the repoint-versus-supersede property, added to the
    **existing** file, scoped as the success criterion states.
  - **Constraints carried forward:** **bounded memory** — the pass already deep-copies
    `prior_chunks` into every plan (`rebalance.rs:369`, `O(chunks)` per plan); do not make it
    worse by deep-copying a segmented root into every plan (`O(chunks × segments)`), and do
    not retain the namespace's decoded chunks. **Do not rebuild the cross-object
    claim-counting apparatus dropped at #651's replan** — leg 2 is a narrow per-pass rule, not
    a reference counter. **A losing CAS does not retract already-published bytes** (#638,
    `results/issue_638/review-rejected.md:15-16`); the refusal and conflict
    paths write **no METADATA at all** — the destination fragment written ahead of the
    commit stays where it is (see the stranding note in Out of scope).
  - **Budget:** ≤ **4** files — `custodian/src/rebalance.rs`,
    `custodian/tests/segmented_map_evacuate.rs` (**new**),
    `custodian/tests/segmented_map_rebalance.rs`, `dst/tests/custodian.rs` — ≤ **200** added
    **semantic** lines of non-test code (non-blank, non-comment, and excluding both `tests/`
    files and `#[cfg(test)]` modules), and `patch.diff` ≤ **95 KB**. A fifth file means the
    shape is wrong; in particular needing to edit `crates/core/src/metadata.rs`,
    `reconstruction.rs`, `backfill.rs`, `restore.rs`, `gc.rs` or `desired_state.rs` means the
    scope has drifted: **STOP and hand back a proposed split.** Needing to change the
    primitive specifically is the one signal worth reporting rather than working around —
    #721 built it for two callers, and if the second does not fit, say so.
    **On the sizer's `oversized` verdict:** this child trips it, and at Plan the reasons were `brief ~26 KB (cutoff 12 KB)` plus difficulty — i.e. it sizes this brief's PROSE, long because it carries two plan-review rounds' corrections, not the slice. The caps just above are the slice, against the parent's actual 7 files / 1595 added lines / 124 KB. `sizing.py:263-267` puts that predictor at 55% precision and reports it separately for exactly this reason. Judged a FALSE POSITIVE at Plan (2026-08-10), after the split that produced this child; do **not** re-split on it without new evidence from the diff itself.
  - **WITHDRAWN at Plan — cross-object `ChunkId` dedup. Do not implement it, do not
    re-litigate it.** The parent's T4 review filed a T3 finding that `plan_evacuations`
    (`rebalance.rs:264`, `:318`, `:367-376`) emits one plan per *(object, chunk)* with no
    cross-object dedup, so two objects naming one `ChunkId` drive two moves and two orphan
    puts. Three independent reviews now say that is not a defect to fix here. (a) It is not a
    **safety** failure: GC never reclaims a fragment any committed map names
    (`crates/custodian/src/gc.rs:143-146`, `:185-193`), so a duplicate orphan mark on a
    still-referenced position is inert — the parent's own adversary reached this conclusion
    independently and recorded it as a refutation that did not land. (b) The obvious
    "fix" is **backwards**: reconstruction's dedup is first-committed-reference-wins
    (`reconstruction.rs:524-529`), and applying it here would leave the second object naming
    the drained server — failing leg 2. (c) It is an **independent change**: the same
    per-object planning already applies to flat maps, so it is not forced by adding `seg:`
    repointing, and folding it in would make this slice two fixes again — the exact failure
    that split the parent. If the duplicate *work* (not risk) is worth removing, it is its own
    tracker item against `rebalance.rs`, on flat and segmented alike.
  - **Out of scope:** the placement primitive itself and its in-crate unit tests (**#721** —
    consume them, do not author them, do not edit `crates/core/src/metadata.rs` at all); the
    repair caller and its tests (**#721**). **The committer, the destination pre-mark, the
    drain fence, rollback and resume (#653)** — proposal 0016's full precondition set
    (`0016:669`) also requires `require(orphan:<P_new> == prior)` and
    `require_absent(desired:dserver:<S_new>)`, and this child, like #721, ships **only** the
    two record preconditions. That is the parent issue's carve-out and a **pre-declared
    sign-off item, and the parent brief got its justification WRONG**: without the pre-mark, a
    move that loses its CAS leaves the copied destination fragment **unreferenced and
    unmarked**, and GC does **not** collect such a fragment — it reclaims only on an orphan
    mark past its grace or an expired pending lease, and otherwise *conservatively keeps* it
    (`crates/custodian/src/gc.rs:196-212`). So it is a **permanent leak, not "collectable
    garbage"**, and the in-tree comment saying otherwise
    (`crates/custodian/src/rebalance.rs:530-531`) is inaccurate. 0016 answers it with the
    pre-mark (X47, `0016:2577`), which is #653's. What stays true is the comparative claim: the
    **flat** drain path behaves identically today, so **no new stranding class is introduced** —
    an existing one is extended to a second record shape. **DECIDED AT PLAN, 2026-08-10** — the
    pre-existing leak is filed as **getwyrd/wyrd#723** (milestone *Foundations*), which owns the
    flat path's leak, the 0016 X47 pre-mark that closes it, and the inaccurate "collectable
    garbage" comments. This child ships the extension **as is**, inheriting a tracked defect
    rather than creating an untracked one; that is what settles the C-1 tension. Do **not**
    implement the pre-mark here, do **not** re-argue it, and do **not** edit the garbage comment
    at `rebalance.rs:530-531` — #723 owns it. (Separately and unaffected:
    GC's committed-reference set is a hard safety gate, `crates/custodian/src/gc.rs:143-147`,
    `:190-193` — that is what makes a *referenced* position safe, and it is why the withdrawn
    dedup was not a safety fix.) Also out: `backfill.rs` (#695, merged), `gc.rs` / `scrub.rs` (#650,
    merged), restore and `desired_state` (#651, merged). The read side generally: no new
    resolving walk, no change to `resolve_chunk_map`, no change to the containment rule
    #695/#696/#697 landed. Any new or edited ADR / spec / proposal (0016 is a **draft** and
    stays untouched); any conformance-vector change; any new dependency.
  - **KEEP THE DISCRIMINATOR ASSERTION-RED — HARD CONSTRAINT.** The new test MUST NOT name any
    symbol **this** patch introduces: the RED leg reverts the production files in *this*
    patch (`run-verify.sh:469-476`), so such a reference makes the target fail to **compile**
    and the gate reports UNVERIFIABLE (exit 77, `:492-500`) rather than a red. Note the exact
    scope — this patch does not touch `crates/core/src/metadata.rs`, so **#721's primitive is
    base-visible here and reverting this patch does not remove it**; naming it would not break
    the red leg. Do not name it anyway: drive everything through `reconcile_step` and observe
    the **store**, so the leg measures the drain pass's behaviour rather than unit-testing
    another slice's function. `MAX_VALUE_BYTES` is likewise base-visible and may be named.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.6% — 80 of 82 instrumentable changed lines executed (floor 80%); 82 of 213 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 18 mutants tested in 4m: 15 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_722/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.24s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #722: make D-server drains converge for segmented objects; the implementation checks pass, with the segment-ceiling contract requiring human resolution.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether to adopt the inherited 100,000-byte maintenance ceiling or re-enter Plan for the specified 50,000-byte ceiling — a 49,999-byte fixture moves where the brief requires refusal (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:548`; `reviewer-half-ceiling-contract.log:23`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production fix reproduces both binding evacuation failures as assertions; the expanded suite has 1 pass and 11 failures, not a compilation failure (`reviewer-red.log:23`; `reviewer-red.log:79`; `reviewer-red.log:121`). |
| C3 Change | PASS | The patch stays within the approved composition boundary: four named files, 90,650 bytes, and at most 105 added semantic production lines; the shared primitive remains outside this patch (`reviewer-shape.log:1`; `crates/custodian/src/rebalance.rs:590`). |
| C4 Verification (red→green) | PASS | Restoring the fix makes all 12 discriminator tests pass; 303 custodian tests and the clean-cache 50-seed DST campaign pass, while the full frozen CI log supplies the sandbox-blocked advisory scan (`reviewer-custodian-green.log:306`; `reviewer-dst-clean.log:546`; `gate-logs/C4-ci.log:3403`); this does not discharge C1's ceiling mismatch. |
| C5 Causal adequacy | PASS | Both segmented references converge, and independently deleting either CAS precondition makes the race property assertion-red; the permanent refusal is removed without a capability-probe fallback (`crates/custodian/tests/segmented_map_evacuate.rs:479`; `crates/dst/tests/custodian.rs:6163`; `reviewer-negations.log:3`). |
| T1 Structure | PASS | Placement and orphan evidence retain one atomic commit through the shared trait-based primitive, matching the established maintenance boundary (`crates/custodian/src/rebalance.rs:590`; `crates/custodian/src/rebalance.rs:640`; `crates/core/src/metadata.rs:3126`). |
| T2 Shape | PASS | The second caller reuses the existing placement operation and keeps per-object/chunk planning; the forced legacy-test rewrite and new DST property remain within the four-file budget (`crates/custodian/src/rebalance.rs:304`; `crates/custodian/tests/segmented_map_rebalance.rs:333`; `reviewer-shape.log:1`). |
| T3 Runtime | PASS | One retained root per affected object avoids multiplying segmented roots by evacuation count; plans retain only owed chunk references, and prepare-time refusals precede fragment writes (`crates/custodian/src/rebalance.rs:101`; `crates/custodian/src/rebalance.rs:436`; `crates/custodian/src/rebalance.rs:599`). |
| T4 Contribution | PASS | Live GitHub checks cover merged history by all four paths and file lists of all 19 closed/unmerged PRs; the overlapping ancestor was rejected for size, which this slice respects (`reviewer-prior-art-summary.log:1`; `reviewer-prior-art-summary.log:11`); contribution artifacts are N/A until publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Observable regressions now distinguish stale generations, corrupt records, and store faults, and prove successor-generation progress; the carried-forward implementation concerns are covered (`crates/custodian/tests/segmented_map_evacuate.rs:661`; `crates/custodian/tests/segmented_map_evacuate.rs:719`; `crates/custodian/tests/segmented_map_evacuate.rs:882`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that demonstrated committed-object evacuation and drain-status convergence satisfy the intended decommission workflow — automated correctness evidence supports the slice, while operational fitness remains a maintainer decision (`crates/custodian/tests/segmented_map_evacuate.rs:464`; `crates/custodian/tests/segmented_map_evacuate.rs:505`). |

The ceiling decision is the only unresolved specification issue found. The brief explicitly requires a segment crossing V/2 to be refused (`brief.md:39`), but the supplied base explicitly reserves V/2 for root publication and permits segment maintenance through the full value ceiling (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3218`). The new test deliberately follows that inherited contract (`crates/custodian/tests/segmented_map_evacuate.rs:548`). In an isolated scratch copy, changing only its first fixture to `MAX_VALUE_BYTES / 2 - 1` produces `Changed` instead of the required `Blocked` (`reviewer-half-ceiling-contract.log:1`, `reviewer-half-ceiling-contract.log:25`). This is an exercised contract disagreement, not an absent-tool or stale-target failure. Resolving it may require changing the brief or the prerequisite; adding a second caller-local ceiling would violate the declared composition boundary.

The independent evidence supports the implemented behavior. I stashed only `rebalance.rs`, retained the added discriminator, ran the red leg, restored the stash, and ran the complete custodian suite: 303 passed, none failed, one privileged Tier-1 test ignored. The required two binding legs failed on the base; the lost-CAS leg was also red because its racer never fired, so that base failure is not credited as proof of conflict handling. Separately, removing the actual ceiling check made leg 3 assertion-red; deleting the root or segment `require` separately made the DST property assertion-red with `MADSIM_TEST_NUM=50` (`reviewer-negations.log:2`, `reviewer-root-require-deleted.log:29`, `reviewer-segment-require-deleted.log:17`). Those mutations were confined to a scratch copy and restored.

The final DST result comes from an isolated clean build cache: `cargo xtask dst` exited zero, including all 32 custodian properties (`reviewer-dst-clean.log:546`). An earlier run reused the build directory used by the mutation copy and reproduced the deleted-segment-pin failure; `reviewer-remaining-checks.log:314` is discarded as contaminated evidence, not attributed to the patch. The independent initial red/green and custodian runs preceded those mutation probes. The supplied target still reverse-checks against the complete original patch, and no reviewer stash remains.

All frozen gate logs were readable and adjudicated individually:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS | Frozen output records actual typos/docs/fmt/clippy/build/tests, dependency scans, conformance, statics, and DST success (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3403`, `gate-logs/C4-ci.log:4007`). |
| C4-verify | PASS | Frozen 12-green / 1-pass-11-fail red result matches the independent rerun (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:132`). |
| C4-diff-cov | PASS | Frozen measurement is 80/82 instrumentable changed lines, 97.6%; 131 unscored lines are not claimed covered (`gate-logs/C4-diff-cov.log:198`, `gate-logs/C4-diff-cov.log:213`). |
| C5-mutants | PASS | Frozen scanner reports 18 tested, 15 caught, 3 unviable; the named ceiling/CAS negations above were independently executed (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | PASS | Frozen review summary reports zero blocking findings; no unavailable detailed review is assumed to have been inspected (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | The PR description is absent by design; the substantive artifact audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen output shows both requested feature compilations finishing successfully (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). |

The local full-CI attempt passed through workspace tests and cargo-machete, then cargo-deny could not lock its read-only advisory database (`reviewer-ci.log:3342`). This is a reviewer-host caveat, not a patch defect: the frozen gate shows that same scan executing successfully (`gate-logs/C4-ci.log:3403`). Conformance and the statics scanner were independently rerun successfully (`reviewer-remaining-checks.log:1`), and DST was completed separately as recorded above. The brief's listed external tools were exercised by the frozen gates; none of these verdicts substitutes code-reading or a shim for compilation. Privileged disk-fault and deployed kill/reconstruct campaigns were not rerun; their scheduled observation remains appropriate for this custodian durability surface (`AGENTS.md:78`).

Prior art is mechanically checked, not left pending because the target has synthetic history. API queries returned 13, 0, 2, and 30 merged-main commits for the four affected paths respectively, and file-path intersection over every closed/unmerged PR found only #647 and #336 (`reviewer-prior-art-summary.log:7`). The maintainer explicitly closed [PR #647 for size](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571); #336's overlapping DST work concerns existing crash/reader properties, not this drain race. Local rejected-ancestor information is limited to the supplied brief (`brief.md:300`). The tracked #698/#699/#723 deferrals remain settled and are not re-raised.

Source citations above refer to the supplied `$PDCA_TARGET`; brief and evidence citations refer to this review directory. No builder notes or unrelated checkout were read, and no proposed fix was made to the target.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #722 (segmented drain evacuation), iteration 4

**Verdict: I could not refute the fix.** I re-ran the red→green proof, re-ran every named negation the brief requires, and tried five input shapes aimed at the new addressing and containment code. All of them held. One item needs a human decision: the brief and the base disagree about which value ceiling applies to a `seg:` record.

## The evidence holds up

- **Red→green reproduced in a scratch copy, not taken from the log.** Patched tree: `segmented_map_evacuate` 12/12 green, `segmented_map_rebalance` 5/5 green. With only `crates/custodian/src/rebalance.rs` reverted: 11 of 12 red. Leg 1 (`crates/custodian/tests/segmented_map_evacuate.rs:434`) and leg 2 (`:492`) fail on `Blocked` vs `Changed`, which is the right reason: the base refuses the segmented object. Every leg goes through the real `reconcile_step` and checks the store afterwards, so no leg re-implements production logic or passes by tautology. Leg 3 is the one that passes on the base, which the brief predicted.
- **The DST named negations are real** (run with `--cfg madsim`, `MADSIM_TEST_NUM=50`). Deleting the `seg:` precondition at `crates/core/src/metadata.rs:3324` makes `drain_repoint_versus_supersede_is_all_or_nothing` fail at `crates/dst/tests/custodian.rs:6167`. Deleting the root precondition at `crates/core/src/metadata.rs:3257` makes it fail at `:6163`. Both mutations were reverted afterwards.
- **The custodian named negations are real too.** Removing the ceiling check in `weighed` (`crates/core/src/metadata.rs:3339-3346`) turns legs 3 and 5 red (`segmented_map_evacuate.rs:563`, `:683`). Removing the `seg:` precondition turns leg 4 red. Iteration 2's hand mutation (pin the scan's snapshot instead of the resolved record, at `crates/custodian/src/rebalance.rs:439`) now turns leg 12 red, so that round's gap is closed.

## Attempts to break the fix (none landed)

- **Resolve restarts onto a FLAT successor** (`rebalance.rs:364-367`, `:436-442`). The scan reads a segmented root, then a flat root naming the drained chunk replaces it mid-resolve. Result: `Changed`, the flat root is repointed, and the drain status is `Satisfied`. The plan's offset and the pinned root both come from the successor.
- **One chunk named by a flat object and a segmented object.** Both references are repointed in one pass (`Changed`, `Satisfied`). Copying the same fragment to the same server twice is safe in production as well as in the test double: `FsChunkStore` treats a same-id rewrite as a no-op (`crates/chunkstore-fs/src/lib.rs:307`).
- **Two drained chunks in one `seg:` record.** Both move in one pass. The second move re-reads the record, sees the first move's write, and commits on top of it.
- **A flat object owing two moves.** The passes answer `Blocked`, `Changed`, `Satisfied`. The second plan copies its fragment and then loses the root CAS (compare-and-swap) in pass 1. The **base gives exactly the same sequence**, so this is the existing #723 leak class, not something this patch adds.
- **Over-broad containment.** The non-canonical-key guard and the overflow guard (`rebalance.rs:413-424`) only run after `evac.is_empty()` has already skipped the chunk (`:410`). So a healthy object, or an oddly keyed one, that owes nothing on the draining server never blocks certification.
- **The removed `refused-segmented` event and `rebalance_refused_records` counter** (deleted at old `rebalance.rs:554-563`). Nothing in `docs/` or the rest of the tree names either, so no docs page or alert goes stale.

## Items for the human

- NEEDS-HUMAN [human] — **Leg 3 ignores an explicit brief instruction, and the base shows the brief was wrong.** The brief says to key the ceiling fixture "to **V/2, not `MAX_VALUE_BYTES`** — #721 settles that a `seg:` record is weighed at V/2" (`brief.md:40-45`, repeated at `:142`). The merged primitive does the opposite: it weighs a `seg:` record against the full `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:593-600`, `:3339-3346`) and keeps the V/2 budget (`MAX_ROOT_VALUE_BYTES`, `:574`) for root tables only. So a fixture at V/2 would commit, and the builder keyed leg 3 to `MAX_VALUE_BYTES`, saying so in the test's doc comment (`segmented_map_evacuate.rs:548-551`). The builder was right to follow the base, and full-V is consistent with the resolver reading `seg:` rows up to `MAX_VALUE_BYTES`. But the brief's author expected V/2. Someone needs to confirm that #721's full-ceiling rule is the intended contract for `seg:` records. If V/2 was meant, the fix belongs in #721's primitive, not in this slice.

## Notes (no action needed in this slice)

- **The C4-diff-cov MISS lines are a tooling artifact.** `rebalance.rs:329` and `:356` (the scan-time `contain` calls) show as unexecuted only because the coverage run executed `segmented_map_evacuate` and the DST crate alone (`gate-logs/C4-diff-cov.log:25-192`). `segmented_map_rebalance.rs:393-417` covers both lines, and C4-ci runs it.
- **Stale doc line outside this slice.** `crates/core/src/metadata.rs:3238` still says "The custodian's reconstruction pass is the caller". After this patch the drain is a second caller. `metadata.rs` belongs to #721 and the brief's four-file budget rules out editing it here, so this is a follow-up, not a defect. In the same way, `docs/design/architecture/06-runtime-view.md:41` describes Rebalance in one generic line, while `:40` documents the reconstruction move in detail. The rubric's docs-currency rule (ports, API operations, RPCs, CLI flags, persisted fields) is not triggered.
- **I can't check the T4 row.** `check-gates.json` reports `T4-batch-review` as 0 blocking after 29.9 s for "3x codex". Its frozen log is a single summary line pointing at `review-batch.md`, which I wasn't given. My own pass found nothing that contradicts a clean review.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed all four changed files against `$PDCA_TARGET`, including evacuation planning (`crates/custodian/src/rebalance.rs:436`), the shared repoint and atomic orphan batch (`crates/custodian/src/rebalance.rs:590`), and the race assertions (`crates/dst/tests/custodian.rs:6132`). Validation used the frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C1 Spec — Decide whether to adopt the inherited 100,000-byte maintenance ceiling or re-enter Plan for the specified 50,000-byte ceiling — a 49,999-byte fixture moves where the brief requires refusal (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:548`; `reviewer-half-ceiling-contract.log:23`).
- [x] Validation — fitness-to-purpose — Confirm that demonstrated committed-object evacuation and drain-status convergence satisfy the intended decommission workflow — automated correctness evidence supports the slice, while operational fitness remains a maintainer decision (`crates/custodian/tests/segmented_map_evacuate.rs:464`; `crates/custodian/tests/segmented_map_evacuate.rs:505`).
- [x] **Leg 3 ignores an explicit brief instruction, and the base shows the brief was wrong.** The brief says to key the ceiling fixture "to **V/2, not `MAX_VALUE_BYTES`** — #721 settles that a `seg:` record is weighed at V/2" (`brief.md:40-45`, repeated at `:142`). The merged primitive does the opposite: it weighs a `seg:` record against the full `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:593-600`, `:3339-3346`) and keeps the V/2 budget (`MAX_ROOT_VALUE_BYTES`, `:574`) for root tables only. So a fixture at V/2 would commit, and the builder keyed leg 3 to `MAX_VALUE_BYTES`, saying so in the test's doc comment (`segmented_map_evacuate.rs:548-551`). The builder was right to follow the base, and full-V is consistent with the resolver reading `seg:` rows up to `MAX_VALUE_BYTES`. But the brief's author expected V/2. Someone needs to confirm that #721's full-ceiling rule is the intended contract for `seg:` records. If V/2 was meant, the fix belongs in #721's primitive, not in this slice.
- [x] The separate shared-`ChunkId` problem is not established as a defect. The target scans every inode and pushes a plan for every matching reference (`crates/custodian/src/rebalance.rs:264`, `:318`, `:367-376`); each plan conditionally updates its own record and publishes an orphan marker (`:538-556`), while GC explicitly never reclaims a fragment named by any committed map (`crates/custodian/src/gc.rs:143-146`, `:185-193`). That evidence supports the segmented refusal at `rebalance.rs:346-355`, but contradicts the brief's implication that two moves/orphan puts are themselves a safety failure (`brief.md:10-14`, `:81-91`). State the observable failure caused by duplicate planning, or remove this second defect.
- [x] Success-criterion leg 2 does not discriminate the requested dedup. Its declared pre-fix red is only the segmented refusal (`brief.md:30-36`); after fixing that refusal, the existing one-plan-per-object shape at `crates/custodian/src/rebalance.rs:264-376` can move both references and satisfy every final-state assertion while still doing the two moves the brief calls defective. Conversely, reconstruction's first-reference-wins dedup (`crates/custodian/src/reconstruction.rs:375`, `:524-529`) would leave the second object naming the draining server and violate the same criterion. Add a red→green observation specific to the alleged duplicate-work failure and define how all references are repointed, or drop the dedup requirement.
- [x] The scope contains an independent second change: the brief itself introduces cross-object dedup with “Separately” (`brief.md:10-14`) and mandates it at `brief.md:115-124`, but the target's same per-object planning already applies to flat maps (`crates/custodian/src/rebalance.rs:314-376`), so it is not forced by adding `seg:` repointing. Split it or supply a causal reason it must land atomically with the segmented-drain fix.
- [x] The DST criterion assigns an incoherent outcome to one interleaving: it says the repoint commits nothing when the repoint commits both before and after the racing write (`brief.md:52-65`). A later racing write cannot retroactively make an earlier successful conditional commit write nothing; the target's existing CAS reports `Conflict` only when its `require` loses at commit (`crates/custodian/src/rebalance.rs:538-556`). Specify separate observable outcomes for race-before-repoint and repoint-before-race.
- [x] The tracker claims cannot be checked because this review bundle contains neither `notes.json` nor `sources/`, yet the shared-chunk addition is justified only as a parent-review “T3 finding” (`brief.md:36-40`) and several constraints rely on unavailable rejected-work records (`brief.md:230-250`). Restore the tracker artifacts or quote the load-bearing tracker comments in the brief before retaining those requirements.
- [x] The declared implementation base is not presently resolvable: `dependency-state.json:2-5` says #721 exists but is still `PLANNED`, and the target still contains the old flat-only reconstruction commit (`crates/custodian/src/reconstruction.rs:894-949`) rather than the promised `repoint_chunk` primitive. This matches the brief's admission that the patch cannot compile until #721 merges (`brief.md:67-80`, `:92-106`); refresh `$PDCA_TARGET` to a merged #721 base before Do, or declare a concrete stacked base that contains the primitive.
- [x] C1 Spec — Reconcile the brief's 50,000-byte segment limit with the inherited 100,000-byte contract—an independently exercised 49,999→50,018-byte move commits, while changing the primitive is excluded from this slice (`brief.md:39`, `brief.md:168`, `crates/core/src/metadata.rs:593`, `crates/custodian/tests/segmented_map_evacuate.rs:444`).
- [x] `crates/custodian/tests/segmented_map_evacuate.rs:444`: The new ceiling test seeds at `MAX_VALUE_BYTES` (100,000), whereas brief leg 3 explicitly requires a crossing of V/2 (50,000). This test cannot establish that requirement. The target's shared primitive explicitly uses the full ceiling for segment records (`crates/core/src/metadata.rs:593`), so this is a brief/dependency contract mismatch, not a request for a duplicate caller-side guard. Decide whether to update the acceptance boundary to the landed primitive's contract or split out a prerequisite primitive change; that file is expressly outside this patch's scope.
- [x] C1 Spec — Resolve the brief’s 50,000-byte segment bound against the inherited primitive’s 100,000-byte bound — a move crossing 50,000 succeeds, so criterion 3 is unmet as written (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:520`; `reviewer-half-ceiling.log:15`).
- [x] `crates/custodian/tests/segmented_map_evacuate.rs:528`: **The new ceiling regression does not exercise the brief's required V/2 refusal.** It seeds a 100,000-byte segment and tests growth beyond that full ceiling. A 49,999-byte segment growing by the same 19 bytes would instead commit, contrary to success criterion 3. This matches the target primitive's intentional full-ceiling contract (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3340`), so the passing test cannot settle the discrepancy. Decide whether to revise the brief to that contract or split out a prerequisite primitive change; the brief forbids changing the primitive or duplicating its ceiling check in this slice.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed
- [x] C1 Spec — Decide whether to adopt the prerequisite's 100,000-byte segment ceiling or re-enter Plan for the brief's 50,000-byte ceiling: an independent 49,999→50,018-byte move succeeds and certifies the drain, contrary to the brief (`brief.md:39`; `crates/core/src/metadata.rs:3218`; `crates/custodian/tests/segmented_map_evacuate.rs:543`; `reviewer-half-ceiling.log:9`).

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
- By / date: Eduard Ralph / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up issue (issue_722 sign-off): the shared repoint_chunk primitive should weigh a seg: record at V/2 (MAX_ROOT_VALUE_BYTES, 50,000 — headroom for future development), not MAX_VALUE_BYTES; rewrite its doc comment (metadata.rs:593-600), re-key segmented_map_evacuate.rs leg 3 to just under V/2, and check for stored seg: records between V/2 and V.
