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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — PASS on confirm — first run failed transiently: xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit stat
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 66 mutants tested in 72s: 11 missed, 26 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #681: make reconstruction, backfill, and rebalance resolve segmented committed maps, contain per-object map failures, and refuse unsafe work without losing healthy maintenance progress.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required safety outcomes, duplicate-id policy, empty-queue meaning, raw-key identity, complexity bound, and file/line budgets are pinned against the three pass entries (`crates/custodian/src/reconstruction.rs:149`, `crates/custodian/src/backfill.rs:76`, `crates/custodian/src/rebalance.rs:120`). |
| C2 Reproduction (red pre-fix) | PASS | A fresh base checkout with only the added discriminator compiled and failed behaviorally 0/6, including the measured Q=3 versus one-scan assertion at `crates/custodian/tests/segmented_map_passes.rs:737`. |
| C3 Change | FAIL | A malformed resolved placement is warned about and skipped before `scan.refused` is increased, so rebalance can return `Satisfied` despite being unable to establish that a draining fragment is safe to decommission (`crates/custodian/src/rebalance.rs:259`, `crates/custodian/src/rebalance.rs:146`). |
| C4 Verification (red→green) | PASS | The same checkout passed all 6 discriminator tests after applying production hunks, and fmt/clippy/build/workspace tests, docs, dependency walls, conformance, statics, and DST all reran green; the initial read-only advisory-lock fault was discharged with a writable scratch `CARGO_HOME` (`crates/custodian/tests/segmented_map_passes.rs:528`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Strengthen independent fault/gauge assertions — 12/66 rerun mutants survived because leg 3 combines both unreadable shapes and checks only aggregate blocking, while removal of the required remaining gauge is also undetected (`crates/custodian/tests/segmented_map_passes.rs:601`, `crates/custodian/tests/segmented_map_passes.rs:613`, `crates/custodian/src/backfill.rs:283`). |
| T1 Structure | PASS | The change stays on existing trait seams, uses the shared resolver in each pass, and the new crate root forbids unsafe code (`crates/custodian/src/backfill.rs:106`, `crates/custodian/src/rebalance.rs:219`, `crates/custodian/src/reconstruction.rs:819`, `crates/custodian/tests/segmented_map_passes.rs:24`). |
| T2 Shape | FAIL | Reduce the discriminator to its ≤470-semantic-line allocation and the patch to ≤880 total — independent nonblank/noncomment counts are 528 and 895 respectively, despite the test meeting its 775/780 raw-line cap (`crates/custodian/tests/segmented_map_passes.rs:1`). |
| T3 Runtime | FAIL | Choose a bounded plan representation — every hit object is retained across the whole scan as both an `InodeRecord` and a copied `Arc<[ChunkRef]>`, so worst-case memory holds the namespace's decoded maps rather than obligations plus one object (`crates/custodian/src/reconstruction.rs:867`, `crates/custodian/src/rebalance.rs:297`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the user-impact opener and tracker remain present — `scripts/pdca` and the contribution artifacts were not supplied, so its green row cannot be independently reproduced; the separate affected-path history audit did find closed #647 and no open competing PR. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild the non-certifying malformed path, bounded retained state, and mutation-sensitive discriminator before sign-off — each is an implementation correction against already-pinned safety and scale decisions (`crates/custodian/src/rebalance.rs:259`, `crates/custodian/src/reconstruction.rs:868`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether corrected in-memory seam evidence is sufficient for this data-loss-sensitive maintenance release — the brief deliberately excludes a real backend/topology exercise, so that acceptance determines production confidence. |

### Advisory — adversary

# Adversarial review — issue #681 (`passes-read-through-resolver-contained`)

Advisory only; I never gate. Everything below was executed against the target working tree at
`/home/eddie/wyrd/wyrd.pdca-wt-l0` (patch applied) and against a scratch copy with the three
production files reverted to `HEAD` (`339da46`). Scratch removed before finishing.

## What I could NOT refute (stated first, because it is the honest half)

- **The red→green is real and behavioural.** I re-ran it myself: with `crates/custodian/src/{reconstruction,backfill,rebalance}.rs` reverted to `HEAD` and `crates/custodian/tests/segmented_map_passes.rs` kept, the file **compiles** and **6/6 tests fail on assertions/`Err`**, not on a missing symbol (`reconstruction::find_chunk met a segmented chunk map…`, `left: Changed / right: Blocked`, `read ONCE…: left: 3 right: 1`). With the patch, 6/6 pass. C4-verify's claim survives attack.
- **Leg 5 genuinely binds the O(N) property**, contrary to what I expected. The two segmented objects sit *after* the flat ones in `BTreeMap` order (`segmented_map_passes.rs:722,728`), so the base's `find_chunk` returns before meeting them and the pass does **not** `Err`; the red at `segmented_map_passes.rs:737` is the Q×N count (3 vs 1) itself. That is the strongest leg in the file.
- **The production path is exercised**, not mirrored: every leg drives `wyrd_custodian::reconcile_step` through the real fence (`segmented_map_passes.rs:465`) and `wyrd_custodian::backfill::reconcile` (`:473`); the doubles are trait impls, not re-implementations of the passes.
- I attempted and failed to construct a data-loss case from: (a) a repair landing while `index.unreadable > 0` and the duplicate hiding in the unreadable record — `gc.rs:155-165`/`ReferenceSet::protects` withholds every fragment while the set is incomplete, so the displaced fragments are not reclaimed; (b) the "assess all, then repair" ordering making a second obligation in the same record lose its CAS — the **base** has the same ordering (`find_chunk` runs in the assess loop, before any `repair_chunk`), so it is not a regression; (c) a flat snapshot superseded mid-resolve — `resolve_snapshot` returns `Answer(Cow::Borrowed)` for a flat map with no store read (`crates/core/src/metadata.rs:2584-2586`), so decision 4 holds by construction as the brief claims.
- I also ran `cargo test --workspace --exclude wyrd-dst` **three times** clean, so I cannot reproduce the C4-ci first-attempt failure recorded as `flaky: true`.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:153` (and `:283`, `emit_remaining`): the rewritten remaining-placement gauge is bound by nothing.** The patch replaces the old post-pass `scan(b"inode:")` recount with in-walk accumulation, and `remaining += to_fill.len() as u64` is the only thing that keeps a **declined segmented** record's empty placements on the operator's drain-to-zero gauge — the exact property the brief's §Scope pins ("The remaining-placement gauge stays correct over a store containing segmented objects"). **Concrete failing case, executed:** change `:153` to `remaining += 0;` and the **entire** `wyrd-custodian` suite — including all six new legs and `tests/backfill_telemetry.rs:207` — stays green. A store whose only empty placements live in segmented objects would then publish `gauge.backfill_placement_remaining = 0` while the population is non-zero, i.e. exactly the "watch the pre-M3 population drain to zero" signal reading zero on a store that has not drained. No leg reads the gauge at all.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:308` (`emit_declined`) and `:270` (`DECLINED_SEGMENTED`): the declined-fill audit line and its counter are dead to the test suite.** The brief's §Scope requires the segmented record be "declined with a stated reason on the audit seam **and counted**". **Concrete failing case, executed:** make `emit_declined` a no-op (`{ let _ = (object, reason, chunks); return; }`) and the whole `wyrd-custodian` suite is green — the `backfill_declined_records` counter and the `action="declined"` line can be deleted outright without a single test noticing. Leg 2 only counts `"action":"refused"` lines on the **rebalance** seam (`segmented_map_passes.rs:584-588`); no leg looks at backfill's decline.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_passes.rs:224` + `:634`: `assert_named` is a bare substring match, and leg 3's assertion for the undecodable record is satisfied by an unrelated object.** `assert_named` does `line.contains(object)`; leg 3 asks for `"inode:2"` while the same store holds the healthy segmented object `"inode:20"` (`:251`, `:253`), which every pass names on its own seam anyway (backfill's decline, rebalance's refusal). **Concrete failing case, executed:** delete the `emit_unresolvable(&crate::gc::object_name(&key), …)` call from backfill's decode-failure arm (`crates/custodian/src/backfill.rs:96`) — leg 3 **still passes**; only leg 6 catches it. The "both damaged objects are named" conjunction the brief made binding for leg 3 is therefore not actually asserted for backfill. Fix is cheap: assert on the JSON field (`"inode":"inode:2"`) rather than a substring of the line, or rename the fixture keys so no key is a prefix of another.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:409`, `:435`, `:1040`: the whole new refusal vocabulary is verified by nothing, and the brief's stated cover for it does not exist.** `REFUSED_SEGMENTED` / `REFUSED_AMBIGUOUS` / `REFUSED_INCOMPLETE`, `rebalance.rs:498`'s `REFUSED_SEGMENTED`, and the six new counters (`{backfill,rebalance,reconstruction}_unresolvable_records`, `backfill_declined_records`, `rebalance_evacuation_refused`, `reconstruction_repair_refused`) appear **zero** times in any test in the repo (`grep` over `crates/{custodian,dst,server}/tests`). The brief parks this at "*Not in the discriminator, covered by the gating `C4-ci`: positive matches on any variant or field this patch introduces*" — but `C4-ci` runs the **pre-existing** per-pass suites, which the same paragraph forbids editing and which know nothing about these fields. So the claim that C4-ci covers them is unwarranted: nothing covers them. This is also the likely substance behind the red advisory `C5-mutants` row, which the brief itself pre-declared as a real signal ("*v2 recorded 0 survivors after the rebuild, so a survivor here is a real signal about the compressed legs, not noise*") — it reports **11 missed**.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:269-272` + `:433-437`: `REFUSED_INCOMPLETE` emits one unattributed `WARN` per obligation, per pass, forever — the flood pinned decision 5 exists to forbid.** Once any single committed record in the store fails to decode or resolve, `index.unreadable > 0` turns **every** obligation the reading did not find from `Drain` into `Assessment::Refused(Refusal { reason: REFUSED_INCOMPLETE, objects: Vec::new() })`, and `:271` emits one `tracing::warn!` line for each — with `objects=[]`, i.e. naming nothing an operator can act on. **Concrete case:** one undecodable `inode:` record plus Q obligations for genuinely deleted chunks ⇒ Q attribution-free WARN lines on `wyrd.custodian.reconstruction.audit` on *every* pass, indefinitely (the obligations can never drain until the record is repaired). Rebalance was explicitly restructured to one line per object for exactly this reason (`rebalance.rs:498`, decision 5); reconstruction's incomplete-reading arm reintroduces the per-item flood in the one place where the line carries no attribution at all. A single summary line ("N obligations withheld: reading incomplete") would carry the same information.

- **NEEDS-HUMAN [human] — the Plan's own line budget is exceeded on the test file, on the counting convention the Plan itself used.** Brief §Budget: `tests/segmented_map_passes.rs` ≤ **470 semantic** / 780 raw, total ≤ **880 semantic**. Measured on `patch.diff` with "non-blank, non-comment" added lines: test file **528**, total **895** (production is inside its allocations: 96/92/179 vs 100/100/210). That convention is the Plan's own — it reproduces v2's published ratios exactly (v2: 1,185 raw → 803 semantic = 0.678; here 775 raw → 528 = 0.681; `reconstruction.rs` v2 373→192 = 0.515, here 350→179 = 0.511). Raw is inside the hard stop (775 ≤ 780), so this is not the "STOP and hand back" trigger — but this bundle has been returned **twice for size / T2 shape**, and whether +12% over the test-file allocation is acceptable is a scope call the human owns, not one I should decide.

- **NEEDS-HUMAN [human] — `check-gates.json:37,42-45`: the sole gating build row is recorded `flaky: true` with the failing test unnamed.** The `path_line` is truncated mid-word (`"failed with exit stat"`), so the artifact records that `cargo test --workspace --exclude wyrd-dst` went red once and green once **without naming what failed**. I could not reproduce it (3× clean workspace runs here), so this is *not* a refutation of the fix — but a diff that changes the answer type of three maintenance passes is precisely the diff for which an unexplained red-then-green on the gating test row should be adjudicated at sign-off rather than absorbed as "transient". Note the surrounding claim is otherwise fine: `run_dst()` **is** inside `cargo xtask ci` (`xtask/src/main.rs:1567`), so the DST custodian cases that assert `Reconciled::Satisfied`/`Changed` (`crates/dst/tests/custodian.rs`) did run against the new `Blocked` answers.

## Attacks that came up empty (for the record)

Tried and could not land: a duplicate `ChunkId` split across a flat and a segmented record (merged correctly by `insert_site`, `reconstruction.rs:1171`); a same-record duplicate (leg 4's `inode:42` covers it); index/`chunk_index` skew between `hits` and `prior_chunks` (both derived from the same `resolved.chunks`); `Ok(None)` mis-skipping a live generation (`resolve_snapshot` only produces `Gone` for the segmented shape); the `ChunkMapError` downcast swallowing a store fault (leg 6 pins it, and it is red on base for the *right* reason — the base returns a decode error, not `STORE_FAULT`); non-canonical key handling (`inode:007` vs `inode:7`, leg 3's twins sub-assertion is genuinely base-red — the base's `inode_key(parse(key))` fills `inode:7` twice and leaves `inode:007` empty); unbounded retention in `locate_queued_chunks` (retains only objects with hits, one `Arc` per object).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Strengthen independent fault/gauge assertions — 12/66 rerun mutants survived because leg 3 combines both unreadable shapes and checks only aggregate blocking, while removal of the required remaining gauge is also undetected (`crates/custodian/tests/segmented_map_passes.rs:601`, `crates/custodian/tests/segmented_map_passes.rs:613`, `crates/custodian/src/backfill.rs:283`).
- [ ] T4 Contribution — Confirm the user-impact opener and tracker remain present — `scripts/pdca` and the contribution artifacts were not supplied, so its green row cannot be independently reproduced; the separate affected-path history audit did find closed #647 and no open competing PR.
- [ ] T5 Judgment — Rebuild the non-certifying malformed path, bounded retained state, and mutation-sensitive discriminator before sign-off — each is an implementation correction against already-pinned safety and scale decisions (`crates/custodian/src/rebalance.rs:259`, `crates/custodian/src/reconstruction.rs:868`).
- [ ] Validation — fitness-to-purpose — Decide whether corrected in-memory seam evidence is sufficient for this data-loss-sensitive maintenance release — the brief deliberately excludes a real backend/topology exercise, so that acceptance determines production confidence.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Strengthen independent fault/gauge assertions — 12/66 rerun mutants survived because leg 3 combines both unreadable shapes and checks only aggregate blocking, while removal of the required remaining gauge is also undetected (`crates/custodian/tests/segmented_map_passes.rs:601`, `crates/custodian/tests/segmented_map_passes.rs:613`, `crates/custodian/src/backfill.rs:283`).; T4 Contribution — Confirm the user-impact opener and tracker remain present — `scripts/pdca` and the contribution artifacts were not supplied, so its green row cannot be independently reproduced; the separate affected-path history audit did find closed #647 and no open competing PR.; T5 Judgment — Rebuild the non-certifying malformed path, bounded retained state, and mutation-sensitive discriminator before sign-off — each is an implementation correction against already-pinned safety and scale decisions (`crates/custodian/src/rebalance.rs:259`, `crates/custodian/src/reconstruction.rs:868`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
