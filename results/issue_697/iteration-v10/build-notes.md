# Build notes — issue 697, iteration 10 (`reconstruction-reads-through-resolver-once-contained`)

Target branch: `getwyrd/wyrd @ main` (base `origin/main @ 339da46`). All edits made in
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`; every `path:line` below is that tree
(= the patched file), with base line numbers marked "(base)".

**Two files, as budgeted.** `crates/custodian/src/reconstruction.rs` (**143** added semantic
lines, cap 160) and the new `crates/custodian/tests/segmented_map_reconstruction.rs` (712 raw
/ 492 semantic — see §6 on the shape cap, pre-accepted by the human at iteration 8's sign-off).

---

## 1. What the patch does

One production file. Three inline `record.chunk_map.as_flat().ok_or(SegmentedMapUnsupported)?`
sites (base `:329-335`, `:579-586`, `:632-638`) and the per-obligation namespace walk
(`find_chunk`, base `:620-646`) are replaced by **one reading of the committed namespace per
pass**, made through the resolver every other consumer already shares:

| Piece | Where | What it is |
|---|---|---|
| `Reading` (+ `contain`) | `reconstruction.rs:355`, `:378` | the pass's one reading: the objects it owes a repair inside, the site of each queued chunk, whether the reading has a hole, and the refused objects (`BTreeSet<Vec<u8>>`, so a refusal is one per **object**) |
| `FlatObject` | `:386` | the scanned generation, held **once** per owed object: `inode_id` parsed from the scanned key + the whole record (the CAS precondition and the ADR-0047 metadata a repair preserves) |
| `Site` / `FlatSite` | `:402`, `:414` | per obligation, O(1) in the object's size: which shared snapshot, which index in it, this chunk's own `ChunkRef` |
| `read_committed` | `:451` | the walk — `decode` contained, `state` checked, `resolve_chunk_map`, `Ok(None)` skipped, `ChunkMapError` downcast contained, anything else propagated |
| `Assessment::Refused` | `:577`, consumed `:255` | a repair this pass may not perform (the chunk's committed reference lives in a `seg:` record; #682 owns that write path) |
| drain gate | `:323` | the one batch both drain paths flow into, withheld while the reading is incomplete |
| answer | `:331` | `Blocked` when the reading had a hole **or** a repair was refused |
| `emit_unresolvable` / `emit_refused` | `:1023`, `:1042` | the two pinned audit rows + their `reconstruction_*_records` counters |

Mirrors, not inventions: `gc::referenced_fragments` (`gc.rs:360-416`) and
`restore::committed_chunks` (`restore.rs:621-658`) for the walk and the downcast rule;
`gc.rs:155-166` for attribution emitted **per object, before the work loop**;
`gc::object_name` (`gc.rs:470-480`) for the injective naming; `gc.rs:564-567` for the shared
`unresolvable-chunk-map` action string; `reconciliation.rs:44` for the `Blocked` vocabulary.

**Everything else about the write path is the base's, unchanged.** The priority sort
(`:260`) and the repair loop (`:303-310`) are the base's lines: one version-conditional commit
per repaired chunk, in chunk-level priority order. `repair_chunk` (`:798`) takes the shared
`&FlatObject` instead of a per-plan clone and is otherwise byte-identical, including the frozen
CAS at base `:598-601` (`inode_key(plan.inode_id)` → `inode_key(object.inode_id)`,
`encode(&plan.prior)` → `encode(&object.prior)` — same key, same bytes, #698 untouched).

## 2. The carry-forward: what changed since iteration 9, and why

Iteration 9 was rejected on five findings, all against **one** mechanism: grouping an object's
plans into a single multi-chunk commit (`repair_object`). This round **removes that mechanism
entirely** and keeps only the half that no round has ever found a defect in — the one reading.

| Iteration-9 blocking finding | How this round answers it |
|---|---|
| Priority inversion: a group ran at its most urgent member's position, so lower-priority repairs preceded more urgent ones in later objects | There are no groups. `plans.sort_by_key(repair_priority)` (`:260`) and the per-chunk commit loop (`:303`) are the base's own lines, untouched — chunk-level urgency ordering is restored by *deletion*, not by a new rule. Leg 4 also drives it: of four obligations inside one object, the one the base's order picks is the one that lands. |
| The grouped commit discarded already-completed repairs when a later chunk's write faulted (chunk A committed on the base; with the patch a fault on B meant A was never repaired — permanent non-convergence) | Each chunk commits on its own again (`:303-310`), so a chunk that lands is durable whatever a later one does — an `Err` out of a later `put_fragment` ends the pass exactly where the base ends it, with the earlier commit already durable. |
| A new concurrent/destructive path (grouped multi-chunk version-conditional commit) shipped without seeded Tier-0 DST | There is no new write path at all now. Every commit this pass makes is built from and conditioned on the generation the scan returned, by the base's own construction. A second obligation inside the same object loses the CAS it **already lost on the base** (the base assesses every obligation before any repair commits — base `:184-185` then `:261-262` — so both plans carry the same `prior`) and stays queued. Nothing to seed a DST case over: the concurrency surface is unchanged, byte for byte. |
| T2 shape: 207 production semantic lines vs the 160 cap | 143. The grouping machinery (`repair_object`, `Rebuilt`, the group index, `RepairOutcome: Clone/Copy`) is gone. |
| (iteration 8) "eliminate the remaining per-obligation whole-record clone/encode path" | Kept from iteration 9, because no round found a defect in it: the reading holds **one** record per owed object (`:517`) and each plan is an index (`RepairPlan::object`, `:120`), so Q obligations inside an N-entry object cost N decoded entries once, not Q×N. The remaining per-*repair* map copy + encode (`:861`, base `:579-601`) **is the base's**, is what the CAS precondition and the put are made of, and is frozen by §Scope — see §4 for why I did not remove it. |
| (iteration 8) "make the test observe full-map clone/rewrite cost" | Not carried forward as an oracle, deliberately — see §4. A heap copy is invisible on the trait seam; the only honest seam-visible complexity oracle is the brief's own leg 4 (one `scan(b"inode:")` per pass, `≤ S` `scan_page`s), plus the convergence loop this round adds. I did **not** manufacture an allocation-counting stand-in for it. |

## 3. Alternatives ruled out, with their cost

**(a) Grouping an object's repairs into one commit (iteration 9's mechanism).** Rejected: it
is what the last sign-off rejected, and the reasons are structural, not cosmetic — priority
inversion across objects, and one faulted chunk discarding its object's already-rebuilt
siblings. Cost, measured on the v9 patch: `repair_object` + `Rebuilt` + the grouping index =
**+64 production semantic lines** (207 total vs 143 here) and a commit shape with no
counterpart on the base.

**(b) Refreshing the shared snapshot after each successful commit** (write the record we just
committed back into `Reading::objects[..]`, so sibling obligations in the same object CAS
against the live generation and all land in one pass). Cheap — about **+4 lines**: change
`RepairOutcome::Committed` to carry the `next` record and assign it in the loop at `:304-309`.
Rejected anyway: it is a *behaviour change beyond the base* (the base repairs one chunk per
object per pass and conflicts the rest), and it creates exactly the **chained multi-commit CAS
within one pass** that iteration 8's review demanded seeded Tier-0 DST coverage for. This
slice's mandate is to change the *reading*, not the writing. The cost of not doing it is Q
passes instead of one for Q obligations inside a single object — which is what the base already
costs, and which leg 4 asserts converges (`tests/…:633-646`), each pass still reading the
namespace exactly once.

**(c) Widening containment to every error** (contain a store fault as "this object is
unreadable"). Rejected: `gc.rs:405-415` states the rule this repo uses, and a store fault is
not one object's. Leg 5 is the guard — without it, containing everything would pass legs 1–4.

**(d) Deleting the two redundant fields in `repair_chunk`'s record construction** (`size:` and
`state:` at `:864`/`:866`, both already supplied identically by `..object.prior.clone()`).
That is what v9 did; it would clear the two advisory mutants in §5. Rejected: they are base
lines outside this slice's subject, and keeping the record construction byte-identical to the
base is the stronger claim for a change whose whole point is "the write is unchanged".

## 4. Judgment calls a reviewer will want stated

* **`RepairPlan`'s base fields moved, they did not change meaning.** §Scope freezes the
  *meaning* — a record identified by an `InodeId` **parsed from the scanned key**, CAS'd under
  a **re-derived** `metadata::inode_key` conditioned on a **re-encoded** scanned record — and
  forbids switching the plan to store key bytes / scanned value bytes (#698's fix). Both fields
  are still exactly that; they now live on `FlatObject` (`:386-398`) because identity is a
  property of the object, not of each obligation inside it, and because a per-obligation copy
  is the Q×N heap cost C5 flagged in rounds 6–8. `parse_inode_key` (`:893`) is unchanged and
  moved with the walk, exactly as §Scope permits.
* **`repair_chunk`'s unreachable arm** (`:853`): a plan exists only for a generation the reading
  found flat, so `as_flat()` cannot fail there. It returns `Aborted` (fail-safe: nothing
  committed, obligation queued, offset on `reconstruction_aborted`) rather than the `?` the base
  had — a pass-ending `?` on the very shape this slice exists to stop ending passes on would be
  the defect wearing a new coat.
* **Duplicate committed `ChunkId`, key identity, generation-restart, `Blocked`'s rustdoc** —
  #700 / #698 / #699 / #701, all out of scope by the brief, none touched. First-match-wins is
  preserved at `:511-513` (the first committed reference in key order claims the chunk, exactly
  as base `find_chunk:639` chose one).
* **`reconcile`'s own rustdoc** (`:137-143`) now names `Blocked`. That is this function's doc in
  this file, not `Reconciled::Blocked`'s rustdoc in `reconciliation.rs` (#701) and not another
  file's docs. Leaving it saying "Changed … Satisfied otherwise" would have been a stale doc on
  the line the patch changes.
* **No DST leg, and none owed** (brief §Verification posture). Stronger this round than in
  v8/v9: there is no new concurrent or destructive path at all — no grouped commit, no chained
  CAS, no new write. Every flat repair resolves by borrow (`crates/core/src/metadata.rs:2585`),
  so it can never be superseded and never restarts; a segmented snapshot is refused and writes
  nothing.
* **No docs / ADR / conformance / `Cargo.toml` change.** The dev-dependencies the test uses
  (`wyrd-coordination-mem`, `wyrd-testkit`, `tokio`, `async-trait`, `bytes`,
  `tracing-subscriber`) were already declared on `crates/custodian`.

## 5. Gates run here (the real runners, not hand-rolled)

| Check | Result |
|---|---|
| `./engine/scripts/run-verify.sh` (C4-verify, red→green) | **PASS** — "red without the fix, green with it (6 test(s) ran red)": with `reconstruction.rs` reverted and the test kept, **5 of 6 legs failed behaviourally** (`SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }` on legs 1–4, and leg 5's `expected ident at line 1 column 2` — the base's `decode(&value)?` ending the pass before any name is out); leg 6 green, exactly as the brief pre-declares |
| `./engine/xtask.sh ci` (C4-ci, whole tree) | **PASS** — "xtask ci: all checks passed" (fmt, clippy `-D warnings`, build, whole test suite, deny, conformance, prose gates) |
| `cargo test -p wyrd-custodian --test reconstruction` | 15 passed — the existing suite is green **unmodified**, as §Scope requires |
| `cargo fmt --all -- --check`, `typos` | clean (the target's own commit hooks) |
| `scripts/mutants-in-diff` (C5, advisory) | 22 mutants: 13 caught, 7 unviable, **2 missed** — see below |

**The two missed mutants are equivalent mutants, not test gaps.** Both are "delete field
`size` / `state` from the `InodeRecord` struct expression in `repair_chunk`" (`:864`, `:866`).
Both fields are supplied identically by the `..object.prior.clone()` update on the next line:
a repair never changes `size`, and `prior.state` is always `InodeState::Committed` because
`read_committed` skips every other state (`:467-469`). Deleting either produces a
byte-identical record, so **no test can kill them**; only deleting the redundant lines removes
them, which §3(d) explains I chose not to do. They are base lines (base `:588-597`), unchanged
by this patch beyond the `plan.prior` → `object.prior` rename.

## 6. Known deviations to weigh at sign-off

* **Test file shape**: 712 raw / 492 semantic vs the brief's 460 / 280 cap. The human accepted
  this overage explicitly at iteration 8's sign-off ("human accepts as fine — do not spend the
  round shrinking the file"); it is unchanged in kind (v9 was 743 raw). The bulk is the single
  shared fixture the brief's compression rules ask for: one `BTreeMap`-backed metadata double
  carrying both seam counters and the injected fault, one parameterised seeding helper that
  asserts its own damage, one audit-capture helper — all shared by six legs.
* **C5**: two equivalent mutants (above), advisory.

## 7. Forced self-refutation (recorded, per the Do protocol)

**(a) Genuine red?** **Yes — measured, not predicted.** `run-verify.sh` reverts
`reconstruction.rs` to `origin/main`, keeps the test file, and re-runs: 5 of the 6 legs fail
**behaviourally** (assertion / `expect` panics on the base's `Err`, not compile errors — the
test names no symbol this patch introduces, so the target still builds). Leg 6 stays green,
which the brief declares in advance as a regression guard rather than a base red. Verdict line:
`run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).`

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point, elected through `Custodian::elect` over `MemCoordination` and authorized
by a real `FencedZone` (`tests/…:399-432`) — which dispatches the production
`reconstruction::reconcile`. No internal helper is called, nothing is re-implemented in the
test: the doubles implement the `MetadataStore` / `ChunkStore` **trait seams** (the store
below the pass), and the resolver under test is the production
`wyrd_core::metadata::resolve_chunk_map`. The only test-side logic is seeding and reading the
store back.

**(c) Fixture includes the fault?** **Yes, and it proves its own damage.** `seed`
(`tests/…:295-331`) resolves every object it plants and asserts `resolves.is_err()` **iff** the
seeded shape is the damaged one — so no leg can pass because its fault silently stopped being
one (or because a shape meant to be healthy quietly became damaged). Leg 2 refuses over a real
segmented object with real `seg:` records (and asserts every non-`repair:` row is byte-identical
afterwards). Leg 3 seeds a segment the root names that was genuinely never written, plus a
record whose bytes genuinely will not decode, **first in key order** over the `BTreeMap`-backed
store so "met first" is a fixture property. Leg 5 injects the fault on the read the **resolver**
performs (`scan_page(b"seg:…")`, `tests/…:111-116`), never on `scan(b"inode:")`, and asserts the
injected fault's own text came back. Nothing is curated out: the damaged objects sit in the same
store as the healthy repair each leg asserts still lands.

## 8. Scratch

`$PDCA_SCRATCH/pdca-builder-697-*` (extracted v9 test for reference, two `xtask ci` logs) —
removed at the end of the run.
