# Brief — issue 681 / passes-read-through-resolver-contained

> **Plan RESTART** (2026-08-06) after the 2026-08-05 sign-off returned this bundle from Check.
> #681 is itself slice **4b of 7** of the #635 re-slicing — it is already a split product, so it
> is **not** cut again here. The two archived attempts (`iteration-v1/`, `iteration-v2/`) passed
> C1–C5, C4-verify red→green and mutation analysis; they failed on **T2 shape** and four review
> findings. The measured cause is narrow and this brief is written against it:
>
> | file | v2 raw added | v2 **semantic** added |
> |---|---|---|
> | `src/reconstruction.rs` | 373 | **192** |
> | `src/backfill.rs` | 192 | **94** |
> | `src/rebalance.rs` | 162 | **88** |
> | `tests/segmented_map_passes.rs` (new) | 1,185 | **803** |
>
> The production change is 374 semantic lines across three files — that is not an oversized
> slice. **68% of the patch was the discriminator fixture.** So the narrowing is applied to the
> TEST, not the fix: the legs below are capped, enumerated and required to share one fixture.
> The v2 production hunks are **salvage** with four named corrections, not a re-derivation.
>
> Siblings: **#649/#650** (shared resolver + GC/scrub, merged `99c7fcf`/`11aa85f`), **#651**
> (restore + drain report, merged `8decc93`), **#652** (merged `b083ec4`), **#682** (4c —
> `repoint_chunk` + record ceilings, depends on this).
> Tracker: https://github.com/getwyrd/wyrd/issues/681

- **Slug:** passes-read-through-resolver-contained
- **Defect:** The three maintenance passes that walk the committed namespace **themselves** —
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
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_passes.rs` passes,
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, no DST leg needed for the discriminator. Verified at this Plan, not assumed:
  - **Base:** `PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh --print-base` →
    `origin/main`. No `Onto branch`, no wave fold (`$PDCA_BASE` / `$PDCA_VERIFY_BASE` unset for a
    single bundle), and `main == origin/main == 339da46`, 0 commits either way.
  - **Classification — `--classify` dry-run on a synthetic patch listing the exact expected file
    set** returned `ADDED_TEST crates/custodian/tests/segmented_map_passes.rs` + `CRATE
    crates/custodian`. So the green leg is `cargo test -p wyrd-custodian --test
    segmented_map_passes`, and the RED leg reverts the three production files while keeping this
    one (`run-verify.sh:97-98`, `:454`). The file does **not** exist on `origin/main` (checked).
  - **The discriminator MUST NOT name a symbol this patch introduces** — no new variant, field,
    helper or `pub fn`. The red leg reverts production, so such a reference makes the target fail
    to **compile** and the red degrades to UNVERIFIABLE (exit 77) instead of a behavioural red.
    `Reconciled::Blocked` already exists on the base (`reconciliation.rs:44`, landed by #650), so
    naming it positively is legal and is what legs 2–4 do.
  - **No vacuous green:** no `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]`
    (grepped on the base), so neither zero-test guard can trip.
  - **Independent corroboration:** the v2 attempt's Check recorded all 8 of its base-visible legs
    compiling and failing on behavioural assertions at `339da46`, then passing with the fix. The
    six legs above are a compression of that same set, so the red is demonstrated, not predicted.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, reached by the §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`; sourced there to the maintainer's rule of 2026-07-25, `0016:2802-2813`
  and `gc.rs:22-25`), stated over this slice's category: **the maintenance passes that restore
  redundancy and execute a drain**.
  - **A pass reads every committed object the same way every other consumer reads it.** Redundancy
    restoration is not a service a store may lose by publishing one large object, and "unsupported
    record shape" is not a bounded state — nothing exits it.
  - **An obligation is discharged or kept; it is never discarded for want of a reading.** "I could
    not read the map" and "no committed map references this chunk" are different facts and only
    the second permits draining. Collapsing them deletes the last record saying live data is
    under-replicated.
  - **Containment is per object, and the answer still gets made.** One damaged record may not cost
    every healthy object its repair, its fill or its evacuation; `Err` for the whole pass is as
    wrong an answer as `Satisfied`. A fault that is **not** one object's — the store failing
    underneath — still ends the pass.
  - **A pass never claims more than it read.** Refused work and an incomplete reading both mean
    the pass answered over less than the store; `Satisfied` there tells an operator a decommission
    is safe or that redundancy is whole.
  - **The work a pass costs is bounded by the obligations it holds, not by their product with the
    namespace.** A repair loop costing Q×N resolves stops converging as a store grows — the
    permanence C-1 forbids, reached through the scheduler.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches, and M4's integration branch is merged and deleted. #648–#652 all landed on
  `main` directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, and the only bundle in this run.** Every code prerequisite is
  already merged on the base and was verified there at this Plan — #649's `resolve_chunk_map`,
  #650's `Reconciled::Blocked` + the GC/scrub containment precedent, #651's restore precedent.
  **#682 depends on this** (it edits `reconstruction.rs` and `rebalance.rs` again and needs this
  slice's refusal path to exist before it can complete the move); when 682 is driven, it belongs
  in a later wave than this bundle. #654's children (#691/#692/#693) share no file with this one
  (`crates/core/*` vs `crates/custodian/*`) and may run in parallel.

  Computed with `waves.compute_waves` over every in-flight bundle at this Plan (`check_dep_graph`
  clean; conflict map `682 ↔ 692`), for the record — this bundle needs none of it, but it is the
  layout a combined run would take:
  `wave 0: #681, #691` · `wave 1: #682` · `wave 2: #692` · `wave 3: #693` · `wave 4: #655`.
  #682 must not be driven before this bundle lands: it would build on a base without the refusal
  path it completes.
- **Surfaces:** data
- **Difficulty:** high   (three production files, seven call-sites, a restructure of
  reconstruction's assessment loop from per-obligation namespace scans to one reading, and a
  change to what each pass's `Reconciled` answer may claim — which `reconcile_step`'s
  `least_certified` fold reads. A diff-reviewer must hold three loops, the resolver's typed-error
  contract, the containment/downcast rule and the complexity property in view at once.)
- **Scope:** the three passes that scan `inode:` **resolve every committed object the way every
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
- **Budget:** **exactly 4 files**, ≤ **880** added semantic lines (non-blank, non-comment) — the
  sum of the four allocations below, and a 25% cut on v2's 1,177 —
  ≤ **1,520** raw added lines — which lands the patch near 90 KB at v2's measured 59 bytes/added
  line, under the driver's 100 KB empirical backstop that v2 tripped at 113 KB. Allocation, so the
  cap is a plan and not a race: `src/reconstruction.rs` ≤ 210 · `src/backfill.rs` ≤ 100 ·
  `src/rebalance.rs` ≤ 100 · `tests/segmented_map_passes.rs` (**new**) ≤ **470 semantic / 780
  raw**, sub-assertions included. A **fifth** file, or a test file past 780 raw lines, means the
  shape is wrong: STOP and hand back rather than finish. Heading past the test cap is the signal
  that the four compression rules below were not applied — re-apply them, do not exceed.
  **How the test file fits in 700 lines** (v2 needed 1,185 — this is the whole narrowing):
  - **ONE metadata double serving all three roles.** A single `BTreeMap`-backed `MemMeta` that
    also carries the two counters leg 5 reads and the optional injected `get` fault leg 6 needs —
    not three separate store types. Its `scan_page` delegates to
    `wyrd_testkit::test_double_scan_page`, as `segmented_map_consumers.rs:109-116` does.
  - **ONE parameterised seeding helper** planting the healthy flat / fillable flat / healthy
    segmented / damaged objects, which every leg calls with different arguments.
  - **SIX tests, each asserting its property across all three passes over one store** — never one
    test per pass.
  - **ONE audit-capture helper, used by the ONE leg that needs it** (leg 3's naming assertion;
    leg 4 reuses it). No second subscriber wiring.
  - Scale reference: the sibling #651 discriminator `tests/segmented_map_restore.rs` is **731
    lines in total**. Match that, not v2's fixture.
- **Repro instruction:** on the target checkout, read the seven sites with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` (and `backfill.rs`,
  `rebalance.rs`) at the lines tabulated under §Defect. Seeding **any** `seg:`-backed committed
  root — or any committed record that will not decode — makes `reconstruction::reconcile`,
  `backfill::reconcile` and `rebalance::reconcile` all return `Err` for the whole store. The
  seeding shape to copy is `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — in that order, the five registered doctor.checks ids at pdca.toml lines 696, 703, 711, 733 and 740, all OK on this host at Plan. Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool. Nothing else beyond the base Rust toolchain: the passes run over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_passes.rs` — a **NEW** file, not optional.
  C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97-98`); appending
  to `segmented_map_consumers.rs` or `segmented_map_restore.rs` makes it a *modified* file, the
  gate takes the green-only branch and proves no red at all. Confirmed by the `--classify`
  dry-run above. The name completes the family (`…_consumers.rs` #650, `…_restore.rs` #651).
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Two things are **pre-declared here** so they arrive at sign-off as expected items rather
  than surprises:
  - **No seeded Tier-0 DST case ships in this slice**, and the v2 review finding asking for one
    (`segmented_map_passes.rs:1077`, *"a new destructive or concurrent path lands with seeded
    Tier-0 DST coverage"*, AGENTS.md §Recurring defect classes / *Test fidelity*) is **settled at
    Plan as recorded-rejected**, with this reason to paste into `review-rejected.md`: *this slice
    introduces no new destructive or concurrent path. Every write it performs is on a FLAT object
    and keeps its existing version-conditional CAS on the scan snapshot, byte-for-byte the
    behaviour on the base — a flat map resolves to a borrow with no store read and no supersede
    check (`metadata.rs:2584-2586`), so the resolver adds no new race to any path that commits.
    What the slice adds on the segmented side is refusal, which writes nothing at all. The seeded
    Tier-0 case for the segmented write path belongs to #682, which builds it.* If Do finds a
    commit path that CAN be reached through a restarted resolve, that falsifies this reasoning:
    say so in `build-notes.md` and leave it for sign-off — do not add a DST file and blow the
    file budget.
  - **The advisory `C5-mutants` row** covers the diff; v2 recorded 0 survivors after the rebuild,
    so a survivor here is a real signal about the compressed legs, not noise.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line
  number in this brief was re-verified against `origin/main` @ `339da46` during this Plan's
  verification pass; still cite by symbol if the base advances.

  **Salvage — the primary lever, not a hint.** `results/issue_681/iteration-v2/patch.diff` holds
  production hunks that already passed C1–C5, C4-verify red→green and mutation analysis. Take
  them and apply pinned decisions 3–6 (the four carried-forward findings); **rebuild the test file
  smaller** to the six legs above rather than trimming the 1,185-line one.

  **Peer callsites Do MAY open — this is a composition slice; mirror them rather than invent a
  shape:**
  - `crates/custodian/src/gc.rs:360-455` — `referenced_fragments`, the canonical walk: decode
    failure contained per object (`unresolvable.insert(key, fault); continue`), resolve via
    `metadata::resolve_chunk_map`, `Ok(None)` skipped as "no live committed generation", and the
    **downcast rule** at `:402-416` — `Ok(ChunkMapError)` is contained as *this record's* fault,
    any other error propagates because a store fault is not one object's. Contain by exactly this
    rule and no other.
  - `crates/custodian/src/gc.rs:164-166` + `:470-480` — attribution emitted by the **consumer**,
    per object, before the rest of the pass, and `object_name`'s injective escaping. Mirror the
    placement, not just the call.
  - `crates/custodian/src/restore.rs:616-688` — the same shape applied a second time by #651, with
    the deferral note to this slice at `:616`.
  - `crates/custodian/src/reconciliation.rs:44` + `:55-61` — `Reconciled::Blocked` and
    `least_certified`, the existing vocabulary for "ran over everything it could read and refuses
    to certify the rest". Reuse it; do not invent a parallel outcome.
  - `crates/core/src/metadata.rs:2256-2272` + `:2619-2632` — `ResolvedChunkMap` (why `record`
    rides along) and `resolve_chunk_map`'s three arms. Decision 4 lives here.
  - `crates/custodian/src/reconstruction.rs:184-232` — the existing per-obligation assessment loop
    and its gauge accounting; every classification and the off-gauge rule survive the rework.
  - `crates/custodian/tests/segmented_map_restore.rs:387-431` (`seed_segmented` / `seed_damaged` —
    raw `seg:` + root seeding, never a committer, with the fixture asserting the fault is real)
    and `crates/custodian/tests/segmented_map_consumers.rs:78-133` (the `BTreeMap`-backed
    `MemMeta` whose ordering makes "the damaged record is met FIRST" a fixture property, not luck;
    its `scan_page` delegates to `wyrd_testkit::test_double_scan_page`, `:109-116`).
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at this Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/{reconstruction,backfill,rebalance}.rs` → 8 commits; the nearest to this
  behaviour are `3e05891` (#648 — the segmented record shape, which **created** these seven
  fail-closed sites), `0c97685` (#430, fragment identity), `5f2f79f` (#397/#348, classify
  placement before scheme), `fddb448` (#350, backfill). **No open PR touches these paths**
  (`gh pr list --state open` → empty). **Closed/rejected:** PR **#647**
  (`enhancement/635-segmented-chunk-map`, closed 2026-07-30 unmerged) is the un-split ancestor —
  closed for size and reviewability, not direction, and its content is landing as this slice
  sequence; its custodian-local `crates/custodian/src/resolve.rs` has been **superseded** by the
  shared `metadata::resolve_chunk_map` (#649) and must not be reintroduced. Merged since: #675
  (#649), #676 (#650), #683 (replay), #688 (#651), #689 (#652). No prior attempt at this
  containment was rejected on the merits — this bundle's own two attempts were returned for
  **size**, with C1–C5 passing both times.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before sign-off
accepts.

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Strengthen independent fault/gauge assertions — 12/66 rerun mutants survived because leg 3 combines both unreadable shapes and checks only aggregate blocking, while removal of the required remaining gauge is also undetected (`crates/custodian/tests/segmented_map_passes.rs:601`, `crates/custodian/tests/segmented_map_passes.rs:613`, `crates/custodian/src/backfill.rs:283`).; T4 Contribution — Confirm the user-impact opener and tracker remain present — `scripts/pdca` and the contribution artifacts were not supplied, so its green row cannot be independently reproduced; the separate affected-path history audit did find closed #647 and no open competing PR.; T5 Judgment — Rebuild the non-certifying malformed path, bounded retained state, and mutation-sensitive discriminator before sign-off — each is an implementation correction against already-pinned safety and scale decisions (`crates/custodian/src/rebalance.rs:259`, `crates/custodian/src/reconstruction.rs:868`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 66 mutants tested in 72s: 11 missed, 26 caught, 29 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Reconcile the reported seven-blocker batched review and confirm the contribution artifacts — `scripts/review-branch` and `scripts/pdca` are absent from the target, so those rows could not be independently rerun; the affected-path audit found closed #647 and no open competing PR.; **The refusal vocabulary this patch introduces is asserted by nothing; the; **Two comments assert a byte-exactness property the code does not; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 74 mutants tested in 69s: 4 missed, 31 caught, 39 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Reason: real, fixable bugs found by review rather than a scope/sizing problem, despite this being an already-split slice (4b of 7) hitting the size backstop a second time — the backstop's iterate-plan suggestion was considered and overridden. Address in the rebuild: 1. rebalance.rs:269 (T4 blocking) — a malformed placement is skipped without incrementing `refused`, so a segmented object with an unreadable placement can leave fragments stuck on a draining server while the pass reports Satisfied. 2. T2 Shape — trim the new test file: it is 8 lines over the brief's 780-line cap. 3. Adversary findings — reconstruction.rs and rebalance.rs re-derive the canonical CAS key without a test binding pinned decision 3 for those two passes (only backfill is actually bound); add the missing sub-assertion(s) so mutating the canonical re-derive back to the raw key fails a test. 4. Adversary finding — reconstruction's "an incomplete reading may not certify" claim (reconstruction.rs:170, `refused` seeded from `index.unaccounted`) is not bound by any leg; add a sub-assertion (enqueue only C_REPAIR, drop C_UNSEEN, over the damaged store) that still requires Blocked. 5. Adversary finding — the gauge assertion in segmented_map_passes.rs (~190-193, ~575, ~672) is a prefix match that would not catch a 10x-inflated value; tighten to match the delimiter or parse the JSON field. 6. T4 TEST-GAP claim (DST coverage) and the backfill-gauge-vs-unreadable-object human question (Validation item) should be explicitly addressed or recorded-rejected with reasoning in the rebuild, not left ambiguous. 7. Confirm the C4-ci flake was transient (note what interfered) as part of the rebuild's verification, not a lingering unknown.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Reason: T4 batched review found 3 new blocking bugs this round (backfill.rs:190 stale gauge on CAS conflict; rebalance.rs:130 evacuation proceeding despite an incomplete/refused read; reconstruction.rs:383 repair allowed despite unaccounted duplicate references) — all implementation-shaped, not scope-shaped. The size backstop tripped again (3 rounds spent post-split, threshold 2) and the advisory review confirms the shape budget is still over (915/880 semantic lines) plus flags a metrics/gauge issue and an operator-visible pending-forever edge case for scope judgment. Human overrode the iterate-plan recommendation: the prior split (#654 -> this slice) was recent, so the process should be given more room to converge via iterate-do before re-splitting. Fix the 3 T4-blocking bugs, address the advisory findings (or record-reject with reasons), and get the semantic-line count back under budget.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: The slicing is the problem, not the implementation. The size backstop's `iterate-plan` recommendation was overridden at v5 and again at v6, both times reasoning the findings were implementation-shaped and the process deserved more room to converge. That experiment has now been run twice on this slice and has not converged: round 4 arrives with 2 fresh T4 blockers, the test file over its cap again (498 semantic vs 470), and a load-bearing guard bound by no test. Totals: 4 sign-off rounds, 7 builder attempts, 3 auto-iterates, 1 prior replan, ~102 KB patch. Re-split at Plan rather than a fifth Do round. Suggested split axis for Plan to author (a hint, not a mandate — the brief bundles three separable kinds of work over the same three files): 1. read through `resolve_chunk_map` + refuse segmented work without discarding it; 2. per-object containment on ANY read fault (unreadable record, unresolvable generation, non-canonical key), the rule copied from gc/scrub; 3. the Q x N -> O(N) restructure of reconstruction's namespace reading. Each round's findings have clustered by which of these three it touched, which is the signal the split should follow. Carry forward into the split — every one of these is unresolved, none is a reason to re-litigate the pinned decisions: - **`rebalance.rs:196` over-containment guard is bound by nothing.** Adversary replaced its body with a no-op: all six new legs AND the whole `wyrd-custodian` suite still passed, while the pass flipped from `Satisfied` to `Blocked` over a healthy segmented object with nothing on the draining server — i.e. no decommission would ever certify on a store holding a multipart object, this slice's own defect in mirror image. Needs a binding sub-assertion (a `step(false, true)` over a segmented object holding nothing on the draining server). Note the C5 `0 missed` row does NOT cover this: mutants pin the arithmetic, not the predicate. - **Two T4 blockers, same theme:** a resolver restart can leave `chunk_index` describing the live generation while `prior_bytes` holds the scanned one, so unchecked indexing can panic before the stale CAS rejects the plan (`rebalance.rs:412`, `reconstruction.rs:659`). - **The decision-4 "unreachable by construction" question is still unsettled after three rounds.** Round 4's adversary demonstrated it false with a working double (a real `version 9 -> 10` commit framed by a generation the pass never scanned); rounds 5 and 6 re-traced and could not find a reachable path; this round's adversary also could not refute it. The two T4 blockers above are the same theme resurfacing. Plan must settle this explicitly rather than pin it again. **Read the FULL finding at `iteration-v4/check-advisory-adversary.md:25`, not the deferred-findings stub — the ledger truncated it mid-sentence.** - **T2 Shape:** the discriminator is 498 semantic added lines against a 470 cap. If the split lands, re-allocate the budget per child rather than trimming. - **C4-ci flaked** (failed, then passed its once-only confirm re-run); what interfered was never established across two rounds that promised to. Not in dispute, and should NOT be rebuilt from scratch by the children: the correctness core is sound and independently confirmed — red->green is real (6 legs fail behaviourally on base `339da46`, pass with the patch, driving production entry points), 82 mutants with no survivors, and the adversary explicitly could not refute the CAS framing, the drain-on-incomplete-reading path, or the test's assertion strength. Carry the existing discriminator legs into the children. Process note: the deferred-findings ledger has been truncating multi-line findings at the first newline since round 4, so three consecutive rounds asked the human to clear §6 items that are unreadable as written. See §10.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
