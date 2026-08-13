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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 74 mutants tested in 69s: 4 missed, 31 caught, 39 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #681: make reconstruction, backfill, and rebalance resolve segmented chunk maps, contain per-object read faults, and refuse unsafe work without abandoning healthy-object maintenance.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief pins the three affected passes, six behavioral legs, containment boundary, duplicate-ID policy, resource bounds, and an exact four-file budget. |
| C2 Reproduction (red pre-fix) | PASS | With only the three production hunks stashed, all six base-visible tests compiled and failed on behavioral assertions, including the segmented-object leg at `crates/custodian/tests/segmented_map_passes.rs:512`. |
| C3 Change | PASS | The scoped code paths now resolve by raw inode key and contain only typed object-map faults while propagating store faults at `crates/custodian/src/backfill.rs:121`, `crates/custodian/src/rebalance.rs:240`, and `crates/custodian/src/reconstruction.rs:823`. |
| C4 Verification (red→green) | PASS | Restoring the production hunks made all six tests green; typos, docs render/link audit, fmt, clippy, build, workspace tests, machete, all three deny audits, conformance, statics, and the DST suite also passed. |
| C5 Causal adequacy | PASS | The flat-only read cause is removed through one resolver-backed namespace reading with typed per-object containment and non-map error propagation, not a capability probe or runtime guard (`crates/custodian/src/reconstruction.rs:790`). |
| T1 Structure | PASS | The change stays in the three pass modules plus one shared in-memory fixture and introduces no dependency, API, persisted-field, or documentation surface (`crates/custodian/tests/segmented_map_passes.rs:293`). |
| T2 Shape | FAIL | Return the test fixture to its allocated size — it adds 525 non-blank/non-comment lines, 55 above the hard 470-semantic-line cap, although its 758 raw lines remain below the separate 780-line cap (`crates/custodian/tests/segmented_map_passes.rs:758`). |
| T3 Runtime | PASS | The discriminator drives the real fenced reconciliation entry over the declared trait seams, and both the full workspace runtime suite and DST suite passed (`crates/custodian/tests/segmented_map_passes.rs:426`). |
| T4 Contribution | NEEDS-HUMAN | Reconcile the reported seven-blocker batched review and confirm the contribution artifacts — `scripts/review-branch` and `scripts/pdca` are absent from the target, so those rows could not be independently rerun; the affected-path audit found closed #647 and no open competing PR. |
| T5 Judgment | PASS | The pinned safety judgments are explicit and exercised: duplicate ambiguity, incomplete-reading retention, raw-key identity, one-scan work, and per-object attribution (`crates/custodian/src/reconstruction.rs:729`); the four surviving mutants only delete fields inherited unchanged by the same struct update. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether in-memory seam plus DST evidence is sufficient to authorize production repair, fill, and drain behavior — no live-backend maintenance cycle was exercised, so operational fitness remains a human sign-off (`crates/custodian/tests/segmented_map_passes.rs:47`). |

### Advisory — adversary

# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; nothing here gates. Toolchain was available: I rebuilt the workspace in scratch
and re-ran the asserted red→green **in both directions** myself.

## The evidence held up

- **Red reproduced.** Copied the tree to scratch, reverted only the three production files to
  `HEAD` (`339da46`) keeping `crates/custodian/tests/segmented_map_passes.rs`: **6/6 fail**, all
  behaviourally (compile is clean — no symbol this patch introduces is named, so the red does not
  degrade to UNVERIFIABLE): `find_chunk met a segmented chunk map`, `key must be a string`,
  `read ONCE, not once per obligation left: 3 right: 1`, `ambiguity is no repair left: Changed`.
  **Green reproduced:** 6/6 pass with the patch restored.
- **It is the production path.** Every leg drives `reconcile_step` (the real fenced control point,
  `crates/custodian/tests/segmented_map_passes.rs:450`) and `backfill::reconcile` (`:458`) over
  trait doubles — no re-implementation, no mock of the code under test. Nothing is tautological:
  each leg asserts *positive* work (placements moved to `[0,1,2]`, bytes landed on server 2,
  obligations discharged) beside the "`Ok` was returned" half.
- One brief inaccuracy for the record: leg 6 is declared "*NOT* base-red … passes before and
  after" (brief line 119), but it fails on base too (the undecodable record raises a decode error
  before the injected store fault). That makes the red stronger, not weaker.

## Findings

- NEEDS-HUMAN [human] — **The brief's decision-4 "unreachable by construction" claim is false, and
  with it the pre-declared recorded-rejection of the Tier-0 DST leg. Demonstrated, not argued.**
  `crates/custodian/src/backfill.rs:121` (same shape at `crates/custodian/src/reconstruction.rs:823`
  and `crates/custodian/src/rebalance.rs:240`): when the scan snapshot is *segmented* and the live
  root has since been replaced by a *flat* one, `resolve_chunk_map` answers `Superseded`
  (`crates/core/src/metadata.rs:2338-2339`), restarts onto the live root, and the pass then finds
  `resolved.record.chunk_map.is_segmented() == false` (`backfill.rs:166`) and **writes**. I built a
  metadata double whose `scan` answers a stale segmented root while `get` answers a live flat root
  carrying an empty placement: `backfill::reconcile` returned `Changed` and committed the record
  from `version 9` to `version 10` — a commit framed by, and CAS'd on, a generation the pass never
  scanned. The *write* is correct (it obeys decision 4 exactly), but the brief's premise — "only a
  segmented resolve can restart … and every segmented write here is refused. So every commit this
  slice performs is framed by the scan snapshot exactly as today" — and the rejection reason
  "every write it performs is on a FLAT object and keeps its existing version-conditional CAS on
  the scan snapshot, byte-for-byte the behaviour on the base" (brief §Verification posture) are
  both untrue on that path. It is not reachable in the deployed build *today* (nothing can publish
  a segmented root until #653), which is why this is a scope/fitness call rather than a bug: the
  brief itself required it to reach sign-off ("If Do finds a commit path that CAN be reached
  through a restarted resolve, that falsifies this reasoning … leave it for sign-off"), and the
  human should confirm whether Do surfaced it and whether the DST rejection still stands.
- NEEDS-HUMAN [impl] — **The refusal vocabulary this patch introduces is asserted by nothing; the
  discriminator would stay green with every reason inverted.** The only audit-vocabulary assertion
  in the whole file is `"action":"refused"` on the *rebalance* seam
  (`crates/custodian/tests/segmented_map_passes.rs:566`); `assert_named` (`:204`) matches on the
  object name alone. Concrete cases that keep all six legs green: (a) swap `REFUSED_SEGMENTED` and
  `REFUSED_INCOMPLETE` at `crates/custodian/src/reconstruction.rs:399` and `:407`, so an operator
  is told a `seg:`-resident chunk is an "incomplete reading" and vice versa; (b) give all three
  `cannot_account_for` call sites one `action` label (`reconstruction.rs:803`, `:817`, `:830`;
  `backfill.rs:102`, `:116`, `:128`; `rebalance.rs:221`, `:235`, `:247`), collapsing "the bytes
  will not decode" / "the map will not resolve" / "the key is not an `inode:` key" into one
  indistinguishable row; (c) delete any of the five new counters
  (`*_unaccounted_records`, `backfill_declined_records`, `reconstruction_repair_refused`,
  `reconstruction_ambiguous_chunk_id`, `rebalance_evacuation_refused`) — nothing reads one back.
  This is inside the brief's own scope, not beyond it: backfill is required to leave the record
  "declined **with a stated reason** on the audit seam **and counted**" (§Scope), and "a stated
  reason, never a silent skip" is what `emit_declined` (`backfill.rs:300`) claims to deliver.
  Cheapest binding: one added assertion per existing leg on the `reason`/`action` string.
- NEEDS-HUMAN [impl] — **Two comments assert a byte-exactness property the code does not
  establish.** `crates/custodian/src/reconstruction.rs:660` ("the CAS requires those EXACT bytes
  back … no re-encoding sits between the read and the precondition") and
  `crates/custodian/src/rebalance.rs:429` ("on the EXACT bytes it answered with, so no re-encoding
  sits between the read and the precondition") both describe `prior_bytes` — which is
  `metadata::encode()` of the *decoded* record (`reconstruction.rs:867`, `rebalance.rs:187`), i.e.
  precisely a re-encoding, the same one the base performed at `require(key, encode(&plan.prior))`.
  Behaviour is unchanged, so this is not a regression; but the CAS still depends on decode→encode
  being byte-identical (guaranteed by `docs/design/architecture/08-crosscutting-concepts.md:85`,
  not by this code), and the rubric's *Serialization identity* class is exactly the place where a
  comment claiming a guarantee the code borrows from elsewhere hides the next defect. Either
  restate the comment or carry the scan value's own bytes on the flat path.

## Attempted refutations that failed

- **`C5-mutants` (red, advisory) is not a real signal here, despite the brief pre-declaring that
  "a survivor here is a real signal … not noise."** All four survivors are *equivalent* mutants:
  `mutants.out/missed.txt` names only `delete field size` / `delete field state` from the
  `InodeRecord` expressions at `crates/custodian/src/rebalance.rs:419`/`:421` and
  `crates/custodian/src/reconstruction.rs:672`/`:674` — and each struct expression ends in
  `..prior.clone()` (`rebalance.rs:426`, `reconstruction.rs:679`), whose `size` is `prior.size` and
  whose `state` is `Committed` on every path that reaches there (both walks skip
  `state != Committed`, and `resolve_current_chunk_map` answers `Ok(None)` for a non-committed
  root). Deleting either field produces a byte-identical record, so no test can kill them. The same
  shape existed on the base; they surface only because the diff touched those lines.
- **Draining an obligation on an incomplete reading** — I tried to reach `Assessment::Drain` with a
  hole in the reading (`reconstruction.rs:403`): the guard is `index.unaccounted == 0`, and every
  containment site increments it through the single `cannot_account_for` entry point, including the
  unparsable-key branch the base silently skipped. Could not.
- **Repairing on an incomplete reading** *is* still permitted (`reconstruction.rs:394-408` refuses
  the drain but not the repoint), so a record the pass could not read may still reference a chunk
  whose fragments a repair orphan-marks. I could not turn it into loss: GC withholds *every*
  reclamation while its own reference set is incomplete (`crates/custodian/src/gc.rs:186-190`), and
  once the record is readable again its fragments are protected as referenced.
- **Duplicate-`ChunkId` rule** — tried the two-in-one-record case bypassing the `Entry::Occupied`
  arm (`reconstruction.rs:737`): `hits` (`reconstruction.rs:839-845`) collects both positions and `note` (`:732`) is called twice, so
  it lands on the ambiguity arm exactly as the two-record case. Could not.
- **Over-containment** — tried to force a spurious `Blocked`: a segmented object with no fillable
  chunk, no draining fragment and no queued chunk raises no refusal in any of the three passes
  (`backfill.rs:153`, `rebalance.rs:289`, `reconstruction.rs:846`). Could not.
- **The Q×N property** — tried to find a second namespace walk: `emit_remaining` no longer scans
  (`backfill.rs:221`) and `locate_queued_chunks` is called once (`reconstruction.rs:165-169`).
  Could not.
- **Memory bound** — `prior_bytes` is one `Arc<[u8]>` per object *holding an obligation*, shared
  across that object's obligations (`reconstruction.rs:867`, `rebalance.rs:313`); strictly less
  retention than the base's per-plan `InodeRecord` clone. Could not.

## Observations (no action implied)

- Budget is at the ceiling and the *test* allocation is over: added semantic (non-blank,
  non-comment) lines are reconstruction 180 / backfill 80 / rebalance 93 / test **525** against
  allocations of 210 / 100 / 100 / **470**. The aggregate (878) fits under 880 only because the
  three production files came in under theirs; the binding STOP conditions (4 files, 758 ≤ 780 raw
  test lines) are met.
- Pinned decision 5 ("a refusal is reported once per object, not once per chunk") is enforced for
  rebalance (`rebalance.rs:328-330`) but reconstruction emits one refusal line per *obligation*
  (`reconstruction.rs:259-262`), so one `seg:` object holding Q queued chunks yields Q lines. The
  decision's second sentence scopes it to rebalance and an obligation is reconstruction's unit of
  work, so I am not raising it as a finding — but it is the reading a later reviewer will re-open.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Reconcile the reported seven-blocker batched review and confirm the contribution artifacts — `scripts/review-branch` and `scripts/pdca` are absent from the target, so those rows could not be independently rerun; the affected-path audit found closed #647 and no open competing PR.
- [ ] Validation — fitness-to-purpose — Decide whether in-memory seam plus DST evidence is sufficient to authorize production repair, fill, and drain behavior — no live-backend maintenance cycle was exercised, so operational fitness remains a human sign-off (`crates/custodian/tests/segmented_map_passes.rs:47`).
- [ ] **The brief's decision-4 "unreachable by construction" claim is false, and
- [ ] **The refusal vocabulary this patch introduces is asserted by nothing; the
- [ ] **Two comments assert a byte-exactness property the code does not
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Reconcile the reported seven-blocker batched review and confirm the contribution artifacts — `scripts/review-branch` and `scripts/pdca` are absent from the target, so those rows could not be independently rerun; the affected-path audit found closed #647 and no open competing PR.; **The refusal vocabulary this patch introduces is asserted by nothing; the; **Two comments assert a byte-exactness property the code does not; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
