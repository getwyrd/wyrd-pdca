# Build notes — #1062 (636.6a-2), iteration 2: the `retire:bytes:` marking drain

Base: `21467e6` (the pdca integration tip with child-1, #1061, folded). Every `path:line` below is
on the patched tree in the worktree. This iteration starts from iteration 1's patch (applied
cleanly to `21467e6`) and fixes what the sign-off carry-forward lists. Iteration 1's notes
(`iteration-v1/build-notes.md`) still describe the drain's overall shape; this file covers what
changed and why, and repeats the refutation and the reds for the whole patch.

## The four carry-forward items

### 1. Empty page with a partial cursor → `RecordUnreadable` hold (all three reviewers)

The defect: in `parts_all` and `segments`, a page that read no record returned `Tail::Ended`
even when the stored cursor counted marks inside an entry (`item > 0`). `Walk::finish` then
deleted the obligation with that entry's remaining positions unmarked and no audit event.

Fix (`crates/custodian/src/retirement/marks.rs`):

- `partial_entry_gone` (`marks.rs:617`): one helper that builds the `RecordUnreadable` hold when
  the stored cursor has `item > 0`, naming the cursor entry's record. It returns `None` for
  `item == 0`, where an empty page is still the end. That case is reached by a normal drain:
  `Tail::At(n + 1)` names a part number that may not exist.
- `Walk::held_at_cursor` (`marks.rs:462`): holds at the stored cursor's own entry and item, so the
  obligation stays byte-identical.
- `parts_all`: the empty-page check after the loop (`marks.rs:595`), and the existing
  first-record-past-the-cursor check now goes through the same helper (`marks.rs:580`).
- `segments`: the same two sites (`marks.rs:832`, `marks.rs:875`); the `index < entry`
  (`SegmentCursorPastRecords`) branch is unchanged. The hold names the missing segment's key
  through `segment_key_of` (`marks.rs:894`). Before, the in-loop branch named the first record
  found instead of the missing one.
- Module doc (`marks.rs:65-79`) and the runtime view say the walk never takes this for the end.

Regression tests: `a_partial_cursors_part_gone_with_no_record_after_it_is_held`
(`multipart_retirement_marks.rs:2025`) and `a_partial_cursors_segment_gone_with_no_record_after_it_is_held`
(`:2090`) are the reviewers' empty-page cases. Each drives a real step that writes 233 marks and
stores `item = 233`, deletes the record, then checks two passes: `Blocked`, one
`record-unreadable` audit naming the record, obligation byte-identical, nothing more marked. Past
`D` plus the grace, GC keeps every fragment and every mark. The `_with_a_record_after_it_` twins
(`:2032`, `:2097`) cover the non-empty-page branch, which was already right.

### 2. Drain marks over a position with no fragment had no deleter (adversary finding 2; the human authorized the GC change)

`crates/custodian/src/gc.rs:918-924`: in `sweep_fragment_less_marks`, a non-`reclaiming` mark whose
event is a retirement token (`OrphanMark::retire_token`) is now judged by one keyed read of its
`retire:bytes:` obligation, `retirement_draining` (the same read GC's reclaim protection makes,
`gc.rs:760`). If the obligation is present, the mark is kept and named `draining-retirement`. If it
is absent, the mark is swept like a legacy mark. The checks before it still apply: listed this
pass, server in the fleet, no reference or staged protection, and at least `D` old. Any other
event mark is still kept as `event-may-await-write`. Docs: the sweep's doc (`gc.rs:857-869`), the
`LATE_WRITE_DEADLINE_MILLIS` doc (`gc.rs:276-280`), and `06-runtime-view.md:84` (the sweep
paragraph) and `:90` (who deletes the drain's fresh mark).

What the change trades. I wrote this into the sweep's doc, because the old rule existed for a
reason (PR #821). The drain marks only after its obligation is installed, and only positions that
a record or list names. A fragment lands at a committed record's position before that record
commits. So the only write that may still be unsettled under a drain mark is a compensated part
attempt's (`{chunks}`, `commit.rs:1342`), and that write was authorized before the compensation
installed the obligation. Its deadline therefore falls at most `W_write` (30 s) after the mark's
stamp. The listing that licenses the sweep comes at least `D − W_write` = 11 s after that
deadline. Only a publication that straddles its deadline by more than 11 s can land after the
sweep. Such a fragment ends up unmarked and unreferenced, and GC keeps it for good: leaked bytes,
never lost data. The old rule leaked a ledger key every time instead (C′'s arm c, any lost write).
Both are leaks; the new one needs a pathological straddle.

The existing GC test had to change with the rule. `gc_mark_sweep.rs` leg B(viii) required marks
whose events were retirement tokens (`g:1:1`, `s:…:7`) to be kept for good. B(viii) now uses events
that are not tokens (a move nonce, and an owned-staging walk's `<upload-id>:<epoch>`) and asserts
exactly what it did before for them (`gc_mark_sweep.rs:1189`). New leg B(ix) (`:1253`) pins the
new rule: a token mark whose obligation is present is kept as `draining-retirement`; two whose
obligations are absent are swept, audited and counted, alongside the control. The module doc's
leg list gained (ix). This is an edit to a test outside this issue's file, made because the human
authorized the rule change.

Tests in this issue's file:

- `c_a_reclaiming_mark_is_never_replaced_and_the_drain_resumes_once_gc_deletes_it` (`:987`) ends
  with GC passes past the grace, then asserts the whole `orphan:` ledger is empty. That covers the
  fresh arm-(c) mark over the destroyed fragment, the case the human named.
- `c_a_drain_mark_over_no_fragment_is_swept_only_once_its_obligation_is_gone` (`:1079`) covers the
  other half. My first version used an aborted session's part, and I rejected it: the `part:`
  record's staged protection kept the mark, so the test would have passed without the new branch.
  The kept version uses a `{chunks}` obligation, where only the obligation protects the mark. The
  first chunk's first fragment is missing, and a `reclaiming` mark on the second chunk stops the
  drain halfway. Past `D` plus the grace, GC keeps the drain's mark over the empty position
  byte-identical while the obligation is present. After the drain finishes, nothing is left.

### 3. Paging paths untested; (C′) `reclaiming` mark moved to position 0

- `paging_an_aborted_sessions_parts_past_one_page` (`:2180`): Abort of 20 one-chunk parts numbered
  1–16 and 18–21 (no 17), giving `{session, all}`. Step 1 marks exactly parts 1–16. Step 2 deletes
  their records, writes no mark, and puts the cursor at `(parts, 17, 0)`. Step 3 reads on and finds
  part 18 first, which must not hold, because `item == 0`. The drain finishes with marks before
  deletes, and past the grace every fragment is gone. This kills the surviving `marks.rs:481`
  mutant, which stalled in the adversary's probe.
- `paging_a_completes_unnamed_parts_past_one_step` (`:2209`): a Complete naming part 1 of 20 gives
  `{parts: [[2,20]]}`. The same shape exercises `parts_set` paging (the `:515` and `:518` mutants).
- `paging_a_segmented_generation_of_more_segments_than_one_page` (`:2246`): 17 parts of 158 RS(2,1)
  chunks, giving 17 segments (8,058 positions), then superseded. It drains to completion, the cursor
  reaches segment 16 (past the first page), every position is marked, no `seg:` record is left, and
  past the grace every fragment is gone.
- (C′) arm 0 (`:987`): the `reclaiming` mark is now at position 0. The first pass must answer
  `Reconciled::Partial`, audit nothing, report the wait `awaiting-gc`, and leave the obligation
  byte-identical. The test reads the wait through a capture of the leg's `emit_wait` event
  (`WAIT_TARGET`), because `Partial` with no audit alone does not kill the `:484` mutant: deleting
  that arm still gives a `no-progress` wait and `Partial`. This kills `:379` and `:484`.
- I also killed nearby survivors the same capture reaches: (G) now asserts that a `segments-first`
  wait was reported (`!self.may_delete` → `false` or deleted `!`), and the three-arms test asserts
  the step counters (`emit_step` → `()`).

### 4. The DST race row requires a witnessed race (`crates/dst/tests/custodian.rs:6435-6920`, the row's block)

The row no longer hopes the timing forms a race. The recording store (`RaceMeta`) now holds each
writer's **first** commit built from the stale mark at a rendezvous (`meet`, `:6537`) until the
other writer's has arrived. Both are then started at one release instant, the later one offset by
the seed's `lag` (from −2 to 2 ms; at 0, madsim's scheduler breaks the tie). Each run asserts
(`:6788` on):

- both writers committed a batch built from the stale bytes;
- of those two, exactly one `Committed` and the other `Conflict`;
- only one commit over the stale bytes ever landed (GC's retry of a lost swap alone conflicts too);
- for `lag != 0`, the earlier commit won;
- no batch was ever built from GC's `reclaiming` value;
- no GC pass failed;
- on GC wins: the drain's re-read observed `reclaiming` after its own conflict and the swap, and
  before GC's key delete (`ReadReclaiming` before the new `KeyDeleted` event); every later drain
  write to the position was arm (c); the fragment is gone;
- on drain wins: GC deleted no key and the fragment stays;
- the obligation drains to empty, and the position carries the drain's fresh mark.

On GC wins, the blind key delete waits, up to 200 ms, for the drain's re-read
(`await_drain_read`, `:6564`). Without that wait, the drain's re-read and GC's key delete are about
one hop apart, and the order is down to the scheduler.

`prop_retirement_restamp_reaches_both_winners` walks every lag and requires both winners. The
`reclaiming`-read check is now enforced on every GC-wins run, not as a campaign-wide flag.

I moved the "GC pass failed" assertion after the race's own properties. With arm 0 removed, the
drain's write over `reclaiming` collides with GC's blind key delete, and the TiKV model reports
that as an error. The red then failed on "GC pass failed" instead of naming the violation. Now it
fails on "a batch re-stamped GC's `reclaiming` mark", with the GC-wins event log.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes, both for the whole file and per defect. Whole file: with
  `gc.rs`, `restore.rs` and `retirement.rs` checked out at `21467e6`, `retirement/marks.rs`
  removed, and the new test file kept, `cargo test -p wyrd-custodian --test
  multipart_retirement_marks` compiles and gives **0 passed, 21 failed**, every leg by assertion.
  `gc_mark_sweep` B(ix) is red on the same base. With the files restored (`git diff | sha256sum`
  identical before and after), the result is 21 passed. Per defect: every row in the table below is
  a temporary edit, the named legs run red, and the file is restored (the same hash check,
  `d9cf1230…`). I did not run `engine/scripts/run-verify.sh` itself: it adds and resets a worktree
  registered in the host's primary checkout (`git -C "$WYRD_REPO" worktree add` and `reset --hard`),
  which is outside the roots I may write. The run above is its red leg done by hand inside my own
  worktree. C4-verify runs the real script at Check.
- **(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` with the contexts
  the deployed loop passes: a reconstruction context whose `now_millis` and `clock` come from one
  `ManualClock`, and GC through a `GcContext` alone. The store is `RedbMetadataStore::in_memory()`
  behind a recording wrapper, and the D servers are a placement-aware double. Obligations come from
  production writers wherever the brief asks: `complete_upload`, `upload_part` re-upload,
  `abort_session`, and the restore fence. The new paging and partial-cursor legs use production
  writers too (Abort, Complete, a superseding Complete). The only planted state is what the brief
  allows or what models damage: the `{chunks}` obligations for (C′) and the sweep leg,
  `reclaiming` marks, the deleted `part:`/`seg:` records (damage), and the earlier legs' plants.
  The DST row drives the production `reconcile_step` for both the drain and GC over the
  simulated-TiKV model.
- **(c) Fixture includes the fault?** Yes. The partial-cursor legs really write 233 marks and store
  `item = 233` before the record goes. The sweep leg removes a real fragment, so the position is
  truly empty, and keeps the obligation present while GC runs past `D` plus the grace. The paging
  legs use more than 16 real entries, including a hole at part 17. The DST rendezvous makes both
  commits read the stale bytes, and red B below shows the row now rejects a run where they don't.

### Demonstrated reds (temporary edit → leg red → file restored)

Rows 1–8 are the brief's list (2′ is row 2's DST half); rows 9–21 are this iteration's.

| # | Edit | Leg(s) | Failure |
|---|---|---|---|
| 1 | marks stamped with the pass's cached `now_millis` (passed down from `reconcile_step`) | (T) | `stamped 10000000, before its retirement at 10120000` |
| 2 | arm 0 removed (`is_reclaiming()` guard false: a `reclaiming` mark falls to arm b) | (C′) arm 0 | `owed, unchanged, nothing held: left Changed, right Partial` |
| 2′ | same, DST | `retirement_restamp_races_gc_swap`, `…_reaches_both_winners` | `a batch re-stamped GC's reclaiming mark (lag 1 ms): [Commit{drain:false,…Committed}, …]` (GC-wins) |
| 3 | an untrusted chunk's positions skipped, so the record is deleted unmarked | (E-hold) | `round 0: left Changed, right Blocked` |
| 4 | owned-residue check skipped | (R) | `round 0: left Changed, right Blocked` |
| 5 | segments-before-parts skipped (`may_delete` forced true) | (G) | step 2: `ChunkInNoPart { chunk: 16003 }` |
| 6 | a record deleted in the batch that marks it (`evidenced` forced true) | (B) | `orphan:0:200:0 marked in batch 0, its record part:…:000002 deleted in batch 0` |
| 7 | arm 0 over a retained record treated as marked | (H) | `round 0: left Changed, right Blocked` |
| 8 | the `{chunks}` row skipped (left byte-identical) | (D, chunks) | `retire:bytes:s:…a77e0002 drains within 4 passes` |
| 9 | iteration 1's shape: an empty page under a partial cursor is the end | both empty-page legs (the two twins stay green) | `round 0: left Changed, right Blocked` |
| 10 | GC sweep change reverted | both (C′) legs | `no mark is left …: left {orphan:0:3000:0: …a77e0003…}` |
| 11 | sweep without the obligation read (always sweeps) | sweep leg | `kept while the obligation is present: left None` |
| 12 | mutant `marks.rs:481` `&&`→`||` | both parts paging legs | `step 2 finished the first page: left (parts,1,3), right (parts,17,0)` |
| 13 | mutant `marks.rs:379` `entry.retained`→`true` | (C′) arm 0 | `left Blocked, right Partial` |
| 14 | mutant `marks.rs:484` `AwaitingGc` arm deleted | (C′) arm 0 | `the wait it reports: left ["no-progress"], right ["awaiting-gc"]` |
| 15 | mutant `parts_set` `visited += 1`→`*= 1` | set paging leg | `part 18 marked by step 1` |
| 16 | mutant `!self.may_delete`→`false` | (G) | `waited for the segments: ["no-progress", "no-progress"]` |
| 17 | mutant `emit_step`→`()` | three arms | counter `left 0, right 1` |
| 18 | mutant `next_part`→`1` | abort paging leg | `left (parts,1,0), right (parts,17,0)` |
| 19 (DST B) | rendezvous removed and the drain started 30 ms late, which the old row accepted | both DST rows | `both writers must commit from the stale mark (lag -2 ms): [Commit{drain:false,…Committed}, ReadReclaiming, KeyDeleted, Commit{drain:true, expected:None…}]` |
| 20 | red 10's edit, run against `gc_mark_sweep` | B(ix) | `it must be kept for that reason and named` (kept as `event-may-await-write`, not `draining-retirement`) |
| 21 | red 11's edit, run against `gc_mark_sweep` | B(ix) | `a mark whose retirement is still draining was swept or rewritten` |

Rows 2, 5, 6, 8, 13 and 14 first failed to compile because the workspace denies warnings (the edit
left dead code). I re-ran them with edits that keep the code reachable (for example
`if false && …` guards), and the table records those runs.

### Surviving mutants I did not chase, and why (C5 is advisory)

- Byte and op budget arithmetic: `marks.rs` `MARK_MAX_BYTES` `+`→`*`, the per-mark `cost`
  arithmetic, the `ops_left`/`bytes_left` checks, and the `-=` updates. The read bound binds first
  every time. A step reads at most `MARKS_PER_STEP` = 233 marks, so it writes at most 233 marks (466
  ops) and deletes at most 16 entries × 2 (32 ops): 498, exactly the op budget. Bytes come to
  under 200 KB of the roughly 5 MB budget. So no test input can make those checks bind before the
  reads do. They stay as local bounds.
- `segments` `Tail::At(index + 1)` and its `MAX_SEGMENT_INDEX` guard (`marks.rs:882-886`):
  unreachable from a segment Complete writes. A page of 16 such segments names thousands of
  positions, and a step reads 233, so a step never finishes a whole page. I wrote that into the
  code. Only a hand-made group of tiny segments reaches it.
- `parts_all` `id == *upload_id` guard → `true`: `part_range(id)` is that session's own prefix, so
  a parsed key always carries the id. `n < MAX_PART_NUMBER` → `true`: it differs only at part
  10,000 with rows after it, which cannot happen.
- `marks.rs:481:61` (the second `&&`): differs only in a step that marks entries past a
  non-deletable cursor entry under segments-first. That only delays the drain to a later step.

## Gates

- `cargo test -p wyrd-custodian`: all 34 test binaries green. The new file has 21 legs, child-1's
  file 19, and `gc_mark_sweep` 21 (B(ix) is new).
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean. `cargo fmt --all`: clean.
- DST: `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test custodian retirement_restamp`, in a
  scratch target dir: both rows green at `MADSIM_TEST_NUM=50`. Reds A and B above came from the
  same setup at 5 seeds. `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`: clean.
- **Full gate**: `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci` (`cargo xtask ci`) on the patched
  worktree: **exit 0, "xtask ci: all checks passed"**. That was 2,481 tests passed and 0 failed,
  with both new DST rows at the gate's seeds, `typos`, `lint_docs` OK, `render_site` (101 pages,
  link audit OK), fmt, workspace clippy, machete, deny (advisories, licenses, bans, sources) and the
  ADR-0035 statics gate. Both brief tools (`typos`, `docs-renderer`) are on this host and ran; they
  were not warn-skipped. After the gate I changed only one comment, in the DST row (naming the
  rendezvous's clock source), then re-ran `cargo fmt --check`, madsim clippy, both DST rows at 50
  seeds, and `typos` on every touched file. All clean.
- `patch.diff` applies cleanly to `21467e6` (`git apply --cached --check` against a scratch index
  read from that commit).

## For the human at sign-off

- **C4 diff coverage cannot measure this bundle.** It applies `patch.diff` to `origin/main`, and
  child-1 (#1061, `20e6e21`) is not on `origin/main` yet (`git merge-base --is-ancestor` says so,
  checked 2026-11-13). This patch is built on the integration tip as the brief directs, so the
  gate will report "does not apply" again until #1061 lands. That is not something a Do iteration
  can fix.
- **The GC sweep rule changed** (item 2). Any mark whose event is a retirement token is now swept
  once its obligation is gone and it is `D` old and over an empty position. Iteration 1 flagged the
  residual trade as a follow-up; the human chose this change. The trade is written up above and in
  `gc.rs:857-869`.
- **`gc_mark_sweep.rs` B(viii) was edited** to drop its two retirement-token events, because the
  rule they pinned is the one the human changed. B(ix) now pins the new rule.
- **No new DST row for the sweep change.** The rubric asks a new destructive path to land with
  seeded Tier-0 coverage. This is a new arm on an existing one (the sweep, DST property 15, which
  still passes), and the brief puts every DST scenario except the drain-versus-GC `reclaiming` row
  out of scope. The arm's one race is a drain re-stamping the same position between the sweep's
  read and its delete. That is a different token's drain, because a token is never reused and its
  own drain is finished once its obligation is gone. The sweep's exact-value precondition already
  covers that race (leg D(i) in `gc_mark_sweep.rs`). If the reviewer asks for a row, it is a
  scope call for the human.
- Iteration 1's notes on throughput (one step per obligation per pass) and the uncalibrated
  `B_ops` still hold.
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed.
