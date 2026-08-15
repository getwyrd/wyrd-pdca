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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #697: make reconstruction resolve committed chunk maps once per non-empty pass, contain per-object read faults, and refuse segmented writes without draining obligations or certifying incomplete work.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the first-match, refusal, no-drain, certification, containment, and unchanged flat-write decisions tightly enough to judge the existing reconciliation entry point (`crates/custodian/src/reconstruction.rs:144`). |
| C2 Reproduction (red pre-fix) | PASS | With only production changes stashed, 5 of 6 discriminator tests failed by behavioral assertions while the declared empty-queue regression guard stayed green (`crates/custodian/tests/segmented_map_reconstruction.rs:450`). |
| C3 Change | PASS | One conditional reading now feeds every assessment, typed per-object faults are contained, and segmented obligations are refused rather than repaired or drained (`crates/custodian/src/reconstruction.rs:164`, `crates/custodian/src/reconstruction.rs:455`, `crates/custodian/src/reconstruction.rs:593`). |
| C4 Verification (red→green) | PASS | Restoring production made all 6 discriminator tests green, and every underlying `cargo xtask ci` stage passed independently after moving `cargo-deny` state out of the host's read-only cache (`crates/custodian/tests/segmented_map_reconstruction.rs:425`). |
| C5 Causal adequacy | PASS | The resolver-once index removes the repeated namespace-scan cause directly; the rerun's two missed mutants are equivalent struct-update omissions because `size` and committed `state` are inherited unchanged (`crates/custodian/src/reconstruction.rs:455`, `crates/custodian/src/reconstruction.rs:868`). |
| T1 Structure | PASS | Private `Reading`, `Site`, and `FlatObject` types keep the new bookkeeping behind the existing reconciler interface and retain one shared flat snapshot per relevant object (`crates/custodian/src/reconstruction.rs:355`, `crates/custodian/src/reconstruction.rs:386`). |
| T2 Shape | PASS | The patch touches only the two declared files and adds 143 semantic production lines; the recorded human override settles the discriminator file's 712-line size overage (`crates/custodian/tests/segmented_map_reconstruction.rs:1`). |
| T3 Runtime | PASS | Tests drive the real fenced `reconcile_step`, while full workspace and 50-seed DST runs pass and the metadata trait contract assigns bounded waiting to each backend (`crates/custodian/tests/segmented_map_reconstruction.rs:390`, `crates/traits/src/lib.rs:1000`). |
| T4 Contribution | NEEDS-HUMAN | Human must decide whether contribution and prior-art review are complete — merged affected-path history was checked, but absent driver tools `scripts/review-branch` / `scripts/pdca` and the unavailable closed/rejected corpus prevent auditing the reported two batch-review blockers. |
| T5 Judgment | PASS | The six legs explicitly bind successful control-point execution, per-object audit/refusal, incomplete-reading drain safety, scan count, convergence, error propagation, and empty-queue behavior with no surviving implementation gap found (`crates/custodian/tests/segmented_map_reconstruction.rs:458`, `crates/custodian/tests/segmented_map_reconstruction.rs:603`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the in-memory fenced proof is sufficient for rollout or whether to trigger Tier-1 disk-fault and Tier-2 kill-reconstruct observation — privileged/live-backend reconstruction scenarios were not exercised (`crates/custodian/tests/segmented_map_reconstruction.rs:399`). |

### Advisory — adversary

# Adversarial review — issue #697 (advisory, non-gating)

Re-ran the asserted red→green independently in a scratch clone (source-only copy, private
`CARGO_TARGET_DIR`, removed afterwards), then attacked the fix with six hand-built probe
fixtures and two counterfactual production mutations. Toolchain was fully available
(cargo 1.96.0), so nothing here is provisional for want of tools.

**Evidence re-run (confirmed, with one caveat).** Base `reconstruction.rs` + the new test
compiles (no patch-introduced symbol is named) and goes **5/6 red behaviourally** —
`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` on legs 1–4 and
the injected-fault mismatch on leg 5; leg 6 passes on the base exactly as `brief.md:110-118`
declares. With the patch: 6/6 green, and the whole `wyrd-custodian` package green with
`crates/custodian/tests/reconstruction.rs` unmodified. 60 parallel + 20 serial repeat runs
of the discriminator binary: zero flakes.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:470`:** leg 1
  is the *only* leg whose fixture holds a healthy segmented object that is owed nothing, and it
  asserts nothing about the audit seam — only the outcome, the flat repoint and the queue
  (`:460-474`). Its own doc comment states the rule it is supposed to bind (`:446-447`, "not
  named, not counted"), and `brief.md:188-190` / `:234` pin it ("an unasserted label is a
  finding waiting to happen"). **Demonstrated concrete failing case:** I moved
  `emit_refused(&object_name(&key))` out of the `if reading.refused.insert(..)` guard at
  `crates/custodian/src/reconstruction.rs:537` (out of its `refused.insert` guard at `:536-539`) so the row is emitted for **every** segmented
  object the walk meets, owed or not, while leaving the `refused` set (and therefore the
  `Blocked` answer) exactly as-is. Result: **all six legs stay green, and so does the entire
  `wyrd-custodian` package** — while every store holding one healthy multipart object now emits
  a `refused-segmented` row naming it and ticks `reconstruction_refused_records` on every pass.
  That is precisely answer-rule A's mirror-image defect ("get this wrong and every store holding
  one multipart object is …", `brief.md:187-193`) leaking through the discriminator. Leg 2
  cannot catch it: its fixture holds exactly one segmented object which *is* owed, so
  `(rows, ticks) == (1, 1)` holds either way (`:507-514`). Contrast the sibling label — the
  `unresolvable-chunk-map` counts are pinned to exact values by legs 3 and 5 (`:561-568`,
  `:683-686`) and the same mutation there **is** caught. Fix is one assertion in leg 1:
  `rows(&logged, "refused-segmented") == 0 && rows(&logged, "unresolvable-chunk-map") == 0 &&
  !names(&logged, "inode:1")`.

## Attempted and could not refute

Everything below I tried to break with a built fixture or a mutation and failed:

- **Index aliasing in the shared snapshot** (`crates/custodian/src/reconstruction.rs:511-531`,
  `:866`): `FlatSite::index` enumerates `resolved.chunks`, and `repair_chunk` indexes
  `object.prior.chunk_map.as_flat()`. For a flat record these are provably the same slice —
  `resolve_snapshot` returns `Cow::Borrowed(chunks)` and never restarts
  (`crates/core/src/metadata.rs:2585`), and `FlatObject` is only ever constructed from that same
  iteration's `record` (sole producer, `:521`). No out-of-range or wrong-chunk repoint is
  reachable. Probed with an 8-chunk object where only odd indices are owed, and with a duplicate
  `ChunkId` at indices 0 and 2 — first index wins, matching the base's `position()`.
- **A hole met LAST in key order.** Every shipped leg seeds the damaged record first. I built the
  opposite (`inode:1` under-replicated, `inode:9` undecodable): the pass still answers `Blocked`,
  still lands the healthy repoint, and still drains nothing — the `incomplete` flag is read after
  the whole reading (`:323`, `:331`), so the property is order-independent.
- **Per-object refusal accounting across *two* segmented objects** (no shipped leg covers this):
  two roots under distinct group nonces, one owed chunk each → exactly 2 rows / 2 ticks, both
  obligations kept. Correct.
- **A refusal beside a complete reading:** a refused `seg:` chunk + a genuinely unreferenced
  obligation → the unreferenced one *is* drained, the refused one is kept, answer `Blocked`.
  Sound: a refused object was read successfully, so "no committed map references this chunk"
  is still a conclusion over a complete namespace.
- **Non-committed owner** (`:471`): an obligation whose owner record is `Pending` is drained.
  Byte-for-byte the base's behaviour (`find_chunk` skipped non-`Committed` and returned `None`),
  so not this patch's defect.
- **Unparseable `inode:` key on a flat record** (`:496-503`): `continue` → the obligation drains.
  Also base parity, and frozen to **#698** by `brief.md:266-277`.
- **`Blocked` swallowing `Changed`** (`:331-345`): matches `gc.rs:234-245` and `scrub.rs:210-214`
  exactly, and `least_certified` (`reconciliation.rs:56-61`) already defines the fold. No caller
  loops on `Changed`.
- **Citations the patch introduces** — `gc.rs:234-241`, `rebalance.rs:115-117`,
  `metadata.rs:2585`, `gc.rs:155-166`, `gc.rs:402-416` — all check out on the target, and
  `read_committed`'s containment is byte-for-byte `gc.rs`'s downcast rule.
- **Budget:** 2 files, 143 added semantic production lines (cap 160). The test file is 712 raw
  against the brief's 460 cap, but a human already waived that at 678 with "do not spend the
  round shrinking the file" (`brief.md:442`), so I did not score the further drift.
- **Not re-raised, per the target rubric's *Deferrals are settled* (`AGENTS.md:200-203`):** the
  `Ok(None)`-for-a-key-this-scan-saw-`Committed` silent drain (`:476`) carries an in-code
  `deferred: #702` marker at `:439` and an explicit human "do NOT fix in-slice" at
  `brief.md:454`. The unbounded `scan`/`resolve` awaits (`:458`, `:474`) and the seeded-Tier-0
  DST demand are likewise already recorded-rejected.

## On the two red advisory/gating rows

- **`C5-mutants` (fail, 2 missed) is not a real signal, and I can corroborate that
  independently.** Both survivors are provably **equivalent**: `size: object.prior.size`
  (`crates/custodian/src/reconstruction.rs:868`) and `state: InodeState::Committed` (`:870`) are
  re-supplied unchanged by the `..object.prior.clone()` functional update at `:875` — and
  `object.prior.state` is *always* `Committed` because `read_committed` filters at `:471` and is
  the sole producer of a `FlatObject` (`:521`). No test can kill either. Separately, the **1
  timeout** row (`:471` `!=`→`==`) is infrastructure noise, not an uncaught mutant: I applied it
  by hand and 14 tests in the existing suite plus 5 of the 6 new legs fail in 0.05 s. Three of
  the seven "unviable" entries (`emit_unresolvable`, `emit_refused`, `Reading::contain` → `()`)
  are unviable only because `-D warnings` rejects the resulting unused parameters, not because
  the behaviour is untested.
- **`C4-verify`'s `path_line` reads "(6 test(s) ran red)".** Six tests *ran* in the red leg; five
  went red. Leg 6 is green on the base by design (`brief.md:110-118`). Read as "six legs
  demonstrated red", that row overstates the evidence by one leg — worth not repeating in the
  SUMMARY.
- `T4-batch-review` is the only gating red (2 blocking). Its findings are not in my inputs, so I
  can neither confirm nor rebut them.

Net: one implementation-level oracle gap, demonstrated with a counterfactual the whole suite
misses. I could not refute the production logic, the containment rule, the one-reading property,
the write path, or the red→green itself.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Human must decide whether contribution and prior-art review are complete — merged affected-path history was checked, but absent driver tools `scripts/review-branch` / `scripts/pdca` and the unavailable closed/rejected corpus prevent auditing the reported two batch-review blockers.
- [x] Validation — fitness-to-purpose — Human must decide whether the in-memory fenced proof is sufficient for rollout or whether to trigger Tier-1 disk-fault and Tier-2 kill-reconstruct observation — privileged/live-backend reconstruction scenarios were not exercised (`crates/custodian/tests/segmented_map_reconstruction.rs:399`).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- [x] size backstop — this slice is behaving oversized: 6 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Re-litigated the same settled finding across ~3+ iterations: recorded rejections are matched by `file:line`, so a settled decision falls off its anchor whenever the patch shifts lines and the T4 gate re-blocks on substance already decided (issue_697 rounds 6-11, DST class).
