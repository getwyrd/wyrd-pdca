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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 75.4% — 46 of 61 instrumentable changed lines executed (below the 80% floor); 61 of 168 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 12 mutants tested in 3m: 2 missed, 7 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_722/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.27s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #722: make D-server decommissioning converge for segmented fragments; the normal drain works, but overflow handling, test gaps, and a ceiling-contract mismatch remain unresolved.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Reconcile the brief's 50,000-byte segment limit with the inherited 100,000-byte contract—an independently exercised 49,999→50,018-byte move commits, while changing the primitive is excluded from this slice (`brief.md:39`, `brief.md:168`, `crates/core/src/metadata.rs:593`, `crates/custodian/tests/segmented_map_evacuate.rs:444`). |
| C2 Reproduction (red pre-fix) | PASS | Independent production-only stash reproduced three assertion failures out of four tests; legs 1–2 demonstrate the stuck drain, while leg 4 fails because its racer never fires (`reviewer-red.log:144`, `crates/custodian/tests/segmented_map_evacuate.rs:323`, `crates/custodian/tests/segmented_map_evacuate.rs:503`). |
| C3 Change | PASS | The normal move preserves reference safety and atomic orphan evidence through the existing primitive; both references to a shared chunk move, without cross-object dedup or destination retraction (`crates/custodian/src/rebalance.rs:409`, `crates/custodian/src/rebalance.rs:555`, `crates/custodian/src/rebalance.rs:605`). |
| C4 Verification (red→green) | FAIL | Independent red→green and nine changed-file tests pass, but independent coverage reproduces 46/61 changed executable lines (75.4%), below the configured 80% floor; this is a coverage failure (`reviewer-green.log:11`, `reviewer-green.log:22`, `reviewer-diff-coverage.log:1`, `gate-logs/C4-diff-cov.log:218`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add observable stale-generation/terminal-verdict regressions—both freshness-check mutants survive the custodian suite, allowing obsolete records to be reported as permanent repair problems; CAS negations do fail as intended (`crates/custodian/src/rebalance.rs:571`, `crates/custodian/src/rebalance.rs:628`, `reviewer-mutants-capped.log:4`). |
| T1 Structure | PASS | Trait boundaries and atomic placement/orphan composition remain intact; the new race fixture has instance-owned state and no new correctness clock (`crates/custodian/src/rebalance.rs:72`, `crates/custodian/src/rebalance.rs:605`, `crates/dst/tests/custodian.rs:5945`). |
| T2 Shape | PASS | Four allowed files, 82 nonblank/noncomment production additions, and 66,983 patch bytes stay within the brief's caps; the primitive and unrelated maintenance callers are unchanged (`patch.diff:1`, `patch.diff:488`, `patch.diff:1025`, `patch.diff:1124`, `brief.md:162`). |
| T3 Runtime | FAIL | An overflowing flat-map prefix permanently strands later drain work as retryable conflicts, without identifying the corrupt object; two independent passes reproduce this behavior (`crates/custodian/src/rebalance.rs:363`, `crates/core/src/metadata.rs:3376`, `reviewer-probes.log:21`). |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art for all four affected paths—the supplied history is one synthetic commit with no remote, and the brief's historical summary cannot mechanically settle the check (`reviewer-prior-art.log:2`, `brief.md:291`); contribution-artifact audit is deferred/N/A until publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Contain and attribute offset overflow before planning moves, with a regression proving the operator receives a repair signal—retrying unchanged corrupt metadata cannot complete the drain (`crates/custodian/src/rebalance.rs:356`, `crates/custodian/src/rebalance.rs:583`, `reviewer-probes.log:37`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated drain and race behavior is sufficient for operator decommissioning after the ceiling contract and findings are resolved; the project's additional INTEGRATION §4 sign-off items were not supplied (`crates/custodian/tests/segmented_map_evacuate.rs:353`, `crates/dst/tests/custodian.rs:5921`, `reviewer-prior-art.log:8`). |

Source citations beginning `crates/` refer to the supplied `$PDCA_TARGET`; evidence and brief citations refer to this review directory. The target is readable, includes the prerequisite primitive, and passes reverse-patch applicability checks. No implementation fixes were made.

1. **Overflow needs explicit corruption handling.** A decoded flat map with chunk lengths `[u64::MAX, 1, 5]`, with only the final chunk owing evacuation, produces two consecutive `Blocked` passes and two `action: conflict` events. Its metadata remains unchanged and no `unresolvable-chunk-map` event names the object (`reviewer-probes.log:21`). Flat decoding accepts this shape (`crates/core/src/metadata.rs:1697`), while the primitive's checked offset walk cannot reach the final reference (`crates/core/src/metadata.rs:3367`). The newly introduced saturation therefore turns permanent corruption into an endless apparent race. Detect the invalid accumulated length, discard that object's plans, and report containment. This independently confirms the frozen batch review's finding (`gate-logs/T4-batch-review.log:10`).

2. **The ceiling requires a Plan decision.** The brief explicitly requires refusal when a segment crosses V/2, but the inherited primitive explicitly documents V for segments and reserves V/2 for segmented roots (`crates/core/src/metadata.rs:593`). The new test checks V (`crates/custodian/tests/segmented_map_evacuate.rs:431`). The independent probe actually copied the destination fragment and committed a 49,999→50,018-byte record (`reviewer-probes.log:13`). Decide whether to revise the brief to the inherited contract or commission a separate primitive change; adding a caller-local ceiling would violate this slice's stated constraints.

3. **Terminal-outcome behavior needs stronger tests.** With mutation-induced unused-code warnings capped, replacing the freshness match guard with `false`, or replacing `still_current` with `Ok(true)`, survives the full custodian suite (`reviewer-mutants-capped.log:4`). These mutations can misattribute a superseded generation's ceiling/version problem to the live object. Exercise current and superseded terminal verdicts and assert their audit attribution, as well as preparation-time corruption/store-error handling. The independent coverage report identifies the same 15 missed lines as the frozen gate, including containment, version exhaustion, and preparation conflict/error handling (`reviewer-diff-coverage.log:6`; `crates/custodian/src/rebalance.rs:578`).

The successful evidence supports the normal drain and conditional-commit behavior:

- Independent production stash/restore: three expected failures pre-fix; all four new tests and all five updated rebalance tests pass after restoration. Leg 3 already passes pre-fix; leg 4's base failure is not proof of conflict handling (`reviewer-red.log:178`, `reviewer-green.log:11`).
- The new DST property passes with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50` (`reviewer-dst.log`). In an isolated copy beneath this review directory, removing the root precondition and removing the segment precondition each produce assertion failures in that property; removing the segment precondition also fails deterministic leg 4, and removing the ceiling check fails leg 3 (`reviewer-negations.log:1`, `reviewer-negations.log:27`, `reviewer-negations.log:53`, `reviewer-negations.log:79`). These prove the asserted CAS and full-ceiling checks; the DST property's stated read→prepare limitation remains accurate.
- Independent formatting, typo, documentation lint, and diff checks pass. The instance-scoped CI wrapper was adjudicated from its frozen output: actual build/test, dependency scanners, conformance, statics, and DST steps complete successfully (`gate-logs/C4-ci.log:3389`, `gate-logs/C4-ci.log:3999`). The TiKV feature compilation also completes (`gate-logs/host-tikv.log:211`). The brief's external tools are evidenced as exercised; no unmet external tool dependency is inferred.
- **T4-contribution: N/A.** Its subject is intentionally absent until publish, where the substantive audit must rerun (`gate-logs/T4-contribution.log:10`). The settled #723 destination-stranding deferral and withdrawn cross-object dedup are not reopened. No capability probe or load-time symptom guard was introduced. The existing Tier-2 durability campaign is an appropriate follow-up observation; no new disk implementation requires a separate Tier-1 check for this slice.

The disposable target's patch files and primitive were verified unchanged after all checks, with no leftover stash. This report is advisory; it does not decide acceptance.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #722 (segmented drain evacuation)

The red→green proof holds. I re-ran it in a scratch copy: on the base, legs 1, 2 and 4 fail (`segmented_map_evacuate.rs:323`, `:380`, `:503`) and leg 3 passes; with the patch all 4 pass, and so does the rewritten `segmented_map_rebalance.rs`. The legs go through the real `reconcile_step` and check the store, so they are not a copy of the production code. Where the fix breaks is three cases. In each one the brief told the builder to copy the reconstruction caller (`reconstruction.rs` as #721 left it), and the drain caller left out a guard that the peer has. I ran each case against both the base and the patched `rebalance.rs`.

- NEEDS-HUMAN [impl] — **An overflowing offset becomes a conflict on every pass, and flat maps regress.** `crates/custodian/src/rebalance.rs:356-363` saturates the running offset instead of failing. Flat maps get no length check at decode (`crates/core/src/metadata.rs:1697-1704` checks segmented maps only), so the bad offset is reachable. Test case: a committed **flat** object with chunks `[len u64::MAX, len 1, drained len 5]` and the drained chunk on server 0. **Base:** the first pass answers `Changed` (fragment copied, orphan-marked) and the next answers `Satisfied`. **Patched:** `Blocked, Blocked, Blocked`. The chunk never moves. Each pass logs `action:"conflict"` ("lost the version-conditional commit; copied fragments are collectable garbage"), but nothing raced and nothing was copied, and the object is never named. The peer handles this exact case at `crates/custodian/src/reconstruction.rs:673-698`: it computes the offset with `checked_add`, and when that returns `None` it contains the object as "the chunk lengths overflow the object's byte range", with the comment "never a conflict every pass". This confirms the blocking T4-batch-review finding with a concrete case. Fix: carry `Option<u64>`, then `emit_unresolvable` and `withheld = true` for that object.

- NEEDS-HUMAN [impl] — **A segmented object under a non-canonical key gets a fragment copy on every pass and an unnamed conflict.** The drain parses `inode:03` to id 3 (`rebalance.rs:320`), then pins the root at the canonical `inode:3` (`rebalance.rs:555-563` → `repoint_chunk`), which is not the row the scan read. The peer contains exactly this case (`reconstruction.rs:652-660`, tested as leg 12 at `crates/custodian/tests/segmented_map_repoint.rs:19`, `:889`). The peer's comment says only the *flat* case belongs to #698, so the #698 deferral does not cover this one. Test case: seed a segmented object, move its root from `inode:3` to `inode:03`, drain server 0. **Base:** each pass answers `Blocked` with a named `refused-segmented` event, and nothing is copied. **Patched:** each pass copies `frag(CHUNK,0)` to the free server, loses the CAS on the absent `inode:3`, logs a plain `conflict`, and answers `Blocked`. The object is never named, and the copy it leaves is unreferenced and unmarked (the #723 leak class), rewritten every pass. Fix: add the peer's `(segmented, key != inode_key(id))` guard and contain the object.

- NEEDS-HUMAN [impl] — **A contained fault is reported once per chunk, not once per object.** `rebalance.rs:188-191` emits `emit_unresolvable` for every plan, and every emit bumps `rebalance_unresolvable_records` (`rebalance.rs:693-694`), a counter named per *record*. The peer contains "once per OBJECT" (`segmented_map_repoint.rs:17`, legs 6–10). The `emit_refused` this patch deletes was also once per object, for this reason: "a line per chunk floods the seam". Test case: one segmented object owing two evacuations in a single `seg:` record, and that record is deleted on the move's own `get` while the root still names it. Result: `Blocked` with **two** `unresolvable-chunk-map` events and the counter at 2 for one record. A 1,000-chunk object would log 1,000 lines. Fix: dedupe by object index, as the peer does.

- NEEDS-HUMAN [impl] — **The drain-specific outcome handling has no test, and the DST comment claims coverage it does not have.** Diff coverage misses `rebalance.rs:188-190` (the Contained arm), `:573` (a stale plan answered as a conflict), `:578-583` (VersionExhausted, and `Repoint::Conflict` at prepare time) and `:587-593` (the downcast, and store-fault propagation). C5 keeps two live mutants at `:571` (guard → `false`) and `:628` (`still_current` → `Ok(true)`). The peer tests these same arms (legs 5b, 6–11). `crates/dst/tests/custodian.rs:5924-5926` says the read→prepare window "is covered by the deterministic legs of `segmented_map_repoint.rs`", but those legs drive `ReconstructionContext`. They cover the shared primitive, not the drain's own handling of its answers. The surviving mutants only change which label a non-certifying outcome gets, so this is not a safety hole. Still, the three findings above all sit in this untested surface. Fix: add drain-side legs for a ceiling refusal on a superseded root (it should come out as a conflict), a torn `seg:` record (contained, named once), and a store fault on the move's `get` (the pass ends with `Err`). Or narrow the DST comment's claim.

- Note, no action: leg 3 is keyed to `MAX_VALUE_BYTES` (`crates/custodian/tests/segmented_map_evacuate.rs:436-444`), not the V/2 the brief asked for. The brief's premise is wrong: #721's primitive weighs a `seg:` record at the full ceiling (`metadata.rs:593-600`), so a V/2 fixture would have committed rather than been refused. The builder was right to deviate, and the reviewer should not count it against the patch.
- Nit [impl, not §6]: `rebalance.rs:142-143` still says `Blocked` covers "an evacuation it may not perform ([`EvacScan::withheld`])", but `withheld` no longer carries refusals (`rebalance.rs:230-233`).

**Attacks I tried that did not land:**
- Two drained chunks in **one** `seg:` record: both land in one pass (`Changed`, then `Satisfied`). The second move re-reads the record, so the first move does not make it conflict.
- Leg 2 does catch a first-reference-wins dedup.
- Memory: one root clone per object. That is better than the base's clone per plan.
- DST negations, checked by reasoning (12 seed arms, 50 seeds):
  - Deleting the `seg:` pin turns the Rewrite / racer-first arm red: the move still publishes an orphan mark.
  - Deleting the root pin turns the Supersede / racer-first arm red the same way.
  - In the Rewrite arm the racer writes the same bytes the move would, so the "record rewritten" assertion cannot fail there. That arm catches a broken move only through the orphan mark and the pass answer. That is enough.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:363`: Saturating the new byte offset turns corrupt flat maps into permanent retry loops. Flat lengths are not validated at decode; with distinct chunks of lengths `[u64::MAX, 1, 1]`, the third chunk is planned at `u64::MAX`, but `repoint_chunk`'s checked walk cannot reach it (`crates/core/src/metadata.rs:3376`). If that chunk owes an evacuation, every pass reports a transient conflict instead of identifying the object's corruption; the previous index-based plan could address it. Track overflow explicitly and contain/name the affected object, as reconstruction does at `crates/custodian/src/reconstruction.rs:693`, with a regression covering continued evacuation of healthy objects. This independently corroborates the frozen T4 finding.

- NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_evacuate.rs:444`: The new ceiling test seeds at `MAX_VALUE_BYTES` (100,000), whereas brief leg 3 explicitly requires a crossing of V/2 (50,000). This test cannot establish that requirement. The target's shared primitive explicitly uses the full ceiling for segment records (`crates/core/src/metadata.rs:593`), so this is a brief/dependency contract mismatch, not a request for a duplicate caller-side guard. Decide whether to update the acceptance boundary to the landed primitive's contract or split out a prerequisite primitive change; that file is expressly outside this patch's scope.

No additional reuse, simplification, or efficiency findings. Reviewed the target source and frozen gate evidence; no builds were rerun and the target was not modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Reconcile the brief's 50,000-byte segment limit with the inherited 100,000-byte contract—an independently exercised 49,999→50,018-byte move commits, while changing the primitive is excluded from this slice (`brief.md:39`, `brief.md:168`, `crates/core/src/metadata.rs:593`, `crates/custodian/tests/segmented_map_evacuate.rs:444`).
- [ ] C5 Causal adequacy — Add observable stale-generation/terminal-verdict regressions—both freshness-check mutants survive the custodian suite, allowing obsolete records to be reported as permanent repair problems; CAS negations do fail as intended (`crates/custodian/src/rebalance.rs:571`, `crates/custodian/src/rebalance.rs:628`, `reviewer-mutants-capped.log:4`).
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art for all four affected paths—the supplied history is one synthetic commit with no remote, and the brief's historical summary cannot mechanically settle the check (`reviewer-prior-art.log:2`, `brief.md:291`); contribution-artifact audit is deferred/N/A until publish (`gate-logs/T4-contribution.log:10`).
- [ ] T5 Judgment — Contain and attribute offset overflow before planning moves, with a regression proving the operator receives a repair signal—retrying unchanged corrupt metadata cannot complete the drain (`crates/custodian/src/rebalance.rs:356`, `crates/custodian/src/rebalance.rs:583`, `reviewer-probes.log:37`).
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated drain and race behavior is sufficient for operator decommissioning after the ceiling contract and findings are resolved; the project's additional INTEGRATION §4 sign-off items were not supplied (`crates/custodian/tests/segmented_map_evacuate.rs:353`, `crates/dst/tests/custodian.rs:5921`, `reviewer-prior-art.log:8`).
- [ ] **An overflowing offset becomes a conflict on every pass, and flat maps regress.** `crates/custodian/src/rebalance.rs:356-363` saturates the running offset instead of failing. Flat maps get no length check at decode (`crates/core/src/metadata.rs:1697-1704` checks segmented maps only), so the bad offset is reachable. Test case: a committed **flat** object with chunks `[len u64::MAX, len 1, drained len 5]` and the drained chunk on server 0. **Base:** the first pass answers `Changed` (fragment copied, orphan-marked) and the next answers `Satisfied`. **Patched:** `Blocked, Blocked, Blocked`. The chunk never moves. Each pass logs `action:"conflict"` ("lost the version-conditional commit; copied fragments are collectable garbage"), but nothing raced and nothing was copied, and the object is never named. The peer handles this exact case at `crates/custodian/src/reconstruction.rs:673-698`: it computes the offset with `checked_add`, and when that returns `None` it contains the object as "the chunk lengths overflow the object's byte range", with the comment "never a conflict every pass". This confirms the blocking T4-batch-review finding with a concrete case. Fix: carry `Option<u64>`, then `emit_unresolvable` and `withheld = true` for that object.
- [ ] **A segmented object under a non-canonical key gets a fragment copy on every pass and an unnamed conflict.** The drain parses `inode:03` to id 3 (`rebalance.rs:320`), then pins the root at the canonical `inode:3` (`rebalance.rs:555-563` → `repoint_chunk`), which is not the row the scan read. The peer contains exactly this case (`reconstruction.rs:652-660`, tested as leg 12 at `crates/custodian/tests/segmented_map_repoint.rs:19`, `:889`). The peer's comment says only the *flat* case belongs to #698, so the #698 deferral does not cover this one. Test case: seed a segmented object, move its root from `inode:3` to `inode:03`, drain server 0. **Base:** each pass answers `Blocked` with a named `refused-segmented` event, and nothing is copied. **Patched:** each pass copies `frag(CHUNK,0)` to the free server, loses the CAS on the absent `inode:3`, logs a plain `conflict`, and answers `Blocked`. The object is never named, and the copy it leaves is unreferenced and unmarked (the #723 leak class), rewritten every pass. Fix: add the peer's `(segmented, key != inode_key(id))` guard and contain the object.
- [ ] **A contained fault is reported once per chunk, not once per object.** `rebalance.rs:188-191` emits `emit_unresolvable` for every plan, and every emit bumps `rebalance_unresolvable_records` (`rebalance.rs:693-694`), a counter named per *record*. The peer contains "once per OBJECT" (`segmented_map_repoint.rs:17`, legs 6–10). The `emit_refused` this patch deletes was also once per object, for this reason: "a line per chunk floods the seam". Test case: one segmented object owing two evacuations in a single `seg:` record, and that record is deleted on the move's own `get` while the root still names it. Result: `Blocked` with **two** `unresolvable-chunk-map` events and the counter at 2 for one record. A 1,000-chunk object would log 1,000 lines. Fix: dedupe by object index, as the peer does.
- [ ] **The drain-specific outcome handling has no test, and the DST comment claims coverage it does not have.** Diff coverage misses `rebalance.rs:188-190` (the Contained arm), `:573` (a stale plan answered as a conflict), `:578-583` (VersionExhausted, and `Repoint::Conflict` at prepare time) and `:587-593` (the downcast, and store-fault propagation). C5 keeps two live mutants at `:571` (guard → `false`) and `:628` (`still_current` → `Ok(true)`). The peer tests these same arms (legs 5b, 6–11). `crates/dst/tests/custodian.rs:5924-5926` says the read→prepare window "is covered by the deterministic legs of `segmented_map_repoint.rs`", but those legs drive `ReconstructionContext`. They cover the shared primitive, not the drain's own handling of its answers. The surviving mutants only change which label a non-certifying outcome gets, so this is not a safety hole. Still, the three findings above all sit in this untested surface. Fix: add drain-side legs for a ceiling refusal on a superseded root (it should come out as a conflict), a torn `seg:` record (contained, named once), and a store fault on the move's `get` (the pass ends with `Err`). Or narrow the DST comment's claim.
- [ ] `crates/custodian/src/rebalance.rs:363`: Saturating the new byte offset turns corrupt flat maps into permanent retry loops. Flat lengths are not validated at decode; with distinct chunks of lengths `[u64::MAX, 1, 1]`, the third chunk is planned at `u64::MAX`, but `repoint_chunk`'s checked walk cannot reach it (`crates/core/src/metadata.rs:3376`). If that chunk owes an evacuation, every pass reports a transient conflict instead of identifying the object's corruption; the previous index-based plan could address it. Track overflow explicitly and contain/name the affected object, as reconstruction does at `crates/custodian/src/reconstruction.rs:693`, with a regression covering continued evacuation of healthy objects. This independently corroborates the frozen T4 finding.
- [ ] `crates/custodian/tests/segmented_map_evacuate.rs:444`: The new ceiling test seeds at `MAX_VALUE_BYTES` (100,000), whereas brief leg 3 explicitly requires a crossing of V/2 (50,000). This test cannot establish that requirement. The target's shared primitive explicitly uses the full ceiling for segment records (`crates/core/src/metadata.rs:593`), so this is a brief/dependency contract mismatch, not a request for a duplicate caller-side guard. Decide whether to update the acceptance boundary to the landed primitive's contract or split out a prerequisite primitive change; that file is expressly outside this patch's scope.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_722/review-b
- [ ] The separate shared-`ChunkId` problem is not established as a defect. The target scans every inode and pushes a plan for every matching reference (`crates/custodian/src/rebalance.rs:264`, `:318`, `:367-376`); each plan conditionally updates its own record and publishes an orphan marker (`:538-556`), while GC explicitly never reclaims a fragment named by any committed map (`crates/custodian/src/gc.rs:143-146`, `:185-193`). That evidence supports the segmented refusal at `rebalance.rs:346-355`, but contradicts the brief's implication that two moves/orphan puts are themselves a safety failure (`brief.md:10-14`, `:81-91`). State the observable failure caused by duplicate planning, or remove this second defect.
- [ ] Success-criterion leg 2 does not discriminate the requested dedup. Its declared pre-fix red is only the segmented refusal (`brief.md:30-36`); after fixing that refusal, the existing one-plan-per-object shape at `crates/custodian/src/rebalance.rs:264-376` can move both references and satisfy every final-state assertion while still doing the two moves the brief calls defective. Conversely, reconstruction's first-reference-wins dedup (`crates/custodian/src/reconstruction.rs:375`, `:524-529`) would leave the second object naming the draining server and violate the same criterion. Add a red→green observation specific to the alleged duplicate-work failure and define how all references are repointed, or drop the dedup requirement.
- [ ] The scope contains an independent second change: the brief itself introduces cross-object dedup with “Separately” (`brief.md:10-14`) and mandates it at `brief.md:115-124`, but the target's same per-object planning already applies to flat maps (`crates/custodian/src/rebalance.rs:314-376`), so it is not forced by adding `seg:` repointing. Split it or supply a causal reason it must land atomically with the segmented-drain fix.
- [ ] The DST criterion assigns an incoherent outcome to one interleaving: it says the repoint commits nothing when the repoint commits both before and after the racing write (`brief.md:52-65`). A later racing write cannot retroactively make an earlier successful conditional commit write nothing; the target's existing CAS reports `Conflict` only when its `require` loses at commit (`crates/custodian/src/rebalance.rs:538-556`). Specify separate observable outcomes for race-before-repoint and repoint-before-race.
- [ ] The tracker claims cannot be checked because this review bundle contains neither `notes.json` nor `sources/`, yet the shared-chunk addition is justified only as a parent-review “T3 finding” (`brief.md:36-40`) and several constraints rely on unavailable rejected-work records (`brief.md:230-250`). Restore the tracker artifacts or quote the load-bearing tracker comments in the brief before retaining those requirements.
- [ ] The declared implementation base is not presently resolvable: `dependency-state.json:2-5` says #721 exists but is still `PLANNED`, and the target still contains the old flat-only reconstruction commit (`crates/custodian/src/reconstruction.rs:894-949`) rather than the promised `repoint_chunk` primitive. This matches the brief's admission that the patch cannot compile until #721 merges (`brief.md:67-80`, `:92-106`); refresh `$PDCA_TARGET` to a merged #721 base before Do, or declare a concrete stacked base that contains the primitive.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Add observable stale-generation/terminal-verdict regressions—both freshness-check mutants survive the custodian suite, allowing obsolete records to be reported as permanent repair problems; CAS negations do fail as intended (`crates/custodian/src/rebalance.rs:571`, `crates/custodian/src/rebalance.rs:628`, `reviewer-mutants-capped.log:4`).; T4 Contribution — Confirm merged and closed/rejected prior art for all four affected paths—the supplied history is one synthetic commit with no remote, and the brief's historical summary cannot mechanically settle the check (`reviewer-prior-art.log:2`, `brief.md:291`); contribution-artifact audit is deferred/N/A until publish (`gate-logs/T4-contribution.log:10`).; T5 Judgment — Contain and attribute offset overflow before planning moves, with a regression proving the operator receives a repair signal—retrying unchanged corrupt metadata cannot complete the drain (`crates/custodian/src/rebalance.rs:356`, `crates/custodian/src/rebalance.rs:583`, `reviewer-probes.log:37`).; **An overflowing offset becomes a conflict on every pass, and flat maps regress.** `crates/custodian/src/rebalance.rs:356-363` saturates the running offset instead of failing. Flat maps get no length check at decode (`crates/core/src/metadata.rs:1697-1704` checks segmented maps only), so the bad offset is reachable. Test case: a committed **flat** object with chunks `[len u64::MAX, len 1, drained len 5]` and the drained chunk on server 0. **Base:** the first pass answers `Changed` (fragment copied, orphan-marked) and the next answers `Satisfied`. **Patched:** `Blocked, Blocked, Blocked`. The chunk never moves. Each pass logs `action:"conflict"` ("lost the version-conditional commit; copied fragments are collectable garbage"), but nothing raced and nothing was copied, and the object is never named. The peer handles this exact case at `crates/custodian/src/reconstruction.rs:673-698`: it computes the offset with `checked_add`, and when that returns `None` it contains the object as "the chunk lengths overflow the object's byte range", with the comment "never a conflict every pass". This confirms the blocking T4-batch-review finding with a concrete case. Fix: carry `Option<u64>`, then `emit_unresolvable` and `withheld = true` for that object.; **A segmented object under a non-canonical key gets a fragment copy on every pass and an unnamed conflict.** The drain parses `inode:03` to id 3 (`rebalance.rs:320`), then pins the root at the canonical `inode:3` (`rebalance.rs:555-563` → `repoint_chunk`), which is not the row the scan read. The peer contains exactly this case (`reconstruction.rs:652-660`, tested as leg 12 at `crates/custodian/tests/segmented_map_repoint.rs:19`, `:889`). The peer's comment says only the *flat* case belongs to #698, so the #698 deferral does not cover this one. Test case: seed a segmented object, move its root from `inode:3` to `inode:03`, drain server 0. **Base:** each pass answers `Blocked` with a named `refused-segmented` event, and nothing is copied. **Patched:** each pass copies `frag(CHUNK,0)` to the free server, loses the CAS on the absent `inode:3`, logs a plain `conflict`, and answers `Blocked`. The object is never named, and the copy it leaves is unreferenced and unmarked (the #723 leak class), rewritten every pass. Fix: add the peer's `(segmented, key != inode_key(id))` guard and contain the object.; **A contained fault is reported once per chunk, not once per object.** `rebalance.rs:188-191` emits `emit_unresolvable` for every plan, and every emit bumps `rebalance_unresolvable_records` (`rebalance.rs:693-694`), a counter named per *record*. The peer contains "once per OBJECT" (`segmented_map_repoint.rs:17`, legs 6–10). The `emit_refused` this patch deletes was also once per object, for this reason: "a line per chunk floods the seam". Test case: one segmented object owing two evacuations in a single `seg:` record, and that record is deleted on the move's own `get` while the root still names it. Result: `Blocked` with **two** `unresolvable-chunk-map` events and the counter at 2 for one record. A 1,000-chunk object would log 1,000 lines. Fix: dedupe by object index, as the peer does.; **The drain-specific outcome handling has no test, and the DST comment claims coverage it does not have.** Diff coverage misses `rebalance.rs:188-190` (the Contained arm), `:573` (a stale plan answered as a conflict), `:578-583` (VersionExhausted, and `Repoint::Conflict` at prepare time) and `:587-593` (the downcast, and store-fault propagation). C5 keeps two live mutants at `:571` (guard → `false`) and `:628` (`still_current` → `Ok(true)`). The peer tests these same arms (legs 5b, 6–11). `crates/dst/tests/custodian.rs:5924-5926` says the read→prepare window "is covered by the deterministic legs of `segmented_map_repoint.rs`", but those legs drive `ReconstructionContext`. They cover the shared primitive, not the drain's own handling of its answers. The surviving mutants only change which label a non-certifying outcome gets, so this is not a safety hole. Still, the three findings above all sit in this untested surface. Fix: add drain-side legs for a ceiling refusal on a superseded root (it should come out as a conflict), a torn `seg:` record (contained, named once), and a store fault on the move's `get` (the pass ends with `Err`). Or narrow the DST comment's claim.; `crates/custodian/src/rebalance.rs:363`: Saturating the new byte offset turns corrupt flat maps into permanent retry loops. Flat lengths are not validated at decode; with distinct chunks of lengths `[u64::MAX, 1, 1]`, the third chunk is planned at `u64::MAX`, but `repoint_chunk`'s checked walk cannot reach it (`crates/core/src/metadata.rs:3376`). If that chunk owes an evacuation, every pass reports a transient conflict instead of identifying the object's corruption; the previous index-based plan could address it. Track overflow explicitly and contain/name the affected object, as reconstruction does at `crates/custodian/src/reconstruction.rs:693`, with a regression covering continued evacuation of healthy objects. This independently corroborates the frozen T4 finding.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_722/review-b. Deferred, not addressed here: 8 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
