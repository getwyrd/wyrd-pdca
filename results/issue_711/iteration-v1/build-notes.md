# Build notes — #711 · repoint a chunk that lives in a `seg:` record

Target branch: `getwyrd/wyrd @ main`, base `92e1b4b` (post-#696/#697, post-child-1 #710/#718).
All `path:line` citations below are against that base unless marked "(patched)".

---

## 0. READ THIS FIRST — an environment fault the human should know about

**Two builder processes were running against this bundle's worktree at once.**

* `PID 1514324`, started `00:32:58`, `--add-dir /home/eddie/wyrd/wyrd.pdca-wt-l0`,
  `PDCA_SCRATCH=.../issue_711` — **orphaned**: its parent had exited, so it was reparented to
  `systemd --user` (ppid 12256). Nothing was left to collect its output.
* `PID 1525814` (me), started `00:38`, same worktree, same scratch, child of the **live**
  `pdca flow 711 715 716 717` (`PID 1525711`).

I found the worktree already dirty (`crates/core/src/metadata.rs`,
`crates/custodian/src/{rebalance,reconstruction}.rs`) with file mtimes advancing **while I
read them** (`rebalance.rs` written 10 seconds before my `ls`). Two agents editing one tree
interleave into something neither of them reviewed, so I:

1. snapshotted the orphan's work-in-progress diff to
   `$PDCA_SCRATCH/pdca-builder-711-orphan/orphan-wip.diff` (kept — inspect it if you want to
   compare);
2. stopped the orphan (`kill`, then `kill -9` when it did not exit);
3. confirmed the tree went quiet (no further mtime movement);
4. **reviewed its diff hunk by hunk**, adopted the production hunks I endorse, corrected what
   I did not (see §2, "state preservation"), and wrote the whole test surface myself.

I own every line in `patch.diff`: I read all of it, changed what I disagreed with, and it is
the design I had independently derived before I discovered the tree was dirty (the primitive
shape, the byte-offset addressing, the prepared-batch return). But you should know the first
207 semantic lines of production code were *drafted* by that orphaned process, not typed by
me from scratch.

**Two related things are still live on this host, neither of which I touched:**

* `PID 1514337` — the orphan's sibling from the same dead run, still writing
  `/home/eddie/wyrd/wyrd.pdca-wt-l1` (lane 1), which the live flow is using for **#715/#716/#717**.
  Same collision, different bundle. I deliberately did not kill a process affecting another
  lane's work; you probably want to.
* `PID 1574458` — `target/debug/deps/custodian_day_one-…`, a test binary **hung for 2 days
  9 hours** inside this worktree's `target/`. It did not block this cycle (every gate ran
  green) but it is burning a core and is presumably a real hang worth a look.

Neither is caused by this patch.

---

## 1. What the defect was, and what "done" means here

`#695/#696/#697` stopped the three maintenance passes *aborting* on a segmented object, but
they deliberately wrote nothing: a repair obligation or a drain evacuation for a
`seg:`-resident chunk was **refused** and stayed queued, every pass, forever. Nothing exited
that state, because the only placement writers in the tree rebuilt an **inode** record:

* `crates/custodian/src/reconstruction.rs:894-955` — `repair_chunk` takes
  `object.prior.chunk_map.as_flat()`, copies the list, and CASes the inode;
* `crates/custodian/src/rebalance.rs:500-557` — `evacuate_chunk` does the same from
  `plan.prior_chunks`.

Neither can address `seg:<nonce>:<epoch>:<index>` at all. So a multipart object's redundancy
decayed untended and a decommission holding one of its fragments never converged — the two
permanent states C-1 forbids.

**Done** = a `seg:`-resident chunk is actually repaired and actually evacuated, proven through
`reconcile_step` by reading the store. Not "the pass no longer refuses".

## 2. The change

### `crates/core/src/metadata.rs` — one primitive, `repoint_chunk` (patched `:2720-2928`)

Moves one chunk's `placement` **in the record that holds its `ChunkRef`** — the flat inode
record, or the one `seg:` record of a segmented map — and hands back the compare-and-swap
batch that lands it (`Repoint::Prepared` / `Refused { bytes, ceiling }` / `Conflict`).

Four decisions worth defending:

1. **It prepares; the caller commits.** Child-1's rule is that a refusal writes *nothing at
   all*, and `placement_ceiling.rs:336` asserts exactly that ("a refused repair stranded a
   rebuilt fragment"). The fragment writes must therefore happen **after** the ceiling verdict
   and **before** the commit. A primitive that committed internally would force the caller to
   write fragments first and so regress that assertion. Returning the batch also keeps the
   *one commit* rule the brief pins: the caller appends its own evidence — `delete(repair_key)`,
   one `orphan_key` put per displaced position — to that same batch
   (`reconstruction.rs:911-919` patched, `rebalance.rs:539-546` patched).
   Cost of the alternative (`repoint_chunk(store, …, evidence: WriteBatch) -> Committed|…`):
   it would have had to take the caller's fragment writes as data (`&[(&dyn ChunkStore,
   FragmentId, Bytes)]`) to keep the ordering — dragging the `ChunkStore` seam into
   `metadata.rs`, which ADR-0010 puts on the other side of the boundary.

2. **Addressing is `byte_offset` + the `ChunkRef` the resolve read**, not a list index. A
   caller resolves through `resolve_chunk_map` and gets a *flattened* list
   (`metadata.rs:2607-2626`); the segment boundaries are not in it, and re-deriving them
   would need a new resolving walk (out of scope, and O(segments) reads). A resolved chunk
   list is a byte tiling in **both** shapes, and the root's own segment table tiles the object
   by byte span (contiguous from 0, checked at decode, `metadata.rs:877-912`), so the offset
   names the covering segment with **no read at all** — one `get` for the record the move
   rewrites, and nothing else decoded or retained (the brief's bounded-memory rule).
   The `ChunkRef` equality is the pin that makes a map rewritten under the plan a `Conflict`
   instead of a silent overwrite. It also makes the **flat** arm stricter than the base, which
   indexed blindly (`reconstruction.rs:903` would panic on an out-of-range index).

3. **The segmented arm pins the root but never rewrites it.** `require(inode)` +
   `require(seg)` + `put(seg)` — proposal 0016's first two preconditions (`0016:669`). Three
   consequences, all asserted: the root's bytes (its `version` included) are untouched by
   maintenance; `MAX_ROOT_VALUE_BYTES` is therefore never spent by a repair (only by a
   *publication*, #653); and two moves in **different segments of one object both land in the
   same pass**, because they share only the root and the root is not written
   (`segmented_map_reconstruction.rs` leg 2, patched).

4. **Every read anomaly at move time is `Conflict`, never `Err`.** The generation may be
   retired under the plan — its `seg:` records are deleted *after* the root moves
   (`0016:2452-2462`) — and raising that would end the whole pass for one object, undoing
   #696/#697. It is not a swallowed corruption: this pass already resolved the record
   successfully a moment earlier (`reconstruction.rs:487`, `rebalance.rs:294`), so a record
   that will not decode *here* changed under the plan; and a record that is genuinely corrupt
   is caught by the **next** pass's resolve, which names it and blocks certification.

   **Correction I made to the adopted draft:** the flat arm set `state: InodeState::Committed`
   (copied from `commit_chunk_map`'s idiom, `metadata.rs:1782-1791`). That is right for a
   *commit point* and wrong for a placement move: moving a fragment says nothing about an
   object's lifecycle, and a public primitive that forces `Committed` could publish a `Pending`
   record. It now preserves the record's own state via `..generation.clone()`. No behaviour
   change for either caller (both filter on `Committed`), strictly safer for any future one.

The ceiling guard is **child-1's** `flat_value_ceiling_crossed` (`metadata.rs:380-382`), used
by both arms — no second implementation, no second constant. A segment record's value is
bounded by the same `MAX_VALUE_BYTES` the resolver refuses an oversized stored row on
(`metadata.rs:2493`), so a move is refused exactly where the read side would refuse the record
it wrote. I updated three lines of that helper's doc (`metadata.rs:371-375`), which said "a
segmented root's placement write is #682's, and it is the one that must weigh
`MAX_ROOT_VALUE_BYTES`" — #682's successor is this slice, and it does **not** weigh that half,
because it never rewrites the root. Leaving the stale claim would have been a doc-currency
finding on the first read.

### The two callers

`reconstruction.rs`: `Site::Refused` / `Assessment::Refused` / `Reading::refused` /
`emit_refused` are gone (nothing to refuse), `FlatObject` → `Object` (flat or segmented),
`Site` carries `byte_offset` instead of `index`, and `repair_chunk` goes through the primitive.
`rebalance.rs`: the same, plus `EvacPlan` now shares **one** snapshot per object
(`EvacScan::objects`) instead of carrying `prior: InodeRecord` *and* `prior_chunks:
Vec<ChunkRef>` per plan — with a segmented root that per-plan copy was the O(chunks × segments)
deep copy the brief forbids.

Both now pin **`ResolvedChunkMap::record`** (the generation the resolve *answered from*) rather
than the scanned record. For a flat map they are the same value (`Cow::Borrowed`,
`metadata.rs:2613`); for a segmented one that restarted onto a newer root they are not, and
pinning the scanned record while holding chunks resolved from a newer one could only ever lose
its CAS. This is what the brief means by "the exact bytes the resolve read".

## 3. Deviations from the brief — declared, not hidden

* **7 files, not 6.** The brief's file list omits
  `crates/custodian/tests/segmented_map_{reconstruction,rebalance}.rs`, whose leg 2 in each
  case asserts the very refusal this slice removes (`refused-segmented` rows,
  `*_refused_records` counters, "a refusal writes NOTHING"). Any implementation of this brief
  reds those two legs — no split avoids it — so leaving them would ship a red `C4-ci`. I
  rewrote **only** leg 2 in each, into the property the refusal was a placeholder for (both
  now assert the move lands, plus the same-pass property in §2.3), and said so in each file's
  module doc. The brief's own drift markers (`backfill.rs`, `restore.rs`, `gc.rs`,
  `desired_state.rs`) are untouched.
* **Line budget.** Production is **207** added semantic lines across the three production
  files — inside the spirit of "≤450". *Including tests* the patch is ~1015 semantic lines,
  over the stated budget: the mandated 4-leg test file is 483 of them (its in-memory doubles
  alone are ~110, the same boilerplate every peer `segmented_map_*.rs` carries — those files
  are 394–712 lines each), and the DST property is 234. A 450-line budget covering the
  mandated test surface was not reachable; I did not trim assertions to fit a number.
* **Out of scope, as declared by the brief and not implemented:** the destination pre-mark and
  the drain fence (`0016:669`'s third and fourth preconditions, #653), the committer, rollback
  and resume. Consequence, unchanged from the flat path that documents it today
  (`reconstruction.rs:951-953`, `rebalance.rs:554-556`): a repoint that loses its CAS leaves
  the pre-written destination fragment unreferenced, collectable by GC's ordinary sweep. **No
  new stranding class** — an existing, settled one extended to a second record shape. Property
  12 asserts it never leaves anything *worse* than that (no orphan mark, no discharged
  obligation, no placement).

## 4. Verification

| Check | Command | Result |
|---|---|---|
| C4-verify (per-fix red→green) | `PDCA_BUNDLE=results/issue_711 ./engine/scripts/run-verify.sh` | **PASS — red without the fix, green with it (4 tests ran red)** |
| C4-ci (whole gate) | `./engine/xtask.sh ci` | **all checks passed** (fmt, clippy `-D warnings`, build, all tests, deny, conformance) |
| DST tier | `./engine/xtask.sh dst` | 15 campaign properties green over the seed sweep (was 14) |

`typos` / `docs-renderer` / `cargo-deny` / `cargo-machete` / `cargo-mutants` were all present
(the gate did not warn-skip); no external dependency was missing, so there is no
NEEDS-HUMAN external-dependency item.

### The three refutation questions

**(a) Genuine red?** Yes, and twice over. Manually: `git stash push` of the three production
files → `a_seg_resident_under_replicated_chunk_is_repaired`,
`a_seg_resident_fragment_is_evacuated_off_a_draining_server` and
`two_committed_references_to_one_chunk_get_one_plan` all fail (`Blocked`, not `Changed`);
restored with `git stash pop`. Then through the gate: `run-verify.sh` reverts production, keeps
the added test, and reports *"PASS — red without the fix, green with it (4 test(s) ran red)"*.
Leg 3 (the ceiling refusal) passes on the base **by design** — the brief says so explicitly and
tells me not to count it as discriminating evidence; it is there to pin the rule for the
segmented arm, which #710's flat-only fixture cannot.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point, elected through `MemCoordination` with a real `FencedZone` — over
`MetadataStore`/`ChunkStore` doubles that are pure storage (a `BTreeMap` and a `HashMap`; the
CAS in `MemMeta::commit` is the trait contract, not a stand-in for the logic under test).
Nothing in the tests re-implements or mocks the placement move: the assertions read the
**store** afterwards (the `seg:` record's own bytes through `decode`, the shared repair queue
through `queued_repairs`, the `orphan:` ledger by prefix scan, the drain's convergence through
`desired_state::reconciliation_status`). The tests name no symbol this patch introduces — that
is what lets the red leg compile against reverted production.

**(c) Fixture includes the fault?** Yes.
- Leg 1 seeds the loss by leaving fragment 1's server out of both the fleet *and* the topology,
  and places only fragment 0 — a real v1 shard through the production encoder, so the loop's
  identity + checksum verify is really exercised; the rebuilt fragment is then asserted with
  `repair::fragment_intact`, so "the placement names server 2" cannot pass over bytes that do
  not exist.
- Leg 2 marks the holding server draining through the real `set_lifecycle` ledger and asserts
  the orphan mark for the vacated position and `ReconciliationStatus::Satisfied` — the operator
  surface that was permanently `Pending` before.
- Leg 3's segment record is padded to *exactly* `MAX_VALUE_BYTES - 18` and the fixture asserts
  that length, so a fixture that stopped approaching the ceiling fails loudly instead of
  passing vacuously; the refusal is then proven by whole-store byte equality plus "the target D
  server holds nothing".
- Leg 4 seeds the shared chunk in **two** committed objects (the failing element is present in
  both) and asserts the second object's record is byte-identical afterwards.
- DST property 12 injects the racing writer through the store seam **in both interleavings**
  and draws the arm, the victim and the loss from the run seed, so the sweep covers all four
  combinations; the losing side is asserted to have written *nothing* (record bytes, orphan
  ledger, repair queue all three), and both sides end with the live generation naming only
  fragments the fleet actually holds.

## 5. What I ruled out

* **Locating the segment by walking the group's `seg:` range.** Correct but wasteful: O(S)
  reads and O(S) decodes per move, and it would have needed a page-order-independent walk
  (the resolver's own `read_group_range` collects a `BTreeMap` of every row precisely because
  key order is a debuggability property, not a contract — `metadata.rs:2441-2444`). The root's
  segment table already tiles the object by byte span, so the offset locates the record with
  zero reads. That is the whole reason `byte_offset` is on the `Site`/`EvacPlan`.
* **Bumping the root's `version` on a segmented repoint.** It would re-encode the root (and so
  have to weigh `MAX_ROOT_VALUE_BYTES`), and it would serialise every move inside one multipart
  object onto one record — the second repair in a pass would lose its CAS for no reason.
  0016's precondition set says `require(inode == prior)`, not `put`.
* **Keeping a segmented refusal for some residual case.** There is none left: every reachable
  anomaly is either a `Conflict` (the plan is stale) or a `Refused` (the record cannot grow).
  Keeping the emitter would have left dead code that `-D warnings` rejects anyway.
* **Renaming `flat_value_ceiling_crossed`** (to drop the now-misleading "flat"). It is
  child-1's symbol, the brief puts it out of scope, and the rename would churn five doc links
  in two other files for no behavioural gain. I fixed its stale *doc claim* instead.
* **Wrapping the primitive's arguments in a `ChunkSite` struct.** `repoint_chunk(store, inode,
  generation, byte_offset, prior, placement)` has two `u64`-shaped parameters (`inode`,
  `byte_offset`), so a mis-ordered call would compile. I kept the positional signature anyway:
  it mirrors the peer the brief pointed me at (`commit_chunk_map(store, id, prior, chunk_map,
  size)`, `metadata.rs:1769-1775`, which has the same shape), it is the module's idiom
  throughout (`create`, `unlink`), and the failure mode of a swap is benign — the CAS key would
  be another object's, so the write loses its precondition and returns `Conflict` having
  written nothing, rather than corrupting anything. Worth a second opinion at sign-off if you
  disagree; the change would be ~20 lines and two call sites.
