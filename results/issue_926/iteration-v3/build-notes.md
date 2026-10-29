# Build notes — issue 926 / multipart-upload-part-stage (657.1), iteration 3

## Base and target

Target `getwyrd/wyrd` `main`. Built in the per-cycle worktree on its base `61e5341`
(`pdca-integrate: issue_744`; stack base `pdca-integration/main-r42c5348f203d`), the same base as
iterations 1 and 2. I started from iteration 2's `patch.diff` (it applied cleanly) and changed only
what this carry-forward asks for. `patch.diff` is the worktree against `61e5341`, the two new files
included. All `path:line` below are on the patched tree.

Files changed this iteration: `crates/core/src/multipart/stage.rs`,
`crates/core/tests/multipart_upload_part_stage.rs`, one sentence added to
`docs/design/architecture/05-building-block-view.md:244`, and six lines of
`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1714-1719` (the human's
authorization). `metadata.rs`, `multipart.rs`, `write.rs` and the custodian are exactly as
iteration 2 left them.

## The carry-forward, item by item

| # | Carry-forward item | What changed | Legs that prove it |
|---|---|---|---|
| 1 | Renewal must not block the fragment-write deadline (`stage.rs:1334` in iteration 2): race it against the pending work and the deadline | `renewing` (`stage.rs:1470`) no longer awaits `renew_if_due` first. When a renewal is due it builds the renewal future and polls it, the pending work and a deadline timer together in one `poll_fn` (`:1515-1524`). The renewal is polled first (it fell due before the work's next poll), then the work, then the timer | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing` (test `:1486`) — the reviewers' schedule |
| 1a | A write answered during a renewal must not be discarded as `WriteDeadlineElapsed` | Work that has finished is taken whatever the clock now reads: at the top of every loop round after the first, the work is polled once (`now_or_never`) **before** the deadline is judged (`:1482-1486`). In the race, the work is polled before the timer. Once the work is done, the renewal is run to its end (`:1526`) | `a_prime_a_write_answered_while_a_renewal_commits_is_taken` (`:1583`) — the adversary's exact schedule; `a_prime_a_write_answered_while_a_slow_renewal_waits_is_taken` (`:1644`) |
| 1b | A submitted renewal cut off by the deadline is reported unknown, keeping the `may_still_commit` doubt | The renewal's doubt and an `awaiting` flag now live in a `RenewalLedger` the caller owns (`:690`), so they survive the renewal future being dropped. `Footprint::renew` sets `awaiting` around its commit (`:1230-1232`). On abandonment, `RenewalLedger::abandoned` (`:700`) answers `InDoubt { may_still_commit: true }` if a commit was awaiting its answer (the store's own held report if it has one, else one staging writes with `backend` `ABANDONED_COMMIT`, `:152`); otherwise exactly the doubt earlier rounds left | the first leg above (InDoubt, `may_still_commit`, `Indeterminate`); `a_prime_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt` (`:1710`, two variants: nothing in doubt after a conflict; the store's own `may_still_commit: false` doubt after an unknown round) |
| 2 | Body-read timeout: core takes a body-idle bound, a caller parameter, no spawn | `StageRequest::body_idle_millis` (`:189`), validated non-zero (`:1091`). Each body read gets `Deadline::Body { at: now + body_idle_millis }` at the instant it begins (`:1392`), on the attempt's own clock, and goes through the same `renewing` race as a write deadline. A read still pending at it is `Stop::BodyIdle` (`:503`) | `d_a_stalled_body_is_given_up_at_its_idle_bound` (`:2532`, driven by the hand-fired timer); `d_the_idle_bound_is_per_read_not_per_body` (`:2598`); the new case in `an_invalid_request_writes_nothing` (`:3950`) |
| 3 | Proposal 0016 `:1714`: match the code's reading (refuse `SlowDown`, never probe onward) | `0016:1714-1719` reworded: another attempt's id at the pinned index proves no loss either (that attempt may release it, and the held batch then land), so the reserver never probes onward; it refuses `503` on reading one, or after the bounded rounds. Reflowed to **the same six lines**, because 133 citations elsewhere in the repo point at 0016 lines after it; a two-line growth would have made all of them stale. The code comment at `stage.rs:1003-1005` now cites it | `c_a_held_index_taken_by_another_refuses` (unchanged) pins the behaviour the text now states |

Docs currency: `StageRequest` gained a field, so `05-building-block-view.md:244` gains one sentence
(the renewal never holds staging past a write deadline; a body idle past the caller's bound stops
staging). The module doc (`stage.rs:29-36`, `:47-50`, `:96-101`) and `stage_part`'s *Termination*
section (`:1059-1064`) say the same; the latter no longer hands the body bound to the gateway.

### Design points worth a look

- **Poll order in the race: renewal, then work, then timer.** My first cut polled the work first.
  `d_renewal_runs_while_the_body_arrives_before_the_first_chunk` caught it: when a renewal is
  already due as a body read begins, polling the read first lets the read run (and, in that leg,
  move the clock) before the renewal reads its clock, so the renewal found the slot lease lapsed.
  The renewal was due before the read started, so it goes first, as it did before this iteration.
  An answer the work has is still never lost: if the renewal lands in the same poll, the loop polls
  the work again before it judges the deadline (`:1482`). Red row N7.
- **Why a finished write is taken past its deadline.** A D server refuses a write after its
  deadline (#638), so a write it answered `Ok` was applied in time, however late staging sees the
  answer. This is what makes the adversary's case pass: no deadline is judged once the work has
  finished.
- **Why the renewal is finished, not dropped, once the work is done.** Every later batch requires
  the slot at the bytes this attempt last put. A renewal dropped mid-commit that lands later would
  change the slot under the next intent, which would then conflict and stop with `SlotLost` (the
  session is unchanged, `Footprint::reconcile`). Finishing it costs one store call that is bounded
  by the store's contract, like every other metadata call here, and the work's deadline no longer
  applies once the work is done.
- **`StageRuntime::wait` now takes `&self`** (`:209`), where `CreateRuntime::wait` takes
  `&mut self`. The renewal reads the runtime's clock while the deadline timer is pending beside
  it, and one object cannot be borrowed shared (the clock reads) and exclusively (the pending
  wait) at once. Keeping `&mut self` would need the clock to be a second object: `stage_part`
  taking `clock: &impl StageClock` and `runtime: &mut impl StageRuntime` as two arguments, for
  every caller (657.2's included), to keep a `&mut` nothing uses — the test runtime's `wait` only
  clones an `Arc`. The trait is new in this slice, so no caller outside the test changes.
- **Deadline before renewal.** At the top of each round the deadline is judged before a due
  renewal starts. A wake where both are due now ends with the deadline, not a renewal: nothing is
  gained by renewing a request that is stopping. One existing leg depended on the old order:
  `d_a_lapsed_owned_lease_is_refused_never_revived` held a write past its 3 s deadline and expected
  the lapse to win. I widened its write window to `3 * TTL` (test `:2871`), as `d_ii_…` already
  does, so the lease lapse is again the only thing that can stop it. Its assertions are unchanged.

## Alternatives ruled out

- **The adversary's one-line fix** (poll the work with `now_or_never` right after `renew_if_due`).
  It makes the adversary's probe pass, but the deadline is still not enforced while the renewal
  commits: with the renewal commit held, staging stays pending past the deadline. Red row N2
  ("a due renewal awaited to its end, work re-polled after") is exactly that design, and it fails
  the reviewers' leg and the slow-renewal leg.
- **Check the deadline before starting a renewal, and await the renewal otherwise.** Same hole: a
  renewal that starts before the deadline and commits past it still holds staging. Not a fix for
  the reviewers' schedule.
- **Spawn the renewal.** Core is runtime-free (ADR-0009); the caller-supplied timer is the seam.
- **Report an abandoned renewal as `WriteDeadlineElapsed` with nothing in doubt.** That is the
  "dropped write reported as a clean outcome" the rubric's *Transactions* rule forbids; a commit
  whose await was abandoned may still land (`metadata-tikv`'s `timed_out_commit` makes the same
  call). Red row N4.
- **A whole-request bound instead of a per-read idle bound.** It would cut off an honest slow
  client that keeps sending, which renewal exists to keep alive (`0016:1824-1837`). Per-read is
  what "idle" means; `d_the_idle_bound_is_per_read_not_per_body` pins it (red row N9).
- **An optional idle bound (`Option<u64>`).** A caller could leave it unset and be back where
  iteration 2 was; the carry-forward says a stalled client must not hold its slot for ever. It is
  required, and `0` is refused as an invalid request.

## Refuting my own test

- **(a) Genuine red? Yes, with the limit the brief pre-declares.** The staging functions do not
  exist on the base, so with the whole fix reverted the test file does not compile, and C4-verify
  will report UNVERIFIABLE (exit 77). The binding evidence is the 52 demonstrated reds below. For
  this iteration the key one is row 38: iteration 2's `renewing`, put back as it was (renewal
  awaited before the deadline is checked or the work polled), turns all three new deadline legs
  red — the reviewers' schedule, the adversary's schedule and the slow-renewal schedule — plus the
  cut-between-commits leg. Rows 39-51 each remove one piece of the new design, and each turns its
  own leg red; row 52 is the C5 survivor, killed by a leg added for it.
- **(b) Production path? Yes.** Every leg calls the production `stage_part`, `reserve_slot` or
  `StagedAttempt::renew` over a real `RedbMetadataStore::in_memory()`. Sessions are made by the
  production `create_session`, fences by `abort_session`, and chunk bytes are read back by
  `read::read_chunk_verified`. The doubles sit only at the seams the brief names. The store
  wrapper delegates every applied batch and read to redb; the new `Act::Gate` (test `:582`) and
  `Then::Wait` (`:597`) only **delay** a real commit or a real read, which lands on redb when
  released or is dropped unapplied if staging abandons it. The chunk-store double holds a real put.
  The runtime is a manual clock with a hand-fired timer, and now also logs the length of every
  wait it is asked for (`:913`).
- **(c) Fixture includes the fault? Yes.** The slow renewal is the attempt's real renewal batch,
  held pending at the store seam while a real fragment put is held at the chunk-store seam. The
  deadline passes by moving the same clock staging reads and firing the same timer it waits on.
  In the reviewers' leg the gate is never opened, so staging can only end by abandoning the
  renewal; the leg waits for staging to end with the renewal still held. In the adversary's leg the
  put is answered from inside the renewal's own commit, before its deadline, and the commit then
  carries the clock past the deadline. The stalled body is a real channel body that sends nothing
  more; the idle bound passes on the same clock.

## Demonstrated reds

Each edit was applied alone to the final `crates/core/src/multipart/stage.rs`; the whole test file
was run (`cargo test -p wyrd-core --test multipart_upload_part_stage`, under `timeout 600`, with
`RUSTFLAGS=--cap-lints=warn` and a separate target dir — the workspace denies warnings, and a
mutant's unused binding must not stop it compiling, the same cap the C5 row uses); and the file was
restored and compared byte for byte with a saved copy after every run. A run counts as red only if
it compiled, exited non-zero, and every named leg is in its failure list. All 52 did. Rows 1-19
are the brief's required list, in its order; rows 20-37 are iterations 1-2's carry-forward and C5
reds, re-run because `renewing` changed under them; rows 38-52 are this iteration's.

Rows 1-49 ran before I added the two wait-length assertions (test `:1474`, `:1570`); rows 50-51
ran after, to show those assertions bind; row 52 ran after the leg the C5 run asked for (test
`:2687`). Adding an assertion or a leg cannot turn a failing leg green, so every row stands on the
final test file. `stage.rs` did not change between the runs (compared with the saved copy before
and after each batch). The driver script and its JSON record
are in my scratch directory (see Housekeeping); this table is the record.

| # | Edit (the brief's label first) | Named leg(s) that failed | Also failed | |
|---|---|---|---|---|
| 1 | B1: per-server require_absent removed | `b1_a_drain_recorded_just_before_the_intent_commits_makes_it_replan` | 0 | red |
| 2 | B2: full topology instead of topology minus draining | `b2_a_drain_recorded_before_staging_is_planned_around` | 4 | red |
| 3 | D: slot rewrite dropped from the renewal batch | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_renewal_commits_while_a_body_read_is_pending`, `d_ii_renewal_commits_while_a_fragment_write_is_pending` | 7 | red |
| 4 | D: renewal only at chunk boundaries (body read neither raced nor renewed) | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_renewal_commits_while_a_body_read_is_pending` | 6 | red |
| 5 | D-i: renewal only between delivered pieces, no timer (the wait made pending) | `d_i_renewal_commits_while_a_body_read_is_pending` | 13 | red |
| 6 | D-ii: the timer not raced against a pending fragment put | `d_ii_renewal_commits_while_a_fragment_write_is_pending` | 6 | red |
| 7 | A: last ChunkRef len == chunk_size | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | 0 | red |
| 8 | C: after an ambiguous reserve refuses, probe a fresh index | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | 0 | red |
| 9 | D: renewal moves a lapsed lease forward (check ignored) | `d_a_lapsed_slot_lease_is_refused_never_revived`, `d_a_lapsed_owned_lease_is_refused_never_revived` | 2 | red |
| 10 | C: same-index pin removed (lowest free index) | `c_an_ambiguous_reserve_retries_the_same_index` | 3 | red |
| 11 | E: absent read after a held intent taken as did-not-land | `e_ii_a_held_intent_that_never_settles_ends_in_doubt` | 1 | red |
| 12 | F: intent rounds += 1 made *= 1 | `f_the_intent_loop_is_bounded` | 3 | red |
| 13 | F: renewal rounds += 1 made *= 1 | `f_the_renewal_loop_is_bounded` | 1 | red |
| 14 | F: reserve probes += 1 made *= 1 | `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving` | 0 | red |
| 15 | F/C: reserve at_pin += 1 made *= 1 | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | 0 | red |
| 16 | A': fragment put deadline None | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | 2 | red |
| 17 | A': fragment put deadline Some(u64::MAX) | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | 2 | red |
| 18 | A': deadline taken at dispatch | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | 1 | red |
| 19 | A: a piece spanning a boundary merged into one oversize chunk | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | 16 | red |
| 20 | CF1: intent sends without checking any lease | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read`, `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read`, `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` | 0 | red |
| 21 | CF1: intent checks only the slot's lease | `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read` | 0 | red |
| 22 | CF1: intent checks on its first round only | `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` | 0 | red |
| 23 | CF1: intent checks at a reading taken before its drain scan | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read`, `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read` | 0 | red |
| 24 | CF2/3: renewal reads the clock once, before its first round | `d_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds`, `d_the_quiesce_renewal_reads_the_clock_every_round` | 0 | red |
| 25 | CF2/3: renewal checks on its first round only | `d_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds`, `d_the_quiesce_renewal_reads_the_clock_every_round` | 0 | red |
| 26 | CF4: slot read matching a sent intent not settled from the entry | `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted`, `e_an_entry_removed_after_its_intent_landed_is_a_typed_error` | 0 | red |
| 27 | CF5: reserve unpins on a session move without re-reading | `c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot` | 0 | red |
| 28 | item 7: intent releases on a session move without re-reading | `e_a_held_intent_landing_before_the_session_moves_is_adopted` | 0 | red |
| 29 | item 7: renewal releases on a session move without re-reading | `e_a_held_renewal_landing_before_the_session_moves_is_adopted` | 0 | red |
| 30 | item 8: undecodable slot read as another attempt's | `c_an_undecodable_slot_at_the_reserved_index_is_a_typed_error` | 0 | red |
| 31 | iter-1 extra: fragment wait not bounded by its deadline | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | 2 | red |
| 32 | iter-1 C5: check_live always Ok | `d_a_lapsed_slot_lease_is_refused_never_revived` | 6 | red |
| 33 | iter-1 C5: renew_if_due < made == | (no single named leg) | 17 | red |
| 34 | iter-1 C5: renew_if_due < made > | (no single named leg) | 18 | red |
| 35 | iter-1 C5: renew_if_due always returns early | (no single named leg) | 1 | red |
| 36 | iter-1 C5: chunk boundary == made != (watchdog) | (no single named leg) | 0 | red; the watchdog ended the process |
| 37 | C5 fix: re-read gated on an empty sent (intent and renewal) | `e_a_held_intent_landing_before_the_session_moves_is_adopted`, `e_a_held_renewal_landing_before_the_session_moves_is_adopted` | 0 | red |
| 38 | N1: iteration 2's renewing (renewal awaited before the deadline and the work) | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing`, `a_prime_a_write_answered_while_a_renewal_commits_is_taken`, `a_prime_a_write_answered_while_a_slow_renewal_waits_is_taken` | 1 | red |
| 39 | N2: a due renewal awaited to its end, not raced (work re-polled after) | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing`, `a_prime_a_write_answered_while_a_slow_renewal_waits_is_taken` | 1 | red |
| 40 | N3: no work poll after a renewal lands (deadline judged first) | `a_prime_a_write_answered_while_a_renewal_commits_is_taken` | 0 | red |
| 41 | N4: abandoned renewal ignores a commit still awaiting its answer | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing` | 0 | red |
| 42 | N5: awaiting never cleared after the commit answers | `a_prime_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt` | 0 | red |
| 43 | N6: race without the deadline timer | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing`, `a_prime_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt` | 0 | red |
| 44 | N7: race polls the work before the due renewal | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk` | 0 | red |
| 45 | N8: no body-idle bound (u64::MAX) | `d_a_stalled_body_is_given_up_at_its_idle_bound` | 0 | red |
| 46 | N9: idle bound measured once per body, not per read | `d_the_idle_bound_is_per_read_not_per_body` | 0 | red |
| 47 | N10: a zero body-idle bound not refused | `an_invalid_request_writes_nothing` | 0 | red |
| 48 | N11: the race's own deadline check dropped | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing` | 1 | red |
| 49 | N12: the outer deadline check judges before a due renewal is raced (deadline check dropped) | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | 1 | red |
| 50 | W1: the timer armed for wake + now, not wake - now | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | 1 | red |
| 51 | W2: the race's timer armed for deadline + now | `a_prime_the_write_deadline_cuts_off_a_renewal_still_committing` | 0 | red |
| 52 | C5 survivor: renew_if_due < made <= (the renewal due as an intent starts) | `d_a_renewal_due_as_an_intent_starts_is_sent_before_it` | 0 | red |

Row 36 is a loop that never yields; only the test file's watchdog can end it (exit 101 with its
message on stderr, about 35 s in). Rows 33-35 have no single named leg: they break renewal
scheduling as a whole, and 17, 18 and 1 legs fail. Row 35 fails only one leg now because
`renew_if_due` runs only before an intent; every renewal during a body read or a fragment write
now goes through the race in `renewing`, which rows 38-43 break instead.

## Gates run on the final tree

- **`cargo xtask ci`, through the project's runner** (`./engine/xtask.sh ci`, `$PDCA_WORKTREE`
  set), on the final tree `patch.diff` records (run twice: before and after the last leg was
  added; both `xtask ci: all checks passed`, exit 0). Its 18 steps ran:
  `typos`; the docs lint (`lint_docs: OK`); the docs render (99 pages, `render_site: link audit
  OK` — both brief-listed external dependencies were present); the gitlink, unsafe and blackbox
  guards; `cargo fmt --check`; workspace clippy with warnings denied; build; `cargo test
  --workspace --exclude wyrd-dst` (1,918 passed, 0 failed, the new file's 61 legs included);
  cargo-machete; all three cargo-deny runs; the ADR-0035 statics gate; the deploy guard; and
  clippy plus tests for `wyrd-dst` under `--cfg madsim` (101 passed). This is the brief's (H).
- `cargo fmt --all` leaves the tree unchanged, and clippy on `wyrd-core` and `wyrd-custodian`
  with `--all-targets` and warnings denied is clean: the target's commit-time checks pass.
- C4-verify will see a test that cannot compile on the reverted base and report UNVERIFIABLE
  (exit 77), as the brief's Verification posture pre-declares.

## C5: `cargo mutants --in-diff`

Run as the project's C5 row runs it (`scripts/mutants-in-diff`: `cargo mutants --in-diff
patch.diff --no-shuffle --cap-lints=true`, from the worktree, whole-package test suites), with two
local choices that do not change what is tested: `TMPDIR` set to my scratch directory for the tree
copy, and `--output` there, so no `mutants.out` lands in the worktree.

- **Full run, on the patch before the last leg was added: 143 mutants, 106 caught, 36 unviable,
  1 missed, 0 timeouts, in 16 min.** All 36 unviable are replacements with `Default::default()`
  (or a leaked box or vec of one) for types with no `Default`, which cannot compile even with
  lints capped.
- **The one survivor was a real gap, now closed:** `stage.rs:1439:33: replace < with <= in
  Stager::renew_if_due`. Since this iteration `renew_if_due` runs only right before an intent
  (renewal during a body read or a fragment write goes through the race in `renewing`), and no
  leg had a renewal falling due at exactly that instant. With `<=`, that renewal is deferred until
  the chunk's fragment writes are pending, so the slot and owned leases are renewed after the
  intent instead of before it. The new leg `d_a_renewal_due_as_an_intent_starts_is_sent_before_it`
  (test `:2687`) feeds one whole chunk at exactly `T0 + HALF` and asserts the renewal commits
  before the intent, at that reading. It fails on that mutant and on nothing else (reds row 52).
- **Re-check after the fix:** I re-ran only `renew_if_due`'s mutants on the final patch
  (`-F renew_if_due`, 4 mutants). The `<=` mutant is now caught, as is the `Ok(())` body. The
  other two, `<` made `==` and `>`, hit that subset run's 20 s auto timeout: with only `stage.rs`
  mutated it tests `wyrd-core` alone, a 3 s baseline, so its timeout is shorter than the test
  file's own 30 s leg timeout. In the full run above, with the C5 row's own baseline (`wyrd-core`
  and `wyrd-custodian`, 18 s, timeout 91 s; iteration 2's gate had 92 s), both were caught, and
  adding a leg cannot make a caught mutant survive.

So no mutant in this slice's code is left alive.

## For the human at sign-off

- **C4-verify will report UNVERIFIABLE**, as the brief pre-declares (the test calls API the patch
  adds). The evidence it cannot produce is the 52 demonstrated reds above; row 38 is the rejected
  iteration-2 `renewing` failing every new deadline leg.
- **Proposal 0016 is edited** (`0016:1714-1719`), on your authorization in the iteration-2
  carry-forward, overriding the brief's "no edit to proposal 0016" scope line. Same six lines, so
  no other citation of 0016 moves. Iteration 2's "deliberate departure from `0016:1714-1716`" is
  gone: the text and the code now say the same thing.
- **API that 657.2 builds on, changed this iteration:** `StageRequest::body_idle_millis` (a new
  required field; `0` is an invalid request), `StageRuntime::wait(&self, …)` (was `&mut self`),
  the new `Stop::BodyIdle`, and the new `ABANDONED_COMMIT` (the `backend` of the unknown outcome
  staging reports for a renewal it abandoned). `StagedAttempt::renew` is unchanged.
- **Behaviour worth knowing:** a renewal cut off by a deadline makes staging end
  `StageError::InDoubt` (classified `Indeterminate`), not `Stopped`, whenever its commit may still
  land. And once the pending work is done, a renewal already committing is run to its end; that
  wait is bounded only by the store's own contract, like every intent commit.
- **Size:** the patch is still mostly the test file. Seven legs (six for the carry-forward, one for
  the C5 survivor) and three small harness pieces (`Act::Gate`, `Then::Wait`, the wait-length log)
  were added this iteration. The production change is one function (`renewing`) rewritten, one
  small ledger type, one request field.
- **Seeded DST coverage** of these races stays out of this slice (`deferred: #660, #665` in the
  module doc).
- No external dependency was missing: `typos` and the docs renderer both ran inside `cargo xtask
  ci`. No `NEEDS-HUMAN` item from the build.

## Housekeeping

Nothing was committed, pushed or opened. The worktree holds the change uncommitted, matching
`patch.diff` (the two new files are untracked; I used `git add -N` only to produce the diff and
reset it after). `patch.diff` was checked to apply to `61e5341` in a throwaway index. The test is
also copied into the bundle at `multipart_upload_part_stage.rs` and
`crates/core/tests/multipart_upload_part_stage.rs`. **Scratch left for the harness:** my `rm -rf` of
my own scratch items was refused by the sandbox, so they are still under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_926/`), all named `pdca-builder-926-*`: the two build
caches `pdca-builder-926-target` (1.8 GB) and `pdca-builder-926-reds-target` (1.1 GB), the
mutants output dirs `-mutants` (23 MB) and `-mutants-recheck` (1.3 MB), the red driver dir `-reds`
(236 KB; the driver script and its JSON record), and small logs (`-ci.log`, `-ci-final.log` and its
`.counts`, `-mutants.log`, `-mutants-recheck.log`, `-apply.idx`). Nothing in them is needed: the
results are in this file. The `wyrd-docs-build-*` directories there are the docs render's own,
made by `cargo xtask ci`. In the worktree, the ignored `.cache/` and `mutants.out/` predate this
session (the previous Check beat's gate runs, 18:53 and 19:01-19:12) and are not mine.
