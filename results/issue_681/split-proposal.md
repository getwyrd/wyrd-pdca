<!-- pdca:split-proposal v1 -->
# Split proposal — issue_681

Authored at the 2026-08-06 Plan restart, after sign-off returned the bundle with
*"The slicing is the problem, not the implementation … Re-split at Plan rather than a fifth Do
round"* (`iteration-v7/SUMMARY.md` §9).

## The axis: by pass, not by property

The sign-off's suggested axis was by property (read-through-resolver / containment / Q×N). This
proposal departs from it deliberately, agreed with the human at this Plan:

- **By property, every child edits all three files.** The children then both *conflict* and
  *serialise* into three waves, and the first child is still "all three passes, seven sites" —
  barely smaller than the thing that failed seven times.
- **Properties 1 and 2 are not separable.** The moment a child calls `resolve_chunk_map` it must
  already decide what to do with a typed read fault; the containment child would rewrite the first
  child's error handling.
- **The seven fail-closed sites partition exactly by file** — backfill 2, rebalance 2,
  reconstruction 3 — and the v7 patch measured **380 semantic production lines across three
  independent files with no shared module**. There is nothing to disentangle.
- **The blow-up was the shared test file:** 500 semantic lines, **68% of the patch**, precisely
  because every leg had to drive all three passes over one store. Splitting by pass removes that
  pressure at the root.
- **Fixture duplication is the house pattern, not a new cost:**
  `git grep "struct MemMeta" -- crates/custodian/tests/` finds **twelve** independent definitions
  on `origin/main` today.

## Wave sketch

**One wave. All three children run in parallel.**

They touch disjoint production files (`backfill.rs` / `rebalance.rs` / `reconstruction.rs`) and
disjoint new test files, so no child carries `Depends on` or `Conflicts with`. Verified against the
v7 diffstat: no child shares a file with another, and none of them touches `gc.rs`, `scrub.rs`,
`restore.rs`, `desired_state.rs`, `metadata.rs` or any `Cargo.toml`.

**#682 (4c) depends on all three** and must not be driven before they land — its brief currently
reads `Depends on: 681` (`results/issue_682/brief.md:155`) and needs repointing at the three child
ids once filed. That edit is outside this bundle.

## What every child carries

Rules A–E from the parent brief, each **bound by a test leg**. Rule A is the one that matters
most: the previous brief pinned it as *"unreachable by construction"* and explicitly forbade any
test from binding it, and four consecutive review rounds then rediscovered it — most recently as
round 7's two T4 blockers. A rule bound by no test is a rule the next reviewer re-opens.

<!-- pdca:child child-1 -->
# custodian: backfill reads through the resolver, contained (635.4b.1)

> Child 1 of 3 from the #681 split (2026-08-06). Siblings — **rebalance** and **reconstruction** —
> are independent: disjoint files, same wave, no ordering between them. **#682 depends on all
> three.** Parent context, the measured history and the full rule derivations:
> `results/issue_681/brief.md`.

- **Slug:** backfill-reads-through-resolver-contained
- **Defect:** `crates/custodian/src/backfill.rs` reads the chunk map inline out of the inode
  record at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/backfill.rs:99` | `reconcile` (`:76`) | the fill scan, for the whole store |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` (`:171`) | the remaining-placement gauge |

  So a **single** segmented object stops backfill for **every** object in the store — a store that
  has published one multipart object stops filling placements. Separately, containment is not per
  object: a record that will not `decode` ends the walk at `backfill.rs:80` and `:174`, before any
  resolver is involved.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_backfill.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::backfill::{reconcile,
  BackfillContext}`, `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}`,
  `wyrd_custodian::reconciliation::Reconciled` — over in-memory `MetadataStore` / `ChunkStore`
  doubles. **The discriminator MUST NOT name any symbol this patch introduces** (no new variant,
  field, helper or `pub fn`): the red leg reverts production, so such a reference makes the target
  fail to compile and the red degrades to UNVERIFIABLE (exit 77) instead of a behavioural red.
  `Reconciled::Blocked` already exists on the base (`reconciliation.rs:44`) and may be named.

  **Seven legs over ONE shared fixture** — one store, one seeding helper, one metadata double:

  1. **A segmented object no longer ends the pass, and the flat work in the same store still
     happens.** One healthy segmented object (raw `seg:` records + a segmented root, **never** a
     committer) beside a **fillable flat** record (empty `placement`): `reconcile` returns `Ok`
     (today `Err`) and the flat record **is filled**. *(binding — base-red)*
  2. **A segmented record is declined, not mutated, and the pass does not certify.** The `seg:`
     record's bytes and the root's `version` are **byte-identical** afterwards; the decline carries
     a **stated reason** on the audit seam and a **counted** gauge; the pass answers
     `Reconciled::Blocked`. *(binding — base-red)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store so it is a fixture property and not
     luck — (a) a committed root naming a `SegmentRef` whose `seg:` record was never written, and
     (b) a committed record whose own bytes will not `decode`; assert in the fixture that
     `resolve_chunk_map` really errors on (a). Beside them, a fillable flat record. Assert the
     conjunction: `Ok`, `Blocked` (never `Satisfied`), **the healthy record is still filled**, and
     both damaged objects are **named** on the audit seam by their `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470`). *(binding — base-red)*
  4. **Rule A — the pass never writes to a generation it did not read.** A metadata double whose
     `scan` answers a **stale segmented** root while `get` answers a **live flat** root carrying a
     fillable placement: `reconcile` returns `Ok`, writes **nothing**, leaves the live record's
     `version` unchanged, and answers `Blocked`. *(binding — base-red; this is the leg whose
     absence let four review rounds re-open the same question)*
  5. **Rule C — a record is read, written and named under exactly the key the store gave it.**
     Seed a committed, fillable record under `inode:007` beside `inode:7`: after the pass `inode:7`
     is **byte-unchanged**, `inode:007` is either filled in place or left untouched, and the pass
     does not answer `Satisfied` if it left work undone. *(binding — base-red)*
  6. **One resolving reading per pass.** On a store of S segmented objects, the counted double
     records **≤ S** `seg:` range reads and exactly **one** `scan(b"inode:")` across `reconcile`
     *and* its remaining-placement gauge — the gauge must not cost a second resolving walk.
     *(binding — base-red)*
  7. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes `reconcile` return `Err`. *(NOT base-red —
     this guards against over-containment; it passes before and after. It is the only non-red leg
     and it is required: without it, containing everything would pass every other leg.)*
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, no DST leg. Verified at Plan, not assumed: `--print-base` → `origin/main`;
  `main == origin/main == 339da46`; the `--classify` dry-run on a synthetic patch listing exactly
  `src/backfill.rs` + the new test returns `ADDED_TEST` + `CRATE crates/custodian`, so the green
  leg is `cargo test -p wyrd-custodian --test segmented_map_backfill` and the red leg reverts
  `backfill.rs` while keeping the test (`run-verify.sh:97-98`, `:454`). No
  `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped on the base), so
  neither zero-test guard can trip. Six of the seven legs go red on the base; leg 7 is declared
  non-red above.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`), over **the maintenance pass that fills placements**: it reads every
  committed object the way every other consumer reads it; containment is per object and the answer
  still gets made; it never claims more than it read; it never writes to a generation it did not
  read; and its work is one reading of the namespace, not two.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with its two siblings.** Touches
  `crates/custodian/src/backfill.rs` and one new test file; neither sibling touches either. Every
  code prerequisite is already merged on the base (#649's `resolve_chunk_map`, #650's
  `Reconciled::Blocked` and the GC containment precedent, #651's restore precedent). **#682 depends
  on this child** and must land after it.
- **Surfaces:** data
- **Difficulty:** medium   (one production file, two call-sites; effects reach
  `reconcile_step`'s `least_certified` fold through the `Reconciled` answer, and the gauge's
  reading budget.)
- **Scope:** backfill **resolves every committed object the way every other consumer already
  resolves one, contains per object what it cannot read, and declines — rather than aborts or
  silently mutates — the work it does not own.** A segmented record is left **byte-identical**,
  declined with a stated reason on the audit seam and counted, while a fillable **flat** record in
  the same store is still filled in the same pass. The remaining-placement gauge stays correct over
  a store containing segmented objects and costs no second resolving walk.

  **Rules, pinned at Plan — do not relitigate; each is bound by a leg above.**
  1. **Rule A — a pass acts only on a resolve that did not restart.** `resolve_chunk_map` restarts
     onto the live root when the caller's **segmented** snapshot was superseded
     (`metadata.rs:2338-2339`) and then answers a `ResolvedChunkMap` whose `record` is a generation
     the pass never scanned. If the resolve did not answer from the scanned generation the object
     changed under the scan: contain it, write nothing, answer `Blocked`, re-read next pass —
     the resolver hands back the generation it resolved FROM — `ResolvedChunkMap.record`, a
     `Cow` carrying its own `version` (`metadata.rs:2256-2272`) — so the pass is able to tell
     the two apart; **how it tells is Do's to choose**. Bounded per C-1.
     *(The previous brief pinned this as "unreachable by construction" and forbade a test; it was
     demonstrated false with a working double at
     `results/issue_681/iteration-v4/check-advisory-adversary.md:25`. Leg 4 binds it.)*
  2. **Rule B — an incomplete reading changes what the pass may CLAIM and what it may DISCARD,
     never what it may DO for the objects it read successfully.** Verified safe rather than argued:
     this pass mutates only flat records it read, and GC reclaims a marked fragment only past
     `ReferenceSet::protection`, which returns `incomplete-reference-set` and withholds **every**
     fragment while any object is unresolvable (`gc.rs:306-316`, consulted before every delete at
     `:191-194`). Strictness would buy no safety while costing every healthy object its fill.
  3. **Rule C — read, write and name a record under exactly the key the store gave it.** On the
     base the pass parses the scanned key to an `InodeId` then re-derives `metadata::inode_key(id)`
     for the CAS (`backfill.rs:142`); `"inode:007"` and `"inode:+3"` both parse, so the pass reads
     one record and CASes another. Precedent: `gc.rs:280-294`, `:402`.
  4. **Rule D — a decline is reported once per object, not once per chunk.**
  5. **Rule E — attribution for an object the pass could not read is emitted where the object is
     read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
     cannot cost the operator the name of the record to repair. **Load-bearing, not logging
     hygiene:** a genuinely corrupt root has no repair path (a fragment carries only
     `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`) and no operator tooling
     (tracked as **#694**), and reclamation is halted store-wide meanwhile — that name is the
     operator's entire situational awareness.

  **Constraints (they bound the shape; they do not name it):** bounded memory — work proportional
  to one object at a time, never the whole namespace's decoded chunk lists and never any segment's
  exact bytes; **one** resolving reading of the namespace per pass; containment on **any** read
  fault by exactly gc.rs's downcast rule (`gc.rs:402-416`) — `Ok(ChunkMapError)` is contained as
  *this record's* fault, any other error propagates because a store fault is not one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the write path
    for a `seg:`-resident chunk are **#682**. A decline writes **nothing at all**.
  - `rebalance.rs` and `reconstruction.rs` — **the two sibling children**. Do not touch them; a
    diff that does will conflict with a bundle building in the same wave.
  - `gc.rs`, `scrub.rs` (#650), `restore.rs`, `desired_state.rs` (#651) — untouched. Sharing ONE
    namespace walk across all loops is a separate refactor.
  - The chunk-id floor (#652); the committer, fence, rollback and resume (#653); the M8 operator
    surface (#694).
  - The existing suite `crates/custodian/tests/backfill.rs` must stay green **unmodified** — v2
    achieved that with these same production changes, so a need to edit it signals an answer
    changed further than intended, not a licence to edit it.
  - **No docs edit** (checked at Plan: `06-runtime-view.md` §6.2 already states the containment
    rule fleet-wide and stays true after this change); no new or edited ADR / spec / proposal; no
    conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the discriminator
    needs (`wyrd-testkit`, `tokio`, `async-trait`, `bytes`, `tracing-subscriber`) is already
    declared on `crates/custodian`, verified at Plan; adding one would trip the ADR-0003 audit.
- **Budget:** **exactly 2 files**, `src/backfill.rs` ≤ **130** added semantic lines (non-blank,
  non-comment) and `tests/segmented_map_backfill.rs` ≤ **320 semantic / 520 raw**. v7 measured 100
  semantic for this file's production hunks, so the cap is that plus rules A and C. A **third**
  file, or a test file past 520 raw, means the shape is wrong: STOP and hand back rather than
  finish. Compression rules: ONE `BTreeMap`-backed metadata double carrying the counters leg 6
  reads and the injected `get` fault leg 7 needs; ONE parameterised seeding helper; ONE
  audit-capture helper used by the legs that need it.
- **Repro instruction:** on the target checkout,
  `git -C ../wyrd show origin/main:crates/custodian/src/backfill.rs` at `:99` and `:181`. Seeding
  **any** `seg:`-backed committed root — or any committed record that will not decode — makes
  `backfill::reconcile` return `Err` for the whole store. The seeding shape to copy is
  `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, all OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_backfill.rs` — a **NEW** file, not optional and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97-98`); appending to `segmented_map_consumers.rs` or `segmented_map_restore.rs` makes it a *modified* file, the gate takes the green-only branch and proves no red at all. Confirmed by the `--classify` dry-run at Plan. The name completes the family (`…_consumers.rs` #650, `…_restore.rs` #651).
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Pre-declared so it arrives at sign-off as expected rather than as a surprise: **no seeded
  Tier-0 DST case ships in this child**, and the standing review finding asking for one is settled
  at Plan as recorded-rejected with this reason — *this slice introduces no new destructive or
  concurrent path: every write it performs is on a flat object read from the generation it
  scanned (Rule A, bound by leg 4) and keeps its existing version-conditional CAS, and what it adds
  on the segmented side is a decline, which writes nothing at all. The seeded Tier-0 case for the
  segmented write path belongs to #682, which builds it.* Note this reason is now **stronger than
  in v7**: rule A makes the CAS framing a tested property rather than an asserted one.
- **Citations expected:** cite `path:line` on the target branch for every change.
  **Salvage first:** `results/issue_681/iteration-v7/patch.diff` contains this file's production
  hunks (100 semantic lines) which passed C1–C5, C4-verify red→green and mutation analysis (82
  mutants, 0 survivors across the whole patch), and whose red→green an adversary independently
  reproduced in both directions. Take them and apply rules A and C; do not re-derive.
  **Peer callsites Do MAY open — this is a composition slice; mirror rather than invent:**
  `crates/custodian/src/gc.rs:360-455` (the canonical walk: decode failure contained per object via
  `unresolvable.insert(key, fault); continue`, resolve via `metadata::resolve_chunk_map`, `Ok(None)`
  skipped, and the downcast rule at `:402-416` — contain by exactly this rule and no other);
  `crates/custodian/src/gc.rs:164-166` + `:470-480` (attribution emitted by the consumer, per
  object, before the work loop; `object_name`'s injective escaping — mirror the placement, not just
  the call); `crates/custodian/src/restore.rs:616-688` (the same shape applied a second time by
  #651); `crates/custodian/src/reconciliation.rs:44` + `:55-61` (`Reconciled::Blocked` and
  `least_certified` — reuse this vocabulary, do not invent a parallel outcome);
  `crates/core/src/metadata.rs:2256-2272` + `:2619-2632` (`ResolvedChunkMap` — why `record` rides
  along — and `resolve_chunk_map`'s three arms; **rule A lives here**);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` and
  `crates/custodian/tests/segmented_map_consumers.rs:78-133` (the `BTreeMap`-backed `MemMeta` whose
  ordering makes "the damaged record is met FIRST" a fixture property rather than luck; its
  `scan_page` delegates to `wyrd_testkit::test_double_scan_page`, `:109-116`).
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/backfill.rs` → the nearest are `3e05891` (#648 — the segmented record shape,
  which **created** these two sites) and `fddb448` (identity placement backfill). No open PR touches
  this file. Seven prior attempts on the un-split #681 are archived in
  `results/issue_681/iteration-v1..v7/`.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# custodian: rebalance reads through the resolver, contained (635.4b.2)

> Child 2 of 3 from the #681 split (2026-08-06). Siblings — **backfill** and **reconstruction** —
> are independent: disjoint files, same wave, no ordering between them. **#682 depends on all
> three.** Parent context, the measured history and the full rule derivations:
> `results/issue_681/brief.md`.

- **Slug:** rebalance-reads-through-resolver-contained
- **Defect:** `crates/custodian/src/rebalance.rs` reads the chunk map inline out of the inode
  record at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` (`:141`) | the evacuation scan, for the whole store |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` (`:232`) | the binding evacuation commit |

  So a **single** segmented object stops every drain in the store — no server can be
  decommissioned once one multipart object exists. Separately, containment is not per object: a
  record that will not `decode` ends the walk at `rebalance.rs:148` before any resolver is involved.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_rebalance.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step, Custodian,
  FencedZone, RebalanceContext, Reconciled}`, `wyrd_custodian::desired_state::set_lifecycle`,
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}`, plus `Custodian::elect` +
  `FencedZone::new` over `wyrd_coordination_mem::MemCoordination` for the fence (`leadership.rs:31`,
  `:69`; the shape is `segmented_map_consumers.rs:406-410`) — over in-memory `MetadataStore` /
  `ChunkStore` doubles. Each leg drives `reconcile_step`, the real fenced control point, not an
  internal helper. **The discriminator MUST NOT name any symbol this patch introduces**: the red leg
  reverts production, so such a reference makes the target fail to compile and the red degrades to
  UNVERIFIABLE (exit 77). `Reconciled::Blocked` already exists on the base (`reconciliation.rs:44`)
  and may be named.

  **Seven legs over ONE shared fixture** — one store, one seeding helper, one metadata double:

  1. **A segmented object no longer ends the pass, and the flat work in the same store still
     happens.** One healthy segmented object (raw `seg:` records + a segmented root, **never** a
     committer) beside a flat chunk with a fragment on a **draining** server: `reconcile_step` with
     a `RebalanceContext` returns `Ok` (today `Err`) and the flat fragment **is evacuated**.
     *(binding — base-red)*
  2. **A fragment whose chunk lives in a `seg:` record stays on the draining server, refused, and
     the drain does not certify.** The draining server **still holds** that fragment afterwards; the
     `seg:` record's bytes and the root's `version` are **byte-identical**; the refusal carries a
     stated reason and a counted gauge on the audit seam; the pass answers `Reconciled::Blocked`.
     *(binding — base-red)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store — (a) a committed root naming a
     `SegmentRef` whose `seg:` record was never written, and (b) a committed record whose own bytes
     will not `decode`; assert in the fixture that `resolve_chunk_map` really errors on (a). Beside
     them, a healthy flat chunk on the draining server. Assert the conjunction: `Ok`, `Blocked`
     (never `Satisfied`), **the healthy fragment is still evacuated**, and both damaged objects are
     **named** by their `inode:` key (`gc::object_name`'s escaping shape, `gc.rs:470`).
     *(binding — base-red)*
  4. **Rule A — the pass never writes to a generation it did not read.** A metadata double whose
     `scan` answers a **stale segmented** root while `get` answers a **live flat** root placing a
     fragment on the draining server: `reconcile_step` returns `Ok`, moves **nothing**, leaves the
     live record's `version` unchanged, and answers `Blocked`. *(binding — base-red; this is the leg
     whose absence let four review rounds re-open the same question, and it is the same fact as
     round 7's T4 blocker at `rebalance.rs:412` **on the v7 tree** — these line numbers index
     `iteration-v7/patch.diff`, NOT the base)*
  5. **The containment guard is not over-broad.** A **healthy segmented** object that holds
     **nothing** on the draining server must **not** cost the drain its certification: with every
     flat evacuation complete, the pass answers `Satisfied` — a `step(false, true)` shape over that
     store. *(binding — REQUIRED, and the reason is specific: at v7 an adversary replaced the
     `rebalance.rs:196` over-containment guard's body (a **v7-tree** line, not a base line) with a
     no-op and **all six legs plus the whole
     `wyrd-custodian` suite still passed**, while the pass flipped `Satisfied`→`Blocked` over
     exactly this store — i.e. no decommission would ever certify on a store holding a multipart
     object, this slice's own defect in mirror image. Note the C5 `0 missed` row does NOT cover
     this: mutants pin the arithmetic, not the predicate.)*
  6. **Rule D — a refusal is reported once per object, not once per chunk.** Over a segmented
     object of **≥ 3 chunks** with **≥ 2** draining fragments, the captured audit stream carries
     **exactly one** refusal line for that object. *(binding — base-red; carried-forward finding:
     per-chunk logging floods the seam)*
  7. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes the pass return `Err`. *(NOT base-red — this
     guards against over-containment; it passes before and after. Required despite that: without
     it, containing EVERY error would pass every other leg. Legs 5 and 7 are this child's two
     non-red legs and both are deliberate.)*

  Also assert **Rule C** as a sub-assertion of leg 3 (**≤ ~20 lines**, no seventh test): a committed
  record under `inode:007` beside `inode:7` leaves `inode:7` **byte-unchanged** and is never
  silently reinterpreted into it.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, no DST leg. Verified at Plan: `--print-base` → `origin/main`;
  `main == origin/main == 339da46`; the `--classify` dry-run on a synthetic patch listing exactly
  `src/rebalance.rs` + the new test returns `ADDED_TEST` + `CRATE crates/custodian`, so the green
  leg is `cargo test -p wyrd-custodian --test segmented_map_rebalance` and the red leg reverts
  `rebalance.rs` while keeping the test (`run-verify.sh:97-98`, `:454`). No
  `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped on the base). Legs
  1–4 and 6 go red on the base; legs 5 and 7 are declared non-red above and exist to bind
  over-containment.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`), over **the maintenance pass that executes a drain**: it reads every
  committed object the way every other consumer reads it; containment is per object and the answer
  still gets made; it never claims more than it read (a `Satisfied` drain tells an operator a
  decommission is safe); and it never writes to a generation it did not read.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with its two siblings.** Touches
  `crates/custodian/src/rebalance.rs` and one new test file; neither sibling touches either. Every
  code prerequisite is already merged on the base (#649's `resolve_chunk_map`, #650's
  `Reconciled::Blocked` and the GC containment precedent, #651's restore precedent). **#682 depends
  on this child** and must land after it.
- **Surfaces:** data
- **Difficulty:** medium   (one production file, two call-sites; effects reach `reconcile_step`'s
  `least_certified` fold through the `Reconciled` answer, and a wrong containment predicate here
  silently withholds every decommission — which is why leg 5 is mandatory.)
- **Scope:** rebalance **resolves every committed object the way every other consumer already
  resolves one, contains per object what it cannot read, and refuses — rather than aborts or
  silently discards — the evacuation it does not own.** The evacuation scan resolves per object; a
  fragment whose chunk lives in a `seg:` record **stays on the draining server**, refused, and the
  pass does not report the drain satisfied. A flat chunk is evacuated exactly as today.

  **Rules, pinned at Plan — do not relitigate; each is bound by a leg above.**
  1. **Rule A — a pass acts only on a resolve that did not restart.** `resolve_chunk_map` restarts
     onto the live root when the caller's **segmented** snapshot was superseded
     (`metadata.rs:2338-2339`) and then answers a `ResolvedChunkMap` whose `record` is a generation
     the pass never scanned. Mixing that generation's chunk list with the snapshot's `prior_bytes`
     is exactly what makes unchecked indexing panic before the stale CAS can reject the plan —
     round 7's T4 blocker at `rebalance.rs:412` (a **v7-tree** line, not a base line). If the
     resolve did not answer from the scanned
     generation, contain the object, move nothing, answer `Blocked`, re-read next pass — the resolver hands back the generation it resolved FROM — `ResolvedChunkMap.record`, a
     `Cow` carrying its own `version` (`metadata.rs:2256-2272`) — so the pass is able to tell
     the two apart; **how it tells is Do's to choose**.
     Bounded per C-1. Leg 4 binds it.
  2. **Rule B — an incomplete reading changes what the pass may CLAIM and what it may DISCARD,
     never what it may DO for the objects it read successfully.** Verified safe rather than argued:
     the evacuation does **not** delete the source fragment, it orphan-**marks** it
     (`rebalance.rs:425-430` on the v7 tree), and GC reclaims a marked fragment only past
     `ReferenceSet::protection`, which returns `incomplete-reference-set` and withholds **every**
     fragment while any object is unresolvable (`gc.rs:306-316`, consulted before every delete at
     `:191-194`). So the loss chain cannot close and strictness buys **no** safety, while costing
     every healthy object its evacuation — "one damaged record costs the whole fleet its drain",
     the C-1 violation this child exists to remove. Leg 3 pins the progress half; leg 2 pins the
     non-certification half.
  3. **Rule C — read, write and name a record under exactly the key the store gave it.** On the
     base the pass parses the scanned key to an `InodeId` then re-derives `metadata::inode_key(id)`
     for the CAS (`rebalance.rs:310`); `"inode:007"` and `"inode:+3"` both parse, so the pass reads
     one record and CASes another. Precedent: `gc.rs:280-294`, `:402`.
  4. **Rule D — a refusal is reported once per object, not once per chunk.** Leg 6 binds it.
  5. **Rule E — attribution for an object the pass could not read is emitted where the object is
     read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
     cannot cost the operator the name of the record to repair. **Load-bearing, not logging
     hygiene:** a genuinely corrupt root has no repair path (a fragment carries only
     `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`) and no operator tooling
     (tracked as **#694**), and reclamation is halted store-wide meanwhile.

  **Constraints (they bound the shape; they do not name it):** bounded memory — work proportional
  to one object at a time, never the whole namespace's decoded chunk lists, never any segment's
  exact bytes, and never a per-chunk deep copy of a segmented root into a plan; **one** resolving
  reading of the namespace per pass; containment on **any** read fault by exactly gc.rs's downcast
  rule (`gc.rs:402-416`) — `Ok(ChunkMapError)` is contained as *this record's* fault, any other
  error propagates because a store fault is not one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the evacuation
    write path for a `seg:`-resident chunk are **#682**. A refusal writes **nothing at all**.
  - `backfill.rs` and `reconstruction.rs` — **the two sibling children**. Do not touch them; a
    diff that does will conflict with a bundle building in the same wave.
  - `gc.rs`, `scrub.rs` (#650), `restore.rs`, `desired_state.rs` (#651) — untouched.
  - The chunk-id floor (#652); the committer, fence, rollback and resume (#653); the M8 operator
    surface (#694).
  - **The pre-existing question of whether an ordinary `EvacOutcome::Aborted` (no free domain, a
    missing fragment) should certify** — that is #682's to settle. This child makes only the
    refusal **it introduces** non-certifying.
  - The existing suite `crates/custodian/tests/rebalance.rs` must stay green **unmodified**.
  - **No docs edit** (checked at Plan); no new or edited ADR / spec / proposal; no
    conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the discriminator
    needs (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
    `tracing-subscriber`) is already declared on `crates/custodian`, verified at Plan.
- **Budget:** **exactly 2 files**, `src/rebalance.rs` ≤ **130** added semantic lines (non-blank,
  non-comment) and `tests/segmented_map_rebalance.rs` ≤ **330 semantic / 540 raw**. v7 measured 100
  semantic for this file's production hunks, so the cap is that plus rules A and C. A **third**
  file, or a test file past 540 raw, means the shape is wrong: STOP and hand back rather than
  finish. Compression rules: ONE `BTreeMap`-backed metadata double carrying the injected `get`
  fault leg 7 needs; ONE parameterised seeding helper; ONE audit-capture helper shared by legs 3
  and 6.
- **Repro instruction:** on the target checkout,
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` at `:162` and `:259`. Seeding
  **any** `seg:`-backed committed root — or any committed record that will not decode — makes
  `rebalance::reconcile` return `Err` for the whole store. The seeding shape to copy is
  `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`; the fence shape is
  `segmented_map_consumers.rs:406-410`.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, all OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_rebalance.rs` — a **NEW** file, not optional and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97-98`); appending to an existing file makes it *modified*, the gate takes the green-only branch and proves no red at all. Confirmed by the `--classify` dry-run at Plan.
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Pre-declared: **no seeded Tier-0 DST case ships in this child**, and the standing review
  finding asking for one is settled at Plan as recorded-rejected with this reason — *this slice
  introduces no new destructive or concurrent path: every write it performs is on a flat object read
  from the generation it scanned (Rule A, bound by leg 4) and keeps its existing version-conditional
  CAS, and what it adds on the segmented side is a refusal, which writes nothing at all. The seeded
  Tier-0 case for the segmented write path belongs to #682, which builds it.* Also pre-declared:
  **legs 5 and 7 are deliberately not base-red** — they bind over-containment, which has no base
  behaviour to flip, and leg 5 exists because the v7 adversary proved the guard was bound by
  nothing.
- **Citations expected:** cite `path:line` on the target branch for every change.
  **Salvage first:** `results/issue_681/iteration-v7/patch.diff` contains this file's production
  hunks (100 semantic lines) which passed C1–C5, C4-verify red→green and mutation analysis, and
  whose red→green an adversary independently reproduced in both directions. Take them and apply
  rules A and C, and bind the `:196` guard per leg 5; do not re-derive.
  **Peer callsites Do MAY open — this is a composition slice; mirror rather than invent:**
  `crates/custodian/src/gc.rs:360-455` (the canonical walk and the downcast rule at `:402-416` —
  contain by exactly this rule and no other); `crates/custodian/src/gc.rs:164-166` + `:470-480`
  (attribution emitted by the consumer, per object, before the work loop; `object_name`'s injective
  escaping); `crates/custodian/src/gc.rs:306-316` + `:191-194` (`ReferenceSet::protection` — the
  backstop rule B rests on; read it before proposing to widen containment);
  `crates/custodian/src/restore.rs:616-688` (the same shape applied a second time by #651);
  `crates/custodian/src/reconciliation.rs:44` + `:55-61` (`Reconciled::Blocked` and
  `least_certified` — reuse this vocabulary, do not invent a parallel outcome);
  `crates/custodian/src/rebalance.rs:115-117` (the existing precedent for answering `Satisfied`
  without reading `inode:` when no server is draining);
  `crates/core/src/metadata.rs:2256-2272` + `:2619-2632` (`ResolvedChunkMap` and
  `resolve_chunk_map`'s three arms; **rule A lives here**);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` and
  `crates/custodian/tests/segmented_map_consumers.rs:78-133` + `:406-410` (the `BTreeMap`-backed
  `MemMeta` whose ordering makes "the damaged record is met FIRST" a fixture property rather than
  luck, and the fence shape).
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/rebalance.rs` → the nearest is `3e05891` (#648 — the segmented record shape,
  which **created** these two sites). No open PR touches this file. Seven prior attempts on the
  un-split #681 are archived in `results/issue_681/iteration-v1..v7/`; two review findings on this
  file were **recorded-rejected** with reasons that still stand and must not be re-litigated —
  see `results/issue_681/review-rejected.md` (`rebalance.rs:130`/`:141`/`:142`, "orphan-marks for
  GC"), which is rule B above.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
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
<!-- pdca:end child-3 -->
