# Build notes — issue 1017 (625.3: the reaper's terminal exits) — iteration 2

Base: `6e44de9 pdca-integrate: issue_1015` (the harness worktree, getwyrd/wyrd @ main lineage).
625.1 is on it (`427e5ec custodian: reap multipart uploads whose client went silent`, folded by
`6181de4`/`6e44de9`), and so is #659's drain: `20e6e21` (records drain, #1061), `3a392c8` (bytes
drain + mark guard incl. arm (0), #1062), `f54db83` (session leg + terminal delete, #1013). Arm (0)
is `crates/custodian/src/retirement/marks.rs:307`.

This iteration starts from iteration 1's patch (applied unchanged, then fixed in place). Line
numbers are on the patched tree unless marked "base".

## What the carry-forward asked, and what changed

### 1. Clock identity of the tombstone judgment (the C5 finding, 3 batch-review findings)

Iteration 1 put `clock_source: WALL_CLOCK_SOURCE` inside `Tombstone::Window` in
`ReaperPolicy::default()`, while the reaper held sessions by `ReaperContext::clock_source`. A
logical-clock context therefore held a wall tombstone as foreign and then deleted it, and kept its
own expired ones (the reviewer's `clock-probe`).

Fix: the policy names **no** clock. `Tombstone::Window { window_millis }`
(`crates/custodian/src/retirement.rs:414`). The judgment takes a `SessionClock` — the reaper's
`clock` and `clock_source`, always taken as one pair from the one `ReaperContext`
(`retirement.rs:448-461`, `SessionClock::of`). `Tombstone::passed(session, on)`
(`retirement.rs:424-442`) judges a `Window` only when `on` is `Some`, only for a session whose
`clock_source` equals `on.source`, and reads `on.clock` only then.

* Wiring, all inside `crates/custodian` (no server edit): `reconcile_step` builds
  `reaper.map(SessionClock::of)` and hands it to `retirement::reconcile(ctx, sessions)` on the
  deployed (reconstruction) path (`crates/custodian/src/reconciliation.rs:189-193`), and
  `reconcile_record_only(reaper)` takes the whole `ReaperContext` on the one-shot path
  (`reconciliation.rs:207`, `retirement.rs:630-638`). `Drain` gains `sessions`
  (`retirement.rs:650`); the terminal gate reads it (`crates/custodian/src/retirement/sessions.rs:925`).
* Which clock owns the lifecycle (rubric, one clock per lifecycle): the session-stamp lifecycle
  (`created_at`, `fenced_at`, `completed_at`, all gateway-stamped under `clock_source`) is judged
  on `ReaperContext::clock`. The drain's marks keep reading `Drain::clock`, their own lifecycle
  (GC's grace). In both deployed shapes the two are the same `LoopClock`
  (`crates/server/src/custodian.rs:618`/`:636`, `:477`), so behaviour there is unchanged.
* Fail closed with no reaper: a step carrying a `Window` and no reaper judges no tombstone, keeps
  every `Completed` record, emits `tombstone-window-unjudged`, and its retirement pass answers
  `Blocked`, never the `Satisfied` a `Retain` input could give (`retirement.rs:654-672`,
  `:1205`). Found in my own final review: this iteration's first cut only logged, so the step
  could still certify tombstones it had not judged. No production caller hits it: the only
  production `ReconstructionContext` (`crates/server/src/custodian.rs:611`) is always passed with
  `Some(&reaper)` (`:673-681`).
* Stamp-ahead on `Completed` (new, small): a local `Completed` session whose
  `completed_at_millis` is ahead of the reaper's clock by more than `δ_clock` is **held** and
  named (`crates/custodian/src/reaper.rs:1005-1014`), as 625.1 holds an ahead `created_at`. The
  saturating age reads such a tombstone as inside its window for as long as the stamp is ahead;
  without the hold that record and its slot would stay with no signal. Only when the policy's
  tombstone is a `Window` (a `Retain`/`Elapsed` input judges no stamp).
* Tests, both directions as asked:
  - `a_logical_context_expires_its_own_tombstones_and_holds_wall_ones_as_foreign`
    (`crates/custodian/src/reaper/tests.rs:2635`): logical context, default policy — the wall
    tombstone is held and kept byte for byte, the logical one deleted, count 2 → 1. Red under
    iteration 1's behaviour (negation C1 below).
  - `the_deployed_shape_judges_a_tombstone_on_its_reapers_clock_identity`
    (`crates/custodian/tests/gc.rs:1584`): the same through the reconstruction + reaper step the
    deployed loop runs, plus the no-reaper step keeping the store unchanged and answering
    `Blocked`.
  - `a_window_with_no_reaper_clock_is_unjudged_and_a_retain_input_reads_no_stamp`
    (`reaper/tests.rs:2677`): the no-reaper step emits the signal and answers `Blocked`; the same
    store under a step with its reaper is judged (deleted, no signal); and under a `Retain`
    policy an ahead `Completed` stamp is not held.

### 2. `W_tombstone` = 2 minutes (decided at sign-off)

`W_TOMBSTONE_MILLIS = 2 * MINUTE_MILLIS` (`reaper.rs:277`), doc updated to the sign-off's reason
(SDK Complete retries) and the throughput figure: 46 slots per 2 minutes, about 23 a minute (a
15-minute window: about 3). `W_TOMBSTONE < W_OPEN` still held (`reaper.rs:282`). The unit check
is `assert_eq!(W_TOMBSTONE_MILLIS, 120_000)` (`reaper/tests.rs:518`). `W_completing` stays 15
minutes (`reaper.rs:262`); nothing in the sign-off changed it.

### 3. The C4-ci failure: `custodian_gc.rs` fixture (narrow scope exception granted)

`deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`
published at `completed_at_millis: 3_000` against `LOOP_NOW = 10_000_000`, so the new window
(correctly) expired it. Changed **only** the tombstone's timestamps, so the completed upload is
inside the 2-minute window at every pass: `fenced_at_millis: LOOP_NOW - 2_000`,
`completed_at_millis: LOOP_NOW - 1_000` (`crates/server/tests/custodian_gc.rs:1466`, `:1473`),
plus the two comment phrases that said the input was `Retain` (`:53`, `:1413`). Every
partial-fleet assertion is unchanged.

Base-neutral, checked rather than argued: exported the base tree (`git archive 6e44de9`) to
scratch, applied only this file's diff, and ran the test there:
`test deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones ... ok`
(1 passed). With the patch it passes too (12/12 `deployed_*` in that file).

Not edited, though stale (outside the exception: "nothing else in `crates/server/*`"): the
comment at `crates/server/src/custodian.rs:625-627` still says "where `W_tombstone` will be
measured: until it is, every `Completed` multipart session keeps its record". See §6 below.

### 4. Living docs agree with 08

* `docs/design/architecture/06-runtime-view.md:92` — the tombstone sentence: kept for two minutes
  after publication, judged on the reaper's clock; a foreign one waits for an operator.
* `06-runtime-view.md:94` — "It judges only an **open** one" was false after this slice; now it
  judges open or completing ones, says what the two `Completing` exits do, that a foreign
  completing/completed upload is held, and the ahead-stamp hold; the one-shot's exit-status
  sentence names completing/completed too. (The review finding named this sentence; the
  exception's purpose is "so the living docs agree with 08". Flagging it here in case the human
  reads the exception as the tombstone sentence only.)
* `docs/design/architecture/05-building-block-view.md:246` — the "keep it until `W_tombstone` is
  measured" clause now says the deployed custodian judges `W_tombstone` on its reaper's clock.
* `docs/design/architecture/08-crosscutting-concepts.md:114` — 2 minutes / 23 a minute, the
  clock-identity rule, the ahead-stamp hold, and the measured `B_ops` figures (raw output below).
* In-crate docs: `crates/custodian/src/reconstruction.rs:150-156` (said "`Retain` until
  `W_tombstone` is measured"), the reaper module doc (`reaper.rs:135-143`, `:170-176`).

### 5. The `core/multipart.rs` transition API — justification (asked: justify or move)

Kept in core. `SessionRecord::completing_rollback` + `CompletingRollback`
(`crates/core/src/multipart.rs:2781-2803`, `:2936-2953`), ~40 lines with docs.

Why it cannot move to the custodian: the rollback's output is a `SessionRecord` at `Open@E+1`.
`SessionRecord`'s fields are private, and every constructor that changes state or epoch is
`pub(crate)` — the release a Complete takes of its own fence is `fence_released`
(`multipart.rs:2880`, `pub(crate)`), and the obligation builder `RetireObligation::attempt_segments`
is private (`multipart.rs:4678`). Core's rule is that it alone mints session records and
obligations (base `multipart.rs:4598-4602`, "a later writer's row is added as its own
constructor"). The only custodian-side route is a second codec spelling of the record:

```rust
// custodian-side alternative (rejected), ~25 lines:
let mut json: serde_json::Value = serde_json::from_slice(read)?;   // the stored mpu: bytes
json["epoch"] = (record.epoch() + 1).into();                        // overflow check by hand
json["state"] = serde_json::json!({ "kind": "Open" });              // fields core owns
let bytes = serde_json::to_vec(&json)?;
let session = decode_session_record(&bytes)?;                       // re-validate
let records = record.completing_teardown(upload)?.records().clone(); // pub, but mints the
                                                                    // teardown's session too
```

That puts the session's JSON layout in a second crate (the drift core's constructors exist to
prevent), needs `serde_json` in the custodian's deps, and couples the rollback to the teardown's
obligation by accident. The core route reuses the two existing private pieces in ~20 lines of
code and keeps one codec. The previous iteration's mutant `completing_rollback -> None` survived
because no core test covered it; added
`only_a_completing_session_rolls_back_and_its_obligation_matches_its_key`
(`multipart.rs:6514`), beside its twin `completing_teardown`'s test: `Open@E+1`, attempts
unchanged, the obligation's key/value round trip, equal to the teardown's records obligation for
the same attempt, and `None` at `u64::MAX` and in every other state.

The other core edits are the brief's: `SLOWEST_OP_MILLIS` made `pub` with its doc
(`multipart.rs:5968-5989`), and `MAX_BATCH_OPS`'s doc (`multipart.rs:6003-6008`), whose
"**Uncalibrated** … #625 must replace `SLOWEST_OP_MILLIS` with a measured figure" paragraph was
stale once the measurement existed. Values unchanged.

### 6. DST: a real crash, and a witnessed flip-first race

`crates/dst/tests/custodian.rs`, `reaper_rows`:

* `CrashAt` (`:8828-8915`): at its `at`-th commit the batch lands or not (drawn from the seed),
  then **every** `get`/`scan`/`scan_page`/`commit` fails — the process is gone, nothing after
  the crash point reaches the store. It snapshots the store after each commit that goes through,
  so a clean run records the boundary state for each crash point.
* `prop_crash_at_each_step_of_the_reapers_exits` (`:8997`), registered as
  `reaper_crash_at_each_step_of_the_terminal_exits` (`:9419`). Three exits: rollback, over-age
  edge, and **tombstone expiry** (new: its terminal delete is the commit that releases the slot).
  At every crash point it asserts the **boundary state**: the store equals the clean run's state
  after the commits before the crash (plus the crashing one if it landed) (`:9060-9063`); the
  `mpuctl` count equals the session records present; and the attempt's `seg:` records always
  have an owner (session still `Completing@1`, or the `retire:records:` obligation). Then clean
  steps after restart converge to the one end state each exit has; tombstone expiry also keeps a
  tombstone exactly `W_tombstone` old byte for byte.
* `prop_completing_rollback_race` (`:8712`), both interleavings, each with a `Witness`
  (`:8495`). Rollback-first: the session is `Open@2` when the flip is sent, and the flip comes
  back `Conflict`. Flip-first: `FlipGate` (`:8605`) records that the reaper's listing showed the
  session `Completing` while the flip was held, lands the flip only when the reaper sends its
  rollback fence (a commit putting `mpu:<id>` at `Open`), then the fence; the row asserts flip
  `Committed`, rollback `Conflict`. **No fallback**: if the fence never comes, the flip is never
  sent and the Complete gets an error (`FlipRace::commit`, `:8582-8597`).
* #659 child-1's DST rows cover the drain's own steps and its race with GC, not the reaper's
  transitions or the time-triggered terminal delete, so this row is kept (brief: "kept only if
  … does not already cover"). The reaper writes no `orphan:` mark itself, so no reaper-vs-GC row.

### 7. Boundary tests (exact tick and neighbour tick) and the "fresh Completing is kept" test

* `the_completing_arms_judge_at_their_boundaries` (`reaper/tests.rs:2497`): `W_session` on a
  `Completing` session at exactly the window (kept) and +1 ms (straight to `Aborting@2`, both
  obligations); `W_completing` exactly (kept) and +1 ms (`Open@2`, records obligation only,
  attempts unchanged); a session fenced 5 s ago with creation and a part 20 days old (kept: the
  window runs from the fence, F16, `0016:1027`); over-age **and** stale (ceiling wins, never
  rolled back first). Asserts the whole `Pass` tally, `fenced() == 3` and `Changed`.
* `the_tombstone_window_is_judged_at_its_boundary_and_its_stamp_guarded`
  (`reaper/tests.rs:2588`): exactly `W_tombstone` (kept), +1 ms (deleted, count −1), a foreign
  one (held, kept), a stamp exactly `δ_clock` ahead (kept, not held), `δ_clock + 1` ahead (held).
* `completing_session_fenced_moments_ago_is_kept_however_old_its_parts`
  (`crates/server/tests/multipart_reaper_exits.rs:734`): the same F16 case through the one-shot
  subprocess — the reviewer's mutation (`if true || …` at the `W_completing` check) now goes red
  here and in the unit test (negation K).

### 8. `G_orphan`: production does not read the bound — raised, not wired

`G_ORPHAN_MILLIS = 60_000` (`crates/custodian/src/gc.rs:308`) with its strict bounds held at
compile time (`:310-311`). The deployed loop hands GC `GC_GRACE_WINDOW_MILLIS =
crate::cli::LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`); making it read
`G_ORPHAN_MILLIS` is a server edit, which the sign-off said to raise as §6 rather than make. Done
below. Inside this slice: `G_ORPHAN_MILLIS`'s doc (`gc.rs:303-307`) and the boundary test's doc
(`crates/custodian/tests/gc.rs:1893-1898`) now say plainly that the deployed loop does not read
it yet; the test's earlier claim "GC then runs at the deployed grace" is gone.

### 9. Calibration fixes

* TiKV per-run identity: the namespace is `wyrd-conformance/<pid>/<tag>/<run_stamp>/`, the stamp
  taken once per invocation (`crates/metadata-tikv/tests/conformance.rs:73-81`), as FDB's prefix
  already had (`crates/metadata-fdb/tests/conformance.rs:201-207`).
* Ops actually sent: `calibrate_batch_ops` counts preconditions + puts + deletes on the batch as
  built, asserts it equals the requested `ops`, and records and prints that count
  (`crates/metadata-conformance/src/lib.rs:1648-1668`). The halving now lives inside
  `calibration_batch`, which takes `ops` (`lib.rs:1697-1706`), and the unit test calls it with the
  real count (`lib.rs:1810`), so C5's `/`→`*` mutant now fails it.
* The doc's description of batch (i) was wrong ("500 `require_absent` puts"); now "250 keys,
  each a `require_absent` and a put" (`multipart.rs:5973-5982`).

## Calibration — raw first-attempt output (live clusters on this host)

Both ran through the project's targets, `cargo xtask fdb-conformance` and
`cargo xtask tikv-conformance`, each wrapped in `timeout`. Single-node docker clusters
(`deploy/fdb-single-node`, image `foundationdb/foundationdb:7.3.77`; `deploy/tikv-single-node`,
`pingcap/pd:v8.5.1` + `pingcap/tikv:v8.5.1`). Host `saic`, AMD RYZEN AI MAX+ 395 (32 threads),
Linux 7.0.0-42-generic, 2026-11-22 (FDB started 21:05:39Z, TiKV 21:06:03Z). Both exited 0; the
whole FDB and TiKV conformance suites passed. **First attempt for both**: the TiKV runner's retry
message ("attempt N/5 failed") does not appear in its log, and the FDB runner made one run.
Note: libtest ran the calibration in parallel with the other tests of the same binary
(`trait_contract_against_*`), so the timings include that contention.

FoundationDB (`fdb-conformance.log` lines 104-124, verbatim):

```
$ cargo test -p wyrd-metadata-fdb --features fdb --test conformance
running 3 tests
B_ops calibration: OperationBound run 0: 500 operations sent, 8250 bytes, committed in 22 ms
B_ops calibration: OperationBound run 1: 500 operations sent, 8250 bytes, committed in 16 ms
test trait_contract_against_fdb ... ok
B_ops calibration: OperationBound run 2: 500 operations sent, 8250 bytes, committed in 14 ms
B_ops calibration: OperationBound run 3: 500 operations sent, 8250 bytes, committed in 14 ms
B_ops calibration: OperationBound run 4: 500 operations sent, 8250 bytes, committed in 16 ms
B_ops calibration: OperationBound: slowest of 5 runs 22 ms against the bound 2500 ms
test connect_probes_the_real_cluster_from_inside_a_runtime ... ok
B_ops calibration: ByteBound run 0: 500 operations sent, 5000000 bytes, committed in 35 ms
B_ops calibration: ByteBound run 1: 500 operations sent, 5000000 bytes, committed in 43 ms
B_ops calibration: ByteBound run 2: 500 operations sent, 5000000 bytes, committed in 41 ms
B_ops calibration: ByteBound run 3: 500 operations sent, 5000000 bytes, committed in 44 ms
B_ops calibration: ByteBound run 4: 500 operations sent, 5000000 bytes, committed in 41 ms
B_ops calibration: ByteBound: slowest of 5 runs 44 ms against the bound 2500 ms
test batch_ops_calibration_against_fdb ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
...
xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and the contention properties
```

TiKV (`tikv-conformance.log` lines 12, 100-116, verbatim):

```
$ cargo test -p wyrd-metadata-tikv --features tikv --test conformance
running 2 tests
test trait_contract_against_tikv ... ok
B_ops calibration: OperationBound run 0: 500 operations sent, 8250 bytes, committed in 252 ms
B_ops calibration: OperationBound run 1: 500 operations sent, 8250 bytes, committed in 230 ms
B_ops calibration: OperationBound run 2: 500 operations sent, 8250 bytes, committed in 220 ms
B_ops calibration: OperationBound run 3: 500 operations sent, 8250 bytes, committed in 222 ms
B_ops calibration: OperationBound run 4: 500 operations sent, 8250 bytes, committed in 236 ms
B_ops calibration: OperationBound: slowest of 5 runs 252 ms against the bound 2500 ms
B_ops calibration: ByteBound run 0: 500 operations sent, 5000000 bytes, committed in 322 ms
B_ops calibration: ByteBound run 1: 500 operations sent, 5000000 bytes, committed in 320 ms
B_ops calibration: ByteBound run 2: 500 operations sent, 5000000 bytes, committed in 329 ms
B_ops calibration: ByteBound run 3: 500 operations sent, 5000000 bytes, committed in 338 ms
B_ops calibration: ByteBound run 4: 500 operations sent, 5000000 bytes, committed in 373 ms
B_ops calibration: ByteBound: slowest of 5 runs 373 ms against the bound 2500 ms
test batch_ops_calibration_against_tikv ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s
```

Within the bound (worst 373 ms of 2,500 ms), so per the brief only the "assumed, not measured"
wording changed; no constant changed. These replace iteration 1's figures (23/38 ms FDB,
281/337 ms TiKV), which had no raw output behind them. One host, single-node clusters: a
multi-node deployment adds network round trips, which the doc says. The full logs are in
`$PDCA_SCRATCH/pdca-builder-1017-calibration/` until the harness reclaims scratch.

## Production reach (call path)

Deployed loop: `CustodianService::run_reconstruction_until` (`crates/server/src/custodian.rs:590`
builds `ReaperPolicy::default()`; `:628` puts `policy.tombstone()` in the `ReconstructionContext`;
`:634-639` the `ReaperContext`; `:673-681` passes both) → `reconcile_step` → `reap(...)`
(`reconciliation.rs:194`) → `reaper::reconcile` → `pass` → `walk` → `visit` (`reaper.rs:958`
clock guard, `:996` `judge_completing`, `:1005` the `Completed` ahead-stamp hold) → `fence`; then
`retirement::reconcile(ctx, sessions)` (`reconciliation.rs:193`) → `sessions::reconcile` →
`terminal` → `Tombstone::passed(session, ctx.sessions)` (`sessions.rs:925`). One-shot
(`wyrd custodian --reap-abandoned-uploads` → `reap_abandoned_uploads_once`,
`crates/server/src/custodian.rs:463-491`) → `reconcile_step` with the reaper only → the same
`reap(...)` → `retirement::reconcile_record_only(reaper)` (`reconciliation.rs:207`). No new entry
point; no server source edit.

## Red → green (the project's runner)

`engine/scripts/run-verify.sh` (C4-verify), `PDCA_LANE=1`,
`WYRD_VERIFY_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, on this bundle's `patch.diff`:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test multipart_reaper_exits (fix applied)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
run-verify.sh: RED — cargo test -p wyrd-server --test multipart_reaper_exits (production reverted, test kept)
rerunning_the_one_shot_over_a_partial_teardown_converges_with_no_double_decrement --- FAILED
the_reapers_teardown_waits_for_a_reclaiming_mark_and_resumes_once_gc_deletes_it --- FAILED
foreign_clocked_completing_and_completed_sessions_are_never_judged --- FAILED
completed_session_past_its_tombstone_window_is_deleted_and_one_inside_it_answers_a_retry --- FAILED
over_age_completing_session_is_fenced_straight_to_aborting_and_torn_down --- FAILED
stale_completing_session_rolls_back_to_open_and_drains_only_its_attempts_segments --- FAILED
test result: FAILED. 2 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
```

RED leg: **8 executed, 6 failed, 2 passed.** The two green on the base, both expected:
`profile_mismatch_refuses_the_completing_arms` (H-terminal — the brief says it is vacuous on the
base) and the new `completing_session_fenced_moments_ago_is_kept_however_old_its_parts` (the base
judges no `Completing` session, so it keeps this one too; the leg guards against an over-eager
rollback, negation K). E-terminal is red on the base, not vacuous: the base leaves the foreign
sessions' records alone but does not hold them, so the one-shot exits 0 and never names them.
Both F legs are red on the base because their fixtures reach teardown through this slice's
over-age `Completing` edge; #659's mechanism itself is evidenced by negations (8) and (9).

## Negations (each applied alone to the patched tree, its tests run, the file restored; the tree
was byte-identical to a pre-negation snapshot afterwards, checked with `cmp`)

| # | Flip | Result |
|---|---|---|
| (4) | `sessions.rs` session loop: owned walk only for `Aborting`; a `Completed` session goes to the terminal delete only if `sidx:` already reads empty | **RED** — G (`completed_session_past_its_tombstone_window…`) |
| (5) | `sessions.rs`: terminal delete after any walk step, whatever it observed | **RED** — G |
| (6′) | `reaper.rs:958`: clock guard skipped for `Completing` | **RED** — E-terminal |
| (8) | `marks.rs:307` (#659's arm 0): `reclaiming` falls through to the different-event arm | **RED** — F obligation-wait |
| (9) | `sessions.rs` `terminal`: skip the session-obligation check | **RED** — F idempotence and F obligation-wait |
| (15) | `reaper.rs:1107`: ceiling arm disabled for `Completing` | **RED** — D over-age, both F legs; unit `the_completing_arms_judge_at_their_boundaries` |
| C1 | `retirement.rs:432`: tombstone judged against a fixed `wall` (iteration 1's behaviour) | **RED** — `a_logical_context_expires_its_own_tombstones…` and `the_deployed_shape_judges_a_tombstone…` |
| B1 | `reaper.rs:1107` `>` → `>=` (`W_session`, `Completing`) | **RED** — `the_completing_arms_judge_at_their_boundaries` |
| B2 | `reaper.rs:1124` `>` → `>=` (`W_completing`) | **RED** — same |
| B3 | `retirement.rs:437` `>` → `>=` (`W_tombstone`) | **RED** — `the_tombstone_window_is_judged_at_its_boundary…` |
| K | `reaper.rs:1124` `if true \|\| …` (roll back on sight — the adversary's mutation) | **RED** — `completing_session_fenced_moments_ago_is_kept…` and the boundary unit test |
| S | `reaper.rs:1009`: ahead-stamp hold for `Completed` disabled | **RED** — `the_tombstone_window_is_judged_at_its_boundary…` |
| C2 | DST `CrashAt::alive` never fails (a crash is one transient error, iteration 1's model) | **RED** — `reaper_crash_at_each_step_of_the_terminal_exits` (the boundary-state check) |
| C3 | `reaper.rs` `judge_completing` returns at once (the base's behaviour) | **RED** — both new DST rows |
| C4 | DST `FlipGate` never intercepts the rollback | **RED** — `reaper_completing_rollback_race_both_interleavings`, at the flip-first witness (`custodian.rs:8789`, "the flip landed in the gap"): no fallback publishes it |
| U1 | `retirement.rs:668`: a no-reaper `Window` step certifies as usual (drop the `Blocked`) | **RED** — `a_window_with_no_reaper_clock_is_unjudged…` and `the_deployed_shape_judges_a_tombstone…` |
| U2 | `retirement.rs:660`: the `tombstone-window-unjudged` signal not emitted | **RED** — `a_window_with_no_reaper_clock_is_unjudged…` |

F's obligation-wait fixture is green with arm (0) intact, so #659 child-1's arm (0) is present and
correct on this base; F is regression coverage of #659's mechanism through the reaper's path.

## Refuting my own test

* **(a) Genuine red?** Yes. `run-verify.sh` reverted the production change and kept the test: 8
  executed, 6 failed (the 2 that pass are named above with why). Every negation above goes red
  on the leg it targets; C1, B1-B3, K, S, C2, C3, C4, U1 and U2 are new this iteration and cover
  each carry-forward item and the new fail-closed branch.
* **(b) Production path?** Yes. Every binding leg spawns the built binary
  (`env!("CARGO_BIN_EXE_wyrd")`) with `custodian --reap-abandoned-uploads` over a real redb store
  — the production one-shot, `reconcile_step`, the reaper and the real retirement leg. The unit
  and `tests/gc.rs` tests call the production `reconcile_step`; the DST rows call the production
  `create_session`, `complete_upload` and `reconcile_step`.
* **(c) Fixture includes the fault?** Yes. D seeds a stuck `Completing` session with real `seg:`
  records and a later epoch's group; the kept leg seeds old parts beside a fresh fence (the case
  a rollback-on-sight reaper fails); E-terminal seeds foreign sessions past every window; G seeds
  a `Completed` session with crashed-part residue; F seeds a real `reclaiming` mark; the clock
  tests seed a wall tombstone under a logical context; the DST crash row crashes at every commit
  a clean step makes, landed and not, with all later I/O failing; the flip-first race only passes
  when the flip actually lands between the reaper's read and its fence.

## Gates run here

* `cargo fmt --all -- --check`: clean (after `cargo fmt --all`).
* `cargo clippy --workspace --exclude wyrd-dst --all-targets`: clean.
  `RUSTFLAGS='--cfg madsim' cargo clippy -p wyrd-dst --all-targets`: clean.
* DST: `reaper_*` rows green at `MADSIM_TEST_NUM=8`.
* `cargo xtask ci` on the final tree (started 2026-11-22T21:16:23Z): **`xtask ci: all checks
  passed`, exit 0** — typos, docs lint and render, the gitlink/unsafe/blackbox guards, fmt,
  workspace clippy, build, `cargo test --workspace --exclude wyrd-dst` (no failed result line),
  cargo-machete, `cargo deny` (incl. all-features advisories), statics, deploy-guard, DST clippy
  and the DST suite at the xtask's seed count. The deployed-retirement test that failed C4-ci in
  iteration 1 passes. (An earlier full run on the tree before the `Blocked` change also passed.)
* `run-verify.sh` re-run on the final `patch.diff`: same verdict as above (GREEN 8/8; RED 8 ran,
  6 failed, the same 2 passing; `PASS`).
* The final `patch.diff` applies cleanly to a fresh `git archive 6e44de9` export
  (`git apply --check`).

## Review findings (T4 batch) — disposition

Fixed: the three clock-binding findings (§1), the three TiKV-namespace findings (§9), the
deployed-retirement fixture (§3), the crash injector ×3 and flip-first fallback ×2 (§6), and the
two docs-currency findings (§4). Recorded as rejected, with reasons, in `review-rejected.md` (this
bundle): the `G_orphan` separate-constant findings ×3 (server wiring is §6 below, per the
sign-off), the calibration-shape findings ×3 (the brief fixes the two shapes), and the two
`sessions.rs` drainer-race halves (X56, #660's; their crash and boundary halves are fixed). The
human may overrule any of those.

## NEEDS-HUMAN

NEEDS-HUMAN: `G_orphan` wiring — production does not read the bound the boundary test checks. The deployed GC grace is `GC_GRACE_WINDOW_MILLIS = crate::cli::LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`, child-2's file); `wyrd_custodian::gc::G_ORPHAN_MILLIS` (`crates/custodian/src/gc.rs:308`) has the same value (60 s) and the strict 0016 bounds, but nothing in the server reads it, so a change to `LEASE_TTL_MILLIS` would move the deployed reclaim tick with every test green. The sign-off said to raise this rather than edit `crates/server/src/*`. The fix is two lines in child-2's files (or #1016 takes them):

```diff
--- a/crates/server/src/custodian.rs
+++ b/crates/server/src/custodian.rs
@@ (line 115)
-const GC_GRACE_WINDOW_MILLIS: u64 = crate::cli::LEASE_TTL_MILLIS;
+const GC_GRACE_WINDOW_MILLIS: u64 = wyrd_custodian::gc::G_ORPHAN_MILLIS;
--- a/crates/server/tests/custodian_gc.rs
+++ b/crates/server/tests/custodian_gc.rs
@@ (line 429)
-const GRACE_MILLIS: u64 = 60_000;
+const GRACE_MILLIS: u64 = wyrd_custodian::gc::G_ORPHAN_MILLIS;
```

The second line makes the existing deployed-loop boundary tests (`deployed_role_reclaims_at_the_exact_grace_boundary` and its ±1 ms neighbours) pin the deployed tick to `G_ORPHAN_MILLIS`. `cli.rs:101` (`RESTORE_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`) could take the same constant. If merged before this slice, `G_ORPHAN_MILLIS`'s "not yet read" doc paragraph (`gc.rs:303-307`) should be dropped.

Other items for sign-off (not blocking):
* Stale comment in child-2's file, not editable here: `crates/server/src/custodian.rs:625-627`
  ("where `W_tombstone` will be measured: until it is, every `Completed` multipart session keeps
  its record"). Suggested text: "Routed through the reaper's policy: `W_tombstone`, judged on the
  reaper context below." Expect it in the #1016 merge.
* `06-runtime-view.md:94` was edited beyond the tombstone sentence (§4); confirm that is inside
  the granted exception.
* `W_completing` = 15 minutes is still a judgment value (0016 gives no number); the sign-off only
  re-decided `W_tombstone`.
* #633 should be re-pointed to this slice and to child-2 (brief): foreign-clocked `Completing` and
  `Completed` sessions, and `Completed` ones stamped ahead of the clock, are held and wait for
  #633's verbs.
* Expect a small merge conflict with #1016 in `custodian_gc.rs` and `06-runtime-view.md`, as the
  sign-off said.
