# Build notes — issue 651 / repair-passes-through-resolver-with-containment (iteration 3)

*Withheld from the reviewer; written for the human at sign-off.*

---

## 1. What the previous two attempts got wrong, and what changed

The carry-forward blocks name three things. All three are addressed by a **different design**, not
by another round of the same one.

| Iteration | What it did | Why it failed |
|---|---|---|
| v1 | Resolver wired in; destination fragments written before the CAS, as `main` has always done | C5: the ceiling fixture seeded an **already-oversize** record and moved server 0→3, so it never exercised a legal→oversize transition; 18 changed-logic mutants survived; both repair passes fell through to `Satisfied` over an incomplete walk |
| v2 | Added the normative **pre-mark → write → exact-bytes CAS** sequence plus a `require_absent(desired:dserver:<S_new>)` drain fence | 14 blocking review findings, **every one of them about that machinery** (orphan-mark overwrite races, pre-mark grace refresh, a pre-mark with no reclamation path, GC deleting bytes before the orphan key…), plus 2,184 semantic lines against a ~1,500 ceiling. A refused evacuation still reported `Satisfied` |

**v3's move: delete the cause instead of guarding it.** The pre-mark/fence is #653's
(`committer, fence, rollback and resume` — the brief's *Out of scope*), and v2's own
`repoint_chunk` doc admitted it closes nothing without its other half (the retirement drain
re-reading each segment's current placement, `0016:2416-2430`). So it is **out** — and instead of
restoring v1's "write first, refuse later" hazard, the two moving passes now **plan the whole move
in memory, including the repoint the record has to accept, before a single byte is written**:

- `crates/custodian/src/reconstruction.rs:699-766` — targets resolved, shards encoded, and
  `metadata::repoint_chunk(...)` built **first**; only then `store.put_fragment(...)`, then the
  commit.
- `crates/custodian/src/rebalance.rs:288-355` — the same order for the evacuation copy.

That single ordering change makes **all fourteen v2 findings unreachable**: a refused repoint now
writes nothing at all, so there is no abandoned destination copy to pre-mark, to re-stamp, to race
GC over, or to leak. It also strictly improves on `main`: on `main` a repair that the record would
refuse still wrote its rebuilt fragment first.

The two remaining v2 blockers are answered directly:

- **T5 — a refused evacuation must be non-certifying.** `EvacOutcome::Refused` /
  `RepairOutcome::Refused` are new arms, distinct from `Aborted` (`rebalance.rs:255-271`,
  `reconstruction.rs:644-655`): an abort is missing capacity *now* and clears on its own, a refusal
  is a record no retry will ever repair. A refusal makes the pass return `Reconciled::Blocked` and
  emits an attributed `repoint-refused` audit line. Test:
  `a_repoint_that_would_cross_the_value_ceiling_is_refused_and_nothing_is_written`.
- **C5 — the ceiling fixture must exercise a legal→oversize transition.** The fixture builds a
  `seg:` record padded to *just inside* `MAX_VALUE_BYTES` and asserts **both** halves before the
  pass runs: `stored.len() <= MAX_VALUE_BYTES` ("the record starts LEGAL") and
  `MAX_VALUE_BYTES - stored.len() < growth` ("less headroom than the move needs"), where `growth`
  is the six placement entries widening from a 1-digit to a 20-digit D-server id
  (`crates/custodian/tests/segmented_map_repair.rs:728-744`).

---

## 2. The four criteria, and where each is proven

| Criterion | Test | The positive observable |
|---|---|---|
| (1) `stranded_marked == 0` over segmented objects, every fragment present | `restore_over_segmented_objects_marks_nothing_and_keeps_every_fragment` | A **GC pass past the grace window** follows the restore pass: a wrongly-marked fragment would be genuinely `delete_fragment`ed. And the same pass over the same store then marks a real stray, which the next GC pass reclaims — so the zero is a decision, not inaction |
| (2) repair + evacuation reach a `seg:`-resident `ChunkRef`; a ceiling-crossing repoint is refused | `reconstruction_repairs_a_chunk_whose_ref_lives_in_a_seg_record`, `rebalance_evacuates_a_seg_record_chunk_off_a_draining_server`, `a_repoint_that_would_cross_the_value_ceiling_is_refused_and_nothing_is_written` | The rebuilt/copied fragment is on the server the **repointed** placement names and passes `repair::fragment_intact`; the root's version does **not** move (the segment record is what changed); on refusal the `seg:` bytes are identical, every destination store is **empty**, and the audit names the ceiling |
| (3) backfill declines a segmented record with a stated reason, still fills a flat one | `backfill_leaves_a_segmented_record_byte_identical_and_still_fills_a_flat_one` | `seg:` record **and** root byte-identical; the flat record's placement materialized in the same pass; `declined-segmented` audit line with the record name and the unfilled count; both gauges (`backfill_placement_remaining=1`, `backfill_records_unassessed=1`) |
| (4) O(N) resolutions, not O(Q × N) | `reconstruction_resolves_each_object_once_per_pass_not_once_per_queued_chunk` | Q = 9 obligations over N = 3 objects; the instrumented store **counts** `seg:` range reads and the assertion is `== 3`, with the Q × N figure in the failure message. The queue is emptied, so the count is over a pass that assessed all nine |

Containment ships as `a_damaged_object_does_not_starve_the_healthy_ones_and_its_obligation_stays_queued`:
the healthy object is repaired **and** drained (`placement == [0, 3, 2]`), the obligation whose chunk
may live in the unreadable map is **still queued**, nothing of the damaged object moved, the blocker
is named on the audit seam, and the step reports `Blocked`.

---

## 3. Design decisions worth the human's attention

**One shared walk, `crates/custodian/src/resolve.rs` (61 semantic lines).** Four passes needed the
same thing — resolve every committed object once, per-object containment, homes attached — and the
alternative was that logic copy-pasted four times (~4 × 35 lines, and four places for the
containment rule to drift). It emits nothing itself: each consumer emits its own counter, the rule
#650 set (`gc.rs:555-558`: a `gc_` counter ticked inside the shared builder would fire for passes GC
never ran).

**`ChunkHome::Segment` holds `Arc<SegmentHome>`.** A segment carries many chunks; a per-chunk copy of
its key + prior bytes would make resolving an object quadratic in exactly the shape segmentation
exists to make cheap. Same reasoning for `Arc<InodeRecord>` in the reconstruction index: one root per
object, not one per chunk. (Both were v2 findings, fixed there and kept here.)

**The `seg:` row's exact bytes ride with the decoded record** (`metadata.rs:2590-2600`). The repoint's
precondition is `require(key, <those bytes>)`. Re-encoding the decoded record to rebuild the
precondition would make every repoint depend on decode→encode being byte-identical — the rubric's
*Serialization identity* class, and a silent no-op CAS failure if it ever were not.

**Reconstruction's `find_chunk` is gone; a per-pass `ChunkIndex` replaces it.** That is defect (2).
The index is built from one walk and consumed by move (`ChunkIndex::build`), so the store's chunk
lists are not copied to be looked up. `find` answers `Absent` only when the walk was **complete** —
otherwise `Unassessable`, which keeps the obligation queued.

**An idle reconstruction pass returns before building the index** — but **after** emitting the four
levels at zero (`reconstruction.rs:150-162`). Emitting them was not optional: the first version of
this early return broke `crates/server/tests/closed_write_path.rs:348`, which reads the day-one
"rises then returns to zero" signal on exactly the pass after the last repair drained. Caught by the
workspace suite, not by review.

**`ReconciliationStatus::PendingUnresolvable`** discharges the in-code `// deferred: #651` marker
`desired_state.rs` carried, and #650's two assertions that named it are updated to the attributed
answer (they are the only two `segmented_map_consumers.rs` lines this patch touches).

**Ceiling checks: values only, not keys.** Both keys a repoint names (`inode:<id>`, a fixed-width
`seg:` key) are bounded by their own grammars, so a key ceiling could never bind here; the salvage's
`MAX_KEY_BYTES` constant would be an unused surface. Stated at `metadata.rs:3042-3045`.

**One thing I preserved rather than fixed, so the human sees it:** the *flat* arm's precondition is
`require(inode_key, encode(prior_root))` — a re-encode of the decoded record, which depends on
decode→encode being byte-identical. That is `origin/main`'s existing shape verbatim
(`rebalance.rs:312` / `reconstruction.rs:600` on the base) and the segmented arm deliberately does
**not** repeat it (it names the row's exact bytes). Changing the flat contract is not this slice's
scope; it is called out here in case a reviewer reads the asymmetry as an oversight.

**Charged on growth in the flat arm, absolutely in the segmented arm.** A flat root can *predate* this
ceiling discipline (`main`'s `commit_chunk_map` puts a map of any size and redb/TiKV/etcd store it);
charging it absolutely would refuse to repair such an object **at all**, including a move onto
narrower ids that shrinks it. A `seg:` record has exactly one writer, which charges every batch, so
an over-ceiling one cannot have been written.

**What is NOT here, deliberately:** the destination pre-mark and the destination drain fence
(`// deferred: #653`, stated in `repoint_chunk`'s doc at `metadata.rs:2941`). They close nothing
without the retirement drain that is their other half, they would change the **flat** path too (both
shapes go through the one builder), and they are the committer/fence #653 owns. The
plan-before-write ordering means a losing or refused repoint leaves *less* garbage than `main` does,
not more.

---

## 4. Alternatives ruled out, with the cost shown

- **Keep v2's pre-mark + drain fence and fix the 14 findings.** Cost, measured: v2's
  `crates/core/src/metadata.rs` hunk was **+799 raw lines** vs this patch's +490 production, and the 14 findings
  are structural to the design (a pre-mark that is written before the bytes needs an
  expiry/refresh/reclamation story that only #653's retirement drain provides). Rejected on scope —
  it is #653's slice — and because the cheaper fix *removes* the failure mode: nothing is written
  before the batch is built, so there is no abandoned copy to reason about.
- **Backfill resolves and rewrites a segmented record.** Standing rejection (iv), and structurally
  wrong: the fill is an inode CAS, and a segmented chunk's `ChunkRef` is not in the inode. Recorded
  in `review-rejected.md` at every line the finding lands on.
- **Ending the walk at the first unreadable record** (what `main` does, via `?`). Cost: one damaged
  object stops every *healthy* object's repair, evacuation and fill fleet-wide — the outage
  `docs/principles.md` §5 C-1 forbids buying for no safety at all.
- **Per-queued-chunk resolution** (the salvage's `find_chunk`, which resolves every scanned object
  per queued chunk). That is defect (2) and is what criterion (4) counts: Q × N = 27 range reads on
  this slice's fixture versus N = 3.
- **A `MAX_KEY_BYTES` ceiling helper** (in the salvage): ~14 lines (constant + variant + Display arm
  + check) that cannot bind on either key this batch names.

---

## 5. Budget — over the ceiling on tests; the exact numbers and the split I would propose

**12 files** (ceiling 15). Added lines, three ways — the middle column is the method the v2
reviewer used ("rough nonblank/noncomment additions", which scored v2 at 2,184):

| | raw `+` | non-blank, non-comment | strict (also excluding lines that are only `}` / `)` / `{`, which rustfmt introduces) |
|---|---|---|---|
| **total** | 2,826 | **1,985** | **1,641** |
| production (`metadata.rs` non-test 285, `reconstruction` 128, `rebalance` 65, `resolve` 56, `backfill` 54, `restore` 33, `desired_state` 10, `lib` 1, docs 2) | 991 | 634 | ~490 |
| — of which the **mechanical migration** the brief allows on top (pass callsites taking the resolved homed-chunk list in place of `record.chunk_map` / `as_flat()` indexing) | ~50 | ~44 | ~40 |
| tests: the discriminator `segmented_map_repair.rs` | 1,068 | 905 | 759 |
| tests: `metadata.rs` unit tests (§6, the mutation gaps) | 319 | 270 | ~215 |
| tests: DST property 11 | 200 | 163 | 131 |
| tests: #650's two updated drain-status assertions | 18 | 13 | 10 |

**So: ~1,940 non-comment / ~1,600 strict once the mechanical migration is deducted, against a
"≤ ~1,500" ceiling. This is over, and the human should read it as over.** Where it went:

- **Tests are 1,351 of the 1,985** — exactly the risk the brief flags. They are already pruned to
  the per-pass binding legs: 7 discriminator legs, one per criterion clause plus containment, each
  with a positive observable; 6 `metadata.rs` unit tests that exist only because `cargo-mutants`
  scopes a `wyrd-core` mutant to `wyrd-core`'s own tests (§6); and the DST property the brief
  requires to ship in this patch.
- **Production is 634 non-comment against the brief's ~310 salvage estimate.** The delta is
  itemized: the salvage's figure counted the five pass files and the ceiling helpers only, and did
  not include (a) the homed resolver and its four public types (~140), (b) six typed refusals with
  their `Display` arms (~100 — pure boilerplate the enum's shape requires), or (c) the shared walk
  `resolve.rs` (56), which exists so four passes do not each carry their own containment rule.

**The split I would propose if the human wants one** — and why I judge it worse: cut
`repoint_chunk` + the ceiling helpers + the homed resolver into their own slice, leaving the passes
routed through the resolver but still unable to *move* a segmented chunk. That halves this patch,
and it violates the brief's own **Caller-first** rule ("every production symbol introduced here has
a caller in this slice"): the repoint would land with no caller, and the pass slice would land a
reconstruction that can *see* a segmented chunk and not repair it — a pass that reports work it
cannot do, which is the failure mode this slice exists to remove. The cheapest honest reduction
inside this slice is dropping the `metadata.rs` unit tests (−270 non-comment), at the cost of the
17 mutants they kill (§6).

---

## 6. Mutation coverage (C5), and why unit tests landed in `wyrd-core`

`cargo mutants --in-diff` on the first draft reported 28 missed, 17 of them in
`crates/core/src/metadata.rs`. The cause is the tool's scoping, not a test gap in the usual sense:
cargo-mutants tests **only the package containing the mutant**, so a mutant in `repoint_chunk` is
judged by `wyrd-core`'s own tests — which never exercised it — even though the custodian integration
tests would catch it instantly. The honest fix is to test the surface where it lives: five sync unit
tests in `metadata.rs`'s existing `segmented_shape_invariants` module cover the placement-width
refusal, both arms' batch shape (which record is pinned, which is put), the growth-charged ceiling
(including that a *shrinking* move is still allowed), and the three refusals of a stale home.

Custodian-side misses were addressed by removing zero-start `+=` counters where the value is simply
the walk's own length (`backfill.rs:84`, `:207`), by asserting the levels the passes emit
(`gauge.backfill_placement_remaining`, `gauge.backfill_records_unassessed`,
`gauge.reconstruction_unassessable`) plus `RestoreReport::is_clean()`, by running the containment
leg's two passes in **separate** steps (a step reports the least certified of its loops, so one
loop's refusal was standing in for the other's), and by queueing one obligation for a genuinely
deleted chunk so the `Absent` arm — draining an obligation over a *complete* walk — is exercised
too.

**Result: 28 missed → 2**, and one of those two (`repoint_chunk`'s `>` → `>=`) is killed by the
last test added, verified by hand: flipping the operator in the source makes
`a_repoint_is_charged_on_growth_and_refused_at_the_value_ceiling` fail, and the source was restored
after the check. So **1 survivor stands**:

> `crates/core/src/metadata.rs:2846: replace != with == in resolve_chunk_homes` — the
> `current.state != InodeState::Committed` check on the **restart** path (reached only when a
> generation is superseded *during* the resolve). Killing it inside `wyrd-core` needs a store
> double that mutates between two reads (~45 lines in `metadata.rs`'s test module). Accepted, with
> the reason stated rather than hidden: it is the same rule `resolve_current_chunk_map` already
> enforces and #649 tests, on the same tested restart shape, and the consequence of the mutation is
> conservative (a restart landing on an uncommitted root would resolve it instead of answering "no
> live generation") rather than data-losing. It is the one row I would spend budget on next.

---

## 7. Verification

- **Red → green through the project's own runner**, re-run on the exact patch that ships:
  `PDCA_BUNDLE=… PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh`
  → `PASS — red without the fix, green with it`. With the five pass files, `metadata.rs` and
  `resolve.rs` reverted and the discriminator kept, **7 failed / 0 passed**; with the patch applied,
  **7 passed / 0 failed**. `--classify` confirms the single discriminator is
  `ADDED_TEST crates/custodian/tests/segmented_map_repair.rs`, as the brief requires.
- `./engine/xtask.sh ci` (`cargo xtask ci`: typos, docs render, link audit, gitlink/unsafe guards,
  fmt, clippy `--all-targets`, build, workspace tests, three dependency-wall checks, conformance,
  statics) → **all checks passed**.
- `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian` → 13 passed,
  including the new property 11.
- The formatter the target's commit hook runs (`cargo fmt --all`) has been applied to every touched
  file; `cargo fmt --all -- --check` is clean inside `cargo xtask ci`.

### The three forced questions

**(a) Genuine red?** Yes — *actually reverted and re-run*, not asserted. `run-verify.sh` resets
`../wyrd-verify` to `origin/pdca-integration/main`, applies the patch, then reverts every modified
production file and deletes the added non-test file (`resolve.rs`), keeping only the discriminator.
All 7 tests fail there:

```
restore_…                     SegmentedMapUnsupported { operation: "restore::committed_chunks" }
reconstruction_repairs_…      SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }
reconstruction_resolves_…     SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }
rebalance_evacuates_…         SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" }
a_repoint_…ceiling…           SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" }
backfill_…                    SegmentedMapUnsupported { operation: "backfill::reconcile" }
a_damaged_object_…            SegmentedMapUnsupported { operation: "reconstruction::find_chunk" }
```

Those reds are the base failing closed, which **is** defect (1). The assertions bind past that: each
leg's verdict is a positive state (a rebuilt fragment that verifies, a placement that moved to a
named server, byte-identity, an obligation still queued, a counted number of resolutions, a
`Blocked` outcome), so a hypothetical "silently skip segmented records" fix would fail them too —
which the ceiling leg makes concrete, since it asserts on destination stores being **empty** and on
the record being byte-identical rather than on any error.

**(b) Production path?** Yes. Every leg drives a real entry: `wyrd_custodian::reconcile_step` (the
fenced control point — reconstruction, rebalance, GC), `reconcile_after_restore`, and
`backfill::reconcile`, over the `MetadataStore` / `ChunkStore` **trait seams** with in-memory
doubles. No production logic is reimplemented in the test: placements are read back through
`metadata::resolve_current_chunk_map` (the production resolver), fragments are verified with
`repair::fragment_intact` (the production verify), and the erasure payloads are built with
`erasure::encode` + `write::encode_ec_fragment` (the production encoders). The DST property drives
`reconcile_step` under madsim with the race applied inside the production commit path.

**(c) Fixture includes the fault?** Yes, in every leg:
- criterion (1) seeds the fragments that would be stranded **and** a lapsed-grace GC pass that would
  destroy them, plus a genuine stray to prove the marker still marks;
- criterion (2a) deletes a real fragment (a real loss) and queues a real obligation; (2b) puts a real
  intact fragment on a server the operator really marked `Decommissioning`; (2c) seeds a record that
  is legal *and* has less headroom than the move needs, with the six destinations present in the
  fleet so the pass reaches the repoint rather than aborting earlier;
- criterion (3) keeps the segmented record in the same store as the fillable flat one, and the
  segmented chunk carries the empty placement the pass would otherwise fill;
- criterion (4) queues all Q = 9 obligations and asserts the queue was actually drained, so the
  counted resolutions are over a pass that assessed every one;
- containment seeds a genuinely unreadable object (`SegmentAbsent`: a segment the root names that was
  never written) **first** in key order, so the healthy object is only reached past it.

---

## 8. Files touched (12)

| File | Why |
|---|---|
| `crates/core/src/metadata.rs` | `ChunkHome`/`HomedChunk`/`SegmentHome`/`ResolvedChunkHomes`, `resolve_chunk_homes`, `repoint_chunk`, `SegmentRecord::repoint`, `check_value_ceiling`, `next_root_version`, 6 typed refusals; `read_segments` carries each row's exact bytes; unit tests |
| `crates/custodian/src/resolve.rs` (**new**) | the shared per-pass maintenance walk with per-object containment |
| `crates/custodian/src/reconstruction.rs` | per-pass `ChunkIndex` (defect 2), `Unassessable`, repoint-before-write, `Refused`, `Blocked` |
| `crates/custodian/src/rebalance.rs` | homed walk, containment, repoint-before-copy, `Refused`, `Blocked` |
| `crates/custodian/src/backfill.rs` | resolve, decline a segmented record with a stated reason, companion gauge, `Blocked` |
| `crates/custodian/src/restore.rs` | `committed_chunks` resolves through the walk; `RestoreReport::unresolvable` |
| `crates/custodian/src/desired_state.rs` | `PendingUnresolvable` (discharges the `// deferred: #651` marker) |
| `crates/custodian/src/lib.rs` | `mod resolve;` |
| `crates/custodian/tests/segmented_map_repair.rs` (**new**) | the discriminator: 7 legs |
| `crates/custodian/tests/segmented_map_consumers.rs` | #650's two drain-status assertions move to the attributed answer |
| `crates/dst/tests/custodian.rs` | property 11: a segmented repoint loses to a concurrent supersede |
| `docs/design/architecture/06-runtime-view.md` | §6.2 step 2: the repair/evacuation-walk sentences |
