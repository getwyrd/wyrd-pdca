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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 17 mutants tested in 24s: 11 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #695: make backfill resolve segmented chunk maps per object, contain unreadable records, decline unsupported segmented fills, and publish trustworthy one-pass drain gauges.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the acceptance decision objective: it fixes containment, certification, audit/gauge semantics, lost-CAS behavior, five observable legs, exclusions, and hard size limits. |
| C2 Reproduction (red pre-fix) | PASS | With only the base-visible discriminator added, all five tests compiled and failed behaviorally—covering healthy/declined/damaged segmented objects, one namespace scan, and non-map store faults (`crates/custodian/tests/segmented_map_backfill.rs:253`, `crates/custodian/tests/segmented_map_backfill.rs:383`). |
| C3 Change | PASS | The authorized maintenance behavior is fully covered without crossing the frozen key/CAS surface: resolution and fault classification happen inside the existing pass, segmented writes are declined, and gauges use that same walk (`crates/custodian/src/backfill.rs:127`, `crates/custodian/src/backfill.rs:156`, `crates/custodian/src/backfill.rs:219`, `crates/custodian/src/backfill.rs:280`). |
| C4 Verification (red→green) | PASS | Independent scratch reruns produced 5/5 assertion failures on base, 5/5 passes with the production hunk, and a complete green `cargo xtask ci` including dependency walls, conformance, statics, and DST (`crates/custodian/tests/segmented_map_backfill.rs:253`). |
| C5 Causal adequacy | PASS | The causal defect was the inline flat-map read: every committed object now reaches the shared resolver, while the only remaining shape branch declines an intentionally unsupported write path rather than probing around resolver capability (`crates/custodian/src/backfill.rs:150`, `crates/custodian/src/backfill.rs:219`). |
| T1 Structure | PASS | The existing pass remains the single coordinator, with one shared resolver path and small telemetry helpers; the discriminator likewise has one store double, one seeder, and one audit capture (`crates/custodian/src/backfill.rs:315`, `crates/custodian/tests/segmented_map_backfill.rs:51`, `crates/custodian/tests/segmented_map_backfill.rs:150`, `crates/custodian/tests/segmented_map_backfill.rs:201`). |
| T2 Shape | PASS | The hard budget is met exactly as intended: 2 files, 61/95 production semantic lines, 237/240 test semantic lines, and 394/400 test raw lines, with no frozen or sibling-file hunk (`crates/custodian/tests/segmented_map_backfill.rs:394`). |
| T3 Runtime | PASS | The public reconciler was exercised over ordered, fault-injecting in-memory trait doubles, and the full workspace plus 50-seed DST gate passed; no live service or topology is an external dependency for this slice (`crates/custodian/tests/segmented_map_backfill.rs:51`, `crates/custodian/tests/segmented_map_backfill.rs:218`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the contribution evidence is sufficient despite an unreproducible driver row—the target lacks `scripts/review-branch` and `scripts/pdca`, so the recorded batch-review FAIL and contribution PASS cannot be rerun, although merged and closed-PR history was independently scanned by both affected paths. |
| T5 Judgment | PASS | No further implementation or architectural judgment is exposed: the lost-CAS accounting concern is explicitly deferred to #699 and therefore settled by the repository protocol, and this patch adds no capability-probe symptom guard (`crates/custodian/src/backfill.rs:263`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether resolver-double and full-gate evidence is sufficient for production sign-off—the change controls custodian certification and operator drain signals, so operational fitness remains a human risk decision (`crates/custodian/src/backfill.rs:280`, `crates/custodian/src/backfill.rs:315`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Decide whether the contribution evidence is sufficient despite an unreproducible driver row—the target lacks `scripts/review-branch` and `scripts/pdca`, so the recorded batch-review FAIL and contribution PASS cannot be rerun, although merged and closed-PR history was independently scanned by both affected paths.
- [x] Validation — fitness-to-purpose — Decide whether resolver-double and full-gate evidence is sufficient for production sign-off—the change controls custodian certification and operator drain signals, so operational fitness remains a human risk decision (`crates/custodian/src/backfill.rs:280`, `crates/custodian/src/backfill.rs:315`).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- [x] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_695: M8 repair tool (#694) is the settling path for corrupt rows flagged by backfill/GC — confirm its scope covers repair-or-removal of undecodable `inode:` rows so flagged stores can re-certify.
