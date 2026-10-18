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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.6% — 80 of 82 instrumentable changed lines executed (floor 80%); 82 of 213 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 18 mutants tested in 4m: 15 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_722/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #722’s segmented-fragment drain completion: convergence is demonstrated, but the segment-size acceptance criterion needs a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Resolve the brief’s 50,000-byte segment bound against the inherited primitive’s 100,000-byte bound — a move crossing 50,000 succeeds, so criterion 3 is unmet as written (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:520`; `reviewer-half-ceiling.log:15`). |
| C2 Reproduction (red pre-fix) | PASS | Independently reverting production while retaining the discriminator reproduced 10 failures and one pass; both binding convergence cases fail on `Blocked` instead of `Changed`, without a compile failure (`reviewer-red.log:245`; `crates/custodian/tests/segmented_map_evacuate.rs:407`; `crates/custodian/tests/segmented_map_evacuate.rs:464`). |
| C3 Change | PASS | The four allowed files implement one drain-caller conversion through the inherited primitive; every object keeps its own plan, preserving convergence for shared chunks (`crates/custodian/src/rebalance.rs:443`; `crates/custodian/src/rebalance.rs:590`; `reviewer-shape.log:2`). |
| C4 Verification (red→green) | PASS | Independent restoration yields 302 custodian tests passed and one privileged Tier-1 test ignored, including all 11 discriminator cases passing; the new DST property also passes with `MADSIM_TEST_NUM=50`; these results establish convergence, with the separate acceptance-bound discrepancy recorded in C1 (`reviewer-green.log:305`; `reviewer-dst-restored.log:10`; `gate-logs/C4-ci.log:6001`). |
| C5 Causal adequacy | PASS | Removing the blanket segmented refusal fixes the demonstrated cause; independent deletion of either CAS pin breaks the DST assertions, and deleting the ceiling guard breaks the refusal test at the implemented V bound (`crates/custodian/src/rebalance.rs:590`; `reviewer-named-mutations.log:1`). |
| T1 Structure | PASS | The shared metadata primitive remains the write authority, and placement plus vacated-position evidence share one conditional commit; no new dependency, clock, public API, or persisted field appears (`crates/custodian/src/rebalance.rs:590`; `crates/custodian/src/rebalance.rs:640`). |
| T2 Shape | PASS | The patch stays within all explicit caps: four files, 105 added nonblank/noncomment production lines including delimiters, and 87,307 bytes (`reviewer-shape.log:1`; `brief.md:163`). |
| T3 Runtime | PASS | One root snapshot is retained per owed object; plans retain only their chunk, and segmented writes reread the addressed record; corrupt objects withhold certification while healthy work continues (`crates/custodian/src/rebalance.rs:436`; `crates/core/src/metadata.rs:3235`; `crates/custodian/tests/segmented_map_evacuate.rs:803`). |
| T4 Contribution | PASS | Merged history was queried for all four paths; all 19 closed/unmerged PR file lists were checked (#647: size rejection; #336: withdrawn duplicate ADR); contribution-artifact audit is N/A until publish (`reviewer-prior-art.log:184`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The added regressions exercise stale-generation classification, overflow, noncanonical keys, once-per-object containment, and store-error propagation; the tracked #698/#699/#723 deferrals remain settled (`crates/custodian/tests/segmented_map_evacuate.rs:633`; `crates/custodian/tests/segmented_map_evacuate.rs:691`; `crates/custodian/tests/segmented_map_evacuate.rs:788`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated committed-object drain convergence meets the intended decommission use case after resolving C1 — automated metadata/fragment assertions cannot decide operational fitness (`crates/custodian/tests/segmented_map_evacuate.rs:437`; `crates/custodian/tests/segmented_map_evacuate.rs:477`). |

Source citations under `crates/` refer to the supplied `$PDCA_TARGET` (`target/`); evidence logs and `brief.md` are in this review directory. The target contains the prerequisite primitive and compiled successfully; no stale-target exception was needed.

**The ceiling discrepancy needs a scope decision, not a local workaround.** Criterion 3 explicitly requires a segment starting below V/2 to refuse a crossing. The new test deliberately seeds at V, and the inherited primitive documents V for segment records and V/2 for segmented-root tables. I changed only that test’s fixture size to `MAX_VALUE_BYTES / 2 - 1`, kept its refusal assertions, and ran it against unchanged production: it returned `Changed`, failing the required `Blocked` assertion (`reviewer-half-ceiling.log:1`). The 49,999-byte fixture gains 19 bytes when server 0 becomes `u64::MAX`. The fixture was restored. Resolve whether to update the brief to the inherited V contract or return the bound to the prerequisite’s scope; this brief expressly forbids a duplicate caller-side ceiling and changes to the primitive (`brief.md:142`; `brief.md:168`). This is an acceptance-contract discrepancy, not evidence that the supplied patch fails to compile or that 50,018 bytes violates the backend’s V limit.

**The regression evidence distinguishes convergence from race safety.** The base’s lost-CAS leg fails because its racer never fires, and its ceiling leg passes because all segmented moves are refused. Neither establishes its intended property on the base. Independently deleting the root pin produces an overwritten segment after supersede; deleting the segment pin publishes orphan evidence after a lost race; deleting the ceiling guard lets the oversized move succeed. All three compile and fail assertions (`reviewer-mutation-root-pin.log`; `reviewer-mutation-segment-pin.log`; `reviewer-mutation-ceiling-guard.log`). The DST comment accurately limits its claim to commit-time interleavings (`crates/dst/tests/custodian.rs:5921`).

**Frozen gate evidence supports the remaining toolchain checks.** The CI log shows `typos`, documentation lint/render, dependency scanners, and conformance executed; the first attempt failed an unrelated health-probe RPC assertion, and the second completed all checks (`gate-logs/C4-ci.log:17`; `gate-logs/C4-ci.log:2534`; `gate-logs/C4-ci.log:5391`; `gate-logs/C4-ci.log:6001`). Diff coverage is 80/82 instrumentable changed lines, with 131 unscored lines, not 97.6% of all changed lines (`gate-logs/C4-diff-cov.log:197`). The TiKV log contains both feature builds; batch review reports zero blocking findings (`gate-logs/host-tikv.log:211`; `gate-logs/T4-batch-review.log:10`). T4-contribution is **N/A**: its inputs are drafted later and its substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`). No listed external dependency was left unexercised in these logs. Tier-1 disk-fault coverage remains intentionally ignored in the local suite; Tier-1 and Tier-2 kill/reconstruct observation can complement the Tier-0 evidence without becoming a new acceptance gate.

**Independent scanner reruns found no surviving viable mutant.** The first diff scan caught 10 mutants and rejected eight at compilation. Five rejections were caused only by warnings-as-errors; rerunning those five with `--cap-lints true` caught all five on the new regression assertions (`reviewer-mutants.log:21`; `reviewer-mutants-capped.log:8`). The combined evidence is 15 caught and three unviable, agreeing with the frozen scan. After restoring every temporary mutation, formatting, custodian clippy, both changed custodian test targets, and the 50-seed DST property passed (`reviewer-final-checks.log`; `reviewer-dst-restored.log`). All four files were independently reconstructed from the supplied patch and compared byte-for-byte; no extra tracked changes or stash remain (`reviewer-restoration.log:1`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #722 (a drain over a `seg:`-resident fragment completes)

**Verdict: I could not refute the fix.** One test gap is worth a small rebuild. The rest are notes.

What I re-ran myself, on a scratch copy of `$PDCA_TARGET`:

- **Red→green reproduced.** Patched: `segmented_map_evacuate` 11/11 green and `segmented_map_rebalance` 5/5 green. With the base `rebalance.rs` restored: 10 failed and 1 passed (leg 3). Legs 1 and 2 fail on their first real assertion (`Blocked` instead of `Changed`, `crates/custodian/tests/segmented_map_evacuate.rs:407`, `:464`), so they fail for the right reason. Both go through the real `reconcile_step` and check the store, not a copy of the production code.
- **The DST property's named negations hold** (DST = the deterministic simulation suite). At `MADSIM_TEST_NUM=50` the property is green. Deleting the `seg:` `require` in `repoint_chunk` (`crates/core/src/metadata.rs:3324`) turns it red at `crates/dst/tests/custodian.rs:6167`, because a lost move publishes an orphan mark. Deleting the root pin (`metadata.rs:3257`) turns it red at `:6163`, because a lost move rewrites the record.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:298-302` and `:439`: no test checks the claim that every move pins "the generation the resolve ANSWERED FROM". I made a hand mutation, `prior: record.clone()`, which pins the scanned snapshot instead of `resolved.record`. It passes the whole `wyrd-custodian` test suite. The DST property can't catch it either, because its racer fires inside `commit()`, after the resolve. **Concrete case:** a segmented root R1 sits at `inode:3`. Between the resolver's `seg:` page read and its root re-read, a supersede installs R2 (a new group naming the same drained chunk), so the resolve restarts onto R2. **Patched:** `Changed`, R2's `seg:` record is repointed, and the drain status is `Satisfied`. **Mutant:** the move walks R1's table and pins R1's root. It copies the fragment to the free server, loses the CAS (compare-and-swap), and answers `Blocked` with a `conflict` line. That leaves an unreferenced, unmarked copy (the #723 leak class) and wastes a pass. This is not a safety hole, only lost progress. But it is the one property the new `EvacObject` refactor introduces, and nothing tests it. **Fix:** add a drain leg with a `scan_page` hook that supersedes the root after the first `seg:` page. I ran exactly that as a probe, and it is about 25 lines on the existing fixture.
- `crates/custodian/tests/segmented_map_evacuate.rs:447-449`: leg 2's doc comment says an object left naming the drained server "would name a fragment GC is free to reclaim once the other object's move orphan-marks it". That is false. GC (garbage collection) never reclaims a fragment that any committed map names (`crates/custodian/src/gc.rs:442-445`). That was the brief's own reason (a) for dropping cross-object dedup. The real cost of a missed reference is that the drain never certifies. This is a one-line comment fix, so the test doesn't argue for the dedup the brief rejected. Fold it into the rebuild above. It is cosmetic, so it is not a NEEDS-HUMAN item.
- Leg 3 departs from the brief, and the build is right. The brief said to key the fixture to V/2 because "#721 settles that a `seg:` record is weighed at V/2". The base primitive actually weighs a `seg:` record against the full `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:593-600`, `:3218-3219`), so a fixture keyed to V/2 would commit instead of being refused. The test keys to `MAX_VALUE_BYTES` and says why (`segmented_map_evacuate.rs:520-521`). The brief's premise was wrong, not the build. As the brief declared, leg 3 also passes on the base. It shows only that the drain copies nothing ahead of a refusal.
- `crates/custodian/src/rebalance.rs:747`: the `emit_conflict` audit line says "copied fragments are collectable garbage". The patch now sends two arms there that copy nothing: the `Repoint::Conflict` returned while preparing the move (`:618`), and a stale `Refused`/`VersionExhausted` (`:605-609`). The "collectable garbage" wording belongs to #723, which is settled, so I'm not raising it here. When #723 rewrites that line, it should cover these two no-copy arms too.

## Refutation attempts that did not land

Each was run as a probe through `reconcile_step` on the patched tree:

- Two drained chunks in one `seg:` record both move in one pass, with no conflict. The move re-reads the `seg:` record (`metadata.rs:3288`), so the second plan sees the first commit.
- A zero-length drained chunk at a segment boundary moves and answers `Changed`, whether it trails segment 0 or leads segment 1 (`metadata.rs:3354-3359`, `rebalance.rs:383-385`).
- The same `ChunkRef` appearing twice in one object (two segments): both copies are repointed in one pass.
- A chunk shared between a flat object and a segmented object: both are repointed, and the status is `Satisfied`.
- A sibling chunk edited just before the move's `seg:` re-read is merged: the answer is `Changed` and the sibling's edit is kept.
- In a flat map, a drained chunk that comes before a length overflow still moves. The object is contained and named once, and the pass answers `Blocked` (`rebalance.rs:418-424`). It is never an anonymous conflict.
- A flat object with two drained chunks: pass 1 is `Blocked` with one conflict, because the second plan pins the root as it was before the first commit. Pass 2 is `Changed`. The base `rebalance.rs` behaves identically, so this is pre-existing and not this diff.
- Gate claims check out. C4-ci's failed first attempt was `a_hanging_health_probe_fails_closed_within_a_bounded_wait` (`crates/server/tests/health_probe.rs:537`, `gate-logs/C4-ci.log:2534`), which is unrelated to this diff. The madsim DST suite ran, and the new property passed (`gate-logs/C4-ci.log:5934`). C4-verify's "11 test(s) ran red" is a count of tests run; the real split was 10 failing and 1 passing. I found nothing that contradicts T4's "0 blocking".

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_evacuate.rs:528`: **The new ceiling regression does not exercise the brief's required V/2 refusal.** It seeds a 100,000-byte segment and tests growth beyond that full ceiling. A 49,999-byte segment growing by the same 19 bytes would instead commit, contrary to success criterion 3. This matches the target primitive's intentional full-ceiling contract (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3340`), so the passing test cannot settle the discrepancy. Decide whether to revise the brief to that contract or split out a prerequisite primitive change; the brief forbids changing the primitive or duplicating its ceiling check in this slice.

No additional correctness, reuse/simplification, or efficiency findings in the diff. Validation was assessed from the frozen gate logs; no tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Resolve the brief’s 50,000-byte segment bound against the inherited primitive’s 100,000-byte bound — a move crossing 50,000 succeeds, so criterion 3 is unmet as written (`brief.md:39`; `crates/core/src/metadata.rs:593`; `crates/custodian/tests/segmented_map_evacuate.rs:520`; `reviewer-half-ceiling.log:15`).
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated committed-object drain convergence meets the intended decommission use case after resolving C1 — automated metadata/fragment assertions cannot decide operational fitness (`crates/custodian/tests/segmented_map_evacuate.rs:437`; `crates/custodian/tests/segmented_map_evacuate.rs:477`).
- [ ] `crates/custodian/src/rebalance.rs:298-302` and `:439`: no test checks the claim that every move pins "the generation the resolve ANSWERED FROM". I made a hand mutation, `prior: record.clone()`, which pins the scanned snapshot instead of `resolved.record`. It passes the whole `wyrd-custodian` test suite. The DST property can't catch it either, because its racer fires inside `commit()`, after the resolve. **Concrete case:** a segmented root R1 sits at `inode:3`. Between the resolver's `seg:` page read and its root re-read, a supersede installs R2 (a new group naming the same drained chunk), so the resolve restarts onto R2. **Patched:** `Changed`, R2's `seg:` record is repointed, and the drain status is `Satisfied`. **Mutant:** the move walks R1's table and pins R1's root. It copies the fragment to the free server, loses the CAS (compare-and-swap), and answers `Blocked` with a `conflict` line. That leaves an unreferenced, unmarked copy (the #723 leak class) and wastes a pass. This is not a safety hole, only lost progress. But it is the one property the new `EvacObject` refactor introduces, and nothing tests it. **Fix:** add a drain leg with a `scan_page` hook that supersedes the root after the first `seg:` page. I ran exactly that as a probe, and it is about 25 lines on the existing fixture.
- [ ] `crates/custodian/tests/segmented_map_evacuate.rs:528`: **The new ceiling regression does not exercise the brief's required V/2 refusal.** It seeds a 100,000-byte segment and tests growth beyond that full ceiling. A 49,999-byte segment growing by the same 19 bytes would instead commit, contrary to success criterion 3. This matches the target primitive's intentional full-ceiling contract (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3340`), so the passing test cannot settle the discrepancy. Decide whether to revise the brief to that contract or split out a prerequisite primitive change; the brief forbids changing the primitive or duplicating its ceiling check in this slice.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed
- [ ] The separate shared-`ChunkId` problem is not established as a defect. The target scans every inode and pushes a plan for every matching reference (`crates/custodian/src/rebalance.rs:264`, `:318`, `:367-376`); each plan conditionally updates its own record and publishes an orphan marker (`:538-556`), while GC explicitly never reclaims a fragment named by any committed map (`crates/custodian/src/gc.rs:143-146`, `:185-193`). That evidence supports the segmented refusal at `rebalance.rs:346-355`, but contradicts the brief's implication that two moves/orphan puts are themselves a safety failure (`brief.md:10-14`, `:81-91`). State the observable failure caused by duplicate planning, or remove this second defect.
- [ ] Success-criterion leg 2 does not discriminate the requested dedup. Its declared pre-fix red is only the segmented refusal (`brief.md:30-36`); after fixing that refusal, the existing one-plan-per-object shape at `crates/custodian/src/rebalance.rs:264-376` can move both references and satisfy every final-state assertion while still doing the two moves the brief calls defective. Conversely, reconstruction's first-reference-wins dedup (`crates/custodian/src/reconstruction.rs:375`, `:524-529`) would leave the second object naming the draining server and violate the same criterion. Add a red→green observation specific to the alleged duplicate-work failure and define how all references are repointed, or drop the dedup requirement.
- [ ] The scope contains an independent second change: the brief itself introduces cross-object dedup with “Separately” (`brief.md:10-14`) and mandates it at `brief.md:115-124`, but the target's same per-object planning already applies to flat maps (`crates/custodian/src/rebalance.rs:314-376`), so it is not forced by adding `seg:` repointing. Split it or supply a causal reason it must land atomically with the segmented-drain fix.
- [ ] The DST criterion assigns an incoherent outcome to one interleaving: it says the repoint commits nothing when the repoint commits both before and after the racing write (`brief.md:52-65`). A later racing write cannot retroactively make an earlier successful conditional commit write nothing; the target's existing CAS reports `Conflict` only when its `require` loses at commit (`crates/custodian/src/rebalance.rs:538-556`). Specify separate observable outcomes for race-before-repoint and repoint-before-race.
- [ ] The tracker claims cannot be checked because this review bundle contains neither `notes.json` nor `sources/`, yet the shared-chunk addition is justified only as a parent-review “T3 finding” (`brief.md:36-40`) and several constraints rely on unavailable rejected-work records (`brief.md:230-250`). Restore the tracker artifacts or quote the load-bearing tracker comments in the brief before retaining those requirements.
- [ ] The declared implementation base is not presently resolvable: `dependency-state.json:2-5` says #721 exists but is still `PLANNED`, and the target still contains the old flat-only reconstruction commit (`crates/custodian/src/reconstruction.rs:894-949`) rather than the promised `repoint_chunk` primitive. This matches the brief's admission that the patch cannot compile until #721 merges (`brief.md:67-80`, `:92-106`); refresh `$PDCA_TARGET` to a merged #721 base before Do, or declare a concrete stacked base that contains the primitive.
- [ ] C1 Spec — Reconcile the brief's 50,000-byte segment limit with the inherited 100,000-byte contract—an independently exercised 49,999→50,018-byte move commits, while changing the primitive is excluded from this slice (`brief.md:39`, `brief.md:168`, `crates/core/src/metadata.rs:593`, `crates/custodian/tests/segmented_map_evacuate.rs:444`).
- [ ] `crates/custodian/tests/segmented_map_evacuate.rs:444`: The new ceiling test seeds at `MAX_VALUE_BYTES` (100,000), whereas brief leg 3 explicitly requires a crossing of V/2 (50,000). This test cannot establish that requirement. The target's shared primitive explicitly uses the full ceiling for segment records (`crates/core/src/metadata.rs:593`), so this is a brief/dependency contract mismatch, not a request for a duplicate caller-side guard. Decide whether to update the acceptance boundary to the landed primitive's contract or split out a prerequisite primitive change; that file is expressly outside this patch's scope.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — `crates/custodian/src/rebalance.rs:298-302` and `:439`: no test checks the claim that every move pins "the generation the resolve ANSWERED FROM". I made a hand mutation, `prior: record.clone()`, which pins the scanned snapshot instead of `resolved.record`. It passes the whole `wyrd-custodian` test suite. The DST property can't catch it either, because its racer fires inside `commit()`, after the resolve. **Concrete case:** a segmented root R1 sits at `inode:3`. Between the resolver's `seg:` page read and its root re-read, a supersede installs R2 (a new group naming the same drained chunk), so the resolve restarts onto R2. **Patched:** `Changed`, R2's `seg:` record is repointed, and the drain status is `Satisfied`. **Mutant:** the move walks R1's table and pins R1's root. It copies the fragment to the free server, loses the CAS (compare-and-swap), and answers `Blocked` with a `conflict` line. That leaves an unreferenced, unmarked copy (the #723 leak class) and wastes a pass. This is not a safety hole, only lost progress. But it is the one property the new `EvacObject` refactor introduces, and nothing tests it. **Fix:** add a drain leg with a `scan_page` hook that supersedes the root after the first `seg:` page. I ran exactly that as a probe, and it is about 25 lines on the existing fixture.. Deferred, not addressed here: 9 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
