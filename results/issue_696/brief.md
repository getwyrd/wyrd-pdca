# Brief — issue 696 / rebalance-reads-through-resolver-contained

> Child 2 of 3 from the #681 split; siblings **#695** (backfill) and **#697** (reconstruction)
> touch disjoint files. **#682 depends on this one.**
>
> **Re-planned 2026-08-07 after five rejected rounds.** Deliberately SMALLER than the brief it
> replaces (`iteration-v5/brief.md`): two rules that one carried — a generation-restart comparison
> ("Rule A") and a key-identity predicate ("Rule C") — are **out of this slice**, and the lines they
> touched are **frozen at the base**. A third recurring finding (a malformed placement not
> withholding certification) is answered by freezing the base arm and citing the design that already
> covers it. §Scope says why; the closing table is the evidence. This mirrors the #695 re-plan of the
> same day, whose five rounds carried the identical signature.

- **Slug:** rebalance-reads-through-resolver-contained
- **Defect:** `crates/custodian/src/rebalance.rs` reads the chunk map inline out of the inode record
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
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_rebalance.rs` passes,
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, **no DST leg**. Verified at Plan, not assumed: `main == origin/main == 339da46`;
  the `--classify` dry-run on a synthetic patch listing exactly `crates/custodian/src/rebalance.rs`
  + the new test returns `ADDED_TEST crates/custodian/tests/segmented_map_rebalance.rs` and `CRATE
  crates/custodian`, so the green leg is `cargo test -p wyrd-custodian --test
  segmented_map_rebalance` and the red leg reverts `rebalance.rs` while keeping the test
  (`engine/scripts/run-verify.sh:250-257`, `:454`, `:466-470`). No `crates/custodian/tests/*.rs`
  carries a crate-level `#![cfg(...)]` (grepped on the base), so neither zero-test guard (`:458`,
  exit 77) can trip. `_resolve_base_ref` (`run-verify.sh:242-247`) honours `$PDCA_BASE` →
  `$PDCA_VERIFY_BASE` → `$WYRD_VERIFY_BASE` → the brief's base; this bundle is wave 0 with no
  `Onto branch`, so the base is `origin/main` — the same ref the PR opens against.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*, `:137`), over
  **the maintenance pass that executes a drain**: it reads every committed object the way every
  other consumer reads it; a fault it meets is contained to the object that owns it and the answer
  still gets made for the rest; and it never reports a drain satisfied over an evacuation it did not
  perform — an operator reading `Satisfied` is being told the server is safe to decommission, and
  will act on it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with #695 and #697.** Touches
  `crates/custodian/src/rebalance.rs` plus one new test file; neither sibling touches either (#695 is
  `backfill.rs`, #697 is `reconstruction.rs`), so no `Conflicts with:` is owed even though all three
  may build against the same base at once. Every code prerequisite is already merged on the base
  (#649's `resolve_chunk_map`, #650's `Reconciled::Blocked` and the GC containment precedent, #651's
  restore precedent). **#682 depends on this child** and lands after it.
- **Surfaces:** data
- **Difficulty:** medium   (one production file, two call-sites; the answer change propagates through
  `reconcile_step`'s `least_certified` fold, `reconciliation.rs:55-61`, and a wrong containment
  predicate here silently withholds every decommission — which is why leg 4 is mandatory.)
- **Scope:** rebalance **reads every committed object through the resolver every other consumer
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
- **Budget:** **exactly 2 files.** `src/rebalance.rs` ≤ **85** added semantic lines (non-blank,
  non-comment); `tests/segmented_map_rebalance.rs` ≤ **265 semantic / 440 raw**. Calibration,
  measured at Plan on `iteration-v5/patch.diff`: that rejected patch spent **94** production
  semantic lines *including* Rules A and C, so the core alone sits inside 85; and **329 semantic /
  507 raw** of test for **seven** legs, where this brief asks five (v5's legs 2 and 6 merge into leg
  2 here, its leg 4 goes with Rule A, and its Rule C sub-assertion goes with Rule C). The test cap is
  above #695's because this fixture also carries a fleet of `ChunkStore` doubles, a `Topology`,
  desired-state seeding and the fence — `rebalance::reconcile` is `pub(crate)`. A **third file**, a
  `crates/dst/` hunk, or a test file past 440 raw means the shape is wrong: **STOP and hand back
  rather than finish.** Compression rules: ONE `BTreeMap`-backed metadata double carrying the
  injected `get` fault leg 5 needs; ONE parameterised seeding helper; ONE audit-capture helper shared
  by legs 2 and 3.
- **Repro instruction:** on the target checkout, `git -C ../wyrd show
  origin/main:crates/custodian/src/rebalance.rs` at `:158-164` and `:255-261`. With any D server
  marked draining, seeding **any** `seg:`-backed committed root — or any committed record that will
  not decode — makes `rebalance::reconcile` return `Err` for the whole store, so no flat chunk
  anywhere is evacuated either. The seeding shape to copy is `seed_segmented` at
  `crates/custodian/tests/segmented_map_restore.rs:387-410`, and `seed_damaged` at `:417-431`, which
  asserts its own fixture is genuinely unreadable.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, each re-run and OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_rebalance.rs` — a **NEW** file, not optional
  and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:250-252`, `:350-351`); appending to `segmented_map_consumers.rs` or
  `segmented_map_restore.rs` makes it a *modified* file, the gate takes the green-only branch
  (`:454`, `:466-470`) and proves no red at all. Confirmed by the `--classify` dry-run at Plan. The
  name completes the family (`…_consumers.rs` #650, `…_restore.rs` #651, `…_backfill.rs` #695).
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Pre-declared so it arrives at sign-off settled rather than as a surprise: **no seeded
  Tier-0 DST case ships in this child, and none is owed.** The repo rubric asks a *new concurrent or
  destructive path* for seeded Tier-0 coverage; this slice adds neither. Every write it performs is
  on a flat chunk resolved by borrow from the generation the scan returned
  (`crates/core/src/metadata.rs:2585` — a flat snapshot reads nothing and can never be superseded),
  committed under the base's own unmodified version-conditional CAS (`rebalance.rs:310-313`); the
  segmented side performs a refusal, which writes nothing (leg 2 asserts the `seg:` bytes and the
  root's `version` are byte-identical afterwards). What this patch adds at the fault sites is a
  `continue`, never a write. A review finding asking for a DST leg here is **recorded-rejected** in
  `review-rejected.md` with that reason, citing `metadata.rs:2585` and `:2629` and the carve-out
  **#699** — it is not fixed by adding one, and adding one puts the bundle over budget and out of
  scope. The same applies to a finding asking that a malformed placement withhold certification: the
  reference is the three-part carve-out in §Scope, and #690 for the restore-side analogue.
- **Production reach:** production **does** traverse this seam at Check — every leg drives the real
  fenced `reconcile_step`, and only the `MetadataStore` / `ChunkStore` backends are doubles, which is
  how every existing custodian suite is built. One residual is declared here rather than discovered
  at sign-off, because **all five previous rounds raised it as a "Validation — fitness-to-purpose"
  NEEDS-HUMAN** (5/5 — the most repeated item on this bundle):

  **What the operator sees for a refused segmented object is the loop's answer, not the drain
  query's.** This slice makes the pass answer `Reconciled::Blocked` and name the object on
  `wyrd.custodian.rebalance.audit`. It does **not** change
  `desired_state::reconciliation_status`, which for a *healthy* segmented object holding a fragment
  on the draining server still answers a bare `ReconciliationStatus::Pending` (`desired_state.rs:188-196`
  — the fragment is a genuine, resolvable reference, so `genuinely_holds` is true). So an operator
  watching only the drain query is told "an evacuation is running", which will not finish until
  **#682** builds the segmented write path. That is a real residual, and it is deliberately not
  fixed here: `desired_state.rs` is out of scope, and closing it means deciding what the query should
  say about work that is refused-pending-#682 — #682's call, with the repair surface at **#694**.

  Net against the base this is still strictly better in the direction that matters: today a single
  segmented object makes the whole pass return `Err`, so **no** server in the store drains at all and
  nothing is named; after this slice every flat evacuation proceeds, the blocker is named per object
  on the audit seam, and the loop refuses to certify. The sign-off question is therefore "is
  `Blocked` + named audit attribution the right operator contract **until #682**" — pre-declared
  here, not a surprise.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice: mirror the two merged peers rather than invent.** Peer callsites Do
  MAY open:
  * `crates/custodian/src/restore.rs:621-660` — **the closest peer and primary model** (#651, merged
    `8decc93`): the same walk over the same namespace — decode contained (`:631-637`), state checked
    (`:638-640`), `Ok(None)` skipped (`:646`), and the `ChunkMapError` downcast rule (`:647-657`).
  * `crates/custodian/src/gc.rs:360-416` — the same walk a second time (#650), the downcast rule
    stated in full at `:402-416`. **Contain by exactly this rule and no other:** `Ok(fault)` is
    contained as *this record's* fault, `Err(err)` propagates because a store fault is not one
    object's. That is leg 5.
  * `crates/custodian/src/gc.rs:155-166` — attribution emitted **per object, before the work loop**,
    and the comment saying why; `:470-480` for `object_name`'s injective escaping. Mirror the
    placement, not just the call.
  * `crates/custodian/src/gc.rs:234-246` — the refusal to certify over an incomplete reading, and why
    it is not `Satisfied`. Reuse this shape.
  * `crates/custodian/src/gc.rs:563-571` — `emit_unresolvable`: the exact action string, counter and
    message shape the vocabulary above reuses.
  * `crates/custodian/src/reconciliation.rs:44` + `:55-61` — `Reconciled::Blocked` and
    `least_certified`. Reuse this vocabulary; do not invent a parallel outcome.
  * `crates/core/src/metadata.rs:2579-2587` (a flat map resolves by borrow and never restarts),
    `:2619-2632` (`resolve_chunk_map`'s three arms) and `:2266-2272` (`ResolvedChunkMap`) — the
    §Scope constraint lives here.
  * `crates/custodian/tests/segmented_map_consumers.rs:85-116` — the `BTreeMap`-backed `MemMeta`
    whose ordering makes "the damaged record is met FIRST" a fixture property rather than luck (its
    `scan_page` delegates to `wyrd_testkit::test_double_scan_page`, `:109-116`); `:406-410` for the
    `Custodian::elect` + `FencedZone::new` fence shape every leg needs.
  * `crates/custodian/tests/segmented_map_restore.rs:221-248` (the `Capture` `MakeWriter`) and
    **`:250-264` (`enable_audit_callsites`)** — read the second one before writing legs 2 and 3.
    `tracing` latches each callsite's `Interest` process-globally the first time it is hit, so
    without that `Once` a sibling test in the same binary can disable the audit callsite for the
    whole process and leave the capture **empty** (wyrd #214) — a silently vacuous assertion in
    exactly the legs that assert on the audit stream. `:266-270` is the capturing dispatch.

  **Salvage, carefully.** `results/issue_696/iteration-v5/patch.diff` is the rejected fifth attempt;
  its containment core passed C4-ci, C4-verify red→green over 7 tests and C5 with 0 survivors, and
  its module-doc and emitter wording are reusable. It also carries Rules A and C, **both out of scope
  here**. Reference, not a starting diff to subtract from — the peers above are the positive model.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/rebalance.rs` → the nearest is `3e05891` (#648 — the segmented record shape,
  which **created** these two sites); unchanged since. No open PR touches this file. Prior attempts:
  seven on the un-split #681 (`results/issue_681/iteration-v1..v7/`) and five here
  (`results/issue_696/iteration-v1..v5/`); the standing recorded-rejections are in
  `results/issue_696/review-rejected.md` and their reasons still hold — do not re-litigate them.
- **Disposition hint:** likely-fix

## What five rounds measured (why this brief is smaller, not different)

Read off `iteration-v*/gate-logs/T4-batch-review.log` and each round's SUMMARY §6, not from memory.

| Round | Blocking findings | Traces to |
|---|---|---|
| v1 | `src:148` aborted evacuation not counted as a refusal; `tests:379` Tier-0 DST for the stale-generation path; C5 3 surviving mutants on the `unreadable` arithmetic/predicate | the refusal-accounting arm + **Rule A** |
| v2 | `src:153` + `src:310` malformed placement emits `needs-human` without incrementing `refused`; T5 "leg 5 must be genuinely non-base-red" | the malformed arm + **the leg-5 red/non-red contradiction** |
| v3 | `tests:168` + `tests:199` the `certifies` helper silently accepts every `Err`; `src:259` + `tests:400` Tier-0 DST for the supersession path | the test helper + **Rule A** |
| v4 | `src:243` removing `parse_inode_key` makes `inode:foo` CAS-eligible; `src:140` Tier-0 DST for partial progress; C1 + T5 on leg 5 | **Rule C** + **Rule A** + the leg-5 contradiction |
| v5 | `src:223` a segmented object of only malformed placements increments no refusal; `tests:432` Tier-0 DST; C1 on leg 5 | the malformed arm + **Rule A** + the leg-5 contradiction |

Tallied: **Rule A / the Tier-0 DST ask blocked 4 of 5 rounds**; the malformed-placement
refusal-accounting arm 3 of 5; the leg-5 red/non-red contradiction 3 of 5; Rule C 1; the `certifies`
helper 1. **Not one finding landed on the containment core.** v5's gates were otherwise all green —
`C4-ci` pass, `C4-verify` red→green over 7 tests, `C5` 11 caught / 10 unviable / **0 surviving**,
reviewer PASS on C2–C5 / T1 / T2 / T3 / T5 — with the **gating** `T4-batch-review` the only failure.

Every one of those five causes is removed or pre-settled by this brief: Rule A's path is closed by
construction, the malformed arm and Rule C's lines are frozen at the base, the leg-5 contradiction is
resolved in the only direction the code allows, and the `certifies` trap is named above. Sibling
#695 shows the identical signature and was re-planned the same way on 2026-08-07; #697 is still to
follow.

So: one real defect carrying two unrelated hardening rules that each kept re-opening, plus one
finding against an arm the patch never needed to touch. The rules are **removed, not redistributed** —
Rule A's path is closed by construction, Rule C's lines are frozen at the base as a known,
unreachable, pre-existing hazard, and the malformed arm is frozen because the base already answers it
correctly at the surface that matters. Neither carve-out is dropped: both were filed at Plan of #695
as **#699** and **#698** (milestone *Foundations*), each carrying its evidence and the question it has
to settle — so a reviewer who raises either has a tracker reference to be pointed at rather than a
rebuild to trigger.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide the disposition of the reported one-blocker batched review and confirm affected-path prior art across closed/rejected work — `scripts/review-branch`, its gate log, `scripts/pdca`, and the prior-attempt artifacts were not supplied, so only merged history (nearest affected-path commit `3e05891`) was independently checkable.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_696/review-b
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the contribution and prior-art disposition is sufficient — merged affected-path history confirms `3e05891` as the nearest change, but the closed/rejected-work artifacts and `review-branch`/`scripts/pdca` tools were not supplied, so those asserted pass rows cannot be independently replayed against `crates/custodian/src/rebalance.rs:1`..
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the T5 Judgment test gap: the unreadable-object leg must assert the `rebalance_unresolvable_records` counter specifically (not just a generic action/name substring), so that removing/breaking that counter would fail the test (crates/custodian/src/rebalance.rs:521, crates/custodian/tests/segmented_map_rebalance.rs:374). T4-contribution, fitness-to-purpose, and the size-backstop items remain open for the next sign-off pass.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
