# Result — issue 681 / passes-read-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The three maintenance passes that walk the committed namespace **themselves** —
  reconstruction, backfill, rebalance — still read the chunk map inline out of the inode record,
  so a **single** segmented object aborts the whole pass for **every** object; and an object whose
  map cannot be read for any other reason ends the walk with an `Err` instead of being contained.
  Seven sites, each re-verified on `origin/main` @ `339da46` at this Plan:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/reconstruction.rs:332` | `assess` (`:317`) | one obligation's assessment, and the pass |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | the binding repair commit |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` (`:620`) | the whole `inode:` scan, for every obligation |
  | `crates/custodian/src/backfill.rs:99` | `reconcile` (`:76`) | the fill scan |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` (`:171`) | the remaining-placement gauge |
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` (`:141`) | the evacuation scan |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` (`:232`) | the binding evacuation commit |

  Each is `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`.
  Four live consequences on the base:
  1. **One segmented object disables repair, backfill and drain for the entire store** — these are
     the passes that *restore redundancy*, so a store that has published a single multipart object
     stops self-healing.
  2. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as "referenced by no committed chunk
     map" → `Assessment::Drain` (`reconstruction.rs:322-325`), deleted in the drain batch
     (`:270-276`). Latent today (the error fires first), but it is the loss this slice must not
     introduce while removing the abort.
  3. **The deployed repair loop is Q namespace scans × N point reads.** `reconcile` calls `assess`
     per obligation (`:185`) and `assess` calls `find_chunk` (`:322`), which scans all of `inode:`
     and decodes every record. Wire the resolver in naively and each of those N objects also costs
     a bounded `seg:` range read — Q×N *resolves*. This is the finding still open at #647's close.
  4. **Containment is not per object.** #650 and #651 contain a damaged record in GC/scrub
     (`gc.rs:360-455`) and in restore/drain (`restore.rs:616-688`); these three passes have no such
     rule — a record that will not `decode` ends the walk at `reconstruction.rs:625`,
     `backfill.rs:80`, `backfill.rs:174`, `rebalance.rs:148`, before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_passes.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step, Custodian,
  FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::backfill::{reconcile, BackfillContext}`,
  `wyrd_custodian::desired_state::set_lifecycle`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, encode, decode, inode_key,
  resolve_chunk_map, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap,
  InodeRecord}`, plus `Custodian::elect` + `FencedZone::new` over
  `wyrd_coordination_mem::MemCoordination` for the fence (`leadership.rs:31`, `:69`; the shape is
  `segmented_map_consumers.rs:406-410`) — over in-memory `MetadataStore` / `ChunkStore` doubles.
  **No `Cargo.toml` change:** every dev-dependency the discriminator needs
  (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
  `tracing-subscriber`) is already declared on `crates/custodian`, verified at this Plan — adding
  one would trip the ADR-0003 dependency audit and is neither needed nor in budget.

  **Six legs. Each leg drives ALL THREE passes over ONE store** — one test per property, never one
  test per pass (that three-fold repetition is what produced the 803-line v2 fixture):

  1. **A segmented object no longer ends any of the three passes, and the flat work in the same
     store still happens.** Store: one healthy segmented object (raw `seg:` records + a segmented
     root, **never** a committer) beside flat objects carrying real work. `reconcile_step` with a
     `ReconstructionContext` **and** a `RebalanceContext` returns `Ok`, and `backfill::reconcile`
     returns `Ok` — today all three return `Err`. Assert **positively**: a queued repair for an
     under-replicated **flat** chunk is assessed and repaired (placement moved, obligation
     drained); a **fillable flat** record (empty `placement`) is filled; a flat fragment on a
     draining server is evacuated. *(binding — base-red)*
  2. **Work in a `seg:` record is refused, never discarded, and the pass does not certify.** Over
     the same shape: a repair enqueued for a chunk living in the segmented record is **still in
     `queued_repairs`** afterwards; a draining server holding that chunk's fragment **still holds
     it**; the `seg:` record's bytes and the root's `version` are **byte-identical**; and each
     pass answers `Reconciled::Blocked` (base-visible, `reconciliation.rs:44`). *(binding —
     base-red)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store so that is a property of the fixture
     and not of luck — (a) a committed root naming a `SegmentRef` whose `seg:` record was never
     written, and (b) a committed record whose own bytes will not `decode`; assert in the fixture
     that `resolve_chunk_map` really errors on (a). Beside them, a healthy flat object carrying
     the same work as leg 1. Assert the conjunction: each pass returns `Ok`, answers `Blocked`
     (never `Satisfied`), the healthy object's repair **and** fill still happen, and both damaged
     objects are **named** on the audit seam by their `inode:` key (`gc::object_name`'s escaping
     shape, `gc.rs:470`). *(binding — base-red)*
  4. **A duplicate committed `ChunkId` is repaired by neither reference, and both objects are
     named.** Two committed objects (and, in the same store, one record carrying the id twice)
     referencing one `ChunkId`, with that chunk enqueued for repair: neither placement changes,
     the obligation is **still queued**, both `inode:` keys are named, and the pass answers
     `Blocked`. Today the base repairs whichever reference `find_chunk` meets first
     (`reconstruction.rs:639`) and drains the obligation. **≤ 40 lines, reusing the shared
     fixture.** *(binding — base-red; this is the v2 T5 gap)*
  5. **Reconstruction reads the namespace once per pass — O(N), not O(Q×N).** With **Q ≥ 3**
     queued obligations over **N ≥ 3** committed flat objects on the counted double: exactly
     **one** `scan(b"inode:")`, independent of Q (the base does Q of them), and the repairs still
     land. Then, on a store holding S segmented objects, the `seg:` range reads are **≤ S**.
     Build this leg with a `ReconstructionContext` and **no** GC / scrub / rebalance context
     beside it: the other loops walk `inode:` themselves and sharing one walk across passes is a
     much larger refactor that is **out of scope** — a store-wide scan count would demand it by
     the back door. *(binding — base-red)*
  6. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a non-`ChunkMapError` error makes each pass return `Err`. *(NOT base-red — this
     one guards against over-containment; it passes before and after)*

  **Every pinned decision below is bound by a leg — as a sub-assertion of an existing leg, NOT as
  a seventh test** (this mapping is the completeness check; an unbound decision is one a rebuild
  or a reviewer will relitigate):

  | pinned decision | bound by | how, in ≤ ~25 lines |
  |---|---|---|
  | 1 duplicate `ChunkId` | **leg 4** | its own leg (above) |
  | 2 empty queue ⇒ `Satisfied` | **leg 3** | over the store that already holds an unreadable object, run reconstruction with **no** obligations queued: `Satisfied`, and the counted double records **zero** `scan(b"inode:")` |
  | 3 non-canonical `inode:` key | **leg 3** | seed a committed, fillable record under `inode:007` beside `inode:7`: after the pass, `inode:7` is byte-unchanged, `inode:007` is either filled in place or left untouched, and the pass does not answer `Satisfied` if it left work undone |
  | 4 CAS on the generation read | **none — by design** | unreachable in this slice: only a segmented resolve can restart, and every segmented write is refused. See decision 4; do **not** build a leg (or a DST case) for a path this slice cannot execute |
  | 5 one refusal line per object | **leg 2** | over a segmented object of ≥ 3 chunks with ≥ 2 draining fragments, the captured audit stream carries **exactly one** rebalance refusal line for that object |
  | 6 attribution before the work | **leg 6** | with the store faulting **after** the reading, the unreadable object's name is **already** on the audit seam even though the pass returns `Err` |

  **Not in the discriminator, covered by the gating `C4-ci`:** positive matches on any variant or
  field this patch introduces. The existing per-pass suites
  `crates/custodian/tests/{reconstruction,backfill,rebalance}.rs` must stay green **unmodified** —
  v2 achieved that with these same production changes, so a need to edit one is a signal that an
  answer changed further than this slice intends, not a licence to edit it.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches, and M4's integration branch is merged and deleted. #648–#652 all landed on
  `main` directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- Scope (one logical fix) / out of scope: the three passes that scan `inode:` **resolve every committed object the way every
  other consumer already resolves one, contain per object what they cannot read, and refuse —
  rather than abort or silently discard — the work this slice does not own.**
  - `reconstruction.rs` — the pass reads each committed object's map **once per pass** and
    assesses its queued obligations against that reading. A chunk whose `ChunkRef` lives in a
    `seg:` record is **refused, not drained**: the obligation stays queued and the pass does not
    certify. A chunk in a flat record is repaired exactly as today, with every existing
    classification (`Repairable` / `Drain` / `Unreachable` / `Blocked` / `Unrepairable` /
    `Malformed`) and its gauge accounting preserved, including the rule that a never-repaired
    condition stays **off** the repairable-backlog gauge.
  - `backfill.rs` — a segmented record is left **byte-identical**, declined with a stated reason
    on the audit seam and counted, while a fillable **flat** record in the same store is still
    filled in the same pass. The remaining-placement gauge stays correct over a store containing
    segmented objects.
  - `rebalance.rs` — the evacuation scan resolves per object; a fragment whose chunk lives in a
    `seg:` record **stays on the draining server**, refused, and the pass does not report the
    drain satisfied. A flat chunk is evacuated exactly as today.
  - All three: a record that will not `decode`, and a generation the resolver cannot read, are
    **contained per object** — named on the audit seam, the walk continues, the pass answers
    non-certifying. A fault that is not a `ChunkMapError` still propagates.

  **Plan decisions, pinned — settled here so no Do round and no review round relitigates them.**
  The first two are the open judgment calls the 2026-08-05 sign-off referred to Plan:
  1. **A duplicate committed `ChunkId` is ambiguous: neither reference is repaired, the obligation
     stays queued, both objects are named, the pass does not certify — and the rule is the same
     whether the duplicates sit in one record or two.** An obligation is keyed by chunk alone
     (`repair:<chunk_id>`, `repair.rs:32`) and both references address the same
     `FragmentId{chunk,index}`, so repairing "the first in scan order" repoints one record while
     orphan-marking a fragment the other still points at — which GC reclaims after its grace
     window. Ids are allocator-minted (`write.rs:170`), never content-addressed, so a duplicate is
     always an anomaly, never legitimate dedup. This is the narrow rule ONLY: no cross-object
     claim-counting apparatus, no new report schema, no `ambiguous-*` verdict surface.
  2. **A pass certifies only over the reading it performed. Reconstruction with an EMPTY repair
     queue performs no namespace reading and answers `Satisfied`** — it makes no claim about
     objects it never read. When the queue is non-empty the pass DOES read the namespace, and an
     object it cannot read then makes that reading incomplete → `Blocked`, because "drain this
     obligation as unreferenced" is only knowable over a complete reading. Precedent in this very
     file family: `rebalance.rs:115-117` already returns `Satisfied` without reading `inode:` when
     no server is draining. Walking N objects and resolving every segmented one purely to look for
     damage the pass will not act on is the Q×N-shaped waste this slice exists to remove — and GC
     and scrub already answer `Blocked` over the same store every pass (#650).
  3. **A record is read, written and named under exactly the key the store gave it.** A key that
     is not the canonical spelling of its id is never silently reinterpreted into one. On the base
     all three passes parse the scanned key to an `InodeId` and then re-derive
     `metadata::inode_key(id)` for the CAS (`backfill.rs:142`, `rebalance.rs:310`,
     `reconstruction.rs:598`); `"inode:007"` and `"inode:+3"` both parse (`u64::from_str` accepts
     a leading `+` and leading zeros), so the pass reads one record and CASes another — a lost
     conflict reported as `Satisfied` at best, and a clobber of an unrelated object at worst when
     the two records' bytes coincide. `gc.rs` is the precedent: it resolves against `&key` and
     keys its attribution by the raw bytes, deliberately (`gc.rs:280-294`, `:402`). *(closes three
     of the four carried-forward review findings)*
  4. **A write is CAS'd on, and framed by, the generation its chunks were read from** — the
     precondition record AND the preserved fields (`size`, `ObjectMeta`, `version`) come from
     `ResolvedChunkMap::record`, never from a scan snapshot the resolve moved off
     (`metadata.rs:2256-2272`). **Checked at this Plan and stated as a constraint, not a leg,
     because it is unreachable by construction in this slice:** only a *segmented* resolve can
     restart — a flat map returns `Answer(Cow::Borrowed(chunks))` with no store read and no
     supersede check at all (`metadata.rs:2584-2586`) — and every segmented write here is refused.
     So every commit this slice performs is framed by the scan snapshot exactly as today. The rule
     is written down so a naive wiring (resolve, then commit the restarted generation's chunks
     into the snapshot's record) is recognised as the bug it would be; **no test leg binds it, and
     none can until #682 builds the segmented write path.**
     Corollary, and the reason a stale reading is not a permanent state: an object whose *shape*
     changed under the scan (flat→segmented or the reverse) is refused or conflicts on this pass
     and re-assessed on the next, because the obligation stays queued — bounded, per C-1.
  5. **A refusal is reported once per object, not once per chunk.** Rebalance's per-chunk refusal
     logging floods the audit seam; match backfill's per-object decline. *(carried-forward
     finding)*
  6. **Attribution for an object the pass could not read is emitted where the object is read,
     before the work loop** — mirroring `gc.rs:164-166` — so a later transient store fault cannot
     cost the operator the name of the record to repair. *(carried-forward finding)*

  **Constraints (they bound the shape; they do not name it):**
  - **Bounded memory.** A pass may retain work proportional to the **obligations it holds** and to
    **one object at a time** — never the whole namespace's decoded chunk lists, never any
    segment's exact bytes, never a per-chunk deep copy of a segmented root. This slice writes no
    segmented record, so it needs to pin nothing; pinning exact bytes is #682's.
  - **Bounded work.** Backfill's remaining-placement gauge must not cost a **second resolving
    walk** of the namespace: on a store of segmented objects that doubles every `seg:` range read
    for a number the pass has already seen. One resolving reading per pass.
  - **Containment on *any* read fault**, not just a segmented shape — a serde decode failure on a
    concurrently-replaced root contains that object and does not abort the walk.

  **/ out of scope:**
  - **Any write to a segmented record.** `repoint_chunk`, the record ceilings, and the
    repair/evacuation write path for a `seg:`-resident chunk are **#682**. A refusal here writes
    **nothing at all**.
  - Restore and `desired_state` (**#651**, merged): `restore.rs` / `desired_state.rs` are not
    edited. The deferral marker at `restore.rs:616` names this slice — **leave it**; do not
    refactor restore into the new walk (its per-reference granularity differs, as that comment
    says).
  - `gc.rs` / `scrub.rs` (**#650**, merged) — untouched. Sharing ONE namespace walk across all
    loops is a separate refactor and is not in this slice.
  - The chunk-id floor (**#652**, merged); the committer, fence, rollback and resume (**#653**).
  - The pre-existing question of whether an ordinary `EvacOutcome::Aborted` (no free domain, a
    missing fragment) should certify — that is #682's to settle. This slice makes only the refusal
    **it introduces** non-certifying.
  - **No docs edit.** Checked at this Plan: `docs/design/architecture/06-runtime-view.md` §6.2
    already states the containment rule fleet-wide, and its clause about "a consumer that has not
    yet adopted it refuses a segmented map outright" stays true after this slice
    (`commit_chunk_map`, `read.rs:96` and `high_water_marks` still refuse). §6.3 states nothing
    this changes. This slice alters no port, API operation, RPC, CLI flag or persisted field, so
    the AGENTS.md docs-currency merge requirement is not triggered. Do **not** add a doc hunk.
  - Any new or edited ADR / spec / proposal; any conformance-vector change.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 82 mutants tested in 74s: 45 caught, 37 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #681: make reconstruction, backfill, and rebalance resolve segmented committed maps, contain per-object read failures, and preserve/refuse work without aborting the store-wide pass.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Choose whether an incomplete namespace read may still mutate known flat objects—the binding unreadable-object leg requires progress (`crates/custodian/tests/segmented_map_passes.rs:588`), while the iteration-6 carry-forward requires those mutations stopped; the choice changes availability versus hidden-alias safety. |
| C2 Reproduction (red pre-fix) | PASS | On base `339da46`, the added six-test discriminator compiled and all six failed behaviorally at assertions such as the segmented-pass expectation (`crates/custodian/tests/segmented_map_passes.rs:497`), establishing a non-vacuous red. |
| C3 Change | PASS | The change stays on the declared three consumer loops and one new discriminator, with resolver/containment entry points in reconstruction, backfill, and rebalance (`crates/custodian/src/reconstruction.rs:766`, `crates/custodian/src/backfill.rs:84`, `crates/custodian/src/rebalance.rs:219`). |
| C4 Verification (red→green) | PASS | The same six tests pass with the patch (`crates/custodian/tests/segmented_map_passes.rs:490`), and independent fmt, clippy, build, workspace tests, docs, dependency audits, conformance, statics, and DST all passed; the initial read-only Cargo advisory lock was eliminated with a writable scratch `CARGO_HOME`. |
| C5 Causal adequacy | PASS | The patch replaces inline flat-map assumptions with the shared resolver and typed per-object containment rather than adding a capability probe or symptom guard (`crates/custodian/src/reconstruction.rs:796`, `crates/custodian/src/backfill.rs:111`, `crates/custodian/src/rebalance.rs:245`); all 82 diff mutants were caught or unviable. |
| T1 Structure | PASS | Exactly the planned four files change—three pass modules plus the new six-leg test—and no manifest, frozen design document, API, or persisted-field surface is added (`crates/custodian/tests/segmented_map_passes.rs:1`). |
| T2 Shape | FAIL | Trim the discriminator or explicitly re-plan its allocation—the file is 498 semantic added lines against its 470-line cap, although its 732 raw lines and the patch's 878 semantic total remain within the global limits (`crates/custodian/tests/segmented_map_passes.rs:1`). |
| T3 Runtime | PASS | The in-memory seam exercises all six runtime properties, including positive work and non-object store-fault propagation (`crates/custodian/tests/segmented_map_passes.rs:490`, `crates/custodian/tests/segmented_map_passes.rs:716`), and the brief requires no live backend or container dependency. |
| T4 Contribution | NEEDS-HUMAN | Resolve the two reported batched-review blockers—the unavailable `scripts/review-branch` wrapper makes that red row provisional, while source inspection localizes the contested behavior to mutation after an incomplete scan (`crates/custodian/src/reconstruction.rs:289`, `crates/custodian/src/rebalance.rs:132`); the affected-path history check otherwise found closed #647 and no open PR. |
| T5 Judgment | NEEDS-HUMAN | Decide whether GC's complete-reference backstop is sufficient to permit known flat repairs and evacuations after unreadable records—the current policy preserves progress (`crates/custodian/src/reconstruction.rs:289`, `crates/custodian/src/rebalance.rs:132`), while rejecting it avoids mutation under a potentially hidden duplicate reference. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether operational fitness tolerates segmented repair/evacuation work remaining queued and on draining servers until #682—the discriminator proves explicit refusal and non-certification, not completion (`crates/custodian/tests/segmented_map_passes.rs:510`). |

### Advisory — adversary

# Adversarial review — issue #681 (advisory, non-gating)

Re-ran the asserted red→green on a throwaway copy of `$PDCA_TARGET` (scratch dir removed).
**Red is real:** with only `crates/custodian/src/{backfill,rebalance,reconstruction}.rs` reverted to
`origin/main` @ `339da46` and the new test file kept, all 6 legs *compile* and fail behaviourally
(`find_chunk met a segmented chunk map…`, `key must be a string`, `left: Satisfied right: Blocked`) —
not a compile error, not a degraded UNVERIFIABLE. **Green is real:** 6/6 pass with the patch, and the
legs drive the production entry points (`reconcile_step` through `Custodian::elect`/`FencedZone`, and
`backfill::reconcile`), not a re-implementation. The `C4-verify` row is therefore not refuted.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:196` — the over-containment control for
  rebalance is load-bearing and bound by nothing.** `Refusals::refuse` returns without counting when
  `fragments + unreadable == 0`; that guard is the only thing stopping a *healthy* segmented object
  from blocking a drain it has no stake in. Concrete failing case, run and measured: seed one
  segmented object whose chunks are placed entirely off the draining server
  (`seed_seg(SEG, NONCE, 2, [(S_DRAIN_A,[0,1,4],…),(S_DRAIN_B,[0,1,4],…)])`) plus a fully-placed flat
  object, mark server 3 draining, and run `step(false, true)`. With the guard present the pass
  answers `Satisfied` (correct). Replace the guard body with a no-op and the pass answers **`Blocked`
  — and all six legs in `crates/custodian/tests/segmented_map_passes.rs` still pass, and the entire
  `cargo test -p wyrd-custodian` suite still passes** (verified: 11 green binaries, 0 failures). That
  is the *common* production shape — most objects hold nothing on the one draining server — so the
  unguarded behaviour would mean no decommission ever certifies on any store that has published a
  multipart object, i.e. exactly the class of defect this slice exists to remove, in mirror image.
  The brief builds this control explicitly for reconstruction
  (`crates/custodian/tests/segmented_map_passes.rs:698`, *"the control against OVER-containment …
  this store's answer is the certifying one"*) and for the store-fault path (leg 6), but leg 2 only
  ever seeds a segmented object that *does* hold draining fragments
  (`crates/custodian/tests/segmented_map_passes.rs:512`, `:562`), so rebalance's certifying answer
  over a segmented object is never asserted. Fix is one sub-assertion in leg 1 or leg 5 (a
  `step(false, true)` over a segmented object with nothing on the draining server, asserting the
  answer is not `Blocked`). The same hole exists in backfill for a segmented record with no empty
  placement (`crates/custodian/src/backfill.rs:142`/`:156`), though there the decline is structurally
  gated by `to_fill.is_empty()` and a mutation of it is caught incidentally by leg 2's gauge
  assertion.

- **The `C5-mutants` row is weaker evidence than its "45 caught, 37 unviable, 0 missed" reads.**
  `cargo mutants` mutates operators and replaces function bodies; it does not express *"delete this
  early-return guard"*. For `rebalance.rs:196` every mutant it can generate is caught (`==`→`!=` and
  `+`→`*` both flip leg 2 to non-`Blocked`; `+`→`-` panics on `0usize - 1`; the whole-body mutant
  kills leg 2), while the one change that actually matters survives the whole suite. A reviewer
  reading `0 missed` as "the refusal predicates are pinned" would be rationalising; it pins the
  arithmetic, not the predicate.

## Attempted and could not refute

- **Pinned decision 4 (a write is CAS'd on, and framed by, the generation its chunks were read
  from).** Tried to reach a commit framed by a *restarted* resolve. Cannot: all three passes branch
  on the **snapshot's** shape (`backfill.rs:156`, `rebalance.rs:265`, `reconstruction.rs:825`) and a
  flat snapshot returns `Resolution::Answer(Cow::Borrowed(chunks))` with no store read and no
  supersede check (`crates/core/src/metadata.rs:2585-2586`), so `resolved.chunks` is provably the
  snapshot's own list on every path that writes. A segmented snapshot that restarted onto a live
  *flat* root is (conservatively) declined/refused, not written.
- **Two obligations inside one record now sharing one stale `prior_bytes`.** Suspected the single
  per-pass index would lose the second repair where the base's per-obligation `find_chunk` would not.
  Built the case and ran it on base and on the patch with a canonically-encoded record: both answer
  `Changed`, both repair chunk 0 and leave chunk 1 queued on a lost CAS (`reconstruction.rs:663`).
  Identical — not a regression.
- **`unparsable-inode-key` as a new permanent `Blocked` state** (`backfill.rs:105`,
  `rebalance.rs:239`, `reconstruction.rs:790`), which `gc::referenced_fragments` — the containment
  rule the brief mandates copying — does *not* have. Measured: a store with one `inode:-1` row makes
  all three passes answer `Blocked` forever and withholds every drain. But `metadata::inode_key` is
  the sole writer of the prefix (grep across `crates/`), and #652's `high_water_marks`
  (`crates/core/src/metadata.rs:2158-2172`) already names-and-continues on exactly this row with
  exactly this reasoning, so the divergence has repo precedent and is strictly better than the base's
  silent skip.
- **Draining an obligation on an incomplete reading.** `assess` gates `Drain` on
  `index.unaccounted == 0` (`reconstruction.rs:396`) and `Ok(None)` from the resolver is the resolver's
  own "no live committed generation" (`crates/core/src/metadata.rs:2630`), which the base treated the
  same way. Could not construct a store where a live reference is drained.
- **`Reconciled::Blocked` leaking into an operator-visible "decommission is safe" answer.** Grepped
  every consumer: outside tests nothing reads these three loops' `Reconciled`, and the drain-status
  surface (`desired_state::reconciliation_status`, `crates/custodian/src/desired_state.rs:181-246`) is
  computed from `gc::referenced_fragments`, which already resolves segmented maps and already answers
  `Pending`. No fitness gap there.
- **Test tautology / over-broad assertions.** `assert_gauge`
  (`crates/custodian/tests/segmented_map_passes.rs:190`) parses the full digit run and requires
  exactly one sample, so a 10× over-report or a duplicate emission fails; `assert_seam` searches the
  quoted key so `"inode:0"` cannot match `"inode:006"`; the CAS-key and CAS-bytes rules are bound
  indirectly but soundly by `assert_flat_work_done` (`:475`) — re-deriving `metadata::inode_key(id)`
  or re-encoding the record loses the CAS against the fixture's deliberately non-canonical `stored()`
  spelling (`:272`), and the "the work happened" assertions then fail.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Choose whether an incomplete namespace read may still mutate known flat objects—the binding unreadable-object leg requires progress (`crates/custodian/tests/segmented_map_passes.rs:588`), while the iteration-6 carry-forward requires those mutations stopped; the choice changes availability versus hidden-alias safety.
- [ ] T4 Contribution — Resolve the two reported batched-review blockers—the unavailable `scripts/review-branch` wrapper makes that red row provisional, while source inspection localizes the contested behavior to mutation after an incomplete scan (`crates/custodian/src/reconstruction.rs:289`, `crates/custodian/src/rebalance.rs:132`); the affected-path history check otherwise found closed #647 and no open PR.
- [ ] T5 Judgment — Decide whether GC's complete-reference backstop is sufficient to permit known flat repairs and evacuations after unreadable records—the current policy preserves progress (`crates/custodian/src/reconstruction.rs:289`, `crates/custodian/src/rebalance.rs:132`), while rejecting it avoids mutation under a potentially hidden duplicate reference.
- [ ] Validation — fitness-to-purpose — Decide whether operational fitness tolerates segmented repair/evacuation work remaining queued and on draining servers until #682—the discriminator proves explicit refusal and non-certification, not completion (`crates/custodian/tests/segmented_map_passes.rs:510`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- [ ] size backstop — this slice is behaving oversized: 4 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
- [ ] **The brief's decision-4 "unreachable by construction" claim is false, and

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): The slicing is the problem, not the implementation. The size backstop's `iterate-plan` recommendation was overridden at v5 and again at v6, both times reasoning the findings were implementation-shaped and the process deserved more room to converge. That experiment has now been run twice on this slice and has not converged: round 4 arrives with 2 fresh T4 blockers, the test file over its cap again (498 semantic vs 470), and a load-bearing guard bound by no test. Totals: 4 sign-off rounds, 7 builder attempts, 3 auto-iterates, 1 prior replan, ~102 KB patch. Re-split at Plan rather than a fifth Do round. Suggested split axis for Plan to author (a hint, not a mandate — the brief bundles three separable kinds of work over the same three files): 1. read through `resolve_chunk_map` + refuse segmented work without discarding it; 2. per-object containment on ANY read fault (unreadable record, unresolvable generation, non-canonical key), the rule copied from gc/scrub; 3. the Q x N -> O(N) restructure of reconstruction's namespace reading. Each round's findings have clustered by which of these three it touched, which is the signal the split should follow. Carry forward into the split — every one of these is unresolved, none is a reason to re-litigate the pinned decisions: - **`rebalance.rs:196` over-containment guard is bound by nothing.** Adversary replaced its body with a no-op: all six new legs AND the whole `wyrd-custodian` suite still passed, while the pass flipped from `Satisfied` to `Blocked` over a healthy segmented object with nothing on the draining server — i.e. no decommission would ever certify on a store holding a multipart object, this slice's own defect in mirror image. Needs a binding sub-assertion (a `step(false, true)` over a segmented object holding nothing on the draining server). Note the C5 `0 missed` row does NOT cover this: mutants pin the arithmetic, not the predicate. - **Two T4 blockers, same theme:** a resolver restart can leave `chunk_index` describing the live generation while `prior_bytes` holds the scanned one, so unchecked indexing can panic before the stale CAS rejects the plan (`rebalance.rs:412`, `reconstruction.rs:659`). - **The decision-4 "unreachable by construction" question is still unsettled after three rounds.** Round 4's adversary demonstrated it false with a working double (a real `version 9 -> 10` commit framed by a generation the pass never scanned); rounds 5 and 6 re-traced and could not find a reachable path; this round's adversary also could not refute it. The two T4 blockers above are the same theme resurfacing. Plan must settle this explicitly rather than pin it again. **Read the FULL finding at `iteration-v4/check-advisory-adversary.md:25`, not the deferred-findings stub — the ledger truncated it mid-sentence.** - **T2 Shape:** the discriminator is 498 semantic added lines against a 470 cap. If the split lands, re-allocate the budget per child rather than trimming. - **C4-ci flaked** (failed, then passed its once-only confirm re-run); what interfered was never established across two rounds that promised to. Not in dispute, and should NOT be rebuilt from scratch by the children: the correctness core is sound and independently confirmed — red->green is real (6 legs fail behaviourally on base `339da46`, pass with the patch, driving production entry points), 82 mutants with no survivors, and the adversary explicitly could not refute the CAS framing, the drain-on-incomplete-reading path, or the test's assertion strength. Carry the existing discriminator legs into the children. Process note: the deferred-findings ledger has been truncating multi-line findings at the first newline since round 4, so three consecutive rounds asked the human to clear §6 items that are unreadable as written. See §10.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Driver drops sign-off decisions: `signoff-decision` is written durably by the leaf but consumed only in-process by `_apply_decision` (flow.py:727-736), so an interrupted/standalone session orphans it (seen on issue_691: valid `iterate-do` from 12:43, §9 never recorded, re-queued to a human 8h later against a stale SUMMARY); next pass neither applies nor surfaces it, `autoiterate.write_decision` (autoiterate.py:527) would silently clobber it, and a session that writes nothing lets the driver adopt the stale token as this pass's §9 — treat a pre-existing decision as un-consumed input (apply or quarantine loudly), name it in the batch prompt, and stop `write_decision` overwriting a decision it did not author.
- Deferred-findings ledger truncates multi-line findings at the first newline: issue_681 carried three unreadable §6/§9 items across rounds 4-6 (`**The brief's decision-4 "unreachable by construction" claim is false, and`, `**The refusal vocabulary this patch introduces is asserted by nothing; the`, `**Two comments assert a byte-exactness property the code does not`), so the human was asked to clear items they cannot read and each rebuild carried forward a stub instead of the finding — the decision-4 one was substantive (a demonstrated `version 9 -> 10` commit framed by an unscanned generation) and its full text survived only in `iteration-v4/check-advisory-adversary.md:25`; store the whole finding, or a pointer to its source artifact, and never a first-line prefix.
