# Build notes — #932 (657.1a): UploadPart staging, safe half

Withheld from the reviewer. For the human at sign-off.

## Base

Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at `582da99`
(`pdca-integration/main-r42c5348f203d`: `getwyrd/wyrd` `main` with #653, #744 and #853 folded,
the stack base the driver chose). The v3 reference patch (`results/issue_926/iteration-v3/
patch.diff`) applied to this base cleanly, so every base citation in the brief (verified at
`4bac538`) still points at the same text here; I spot-checked `multipart.rs:891`, `:1000`,
`:1251`, `:1261`, `:1328`, `:1339`, `:2719`, `:2930`, `:4045`, `:4136`, `:4487`, `:4590`,
`:4972`, `:5032` and `05-building-block-view.md:244`. Line numbers below are in the **patched**
tree unless marked "base".

## What changed (path:line, patched tree)

| File | Change |
|---|---|
| `crates/core/src/multipart/stage.rs` (new, 1548 lines) | the staging functions: `reserve_slot` `:839`, `stage_part` `:1061`, the reserve loop `:859`, the intent loop `:1402`, `write_fragments` `:1336`, the bounded wait `bounded` `:748`, the body read `next_piece` `:773` / `yield_once` `:786`, the hand-off `StagedAttempt` `:276`, the error vocabulary `StageError` `:377` / `Stop` `:409` (`WriteDeadlineElapsed` `:452`, `WriteEffectUnknown` `:466`) |
| `crates/core/src/multipart.rs` | `pub mod stage;` `:155` beside `abort`/`create` (base `:150-151`); header prose `:8-23`, `:69`, `:105-107`; `DigestHasher` `:1092` (the one place SHA-256 is named, `Digest::sha256` `:1053` goes through it); the slot record's crate-private writer constructors `SlotRecord::reserve` `:2762` and `SlotRecord::with_lease` `:2780`, modelled on `SessionRecord::open` (base `:2323-2328`), both going through the same `TryFrom<SlotRecordWire>` decode uses |
| `crates/core/src/metadata.rs` | the drain key's one home in core: `DESIRED_DSERVER_PREFIX` `:98`, `desired_dserver_key` `:102`, `parse_desired_dserver_key` `:108` — byte-for-byte the custodian's old spelling (base `custodian/src/desired_state.rs:35-48`) |
| `crates/core/src/write.rs` | `encode_chunk` made `pub(crate)` `:140` (base `:138`, private), so a part's chunks are encoded exactly as an object's |
| `crates/custodian/src/desired_state.rs` | uses the core key (`:28`, re-export `desired_key` `:39`, so `staged.rs:104` and `tests/staged_repair.rs:83` are untouched); the two `deferred: #657` notes (base `:226-230`, `:246-249`) now say the intent writer fences, the seeded interleaving stays `deferred: #665` (`:214-221`, `:236-239`) |
| `docs/design/architecture/05-building-block-view.md` | one new paragraph `:246`, after `:244` (not an edit of `:244`, per the brief); nothing about renewal; no landing-bound claim |
| `docs/design/proposals/draft/0016-multipart-commit-protocol.md` | the one authorized sentence, `:1714-1719` (the next sentence is only re-wrapped, same words) |
| `crates/core/tests/multipart_upload_part_stage.rs` (new) | 54 tests, harness kept in this file for 657.1b |

No production caller is added. `crates/core/src/lib.rs`, `08-crosscutting-concepts.md`, ADRs,
`Cargo.lock` and `deny*.toml` are untouched.

## How I carved v3

Kept: the reserve loop (pinning, ambiguous rounds, re-read after a session move), the intent
loop (fresh `t_auth` and lapse check every round after the drain read, settle-by-exact-bytes,
the entry-then-slot-then-entry adoption, `reconcile`/`moved_on`), `Doubt`, `Footprint`, the
hand-off shape, `DigestHasher`, the slot constructors, the drain key move, the custodian notes,
the 0016 sentence, and most of the test harness.

Removed (657.1b's): `Footprint::renew`/`apply`, `RenewalLedger`, `renew_if_due`, the renewal
arm of `Stager::renewing`, `StagedAttempt::renew`, `RenewOutcome`, `RENEWAL_ROUNDS`,
`ABANDONED_COMMIT`, `Stop::RenewalRoundsSpent`, `Kind::Renewal`, `Act::Gate`/`Then::Wait` and
every renewal leg. v3's separate `StageClock` trait existed only so `StagedAttempt::renew` could
take a bare clock; I folded `now_millis` into `StageRuntime` (`:201`). 657.1b can split it back
out when it adds `renew`.

Changed on purpose (the brief's fixes and the things v3 got wrong):

1. **Per-write outcome tracking** (`write_fragments` `:1336`). v3 used `try_join_all`, which
   drops the other writes on the first error and loses the answers already in when the deadline
   fires. Now the writes sit in a `FuturesUnordered` and their answers go into a `Vec` that lives
   *outside* the raced future (`:1361-1366`), so at the deadline staging knows exactly which
   writes answered how and which it abandoned. No spawning (ADR-0009).
2. **Two write stops, never conflated.** `Stop::WriteDeadlineElapsed` is the clean one: every
   non-`Ok` write was refused `NotApplied`, or the deadline had passed before dispatch (checked
   at `:1345` so nothing is sent late). `Stop::WriteEffectUnknown` covers a write still pending
   at the deadline (abandoned), a store `WriteEffect::Unknown`, or any other failure (the trait
   says a fault does not promise nothing landed). Unknown outranks refused.
3. **Empty pieces.** v3 armed the idle deadline per `body.next()`, so an empty piece re-armed it
   (v3 `stage.rs:1392-1401`). Now one read is `next_piece` (`:773`), which skips empty pieces
   inside itself, and the whole read is bounded once from its start (`:1256-1259`). So an empty
   piece cannot move the deadline by construction, and a piece is never dropped or re-asked.
4. **The fake enforces the deadline at its publication point** (test file, `FleetState::apply`)
   with `WriteDeadlineExpired::if_elapsed`, and checks again after its publishing step with
   `if_publication_unverified`. v3's fake ignored `deadline_millis`.
5. **Refusal-answer legs.** New legs for `stage_part` with a full slot range, the session leaving
   `Open` under the reserve (plain and pinned paths), and the intent's own session fence (E′).
6. **One lease per intent batch** (`:1441`): the new owned entry and the slot both get
   `next_expiry(t_auth, ttl)` = `max(t_auth + ttl, prior slot lease + 1)`. v3 gave the owned
   entry `t_auth + ttl` and the slot the max; when two intents share a clock reading, v3's owned
   lease came out one millisecond *earlier* than the slot lease the batch replaced, which breaks
   the brief's (D) line "no lease this slice writes is earlier than the slot lease it replaces".
   The cost: the owned lease can be a few ms past `t_auth + ttl`. I read (D)'s "at `now + TTL`"
   as the base value before that forward bump; the leg pins the exact values
   (`d_the_reserve_and_every_intent_write_their_leases_forward`).
7. **`Deadline` enum dropped.** The brief says keep "`Stager::renewing` without its renewal arm,
   and `Deadline`". With the renewal arm gone, the race is `bounded()` (`:748`): check the clock,
   race the work against `runtime.wait(deadline - now)`, take an answer the work has when the
   timer fires, wait out an early timer. The write path's stop now depends on per-write outcomes
   (item 1), so a `Deadline::cause()` mapping no longer fits; each caller maps `None` itself. Same
   race, less code. If the human wants the enum back for 657.1b's renewal race, it is a small add.
8. **ChunkRef pushed after the writes land** (`:1325`), not before: the hand-off never lists a
   chunk whose writes did not all land. Staging stops in that case anyway, so this only matters
   for readability.

## Judgment calls the human should see

* **`classify` of an abandoned-only write stop reads `Terminal`.** `StageError::source()` for
  `WriteEffectUnknown` is the first store report it carries (`:619-621`), so a store
  `WriteEffect::Unknown` classifies `Indeterminate`. A write staging abandoned at the deadline has
  no store report. I did **not** fabricate one (a `WriteDeadlineExpired` or a
  `CommitUnknownResult` the store never raised), so `classify` falls to its default there. The
  typed variant says "unknown" in both cases and its `Display` says so; the `StageError` doc
  (`:367-375`) states the gap. A reviewer may want a fabricated `Unknown` instead; I think a
  made-up seam error is worse. No production caller reads this class yet (657.2 branches on the
  variant).
* **Where the idle bound is measured from.** The brief says "the bound measured from the last
  non-empty piece (or from the start of the body)". I measure from the instant the read *began*:
  right after the previous non-empty piece was taken in and any chunk it completed was staged.
  With no chunk staged in between (the empty-pieces leg) the two are the same instant. Where a
  chunk is staged, my reading does not charge the client for staging's own intent and write
  time, which is the "per read, not per body" rule. Say so if the stricter reading was meant.
* **A yield after each empty piece** (`next_piece` `:779`, `yield_once` `:786`). Not in the
  brief. Without it, a body that always has an empty piece ready keeps `next_piece` looping
  inside one poll, so the timer is never polled and the idle bound never fires (an unbounded
  loop). The leg `a_double_prime_an_endless_run_of_empty_pieces_stops_at_the_bound` binds it;
  without the yield the leg trips the watchdog (process exit 101 after 30 s + 5 s).
* **A zero `write_window_millis` is an invalid request** (`:1079-1081`). The brief lists three
  invalid inputs; a zero window can never be honoured either (every write would be too late),
  so I added it to the same check rather than let it reach the store.
* **`zip(placement)` in `write_fragments`** (`:1352-1358`) cannot truncate: the placement is
  planned for exactly `fragments.len()` servers (`intent` is called with that count, `:1309`, and
  `select_distinct_domains` returns exactly `n` or an error). I did not add a runtime check.
* **The test fake runs each put in its own `tokio::spawn`ed task**, as a remote D server keeps
  working on a put after its caller stops waiting. Without that, dropping the stager's future
  would cancel the in-process put, and the late-put leg and straddle run 2 could not show what the
  store does afterwards. This is in the test only; core spawns nothing.

## ⚑ For the human (from the brief's Design)

This slice does **not** establish an absolute landing bound. 0016 sizes the orphan grace on "no
accepted write can be applied more than `W_write` after it was authorized"
(`G_orphan > W_write + δ_clock`, `0016:1553-1578`). The seam gives less: a publishing step no
store bounds can straddle the deadline by any amount, which is exactly `WriteEffect::Unknown`
(`crates/traits/src/lib.rs:871-892`, `:1128-1141`). What this slice guarantees: no publication
*begins* after the deadline (the store judges at its publication point), an unknown or abandoned
write is never counted either way, and every position it could land on is named by a durable
owned entry first. The module doc (`stage.rs:46-53`) says that and claims no bound. Decide
whether the gap gets an issue (the backends under #638, or the `G_orphan` owner #625) before the
multipart verbs ship.

## Test runner

For the many red/green runs I used `cargo test -p wyrd-core --test multipart_upload_part_stage`
under `timeout` (the brief: "RED is reachable in-process on the plain `cargo test -p wyrd-core`
target"); every leg also has its own 30 s watchdog. The whole gate is `./engine/xtask.sh ci`
(`cargo xtask ci`), run once at the end — result below. The test needs no display or service:
an in-memory redb store, an in-process fake, tokio.

## Verification posture

Net-new (posture (a)): `stage.rs` does not exist on the base, so the test cannot compile there
and C4-verify should report UNVERIFIABLE (exit 77), as the brief pre-declares. The demonstrated
reds below are the red half.

## Demonstrated reds (required)

Each row: the temporary edit, the leg run, the result. Driven by a script that applies the edit,
runs the leg by exact name, restores both files from memory, and checks their SHA-256 against
the clean copies afterwards (both matched). 31 runs, 31 red, 0 green, 0 compile errors.

| Edit (`stage.rs` unless noted) | Leg | Result (failing assertion) |
|---|---|---|
| per-server `require_absent(desired:dserver:<S>)` removed from the intent batch (`:1460` under `if false`) | `b1_a_drain_recorded_just_before_the_intent_commits_makes_it_replan` | red: "the drain fence failed the intent", `left: Committed` |
| plan against the full topology (`plan` `:730` → `select_distinct_domains(topology, ..)`) | `b2_a_drain_recorded_before_staging_is_planned_around` | red: `IntentRoundsSpent { chunk: 1, rounds: 4 }` — every round planned onto S and lost to the fence |
| same-index pin removed (`pinned = Some(index)` dropped) | `c_an_ambiguous_reserve_retries_the_same_index` | red: `Reserved { index: SlotIndex(0), .. }`, not 1 |
| after the ambiguous rounds, probe a fresh index (unpin, `lost.insert`, `continue`) | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | red: `Reserved { index: SlotIndex(2), .. }` instead of `SlowDown` |
| `stage_part`'s full-range refusal answered `NoSuchUpload` (`:1104`) | `c_stage_part_with_every_slot_held_answers_slow_down` | red: got `NoSuchUpload` |
| session left `Open`, plain path, answered `SlowDown` (`:989`) | `c_a_session_leaving_open_under_the_reserve_is_no_such_upload` | red: got `SlowDown` (the `reserve_slot` run) |
| session left `Open`, pinned path, answered `SlowDown` (`:976`) | `c_the_pinned_reserve_answers_no_such_upload_once_the_session_left_open` | red: got `SlowDown` (lands run) |
| pinned index cleared on a newer epoch without settling (`:961` → `if false && ..`) | `c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot` | red: "exactly one slot of this attempt", `left: 2` |
| lapse check skipped on an intent retry (`check_live(if rounds == 1 { t_auth } else { 0 })`) | `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` | red: staged — the retry was sent and revived the slot |
| lapse check at a reading taken **before** the drain read | `d_an_intent_refuses_a_slot_lease_that_lapsed_during_its_drain_read` and `d_an_intent_refuses_an_owned_lease_that_lapsed_during_its_drain_read` | red, both: staged |
| after an unknown intent outcome, an absent read taken as "did not land" (`doubt.release(); sent.clear()` in place of `doubt.settle()`) | `e_ii_a_held_intent_that_never_settles_ends_in_doubt` and `e_i_a_held_intent_released_before_the_next_batch_is_adopted` | red, both (`Stopped` not `InDoubt`; `ChunkIdTaken`) |
| slot-read path answers "slot lost" without re-reading the entry (branch at `:1495` under `if false`) | `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted` | red: `SlotLost` |
| `rounds += 1` → `rounds *= 1` (intent) | `f_the_intent_loop_is_bounded` | red: wrapper's runaway guard, "staging never stopped committing (401 commits)" |
| `probes += 1` → `probes *= 1` (reserve) | `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving` | red: same runaway guard (401 commits) |
| `at_pin += 1` → `at_pin *= 1` (ambiguous reserve) | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | red: the 4th round reserved, `Reserved { index: SlotIndex(1), .. }` |
| fragment put with `None` (`:1357`) | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | red: "deadline = t_auth + W", `left: None` |
| fragment put with `Some(u64::MAX)` | same | red: `left: Some(18446744073709551615)` |
| fragment put with its deadline taken at dispatch (`now + W`) | same | red: `left: Some(1003777)` = `t_auth + 777 + W` |
| the write wait not bounded (`bounded(..)` → `every.await`, `:1367`) | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | red: "timed out waiting for staging to stop at the deadline" (10 s) |
| the fake's publication-point check removed (test file, `if_elapsed(..).filter(\|_\| false)`) | `a_prime_a_held_put_released_after_expiry_is_refused_and_not_published` | red: "nothing of it was published", `left: Some(0)` |
| a `WriteEffect::Unknown` answer counted as written | `a_prime_a_publication_straddling_the_deadline_is_never_counted` | red: run 1 got `Staged` |
| an abandoned put reported as nothing written (abandoned → `WriteDeadlineElapsed`) | same | red: run 2 got "none was sent" |
| the intent's `require(mpu == Open@E)` removed, nothing else (`:1456`) | `e_prime_1_an_abort_just_before_an_intent_commits_stops_it` and `e_prime_2_a_held_intent_then_an_abort_stops_with_nothing_in_doubt` | red, both: E′1 staged all four chunks under `Aborting`; E′2's released batch `Committed` |
| empty piece re-arms the idle deadline (each `body.next()` its own bounded read) | `a_double_prime_empty_pieces_do_not_count_as_progress` | red: staged — the body ran to its end |
| body read awaited without the idle bound | `a_double_prime_a_stalled_body_read_stops_at_its_idle_bound` | red: no wait ever armed (10 s) |
| a piece spanning a boundary merged into one oversize chunk (`split_at(rest.len())`, `>=`) | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | red: 2 chunks, not 4 |
| last `ChunkRef.len` = `chunk_size` (`:1328`) | same | red: read-back differs (zero-padded tail) |
| extra: the yield after an empty piece removed | `a_double_prime_an_endless_run_of_empty_pieces_stops_at_the_bound` | red: watchdog ended the process (exit 101, 37 s) |

Note on the stalled-body and unbounded-write reds: they fail at a sync point (a 10 s `until`)
because the mutant never reaches the state the next assertion checks. They still fail fast and
for the right reason. The late-put leg first failed the same way ("timed out waiting for the fake
to refuse the late put"); I changed it to wait for either answer and then assert, re-ran that
mutant, and it now fails on the real assertion (the row above).

## Refuting my own test

* **(a) Genuine red?** Yes. Every required edit above was applied for real and the named leg went
  red (31/31). The whole file cannot compile without `stage.rs`, which is the net-new posture.
* **(b) Production path?** Yes. Every leg calls the production `stage_part` / `reserve_slot` in
  `crates/core/src/multipart/stage.rs` over a real `RedbMetadataStore::in_memory()`, real Create
  (#894) for every session and real Abort (#895) for the fences. The wrapper only reorders or
  answers commits; the fake is a test double for the chunk store (the trait seam), not for any
  code under test. The red edits are in the production file, except the fake's own check (whose
  leg tests the fake, as the brief asks).
* **(c) Fixture includes the fault?** Yes. B1 writes the drain record for a server the plan
  actually selected, read off the batch under commit; B2 drains the selector's first pick on the
  full topology (asserted). (A′) holds the real put and moves the real clock past its real
  deadline; the straddle leg holds the put *inside* the publishing step after it passed the
  check; the late put is released after expiry and reaches the fake's real check. (E′) runs a
  real Abort, not a hand-written session value. (C)'s ambiguous legs hold or apply the actual
  batch staging sent. The drivers wait for the other five puts to be acknowledged before moving
  the clock, so the fault reaches exactly the one held put.

## Flake check

Two driver races found and fixed during the build (both in the test, not in staging):
`Semaphore::add_permits` hands a permit straight to a queued waiter, so "permits back to 0" does
not mean staging ran — the empty-pieces driver now syncs on the wait log; and the unheld puts run
in their own tasks, so a driver that moved the clock as soon as the held put reached its gate
could make them late — held-put drivers now wait for the other five `Acked` events. After the
fixes: 16 consecutive full runs of the file, 0 failures.

## C5 (advisory): `cargo mutants --in-diff`

Run with `--in-place --in-diff patch.diff`, tests limited to this file plus core's lib unit
tests (`--test multipart_upload_part_stage --lib`), timeout 90-120 s.

* **First run, lints as configured:** 133 mutants in core, 65 caught, 68 unviable, 0 missed.
  Too many unviable: the workspace sets `warnings = "deny"`, so a mutant that leaves a parameter
  unused (e.g. `check_live -> Ok(())`, `next_expiry -> 0`, `> with >=` on a `usize` against 0)
  fails to compile and hides whether a test would have caught it.
* **Second run, `--cap-lints true`:** 133 mutants, **102 caught, 31 unviable, 0 missed,
  0 timeouts**. The 31 left are real compile failures: `Default::default()` (or a leaked
  `Default`) for types with no `Default` impl — `Digest`, `SlotRecord`, `UploadId`, `AttemptId`,
  `PartNumber`, `SlotIndex`, `OwnedEntry`, `StageError`, `Halt`, `Session`, `Refusal`,
  `ReserveOutcome`, `StagedAttempt`, `Intent`, `Reserve`, `Found`, `StageOutcome`, and
  `Some(Default::default())` for `bounded`'s generic output. None is a behaviour a test could
  miss; each is unrepresentable.
* **Custodian** (`--package wyrd-custodian`, its own full test suite, `--cap-lints true`): 2
  mutants on `draining_servers`, 1 caught (`-> Ok(BTreeMap::new())`), 1 unviable
  (`DServerLifecycle` has no `Default`).

**Survivors: none.** Every viable mutant in this slice's code was caught by the selected tests
(this file plus core's lib unit tests; cargo-mutants does not record which test caught each).

## Gates

`./engine/xtask.sh ci` (`cargo xtask ci` in the worktree) on the final tree: **exit 0, "xtask
ci: all checks passed"**. Every step ran, none skipped: `typos`, `lint_docs.py`, `render_site.py
--check` (99 pages, link audit OK), gitlink-guard, unsafe-guard, blackbox-guard, `cargo fmt
--check`, `cargo clippy --workspace --all-targets` (warnings denied), build, `cargo test
--workspace --exclude wyrd-dst` (this file: 54 passed), `cargo-machete`, `cargo deny check`
(both configs: advisories, bans, licenses, sources ok), statics (ADR-0035), deploy-guard
(ADR-0010), and the DST tier (`clippy` + `test -p wyrd-dst` under `--cfg madsim`).

Commit-readiness: the target repo configures no git hooks (`core.hooksPath` unset, only sample
hooks); its formatter and linters are the `cargo fmt` / `clippy` steps above, which pass. The
publish commit still needs its DCO `Signed-off-by` (the publish step's job). `patch.diff` is
`git diff` of the final tree against `582da99` (new files via intent-to-add), and `git apply
--check --reverse` on the tree confirms it is exactly the delta.

## External dependencies

`typos` (installed, `typos-cli`) — run over every changed file: clean.
`docs-renderer` — `python3 docs/publishing/tools/render_site.py --check` rendered 99 pages, link
audit OK; `lint_docs.py` OK.
