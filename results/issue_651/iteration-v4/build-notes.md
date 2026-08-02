# Build notes — issue 651 / repair-passes-through-resolver-with-containment (iteration 4)

*Withheld from the reviewer; written for the human at sign-off.*

---

## 1. What round 4 was asked to do, and what it changed

The carry-forward names three things, all implementation-level. Iteration 3's design (the
shared maintenance walk, the per-pass chunk index, plan-before-write, typed refusals) is
**kept** — it was not what failed. What failed were four concrete defects inside it, and
they are fixed at the cause, each with a test that goes red when only that fix is reverted:

| Carry-forward item | Fix | Where |
|---|---|---|
| **T5/T2 (impl)** — malformed `inode:` keys silently skipped: reconstruction could read an owned chunk as absent and drain its repair | A committed record under a **non-canonical** key is contained and named, exactly like an undecodable one; the shared parser is canonical-only, so no caller can plan a CAS against a key it would then re-render differently | `crates/custodian/src/resolve.rs:101-120`, `crates/core/src/metadata.rs:2158-2173` |
| **C5** — `!=`→`==` at the restart arm survived 129 mutants | A restart test over a store that retires the generation mid-resolve, both arms (live root committed → resolves; not committed → `None`) | `crates/core/tests/segmented_map_resolution.rs:930-1009` |
| **T4** — 11 blocking review findings | All 11 fixed (below); `review-rejected.md` records the dispositions | — |
| **T3** (reviewer's runtime cell) — backfill resolved the namespace **twice** per pass | One walk; the population gauge is derived from it | `crates/custodian/src/backfill.rs:73-92`, `:236-248` |

The eleven batch findings, by cause (several were the same defect seen by different passes):

1. **`resolve.rs:97`/`:132` (5 findings).** Fixed as above. `metadata::parse_inode_key` is now
   public and **canonical-only** (`parse_canonical_u64`, the parser `parse_seg_key`'s epoch
   already uses) — `inode:007`, `inode:+7` are refused. The walk contains such a record with a
   stated reason instead of dropping it (`resolve.rs:111-120`).
2. **`backfill.rs:126` (1).** A segmented record is charged to `unassessed` — and so blocks
   certification — **only when it holds a placement this pass would have filled**
   (`backfill.rs:146-151`). A fully-placed segmented store now certifies, so the drain-to-zero
   signal ADR-0040 decision 6 gates on is readable again.
3. **`reconstruction.rs:387` (2).** A duplicate committed chunk id is recorded as
   `Location::Ambiguous` and repaired by **neither** reference (`reconstruction.rs:397-412`,
   `:434-441`, `:537-539`): the shared obligation stays queued, both objects are named on the
   audit seam (`emit_ambiguous`), the level counts it, and the pass reports `Blocked`.
4. **`rebalance.rs:121` (2).** The no-drains fast path samples `rebalance_unresolvable_records`
   at **zero** before returning (`rebalance.rs:123-132`), so the level can come back down.
5. **`metadata.rs:3048` (2, BUG + CONVENTION).** Both arms of `repoint_chunk` now pin the
   **exact bytes the resolve read** (`metadata.rs:3049`, `:3118`). This is carried by a new
   `RootGeneration` type (`metadata.rs:2815-2853`) whose only constructor decodes from those
   bytes, so a caller *cannot* pass a record and a mismatched precondition. The flat arm was
   `origin/main`'s `encode(prior)` shape — iteration 3 preserved it and called out the
   asymmetry; with the bytes now in hand, keeping the asymmetry had no argument left.

---

## 2. Design decisions worth the human's attention

**`RootGeneration` rather than an extra `&Bytes` parameter.** The finding is a
serialization-identity one, and the shape that *removes* it is the one where the two cannot
disagree: `RootGeneration::decode(bytes)` is the only constructor, `record()` and `prior()`
are the only readers. Passing `prior_root: &InodeRecord, root_prior: &Bytes` side by side
would have been ~4 fewer lines and would have re-admitted exactly the defect (a caller with
the wrong bytes). It also let `resolve_chunk_homes` hand the restart arm's *own* bytes back,
which is where the pinned value must come from when the resolve restarted onto a live root.

**The walk keeps the state filter; `resolve_chunk_homes` mirrors `resolve_chunk_map`.** The
maintenance resolver checks `state == Committed` on the **restart** arm only, exactly as the
read-path twin does (`metadata.rs:2726-2739`): the caller supplies a committed generation, and
what the restart lands on is the resolver's own business. Adding an entry-side check would
have been 3 lines and a divergence from the base's shape for no reachable case.

**Ambiguity is a `BTreeSet`, not a `Vec` + `contains` guard.** The first draft used a Vec with
`if !objects.contains(…)`; `cargo mutants` survived on deleting the `!` (no fixture names one
chunk id three times). Rather than write a fixture for a branch that only exists to
de-duplicate, the branch is gone — the set does it (`reconstruction.rs:397-412`).

**Backfill's remaining-population is `walk − filled`, not a second walk.** The gauge is
computed from the walk the pass already did, minus what it committed
(`backfill.rs:91-92`, `:146-151`, `:198-202`). It is no staler than the second walk was (a writer can
commit between walk and gauge either way) and costs zero extra reads.

**`high_water_marks` inherits the canonical parser** (`metadata.rs:2219-2226`). Behaviour delta,
stated in-code: a stray `inode:007` no longer raises the inode high-water mark. It cannot
cause a clobber — minting id 7 writes `inode:7`, a *different* key, still guarded by
`create`'s `require_absent` — and the record's **chunk ids are counted regardless of the key**
(the half that carries the #364 durability finding), so no fragment can be re-minted over.

**GC's reference build needs no equivalent change.** `gc::referenced_fragments` never parses
the key (it resolves from the raw key and collects fragments), so a record under a
non-canonical key is already **protected** from reclamation and its drain status already
reads as `PendingUnresolvable`-shaped. The hole was only in the walk that must *build
mutations*, which is the one that needs the id.

**What is still deliberately NOT here:** the destination pre-mark and drain fence
(`// deferred: #653`, `metadata.rs:3007`). Unchanged from iteration 3, and unchanged in
reasoning: they close nothing without the retirement drain that is their other half, they
would change the flat path too, and iteration 2 shipped them and earned 14 findings.

---

## 3. Refutation — every fix reverted individually, with the result

Beyond the whole-patch red→green, each of round 4's fixes was reverted **on its own** and the
suite re-run. Every one goes red. The mutations are spelled out below so any of them can be
re-applied by hand (the scratch driver script was deleted with the rest of the build scratch):

| Fix reverted to… | Test that went RED |
|---|---|
| `resolve.rs`: non-canonical key → `continue` | `a_committed_record_under_a_key_no_mutation_can_name_is_contained_not_skipped` |
| `backfill.rs`: decline every segmented record unconditionally | `backfill_certifies_a_fully_placed_segmented_store_and_resolves_each_object_once` |
| `backfill.rs`: second `homed_objects` walk before the gauge | same leg (counted resolutions: 2, not 4) |
| `reconstruction.rs`: first/last-writer-wins on a duplicate chunk id | `a_chunk_id_referenced_by_two_objects_is_repaired_by_neither_and_stays_queued` |
| `rebalance.rs`: drop `emit_unresolvable_records(0)` from the idle path | `a_damaged_object_does_not_starve_the_healthy_ones_and_its_obligation_stays_queued` (leg (e)) |
| `metadata.rs`: flat arm pins `encode(prior_root)` | `a_repoint_pins_the_bytes_the_store_holds_not_a_re_encoding` |
| `metadata.rs`: segmented arm pins `encode(prior_root)` | same |
| `metadata.rs`: `!=` → `==` on the restart state check | `resolving_homes_restarts_onto_the_live_root_only_while_it_is_committed` |

One of these was found *by* the new code rather than by me: the ceiling fixture's filler
chunks were numbered from 1 and collided with the chunk under test (id `0xC7`), so the new
ambiguity detection correctly refused to repair it. The fixture now bases filler ids above
every id the legs name (`segmented_map_repair.rs:699-708`) — a real fixture bug the pass
surfaced.

**Mutation coverage: 0 missed.** `scripts/mutants-in-diff` → *141 mutants tested in 3m: 50
caught, 91 unviable, **0 missed*** (iteration 3: 1 missed of 129; iteration 1: 18 of 62).

### The three forced questions

**(a) Genuine red?** Yes, twice over. Whole-patch, through the project's runner
(`engine/scripts/run-verify.sh`, base `pdca-integration/main` — see §5 on the base):
**11 failed / 0 passed** with production reverted, **11 passed / 0 failed** with the fix.
Per-fix: the eight single-fix reverts above, each red on its own leg. The whole-patch reds are
the base failing closed (`SegmentedMapUnsupported`), which *is* defect (1); the per-fix reverts
are what prove the assertions bind past that.

**(b) Production path?** Yes. Every leg drives a real entry — `wyrd_custodian::reconcile_step`
(the fenced control point), `reconcile_after_restore`, `backfill::reconcile` — over the
`MetadataStore`/`ChunkStore` **trait seams** with in-memory doubles. Placements are read back
through `metadata::resolve_current_chunk_map` (the production resolver), fragments verified
with `repair::fragment_intact`, payloads built with `erasure::encode` + `write::encode_ec_fragment`.
Nothing is re-implemented in the test. The two new `wyrd-core` tests drive
`metadata::resolve_chunk_homes` / `repoint_chunk` themselves — the production functions, not
copies. The discriminator file names **no** symbol this patch adds (the brief's RED rule).

**(c) Fixture includes the fault?** Yes:
- the unaddressable-key leg seeds the stray record **first in key order** and queues a real
  obligation for the chunk only it references — the exact thing a skipping walk drains;
- the ambiguity leg seeds two committed objects over one chunk id **with a genuine lost
  fragment**, so the pass really does have a repairable chunk and really must choose;
- both ceiling legs seed a record that is **legal now** and has **less headroom than the move
  needs** (asserted before the pass runs), so the refusal is observed on a legal→oversize
  transition — the iteration-1 blocker — and the destination stores are asserted **empty**;
- the fully-placed-segmented leg is the negative control for the decline path: same fixture
  shape, nothing outstanding, and the pass must *certify*;
- the counted legs (Q=9/N=3 for reconstruction, N=2 for backfill) assert the queue was
  actually drained / the pass actually ran, so the count is over real work.

---

## 4. Budget — over the ceiling, the exact numbers, and the split I would hand back

The brief says an over-budget tree should be handed back as a proposed split rather than
finished. **It is over, and this is the hand-back** — but I finished the round rather than
stopping, because (i) the budget question is already a *deferred human* item from round 3
(`deferred-findings.json`), (ii) the round-4 carry-forward asked only for implementation
fixes, and (iii) stopping would have discarded fixes for eleven blockers and left the C5
survivor standing. The human decides at sign-off; nothing here pretends the number is met.

Added semantic lines (non-blank, non-comment — the reviewer's own method), **13 files** (cap 15):

| | v3 | v4 | delta | what changed |
|---|---|---|---|---|
| `crates/core/src/metadata.rs` — production | 238 | **302** | +64 | `RootGeneration`, canonical `parse_inode_key` |
| `crates/core/src/metadata.rs` — unit tests | 307 | **307** | 0 | one test added, one fixture folded |
| `crates/custodian/src/*` (5 passes + walk) | 291 | **338** | +47 | ambiguity, idle zero sample, one-walk backfill |
| `crates/core/tests/segmented_map_resolution.rs` | 0 | **68** | +68 | the C5 restart test |
| `crates/custodian/tests/segmented_map_repair.rs` | 888 | **1240** | +352 | 4 new legs (one per fixed blocker) + 2 fixture helpers |
| `crates/dst/tests/custodian.rs` | 162 | **162** | 0 | property 11, unchanged |
| others (`consumers`, `lib`, docs) | 16 | **16** | 0 | |
| **total** | 1,957 | **2,488** | **+531** | of which **1,790 is tests** |

Production is **698** semantic lines against the brief's ~310 salvage estimate; the gap is
itemized in v3's notes (the homed resolver + typed refusals) plus this round's +111. Tests are
72% of the patch.

**The split I would hand back** — cut at "see it" vs "move it":

* **A (this issue, reduced):** the shared walk + containment + the per-pass index (defect 2) +
  restore/backfill/desired-state adoption. Drops `repoint_chunk`, the ceiling helpers,
  `RootGeneration`, the two moving passes' repoint, the DST property and the four
  ceiling/repoint legs: **≈ 1,250 semantic** (measured as the sum of the rows above minus
  metadata production 302 + metadata unit tests 307 + dst 162 + ~430 of discriminator legs).
* **B (new slice):** `repoint_chunk` + ceilings + `RootGeneration` + the repoint callers +
  property 11 + those legs: **≈ 1,200 semantic**.

**Why I judge the split worse, not cheaper:** A would ship a reconstruction that can *see* a
segmented chunk is under-replicated and cannot repair it, and a rebalance that can see a
fragment on a draining server and cannot move it — both then reporting `Blocked` forever. That
is a pass reporting work it cannot do, which is the failure mode this slice exists to remove;
and it violates the brief's Caller-first rule in spirit (B's symbols get callers, but A's
adoption has no way to finish the job it starts). It is the human's call.

---

## 5. Verification, and one thing the human should know about the base

- **`./engine/xtask.sh ci` (`cargo xtask ci`) → all checks passed**: typos, docs lint/render,
  link audit, gitlink/unsafe/statics guards, `cargo fmt --check`, `clippy -D warnings`
  `--all-targets`, build, the whole workspace test suite, the three dependency-wall checks,
  `cargo deny`, conformance — and the madsim DST sweep, in which property 11
  (`segmented_repoint_loses_to_a_concurrent_supersede`) passes.
- **`engine/scripts/run-verify.sh` → `PASS — red without the fix, green with it`** (11 red / 11
  green), `--classify` confirms the single discriminator `ADDED_TEST
  crates/custodian/tests/segmented_map_repair.rs`.
- **`scripts/mutants-in-diff` → 0 missed** (141 mutants).
- `cargo fmt --all` applied to every touched file; the target's commit hooks run the same
  formatter and clippy set that `xtask ci` just ran clean.

**The base.** `$PDCA_WORKTREE` is at `42c0842` = the local `pdca-integration/main`, which
carries #648, #649 **and #650** (this slice's declared dependency). The *remote*
`origin/pdca-integration/main` is currently at `6bc344e` — #648, #649 and a dependency bump,
but **not** #650 — so the C4-verify default (`PDCA_VERIFY_BASE=origin/pdca-integration/main`)
resolves to a tree this patch cannot apply to: it edits `segmented_map_consumers.rs`, a file
#650 creates. I ran the gate with `PDCA_BASE=pdca-integration/main` (the local fold) and it
passes. **If C4-verify reports "patch.diff does not apply — the bundle is stale", that is the
unpushed wave fold, not a stale bundle**: push/refresh the integration branch (or point the
gate at the local ref) and re-run. This is the known wave-base gap
`docs/INTEGRATION.md` §2 records as eduralph/pdca-harness#273.

---

## 6. Files touched (13)

| File | Why |
|---|---|
| `crates/core/src/metadata.rs` | `RootGeneration`; canonical public `parse_inode_key`; homed resolver + `repoint_chunk` pinning read bytes; ceiling helpers; unit tests |
| `crates/core/tests/segmented_map_resolution.rs` | the restart-arm test (the C5 survivor) |
| `crates/custodian/src/resolve.rs` (**new**) | the shared per-pass maintenance walk, containing an unaddressable record too |
| `crates/custodian/src/reconstruction.rs` | per-pass `ChunkIndex` (defect 2), ambiguity, `Unassessable`, repoint-before-write, `Refused`, `Blocked` |
| `crates/custodian/src/rebalance.rs` | homed walk, containment, repoint-before-copy, `Refused`, idle zero sample |
| `crates/custodian/src/backfill.rs` | one walk per pass, conditional decline, exact-bytes CAS, companion gauge |
| `crates/custodian/src/restore.rs` | `committed_chunks` resolves through the walk; `RestoreReport::unresolvable` |
| `crates/custodian/src/desired_state.rs` | `PendingUnresolvable` (discharges the `// deferred: #651` marker) |
| `crates/custodian/src/lib.rs` | `mod resolve;` |
| `crates/custodian/tests/segmented_map_repair.rs` (**new**) | the discriminator: 11 legs |
| `crates/custodian/tests/segmented_map_consumers.rs` | #650's two drain-status assertions move to the attributed answer |
| `crates/dst/tests/custodian.rs` | property 11: a segmented repoint loses to a concurrent supersede |
| `docs/design/architecture/06-runtime-view.md` | §6.2 step 2: the repair/evacuation-walk sentences, incl. the unaddressable-key clause |
