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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 24 mutants tested in 31s: 14 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #697: make reconstruction resolve committed chunk maps once per non-empty pass, contain unreadable objects, refuse segmented writes, and preserve repair obligations and certification.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The containment, refusal, no-drain, complexity, fault-propagation, and empty-queue decisions are explicit and independently testable through the public fenced entry point (`crates/custodian/tests/segmented_map_reconstruction.rs:487`). |
| C2 Reproduction (red pre-fix) | PASS | On clean base `339da46` with only the added test retained, all 6 tests compiled and ran: the 5 binding legs failed behaviorally and the declared empty-queue regression guard passed (`crates/custodian/tests/segmented_map_reconstruction.rs:487`). |
| C3 Change | PASS | The scoped decision is implemented at the single resolver walk, incomplete-reading drain gate, and per-object refusal seam without a capability probe (`crates/custodian/src/reconstruction.rs:468`, `crates/custodian/src/reconstruction.rs:338`, `crates/custodian/src/reconstruction.rs:550`). |
| C4 Verification (red→green) | PASS | The patched 6-leg test is green and a full `cargo xtask ci` rerun passed; the initial read-only advisory-DB lock was a host caveat and `cargo deny` plus the full gate passed with a writable scratch DB (`crates/custodian/tests/segmented_map_reconstruction.rs:487`). |
| C5 Causal adequacy | PASS | The eager per-obligation namespace/map lookup is removed rather than guarded: one resolver-backed reading supplies every assessment, and 24 diff mutants reran as 14 caught and 10 unviable (`crates/custodian/src/reconstruction.rs:468`, `crates/custodian/src/reconstruction.rs:606`). |
| T1 Structure | PASS | The change stays in the two scoped files, reuses the resolver and object-name seams, and adds a public-entry integration test without dependency, API, or documentation expansion (`crates/custodian/src/reconstruction.rs:468`, `crates/custodian/tests/segmented_map_reconstruction.rs:487`). |
| T2 Shape | FAIL | Rebuild must meet or obtain an explicit waiver for the unwaived production cap: the patch adds 207 nonblank, non-comment production lines against the hard maximum of 160; the carry-forward waiver names only the test-file overage (`crates/custodian/src/reconstruction.rs:363`). |
| T3 Runtime | PASS | The six specified in-memory outcomes, backend-fault propagation, existing workspace tests, clippy/build, docs, typos, dependency scanners, and existing DST suite all ran green (`crates/custodian/tests/segmented_map_reconstruction.rs:566`, `crates/custodian/tests/segmented_map_reconstruction.rs:698`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect and disposition the 5 opaque batch-review blockers and confirm closed/rejected affected-path prior art — `scripts/review-branch` and its driver output are absent here, while merged history was independently checked through `3e05891` and `5f2f79f` (`crates/custodian/src/reconstruction.rs:143`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must preserve or directly test chunk-level urgency: grouping by first-seen object moves that object's less-urgent plans ahead of more-urgent plans in later objects, while every added fixture fixes `K = M = 1` and cannot expose the reordering (`crates/custodian/src/reconstruction.rs:259`, `crates/custodian/src/reconstruction.rs:303`, `crates/custodian/tests/segmented_map_reconstruction.rs:255`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the in-memory proof is sufficient for this durability-path batching change and whether to trigger `cargo xtask disk-faults` / `cargo xtask kill-reconstruct` — local CI leaves those real-environment scenarios ignored, so production fault fitness remains a sign-off judgment (`crates/custodian/src/reconstruction.rs:834`). |

### Advisory — adversary

# Adversarial review — issue 697 (advisory, never gating)

Re-ran the asserted red→green myself in a throwaway copy of `$PDCA_TARGET` (scratch, since
removed): green = 6/6 pass with the patch; red (production `reconstruction.rs` reverted to
`339da46`, test kept) = **5 failed, 1 passed**, every failure behavioural
(`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })`), not a compile
error. The whole `-p wyrd-custodian` suite is green on the patched tree, and
`crates/custodian/tests/reconstruction.rs` is untouched. The evidence is real; the legs drive
`reconcile_step`, not a helper. Two things survive the pass anyway.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:867` (the `?` on `repair_chunk`
  inside `repair_object`'s plan loop) discards *already-completed* repairs in the same object,
  turning a one-chunk write fault into permanent non-convergence for its neighbours.** Because
  the object's single repoint commit (`:896`) now happens only *after* every plan in the group
  has been rebuilt, a `put_fragment` error on a later chunk propagates out of `repair_object`
  and out of `reconcile` (`:318`) before the earlier chunks' successful rebuilds are ever
  committed. Concrete case, demonstrated (not argued): flat `inode:1` with two under-replicated
  chunks `A=0xA100`, `B=0xA200`, RS(1,1), survivor on d0, rebuild target d2; d2 accepts A's
  fragment and persistently rejects B's (`std::io::Error::other`, i.e. transient class, so it
  propagates by design per `:673-674`). On `origin/main` the pass returns `Err` **after** A is
  repaired — `inode:1` `version=2`, `placement=[[0,2],[0,1]]`, only B still queued. With this
  patch the pass returns the same `Err` but `version=1`, `placement=[[0,1],[0,1]]`, **both**
  still queued — and since the fault is deterministic every subsequent pass repeats it
  identically, so A is *never* repaired and each pass strands a fresh unreferenced rebuilt
  fragment on d2 whose `orphan:` mark rode the batch that never committed. That is a repair
  loop that stops converging for a reason inside the loop, i.e. exactly the C-1 permanence the
  brief invokes — introduced by this diff, not pre-existing. It is not a scope question: the
  brief only requires *one commit per object*, so committing the chunks already rebuilt before
  propagating the fault (or classifying a per-chunk store fault as that chunk's abort) satisfies
  it. No leg covers a chunk-store fault at all, which is why it went unseen.
- **NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_reconstruction.rs:652-660` still
  cannot falsify the per-obligation whole-record clone, the exact regression the iteration-8
  sign-off named as the item that MUST be resolved.** I re-ran the sign-off's own injection —
  `let _ = std::hint::black_box(object.prior.clone());` at the head of `repair_object`'s plan
  loop (`reconstruction.rs:866`), i.e. a Q×N heap/CPU copy of the eight-entry map for each of the
  four obligations inside `inode:2` — and all six legs stayed **green**. The new `rewrites`
  counter binds the *encode/commit* half only (it charges bytes that cross the store seam), and
  an in-process clone crosses no seam. In fairness the production half **is** done — `RepairPlan`
  now carries `object: usize` (`:112-124`) and no per-obligation record copy remains anywhere on
  the path — so this is a regression-guard gap, not a live defect. The judgment a human owes:
  accept the seam-visible oracle as the achievable bound, or require a test-binary allocation
  probe. Routing it back to Do unqualified risks a fourth round chasing a property no black-box
  test over trait doubles can observe.
- `check-gates.json` C4-verify records *"red without the fix, green with it (6 test(s) ran red)"*.
  Measured: **5** red, 1 green — `an_empty_queue_reads_nothing_and_answers_satisfied` passes on
  the base, exactly as brief §Success-criterion leg 6 pre-declares. The verdict is right; the
  count in the row is not, and a reader taking it at face value would believe leg 6 is a
  behavioural red it is not. Not raised as NEEDS-HUMAN: harness phrasing, no bearing on the fix.
- The test file is **743 raw lines** against the brief's `≤ 460 raw` STOP threshold
  (`brief.md:315-320`); the iteration-8 sign-off accepted 678 explicitly ("do not spend the round
  shrinking the file") and it has grown 65 lines since. Recorded, not raised — the deferral is
  settled per the target rubric's *Deferrals are settled*.

## Refutations attempted that failed

- *Index aliasing between the resolver's answer and the scanned record.* `site.index` comes from
  `resolved.chunks` (`reconstruction.rs:524`) but `repair_object` indexes `object.prior`'s own
  list (`:871`). For a flat map `resolve_snapshot` returns `Cow::Borrowed(&record.chunk_map)`
  (`crates/core/src/metadata.rs:2585`) and cannot restart, so the two are one slice; a segmented
  snapshot — the only one that can be `Superseded` onto a different list (`:2629`) — is refused
  before an index is taken (`:509-516`). No mismatch, no panic path.
- *An unbounded `WriteBatch` per object* (the hazard `restore.rs:95-102` and `:414-424` bound with
  `MARK_BATCH = 1_000` for the same backend). Wrong here: a flat chunk map is one metadata value
  and is capped by `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:327`) — which is why
  segmented maps exist — so the group is a couple of thousand chunks at worst and the batch stays
  a few hundred KB, well inside FDB's transaction limit.
- *First-match-wins drift from the base.* `read_committed`'s `reading.sites.contains_key` guard
  (`:528`) and the unparseable-key `continue` (`:514`) reproduce `find_chunk`'s choice on the base
  row-for-row, including the base's own "skip the record, let a later one claim the chunk" on an
  unparseable key. Duplicate-id behaviour is byte-for-byte the base's (#700, settled).
- *A hole in the drain rule.* Both drain paths — the missing-site miss (`:613`) and
  already-at-full-redundancy (`:707`) — flow into the one `drain_only` batch gated by
  `!reading.incomplete` (`:338`), so no site can drift; a refusal correctly does **not** gate it,
  since a read succeeded.
- *Containment wider or narrower than gc's rule.* `:477-501` is line-for-line `gc.rs:378-416`,
  including containing a decode failure before the `state` check and propagating a non-
  `ChunkMapError` downcast; leg 5 pins the propagation and the already-emitted name.
- *`inode:` prefix pollution* (rows under `inode:` that are not records, which would silently
  mark every reading incomplete): `metadata::inode_key` (`crates/core/src/metadata.rs:33-36`) is
  the only writer and there is no sub-namespace.
- *A refusal writing something, or refusal accounting being per chunk.* Leg 2 compares every
  non-`repair:` row byte-for-byte and asserts exactly one `refused-segmented` row for two
  obligations in one object; `reading.refused` is keyed by the store's own key bytes (`:549`).
- *Vocabulary drift.* `action = "unresolvable-chunk-map"` matches `gc.rs:567`, `restore.rs:830`,
  `scrub.rs:233`, `desired_state.rs:263` field-for-field including `fault`.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Human must inspect and disposition the 5 opaque batch-review blockers and confirm closed/rejected affected-path prior art — `scripts/review-branch` and its driver output are absent here, while merged history was independently checked through `3e05891` and `5f2f79f` (`crates/custodian/src/reconstruction.rs:143`).
- [ ] T5 Judgment — Rebuild must preserve or directly test chunk-level urgency: grouping by first-seen object moves that object's less-urgent plans ahead of more-urgent plans in later objects, while every added fixture fixes `K = M = 1` and cannot expose the reordering (`crates/custodian/src/reconstruction.rs:259`, `crates/custodian/src/reconstruction.rs:303`, `crates/custodian/tests/segmented_map_reconstruction.rs:255`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the in-memory proof is sufficient for this durability-path batching change and whether to trigger `cargo xtask disk-faults` / `cargo xtask kill-reconstruct` — local CI leaves those real-environment scenarios ignored, so production fault fitness remains a sign-off judgment (`crates/custodian/src/reconstruction.rs:834`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- [ ] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rejected on the T4 batch-review blockers (5 findings, 0 triaged) — fix or record-reject each: - Priority inversion (reconstruction.rs:314, :316): grouping all of an object's plans at its most urgent member's position runs lower-priority repairs ahead of more urgent ones in later objects. Restore chunk-level urgency ordering, or test it directly. - DST gaps (reconstruction.rs:834, :892, :896): the grouped multi-chunk version-conditional commit is a new concurrent/destructive path shipped without the rubric-required seeded Tier-0 DST coverage — and the brief said not to add a new concurrent path at all. Same root, must be resolved by the same fix: the adversary demonstrated that the grouped commit discards already-completed repairs when a later chunk's write faults (reconstruction.rs:867) — on the base chunk A commits and only B stays queued; with this patch a deterministic fault on B means A is never repaired (permanent non-convergence, the C-1 invariant this slice exists to restore). Committing chunks as completed before propagating the fault satisfies the brief's one-commit-per-object reading per the adversary. Undoing/simplifying the grouping machinery is also expected to clear the T2 shape FAIL (207 production lines vs the 160 hard cap, unwaived).
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
