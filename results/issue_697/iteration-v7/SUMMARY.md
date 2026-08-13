# Result — issue 697 / reconstruction-reads-through-resolver-once-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/reconstruction.rs` reads the chunk map inline out of the inode
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
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_reconstruction.rs`
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: reconstruction **reads every committed object through the resolver every other consumer
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 29s: 13 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #697: make reconstruction resolve the committed namespace once per non-empty pass, contain unreadable objects, and refuse segmented-map repairs without draining their obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is executable and fixes the affected behavior, complexity bound, refusal semantics, deferrals, and exact scope without leaving an architectural choice implicit. |
| C2 Reproduction (red pre-fix) | PASS | The base discriminator is behavioral: stashing only production left the test compiling with 1 pass and 6 assertion failures, while the empty-queue regression guard stayed green (`crates/custodian/tests/segmented_map_reconstruction.rs:464`). |
| C3 Change | PASS | The change stays on the two authorized reconstruction files and uses the shared resolver with per-object containment/refusal; none of the explicitly deferred write, identity, duplicate-claim, or documentation surfaces is changed (`crates/custodian/src/reconstruction.rs:463`). |
| C4 Verification (red→green) | PASS | Independent scratch runs reproduced 6-red/1-green → 7-green; the complete `cargo xtask ci` passed with a writable advisory cache, and 20 in-diff mutants yielded 13 caught and 7 unviable (`crates/custodian/tests/segmented_map_reconstruction.rs:464`). |
| C5 Causal adequacy | PASS | The repeated-scan cause is removed through one resolver-backed reading and one shared snapshot per matched flat object, with no capability probe or runtime symptom guard (`crates/custodian/src/reconstruction.rs:166`, `crates/custodian/src/reconstruction.rs:463`, `crates/custodian/src/reconstruction.rs:528`). |
| T1 Structure | PASS | The implementation preserves the narrow traits/core seams and existing resolver/audit vocabulary, with one production file and one integration-test file (`crates/custodian/src/reconstruction.rs:463`, `crates/custodian/tests/segmented_map_reconstruction.rs:102`). |
| T2 Shape | FAIL | Rebuild must honor the brief's hard budget: production adds 161 semantic lines against 160, and the new test reaches 731 raw / about 513 semantic lines against 460 / 280 (`crates/custodian/tests/segmented_map_reconstruction.rs:731`). |
| T3 Runtime | PASS | The runtime surface is metadata traversal rather than a new concurrent/destructive path; the real fenced control point, complete CI/DST, one-scan behavior, propagation, containment, refusal, and repair outcomes all passed (`crates/custodian/tests/segmented_map_reconstruction.rs:413`, `crates/custodian/tests/segmented_map_reconstruction.rs:590`, `crates/custodian/tests/segmented_map_reconstruction.rs:639`). |
| T4 Contribution | NEEDS-HUMAN | Human must confirm prior-art and contribution completeness — affected-path merged history was checked (nearest relevant commits `3e05891` and `5f2f79f`), but the closed/rejected-work corpus and driver review output are absent, so that half cannot be mechanically settled. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make the complexity oracle falsify per-obligation full-map copying — the added leg uses only three chunks and asserts placements plus scan count, not clone/allocation cost, so a Q×N CPU/heap regression can still pass (`crates/custodian/tests/segmented_map_reconstruction.rs:700`, `crates/custodian/tests/segmented_map_reconstruction.rs:726`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the in-memory plus DST evidence and refusal-until-#682 posture are sufficient for production durability convergence — acceptance determines whether segmented obligations may remain intentionally blocked (`crates/custodian/src/reconstruction.rs:341`). |

### Advisory — adversary

# Adversarial review — issue #697 (advisory, non-gating)

Toolchain was available (`cargo 1.96.0`); the red→green was re-run end to end in a scratch
copy of `$PDCA_TARGET`, and `cargo mutants --in-diff` was reproduced. No verdict below is
provisional for want of tools.

## What I could confirm (so the refutations below are not noise)

- **Red→green is real, and on the production path.** Base `339da46`'s
  `crates/custodian/src/reconstruction.rs` + the new test → **6 of 7 legs fail
  behaviourally** (`SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }`,
  and leg 7's `left: ([[0,2],[0,1],[0,1]], 2)`); patched → **7/7 green**. No compile
  break, so the red is behavioural, not the exit-77 degradation the brief warned about.
  Every leg drives `reconcile_step` (`crates/custodian/tests/segmented_map_reconstruction.rs:1042`),
  the real fenced entry, over the real `reconstruction::reconcile` — not a re-implementation.
- `cargo test -p wyrd-custodian` (all 16 binaries) and `-p wyrd-server --test custodian_day_one`
  are green with the patch; `crates/custodian/tests/reconstruction.rs` is untouched and still 15/15.
- `cargo mutants --in-diff` reproduces exactly: **20 mutants, 13 caught, 7 unviable, 0 missed**.
  A wider run over `reconstruction.rs`'s touched functions surfaces 9 missed mutants, but all
  of them sit on lines the diff never touched (`:213`, `:225`, `:231`, `:693`, `:711`, `:857`)
  — pre-existing debt, not this patch's.
- The containment core (`reconstruction.rs:472-511`) is byte-for-byte the `gc.rs:365-416` /
  `restore.rs:625-657` shape, decode-before-`state`, `Ok(None)` skipped, `ChunkMapError`
  downcast contained and everything else propagated. The emitters
  (`reconstruction.rs:1029-1069`) mirror `gc.rs:562-571` / `restore.rs:826-835` field for field.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:498-535` (leg 2)
  does not bind "a refusal blocks nothing but itself", and I broke the fix without turning a
  single test red.** Insert, immediately above `crates/custodian/src/reconstruction.rs:294`
  (`let mut changed = false;`), the four lines
  `if !reading.refused.is_empty() { return Ok(Reconciled::Blocked); }` — i.e. "a refusal means
  the pass cannot certify, so don't bother writing". All **7 legs stay green**, the whole
  `cargo test -p wyrd-custodian` suite stays green (89 tests), and `-p wyrd-server --test
  custodian_day_one` stays green (15 tests). That tree stops **every flat repair and every
  drain store-wide** the moment one multipart object owes a chunk — the exact defect this slice
  exists to remove, wearing a different hat. The hole is a fixture-compression artefact: leg 1
  (`:465`) has flat work but no refusal, leg 2 (`:498`) has a refusal but *only* the segmented
  object beside it, so nothing in the bundle ever puts the two in one pass. The brief's own
  §Scope answer rule 1 claims this property is "Bound by legs 1 and 2" — it is not.
  Cheap fix: add `seed(&meta, &d0, 2, UnderReplicated { chunks: &FLAT[..1] })` +
  `enqueue(&meta, &[FLAT[0], DELETED])` to leg 2 and assert the flat repoint landed and
  `DELETED` was drained, exactly as leg 3 does for the *incomplete* case.

- **NEEDS-HUMAN [human] — the test file is 59% over the brief's hard raw ceiling and 83% over
  its semantic one, and the brief pre-declared that as a stop condition, not a stretch.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:1` — **731 raw lines** against a
  budget of **≤ 460 raw / ≤ 280 semantic** (513 semantic by a non-blank/non-comment count).
  brief.md §Budget: *"a test file past 460 raw means the shape is wrong: **STOP and hand back
  rather than finish**."* The production side is 161 added semantic lines against ≤ 160 — that
  one is counting-method noise and I would not raise it alone; the test file is not. This is
  the same over-run signature the brief's own §Ordering note says got rounds 1–5 rejected, and
  it is not something a reviewer may wave through as "the legs needed it": seven legs shipped
  where six were briefed (see next bullet), which accounts for part of the overrun.

- **NEEDS-HUMAN [human] — the intra-pass generation chaining at
  `crates/custodian/src/reconstruction.rs:308-316` and `:893` is unbriefed scope, and it
  falsifies the brief's stated basis for shipping no seeded Tier-0 DST leg.** The patch keeps a
  `HashMap<usize, InodeRecord>` of records *this pass wrote* and conditions each object's second
  and later repairs on them (`:310` `repaired.get(&plan.object).unwrap_or(plan.prior)`; `:893`
  `.require(inode_key.clone(), metadata::encode(prior))`). Three problems, none of which the
  builder can settle alone:
  1. brief.md §Verification posture justifies "no seeded Tier-0 DST case ships in this child, and
     none is owed" with *"Every write it performs is on a flat record resolved by borrow from the
     generation the scan returned … committed under the base's own **unmodified**
     version-conditional CAS (`:598-608`)"*. After this patch that is no longer true for writes
     2..Q inside one object: the CAS precondition is a record synthesized in memory, never read
     back from the store. The rubric's *Test fidelity* class ("a new destructive or concurrent
     path lands with seeded Tier-0 DST coverage") now has a surface it did not have when the
     posture was written.
  2. brief.md §Scope, the #698 carve-out, pins `repair_chunk:598-601` to stay **byte-identical**
     and `RepairPlan` to keep its base field `prior: InodeRecord`. Base `:600` was
     `.require(inode_key.clone(), metadata::encode(&plan.prior))`; the patch changed it, and
     `RepairPlan.prior` became `&'a InodeRecord` (`reconstruction.rs:125`). Not #698's fix, but
     not the frozen line either.
  3. Leg 7 (`:699`) exists only to bind this mechanism, and its base red
     (`([[0,2],[0,1],[0,1]], 2)` — I reproduced it) demonstrates a *pre-existing, unbriefed*
     defect: on `origin/main` a pass repairs only the first obligation per object and the rest
     lose their CAS. Real, and worth fixing — but it is not #697's defect, it is not in the
     brief's six legs, and shipping it here means part of this bundle's red→green evidence is
     about a different bug. A human should say whether it lands here or as its own issue.
  I could not turn the chaining into a *wrong* answer: I walked the CAS through same-pass
  supersede, external racer, `Conflict` and `Aborted` first-repair, plans re-ordered by
  `repair_priority`, and two keys parsing to one `InodeId` — every path either commits the
  right bytes or fails closed to `Conflict` with the obligation queued. The objection is scope
  and evidence, not correctness.

## Attempted and could not refute

- **Over-containment.** I could not find an error class the patch swallows that it should
  propagate: the downcast at `reconstruction.rs:504-511` is `gc.rs:402-416` verbatim, and leg 5
  (`:638`) genuinely injects on `scan_page(b"seg:")` — the resolver's read — not on
  `scan(b"inode:")`, so it is a real over-containment guard and not a tautology.
- **Silent drain.** Every route into `drain_only` passes the one `!reading.incomplete` gate at
  `:333`; there is no second drain site to drift. Leg 3 (`:543`) binds it and would go red if
  the gate were dropped.
- **Index/shape confusion.** `resolved.chunks` for a flat snapshot is `Cow::Borrowed` of the
  record's own list (`crates/core/src/metadata.rs:2585`), so `FlatSite::index` addresses the very
  list `repair_chunk` mutates; a segmented snapshot's restart-onto-a-newer-root can never reach a
  write because write-eligibility is read off the **scanned** `record.chunk_map`
  (`reconstruction.rs:507-511`), exactly as §Scope demands.
- **Memory / complexity regression.** `Reading::objects` holds one record per object *owed* a
  repair; base held one full `InodeRecord` clone per *obligation* inside `RepairPlan`, so this is
  strictly less. The per-pass Θ(S) `seg:` resolves for objects nothing is owed on are real but
  explicitly blessed ("≤ S") by the brief.
- **Duplicate `ChunkId` / non-canonical `inode:` keys.** Both reachable only through #700 / #698,
  both carved out, and neither is made worse: first-committed-reference-in-key-order is preserved
  (`:527`) and a key that will not parse is skipped exactly where the base's `find_chunk` skipped
  it.
- **Audit vocabulary drift.** `unresolvable-chunk-map` / `reconstruction_unresolvable_records` and
  `refused-segmented` / `reconstruction_refused_records` match the pinned set exactly; no docs
  table anywhere in the repo enumerates the sibling counters, so no docs-currency debt is owed.

## On the gate rows

- `check-gates.json:48` reports C4-verify as *"red without the fix, green with it (**7** test(s)
  ran red)"*. Only **6** go red on the base; leg 6 (`:667`, empty queue) is green there by
  design and the brief says so. If that string is meant as "7 tests went red" it overstates the
  evidence by one leg; if it means "7 tests ran in the red leg" it is fine. Worth one word from
  whoever owns `run-verify.sh`, not a refutation of the fix.
- `check-gates.json:94` — `T4 batched multi-pass rubric review` is **gating and `fail`** (3
  blocking), and `overall` is `fail`. Nothing in this advisory should be read as clearing that.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Human must confirm prior-art and contribution completeness — affected-path merged history was checked (nearest relevant commits `3e05891` and `5f2f79f`), but the closed/rejected-work corpus and driver review output are absent, so that half cannot be mechanically settled.
- [ ] T5 Judgment — Rebuild must make the complexity oracle falsify per-obligation full-map copying — the added leg uses only three chunks and asserts placements plus scan count, not clone/allocation cost, so a Q×N CPU/heap regression can still pass (`crates/custodian/tests/segmented_map_reconstruction.rs:700`, `crates/custodian/tests/segmented_map_reconstruction.rs:726`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the in-memory plus DST evidence and refusal-until-#682 posture are sufficient for production durability convergence — acceptance determines whether segmented obligations may remain intentionally blocked (`crates/custodian/src/reconstruction.rs:341`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Human must confirm prior-art and contribution completeness — affected-path merged history was checked (nearest relevant commits `3e05891` and `5f2f79f`), but the closed/rejected-work corpus and driver review output are absent, so that half cannot be mechanically settled.; T5 Judgment — Rebuild must make the complexity oracle falsify per-obligation full-map copying — the added leg uses only three chunks and asserts placements plus scan count, not clone/allocation cost, so a Q×N CPU/heap regression can still pass (`crates/custodian/tests/segmented_map_reconstruction.rs:700`, `crates/custodian/tests/segmented_map_reconstruction.rs:726`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
