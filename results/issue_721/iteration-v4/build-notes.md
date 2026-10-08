# Build notes — #721, iteration 4 (segmented-map repoint completes through reconstruction)

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 3's sign-off named **two substantive implementation defects** and explicitly scoped
this rebuild to them ("not the whole §6 list"). It also explicitly ruled two things **out**: the
rustdoc intra-doc link nits and the `cargo deny` / RUSTSEC-2026-0258 `h2` advisory. I did exactly
that: iteration 3's patch is the base of this one, plus the two fixes, plus the test coverage that
binds each, plus a size-reduction pass. No other production behaviour moved.

Line numbers below are **post-patch, in `$PDCA_WORKTREE`** (`/home/eddie/wyrd/wyrd.pdca-wt-l0`,
detached at `a801997`, the resolved target base for this cycle).

## Defect 1 — containment counted per *obligation*, not per *object*

**What was wrong.** Iteration 3 deleted `Reading::refused: BTreeSet<Vec<u8>>` (the base's
`origin/main` per-object dedupe at `reconstruction.rs:549-551`, `if reading.refused.insert(key)`
→ `emit_refused`) together with the segmented refusal it guarded, and routed the repair loop's new
typed-fault containment through `Reading::contain` — which emitted unconditionally
(`reconstruction.rs:322` in iteration 3). A segmented object with two queued chunks in **one**
`seg:` record therefore produced **two** `unresolvable-chunk-map` rows and two counter ticks for
**one** damaged record: an inflated counter and two repair obligations handed to an operator for
one stored row.

**The fix** (`crates/custodian/src/reconstruction.rs:408-424`): `Reading` carries
`contained: BTreeSet<Vec<u8>>` — the keys of every object this reading could not read — and
`contain` emits only on first insert (`:414-418`). The separate `incomplete: bool` is **gone**:
`incomplete()` (`:422-424`) is derived from that set, so "named" and "there is a hole" cannot
drift apart (one source of truth, and the mutant that returns a constant is killed by the drain
legs). Callers: `:352` (the drain gate) and `:363` (the certification gate).

I chose the set over "keep the bool and add a set" deliberately: two fields tracking the same fact
is precisely the drift that let iteration 3 ship a per-obligation count under a doc comment saying
"per object".

**Bound by** `crates/custodian/tests/segmented_map_repoint.rs:772`
(`a_torn_segment_record_is_contained_once_per_object_never_re_planned`): the fixture puts **two**
obligations in ONE `seg:` record and tears that record in the read→prepare window, so both plans
raise `SegmentRecordUndecodable` against the same object; the leg asserts exactly `(1, 1)` rows and
ticks, both obligations still queued, the record byte-identical, `Blocked`.

## Defect 2 — an unattributable committed row was silently skipped

**What was wrong.** Iteration 3's `read_committed` did `let Some(inode_id) = parse_inode_key(&key)
else { continue };` **after** resolving a committed record. On `origin/main` that skip only applied
to a **flat** map (a segmented one fell to `Site::Refused`, which set a hole and kept the
obligation). Once both shapes became writable the skip swallowed the segmented case too: the
reading stayed *complete*, so a chunk that committed map still references looked referenced by
**nothing**, `assess` answered `Drain`, and the obligation — the last record saying live data is
under-replicated — was **deleted**. That is the data loss this walk exists to prevent, reintroduced
through a bare `continue`.

**The fix** (`crates/custodian/src/reconstruction.rs:540-543`): route it through
`reading.contain(&key, UNATTRIBUTABLE_KEY)` — the fail-closed "never a silent skip" rule the rubric
names. The fault string is a const at `:465-466`, worded like core's own startup walk
(`unparsable-inode-key`, `crates/core/src/metadata.rs:2110`). The row is named once, nothing is
drained, and the pass answers `Blocked` until #698 makes such a row addressable again.

Deliberately unchanged: the check still sits **after** the `Committed` gate and after the resolve,
where the base put it — an uncommitted row cannot be the committed map that references a queued
chunk, so containing one would block every pass on a row that owes nothing.

**Bound by** `crates/custodian/tests/segmented_map_reconstruction.rs:650`
(`a_committed_row_whose_key_names_no_inode_is_contained_and_drains_nothing`): a **committed
segmented** object seeded under `inode:1-shadow` (`:228`, `seed_unattributable` at `:354`), its
chunk queued; the leg asserts the obligation is kept, the row named once, `Blocked`.

## The three forced questions

**(a) Genuine red?** Yes, three ways, all re-run against the final code:

1. **Against the base** (production reverted, tests kept — what C4-verify does): 7 tests ran in
   `segmented_map_repoint.rs`, **3 failed** — `a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record`,
   `a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged`, and
   `a_torn_segment_record_is_contained_once_per_object_never_re_planned`. Green post-fix (7/7).
   **Note for §6:** the brief predicted "5 tests ran, 2 failing". The file now carries **7** legs
   (the brief's 5 plus the extent-mismatch race and the torn-record containment legs that
   iterations 1–2's sign-offs required), and **3** are red because the containment leg names an
   action the base never emits for a segmented object. Read the gate's number as a count, as the
   brief says.
2. **Against iteration 3** (this rebuild's actual baseline), for each of the two fixes — see the
   negations below (a) and (b): each turns its leg red.
3. Every pin in the primitive still has its named negation red (below).

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point — over in-memory `MetadataStore`/`ChunkStore` doubles, and observes the **store** (and
the real `tracing` durability seam through a capture layer, the in-tree pattern from
`crates/core/tests/read_repair.rs`). The in-crate `metadata.rs` tests drive `repoint_chunk` over a
real `wyrd_metadata_redb::RedbMetadataStore::in_memory()`, not a hand-rolled map. No mock of the
behaviour under test exists anywhere in the patch.

**(c) Fixture includes the fault?** Yes. The repaired chunk's second fragment is genuinely absent
(server `LOST` is in no leg's fleet or topology); the racing writer genuinely lands inside the
read→prepare window and each racing leg asserts `meta.raced()` before asserting anything else — a
leg cannot pass because its race never happened; the torn record is genuinely undecodable (asserted
in the fixture); the unattributable row is a genuinely committed, genuinely resolvable object whose
**key** is the only thing wrong (the fixture asserts it resolves). Nothing that would exhibit the
fault is curated out.

## Named negations — demonstrated, not asserted

Each was applied to the final tree, the named test re-run, the pin restored. All seven went red:

| # | Negation | Leg that went red |
|---|---|---|
| a | `contain` emits unconditionally (drop the contained-key set) | `a_torn_segment_record_is_contained_once_per_object_never_re_planned` |
| b | restore the bare `continue` for an unparsable key | `a_committed_row_whose_key_names_no_inode_is_contained_and_drains_nothing` (`Satisfied`, obligation drained) |
| c | delete the `chunk == prior` equality in `chunk_at` | `a_racing_move_of_the_planned_chunk_itself_is_a_conflict_that_writes_nothing` |
| d | drop the root-generation precondition from the segmented batch | `a_superseded_root_generation_makes_the_repair_lose_without_writing_its_own_metadata` |
| e | weigh a `seg:` record against `MAX_VALUE_BYTES` instead of `MAX_ROOT_VALUE_BYTES` | `a_repoint_that_would_outgrow_a_segment_records_budget_is_refused` |
| f | delete the root-table extent comparison | `a_racing_rewrite_that_leaves_the_roots_segment_table_behind_is_a_conflict` |
| g | answer an undecodable `seg:` row with `Conflict` instead of the typed fault | `a_torn_segment_record_is_contained_once_per_object_never_re_planned` |

(c), (d) and (e) are the brief's required negations for legs 3, 4 and 5; (f) and (g) are
iterations 1–2's T5 requirements; (a) and (b) are this iteration's.

## Gate status

`./engine/xtask.sh ci` (the project's own gate, run in `$PDCA_WORKTREE`): typos, doc lint, doc
render, gitlink guard, unsafe guard, `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets` (`-D warnings`), build, `cargo test --workspace` (168 test binaries, **0 failures**),
`cargo-machete` — **all green**. The single failure is `cargo deny check` →
`error[vulnerability]: h2 unbounded empty DATA frames` (RUSTSEC-2026-0258, a transitive dependency
advisory). It is **identical to iteration 3's**, is not caused by this patch — the patch adds no
dependency — and iteration 3's sign-off explicitly placed it out of scope for this iteration. It
will fail C4-ci again until the advisory is addressed on its own ticket.

The patch is commit-ready for the target's hooks: `cargo fmt --all` was run over every touched file
and `clippy -D warnings` is clean.

## Budget — one deviation, stated plainly

| Budget (brief) | This patch |
|---|---|
| ≤ 4 files | **4** — `core/src/metadata.rs`, `custodian/src/reconstruction.rs`, `custodian/tests/segmented_map_repoint.rs` (new), `custodian/tests/segmented_map_reconstruction.rs` |
| ≤ 250 added semantic non-test lines | **193** (metadata.rs 114 outside `#[cfg(test)]`, reconstruction.rs 79; it also *removes* 86) |
| `patch.diff` ≤ 95 KB | **104.5 KiB (107,031 bytes)** — over, and over the driver's 100 KB backstop |

**The shape is inside budget; the evidence is what grew.** The slice is still 4 files and 193
semantic lines of production code. The bytes are prose and tests added *at the reviewers' request*
across rounds: ~380 lines of in-crate `#[cfg(test)]` tests in `metadata.rs` (iteration 1's C5
finding — mutants in `wyrd-core` are only killed by `wyrd-core`'s own tests, so the custodian
integration legs cannot reach them; iteration 2's T5 findings added the extent/version/decode
cases), and two extra race legs in the discriminator. This iteration adds ~1.4 KB of production
code + comments and ~3 KB of test for the two fixes.

I spent a real pass trying to land it under 95 KB and got ~3.3 KB back (merging the two
torn-record legs into one two-obligation leg; compressing ~40 comment blocks). The remaining
~10 KB is only reachable by deleting substance:

* the five redb-backed in-crate `repoint_chunk` tests in `metadata.rs` are ~15 KB of the diff —
  cutting them would take the patch to ~92 KB, and would re-open iteration 1's C5 finding verbatim
  (4 missed mutants at `metadata.rs:2859`), which is why I did **not** do it;
* the discriminator's extent-mismatch and torn-record race legs are ~4 KB — cutting them re-opens
  iteration 2's T5 findings;
* a `-U0` diff saves 3.6 KB but the harness applies patches with plain `git apply`
  (`engine/scripts/run-verify.sh:472`, `:509`, `publish.py`), which rejects zero-context patches
  without `--unidiff-zero`. Not viable.

So the honest options at sign-off are: accept the 104.5 KiB (clearing the size-backstop §6 item,
as the brief's own Budget note pre-judged the sizer a false positive for this child), or direct
which body of evidence to drop. I did not silently drop any of it.

## What I ruled out, and why

* **Testing the once-per-object rule on the flat arm** (a flat record at `version: u64::MAX`, whose
  two obligations both raise `VersionExhausted`) — no race hook needed, ~25 lines instead of ~90,
  and it lives in the file that already has audit capture. Rejected: the sign-off's requirement is
  specific — "a segmented object with two queued chunks in the same `seg:` record must produce one
  refusal tick, not two" — and the segmented route is the one that exercises the window this child
  introduced. I paid the ~45 lines of capture machinery in the new file instead (duplicated from
  `segmented_map_reconstruction.rs:165-209`, which is how this repo shares test doubles).
* **Folding the unattributable-row leg into the existing containment leg** (leg 3 of
  `segmented_map_reconstruction.rs`). I built it, measured it, and reverted it: folding *modifies*
  base lines, so the diff grew by 783 bytes versus a standalone leg that is pure addition, for a
  test that reads no better. Concrete numbers, not an adjective: folded region 13,581 bytes vs
  12,798 standalone; the final compact standalone leg is ~1.4 KB cheaper than the fold.
* **Checking `parse_inode_key` before `resolve_chunk_map`** (saves one wasted `seg:` page read for
  a pathological row). Rejected: it moves the check off the position the base gave it, changes
  which fault an operator sees for a doubly-damaged row, and buys nothing on any healthy store.
* **Touching the two "collectable garbage" comments** (`reconstruction.rs:934`, `:950`, and the audit line at `:1106`) — #723 owns
  that wording per the brief, so I also **reverted** iteration 3's rewrite of `emit_conflict`'s
  doc comment rather than keeping an improvement in this child's territory. (Kept minimal: the
  `RepairOutcome::Conflict` variant doc does state that a move settled at *prepare* time writes
  nothing at all, because that is a fact about the new code path, not a re-wording of the
  stranding note.)
* **Any doc/ADR edit.** `repoint_chunk` is a new public function in `wyrd-core::metadata`, not a
  port/RPC/CLI flag/persisted field — the on-disk shapes are unchanged — and the brief puts every
  ADR/spec/proposal edit (0016 included) out of scope with a 4-file cap. Flagging it here rather
  than acting on it.

## Known residue for the human

1. **C4-ci will be red on `cargo deny`** (RUSTSEC-2026-0258 `h2`). Pre-existing, unrelated,
   explicitly out of scope per iteration 3's sign-off.
2. **The size backstop will raise a §6 item** (104.5 KiB ≥ 100 KB). See the budget section for the
   measured trade; it also disqualifies auto-iterate, so a further Check round will stop for you
   rather than rebuild unattended.
3. The two rustdoc intra-doc link nits from iteration 3's review were left alone, as instructed.
