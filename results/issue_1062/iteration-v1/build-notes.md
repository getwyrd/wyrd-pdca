# Build notes — #1062 (636.6a-2): the `retire:bytes:` marking drain

Base: `21467e6` (the pdca integration tip with child-1, #1061, folded). Every `path:line` below is
on the patched tree in the worktree.

## What changed

- **New module `crates/custodian/src/retirement/marks.rs`** — the `retire:bytes:` mode.
  - `plan` (`marks.rs:132`): the stamp comes in from the caller; the `session` component's
    owned-residue check (`owned_residue`, `marks.rs:207`, one `sidx:` row); cursor/component
    check; then one walk over the work component (`parts` set `marks.rs:502`, `parts: all`
    `marks.rs:530`, `{chunks}` / flat `{generation}` `list` `marks.rs:712`, segmented
    `{generation}` `segments` `marks.rs:749`).
  - Mark guard: `read_arm` (`marks.rs:251`) — `reclaiming` first (`marks.rs:258`), then own
    event (a), other/legacy (b, exact-value precondition), absent (c, `require_absent`).
    `Arm::Reclaiming` over a retained `part:` record → `Hold::ReclaimingRetained`
    (`marks.rs:379`); elsewhere → `Wait("awaiting-gc")`.
  - Marks strictly before deletes: `Walk::delete` (`marks.rs:428`) deletes an entry's records
    only when every position already carried this obligation's mark *before* the step
    (`evidenced`, `marks.rs:430`) — the cursor's prefix or arm-(a) reads. So the marking batch and
    the deleting batch are always different commits.
  - Segments before parts: `segments_drained` (`marks.rs:226`), one keyed read of the same
    token's `retire:records:` obligation; while it owes `{seg}`, no `part:` deletes.
  - Placement validation: `load_part` (`marks.rs:619`) holds `part:` records to
    `crate::gc::staged_placement` (`marks.rs:643`); `{chunks}` uses the staged rule, generation
    chunks the committed rule (`Rule::positions`, `marks.rs:703`).
  - `Walk::finish` (`marks.rs:460`): obligation deleted when every entry is finished and the
    component ended; otherwise marks + deletes + advanced cursor; a step that changed nothing
    returns the hold or the wait that stopped it.
- **`crates/custodian/src/retirement.rs`**
  - `mod marks;` (`:113`); constants `ENTRIES_PER_MARKING_STEP` (`:168`), `MARKS_PER_STEP`
    (`:177`, 233, op-derived), `READS_PER_MARKING_STEP` (`:184`, 252), `READS_PER_PASS` now
    sized by the marking step (`:195`, 4,038; was 70).
  - Five new `Hold` reasons (`:231-254`).
  - `visit` (`:394`) decodes both modes; for `retire:bytes:` it reads
    `ctx.clock.now_millis()` **inside the step, after the obligation bytes are in hand**
    (`:427`) and calls `marks::plan` (`:428`). Batch building adds each mark's own precondition
    (`require` for b, `require_absent` for c). `Planned` (`:483`), `Mark` (`:500`),
    `advanced_value` factored out (`:582`), `hold_at` emits `position`/`record` fields (`:776`),
    `emit_wait` (`:802`). The `deferred: #1062` marker is gone; `deferred: #660` stays.
  - Removed the `retirement_bytes_obligations_deferred` counter (nothing else in the tree reads it;
    only a stale `mutants.out` artifact mentions it).
- **`crates/custodian/src/gc.rs:1472`** — `staged_placement` made `pub(crate)` (shared rule, doc
  updated); no behaviour change.
- **`crates/custodian/src/restore.rs:1158`** — the `deferred: #659` marker replaced by what the
  drain now guarantees between two restore runs; `:382` comment updated (drain holds untrusted
  records, #1013 retires them).
- **Docs**: `docs/design/architecture/06-runtime-view.md:88-90` (new paragraph: marks before
  deletes, grace starts at the drain, `reclaiming` never overwritten, GC resumes except over a
  retained record → human, residue hold, untrusted placement hold, segments before parts);
  `05-building-block-view.md:246` (the `retire:bytes:` sentence).
- **DST row** appended to `crates/dst/tests/custodian.rs` (`drain_races_gc` `:6601`,
  campaign `:6783`, reach sweep `:6792`, registrations `:7009`ff, added to the committed
  regression replay `:7064`). No new file under `crates/dst/tests/`.
- **New test** `crates/custodian/tests/multipart_retirement_marks.rs` (13 legs).
- **Child-1's test edited** (`crates/custodian/tests/multipart_retirement_drain.rs`) — see below.

## Why this shape

**The stamp.** The first attempt stamped with the pass's `now_millis`, read before reconstruction
ran. The deployed loop builds `ReconstructionContext::clock` from the same `LoopClock` as
`now_millis` (`crates/server/src/custodian.rs`, `LoopClock`, and the context at the
reconstruction pass), so reading `ctx.clock` per step keeps one time source (ADR-0009) while
never predating an obligation installed during the pass. Re-read per re-plan after a lost
precondition, too.

**Two commits, mark then delete.** 0016's batch row says "`B` orphan puts **or** `B` record
deletes" and the brief's (B) asks for marks *before* deletes shown by a recording wrapper. A
same-batch mark+delete would satisfy the invariant's "no instant" wording but not "before"; red 6
shows the test binds the strict order. To avoid one step per entry, a step may delete the records
of entries whose positions it reads back as already carrying its own mark (arm a), then mark
further. The cursor only names the first unfinished entry, so marks past it are re-read by the
next step. Cost: at most one extra `get` per position on that re-read, bounded by
`MARKS_PER_STEP`.

*Rejected*: a second cursor field recording "marked through entry m". That is a persisted-format
change (core codec, `RETIRE_CURSOR_MAX_BYTES` reservation, every installer's ceiling, decode
tests, building-block doc) — `crates/core/src/multipart.rs:3999-4079` plus its test file — for a
read saving the per-step bound already covers. Out of this child's scope (child-1 owns the cursor
codec).

**Waits are not holds.** A `reclaiming` mark GC will finish, and segments still owed, are normal
transient states: the step answers `Wait`, the pass `Partial`, nothing is audited as a fault. The
holds are the states that need a human or #1013.

**Absent part record in a `{parts: <set>}`** → `Hold::RecordUnreadable`, not a skip: the bytes
it named are unknown and the rubric forbids a silent skip. No in-tree path produces it.

**Per-pass reads grew from 70 to 4,038.** Mark reads are one keyed `get` each (orphan keys for
one chunk are on different D servers, so no range read covers them). `MARKS_PER_STEP` is the
op-derived `B` (233). I kept `STEPS_PER_PASS = 16`. On a 5 ms backend a pass full of marking
steps is ~20 s of reads; on the in-memory/redb stores it is milliseconds. A lower
`MARKS_PER_STEP` trades that for more passes; I left the op-derived value and flag it.

## Child-1's test (`multipart_retirement_drain.rs`) — edited, and why

Five legs encoded "`retire:bytes:` obligations are left byte-identical", which is exactly the gap
this child closes:

- `the_built_bounds_are_the_promised_ones`: the read bound is now `1 + 5 + 252 × 16 = 4,038`
  (stated literals `RECORDS_STEP_READ_BOUND`, `MARKING_STEP_READ_BOUND`).
- `b_rec_…`: "no orphan: record at all" → "no orphan mark names a published chunk"; the
  "unnamed parts untouched" asserts replaced by "the records mode deleted only published records,
  and an unnamed part's record goes only through its bytes obligation" (checked on the batch log).
- `l1_a_productive_backlog…`: dropped the eight inert `retire:bytes:` fillers (every obligation now
  takes a step); one page per pass; end state unchanged (`Partial` then `Satisfied`).
- `s_a_pass_that_resumed_part_way…`: the resume-past-a-half-drained-obligation case is now built
  from 15 one-step records obligations + the 3-step one (steps run out, not visits); still asserts
  the resumed pass answers `Partial` with work behind it.
- `g_rec_…`: "bytes obligation untouched" → "bytes obligation still owed and every part record
  present while the segments are owed".

All 19 legs pass after the edits.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes. With `crates/custodian/src/retirement.rs` and `gc.rs` reverted to
  `HEAD` (child-1's drain), all 13 legs of the new file fail (`0 passed; 13 failed`). Each
  demonstrated red below was a temporary edit, run, then restored from a scratch copy (`cmp`
  verified).
- **(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` with a
  reconstruction context only (the deployed reconstruction pass), its `now_millis` and its
  `clock` one `ManualClock`, and GC through `reconcile_step` with a `GcContext` only — the deployed
  GC pass. Obligations come from production writers where the brief says so: `complete_upload`
  (B, H, D-flat, S-seg), `upload_part` re-upload (D-chunks), `abort_session` (D-session, R),
  `reconcile_after_restore` fence (G). Planted only where the brief allows: (T)'s installed
  obligation, (C′)'s `{chunks}` + `reclaiming` mark, (H)'s `reclaiming` mark, (E-hold)'s untrusted
  record + obligation, (S-part)'s 240-position part + obligation, (R)'s `sidx:` entry. Metadata is
  `RedbMetadataStore::in_memory()`; the chunk store is a placement-aware multi-server double.
- **(c) Fixture includes the fault?** Yes. (T) installs the obligation *during* the pass, after
  moving the clock two graces; (C′)/(H) plant real `reclaiming` marks and run real GC passes over
  fragments that exist; (E-hold) the decodable RS(4,2)-on-5 record with all six fragments on
  disk; (R) a real owned `sidx:` entry; (G) 1,100 segments (three records steps) so a part delete
  would land with segments still present; (S) entries past `MARKS_PER_STEP`; the DST row a real
  concurrent GC over the simulated-TiKV store with the drain start swept.

### Demonstrated reds (each temporary edit, the named leg red, file restored)

| Edit | Leg | Failure |
|---|---|---|
| marks stamped with the pass's `now_millis` (passed down from `reconcile_step`) | (T) | `stamped 10000000, before its retirement at 10120000` |
| arm 0 neutered (a `reclaiming` mark falls to arm b) | (C′) arm 0 | `byte-identical` |
| same | DST `retirement_restamp_reaches_both_winners` | `a batch re-stamped GC's reclaiming mark (drain at 4 ms)` — the GC-wins interleaving; the per-seed campaign leg passed for its 5 seeds, the sweep catches it |
| untrusted chunk's positions skipped → record deleted unmarked | (E-hold) | `round 0` outcome not `Blocked` (record deleted) |
| owned-residue check skipped | (R) | `round 0` outcome not `Blocked` |
| segments-before-parts skipped | (G) | step 2: `ChunkInNoPart { chunk: 16003 }` |
| record deleted in the batch that marks it (`evidenced` ignored) | (B) | `orphan:0:200:0 marked in batch 0, its record … deleted in batch 0` |
| arm 0 over a retained record treated as marked | (H) | `round 0` outcome not `Blocked` |
| `{chunks}` row skipped (left byte-identical) | (D, chunks) | `drains within 4 passes` |

## Gates run

- `cargo test -p wyrd-custodian` (all 33 test files) green; `cargo clippy -p wyrd-custodian
  --all-targets -D warnings` clean; `cargo fmt --all --check` clean.
- `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets -D warnings` clean; the two new
  DST rows green (`MADSIM_TEST_NUM=5`).
- `cargo xtask ci` via `engine/xtask.sh ci` on the patched worktree: **exit 0, "all checks
  passed"** — 2,472 tests passed, 0 failed, including both new DST rows at the gate's 50 seeds,
  plus `typos`, docs lint and render (both tools present on this host, so not warn-skipped), fmt,
  workspace clippy, cargo-machete and cargo-deny. Two doc-comment edits landed after the gate
  started; I re-ran custodian clippy, `cargo fmt --check`, `typos` on every touched file and both
  retirement test files afterwards — all clean.
- `patch.diff` applies cleanly to `21467e6` (`git apply --cached --check` against a scratch index
  of `HEAD`).

## For the human at sign-off

- **Fragment-less drain marks stay.** GC's fragment-less sweep keeps any structured,
  non-`reclaiming` mark (`event-may-await-write`, `crates/custodian/src/gc.rs`, the sweep doc),
  "left to the slice that introduces its writer". This drain is such a writer: a mark it writes
  over a position whose fragment never landed (a losing writer's unwritten fragment) or was
  already destroyed (C′'s arm c after GC finished) stays in the ledger. Bounded garbage, never a
  lost fragment. Adding retirement events to the swept set needs the late-write reasoning for
  `{chunks}` compensations and is not in this brief; worth a follow-up issue.
- **Throughput.** Child-1's walk takes one step per obligation per pass, so one max segmented
  generation (~1.78 M positions, 0016) needs ~7,600 passes at 233 marks per step. Correct and
  bounded, slow for huge deletes. A multi-step visit is a walk change (child-1's).
- **`B_ops` is uncalibrated** (`MAX_BATCH_OPS`, its own doc); `MARKS_PER_STEP` inherits that.
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed.
