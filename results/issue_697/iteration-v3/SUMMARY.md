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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 36 mutants tested in 60s: 18 caught, 17 unviable, 1 timeouts

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

Review of the fix that makes reconstruction resolve the committed namespace once per non-empty pass, contain unreadable objects, and refuse unsafe segmented or ambiguous repairs without draining their obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The eight behavioral legs, bounded scope, and base-visible discriminator make the requested durability decision testable; leg 8 explicitly separates unchanged fault propagation from its new attribution oracle at `crates/custodian/tests/segmented_map_reconstruction.rs:536`. |
| C2 Reproduction (red pre-fix) | PASS | With production stashed at base `339da46`, the added test compiled and 7 of 8 legs failed behaviorally while the declared empty-queue regression guard at `crates/custodian/tests/segmented_map_reconstruction.rs:524` passed. |
| C3 Change | PASS | The two-file change addresses the scoped data-loss and convergence decisions by indexing once for a non-empty queue and withholding every no-op drain after an incomplete reading at `crates/custodian/src/reconstruction.rs:162` and `crates/custodian/src/reconstruction.rs:229`. |
| C4 Verification (red→green) | PASS | A fresh reviewer scratch replay produced behavioral red on base, 8/8 green with the patch, and a complete `cargo xtask ci` pass; the fenced production entry is exercised at `crates/custodian/tests/segmented_map_reconstruction.rs:337`. |
| C5 Causal adequacy | PASS | The change removes the per-obligation namespace lookup rather than adding a capability probe or symptom guard, and an independent 36-mutant replay caught all 19 viable mutants around the causal index at `crates/custodian/src/reconstruction.rs:800`. |
| T1 Structure | PASS | The patch touches exactly the planned production file and new discriminator, with the latter documenting and driving the public fenced control point at `crates/custodian/tests/segmented_map_reconstruction.rs:1`. |
| T2 Shape | PASS | The measured shape is within every hard cap: 201 added production semantic lines, and 376 semantic/565 raw test lines; the required single metadata double begins at `crates/custodian/tests/segmented_map_reconstruction.rs:36`. |
| T3 Runtime | PASS | All declared tools were present and exercised by mutation analysis/full CI, while object-local map faults remain contained and non-map store faults remain fail-closed at `crates/custodian/src/reconstruction.rs:834`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether contribution readiness can stand without replaying the unavailable `scripts/review-branch --bundle` report — `check-gates.json` records three blockers but supplies neither wrapper nor report, although affected-path history found closed #647 and no open overlap. |
| T5 Judgment | PASS | The rebuilt discriminator now directly covers multi-obligation per-object refusal, both incomplete-reading drain routes, the duplicate's second placement, and pre-error attribution at `crates/custodian/tests/segmented_map_reconstruction.rs:355`, `crates/custodian/tests/segmented_map_reconstruction.rs:404`, `crates/custodian/tests/segmented_map_reconstruction.rs:477`, and `crates/custodian/tests/segmented_map_reconstruction.rs:564`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-memory proof is sufficient for production durability sign-off or whether to run `cargo xtask disk-faults` and `cargo xtask kill-reconstruct` — the fixture uses `MemMeta`/`MemDServer` doubles at `crates/custodian/tests/segmented_map_reconstruction.rs:36` and `crates/custodian/tests/segmented_map_reconstruction.rs:84`, so no live backend topology was exercised. |

### Advisory — adversary

# Adversarial review — issue #697 (advisory; never gates)

Evidence re-run in both directions. **Green**: `cargo test -p wyrd-custodian --test
segmented_map_reconstruction` at `$PDCA_TARGET` → 8/8 pass. **Red**: the C4-verify log records 7
behavioural failures on `339da46` (leg 4 `Satisfied != Blocked`, leg 6 `3 != 1` namespace scans, leg
2/3 on `SegmentedMapUnsupported`), leg 7 green both sides exactly as the brief declares. Every leg
drives `reconcile_step`, the real fenced control point — no parallel re-implementation. Findings
below were reproduced in a throwaway clone (since removed) driving the same production entry point.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:891`: the refusal row's
  `obligations` field counts *references*, not obligations, so a duplicate id inside one segmented
  object over-reports it.** `hits` (`:870-876`) enumerates *positions in `resolved.chunks`*, and
  `emit_refused(&inode_key, REFUSED_SEGMENTED, hits.len())` publishes that as the obligation count.
  Concrete failing case, reproduced: one committed segmented object `inode:90` whose two `seg:`
  records both reference chunk `S_A` — the duplicate-id anomaly the brief's rule 4 puts explicitly
  in scope ("the rule is the same whether the duplicates sit in one record or two") — with `S_A`
  queued **once**. The pass emits
  `"action":"refused","inode":"inode:90","reason":"segmented-chunk-map","obligations":2` while
  exactly **one** obligation exists and one is kept (`queued=[0xB1]`, outcome `Blocked`). That
  contradicts the field's own stated contract at `:1069-1071` ("the obligation count is a field on
  it … a counter that ticked per chunk would measure the queue rather than the store"). The
  discriminator cannot see it: leg 2 (`crates/custodian/tests/segmented_map_reconstruction.rs:381`)
  uses two *distinct* queued chunks, where `hits.len()` and the obligation count coincide — and the
  oracle there is a whole-log substring search (`logged(&audit).contains(BOTH)`, also `:416`), not
  an assertion tied to the refusal row, so it would stay green with the count on the wrong row.
  Fix: count distinct chunk ids in `hits` (leg 2 stays green under that change).

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:686`: one object still repairs at
  most ONE obligation per pass, and the other q−1 are reported as lost CAS races that never
  happened.** Every plan built from the same object shares one `prior_bytes` handle (`:121`,
  `:527-528`), so after the first commit rewrites the record, the second
  `.require(inode_key, plan.prior_bytes.to_vec())` at `:686` can never match. Reproduced: a committed
  flat object holding two queued, under-replicated chunks → outcome `Changed`, chunk 0 repointed to
  `[0,1,2]`, chunk 1 left at `[0,1,3]` with its obligation still queued, and one
  `monotonic_counter.reconstruction_conflict` increment — the "raced another writer / superseded
  custodian" signal — with no racing writer in the store. **Verified identical on base `339da46`**
  (`Changed`, one repair, one conflict), so I am *not* claiming a regression. It is filed because
  the patch newly documents `prior_bytes` as "shared by every obligation inside the object"
  (`:121`) — which reads as a served case — and because the brief's C-1 convergence claim ("its work
  is bounded by the obligations it holds") is closed only on the *reading* axis: a multipart object
  that lost a failure domain still needs one pass per chunk and mis-attributes q−1 self-inflicted
  conflicts per pass, on the same surface #682 opens. Human call: accept as out of scope / track, or
  correct the comment here.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:408-411`: leg 3's
  rule-E *ordering* oracle cannot fail for the property it names.** `repair` is the index of the
  first `"action":"repair"` row, i.e. `emit_repaired`, which `reconcile` emits in the metrics block
  at `crates/custodian/src/reconstruction.rs:302-304` — before the repair loop at `:316-322` and
  unconditionally. So `said(&audit, &named(key)) < repair` only proves the names precede a *metric*;
  an implementation that named objects anywhere inside `assess`'s per-obligation loop (violating rule
  E's "where the object is read, before the work loop", and rule D with it) would still pass. The
  line earned its base-red from the *absence* of the name, not from the ordering. A binding oracle
  would compare against the first `commit` the `MemMeta` double observed (it already sees every
  commit at `:70`). Narrow: leg 8 (`:456`) carries the binding "the pass ended and the name
  survived" half.

## Attempted and could not refute

- **C5's red is a timeout, not a survivor — do not spend a finding on it.** I re-ran the exact
  reported mutant (`!=` → `==` at `crates/custodian/src/reconstruction.rs:864`, the rule A guard) in
  a scratch clone: it is caught loudly — 14 of the 15 tests in the untouched
  `crates/custodian/tests/reconstruction.rs` fail, and leg 4 fails on `said(&audit, INCOMPLETE)`
  (with the guard bypassed the scanned segmented record takes the refusal branch, so no
  `withheld` row is emitted). `check-gates.json`'s `C5-mutants` `fail` row (`1 timeouts`, `0 missed`)
  is a host-load artifact under a 20 s auto-timeout, not a coverage hole.
- **Rule A firing spuriously on flat records / livelocking a hot object**: refuted —
  `resolve_snapshot` short-circuits `ChunkMap::Flat` to `Cow::Borrowed(record)`
  (`crates/core/src/metadata.rs:2585`), so `:864` can only ever fire on a segmented root; a
  frequently-rewritten flat object is untouched by the containment.
- **A drain escaping the incomplete-reading gate through the *second* `Drain` route** (`assess`'s
  `missing.is_empty()` at `crates/custodian/src/reconstruction.rs:496-498`, the iteration-2
  carry-forward): refuted — the gate is at the `reconcile` match arm (`:229-230`), so both routes are
  withheld, and leg 3 queues `C_IDLE`, a fully-healthy chunk in a *readable* flat record, asserting
  `queued == [C_IDLE, C_UNSEEN]`.
- **`Ok(None)` from the resolver silently draining an object that is only mid-overwrite**
  (`:833`): refuted — I built a segmented generation whose live root is `Pending`; the resolver
  answers a typed `ChunkMapError` ("seg:… is absent while the root still names this generation"), so
  the object is contained (`unresolvable-chunk-map`, `Blocked`, `obligations:1` withheld), not
  drained.
- **Rule C being non-discriminating**: refuted — re-deriving `metadata::inode_key(7)` from the key
  `inode:007` would make the `require` at `:686` conflict, so leg 3's
  `placement(F_WORK, 0) == [0,1,2]` goes red.
- **An unparsable `inode:` key silently skipped** (the base's behaviour): refuted — `inode:zz`
  beside healthy work yields `"action":"unparsable-inode-key"`, `Blocked`, and the healthy repair
  still lands. Fail-closed as rule C asks; no production writer produces such a key
  (`crates/core/src/metadata.rs:35` is the sole `inode:` constructor).
- **Rule B's orphan-marking of a fragment a hidden object still references** is
  recorded-rejected in `results/issue_681/review-rejected.md` and settled per the target rubric's
  *Deferrals are settled* protocol — not re-raised.
- **Budget** (informational): 201 production / 379 test semantic added lines against the brief's
  230 / 380 caps, 565 raw against 620, exactly 2 files; `crates/custodian/tests/reconstruction.rs`
  unmodified. The test file sits **one** semantic line under its cap, which constrains any follow-up
  assertion.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether contribution readiness can stand without replaying the unavailable `scripts/review-branch --bundle` report — `check-gates.json` records three blockers but supplies neither wrapper nor report, although affected-path history found closed #647 and no open overlap.
- [ ] Validation — fitness-to-purpose — Decide whether the in-memory proof is sufficient for production durability sign-off or whether to run `cargo xtask disk-faults` and `cargo xtask kill-reconstruct` — the fixture uses `MemMeta`/`MemDServer` doubles at `crates/custodian/tests/segmented_map_reconstruction.rs:36` and `crates/custodian/tests/segmented_map_reconstruction.rs:84`, so no live backend topology was exercised.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): T4 blocking findings (review-batch.md), 3 items reduce to 2 distinct issues: 1. Primary, real BUG (reconstruction.rs:316 and :313, same root cause): when the namespace reading is incomplete (some committed object could not be read this pass), repair can still proceed and overwrite a fragment via `put_fragment` for a chunk ID that a DIFFERENT, unreadable object also references — before any CAS or GC incomplete-reference-set protection applies, since those safety nets only guard against conflicts the pass already knows about, not a duplicate hidden inside the object it never got to read. Rebuild must withhold repairs (not just drains) for a chunk ID whenever there's any object this pass could not read, until it is known no unreadable object shares that chunk ID — or otherwise close the overwrite path before landing the fragment. 2. `reconstruction.rs:864` (TEST-GAP, Tier-0 DST for the new concurrent generation-restart path): same recurring question already settled at Plan for the sibling children with matching Verification-posture reasoning ("no seeded Tier-0 DST case ships in this child... belongs to #682"). Rebuild should record-reject this with the same brief-pinned reasoning rather than adding DST coverage. Also noted from the advisory adversary review (non-gating, but worth the rebuild's attention alongside item 1 since it's in the same area): - Obligations-count field at a segmented refusal counts references, not distinct chunk ids, over-reporting on a duplicate-id object (reconstruction.rs:891) — minor/cosmetic on the audit trail, fix opportunistically if touching this code. - Multi-obligation objects only complete 1 repair per pass and mis-attribute the rest as "conflict" with no racing writer (reconstruction.rs:686) — confirmed identical on base, NOT a regression from this patch, and the reviewer left it as a human call (track/out-of-scope, or just correct the misleading comment). Not required for this rebuild. Context: across the two prior rounds, findings did not converge downward (4 -> 2 -> 3 blocking; impl-finding ledger 3 -> 4 -> 2) — each round fixed what was reported and a new finding surfaced in the same incomplete-reading/data-safety seam. This round's finding (item 1) is a genuine escalation in that same seam (a potential silent overwrite, not just a certification/accounting gap), so the rebuild should treat it as the priority and watch specifically for another finding re-emerging in this same area next round; if it does, reconsider iterate-plan instead of a further iterate-do.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
