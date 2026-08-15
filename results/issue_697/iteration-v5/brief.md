# custodian: reconstruction reads through the resolver once per pass, contained (635.4b.3)

> Child 3 of 3 from the #681 split (2026-08-06). Siblings — **backfill** and **rebalance** — are
> independent: disjoint files, same wave, no ordering between them. **#682 depends on all three.**
> Parent context, the measured history and the full rule derivations:
> `results/issue_681/brief.md`.

- **Slug:** reconstruction-reads-through-resolver-once-contained
- **Defect:** `crates/custodian/src/reconstruction.rs` reads the chunk map inline out of the inode
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
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_reconstruction.rs`
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, no DST leg. Verified at Plan: `--print-base` → `origin/main`;
  `main == origin/main == 339da46`; the `--classify` dry-run on a synthetic patch listing exactly
  `src/reconstruction.rs` + the new test returns `ADDED_TEST` + `CRATE crates/custodian`, so the
  green leg is `cargo test -p wyrd-custodian --test segmented_map_reconstruction` and the red leg
  reverts `reconstruction.rs` while keeping the test (`run-verify.sh:97-98`, `:454`). No
  `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped on the base). **Legs
  1–6 go red on the base; legs 7 and 8 are declared non-red above** — checked at Plan against the
  base's own control flow, not assumed from the leg's wording. Independent corroboration: the v2 and
  v7 attempts each recorded their base-visible legs compiling and failing on **behavioural**
  assertions at `339da46`, then passing with the fix — the red is demonstrated, not predicted.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`), over **the maintenance pass that restores redundancy**:
  - it reads every committed object the way every other consumer reads it;
  - **an obligation is discharged or kept; it is never discarded for want of a reading** — "I could
    not read the map" and "no committed map references this chunk" are different facts, and only
    the second permits draining;
  - containment is per object and the answer still gets made;
  - it never claims more than it read, and never writes to a generation it did not read;
  - **its work is bounded by the obligations it holds, not by their product with the namespace** —
    a repair loop costing Q×N resolves stops converging as a store grows, the permanence C-1
    forbids, reached through the scheduler.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with its two siblings.** Touches
  `crates/custodian/src/reconstruction.rs` and one new test file; neither sibling touches either.
  Every code prerequisite is already merged on the base (#649's `resolve_chunk_map`, #650's
  `Reconciled::Blocked` and the GC containment precedent, #651's restore precedent). **#682 depends
  on this child** and must land after it.
- **Surfaces:** data
- **Difficulty:** high   (one production file, but three call-sites plus a restructure of the
  assessment loop from per-obligation namespace scans to one reading, and a change to what the
  pass's `Reconciled` answer may claim — which `reconcile_step`'s `least_certified` fold reads. A
  diff-reviewer must hold the loop, the resolver's typed-error contract, the containment/downcast
  rule, every existing classification and its gauge accounting, and the complexity property in view
  at once. Rated up deliberately: this is the largest of the three children.)
- **Scope:** reconstruction **reads each committed object's map once per pass, assesses its queued
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
- **Budget:** **exactly 2 files**, `src/reconstruction.rs` ≤ **230** added semantic lines
  (non-blank, non-comment) and `tests/segmented_map_reconstruction.rs` ≤ **380 semantic / 620 raw**.
  v7 measured 180 semantic for this file's production hunks, so the cap is that plus rules A and C.
  A **third** file, or a test file past 620 raw, means the shape is wrong: STOP and hand back rather
  than finish. Compression rules: ONE `BTreeMap`-backed metadata double carrying the scan/`seg:`
  counters legs 6 and 7 read and the injected `get` fault leg 8 needs — not three store types; ONE
  parameterised seeding helper planting healthy-flat / under-replicated-flat / healthy-segmented /
  damaged / duplicate-id objects; ONE audit-capture helper shared by legs 3, 5 and 8. Scale
  reference: the sibling #651 discriminator `tests/segmented_map_restore.rs` is 731 lines in total
  and covers more.
- **Repro instruction:** on the target checkout,
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` at `:332`, `:583`,
  `:636`. Seeding **any** `seg:`-backed committed root — or any committed record that will not
  decode — makes `reconstruction::reconcile` return `Err` for the whole store; enqueueing Q ≥ 2
  repairs over N ≥ 2 committed objects shows Q separate `scan(b"inode:")` calls. The seeding shape
  to copy is `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`; the
  fence shape is `segmented_map_consumers.rs:406-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, all OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_reconstruction.rs` — a **NEW** file, not optional and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97-98`); appending to an existing file makes it *modified*, the gate takes the green-only branch and proves no red at all. Confirmed by the `--classify` dry-run at Plan.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Pre-declared: **no seeded Tier-0 DST case ships in this child**, and the standing review
  finding asking for one is settled at Plan as recorded-rejected with this reason — *this slice
  introduces no new destructive or concurrent path: every write it performs is on a flat object read
  from the generation it scanned (Rule A, bound by leg 4) and keeps its existing version-conditional
  CAS, and what it adds on the segmented side is a refusal, which writes nothing at all. The seeded
  Tier-0 case for the segmented write path belongs to #682, which builds it.* Note this reason is
  now **stronger than in v7**: rule A makes the CAS framing a tested property rather than an
  asserted one. The advisory `C5-mutants` row covers the diff; v7 recorded 0 survivors, so a
  survivor here is a real signal about the compressed legs, not noise.
- **Citations expected:** cite `path:line` on the target branch for every change.
  **Salvage first:** `results/issue_681/iteration-v7/patch.diff` contains this file's production
  hunks (180 semantic lines) which passed C1–C5, C4-verify red→green and mutation analysis (82
  mutants, 0 survivors across the whole patch), and whose red→green an adversary independently
  reproduced in both directions — 6 legs failing behaviourally on base `339da46` and passing with
  the patch, driving production entry points. Take them and apply rules A and C; do not re-derive
  the correctness core.
  **Peer callsites Do MAY open — this is a composition slice; mirror rather than invent:**
  `crates/custodian/src/gc.rs:360-455` (the canonical walk: decode failure contained per object via
  `unresolvable.insert(key, fault); continue`, resolve via `metadata::resolve_chunk_map`, `Ok(None)`
  skipped as "no live committed generation", and the downcast rule at `:402-416` — contain by
  exactly this rule and no other); `crates/custodian/src/gc.rs:164-166` + `:470-480` (attribution
  emitted by the consumer, per object, before the work loop; `object_name`'s injective escaping —
  mirror the placement, not just the call); `crates/custodian/src/gc.rs:306-316` + `:191-194`
  (`ReferenceSet::protection` — the backstop rule B rests on; read it before proposing to widen
  containment); `crates/custodian/src/restore.rs:616-688` (the same shape applied a second time by
  #651); `crates/custodian/src/reconciliation.rs:44` + `:55-61` (`Reconciled::Blocked` and
  `least_certified` — reuse this vocabulary, do not invent a parallel outcome);
  `crates/core/src/metadata.rs:2256-2272` + `:2619-2632` (`ResolvedChunkMap` — why `record` rides
  along — and `resolve_chunk_map`'s three arms; **rule A lives here**);
  `crates/custodian/src/reconstruction.rs:184-232` (the existing per-obligation assessment loop and
  its gauge accounting — every classification and the off-gauge rule survive the rework);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` and
  `crates/custodian/tests/segmented_map_consumers.rs:78-133` + `:406-410` (the `BTreeMap`-backed
  `MemMeta` whose ordering makes "the damaged record is met FIRST" a fixture property rather than
  luck, its `scan_page` delegating to `wyrd_testkit::test_double_scan_page` at `:109-116`, and the
  fence shape).
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/reconstruction.rs` → the nearest are `3e05891` (#648 — the segmented record
  shape, which **created** these three sites) and `5f2f79f` (assess's classification order). No open
  PR touches this file. **#647 is closed with the Q×N finding still open** — defect 2 above, and
  this child closes it. Seven prior attempts on the un-split #681 are archived in
  `results/issue_681/iteration-v1..v7/`; four review findings on this file were
  **recorded-rejected** with reasons that still stand and must not be re-litigated — see
  `results/issue_681/review-rejected.md` (`reconstruction.rs:383`/`:210`/`:302`/`:387`,
  "orphan-mark a fragment still referenced by the hidden object"), which is rule B above.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Whether refusal accounting is truly per object must be re-established with a multi-obligation fixture — the current one-obligation refusal leg at `crates/custodian/tests/segmented_map_reconstruction.rs:434` cannot expose the Rule D violation.; T5 Judgment — Restore direct oracles before rebuild sign-off — leg 1's queue-drain assertion is commented out at `crates/custodian/tests/segmented_map_reconstruction.rs:427`, and leg 5 checks only index 0 rather than the duplicate second placement at `crates/custodian/tests/segmented_map_reconstruction.rs:571`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must withhold every no-op drain after an incomplete reading — a readable flat site can still return `Drain` at `crates/custodian/src/reconstruction.rs:501` and be deleted at `crates/custodian/src/reconstruction.rs:322` even though `unaccounted != 0` selects withholding only for absent sites at `crates/custodian/src/reconstruction.rs:409`, so an unreadable record can hide the reference the obligation protects.; T4 Contribution — Whether to accept contribution readiness without an independent contribution-artifact replay is owed — the `scripts/pdca contribcheck` wrapper and the artifacts it validates were not supplied, although affected-path history found merged predecessors plus closed-unmerged #647 and no open overlap.; T5 Judgment — Rebuild must restore Rule E's direct oracle and behavior — the non-`ChunkMapError` branch returns before naming the object at `crates/custodian/src/reconstruction.rs:837`, while leg 8 discards its capture and checks only error/state at `crates/custodian/tests/segmented_map_reconstruction.rs:615`, leaving the required pre-error operator attribution absent and untested.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 34 mutants tested in 64s: 1 missed, 15 caught, 17 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 blocking findings (review-batch.md), 3 items reduce to 2 distinct issues: 1. Primary, real BUG (reconstruction.rs:316 and :313, same root cause): when the namespace reading is incomplete (some committed object could not be read this pass), repair can still proceed and overwrite a fragment via `put_fragment` for a chunk ID that a DIFFERENT, unreadable object also references — before any CAS or GC incomplete-reference-set protection applies, since those safety nets only guard against conflicts the pass already knows about, not a duplicate hidden inside the object it never got to read. Rebuild must withhold repairs (not just drains) for a chunk ID whenever there's any object this pass could not read, until it is known no unreadable object shares that chunk ID — or otherwise close the overwrite path before landing the fragment. 2. `reconstruction.rs:864` (TEST-GAP, Tier-0 DST for the new concurrent generation-restart path): same recurring question already settled at Plan for the sibling children with matching Verification-posture reasoning ("no seeded Tier-0 DST case ships in this child... belongs to #682"). Rebuild should record-reject this with the same brief-pinned reasoning rather than adding DST coverage. Also noted from the advisory adversary review (non-gating, but worth the rebuild's attention alongside item 1 since it's in the same area): - Obligations-count field at a segmented refusal counts references, not distinct chunk ids, over-reporting on a duplicate-id object (reconstruction.rs:891) — minor/cosmetic on the audit trail, fix opportunistically if touching this code. - Multi-obligation objects only complete 1 repair per pass and mis-attribute the rest as "conflict" with no racing writer (reconstruction.rs:686) — confirmed identical on base, NOT a regression from this patch, and the reviewer left it as a human call (track/out-of-scope, or just correct the misleading comment). Not required for this rebuild. Context: across the two prior rounds, findings did not converge downward (4 -> 2 -> 3 blocking; impl-finding ledger 3 -> 4 -> 2) — each round fixed what was reported and a new finding surfaced in the same incomplete-reading/data-safety seam. This round's finding (item 1) is a genuine escalation in that same seam (a potential silent overwrite, not just a certification/accounting gap), so the rebuild should treat it as the priority and watch specifically for another finding re-emerging in this same area next round; if it does, reconsider iterate-plan instead of a further iterate-do.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 36 mutants tested in 60s: 18 caught, 17 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrides the size-backstop's iterate-plan recommendation, on the basis that the outstanding items are concrete, demonstrated implementation bugs in the leg-9 (`may_land`) guard the builder added beyond the brief's 8-leg scope. Next round must: 1. Fix the permanent-stall bug at crates/custodian/src/reconstruction.rs:671 — `may_land` is applied unconditionally on every claimed slot instead of only when the pass's own reading is incomplete. Per the adversary's one-line scope fix: only consult `may_land` when `index`/the reading is incomplete; otherwise a completable repair the base would finish can block forever with no exit path (demonstrated reproduction in check-review.md's adversary section). 2. Fix the gauge-floor bug at reconstruction.rs:215/:293 — a `would-overwrite` refusal must not permanently floor `gauge.reconstruction_under_replicated`; the brief explicitly requires a never-repaired condition to stay off the repairable-backlog gauge (as `Assessment::Blocked`/`Assessment::Refused` already do). 3. Add the missing `inode` field to the `would-overwrite` NEEDS-HUMAN audit row (reconstruction.rs:1154-1164) — the pass has `plan.inode_key` in scope but the emitted row omits it, unlike every other new emitter. 4. Rewrite or replace the leg-9 test fixture (segmented_map_reconstruction.rs:579-613) — it does not construct the hidden-duplicate hazard it claims to test (the reading is complete in its own fixture), so it currently just locks in the over-broad guard behavior. Once (1) is fixed, this leg must actually assert the guard fires only under an incomplete reading, or be dropped if it can't be made to. 5. Resolve the two contract contradictions: the ADR's counter/netting formula (docs/design/adr/0011) and `Reconciled::Blocked`'s rustdoc (`reconciliation.rs:22-44`, "chunk map could not be read" / incomplete reference set) are both now inconsistent with `Blocked` being returned over a complete reading whenever `refused > 0`. Since the brief forbids editing the ADR or reconciliation.rs's doc, either narrow the new behavior so it doesn't contradict them, or bring this explicitly back to Plan if it can't be reconciled within the existing 2-file/8-leg budget — do not ship a rebuild that leaves the written contracts contradicted. 6. Revisit whether leg 9 belongs in this slice at all, now that its guard is being narrowed to only the incomplete-reading case in (1) — it may fold naturally into the existing 8 legs (e.g. as part of leg 3/4) rather than remaining a declared 9th leg over the brief's cap; prefer bringing the test file back within budget over carrying an explicit overage into the next round. T4-Contribution and Validation fitness-to-purpose remain open human-judgment items for the next sign-off, not implementation work for this round. Do not treat this as license to keep growing the slice: if a 4th round still can't land these within the existing scope (or close to it), revisit iterate-plan — the demonstrated hazard (hidden duplicate claimant behind an unreadable object) may turn out to need its own properly-scoped brief rather than a bolt-on guard.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Same size-backstop pattern as siblings issue_695/issue_696 (4 rounds spent against threshold 2), but with sharper evidence this round that the slice is genuinely overloaded rather than merely slow to converge: the adversarial reviewer reproduced a live correctness defect, not a style nitpick — the new "would-overwrite" guard has no exit inside the exact window it is armed in (GC's blanket withholding), so a repair can stall indefinitely while silently falling off the operator-visible backlog gauge. On top of that, T4 batched review carries 6 blocking findings (the most of the three siblings): a widened `Reconciled::Blocked` contract that contradicts its own public rustdoc, a per-claim-instead-of-per-object double-count in the ambiguous-chunk audit trail, and an `Aborted` outcome now firing after bytes already landed, contradicting its own documented meaning. C5 mutants also failed this round (2 missed), though those were traced to pre-existing, untouched coverage debt rather than this patch's new logic. Ask for the re-plan: split so the resolver-once-per-pass / containment-and-refusal mechanics (Rule A/B/C, the single-namespace-read requirement) are separated from the write-time guard concern (the would-overwrite/no-exit stall, and the Aborted/Blocked contract semantics it touches) and from the per-object audit-accounting concern (ambiguous-chunk-id reporting, once per object not once per claim). Each should land as its own bounded slice with its own binding legs, so the guard's exit condition and the contract widening get designed deliberately rather than as a byproduct of a resolver-wiring patch.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 42 mutants tested in 49s: 2 missed, 23 caught, 17 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
