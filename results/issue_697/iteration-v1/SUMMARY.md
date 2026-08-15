# Result — issue 697 / reconstruction-reads-through-resolver-once-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `crates/custodian/src/reconstruction.rs` reads the chunk map inline out of the inode
  record at **three** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`,
  re-verified on `origin/main @ 339da46`:

  | Site | Function | What its `?` ends |
  |---|---|---|
  | `crates/custodian/src/reconstruction.rs:332` | `assess` (`:317`) | one obligation's assessment, and the pass |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | the binding repair commit |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` (`:620`) | the whole `inode:` scan, for every obligation |

  Three live consequences beyond the shared one (a single segmented object stops repair for the
  whole store, so a store that has published one multipart object stops restoring redundancy):
  1. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted.** `assess` reads `find_chunk` returning `None` as "referenced by no committed chunk
     map" → `Assessment::Drain` (`:322-325`), deleted in the drain batch (`:270-276`). Latent today
     (the error fires first), but it is the loss this child must not introduce while removing the
     abort — the last record saying live data is under-replicated.
  2. **The deployed repair loop is Q namespace scans × N point reads.** `reconcile` calls `assess`
     per obligation (`:185`); `assess` calls `find_chunk` (`:322`), which scans all of `inode:` and
     decodes every record. Wiring the resolver in naively makes each of those N objects also cost a
     bounded `seg:` range read — Q×N *resolves*. **This is the finding still open at #647's close.**
  3. **Containment is not per object** — a record that will not `decode` ends the walk at `:625`,
     before any resolver is involved.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_reconstruction.rs`
  passes, driven only through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}`,
  `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}`, plus `Custodian::elect` +
  `FencedZone::new` over `wyrd_coordination_mem::MemCoordination` for the fence (`leadership.rs:31`,
  `:69`; the shape is `segmented_map_consumers.rs:406-410`) — over in-memory `MetadataStore` /
  `ChunkStore` doubles. Each leg drives `reconcile_step`, the real fenced control point, not an
  internal helper. **The discriminator MUST NOT name any symbol this patch introduces**: the red leg
  reverts production, so such a reference makes the target fail to compile and the red degrades to
  UNVERIFIABLE (exit 77). `Reconciled::Blocked` already exists on the base
  (`reconciliation.rs:44`) and may be named.

  **Eight legs over ONE shared fixture** — one store, one seeding helper, one counted metadata
  double:

  1. **A segmented object no longer ends the pass, and the flat work in the same store still
     happens.** One healthy segmented object (raw `seg:` records + a segmented root, **never** a
     committer) beside an under-replicated **flat** chunk with a queued repair: `reconcile_step`
     with a `ReconstructionContext` returns `Ok` (today `Err`), the flat chunk's placement moves,
     and its obligation is drained. *(binding — base-red)*
  2. **A repair for a chunk in a `seg:` record is refused, never discarded, and the pass does not
     certify.** That obligation is **still in `queued_repairs`** afterwards; the `seg:` record's
     bytes and the root's `version` are **byte-identical**; the refusal carries a stated reason and
     a counted gauge; the pass answers `Reconciled::Blocked`. *(binding — base-red; this is
     defect 1 above, the loss this child must not introduce)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store — (a) a committed root naming a
     `SegmentRef` whose `seg:` record was never written, and (b) a committed record whose own bytes
     will not `decode`; assert in the fixture that `resolve_chunk_map` really errors on (a). Beside
     them, a healthy flat chunk carrying the same work as leg 1. Assert the conjunction: `Ok`,
     `Blocked` (never `Satisfied`), **the healthy object's repair still happens**, no obligation is
     drained for want of a reading, and both damaged objects are **named** by their `inode:` key
     (`gc::object_name`'s escaping shape, `gc.rs:470`). *(binding — base-red)*
  4. **Rule A — the pass never writes to a generation it did not read.** A metadata double whose
     `scan` answers a **stale segmented** root while `get` answers a **live flat** root placing an
     under-replicated chunk: `reconcile_step` returns `Ok`, repairs **nothing**, leaves the live
     record's `version` unchanged, keeps the obligation queued, and answers `Blocked`.
     *(binding — base-red; this is the leg whose absence let four review rounds re-open the same
     question, and it is the same fact as round 7's T4 blocker at `reconstruction.rs:659` **on the
     v7 tree** — these line numbers index `iteration-v7/patch.diff`, NOT the base)*
  5. **A duplicate committed `ChunkId` is repaired by neither reference, and both objects are
     named.** Two committed objects — and, in the same store, one record carrying the id twice —
     referencing one `ChunkId`, with that chunk enqueued for repair: neither placement changes, the
     obligation is **still queued**, both `inode:` keys are named, and the pass answers `Blocked`.
     Today the base repairs whichever reference `find_chunk` meets first (`:639`) and drains the
     obligation. **≤ 40 lines, reusing the shared fixture.** *(binding — base-red)*
  6. **The namespace is read ONCE per pass — O(N), not O(Q×N).** With **Q ≥ 3** queued obligations
     over **N ≥ 3** committed flat objects on the counted double: exactly **one**
     `scan(b"inode:")`, independent of Q (the base does Q of them), and the repairs still land.
     Then, on a store holding S segmented objects, the `seg:` range reads are **≤ S**. Build this
     leg with a `ReconstructionContext` and **no** GC / scrub / rebalance context beside it: the
     other loops walk `inode:` themselves and sharing one walk across passes is a much larger
     refactor that is **out of scope** — a store-wide scan count would demand it by the back door.
     *(binding — base-red; this closes the finding left open at #647)*
  7. **An empty queue performs no reading and answers `Satisfied`.** Over the store that already
     holds an unreadable object, run reconstruction with **no** obligations queued: `Satisfied`, and
     the counted double records **zero** `scan(b"inode:")`. *(NOT base-red, verified at Plan and
     stated rather than assumed: the base's loop is `for chunk in queue { assess(..) }`
     (`reconstruction.rs:185`), so an empty queue already scans zero times and already answers
     `Satisfied`. This leg is a REGRESSION guard on the restructure — leg 6 moves the reading OUT
     of the per-obligation loop, and the obvious way to do that is to read the namespace once
     unconditionally at the top of the pass, which would silently break this property and make the
     pass claim over objects it never needed to read. Required for that reason.)*
  8. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes the pass return `Err`, and the unreadable
     object's name is **already** on the audit seam even though the pass returns `Err` (Rule E).
     *(NOT base-red — this guards against over-containment; it passes before and after. Required
     despite that: without it, containing EVERY error would pass every other leg. Legs 7 and 8 are
     this child's two non-red legs and both are deliberate.)*

  Also assert **Rule C** as a sub-assertion of leg 3 (**≤ ~20 lines**, no ninth test): a committed
  record under `inode:007` beside `inode:7` leaves `inode:7` **byte-unchanged** and is never
  silently reinterpreted into it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: reconstruction **reads each committed object's map once per pass, assesses its queued
  obligations against that reading, contains per object what it cannot read, and refuses — rather
  than aborts or silently drains — the repair it does not own.** A chunk whose `ChunkRef` lives in
  a `seg:` record is **refused, not drained**: the obligation stays queued and the pass does not
  certify. A chunk in a flat record is repaired exactly as today, with **every** existing
  classification (`Repairable` / `Drain` / `Unreachable` / `Blocked` / `Unrepairable` / `Malformed`)
  and its gauge accounting preserved, **including the rule that a never-repaired condition stays
  off the repairable-backlog gauge**.

  **Rules, pinned at Plan — do not relitigate; each is bound by a leg above.**
  1. **Rule A — a pass acts only on a resolve that did not restart.** `resolve_chunk_map` restarts
     onto the live root when the caller's **segmented** snapshot was superseded
     (`metadata.rs:2338-2339`) and then answers a `ResolvedChunkMap` whose `record` is a generation
     the pass never scanned. Mixing that generation's chunk list with the snapshot's `prior_bytes`
     is exactly what makes unchecked indexing panic before the stale CAS can reject the plan —
     round 7's T4 blocker at `reconstruction.rs:659` (a **v7-tree** line, not a base line). If the
     resolve did not answer from the
     scanned generation, contain the object, repair nothing, keep the obligation queued, answer
     `Blocked`, re-read next pass — the resolver hands back the generation it resolved FROM — `ResolvedChunkMap.record`, a
     `Cow` carrying its own `version` (`metadata.rs:2256-2272`) — so the pass is able to tell
     the two apart; **how it tells is Do's to choose**. Bounded per C-1: an object whose shape
     changed under the scan is re-assessed on the next pass **because the obligation stays queued**.
     Leg 4 binds it. *(The previous brief pinned this as "unreachable by construction" and forbade
     a test; it was demonstrated false with a working double at
     `results/issue_681/iteration-v4/check-advisory-adversary.md:25`.)*
  2. **Rule B — an incomplete reading changes what the pass may CLAIM and what it may DISCARD,
     never what it may DO for the objects it read successfully.** A pass that met an unreadable
     object answers `Blocked` and **never drains an obligation**, but still repairs the healthy
     objects in the same store. Verified safe rather than argued: `repair_chunk` orphan-**marks**
     the displaced fragment (`reconstruction.rs:665-671` on the v7 tree), it never deletes it, and
     GC reclaims a marked fragment only past `ReferenceSet::protection`, which returns
     `incomplete-reference-set` and withholds **every** fragment while any object is unresolvable
     (`gc.rs:306-316`, consulted before every delete at `:191-194`); the displaced fragment is also,
     by construction, one the assessment proved MISSING or checksum-failing at its placed server.
     So the loss chain cannot close and strictness buys **no** safety, while costing every healthy
     object its repair. Leg 3 pins the progress half; legs 2 and 3 pin the never-discard half.
  3. **A pass certifies only over the reading it performed.** With an **empty** repair queue the
     pass performs no namespace reading and answers `Satisfied` — it makes no claim about objects it
     never read. With a non-empty queue it DOES read the namespace, and an object it cannot read
     then makes that reading incomplete → `Blocked`, because "drain this obligation as unreferenced"
     is only knowable over a complete reading. Precedent in this file family:
     `rebalance.rs:115-117` already returns `Satisfied` without reading `inode:` when no server is
     draining. Leg 7 binds it.
  4. **A duplicate committed `ChunkId` is ambiguous: neither reference is repaired, the obligation
     stays queued, both objects are named, the pass does not certify — and the rule is the same
     whether the duplicates sit in one record or two.** An obligation is keyed by chunk alone
     (`repair:<chunk_id>`, `repair.rs:32`) and both references address the same
     `FragmentId{chunk,index}`, so repairing "the first in scan order" repoints one record while
     orphan-marking a fragment the other still points at. Ids are allocator-minted
     (`write.rs:170`), never content-addressed, so a duplicate is always an anomaly, never
     legitimate dedup. **This is the narrow rule ONLY:** no cross-object claim-counting apparatus,
     no new report schema, no `ambiguous-*` verdict surface. Leg 5 binds it.
  5. **Rule C — read, write and name a record under exactly the key the store gave it.** On the
     base the pass parses the scanned key to an `InodeId` then re-derives `metadata::inode_key(id)`
     for the CAS (`reconstruction.rs:598`); `"inode:007"` and `"inode:+3"` both parse, so the pass
     reads one record and CASes another. Precedent: `gc.rs:280-294`, `:402`.
  6. **Rule D — a refusal is reported once per object, not once per chunk.**
  7. **Rule E — attribution for an object the pass could not read is emitted where the object is
     read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
     cannot cost the operator the name of the record to repair. **Load-bearing, not logging
     hygiene:** a genuinely corrupt root has no repair path (a fragment carries only
     `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`) and no operator tooling
     (tracked as **#694**), and reclamation is halted store-wide meanwhile — that name is the
     operator's entire situational awareness. Leg 8 binds the placement.

  **Constraints (they bound the shape; they do not name it):** bounded memory — work proportional
  to the **obligations held** and to **one object at a time**, never the whole namespace's decoded
  chunk lists and never any segment's exact bytes; **one** resolving reading of the namespace per
  pass; containment on **any** read fault by exactly gc.rs's downcast rule (`gc.rs:402-416`) —
  `Ok(ChunkMapError)` is contained as *this record's* fault, any other error propagates because a
  store fault is not one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the repair write
    path for a `seg:`-resident chunk are **#682**. A refusal writes **nothing at all**.
  - `backfill.rs` and `rebalance.rs` — **the two sibling children**. Do not touch them; a diff that
    does will conflict with a bundle building in the same wave.
  - **Sharing ONE namespace walk across all loops** (GC / scrub / rebalance / reconstruction) — a
    separate refactor, explicitly not this child's. Leg 6 is scoped to a reconstruction-only context
    for exactly this reason.
  - `gc.rs`, `scrub.rs` (#650), `restore.rs`, `desired_state.rs` (#651) — untouched; **leave** the
    deferral marker at `restore.rs:616` (its per-reference granularity differs, as that comment
    says).
  - The chunk-id floor (#652); the committer, fence, rollback and resume (#653); the M8 operator
    surface (#694).
  - The existing suite `crates/custodian/tests/reconstruction.rs` must stay green **unmodified** —
    v2 achieved that with these same production changes, so a need to edit it signals an answer
    changed further than intended, not a licence to edit it.
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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 26 mutants tested in 30s: 12 caught, 14 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #697's reconstruction change: resolve committed maps once per non-empty pass, contain unreadable objects, refuse non-owned or ambiguous repairs, and preserve flat repair progress.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | FAIL | The declared red partition is internally false: leg 8 is specified as base-green although its required store-fault/audit outcome cannot survive the base walk's earlier decode abort at `crates/custodian/tests/segmented_map_reconstruction.rs:655`. |
| C2 Reproduction (red pre-fix) | FAIL | The independent base run compiled but produced 7 failing and 1 passing test, not the asserted six-red/two-green partition; isolated leg 8 also exited 101 at `crates/custodian/tests/segmented_map_reconstruction.rs:655`. |
| C3 Change | FAIL | Rule D is not delivered: refusal reporting occurs inside the per-obligation queue loop at `crates/custodian/src/reconstruction.rs:209` and `crates/custodian/src/reconstruction.rs:254`, so one object can be reported once per queued chunk. |
| C4 Verification (red→green) | PASS | Restoring production made all 8 new tests pass, and an isolated writable-cache rerun completed full `cargo xtask ci`; the behavioral discriminator enters through `crates/custodian/tests/segmented_map_reconstruction.rs:410`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Whether refusal accounting is truly per object must be re-established with a multi-obligation fixture — the current one-obligation refusal leg at `crates/custodian/tests/segmented_map_reconstruction.rs:434` cannot expose the Rule D violation. |
| T1 Structure | PASS | The patch is confined to the expected production file and one new integration test that drives the public fenced control point at `crates/custodian/tests/segmented_map_reconstruction.rs:375`. |
| T2 Shape | FAIL | The new test reaches 657 raw lines at `crates/custodian/tests/segmented_map_reconstruction.rs:657`, exceeding the brief's hard 620-line ceiling and therefore requiring the prescribed STOP/restructure. |
| T3 Runtime | FAIL | A segmented object with multiple queued chunks emits multiple refusal counters and audit rows through `crates/custodian/src/reconstruction.rs:1037`, inflating the operator signal that Rule D defines per object. |
| T4 Contribution | FAIL | Contribution sign-off is premature while the T2/T3 defects remain; affected-path prior art found closed #647 and no open overlap, while the declared `scripts/pdca contribcheck` runner was unavailable for an independent rerun. |
| T5 Judgment | NEEDS-HUMAN [impl] | Restore direct oracles before rebuild sign-off — leg 1's queue-drain assertion is commented out at `crates/custodian/tests/segmented_map_reconstruction.rs:427`, and leg 5 checks only index 0 rather than the duplicate second placement at `crates/custodian/tests/segmented_map_reconstruction.rs:571`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide production fitness and whether Tier-1 disk-fault or Tier-2 kill-and-reconstruct observation is warranted — CI/DST are green, but operational reconstruction risk remains a human sign-off decision. |

### Advisory — adversary

# Adversarial review — issue #697 (advisory, non-gating)

Toolchain was available; the red→green was re-run in a scratch copy of `$PDCA_TARGET`
(`cargo 1.96.0`), green with the patch (8/8 pass) and red with `crates/custodian/src/reconstruction.rs`
reverted to `origin/main @ 339da46` (**7** of 8 fail). Findings below are grounded on the target tree.

## Refutations that landed

- **NEEDS-HUMAN [impl]** — `crates/custodian/tests/segmented_map_reconstruction.rs:427`: leg 1's
  binding sub-assertion is **swallowed by a comment and never executes**. The line reads
  `    // referenced by nothing — which only a COMPLETE reading may answer (leg 3 is the other side).    assert!(store.queued().await.is_empty(), "both discharged");`
  — the `assert!` sits on the same physical line, after `//`. The brief's leg 1 (brief.md:52-55)
  requires "the flat chunk's placement moves, **and its obligation is drained**"; the drain half is
  unasserted, so leg 1 as shipped would stay green against a regression that stopped draining
  `C_UNSEEN`. Verified: restoring the assertion onto its own line still passes with the fix, so the
  property holds and the fix is fine — the **test** is the defect. Neither `cargo fmt` nor clippy can
  see this (rustfmt does not reflow comments), so C4-ci's green says nothing about it.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/reconstruction.rs:253-256` (+ `:1035`
  `emit_refused`): **Rule D is not implemented.** brief.md:202 pins, do-not-relitigate, "*Rule D — a
  refusal is reported once per **object**, not once per chunk*". `emit_refused` is called from inside
  `for &chunk in &queue`, i.e. once per **obligation**. Measured concrete case (probe driven through
  `reconcile_step` over the patch's own fixture): one segmented object `inode:006` holding two queued
  chunks (`S_A`, `S_B`) emits **2** `action=refused` audit lines, **both** carrying
  `"inode":"inode:006"`, and increments `monotonic_counter.reconstruction_repair_refused` **twice**.
  A real multipart object whose `seg:` records hold Q queued chunks therefore floods the durability
  seam with Q copies of one object's refusal and makes the counter measure obligations rather than
  objects. No leg binds Rule D (the brief claims "each is bound by a leg above" at brief.md:155 —
  that claim is unwarranted for rule 6), which is why nothing caught it.

- **NEEDS-HUMAN [human]** — **the test file busts the brief's hard STOP budget.**
  `crates/custodian/tests/segmented_map_reconstruction.rs` is **657 raw** lines (`wc -l`; the diff
  header is `@@ -0,0 +1,657 @@`) and ~**474 semantic** (non-blank, non-comment). brief.md:238-247
  caps it at "**≤ 380 semantic / 620 raw**" and states: "*a test file past 620 raw means the shape is
  wrong: STOP and hand back rather than finish*". Production is within budget (189 added semantic vs
  the 230 cap), so this is purely the discriminator. `check-gates.json` carries **no** budget row and
  the reviewer's verdict does not mention it, so the STOP condition the Plan wrote was silently
  skipped rather than adjudicated.

- **NEEDS-HUMAN [impl]** — brief.md:97-102 / :114-116 declare leg 8
  (`a_fault_that_is_not_one_objects_map_still_ends_the_pass`) **not base-red** — "*it passes before
  and after*", "*legs 7 and 8 are declared non-red*". **Measured false.** On `339da46` with
  production reverted, leg 8 FAILS:
  `the pass absorbed it: reconciliation store access: key must be a string at line 1 column 2`.
  Its fixture seeds `seed_damaged()` first, so on the base the pass aborts at the undecodable
  `inode:0` record long before the injected `get` fault is reachable — the leg goes red for a reason
  that has nothing to do with the over-containment property it exists to guard. Exactly 7 of 8 legs
  are red on the base, not the 6 the brief predicts. Either the pre-declaration or leg 8's fixture
  (drop `seed_damaged()`, or assert only the store-fault half) needs correcting.

- **NEEDS-HUMAN [impl]** — `check-gates.json:48`'s C4-verify evidence line, "*red without the fix,
  green with it (**8 test(s) ran red**)*", reads as "8 legs discriminate" but
  `engine/scripts/run-verify.sh:508` interpolates `$TESTS_RAN` — the number of tests that **ran**,
  not that **failed**. Measured: 7 failed, 1 passed
  (`an_empty_queue_reads_nothing_and_certifies`, correctly non-red by brief.md:88-96). The row is not
  wrong about the gate's verdict, but it is not evidence for the per-leg red the brief asserts, and
  it is the only red→green artifact in the bundle. Worth one line at sign-off so the "8" is not read
  as a per-leg count.

## Attacked and could not refute

- **Rule A containment** (`reconstruction.rs:837`, value-compare `resolved.record` against the
  scanned record). I tried to get a repoint through on a generation the pass never scanned, and I
  tried the quieter sibling: the `Ok(None) => continue` arm at `reconstruction.rs:813`, hoping a
  `seg:`-resident obligation would be **drained** (violating leg 2's "refused, never discarded")
  when the root is retired under the resolve. It does not reach: `root_dropped`
  (`crates/core/src/metadata.rs:2323-2340`) compares only the segment **group**, so a Pending
  overwrite sharing the group is not "dropped" at all (my probe came back `Blocked`, obligation
  kept, reason `segmented-chunk-map`); and where `Ok(None)` genuinely fires the object really has no
  live committed generation, so the drain is the correct answer. Flat records never take this arm
  (`metadata.rs:2585` answers `Cow::Borrowed`), so Rule A is trivially satisfied for every record
  this slice writes to.
- **The Q×N discriminator.** In the shipped file leg 6 goes red on the base at
  `assert_eq!(got, Reconciled::Changed)` (base answers `Satisfied`, because the fixture's `stored()`
  spelling defeats the base's `require(key, encode(&prior))` CAS) — so the headline scan-count
  assertion is *masked* and I expected it to be untested. It is not: relaxing that earlier assertion
  and re-running on `339da46` gives `namespace == 3` vs `1` at
  `segmented_map_reconstruction.rs:606`. The #647 property is a genuine discriminator.
- **`repair_chunk`'s new guards** (`reconstruction.rs:653-669`): I could not construct a
  `prior_bytes` whose `chunk_index` is in range but names a different chunk, nor a non-flat one —
  `hits` is enumerated off the same generation and the id filter is a second guard on the same fact.
- **Rule C** (`reconstruction.rs:676` CASes on the raw scan key): `inode:007` beside `inode:7` is
  correctly separated, and I could not find a spelling that reads one and commits the other.
- **The new `parse_inode_key` containment** (`reconstruction.rs:805`) over-contains relative to GC
  (`gc.rs:360-455` has no such check), so a stray key under the `inode:` prefix would pin the pass at
  `Blocked` and block every drain forever. Not raised as a defect: `metadata::inode_key`
  (`crates/core/src/metadata.rs:35`) is `format!("inode:{id}")`, so no production writer can mint
  such a key, and the direction is fail-closed.
- **Memory / bounded work**: retention is one `Arc<[u8]>` of stored bytes and one `ChunkRef` per
  *obligation*, ≤ Q objects; no decoded chunk list is kept per object. The whole-namespace
  `meta.scan(b"inode:")` materialisation is pre-existing and shared with `gc.rs:365`.
- **Not re-raised, per the target rubric's reviewer protocol** (`AGENTS.md:200-203`): the absent
  Tier-0 DST leg (settled recorded-rejected at brief.md:260-265) and the four `review-rejected.md`
  findings about orphan-marking a fragment a hidden object still references (rule B).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Whether refusal accounting is truly per object must be re-established with a multi-obligation fixture — the current one-obligation refusal leg at `crates/custodian/tests/segmented_map_reconstruction.rs:434` cannot expose the Rule D violation.
- [ ] T5 Judgment — Restore direct oracles before rebuild sign-off — leg 1's queue-drain assertion is commented out at `crates/custodian/tests/segmented_map_reconstruction.rs:427`, and leg 5 checks only index 0 rather than the duplicate second placement at `crates/custodian/tests/segmented_map_reconstruction.rs:571`.
- [ ] Validation — fitness-to-purpose — Decide production fitness and whether Tier-1 disk-fault or Tier-2 kill-and-reconstruct observation is warranted — CI/DST are green, but operational reconstruction risk remains a human sign-off decision.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Whether refusal accounting is truly per object must be re-established with a multi-obligation fixture — the current one-obligation refusal leg at `crates/custodian/tests/segmented_map_reconstruction.rs:434` cannot expose the Rule D violation.; T5 Judgment — Restore direct oracles before rebuild sign-off — leg 1's queue-drain assertion is commented out at `crates/custodian/tests/segmented_map_reconstruction.rs:427`, and leg 5 checks only index 0 rather than the duplicate second placement at `crates/custodian/tests/segmented_map_reconstruction.rs:571`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
