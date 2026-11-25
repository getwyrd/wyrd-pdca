# Build notes — #1062 (636.6a-2), iteration 3: tests for the untested holds and the segments tail

Base: `21467e6` (the pdca integration tip with child-1, #1061, folded). Every `path:line` below is
on the patched tree in the worktree. This iteration starts from iteration 2's patch, which applied
cleanly to `21467e6`, and follows the sign-off's narrow list. **No production file changed.** The
only edited file is `crates/custodian/tests/multipart_retirement_marks.rs`: five new tests, seven
helpers, one constant (`RS42`, which the E-hold test now uses in place of its local copy), and the
module doc (two bullets extended, one added). `marks.rs` is byte-identical to iteration 2
(sha256 `7ad8b939…`), and so are `gc.rs`, `restore.rs`, `retirement.rs`, the docs, the DST row and
the other two test files. I applied iteration 2's patch as a temporary commit, and `git diff` against
it at the end lists only `multipart_retirement_marks.rs` (291 lines added, 4 removed).
Iteration 2's notes (`iteration-v2/build-notes.md`) still describe the drain and its reds; this file
covers what this iteration added and re-checks the reds.

## The four sign-off items

### 1. Untrusted placements outside `part:` records — done

Two holds had no test: the list rows' hold at `crates/custodian/src/retirement/marks.rs:774-777`
and the `seg:` record hold at `marks.rs:857-861`.

- `list_with_an_untrusted_placement` (`multipart_retirement_marks.rs:1710`) plants an obligation
  whose own list is `[bad, sound]`. `bad` is an RS(4,2) chunk on five servers (six positions, five
  servers). `sound` is an RS(2,1) chunk whose placement holds. It runs twice:
  - `e_an_untrusted_chunk_in_a_chunks_list_is_held` (`:1731`) as `{chunks}` (the staged rule,
    `gc::staged_placement`);
  - `e_an_untrusted_chunk_in_a_flat_generation_is_held` (`:1738`) as a flat `{generation}` (the
    committed rule, `ChunkRef::checked_fragments`).

  Both rules refuse that chunk, and the obligation codec does not check placement length
  (`crates/core/src/multipart.rs:3642-3646`), so both obligations decode.
- `e_an_untrusted_chunk_in_a_segment_record_is_held` (`:1749`) plants a segmented `{generation}` over
  two `seg:` records. Segment 0 names three RS(2,1) chunks; the middle one is on four servers.
  Segment 1 is sound. `SegmentRecord` does not check placement length either
  (`crates/core/src/metadata.rs:1386-1408`).

Each test uses `assert_held_untrusted` (`:1663`) for two passes. It checks: `Blocked`; one
`untrusted-placement` audit event naming the chunk (`"chunk <id> names"`) and the record (empty for
a list row, segment 0's key for the `seg:` case); the obligation byte-identical; every pinned
`seg:` record byte-identical; and **no** `orphan:` key anywhere. Then
`assert_gc_keeps_a_held_obligation` runs GC past the late-write deadline and the grace, and every
fragment of both chunks stays.

The bad chunk comes **before** a sound one: first in the list, and in segment 0 ahead of segment 1.
So "no mark anywhere" also proves the step stopped at the hold. A silent skip would have marked the
sound chunk after it. For a list row, it would also have deleted the obligation in the same batch
(a row with no records goes with its last mark).

I planted the obligations rather than using a production writer. The adversary's case (a re-upload
over a `part:` record naming the bad chunk) would also need a planted bad `part:` record, so it adds
no realism. It also depends on whether the commit path accepts reading such a record, which is not
what this item is about. `plant_obligation` checks every planted value with the production decoder
and requires canonical bytes (`:251-256`).

### 2. `Hold::UnreadableMark` — done

`c_a_mark_of_none_of_the_three_shapes_is_held_and_never_restamped` (`:1256`) plants a `{chunks}`
obligation over two RS(2,1) chunks. Its first position holds
`{"orphaned_at_millis":9400000,"event":"g:80:4","reclaiming":true,"by":"gc-v2"}`, which is a
`reclaiming` mark with a field this build does not know. The test first asserts that
`decode_orphan_mark` refuses it. I chose this shape because it is the real hazard: it may be GC's
commitment to reclaim, written by a newer build, and arm (b) would erase it.

For two passes, the test checks: `Blocked`; one `unreadable-mark` audit naming that position; the
value byte-identical; the obligation byte-identical; and no other position marked. Then GC runs past
the late-write deadline and the grace. It keeps the value and every fragment
(`crates/custodian/src/gc.rs:709-712`).

### 3. The segments-tail mutants at `marks.rs:886` — done

`paging_a_group_of_small_segments_past_one_page` (`:2533`) plants twenty `seg:` records of one RS(2,1)
chunk each under a segmented `{generation}` obligation. That is 60 positions, so a step finishes a
full page of sixteen, which is the only way to reach `Tail::At(index + 1)` (the comment at
`marks.rs:882-885` says so). The test drains it and checks:

- every segment's positions were put in a batch strictly before the batch deleting its `seg:`
  record (the recording wrapper, `assert_marked_before_deleted`);
- every `seg:` record was deleted in a batch no later than the one deleting the obligation;
- every position carries this obligation's mark, and no `seg:` record is left;
- the cursor after each step is exactly `(generation,0,3)`, `(generation,16,0)`,
  `(generation,16,3)`, then gone. Step 2 is the one that takes `Tail::At(16)`;
- nothing is held, and past the grace every fragment and every mark is gone.

The semantic checks come before the cursor-sequence check, so a mutant that ends the group early
fails on "segment 16's record was never deleted", not on a cursor value.

### 4. Arm-(0) "wait on GC" with no time limit — NOT done: the policy choice was never recorded

The sign-off says "Policy: PENDING human choice (see below)", but nothing follows it. Not in the
brief, not in `iteration-v2/session-carry-forward`, and not in `iteration-v2/SUMMARY.md` §9. The
same sign-off also says to keep the production code as it is unless a test exposes a bug. So I did
not pick a policy and changed nothing for this item. **The human needs to choose at this sign-off.**

What the code does today. At a `reclaiming` mark over a position no kept record names, the step
returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That logs a `debug!` event and
the `retirement_steps_waiting` counter (`crates/custodian/src/retirement.rs:803-812`). GC judges only
the fragments its fleet lists (`gc.rs:636-646`). Its fragment-less sweep keeps a mark whose D server
is not in the fleet (`gc.rs:904-905`). So if that server leaves the fleet before GC destroys the
fragment, the obligation waits on every pass for good, with no warn-level event, and nothing past
that position is ever marked. The adversary's case needs damage (or a post-restore legacy mark) plus
a lost server, so the risk is low.

The options, with their cost:

- **A. Escalate a long wait to an audited hold.** `Arm::Reclaiming` would carry the mark's
  `orphaned_at_millis`. `Walk` would keep the step's `stamp_millis`. A new branch would build
  `Held { reason: Hold::ReclaimingUnfinished, position, record }` once `now ≥ stamp + bound`. That
  is about 25 lines in `marks.rs`, 3 in `retirement.rs` (the variant and its `as_str`), a test of
  about 60 lines, and one sentence in `06-runtime-view.md:90`. Two catches:
  - The bound needs the grace, and `ReconstructionContext` carries no grace. So either add a
    constant, or add a context field plus wiring at `crates/server/src/custodian.rs` (a port change,
    so the architecture doc must change too).
  - The `reclaiming` stamp was written by another lifecycle's writer (GC, or a post-restore legacy
    writer), and the swap does not move it (`gc.rs:679-681`). Comparing it with the drain's clock
    mixes two writers' clocks inside one decision, which the rubric's first MUST (ADR-0009) asks
    to avoid or to justify.
- **B. Accept the gap and document it.** Qualify `marks.rs:31-40` and `06-runtime-view.md:90`. GC
  finishes the reclaim only while the mark's D server is in its fleet; otherwise the drain waits,
  counted but not audited, until the server returns or an operator repairs it. About 4 lines of
  docs and no behaviour change.
- **C. Keep the wait, but make it visible.** Emit `AWAITING_GC` on the audit target at warn level.
  About 5 lines in `retirement.rs:438-441` and `:803-812`, plus a test. It adds no time limit, and
  it warns on every normal wait too (GC usually finishes within a pass or two), so it adds noise.

Whichever you choose, the doc claim at `marks.rs:34-35` and `06-runtime-view.md:90` ("GC destroys
the fragment and deletes the record itself") is too strong today. I left it alone because rewording
it is part of choosing B.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes, for the whole file and for each new test.
  - **Whole file:** I checked out `gc.rs`, `restore.rs` and `retirement.rs` at `21467e6`, removed
    `retirement/marks.rs`, and kept the new test file. `cargo test -p wyrd-custodian --test
    multipart_retirement_marks` compiles and gives **0 passed, 26 failed**. The five new tests fail
    by assertion: the three untrusted-placement tests and the unreadable-mark test on `round 0:
    left Satisfied, right Blocked`; the small-segments test on `every step changes it` (child-1
    leaves a `retire:bytes:` obligation untouched). After restoring, `git diff HEAD | sha256sum` was
    the same before and after (`9e8dc861…`), and the file gives 26 passed.
  - **Per item:** each edit below was made to `marks.rs`, the named test went red, and the file was
    restored (`sha256sum` back to `7ad8b939…`).
- **(b) Production path?** Yes. Every new test drives `wyrd_custodian::reconcile_step` through the
  rig's `drain()` (a reconstruction context whose `now_millis` and `clock` come from one
  `ManualClock`, as the deployed loop passes them, `crates/server/src/custodian.rs:397`) and GC
  through a `GcContext` alone. The store is `RedbMetadataStore::in_memory()` behind the recording
  wrapper, and the fleet is the placement-aware double. Holds are read from the audit seam and from
  stored bytes, never from a type this change adds.
- **(c) Fixture includes the fault?** Yes. The untrusted chunks are really malformed (5 servers for
  6 fragments, 4 for 3) and decode through the production codecs. The fragments are on the fleet.
  The unreadable value is really refused by `decode_orphan_mark` (asserted in the test). The small
  segments really number twenty, more than a page, and small enough to finish a page in one step:
  the cursor sequence proves step 2 took `Tail::At(16)`.

### Demonstrated reds this iteration (temporary edit → test red → file restored)

| # | Edit (in `marks.rs`) | Test(s) | Failure |
|---|---|---|---|
| 22 | list hold → silent skip: `Err(m) => { let _ = (m.expected, m.actual); }` at `:774` | both list tests | `round 0: left Changed, right Blocked` |
| 23 | `seg:` hold → silent skip, same shape, at `:857` | segment test | `round 0: left Changed, right Blocked` |
| 24 | unreadable mark re-stamped via arm (b): `Err(_) => Arm::Other(stored.to_vec())` at `:259` | unreadable-mark test | `round 0: left Changed, right Blocked` |
| 25 | `:886` guard → `false` (as `if false && index < MAX_SEGMENT_INDEX`) | small-segments test | `seg:…5e650019:1:000016 deleted` (never deleted; the obligation went at step 2) |
| 26 | `:886` `<` → `>` | small-segments test | same as 25 |
| 27 | `:886` `<` → `==` | small-segments test | cursor `[(0,3), None]` (before the reorder) |
| 28 | `:886` `index + 1` → `index * 1` | small-segments test | cursor `(15,0)` instead of `(16,0)` |
| 29 | `:886` `index + 1` → `index - 1` | small-segments test | cursor `(14,0)` instead of `(16,0)` |

Rows 25–29 were first run with the cursor check before the semantic checks. Then I moved the
semantic checks first and re-ran rows 25 and 26, which now fail as shown. A bare `if false` does not
compile (the workspace denies the unused binding and import that it leaves), hence the `&&` form,
which is what the mutant means.

`<` → `<=` at `:886` survives. It differs only when the last record on a full page is segment
`MAX_SEGMENT_INDEX`, which needs a group of that many segments. That is an equivalent mutant for any
test input; I record it rather than chase it.

### The brief's required reds (rows 1–8): re-run on the unchanged production code

The production code is unchanged, but the test file grew, so I re-ran each red on this tree. Each
edit was made, the whole file was run, and the files were restored. After all of them, `sha256sum -c`
against hashes of `marks.rs`, `retirement.rs` and `reconciliation.rs` taken first came back OK.

| # | Edit | Failing test(s) | Failure |
|---|---|---|---|
| 1 | marks stamped with the pass's cached `now_millis` (passed from `reconciliation.rs:167` into `retirement::reconcile` and `visit`, used in place of `clock.now_millis()` at `retirement.rs:428`) | (T) | `(0, FragmentId { chunk: 2000, index: 0 }) stamped 10000000, before its retirement at 10120000` |
| 2 | arm 0 removed (`Ok(mark) if false && mark.is_reclaiming()`, `marks.rs:261`): a `reclaiming` mark falls to arm (a)/(b) | (C′) arm 0, (C′) sweep, (H) | arm 0: `left Changed, right Partial`; (H): `left Changed, right Blocked` |
| 3 | an untrusted `part:` chunk skipped (`Err(_) => {}` in `load_part`, `marks.rs:688`), so its record is deleted unmarked | (E-hold) | `round 0: left Changed, right Blocked` |
| 4 | owned-residue check skipped (`if false && payload.session()`, `marks.rs:151`) | (R) | `round 0: left Changed, right Blocked` |
| 5 | segments-before-parts skipped (`may_delete = … \|\| true`, `marks.rs:187`) | (G) | step 2: `ChunkInNoPart { chunk: 16003 }` |
| 6 | a record deleted in the batch that marks it (`(evidenced \|\| true)`, `marks.rs:433`) | (B), (D abort), both part-paging tests, small-segments test | `orphan:0:200:0 marked in batch 0, its record part:…:000002 deleted in batch 0` |
| 7 | `reclaiming` over a retained record treated as marked (an empty `Arm::Reclaiming if entry.retained => {}` arm ahead of the hold, `marks.rs:382`) | (H) | `round 0: left Changed, right Blocked` |
| 8 | the `{chunks}` row skipped, left byte-identical (`Planned::Wait(NO_PROGRESS)` for `RetireComponent::Chunks`, `marks.rs:193`) | (D chunks), plus every other `{chunks}` test | `retire:bytes:s:…:a77e0002 drains within 4 passes` |

Row 2′ (the DST half of row 2) I did **not** re-run. It needs a separate `--cfg madsim` build. The
DST row (`crates/dst/tests/custodian.rs`) and `marks.rs` are byte-identical to iteration 2, where it
went red (`iteration-v2/build-notes.md`, row 2′). Both DST rows pass in this iteration's
`cargo xtask ci` run (below).

## Gates

- **Full gate:** `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci` (`cargo xtask ci`) on the patched
  worktree: **exit 0, "xtask ci: all checks passed"**. That was 2,486 tests passed and 0 failed
  (iteration 2: 2,481; the 5 new tests account for the difference). It includes both DST rows
  (`retirement_restamp_races_gc_swap`, `retirement_restamp_reaches_both_winners`) under
  `--cfg madsim`, `typos`, `lint_docs` OK, `render_site` (101 pages, link audit OK), fmt,
  workspace clippy, deny and the ADR-0035 statics gate. Both brief tools (`typos`, `docs-renderer`)
  are on this host and ran.
- After the gate I changed nothing. The red edits were each restored (hash-checked), and a final
  `cargo test -p wyrd-custodian --test multipart_retirement_marks` gives 26 passed.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`:
  clean. `typos` on the test file: clean.
- `patch.diff` is `git diff 21467e6` of the worktree. It applies cleanly to `21467e6`
  (`git apply --cached --check` against a scratch index read from that commit). The bundle's
  `multipart_retirement_marks.rs` is byte-identical to the worktree's.

## For the human at sign-off

- **Item 4 needs your policy choice** (above). Nothing changed for it.
- **C4 diff coverage will again report "does not apply on origin/main".** Child-1 (#1061, `20e6e21`)
  is still not on `origin/main` (`git merge-base --is-ancestor 20e6e21 origin/main` fails, checked
  2026-11-13). This bundle is built on the integration tip as the brief directs. A Do iteration
  cannot fix that.
- The size backstop was overridden at the last sign-off. This iteration adds about 290 lines, all in
  the one test file.
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed.
