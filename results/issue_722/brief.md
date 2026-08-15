- **Slug:** segmented-drain-evacuation-completes
- **Defect:** **A D-server decommission holding a `seg:`-resident fragment never
  converges.** #696 stopped rebalance aborting on a segmented object, but it deliberately
  writes nothing: an evacuation owed by a chunk whose `ChunkRef` lives in a `seg:` record is
  **refused and stays owed**, every pass, forever (`crates/custodian/src/rebalance.rs:352-355`
  — `scanned_flat` is `None`, so the chunk produces no plan and the object is counted refused
  at `:378-381`; the drain is never certified, `:185-190`). The operator's drain therefore never
  completes and the server can never be retired. Sibling slice #721 lands the placement move for
  whichever record holds a chunk; this child is the drain caller completing through it.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_evacuate.rs`
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
- **Falsifiability:** legs 1 and 2 go RED on this child's base — `origin/main` **after
  #721's PR is merged** — with no special topology and no external service. The forbidden
  state is reachable by seeding: a segmented object is hand-written as raw `seg:` records plus
  a segmented root (this build ships no producer of segmented maps), and `set_lifecycle(..,
  Draining)` puts a server into the drain state deterministically; the refusal then occurs on
  every pass, so no seed sweep is needed to observe it. Verified by dry-running the gate's
  classifier over a synthetic patch of this child's exact file set: it returns
  `ADDED_TEST crates/custodian/tests/segmented_map_evacuate.rs` as the sole discriminator, so
  the gate runs `-p wyrd-custodian --test segmented_map_evacuate`; that file carries no
  `#![cfg(...)]` and is genuinely compiled and executed in both legs, and the `crates/dst`
  edit — which *is* `#![cfg(madsim)]` — is applied but **not** compiled by the discriminator
  run, so it can neither vacuously green nor false-red the gate. **Base precondition:** if
  #721's PR is not merged when this child builds, `repoint_chunk` is absent and the whole
  patch fails to compile — that is why the wave boundary is a hard stop, not a preference.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost: every durable byte is, at every instant, protected by a record that names
  it *or* evidenced for reclamation, and every state has an actor that exits it in bounded
  time** (`docs/principles.md:137`, §6 row *Storage lifecycle / reclamation*, sourced to §5
  C-1 at `:109`; the maintainer's standing rule of 2026-07-25; `0016:2802-2813`;
  `crates/custodian/src/gc.rs:22-25`). A drain that can never certify is a state with **no**
  actor that exits it, and a fragment two objects reference is a durable byte whose protection
  must hold under *both* references at every instant. The invariant is restored only when the
  drain pass **completes the move** for a `seg:`-resident fragment and leaves every committed
  reference naming a fragment that exists — not when the refusal is better counted or the
  double-move is argued to be harmless.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; the base this child builds
  on is `main` **after #721 has merged** — see Ordering note.)
- **Depends on:** 721
- **Conflicts with:** 717
- **Ordering note:** **Wave 1 — `Depends on: #721`** is a genuine build-on dependency, not
  a file conflict: this child calls the placement primitive #721 authors in
  `crates/core/src/metadata.rs` and cannot compile without it. The two children share **no
  file** — #721 owns `metadata.rs`, `reconstruction.rs` and the two reconstruction-side
  tests; this child owns `rebalance.rs`, the two rebalance-side tests and
  `dst/tests/custodian.rs`. Under `wave_mode = "merge"` with `[driver].auto_merge = false`
  (INTEGRATION §2) the driver does **not** merge at the wave boundary — it **stops**, the
  human merges #721's PR into `main`, and the run resumes; only then does the
  `origin/main` that `C4-verify` resolves genuinely contain the primitive
  (`run-verify.sh:_resolve_base_ref`). **If #721 is not accepted and merged, hold this
  bundle** — do not rebuild it against a base without the primitive and do not let it absorb
  #721's write path; that re-creates the un-splitting that closed PR #647. Every *external*
  prerequisite is already merged (#710 as PR #718; #695/#696/#697 as PRs #704/#705/#706), so
  no `Depends on (merged):` is needed. **Never share a wave with #717** — it edits
  `crates/dst/tests/custodian.rs`, which is in this child's file set (its substantive DST edit
  against this child's; #717's own brief already declares the reciprocal conflict). **Cite by
  symbol, not by number** — #721 will have moved `reconstruction.rs`'s and `metadata.rs`'s
  line numbers before this child builds.
- **Difficulty:** medium
- **Scope:** **the drain pass completing the placement move for a `seg:`-resident fragment.**
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
- **Repro instruction:** on the target checkout, read the refusal with
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` — `:352-355`
  (`let Some(prior_chunks) = scanned_flat else { refused = true; continue; }`) and the
  binding commit at `:538-556`, which CASes `inode:` and can address no `seg:` record. Then
  seed a committed segmented object (raw `seg:` records + a segmented root, per
  `crates/custodian/tests/segmented_map_rebalance.rs:221-253`), `set_lifecycle` the server
  holding one of its fragments to `Draining`, and run `reconcile_step` with a
  `RebalanceContext`: the evacuation is refused, nothing is written, and the drain is never
  certified — every pass, forever. For the second defect, seed **two** committed objects whose
  maps name the same `ChunkId` and read `plan_evacuations` at `:318-376`: one plan is pushed
  per object, with no dedup between them.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/custodian/tests/segmented_map_evacuate.rs` — a **NEW** file, not
  optional. This project's `C4-verify` earns its red **only** from an *added* `*/tests/*.rs`
  (`run-verify.sh:97-98`); appending to `segmented_map_rebalance.rs` would make the gate take
  the green-only branch (`:454-464`) and prove no red. Confirmed at Plan by dry-running
  `run-verify.sh --classify` over a synthetic patch of this child's exact file set: the sole
  `ADDED_TEST` is this file, so the gate runs `-p wyrd-custodian --test segmented_map_evacuate`
  and reads its cfg gates off **this file only** — which is precisely why the DST property must
  go into the **existing** `crates/dst/tests/custodian.rs`: a new `crates/dst/tests/*.rs` would
  be a second `ADDED_TEST`, would put `--cfg madsim` on the run
  (`run-verify.sh:_crate_cfgs`, `:115-121`), and would change what the gate compiles. The edits
  to `segmented_map_rebalance.rs` and `dst/tests/custodian.rs` ship **in addition** and are
  covered by C4-ci.
- **Verification posture:** the DEFAULT flippable-regression posture holds and is what
  C4-verify measures — legs 1 and 2 are red pre-fix and green post-fix. Declared here only to
  pre-empt a §6 surprise, on two points. **(a)** Leg 3 passes on the base (the pass refuses for
  the other reason), and leg 4 fails on the base **for the wrong reason** — pre-fix no plan is
  built, so its racer never fires. Expect the red leg to report **4 tests ran with 3 failing**;
  only legs 1 and 2 are discriminating evidence, and the gate's summary line counts tests that
  *ran*, not tests that failed, so read the count as a count. **(b)** The DST repoint-versus-supersede property is **not** measured by
  C4-verify at all: `crates/dst/tests/custodian.rs` is `#![cfg(madsim)]`, and because the sole
  added test file is the custodian one, the gate reads its cfgs off that file only and never
  puts `--cfg madsim` on the run — so the DST edit ships applied-but-uncompiled by the
  discriminator and is covered by **C4-ci**, which does sweep it at `MADSIM_TEST_NUM=50`. Both
  the DST property and legs 3–4 are bound by the **mutation** oracle instead, which is why each
  carries a required named negation in `build-notes.md` (delete the precondition, show it go
  red at 50 seeds, restore it) — an assertion that it *would* fail is not the evidence asked
  for. Nothing here is deferred off-Check: no Docker host, no env var, no live CI run.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice — mirror these peers rather than invent a shape:**
  `crates/custodian/src/reconstruction.rs` **as #721 leaves it** — the repair caller's
  conversion to the primitive is the exact pattern this child repeats for the drain, including
  where the ceiling refusal and the orphan puts land; read it first;
  `crates/custodian/src/rebalance.rs:429-556` (`evacuate_chunk`, the binding commit being
  replaced, including the `gc::orphan_key` puts that must stay **in the same batch** as the
  placement change, and the ceiling refusal at `:522-528` the primitive now owns);
  `crates/custodian/src/rebalance.rs:257-381` (`plan_evacuations` — where the shared-`ChunkId`
  gap lives) against `crates/custodian/src/reconstruction.rs:375` and `:528`
  (`sites: HashMap<ChunkId, Site>`, the peer's per-pass dedup);
  `crates/custodian/src/gc.rs:143-147` and `:190-193` (the committed-reference safety gate —
  what actually protects a still-referenced position that carries a stale orphan mark);
  `crates/custodian/tests/segmented_map_rebalance.rs:221-253` (`seed_segmented`: raw `seg:` +
  root seeding) and `:296-311` (`assert_flat_evacuated`, the assertion shape to mirror);
  `crates/dst/tests/custodian.rs` (the existing seeded Tier-0 custodian properties, for the
  shape the new one must match). **Salvage:** the archived parent attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a working
  rebalance caller and a working `RaceAtRepoint` DST double that passed C4-ci — reuse them,
  but (a) correct the doc site claiming the move pins "the exact bytes the resolve read", (b)
  add legs 2 and 4 and re-scope the DST property's doc comment to what it actually proves, and
  (c) drop everything in `metadata.rs`, `reconstruction.rs` and
  `segmented_map_reconstruction.rs`, which belong to #721.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/custodian/src/rebalance.rs`
  → `3829097` (#696, the containment this completes), the drain loop (#145) and its fixes
  (#346 identity-placement fallback; #348 malformed placement). `git -C ../wyrd log origin/main
  -- crates/dst/tests/custodian.rs` → the Tier-0 custodian property set; no repoint property
  exists. No open PR touches these paths. **Closed/rejected:** PR **#647** (CLOSED 2026-07-30,
  unmerged) is the un-split ancestor of the whole #682 family and contained a `repoint`-shaped
  drain write; it was closed for **size and reviewability**, not direction — do not reintroduce
  its custodian-local `crates/custodian/src/resolve.rs`, superseded by the shared resolver
  (#649). The parent bundle's own first attempt (`results/issue_711/iteration-v1/`) is
  rejected-for-size, **not** rejected-for-direction; its rebalance hunk is salvage, its file
  count is the thing that must not recur. Within the harness,
  `results/issue_638/review-rejected.md:15-16` records the standing, four-times-rejected rule
  that a losing/late write is **not** retracted; do not re-litigate it.
- **Disposition hint:** likely-fix
