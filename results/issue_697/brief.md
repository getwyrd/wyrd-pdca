# Brief — issue 697 / reconstruction-reads-through-resolver-once-contained

> Child 3 of 3 from the #681 split; siblings **#695** (backfill) and **#696** (rebalance) touch
> disjoint files. **#682 depends on this one.**
>
> **Re-planned 2026-08-07 after five rejected rounds.** Deliberately SMALLER than the brief it
> replaces (`iteration-v5/brief.md`): **four** things that one carried — a generation-restart
> comparison ("Rule A"), a key-identity predicate ("Rule C"), the duplicate-`ChunkId` ambiguity rule,
> and the write-time landing guard that rule bought — are **out of this slice**, and the lines they
> touched are **frozen at the base**. Each was filed at Plan as its own issue (**#699**, **#698**,
> **#700**), and the `Reconciled::Blocked` contract question as **#701**. §Scope says why; the
> closing table is the evidence. This mirrors the #695 / #696 re-plans of the same day, whose five
> rounds carried the same signature.

- **Slug:** reconstruction-reads-through-resolver-once-contained
- **Defect:** `crates/custodian/src/reconstruction.rs` reads the chunk map inline out of the inode
  record at **three** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `reconstruction.rs:329-335` | `assess` (`:317`) | one obligation's assessment, and the pass |
  | `reconstruction.rs:579-586` | `repair_chunk` (`:525`) | the binding repair commit |
  | `reconstruction.rs:632-638` | `find_chunk` (`:620`) | the whole `inode:` scan, for every obligation |

  So a **single** segmented object stops repair for **every** chunk in the store: a store that has
  published one multipart object stops restoring redundancy altogether. Containment is not per object
  either — a record that will not `decode` ends the walk at `:625`, before any resolver is involved.
  Two further live consequences:

  1. **An obligation whose chunk lives in a `seg:` record would be drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as "referenced by no committed chunk
     map" → `Assessment::Drain` (`:322-326`), deleted in the drain batch (`:270-276`). Latent today
     (the `?` at `:636` fires first), but it is the loss this slice must not introduce while removing
     the abort — the obligation is the last record saying live data is under-replicated.
  2. **The deployed repair loop costs Q namespace scans × N point reads.** `reconcile` calls `assess`
     per obligation (`:184-185`); `assess` calls `find_chunk` (`:322`), which scans all of `inode:`
     and decodes every record (`:624-625`). Wiring the resolver in naively makes each of those N
     objects also cost a bounded `seg:` range read — **Q×N resolves**. This is the finding left open
     at #647's close, and it is this slice's to shut.

  Reconstruction is the last of the four custodian loops still reading this way — GC (#650), scrub
  and restore (#651) already read through the shared resolver and contain per object.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_reconstruction.rs`
  passes, driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}` (`lib.rs:39`, `:41`, `:42`),
  `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`, and
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}` — over in-memory `MetadataStore` /
  `ChunkStore` doubles, with `Custodian::elect` + `FencedZone::new` over
  `wyrd_coordination_mem::MemCoordination` for the fence. Each leg drives **`reconcile_step`**, the
  real fenced control point, never an internal helper. **The discriminator MUST NOT name any symbol
  this patch introduces** (no new variant, field, helper or `pub fn`): the red leg reverts
  `reconstruction.rs` and keeps the test, so such a reference makes the target fail to compile and
  the red degrades to UNVERIFIABLE (exit 77) instead of a behavioural red. `Reconciled::Blocked`
  exists on the base (`reconciliation.rs:44`) and may be named.

  **Six legs over ONE shared fixture** — one `BTreeMap`-backed metadata double carrying the counters
  and the injected fault, one parameterised seeding helper, one audit/metric capture helper:

  1. **A healthy segmented object no longer ends the pass, blocks nothing, and the flat work in the
     same store still happens.** One healthy segmented object that owns **no queued chunk** (raw
     `seg:` records + a segmented root, **never** a committer) beside (a) an under-replicated **flat**
     chunk with a queued repair and (b) a queued obligation for a chunk **no** committed record
     references: `reconcile_step` returns `Ok` (today `Err`), the flat chunk's placement moves, its
     obligation is discharged by the repair commit (`:602`), the unreferenced obligation **is**
     drained (`:270-276` — the base's `Drain` path survives), and the answer is
     `Reconciled::Changed` — **not** `Blocked`. *(binding — base-red; also binds answer rule 1)*
  2. **An obligation for a chunk inside a `seg:` record is refused, never discarded, never counted,
     and the pass does not certify.** **Two** obligations whose chunks live in the **same** segmented
     object: both are **still in `queued_repairs`** afterwards; the `seg:` record bytes and the root's
     `version` are **byte-identical**; the audit seam carries **exactly one** `refused-segmented` row,
     naming that object (§Scope pins the vocabulary); `gauge.reconstruction_under_replicated` does
     **not** count them; the pass answers `Reconciled::Blocked`. *(binding — base-red; this is defect
     1 above, plus the per-object accounting rule, in one fixture)*
  3. **An unreadable committed object is named, the walk continues, NOTHING is drained, and nothing
     certifies.** Seed — **first in key order**, over the `BTreeMap`-backed store, so it is a fixture
     property and not luck — (a) a committed root naming a `SegmentRef` whose `seg:` record was never
     written, and (b) a committed record whose own bytes will not `decode`; assert in the fixture that
     `resolve_chunk_map` really errors on (a). Beside them, the same healthy flat repair as leg 1 and
     the same obligation for an unreferenced chunk. Assert the conjunction: `Ok`; `Blocked` (never
     `Satisfied`); **the healthy repair still lands**; **the unreferenced obligation is NOT drained
     and is still queued**; both damaged objects **named** on the audit seam by their `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470-480`). *(binding — base-red)*
  4. **The namespace is read ONCE per pass — O(N), not O(Q×N).** With **Q ≥ 3** queued obligations
     over **N ≥ 3** committed flat objects, the counting double records exactly **one**
     `scan(b"inode:")`, independent of Q (the base does Q of them), and the repairs still land. On a
     store holding S segmented objects the resolver's `seg:` reads are **≤ S**. Count the two on their
     **separate seam methods** — `MetadataStore::scan` and `MetadataStore::scan_page` are distinct
     required trait methods (`crates/traits/src/lib.rs:1023`, `:1105`), the `inode:` walk uses `scan`
     (`reconstruction.rs:624`) and the resolver's `seg:` range uses `scan_page`
     (`crates/core/src/metadata.rs:2431-2433`, via `read_group_range`). Keep every fixture object's
     segment count well under `SEGMENT_PAGE_LIMIT` (128, `metadata.rs:2249`) so one segmented object
     is exactly one `scan_page` and the bound is a clean equality rather than a page-count race. Build
     this leg with a
     `ReconstructionContext` and **no** GC / scrub / rebalance context beside it — the other loops
     walk `inode:` themselves and a store-wide scan count would demand the shared-walk refactor that
     §Scope puts out of bounds. *(binding — base-red; this closes the finding left open at #647)*
  5. **A fault that is not one object's map still ends the pass, and the name is already out.** The
     pass returns `Err`, **and** the unreadable object met earlier in the same walk is **already**
     named on the audit seam. Fixture, stated precisely so the ordering is a property and not luck:
     over the `BTreeMap`-backed store, (a) **first** in key order, a committed record whose bytes will
     not `decode` — contained and named; (b) **after** it, a committed **segmented** root whose `seg:`
     read the double answers with a **non-`ChunkMapError`** error. Inject the fault on the read the
     **resolver** performs, never on `scan(b"inode:")` itself — a fault on the namespace scan aborts
     before anything is named and would assert the opposite of this leg. *(the over-containment guard
     — without it, containing EVERY error would pass legs 1–4 — and the placement oracle for the
     attribution rule. Its base behaviour is incidental: the base fails closed on almost everything,
     so it may go red there too; do not spend effort making it non-red.)*
  6. **An empty queue performs no reading and answers `Satisfied`.** Over the store that already holds
     an unreadable object, run reconstruction with **no** obligations queued: `Satisfied`, and the
     counted double records **zero** `scan(b"inode:")`. *(NOT base-red, verified at Plan rather than
     assumed: the base's loop is `for chunk in queue { assess(..) }` (`:184`), so an empty queue
     already scans zero times and already answers `Satisfied` (`:278-282`). This is a REGRESSION guard
     on the restructure — leg 4 moves the reading OUT of the per-obligation loop, and the obvious way
     to do that is to read the namespace unconditionally at the top of the pass, which would silently
     break this and make the pass claim over objects it never needed to read. Required for that
     reason.)*

  **A leg must assert the call succeeded before it inspects the outcome.** Any `certifies`-style
  helper folding `Result<Reconciled, _>` to a bool MUST fail on `Err` rather than read it as "did not
  certify" — a helper that silently accepts every `Err` makes legs 1–4 pass on a tree where the pass
  aborts outright, which is the defect itself. #696's round 3 lost the whole gate to exactly this.
  Assert `Ok(..)` explicitly.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux workspace
  over in-memory trait doubles — no topology, no cfg gate, no Docker, no new dev-dependency, **no DST
  leg**. Verified at Plan, not assumed: `main == origin/main == 339da46`; `--print-base` on this
  bundle → `origin/main`; the `--classify` dry-run on a synthetic patch listing exactly
  `crates/custodian/src/reconstruction.rs` + the new test returns `ADDED_TEST
  crates/custodian/tests/segmented_map_reconstruction.rs` and `CRATE crates/custodian`, so the green
  leg is `cargo test -p wyrd-custodian --test segmented_map_reconstruction` and the red leg reverts
  `reconstruction.rs` while keeping the test (`engine/scripts/run-verify.sh:466-475`). No
  `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped on the base), so the
  zero-test guard (`:445-451`, exit 77) cannot trip. `_resolve_base_ref` (`run-verify.sh:240-246`)
  honours `$PDCA_BASE` → `$PDCA_VERIFY_BASE` → `$WYRD_VERIFY_BASE` → the brief's base; this bundle is
  wave 0 with no `Onto branch`, so the base is `origin/main` — the same ref the PR opens against.
  **Legs 1–4 go red on the base; legs 5 and 6 are declared above.** Independent corroboration that
  the red is demonstrable and not predicted, both re-read at Plan: this bundle's v5 reviewer scored
  **C2 PASS** (production stashed, test kept → 7 of 8 legs failed *behaviourally*, only the declared
  empty-queue leg green) and **C4 PASS** (production restored → all 8 green); and #681's v7 recorded
  6 legs failing behaviourally on base `339da46` and passing with the patch, driving production entry
  points (`results/issue_681/iteration-v7/SUMMARY.md:373`).
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*, `:137`), over
  **the maintenance pass that restores redundancy**:
  - it reads every committed object the way every other consumer reads it;
  - **an obligation is discharged or kept; it is never discarded for want of a reading** — "I could
    not read the map" and "no committed map references this chunk" are different facts, and only the
    second permits draining;
  - containment is per object and the answer still gets made for the rest;
  - it never claims more than it read — an operator reading `Satisfied` is being told redundancy is
    restored, and will act on it;
  - **its work is bounded by the obligations it holds, not by their product with the namespace** — a
    repair loop costing Q×N resolves stops converging as a store grows, which is the permanence C-1
    forbids, reached through the scheduler.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with #695 and #696.** Touches
  `crates/custodian/src/reconstruction.rs` plus one new test file; neither sibling touches either
  (#695 is `backfill.rs`, #696 is `rebalance.rs`), so no `Conflicts with:` is owed even though all
  three may build against the same base at once. Every code prerequisite is already merged on the base
  (#649's `resolve_chunk_map`, #650's `Reconciled::Blocked` and the GC containment precedent, #651's
  restore precedent). **#682 depends on this child** and lands after it.
- **Surfaces:** data
- **Difficulty:** high   (one production file, but three call-sites plus a restructure of the
  assessment loop from per-obligation namespace scans to one reading, and a change to what the pass's
  `Reconciled` answer may claim — which `reconcile_step`'s `least_certified` fold reads,
  `reconciliation.rs:51-61`. A diff-reviewer must hold the loop, the resolver's typed-error contract,
  the containment/downcast rule, every existing classification and its gauge accounting, and the
  complexity property in view at once. Rated up deliberately: still the largest of the three
  children, even after this reduction.)
- **Scope:** reconstruction **reads every committed object through the resolver every other consumer
  already shares — once per pass, not once per obligation — contains per object what it cannot read,
  and refuses, rather than aborts or silently drains, the repair it does not own.** A chunk whose
  `ChunkRef` lives in a `seg:` record is **refused**: the segmented record is left byte-identical, the
  obligation stays queued, and the pass does not certify (the segmented write path is #682's). A
  chunk in a flat record is repaired exactly as today, with **every** existing classification
  (`Repairable` / `Drain` / `Unreachable` / `Blocked` / `Unrepairable` / `Malformed`) and its gauge
  accounting preserved, **including the rule that a never-repaired condition stays off the
  repairable-backlog gauge** (`:199`, `:205`, `:214`, `:223`, and the tally comment at `:150-174`) —
  a **refusal** joins that set. Leg 2 is the oracle for the refusal; the oracle for the five
  classifications this slice's own legs do not drive is the existing suite
  `crates/custodian/tests/reconstruction.rs`, which must stay green **unmodified** under `C4-ci`.

  **Three answer rules, pinned so they are not re-derived (each is a finding waiting to happen):**
  * **A refusal is per object, not per chunk — and a segmented object nothing is owed on blocks
    nothing.** A segmented object the pass read successfully that holds **no queued chunk** is
    ordinary and healthy: it is not named, not counted, and the pass may still answer `Satisfied` /
    `Changed`. Get this wrong and every store holding one multipart object is `Blocked` forever —
    this slice's own defect in mirror image. Two obligations inside one segmented object are **one**
    refusal row, not two: round 1's sole blocking finding was that the accounting could not be shown
    to be per object because the fixture held a single obligation. Bound by legs 1 and 2.
  * **While the pass's reading is incomplete, it drains NOTHING.** Both of the base's drain paths —
    "no committed map references this chunk" (`:322-326`) and "already at full redundancy"
    (`:420-423`) — are conclusions over the whole committed namespace, and an object this pass could
    not read is a hole in it. One rule, no exceptions, no per-site predicate: round 2's sole blocking
    finding was a readable site still draining while the reading had a hole. Over a **complete**
    reading both paths behave exactly as on the base. Bound by legs 1 (complete → drains) and 3
    (incomplete → drains nothing).
  * **The pass certifies only over the reading it performed.** An **empty** queue reads nothing and
    answers `Satisfied` — it makes no claim about objects it never read, and the base already behaves
    this way. A non-empty queue DOES read the namespace, and an object it cannot read, or an
    obligation it refused, withholds certification → `Blocked`. Precedent for answering without
    reading `inode:` at all: `rebalance.rs:115-117`. Bound by leg 6 (empty queue → `Satisfied`, zero
    reads), leg 3 (unreadable → `Blocked`) and leg 2 (refused → `Blocked`).

  **The constraint that keeps the write honest — it bounds the shape, it names no mechanism.**
  Whether this pass may write for an object, and the bytes any write is built from and conditioned
  on, are decided from **the generation the scan returned** — never from what a resolve answered after
  restarting onto a newer root. *Why that needs no machinery of its own:* a **flat** snapshot resolves
  to a borrow of the record and reads nothing — `ChunkMap::Flat(chunks) => return
  Ok(Resolution::Answer(Cow::Borrowed(chunks)))`, `crates/core/src/metadata.rs:2585` — so it can never
  be `Superseded` and never restarts (`:2629`). Only a **segmented** snapshot can, and a segmented
  snapshot is one this slice refuses. Concretely: read the write-eligibility decision off the
  **scanned record's own `chunk_map` shape**, which is already in hand — not off the shape of whatever
  the resolve answered. Honour that and the restart path reaches no write at all, **by
  construction**: no generation comparison, no new counter, no new concurrent path to sweep. (The
  previous brief added the comparison, then had to buy a seeded DST property to justify it. Both go
  with the path they guarded — #699.)

  **Containment is by exactly gc.rs's downcast rule and no other** (`gc.rs:402-416`): `Ok(fault)` from
  `err.downcast::<ChunkMapError>()` is contained as *this record's* fault and the walk continues; any
  other error propagates, because a store fault is not one object's. A record that will not `decode`
  is contained the same way, before its `state` is consulted (`gc.rs:378-384`, `restore.rs:631-637`).
  `Ok(None)` from the resolver is **skipped**, exactly as both merged peers skip it (`gc.rs:404`,
  `restore.rs:646`) — not counted, not named. Bounded memory: work proportional to the **obligations
  held** and to **one object at a time**, never the whole namespace's decoded chunk lists and never
  any segment's exact bytes.

  **The added audit/metric vocabulary, pinned at Plan — do not invent a parallel set, do not
  relitigate the names.** Exactly this, each on reconstruction's existing audit target
  `"wyrd.custodian.reconstruction.audit"` (`:719-720`, `:735-736`, `:758-759`), and each MUST be
  asserted by a leg above (an unasserted label is a finding waiting to happen):
  * `action = "unresolvable-chunk-map"` + `monotonic_counter.reconstruction_unresolvable_records` for
    a record that will not decode or a generation the resolver refused — the **same action string**
    gc, restore, scrub and drain-status already publish (`gc.rs:564-567`, `restore.rs:827-830`,
    `scrub.rs:230-233`, `desired_state.rs:260-263`), each with its own `<loop>_unresolvable_records`
    counter, so one grep finds all five. Emitted **per object, before the work loop** (mirroring
    `gc.rs:155-166`) so a later transient store fault cannot cost the operator the name of the record
    to repair — load-bearing, not logging hygiene: a genuinely corrupt root has no repair path (a
    fragment carries only `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-50`) and no
    operator tooling (**#694**), and reclamation is halted store-wide meanwhile
    (`gc.rs:306-316`), so that name is the operator's entire situational awareness. Leg 5 binds the
    placement.
  * `action = "refused-segmented"` + `monotonic_counter.reconstruction_refused_records` for a repair
    this pass may not perform, **once per object**. (#696 names rebalance's equivalent identically;
    #695 names backfill's `declined-segmented`. The shared `-segmented` suffix is what makes the
    family greppable; each loop keeps the verb its own docs already use.)

  Nothing else. Naming is by the store's own key through `gc::object_name` (`gc.rs:470-480`), which
  escapes rather than replaces, so two damaged records never arrive under one name.

  **/ out of scope. The first three are BEHAVIOUR-frozen: the reading moves, but what these decide
  must answer exactly as `origin/main` answers it today —**
  * **A duplicate committed `ChunkId`, and any write guard for a claimant hidden behind an unreadable
    record — DO NOT BUILD. Tracked as #700.** The **first committed reference in key order wins**
    (`find_chunk:639` today) and that is all: no claimant index, no `ambiguous-*` verdict or audit
    surface, no cross-object claim counting, and **no probe that withholds a landing**
    (`may_land` / `nothing_stands_at` — both gone). One reference is chosen and repaired, exactly as
    the base chooses one; the second is neither detected nor reported. This one rule generated the
    sole blocking finding in rounds 1, 3, 4 and 5; its guard had no exit condition inside the window
    it was armed in, and designing that exit needs #694 beside it. If a reviewer raises either half:
    *"first-match-wins is unchanged from `origin/main`; carved out to #700 and out of scope by the
    brief"* — record-reject with that reference, do not fix.
  * **Key identity and attribution (the previous brief's "Rule C") — DO NOT FIX. Tracked as #698.**
    The pass keeps identifying a record by an `InodeId` **parsed from the scanned key** (the
    `parse_inode_key` shape at `:648-654`, used at `:640-641`) and keeps CASing under a **re-derived**
    `metadata::inode_key(plan.inode_id)` conditioned on a **re-encoded** `metadata::encode(&plan.prior)`
    (`:598-601`). `RepairPlan` therefore keeps its base fields `inode_id: InodeId` + `prior:
    InodeRecord` (`:113-116`) — **do not** switch the plan to carrying the store's own key bytes or the
    scanned `value` bytes, which is precisely #698's fix. `repair_chunk:598-601` is outside the
    restructure and stays **byte-identical**; the parse may MOVE with the walk but must not change
    meaning. Yes, a row under a non-canonical spelling (`inode:007`) would then be read at one key and
    CAS'd at another — real, **pre-existing**, unreachable today (`metadata::inode_key` is the sole
    writer of the `inode:` prefix, `crates/core/src/metadata.rs:33-36`), and **not this issue's
    defect**. #698 carries the reconstruction sites explicitly (comment, 2026-08-07).
  * **A generation-restart comparison, a `changed-under-scan` class, and any seeded Tier-0 DST leg
    (the previous brief's "Rule A") — DO NOT BUILD. Tracked as #699.** The constraint above removes
    the path instead of guarding it. **`crates/dst/` is not a file this bundle may touch.**
  * **`Reconciled::Blocked`'s rustdoc and any other file's docs — DO NOT EDIT. Tracked as #701.**
    `reconciliation.rs:25-28` still says `Blocked` means an object "could not be read", while this
    slice (and both siblings) also answer it for a **refusal over a complete reading**. Real, shared
    by three slices, and a **third file** for every one of them — so it is filed, not fixed here. A
    finding on the wording is record-rejected with the #701 reference.
  * **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the repair write
    path for a `seg:`-resident chunk are **#682**. A refusal writes **nothing at all**.
  * `backfill.rs` and `rebalance.rs` — the sibling children **#695** / **#696**. Do not touch them; a
    diff that does collides with a bundle building in the same wave.
  * `gc.rs`, `scrub.rs`, `restore.rs`, `desired_state.rs`, `reconciliation.rs`, `metadata.rs` —
    untouched (`object_name` and `resolve_chunk_map` are *used*, not changed); **leave** the deferral
    marker at `restore.rs:616` (its per-reference granularity differs, as that comment says).
  * **Sharing ONE namespace walk across all loops** (GC / scrub / rebalance / reconstruction) — a
    separate refactor, explicitly not this child's. Leg 4 is scoped to a reconstruction-only context
    for exactly this reason.
  * The chunk-id floor (#652, closed; its live remainder is #687 — the bounded recovery walk and the
    allocator); the committer, fence, rollback and resume (#653); the operator repair surface (#694);
    restore's malformed-placement report (#690).
  * **Whether a queued `Malformed` / `Unrepairable` should stop the pass certifying** — pre-existing
    base behaviour (`:214`, `:223` leave the outcome untouched) that the existing suite asserts twice
    (`crates/custodian/tests/reconstruction.rs:827-831`, `:927-935`). Already recorded-rejected on
    this bundle with that evidence (`review-rejected.md`, the `reconstruction.rs:320` entry) — point
    at it, do not re-fix.
  * The existing suite `crates/custodian/tests/reconstruction.rs` must stay green **unmodified** — it
    was green under the much larger v5 patch, so needing to edit it signals an answer changed further
    than intended; it is not a licence to edit it.
  * **No docs edit** (checked at Plan: `docs/design/architecture/06-runtime-view.md:31` already
    states this containment rule fleet-wide — *"the containment is per object throughout — the damaged
    record is attributed and the walk continues"*, and a pass that could not read every object
    *"reports the store **not certified** rather than clean"*); no new or edited ADR
    / spec / proposal; no conformance-vector change; **no `Cargo.toml` change** — every dev-dependency
    the discriminator needs (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
    `tracing-subscriber`) is already declared on `crates/custodian` (verified at Plan); adding one
    would trip the ADR-0003 audit.
- **Budget:** **exactly 2 files.** `src/reconstruction.rs` ≤ **160** added semantic lines (non-blank,
  non-comment); `tests/segmented_map_reconstruction.rs` ≤ **280 semantic / 460 raw**. Calibration:
  the #681 v7 patch spent **180** production semantic lines on this file *including* Rules A and C;
  the rejected v5 here spent **219** including the claimant index and the landing guard. With all four
  carve-outs gone the core sits inside 160. A **third file**, a `crates/dst/` hunk, or a test file past
  460 raw means the shape is wrong: **STOP and hand back rather than finish.** Compression rules: ONE
  `BTreeMap`-backed metadata double carrying the `scan`/`scan_page` counters legs 4 and 6 read *and*
  the injected `seg:`-read fault leg 5 needs — not three store types; ONE parameterised seeding helper planting
  healthy-flat / under-replicated-flat / healthy-segmented / damaged objects; ONE capture helper for
  the audit rows and gauges shared by legs 2, 3 and 5.
- **Repro instruction:** on the target checkout, `git -C ../wyrd show
  origin/main:crates/custodian/src/reconstruction.rs` at `:329-335`, `:579-586`, `:632-638`. Seeding
  **any** `seg:`-backed committed root — or any committed record that will not decode — makes
  `reconstruction::reconcile` return `Err` for the whole store; enqueueing Q ≥ 2 repairs over N ≥ 2
  committed objects shows Q separate `scan(b"inode:")` calls. The seeding shapes to copy are
  `seed_segmented` (`crates/custodian/tests/segmented_map_restore.rs:387-410`), `seed_damaged`
  (`:417-431`, which asserts its own fixture is genuinely unreadable) and `seed_flat` (`:435`); the
  fence shape is `crates/custodian/tests/segmented_map_consumers.rs:406-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, each re-run and OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_reconstruction.rs` — a **NEW** file, not
  optional and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:350-351`, and the green-only branch it falls to at `:454-464`);
  appending to `segmented_map_consumers.rs` or
  `segmented_map_restore.rs` makes it a *modified* file, the gate takes the green-only branch and
  proves no red at all. Confirmed by the `--classify` dry-run at Plan. The name completes the family
  (`…_consumers.rs` #650, `…_restore.rs` #651, `…_backfill.rs` #695, `…_rebalance.rs` #696).
- **Verification posture:** default — assertion-red on the base, green with this patch, both at Check.
  Pre-declared so it arrives at sign-off settled rather than as a surprise: **no seeded Tier-0 DST
  case ships in this child, and none is owed.** The repo rubric asks a *new concurrent or destructive
  path* for seeded Tier-0 coverage; this slice adds neither. Every write it performs is on a flat
  record resolved by borrow from the generation the scan returned (`crates/core/src/metadata.rs:2585`
  — a flat snapshot reads nothing and can never be superseded), committed under the base's own
  unmodified version-conditional CAS (`:598-608`); the segmented side performs a refusal, which writes
  nothing at all; and the guard that *did* touch the fragment seam in rounds 4–5 is carved out to
  #700. A review finding asking for a DST leg here is **recorded-rejected** with that reason, citing
  `metadata.rs:2585` and `:2629` and the carve-outs **#699** / **#700** — it is not fixed by adding
  one, and adding one puts the bundle over budget and out of scope. The advisory `C5-mutants` row
  covers the diff; the #681 v7 attempt recorded 0 survivors over 82 mutants on a superset of these
  production hunks, so a survivor here is a real signal about the compressed legs, not noise.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice: mirror the merged peers rather than invent.** Peer callsites Do MAY
  open:
  * `crates/custodian/src/restore.rs:621-658` — **the closest peer and primary model** (#651): the
    same walk over the same namespace — decode contained (`:631-637`), state checked (`:638-640`),
    `Ok(None)` skipped (`:646`), and the `ChunkMapError` downcast rule (`:647-657`).
  * `crates/custodian/src/gc.rs:360-416` — the same walk a second time (#650), the downcast rule
    stated in full at `:402-416`. **Contain by exactly this rule and no other.** That is leg 5.
  * `crates/custodian/src/gc.rs:155-166` — attribution emitted **per object, before the work loop**;
    `:470-480` for `object_name`'s injective escaping. Mirror the placement, not just the call.
  * `crates/custodian/src/gc.rs:306-316` + `:191-194` — `ReferenceSet::protection`: while any object
    is unresolvable, **every** fragment in the fleet is withheld from reclamation. This is why
    continuing the walk past a record it could not read costs nothing that can be lost — an
    evacuation/repair orphan-**marks** the displaced fragment, it never deletes it, and nothing is
    reclaimable at all while the hole exists. Read it before proposing to widen containment.
  * `crates/custodian/src/reconciliation.rs:44` + `:51-61` — `Reconciled::Blocked` and
    `least_certified`. Reuse this vocabulary; do not invent a parallel outcome (and do not edit the
    doc — #701).
  * `crates/core/src/metadata.rs:2585` (flat resolves by borrow, never restarts), `:2619-2632`
    (`resolve_chunk_map`'s three arms) and `:2266-2272` (`ResolvedChunkMap` — why `record` rides
    along) — the §Scope constraint lives here.
  * `crates/custodian/src/reconstruction.rs:150-174` + `:184-232` — the existing per-obligation
    assessment loop, every classification, and the tally comment stating why a never-repaired
    condition stays off the repairable-backlog gauge. Every one of those rules survives the rework;
    a refusal joins them.
  * `crates/custodian/tests/segmented_map_restore.rs:387-435` and
    `crates/custodian/tests/segmented_map_consumers.rs:85-116` + `:406-410` — the `BTreeMap`-backed
    `MemMeta` whose ordering makes "the damaged record is met FIRST" a fixture property rather than
    luck, its `scan_page` delegating to `wyrd_testkit::test_double_scan_page` (`:109-116`), and the
    `Custodian::elect` + `FencedZone::new` fence shape.

  **Salvage, carefully.** `results/issue_697/iteration-v5/patch.diff` is the rejected fifth attempt;
  its containment core and one-reading index passed C4-ci, C4-verify red→green over 8 legs, and its
  reviewer scored C3/T1/T2/T3 PASS. It also carries Rules A and C, the claimant index and the landing
  guard — **all out of scope here**. Reference, not a starting diff to subtract from; the peers above
  are the positive model.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/reconstruction.rs` → the nearest are `3e05891` (#648 — the segmented record
  shape, which **created** these three sites) and `5f2f79f` (assess's classification order); unchanged
  since. No open PR touches this file. **#647 is closed with the Q×N finding still open** — defect 2
  above, which leg 4 shuts. Prior attempts: seven on the un-split #681
  (`results/issue_681/iteration-v1..v7/`) and five here (`results/issue_697/iteration-v1..v5/`); the
  recorded-rejected findings that still stand are in `results/issue_697/review-rejected.md` and must
  not be re-litigated.
- **Disposition hint:** likely-fix

## What five rounds measured (why this brief is smaller, not different)

| Round | Sole/primary blocking finding | Rule that generated it |
|---|---|---|
| v1 | refusal accounting not shown per object (single-obligation fixture); commented-out queue-drain oracle | Rule D — **kept, and fixed here** (leg 2) |
| v2 | a readable site still drains while the reading is incomplete; Rule E oracle absent | drain rule — **kept, and fixed here** (legs 1/3/5) |
| v3 | repair can overwrite bytes of a duplicate claimant hidden behind an unreadable record | duplicate-id rule → **#700** |
| v4 | `may_land` stalls a completable repair forever; gauge floored; leg-9 fixture proves nothing | the guard v3 bought → **#700** |
| v5 | guard still has no exit inside its own window; `Blocked` contradicts its rustdoc; per-claim double-count; `Aborted` after bytes landed | the guard + its accounting → **#700** / **#701** |

Not one finding landed on the containment core or on the one-reading index. v5's gates were otherwise
green — `C4-ci` pass, `C4-verify` red→green over 8 tests, reviewer PASS on C2/C3/C4/T1/T2/T3/T5 — with
`T4-batch-review` the only gating failure. Siblings #695 and #696 show the identical signature. So:
one real defect carrying rules of two different kinds. The two that were genuinely this slice's —
per-object refusal accounting (v1) and no-drain-under-an-incomplete-reading (v2) — are **fixed here**,
each now bound by a leg with a fixture that can actually expose it. The rest are **removed, not
redistributed**: Rule A's path is closed by construction, Rule C's behaviour is frozen as a known
unreachable pre-existing hazard, and the duplicate-id rule with the guard it bought is gone whole. None is dropped: all were filed at Plan as **#699**, **#698**, **#700** and **#701** (milestone
*Foundations*), each carrying its evidence and the question it has to settle — so a reviewer who
raises one has a tracker reference to be pointed at rather than a rebuild to trigger.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Mutation rerun cleared with 12 caught and 3 unviable, but rebuild must share each flat snapshot instead of cloning its N-entry map for each of Q obligations, or the prohibited Q×N CPU/heap path remains (`crates/custodian/src/reconstruction.rs:483`).; T4 Contribution — Human must confirm contribution and prior-art completeness — the driver-only `scripts/pdca` / `scripts/review-branch` and logs are absent, so merged affected-path history was checked but closed/rejected work cannot be mechanically settled.; T5 Judgment — Rebuild must cover many obligations in one large flat object: the current complexity leg seeds one chunk per separate flat object, so it cannot falsify the retained-map Q×N regression (`crates/custodian/tests/segmented_map_reconstruction.rs:620`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 15 mutants tested in 42s: 7 caught, 7 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Human must confirm prior-art and contribution completeness — affected-path merged history was checked (nearest relevant commits `3e05891` and `5f2f79f`), but the closed/rejected-work corpus and driver review output are absent, so that half cannot be mechanically settled.; T5 Judgment — Rebuild must make the complexity oracle falsify per-obligation full-map copying — the added leg uses only three chunks and asserts placements plus scan count, not clone/allocation cost, so a Q×N CPU/heap regression can still pass (`crates/custodian/tests/segmented_map_reconstruction.rs:700`, `crates/custodian/tests/segmented_map_reconstruction.rs:726`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: The three T4 batch-review blockers (seeded Tier-0 DST for the chained multi-commit CAS path, reconstruction.rs:298/:309, tests:568) are the DST-coverage class the brief pre-declares out of scope — record-reject them in review-rejected.md with the #699 reference (#699 point 3 explicitly assigns the DST cost to the slice that adopts the fleet-wide write rule, i.e. #682); each commit is version-conditional on scanned bytes, so a racing writer loses the CAS rather than corrupting. Do NOT build a DST leg. T2 shape overage (test file 678 vs 460-line cap): human accepts as fine — do not spend the round shrinking the file. The item that MUST be resolved is T5: the complexity oracle stayed green with an injected per-obligation `black_box(record.clone())` (tests:595/:607), so it cannot falsify the Q×N clone/encode work C5 flags (reconstruction.rs:307/:876/:891). Make the test observe full-map clone/rewrite cost — and eliminate the remaining per-obligation whole-record clone/encode path it should catch.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the T4 batch-review blockers (5 findings, 0 triaged) — fix or record-reject each: - Priority inversion (reconstruction.rs:314, :316): grouping all of an object's plans at its most urgent member's position runs lower-priority repairs ahead of more urgent ones in later objects. Restore chunk-level urgency ordering, or test it directly. - DST gaps (reconstruction.rs:834, :892, :896): the grouped multi-chunk version-conditional commit is a new concurrent/destructive path shipped without the rubric-required seeded Tier-0 DST coverage — and the brief said not to add a new concurrent path at all. Same root, must be resolved by the same fix: the adversary demonstrated that the grouped commit discards already-completed repairs when a later chunk's write faults (reconstruction.rs:867) — on the base chunk A commits and only B stays queued; with this patch a deterministic fault on B means A is never repaired (permanent non-convergence, the C-1 invariant this slice exists to restore). Committing chunks as completed before propagating the fault satisfies the brief's one-commit-per-object reading per the adversary. Undoing/simplifying the grouping machinery is also expected to clear the T2 shape FAIL (207 production lines vs the 160 hard cap, unwaived).
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 10 — carry-forward (from the previous attempt)
- Sign-off rationale: Do NOT rebuild the mechanism — the resolver-once restructure, the refusal path, and the write shape are accepted as-is. The residue is bounded pointer-work: 1. Record-reject T4 batch finding 1 (reconstruction.rs:300, "repairs commit while reading.incomplete") in review-rejected.md at the new line: it contradicts brief leg 3 (the healthy repair MUST land beside unreadable objects) and the duplicate-behind-unreadable guard is #700 DO-NOT-BUILD; same finding as round 4's :689 entry. 2. Record-reject T4 batch finding 2 (reconstruction.rs:423, unbounded await) at the new line: identical to the round-3 :482 rejection (peers gc.rs:394/restore.rs:604 make the same unbounded call, timeout would need a forbidden Cargo.toml change). 3. Narrow the overstated performance claim in the diff's comments (:154-158 "O(N) rows instead of O(Q×N)"): the SCAN is once per pass; per-repair clone/encode cost is base parity (adversary measured identical 8 passes on origin/main). Say exactly that — do not remove the clone (:861 is deliberately byte-identical to the base per build-notes §3(d)) and do not rebuild iteration 9's grouping. 4. Explicitly record-reject the C5/T5 demand for a test oracle that fails on full-map copying: copies are invisible through the MetadataStore/ChunkStore seams (adversary's own conclusion); reject it in review-rejected.md so it stops re-surfacing each round. 5. File the adversary's Ok(None) silent-drain race (scan sees Committed segmented root, resolver later answers Ok(None) → obligation drained, pass certifies, zero audit rows) as a tracker issue against #653/#682 — do NOT fix in-slice; the fix contradicts the brief-pinned "Ok(None) is skipped" rule and the path is unreachable until #653 lands. Reference the new issue in build-notes. 6. Leave the two C5 "missed" mutants alone — adversary showed both are equivalent mutants (fields re-supplied by ..object.prior.clone(), read_committed admits only Committed); note that in build-notes.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 22 mutants tested in 52s: 2 missed, 12 caught, 7 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v10/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 11 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrides the size backstop's iterate-plan recommendation: the remaining blockers are triage/closure work, not slice-shape problems. Next attempt must: - Record-reject the T4 finding at reconstruction.rs:302 (seeded Tier-0 DST leg for root-change-under-scan): the brief explicitly forbids building it — tracked as #699, crates/dst/ is out of bounds for this bundle. - Record-reject the T4 finding at reconstruction.rs:458 (caller-side timeout on scan/resolve awaits) on the same basis as the already-rejected duplicate at :474 in review-rejected.md, or add the bounded-await if that rejection basis does not hold. - Clear C5: kill or document as equivalent the 2 missed mutants (advisory review argues both equivalent — deleted size/state fields supplied unchanged by struct update; admitted records already Committed — make that case in the artifacts so the gate settles). - Re-run the adversary advisory leaf, which produced no artifact this round. Do not change the fix's behaviour or design — the six-leg discriminator and C4 red→green are passing; this round is gate closure only.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 22 mutants tested in 32s: 2 missed, 13 caught, 7 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v11/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
