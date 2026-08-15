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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — The rebuild must add a malformed segmented-placement case asserting blocked/refusal accounting: three independently reproduced survivors leave the new `unreadable` arithmetic and predicate unbound at `crates/custodian/src/rebalance.rs:199` and `crates/custodian/src/rebalance.rs:205`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 25 mutants tested in 36s: 3 missed, 12 caught, 10 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the two reported batch-review blockers and closed/rejected prior work are settled — the supplied bundle omits the failing review wrapper/log and rejection archive, so contribution readiness cannot be independently established.; T5 Judgment — Rebuild must make leg 5 genuinely non-base-red (or return the classification to Plan) — its first `expect` fails on base before the `Satisfied` control, contrary to the brief's explicit evidence signature (`crates/custodian/tests/segmented_map_rebalance.rs:417`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 blocking findings (review-batch.md), reduced to their real content: 1. Primary: the `certifies` test helper (segmented_map_rebalance.rs:199 and :168) silently accepts any `Err`, so leg 5 (the over-containment guard — the leg added specifically because a v7 adversary flipped Satisfied->Blocked undetected) does not actually prove `reconcile_step` returns Changed/Satisfied rather than erroring. Rebuild must make the helper assert on the Ok variant explicitly (or equivalent) so leg 5 is a real assertion again. 2. The two Tier-0 DST findings (rebalance.rs:259, tests/...rs:400) are not new issues — they restate the question already recorded-rejected in review-rejected.md (at :379) with the brief's own pre-declared Verification-posture reasoning ("this slice introduces no new destructive or concurrent path... the Tier-0 case belongs to #682"). Rebuild should record-reject these two new line-number instances with the same reasoning rather than adding DST coverage.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrides the size-backstop's iterate-plan recommendation, on the basis that the outstanding items are concrete implementation gaps. Next round must: 1. Fix the BUG at crates/custodian/src/rebalance.rs:243 — removing parse_inode_key validation makes a structurally malformed key (e.g. `inode:foo`) eligible for evacuation and a CAS write instead of being explicitly refused. Restore the validation / refuse malformed keys. 2. Address the TEST-GAP at rebalance.rs:140 — continuing evacuation/repointing after per-object scan failures is a new destructive partial-progress path lacking the rubric-required seeded Tier-0 DST coverage. Add the coverage or record-reject with a specific, checkable reason in review-rejected.md (the existing entries in that file show the expected level of rigor). 3. Fix the leg-5 test fixture in crates/custodian/tests/segmented_map_rebalance.rs:438 — it currently seeds a malformed placement instead of a genuinely healthy segmented object, so it doesn't isolate the promised `Satisfied` over-containment guard and can overstate coverage. The C1 Spec question (whether leg 5 becomes a sixth base-red case) and the T4-Contribution / Validation fitness-to-purpose items remain open for human judgment at the next sign-off; they are not implementation work for this round. Do not treat this as license to keep growing the slice: if a 4th round still can't land these within the existing scope, revisit iterate-plan.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on size, not on the fix itself — same pattern as sibling issue_695. Five rounds of review findings show a whack-a-mole pattern within one undivided function (rebalance::plan_evacuations / reconcile_step): each round's fix for the current blocker disturbs a neighboring invariant a prior round had already satisfied (refusal/gauge accounting for aborted or malformed placements in v1/v2/current, malformed-key CAS eligibility in v4, and missing Tier-0 DST coverage for the new concurrent generation-restart path recurring in v1, v3, and again in the current round). The size backstop flagged 4 rounds spent against a threshold of 2, and the current round still carries 2 blocking findings. Ask for the re-plan: split this slice so the refusal/gauge-accounting concern (malformed or unclassifiable placements not incrementing `refused`, certifying prematurely) is separated from the concurrency/DST-coverage concern (generation-restart/supersession races needing seeded Tier-0 coverage), and from the key-validation-vs-mutation concern (Rule C). Each half should get its own brief with its own binding legs, rather than one brief whose fixes keep trading one invariant for another.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
