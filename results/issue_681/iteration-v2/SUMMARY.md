# Result — issue 681 / passes-read-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The three maintenance passes that walk the committed namespace **themselves** still
  read the chunk map out of the inode record inline, so a **single** segmented object aborts the
  whole pass — and an object whose map cannot be read for any other reason ends the walk with an
  `Err` rather than being contained. Seven sites, each re-verified on `origin/main` at `339da46`
  with its enclosing function:

  | Site | Function | What it ends |
  |---|---|---|
  | `crates/custodian/src/reconstruction.rs:332` | `assess` | one obligation's assessment, and the pass (`?`) |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | the binding repair commit |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` | the whole `inode:` scan, for every obligation |
  | `crates/custodian/src/backfill.rs:99` | `reconcile` | the fill scan |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` | the remaining-placement gauge |
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` | the evacuation scan |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` | the binding evacuation commit |

  Each is `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`, and
  each `?`s out of a function whose error ends the pass. Consequences, all live on the base:
  1. **One segmented object disables repair, backfill and drain for the entire store.** Not for
     that object — for every object, because the error propagates out of the `scan(b"inode:")`
     loop. The three passes are the ones that *restore redundancy*; a store that has published a
     single multipart object therefore stops self-healing.
  2. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as *"referenced by no committed chunk
     map"* → `Assessment::Drain` (`reconstruction.rs:322-325`, returning at `:325`), and the obligation is deleted in
     the drain batch (`:271-277`). Today `find_chunk` errors before it can return `None`, so this
     is latent — but any implementation that makes the walk *skip* a segmented object instead of
     containing it silently discards the repair obligation for live, under-replicated data. That
     is the loss this slice must not introduce while removing the abort.
  3. **The deployed repair loop is Q namespace scans × N point reads.** `reconcile` calls `assess`
     once per queued obligation (`reconstruction.rs:185`), and `assess` calls `find_chunk`
     (`:322`), which scans the entire `inode:` namespace and decodes every record (`:620-644`).
     With the resolver wired in, each of those N objects also costs a bounded `seg:` range read —
     so the pass would go from Q×N decodes to Q×N *resolves*. This is the finding left open at
     #647's close and it is in scope here.
  4. **Containment is not per object.** #650 and #651 contain a damaged record in GC/scrub and in
     restore/drain (`gc.rs:360-450`, `restore.rs:621-688`); these three passes have no such rule.
     A record that will not `decode` ends the walk at `reconstruction.rs:625`, `backfill.rs:80`,
     `backfill.rs:174`, `rebalance.rs:148` — before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_passes.rs` passes,
  driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step, Custodian,
  FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::backfill::{reconcile, BackfillContext}`,
  `wyrd_custodian::desired_state::set_lifecycle`, `wyrd_core::repair::{enqueue_repair, repair_key,
  queued_repairs}`, `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}` — over in-memory
  `MetadataStore` / `ChunkStore` doubles. Five legs, each binding:
  1. **A segmented object no longer ends any of the three passes.** Over a store holding one
     healthy segmented object (raw `seg:` records + a segmented root, **never** a committer) beside
     one healthy flat object: `reconcile_step` with a `ReconstructionContext` **and** a
     `RebalanceContext` returns `Ok`, and `backfill::reconcile` returns `Ok` — today all three
     return `Err`. Assert **positively**, not merely "no error": the flat object's genuine work
     still happens in the same pass — a queued repair for an under-replicated **flat** chunk is
     assessed and repaired (its placement moved, its obligation drained), and a **fillable flat**
     record with an empty `placement` is still filled.
  2. **A repair obligation for a chunk in a `seg:` record is NOT drained.** Enqueue a repair for a
     chunk that lives in a segmented record, run the pass, and assert `queued_repairs` still
     contains that chunk id and the `seg:` record's bytes are **unchanged**. Then assert the pass
     did **not** certify: `!= Reconciled::Satisfied`. (Spell the negative as
     `!= Satisfied && != Changed` where the pass converged nothing, so the leg cannot be scored by
     a variant this patch introduces — `Reconciled::Blocked` **does** exist on the base
     (`reconciliation.rs:44`, #650), so naming it positively is also legal here; prefer naming it.)
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed a
     committed root whose map genuinely fails to resolve (a `SegmentRef` the root names whose `seg:`
     record was never written — assert in the fixture that `resolve_chunk_map` really errors, as
     `segmented_map_restore.rs:415-431` does) **first** in key order, beside a healthy flat object
     carrying real work. Assert the conjunction: each pass returns `Ok`, answers `Blocked` (never
     `Satisfied`), the healthy object's queued repair is still assessed **and** its fill still
     happens, and the unreadable object is **named** on the audit seam by its `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470`). A record whose own bytes will not `decode`
     is contained by the same rule.
  4. **A store fault under the resolve still ends the pass.** A metadata double that fails a `get`
     with a non-`ChunkMapError` error makes the pass return `Err` — a walk that cannot reach the
     store has no answer for any object, not one unreadable object. This is the other half of (3)
     and it is what the downcast rule at `gc.rs:402-416` exists for.
  5. **Reconstruction resolves each object's root once per pass — O(N), not O(Q×N).** Drive the
     pass with **Q ≥ 3** queued obligations over **N ≥ 3** committed objects on a *counted*
     instrumented `MetadataStore` double, and assert the number of `inode:` scans is **1** and the
     number of `seg:` range reads is **≤ N** (one per segmented object), independent of Q. Assert
     the count, not a timing. The same store must still produce the correct repairs, so the leg
     cannot be satisfied by a pass that stopped doing the work. **Scope the count to the
     reconstruction pass alone** — build the `ReconstructionContext` leg with no `GcContext` /
     `ScrubContext` / `RebalanceContext` beside it. The other loops each walk `inode:` themselves
     today and sharing one walk across passes is a much larger refactor that is **not** in scope;
     asserting a store-wide scan count would demand it by the back door.

  Legs (1), (2), (3) and (5) are the binding ones — each fails on the base for a *behavioural*
  reason, not a missing symbol.

  **Not in the discriminator, covered by C4-ci:** positive matches on any variant or field this
  patch introduces, and the per-pass regressions in `crates/custodian/tests/{reconstruction,
  backfill,rebalance}.rs`.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches and M4's integration branch is merged and deleted. #648/#649/#650/#651/#652
  all landed on `main` directly. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`. Not
  `pdca-integration/main` — that is the driver's run-scoped fold branch, and `wave_mode = "merge"`
  means a dependent builds on a genuinely merged `origin/main` anyway.)
- Scope (one logical fix) / out of scope: make the three passes that scan `inode:` **read every committed object through the
  shared resolver, contain what they cannot read, and refuse rather than abort** the writes this
  slice does not own.
  - `crates/custodian/src/reconstruction.rs` — the pass reads each committed object's chunk map
    **once per pass** and assesses its queued obligations against that reading, instead of
    re-reading the namespace per obligation. A chunk whose `ChunkRef` lives in a `seg:` record is
    **refused, not drained**: the obligation stays queued and the pass does not certify. A chunk in
    a flat record is repaired exactly as today.
  - `crates/custodian/src/backfill.rs` — a segmented record is left **byte-identical**, declined
    with a stated reason on the audit seam and counted on a gauge, while a fillable **flat** record
    in the same store is still filled in the same pass. The remaining-placement gauge stays
    correct over a store containing segmented objects.
  - `crates/custodian/src/rebalance.rs` — the evacuation scan resolves per object; a fragment whose
    chunk lives in a `seg:` record is **refused and stays on the draining server**, and the pass
    does not report the drain satisfied. A flat chunk is evacuated exactly as today.
  - All three: a record that will not `decode`, and a generation the resolver cannot read, are
    **contained per object** — named on the audit seam, the walk continues, the pass answers
    non-certifying. A fault that is not a `ChunkMapError` still propagates.

  **Constraints carried forward (blockers found on the old #651 — must not recur; these bound the
  shape, they do not name it):**
  - **Bounded memory.** A pass may retain work proportional to the **obligations it holds** and to
    **one object at a time** — never the whole namespace's decoded chunk list, never any segment's
    exact bytes, and never a per-chunk deep copy of a segmented root (that is O(chunks × segments)).
    This slice writes no segmented record, so it needs to pin **nothing**; pinning exact bytes is
    #682's.
  - **Bounded work.** Backfill's remaining-placement gauge must not cost a **second resolving walk**
    of the namespace: on a store of segmented objects that doubles every `seg:` range read for a
    number the pass has already seen. One resolving reading per pass.
  - **Containment on *any* read fault, not just a segmented shape.** A serde decode failure on a
    concurrently-replaced root must contain that object, not abort the walk.
  - **A duplicate committed chunk id is ambiguous**, repaired by neither reference, and **both**
    objects are named. Do **not** rebuild the cross-object claim-counting apparatus that was
    dropped at #651's replan — this is the narrow rule only: if the one reading finds two committed
    references to the same `ChunkId`, neither is repaired and both keys are named on the audit
    seam. No new report schema, no `ambiguous-*` verdict surface, no mark-withholding keyed on it.

  **Out of scope:**
  - **Any write to a segmented record.** `repoint_chunk`, the record ceilings, and the
    repair/evacuation write path for a `seg:`-resident chunk are **#682**. A refusal in this slice
    writes **nothing at all**.
  - Restore and `desired_state` (**#651**, merged): `restore.rs` and `desired_state.rs` are **not**
    edited here. The marker comment at `restore.rs:616` names this slice; leave it, or update only
    its wording if the shared walk genuinely subsumes `committed_chunks` — do **not** refactor
    restore into it.
  - `gc.rs` / `scrub.rs` (**#650**, merged) — untouched.
  - The chunk-id floor (**#652**, merged); the committer, fence, rollback and resume (**#653**).
  - The **pre-existing** question of whether an ordinary `EvacOutcome::Aborted` (no free domain, a
    missing fragment) should certify — that is listed in #682's carried-forward constraints and is
    its to settle. This slice makes only the refusal **it introduces** non-certifying.
  - Any new or edited ADR / spec / proposal; any conformance-vector change; any new CLI subcommand
    or report-schema change.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 59 mutants tested in 61s: 31 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #681: make reconstruction, backfill, and rebalance resolve segmented chunk maps, contain per-object read faults, preserve refused work, and bound reconstruction to one namespace read.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable boundary between per-object chunk-map faults and store-wide faults, requires non-certification, and is supported by the target resolver contract (`crates/core/src/metadata.rs:2619`). |
| C2 Reproduction (red pre-fix) | PASS | In a scratch clone at `339da46`, all 8 base-visible discriminator tests compiled and failed on behavioral assertions (0 passed), including segmented-pass aborts and Q namespace scans (`crates/custodian/tests/segmented_map_passes.rs:651`, `crates/custodian/tests/segmented_map_passes.rs:993`). |
| C3 Change | PASS | The three affected scans now use the shared resolver, distinguish typed object faults from store faults, and keep flat-object work active while refusing unowned segmented writes (`crates/custodian/src/reconstruction.rs:795`, `crates/custodian/src/backfill.rs:96`, `crates/custodian/src/rebalance.rs:185`). |
| C4 Verification (red→green) | PASS | Restoring the production changes made the same 8 tests pass; isolated `cargo xtask ci` then passed, and 59 diff mutants produced 31 caught plus 28 unviable with no survivors (`crates/custodian/tests/segmented_map_passes.rs:651`). |
| C5 Causal adequacy | PASS | The change removes the seven inline flat-map reads in favor of the shared resolver rather than adding a capability probe, and an incomplete reading withholds queue drain and certification at the decision point (`crates/custodian/src/reconstruction.rs:424`). |
| T1 Structure | PASS | The patch stays within the four allocated files, adds no dependency or public API, and preserves the core/traits seam (`crates/custodian/src/backfill.rs:50`, `crates/custodian/src/rebalance.rs:52`, `crates/custodian/src/reconstruction.rs:51`). |
| T2 Shape | FAIL | The patch exceeds the brief's ≤900 added-semantic-line budget: 1,177 added nonblank/noncomment lines remain about 957 even after delimiter-only mechanical lines are excluded, with the new fixture alone reaching 1,185 lines (`crates/custodian/tests/segmented_map_passes.rs:1185`). |
| T3 Runtime | PASS | In-memory fault/race legs and the full workspace plus DST runtime suite pass; the brief requires no live backend, though the repository's scheduled kill-and-reconstruct observation remains advisable for this durability surface (`crates/custodian/tests/segmented_map_passes.rs:831`, `crates/custodian/tests/segmented_map_passes.rs:1061`). |
| T4 Contribution | NEEDS-HUMAN | The owner must reconcile the four reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently reproduced or triaged; affected-path history and closed PR #647 were independently confirmed. |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must add a direct regression for duplicate committed `ChunkId` ambiguity — no test proves that neither reference is repaired and both object keys are named, so the required loss-prevention branch at `crates/custodian/src/reconstruction.rs:917` is not exercised. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether the contained/non-certifying behavior and planned production-tier observation are sufficient for self-healing and decommission safety — automated in-memory and DST evidence cannot authorize that operational durability judgment. |

### Advisory — adversary

# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; nothing here gates. Every citation is grounded on the target tree at
`/home/eddie/wyrd/wyrd.pdca-wt-l1`. The red→green evidence was re-run independently
(source copied to scratch, the three production files reverted with `git checkout`, the new
test kept): **8/8 red pre-fix as assertion/behaviour reds, 8/8 green post-fix** — the C4-verify
row's claim stands, and the reds are not compile reds. `cargo test -p wyrd-custodian` is green
on the target. Findings below come from probe tests I wrote against the patched tree (and,
where a regression is claimed, re-run against the reverted tree); the scratch copy has been
removed.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:279-280` refuses (and logs) *per
  chunk*, so one ordinary segmented object floods the durability seam every pass.** Concrete
  case, measured: a single segmented object of 64 chunks whose fragments sit on a draining
  server produced **64** `action="refused"` audit warns and **64**
  `rebalance_evacuation_refused` counter increments in one `rebalance::reconcile` pass, while
  the sibling path in this same patch (`crates/custodian/src/backfill.rs:172`) emits **one**
  `action="declined"` record carrying a `chunks` count for the same object. A segmented map
  only exists because the flat map exceeded the 100 KB value ceiling
  (`crates/core/src/metadata.rs:249-254`), i.e. thousands of chunks, and under any RS scheme
  spread across the fleet *every* chunk of that object has a fragment on the draining server —
  so a 10 GiB multipart object emits ~10⁴ warn events plus ~10⁴ counter increments **on every
  rebalance cadence, indefinitely** until #682 lands. The brief's cited peer callsite
  (`crates/custodian/src/gc.rs:155-165`) attributes **per object**; `emit_declined` already
  shows the aggregating shape in this diff. Fix is local: accumulate refused chunks per object
  and emit once with a count (keep `refused` charged per chunk if the certification arithmetic
  wants it).

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:271` names a refusal only in
  the assessment loop, so a later transient fault costs the operator the attribution the brief
  told this slice to protect.** Concrete case, measured: two queued obligations — the first
  (`repair:41216`, first in `queued_repairs` order) on a flat record whose fragment fetch
  raises a *transient* chunk-store fault, the second resident in a healthy segmented record.
  The pass propagates at `crates/custodian/src/reconstruction.rs:502` and emits **zero**
  `action="refused"` records: the operator gets `Err(... d server busy)` and no name for the
  object that is actually blocking the repair. The refusal is already known at
  `crates/custodian/src/reconstruction.rs:885` / `:917-928` (where `Site::Refused` is
  recorded); emitting there — as `emit_unresolvable` correctly does, mid-walk — is the
  placement the brief demanded ("attribution emitted by the consumer, per object, **before**
  the rest of the pass, so a later transient fault cannot cost the operator the record's
  name… mirror the placement, not just the call", brief §Citations expected). Note a
  *readable* segmented record produces no `unresolvable` record at all, so `emit_refused` is
  its **only** attribution — this is not a duplicate signal.

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:917-928` turns a repair the
  base performed into a permanent, never-certifying stall when one committed record names the
  same `ChunkId` twice.** Measured both ways with the same fixture: on the reverted tree the
  pass answers `Changed`, rebuilds the chunk and drains the obligation; on the patched tree it
  answers `Blocked`, logs `ambiguous-chunk-id`, and the obligation stays queued forever — the
  store never converges and reconstruction never certifies again. The brief's duplicate rule
  is written for two *objects* ("both keys are named"), and its stated rationale
  (`crates/custodian/src/reconstruction.rs:402-405`: "repointing the wrong record loses the
  other object's bytes") does not hold when both references live in the one record the pass
  would CAS. Honest caveat, which is why this is a judgment call rather than a build defect:
  the production minter (`crates/server/src/cli.rs:1964-1971`, `(inode<<64)|seq`) cannot emit
  an intra-record duplicate, so the input is corruption-only — but C-1 ("nothing exits the
  state") is exactly the invariant the brief invoked, and the base did exit it.

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:170-174`: with an empty
  repair queue the pass certifies `Satisfied` over a store holding an undecodable committed
  record, and names nothing.** Measured: one undecodable `inode:1` beside one healthy object,
  empty queue → `outcome=Satisfied`, zero `unresolvable-chunk-map` records. That is precisely
  the claim the same function forbids twenty lines later
  (`crates/custodian/src/reconstruction.rs:176-180`: "even when no obligation names one of
  their chunks, this pass has answered over LESS than the store, and saying `Satisfied` there
  tells an operator redundancy is whole"). It is base-parity and deliberately documented at
  `:164-169` (an idle pass should not resolve the namespace), and success-criterion leg (3)
  cannot see it because its fixture always carries a queued obligation — so the reviewer had
  no gate to trip. The human call: is a certification answer that flips with queue depth the
  intended reading of "a pass that refused work does not certify", or should the carve-out be
  narrowed (e.g. still walk when the previous pass reported unreadable records)?

## Refutations attempted that failed

- *Is the red a real red on the production path?* Reverting only the three `src` files and
  keeping `crates/custodian/tests/segmented_map_passes.rs` reproduces 8 failures, each an
  assertion or a `Store(SegmentedMapUnsupported{...})` behaviour red — no missing-symbol
  compile red, no vacuous green, and the tests drive the real entries (`reconcile_step`,
  `backfill::reconcile`) over trait doubles, not a re-implementation.
- *Does the backfill gauge regress now that `emit_remaining` counts inside the walk
  (`crates/custodian/src/backfill.rs:233`) instead of re-scanning?* I looked for a population
  the old post-pass scan counted and the new walk does not: a malformed chunk cannot have an
  empty `placement` (`crates/core/src/metadata.rs:219-230`), a filled record contributes 0, a
  declined record contributes `to_fill.len()`, a lost CAS re-reads the live generation. Probed
  the mixed fill+malformed+full store: gauge 0 then 0, same as the base would publish. No
  divergence found.
- *Do two obligations in one record now collide on the shared `Arc<InodeRecord>` snapshot?*
  The second repair loses its CAS and stays queued — but the reverted tree behaves identically
  (all assessments precede all repairs there too), so it is pre-existing, not this diff's.
- *Can `plan.prior_chunks` ever disagree with `plan.prior.chunk_map.as_flat()`?* No: a
  segmented `resolved.record` is refused before any plan is built
  (`crates/custodian/src/rebalance.rs:232-239`, `reconstruction.rs:869-889`), and the resolver
  returns the record and chunk list from the *same* generation
  (`crates/core/src/metadata.rs:2619-2632`, `:2652-2687`), including on the supersede restart.
- *Is the store-fault leg mocked away?* No — the injected fault is a plain `io::Error` on
  `get` under `inode:`, which only the segmented resolve's settle re-read issues
  (`crates/core/src/metadata.rs:2570`), and it propagates through the real downcast rule.
- *Docs currency:* `docs/design/architecture/06-runtime-view.md:29-31`'s "a consumer that has
  not yet adopted it refuses a segmented map outright" still reads true for the remaining
  non-adopters (`crates/core/src/read.rs:96`, `commit_chunk_map`, `high_water_marks`), so the
  confirm-only touch was correctly a no-op.
- One further behaviour I deliberately did **not** score as a refutation, since the brief
  settles it and #682 tracks it: a *healthy* chunk resident in a segmented record whose stale
  obligation can never be drained keeps reconstruction permanently `Blocked` on an otherwise
  healthy store (measured over three consecutive passes: `Blocked`, obligation still queued),
  even though discharging it needs no write at all.

## On the gate rows

- `C4-verify` "PASS — red without the fix, green with it (8 test(s) ran red)" — independently
  reproduced; the claim is warranted.
- `T4-batch-review` is **fail (gating)** with 4 blocking findings whose log
  (`review-b…`) is not in this artifact-only bundle, so I could not triage or contest them;
  that row, not this file, is what blocks.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — The owner must reconcile the four reported batched-review blockers before sign-off — `scripts/review-branch` and its detailed logs are absent from this artifact-only bundle, so that red row cannot be independently reproduced or triaged; affected-path history and closed PR #647 were independently confirmed.
- [ ] T5 Judgment — The rebuild must add a direct regression for duplicate committed `ChunkId` ambiguity — no test proves that neither reference is repaired and both object keys are named, so the required loss-prevention branch at `crates/custodian/src/reconstruction.rs:917` is not exercised.
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether the contained/non-certifying behavior and planned production-tier observation are sufficient for self-healing and decommission safety — automated in-memory and DST evidence cannot authorize that operational durability judgment.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_681/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 111 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized (113 KB patch vs 100 KB threshold; ~1,177 added semantic lines vs ~900 budget) and not converging: 2 build rounds with a flat (not shrinking) impl-finding count, plus T4 batched-review still gating with 4 unresolved findings. Re-split at Plan rather than another Do round — per docs/2026-07-31-oversized-slices-report.md, over-budget slices don't converge with more Do iterations. Carry forward into the split: - noncanonical inode-key parsing bugs (backfill.rs:174, rebalance.rs:259, reconstruction.rs:880) that can cause writable/CAS races on non-canonical keys. - rebalance's per-chunk refusal logging floods the audit seam (should aggregate per object like backfill's emit_declined). - transient store fault can suppress the "refused" attribution the brief required. - open judgment calls for the next Plan to settle explicitly: duplicate ChunkId within a single record (base used to repair it; brief's rule targets cross-object duplicates) and whether an idle pass with an empty repair queue should be allowed to report Satisfied over an unreadable object in the store.
- By / date: Eduard Ralph / 2026-08-05

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
