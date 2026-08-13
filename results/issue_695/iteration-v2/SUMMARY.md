# Result — issue 695 / backfill-reads-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/backfill.rs` reads the chunk map inline out of the inode
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
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_backfill.rs` passes,
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: backfill **resolves every committed object the way every other consumer already
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 43 mutants tested in 48s: 30 caught, 13 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make custodian backfill resolve segmented chunk maps, contain object-local failures, and preserve safe exact-key/CAS behavior while continuing healthy work.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit about containment, stale-generation refusal, exact scanned-key identity, bounded reads, and propagation of non-map store faults; no unresolved scope choice remains. |
| C2 Reproduction (red pre-fix) | PASS | With only the production hunk removed, the unchanged eight-leg test ran and seven behavioral legs failed as expected (the declared non-red over-containment guard alone passed), grounding the defect at `crates/custodian/tests/segmented_map_backfill.rs:227`. |
| C3 Change | PASS | The change stays within the two-file budget and addresses the specified boundary through per-object resolution plus raw scanned-key/value CAS, so healthy work can continue without weakening write identity (`crates/custodian/src/backfill.rs:132`, `crates/custodian/src/backfill.rs:214`). |
| C4 Verification (red→green) | PASS | The focused suite independently changed from 7/8 failing to 8/8 passing, and the full CI components passed with the real compiler, scanners, deny checks, conformance, statics, and DST; relocating Cargo's cache only avoided a read-only host lock and did not substitute a dependency (`crates/custodian/tests/segmented_map_backfill.rs:227`). |
| C5 Causal adequacy | PASS | The direct flat-only assumption is removed rather than guarded by a capability probe: resolution occurs before classification, generation changes refuse writes, and object-local typed failures remain contained (`crates/custodian/src/backfill.rs:132`, `crates/custodian/src/backfill.rs:157`). |
| T1 Structure | PASS | The patch touches exactly the scoped production file and one shared-fixture test file, with 127/130 production semantic lines and 314/320 test semantic lines, so the stated structural limits are met (`crates/custodian/src/backfill.rs:100`, `crates/custodian/tests/segmented_map_backfill.rs:1`). |
| T2 Shape | PASS | One prefix scan feeds one resolver call per object and an exact raw-key CAS, while the fixture measures scans and segmented reads; this preserves the required traversal and identity shape (`crates/custodian/src/backfill.rs:108`, `crates/custodian/src/backfill.rs:221`, `crates/custodian/tests/segmented_map_backfill.rs:53`). |
| T3 Runtime | PASS | Real runtime evidence covers segmented success/refusal, malformed-record continuation, stale-generation refusal, exact-key preservation, bounded reads, fault propagation, and lost-CAS telemetry; the metadata-store trait supplies the required operation-termination contract (`crates/custodian/tests/segmented_map_backfill.rs:227`, `crates/traits/src/lib.rs:1001`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the four blockers reported by the batch-review gate are substantive and resolved—the referenced review output and its repository wrapper are absent here, so that red result cannot be independently reproduced, and accepting without it could bypass the repository's required deep-review contribution check. |
| T5 Judgment | PASS | Independent source/test review found no uncovered implementation case, and real `cargo mutants` testing caught 30 of 43 mutants with the remaining 13 unviable and zero survivors; the eight legs exercise the claimed decision boundaries (`crates/custodian/tests/segmented_map_backfill.rs:216`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether contained progress plus the new incomplete-population signal is operationally fit for deployment—automation proves mechanics but cannot determine whether operators accept the resulting Blocked/audit/metric semantics (`crates/custodian/src/backfill.rs:342`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the four blockers reported by the batch-review gate are substantive and resolved—the referenced review output and its repository wrapper are absent here, so that red result cannot be independently reproduced, and accepting without it could bypass the repository's required deep-review contribution check.
- [ ] Validation — fitness-to-purpose — Decide whether contained progress plus the new incomplete-population signal is operationally fit for deployment—automation proves mechanics but cannot determine whether operators accept the resulting Blocked/audit/metric semantics (`crates/custodian/src/backfill.rs:342`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the four blockers reported by the batch-review gate are substantive and resolved—the referenced review output and its repository wrapper are absent here, so that red result cannot be independently reproduced, and accepting without it could bypass the repository's required deep-review contribution check.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
