# Brief — issue 681 / passes-read-through-resolver-contained

> **Plan RESTART (2026-08-06, second) — this brief exists to be SPLIT.**
> #681 was returned from Check after **4 sign-off rounds, 7 builder attempts, 3 auto-iterates,
> 1 prior replan, ~102 KB patch**. The sign-off decision (`iteration-v7/SUMMARY.md` §9) was
> explicit: *"The slicing is the problem, not the implementation … Re-split at Plan rather than a
> fifth Do round."* The `iterate-plan` recommendation had already been overridden at v5 and v6 on
> the reasoning that the findings were implementation-shaped; that experiment ran twice and did
> not converge.
>
> Everything below is the **corrected** statement of the problem. Two things changed at this
> re-plan and both were verified against `origin/main @ 339da46`, not carried over:
> **(1)** the old decision 4 ("unreachable by construction, bound by no test") is **false** and is
> replaced by **rule A** below; **(2)** the split axis is **by pass**, not by property.
>
> Siblings: **#649/#650** (shared resolver + GC/scrub, merged), **#651** (4a, restore + drain
> report, merged `8decc93`), **#652** (merged `b083ec4`), **#682** (4c — `repoint_chunk` + record
> ceilings; **its `Depends on: 681` must be repointed at this brief's children**).
> Related but NOT part of this work: **#694** (M8 operator surface for an unreadable record).
> Tracker: https://github.com/getwyrd/wyrd/issues/681

- **Slug:** passes-read-through-resolver-contained
- **Defect:** The three maintenance passes that walk the committed namespace **themselves** —
  reconstruction, backfill, rebalance — still read the chunk map inline out of the inode record,
  so a **single** segmented object aborts the whole pass for **every** object; and an object whose
  map cannot be read for any other reason ends the walk with an `Err` instead of being contained.
  Seven sites, each re-verified on `origin/main @ 339da46` at this Plan, and they partition
  **exactly by file** — which is what makes the by-pass split clean:

  | Site | Function | Pass |
  |---|---|---|
  | `crates/custodian/src/backfill.rs:99` | `reconcile` | backfill |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` | backfill |
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` | rebalance |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` | rebalance |
  | `crates/custodian/src/reconstruction.rs:332` | `assess` | reconstruction |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | reconstruction |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` | reconstruction |

  Each is `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`.
  Four live consequences on the base:
  1. **One segmented object disables repair, backfill and drain for the entire store** — these are
     the passes that *restore redundancy*, so a store that has published a single multipart object
     stops self-healing.
  2. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted** (`reconstruction.rs:322-325` → `Assessment::Drain`, deleted at `:270-276`). Latent
     today (the error fires first), but it is the loss this work must not introduce while removing
     the abort.
  3. **The deployed repair loop is Q namespace scans × N point reads** — `reconcile` calls `assess`
     per obligation (`:185`), `assess` calls `find_chunk` (`:322`) which scans all of `inode:`.
     Wiring the resolver in naively makes each of those N objects also cost a bounded `seg:` range
     read. This is the finding still open at #647's close.
  4. **Containment is not per object.** #650 and #651 contain a damaged record in GC/scrub
     (`gc.rs:360-455`) and restore/drain (`restore.rs:616-688`); these three passes have no such
     rule — a record that will not `decode` ends the walk before any resolver is involved.
- **Success criterion:** **this bundle ships no code.** It is satisfied when `split-proposal.md`
  has been accepted, three child issues exist as sub-issues of #681, and each child bundle carries
  its own `brief.md`. The shippable criteria live in the children.
- **Falsifiability:** n/a for the parent (no patch). Each child's criterion is an **assertion**
  red on base-visible symbols over in-memory trait doubles on a plain Linux workspace — no
  topology, no cfg gate, no Docker, no new dev-dependency. Verified at this Plan:
  `PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh --print-base` → `origin/main`;
  `main == origin/main == 339da46`. Each child names a **NEW** `crates/custodian/tests/*.rs` file,
  because C4-verify earns its red only from an added test file (`run-verify.sh:97-98`).
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`), stated over **the maintenance passes that restore redundancy and
  execute a drain**:
  - **A pass reads every committed object the way every other consumer reads it.** Redundancy
    restoration is not a service a store may lose by publishing one large object.
  - **An obligation is discharged or kept; it is never discarded for want of a reading.** "I could
    not read the map" and "no committed map references this chunk" are different facts.
  - **Containment is per object, and the answer still gets made.** One damaged record may not cost
    every healthy object its repair, fill or evacuation; `Err` for the whole pass is as wrong an
    answer as `Satisfied`. A fault that is *not* one object's still ends the pass.
  - **A pass never claims more than it read.**
  - **A pass never writes to a generation it did not read.**
  - **Work is bounded by the obligations held, not by their product with the namespace.**
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: Wyrd has no maintenance
  branches; M4's integration branch is merged and deleted; #648–#652 all landed on `main` directly.)
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** The three children touch **disjoint** production files and **disjoint** new
  test files, so they carry no `Depends on` and no `Conflicts with` between them: **one parallel
  wave**. **#682 depends on all three** and must not be driven before they land.
- **Surfaces:** data
- **Difficulty:** high   (for the parent, as the whole; each child is `medium`.)
- **Scope:** **split this bundle into three children, one per pass**, each carrying the complete
  rule set for its own pass and its own new test file:

  1. **backfill** — `crates/custodian/src/backfill.rs` (2 sites),
     `crates/custodian/tests/segmented_map_backfill.rs` (new).
  2. **rebalance** — `crates/custodian/src/rebalance.rs` (2 sites),
     `crates/custodian/tests/segmented_map_rebalance.rs` (new).
  3. **reconstruction** — `crates/custodian/src/reconstruction.rs` (3 sites),
     `crates/custodian/tests/segmented_map_reconstruction.rs` (new). Keeps the Q×N→O(N)
     restructure, the duplicate-`ChunkId` rule, and the drain-vs-refuse loss rule.

  **Why by pass and not by property** (the sign-off's hint was by property — this is the
  considered departure, agreed with the human at this Plan): a by-property cut has every child
  editing all three files, so the children serialise into three waves *and* conflict; and
  properties 1 and 2 are not separable — the moment a child calls `resolve_chunk_map` it must
  already decide what to do with a typed read fault, so the second child would rewrite the first's
  error handling. By pass, the children are disjoint, parallel, and each is independently
  reviewable. The measured v7 patch supports this: production was **380 semantic lines across
  three independent files with no shared module** (`backfill` 100, `rebalance` 100,
  `reconstruction` 180), while the single shared test file was **500 semantic lines — 68% of the
  patch** — precisely because every leg had to drive all three passes over one store. Splitting by
  pass removes that pressure entirely. Fixture duplication across the children is **the house
  pattern, not a new cost**: `git grep "struct MemMeta" -- crates/custodian/tests/` finds **twelve**
  independent definitions today.

  **Rules every child carries (settled here; no Do round and no review round relitigates them).**

  **Rule A — a pass acts only on a resolve that did not restart.** *(This REPLACES the old
  decision 4, which claimed the path was "unreachable by construction" and forbade any test from
  binding it. That claim is false and was demonstrated false with a working double at
  `iteration-v4/check-advisory-adversary.md:25`; because nothing bound it, four consecutive review
  rounds rediscovered it, and round 7's two T4 blockers — `rebalance.rs:412`,
  `reconstruction.rs:659` — are the same fact in a third form.)* `resolve_chunk_map` restarts onto
  the live root when the caller's **segmented** snapshot was superseded (`metadata.rs:2338-2339`),
  and then answers a `ResolvedChunkMap` whose `record` is a generation the pass never scanned.
  Mixing that generation's chunk list with the snapshot's `prior_bytes` is what makes unchecked
  indexing panic before the stale CAS can reject the plan. Therefore: **if the resolve did not
  answer from the generation the pass scanned** (detectable as `Cow::Owned`, or
  `resolved.record.version != snapshot.version`), the object changed under the scan — contain it,
  **keep the obligation queued**, answer `Blocked`, re-assess next pass. Bounded per C-1: nothing
  is discarded, the next pass re-reads. **Every child MUST bind this with a test leg** (a metadata
  double whose `scan` answers a stale segmented root while `get` answers a live flat one; the pass
  must write nothing). A rule bound by no test is a rule the next reviewer re-opens.

  **Rule B — an incomplete reading changes what a pass may CLAIM and what it may DISCARD, never
  what it may DO for the objects it read successfully.** A pass that met an unreadable object
  answers `Blocked` (never `Satisfied`) and never drains an obligation, but still repairs, fills
  and evacuates the healthy objects in the same store. Verified safe at this Plan rather than
  argued: these passes **orphan-mark, never delete**, and GC reclaims a marked fragment only past
  `ReferenceSet::protection`, which returns `incomplete-reference-set` and withholds **every**
  fragment in the fleet while any object is unresolvable (`gc.rs:306-316`, consulted before every
  delete at `:191-194`). So the loss chain cannot close and strictness buys **no** safety — while
  costing every healthy object its repair, which is the C-1 violation this work exists to remove.
  Weight for the next reader: a genuinely corrupt root has **no repair path at all** (a fragment
  carries only `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`, so chunks cannot be
  walked back to their object) and no operator tooling (**#694**), so "until a human fixes it" is
  an unbounded window — strictness would halt fleet-wide redundancy repair with no supported way
  to un-halt it.

  **Rule C — a record is read, written and named under exactly the key the store gave it.** A key
  that is not the canonical spelling of its id is never silently reinterpreted. On the base all
  three passes parse the scanned key to an `InodeId` then re-derive `metadata::inode_key(id)` for
  the CAS (`backfill.rs:142`, `rebalance.rs:310`, `reconstruction.rs:598`); `"inode:007"` and
  `"inode:+3"` both parse, so the pass reads one record and CASes another. `gc.rs:280-294`, `:402`
  is the precedent — resolve against `&key`, key attribution by the raw bytes.

  **Rule D — a refusal is reported once per object, not once per chunk.**

  **Rule E — attribution for an object the pass could not read is emitted where the object is
  read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
  cannot cost the operator the name of the record to repair. **Load-bearing, not logging
  hygiene:** with no repair tooling (#694) and reclamation halted store-wide, that name is the
  operator's entire situational awareness — it is what distinguishes a rolling-upgrade artefact
  that will clear itself from permanent loss. Do not let a child optimise it away.

  **Constraints (they bound the shape; they do not name it):**
  - **Bounded memory** — work proportional to the obligations held and to one object at a time;
    never the whole namespace's decoded chunk lists, never any segment's exact bytes.
  - **Bounded work** — one resolving reading of the namespace per pass.
  - **Containment on *any* read fault**, by exactly gc.rs's downcast rule: `Ok(ChunkMapError)` is
    contained as *this record's* fault; any other error propagates, because a store fault is not
    one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings, and the
    repair/evacuation write path for a `seg:`-resident chunk are **#682**. A refusal writes
    **nothing at all**.
  - Restore / `desired_state` (**#651**, merged) — `restore.rs`, `desired_state.rs` untouched;
    **leave** the deferral marker at `restore.rs:616`.
  - `gc.rs` / `scrub.rs` (**#650**, merged) — untouched. Sharing ONE namespace walk across all
    loops is a separate refactor.
  - The chunk-id floor (**#652**); the committer, fence, rollback and resume (**#653**).
  - **The operator surface for an unreadable record — #694, milestone M8.** No child touches it.
  - **No docs edit** (checked: `06-runtime-view.md` §6.2 already states the containment rule
    fleet-wide and stays true); no new or edited ADR / spec / proposal; no conformance-vector
    change; no `Cargo.toml` change (every dev-dependency the discriminators need is already
    declared on `crates/custodian`).
- **Repro instruction:** on the target checkout, read the seven sites with
  `git -C ../wyrd show origin/main:crates/custodian/src/backfill.rs` (and `rebalance.rs`,
  `reconstruction.rs`) at the lines tabulated under §Defect. Seeding **any** `seg:`-backed
  committed root — or any committed record that will not decode — makes all three passes return
  `Err` for the whole store. The seeding shape to copy is `seed_segmented` at
  `crates/custodian/tests/segmented_map_restore.rs:387-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, all OK on this host at Plan. Nothing else beyond the base Rust toolchain: the passes run over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** n/a for the parent. Each child ships a **NEW** `crates/custodian/tests/segmented_map_<pass>.rs` — new, not appended, because C4-verify earns its red only from an added test file (`run-verify.sh:97-98`); appending to `segmented_map_consumers.rs` or `segmented_map_restore.rs` makes it a *modified* file and the gate proves no red at all.
- **Verification posture:** n/a for the parent (no patch). Children: default — assertion-red on
  the base, green with the patch, both at Check. Carried forward and **still settled as
  recorded-rejected**: no seeded Tier-0 DST case ships in these children — every write they
  perform is on a flat object with its existing version-conditional CAS, and rule A now makes that
  a *tested* property rather than an asserted one; the seeded Tier-0 case for the segmented write
  path belongs to #682.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line
  number in this brief was re-verified against `origin/main @ 339da46` during this Plan's
  verification pass.

  **Salvage — the primary lever, not a hint.** `results/issue_681/iteration-v7/patch.diff` holds
  per-file production hunks that passed C1–C5, C4-verify red→green and mutation analysis (82
  mutants, 0 survivors), and whose red→green the adversary independently reproduced in both
  directions. Each child takes **its own file's hunks** and applies rules A–E. Carry the existing
  discriminator legs across; do not rebuild the correctness core from scratch.

  **Peer callsites a child MAY open — this is a composition slice; mirror rather than invent:**
  - `crates/custodian/src/gc.rs:360-455` — the canonical walk: decode failure contained per object
    (`unresolvable.insert(key, fault); continue`), resolve via `metadata::resolve_chunk_map`,
    `Ok(None)` skipped, and the **downcast rule** at `:402-416`. Contain by exactly this rule.
  - `crates/custodian/src/gc.rs:164-166` + `:470-480` — attribution emitted by the consumer, per
    object, before the work loop; `object_name`'s injective escaping. Mirror the placement.
  - `crates/custodian/src/restore.rs:616-688` — the same shape applied a second time by #651.
  - `crates/custodian/src/reconciliation.rs:44` + `:55-61` — `Reconciled::Blocked` and
    `least_certified`. Reuse it; do not invent a parallel outcome.
  - `crates/core/src/metadata.rs:2256-2272` + `:2619-2632` — `ResolvedChunkMap` (why `record` rides
    along) and `resolve_chunk_map`'s three arms. **Rule A lives here.**
  - `crates/custodian/tests/segmented_map_restore.rs:387-431` and
    `crates/custodian/tests/segmented_map_consumers.rs:78-133` — the `BTreeMap`-backed `MemMeta`
    whose ordering makes "the damaged record is met FIRST" a fixture property rather than luck;
    its `scan_page` delegates to `wyrd_testkit::test_double_scan_page` (`:109-116`).
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at this Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/{reconstruction,backfill,rebalance}.rs` → 8 commits; nearest are `3e05891`
  (#648 — the segmented record shape, which **created** these seven sites) and `5f2f79f`
  (assess's classification order). No open PR touches these files. #647 is closed with the Q×N
  finding still open — item 3 above, and it belongs to the reconstruction child.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
