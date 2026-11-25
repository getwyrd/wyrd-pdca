# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Attempt 7. All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`, base `6e44de9` =
`pdca-integration/main-r9dd123c9ed70`, which carries #1013 and #1015) with this patch applied,
unless marked *base* or *v6* (`iteration-v6/patch.diff`).

This attempt starts from v6's patch, which applies cleanly on the base, and changes only what the
iteration-6 sign-off asked for. The design is unchanged: destination pre-mark, write gate with a
D-server deadline, adoption that consumes the pre-mark, per-move nonce. The rationale for the
design is in `iteration-v4/build-notes.md` §2–§6, `iteration-v5/build-notes.md` §2–§4 and
`iteration-v6/build-notes.md` §2–§5, and is not repeated here.

## 0. What this attempt changed

| Carry-forward item | What I did | Where |
|---|---|---|
| **F1: a lost move must not certify** | A planned repair that lost its race now withholds certification, whatever moved the record it was pinned to. One rule in the pass loop: the `RepairOutcome::Conflict` arm sets `refused`, so the pass answers `Blocked`. The loss is still named `conflict` on the audit seam and counter. D6 requires `Blocked`, on both phases (lost at the pre-mark, lost at the adoption) and both shapes. Four existing tests asserted the old answer and now assert `Blocked` (§2.3). | `reconstruction.rs:494-502`; `move_destination_marks.rs:1706-1797`, `:1807-1845`, `:2214-2220`; `segmented_map_repoint.rs:522-548`, `:660`, `:797`; `segmented_map_reconstruction.rs:644-649`; `staged_repair.rs:1891-1895` |
| **`seg:` pin regression guard** | One leg per mover on the segmented shape: a sibling chunk's repoint rewrites the `seg:` record (root byte-identical) just before the pre-mark commit. Asserts server 2 was sent no write and holds no mark. Kills the "pin only the root" mutation (§3). | `move_destination_marks.rs:1851-1895`, helper `:1125-1146`; `rebalance.rs:2470-2513`, helper `:1973-1995` |
| **`Assessment::Refused` comment** | Corrected: a `reclaiming` mark on a position the placement still names does not clear by itself, because GC reclaims no referenced position; points at #1083. Refusal behaviour unchanged. | `reconstruction.rs:810-821` |
| **Optional: mixed-result leg** | Taken, one leg per mover on both shapes: two writes sent, one lands, one is held to its deadline and refused. Asserts the whole move refuses (§5). | `move_destination_marks.rs:2401-2452`; `rebalance.rs:2846-2901` |

Beyond the four items, three small things each forced by F1:

- `06-runtime-view.md:40` gained one sentence: a repair that loses to a root that moved is named a
  conflict and is not certified over (the paragraph lists which repairs do not certify, and the
  list changed).
- Two doc comments say where the rule reaches (`RepairOutcome::Conflict`, `reconstruction.rs:1137-1147`;
  the staged re-place's outcome list, `reconstruction/staged.rs:460-466`).
- `review-rejected.md`: one entry re-pinned, line numbers refreshed (§6).

Size of this attempt's change over v6, measured by rebuilding v6's files in scratch
(`v6tree/`) and diffing each (changed lines, added plus removed):

| File | Lines | Of which code |
|---|---|---|
| `src/reconstruction.rs` | 81 | 5 (the arm: 1 removed, 4 added); the rest comments |
| `src/reconstruction/staged.rs` | 7 | 0 (doc comment) |
| `src/rebalance.rs` | 117 | 0 production; all in the inline test module |
| `tests/move_destination_marks.rs` | 247 | test |
| `tests/segmented_map_repoint.rs` | 27 | test |
| `tests/segmented_map_reconstruction.rs` | 5 | test |
| `tests/staged_repair.rs` | 6 | test |
| `06-runtime-view.md` | 2 | doc |

Every other file is byte-identical to v6. `premark.rs` is untouched. The patch touches the same 25
files as v6.

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base, so I built: `reconcile_step` calls `retirement::reconcile`
(`crates/custodian/src/reconciliation.rs:187`), and `wyrd-metadata-redb` is a custodian
dev-dependency (`crates/custodian/Cargo.toml:65`). No dependency or drain added. `Cargo.lock`,
`deny*.toml` and every `Cargo.toml` are untouched.

## 2. F1 — a lost move never certifies

### 2.1 The defect, as the sign-off stated it

`lost()` (*v6* `reconstruction.rs:1368-1378`) answered `RepairOutcome::Conflict` whenever the
root's bytes had changed. The pass loop treated `Conflict` as "not a hole", so a pass whose only
repair lost to *any* root change answered `Satisfied` — with the fragment still missing and the
obligation still queued. A root that moved does not show the chunk was repaired: a sibling
chunk's repoint rewrites a flat root and leaves this chunk exactly as broken as before.

### 2.2 The change

```rust
// reconstruction.rs:499-502
RepairOutcome::Conflict => {
    refused = true;
    emit_conflict(plan.chunk_id);
}
```

`refused` is a term of `hole` (`reconstruction.rs:559`), so the pass answers `Blocked`. `lost()`
is unchanged in what it returns (`reconstruction.rs:1389-1399`): it now decides only how the loss
is *named* — `conflict` when the root moved, `aborted` with `pre-mark-lost` / `adoption-lost` when
it did not. Neither answer certifies. This is the rule evacuation already applies to every move
that did not persist (`rebalance.rs:203`, `EvacOutcome::persisted`, `:531-533`), which is the
first of the two options the sign-off offered.

The rule sits on the outcome, in the loop, so it covers every way a planned repair answers
`Conflict`, not only `lost()`:

| Where `Conflict` comes from | Before | Now |
|---|---|---|
| Pre-mark lost, root moved (`lost`, called `:1340`) | `Satisfied` | `Blocked` |
| Adoption lost, root moved (`lost`, called `:1369`) | `Satisfied` | `Blocked` |
| The move found the plan stale as it prepared it (`Repoint::Conflict`, `:1312`) | `Satisfied` | `Blocked` |
| A ceiling refusal or exhausted version weighed on a generation the root has left (`:1304-1308`) | `Satisfied` | `Blocked` |
| A staged re-place's lost adoption (`reconstruction/staged.rs:631`) | `Satisfied` | `Blocked` |

### 2.3 Existing tests that asserted the old answer

Four, found by running the whole custodian suite after the one-line change (six failures: these
four and the two D6 legs; `f1-custodian.log`). Each encoded "a lost CAS is a retry, not a hole",
which the sign-off overturned.

| Test | Was | Now | Why it is the F1 case |
|---|---|---|---|
| `segmented_map_repoint.rs::a_superseded_root_generation_makes_the_repair_lose` (`:660`) | `Satisfied` | `Blocked` | Its "supersede" is the same root with `version + 1`: the chunk map is unchanged and the fragment is still missing. This is the case the iteration-6 batch row describes. |
| `…::a_racing_move_of_the_planned_chunk_itself_is_a_conflict` (`:639`, through `assert_lost`, `:522`) | `Satisfied` | `Blocked` | A racer changed this chunk's reference before the move's own read. The pass did not check that the racer's placement is whole. |
| `…::a_ceiling_refusal_on_a_superseded_generation_is_a_conflict_not_a_ceiling_fault` (`:797`) | `Satisfied` | `Blocked` | Renamed from `…_not_a_block`: the point of the test is that the stale refusal is named a conflict and not a ceiling fault, which still holds (`ceiling_refused` 0, `conflict` 1). The pass verdict is no longer what tells them apart. |
| `segmented_map_reconstruction.rs::the_committed_namespace_is_read_once_per_pass` (`:631`) | `Changed` | `Blocked` | Four obligations inside one flat object: the first repoint lands, the other three lose the CAS it superseded and are still owed. |

`segmented_map_repoint.rs::torn_under_the_move` (`:860`) did not fail, but one of its "named
negation" comments said a conflict would read `Satisfied`; I corrected the comment.

### 2.4 Three things the human should know

**A dated decision is overturned.** `assert_lost`'s comment carried "a lost CAS is a retry, not a
hole (2026-08-19)". The iteration-6 sign-off says the opposite for the committed mover, and I
followed the sign-off. If the 2026-08-19 ruling was recorded somewhere outside the code (an ADR, an
issue), it needs the same correction; I searched `docs/design/architecture` for the certification
wording and found only `06-runtime-view.md:40`, which this patch updates.

**A flat object with several degraded chunks now reads `Blocked` on every pass but the last.** One
repoint per flat object lands per pass (that part is the base's, unchanged); the others lose the
CAS and stay queued. The pass used to answer `Changed`; it now answers `Blocked`, because it still
owes those repairs. Convergence is unchanged — same number of passes, same writes. Nothing in the
deployed loop branches on the verdict: `Reconciled::` is read only by the reaper's one-shot CLI
(`crates/server/src/cli.rs:1126-1138`). The server suite passes unchanged (`f1-server.log`, 53
binaries).

**The staged re-place's lost adoption is covered too.** It answers the same `RepairOutcome::Conflict`
and goes through the same arm. I kept it in for the same reason as last round's `Assessment::Blocked`
(which the staged path also shares, and the sign-off accepted): the invariant — no pass reports
redundancy restored while a repair it owes is undone — does not depend on which record names the
chunk. The existing staged test for a lost adoption asserted only `!= Changed`; it now asserts
`Blocked` (`staged_repair.rs:1891-1895`), and goes red if the arm stops applying (§7.3). The staged path's *aborts* (pre-mark lost, stale, write
refused) still answer `Satisfied`; that is unchanged and listed under residuals (§10).

### 2.5 What I ruled out

**(a) Re-resolve the chunk and certify only when the current reference no longer owes the
repair** — the sign-off's second option. Sketch:

```rust
/// Whether the object's current generation still names the chunk on the servers the plan
/// found missing.
async fn still_owed(meta: &dyn MetadataStore, object: &Object, plan: &RepairPlan) -> Result<bool> {
    let key = metadata::inode_key(object.inode_id);
    let Some(bytes) = meta.get(&key).await? else { return Ok(false) };          // deleted
    let Ok(root) = metadata::decode::<InodeRecord>(&bytes) else { return Ok(true) };
    let resolved = match metadata::resolve_chunk_map(meta, &key, &root).await {  // every seg: record
        Ok(Some(resolved)) => resolved,
        Ok(None) => return Ok(false),
        Err(fault) => /* downcast ChunkMapError → true, else Err */,
    };
    let Some(now) = resolved.chunks.iter().find(|c| c.id == plan.chunk_id) else { return Ok(false) };
    Ok(plan.missing.iter().any(|&i| now.placed_dserver(i as u16) == Some(plan.placement[i])))
}
```

About 25 lines for the function, 3 call sites (`:1308`, `:1312`, and inside `lost`), and about 10
more to turn a resolve fault into a non-certifying answer: roughly 40 lines of production code
against the 4 I shipped. Not taken, and size is not the main reason:

- It answers the same `Blocked` for every case the sign-off named (a sibling's repoint, a version
  bump with the chunks unchanged). The two differ only when the object was superseded or the chunk
  was re-placed by someone else — and there the next pass gives the exact answer anyway (it drains
  the obligation; `d6_superseded_before_the_premark` asserts this, `move_destination_marks.rs:1839-1844`).
- `resolve_chunk_map` flattens every `seg:` record of the object, so a lost move in a large
  segmented object would read the whole map once per loss. A bounded lookup would be a new public
  function in `wyrd_core::metadata`, a file this child does not otherwise touch.
- "The placement changed at that index" is not "the fragment is there": certifying on it trusts
  the racer's word. Only a pass that fetches the fragments can say the chunk is whole, and that is
  the next pass's assessment.

**(b) Fix `lost()` alone, by answering `Stopped` for both cases.** Sketch:

```rust
-        return lost(ctx.meta, object, "pre-mark-lost").await;
+        return Ok(RepairOutcome::Stopped("pre-mark-lost"));
 …
-    lost(ctx.meta, object, "adoption-lost").await
+    Ok(RepairOutcome::Stopped("adoption-lost"))
-async fn lost(…) { … }            // 11 lines, plus its 17-line doc
```

About 30 lines *fewer* than what I shipped. Not taken, for two reasons. It leaves the same defect
30 lines up: `Repoint::Conflict` at the prepare (`reconstruction.rs:1312`) would still certify, and
that is reachable on a segmented object when a competing writer changes this chunk's reference
before the move's own read — `a_racing_move_of_the_planned_chunk_itself_is_a_conflict` drives it.
And it renames a CAS lost to a supersede from `conflict` to `aborted` on the audit seam and moves
it from `reconstruction_conflict` to `reconstruction_aborted`, which changes an operator-facing
counter and breaks the naming assertions of three existing tests
(`segmented_map_repoint.rs:544-548`, `:698-702`, `:833-841`).

**(c) Exempt the staged re-place.** Sketch: move the flag out of the arm and next to the committed
call.

```rust
 Target::Committed(target) => {
     let object = &reading.objects[target.site.object];
-    repair_chunk(ctx, &stores, object, target, plan).await?
+    let outcome = repair_chunk(ctx, &stores, object, target, plan).await?;
+    refused |= matches!(outcome, RepairOutcome::Conflict);
+    outcome
 }
```

Three lines instead of one, to keep a `Satisfied` the invariant says is wrong. Not taken (§2.4).
If you want the staged path left exactly as it was, this is the mechanical change, and
`staged_repair.rs:1891` goes back to `assert_ne!(outcome, Reconciled::Changed, …)`.

**(d) Making an `unplaced` abort non-certifying too.** Same position as last round
(`iteration-v6/build-notes.md` §2.3): one line, not asked for, would change the base's test for
that abort. Still a residual (§10).

## 3. The `seg:` pin guard

`premark::premark` folds the repoint batch's own preconditions into the pre-mark batch
(`premark.rs:465-471`): the root generation and, for a segmented map, the `seg:` record. v6's
`d6_superseded_before_the_premark` kills the mutation that drops *all* the pins, but a supersede
moves the root, so nothing failed if only the root pin was kept.

The new legs rewrite the `seg:` record and nothing else. `Rig::sibling_repoint`
(`move_destination_marks.rs:1128`, `rebalance.rs:1977`) reads the object's first segment record,
moves the first filler chunk from server 3 to server 4 (and moves its fragment in the fleet, so the
record stays true), and returns the re-encoded record. The leg arms `premarks_free` with
`Act::PlantFirst` of that record, so it lands after the move prepared its repoint and just before
its pre-mark commit.

Each leg asserts: the `seg:` record holds the sibling's bytes; the root is byte-identical; server
2 was sent no write; server 2's position has no mark; the placement still names server 5; the pass
answers `Blocked`; a later pass moves the chunk; and the sibling's edit survived that later move.
The repair leg also asserts the audit names `aborted` / `pre-mark-lost` (the root did not move, so
this is not a `conflict`).

Mutation `seg-pin-dropped` (`.take(1)` on the pin fold, which keeps the root pin and drops the
`seg:` pin): exactly these two legs fail, both at "sent a write" / "sent a copy"; the other 132
tests in the two targets pass (`mut-seg-pin-dropped.log`).

## 4. The `Assessment::Refused` comment

v6 said "It clears when GC finishes the reclaim, or a human the mark". That is false for one case:
a `reclaiming` mark on a position the placement still names (the destination of a rebuild in
place). GC reclaims no referenced position, so it never finishes that reclaim, and the refusal
repeats every pass. The comment now says so and names #1083 (`reconstruction.rs:814-820`). No code
changed; the refusal is the brief's D4.

## 5. The mixed-result legs (optional item, taken)

`premark::refusal` (`premark.rs:600`) folds every write's answer into one verdict. v6 had legs with
all writes landing (the swap legs) and legs with one write refused (D7b, D7c, D9), but none with
both in one move.

`mixed` reuses the swap legs' fixtures: an RS(2,2) chunk missing two fragments for repair
(`move_destination_marks.rs:2406`), two servers draining for evacuation (`rebalance.rs:2851`). Both
writes are sent together; the double holds one in flight while the clock moves to its deadline,
then refuses it; the other has landed. Asserts: the landed fragment is there; the refused one is
not; the placement is unchanged; the pass answers `Blocked`; the audit names
`write-deadline-expired`; the landed fragment is still under its pre-mark; a later pass moves both.

Mutation `first-answer-only` (`refusal` judges only the first answer): all four legs fail at "a
move with one refused write adopted", the placement naming a server that holds nothing
(`mut-first-answer-only.log`).

I judged it cheap: about 55 lines per mover with its doc comment, and no new fixture. It also turns the iteration-6 batch's third
blocking row (`premark.rs:600` TEST-GAP) into a fix instead of a recorded rejection.

## 6. The iteration-6 review batch (3 blocking rows) and `review-rejected.md`

| Row | Disposition |
|---|---|
| `reconstruction.rs:1376` BUG (a root version bump certifies) | Fixed (§2). |
| `premark.rs:600` TEST-GAP (no mixed-result test) | Fixed (§5). |
| `premark.rs:167` BUG (a `reclaiming` mark on a still-referenced position blocks an in-place repair for good) | The sign-off's decision: doc fix only, refusal unchanged, point at #1083 (§4). Recorded-rejected. |

The third row already had an entry, but the reviewer reworded the finding and the old MATCH phrase
("permanently ineligible for in-place repair") no longer occurred in it, so it blocked again. I
re-pinned the entry's MATCH to `reclaiming`, the one word every wording of this finding has
carried, and added the sign-off's ruling to its comment and reason.

**The MATCH is broad on purpose, and you may want it narrower.** A decision binds only to the one
finding nearest its recorded line, so it cannot clear two findings. But a *different* BUG reported
near `premark.rs:167` whose text contains "reclaiming" would be cleared by it. The alternative is a
longer phrase from this round's wording ("blocked indefinitely"), which is safe against that and
will stop matching the next time a reviewer rewords it — as has now happened twice.

Other edits to the file: line numbers moved to the iteration-7 tree; the #682 entry now names the
new mixed legs; a closing note says how each of the three rows was handled.

Dry run against the iteration-6 batch with the gate's own matcher (`scripts/review-branch`,
`load_rejected` / `is_rejected`, driven by `pdca-builder-1084-i6/check_rejections.py`): the
`premark.rs:167` row resolves as rejected; the other two do not, as they should not, because they
are fixed rather than recorded.

**Still open from last round, unchanged:** the drain-fence entry (`premark.rs:550`) has no open
tracker issue behind it (#653 is closed). The draft issue text is in
`iteration-v6/build-notes.md` §6. So is the suggested widening of #1100's title and body (§5
there). Both are the human's to file.

## 7. Tests

### 7.1 Counts

- `crates/custodian/tests/move_destination_marks.rs`: **55** tests (v6: 50). New: D6 lost at the
  pre-mark ×2, the `seg:` pin leg ×1, mixed ×2. Changed: D6 lost at the adoption ×2,
  `d6_superseded_before_the_premark` ×2, D10a lost adoption ×2.
- Inline evacuation legs, `crates/custodian/src/rebalance.rs`: **39** (v6: 36). New: the `seg:`
  pin leg ×1, mixed ×2.
- `tests/segmented_map_repoint.rs` 14, `tests/segmented_map_reconstruction.rs` 6,
  `tests/staged_repair.rs` 30: same tests, five assertions changed and one test renamed (§2.3).

### 7.2 Red on the base

Every tracked file back to `6e44de9` (`git apply -R` of the tracked diff), the new test file kept;
the base `lib.rs` has no `mod premark`, so `premark.rs` is not compiled. The file compiles there,
and **53 of 55 fail**, each at its own claim (`base-red.log`). The legs this attempt added or
changed:

- D6 lost at the pre-mark, segmented: "the racer never landed" — the base commits no pre-mark, so
  the hook never fires. Flat: "the move adopted a destination it may not publish".
- D6 lost at the adoption ×2: "the lost move's fragment is kept for ever" (the brief's own base
  claim for D6; I ordered the verdict assertion after it so the base still fails there).
- `seg:` pin leg: "the `seg:` record does not hold the sibling's repoint" — no pre-mark commit to
  race.
- mixed ×2: "the move's pre-mark" — the base writes with no pre-mark.
- `d6_superseded_before_the_premark` ×2: "the move never reached its pre-mark commit".
- D10a lost adoption ×2: the stale mark left on a named position, as in v6.

The two that pass on the base are the D10a crash legs, as in every round since v4 (they guard v4's
mutation M11: a pre-mark written at an in-place position).

The tree was restored with `git apply` of the same diff and checked against saved hashes of every
changed and new file (`tree-before.sha`).

### 7.3 Mutations of the fixed tree (this attempt's)

Each applied alone by `mutate.py`, the named targets run with `--no-fail-fast`, the file restored
from the saved bytes and its hash checked. Script and logs:
`$PDCA_SCRATCH/pdca-builder-1084-i7/`.

| Mutation | Red | Which tests |
|---|---|---|
| `conflict-certifies`: `refused = true` removed from the `Conflict` arm (`reconstruction.rs:500`) | 13 | D6 at the pre-mark ×2, D6 at the adoption ×2, `d6_…_superseded_before_its_pre_mark` ×2, D10a lost adoption ×2; `segmented_map_repoint` ×3; `the_committed_namespace_is_read_once_per_pass`; staged `gc_reclaiming_the_vacated_position_before_the_adoption_makes_it_lose` |
| `seg-pin-dropped`: `.take(1)` on the pin fold (`premark.rs:465-471`) | 2 | the two `…pre_mark_loses_to_a_seg_record_rewrite` legs |
| `first-answer-only`: `refusal` judges `answers.into_iter().take(1)` (`premark.rs:602`) | 4 | `one_refused_write_refuses_a_whole_{flat,segmented}_repair`, `one_refused_copy_refuses_a_whole_{flat,segmented}_evacuation` |

All three were run once more on the final tree, after the last test edit; the counts above are
from those runs (`mut-conflict-certifies.log`, `mut-seg-pin-dropped.log`,
`mut-first-answer-only.log`). After the last one, every changed and new file was checked against
the saved hashes.

The earlier rounds' mutation tables (v4 M1–M12; v5 F1a–c, F2a–c, C5a–b; v6's three) cover code
this attempt did not touch, and I did not re-run them. `premark.rs` is byte-identical to v6.

### 7.4 Refuting my own test

- **(a) Genuine red?** Yes. On the base 53 of 55 fail at their own claims (§7.2); the two that
  pass go red under v4's M11 (v4's notes; not re-run here). Each rule this attempt added or
  guarded was checked by breaking the line it binds and re-running: three mutations, 19 reds, and
  no test outside the intended ones fails (§7.3).
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` or `rebalance::reconcile` and the real `premark` module. The
  racer in the `seg:` legs is a real segment record, decoded and re-encoded through
  `wyrd_core::metadata::SegmentRecord`, not a marker value. The D servers apply the trait crate's
  own deadline checks. No production logic is copied into a test.
- **(c) Fixture includes the fault?** Yes. Each D6 leg asserts the racer landed (the root's
  version, or the `seg:` record's bytes) and that the move did not adopt. On the flat shape the
  version check alone could be met by the repair's own commit, which also bumps the version by
  one; it is the pair that binds, since a repair that committed fails "did not adopt". The `seg:`
  legs also assert the root did *not* move, so they cannot pass on the root pin. The mixed legs
  assert the hold was reached, that one fragment landed and that the other did not.

One honest limit: the phase a D6 leg races is chosen by matching the commit's shape (a pre-mark
batch, or a batch that rewrites `inode:` / `seg:`). A leg does not prove no *third* commit exists
between them; the move has exactly those two (`reconstruction.rs:1336-1369`).

## 8. Gates run

All on the final tree. `patch.diff` is `git diff HEAD` of the worktree with the two new files
marked intent-to-add for the diff and un-marked again afterwards (25 files).

- **The project's gate, `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci`: exit 0, "xtask ci: all
  checks passed"** (`ci-i7-run2.log`). It ran `typos` (clean), the docs lint and the site render
  with its link audit, the gitlink, unsafe and blackbox guards, `cargo fmt --all -- --check`,
  workspace clippy, build and test, cargo-machete, the three `cargo deny` runs, the statics gate
  (ADR-0035), the deploy guard, and clippy plus tests for `wyrd-dst` under `--cfg madsim`. Totals:
  235 test binaries, **2700 passed, 0 failed, 16 ignored** (v6: 2692; the eight more are the five
  new repair legs and the three new evacuation legs). In the DST leg the nonce's replay test
  (`a_committed_moves_pre_mark_nonce_replays_under_its_seed`) passes.
- I ran the gate twice. The first run (`ci-i7.log`, also exit 0, 2700 passed) was on the tree
  before three last edits: the staged test's pin (`staged_repair.rs:1891-1895`) and the two doc
  comments that say the rule reaches the staged path. The second run is on the tree that includes
  them. For both runs the hash of every changed and new file was recorded when the run started
  and checked equal when it ended (`tree-at-gate.sha`, `tree-at-gate2.sha`), and checked again when
  `patch.diff` was written.
- Red on the base and the three mutations: §7.2, §7.3. These used
  `timeout <n> cargo test --offline -p wyrd-custodian …`, the command the verify gate itself runs
  (`engine/scripts/run-verify.sh`, `run_test`), with a hard timeout, because `cargo xtask` has no
  way to run one test file. Both were run again after the last test edit, on the final file.
- `engine/scripts/run-verify.sh --classify patch.diff`: `ADDED_TEST
  crates/custodian/tests/move_destination_marks.rs`; crates `chunkstore-grpc`, `custodian`, `dst`,
  `server`. The same as v6. I did not run its full mode: it builds a `../wyrd-verify` worktree
  outside the roots I may write. §7.2 is the same red leg, done in the cycle worktree.
- `patch.diff` applies forward on a copy of the base's files and reproduces the worktree byte for
  byte, all 25 files (checked in scratch, `applycheck/`); it also reverses cleanly on the worktree
  (`git apply -R --check`); `git diff --check` is clean. `Cargo.lock`, `deny*.toml` and every
  `Cargo.toml` are untouched. The bundle's `move_destination_marks.rs` is byte-identical to the
  worktree's.
- Commit hooks: the target repo configures no pre-commit hook of its own (no
  `.pre-commit-config.yaml`, no `core.hooksPath`); its formatter is `cargo fmt` and its spell
  checker `typos`, and both ran inside the gate on the final tree.
- External dependencies: the brief names `typos` and `docs-renderer`. Both were present and both
  ran in the gate. I met no dependency the brief did not name.
- The target's review rubric (`AGENTS.md`, *Review rubric & protocol*): I re-read this attempt's
  diff against it. No new clock read, trait seam, global, crate root, persisted field or await on
  external work is added: the production change is one flag assignment. Docs currency: the
  verdict change is in `06-runtime-view.md:40`. *Absent or unsupported entries* is the class F1
  belongs to ("never silent success"), and the rule now holds for every lost repair. *Test
  fidelity*: the seeded DST form of these interleavings stays deferred to #682, as the brief says.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013 and
  #1015, so it could not measure in the last three rounds. Nothing in the patch can change that.

## 9. Manual validation (nightly tiers, not run here)

Not declared as dependencies (pdca-harness #969). They compile in this tree (the gate builds every
target). No assertion in them changed in this attempt.

- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`
  (docker). Phase 1 expects `Reconciled::Blocked` (`:651`), phase 2 `Changed` (`:740`).
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen` (docker).
  Phase 1 of both legs expects `Blocked` (`:948`, `:1519`).
- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (root +
  device-mapper).

What to check: phase 1 of each scenario answers `Blocked`, the later phases still repair
(`Changed`) and then answer `Satisfied`. New in this attempt: a pass that repairs one chunk of a
flat object and loses the CAS on another chunk of the same object now answers `Blocked`, not
`Changed`. I read all three scenarios for that: each seeds one object with one chunk and queues
one repair, so no pass in them can lose a sibling CAS, and their expected verdicts stand.

## 10. Residuals

- **#1100** (the wider class), **#825** (settling an aborted move's pre-marks), **#682** (seeded
  DST for these interleavings): deferred, as before.
- **The destination drain fence** on the committed movers (`0016:670`): missing on the base and
  here, with no open issue (§6).
- **An `unplaced` abort still answers `Satisfied`** while the repair is owed: a committed repair
  whose chosen server is outside the fleet view, and every staged re-place that stops before its
  adoption (`RepairOutcome::Aborted`, `reconstruction.rs:503`). After F1 this is the only planned
  repair outcome that still certifies. Library-only for the committed case (the deployed loop
  builds fleet and topology together); real for the staged case. Not asked for this round (§2.5 d).
- **A `reclaiming` mark on a position the placement already names** keeps an in-place repair
  refused until the mark is cleaned up. Named on the audit seam every pass, and now stated in the
  code (§4); GC's side is #1083.
- `tests/staged_drain_status.rs`'s double still drops the write deadline; no write reaches it.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013 and
  #1015. It could not measure in the last three rounds for that reason, and nothing in the patch
  can change it.
- The rest of v4's list (`iteration-v4/build-notes.md` §11) stands unchanged.

## 11. Scratch

`$PDCA_SCRATCH/pdca-builder-1084-i7/`: `mutate.py` and its logs, `base-red.log`,
`f1-custodian.log`, `f1-server.log`, the gate logs, the saved hash lists, and a copy of v6's files
(`v6tree/`, used to measure this attempt's delta). No build directory of my own: every run used
the worktree's `target/`. I left it in place because these notes cite its logs; the harness
reclaims it.
