# Build notes — issue 661, round 2 (gc-orphan-ledger-paged-walk)

Base: `origin/main` @ `605b33a` (the worktree's HEAD; the brief's `3969a3a` plus #799, which
touches `crates/core` multipart code and tests, `Cargo.lock` and one architecture doc — none of
the files this slice cites moved). Every `path:line` below is on the patched tree unless it says
"base".

## What the patch does

GC and the post-restore pass no longer read the `orphan:` ledger with one `scan`.

* **GC reads one window per pass** (`crates/custodian/src/gc.rs:855-898`, `read_window`): at most
  `ORPHAN_WALK_BUDGET` = 65,536 entries (`gc.rs:80`), over as many `scan_page` calls as the
  store's page cap needs, resuming after the key the previous pass stopped at. That position is a
  persisted record, `gc:orphan-cursor` (`gc.rs:139`), written last, only if it still holds the
  value the pass started from (`gc.rs:1060-1080`, `advance_cursor`).
* **Only retention-safe conclusions from a partial read.** A fragment with no mark in the window
  is kept (`gc.rs:371`) unless its chunk holds an expired pending lease; then the window's silence
  counts only inside the key range it covered (`Span::covers`, `gc.rs:835-842`), and outside it
  the fragment's own key is read (`unread_mark`, `gc.rs:904-915`). So a mark on another page still
  outranks the lease — round 1's main bug.
* **Fragment-less marks are swept** (`gc.rs:417-453`): a window mark at a position no listing of
  its server reported this pass, older than `D = 16 s` (`gc.rs:125`), not protected by the
  reference set, on a server in the fleet. Listings are gathered over the whole fleet first
  (`gc.rs:325-329`), so a fleet naming one server twice is still one server.
* **Every mark delete is guarded** (`GuardedBatch`, `gc.rs:958-1025`): a reclaim's mark delete and
  a sweep delete require the exact bytes the pass judged; a batch that conflicts is retried write
  by write, so one refreshed mark costs only itself. At most `LEDGER_WRITE_BATCH` = 1,000 writes
  per commit (`gc.rs:93`), including the pending-ledger deletes (`gc.rs:470-475`).
* **Restore asks each candidate's own key** (`crates/custodian/src/restore.rs:418-458`): one point
  `get` per unreferenced fragment, and the mark is a put-if-absent in `MARK_BATCH` batches
  (`restore.rs:452-458`, `record_marks` at `restore.rs:524-534`). It never pages the ledger.
* **Leg E** is a compile-time assertion next to the deployed grace
  (`crates/server/src/custodian.rs:116-127`).
* **Signals** (never a silent skip): unreadable marks (`gc.rs:1119`), marks of servers outside the
  fleet (`gc.rs:456-465`, `gc.rs:1149`), a lost cursor update (`gc.rs:1163`), sweeps
  (`gc.rs:1102`), and one progress event per pass (`gc.rs:1132`).
* **Docs**: `docs/design/architecture/06-runtime-view.md` §6.7, step 2 (four sub-bullets), for the
  new persisted record and the paged walk.

## How each round-1 finding is addressed

| Round-1 finding | What I did |
|---|---|
| T4 BUG ×3 — an orphan outside the page falls through to expired-pending reclamation | Fixed: `gc.rs:361-372` + `unread_mark`. Test: `an_unread_mark_on_another_page_outranks_an_expired_pending_lease` (`gc_ledger_walk.rs:864`), with marks both on the cursor and strictly behind it. |
| T4 BUG — the sweep deletes unconditionally, a concurrent refresh loses | Fixed: guarded deletes (`gc.rs:447-452`, `gc.rs:1014-1025`). Tests: `a_sweep_loses_to_a_mark_refreshed_after_the_pass_read_it` (`:963`), `a_reclaim_retires_only_the_mark_it_judged` (`:1038`), `a_pass_that_reclaimed_is_changed_even_when_its_sweep_loses` (`:999`). |
| T4 BUG — writers pass `None` for their write deadline | Recorded-rejected in `review-rejected.md` with reasons; it is the same question as the deferred C1 NEEDS-HUMAN item. See "The open policy question" below. |
| T4 TEST-GAP ×3 — no seeded Tier-0 DST for the destructive path | Added DST property 12 (`crates/dst/tests/custodian.rs:2177-2472`): a real concurrent task re-stamps a sweep target and a reclaim target at a seed-drawn instant while GC walks a paged ledger on the simulated-TiKV store; plus a coverage leg proving the race is reached; plus a regression-seed entry (`:2600`). |
| C5 — causal counter-cases | Both are sequential tests above, and the DST property covers the refresh race under a real scheduler. |
| T5 / C5-mutants — intermediate commit count and size; 4 batch-control mutants survived | Leg B asserts every pass's commit sizes (`[1000 × 65, 536]`); restore's batching test asserts `[1000, 1000, 500]`. `cargo mutants --in-diff` via `scripts/mutants-in-diff`: **69 mutants, 40 caught, 29 unviable, 0 missed**. |
| T2 — restore still loads the whole ledger into one `HashMap` | Replaced with point reads; leg F asserts restore receives zero `orphan:` page entries. |
| C4 — `cargo xtask ci` and diff-coverage timed out at 7200 s | `./engine/xtask.sh ci` passed end to end here (`xtask ci: all checks passed`). The round-1 log shows six `crates/server/tests/custodian_gc.rs` tests "running for over 60 seconds", including `deployed_run_loop_refuses_duplicate_ids`, which never runs GC — so I read that as host contention, not GC logic. With this patch that file runs in 0.17 s. |

## Success criterion, leg by leg

All in `crates/custodian/tests/gc_ledger_walk.rs` (17 tests). The metadata double
(`LedgerMeta`, `:97-262`) refuses `scan` past `LOWERED_CAP` = 1,000 with `ScanCapExceeded`; its
`scan_page` is written directly over its `BTreeMap` with `page_limit`/`page_start`/`page_cursor`,
and caps pages at the same 1,000, as redb's `with_scan_cap` does — so one pass needs 66 pages to
read its budget. It counts `orphan:` entries received, records any `scan` that could reach
`orphan:`, logs every commit, and can land a concurrent write just before the commit that writes
a key. Every pass builds a fresh `GcContext` (`gc_pass`, `:364`).

* **A** — `a_gc_pass_survives_an_orphan_ledger_past_the_cap` (`:459`): 3,007 actionable marks.
* **B** — `one_pass_receives_exactly_the_budget_and_the_ledger_drains_in_ceil_p_over_b_passes`
  (`:489`): P = 2B + 7; each pass receives exactly `min(left, B)`; drains in exactly 3 passes; no
  orphan `scan`; commit sizes per pass; a fourth pass over the empty ledger answers `Satisfied`.
* **C** — `the_actionable_tail_is_reached_behind_a_retention_safe_head` (`:565`): head B + 1 in
  grace, tail 5; tail reclaimed within ⌈(head + tail) / B⌉ + 1 = 3 passes (it takes 2); head
  untouched, byte for byte; the first (partial) pass answers `Changed`, never `Satisfied`.
* **D** — `d1_…` to `d5_…` (`:625-790`). D(iii) also asserts the off-fleet audit line.
* **E** — `crates/server/src/custodian.rs:123-127`, `const _: () = assert!(LATE_WRITE_DEADLINE_MILLIS
  < GC_GRACE_WINDOW_MILLIS)`. It lives in the server crate because `GC_GRACE_WINDOW_MILLIS` is
  private to `crates/server/src/custodian.rs`; `LATE_WRITE_DEADLINE_MILLIS` is `pub` in
  `wyrd_custodian::gc` for exactly this. A const assertion fails every build, not only `cargo
  test`. It is not in the red→green file, so it is not part of C4-verify's count.
* **F** — `restore_sees_every_mark_whatever_its_page_and_never_restamps_one` (`:796`):
  B + 1,000 + 1 fragment-less marks ahead of three pre-marked fragments, the last being the
  ledger's last key; all three `already_marked` with bytes unchanged; the stray still marked.
* **G** — `cargo xtask ci` green (above).

Extra tests beyond the legs: the duplicate-id fleet (`:923`), the cursor another pass moved
(`:1075`), unreadable marks (`:1114`), and restore's batching plus a concurrent mark (`:1156`).

## Constants

* `B = 65,536` (`gc.rs:64-80`): 1/16 of `SCAN_CAP`, the ceiling the brief allows. Memory: a key is
  at most 73 bytes, a value 20, so a full window stays under 10 MiB. Progress: a 1.78 M-mark
  retirement drains in 28 passes, ~14 minutes at the deployed 30 s interval. Every pass lists the
  whole fleet anyway, so a smaller B only adds passes.
* `LEDGER_WRITE_BATCH = 1,000` (`gc.rs:82-93`): ~200 KB per guarded batch against FDB's 10 MB, and
  the same count restore's `MARK_BATCH` uses.
* `D = 16,000 ms` (`gc.rs:95-125`): `W_repoint` 5 s + `W_write` 10 s + `δ_clock` 1 s. These are
  this slice's choices; no constant for them existed. The W's are private to `gc.rs`; the first
  writer slice that enforces them will need them in a crate the write path can reach (`core`) —
  that move is its call.

## Design choices and the alternatives I rejected

1. **Point reads for expired-lease candidates, not "judge only in-window fragments".** A chunk's
   fragments sit under different D servers, so their keys (`orphan:<d>:<chunk>:<i>`) are spread
   across the ledger. Judging only in-window fragments would reclaim part of an expired chunk and
   then delete its `pending:` entry (base `gc.rs:225-229`), leaving the out-of-window fragments
   with no evidence at all — kept forever. Avoiding that needs a second persisted record per
   chunk across passes. The point read costs one `get` per listed fragment of an expired-lease
   chunk that the window did not cover, and only under `ExpiredPendingPolicy::Reclaim`; the
   deployed default is `Defer`, where it costs nothing.
2. **Restore point reads, not paging the ledger into a map (round 1).** Paging into a map keeps
   every mark: ~1.78 M × ~140 B ≈ 250 MB for one maximal retirement, growing with the ledger. A
   point read keeps O(1) per candidate; restore already holds the fleet listing. Cost: one round
   trip per unreferenced fragment — for 1 M strays on TiKV at ~1 ms, ~17 minutes for an operator
   one-shot. (At that scale restore's own `scan(b"inode:")` of 1 M+ objects already fails; that
   is outside this slice.)
3. **Guarded deletes, not blind deletes plus a re-read.** A re-read cannot close the window
   between it and the delete. The guard costs one precondition per write (the key read once more
   inside the transaction), plus a per-write retry only when a batch conflicts.
4. **A loop of pages inside one pass, not one `scan_page` per pass.** On a store whose page cap is
   below B, one call would under-read (leg B requires exactly B). The loop costs ⌈B / cap⌉ calls:
   66 at the double's cap.
5. **The sweep answers to the reference-set gate** (`gc.rs:436`). Stricter than the brief asks: no
   sweep while the reference set is incomplete, and none at a referenced position. It only ever
   keeps a mark longer.
6. **`Changed` for a partial pass** (`gc.rs:488-492`). `Satisfied` means "nothing to do"; a pass
   that read one window of a larger ledger cannot say that. The deployed loop ignores the outcome;
   existing tests with small ledgers still see `Satisfied`.
7. **Base behaviour I changed deliberately**: the expired-lease arm no longer deletes the orphan key
   (base `gc.rs:216` deleted it blind; here there was no mark in the pass's read, and deleting one
   written since would lose evidence); an unreadable mark now keeps its fragment and is never
   deleted (base skipped the value, so its fragment looked unmarked and could be reclaimed on a
   lease, and the blind cleanup then deleted the unreadable mark — the brief says such a value is
   "left untouched, never rewritten, and never reclaimed on").
8. **No knob to turn the sweep off.** It would need a `GcContext` field (the brief forbids it) or
   global state (the ADR-0035 statics gate forbids it), and leg D requires the sweep.

## Invariant to restore — how each clause holds

* *Readable at every size*: no `scan` of `orphan:` remains (`orphan_leases` is gone); every test
  asserts zero orphan `scan`s.
* *Every mark has a deleter*: a mark is consumed by a reclaim or by the sweep; the exceptions are
  deliberate and signalled — unreadable values (child-2's to decode), marks of servers outside the
  fleet (never observed), and marks at referenced positions.
* *Footprint bounded by a constant*: ≤ B entries per pass; point reads bounded by expired-lease
  fragments (GC) or by the fleet listing (restore).
* *Only retention-safe conclusions*: see the three bullets above; nothing is destroyed or
  overwritten because a record was missing from a partial read.

## Leader change

Survives it. The cursor is in the metadata store, so the next leader resumes where the last one
stopped. The update is conditional on the value the pass started from, so a deposed leader still
finishing a pass cannot rewind a newer cursor (`a_pass_never_moves_a_cursor_another_pass_moved`).
At worst a window is read twice, which is harmless: deletes are guarded.

## Red → green (C4-verify, the project's runner)

`engine/scripts/run-verify.sh`, run with `WYRD_VERIFY` pointed into `$PDCA_SCRATCH` (worktree and
branch removed afterwards):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test gc_ledger_walk (fix applied)
test result: ok. 17 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-custodian --test gc_ledger_walk (production reverted, test kept)
test result: FAILED. 3 passed; 14 failed
run-verify.sh: PASS — red without the fix, green with it (17 test(s) ran red).
```

**14 of 17 ran red, each at an assertion in the test file** (lines 468, 501, 580, 640, 708, 819,
881, 943, 982, 1022, 1060, 1089, 1137, 1179; line 819 is leg F's `.expect(...)` on the pass's
`Result`, the rest `assert!`/`assert_eq!`). No compile error: the file names only symbols on the
base. The 3 green on the base are D(ii), D(iv) and D(v) — the over-deletion guards the brief
expects to be green there. (The script says "17 test(s) ran red": it counts tests that ran in the
red leg, 3 of which passed.)

## Refuting my own tests

* **(a) Genuine red?** Yes. With production reverted, 14 tests fail at assertions (above). I also
  mutated the guarded delete into a blind delete: both DST properties and both sequential race
  tests went red; and `cargo mutants --in-diff` leaves 0 of 69 mutants alive.
* **(b) Production path?** Yes. Every test drives the production `reconcile_step` (fenced control
  point) or `reconcile_after_restore`; only the stores are doubles, as `tests/gc.rs` does. The DST
  property drives the same entry points over the simulated-TiKV model.
* **(c) Fixture includes the fault?** Yes. The ledgers are larger than the store's `scan` cap
  (legs A, B, C, F, and the unread-mark and cursor tests), the double refuses over-cap scans with
  the real `ScanCapExceeded` and caps pages as a real backend does, and the races are real writes
  landing between the pass's read and its delete — a concurrent madsim task in DST, an interposed
  write at the commit in the sequential tests. Nothing is curated out: fillers on off-fleet
  servers exist precisely to push the targets onto later pages.

## Other files touched, and why

* `crates/custodian/tests/segmented_map_restore.rs:631-648` — its fixture poisoned restore's
  whole-ledger `orphan:` scan to prove an unreadable record is named before a later read fails.
  That scan no longer exists (restore reads each candidate's own key, and a pass holding an
  unreadable record marks nothing, so it never reads one). The `pending:` leg stays; the doc says
  why the `orphan:` leg went.
* `crates/dst/tests/custodian.rs:1785-1796`, `:2001-2002` — the restore two-readings coverage leg
  assumed the pass's two `inode:` readings were three hops apart. Removing the `orphan:` scan made
  them two hops apart, and the writer's earliest landing (its start tick plus a two-hop commit)
  then always tied with the second reading and lost the tie: no delay reached the divergence
  window (I printed the interleavings: `saw=[]` for every delay). `RESTORE_PASS_LEAD = 2` starts
  the writer 2 ms ahead of the pass; the coverage leg passes again over 50 seeds, and the
  invariants it asserts are unchanged.

## Commands run (all bounded by `timeout`)

* `cargo fmt --all -- --check`: clean.
* `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets` and
  `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets`: clean.
* `cargo test -p wyrd-custodian`: all green. `cargo test -p wyrd-server --test custodian_gc`: 10
  pass in 0.17 s.
* `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian`: 16 pass
  (13 s), including `gc_walk_loses_every_race_to_a_refresh` and `gc_walk_reaches_the_race`.
* `./engine/xtask.sh ci` (the configured C4-ci gate): `xtask ci: all checks passed` — typos, docs
  lint and render, guards, fmt, clippy, build, workspace tests, machete, deny, conformance
  vectors, statics, deploy guard, DST.
* `scripts/mutants-in-diff`: 69 mutants — 40 caught, 29 unviable, 0 missed.
* `cargo doc -p wyrd-custodian --no-deps`: 12 private-intra-doc-link errors, the same 12 as on the
  base (CI does not build rustdoc); this patch adds none.

## The open policy question (for sign-off)

Round 1's C1 item, already deferred to sign-off: should the fragment-less sweep ship before any
writer enforces `W_write` / `W_repoint`? My reading of today's tree: no writer relies on a mark
written *before* its fragment lands (the write path commits after all acks,
`crates/core/src/write.rs:227-232`; reconstruction and rebalance write first and mark only
displaced positions in the repoint commit, `reconstruction.rs:931-947`, `rebalance.rs:530-547`).
So the sweep removes no evidence any current writer depends on; the risk is for the multipart
teardown and repoint slices, which 0016 obliges to enforce both deadlines before they write such
marks. The sweep also narrows a base hazard: a stale, expired mark left at a position that a later
repair writes into lets GC delete the new fragment before its repoint commit lands (base and this
patch alike, if the mark is still there when the write lands); base kept fragment-less marks
forever, the sweep removes them once they are past `D` and listed empty. If you decide the
sweep must wait, the smallest change is to skip the sweep loop (`gc.rs:417-453`), and leg D(i)
and the DST sweep assertions would then have to be dropped with it.

Also still for you (round 1's validation row): whether the per-pass envelope — ≤ 10 MiB window,
1,000-write commits, ~14-minute laps at maximum cardinality, and the progress signal
(`gc_orphan_walk_entries`, `gc_orphan_walk_laps`) — is acceptable in production. In-memory tests
cannot establish that.
