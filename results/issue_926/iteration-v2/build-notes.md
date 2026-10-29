# Build notes — issue 926 / multipart-upload-part-stage (657.1), iteration 2

## Base and target

Target `getwyrd/wyrd` `main`. Built in the per-cycle worktree on its base `61e5341`
(`pdca-integrate: issue_744`; stack base `pdca-integration/main-r42c5348f203d`), the same base as
iteration 1. I started from iteration 1's `patch.diff` (it applied cleanly) and changed only what
the carry-forward asks for, plus one more case of the same bug class (item 7 below). `patch.diff`
is the worktree against `61e5341`, the two new files included; it was checked to apply cleanly to
`61e5341` in a throwaway index. All `path:line` below are on the patched tree.

Files changed this iteration: `crates/core/src/multipart/stage.rs`,
`crates/core/tests/multipart_upload_part_stage.rs`, and one sentence of
`docs/design/architecture/05-building-block-view.md:244`. `metadata.rs`, `multipart.rs`,
`write.rs` and the custodian are exactly as iteration 1 left them.

## The carry-forward, item by item

| # | Carry-forward item (and the 4 batched-review findings) | What changed | Leg that proves it |
|---|---|---|---|
| 1 | Intent must check the slot lease and every earlier owned lease at its fresh `t_auth` before each submission, after the drain scan, keeping any `may_still_commit` doubt (review findings `stage.rs:1296`, `:1310`, `:1317`) | `Footprint::check_live` now returns a `Stop` (`stage.rs:1041`), and the intent calls it every round at `t_auth`, which is read after the drain scan, wrapping the cause in `doubt.end` so a held batch's doubt is kept (`stage.rs:1425-1433`) | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read` (test `:2528`), `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read` (`:2563`), `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` (`:2599`, conflict and held variants; held ends `InDoubt` with `may_still_commit`) |
| 2 | Renewal retries must re-read the clock and re-run `check_live` before each new submission, still settling batches already sent (review finding `stage.rs:997`) | `Footprint::renew` takes the clock, reads it at the top of every round and checks every lease there before sending; earlier rounds were already settled by the read after their commit (`stage.rs:1064-1084`). It returns the landed round's reading, which `renew_if_due` uses for the next due instant (`stage.rs:1157-1162`, `:1304-1318`) | `d_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds` (`:2634`, conflict and held variants) |
| 3 | `StagedAttempt::renew` must take the clock, not a fixed `now_millis`, and re-check per round | New `StageClock` trait (`stage.rs:175`); `StageRuntime: StageClock` (`:181`); `StagedAttempt::renew(store, clock: &impl StageClock, ttl)` (`:363`). A clock-only trait so 657.2 does not need a whole staging runtime to quiesce | `d_the_quiesce_renewal_reads_the_clock_every_round` (`:2680`); `d_the_quiesce_renewal_never_moves_a_lease_backwards` moved to the new signature |
| 4 | Intent recovery: a fresh slot read matching an intent already sent settles that intent (re-read `sidx:`), not `SlotLost` | After the entry read finds nothing, a slot read that equals a sent intent's slot value reads the entry again and adopts the intent whose exact bytes are there (`stage.rs:1486-1511`). If the slot proves a batch landed but the entry is gone or rewritten, that is a new typed error, `Stop::EntryLost` (`:457`), with no fragment written | `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted` (`:3041`), `e_an_entry_removed_after_its_intent_landed_is_a_typed_error` (`:3179`) |
| 5 | Reserve: don't clear the pinned index after an absent read followed by a newer session epoch; re-read the original index first | When the session read finds the session moved and an index is pinned, the reserve reads that index again before unpinning; its own slot there is kept, fenced on the new `Open` bytes, or answered with the refusal if the session left `Open` (`stage.rs:852-888`). The slot read is now one helper, `read_own_slot` (`:916`) | `c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot` (`:1938`) |
| 6 | `d_a_lapsed_slot_lease` should drop `tx` inside the driver | The driver waits until the timer permit is taken (so the timer path, not end-of-body, is what refuses), then drops `tx` (`:2278`). Same change in `d_a_renewal_after_the_session_left_open_resurrects_nothing` (`:2385`) and `d_a_slot_lost_under_an_unchanged_session_is_a_typed_error` (`:2500`), which had the same hang | the iteration-1 C5 timeouts below now fail in seconds |
| 7 | *(mine, same class as 5)* | The intent and the renewal had the same pattern as the reserve: a session move released the doubt without one more read of the batch's own record. `reconcile` now returns the moved session instead of releasing (`stage.rs:1170-1194`); when a round of the loop had an unknown outcome (`!sent.is_empty()`), the intent re-reads its entry and the renewal its slot, then both call `moved_on` (`:1200`), which releases and adopts or refuses (`:1133-1152`, `:1512-1532`) | `e_a_held_intent_landing_before_the_session_moves_is_adopted` (`:3102`), `e_a_held_renewal_landing_before_the_session_moves_is_adopted` (`:3142`) |
| 8 | *(mine, rubric: ADR-0045)* | The reserve treated a slot value that does not decode as "another attempt's". It is now `Stop::Record`, keeping the doubt (`stage.rs:916-933`) | `c_an_undecodable_slot_at_the_reserved_index_is_a_typed_error` (`:1976`) |

Item 7 in more detail, since the carry-forward did not ask for it. Without it, a held intent or
renewal that lands after its own record was read and before the session moves is missed; the doubt
is released anyway. In most schedules the next round's settle read still finds it (every later
batch of the loop requires the slot bytes the landed one replaced, so it conflicts, and the read
after that conflict sees the landed record). It goes wrong when that batch was the loop's **last**
round, or when the next round's lease check refuses: staging then stops "with nothing in doubt"
while the batch it sent has landed. That contradicts `StageError::InDoubt`'s own contract ("never
'nothing was written'", `stage.rs:387-388`). The two legs schedule exactly that: the held batch is
the last round.

Docs currency: `05-building-block-view.md:244` now says every round of an intent or renewal,
retries included, checks every lease at its own fresh reading, and that an unknown outcome is
re-read once more after the session is seen to move. The module doc says the same
(`stage.rs:46-54`, `:56-76`) and the `reserve_slot` doc names both reserve changes (`:730-734`).

## Decisions carried from iteration 1 (unchanged)

1. A held reserve never probes elsewhere; `SlowDown` if another attempt holds the pinned index or
   after `RESERVE_AMBIGUOUS_ROUNDS` (this is still a deliberate departure from `0016:1714-1715`,
   see "For the human").
2. Reserve probe exhaustion is `SlowDown`.
3. A lease never moves backwards: every batch stamps `max(now + TTL, current slot lease + 1)`.
4. The intent also `require_absent`s its own `sidx:` key.
5. Placement is checked before the reserve, so a fleet with too few non-draining domains leaves no
   slot.
6. The wait for fragment writes is bounded at their deadline (`Stop::WriteDeadlineElapsed`).
7. The runtime seam (now `StageClock` + `StageRuntime`); every clock read goes through
   `StageClock::now_millis`, the one clock of the staging lease lifecycle (rubric: one clock per
   lifecycle; the module doc's "One clock", `stage.rs:88-94`).
8. A session at a later `Open` epoch is adopted; one no longer `Open` is `Refused(NoSuchUpload)`.
9. The chunk bound stops reading.
10. Renewal is checked before each intent, before and during each body read and fragment write.

## The test harness changes

- **Read hooks** in the store wrapper (`Then`, `ReadHook`, `after_read`, `on_read`, test
  `:530-622`): after the n-th read of a key (a `get`) or prefix (a `scan`) returns, and before its
  answer is handed back, the wrapper can advance the clock, release the held batch, or commit a
  batch. That is how a lease is lapsed *during* the drain scan and how a held batch lands
  *between* two of staging's reads.
- **`Act::ConflictThenAdvance`** (`:508`): a round lost slowly.
- **A watchdog** (`within`, `:101`): `LEG_TIMEOUT` is now 30 s (the repo's watchdog in
  `chunkstore-grpc/tests/write_deadline.rs:63`), down from 120 s, which was longer than
  cargo-mutants' 88 s auto timeout and turned 7 detections into "timeouts". A leg that spins
  without yielding cannot be timed out from its own task, so a std thread ends the test process
  (exit 101) 5 s after the timeout. It writes its message with `writeln!(io::stderr())` because
  libtest captures `eprintln!` from threads a test spawns and the process ends before that capture
  is shown. No `static` (ADR-0035).

## Alternatives ruled out

- **Renew between intent rounds instead of refusing an owned lease that lapsed mid-intent.** A
  renewal sent while an intent of the same chunk may still land would only conflict with it (both
  rewrite the slot under `require(slot == prior)`), which is why the brief forbids it. A lapse
  inside the intent loop needs half a TTL of store latency inside at most 4 rounds, because a
  renewal runs just before every intent (`stage.rs:1365-1366`). Refusing is `0016:1843-1847`'s rule.
- **Fold renewal into the intent (every intent rewrites every owned lease).** That changes the
  intent batch the brief specifies, and its size grows with the chunk count: a part of N chunks
  would put N(N+1)/2 owned entries across its intents instead of N (5,050 instead of 100 for
  N = 100).
- **Settle an intent by reading the slot first and the entry second.** This also closes the
  race in item 4. In the common path it costs the same two reads; it saves one read only in the
  rare slot-match path. I kept the entry-first order with the extra entry read because that is the
  carry-forward's literal instruction and it keeps iteration 1's structure; the diff for either is
  about the same size (one `if` block, ~20 lines).
- **For the reserve, recognise its own attempt in the probe's range scan instead of re-reading the
  pinned index.** It would decode every slot value on every probe (up to `MAX_INFLIGHT_PARTS` = 16,
  `multipart.rs:5094`), and it would be a second mechanism for the same property, so neither
  could be shown to be needed (a mutant of one survives the other). I did the direct re-read the
  carry-forward names.
- **Restructure the chunking loop so no mutant can stall it, instead of the watchdog.** Not
  possible in general: any loop whose progress is a computed length can be mutated to make zero
  progress. The watchdog is ~20 lines of test code and no production change.

## Refuting my own test

- **(a) Genuine red? Yes, with the limit the brief pre-declares.** The staging functions do not
  exist on the base, so with the whole fix reverted the test file does not compile; C4-verify will
  report UNVERIFIABLE (exit 77). The binding evidence is the 37 demonstrated reds below: each
  named production edit, applied alone to the final `stage.rs`, turned its named leg red, and the
  file was restored and compared byte for byte after each (`cmp`-equivalent check in the driver
  script, and a final `cmp` against the saved copy).
- **(b) Production path? Yes.** Every leg calls the production `stage_part`, `reserve_slot` or
  `StagedAttempt::renew` over a real `RedbMetadataStore::in_memory()`; sessions are made by the
  production `create_session`, fences by the production `abort_session`, and chunk bytes are read
  back by the production `read::read_chunk_verified`. The doubles are only at the seams the brief
  names: the store wrapper (it delegates every applied batch and every read to redb; hooks only
  add a real write or a clock step between reads), the placement-aware chunk store, and a runtime
  with a manual clock and a hand-fired timer.
- **(c) Fixture includes the fault? Yes.** Each lapse is produced by moving the same clock the
  code reads, at the moment named: inside the real drain scan of the intent round under test, or
  inside a real lost commit round. Each "held batch lands between two reads" is the real held batch
  committed on the real store between the real reads; each session move is a real committed write
  of a canonical later-epoch session record (`open_at`, checked to decode and re-encode byte for
  byte). The undecodable slot is real bytes at the real key.

## Demonstrated reds

Each edit was applied alone to the final `crates/core/src/multipart/stage.rs`, the whole test file
was run (`cargo test -p wyrd-core --test multipart_upload_part_stage`, under `timeout 600`), and
the file was restored and compared with a saved copy. The final run, on the final `stage.rs` and
the final test file, covered all 37: every one compiled, turned every named leg red, and was
restored. (An earlier run of the first 36 on the tree before the C5 fix below gave the same
result.) The driver script and its JSON record lived in my scratch directory (removed at the
end; this table is the record).

| # | Edit (brief's label first) | Named leg(s) that failed | Also failed |
|---|---|---|---|
| 1 | Per-server `require_absent(desired:dserver:<S>)` removed (the loop iterates an empty set) | `b1_a_drain_recorded_just_before_the_intent_commits_makes_it_replan` | — |
| 2 | Full topology instead of topology minus draining (`topology.clone()` for `topology.excluding(draining)`) | `b2_a_drain_recorded_before_staging_is_planned_around` | b1, three b3 legs |
| 3 | The slot rewrite dropped from the renewal batch | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_…`, `d_ii_…` | `d_renewal_falls_due_…`, `d_the_quiesce_renewal_never_moves_…`, `e_a_held_renewal_landing_…`, `e_an_unknown_renewal_…` |
| 4 | Renewal only at chunk boundaries (the body read neither raced nor renewed before: `next.await`) | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_…` | 4 other D legs |
| 5 | Renewal checked only between delivered pieces, no timer (the wait made `pending()`) | `d_i_renewal_commits_while_a_body_read_is_pending` | `d_ii_…`, both lapse legs, session-left, slot-lost, due-at-half, deadline leg |
| 6 | The timer not raced against a pending fragment put (`try_join_all(puts).await`) | `d_ii_renewal_commits_while_a_fragment_write_is_pending` | `d_a_lapsed_owned_lease_…`, `a_prime_a_write_pending_…` |
| 7 | The hand-off's last `ChunkRef` given `len == chunk_size` | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | — |
| 8 | After an ambiguous reserve refuses, a probe of a fresh index (unpin, mark lost, `continue`) | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | — |
| 9 | Renewal that moves a lapsed lease forward (the renewal's `check_live` result ignored) | `d_a_lapsed_slot_lease_is_refused_never_revived`, `d_a_lapsed_owned_lease_is_refused_never_revived` | `d_a_renewal_retry_…`, `d_the_quiesce_renewal_reads_…` |
| 10 | The same-index pin removed (never pinned: lowest free index) | `c_an_ambiguous_reserve_retries_the_same_index` | `c_a_held_index_taken_…`, `c_a_held_reserve_landing_after_…`, `c_a_held_reserve_landing_before_…` |
| 11 | After an unknown intent outcome, an absent read taken as "did not land" with `may_still_commit: true` (`doubt.settle()` made `doubt.release()`) | `e_ii_a_held_intent_that_never_settles_ends_in_doubt` | `d_an_intent_retry_refuses_…` (its held variant) |
| 12 | `rounds += 1` made `rounds *= 1`, intent loop | `f_the_intent_loop_is_bounded` | `e_ii_…`, `e_a_held_intent_whose_session_moved_on_…`, `e_an_out_of_flight_unknown_a_read_settled_…` |
| 13 | `rounds += 1` made `rounds *= 1`, renewal loop | `f_the_renewal_loop_is_bounded` | `f_a_held_renewal_…` |
| 14 | `probes += 1` made `probes *= 1`, reserve | `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving` | — |
| 15 | `at_pin += 1` made `at_pin *= 1`, reserve | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` (exact commit count) | — |
| 16 | Fragment put with deadline `None` | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…`, `e_a_held_intent_landing_between_…` |
| 17 | Fragment put with deadline `Some(u64::MAX)` | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…`, `e_a_held_intent_landing_between_…` |
| 18 | Deadline taken at dispatch (`now_at_dispatch + W`) | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…` |
| 19 | A piece spanning a chunk boundary merged into one oversize chunk (`split_at(rest.len())`) | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | 14 others |
| 20 | **CF1** The intent sends without checking any lease (`let _ = footprint.check_live(t_auth)`) | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read`, `d_an_intent_refuses_an_owned_lease_…`, `d_an_intent_retry_refuses_…` | — |
| 21 | **CF1** The intent checks only the slot's lease | `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read` | — |
| 22 | **CF1** The intent checks on its first round only (`if rounds == 1`) | `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` | — |
| 23 | **CF1** The intent checks at a reading taken before its drain scan | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read`, `d_an_intent_refuses_an_owned_lease_…` | — |
| 24 | **CF2/3** The renewal reads the clock once, before its first round | `d_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds`, `d_the_quiesce_renewal_reads_the_clock_every_round` | — |
| 25 | **CF2/3** The renewal checks on its first round only (`if rounds == 0`) | `d_a_renewal_retry_refuses_…`, `d_the_quiesce_renewal_reads_…` | — |
| 26 | **CF4** A slot read matching a sent intent is not settled from the entry (`.any(|_| false)`; falls to `SlotLost`) | `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted`, `e_an_entry_removed_after_its_intent_landed_is_a_typed_error` | — |
| 27 | **CF5** The reserve unpins on a session move without re-reading (`if pinned.is_some() && false`) | `c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot` | — |
| 28 | *(item 7)* The intent releases on a session move without re-reading its entry (`if !sent.is_empty() && false`) | `e_a_held_intent_landing_before_the_session_moves_is_adopted` | — |
| 29 | *(item 7)* The renewal releases on a session move without re-reading its slot (`if !sent.is_empty() && false`) | `e_a_held_renewal_landing_before_the_session_moves_is_adopted` | — |
| 30 | *(item 8)* An undecodable slot read as another attempt's | `c_an_undecodable_slot_at_the_reserved_index_is_a_typed_error` | — |
| 31 | *(iteration 1 extra)* The wait for fragment writes not bounded by their deadline | `a_prime_a_write_pending_past_its_deadline_is_abandoned` (at the 30 s leg timeout) | — |
| 32 | *(iteration 1 C5 timeout)* `Footprint::check_live` always `Ok(())` | `d_a_lapsed_slot_lease_is_refused_never_revived` | 6 more lapse legs; no hang |
| 33 | *(iteration 1 C5 timeout)* `renew_if_due` `<` made `==` | (no single named leg) | 20 legs; no hang |
| 34 | *(iteration 1 C5 timeout)* `renew_if_due` `<` made `>` | (no single named leg) | 20 legs; no hang |
| 35 | *(iteration 1 C5 timeout)* `renew_if_due` always returns early | (no single named leg) | 11 legs; no hang |
| 36 | *(iteration 1 C5 timeout, a loop that never yields)* the chunk boundary `==` made `!=` | (the watchdog) | 6 legs failed, 19 never finished; the watchdog ended the process (exit 101) with its message on stderr, about 35 s in |
| 37 | *(item 7, the C5 fix below)* The re-read gated on an empty `sent` (the `!` dropped), intent and renewal | `e_a_held_intent_landing_before_the_session_moves_is_adopted`, `e_a_held_renewal_landing_before_the_session_moves_is_adopted` | — |

Iteration 1's seven C5 timeouts were, in its numbering: `check_live` → `Ok(())` (`stage.rs:961`),
its `<=` → `>` (`:967`), `Footprint::renew` → `Ok(())` (`:986`), the chunk boundary `==` → `!=`
(`:1166`), `renew_if_due` → `Ok(())` (`:1194`), and its `<` → `==` and `>` (`:1195`). Rows 32-36
reproduce five of them by hand; the final cargo-mutants run below catches all seven (0
timeouts). Six fail fast now because the hung legs' drivers drop the body sender; the seventh (row
36) is a loop that never yields, which only the watchdog can end.

## Gates run on the final tree

- **`cargo xtask ci`, through the project's runner** (`./engine/xtask.sh ci`, `$PDCA_WORKTREE`
  set), run twice: once before the C5 fix and again on the final tree. Both: `xtask ci: all checks
  passed`, exit 0. It ran `typos`, the docs lint, the docs render with
  its link audit (`render_site: link audit OK` — both brief-listed external dependencies were
  present), the gitlink, unsafe and blackbox guards, `cargo fmt --check`, workspace clippy with
  warnings denied, build, `cargo test --workspace --exclude wyrd-dst` (2,012 passed, 0 failed; the
  new file's 54 legs included), cargo-machete, all three cargo-deny runs, the ADR-0035 statics
  gate, the deploy guard, and clippy plus tests for `wyrd-dst` under `--cfg madsim`. This is the
  brief's (H), on the tree `patch.diff` records.
- `cargo fmt --all -- --check` clean, and clippy on `wyrd-core` and `wyrd-custodian` with
  `--all-targets` clean: the target's commit-time checks pass.
- C4-verify will see a test that cannot compile on the reverted base and report UNVERIFIABLE (exit
  77), as the brief's Verification posture pre-declares.

## C5: `cargo mutants --in-diff`

Run as the project's C5 row runs it (`scripts/mutants-in-diff`: `cargo mutants --in-diff
patch.diff --no-shuffle --cap-lints=true`, from the worktree, whole-package test suites), with two
local choices that do not change what is tested: the tree copy under my scratch `TMPDIR`, and
`--output` there, so no `mutants.out` lands in the worktree.

- **Final run, on the final `patch.diff`: 132 mutants, 98 caught, 34 unviable, 0 missed, 0
  timeouts; exit 0.** Caught by file: `stage.rs` 91, `metadata.rs` 5, `multipart.rs` 1,
  `desired_state.rs` 1. All 34 unviable are replacements with `Default::default()` (or a leaked
  box or vec of one) for types that have no `Default`, which cannot compile even with lints
  capped.
- **Iteration 1's 7 timeouts are gone.** They were real detections that took longer than the
  88-96 s auto timeout: the lapse legs' bodies stayed open, and one mutant made a loop that never
  yields. Fixed by carry-forward item 6 (and the same fix in two more legs), the 30 s leg timeout,
  and the watchdog (see "The test harness changes"; rows 32-36 of the reds table).
- **One survivor on the first run of this iteration, fixed.** That run (on the tree before the
  last change) reported 132 mutants, 97 caught, 34 unviable, 1 missed: `Doubt::holds -> true` at
  the then `stage.rs:622`. It was equivalent — the gate decided whether to read the record again
  after a session move, and reading it when nothing was held changes no outcome, only adds a
  read. Rather than record an equivalent mutant, I removed `Doubt::holds` and gated the re-read
  on "a round of this loop had an unknown outcome" (`!sent.is_empty()`, `stage.rs:1136`,
  `:1515`). Its only mutant, dropping the `!`, is caught by both session-move legs (reds row 37,
  and caught in the final run).

So no mutant in this slice's code is left alive.

## For the human at sign-off

- **C4-verify will report UNVERIFIABLE**, as the brief pre-declares. The evidence it cannot
  produce is the 37 demonstrated reds.
- **API change 657.2 builds on:** `StagedAttempt::renew(&mut self, store, clock: &impl
  StageClock, lease_ttl_millis)`. `StageClock` is new, and `StageRuntime` now requires it.
- **Two changes beyond the carry-forward** (items 7 and 8): the intent and renewal re-read their
  own record after a session move, as the reserve now does; and an undecodable slot is an error.
  Both are small and each has its own leg and red.
- **Still a deliberate departure from 0016's text** (iteration 1's decision 1): a held reserve
  whose index another attempt then holds refuses `SlowDown` instead of probing onward, because
  probing onward could give one attempt two slots. Worth a look against `0016:1714-1716`.
- **The watchdog ends the whole test process** if a leg spins without yielding. That only happens
  on a broken build, and it turns a hang into a failure the gates see.
- **Seeded DST coverage** of these races stays out of this slice (`deferred: #660, #665` in the
  module doc).
- No external dependency was missing: `typos` and the docs renderer both ran inside `cargo xtask
  ci`. No `NEEDS-HUMAN` item from the build.

## Housekeeping

Nothing was committed, pushed or opened. The worktree holds the change uncommitted, matching
`patch.diff` (the two new files are untracked; I used `git add -N` only to produce the diff and
reset it after). The test is also copied into the bundle at `multipart_upload_part_stage.rs` and
`crates/core/tests/multipart_upload_part_stage.rs`. My scratch directories under `$PDCA_SCRATCH`
(`pdca-builder-926-reds`, `-v1`, `-apply`, `-mutants`, `-mutants-final`, `-target`, and the
`-ci.log` and `-ci-final.log` files) are removed at the end. The `wyrd-docs-build-*` directories
there are the docs render's own, made by `cargo xtask ci`, and are left to the harness.
