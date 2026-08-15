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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 25 mutants tested in 36s: 3 missed, 12 caught, 10 unviable

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

Review: make custodian rebalance resolve segmented chunk maps per object, continue safe flat evacuations, and withhold drain certification for unreadable or refused objects.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | FAIL | Plan must correct leg 5's declared non-red status: base `339da46` actually fails at `crates/custodian/tests/segmented_map_rebalance.rs:496` with `SegmentedMapUnsupported`, so the brief's falsifiability accounting is inaccurate. |
| C2 Reproduction (red pre-fix) | PASS | The base-visible discriminator compiled and produced behavioral red—six failures and one pass—with the first segmented-map expectation grounded at `crates/custodian/tests/segmented_map_rebalance.rs:336`. |
| C3 Change | PASS | No scope choice remains: the patch uses the shared resolver and typed per-object containment, then preserves the scanned key for CAS at `crates/custodian/src/rebalance.rs:250` and `crates/custodian/src/rebalance.rs:438`. |
| C4 Verification (red→green) | PASS | Independent scratch runs changed six-fail/one-pass on base to seven-pass with the patch, and typos, docs render, fmt, clippy, build/tests, machete, all three deny walls, conformance, statics, and DST passed; the exercised entry point is `crates/custodian/tests/segmented_map_rebalance.rs:271`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must add a malformed segmented-placement case asserting blocked/refusal accounting: three independently reproduced survivors leave the new `unreadable` arithmetic and predicate unbound at `crates/custodian/src/rebalance.rs:199` and `crates/custodian/src/rebalance.rs:205`. |
| T1 Structure | PASS | The change stays in the two prescribed files and every leg drives the real fenced `reconcile_step` seam at `crates/custodian/tests/segmented_map_rebalance.rs:271`. |
| T2 Shape | FAIL | The rebuild must compress/restructure the fixture: `crates/custodian/tests/segmented_map_rebalance.rs:540` is 540 raw but 439 nonblank/non-comment lines, exceeding the brief's hard 330-semantic-line ceiling. |
| T3 Runtime | PASS | Runtime behavior was exercised through the in-memory trait doubles and fenced control point, including the propagated non-map store fault at `crates/custodian/tests/segmented_map_rebalance.rs:529`; no declared external dependency remains unexercised. |
| T4 Contribution | PASS | Affected-path audit found merged precedent through #648, closed/unmerged prior work in #647, and no open PR touching either affected path, so no unresolved prior-art decision remains. |
| T5 Judgment | PASS | No additional architectural decision is exposed: the resolver replaces the inline segmented-map failure rather than probing around it, while restarted generations and the deferred #682 write surface are explicitly refused at `crates/custodian/src/rebalance.rs:275` and `crates/custodian/src/rebalance.rs:285`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether per-object progress paired with `Blocked` certification is operationally fit for staged decommissioning—the automated in-memory evidence validates mechanics but not operator acceptance of the contract at `crates/custodian/src/rebalance.rs:152`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The rebuild must add a malformed segmented-placement case asserting blocked/refusal accounting: three independently reproduced survivors leave the new `unreadable` arithmetic and predicate unbound at `crates/custodian/src/rebalance.rs:199` and `crates/custodian/src/rebalance.rs:205`.
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether per-object progress paired with `Blocked` certification is operationally fit for staged decommissioning—the automated in-memory evidence validates mechanics but not operator acceptance of the contract at `crates/custodian/src/rebalance.rs:152`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — The rebuild must add a malformed segmented-placement case asserting blocked/refusal accounting: three independently reproduced survivors leave the new `unreadable` arithmetic and predicate unbound at `crates/custodian/src/rebalance.rs:199` and `crates/custodian/src/rebalance.rs:205`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
