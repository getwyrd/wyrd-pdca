# Result — issue 697 / reconstruction-reads-through-resolver-once-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/reconstruction.rs` reads the chunk map inline out of the inode
  record at **three** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/reconstruction.rs:332` | `assess` (`:317`) | one obligation's assessment, and the pass |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | the binding repair commit |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` (`:620`) | the whole `inode:` scan, for every obligation |

  Three live consequences beyond the shared one (a single segmented object stops repair for the
  whole store, so a store that has published one multipart object stops restoring redundancy):
  1. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as "referenced by no committed chunk
     map" → `Assessment::Drain` (`:322-325`), deleted in the drain batch (`:270-276`). Latent today
     (the error fires first), but it is the loss this child must not introduce while removing the
     abort — the last record saying live data is under-replicated.
  2. **The deployed repair loop is Q namespace scans × N point reads.** `reconcile` calls `assess`
     per obligation (`:185`); `assess` calls `find_chunk` (`:322`), which scans all of `inode:` and
     decodes every record. Wiring the resolver in naively makes each of those N objects also cost a
     bounded `seg:` range read — Q×N *resolves*. **This is the finding still open at #647's close.**
  3. **Containment is not per object** — a record that will not `decode` ends the walk at `:625`,
     before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_reconstruction.rs`
  passes, driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}`,
  `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}`, plus `Custodian::elect` +
  `FencedZone::new` over `wyrd_coordination_mem::MemCoordination` for the fence (`leadership.rs:31`,
  `:69`; the shape is `segmented_map_consumers.rs:406-410`) — over in-memory `MetadataStore` /
  `ChunkStore` doubles. Each leg drives `reconcile_step`, the real fenced control point, not an
  internal helper. **The discriminator MUST NOT name any symbol this patch introduces**: the red leg
  reverts production, so such a reference makes the target fail to compile and the red degrades to
  UNVERIFIABLE (exit 77). `Reconciled::Blocked` already exists on the base
  (`reconciliation.rs:44`) and may be named.

  **Eight legs over ONE shared fixture** — one store, one seeding helper, one counted metadata
  double:

  1. **A segmented object no longer ends the pass, and the flat work in the same store still
     happens.** One healthy segmented object (raw `seg:` records + a segmented root, **never** a
     committer) beside an under-replicated **flat** chunk with a queued repair: `reconcile_step`
     with a `ReconstructionContext` returns `Ok` (today `Err`), the flat chunk's placement moves,
     and its obligation is drained. *(binding — base-red)*
  2. **A repair for a chunk in a `seg:` record is refused, never discarded, and the pass does not
     certify.** That obligation is **still in `queued_repairs`** afterwards; the `seg:` record's
     bytes and the root's `version` are **byte-identical**; the refusal carries a stated reason and
     a counted gauge; the pass answers `Reconciled::Blocked`. *(binding — base-red; this is
     defect 1 above, the loss this child must not introduce)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store — (a) a committed root naming a
     `SegmentRef` whose `seg:` record was never written, and (b) a committed record whose own bytes
     will not `decode`; assert in the fixture that `resolve_chunk_map` really errors on (a). Beside
     them, a healthy flat chunk carrying the same work as leg 1. Assert the conjunction: `Ok`,
     `Blocked` (never `Satisfied`), **the healthy object's repair still happens**, no obligation is
     drained for want of a reading, and both damaged objects are **named** by their `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470`). *(binding — base-red)*
  4. **Rule A — the pass never writes to a generation it did not read.** A metadata double whose
     `scan` answers a **stale segmented** root while `get` answers a **live flat** root placing an
     under-replicated chunk: `reconcile_step` returns `Ok`, repairs **nothing**, leaves the live
     record's `version` unchanged, keeps the obligation queued, and answers `Blocked`.
     *(binding — base-red; this is the leg whose absence let four review rounds re-open the same
     question, and it is the same fact as round 7's T4 blocker at `reconstruction.rs:659` **on the
     v7 tree** — these line numbers index `iteration-v7/patch.diff`, NOT the base)*
  5. **A duplicate committed `ChunkId` is repaired by neither reference, and both objects are
     named.** Two committed objects — and, in the same store, one record carrying the id twice —
     referencing one `ChunkId`, with that chunk enqueued for repair: neither placement changes, the
     obligation is **still queued**, both `inode:` keys are named, and the pass answers `Blocked`.
     Today the base repairs whichever reference `find_chunk` meets first (`:639`) and drains the
     obligation. **≤ 40 lines, reusing the shared fixture.** *(binding — base-red)*
  6. **The namespace is read ONCE per pass — O(N), not O(Q×N).** With **Q ≥ 3** queued obligations
     over **N ≥ 3** committed flat objects on the counted double: exactly **one**
     `scan(b"inode:")`, independent of Q (the base does Q of them), and the repairs still land.
     Then, on a store holding S segmented objects, the `seg:` range reads are **≤ S**. Build this
     leg with a `ReconstructionContext` and **no** GC / scrub / rebalance context beside it: the
     other loops walk `inode:` themselves and sharing one walk across passes is a much larger
     refactor that is **out of scope** — a store-wide scan count would demand it by the back door.
     *(binding — base-red; this closes the finding left open at #647)*
  7. **An empty queue performs no reading and answers `Satisfied`.** Over the store that already
     holds an unreadable object, run reconstruction with **no** obligations queued: `Satisfied`, and
     the counted double records **zero** `scan(b"inode:")`. *(NOT base-red, verified at Plan and
     stated rather than assumed: the base's loop is `for chunk in queue { assess(..) }`
     (`reconstruction.rs:185`), so an empty queue already scans zero times and already answers
     `Satisfied`. This leg is a REGRESSION guard on the restructure — leg 6 moves the reading OUT
     of the per-obligation loop, and the obvious way to do that is to read the namespace once
     unconditionally at the top of the pass, which would silently break this property and make the
     pass claim over objects it never needed to read. Required for that reason.)*
  8. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes the pass return `Err`, and the unreadable
     object's name is **already** on the audit seam even though the pass returns `Err` (Rule E).
     *(NOT base-red — this guards against over-containment; it passes before and after. Required
     despite that: without it, containing EVERY error would pass every other leg. Legs 7 and 8 are
     this child's two non-red legs and both are deliberate.)*

  Also assert **Rule C** as a sub-assertion of leg 3 (**≤ ~20 lines**, no ninth test): a committed
  record under `inode:007` beside `inode:7` leaves `inode:7` **byte-unchanged** and is never
  silently reinterpreted into it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: reconstruction **reads each committed object's map once per pass, assesses its queued
  obligations against that reading, contains per object what it cannot read, and refuses — rather
  than aborts or silently drains — the repair it does not own.** A chunk whose `ChunkRef` lives in
  a `seg:` record is **refused, not drained**: the obligation stays queued and the pass does not
  certify. A chunk in a flat record is repaired exactly as today, with **every** existing
  classification (`Repairable` / `Drain` / `Unreachable` / `Blocked` / `Unrepairable` / `Malformed`)
  and its gauge accounting preserved, **including the rule that a never-repaired condition stays
  off the repairable-backlog gauge**.

  **Rules, pinned at Plan — do not relitigate; each is bound by a leg above.**
  1. **Rule A — a pass acts only on a resolve that did not restart.** `resolve_chunk_map` restarts
     onto the live root when the caller's **segmented** snapshot was superseded
     (`metadata.rs:2338-2339`) and then answers a `ResolvedChunkMap` whose `record` is a generation
     the pass never scanned. Mixing that generation's chunk list with the snapshot's `prior_bytes`
     is exactly what makes unchecked indexing panic before the stale CAS can reject the plan —
     round 7's T4 blocker at `reconstruction.rs:659` (a **v7-tree** line, not a base line). If the
     resolve did not answer from the
     scanned generation, contain the object, repair nothing, keep the obligation queued, answer
     `Blocked`, re-read next pass — the resolver hands back the generation it resolved FROM — `ResolvedChunkMap.record`, a
     `Cow` carrying its own `version` (`metadata.rs:2256-2272`) — so the pass is able to tell
     the two apart; **how it tells is Do's to choose**. Bounded per C-1: an object whose shape
     changed under the scan is re-assessed on the next pass **because the obligation stays queued**.
     Leg 4 binds it. *(The previous brief pinned this as "unreachable by construction" and forbade
     a test; it was demonstrated false with a working double at
     `results/issue_681/iteration-v4/check-advisory-adversary.md:25`.)*
  2. **Rule B — an incomplete reading changes what the pass may CLAIM and what it may DISCARD,
     never what it may DO for the objects it read successfully.** A pass that met an unreadable
     object answers `Blocked` and **never drains an obligation**, but still repairs the healthy
     objects in the same store. Verified safe rather than argued: `repair_chunk` orphan-**marks**
     the displaced fragment (`reconstruction.rs:665-671` on the v7 tree), it never deletes it, and
     GC reclaims a marked fragment only past `ReferenceSet::protection`, which returns
     `incomplete-reference-set` and withholds **every** fragment while any object is unresolvable
     (`gc.rs:306-316`, consulted before every delete at `:191-194`); the displaced fragment is also,
     by construction, one the assessment proved MISSING or checksum-failing at its placed server.
     So the loss chain cannot close and strictness buys **no** safety, while costing every healthy
     object its repair. Leg 3 pins the progress half; legs 2 and 3 pin the never-discard half.
  3. **A pass certifies only over the reading it performed.** With an **empty** repair queue the
     pass performs no namespace reading and answers `Satisfied` — it makes no claim about objects it
     never read. With a non-empty queue it DOES read the namespace, and an object it cannot read
     then makes that reading incomplete → `Blocked`, because "drain this obligation as unreferenced"
     is only knowable over a complete reading. Precedent in this file family:
     `rebalance.rs:115-117` already returns `Satisfied` without reading `inode:` when no server is
     draining. Leg 7 binds it.
  4. **A duplicate committed `ChunkId` is ambiguous: neither reference is repaired, the obligation
     stays queued, both objects are named, the pass does not certify — and the rule is the same
     whether the duplicates sit in one record or two.** An obligation is keyed by chunk alone
     (`repair:<chunk_id>`, `repair.rs:32`) and both references address the same
     `FragmentId{chunk,index}`, so repairing "the first in scan order" repoints one record while
     orphan-marking a fragment the other still points at. Ids are allocator-minted
     (`write.rs:170`), never content-addressed, so a duplicate is always an anomaly, never
     legitimate dedup. **This is the narrow rule ONLY:** no cross-object claim-counting apparatus,
     no new report schema, no `ambiguous-*` verdict surface. Leg 5 binds it.
  5. **Rule C — read, write and name a record under exactly the key the store gave it.** On the
     base the pass parses the scanned key to an `InodeId` then re-derives `metadata::inode_key(id)`
     for the CAS (`reconstruction.rs:598`); `"inode:007"` and `"inode:+3"` both parse, so the pass
     reads one record and CASes another. Precedent: `gc.rs:280-294`, `:402`.
  6. **Rule D — a refusal is reported once per object, not once per chunk.**
  7. **Rule E — attribution for an object the pass could not read is emitted where the object is
     read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
     cannot cost the operator the name of the record to repair. **Load-bearing, not logging
     hygiene:** a genuinely corrupt root has no repair path (a fragment carries only
     `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`) and no operator tooling
     (tracked as **#694**), and reclamation is halted store-wide meanwhile — that name is the
     operator's entire situational awareness. Leg 8 binds the placement.

  **Constraints (they bound the shape; they do not name it):** bounded memory — work proportional
  to the **obligations held** and to **one object at a time**, never the whole namespace's decoded
  chunk lists and never any segment's exact bytes; **one** resolving reading of the namespace per
  pass; containment on **any** read fault by exactly gc.rs's downcast rule (`gc.rs:402-416`) —
  `Ok(ChunkMapError)` is contained as *this record's* fault, any other error propagates because a
  store fault is not one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the repair write
    path for a `seg:`-resident chunk are **#682**. A refusal writes **nothing at all**.
  - `backfill.rs` and `rebalance.rs` — **the two sibling children**. Do not touch them; a diff that
    does will conflict with a bundle building in the same wave.
  - **Sharing ONE namespace walk across all loops** (GC / scrub / rebalance / reconstruction) — a
    separate refactor, explicitly not this child's. Leg 6 is scoped to a reconstruction-only context
    for exactly this reason.
  - `gc.rs`, `scrub.rs` (#650), `restore.rs`, `desired_state.rs` (#651) — untouched; **leave** the
    deferral marker at `restore.rs:616` (its per-reference granularity differs, as that comment
    says).
  - The chunk-id floor (#652); the committer, fence, rollback and resume (#653); the M8 operator
    surface (#694).
  - The existing suite `crates/custodian/tests/reconstruction.rs` must stay green **unmodified** —
    v2 achieved that with these same production changes, so a need to edit it signals an answer
    changed further than intended, not a licence to edit it.
  - **No docs edit** (checked at Plan); no new or edited ADR / spec / proposal; no
    conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the discriminator
    needs (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
    `tracing-subscriber`) is already declared on `crates/custodian`, verified at Plan.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 45 mutants tested in 49s: 25 caught, 20 unviable

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

Reviewing issue #697: make custodian reconstruction resolve the committed namespace once per pass, contain unreadable objects, and preserve repair obligations it cannot safely discharge.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit—one resolver reading, per-object containment, exact-key CAS, no drain on incomplete evidence, and no segmented writes—so the safety decision is testable at `crates/custodian/src/reconstruction.rs:156`. |
| C2 Reproduction (red pre-fix) | PASS | Independent stash replay kept the new test on base `339da46`: it compiled and failed 8 of 9 tests on behavioral assertions, grounding the defect at `crates/custodian/tests/segmented_map_reconstruction.rs:343`. |
| C3 Change | NEEDS-HUMAN | Plan must decide whether to accept the ninth safety leg and its wider guard or require compression—the brief mandates exactly eight legs, while the patch explicitly declares nine at `crates/custodian/tests/segmented_map_reconstruction.rs:7`. |
| C4 Verification (red→green) | PASS | Independent restore replay passed all 9 discriminator tests, and full `cargo xtask ci` passed with real typos, docs, deny/machete, conformance, workspace, and 50-seed DST checks; the exercised entry point is at `crates/custodian/tests/segmented_map_reconstruction.rs:321`. |
| C5 Causal adequacy | NEEDS-HUMAN | Architecture must choose between suspending repair on an incomplete namespace and adding content/conditional-write authority—the landing guard at `crates/custodian/src/reconstruction.rs:671` cannot stop an unreadable duplicate's valid shard from being accepted at `crates/custodian/src/reconstruction.rs:481` and poisoning reconstruction at `crates/custodian/src/reconstruction.rs:625`. |
| T1 Structure | PASS | The patch has exactly the prescribed production file plus one new integration-test file, whose crate root carries `#![forbid(unsafe_code)]` at `crates/custodian/tests/segmented_map_reconstruction.rs:16`. |
| T2 Shape | FAIL | The hard discriminator budget is exceeded: 9 legs and 400 semantic/614 raw lines versus exactly 8 and at most 380/620; the extra leg and over-budget rationale are explicit at `crates/custodian/tests/segmented_map_reconstruction.rs:7`. |
| T3 Runtime | FAIL | A deterministic in-memory interleaving reconstructed and landed foreign payload from a valid shard hidden behind an unreadable duplicate claimant; the permissive survivor admission is at `crates/custodian/src/reconstruction.rs:481`, so C-1 is not preserved. |
| T4 Contribution | NEEDS-HUMAN | Human must accept contribution readiness without an independent artifact-wrapper replay—the supplied bundle omitted `scripts/pdca contribcheck`; affected-path history was independently checked and found no open overlap, with closed-unmerged #647 as the only rejected prior work. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add direct poisoned-survivor and get→put interleaving oracles—the current leg pre-seeds the target before driving at `crates/custodian/tests/segmented_map_reconstruction.rs:598`, so it cannot expose either causal failure. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether progress during an incomplete namespace is fit for purpose despite the demonstrated foreign-shard risk—this determines whether Rule B changes or the storage seam/scope expands beyond the policy stated at `crates/custodian/src/reconstruction.rs:307`. |

### Advisory — adversary

# Adversarial review — issue #697 (reconstruction reads through the resolver once, contained)

**Evidence re-run, independently, in both directions** (scratch clone of `339da46`, `cargo 1.96.0`):
base + new test → **8 of 9 legs red** (legs 1–6, 8, 9; leg 7 green, as the brief declares);
patched → **9/9 green**; whole `-p wyrd-custodian` suite green, `tests/reconstruction.rs` untouched
and green. The legs drive `reconcile_step`, name no patch-introduced symbol, and go red on
behaviour (`find_chunk met a segmented chunk map`, `namespace reads 3 ≠ 1`, `Satisfied ≠ Blocked`),
not on compilation. The C4-verify claim holds. Budget: production 229/230 semantic lines (pass);
test file **400 semantic vs. the brief's 380 cap** (614/620 raw), self-declared in its header.

The findings below are all against **leg 9 / `may_land`** — the one leg the builder added beyond
the brief's eight. The rest of the patch I could not break; see the last section.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:671`: `may_land` is applied on
  *every* claimed slot, but its own justification is conditional — so it permanently stalls a
  repair the base completes.** The guard's stated reason (`:662-667`) is "*While its reading of the
  namespace has a hole in it*, an object it could NOT read may reference this very `FragmentId`".
  Under a **complete** reading no claimant can be hidden — `CommittedIndex::note` (`:799-811`) has
  already turned any second claim into `Site::Refused`, so nothing is repaired anyway. Yet the call
  site never consults `index.complete()`. Demonstrated failing case (run, both directions): one
  committed flat object under `inode:7`, chunk `0xA1` RS(2,1) placed `[0,1,4]` with fragment 2
  absent, no segmented object, no unreadable record, and a stray non-identical fragment already
  standing at `FragmentId{0xA1, 2}` on server 2 (the domain the selector deterministically picks).
  **Base:** pass 0 → `Changed`, placement `[0,1,2]`, obligation drained. **Patched:** pass 0, 1, 2 →
  `Blocked`, placement still `[0,1,4]`, obligation still queued, a fresh `NEEDS-HUMAN`
  `would-overwrite` row each pass. There is **no exit**: the selector is deterministic, and GC never
  reclaims an un-orphan-marked stray (`crates/custodian/src/gc.rs:196-211` — no orphan lease, no
  expired pending lease ⇒ `reason = None`, "conservatively keep it"), so the chunk stays
  under-replicated forever. That is the permanent failure mode C-1 forbids, introduced by the fix
  rather than removed by it. One-line scope fix: only ask `may_land` when the pass's reading is
  incomplete.
- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:579-613`: leg 9
  never constructs the hazard it names, and pins the over-broad guard.** Its doc says the foreign
  bytes belong to "an object legs 2-4 show this pass may be unable to READ, which leaves
  `ambiguous-chunk-id` blind to it" — but its fixture is `seed_fixture()` + `inode:8`, every record
  of which decodes, parses and resolves. Measured on the patched tree: **0 `unaccounted` rows, 0
  `incomplete-reading` rows, 0 `refused` rows, 1 `would-overwrite` row** — the reading is
  *complete*, so in that store a hidden second claimant is impossible and the planted bytes could
  not exist. The leg therefore asserts nothing about the hidden-duplicate case and instead *locks
  in* the unconditional guard: scope `may_land` correctly (previous bullet) and leg 9 fails on
  `assert_eq!(held, Some(foreign))` at `:601`. Related gap in the same helper: no leg drives a
  **transient** `get_fragment` fault at the re-placement target, so the "fail-closed" half of
  `reconstruction.rs:756-760` is unasserted (`--in-diff` mutants replace whole functions, not match
  arms, so C5's 0 survivors does not cover it).
- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:215` + `:293`: a `would-overwrite`
  refusal floors `gauge.reconstruction_under_replicated` at ≥1 forever.** `under_replicated += 1`
  happens in the assessment loop; the refusal happens later, in the repair loop (`:326`), so a
  never-completable repair stays **on** the repairable-backlog gauge. Measured over three
  consecutive passes on the store above: `{"gauge.reconstruction_under_replicated":1}` every pass,
  with the repair never landing. This is exactly the defect class this file's own comments call an
  iteration-5/7 MUST-FIX (`:178-202`, `:236-242`, `:518-530` — `Assessment::Blocked` and
  `Assessment::Refused` are both deliberately diverted **off** the gauge for this reason) and which
  the brief pins as preserved ("including the rule that a never-repaired condition stays off the
  repairable-backlog gauge", brief §Scope). The day-one "rise then return to zero" signal is
  unobservable on any store carrying one such slot.
- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:1154-1164`: the `would-overwrite`
  NEEDS-HUMAN row names no object, though the pass is holding its key.** Rule E's own rationale
  (brief rule 7) is that "a fragment carries only `FragmentId { chunk, index }`
  (`crates/traits/src/lib.rs:45-48`) and there is no operator tooling (#694), so that name is the
  operator's entire situational awareness". `emit_would_overwrite` emits `dserver`, `chunk`, `index`
  and stops — captured row: `{"action":"would-overwrite","dserver":2,"chunk":"…a1","index":2}`, no
  `inode` field — while the caller has `plan.inode_key` in scope two lines away (`:672`, used at
  `:717`). Every other new emitter names its object (`emit_unaccounted`, `emit_ambiguous`,
  `emit_refused`). One added field.
- **NEEDS-HUMAN [human] — two written contracts are now contradicted, and the brief forbids editing
  either file.** (a) `docs/design/adr/0011-…md:32-42` documents exactly three reconstruction
  counters and the netting rule *"successful repairs ≈ `reconstruction_repaired` −
  `reconstruction_conflict` − `reconstruction_aborted`"*, naming `reconstruction.rs` as its source
  of truth; the patch adds a **fourth** terminal offset and rewrites the formula in a code comment
  (`crates/custodian/src/reconstruction.rs:282-285`) only — an operator applying the ADR now
  over-counts successful repairs by every `would-overwrite`. (b) `Reconciled::Blocked`'s public
  rustdoc (`crates/custodian/src/reconciliation.rs:22-44`) defines the outcome as *"at least one
  committed object's chunk map could not be read … so the reference set the loop reasoned over is
  incomplete"*; reconstruction now answers `Blocked` with a **complete** reading whenever
  `refused > 0` (`:339`). The brief pins "no new or edited ADR" and "exactly 2 files", so neither
  can be corrected inside this bundle — a human must decide whether to widen the budget, update the
  ADR, or drop the extra counter along with the guard above.

## Attempted and could not refute

Tried, with a working fixture, and failed to break: **Rule A** (`:924` compares `resolved.record`
by value — a flat resolve borrows the caller's record so equality is exact, and a superseded
segmented resolve genuinely restarts onto a differently-grouped root; leg 4 goes red on the base for
the right reason). **Rule B's backstop** — verified independently at `gc.rs:296-316` and `:191-195`:
`protection` really does withhold *every* fragment while `unresolvable` is non-empty, so a repair
made under an incomplete reading cannot have its displaced fragment reclaimed. **Rule C** — the base
demonstrably reads `inode:007` and CASes `inode:7` (I reproduced the lost CAS); the patch keys on the
raw scanned bytes throughout. **Serialization identity** — the CAS precondition is now the *stored*
bytes (`:719`), and the fixture's deliberately non-canonical spelling makes that binding. **The Q×N
property** — leg 6 measures 3 scans on the base against 1 with the patch, and `seg:` reads ≤ S.
**Containment reach** — I searched for a legitimate `inode:`-prefixed key that is not
`metadata::inode_key(id)` (which would make `unparsable-inode-key` block every pass forever); there
is none in the tree. **Multi-obligation objects** — two queued chunks in one flat record still
converge (one per pass, one bogus `conflict` row), byte-identical to base behaviour, not a
regression. **`assess`'s six existing classifications and their gauge accounting** are unchanged
apart from the flooring above.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Plan must decide whether to accept the ninth safety leg and its wider guard or require compression—the brief mandates exactly eight legs, while the patch explicitly declares nine at `crates/custodian/tests/segmented_map_reconstruction.rs:7`.
- [ ] C5 Causal adequacy — Architecture must choose between suspending repair on an incomplete namespace and adding content/conditional-write authority—the landing guard at `crates/custodian/src/reconstruction.rs:671` cannot stop an unreadable duplicate's valid shard from being accepted at `crates/custodian/src/reconstruction.rs:481` and poisoning reconstruction at `crates/custodian/src/reconstruction.rs:625`.
- [ ] T4 Contribution — Human must accept contribution readiness without an independent artifact-wrapper replay—the supplied bundle omitted `scripts/pdca contribcheck`; affected-path history was independently checked and found no open overlap, with closed-unmerged #647 as the only rejected prior work.
- [ ] T5 Judgment — Rebuild must add direct poisoned-survivor and get→put interleaving oracles—the current leg pre-seeds the target before driving at `crates/custodian/tests/segmented_map_reconstruction.rs:598`, so it cannot expose either causal failure.
- [ ] Validation — fitness-to-purpose — Human must decide whether progress during an incomplete namespace is fit for purpose despite the demonstrated foreign-shard risk—this determines whether Rule B changes or the storage seam/scope expands beyond the policy stated at `crates/custodian/src/reconstruction.rs:307`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
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
- Iteration delta (if iterating): Human overrides the size-backstop's iterate-plan recommendation, on the basis that the outstanding items are concrete, demonstrated implementation bugs in the leg-9 (`may_land`) guard the builder added beyond the brief's 8-leg scope. Next round must: 1. Fix the permanent-stall bug at crates/custodian/src/reconstruction.rs:671 — `may_land` is applied unconditionally on every claimed slot instead of only when the pass's own reading is incomplete. Per the adversary's one-line scope fix: only consult `may_land` when `index`/the reading is incomplete; otherwise a completable repair the base would finish can block forever with no exit path (demonstrated reproduction in check-review.md's adversary section). 2. Fix the gauge-floor bug at reconstruction.rs:215/:293 — a `would-overwrite` refusal must not permanently floor `gauge.reconstruction_under_replicated`; the brief explicitly requires a never-repaired condition to stay off the repairable-backlog gauge (as `Assessment::Blocked`/`Assessment::Refused` already do). 3. Add the missing `inode` field to the `would-overwrite` NEEDS-HUMAN audit row (reconstruction.rs:1154-1164) — the pass has `plan.inode_key` in scope but the emitted row omits it, unlike every other new emitter. 4. Rewrite or replace the leg-9 test fixture (segmented_map_reconstruction.rs:579-613) — it does not construct the hidden-duplicate hazard it claims to test (the reading is complete in its own fixture), so it currently just locks in the over-broad guard behavior. Once (1) is fixed, this leg must actually assert the guard fires only under an incomplete reading, or be dropped if it can't be made to. 5. Resolve the two contract contradictions: the ADR's counter/netting formula (docs/design/adr/0011) and `Reconciled::Blocked`'s rustdoc (`reconciliation.rs:22-44`, "chunk map could not be read" / incomplete reference set) are both now inconsistent with `Blocked` being returned over a complete reading whenever `refused > 0`. Since the brief forbids editing the ADR or reconciliation.rs's doc, either narrow the new behavior so it doesn't contradict them, or bring this explicitly back to Plan if it can't be reconciled within the existing 2-file/8-leg budget — do not ship a rebuild that leaves the written contracts contradicted. 6. Revisit whether leg 9 belongs in this slice at all, now that its guard is being narrowed to only the incomplete-reading case in (1) — it may fold naturally into the existing 8 legs (e.g. as part of leg 3/4) rather than remaining a declared 9th leg over the brief's cap; prefer bringing the test file back within budget over carrying an explicit overage into the next round. T4-Contribution and Validation fitness-to-purpose remain open human-judgment items for the next sign-off, not implementation work for this round. Do not treat this as license to keep growing the slice: if a 4th round still can't land these within the existing scope (or close to it), revisit iterate-plan — the demonstrated hazard (hidden duplicate claimant behind an unreadable object) may turn out to need its own properly-scoped brief rather than a bolt-on guard.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
