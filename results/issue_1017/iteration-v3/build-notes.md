# Build notes — issue 1017 (625.3: the reaper's terminal exits) — iteration 3

Base: `6e44de9 pdca-integrate: issue_1015` (the harness worktree, getwyrd/wyrd @ main lineage;
`stack-base` = `pdca-integration/main-r9dd123c9ed70`). 625.1 is on it (`427e5ec custodian: reap
multipart uploads whose client went silent`, folded by `6181de4`/`6e44de9`), and so is #659's
drain: `20e6e21` (records drain, #1061), `3a392c8` (bytes drain + mark guard incl. arm (0), #1062),
`f54db83` (session leg + terminal delete, #1013). Arm (0) is
`crates/custodian/src/retirement/marks.rs:307`.

This iteration starts from iteration 2's patch (applied unchanged to `6e44de9`, then fixed in
place). Line numbers are on the patched tree unless marked "base".

## What the carry-forward asked, and what changed

### 1. REAL DEFECT — the tombstone retry window: the stamp is now taken at publication

**Cause.** `W_tombstone` is measured from the `Completed` record's `completed_at_millis`
(`crates/custodian/src/retirement.rs:425-441`, `Tombstone::passed`). That value was `PublishRequest::completed_at_millis`,
a number the caller read *before* calling `complete_upload` (base
`crates/core/src/multipart/complete.rs:1474`), so the fence, the part reads and the segment phase
all ran inside the window, and a Complete slower than two minutes published a tombstone that the
next reaper pass could delete (the reviewer's probe: `AlreadyCompleted`, then `NoSuchUpload` with
zero time elapsed since publication).

**Fix (core, because the stamp is minted there).**

* `crates/core/src/multipart/complete.rs:1493-1502` — new `PublicationClock` seam (one method,
  `now_millis`; any `Fn() -> u64` is one, so callers pass a closure over their clock). Its doc
  names the source that owns the lifecycle: the session's clock, the one `clock_source` names and
  `fenced_at_millis` was read on — the clock the reaper judges `W_tombstone` on (rubric: "a new
  clock read states which source owns its lifecycle").
* `complete.rs:1478-1485` — `PublishRequest` loses `completed_at_millis` (it keeps `fresh_inode`),
  so no caller can hand in a stale stamp. `complete_upload` takes `clock: &impl PublicationClock`
  (`complete.rs:1545-1558`).
* `complete.rs:1617-1711` (the round loop, up to the existing lost-round judgment): each round reads the clock twice. `checked_at`
  (`:1622`) stamps the flip the committer checks before the round's first commit (every refusal
  still precedes any write). `at` (`:1655`) is read **after the segment phase, just before the flip
  is sent** — the flip built on it is what goes out. Each round restamps, so a retried flip carries
  a later stamp. The only thing between `at` and the flip landing is the flip's own commit (inside
  the 5 s envelope, `crates/core/src/multipart.rs:5939` `E_TX_MILLIS`).
* `crates/core/src/staged_publication.rs:450-541` — the committer is split at the point it already
  had, with **identical I/O**: `publish` = `prepare` + `segment_phase` + flip (unchanged
  behaviour); `publish_segments` (`:469`) = `prepare` + `segment_phase`, answering
  `Segmented::{Lost, Ready(ReadyFlip)}`; `ReadyFlip::flip(store, meta, contribution)` (`:506`)
  rebuilds the flip from the caller's new metadata/contribution and checks it again exactly as step
  1 checks the original (`flip_of`, `:628`: fence rules, root record, both envelopes) before
  committing. `Prepared` keeps the `FlipInputs` (`:568`) that re-check needs.
* `complete.rs:1767-1786` — `stop_round`: the existing "refused, or refused for good" branch
  factored out, so the late flip build (whose only failures are stamp-independent and therefore
  already passed on `checked_at`) ends the same way.
* `complete.rs:1692` — an unknown outcome now records the flip in `unknown_flips` only if a flip
  was actually sent; a segment batch's unknown outcome records none (an unsent flip's bytes can
  never be found, so this changes no answer). `DoubtfulCommit` classification is unchanged.

**Why this shape (alternatives, with their costs).**

* *Read the stamp once per round, at the round's start.* One line, but the segment phase (up to
  `MAX_ROOT_SEGMENTS` = 512 segments, about 11 batches of up to 5 s each) still runs inside the
  window. Measured: negation P2 below turns the core test red on its segmented leg (stamp 62,500
  instead of 182,500).
* *Two committer calls per segmented round* (segments, then `publish` with `resume_from = count`).
  Needs no new committer API, but `publish` then re-reads the whole group range twice before the
  flip: one or two extra paged scans of up to 512 records per segmented round, and it changes the
  I/O the #1009/#1010 suite pins (e.g. `take_scans()` "two walks of two pages",
  `crates/core/tests/multipart_complete.rs:2310`). Rejected.
* *Slack in the reaper* (expire at `age > W_tombstone + E_tx`). Changes the judged boundary and
  leaves the cause: the stamp would still be read before the work. Rejected.
* Chosen: the split above. Every existing Complete and committer test passes unchanged (85 in
  `multipart_complete`, 10 in `multipart_complete_value_ceiling`, 21 in `segmented_publication`,
  run before the new tests were added), which is the evidence the I/O did not move.

**Scope.** The brief gives this slice `crates/core/src/multipart.rs` for constants and doc only;
the sign-off's "REAL DEFECT … Take the stamp at publication through the same clock seam" cannot be
done without `complete.rs` and the committer, so they changed. No production code calls
`complete_upload` yet (`complete.rs:312`, "the S3 verb that will is #670"), so the API change
reaches only tests, each changed mechanically to pass a clock with the value it used before:
`crates/core/tests/multipart_complete.rs` (the rig gains a `session_clock`, still `COMPLETED_AT`
unless a leg moves it), `crates/core/tests/multipart_complete_value_ceiling.rs:374-385`,
`crates/custodian/tests/multipart_retirement_drain.rs:708-712`,
`crates/custodian/tests/multipart_retirement_marks.rs:732-735`,
`crates/custodian/tests/multipart_session_terminal.rs` (`complete`),
`crates/dst/tests/custodian.rs:8239-8242`, and `crates/server/tests/custodian_gc.rs:1471-1473`.
That last one is child-2's file: the edit is the deployed-retirement test's tombstone stamp only
(same value, `LOOP_NOW - 1_000`, now passed as `|| LOOP_NOW - 1_000`), inside last round's
exception. It is no longer base-*compilable* (the base has the old signature), which no edit to
that call could avoid once the API changed; every assertion and the value are as before.

**Regressions (each red with the fix reverted, see Negations).**

* `crates/core/tests/multipart_complete.rs:7195`
  `the_publication_stamp_is_read_at_the_flip_behind_every_segment_batch` — a scripted wrapper moves
  the session clock a minute before the fence and before each of the deep fixture's segment
  batches (`Elapse`, `:7146`); the answer's publication, the `Completed` record's
  `completed_at_millis` and the root's `modified` all carry `COMPLETED_AT` + every minute. Three
  legs: flat, flat with a lost first flip (second round restamped), and segmented.
* `crates/core/tests/segmented_publication.rs:1217` (leg G) — `publish_segments` then
  `ReadyFlip::flip`: the same commits as `publish`, the root carries the stamp handed in after the
  segment phase, and a rebuilt flip that does not move the fence is refused (`FenceNotMoved`) with
  no flip sent and the fence where the phase left it.
* `crates/custodian/tests/multipart_session_terminal.rs:1389`
  `d_a_slow_completes_tombstone_lasts_the_whole_window_after_its_publication` — **the delayed-Complete
  regression the sign-off asked for**, end to end: production Create, planted part, production
  Complete through a wrapper whose first read of the `part:` record moves the one clock by
  `W_tombstone + 1 ms` (`stall`, `:358`), then deployed passes **with the reaper** beside them on
  that clock (`Rig::reap`, `:1025`: `ReconstructionContext` with `policy.tombstone()` and a
  `ReaperContext`, as `run_reconstruction_until` builds them). Straight after publication: the
  obligation drains, the record and its slot stay, an identical retry answers `AlreadyCompleted`;
  exactly `W_tombstone` later: still kept, retry `AlreadyCompleted`; one millisecond more: deleted,
  count −1, retry `NoSuchUpload`. The full two-minute window survives publication, measured.

### 2. The `W_tombstone` doc comment (`crates/custodian/src/reaper.rs:264-293`)

* New paragraph **"When it starts: at the publication"** (`:269-276`): read on the session's clock
  after the segment phase, just before the flip; only the flip's commit lies between.
* Throughput corrected: the record goes at the first fleet-loop pass after the window
  (`--interval-secs`, 30 s default, the cadence stated at `reaper.rs:377-381`), and only once the
  drain has finished; so a slot is held 2 to 2.5 minutes after publication, 46 slots sustain at
  most about 23 completions a minute, about 18 when each delete waits a whole interval, and fewer
  for real uploads, whose own life before Complete holds the slot too. Value unchanged
  (`2 * MINUTE_MILLIS`, `:293`); `W_TOMBSTONE < W_OPEN` still asserted.
* Same facts in `docs/design/architecture/08-crosscutting-concepts.md:114`,
  `06-runtime-view.md:92` (tombstone sentence) and `05-building-block-view.md:254` (the Complete
  verb's paragraph: when the publication time is read — docs currency for the API change), and the
  `Tombstone::Window` doc (`crates/custodian/src/retirement.rs:405-406`).

### 3. E-terminal server leg (`crates/server/tests/multipart_reaper_exits.rs:838-850`)

The leg now requires, per session, **one stderr line** carrying `reaper-session-held`, the
`"foreign-clock"` hold reason (quoted, as the fmt layer prints the field) and `mpu:<id>` together.
The `reaper-foreign-clock` notice names every foreign session whether held or not, so it no longer
counts. Negation E1 (the adversary's: the foreign `Completed` arm returns before holding) now turns
this leg red; in iteration 2 it stayed 8/8 green.

### 4. Server edits under the widened exception (`crates/server/src/custodian.rs`)

* (a) `:120` `GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`; the strict late-write
  check is kept (`:132`, `LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS`). The doc above it
  (`:88-119`, new paragraph at `:99-102`) now says it is `G_orphan`. One more line, `:123`,
  `assert!(GC_GRACE_WINDOW_MILLIS >= crate::cli::LEASE_TTL_MILLIS)`: the existing doc's claim that
  "the deployed floor is no shorter than the grace the write path already relies on" was true by
  construction when the value *was* `LEASE_TTL_MILLIS`; after the rewiring it would be an unchecked
  claim. I read it as part of (a); if the human reads "exactly two things" strictly, it is one line
  to drop.
* (b) `:633-638` the stale "until it is [measured], every `Completed` multipart session keeps its
  record" comment now says the window is judged on the reaper context's clock and `clock_source`,
  and what keeps a foreign one.
* `crates/custodian/src/gc.rs:294-307` — `G_ORPHAN_MILLIS`'s doc says production now reads it; the
  "not yet read" paragraph is gone. `crates/custodian/tests/gc.rs:1895-1898` — the boundary test's
  doc says the tick it pins is the deployed one.
* Not edited (outside every exception): `crates/server/tests/custodian_gc.rs`'s own
  `GRACE_MILLIS = 60_000` literal. It now equals the deployed value through the wiring; if
  `G_ORPHAN_MILLIS` ever changes, those deployed-loop boundary tests go red, which is the drift
  detection we want. `crates/server/src/cli.rs` `RESTORE_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS` is
  left as is.

### 5. The 12 batch-review findings — disposition

Fixed: the two `retirement.rs:431` / stamp findings (§1), the four `G_orphan` separate-constant
findings at `gc.rs:306` and `tests/gc.rs:1995` (§4), and the two `multipart_reaper_exits.rs:839`
findings (§3). Recorded as rejected in `review-rejected.md`, each with the finding's own phrase as
MATCH (last round's entries did not match the new wording, which is why the gate counted 0
recorded-rejected): `retirement.rs:617` (mismatched policies — not reachable from the deployed
loop; see the sketch below), the three calibration-shape findings (the brief fixes the two
shapes), the redb calibration finding (the brief scopes it to the networked backends; a redb leg
would put a wall-time assertion into every `cargo xtask ci`), and the TiKV-retry finding (the
brief prescribes "record the first attempt's figures"). Iteration-2 entries still valid were kept;
the three `G_orphan` entries were removed, since that is now fixed.

`retirement.rs:617` — the guard I did not add, and what it would cost. In
`retirement::reconcile` (`crates/custodian/src/retirement.rs:611-624`), take the
`ReaperContext` instead of a `SessionClock`, and when
`reaper.policy.tombstone() != ctx.tombstone` judge no tombstone (`Retain`), emit a
`tombstone-inputs-disagree` signal and answer `Blocked`, as the no-reaper case already does
(`run`, `:655-673`). About 15 lines in `retirement.rs`, 2 in `reconciliation.rs:189-193`, plus a unit
test of about 30. Not done because the deployed loop passes one policy to both
(`crates/server/src/custodian.rs:638`, `:648`), both Check reviewers of iteration 2 found no
reachable defect, and composition is #1016's. The human may want it anyway.

## Calibration — raw first-attempt output (carried from iteration 2; code unchanged)

The calibration code did not change in this iteration (`git diff` of `crates/metadata-conformance`,
`crates/metadata-fdb`, `crates/metadata-tikv` and `crates/core/src/multipart.rs` against
iteration 2's patch is empty), so I did not re-run it: no FoundationDB or TiKV cluster is running on
this host now (`docker ps` shows none). The figures in `multipart.rs:5968-5989` and `08` are the
ones below, from iteration 2's live run, and the raw logs are still in
`$PDCA_SCRATCH/pdca-builder-1017-calibration/` (`fdb-conformance.log`, `tikv-conformance.log`) until
the harness reclaims scratch.

Ran through `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance`, each wrapped in
`timeout`. Single-node docker clusters (`deploy/fdb-single-node`, image
`foundationdb/foundationdb:7.3.77`; `deploy/tikv-single-node`, `pingcap/pd:v8.5.1` +
`pingcap/tikv:v8.5.1`). Host `saic`, AMD RYZEN AI MAX+ 395 (32 threads), Linux 7.0.0-42-generic,
2026-11-22 (FDB started 21:05:39Z, TiKV 21:06:03Z). Both exited 0. **First attempt for both**: the
TiKV runner's retry message ("attempt N/5 failed") does not appear in its log, and the FDB runner
made one run. libtest ran the calibration in parallel with the other tests of the same binary, so
the timings include that contention.

FoundationDB (`fdb-conformance.log` lines 102-124, verbatim):

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
```

TiKV (`tikv-conformance.log` lines 100-116, verbatim):

```
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
wording changed; no constant changed. One host, single-node clusters: a multi-node deployment adds
network round trips, which the doc says. The Check reviewer of iteration 2 could not reproduce
these (its FDB cluster failed its health probe, and it had no TiKV endpoint); the human may want
Eduard Ralph to re-run both before merge, as the brief allows, and record the figures in SUMMARY §9.

## Production reach (call path)

Deployed loop: `CustodianService::run_reconstruction_until` (`crates/server/src/custodian.rs`
builds `ReaperPolicy::default()`, `:598`; puts `policy.tombstone()` in the `ReconstructionContext`,
`:638`; the `ReaperContext`, `:644-649`; passes both) → `reconcile_step` → `reap(...)`
(`crates/custodian/src/reconciliation.rs:194`) → `reaper::reconcile` → `pass` → `walk` → `visit`
(clock guard, `judge_completing`, the `Completed` ahead-stamp hold) → `fence`; then
`retirement::reconcile(ctx, sessions)` (`reconciliation.rs:189-193`) → `sessions::reconcile` →
`terminal` → `Tombstone::passed(session, ctx.sessions)` (`crates/custodian/src/retirement/sessions.rs:925`).
GC on the same loop now gets `G_ORPHAN_MILLIS` as its grace (`custodian.rs:120`). One-shot
(`wyrd custodian --reap-abandoned-uploads` → `reap_abandoned_uploads_once`) → `reconcile_step`
with the reaper only → the same `reap(...)` → `retirement::reconcile_record_only(reaper)`
(`reconciliation.rs:207`). No new entry point. The Complete-side stamp has no production caller
yet (#670); its call path is `complete_upload` → `publish_held` → `publish_segments` →
`ReadyFlip::flip`.

## Red → green (the project's runner)

`engine/scripts/run-verify.sh` (C4-verify), `PDCA_LANE=1`,
`WYRD_VERIFY_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, on this bundle's final `patch.diff`
(which applies cleanly to a fresh `git archive 6e44de9` export, `git apply --check`):

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test multipart_reaper_exits (fix applied)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
run-verify.sh: RED — cargo test -p wyrd-server --test multipart_reaper_exits (production reverted, test kept)
rerunning_the_one_shot_over_a_partial_teardown_converges_with_no_double_decrement --- FAILED
the_reapers_teardown_waits_for_a_reclaiming_mark_and_resumes_once_gc_deletes_it --- FAILED
foreign_clocked_completing_and_completed_sessions_are_never_judged --- FAILED
over_age_completing_session_is_fenced_straight_to_aborting_and_torn_down --- FAILED
completed_session_past_its_tombstone_window_is_deleted_and_one_inside_it_answers_a_retry --- FAILED
stale_completing_session_rolls_back_to_open_and_drains_only_its_attempts_segments --- FAILED
test result: FAILED. 2 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
```

RED leg: **8 executed, 6 failed, 2 passed** — the "8 test(s) ran red" in the runner's last line
counts tests executed, not failed. The two green on the base, both expected:
`profile_mismatch_refuses_the_completing_arms` (H-terminal — the brief says it is vacuous on the
base) and `completing_session_fenced_moments_ago_is_kept_however_old_its_parts` (the base judges no
`Completing` session, so it keeps this one too; it guards against an over-eager rollback, negation
K). E-terminal is red on the base, not vacuous: the base never holds the foreign sessions, so no
line carries the hold (`multipart_reaper_exits.rs:842`). Both F legs are red on the base because
their fixtures reach teardown through this slice's over-age `Completing` edge; #659's mechanism
itself is evidenced by negations (8) and (9).

The new regressions of §1 live outside the brief's named file (the named file seeds records and
calls no Complete, so it compiles on the base, which the runner needs). Their red is shown by the
negations P1, P2 and G1 below, with the rest of the fix in place.

## Negations

Each applied alone to the final patched tree, its tests run, the file restored. P1, P2, G1 and E1
by hand (`cp` of the file to scratch, edit, run, copy back); the rest with iteration 2's script
(`$PDCA_SCRATCH/pdca-builder-1017-negate.py`, log `pdca-builder-1017-negations-3.log`). Afterwards
`git diff` in the worktree was empty against the staged patch and `git diff --cached 6e44de9`
was byte-identical to `patch.diff` (`cmp`).

New this iteration:

| # | Flip | Result |
|---|---|---|
| P1 | `complete.rs` `complete_upload`: read the clock once at the request and stamp every flip with that reading (the pre-fix behaviour) | **RED** — `the_publication_stamp_is_read_at_the_flip_behind_every_segment_batch` (`multipart_complete.rs:7172`, the answer's stamp) and `d_a_slow_completes_tombstone_lasts_the_whole_window_after_its_publication` (`multipart_session_terminal.rs:1413`, the stamp; the record is then deleted straight after publication) |
| P2 | `complete.rs:1655`: `let at = checked_at;` (stamp read at the round's start, before the segment phase) | **RED** — the core stamp test's segmented leg: `left: 62500, right: 182500` |
| G1 | `staged_publication.rs` `ReadyFlip::flip`: send the flip checked at step 1, ignoring the rebuilt one | **RED** — leg G (`segmented_publication.rs`): `modified` `Some(1)` instead of `Some(2)` |
| E1 | `reaper.rs`, foreign-clock branch: `SessionState::Completed { .. } => return Ok(())` (the adversary's mutation) | **RED** — `foreign_clocked_completing_and_completed_sessions_are_never_judged` (`multipart_reaper_exits.rs:842`); iteration 2's leg stayed green under it |

The brief's negations and iteration 2's, re-run on this tree:

| # | Flip | Result |
|---|---|---|
| (4) | `sessions.rs` session loop: owned walk only for `Aborting`; a `Completed` session goes to the terminal delete only if `sidx:` already reads empty | **RED** — G (`completed_session_past_its_tombstone_window…`) |
| (5) | `sessions.rs`: terminal delete after any walk step, whatever it observed | **RED** — G |
| (6′) | `reaper.rs`: clock guard skipped for `Completing` | **RED** — E-terminal |
| (8) | `marks.rs:307` (#659's arm 0): `reclaiming` falls through to the different-event arm | **RED** — F obligation-wait |
| (9) | `sessions.rs` `terminal`: skip the session-obligation check | **RED** — F idempotence and F obligation-wait |
| (15) | `reaper.rs`: ceiling arm disabled for `Completing` | **RED** — D over-age, both F legs; unit `the_completing_arms_judge_at_their_boundaries` |
| C1 | `retirement.rs`: tombstone judged against a fixed `wall` (iteration 1's behaviour) | **RED** — `a_logical_context_expires_its_own_tombstones…` and `the_deployed_shape_judges_a_tombstone…` |
| B1 | `reaper.rs` `>` → `>=` (`W_session`, `Completing`) | **RED** — `the_completing_arms_judge_at_their_boundaries` |
| B2 | `reaper.rs` `>` → `>=` (`W_completing`) | **RED** — same |
| B3 | `retirement.rs` `>` → `>=` (`W_tombstone`) | **RED** — `the_tombstone_window_is_judged_at_its_boundary…` |
| K | `reaper.rs` `if true \|\| …` at the `W_completing` check (roll back on sight) | **RED** — `completing_session_fenced_moments_ago_is_kept…` and the boundary unit test |
| S | `reaper.rs`: ahead-stamp hold for `Completed` disabled | **RED** — `the_tombstone_window_is_judged_at_its_boundary…` |
| U1 | `retirement.rs`: a no-reaper `Window` step certifies as usual (drop the `Blocked`) | **RED** — `a_window_with_no_reaper_clock_is_unjudged…` and `the_deployed_shape_judges_a_tombstone…` |
| U2 | `retirement.rs`: the `tombstone-window-unjudged` signal not emitted | **RED** — `a_window_with_no_reaper_clock_is_unjudged…` |

Not re-run (carried from iteration 2, where each was red): the DST negations C2 (a crash is one
transient error), C3 (`judge_completing` returns at once) and C4 (`FlipGate` never intercepts the
rollback). The DST rows and the code they target are unchanged this iteration except the DST
Complete helper's one call (`crates/dst/tests/custodian.rs:8239-8242`, the clock argument, same
value), and the whole DST suite passed in this iteration's `cargo xtask ci`.

F's obligation-wait fixture is green with arm (0) intact, so #659 child-1's arm (0) is present and
correct on this base; F is regression coverage of #659's mechanism through the reaper's path.

## Refuting my own test

* **(a) Genuine red?** Yes. `run-verify.sh` reverted every production change and kept the added
  test: 8 executed, 6 failed (the 2 that pass on the base are named above, with why). Each new regression was also run against its own negation with the
  fix otherwise intact: the core stamp test and the slow-Complete test go red with the stamp read
  at the request (P1, the pre-fix behaviour); the core test goes red with it read at the round's
  start (P2); leg G goes red when `ReadyFlip::flip` sends the checked flip instead of the rebuilt
  one (G1); E-terminal goes red when the foreign `Completed` session is not held (E1).
* **(b) Production path?** Yes. The binding legs spawn the built binary
  (`env!("CARGO_BIN_EXE_wyrd")`) with `custodian --reap-abandoned-uploads` over a real redb store.
  The slow-Complete regression runs the production `create_session`, `complete_upload` and
  `reconcile_step` (reconstruction + reaper + retirement, the deployed shape) over
  `RedbMetadataStore`; the core tests drive the production `complete_upload` and committer over
  redb through the suites' recording wrappers. No stand-in replaces any production function.
* **(c) Fixture includes the fault?** Yes. The slow-Complete fixture actually spends
  `W_tombstone + 1 ms` of the one clock between the fence and the flip (asserted: `published_at ==
  requested_at + W_tombstone + 1`), which is the reviewer's counterexample; the core test spends
  clock time before the fence and before every segment batch; leg G hands in a flip that breaks a
  fence rule; E-terminal keeps the foreign `Completed` session past every window.

## Gates run here

* `cargo xtask ci` on the tree (started 2026-11-23T02:12:32Z): **`xtask ci: all checks passed`,
  exit 0** — typos, docs lint and render, the guards, fmt, workspace clippy, build,
  `cargo test --workspace --exclude wyrd-dst` (the deployed-retirement test in `custodian_gc.rs`
  included), cargo-machete, `cargo deny` (incl. all-features advisories), statics, deploy-guard,
  DST clippy and the DST suite. After it, only doc comments changed, in two test files (the
  headers and one stale comment of `crates/core/tests/multipart_complete.rs` and
  `crates/custodian/tests/multipart_session_terminal.rs`); re-checked with
  `cargo fmt --all -- --check` (clean), `typos` over every changed file (clean),
  `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` (clean) and both suites (86 and 44
  passed). Then three more doc-only edits in production files, from my own final review: the
  `PublicationError` doc (`crates/core/src/staged_publication.rs:199-203`) no longer claims every
  input-decided refusal precedes any I/O (`ReadyFlip::flip` decides the flip's again after the
  segment phase), and two links from public docs to private items that earlier iterations of this
  patch added (`reaper.rs:116` `judge_completing`, `retirement.rs:412` `SessionClock`) are plain
  code spans now. Re-checked with fmt and clippy; `run-verify.sh` re-run on the regenerated
  `patch.diff`: same verdict (GREEN 8/8; RED 8 ran, 6 failed; PASS).
* rustdoc is not part of `cargo xtask ci`. `cargo doc --no-deps -p wyrd-custodian` fails on the
  base already (`private_intra_doc_links` under the workspace's `warnings = "deny"`, e.g.
  `backfill.rs:90`, `gc.rs:23`); of the 137 diagnostics `cargo doc` prints for core, custodian and
  server on the final tree, none is on a line this patch adds (checked against `git diff -U0 6e44de9`
  after the two fixes above; before them, those two were the only ones).
* Before the full run: `RUSTFLAGS='--cfg madsim' cargo clippy -p wyrd-dst --all-targets` clean;
  `cargo test -p wyrd-core -p wyrd-custodian` all green; `cargo test -p wyrd-server --test
  custodian_gc --test multipart_reaper --test multipart_reaper_exits` 12 + 13 + 8 passed.
* `run-verify.sh` on the final `patch.diff`: PASS (above).
* No PR pushed or opened.

## NEEDS-HUMAN

No `NEEDS-HUMAN external dependency` item: the binding criterion ran in-process over redb. Items for
sign-off (not blocking):

* **Core API change beyond the brief's core scope.** `complete_upload` gains a clock parameter and
  `PublishRequest` loses `completed_at_millis`; the committer gains `publish_segments` /
  `ReadyFlip`. Asked for by the sign-off's REAL DEFECT item; no production caller exists yet
  (#670 will be the first, and must pass the gateway's session clock).
* **`crates/server/tests/custodian_gc.rs`**: its one Complete call changed with the API (§1). The
  value and every assertion are unchanged; it no longer compiles against the base, unavoidably.
* **The extra compile-time assert** in `crates/server/src/custodian.rs:123` (§4): keep or drop.
* **`retirement.rs:617`** (mismatched tombstone inputs): rejected with a costed sketch (§5); the
  human may want the guard.
* **Calibration**: not re-run this iteration (code unchanged, no cluster up); iteration 2's raw
  first-attempt output is above. The brief's fallback ("Eduard Ralph runs them before merge and
  records the figures in SUMMARY §9") stands if the human wants a fresh run.
* `W_completing` = 15 minutes is still a judgment value (0016 gives no number).
* #633 should be re-pointed to this slice and to child-2 (brief).
* Expect merge conflicts with #1016 in `custodian_gc.rs`, `06-runtime-view.md` and
  `crates/server/src/custodian.rs`, as the sign-off said.
