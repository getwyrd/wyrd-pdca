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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The segmented-fragment drain fix restores progress in independent tests; the segment-size contract, prior-art evidence, and operational fitness still need human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether to adopt the prerequisite's 100,000-byte segment ceiling or re-enter Plan for the brief's 50,000-byte ceiling: an independent 49,999→50,018-byte move succeeds and certifies the drain, contrary to the brief (`brief.md:39`; `crates/core/src/metadata.rs:3218`; `crates/custodian/tests/segmented_map_evacuate.rs:543`; `reviewer-half-ceiling.log:9`). |
| C2 Reproduction (red pre-fix) | PASS | Both binding evacuation cases fail by assertion with production stashed; 12 tests execute, 11 fail, and the ceiling leg passes; the CAS leg fails because its racer never fires, not because it detects a conflict (`reviewer-red.log:163`; `reviewer-red.log:191`; `reviewer-red.log:222`; `reviewer-red.log:260`). |
| C3 Change | PASS | A drain can finish without separating placement publication from reclamation evidence, and the change stays within the four authorized files (`crates/custodian/src/rebalance.rs:590`; `crates/custodian/src/rebalance.rs:640`; `patch.diff:1594`). |
| C4 Verification (red→green) | PASS | Restoring production makes all 12 regression tests pass; all 303 custodian tests and the 50-seed drain property pass independently, verifying the implemented contract subject to C1 (`reviewer-green.log:19`; `reviewer-custodian.log`; `reviewer-dst.log:245`; `gate-logs/C4-ci.log:4007`). |
| C5 Causal adequacy | PASS | Removing the segmented refusal restores actual moves and shared-reference convergence; independent negations of both CAS pins and the ceiling check fail at assertions, so the evidence tests publication safety as well as progress (`crates/custodian/src/rebalance.rs:590`; `reviewer-no-root-pin.log:18`; `reviewer-no-segment-pin.log:18`; `reviewer-no-ceiling.log:16`). |
| T1 Structure | PASS | A shared write boundary keeps maintenance callers consistent, and per-object root snapshots avoid multiplying retained metadata by the number of evacuation plans (`crates/custodian/src/rebalance.rs:364`; `crates/custodian/src/rebalance.rs:436`; `crates/custodian/src/rebalance.rs:590`). |
| T2 Shape | PASS | Four affected files, 105 added nonblank/noncomment production lines, and a 90,650-byte diff fit the explicit caps; the rewritten older test and existing DST file cover the forced companion changes (`brief.md:163`; `patch.diff:1`; `patch.diff:580`; `patch.diff:1495`; `patch.diff:1594`). |
| T3 Runtime | PASS | Tests demonstrate convergence, surviving-fragment availability, refusal before copying, and containment without stopping healthy work; orphan timestamps still use the reconciliation clock (`crates/custodian/tests/segmented_map_evacuate.rs:493`; `crates/custodian/tests/segmented_map_evacuate.rs:816`; `crates/custodian/src/rebalance.rs:643`; `reviewer-green.log:19`). |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art for all four affected paths: this artifact contains one synthetic commit and no remote, so its history cannot settle duplication or rejected direction; the contribution-artifact audit itself is deferred/N/A until publish (`reviewer-prior-art.log:1`; `brief.md:291`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Operators receive once-per-object corruption signals while retired generations remain retryable; the successor-generation test distinguishes actual progress from a needless lost CAS; the tracked #698/#699/#723 deferrals remain settled (`crates/custodian/src/rebalance.rs:257`; `crates/custodian/src/rebalance.rs:605`; `crates/custodian/tests/segmented_map_evacuate.rs:882`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the tested behavior for the intended decommission workflow and clear applicable INTEGRATION §4 human-only items: simulated drain convergence is demonstrated, but operational acceptance belongs to the maintainer and that human-only inventory is absent from this bundle (`crates/custodian/tests/segmented_map_evacuate.rs:464`; `reviewer-prior-art.log:11`). |

No additional builder-fixable defect was established. The unresolved contract is concrete: the brief demands refusal at V/2, while the current prerequisite explicitly permits a segment up to V. The unmodified test seeds 100,000 bytes and checks refusal after growth; an independent variant seeded 49,999 bytes, copied the fragment, committed 50,018 bytes, and returned `Changed` with status `Satisfied` (`reviewer-half-ceiling.log:8`). Adopting V requires reconciling the brief; retaining V/2 requires a prerequisite/scope decision because this slice explicitly excludes changes to the primitive (`brief.md:168`). This is not a stale-target compile failure or a missing-tool caveat.

The independent evidence distinguishes regression failures from race proofs. Production alone was stashed, leaving the new test available, then restored with `git stash pop`; the base compiled and failed by assertions or the existing version overflow. The restored suite passed 303 tests, with one pre-existing ignored test (`reviewer-custodian.log:491`). The drain DST property ran with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50`. Its race happens inside `commit`, so it proves the two commit preconditions; the deterministic drain legs separately exercise prepare-time outcomes (`crates/dst/tests/custodian.rs:5921`). The test topology can exhibit the forbidden failure, as the base rerun demonstrates; it is not an unmet external-service dependency.

| Independent negation | Observed consequence |
|----------------------|----------------------|
| Remove the root precondition | DST detects a losing move rewriting the retired segment (`reviewer-no-root-pin.log:18`). |
| Remove the segment precondition | DST detects orphan evidence from the losing move; the deterministic CAS leg also returns `Changed` incorrectly (`reviewer-no-segment-pin.log:18`; `reviewer-no-segment-pin-cas.log`). |
| Remove the ceiling check | The refusal test observes an incorrectly certified move (`reviewer-no-ceiling.log:16`). |
| Pin the scanned root instead of the resolved root | The successor-generation test observes `Blocked` instead of `Changed` (`reviewer-scanned-generation.log:15`). |
| Treat every planned root as current | The stale-ceiling test detects an incorrect permanent-refusal signal (`reviewer-no-freshness.log:15`). |

The available validators were rerun: formatting, typos, documentation lint, unused-dependency scanning, and custodian Clippy all passed (`reviewer-checks.log:1`; `reviewer-checks.log:129`). The independent full diff scan initially reported 10 caught and eight unviable; five unviable mutations were warning-as-error rejections. Re-running the affected subset with `--cap-lints true` caught all 6 tested mutations (including one already caught), establishing the same combined result as the frozen gate: 15 caught, three structurally unviable, no survivors (`reviewer-mutants.log:3`; `reviewer-mutants-capped.log:3`). The two manual generation mutations were likewise rerun in warning-neutral forms and failed on the intended assertions.

| Frozen gate | Assessment | Evidence actually present |
|-------------|------------|---------------------------|
| C4-ci | PASS | Captured workspace checks, docs rendering, dependency audits, structural guards and DST finish successfully; broader checks not independently rerun are supported by this log (`gate-logs/C4-ci.log:16`; `gate-logs/C4-ci.log:3397`; `gate-logs/C4-ci.log:4007`). |
| C4-verify | PASS | 12 green, then 11 failures and one pass with production reverted; independently reproduced (`gate-logs/C4-verify.log:10`; `gate-logs/C4-verify.log:132`). |
| C4-diff-cov | PASS | 80/82 instrumentable changed lines covered (97.6%); 131 changed lines are unscored, not proven covered (`gate-logs/C4-diff-cov.log:198`; `gate-logs/C4-diff-cov.log:216`). |
| C5-mutants | PASS | Frozen scanner reports 15 caught, three unviable, no survivor among 18 mutants (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | PASS | Log reports zero blocking/rejected/noise findings; it does not supply merged/closed prior-art history (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | PR description is absent by design at Check; the substantive contribution audit reruns at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Actual TiKV adapter and server feature compilation completes; this is compile evidence, not a live TiKV exercise (`gate-logs/host-tikv.log:112`; `gate-logs/host-tikv.log:211`). |

All seven supplied gate logs were readable. The brief's named external tools have execution evidence; no undischarged tool/service dependency was found. The path-based prior-art investigation returned only the synthetic base for the three existing files and no history for the new file (`reviewer-prior-art.log:4`). The brief records useful historical leads, but the closed/rejected check cannot be independently completed from this artifact. Tier-1 disk-fault and Tier-2 kill/reconstruct runs are appropriate follow-up observations for this custodian change, not missing merge gates (`AGENTS.md:78`). No capability-probe/load-time symptom guard was introduced; the new checks classify real stale or corrupt metadata. Settled tracked deferrals were not reopened.

All experimental source changes were restored byte-for-byte; each of the four affected files matches its supplied patch postimage, the stash is empty, and the final regression and 50-seed DST reruns are green (`reviewer-integrity.log:1`; `reviewer-final-green.log`; `reviewer-final-dst.log`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #722 (a drain holding a `seg:`-resident fragment completes)

**Result: I could not refute the fix.** I re-ran the evidence, ran each named negation myself, and probed six edge cases against the patched pass. Nothing broke. No NEEDS-HUMAN items.

- **The red→green evidence holds.** Green reproduces locally (12/12 in `segmented_map_evacuate`, 5/5 in `segmented_map_rebalance`). In the frozen red log (`gate-logs/C4-verify.log`), legs 1 and 2 fail on the base for the defect itself: "the move landed: Blocked vs Changed", next to the base's `refused-segmented` line (`crates/custodian/tests/segmented_map_evacuate.rs:418`, `:479`). Every leg drives the real `reconcile_step` and checks the store, so no test runs against a copy of production logic. The test hooks are faithful too. The resolver reads `seg:` records only through `scan_page` (`crates/core/src/metadata.rs:2839-2854`), so a `seg:` `get` is always the move's own re-read (`metadata.rs:3288`). The hook at `segmented_map_evacuate.rs:115` therefore cannot fire during the scan and pass for the wrong reason.

- **The named negations hold. I ran each one.** Deleting the root-generation `require` (`metadata.rs:3257`) turns the DST (deterministic simulation) property red at `MADSIM_TEST_NUM=50`. Deleting the `seg:` `require` (`metadata.rs:3324`) turns the DST property red and custodian leg 4 red (`segmented_map_evacuate.rs:604`). Disabling the ceiling check in `weighed` (`metadata.rs:3340`) turns legs 3 and 5 red. C4-ci really does sweep 50 seeds (`xtask/src/main.rs:1716`, `:1750`), and its log shows `drain_repoint_versus_supersede_is_all_or_nothing ... ok`. The property's doc comment (`crates/dst/tests/custodian.rs:5921-5929`) correctly says it does not reach the read→prepare window.

- **Edge-case probes against `crates/custodian/src/rebalance.rs:364-452` and `:590-630` did not break it:**
  - two drained chunks in one `seg:` record: one pass, `Changed`, drain `Satisfied`;
  - the same `ChunkRef` twice in one `seg:` record: both positions moved in one pass;
  - one chunk shared by a flat object and a segmented object: both repointed;
  - a segmented root superseded mid-resolve by a *flat* generation: moved through the flat arm on the successor (version 7→8);
  - one object with two `seg:` records, the second torn under the move: the first move lands, the object is named once, `Blocked`;
  - a pre-M3 chunk with an empty placement inside a `seg:` record: `Changed`, and the equality pin matches what the resolver returned.

- **The shared-chunk case (brief leg 2) is safe even when only one of the two moves lands.** The first move's orphan mark then sits on a fragment the second object still names. GC checks references before it judges any mark, including a `Reclaiming` one (`crates/custodian/src/gc.rs:662-669`), and builds that reference set through the shared resolver, `seg:` records included (`gc.rs:1242`). The fragment is kept, so this is not a hole.

- **Note, no action: leg 3 is keyed to `MAX_VALUE_BYTES`, not the V/2 the brief asked for** (`brief.md:39-45`). The brief's premise was wrong. #721's primitive weighs a `seg:` record against the full ceiling and documents why (`metadata.rs:593-600`, `:3218-3219`). The builder followed the primitive and added no second ceiling, which is what the brief's scope line asks (`brief.md:142-144`). The test is right and the brief text is stale.

- **Note: C4-verify's "12 test(s) ran red" overstates the evidence.** Only legs 1 and 2 test the defect. Leg 10 (`segmented_map_evacuate.rs:816`) is red on the base because the base *did* drain that corrupt flat object (the red log shows an `evacuate` line for chunk `c4a1`). The patch now names the object and holds the drain until an operator repairs it. That is a deliberate change. It matches the reconstruction peer (`crates/custodian/src/reconstruction.rs:693-698`), and only a corrupt flat record (chunk lengths summing past `u64`) can reach it, so I don't count it as a refutation. It just shouldn't be counted as evidence of the defect either.

- **Note, pre-existing and not this diff:** a *flat* object owing two evacuations takes two passes (probe: `Blocked`, then `Changed`, then `Satisfied`). The second plan pins the generation the first move just replaced, so it loses its CAS (compare-and-swap) after copying. The base pinned the scanned record per plan in the same way, and that copy falls under the #723 leak class. Segmented objects don't have this problem because the move never rewrites their root.

- **Note, below the rubric's docs-currency bar:** `metadata.rs:3238` still says reconstruction is "the caller" of `repoint_chunk`, and `docs/design/architecture/08-crosscutting-concepts.md:89` names only reconstruction as a `seg:` writer. Both are now incomplete. The brief forbids editing `metadata.rs` and caps the patch at 4 files. The diff also changes no port, API operation, RPC, CLI flag or persisted field. So this is a follow-up, not a blocker.

Attempted to refute: the red→green proof, all three named negations, the hook fidelity, offset addressing over repeated, shared and identity-placed chunks, a mid-resolve supersede onto both shapes, partial failure within one object, and GC safety for a partly moved shared chunk. None of these attempts succeeded.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No new actionable findings in this diff on either advisory lens: introduced correctness defects or reuse, simplification, and efficiency.

Reviewed the shared object snapshots and checked offsets (`crates/custodian/src/rebalance.rs:374`), repoint outcome handling and atomic orphan publication (`crates/custodian/src/rebalance.rs:590`), deterministic regressions (`crates/custodian/tests/segmented_map_evacuate.rs:418`), and scoped CAS race assertions (`crates/dst/tests/custodian.rs:5977`). All source references were checked against `$PDCA_TARGET`.

Validation used the frozen gate logs, without rerunning builds: 12 integration tests passed; the DST property passed; diff coverage was 97.6%; mutation testing reported 15 caught and 3 unviable mutants, with none surviving. Existing tracked deferrals were not reopened.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether to adopt the prerequisite's 100,000-byte segment ceiling or re-enter Plan for the brief's 50,000-byte ceiling: an independent 49,999→50,018-byte move succeeds and certifies the drain, contrary to the brief (`brief.md:39`; `crates/core/src/metadata.rs:3218`; `crates/custodian/tests/segmented_map_evacuate.rs:543`; `reviewer-half-ceiling.log:9`).
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art for all four affected paths: this artifact contains one synthetic commit and no remote, so its history cannot settle duplication or rejected direction; the contribution-artifact audit itself is deferred/N/A until publish (`reviewer-prior-art.log:1`; `brief.md:291`; `gate-logs/T4-contribution.log:10`).
- [ ] Validation — fitness-to-purpose — Approve the tested behavior for the intended decommission workflow and clear applicable INTEGRATION §4 human-only items: simulated drain convergence is demonstrated, but operational acceptance belongs to the maintainer and that human-only inventory is absent from this bundle (`crates/custodian/tests/segmented_map_evacuate.rs:464`; `reviewer-prior-art.log:11`).
- [ ] The separate shared-`ChunkId` problem is not established as a defect. The target scans every inode and pushes a plan for every matching reference (`crates/custodian/src/rebalance.rs:264`, `:318`, `:367-376`); each plan conditionally updates its own record and publishes an orphan marker (`:538-556`), while GC explicitly never reclaims a fragment named by any committed map (`crates/custodian/src/gc.rs:143-146`, `:185-193`). That evidence supports the segmented refusal at `rebalance.rs:346-355`, but contradicts the brief's implication that two moves/orphan puts are themselves a safety failure (`brief.md:10-14`, `:81-91`). State the observable failure caused by duplicate planning, or remove this second defect.
- [ ] Success-criterion leg 2 does not discriminate the requested dedup. Its declared pre-fix red is only the segmented refusal (`brief.md:30-36`); after fixing that refusal, the existing one-plan-per-object shape at `crates/custodian/src/rebalance.rs:264-376` can move both references and satisfy every final-state assertion while still doing the two moves the brief calls defective. Conversely, reconstruction's first-reference-wins dedup (`crates/custodian/src/reconstruction.rs:375`, `:524-529`) would leave the second object naming the draining server and violate the same criterion. Add a red→green observation specific to the alleged duplicate-work failure and define how all references are repointed, or drop the dedup requirement.
- [ ] The scope contains an independent second change: the brief itself introduces cross-object dedup with “Separately” (`brief.md:10-14`) and mandates it at `brief.md:115-124`, but the target's same per-object planning already applies to flat maps (`crates/custodian/src/rebalance.rs:314-376`), so it is not forced by adding `seg:` repointing. Split it or supply a causal reason it must land atomically with the segmented-drain fix.
- [ ] The DST criterion assigns an incoherent outcome to one interleaving: it says the repoint commits nothing when the repoint commits both before and after the racing write (`brief.md:52-65`). A later racing write cannot retroactively make an earlier successful conditional commit write nothing; the target's existing CAS reports `Conflict` only when its `require` loses at commit (`crates/custodian/src/rebalance.rs:538-556`). Specify separate observable outcomes for race-before-repoint and repoint-before-race.
- [ ] The tracker claims cannot be checked because this review bundle contains neither `notes.json` nor `sources/`, yet the shared-chunk addition is justified only as a parent-review “T3 finding” (`brief.md:36-40`) and several constraints rely on unavailable rejected-work records (`brief.md:230-250`). Restore the tracker artifacts or quote the load-bearing tracker comments in the brief before retaining those requirements.
- [ ] The declared implementation base is not presently resolvable: `dependency-state.json:2-5` says #721 exists but is still `PLANNED`, and the target still contains the old flat-only reconstruction commit (`crates/custodian/src/reconstruction.rs:894-949`) rather than the promised `repoint_chunk` primitive. This matches the brief's admission that the patch cannot compile until #721 merges (`brief.md:67-80`, `:92-106`); refresh `$PDCA_TARGET` to a merged #721 base before Do, or declare a concrete stacked base that contains the primitive.
- [ ] C1 Spec — Reconcile the brief's 50,000-byte segment limit with the inherited 100,000-byte contract—an independently exercised 49,999→50,018-byte move commits, while changing the primitive is excluded from this slice (`brief.md:39`, `brief.md:168`, `crates/core/src/metadata.rs:593`, `crates/custodian/tests/segmented_map_evacuate.rs:444`).
- [ ] `crates/custodian/tests/segmented_map_evacuate.rs:444`: The new ceiling test seeds at `MAX_VALUE_BYTES` (100,000), whereas brief leg 3 explicitly requires a crossing of V/2 (50,000). This test cannot establish that requirement. The target's shared primitive explicitly uses the full ceiling for segment records (`crates/core/src/metadata.rs:593`), so this is a brief/dependency contract mismatch, not a request for a duplicate caller-side guard. Decide whether to update the acceptance boundary to the landed primitive's contract or split out a prerequisite primitive change; that file is expressly outside this patch's scope.
- [ ] C1 Spec — Resolve the brief’s 50,000-byte segment bound against the inherited primitive’s 100,000-byte bound — a move crossing 50,000 succeeds, so criterion 3 is unmet as written (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:520`; `reviewer-half-ceiling.log:15`).
- [ ] `crates/custodian/tests/segmented_map_evacuate.rs:528`: **The new ceiling regression does not exercise the brief's required V/2 refusal.** It seeds a 100,000-byte segment and tests growth beyond that full ceiling. A 49,999-byte segment growing by the same 19 bytes would instead commit, contrary to success criterion 3. This matches the target primitive's intentional full-ceiling contract (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3340`), so the passing test cannot settle the discrepancy. Decide whether to revise the brief to that contract or split out a prerequisite primitive change; the brief forbids changing the primitive or duplicating its ceiling check in this slice.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Confirm merged and closed/rejected prior art for all four affected paths: this artifact contains one synthetic commit and no remote, so its history cannot settle duplication or rejected direction; the contribution-artifact audit itself is deferred/N/A until publish (`reviewer-prior-art.log:1`; `brief.md:291`; `gate-logs/T4-contribution.log:10`).. Deferred, not addressed here: 7 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
