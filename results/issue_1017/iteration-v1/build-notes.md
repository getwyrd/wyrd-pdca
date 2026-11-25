# Build notes — issue 1017 (625.3: the reaper's terminal exits)

Base: `6e44de9 pdca-integrate: issue_1015` (getwyrd/wyrd @ main lineage, the harness worktree).
625.1 is on it (`427e5ec custodian: reap multipart uploads whose client went silent`, folded by
`6181de4`/`6e44de9`), and so is #659's drain: `20e6e21` (records drain, #1061), `3a392c8`
(bytes drain + mark guard incl. arm (0), #1062) and `f54db83` (session leg + terminal delete,
#1013). Arm (0) is `crates/custodian/src/retirement/marks.rs:307`.

Line numbers below are on the patched tree unless marked "base".

## What changed, and why

### 1. The `Completing` exits (D) — `crates/custodian/src/reaper.rs`

* `visit` (`reaper.rs:948-994`) now dispatches an owned `Completing` session to
  `judge_completing` (`reaper.rs:1068-1106`) instead of returning (base `reaper.rs:845-850`,
  the `deferred: #1017` marker, removed).
* `judge_completing` checks, in this order, with one clock read:
  1. `created_at` ahead of the clock → held (stamp-ahead, the existing rule);
  2. **`now - created_at > W_session` → `Arm::CompletingCeiling`**, straight to `Aborting@E+1`
     (`0016:1920-1930`, X79) — checked *before* the rollback so an over-age session is never
     rolled back first (negation 15);
  3. `fenced_at_millis` ahead → held;
  4. `now - fenced_at_millis > W_completing` → `Arm::Rollback`, to `Open@E+1`
     (`0016:2197-2198`, X2; measured from the fence instant, F16);
  5. else kept.
* `fence` (`reaper.rs:1337-1436`) is generalised over four arms through `Arm::transition`
  (`reaper.rs:527-548`): it still builds the one `fence_batch` (#895,
  `crates/core/src/multipart/abort.rs:83-99`) — `require(mpu == read)`, `require_absent` on
  every obligation key, the session put, the obligation puts — and still measures it against
  `MAX_BATCH_OPS`/`MAX_BATCH_BYTES` before sending. The over-age edge uses the existing
  `SessionRecord::completing_teardown` (the restore fence's shape: `retire:bytes:{session, all}` +
  `retire:records:{seg:<g>:<E>}`, `multipart.rs:2767-2779` base). 0016's pseudo-code says
  `{session, parts}` for this edge (`0016:2195`); I used `{session, all}` because core's own row
  table already decided that for the `Completing` teardown (`multipart.rs:4157` base — 10,000
  sparse part numbers encode past `MAX_VALUE_BYTES`, #842) and the brief says "the same shape as
  the restore fence".
* A lost fence is classified over **each** obligation key it required absent (`lost_any`,
  `reaper.rs:1442-1458`), using the one shared classifier `abort::lost`
  (`crates/core/src/multipart/abort.rs:381-399`).
* Tallies: `fenced_completing_ceiling` and `rolled_back` join `Pass`; `fenced()` counts them, so
  a pass that rolled back answers `Changed` and the clean-lap tag is cleared.
* The clock guard (`reaper.rs:948-980`) now **holds** a foreign-clocked `Completing` session (its
  only exit is the operator abort, #633) and a foreign-clocked `Completed` session (its only exit
  is the terminal-expiry verb, #633), exactly as 625.1 already held a foreign `Open` one; a
  foreign `Aborting` session is still left to the clock-free drain. Holding makes the pass
  `Blocked` and the one-shot exit non-zero, which E-terminal asserts.

### 2. The rollback transition — `crates/core/src/multipart.rs` (outside the brief's file partition)

`SessionRecord::completing_rollback` (`multipart.rs:2782-2803`) + `CompletingRollback`
(`multipart.rs:2936-2953`). **Deviation from the ordering note**, which limits this slice's edits
to `multipart.rs` to the budget constants and `SLOWEST_OP_MILLIS`'s doc. The rollback needs
`fence_released` (`multipart.rs:2856` base) and `RetireObligation::attempt_segments`
(`multipart.rs:4634` base), both `pub(crate)`/private, and core is the only crate that may mint a
session record or an obligation (its "a later writer's row is added as its own constructor"
rule, `multipart.rs:4598-4602` base). The alternatives were (a) hand-building the `Open@E+1`
JSON in the custodian — a second codec spelling, the exact drift core's constructors exist to
prevent — or (b) a §6 stop. (c) a 20-line `pub` constructor beside its twin
`completing_teardown`, reusing the two existing private pieces, is the smallest change that
restores the invariant. No conflict with child-2: it owns `crates/server/src/*`, not core.
Attempts stay as the fence left them (the "charged" release), so `MAX_COMPLETE_ATTEMPTS` keeps
bounding how many `{seg}` obligations a session can install (X58).

### 3. Tombstone expiry (G) — `crates/custodian/src/retirement.rs`, `retirement/sessions.rs`

* `Tombstone::Window { window_millis, clock_source }` (`retirement.rs:401-415`) and
  `Tombstone::passed` (`retirement.rs:419-436`): per session, `completed_at_millis` judged on the
  leg's one clock, and only for a session stamped under the window's `clock_source`; a foreign
  one is retained (E-terminal). A stamp ahead of the clock reads as inside the window
  (saturating subtraction), erring toward keeping the record.
* The terminal delete's gate (`sessions.rs:924`) calls `passed` instead of comparing with
  `Retain`. The clock read is `ctx.clock` — the same `LoopClock` the drain's marks and the
  reaper's judgments read (ADR-0009, one clock per lifecycle).
* `ReaperPolicy::default()` (`reaper.rs:384-396`) now carries `w_completing_millis` and
  `tombstone: Tombstone::Window { W_TOMBSTONE_MILLIS, WALL_CLOCK_SOURCE }`. The server builds its
  policy from `ReaperPolicy::default()` and hands `policy.tombstone()` to both the fleet loop
  (`crates/server/src/custodian.rs:628`) and the one-shot (via `reconciliation.rs:204`), so the
  flip reaches every deployed shape with **no server edit**.
* `Retain` and `Elapsed` stay: ~40 existing test contexts construct them.

### 4. The knob values

* `W_COMPLETING_MILLIS` = 15 min (`reaper.rs:257`), `W_TOMBSTONE_MILLIS` = 15 min
  (`reaper.rs:270`), each with its derivation in the doc comment and compile-time bounds
  (`< W_OPEN`, `W_COMPLETING < W_SESSION`). **0016 gives no numeric range for either** — I found
  only "the separate, shorter `W_completing`" (`0016:1917`) and the capacity coupling of
  tombstones to `MAX_SESSIONS` (`0016:966-969`). The brief says "inside 0016's range"; there is
  none to be inside, so the values are my judgment:
  - `W_completing`: far above a healthy Complete's bounded commits (each ≤ 5 s); rolling back a
    slow completer is safe (X6); it is the wait a client pays after a completer crash.
  - `W_tombstone`: covers a client retry horizon (minutes) while keeping the capacity cost —
    46 slots ⇒ at most 46 completions per window — tolerable. **This is a real trade-off the
    human should look at**: 1 h would be friendlier to slow retriers and cap completions at 46/h.
* `G_ORPHAN_MILLIS` = 60 s (`crates/custodian/src/gc.rs:304`) with compile-time strict bounds
  `> W_write + δ_clock` (31 s) and `> LATE_WRITE_DEADLINE_MILLIS` (41 s). It equals the deployed
  `GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`), which I may
  not edit; the server keeps its own spelling (see §6 item 2).

### 5. `B_ops` calibration (off-Check)

* `SLOWEST_OP_MILLIS` made `pub`, value unchanged (`multipart.rs:5987`); `MAX_BATCH_OPS` and
  `MAX_BATCH_BYTES` were already `pub`.
* `wyrd_metadata_conformance::calibrate_batch_ops` (`crates/metadata-conformance/src/lib.rs:1622`),
  outside `run_all`, takes every budget value as an argument (the crate stays trait-only,
  ADR-0016). Batches: (i) 250 keys × (`require_absent`, small put) = 500 ops; (ii) 500 ops whose
  bytes total `MAX_BATCH_BYTES`, half the keys guarded by exact-value preconditions on pre-seeded
  keys, half `require_absent`. Five runs each, every measurement printed, slowest asserted
  ≤ `MAX_BATCH_OPS × SLOWEST_OP_MILLIS` = 2,500 ms. An in-crate unit test pins the batch shapes
  (op count exact, bytes ≤ budget and > budget − 500).
* `batch_ops_calibration_against_fdb` (`crates/metadata-fdb/tests/conformance.rs:54`) and
  `..._against_tikv` (`crates/metadata-tikv/tests/conformance.rs:45`), gated exactly as the
  trait-contract test beside each; `wyrd-core` added as a dev-dependency of both crates.
* **Measured on this host** (saic, AMD Ryzen AI Max+ 395, 32 threads, Linux 7.0.0-42;
  single-node docker clusters; 2026-11-22), first attempt for each:

  | Backend | Batch | 5 runs (ms) | slowest | bound |
  |---|---|---|---|---|
  | FoundationDB 7.3.77 (`cargo xtask fdb-conformance`) | (i) op-bound, 500 ops, 8,250 B | 23, 16, 14, 13, 12 | 23 ms | 2,500 ms |
  | FoundationDB 7.3.77 | (ii) byte-bound, 500 ops, 5,000,000 B | 30, 36, 38, 36, 34 | 38 ms | 2,500 ms |
  | TiKV 8.5.1 (`cargo xtask tikv-conformance`, attempt 1, no retry) | (i) | 262, 281, 270, 241, 270 | 281 ms | 2,500 ms |
  | TiKV 8.5.1 | (ii) | 335, 337, 302, 296, 302 | 337 ms | 2,500 ms |

  Within the bound, so per the brief only `SLOWEST_OP_MILLIS`'s "assumed, not measured" wording
  was replaced with these figures; no constant changed. Both xtask runs exited 0 (whole FDB and
  TiKV conformance suites green too). A single-host cluster has no inter-node round trips; a real
  multi-node deployment will be slower, which the doc comment says.
* Not touched, per the brief's "only the wording" rule: `MAX_BATCH_OPS`'s own doc still says
  "**Uncalibrated** … #625 … must replace `SLOWEST_OP_MILLIS` with a measured figure"
  (`multipart.rs:6002-6005`). That paragraph now reads stale; a one-line follow-up for the human.

### 6. Docs — `docs/design/architecture/08-crosscutting-concepts.md:114`

New bullet "Ended and stuck multipart sessions" under 8.9: the two `Completing` exits, the
tombstone window and its capacity cost, the clock guard on them, `G_orphan`'s strict margins and
the measured `B_ops` figures. `06-runtime-view.md` is child-2's and was not edited.

## Production reach (call path)

Deployed loop: `CustodianService::run_reconstruction_until` (`crates/server/src/custodian.rs:590`
builds `ReaperPolicy::default()`, `:628` hands `policy.tombstone()` to the retirement leg, `:633-638`
the `ReaperContext`) → `reconcile_step` → `reconciliation.rs:185` `reap(...)` →
`reaper::reconcile` → `pass` → `walk` → `visit` (`reaper.rs:948`) → `judge_completing` →
`fence`; then `retirement::reconcile` → `sessions::reconcile` → `terminal` →
`Tombstone::passed`. One-shot (`wyrd custodian --reap-abandoned-uploads`, `cli.rs:1011`) →
`reconcile_step` with the reaper only → the same `reap(...)` → `reconcile_record_only(...,
reaper.policy.tombstone())` (`reconciliation.rs:200-206`). No new entry point.

## Tests

* **Binding**: `crates/server/tests/multipart_reaper_exits.rs` (new, not cfg-gated), 7 legs,
  every one a subprocess one-shot over redb with assertions on durable store state; seeding
  helpers copied from `multipart_reaper.rs`. It names no symbol this patch adds (all JSON + base
  API), so it compiles on the base.
  - D rollback, D over-age, E-terminal, G (now with a residue-holding `Completed` session so
    negations 4/5 have something to strand), H-terminal, F idempotence, F obligation-wait.
* DST, appended to `crates/dst/tests/custodian.rs` (`reaper_rows`):
  `prop_completing_rollback_race` (`:8645`, both interleavings over production Create + Complete:
  rollback-before-flip ⇒ nothing published, `Open@2`, nothing owed, part unmarked; flip-before-
  rollback ⇒ published, rollback loses, part never marked) and
  `prop_crash_at_each_step_of_the_completing_exits` (`:8841`, for each exit and every commit a
  clean step makes, a crash at that commit, landed or not by seed, then convergence with the
  count at its true value). Registered as `reaper_completing_rollback_race_both_interleavings`
  and `reaper_crash_at_each_step_of_the_completing_exits`. #659 child-1's rows cover the drain
  steps, not the reaper's `Completing` transitions, so the crash row is kept. The reaper writes no
  `orphan:` mark itself, so no reaper-vs-GC race row.
* `G_orphan` boundary: `crates/custodian/tests/gc.rs:1776`
  `a_completing_fences_mark_is_reclaimable_at_g_orphan_and_never_before_a_late_write` — a mark
  left by the reaper's over-age `Completing` fence, stamped `T0`; GC reclaims nothing at
  `T0 + W_write + δ`, `T0 + W_repoint + W_write + δ`, `T0 + G − 1`, and everything at `T0 + G`.
* Existing tests adjusted: `crates/custodian/src/reaper/tests.rs` (the `Pass` literal gains the
  two new tallies; the default-policy test asserts the new windows and `Tombstone::Window`).

## Red → green

* **Green (patched)**: `cargo test -p wyrd-server --test multipart_reaper_exits` → 7 passed.
  Whole `wyrd-custodian` package green; `multipart_reaper.rs` (625.1's) 13/13; DST new rows green
  at `MADSIM_TEST_NUM=5`.
* **Red (base)** — the project's C4 runner, `engine/scripts/run-verify.sh` (lane 1, base
  `WYRD_VERIFY_BASE=6e44de924cc8`, the worktree's base): GREEN leg 7 passed / 0 failed; RED leg
  (production reverted, test kept) **7 executed, 6 failed, 1 passed** →
  `run-verify.sh: PASS — red without the fix, green with it`.
  - The one leg green on the base is **H-terminal**, vacuous there as the brief predicted (the
    base has no `Completing` arm to refuse).
  - **E-terminal is red on the base**, not vacuous: the base leaves the foreign sessions'
    records alone (state assertions hold) but does not *hold* them, so the one-shot exits 0 and
    never names them — the hold is new behaviour of this slice.
  - **Both F legs are red on the base** — unlike the brief's prediction — because their fixtures
    reach teardown through this slice's over-age `Completing` edge, which the base lacks. #659's
    mechanism is evidenced instead by negations (8) and (9), which turn F red on the patched tree.
* `cargo xtask ci` on the patched tree: everything green (typos, docs lint/render, fmt, both
  clippy runs, build, cargo-machete, cargo deny incl. all-features advisories, statics,
  deploy-guard, the whole DST suite at 50 seeds) **except** the one child-2-owned server test in
  the NEEDS-HUMAN item below. (The second run also caught two lint findings in my own code — a
  manual `is_multiple_of` and two constant `assert!`s — both fixed; clippy re-run clean on the
  workspace and on `wyrd-dst --cfg madsim`.)

### Negations (each applied alone to the patched tree, the test file run, the edit reverted; tree verified byte-identical to the pre-negation snapshot afterwards)

| # | Flip | Result |
|---|---|---|
| (4) | `sessions.rs:361`: owned walk only for `Aborting`; a `Completed` session goes to the terminal delete only if `sidx:` already reads empty | **RED** — G (the residue session is stranded) |
| (5) | `sessions.rs:361-365`: run the walk step, then the terminal delete whatever it observed | **RED** — G (record deleted with `sidx:` residue left) |
| (6′) | `reaper.rs:948`: clock guard skipped for `Completing` | **RED** — E-terminal (foreign over-age `Completing` fenced) |
| (8) | `marks.rs:307` (#659's arm 0): `reclaiming` falls through to the different-event arm | **RED** — F obligation-wait (mark replaced) |
| (9) | `sessions.rs` `terminal`: skip the session-obligation check | **RED** — F idempotence and F obligation-wait (record deleted / slot released while owed) |
| (15) | `reaper.rs:1084`: ceiling arm disabled for `Completing` (so the session is rolled back, then fenced by the `Open` ceiling next pass) | **RED** — D over-age (marks under token `s:<id>:2`, tallies), both F legs |
| base-like | `reaper.rs:986`: `Completing` returns unjudged (the base) | **RED** — both new DST rows |

F's obligation-wait fixture is green with arm (0) intact, i.e. #659 child-1's arm (0) is present
and correct on this base; F is regression coverage of #659's mechanism through the reaper's path,
as the brief predicted.

## Refuting my own test

* **(a) Genuine red?** Yes. `run-verify.sh` reverted the production change and kept the test:
  7 legs executed, 6 failed (the seventh, H-terminal, is the vacuous one the brief names). Each
  negation above also goes red on the leg it targets, and the two DST rows go red with the
  `Completing` arm disabled.
* **(b) Production path?** Yes. Every leg spawns the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) with `custodian --reap-abandoned-uploads` over a real redb
  store — the production one-shot, `reconcile_step`, the reaper and the real retirement leg. The
  DST rows call the production `create_session`, `complete_upload` and `reconcile_step`.
* **(c) Fixture includes the fault?** Yes. D seeds the stuck `Completing` session with real
  `seg:` records and a later epoch's group; E-terminal seeds foreign sessions *past* every window
  (not inside one); G seeds a `Completed` session with crashed-part residue; F seeds an actual
  `reclaiming` mark over a live part position; the DST crash row crashes at every commit index a
  clean step makes, landed and not landed.

## NEEDS-HUMAN

NEEDS-HUMAN: `crates/server/tests/custodian_gc.rs::deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones` fails on this patch, by design of this slice. It pins the pre-#625 "retain" tombstone: its `Completed` session has `completed_at_millis: 3_000` (`custodian_gc.rs:1470`) and the loop runs at `LOOP_NOW = 10_000_000` (`:1176`), 2.8 h later, so under `W_tombstone` = 15 min the deployed loop now (correctly) expires it and the assertion at `:1531` ("RED if the loop hands the leg `Elapsed`") fires. That file is child-2's and this slice must not edit it (brief, *Ordering note*). Proposed fix for child-2 / the human — keep the test's intent (the leg keeps a tombstone **inside** its window) by stamping the publication inside the window:

```diff
--- a/crates/server/tests/custodian_gc.rs
+++ b/crates/server/tests/custodian_gc.rs
@@ (the CompleteRequest / PublishRequest of the completed upload, ~:1460-1471)
-        fenced_at_millis: 2_000,
+        fenced_at_millis: LOOP_NOW - 2_000,
@@
-        completed_at_millis: 3_000,
+        completed_at_millis: LOOP_NOW - 1_000,
```
and reword the two messages at `:1532-1533` / `:1551` from "`Retain` until #625" to "inside `W_tombstone`". Until that lands `cargo xtask ci` has exactly this one failure.

NEEDS-HUMAN: `G_orphan` has two spellings of 60 s — `wyrd_custodian::gc::G_ORPHAN_MILLIS` (new, with the strict 0016 bounds asserted) and the server's `GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS` (`crates/server/src/custodian.rs:115`, child-2's file). The server should take `wyrd_custodian::gc::G_ORPHAN_MILLIS` (or `const _: () = assert!(GC_GRACE_WINDOW_MILLIS == G_ORPHAN_MILLIS)`) so they cannot drift; I could not make that edit.

NEEDS-HUMAN: `W_completing` and `W_tombstone` (15 min each) are judgment values — 0016 states no numeric range for either. Please confirm, especially `W_tombstone`'s capacity trade (46 completions per window under the shipped `MAX_SESSIONS`).

Other items for sign-off (not blocking):
* The `crates/core/src/multipart.rs` edit outside the brief's partition (§2 above).
* `MAX_BATCH_OPS`'s "Uncalibrated" paragraph is now stale (§5).
* #633 should be re-pointed to this slice and to child-2 (brief); a foreign-clocked `Completing`
  or `Completed` session is now *held* by the reaper and waits for #633's verbs.
