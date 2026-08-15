# Result — issue 696 / rebalance-reads-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/rebalance.rs` reads the chunk map inline out of the inode
  record at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` (`:141`) | the evacuation scan, for the whole store |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` (`:232`) | the binding evacuation commit |

  So a **single** segmented object stops every drain in the store — no server can be
  decommissioned once one multipart object exists. Separately, containment is not per object: a
  record that will not `decode` ends the walk at `rebalance.rs:148` before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_rebalance.rs` passes,
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: rebalance **resolves every committed object the way every other consumer already
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 21 mutants tested in 29s: 11 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #696: make rebalance resolve segmented maps, contain per-object read failures, and refuse unsafe drains without blocking healthy flat evacuations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether the evidence contract may make leg 5 a sixth base-red case: the brief declares it non-red, while the test explicitly rejects that signature; accepting the change alters the planned falsifiability boundary (`crates/custodian/tests/segmented_map_rebalance.rs:16`). |
| C2 Reproduction (red pre-fix) | PASS | With only the production fix stashed, the base-visible test compiled and produced six assertion failures plus the intended non-map-fault pass, reproducing the whole-store segmented-map abort (`crates/custodian/tests/segmented_map_rebalance.rs:298`). |
| C3 Change | PASS | The scoped safety contract holds: restarted generations are refused, segmented generations produce no write plans, and malformed keys cannot become CAS targets (`crates/custodian/src/rebalance.rs:314`). |
| C4 Verification (red→green) | PASS | Independent scratch runs gave 6 failed/1 passed pre-fix and 7/7 passed post-fix, and the exact `cargo xtask ci` gate passed with the real tools and a writable advisory database (`crates/custodian/tests/segmented_map_rebalance.rs:296`). |
| C5 Causal adequacy | PASS | The inline flat-map failure is replaced by the shared resolver and generation check rather than a capability probe, and all 21 in-diff mutants were caught or unviable (`crates/custodian/src/rebalance.rs:288`). |
| T1 Structure | PASS | The change is confined to the required production file and new discriminator, with no out-of-scope source, manifest, or documentation edits (`crates/custodian/tests/segmented_map_rebalance.rs:1`). |
| T2 Shape | PASS | The measured shape stays within every cap—94 production semantic lines and 328 test semantic/507 raw lines—while retaining one metadata double and one audit capture (`crates/custodian/tests/segmented_map_rebalance.rs:53`). |
| T3 Runtime | PASS | The real fenced control point was exercised with the Rust toolchain; targeted tests, workspace CI, conformance, dependency walls, and the 50-seed DST command all passed (`crates/custodian/tests/segmented_map_rebalance.rs:268`). |
| T4 Contribution | NEEDS-HUMAN | Decide contribution readiness: merged path history was checked (nearest change `3e05891`), but the reported two blocking batch findings and closed/rejected-work settlement are absent from the allowed artifacts, so the definition of done cannot be established (`crates/custodian/src/rebalance.rs:120`). |
| T5 Judgment | PASS | No rebuild-level gap remains: the seven legs bind progress, refusal/no-write, containment, generation identity, non-overcontainment, once-per-object reporting, and non-map fault propagation (`crates/custodian/tests/segmented_map_rebalance.rs:296`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide production-drain fitness: in-memory fenced tests establish safety state and audit semantics, but human sign-off must accept the operational usefulness of the `Blocked`/refusal behavior (`crates/custodian/tests/segmented_map_rebalance.rs:270`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether the evidence contract may make leg 5 a sixth base-red case: the brief declares it non-red, while the test explicitly rejects that signature; accepting the change alters the planned falsifiability boundary (`crates/custodian/tests/segmented_map_rebalance.rs:16`).
- [ ] T4 Contribution — Decide contribution readiness: merged path history was checked (nearest change `3e05891`), but the reported two blocking batch findings and closed/rejected-work settlement are absent from the allowed artifacts, so the definition of done cannot be established (`crates/custodian/src/rebalance.rs:120`).
- [ ] Validation — fitness-to-purpose — Decide production-drain fitness: in-memory fenced tests establish safety state and audit semantics, but human sign-off must accept the operational usefulness of the `Blocked`/refusal behavior (`crates/custodian/tests/segmented_map_rebalance.rs:270`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
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
- Iteration delta (if iterating): Rejected on size, not on the fix itself — same pattern as sibling issue_695. Five rounds of review findings show a whack-a-mole pattern within one undivided function (rebalance::plan_evacuations / reconcile_step): each round's fix for the current blocker disturbs a neighboring invariant a prior round had already satisfied (refusal/gauge accounting for aborted or malformed placements in v1/v2/current, malformed-key CAS eligibility in v4, and missing Tier-0 DST coverage for the new concurrent generation-restart path recurring in v1, v3, and again in the current round). The size backstop flagged 4 rounds spent against a threshold of 2, and the current round still carries 2 blocking findings. Ask for the re-plan: split this slice so the refusal/gauge-accounting concern (malformed or unclassifiable placements not incrementing `refused`, certifying prematurely) is separated from the concurrency/DST-coverage concern (generation-restart/supersession races needing seeded Tier-0 coverage), and from the key-validation-vs-mutation concern (Rule C). Each half should get its own brief with its own binding legs, rather than one brief whose fixes keep trading one invariant for another.
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
