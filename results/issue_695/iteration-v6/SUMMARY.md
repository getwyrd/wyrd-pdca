# Result — issue 695 / backfill-reads-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/backfill.rs` reads the chunk map inline out of the inode record
  at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported
  { .. })?` — `:98-101` in `reconcile` (`:76`), ending the fill scan for the whole store, and
  `:180-183` in `emit_remaining` (`:171`), ending the drain gauge. Re-verified on `origin/main @
  339da46`. So a **single** segmented object stops backfill for **every** object in the store, and
  stops the gauge being published at all. Containment is not per object either: a record that will
  not `decode` ends the walk at `:80` and `:174`, before any resolver is involved. Backfill is the
  last of the four custodian loops still reading this way — GC (#650) and restore (#651) already
  read through the shared resolver and contain per object.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_backfill.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::backfill::{reconcile,
  BackfillContext}`, `wyrd_custodian::reconciliation::Reconciled`, and
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}` — over in-memory `MetadataStore`
  / `ChunkStore` doubles. **The discriminator MUST NOT name any symbol this patch introduces** (no
  new variant, field, helper or `pub fn`): the red leg reverts `backfill.rs` and keeps the test, so
  such a reference makes the target fail to compile and the red degrades to UNVERIFIABLE (exit 77)
  instead of a behavioural red. `Reconciled::Blocked` exists on the base (`reconciliation.rs:44`)
  and may be named.

  **Five legs over ONE shared fixture** — one store double, one seeding helper, one audit-capture
  helper:

  1. **A healthy segmented object no longer ends the pass, and blocks nothing.** One segmented
     object whose placements are already full — so it needs no fill — (raw `seg:` records + a
     segmented root, **never** a committer) beside a **fillable flat** record (empty `placement`):
     `reconcile` returns `Ok` (today `Err`), the flat record **is filled** with the full-length
     identity vector, and the answer is `Reconciled::Changed` — **not** `Blocked`. *(binding —
     base-red; also binds answer rule 1)*
  2. **A segmented record whose fill this pass may not perform is declined, not mutated, and the
     pass does not certify.** A segmented object carrying an **empty** placement: its `seg:` record
     bytes and its root's `version` are **byte-identical** afterwards; the decline is on the audit
     seam under an action a reader can tell apart from "unreadable" (§Scope pins the vocabulary) and
     is counted; those empty placements are still on the remaining-gauge; `reconcile` answers
     `Reconciled::Blocked`. *(binding — base-red)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store so it is a fixture property and not
     luck — (a) a committed root naming a `SegmentRef` whose `seg:` record was never written, and
     (b) a committed record whose own bytes will not `decode`; assert in the fixture that
     `resolve_chunk_map` really errors on (a). Beside them, a fillable flat record. Assert the
     conjunction: `Ok`, `Blocked` (never `Satisfied`), **the healthy record is still filled**, and
     both damaged objects **named** on the audit seam by their `inode:` key (`gc::object_name`'s
     escaping shape, `gc.rs:470-480`). *(binding — base-red)*
  4. **One reading of the namespace per pass.** Over a store of ordinary **flat** records a counting
     double records exactly **one** `scan(b"inode:")` across `reconcile` *and* the gauge it
     publishes — today two (`:79`, `:173`) — and that gauge's value is unchanged from the base's for
     the same store. Over a store of S segmented objects it makes **≤ S** `seg:` range reads.
     *(binding — base-red on the scan count)*
  5. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes `reconcile` return `Err`. *(binding; the
     over-containment guard — without it, containing everything would pass legs 1–4. Its base
     behaviour is incidental (the base fails closed on almost everything, so it may go red there
     too); do not spend effort making it non-red.)*

  **A lost CAS is not a blocker.** `crates/custodian/tests/backfill.rs:278-325` already pins
  `Reconciled::Satisfied` after a racing writer wins the version-conditional commit. "Declined work
  ⇒ `Blocked`" must NOT be generalised to "any unfilled record ⇒ `Blocked`", or that existing test
  goes red. Conflicts stay exactly what they are on the base.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: backfill **reads every committed object through the resolver every other consumer
  already shares, contains per object what it cannot read, declines — rather than aborts or silently
  mutates — the work it does not own, reports the placement gauge from that same single reading, and
  refuses to certify a pass that answered over less than the committed store.** A record whose
  chunks live in `seg:` records is left **byte-identical** and its fill declined (the segmented write
  path is #682's); a fillable **flat** record in the same store is still filled in the same pass.

  **Two answer rules, pinned so they are not re-derived (each is a finding waiting to happen):**
  * **A decline is per unfilled placement, not per segmented object.** A segmented object the pass
    read successfully and that needs no fill is ordinary and healthy: it blocks nothing and the pass
    may still answer `Satisfied`. Only a **fillable** placement this pass may not write, or an object
    it could not read, withholds certification. Get this wrong and every store holding one multipart
    object is `Blocked` forever, which is worse than the defect being fixed.
  * **An empty placement this pass READ stays on the remaining-gauge until a fill is known to have
    landed** — including one it declined, and including one whose CAS was lost. Only a committed
    fill takes it off. The gauge is the operator's drain signal; a declined fill is still owed.

  **The constraint that keeps the write honest — it bounds the shape, it names no mechanism.**
  Whether this pass may write for an object, and the bytes any write is built from and conditioned
  on, are decided from **the generation the scan returned** — never from what a resolve answered
  after restarting onto a newer root. *Why that needs no machinery of its own:* a **flat** snapshot
  resolves to a borrow of the record and reads nothing — `ChunkMap::Flat(chunks) => return
  Ok(Resolution::Answer(Cow::Borrowed(chunks)))`, `crates/core/src/metadata.rs:2585` — so it can
  never be `Superseded` and never restarts (`:2629`). Only a **segmented** snapshot can, and a
  segmented snapshot is one this slice declines. Honour the constraint and the restart path reaches
  no write at all, **by construction**: no generation comparison, no new counter, no new concurrent
  path to sweep. (The previous brief added the comparison, then had to buy a 325-line seeded DST
  property to justify it. Both go with the path they guarded.)

  **The added audit/metric vocabulary, pinned at Plan — do not invent a parallel set, do not
  relitigate the names.** Exactly this, and each item MUST be asserted by a leg above (an unasserted
  label is a finding waiting to happen):
  * `action = "unresolvable-chunk-map"` + `monotonic_counter.backfill_unresolvable_records` for a
    record that will not decode or a generation the resolver refused — the **same action string** gc
    and restore already publish (`gc.rs:563-573`, `restore.rs:825-835`), so one grep finds all three;
  * `action = "declined-segmented"` + `monotonic_counter.backfill_declined_records` for a fill this
    pass may not perform;
  * `gauge.backfill_placement_incomplete` beside the existing `gauge.backfill_placement_remaining`,
    on the same event, each as its own `gauge.`-prefixed instrument (an unprefixed integer beside a
    gauge reaches the `tracing`→OTel bridge as an attribute on every metric in the event and would
    split the series an operator watches).

  Nothing else. Naming is by the store's own key through `gc::object_name` (`gc.rs:470-480`), which
  escapes rather than replaces, so two damaged records never arrive under one name.

  **/ out of scope — and for the first two, the base lines are FROZEN:**
  * **Key identity and attribution (the previous brief's "Rule C") — DO NOT TOUCH. Tracked as
    #698.** `parse_inode_key` (`backfill.rs:64-70`), its skip (`:84-86`), the CAS key
    `metadata::inode_key(inode_id)` with the `metadata::encode(&record)` precondition (`:142-145`),
    and the `inode_id` audit fields of `emit_backfilled` / `emit_conflict` (`:195`, `:223`) all stay
    **byte-identical to `origin/main`**. Yes, a row under a non-canonical spelling (`inode:007`)
    would be read at one key and CAS'd at another — real, **pre-existing**, unreachable today
    (`metadata::inode_key` is the sole writer of the `inode:` prefix,
    `crates/core/src/metadata.rs:33-36`), and **not this issue's defect**. Removing that parse is
    what produced the sole blocking finding in rounds 3 and 5. If a reviewer raises it: *"unchanged
    from `origin/main`; carved out to #698 and out of scope by the brief"* — record-reject with that
    reference, do not fix.
  * **A generation-restart comparison, a `changed-under-scan` class, and any seeded Tier-0 DST leg
    (the previous brief's "Rule A") — DO NOT BUILD. Tracked as #699.** The constraint above removes
    the path instead of guarding it. `Ok(None)` from the resolver is **skipped**, exactly as both
    merged peers skip it (`gc.rs:404`, `restore.rs:646`) — not counted, not named. **`crates/dst/`
    is not a file this bundle may touch.**
  * **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the write path
    for a `seg:`-resident chunk are **#682**. A decline writes **nothing at all**.
  * `rebalance.rs` and `reconstruction.rs` — the sibling children **#696** / **#697**. Do not touch
    them; a diff that does collides with a bundle building in the same wave.
  * `gc.rs`, `scrub.rs`, `restore.rs`, `desired_state.rs` — untouched (`object_name` is *used*, not
    changed). Sharing ONE namespace walk across the loops is a separate refactor.
  * The chunk-id floor (#652); the committer/fence/rollback/resume (#653); the operator repair
    surface (#694); restore's malformed-placement report (#690).
  * The existing suites `crates/custodian/tests/backfill.rs` and
    `crates/custodian/tests/backfill_telemetry.rs` stay green **unmodified** — both were green under
    the much larger v5 patch, so needing to edit either signals an answer changed further than
    intended; it is not a licence to edit them.
  * **No docs edit** (checked at Plan: `docs/design/architecture/06-runtime-view.md` §6.2, `:29` and
    `:31`, already states this containment rule fleet-wide — *"the damaged record is attributed and
    the walk continues"*, and a pass that cannot read every object *"does not certify"* — so the
    living architecture already describes the post-fix behaviour); no new or edited ADR / spec /
    proposal; no conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the
    discriminator needs (`wyrd-testkit`, `tokio`, `async-trait`, `bytes`, `tracing-subscriber`) is
    already declared on `crates/custodian` (verified at Plan); adding one would trip the ADR-0003
    audit.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 17 mutants tested in 27s: 2 missed, 9 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make custodian backfill read segmented chunk maps through the shared resolver, contain per-object unreadability, decline unsupported segmented fills without mutation, and publish a single-pass remaining/incomplete gauge.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is bounded by five behavioral legs, exact observables, a two-file scope, hard size ceilings, and named tool dependencies; no material requirement is ambiguous. |
| C2 Reproduction (red pre-fix) | PASS | With only the production fix stashed, the base-visible discriminator compiled and all five tests failed by behavioral assertion (0/5), establishing a genuine red; crates/custodian/tests/segmented_map_backfill.rs:298. |
| C3 Change | PASS | The in-scope behavior is present: typed chunk-map faults are contained, non-map store faults propagate, segmented fills are declined without a write, and flat fills retain the CAS path; crates/custodian/src/backfill.rs:143, crates/custodian/src/backfill.rs:150, crates/custodian/src/backfill.rs:200, crates/custodian/src/backfill.rs:235. |
| C4 Verification (red→green) | PASS | Independent execution produced 0/5 red then 5/5 green, and typos, docs lint/render, repository guards, fmt, clippy, workspace build/tests, cargo-machete, and a scratch-local cargo-deny audit all passed; crates/custodian/tests/segmented_map_backfill.rs:298. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Whether each unreadable class independently withholds certification remains unproven — changing either per-object `incomplete += 1` to `*=` survived mutation because the combined test needs only the other blocker; crates/custodian/src/backfill.rs:127, crates/custodian/src/backfill.rs:156, crates/custodian/tests/segmented_map_backfill.rs:369. |
| T1 Structure | PASS | The structural decision is clean: logic remains in backfill and the integration discriminator uses one BTreeMap store, one parameterized seed helper, and one audit helper; crates/custodian/tests/segmented_map_backfill.rs:47, crates/custodian/tests/segmented_map_backfill.rs:159, crates/custodian/tests/segmented_map_backfill.rs:216. |
| T2 Shape | FAIL | The brief's hard hand-back ceiling is exceeded: the new test is 473 raw and 351 semantic lines against caps of 400 and 240, so the bundle must be reshaped before acceptance; crates/custodian/tests/segmented_map_backfill.rs:473. |
| T3 Runtime | PASS | The runtime decision is supported by the green workspace and focused suites, including propagation of the injected non-ChunkMapError store fault; crates/custodian/src/backfill.rs:162, crates/custodian/tests/segmented_map_backfill.rs:456. |
| T4 Contribution | NEEDS-HUMAN | Confirm that the two reported batch-review blockers are resolved and the closed/rejected-work prior-art scan is complete — `scripts/review-branch` and the contribution checker/corpus were absent, while merged history by affected path only re-derived 3e05891 and fddb448. |
| T5 Judgment | NEEDS-HUMAN [impl] | A rebuild is owed before sign-off: compress the discriminator below the hard ceiling and make each unreadable-object increment independently observable, because current evidence permits an incomplete-object undercount; crates/custodian/src/backfill.rs:127, crates/custodian/tests/segmented_map_backfill.rs:473. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether `Blocked` plus the remaining/incomplete gauges and audit attribution are sufficient for operator drain decisions — automated in-memory evidence cannot establish that the operational signal is fit for human use; crates/custodian/src/backfill.rs:259. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Whether each unreadable class independently withholds certification remains unproven — changing either per-object `incomplete += 1` to `*=` survived mutation because the combined test needs only the other blocker; crates/custodian/src/backfill.rs:127, crates/custodian/src/backfill.rs:156, crates/custodian/tests/segmented_map_backfill.rs:369.
- [ ] T4 Contribution — Confirm that the two reported batch-review blockers are resolved and the closed/rejected-work prior-art scan is complete — `scripts/review-branch` and the contribution checker/corpus were absent, while merged history by affected path only re-derived 3e05891 and fddb448.
- [ ] T5 Judgment — A rebuild is owed before sign-off: compress the discriminator below the hard ceiling and make each unreadable-object increment independently observable, because current evidence permits an incomplete-object undercount; crates/custodian/src/backfill.rs:127, crates/custodian/tests/segmented_map_backfill.rs:473.
- [ ] Validation — fitness-to-purpose — Decide whether `Blocked` plus the remaining/incomplete gauges and audit attribution are sufficient for operator drain decisions — automated in-memory evidence cannot establish that the operational signal is fit for human use; crates/custodian/src/backfill.rs:259.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Whether each unreadable class independently withholds certification remains unproven — changing either per-object `incomplete += 1` to `*=` survived mutation because the combined test needs only the other blocker; crates/custodian/src/backfill.rs:127, crates/custodian/src/backfill.rs:156, crates/custodian/tests/segmented_map_backfill.rs:369.; T4 Contribution — Confirm that the two reported batch-review blockers are resolved and the closed/rejected-work prior-art scan is complete — `scripts/review-branch` and the contribution checker/corpus were absent, while merged history by affected path only re-derived 3e05891 and fddb448.; T5 Judgment — A rebuild is owed before sign-off: compress the discriminator below the hard ceiling and make each unreadable-object increment independently observable, because current evidence permits an incomplete-object undercount; crates/custodian/src/backfill.rs:127, crates/custodian/tests/segmented_map_backfill.rs:473.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
