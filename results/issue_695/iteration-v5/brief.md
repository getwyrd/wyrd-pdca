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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make every restarted resolution non-certifying: `Ok(None)` and an empty successor worklist bypass refusal at `crates/custodian/src/backfill.rs:118` and `crates/custodian/src/backfill.rs:151`, and reviewer cases with already-filled or Pending successors returned `Satisfied` instead of Rule A's `Blocked`.; T5 Judgment — Rebuild must add assertions for conflict telemetry and combined refusal accounting: the independently reproduced four surviving mutants at `crates/custodian/src/backfill.rs:209`, `crates/custodian/src/backfill.rs:219`, and `crates/custodian/src/backfill.rs:271` show those claimed effects are not exercised.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 32 mutants tested in 41s: 4 missed, 17 caught, 11 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the four blockers reported by the batch-review gate are substantive and resolved—the referenced review output and its repository wrapper are absent here, so that red result cannot be independently reproduced, and accepting without it could bypass the repository's required deep-review contribution check.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: T2 Shape: the 3-semantic-line overage on segmented_map_backfill.rs (323 vs 320 cap) is accepted — no action needed, do not spend a rebuild trimming it. T4 blocking finding (review-batch.md): rebuild must fix the malformed-inode-key gap. Removing `parse_inode_key` (done to satisfy Rule C — write under the store's own key, never a re-derived one) also removed the old skip-on-parse-failure behavior. A committed row that decodes fine but sits under a key that isn't a valid `inode:<InodeId>` (e.g. `inode:not-an-id`) is now eligible to be filled/mutated like any legitimate object, instead of being attributed as an unaccountable namespace entry the way gc.rs's precedent handles objects it cannot attribute. Add back key validation that names and counts (via the existing emit_unreadable-style path) any row under a malformed inode key as unreadable/unaccountable, without re-deriving the CAS key from the parse (keep Rule C intact — validate, do not use the parsed id to build the write key).
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrides the size-backstop's iterate-plan recommendation: the outstanding items are concrete implementation gaps, not a slicing problem. Next round must: 1. Add seeded Tier-0 DST coverage for the generation-change race path (Rule A) at crates/custodian/src/backfill.rs:190 — the current Tokio test-double test is insufficient per the repo rubric; this is the sole gating T4 batched-review finding and must be resolved (fixed or recorded-rejected in review-rejected.md). 2. Bring crates/custodian/tests/segmented_map_backfill.rs back under the brief's 320-semantic-line cap (currently 336 semantic / 517 raw, 16 over) — trim/refactor the fixture rather than requesting a waiver. Do not treat this as license to keep growing the slice: if a 4th round still can't land both within the existing scope, revisit iterate-plan.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on size, not on the fix itself. Five rounds of review findings show a whack-a-mole pattern within one undivided function (backfill::reconcile): each round's fix for the current blocker disturbs a neighboring invariant that a prior round had already satisfied (gauge/CAS-race accounting in v1-v2, key-validation-vs-mutation in v3, missing Tier-0 DST coverage in v4, and now decode-before-key-validation fault misclassification at backfill.rs:148 in v5). The size backstop flagged 4 rounds spent against a threshold of 2, and findings keep looking implementation-shaped every round without converging to zero — the signature of a slice that bundles concerns that need separate boundaries, not a single stubborn bug. Ask for the re-plan: split this slice so the key-validation/decode-classification concern (Rule C: row attribution, decode-before-vs-after key check, audit naming) is separated from the concurrency/gauge-accounting concern (Rule A: generation-restart races, CAS-conflict gauge correctness, seeded Tier-0 DST coverage). Each half should get its own brief with its own binding legs, rather than one brief whose fixes keep trading one invariant for another.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
