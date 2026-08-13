# Result — issue 696 / rebalance-reads-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/rebalance.rs` reads the chunk map inline out of the inode record
  at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported
  { .. })?`, re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `rebalance.rs:158-164` | `plan_evacuations` (`:141`) | the evacuation scan, for the whole store |
  | `rebalance.rs:255-261` | `evacuate_chunk` (`:232`) | the binding evacuation commit |

  So a **single** segmented object stops every drain in the store — no server can be decommissioned
  once one multipart object exists. Containment is not per object either: a record that will not
  `decode` ends the walk at `:148`, before any resolver is involved. Rebalance is one of the two
  custodian loops still reading this way — GC (#650) and restore (#651) already read through the
  shared resolver and contain per object.

  The operator-facing consequence is already written down in the base source: the drain-status
  surface answers `Pending` for a server whose only remaining fragments belong to a segmented
  object, and `desired_state.rs:206-211` says exactly what that means — *"a bare `Pending` … tells
  them to keep waiting for an evacuation that can never finish, because rebalance cannot move
  fragments of a map it cannot read. A wait with nothing to act on is a state nothing exits."*
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_rebalance.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step, Custodian,
  FencedZone, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle}`, and
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}`, plus the ordinary `wyrd_traits`
  and `wyrd_core::placement` seams the existing suites already use (`Topology`, `FailureDomain`,
  `DServerId`, `ChunkId`, `FragmentId`, `ChunkStore`, `MetadataStore`, `WriteBatch`) — over
  in-memory `MetadataStore` / `ChunkStore` doubles. That list is the *custodian/core surface* the
  legs assert against, not a whitelist that excludes the trait seams the fixture is built from.
  `rebalance::reconcile` is `pub(crate)` (`rebalance.rs:106`), so every leg
  drives **`reconcile_step`** (`reconciliation.rs:104-112`, rebalance arm `:138-144`) — the real
  fenced control point — via `Custodian::elect` + `FencedZone::new` over
  `wyrd_coordination_mem::MemCoordination`. **The discriminator MUST NOT name any symbol this patch
  introduces** (no new variant, field, helper or `pub fn`): the red leg reverts `rebalance.rs` and
  keeps the test, so such a reference makes the target fail to compile and the red degrades to
  UNVERIFIABLE (exit 77) instead of a behavioural red. `Reconciled::Blocked` exists on the base
  (`reconciliation.rs:44`) and may be named.

  **Five legs over ONE shared fixture** — one metadata double, one fleet of chunk-store doubles, one
  seeding helper, one audit-capture helper:

  1. **A segmented object no longer ends the pass, and the flat work in the same store still
     happens.** One healthy segmented object that holds **nothing** on the draining server (raw
     `seg:` records + a segmented root, **never** a committer) beside a **flat** chunk with a
     fragment on a draining server: `reconcile_step` with a `RebalanceContext` returns `Ok` (today
     `Err`), the flat fragment **is evacuated**, and the answer is `Reconciled::Changed` — **not**
     `Blocked`. *(binding — base-red; also binds answer rule 1)*
  2. **A segmented object that owes an evacuation is refused once, mutates nothing, and the pass
     does not certify.** A segmented object of **≥ 3 chunks** with **≥ 2** fragments on the draining
     server: afterwards the draining server **still holds** both fragments; every `seg:` record's
     bytes and the root's `version` are **byte-identical**; the captured audit stream carries
     **exactly one** refusal line for that object (not one per chunk) under the action §Scope pins,
     carrying a stated reason and its counter; `reconcile_step` answers `Reconciled::Blocked`.
     *(binding — base-red; the once-per-object half is a carried-forward finding — per-chunk logging
     floods the seam)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed —
     **first in key order**, over a `BTreeMap`-backed store so it is a fixture property and not luck
     — (a) a committed root naming a `SegmentRef` whose `seg:` record was never written, and (b) a
     committed record whose own bytes will not `decode`; assert in the fixture that
     `resolve_chunk_map` really errors on (a). Beside them, a healthy flat chunk on the draining
     server. Assert the conjunction: `Ok`, `Blocked` (never `Satisfied`), **the healthy fragment is
     still evacuated**, and both damaged objects **named** on the audit seam by their `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470-480`). *(binding — base-red)*
  4. **The containment is not over-broad.** A **healthy segmented** object that holds **nothing** on
     the draining server must **not** cost the drain its certification: over a store where every
     flat evacuation is already complete, the pass answers `Reconciled::Satisfied`. **That object
     must be genuinely healthy — every `seg:` record present and resolvable and every placement
     well-formed.** A malformed placement anywhere in the leg-4 store makes the leg stop isolating
     the guard (the answer could then be explained by the malformed arm instead), which is exactly
     the round-4 T5 finding. *(binding —
     REQUIRED, and the reason is specific: at #681 v7 an adversary replaced the over-containment
     guard's body with a no-op and **every other leg plus the whole `wyrd-custodian` suite still
     passed**, while the pass flipped `Satisfied`→`Blocked` over exactly this store — i.e. no
     decommission would ever certify on a store holding a multipart object, this slice's own defect
     in mirror image. The C5 `0 missed` row does NOT cover it: mutants pin the arithmetic, not the
     predicate.)*
  5. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes the pass return `Err`. *(binding; the
     over-containment guard — without it, containing EVERY error would pass legs 1–4.)*

  **On which legs go red, stated once so it is not re-litigated at Check.** All five go red on the
  base, because the base `?`s out of the whole scan on *any* segmented record and on any record that
  will not decode. That is a fact about the base, not the point of legs 4 and 5: those two exist to
  go red against an **over-broad fix**, which is a failure mode no other leg can see. Do not spend
  effort making them non-red, and do not "simplify" them away because they are red for the same
  reason as legs 1–3. *(This is the single most repeated finding on this bundle: the previous brief
  declared its equivalent of leg 4 **non**-base-red while its test asserted otherwise, and the
  contradiction was raised as a blocking T5 or C1 NEEDS-HUMAN in rounds 2, 4 **and** 5. It is
  settled here, in the brief, in the only direction the code allows.)*

  **A leg must assert the call succeeded before it inspects the outcome.** Any `certifies`-style
  helper that folds `Result<Reconciled, _>` down to a bool MUST fail on `Err` rather than treat it
  as "did not certify" — a helper that silently accepts every `Err` makes legs 1 and 4 pass on a
  tree where the pass aborts outright, which is the defect itself. Round 3 lost the whole gate to
  exactly this, found twice in one pass (`tests:168`, `tests:199`). Assert `Ok(..)` explicitly.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: rebalance **reads every committed object through the resolver every other consumer
  already shares, contains per object what it cannot read, and refuses — rather than aborts or
  silently discards — the evacuation it does not own.** A chunk whose bytes live in a `seg:` record
  is left **byte-identical** and its evacuation refused (the segmented write path is #682's); a
  **flat** chunk in the same store is evacuated exactly as today; and the pass does not report the
  drain satisfied over a refusal or over an object it could not read.

  **Two answer rules, pinned so they are not re-derived (each is a finding waiting to happen):**
  * **A refusal is per evacuation owed, not per segmented object.** A segmented object the pass read
    successfully that holds **nothing** on a draining server is ordinary and healthy: it blocks
    nothing and the pass may still answer `Satisfied`. Only an evacuation this pass may not perform,
    or an object it could not read, withholds certification. Get this wrong and every store holding
    one multipart object is `Blocked` forever, so no decommission ever certifies — this slice's own
    defect in mirror image. Bound by leg 4. Precedent for answering `Satisfied` without reading the
    namespace at all: `rebalance.rs:115-117`.
  * **A refusal is reported once per object, not once per chunk.** Bound by leg 2.

  **Why continuing the walk past an object it could not read is safe — verified, not argued.** Leg 3
  asserts the healthy fragment is *still* evacuated while a damaged record sits in the same store, so
  the question "is partial progress over an incomplete reading a loss risk?" will be asked; it is
  answered on the base, by reading three things. (1) An evacuation never **deletes** the source
  fragment — it orphan-**marks** it in the same batch (`rebalance.rs:314-319`). (2) GC reclaims a
  marked fragment only past `ReferenceSet::protection`, which answers `"incomplete-reference-set"`
  and withholds **every** fragment in the fleet while any object is unresolvable (`gc.rs:306-312`),
  and that is consulted before each delete (`gc.rs:191-194`). So while the record that made this pass
  continue is still damaged, **nothing is reclaimable at all** — the loss chain cannot close, and
  strictness buys no safety while costing every healthy object its evacuation. (3) The bytes are
  copied to their new home *before* the commit, so a crash mid-move leaves collectable garbage, never
  a torn chunk (`rebalance.rs:263-264`). What this patch adds at the two fault sites is a `continue`,
  never a write: the objects the work loop then evacuates are exactly the ones the base would have
  evacuated had the damaged record not existed. This is already recorded-rejected on this bundle
  (`review-rejected.md`, the `rebalance.rs:140` and `:148` entries) — point at it, do not re-fix.

  **The constraint that keeps the write honest — it bounds the shape, it names no mechanism.**
  Whether this pass may write for an object, and the bytes any write is built from and conditioned
  on, are decided from **the generation the scan returned** — never from what a resolve answered
  after restarting onto a newer root. *Why that needs no machinery of its own:* a **flat** snapshot
  resolves to a borrow of the record and reads nothing — `ChunkMap::Flat(chunks) => return
  Ok(Resolution::Answer(Cow::Borrowed(chunks)))`, `crates/core/src/metadata.rs:2585` — so it can
  never be `Superseded` and never restarts (`:2629`). Only a **segmented** snapshot can, and a
  segmented snapshot is one this slice refuses. Honour the constraint and the restart path reaches
  no write at all, **by construction**: no generation comparison, no new counter, no new concurrent
  path to sweep. Concretely: read the eligibility decision off the **scanned record's own
  `chunk_map` shape**, which is already in hand — not off the shape of whatever the resolve
  answered. (The previous brief added the comparison, then had to buy a seeded DST property to
  justify it. Both go with the path they guarded.)

  **The added audit/metric vocabulary, pinned at Plan — do not invent a parallel set, do not
  relitigate the names.** Exactly this, and each item MUST be asserted by a leg above (an unasserted
  label is a finding waiting to happen). Both go on rebalance's existing audit target
  `"wyrd.custodian.rebalance.audit"` (`rebalance.rs:359`, `:375`, `:387`):
  * `action = "unresolvable-chunk-map"` + `monotonic_counter.rebalance_unresolvable_records` for a
    record that will not decode or a map the resolver refused — the **same action string** gc,
    restore and scrub already publish (`gc.rs:563-571`, `restore.rs:827-830`, `scrub.rs:230-233`),
    each with its own `<loop>_unresolvable_records` counter, so one grep finds all four;
  * `action = "refused-segmented"` + `monotonic_counter.rebalance_refused_records` for an evacuation
    this pass may not perform. (#695 names backfill's equivalent `declined-segmented`; the shared
    `-segmented` suffix is what makes the family greppable, and each loop keeps the verb its own
    module doc uses — rebalance's selector already "refuses", `rebalance.rs:38`. Do not "harmonise".)

  Nothing else — no new gauge. Naming is by the store's own key through `gc::object_name`
  (`gc.rs:470-480`), which escapes rather than replaces, so two damaged records never arrive under
  one name; `rebalance.rs:316` already reaches into `crate::gc` for `orphan_key`, so the cross-module
  use needs no new seam. Attribution for an object the pass could not read is emitted **per object,
  where the object is read, before the work loop** — mirroring `gc.rs:155-166` — so a later transient
  store fault cannot cost the operator the name of the record to repair.

  **/ out of scope — and for the first three, the base lines are FROZEN:**
  * **Key identity and attribution (the previous brief's "Rule C") — DO NOT TOUCH. Tracked as
    #698.** `parse_inode_key` (`rebalance.rs:332-338`) and its skip (`:152-154`), the CAS key
    `metadata::inode_key(plan.inode_id)` (`:310`) and its `metadata::encode(&plan.prior)`
    precondition (`:312`) all stay **byte-identical to `origin/main`**. Yes, a row under a
    non-canonical spelling (`inode:007`) would be read at one key and CAS'd at another — real,
    **pre-existing**, unreachable today (`metadata::inode_key` is the sole writer of the `inode:`
    prefix, `crates/core/src/metadata.rs:34`), and **not this issue's defect**. #698 names
    `rebalance.rs` explicitly as a sibling to sweep. Removing that parse is what produced the sole
    blocking finding in #695 rounds 3 and 5 and #696 round 4. If a reviewer raises it: *"unchanged
    from `origin/main`; carved out to #698 and out of scope by the brief"* — record-reject with that
    reference, do not fix.
  * **A generation-restart comparison, a `changed-under-scan` class, and any seeded Tier-0 DST leg
    (the previous brief's "Rule A") — DO NOT BUILD. Tracked as #699**, which asks the question
    "once and fleet-wide rather than per loop" and which #682 depends on. The constraint above
    removes the path instead of guarding it. `Ok(None)` from the resolver is **skipped**, exactly as
    both merged peers skip it (`gc.rs:404`, `restore.rs:646`) — not counted, not named.
    **`crates/dst/` is not a file this bundle may touch.**
  * **A malformed committed placement stays exactly as the base answers it — DO NOT TOUCH.**
    `rebalance.rs:177-183` (`checked_fragments()` → `emit_needs_human(chunk.id); continue;`) and
    `emit_needs_human` (`:372-380`) stay **byte-identical to `origin/main`**, including in whatever
    path now walks a segmented object's chunks. It is **not** counted as a refusal and does **not**
    by itself withhold `Reconciled::Satisfied`. Three checkable reasons, and this is the finding that
    blocked v5:
    1. It is **base** behaviour this slice neither adds nor widens (`rebalance.rs:177-183` on
       `origin/main @ 339da46`).
    2. The premise that an operator could therefore decommission is **false at the operator
       surface**, and the base source says so in as many words: the drain-certification query is
       `desired_state::reconciliation_status`, which returns `ReconciliationStatus::PendingMalformed`
       **cluster-wide** while any malformed placement exists (`desired_state.rs:234-246`) and
       `PendingUnresolvable` while any record is unreadable (`:225-232`). `Reconciled::Satisfied`
       (one loop's convergence answer) and `ReconciliationStatus::Satisfied` (may-I-decommission) are
       **different surfaces**; v5's finding conflated them. GC protects every fragment bearing that
       chunk's id (`gc.rs:186-194`, `:306-312`).
    3. The change is **forbidden by this brief's own constraint**, verified by reading the test:
       `crates/custodian/tests/rebalance.rs:1412`
       (`malformed_placement_rebalance_skips_and_leaves_fragment_in_place`) asserts
       `Reconciled::Satisfied` at `:1457` beside `PendingMalformed` at `:1491`, over exactly this
       fixture — and that suite must stay green **unmodified**. Counting the malformed chunk fails it.

       Re-deciding this means re-opening #348 / ADR-0040 decision 4, not this slice. The restore-side
       analogue is filed as **#690**; there is no rebalance analogue to file, because rebalance
       already attributes it (`emit_needs_human` + `monotonic_counter.rebalance_malformed_placement`
       + `PendingMalformed` naming the chunk ids), which is precisely what #690 says restore lacks.
  * **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the evacuation
    write path for a `seg:`-resident chunk are **#682**. A refusal writes **nothing at all**.
  * **The pre-existing question of whether an ordinary `EvacOutcome::Aborted` (no free domain, an
    off-fleet or missing or checksum-failing fragment) should certify** — base behaviour
    (`rebalance.rs:128` swallows it; `:250`, `:277`, `:284`, `:287` raise it), settled by **#682**.
    This child makes only the refusal **it introduces** non-certifying.
  * `backfill.rs` and `reconstruction.rs` — the sibling children **#695** / **#697**. Do not touch
    them; a diff that does collides with a bundle building in the same wave.
  * `gc.rs`, `scrub.rs`, `restore.rs`, `desired_state.rs` — untouched (`gc::object_name` is *used*,
    not changed). Sharing ONE namespace walk across the loops is a separate refactor.
  * The chunk-id floor (#652); the committer/fence/rollback/resume (#653); the operator repair
    surface (#694).
  * The existing suite `crates/custodian/tests/rebalance.rs` stays green **unmodified** — it was
    green under the much larger v5 patch, so needing to edit it signals an answer changed further
    than intended; it is not a licence to edit it.
  * **No docs edit** (checked at Plan: `docs/design/architecture/06-runtime-view.md` §6.2 already
    states this containment rule fleet-wide — the damaged record is attributed and the walk
    continues, and a pass that cannot read every object does not certify — so the living
    architecture already describes the post-fix behaviour); no new or edited ADR / spec / proposal;
    no conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the
    discriminator needs (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
    `tracing-subscriber`) is already declared on `crates/custodian` (verified at Plan); adding one
    would trip the ADR-0003 audit.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 14 mutants tested in 22s: 4 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make rebalance resolve committed chunk maps, contain object-owned read failures, continue flat evacuations, and refuse unsupported segmented evacuations without falsely certifying a drain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the safety boundary decidable: object-owned map faults are contained, owed segmented evacuations block certification, healthy unowed segmented objects do not, and non-map store faults still propagate; each outcome is independently observable through the existing fenced entry point at `crates/custodian/src/reconciliation.rs:104`. |
| C2 Reproduction (red pre-fix) | PASS | In a scratch clone at base `339da46`, all five added tests compiled against base-visible symbols and failed by assertion on the old whole-pass segmented-map error, including the explicit-success checks beginning at `crates/custodian/tests/segmented_map_rebalance.rs:305`. |
| C3 Change | PASS | The scoped behavior is implemented at the two relevant decisions: resolver errors are split by typed ownership at `crates/custodian/src/rebalance.rs:215`, and only a segmented object that owes an evacuation withholds certification at `crates/custodian/src/rebalance.rs:239`; the frozen key/CAS and malformed-placement behavior remains unchanged. |
| C4 Verification (red→green) | PASS | The same five-test binary went from 0/5 assertion-red on base to 5/5 green with the production hunk; all repository-gate components also passed independently, with `cargo-deny` rerun successfully under a scratch Cargo home after the default advisory-db lock proved read-only (`crates/custodian/tests/segmented_map_rebalance.rs:299`). |
| C5 Causal adequacy | PASS | The change removes the inline flat-map whole-pass failure in favor of resolver-backed typed containment at `crates/custodian/src/rebalance.rs:188`, and the in-diff mutation run found 14 mutants with 4 caught, 10 unviable, and 0 missed; no capability-probe symptom guard was added. |
| T1 Structure | PASS | The patch is limited to the production module and one new integration test, whose shared fixture drives the real fenced `reconcile_step` rather than a test-only entry at `crates/custodian/tests/segmented_map_rebalance.rs:273`. |
| T2 Shape | FAIL | The brief's approved fixture ceiling is exceeded: the new file has 288 nonblank/noncomment lines versus at most 265 (while landing exactly at the 440-raw-line ceiling), so its test shape requires compression at `crates/custodian/tests/segmented_map_rebalance.rs:440`. |
| T3 Runtime | PASS | The production seam was exercised through an elected custodian and real reconciliation dispatch, while workspace tests, conformance, statics, and the 50-seed DST campaign passed; the call under test is at `crates/custodian/tests/segmented_map_rebalance.rs:292`. |
| T4 Contribution | NEEDS-HUMAN | Decide the disposition of the reported one-blocker batched review and confirm affected-path prior art across closed/rejected work — `scripts/review-branch`, its gate log, `scripts/pdca`, and the prior-attempt artifacts were not supplied, so only merged history (nearest affected-path commit `3e05891`) was independently checkable. |
| T5 Judgment | PASS | The five legs distinguish continuation, refusal-without-certification, per-object audit attribution, non-overcontainment, and propagation of an underlying store fault, with explicit `Ok(...)` assertions and fixture-health checks at `crates/custodian/tests/segmented_map_rebalance.rs:313`, `crates/custodian/tests/segmented_map_rebalance.rs:378`, and `crates/custodian/tests/segmented_map_rebalance.rs:408`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept or reject `Blocked` plus named audit attribution as the interim operator contract until #682 — a healthy segmented object that still places a fragment on the draining server continues to make the drain query return bare `Pending` at `crates/custodian/src/desired_state.rs:195`, so an audit-unaware operator can still see an apparently active evacuation that cannot finish. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide the disposition of the reported one-blocker batched review and confirm affected-path prior art across closed/rejected work — `scripts/review-branch`, its gate log, `scripts/pdca`, and the prior-attempt artifacts were not supplied, so only merged history (nearest affected-path commit `3e05891`) was independently checkable.
- [ ] Validation — fitness-to-purpose — Accept or reject `Blocked` plus named audit attribution as the interim operator contract until #682 — a healthy segmented object that still places a fragment on the draining server continues to make the drain query return bare `Pending` at `crates/custodian/src/desired_state.rs:195`, so an audit-unaware operator can still see an apparently active evacuation that cannot finish.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide the disposition of the reported one-blocker batched review and confirm affected-path prior art across closed/rejected work — `scripts/review-branch`, its gate log, `scripts/pdca`, and the prior-attempt artifacts were not supplied, so only merged history (nearest affected-path commit `3e05891`) was independently checkable.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
