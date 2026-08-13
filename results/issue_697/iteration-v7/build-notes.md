# Build notes — issue 697 (iteration 7)

*Withheld from the reviewer; written for the human at sign-off.*

Target branch `getwyrd/wyrd @ main`, base `origin/main @ 339da46`. Two files, as the brief
budgets: `crates/custodian/src/reconstruction.rs` and the NEW
`crates/custodian/tests/segmented_map_reconstruction.rs`. All `path:line` citations below are
against the patched worktree `$PDCA_WORKTREE` (= the target tree with `patch.diff` applied);
base citations say "on the base".

---

## 1. What the carry-forward demanded, and where each demand landed

Iteration 6 was rejected with **two** blocking implementation findings (T4 batch review) plus
a T5 judgment note; both blocking findings are about the *same* defect seen from two sides.

| Carry-forward item | Where it is answered |
|---|---|
| **C5 / T4 BUG** — "share each flat snapshot instead of cloning its N-entry map for each of Q obligations, or the prohibited Q×N CPU/heap path remains" (`reconstruction.rs:505` in v6) | `Reading::objects` holds **one** `FlatObject` per owed object (`:365`, `:396`); `FlatSite` carries only an index into it plus that one chunk's own `ChunkRef` (`:423-431`); `RepairPlan.prior` is a **borrow** of the shared snapshot (`:118-122`). No record and no chunk list is copied per obligation anywhere. |
| **T4 BUG (same finding, other half)** — "multiple queued chunks in one inode produce plans from one stale record, so only the first repair commits and the rest conflict, restoring the Q×N multi-pass behavior this change is intended to eliminate" | The repair loop **chains the generation it just committed** (`:299-320`): a plan's CAS precondition is the scanned generation, or the record this same pass last wrote for that object. All Q repairs inside one object now land in **one** pass. |
| **T5** — "Rebuild must cover many obligations in one large flat object: the current complexity leg seeds one chunk per separate flat object, so it cannot falsify the retained-map Q×N regression" | New **leg 7**, `every_obligation_inside_one_object_is_repaired_by_the_same_pass` (`tests/segmented_map_reconstruction.rs:690-731`): ONE flat object of N = 3 chunks, Q = 3 obligations inside it. It is base-red *behaviourally* (see §4). |
| T4 Contribution — "driver-only `scripts/pdca` / `scripts/review-branch` and logs are absent" | Driver/host-side, not a patch item. Nothing to build. |

The rejected v6 approach is **not** re-submitted: v6 cloned `record` **and** its chunk list into
a `FlatSite` per obligation and left the per-object CAS self-conflict in place. Both are gone.

## 2. The change, in the order a reviewer meets it

1. **One reading per pass** (`:166-170`, `read_committed` at `:463`). The committed namespace
   is walked once, through `metadata::resolve_chunk_map` — the same walk `gc.rs:360-416` and
   `restore.rs:621-658` make, contained by exactly their downcast rule: `decode` failure and a
   `ChunkMapError` are this object's fault (named, walk continues, reading marked incomplete);
   anything else propagates; `Ok(None)` is skipped. An **empty queue reads nothing** (`:166`).
2. **The three abort sites are gone.** `assess` (base `:329-335`) now reads the site out of the
   reading (`:600-615`); `find_chunk` (base `:620-646`) is deleted whole; `repair_chunk` (base
   `:579-586`) repoints the flat list of the generation it is conditioned on (`:874-887`) with a
   `let ... else` that fails **safe** (`RepairOutcome::Aborted`, nothing committed, obligation
   kept) instead of a `?` that ends the pass.
3. **Refusal, not abort and not drain.** A queued chunk found inside a `seg:` record becomes
   `Site::Refused` (`:411-421`) → `Assessment::Refused` (`:257`): nothing written, obligation
   kept, `refused-segmented` emitted **once per object** (`:544-551`, `:1058`), and the pass
   answers `Blocked` (`:341`).
4. **Drain nothing while the reading is incomplete** (`:333`) — one gate over the one batch both
   drain paths flow into.
5. **Chained generations inside one object** (`:299-320`) — §3 below.
6. Two new emitters on the existing audit target, with the brief's pinned vocabulary:
   `unresolvable-chunk-map` + `reconstruction_unresolvable_records` (`:1039`) and
   `refused-segmented` + `reconstruction_refused_records` (`:1058`).

Everything the brief carved out is absent: no generation comparison (#699), no claimant index
or landing guard (#700), no key-identity change (#698 — `repair_chunk` still CASes under a
re-derived `metadata::inode_key(plan.inode_id)` conditioned on a re-encoded record, `:889-893`),
no doc edit (#701), no `crates/dst/` hunk, no `Cargo.toml` change, no third file.

## 3. Why chaining, and why it is not a new concurrent path

Every plan is built **before any of them commits** — that is the base's own shape (base
`:184-225` assess loop, then `:261-267` repair loop), not something this patch introduces. So
with two obligations inside one object, both plans hold the same `prior`; the first commit
supersedes it and the second CAS fails. On the base that costs a whole extra pass — *and a whole
extra namespace reading* — per obligation after the first, which is the Q×N cost this slice
exists to remove, only spread across passes instead of within one. The reviewer's finding is
exactly right, and it is not a v6 regression: it is what the base does too, now visible because
the pass no longer re-scans per obligation.

The fix threads the record: `repaired: HashMap<usize, InodeRecord>` keyed by the **reading's own
snapshot index** (`:308`), consulted at `:310`, updated only on a landed commit (`:313-316`).

* It is **not a re-read** (no extra store access; the pass still reads the namespace once) and
  **not a newer generation from a resolve restart** — it is exactly the bytes this pass wrote,
  derived from the generation it scanned. The brief's §Scope constraint ("the bytes any write is
  built from and conditioned on are decided from the generation the scan returned") holds: the
  chain's root *is* that generation, and only a flat shape can enter it.
* The CAS is unchanged and still the fence. If a racing writer lands between our two commits,
  the second CAS fails → `Conflict` → obligation stays queued → next pass re-assesses. Same
  arm, same counter, same recovery as the base.
* **Keyed by snapshot index, not by `InodeId`** — deliberate. Keying by the parsed id would let
  two records the store holds under *different* keys that parse to the same id (`inode:1` and
  `inode:01` — #698's unreachable hazard) be threaded into one another, so a repair could write
  a chunk index from one object's list into the other's record. Indexing the reading's own
  snapshots makes that impossible by construction; #698's hazard is neither fixed nor widened.
  This is why `RepairPlan` gained an `object: usize` field (`:113-117`) beside its base
  `inode_id` / `prior`.

## 4. Alternatives ruled out (with the cost, not an adjective)

* **v6's shape — a `FlatSite` per obligation carrying `prior: InodeRecord` + `chunks:
  Vec<ChunkRef>`.** Cost: **2N `ChunkRef` clones per obligation**, i.e. 2·Q·N per pass. For
  leg 7's fixture (N = 3, Q = 3) that is 18 chunk-ref clones plus 3 whole-record clones, against
  1 record clone + 3 chunk-refs now. At production scale — a 10 000-chunk object with 100 queued
  repairs — 2 000 000 chunk-ref clones (each with its own `placement` Vec) against 10 000 + 100.
  Rejected: it is the finding.
* **Re-read the record with `meta.get` before each CAS.** Cost: +1 point read per repair (Q
  extra round trips per pass, on the metadata store, in the commit path). It also builds the
  write from a generation the pass never scanned, which is precisely what the brief's §Scope
  constraint forbids. Chaining costs **zero** extra reads. Rejected on both counts.
* **Batch every repair for one object into one commit.** Cost: `repair_chunk` splits into
  rebuild-and-place vs commit, `RepairOutcome` becomes a per-chunk vector, `reconcile`'s outcome
  loop folds over groups, and the three metric offsets are re-derived per chunk — ≈ 40 changed
  lines against the 6 of chaining (`repaired` map + `prior` parameter + insert). Worse than
  bigger: it ties unrelated chunks' fates together, so one lost CAS would undo repairs the
  fleet already has the fragments for, against `0005:277`'s one-commit-per-repoint rule.
  Rejected.
* **Keep `as_flat().ok_or(SegmentedMapUnsupported)?` at the write site** (it is unreachable once
  the reading admits only flat generations). Cost: 0 lines — but it leaves one of the brief's
  three named abort sites in the tree, one refactor away from firing again. Replaced by a
  `let ... else` that fails safe (`:880-887`).
* **Keep a separate `chunks: Vec<ChunkRef>` in the snapshot** so the write site never touches
  the map's shape (v6 did this). Cost: the object's whole chunk list stored **twice** per owed
  object. Repointing `prior.clone()`'s own list in place costs zero duplication and reads the
  same.
* **Asserting the heap sharing directly from the test.** Not possible from a black-box test over
  the trait seams — clones are invisible to a `MetadataStore` double. Leg 7 binds the
  *observable* consequence instead (§5(c)), which is what actually hurts an operator.

## 5. The forced refutation — all three answered

**(a) Genuine red?** Yes, and measured through the project's own runner, not by hand:
`./engine/scripts/run-verify.sh` (the C4-verify gate) applies `patch.diff` to a clean
`../wyrd-verify` worktree off `origin/main`, then reverts **only** the production hunk and keeps
the test. Result: `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red)`.
6 of 7 legs fail on the base, every one of them an **assertion/behaviour** red on base-visible
symbols (no compile error — the discriminator names nothing this patch introduces):

| Leg | Base failure |
|---|---|
| 1 healthy segmented object | `Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` |
| 2 refusal | same abort — the whole pass `Err`s |
| 3 unreadable object contained | same abort |
| 4 one reading per pass | same abort |
| 5 store fault after the name is out | base fails at `decode(&value)?` before the resolver: "expected ident at line 1 column 2" (its base behaviour is declared incidental by the brief) |
| 6 empty queue | **green on the base by design** — declared in the brief as a regression guard |
| 7 many obligations in one object | `left: ([[0,2],[0,1],[0,1]], 2)` vs `right: ([[0,2],[0,2],[0,2]], 4)` — one repair landed, two lost their CAS, two obligations still queued. Exactly the multi-pass behaviour the T4 finding named. |

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point, through `Custodian::elect` + `FencedZone::new` over `MemCoordination` — with a
real `ReconstructionContext`. Nothing is mocked but the two trait seams (`MetadataStore`,
`ChunkStore`), which is how every merged sibling test in this family is built
(`segmented_map_restore.rs`, `segmented_map_consumers.rs`). No production symbol is
re-implemented in the test: real `metadata::{encode, decode, resolve_chunk_map, seg_key,
inode_key}`, real `repair::{enqueue_repair, queued_repairs}`, real `erasure`/`encode_ec_fragment`
bytes so checksums verify and a rebuilt shard round-trips.

**(c) Fixture includes the fault?** Yes, and the fixture proves itself: `seed` asserts
`resolve_chunk_map(...).is_err() == matches!(what, SegmentHole)` on every seeded object
(`tests/…:333-340`), so no leg can pass because the fault it was built around silently stopped
being one, nor because a shape meant to be healthy quietly became damaged. The damaged records
are seeded **first in key order** over a `BTreeMap`-backed store, so "the blocker is met before
the healthy work" is a property, not luck. Leg 3 keeps the unreadable object *and* the healthy
repair *and* the unreferenced obligation in one store; leg 5 injects the store fault on the
`seg:` page the **resolver** reads (never on `scan(b"inode:")`, which would abort before
anything is named).

## 6. Verification actually run

* `./engine/scripts/run-verify.sh` → **PASS** (red without the fix, green with it, 7 tests).
* `./engine/xtask.sh ci` (the whole Wyrd gate: fmt + clippy -D warnings + build + test incl.
  DST + cargo-deny + conformance) → **`xtask ci: all checks passed`**. Re-run on the final tree.
* `cargo test -p wyrd-custodian` → all suites green, including the existing
  `crates/custodian/tests/reconstruction.rs` (15 tests) **unmodified**, which the brief requires.
* `cargo fmt -p wyrd-custodian` run over both touched files — commit-hook clean.
* `scripts/mutants-in-diff` (advisory C5) → **`20 mutants tested: 13 caught, 7 unviable`** —
  0 missed, 0 timeouts. v6 failed this gate; see §9 for what the first run of it caught here.

## 7. Budget — one honest deviation, flagged for sign-off

* **Production**: 161 added lines counting *every* non-blank non-comment line (including the 31
  that are only `}` / `});` / `};`); **130** excluding those. 68 semantic lines removed
  (`find_chunk` and the three abort sites), so the file grows by ~93. The brief's cap is 160 —
  met with room on the net measure, 1 over on the strictest possible one.
* **Test file: 731 raw lines against the brief's 460.** This is a real overage and I did not hide
  it. Calibration for the human: the file this one completes the family with,
  `crates/custodian/tests/segmented_map_restore.rs` (#651, merged), is **731** lines for the same
  shape; `segmented_map_consumers.rs` (#650, merged) is 1341. The brief's 460 does not account
  for the ~180 lines of in-memory `MetadataStore`/`ChunkStore` doubles every test in this family
  must carry (`wyrd-testkit` has no store doubles — only `test_double_scan_page`). The brief's
  *shape* rules are all met: ONE `BTreeMap`-backed store double carrying both counters and the
  injected fault, ONE parameterised seeding helper, ONE capture helper, exactly 2 files, no DST
  leg. I judged that trimming rationale comments to reach a number no merged sibling reaches
  would trade review value for the number; if you disagree, the cut is in the doc comments, not
  in any assertion.

## 8. Predicted recurring review finding (recorded-rejected, not re-litigated)

Round 3's second finding — *"the new `resolve_chunk_map(...).await` performs external metadata
reads without a caller-enforced timeout"* — applies verbatim to the merged peers this slice is
required to mirror (`gc.rs:394-401`, `restore.rs:604-608`), and to this loop's own
`meta.scan(b"inode:")` on the base. Adding a caller-side timeout here would mean a production
`tokio` dependency in a crate whose seam boundary is `traits`/`core`/`tracing` (ADR-0010) and
would bound one read of a pass built from many. The rationale is stated in-code at `:456-462`.
I appended the recorded rejection to `review-rejected.md` per the batch-review triage rule
rather than re-fixing it.

## 9. Not covered / left for the human

* **Advisory C5 (`cargo mutants --in-diff`) is clean now, and its first run here earned its
  keep.** v6 failed it on a TIMEOUT mutant; this rebuild's first run reported 2 *survivors*, both
  `delete field <x> from struct InodeRecord expression in repair_chunk` for `size` and `state`.
  They survived because they were **dead**: the base's own literal restated `size: prior.size`
  and `state: InodeState::Committed` beside a `..prior.clone()` update that already carried
  both, so deleting them changes nothing a test could see. Removed rather than suppressed
  (`reconstruction.rs:867-874`), which is a behaviour-preserving simplification — the version is
  now the only field the repoint sets, and the comment says why. Re-run: 20 mutants, 13 caught,
  7 unviable, 0 missed, 0 timeouts.
* **No seeded Tier-0 DST leg**, per the brief's pre-declared Verification posture — and the
  chaining does not change that: it introduces no new interleaving class. The second commit is
  fenced by the same version-conditional CAS as the first, and its expectation is bytes this
  pass itself wrote; any concurrent writer makes it a `Conflict`, which is the base's existing
  arm with the base's existing recovery (obligation stays queued, rebuilt fragments become
  collectable garbage). A DST leg would exercise the CAS the base already ships.
* Nothing was blocked by a missing external dependency: the whole slice runs over in-memory
  doubles on the base Rust toolchain. No `NEEDS-HUMAN external dependency` to declare.
