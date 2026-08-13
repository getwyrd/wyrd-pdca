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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 22 mutants tested in 52s: 2 missed, 12 caught, 7 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #697: make reconstruction resolve committed chunk maps once per non-empty pass, contain unreadable objects, and refuse segmented repairs without draining obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief unambiguously binds one resolver-backed reading, per-object containment/refusal, no draining across an incomplete reading, and six observable legs. |
| C2 Reproduction (red pre-fix) | PASS | Independent production-stash run produced five behavioral failures while the declared empty-queue guard stayed green; the discriminator drives the public fenced control point at `crates/custodian/tests/segmented_map_reconstruction.rs:451`. |
| C3 Change | PASS | The scoped implementation exists in exactly the two required files and non-empty passes now construct one resolver-backed reading before assessment at `crates/custodian/src/reconstruction.rs:164`. |
| C4 Verification (red→green) | PASS | Independent rerun was five-of-six red before the fix and six-of-six green after it; the real Rust, docs, dependency, conformance, statics, and DST checks passed after giving `cargo-deny` a writable scratch cache. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must remove the per-plan full-map clone/encode path—every plan enters `repair_chunk` at `crates/custodian/src/reconstruction.rs:303`, then copies the whole map at `crates/custodian/src/reconstruction.rs:861`—or the claimed Q×N work bound remains false. |
| T1 Structure | PASS | `Reading` separates one namespace walk, shared flat snapshots, per-chunk sites, incompleteness, and per-object refusals without widening trait seams at `crates/custodian/src/reconstruction.rs:355`. |
| T2 Shape | PASS | The change remains exactly two scoped files and 143 added production semantic lines; the carried-forward brief explicitly accepts the test-file size overage. |
| T3 Runtime | FAIL | Q obligations in one N-entry object each clone the full map and encode full prior/next records before their CAS at `crates/custodian/src/reconstruction.rs:861` and `crates/custodian/src/reconstruction.rs:874`, retaining Q×N CPU/heap work. |
| T4 Contribution | NEEDS-HUMAN | Human must confirm closed/rejected-work prior art—merged affected-path history was checked, but the closed/rejected corpus and driver review output were not supplied, so contribution uniqueness is not mechanically settled. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add an oracle that fails on per-obligation full-map copying—the eight-chunk fixture and scan counters at `crates/custodian/tests/segmented_map_reconstruction.rs:597` and `crates/custodian/tests/segmented_map_reconstruction.rs:610` observe placement and reads, not clone/encode cost. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the remaining Q×N CPU/heap path is acceptable at production object and queue sizes, because that determines whether reconstruction can converge at the scale this fix is meant to protect. |

### Advisory — adversary

# Adversarial review — issue #697 (advisory, non-gating)

Red→green reproduced independently in a scratch clone (`cargo test -p wyrd-custodian --test
segmented_map_reconstruction`): 6/6 green with the patch, 5/6 fail behaviourally with
`reconstruction.rs` reverted (leg 6 green on base, as declared). Whole `wyrd-custodian` suite
green, `crates/custodian/tests/reconstruction.rs` unmodified. Two refutations landed; the rest
of the attack surface held.

## Findings

- **NEEDS-HUMAN [human] — an obligation is silently DRAINED, and the pass falsely certifies, when
  its object retires under the read.** `crates/custodian/src/reconstruction.rs:472`
  (`Ok(None) => continue`) drops the object without marking the reading incomplete;
  `:596` then classifies every chunk that object held as `Assessment::Drain`; `:323` gates the
  drain batch on `reading.incomplete` **only**, so the delete goes through; `:331` answers
  `Changed`/`Satisfied`. Concrete failing case, executed against this exact patch (in-memory
  doubles, `MetadataStore::get` answering the root that a concurrent writer left behind):
  the scan returns a **committed segmented** root at `inode:1` holding queued chunk `0xA100`;
  by the time `resolve_chunk_map` re-reads that root it carries a **Pending** generation, so
  `crates/core/src/metadata.rs:2663-2665` answers `Ok(None)`. Result: `queued == []` — the
  obligation was **deleted** — with **zero** `refused-segmented` and **zero**
  `unresolvable-chunk-map` rows, and the pass answered **`Reconciled::Changed`**. That is
  exactly "an obligation discarded for want of a reading" and a certification over an object the
  pass never read — the two invariants brief §Invariant says this slice exists to restore, and
  the base could not reach it (a segmented record ended the pass with `Err`, draining nothing).
  Why a human, not a rebuild: the obvious fix (treat `Ok(None)` on a record the *scan* saw
  `Committed` as "retired under the read" → `incomplete`) contradicts a **pinned** brief rule
  ("`Ok(None)` from the resolver is **skipped**, exactly as both merged peers skip it — not
  counted, not named"), and today's reachability is bounded by two facts that will not hold for
  long: the segmented shape has no producer in this build (`crates/core/src/metadata.rs:1460-1463`,
  #653) and nothing writes a `Pending` inode root. Decide: fix in-slice, or file it against
  #653/#682 before either lands.

- **NEEDS-HUMAN [human] — leg 4 is not a Q×N oracle; it is a scan counter, and the Q×N work it
  is claimed to forbid walks straight past it.** `crates/custodian/tests/segmented_map_reconstruction.rs:611-616`
  and `:642` assert only `MemMeta::inode_scans == 1`. I injected the prohibited per-obligation
  whole-record copy at `crates/custodian/src/reconstruction.rs:603` —
  `std::hint::black_box(metadata::encode(&reading.objects[site.object].prior.clone()))`, i.e. a
  full N-entry map clone **and** re-encode for each of the Q obligations — and **all six legs
  stayed green**. So the diff's headline claim ("O(N) rows instead of O(Q×N)", `:154-158`) is
  bound only against re-scanning, not against the clone/encode path, and a future rebuild can
  reintroduce Q×N heap/CPU without a single test going red. This is the item iteration 8's
  sign-off recorded as the one that "MUST be resolved" ("Make the test observe full-map
  clone/rewrite cost"); the *implementation* half was done (the `object: usize` index at `:304`
  / `:521` really does share one snapshot — I confirmed that by reading, not by the test), the
  *oracle* half was not. Human call because it may not be bindable through the `MetadataStore` /
  `ChunkStore` seams at all (copies are invisible there): either accept an allocation-counting
  oracle, or **record-reject the demand explicitly** so it stops re-surfacing each round — do not
  leave it silently unmet a third time.

- **NEEDS-HUMAN [human] — the C-1 "work bounded by the obligations, not their product with the
  namespace" claim is true per pass and false per convergence, and this diff's own leg now
  codifies the difference as expected.** `crates/custodian/tests/segmented_map_reconstruction.rs:636-645`
  asserts convergence takes up to `OWED.len()` passes. Measured on this patch: 8 obligations
  inside one 8-chunk flat object need **8 passes**, and after pass 1 the rebuild target already
  holds all 8 rebuilt fragments — every pass erasure-rebuilds and uploads a fragment for every
  not-yet-landed obligation and then throws all but one away on the CAS (`reconstruction.rs:304-308`,
  one version-conditional commit per chunk). Aggregate cost to drain a queue concentrated in one
  object is therefore Θ(Q) namespace scans and Θ(Q²) fragment rebuilds/uploads. **This is base
  parity — I ran the same fixture against `origin/main:crates/custodian/src/reconstruction.rs` and
  got the identical 8 passes — so it is NOT a regression and the grouping machinery must NOT be
  rebuilt** (iteration 9's sign-off settled that route). The finding is against the *claim*: brief
  §Invariant's fifth bullet reads as a convergence property and only the per-pass property was
  delivered. Narrow the claim, or track the residue.

## Attempted and could not refute

- **C5's two "missed" mutants are equivalent mutants, not a coverage gap.** `reconstruction.rs:864`
  (`size: object.prior.size`) and `:866` (`state: InodeState::Committed`) are both re-supplied by
  the struct-update tail `..object.prior.clone()` at `:871`, and `read_committed` admits only
  `Committed` records (`:467`), so deleting either field is behaviour-preserving. No test can kill
  them; the `C5-mutants` red is noise here.
- **Per-object refusal accounting holds.** My first probe showed one row for two segmented objects
  — that was my fixture reusing one `SegmentGroup` nonce. With two genuinely distinct groups the
  pass emits 2 `refused-segmented` rows, 2 counter ticks, and names both `inode:1` and `inode:2`.
- **A refusal does not starve the flat work beside it**: refusal + under-replicated flat chunk in
  one pass → the flat repoint lands, the refused obligation stays queued, `Blocked`, `(scan,
  scan_page) == (1, 1)`.
- **Both drain paths really are behind one gate.** The "already at full redundancy" drain
  (`reconstruction.rs:688-691`) is also suppressed under an incomplete reading — verified by a
  second pass over a store carrying an undecodable record; the obligation was kept.
- Also probed without success: priority ordering (`:260` unchanged, no inversion), first-committed-
  reference-wins parity with the base's `find_chunk`, the unparsable-key `continue` at `:497`
  (base-identical), an undecodable *uncommitted* row under `inode:` (contained per the pinned
  gc.rs rule), index validity of `plan.object` after the priority sort, and the flat CAS
  precondition (`:871-877`, byte-identical construction to the base).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must remove the per-plan full-map clone/encode path—every plan enters `repair_chunk` at `crates/custodian/src/reconstruction.rs:303`, then copies the whole map at `crates/custodian/src/reconstruction.rs:861`—or the claimed Q×N work bound remains false.
- [ ] T4 Contribution — Human must confirm closed/rejected-work prior art—merged affected-path history was checked, but the closed/rejected corpus and driver review output were not supplied, so contribution uniqueness is not mechanically settled.
- [ ] T5 Judgment — Rebuild must add an oracle that fails on per-obligation full-map copying—the eight-chunk fixture and scan counters at `crates/custodian/tests/segmented_map_reconstruction.rs:597` and `crates/custodian/tests/segmented_map_reconstruction.rs:610` observe placement and reads, not clone/encode cost.
- [ ] Validation — fitness-to-purpose — Human must decide whether the remaining Q×N CPU/heap path is acceptable at production object and queue sizes, because that determines whether reconstruction can converge at the scale this fix is meant to protect.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- [ ] size backstop — this slice is behaving oversized: 4 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Do NOT rebuild the mechanism — the resolver-once restructure, the refusal path, and the write shape are accepted as-is. The residue is bounded pointer-work: 1. Record-reject T4 batch finding 1 (reconstruction.rs:300, "repairs commit while reading.incomplete") in review-rejected.md at the new line: it contradicts brief leg 3 (the healthy repair MUST land beside unreadable objects) and the duplicate-behind-unreadable guard is #700 DO-NOT-BUILD; same finding as round 4's :689 entry. 2. Record-reject T4 batch finding 2 (reconstruction.rs:423, unbounded await) at the new line: identical to the round-3 :482 rejection (peers gc.rs:394/restore.rs:604 make the same unbounded call, timeout would need a forbidden Cargo.toml change). 3. Narrow the overstated performance claim in the diff's comments (:154-158 "O(N) rows instead of O(Q×N)"): the SCAN is once per pass; per-repair clone/encode cost is base parity (adversary measured identical 8 passes on origin/main). Say exactly that — do not remove the clone (:861 is deliberately byte-identical to the base per build-notes §3(d)) and do not rebuild iteration 9's grouping. 4. Explicitly record-reject the C5/T5 demand for a test oracle that fails on full-map copying: copies are invisible through the MetadataStore/ChunkStore seams (adversary's own conclusion); reject it in review-rejected.md so it stops re-surfacing each round. 5. File the adversary's Ok(None) silent-drain race (scan sees Committed segmented root, resolver later answers Ok(None) → obligation drained, pass certifies, zero audit rows) as a tracker issue against #653/#682 — do NOT fix in-slice; the fix contradicts the brief-pinned "Ok(None) is skipped" rule and the path is unreachable until #653 lands. Reference the new issue in build-notes. 6. Leave the two C5 "missed" mutants alone — adversary showed both are equivalent mutants (fields re-supplied by ..object.prior.clone(), read_committed admits only Committed); note that in build-notes.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
