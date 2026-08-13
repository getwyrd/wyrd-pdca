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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 32 mutants tested in 41s: 4 missed, 17 caught, 11 unviable

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

Task under review: make custodian backfill resolve segmented chunk maps, contain per-object read faults, preserve generation/key safety, and retain a single-pass placement gauge.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes seven falsifiable legs, exact scope and budgets, explicit external dependencies, and settled deferrals, so the required behavior is unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | Independent base execution ran seven tests: six failed behaviorally while the declared non-red store-fault guard passed; the binding and guard legs are grounded at `crates/custodian/tests/segmented_map_backfill.rs:237` and `crates/custodian/tests/segmented_map_backfill.rs:417`. |
| C3 Change | PASS | The requested change is narrowly bounded to resolver-based per-object containment, no segmented writes, exact-key CAS, and one namespace reading, matching the intended loop surface at `crates/custodian/src/backfill.rs:99`. |
| C4 Verification (red→green) | PASS | The same discriminator moved from six behavioral failures plus one green guard to 7/7 green, existing backfill tests stayed green, and all listed tool dependencies were exercised; the read-only cargo-deny lock was discharged with a scratch `CARGO_HOME` rather than treated as a patch failure (`crates/custodian/tests/segmented_map_backfill.rs:237`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must make every restarted resolution non-certifying: `Ok(None)` and an empty successor worklist bypass refusal at `crates/custodian/src/backfill.rs:118` and `crates/custodian/src/backfill.rs:151`, and reviewer cases with already-filled or Pending successors returned `Satisfied` instead of Rule A's `Blocked`. |
| T1 Structure | PASS | The contribution is confined to the intended production module and new discriminator file, with the production entry point at `crates/custodian/src/backfill.rs:91` and seven test legs beginning at `crates/custodian/tests/segmented_map_backfill.rs:237`. |
| T2 Shape | FAIL | The hard one-parameterized-seeding-helper constraint is not met because `seed_flat` and `seed_seg` are separate helpers at `crates/custodian/tests/segmented_map_backfill.rs:182` and `crates/custodian/tests/segmented_map_backfill.rs:190`, although the file and semantic-line caps are met. |
| T3 Runtime | FAIL | A lost CAS removes the observed empties and increments `superseded`, but the emitted incomplete gauge omits that counter at `crates/custodian/src/backfill.rs:208` and `crates/custodian/src/backfill.rs:219`; the existing race leaves the live placement empty at `crates/custodian/tests/backfill.rs:297`, so the pass can publish a false `0/0` drain signal. |
| T4 Contribution | FAIL | Affected-path prior art was mechanically rechecked (merged PRs #402/#531/#594/#672 and rejected PR #647), but the contribution remains unready while the Rule A and conflict-telemetry defects at `crates/custodian/src/backfill.rs:118` and `crates/custodian/src/backfill.rs:219` remain. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add assertions for conflict telemetry and combined refusal accounting: the independently reproduced four surviving mutants at `crates/custodian/src/backfill.rs:209`, `crates/custodian/src/backfill.rs:219`, and `crates/custodian/src/backfill.rs:271` show those claimed effects are not exercised. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether a one-pass population floor plus an incomplete-reading gauge is operationally sufficient for drain-to-zero and later fallback removal once the implementation defects are fixed, because automated evidence cannot establish operator fitness. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must make every restarted resolution non-certifying: `Ok(None)` and an empty successor worklist bypass refusal at `crates/custodian/src/backfill.rs:118` and `crates/custodian/src/backfill.rs:151`, and reviewer cases with already-filled or Pending successors returned `Satisfied` instead of Rule A's `Blocked`.
- [ ] T5 Judgment — Rebuild must add assertions for conflict telemetry and combined refusal accounting: the independently reproduced four surviving mutants at `crates/custodian/src/backfill.rs:209`, `crates/custodian/src/backfill.rs:219`, and `crates/custodian/src/backfill.rs:271` show those claimed effects are not exercised.
- [ ] Validation — fitness-to-purpose — Human must decide whether a one-pass population floor plus an incomplete-reading gauge is operationally sufficient for drain-to-zero and later fallback removal once the implementation defects are fixed, because automated evidence cannot establish operator fitness.
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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make every restarted resolution non-certifying: `Ok(None)` and an empty successor worklist bypass refusal at `crates/custodian/src/backfill.rs:118` and `crates/custodian/src/backfill.rs:151`, and reviewer cases with already-filled or Pending successors returned `Satisfied` instead of Rule A's `Blocked`.; T5 Judgment — Rebuild must add assertions for conflict telemetry and combined refusal accounting: the independently reproduced four surviving mutants at `crates/custodian/src/backfill.rs:209`, `crates/custodian/src/backfill.rs:219`, and `crates/custodian/src/backfill.rs:271` show those claimed effects are not exercised.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
