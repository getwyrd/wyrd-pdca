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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 34 mutants tested in 64s: 1 missed, 15 caught, 17 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #697: make custodian reconstruction resolve each committed map once per pass, contain unreadable objects, and refuse unsafe segmented/ambiguous repairs without losing obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the containment, no-drain, single-scan, exact-key, duplicate-ID, and non-map-fault decisions with eight bounded legs and explicit non-goals. |
| C2 Reproduction (red pre-fix) | PASS | With production stashed, the base-visible discriminator compiled and six behavioral legs failed while the two declared regression legs passed; the entry-point fixture is at `crates/custodian/tests/segmented_map_reconstruction.rs:344`. |
| C3 Change | PASS | The two-file patch stays on reconstruction and its required discriminator, replacing per-obligation lookup with one resolver-backed index at `crates/custodian/src/reconstruction.rs:797` and preserving raw-key CAS at `crates/custodian/src/reconstruction.rs:688`. |
| C4 Verification (red→green) | PASS | Restoring production made all eight discriminator legs pass, and a scratch-local `cargo xtask ci` completed spelling, docs, fmt, clippy, build, workspace tests, all dependency walls, conformance, statics, deploy guard, and the 50-seed DST tier with no skipped required tool. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must withhold every no-op drain after an incomplete reading — a readable flat site can still return `Drain` at `crates/custodian/src/reconstruction.rs:501` and be deleted at `crates/custodian/src/reconstruction.rs:322` even though `unaccounted != 0` selects withholding only for absent sites at `crates/custodian/src/reconstruction.rs:409`, so an unreadable record can hide the reference the obligation protects. |
| T1 Structure | PASS | The patch changes only `crates/custodian/src/reconstruction.rs:146` and the required new integration discriminator `crates/custodian/tests/segmented_map_reconstruction.rs:1`; the existing reconstruction suite and excluded sibling surfaces remain untouched. |
| T2 Shape | FAIL | The human-readable compactness bound is exceeded — `crates/custodian/tests/segmented_map_reconstruction.rs:620` is exactly 620 raw lines but 452 nonblank/non-comment lines, 72 over the brief's 380-semantic-line cap; production is within its cap at 199/230. |
| T3 Runtime | PASS | The fixture drives the real fenced `reconcile_step` at `crates/custodian/tests/segmented_map_reconstruction.rs:358`, and both its focused run and the repository's full Rust/DST gate passed against the exact target base. |
| T4 Contribution | NEEDS-HUMAN | Whether to accept contribution readiness without an independent contribution-artifact replay is owed — the `scripts/pdca contribcheck` wrapper and the artifacts it validates were not supplied, although affected-path history found merged predecessors plus closed-unmerged #647 and no open overlap. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must restore Rule E's direct oracle and behavior — the non-`ChunkMapError` branch returns before naming the object at `crates/custodian/src/reconstruction.rs:837`, while leg 8 discards its capture and checks only error/state at `crates/custodian/tests/segmented_map_reconstruction.rs:615`, leaving the required pre-error operator attribution absent and untested. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether per-object containment with continued healthy repairs and blocked certification is operationally fit for the durability plane, because it changes the claim and audit behavior operators rely on during metadata damage. |

Mutation rerun note: the sole survivor (`noncertifying += 1` → `*= 1` at `crates/custodian/src/reconstruction.rs:261`) is equivalent because `Assessment::Withheld` is reachable only when `unaccounted` is already nonzero and the result observes only `noncertifying > 0`.

### Advisory — adversary

# Adversarial review — issue #697 (advisory, never gating)

Re-ran the asserted red→green independently: rebuilt the **base** `wyrd-custodian`
(`339da46:crates/custodian/src/reconstruction.rs`) and the **patched** crate side by side in a
scratch package and drove `reconcile_step` through the real fenced control point on both. The
C4-verify log's shape is honest — 6 of 8 legs fail on the base, legs 7 and 8 pass, exactly as the
brief declared. What follows is where the evidence is thinner than it reads, and where the fix
over-corrects.

## Findings

- **NEEDS-HUMAN [impl] — leg 8 silently drops the Rule E assertion the brief made it responsible
  for, and its fixture cannot satisfy it.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:615` drives with `Capture::default()`
  inline and never inspects the seam; the brief's leg 8 requires "the unreadable object's name is
  **already** on the audit seam even though the pass returns `Err` (Rule E) … Leg 8 binds the
  placement". Concrete: the leg seeds only `seed_unresolvable()` (`:609` → `inode:00`), and that is
  the *same* object whose resolve raises the injected `get` fault, so `locate_queued_chunks`
  propagates at `crates/custodian/src/reconstruction.rs:838` having named nothing — the assertion is
  unsatisfiable as fixtured, which is presumably why it is absent. Swapping `:609` to
  `seed_damaged()` plants the undecodable `inode:0`, which sorts *before* `inode:00`, so a name is
  on the seam before the `Err` and the property becomes testable. As shipped, the load-bearing half
  of Rule E ("a later transient store fault cannot cost the operator the name of the record to
  repair" — brief, and `reconstruction.rs:727-728`) has **no** coverage on the `Err` path; only
  leg 3's `Ok` path is covered.

- **NEEDS-HUMAN [impl] — leg 5's red is confounded: on the base every substantive assertion in it
  already holds, so it does not demonstrate the loss it exists to prevent.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:509`. Measured on base `339da46` with the
  leg's own fixture (outcome printed before the enum assertion): `outcome = Satisfied`,
  `queued = [161, 211]` (both `C_REPAIR` and `C_DUP` still queued), `inode:40 = [0,1,4]`,
  `inode:41 = [0,1,4]`, `inode:42[0] = [0,1,4]`, `inode:42[1] = [2,3,4]`. So `"neither"` (`:528`),
  the three `"no repoint"`s (`:531`), `"nor the duplicate"` (`:536`) and `"nothing rebuilt"`
  (`:537`) are all **already true on the base**. The cause is `stored()`
  (`crates/custodian/tests/segmented_map_reconstruction.rs:214`), which seeds every root in a
  space-injected spelling: the base's CAS is `require(key, metadata::encode(&plan.prior))`, so it
  conflicts on *every* object and repairs nothing anywhere. The brief's stated base behaviour —
  "Today the base repairs whichever reference `find_chunk` meets first (`:639`) and drains the
  obligation" — is therefore **not** reproduced; the leg goes red only on `Reconciled::Blocked` and
  on the absent audit rows. Seeding leg 5's three roots canonically (`metadata::encode`) would make
  the base actually repoint one reference and drain `C_REPAIR`, turning `:528`/`:531`/`:536` into
  real discriminators of the ambiguity rule.

- **NEEDS-HUMAN [impl] — the unparsable-key containment at `crates/custodian/src/reconstruction.rs:821`
  is unmandated over-containment: it permanently stalls the whole loop over a record it could
  repair perfectly well.** `parse_inode_key`'s *result* is discarded — it is the only call site
  (`:821`, `:906`) and the raw key is what is read, CAS'd on and named (Rule C, `:869`, `:688`) — so
  the check buys nothing, yet its `cannot_account_for` marks a fully decodable, `Committed`, flat,
  repairable record as unaccounted. Measured, patched build: a store holding `inode:not-an-id`
  (a valid committed flat record) beside `inode:9` (healthy work) and one obligation for a chunk no
  map references answers `Blocked`, leaves the odd-key object's under-replicated chunk at
  `[0,1,4]` **unrepaired**, and leaves the genuinely-unreferenced obligation queued — store-wide,
  every pass, forever, with no remediation but deleting the row. The brief pinned containment as
  "on **any** read fault by exactly gc.rs's downcast rule (`gc.rs:402-416`) … and no other"; a key
  that will not parse is not a read fault, and `gc.rs:365-410` — the walk this is a third copy of —
  has no parse check at all, so reconstruction alone now stalls on a row GC certifies over. (In
  fairness the base is *worse* here: it silently drains both obligations, `outcome = Changed`,
  `queued = []` — a real loss. The finding is that the fix over-corrects into a permanent stall
  rather than simply repairing the record under its raw key, which is exactly what Rule C enables.
  Either drop the check, or name without counting it toward `unaccounted`.)

- **NEEDS-HUMAN [human] — the discriminator is 19% over the brief's semantic budget; adjudicate
  whether the "shape is wrong" clause bites.** `crates/custodian/tests/segmented_map_reconstruction.rs`
  is **452** semantic lines (620 raw − 42 blank − 126 comment-only) against the brief's
  "≤ **380** semantic / 620 raw", and sits at *exactly* 620 raw — i.e. trimmed to clear the stated
  STOP trigger while the semantic budget went unchecked (C1 Spec is `"result": "none"` in
  `check-gates.json`, so nothing measured it). The brief's own reading of an over-budget test file
  is "the shape is wrong: STOP and hand back rather than finish"; only the raw cap is written as the
  trigger, so this is a judgment call, not a mechanical failure.

## Refutations attempted and **not** landed

- **Two repairable obligations inside ONE flat record.** Built it: one committed flat inode holding
  two RS(2,1) chunks, each missing fragment 2, both queued. Patched build repairs only the first —
  `placements = [0,1,2] [0,1,4]`, `still queued = [162]`, outcome `Changed` — because every plan
  carries the *same* `Arc<[u8]> prior_bytes` from the one reading and CASes on it
  (`crates/custodian/src/reconstruction.rs:690`), so the second commit loses and reports a spurious
  `reconstruction_conflict` after having already written its rebuilt fragments as orphan garbage.
  **This is not a refutation:** the base produces byte-identical results (`[0,1,2] [0,1,4]`,
  `queued = [162]`, `Changed`), because there too every `assess` runs before any `repair_chunk`.
  Pre-existing debt this diff neither introduces nor worsens — worth a tracked issue (one chunk per
  object per pass on a large multipart object), not a block on this bundle.
- **Rule A's value-equality guard** (`crates/custodian/src/reconstruction.rs:853`). Tried to find a
  resolve that restarts yet compares equal: `resolve_snapshot` short-circuits flat maps at
  `crates/core/src/metadata.rs:2585` (so `Superseded`/`Gone` are unreachable for a flat record), and
  `root_dropped` (`crates/core/src/metadata.rs:2337-2341`) only reports `Superseded` when the live
  root names a *different* group or a flat map — which forces the records unequal. Could not
  construct a mixed reading.
- **The two new abort paths in `repair_chunk`** (`:666` `as_flat` and `:678-681` index/id). Could
  not reach either; both are second guards behind facts `locate_queued_chunks` already established.
  Their being unreachable is a strength, not a hole — nothing is committed on either path.
- **C5's one surviving mutant is provably equivalent, so the `fail` row is noise.**
  `reconstruction.rs:261:31` (`+=` → `*=`) sits in the `Assessment::Withheld` arm; `Withheld` is
  returned only when `index.unaccounted != 0` (`:409-412`) and `noncertifying` is seeded from
  `index.unaccounted` (`:169`), so that increment can never change `noncertifying > 0`. Line 261 is
  effectively dead. (The `TIMEOUT` row at `:853` is the rule-A guard inverted, which stops all
  convergence — a hang, not a survivor.)
- **Rule B for the refusal path** (a `seg:`-resident refusal must not cost a healthy flat object its
  repair in the same pass): read the loop at `:304-320` — plans execute unconditionally of
  `noncertifying`, and leg 3 proves the equivalent for the unaccounted path. Could not break it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must withhold every no-op drain after an incomplete reading — a readable flat site can still return `Drain` at `crates/custodian/src/reconstruction.rs:501` and be deleted at `crates/custodian/src/reconstruction.rs:322` even though `unaccounted != 0` selects withholding only for absent sites at `crates/custodian/src/reconstruction.rs:409`, so an unreadable record can hide the reference the obligation protects.
- [ ] T4 Contribution — Whether to accept contribution readiness without an independent contribution-artifact replay is owed — the `scripts/pdca contribcheck` wrapper and the artifacts it validates were not supplied, although affected-path history found merged predecessors plus closed-unmerged #647 and no open overlap.
- [ ] T5 Judgment — Rebuild must restore Rule E's direct oracle and behavior — the non-`ChunkMapError` branch returns before naming the object at `crates/custodian/src/reconstruction.rs:837`, while leg 8 discards its capture and checks only error/state at `crates/custodian/tests/segmented_map_reconstruction.rs:615`, leaving the required pre-error operator attribution absent and untested.
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether per-object containment with continued healthy repairs and blocked certification is operationally fit for the durability plane, because it changes the claim and audit behavior operators rely on during metadata damage.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must withhold every no-op drain after an incomplete reading — a readable flat site can still return `Drain` at `crates/custodian/src/reconstruction.rs:501` and be deleted at `crates/custodian/src/reconstruction.rs:322` even though `unaccounted != 0` selects withholding only for absent sites at `crates/custodian/src/reconstruction.rs:409`, so an unreadable record can hide the reference the obligation protects.; T4 Contribution — Whether to accept contribution readiness without an independent contribution-artifact replay is owed — the `scripts/pdca contribcheck` wrapper and the artifacts it validates were not supplied, although affected-path history found merged predecessors plus closed-unmerged #647 and no open overlap.; T5 Judgment — Rebuild must restore Rule E's direct oracle and behavior — the non-`ChunkMapError` branch returns before naming the object at `crates/custodian/src/reconstruction.rs:837`, while leg 8 discards its capture and checks only error/state at `crates/custodian/tests/segmented_map_reconstruction.rs:615`, leaving the required pre-error operator attribution absent and untested.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_697/review-b.
- By / date: auto-iterate / 2026-08-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
