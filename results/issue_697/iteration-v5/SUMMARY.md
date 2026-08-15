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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — PASS on confirm — first run failed transiently: xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit stat
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 42 mutants tested in 49s: 2 missed, 23 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make custodian reconstruction resolve committed chunk maps once per pass, contain unreadable objects, retain uncertain obligations, and refuse unsafe segmented or ambiguous repairs.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Plan must decide whether `Reconciled::Blocked` is widened to readable-but-unwritable or ambiguous work and authorize the contract update — the brief requires that outcome while the public definition still requires an unreadable chunk map (`crates/custodian/src/reconciliation.rs:25`, `crates/custodian/src/reconstruction.rs:335`). |
| C2 Reproduction (red pre-fix) | PASS | Keeping the new base-visible test while stashing production made 7 of 8 tests fail behaviorally, with only the declared empty-queue regression leg green (`crates/custodian/tests/segmented_map_reconstruction.rs:358`). |
| C3 Change | PASS | The in-scope implementation replaces per-obligation lookup with one resolver-backed index and confines the overwrite probe to incomplete readings (`crates/custodian/src/reconstruction.rs:161`, `crates/custodian/src/reconstruction.rs:557`). |
| C4 Verification (red→green) | PASS | Restoring production made all 8 discriminator tests green; typos, docs lint/render, fmt, clippy, build, workspace tests, machete, deny, conformance, statics, orchestrator guard, and the 50-seed DST tier also passed, with the initial deny failure discharged as a read-only global-cache host fault by rerunning the real tool against a writable cache (`crates/custodian/tests/segmented_map_reconstruction.rs:358`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must tighten direct oracles — the unreadable root contains `C_UNSEEN`, while the alleged hidden `C_HELD` claimant is only unowned planted bytes (`crates/custodian/tests/segmented_map_reconstruction.rs:311`, `crates/custodian/tests/segmented_map_reconstruction.rs:431`); an actual hidden-claimant scratch variant passed, but two independently reproduced mutants also show the unreachable gauge remains unobserved (`crates/custodian/src/reconstruction.rs:232`). |
| T1 Structure | PASS | The one-pass index retains work-proportional sites and the commit uses the store-provided key plus exact prior bytes, preserving key and serialization identity (`crates/custodian/src/reconstruction.rs:781`, `crates/custodian/src/reconstruction.rs:732`). |
| T2 Shape | PASS | The change stays at exactly two files and independently measures within every cap: production 219/230 semantic additions; test 377/380 semantic and 607/620 raw lines (`crates/custodian/tests/segmented_map_reconstruction.rs:607`). |
| T3 Runtime | PASS | The real fenced `reconcile_step` entrypoint, unchanged reconstruction suite, whole workspace, and DST campaign are green; no brief-declared runtime dependency remained unexercised (`crates/custodian/tests/segmented_map_reconstruction.rs:318`). |
| T4 Contribution | NEEDS-HUMAN | Human must decide contribution readiness on provisional automation — `scripts/review-branch --bundle` and `scripts/pdca contribcheck` were absent, so the reported six blockers and green artifact check could not be replayed; the independent affected-path check found merged predecessors, closed-unmerged PR #647, and no open overlap. |
| T5 Judgment | PASS | No additional implementation or architectural concern emerged after the stronger hidden-claimant experiment; the remaining decisions are explicitly routed in C1, C5, and T4 (`crates/custodian/src/reconstruction.rs:535`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide production fitness and whether to trigger the non-gating Tier-1 disk-fault and Tier-2 kill/reconstruct follow-ups — local evidence uses in-memory stores plus Tier-0 rather than a live deployment (`crates/custodian/tests/segmented_map_reconstruction.rs:37`, `AGENTS.md:78`). |

### Advisory — adversary

# Adversarial review — issue #697 (reconstruction reads through the resolver once per pass, contained)

Scope: `patch.diff`, `brief.md`, `check-gates.json`. All `path:line` are the patched target tree at
`$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `339da46`). Toolchain was available; every
claim below was executed, not read off. Throwaway clone `pdca-adversary-697-redleg` removed.

## The evidence — re-run in both directions

Reproduced independently: cloned the target at base `339da46`, dropped in **only** the new test file,
and ran it. **7 of 8 legs fail on the base**, each on a behavioural assertion or a real `Err` from the
base's `find_chunk`; only leg 7 (`an_empty_queue_reads_nothing_and_certifies`) passes, exactly as the
brief pre-declared. With the patched `reconstruction.rs` restored, **8/8 pass**. The legs drive
`wyrd_custodian::reconcile_step` — the production fenced entry — over `MetadataStore` / `ChunkStore`
trait doubles, not a parallel re-implementation, and none names a symbol the patch introduces. The
red→green stands.

## Findings

- **NEEDS-HUMAN [impl] — the new `would-overwrite` guard has no exit in the one window it is armed in;
  I reproduced a permanent stall.** `crates/custodian/src/reconstruction.rs:557-566` runs the probe
  only while `!index.complete()`, and `crates/custodian/src/reconstruction.rs:760` claims "Every no is
  a repair deferred … and it clears when the reading does". It does not: the guard's window is
  *exactly* the window in which `ReferenceSet::protection` returns `incomplete-reference-set` and GC
  withholds **every** fragment in the fleet (`crates/custodian/src/gc.rs:306-316`), so the occupant
  that blocks the landing can never be reclaimed while the block is in force. Concrete failing case,
  executed against the patched tree: (1) one under-replicated flat chunk, complete reading, repair
  dispatched — `put_fragment` lands the rebuilt fragment at the selected target `d2`
  (`reconstruction.rs:689`) and the repoint then **loses the CAS race**, which ADR-0011
  (`docs/design/adr/0011-…:33`) documents as a routine outcome leaving "collectable garbage";
  (2) one committed record elsewhere in the namespace stops decoding — an unbounded condition, since
  there is no operator tooling for it (brief, #694); (3) passes 2, 3, 4 and 5 each return
  `Reconciled::Blocked`, repoint nothing, and leave the obligation queued — because the guard sees
  `d2` occupied by **this loop's own previous rebuild, byte-identical to the fragment it would
  write**. The chunk stays under-replicated indefinitely and is off the
  `reconstruction_under_replicated` backlog gauge (`reconstruction.rs:239`), so the day-one
  "returns to zero" signal reads clean. This is the same "no exit path" defect the round-4 sign-off
  asked to be narrowed away (carry-forward item 1); narrowing it to `!index.complete()` did not remove
  it, it aligned it with GC's blanket withholding. Minimal fix inside the existing budget: have
  `nothing_stands_at` compare the occupant's **bytes** against the fragment the repair would write
  rather than testing presence — an orphan or a stranded self-rebuild of the same `FragmentId` is
  always byte-identical (deterministic `erasure::encode`), while a genuine second claimant under a
  different scheme/len is not.

- **NEEDS-HUMAN [human] — `Reconciled::Blocked`'s public rustdoc is still contradicted, which
  round-4 sign-off item 5 required be resolved or brought back to Plan.**
  `crates/custodian/src/reconciliation.rs:25-28` defines `Blocked` as "at least one committed
  object's chunk map **could not be read** … so the reference set the loop reasoned over is
  incomplete". `crates/custodian/src/reconstruction.rs:335` now also returns it when `refused > 0`
  over a reading with no hole in it — legs 2 and 5 (`segmented_map_reconstruction.rs:394`, `:519`)
  assert `Blocked` in stores where **every** chunk map resolved cleanly. The in-code comment at
  `reconstruction.rs:340-348` acknowledges the widening ("WIDER than GC's and scrub's") but neither
  updates the contract nor carries a `// deferred: #N` marker, and the brief's 2-file budget forbids
  the third file that would fix it. A human must decide: widen the doc (third file), narrow the
  behaviour, or record a tracked deferral. *(For the record: the sibling half of that item — the
  ADR-0011 netting formula — is **not** contradicted. I checked
  `docs/design/adr/0011-…:36-40`: no refusal path increments `reconstruction_repaired`, so
  `repaired − conflict − aborted` is unchanged. The builder's claim at `reconstruction.rs:281-284`
  is warranted.)*

- **NEEDS-HUMAN [impl] — the test file asserts in prose a property no leg tests, and finding 1 shows
  the property is false.** `crates/custodian/tests/segmented_map_reconstruction.rs:11-16` and the
  leg-1 doc at `:346-351` claim leg 1 "pins that a healthy store's repair still lands over a stray,
  so the guard can never become a stall with no exit". Leg 1's store has a **complete** reading
  (`:375` asserts nothing was withheld), so `reconstruction.rs:557` never even enters the probe —
  leg 1 cannot bound a guard that is not armed. No leg drives a **second** pass after a withhold, so
  "it clears" is untested everywhere. Either add a leg that runs a follow-up pass and asserts the
  withheld repair completes, or delete the claim.

- **NEEDS-HUMAN [impl] — `emit_ambiguous` fires once per *claim*, not once per ambiguous id, and its
  own rustdoc says otherwise.** `crates/custodian/src/reconstruction.rs:818-821` emits from inside
  `CommittedIndex::note`, whose comment reads "Reported per ambiguous ID, not per claim on it", and
  `reconstruction.rs:1119-1131` (fn at `:1121`)'s rustdoc says it names "BOTH committed objects". Executed against
  the patched tree: one committed record naming a single queued `ChunkId` **three** times produces
  **two** `ambiguous-chunk-id` rows and two `reconstruction_ambiguous_chunk_id` increments for one
  id, and both rows read `"inode":"inode:42","other":"inode:42"` — the same object named as its own
  counterparty. This is rule D's failure mode (one fact reported per reference instead of per
  object). Leg 5 (`segmented_map_reconstruction.rs:528`) uses `said()`, which returns only the first
  matching row, so it cannot catch it. Fix: emit only on the Vacant→Occupied transition.

- **NEEDS-HUMAN [impl] — two new `Aborted` returns strand rebuilt fragments under a counter whose
  documented meaning excludes them.** `crates/custodian/src/reconstruction.rs:710-714` and `:722-725`
  return `RepairOutcome::Aborted`, which fires `emit_aborted` (`:1208-1221`). Both that rustdoc and
  ADR-0011's table (`docs/design/adr/0011-…:34`) define `reconstruction_aborted` as *"the
  failure-domain selector chose a server outside the fleet view, so nothing was committed"* — and the
  pre-existing abort at `:673-677` returns **before** any write, which is what makes "nothing was
  committed" true. The two new ones return **after** `put_fragment` has already landed every rebuilt
  fragment at `:689`, so an "aborted" repair now sometimes leaves stranded bytes, which the ADR
  attributes only to `conflict`. Adjust the emitter's wording / reason field (in-file, within budget)
  so an operator is not told nothing was written.

- **NEEDS-HUMAN [human] — the recorded rejection of Tier-0 DST coverage rests on a premise this
  patch's own content falsifies.** The brief's Verification posture (`brief.md:262-265`) records the
  standing finding as rejected because "this slice introduces no new destructive or concurrent path:
  … what it adds on the segmented side is a refusal, which writes nothing at all." The patch now adds
  a *decision that gates a destructive write*: `nothing_stands_at` is evaluated in the assessment
  frame (`reconstruction.rs:557-566`) and `put_fragment` lands in the repair frame
  (`reconstruction.rs:689`) — after every other assessment and after `plans.sort_by_key` at `:266` —
  so the guard carries a genuine TOCTOU window between probe and write. I am not re-litigating the
  class (the rubric settles recorded rejections); I am flagging that the *stated reason* no longer
  describes the diff, so the rejection should be re-recorded with an accurate reason or reconsidered.

## Correction to the record — the C5 red is not a signal about this patch's new logic

`check-gates.json:64` reports "42 mutants tested: 2 missed, 23 caught, 17 unviable". I reproduced it
exactly (42 / 2 / 23 / 17). **Both survivors are the same statement:**
`crates/custodian/src/reconstruction.rs:232:61` — `Assessment::Unreachable => unreachable_degraded
+= 1`, replaced with `-=` and with `*=`. That line is unchanged by this patch (it is diff *context*),
and it survives because no test in `crates/custodian/tests/` ever supplies a non-empty `unreachable`
set — every `ReconstructionContext` in `crates/custodian/tests/reconstruction.rs` passes
`unreachable: &[]`. Nothing in the new refusal / withhold / index logic survived. So the brief's
pre-declaration at `brief.md:265-266` — "a survivor here is a real signal about the compressed legs,
not noise" — is **not** borne out, and a reviewer treating C5's red as evidence against the new legs
would be wrong. It is pre-existing coverage debt this patch did not touch.

## Attempted and could not refute

- **The red→green itself** — reproduced in both directions on `339da46` (7/8 red, 8/8 green), through
  the production `reconcile_step`. Not a tautology, not a mirrored copy, not mocked away.
- **Budget conformance** — exactly 2 files; production **219** added semantic lines (cap 230); test
  **380** semantic (cap 380) and **607** raw (cap 620). No third file, no `Cargo.toml` change, no
  docs edit, `crates/custodian/tests/reconstruction.rs` untouched.
- **Probe/write slot agreement** — I checked `select_distinct_domains_excluding`
  (`crates/core/src/placement.rs:265-305`) is pure over an immutable `Topology`, so the slot `assess`
  probes really is the slot `repair_chunk` writes; the `targets[slot]` / `missing` pairing at
  `reconstruction.rs:558-559` matches `repair_chunk`'s at `:671-672`.
- **Rule A** — tried to find a false negative: for a flat record `resolve_chunk_map` answers
  `Cow::Borrowed(record)` (`crates/core/src/metadata.rs:2625-2628`), so the check is only live on the
  restart path, which is precisely where it must be; a restart onto a **value-equal** generation is
  safe by the same argument the comment gives.
- **Rule C / the CAS** — the switch from `metadata::encode(&plan.prior)` to the stored
  `plan.prior_bytes` (`reconstruction.rs:734`) is a genuine strengthening against the rubric's
  *Serialization identity* class, and the fixture's `stored()` helper
  (`segmented_map_reconstruction.rs:168-173`) seeds a non-canonical spelling that would break a
  re-encoding CAS. I could not construct a case where the raw key and the CAS key disagree.
- **Hidden claimants over a complete reading** — I tried to find one the index would miss:
  `wanted.contains(&c.id)` is applied to every `ChunkRef` of every committed object
  (`reconstruction.rs:937-938`), so any second committed reference to a *queued* id is caught.
- **Unparsable `inode:` keys as a mass trigger** — checked that no component writes a sub-namespace
  under `inode:`; `metadata::inode_key` (`crates/core/src/metadata.rs:34-36`) is the sole producer, so
  `reconstruction.rs:890-894` cannot mark a healthy deployment's reading incomplete.
- **C4's recorded flake** — ran `cargo test -p wyrd-custodian` four times on the patched tree; zero
  failures, so I could not attribute the gate's first-run failure to this crate. No finding filed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Plan must decide whether `Reconciled::Blocked` is widened to readable-but-unwritable or ambiguous work and authorize the contract update — the brief requires that outcome while the public definition still requires an unreadable chunk map (`crates/custodian/src/reconciliation.rs:25`, `crates/custodian/src/reconstruction.rs:335`).
- [ ] C5 Causal adequacy — Rebuild must tighten direct oracles — the unreadable root contains `C_UNSEEN`, while the alleged hidden `C_HELD` claimant is only unowned planted bytes (`crates/custodian/tests/segmented_map_reconstruction.rs:311`, `crates/custodian/tests/segmented_map_reconstruction.rs:431`); an actual hidden-claimant scratch variant passed, but two independently reproduced mutants also show the unreachable gauge remains unobserved (`crates/custodian/src/reconstruction.rs:232`).
- [ ] T4 Contribution — Human must decide contribution readiness on provisional automation — `scripts/review-branch --bundle` and `scripts/pdca contribcheck` were absent, so the reported six blockers and green artifact check could not be replayed; the independent affected-path check found merged predecessors, closed-unmerged PR #647, and no open overlap.
- [ ] Validation — fitness-to-purpose — Human must decide production fitness and whether to trigger the non-gating Tier-1 disk-fault and Tier-2 kill/reconstruct follow-ups — local evidence uses in-memory stores plus Tier-0 rather than a live deployment (`crates/custodian/tests/segmented_map_reconstruction.rs:37`, `AGENTS.md:78`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
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
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Same size-backstop pattern as siblings issue_695/issue_696 (4 rounds spent against threshold 2), but with sharper evidence this round that the slice is genuinely overloaded rather than merely slow to converge: the adversarial reviewer reproduced a live correctness defect, not a style nitpick — the new "would-overwrite" guard has no exit inside the exact window it is armed in (GC's blanket withholding), so a repair can stall indefinitely while silently falling off the operator-visible backlog gauge. On top of that, T4 batched review carries 6 blocking findings (the most of the three siblings): a widened `Reconciled::Blocked` contract that contradicts its own public rustdoc, a per-claim-instead-of-per-object double-count in the ambiguous-chunk audit trail, and an `Aborted` outcome now firing after bytes already landed, contradicting its own documented meaning. C5 mutants also failed this round (2 missed), though those were traced to pre-existing, untouched coverage debt rather than this patch's new logic. Ask for the re-plan: split so the resolver-once-per-pass / containment-and-refusal mechanics (Rule A/B/C, the single-namespace-read requirement) are separated from the write-time guard concern (the would-overwrite/no-exit stall, and the Aborted/Blocked contract semantics it touches) and from the per-object audit-accounting concern (ambiguous-chunk-id reporting, once per object not once per claim). Each should land as its own bounded slice with its own binding legs, so the guard's exit condition and the contract widening get designed deliberately rather than as a byproduct of a resolver-wiring patch.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
