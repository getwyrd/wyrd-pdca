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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 67 mutants tested in 62s: 31 caught, 36 unviable

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

Reviewing issue #681: make reconstruction, backfill, and rebalance resolve segmented chunk maps, contain per-object faults, and refuse unsafe work without stopping healthy work.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract is testable across all three namespace walks and distinguishes per-object anomalies from store faults at `crates/custodian/src/backfill.rs:95`, `crates/custodian/src/rebalance.rs:204`, and `crates/custodian/src/reconstruction.rs:776`. |
| C2 Reproduction (red pre-fix) | PASS | All six base-visible discriminator tests independently compiled and failed behaviorally before the production fix (0/6), beginning at `crates/custodian/tests/segmented_map_passes.rs:531`. |
| C3 Change | PASS | No scope decision remains: the patch stays within the three inode-walking passes plus their required shared discriminator, while segmented writes remain refused at `crates/custodian/src/reconstruction.rs:834` and `crates/custodian/src/rebalance.rs:286`. |
| C4 Verification (red→green) | PASS | Independent red→green was 0/6→6/6; every CI component passed, with the real cargo-deny audits rerun under a writable scratch Cargo home after the host's read-only lock caveat (`crates/custodian/tests/segmented_map_passes.rs:531`). |
| C5 Causal adequacy | PASS | The eager inline flat-map reads are removed rather than capability-probed, and reconstruction now builds one namespace index per non-empty pass at `crates/custodian/src/reconstruction.rs:158`; mutation rerun caught 31/67 with the other 36 unviable. |
| T1 Structure | PASS | The four-file boundary preserves existing crate seams and centralizes all six legs in one fixture with the required crate-root safety policy at `crates/custodian/tests/segmented_map_passes.rs:16`. |
| T2 Shape | FAIL | Rebuild must remove at least eight physical lines: the brief's explicit STOP cap is 780 lines, while the discriminator ends at `crates/custodian/tests/segmented_map_passes.rs:788`. |
| T3 Runtime | PASS | The real fenced control point and public backfill entry exercise continuing flat repair, fill, and evacuation at `crates/custodian/tests/segmented_map_passes.rs:448`; workspace and 50-seed DST runtime suites passed. |
| T4 Contribution | NEEDS-HUMAN | Confirm the unpublished contribution artifacts carry the user-impact opener and #681 tracker — `scripts/pdca`, `scripts/review-branch`, and those artifacts are absent, so their recorded rows cannot be rerun; the affected-path audit found only closed unmerged #647 and no open PR (`crates/custodian/tests/segmented_map_passes.rs:1`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild the discriminator to pin the stable segmented/ambiguous/incomplete refusal reasons — production defines that operator vocabulary, but the test matches only `action:"refused"` at `crates/custodian/tests/segmented_map_passes.rs:584`, so reason regressions still pass. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether operationally blocking segmented repair, fill, and evacuation until #682 is acceptable — obligations and bytes are preserved, but segmented work deliberately stays in place at `crates/custodian/src/rebalance.rs:286` and `crates/custodian/src/reconstruction.rs:834`. |

### Advisory — adversary

# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; I gate nothing. Evidence was re-run independently at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `339da46`) in a throwaway worktree under
`$PDCA_SCRATCH` (removed).

**The red→green survives the attack.** I reproduced it rather than taking it on trust: the new
`crates/custodian/tests/segmented_map_passes.rs` compiles and fails **6/6** on `origin/main` with
the three production files reverted (assertion/behavioural reds — `SegmentedMapUnsupported`,
`Satisfied` where `Blocked` is required, `3` namespace scans where `1` is required), and passes
**6/6** with the patch. The legs drive the real `reconcile_step` fence and the real
`backfill::reconcile` entry, over trait doubles — no parallel re-implementation. `check-gates.json`'s
`C4-verify` row is honest. Note in passing that leg 6 *is* base-red (it seeds an undecodable record
first), which contradicts the brief's `:118-119` "NOT base-red" prediction — that makes the
discriminator stronger, not weaker, so it is not a finding.

What I could break is the **causal adequacy of the fixture** and one **operator-visible claim**.

- **NEEDS-HUMAN [impl] — pinned decision 3 is unbound for two of the three passes; a re-derived
  canonical CAS key survives the whole suite.** `crates/custodian/src/reconstruction.rs:825` keeps
  `Arc::from(key.as_slice())` (and `:663` CASes on it), `crates/custodian/src/rebalance.rs:304`
  does the same (`:416`). I replaced both with
  `Arc::from(metadata::inode_key(parse_inode_key(&key).expect("parsed")).as_slice())` — i.e. exactly
  the defect decision 3 exists to prevent, "reads one record and commits over the other" — and
  **all 6 discriminator tests plus the entire `cargo test -p wyrd-custodian` suite stayed green.**
  The reason is visible at `crates/custodian/tests/segmented_map_passes.rs:663-675`: the only
  non-canonical keys in the file (`inode:007` / `inode:7`) are seeded into a store with an **empty
  repair queue and no draining server**, so reconstruction returns `Satisfied` unread
  (`reconstruction.rs:163-170`) and rebalance early-returns at `rebalance.rs:125-128` — neither pass
  ever reads those keys. Concrete missing case: seed a repairable chunk (and a fragment on the
  draining server) under `inode:007` beside a *different* record at `inode:7`, then assert the
  repoint/evacuation landed on `inode:007` and left `inode:7` byte-identical. `cargo-mutants` cannot
  generate this mutant (it does not rewrite a field/expression of this shape), so the `C5-mutants`
  "0 missed" row does not cover it.
- **NEEDS-HUMAN [impl] — the same block's comment is an unwarranted claim.**
  `crates/custodian/tests/segmented_map_passes.rs:666-670` says "this store is the control against
  OVER-containment: every pass does its work AND certifies it", and asserts
  `assert_ne!(got, Reconciled::Blocked)` for all three. For reconstruction and rebalance that
  assertion is **vacuously true** — they short-circuit before touching `inode:`. Only backfill is
  actually a control. Either drive the other two over that store (queue an obligation, mark a server
  draining) or drop the claim.
- **NEEDS-HUMAN [impl] — reconstruction's "an incomplete reading may not certify" is asserted by
  nothing.** `crates/custodian/src/reconstruction.rs:170` seeds `refused` from `index.unaccounted`.
  I replaced it with `let mut refused = 0usize;` and all 6 legs plus the full custodian suite stayed
  green. Leg 3 only *appears* to bind it: it always enqueues `C_UNSEEN`, a chunk no record
  references, so `assess` independently produces `Refused(REFUSED_INCOMPLETE)` via
  `reconstruction.rs:394` and reaches `Blocked` by a different route. The uncovered case is the
  realistic one — an unreadable committed object beside a queue whose obligations **all** resolve to
  flat sites: the shipped code correctly answers `Blocked`, but nothing would notice if it stopped.
  Add a sub-assertion to leg 3 that enqueues only `C_REPAIR` (drop `C_UNSEEN`) over the damaged
  store and still requires `Blocked`.
- **NEEDS-HUMAN [human] — backfill publishes `backfill_placement_remaining = 0` over a reading it
  admits is incomplete, and the patch treats "unreadable" inconsistently with "declined".**
  `crates/custodian/src/backfill.rs:166-169` adds a declined **segmented** record's empty placements
  to `remaining` (leg 2 pins that: gauge `1`), but an object taken out at
  `backfill.rs:98-104`/`:118-126` (undecodable record, unparsable key, unresolvable map) contributes
  **zero** — its unknown empty placements are silently counted as none. Probed on the target: a store
  of one undecodable record plus one fillable flat record makes the pass emit
  `outcome=Blocked gauge=0`. On the base the pass returned `Err` and emitted **no** sample at all, so
  this is a new false clean bill on the very gauge `backfill.rs:253-262` calls "ADR-0040 decision 6's
  first precondition". The counter-argument is real (`backfill_unaccounted_records` fires and the
  outcome is `Blocked`), and #350 step 2 requires a sample *every* pass — which is why this needs a
  human: the brief's own invariant "a pass never claims more than it read" (`brief.md:176-178`) and
  #350's "always emit" pull opposite ways here, and neither the code nor the fixture records which
  one won.
- **NEEDS-HUMAN [impl] — the gauge assertion is a prefix match, so it cannot pin the value it
  claims to.** `crates/custodian/tests/segmented_map_passes.rs:190-193` asserts
  `logged.contains(r#""gauge.backfill_placement_remaining":{value}"#)` with no trailing delimiter.
  I changed `emit_remaining(remaining)` to `emit_remaining(remaining * 10)` — a gauge over-reporting
  the drain backlog tenfold — and all 6 legs passed (`:575` wants `1`, sees `10`, matches on the
  prefix; `:672` wants `0`, sees `0`). Given that the round-3 carry-forward
  (`brief.md:420`) rebuilt this bundle *specifically* because the remaining gauge was undetectable,
  the helper should match `":{value},"` / `":{value}}}"` or parse the JSON field.

## Attacks that failed (stated, so the silence is informative)

- **Decision 4 ("write only the generation you read").** I traced the restart path: only a
  *segmented* snapshot can reach `resolve_current_chunk_map` (`crates/core/src/metadata.rs:2584-2586`
  returns `Answer(Cow::Borrowed)` for flat with no store read), and all three passes branch on the
  **snapshot's** `record.chunk_map.is_segmented()` — `reconstruction.rs:834`, `backfill.rs:166`,
  `rebalance.rs:248` — before any write. The leg-2 supersede sub-case
  (`segmented_map_passes.rs:590-612`) exercises it for real. I could not construct a flat-snapshot
  restart.
- **CAS on stored bytes.** `reconstruction.rs:663`, `rebalance.rs:416`, `backfill.rs:194-196` all
  `require` the row's own bytes; the fixture's `stored()` helper (`segmented_map_passes.rs:304-313`)
  seeds every root non-canonically, so a re-encoding regression would lose every CAS and fail
  `assert_flat_work_done`. This is genuinely bound.
- **Over-containment.** `metadata::decode(&plan.prior_bytes)?` at `reconstruction.rs:649` /
  `rebalance.rs:361` and the `Err(err) => return Err(err)` non-`ChunkMapError` arms match
  `gc.rs:402-416` exactly; leg 6 proves a store fault still ends all three passes with the injected
  error text.
- **Duplicate `ChunkId`.** I tried same-record, cross-record, and segmented-then-flat orderings
  against `CommittedIndex::note` (`reconstruction.rs:713-722`); every ordering ends in
  `Site::Refused(REFUSED_AMBIGUOUS, ..)` with both keys named.
- **Arithmetic.** `remaining -= to_fill.len()` (`backfill.rs:202`) is always preceded by the matching
  `+=` in the same iteration — no underflow. `fragment_count()` is `>= 1` for every `EcScheme`
  (`crates/core/src/metadata.rs:148-153`), so the "fill an empty placement to an empty vector"
  gauge-drift case I looked for is unreachable.
- **Retained state.** `FlatSite`/`EvacPlan` hold `Arc<[u8]>` of at most one record per object that
  yields work (`<= Q` objects), not decoded chunk lists — within the brief's "proportional to the
  obligations it holds".
- **Scope.** The DST deferral is recorded-rejected at Plan (`brief.md:349-360`) and the AGENTS.md
  reviewer protocol makes deferrals settled; I did not re-raise it. Rebalance's lack of a duplicate-id
  rule and `EvacOutcome::Aborted`'s certification are pre-existing / #682's by the brief, so they are
  not filed here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Confirm the unpublished contribution artifacts carry the user-impact opener and #681 tracker — `scripts/pdca`, `scripts/review-branch`, and those artifacts are absent, so their recorded rows cannot be rerun; the affected-path audit found only closed unmerged #647 and no open PR (`crates/custodian/tests/segmented_map_passes.rs:1`).
- [ ] T5 Judgment — Rebuild the discriminator to pin the stable segmented/ambiguous/incomplete refusal reasons — production defines that operator vocabulary, but the test matches only `action:"refused"` at `crates/custodian/tests/segmented_map_passes.rs:584`, so reason regressions still pass.
- [ ] Validation — fitness-to-purpose — Decide whether operationally blocking segmented repair, fill, and evacuation until #682 is acceptable — obligations and bytes are preserved, but segmented work deliberately stays in place at `crates/custodian/src/rebalance.rs:286` and `crates/custodian/src/reconstruction.rs:834`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- [ ] external dependency.**
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Reason: real, fixable bugs found by review rather than a scope/sizing problem, despite this being an already-split slice (4b of 7) hitting the size backstop a second time — the backstop's iterate-plan suggestion was considered and overridden. Address in the rebuild: 1. rebalance.rs:269 (T4 blocking) — a malformed placement is skipped without incrementing `refused`, so a segmented object with an unreadable placement can leave fragments stuck on a draining server while the pass reports Satisfied. 2. T2 Shape — trim the new test file: it is 8 lines over the brief's 780-line cap. 3. Adversary findings — reconstruction.rs and rebalance.rs re-derive the canonical CAS key without a test binding pinned decision 3 for those two passes (only backfill is actually bound); add the missing sub-assertion(s) so mutating the canonical re-derive back to the raw key fails a test. 4. Adversary finding — reconstruction's "an incomplete reading may not certify" claim (reconstruction.rs:170, `refused` seeded from `index.unaccounted`) is not bound by any leg; add a sub-assertion (enqueue only C_REPAIR, drop C_UNSEEN, over the damaged store) that still requires Blocked. 5. Adversary finding — the gauge assertion in segmented_map_passes.rs (~190-193, ~575, ~672) is a prefix match that would not catch a 10x-inflated value; tighten to match the delimiter or parse the JSON field. 6. T4 TEST-GAP claim (DST coverage) and the backfill-gauge-vs-unreadable-object human question (Validation item) should be explicitly addressed or recorded-rejected with reasoning in the rebuild, not left ambiguous. 7. Confirm the C4-ci flake was transient (note what interfered) as part of the rebuild's verification, not a lingering unknown.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
