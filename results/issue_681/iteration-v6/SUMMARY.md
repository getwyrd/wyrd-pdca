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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 79 mutants tested in 67s: 39 caught, 40 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review: make reconstruction, backfill, and rebalance resolve segmented chunk maps, contain per-object read faults, and refuse unsupported segmented writes without stopping healthy maintenance work.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete: it distinguishes per-object map faults from store faults, requires preserved flat work and non-certifying refusals, and fixes measurable scan and shape limits. |
| C2 Reproduction (red pre-fix) | PASS | Keeping the added discriminator while restoring the three production files compiled and failed all 6 tests behaviorally, including pass aborts and the Q-scan assertion (`crates/custodian/tests/segmented_map_passes.rs:502`). |
| C3 Change | PASS | The four-file change stays on the specified surface and routes each pass through the shared resolver while preserving raw-key CAS and explicit segmented-write refusal (`crates/custodian/src/backfill.rs:113`, `crates/custodian/src/rebalance.rs:238`, `crates/custodian/src/reconstruction.rs:790`). |
| C4 Verification (red→green) | PASS | Independent scratch reruns produced 6/6 base-red then 6/6 patch-green; typos, docs lint/render, fmt, clippy, build, workspace tests, machete, deny, and 79-mutant analysis also passed, with cargo-deny rerun under a writable isolated Cargo home (`crates/custodian/tests/segmented_map_passes.rs:502`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Strengthen the discriminator to bind the operator-facing refusal contract — changing all three `segmented-chunk-map` reason constants to an invalid value still leaves all 6 tests green (`crates/custodian/src/backfill.rs:258`, `crates/custodian/src/rebalance.rs:204`, `crates/custodian/src/reconstruction.rs:360`). |
| T1 Structure | PASS | The change is exactly the three named production files plus the required new discriminator, with no manifest, dependency, documentation, or unrelated surface change. |
| T2 Shape | FAIL | Rebuild within the pinned semantic allocations — measured additions are 103/100 in rebalance and 544/470 in the test, making 915/880 overall despite the test meeting its 775/780 raw-line cap (`crates/custodian/tests/segmented_map_passes.rs:775`). |
| T3 Runtime | PASS | The real shared resolver, in-memory metadata/chunk-store seams, all existing workspace tests, and the six-pass discriminator run green; the brief declares no live service, Docker, or topology dependency for this slice (`crates/custodian/tests/segmented_map_passes.rs:408`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the recorded three-blocker batch review and contribution artifacts — `scripts/review-branch`, `scripts/pdca`, and their gate logs are withheld from the target, so those gate rows cannot be independently reproduced; the affected-path audit itself found no open PR and confirmed closed-unmerged #647. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild leg 3 to exercise the missing-segment and undecodable records together — it currently tests each damaged shape “ALONE,” so the required combined containment and dual attribution are not demonstrated (`crates/custodian/tests/segmented_map_passes.rs:592`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether publishing `remaining = 0` with a separate `unaccounted = 1` field is operationally safe — consumers that ignore the caveat can still interpret an incomplete reading as completion (`crates/custodian/src/backfill.rs:204`, `crates/custodian/src/backfill.rs:270`). |

### Advisory — adversary

# Adversarial review — issue #681 (`passes-read-through-resolver-contained`)

Method: rebuilt the workspace from `$PDCA_TARGET` into scratch and re-ran the asserted proof
myself — green leg (patch applied) **6/6 pass**; red leg (the three production files reverted to
`HEAD`, the new test kept) **6/6 fail, compiling**, on behavioural assertions and `expect` panics,
not on a compile error. `C4-verify`'s claim survives. Then ran the whole `wyrd-custodian` suite
(all 95 pre-existing tests green, none edited) and `cargo test --workspace --exclude wyrd-dst`
(green; the single `xtask::scan_gitlinks_is_green_over_the_real_index` failure is my scratch copy
having no `.git`, not the patch). Then wrote five probe tests against the fixed tree to try to
break it. Findings below; two probes landed.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:511`: a refusal that leaves fragments
  on a decommissioning server ticks the refusal counter by ZERO.** `emit_refused` does
  `monotonic_counter.rebalance_evacuation_refused = fragments as u64`. `held_for_drain`
  (`rebalance.rs:184-195`) returns `(fragments, unreadable)` and `plan_evacuations`
  (`rebalance.rs:263-265`) refuses whenever `fragments + unreadable > 0` — so the case
  `fragments == 0, unreadable >= 1` (a segmented object whose chunks carry a malformed
  `placement`, the very case the `unreadable` counter was added for) emits a counter increment of
  **0**, i.e. records nothing at all on the metric seam for a refusal that is leaving fragments
  where nobody can see them. Concrete case, run against the patched tree: seed
  `Shape::Segmented(NONCE, 1, vec![seeded(S_DRAIN_A, &[0], &[])])` under `inode:20`, drain server
  3, run rebalance — the pass answers `Blocked` and the captured metric event is literally
  `{"monotonic_counter.rebalance_evacuation_refused":0}` beside an audit line reading
  `"fragments":0,"unreadable":1`. Every other refusal/decline counter this diff adds counts
  **objects** (`= 1_u64` at `backfill.rs:278`, `backfill.rs:292`, `rebalance.rs:496`,
  `reconstruction.rs:971`, `reconstruction.rs:1000`); this one alone counts fragments and so can
  count nothing. The discriminator drives exactly this store
  (`crates/custodian/tests/segmented_map_passes.rs:584-589`) but asserts only `Reconciled::Blocked`
  and never reads the counter, so no test would have gone red — and `C5-mutants`' "0 missed" does
  not cover it either, because mutating the *whole body* of `emit_refused` away is caught by
  leg 2's audit-line assertion at `segmented_map_passes.rs:549-557`.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:271`: the #350-step-2 gauge is now split
  by a varying integer OTel attribute, so the series an operator watches goes stale exactly when
  the incident starts.** `tracing::info!(gauge.backfill_placement_remaining = remaining,
  unaccounted,)` — `unaccounted` carries no metric prefix, and this workspace's bridge
  (`tracing-opentelemetry 0.33.0`, `src/metrics.rs:198-201`: the non-prefixed arm pushes
  `KeyValue::new(field.name(), Value::I64(..))` into `attributes`) therefore records it as an
  **attribute on the gauge**, not as its own instrument. Concrete consequence: on a healthy store
  the population is published as `backfill_placement_remaining{unaccounted="0"}`; the pass after a
  record becomes undecodable it is published as `{unaccounted="1"}` instead, and under cumulative
  temporality the `{unaccounted="0"}` series keeps being exported at its last, pre-incident value.
  An alert or dashboard keyed on the metric name now reads two contradicting series, and the base
  emitted exactly one unlabelled one. The cited precedent, `emit_domain_utilization`
  (`rebalance.rs:454-460`), labels by a *bounded, stable* `domain` string — not by a count. Fix
  shape that keeps the brief's "one sample carries both": give the caveat its own `gauge.`-prefixed
  field on the same event, so the visitor produces two instruments and zero attributes. No test in
  the bundle exercises the bridge — `assert_gauge`
  (`crates/custodian/tests/segmented_map_passes.rs:178-192`) reads the JSON *log* layer, where a
  field and an attribute look identical.

- **NEEDS-HUMAN [human] — the brief's line budget is exceeded and no gate measured it
  (`check-gates.json:66-73`, `"T2 Shape": "none"`).** Counting added lines that are non-blank and
  not comment-only (the same methodology the brief's own v2 table implies — its ratios reproduce):
  `tests/segmented_map_passes.rs` **544 semantic vs the pinned ≤ 470**, `src/rebalance.rs` **103 vs
  ≤ 100**, `src/reconstruction.rs` 180 (≤ 210 ✓), `src/backfill.rs` 88 (≤ 100 ✓) — **total 915 vs
  the pinned ≤ 880**. The brief's *hard* STOP thresholds are not tripped (4 files; test file 775
  raw ≤ 780; 1466 raw ≤ 1520), which is presumably why this passed unnoticed, but iterations 4 and
  5 were both returned for this class (the last one for eight lines). A human should decide whether
  a 16 %-over test allocation is accepted or trimmed; my semantic counter is mine, not the
  project's, so the exact numbers want re-measuring with whatever tool the earlier "788 raw / 780
  cap" finding used.

- **NEEDS-HUMAN [human] — `crates/custodian/src/desired_state.rs:191-197`: the pass now refuses
  forever, but the operator-facing decommission query still answers bare `Pending`.** After this
  patch a draining server holding a fragment of a *segmented* object gets
  `ReconciliationStatus::Pending` — because `gc::referenced_fragments` (`gc.rs:402-435`) resolves
  segmented maps into `placed`, so `genuinely_holds` is true — while `plan_evacuations`
  (`rebalance.rs:260-267`) has just decided that fragment will **never** move until #682 lands.
  `Pending` is documented in that same file (`desired_state.rs:219-224`) as the answer meaning
  "the rebalance loop is moving them", and lines 205-218 say in as many words that a wait with
  nothing to act on is the permanence C-1 forbids "reached through the report instead of through a
  deletion" — which is why the unreadable case got its own attributed
  `PendingUnresolvable`. The base was equally stuck, but loudly (the pass returned `Err`); this
  slice makes the stall quiet and steady, and `desired_state.rs` is explicitly out of the brief's
  scope with no in-code deferral marker covering this case (the `#682` markers at
  `rebalance.rs:203`, `:252`, `backfill.rs:153`, `reconstruction.rs:359` are all about the *write*
  path). Scope/fitness call: accept as #682's, or add an attributed status.

- The brief's falsifiability section is wrong about leg 6: it declares
  `a_fault_that_is_not_one_objects_map_still_ends_the_pass` *"NOT base-red — it passes before and
  after"* (`brief.md:118-119`), and `C4-verify` reports six red tests, not five. Measured on base,
  that leg fails with `reconstruction absorbed: ... key must be a string at line 1 column 2` —
  the base's walk dies on the seeded undecodable record (`segmented_map_passes.rs:761`) long
  before the injected `get` fault is reached, so it is red for the wrong reason. This does not
  weaken the evidence (the leg's post-fix assertion still binds "a store fault is not swallowed"),
  but the brief's prediction and the gate row disagree and the reviewer had no way to see it.

- `crates/custodian/src/restore.rs:616` still reads `deferred: #681 — ... The maintenance walk that
  both would share is that slice's`; this slice deliberately does **not** share the walk
  (`brief.md:298-299`), so the marker points at a closed issue once this lands. The brief forbids
  touching the file and a fifth file trips the STOP, so this is noted, not filed.

## What I tried to refute and could not

- **The red→green itself.** Reproduced both legs; the red is behavioural, not a compile failure,
  and the test drives the real fenced control point (`reconcile_step`) and `backfill::reconcile`,
  not a parallel re-implementation.
- **Decision 4 (CAS framed by the generation actually read).** I looked for a commit path reachable
  through a *restarted* resolve. There is none: `resolve_snapshot`
  (`crates/core/src/metadata.rs:2584-2586`) answers a flat map by borrow with no store read and no
  supersede check, so only a **segmented** snapshot can restart — and all three passes branch on
  `record.chunk_map.is_segmented()` (the snapshot, `backfill.rs:160`, `rebalance.rs:260`,
  `reconstruction.rs:816`) before any write. The brief's recorded-rejection of a Tier-0 DST leg
  holds up on that reasoning.
- **The `as_flat()` "unreachable" fallbacks** (`rebalance.rs:367-372`, `reconstruction.rs:640-645`)
  really are unreachable: `ChunkMap` has exactly two variants (`crates/core/src/metadata.rs:986-993`),
  so `!is_segmented()` implies `as_flat().is_some()` on the same bytes.
- **"Only one obligation per object is repaired per pass"** (second plan loses its CAS on the shared
  `prior_bytes`). Probed it: this is the base's behaviour too — assessment completes for every
  obligation before any repair commits, so both plans were always framed by one snapshot. Not a
  regression.
- **A silent drain through `Ok(None)`.** Probed a segmented root retired mid-resolve while it held
  the only reference to a queued chunk; the obligation was kept and the pass answered `Blocked`.
  `Ok(None)` from the resolver means "no live committed generation"
  (`crates/core/src/metadata.rs:2639-2646`), so skipping it silently is the same answer gc, scrub
  and restore already give.
- **The new `unparsable-inode-key` containment** (`backfill.rs:107`, `rebalance.rs:232`,
  `reconstruction.rs:783`). Probed a healthy repairable object under `inode:-1`: it is never
  repaired and the pass is `Blocked` forever. That looked like an introduced trap until I checked
  the base — where the same store had its obligation silently **drained** (`find_chunk`'s
  `if let Some(inode_id)` fell through to `Ok(None)` → `Assessment::Drain`). The patch's direction
  is fail-closed and matches `metadata::high_water_marks` (`crates/core/src/metadata.rs:2155-2170`).
- **Test-shape false-greens.** `assert_gauge`'s digit-run parse cannot be satisfied by a substring
  of `monotonic_counter.*_unaccounted_records` (the quote-delimited key does not match), it demands
  exactly one sample, and leg 2's `assert_eq!(refusals.len(), 1)` plus `"fragments":3` over a
  fixture with 3 draining fragments across 2 chunks really does discriminate per-object from
  per-chunk logging and fragment-counting from chunk-counting.

Advisory only — nothing here gates.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Strengthen the discriminator to bind the operator-facing refusal contract — changing all three `segmented-chunk-map` reason constants to an invalid value still leaves all 6 tests green (`crates/custodian/src/backfill.rs:258`, `crates/custodian/src/rebalance.rs:204`, `crates/custodian/src/reconstruction.rs:360`).
- [ ] T4 Contribution — Confirm the recorded three-blocker batch review and contribution artifacts — `scripts/review-branch`, `scripts/pdca`, and their gate logs are withheld from the target, so those gate rows cannot be independently reproduced; the affected-path audit itself found no open PR and confirmed closed-unmerged #647.
- [ ] T5 Judgment — Rebuild leg 3 to exercise the missing-segment and undecodable records together — it currently tests each damaged shape “ALONE,” so the required combined containment and dual attribution are not demonstrated (`crates/custodian/tests/segmented_map_passes.rs:592`).
- [ ] Validation — fitness-to-purpose — Decide whether publishing `remaining = 0` with a separate `unaccounted = 1` field is operationally safe — consumers that ignore the caveat can still interpret an incomplete reading as completion (`crates/custodian/src/backfill.rs:204`, `crates/custodian/src/backfill.rs:270`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- [ ] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Iteration delta (if iterating): Reason: T4 batched review found 3 new blocking bugs this round (backfill.rs:190 stale gauge on CAS conflict; rebalance.rs:130 evacuation proceeding despite an incomplete/refused read; reconstruction.rs:383 repair allowed despite unaccounted duplicate references) — all implementation-shaped, not scope-shaped. The size backstop tripped again (3 rounds spent post-split, threshold 2) and the advisory review confirms the shape budget is still over (915/880 semantic lines) plus flags a metrics/gauge issue and an operator-visible pending-forever edge case for scope judgment. Human overrode the iterate-plan recommendation: the prior split (#654 -> this slice) was recent, so the process should be given more room to converge via iterate-do before re-splitting. Fix the 3 T4-blocking bugs, address the advisory findings (or record-reject with reasons), and get the semantic-line count back under budget.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
