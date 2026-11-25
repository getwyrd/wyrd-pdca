# Build notes — issue 1016 (625.2): the multipart reaper runs on every deployed custodian startup shape

Iteration 4. Target: getwyrd/wyrd @ main, stacked on the integration branch
`origin/pdca-integration/main-r9dd123c9ed70` (the bundle's `stack-base`), tip `6e44de9`
("pdca-integrate: issue_1015"). 625.1 is on it (`427e5ec`, "custodian: reap multipart uploads
whose client went silent", an ancestor of `HEAD` in the worktree). 625.1 is not on `origin/main`,
so this patch cannot apply there (see "For the human").

I started from iteration 3's `patch.diff` (it applied cleanly on `6e44de9`) and changed only what
the iteration-3 carry-forward asked for. Line numbers are on the patched tree unless marked
"base" (= `6e44de9`).

## What iteration 3 got wrong, and what changed

| Carry-forward finding | Fix in this iteration |
|---|---|
| C5: a Ctrl-C during the no-endpoints role's first pass killed it by signal. `ctrl_c()` installs its SIGINT handler on its first poll, and the driver first polled `shutdown` after its first pass (v3 `custodian.rs:715-720`). Reproduced by the reviewer, 10/10 signal deaths. | The driver polls `shutdown` once before its first pass (`custodian.rs:712-715`) and remembers whether it was already done (`stop_requested`, honoured at the first wait, `custodian.rs:730-732`; a finished future is never polled again). The poll installs the handler, so a Ctrl-C during the first pass is held until the pass ends, and the role then exits 0 with "shutting down", as for any later pass. Doc: `custodian.rs:680-690`. |
| T5: the subprocess test only signalled after five passes, so the first-pass window was untested. | New `no_endpoints_role_finishes_its_first_pass_when_stopped_during_it` (`multipart_reaper_deployed.rs:793`): 48 idle uploads, SIGINT at the first "fenced an abandoned upload" line (inside pass 1), then: exit 0, "shutting down", exactly one pass tally, 48 fence lines, every upload's `mpu:` record moved. |
| T5: "existing seeded tests bypass the new driver" (rubric: seeded Tier-0 DST for a new destructive path). | Tier-0 is `crates/dst`, which the brief forbids (child-3 owns it) and which does not depend on `wyrd-server`. Added seeded Tier-1 coverage of the real driver in `custodian_gc.rs` (the file this slice owns), using `wyrd_testkit::Sim` as `dst_commit.rs` does: the no-fleet loop over 32 seeds of store faults (`:2013`), the fenced arm (`:2064`), the stop rule on the fleet side (`:2113`). Recorded in `review-rejected.md`, decision 2. |
| Doc claims ("a stop takes effect between two passes, never during one") were false for the first pass. | True now for both looping shapes. `06-runtime-view.md:94` says both loops register Ctrl-C before their first pass, and says the empty-fleet one pass waits for no stop. |
| C4 diff coverage: "patch.diff does not apply on origin/main". | Not fixable in the patch (see "For the human"). |

## The Ctrl-C fix: where it sits, and what I ruled out (with costs)

**Chosen: one poll in the driver** (`custodian.rs:707-715` plus the 3-line check at `:730-732`,
12 added lines in one function). Both looping shapes run this driver, so it fixes the
no-endpoints role and the fleet role in the same line, and every test that drives either shape
covers it. The poll happens after the store is open, right before the first pass.

Ruled out:

- **Arm Ctrl-C in the CLI, before the no-endpoints banner** (a ~12-line `armed_ctrl_c()` helper
  in `cli.rs`, called at `:1203`). It would close the small window between the banner
  (`cli.rs:1204`) and the driver's first poll, but it leaves the fleet role dying on SIGINT during
  its first pass (the same driver, the same doc claim). Fixing the fleet role the same way means
  arming before `run_reconstruction_over_backend` (`cli.rs:1391`), which opens the store inside
  it: a Ctrl-C during a TiKV connect (bounded only by the adapter's operation deadline,
  `crates/metadata-tikv/src/lib.rs:1051-1066`) would then be held until the end of the first
  pass. Arming after the open instead needs a signature change to the `pub`
  `run_reconstruction_over_backend` (its 3 backend arms, `cli.rs:1995-2044`, plus 2 callers in
  `custodian_day_one.rs:1350,1377` and 1 in `closed_write_path.rs`). And the fleet arming would be
  untested: there is no subprocess test of the fleet role, and adding one needs a live
  `wyrd d-server` subprocess with port selection.
- **Leave the fleet role alone** (scope rule: a pre-existing gap gets a decline, not an in-PR
  fix). Rejected because the gap is in the driver this slice made shared, not in fleet-only code:
  with the poll in the driver, fixing the no-endpoints role fixes the fleet role at no extra cost.

What the chosen fix costs:

- **Timing of callers' stop futures.** A stop whose timer starts on its first poll (the
  in-process tests' `async { sleep(60 ms).await }`, `custodian_gc.rs:518`;
  `custodian_day_one.rs` uses 120 ms and 80 ms) now starts its window before the first pass, not
  after it. The loop still runs at least one pass, and starts each later pass only while the
  window is still open. All 54 `wyrd-server` test targets pass (`cargo test -p wyrd-server`), and
  `cargo xtask ci` passes (below). A first pass longer than such a window would leave those tests
  with one pass; none of them is near that on this host.
- **The banner window.** A SIGINT that lands between the no-endpoints banner (`cli.rs:1204`) and
  the driver's first poll still has its default action. The store is open but nothing has been
  written yet. The new test sends its SIGINT at the first fence line, which is printed after the
  poll, so it is deterministic.
- **The empty-fleet pass waits for no stop.** It is the one-shot's pass
  (`reap_abandoned_uploads_once`), and like the one-shot it registers no handler: a Ctrl-C during it ends the process at
  once. The role panics right after it either way. Each fence and drain step is its own commit, so
  what the pass committed stays and the next start's pass goes on from the store (G2c shows the
  teardown completing across starts). Said in `cli.rs:1293-1296` and `06-runtime-view.md:94`.

## What changed, file by file (whole patch)

### `crates/server/src/custodian.rs`

- `FleetPlane` (`:324`, private, `Copy`): the fleet inputs of the loop.
- `reap_abandoned_uploads_once` (`:476`) calls `reaper_pass(.., None)` (`:488`).
- `reaper_pass` (`:508`, private): the reaper's one step, one fenced `reconcile_pass` with the
  reconstruction context when there is a fleet.
- `run_reaper_until` (`:545`, pub): `run_until(.., None, ..)`.
- `run_reconstruction_until` (`:636`): signature unchanged; body is `run_until(.., Some(plane), ..)`.
- `run_until` (`:692`, private): the one interval driver. **New this iteration:** the stop polled
  once before the first pass (`:707-715`) and `stop_requested` honoured at the first wait
  (`:730-732`); doc `:680-690`. As in iteration 3: one wait at the loop top, skipped on the first
  interval (`:729-737`); the no-fleet arm (`:739-763`): `reaper_pass(.., None)`, `Fenced` stops,
  `Store` is logged and retried; the fleet's reaper dispatch `reaper_pass(.., Some(&ctx))`
  (`:840`); GC's gate (`:907`) unchanged.

### `crates/server/src/cli.rs`

As in iteration 3, plus one comment (`:1293-1296`, the empty-fleet pass and Ctrl-C):

- `usage()` and the `cmd_custodian` doc (`:962-971`).
- Refusals before the runtime starts (`:1046-1066`): no-fleet flags, `--interval-secs 0`.
- No `--endpoints` (`:1185-1215`): `open_no_fleet_meta` (`:1203`), banner, `run_reaper_until`
  (`:1212`) until Ctrl-C, then success.
- Empty fleet (`:1275-1322`): one pass when `!reconcile_after_restore` (`:1303`), errors logged,
  then the unchanged `panic!` (`:1317`).
- `NO_FLEET_EXCLUDES` / `no_fleet_conflict` (`:1808`, `:1825`), `open_no_fleet_meta` (`:1943`),
  `open_existing_local_meta_redb_for`.

### `docs/design/architecture/06-runtime-view.md`

Line 94, 625.1's reaper paragraph: the startup shapes, as in iteration 3, with the stop sentence
corrected ("Both loops register Ctrl-C before their first pass … the first pass included") and a
sentence on the empty-fleet pass and Ctrl-C.

### Tests

- `crates/server/tests/multipart_reaper_deployed.rs` (NEW, not `#![cfg]`-gated), nine tests. New
  this iteration: `Interrupt` (`:284`) so `run_role` (`:343`) can send SIGINT at a stderr line;
  `FENCED` (`:130`), `FIRST_PASS_UPLOADS` (`:136`, checked below `SESSIONS_PER_PASS`),
  `seed_idle_uploads` (`:605`), and the first-pass test (`:793`). Unchanged: G2a (`:726`), G2c
  (`:850`), the restore guard (`:904`), the two no-store tests (`:939`, `:970`), the refusals
  (`:1000`), the zero interval (`:1038`), the usage (`:1102`).
- `crates/server/tests/custodian_gc.rs` (APPENDED): `leg_a_records` (`:1688`); `LegA` (`:1764`),
  which G2b now uses for its fixture and its closing assertions (same assertions as iteration
  3); G2b (`:1845`); new `FlakyMeta` (`:1912`) and three tests (`:2013`, `:2064`, `:2113`).

## Call path from `cmd_custodian` to the reaper, per shape

1. **No `--endpoints`.** `cmd_custodian` (`cli.rs:978`) → refusals (`:1046-1066`) →
   `Custodian::elect` → `let Some(endpoints) = endpoints else` (`:1185`) → `open_no_fleet_meta`
   (`:1203` → `:1943`) → `CustodianService::run_reaper_until` (`:1212` → `custodian.rs:545`) →
   `run_until(.., None, ..)` (`custodian.rs:692`) → stop polled, Ctrl-C handler installed
   (`:712`) → each interval `reaper_pass(.., None)` (`:749` → `:508`) → `reconcile_pass` →
   `wyrd_custodian::reconcile_step` with `reconstruction = None, reaper = Some` → the reaper, then
   the record-only retirement leg. Wait `interval` (`:729-737`), repeat, until Ctrl-C.
2. **`--endpoints` given, none reachable.** `cmd_custodian` → `Custodian::elect` → `connect_fleet`
   (`cli.rs:1266`) returns empty → `if configured.is_empty()` (`:1275`) →
   `if !reconcile_after_restore` (`:1303`) → `open_no_fleet_meta` (`:1305`) →
   `reap_abandoned_uploads_once` (`custodian.rs:476`) → one `reaper_pass(.., None)` →
   `reconcile_step` record-only → `panic!` (`cli.rs:1317`), unchanged.
3. **Fleet (whole or degraded).** `cmd_custodian` → `connect_fleet` (non-empty) →
   `run_reconstruction_over_backend` (`cli.rs:1391`) → `run_reconstruction_until`
   (`custodian.rs:636`) → `run_until(.., Some(plane), ..)` → stop polled (`:712`) → each interval,
   after the scrub pass, `reaper_pass(.., Some(&ctx))` (`:840`) → `reconcile_step` with
   `reconstruction = Some, reaper = Some`. Then GC, only if `fleet.len() == operator_fleet_size`
   (`:907`).

## Red → green evidence

Runner: the project's verify gate `engine/scripts/run-verify.sh`, on the final `patch.diff`, with
`PDCA_BUNDLE` = a scratch copy of the bundle (patch + brief only),
`PDCA_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, `WYRD_REPO=$PDCA_WORKTREE`, its worktree
and target dir under `$PDCA_SCRATCH`, `PDCA_LANE=b1016`. Base commit: `6e44de9`; 625.1 is on it.

- GREEN (fix applied): `running 9 tests` … `test result: ok. 9 passed; 0 failed` (4.14 s).
- RED (production reverted, tests kept): `running 9 tests` … `test result: FAILED. 2 passed;
  7 failed` (0.35 s). **Executed: 9, failed: 7.**
  - G2a fails on the store assertion (`:736`): 0 passes, the role exited 0 at once ("no
    --endpoints; nothing to reconstruct").
  - The new first-pass test fails at `:800`: never interrupted, since the base role writes no
    fence line before exiting.
  - G2c fails on the store assertion (`:888`): no convergence in 6 starts; every start panicked
    (exit 101) with the diagnostic and made 0 passes. The status is non-zero on both trees, so the
    store assertion is what tells them apart, as the brief says.
  - The data-dir refusal (`:951`), the flag refusals (`:1012`), the zero interval (`:1062`), the
    usage (`:1119`).
  - **Pass on base by design:** `empty_fleet_restore_one_shot_fails_with_the_store_untouched` and
    `empty_fleet_role_creates_no_store` (on base the empty fleet panics before touching any
    store). Each guards an earlier iteration's defect, pinned by mutants F and G (iteration 3).
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).`

**Red against iteration 3's code** (the defect this iteration fixes): with only the poll removed
(`let stop_requested = false;`), the first-pass test failed **5 of 5 runs** at `:806` (re-run on the final tree), "not by the
signal (Some(ExitStatus(unix_wait_status(2))))": the role died of SIGINT mid-pass. The in-process
checks fail the same way: `fleet_loop_polls_its_stop_before_its_first_pass` (`left: 1, right: 0`
probes when the stop was first polled) and the seeded test (seed 0: 13 store calls before the stop
was polled).

### G2b does not go red on the base: the brief's claim for it is stale

Unchanged since iteration 2. 625.1 already dispatches the reaper on the reconstruction pass (base
`custodian.rs:672-683`, `Some(&reaper)` at `:680`), outside GC's gate (base `:749`). So G2b passes
on the base. It is a regression guard, pinned by negation 11, and `cargo xtask ci` evidence as
the brief says.

### Negations and mutants (run this iteration, on the final tree)

Each was a scripted text edit to one production file, then the named test through `cargo test -p
wyrd-server --test <file>` under `timeout`, then the file restored from a scratch copy and checked
with `cmp`.

- **(11)** the fleet loop dispatches the reaper only when `fleet.len() == operator_fleet_size`
  (else a reconstruction pass with no reaper): G2b **FAILED** at `custodian_gc.rs:1807`; the other
  15 tests in the file passed.
- **(12)** the no-endpoints early exit left in place (`run_reaper_until` call replaced by
  `drop((meta, shutdown))`): G2a **FAILED** at `:736`, 0 passes, exited 0 by itself.
- **(13)** the empty-fleet pass skipped (`if !reconcile_after_restore` → `if false`): G2c
  **FAILED** at `:888`, never converged.
- **A** the wait is `sleep(interval.saturating_sub(interval))`: G2a **FAILED** at `:765`, the
  spacing assertion.
- **K** (this iteration's defect) the stop not polled before the first pass: see "Red against
  iteration 3's code" above.
- **L** the no-fleet `Store` arm returns the error instead of retrying: the seeded test **FAILED**
  at seed 0 ("a store fault ended the loop: reconciliation store access: injected metadata
  fault"). With the fix, 162 passes across the 32 seeds failed on an injected fault and were
  retried (counted from the "retrying next interval" lines with `--nocapture`), so the retry arm
  is exercised, not only reachable.
- **M** the no-fleet `Fenced` arm keeps looping: the fenced test **FAILED** (its 60 s bound,
  `custodian_gc.rs:2094`).
- **N** a stop already done at the first poll not honoured (`if false && stop_requested`): the
  fleet stop test **FAILED** with "`async fn` resumed after completion" — the driver re-polled a
  finished future. (Deleting the check outright does not compile under `-D warnings`.)

## Refuting my own test

- **(a) Genuine red?** Yes. The verify gate reverted production and kept the tests: 9 executed, 7
  failed, all on assertions (the file names no symbol the patch adds and drives the binary by
  argv, so it compiles on the base). Against iteration 3's code, the new first-pass test went red
  5/5 by signal death. Negations 11–13 and mutants A, K, L, M, N each fail their own test.
- **(b) Production path?** Yes. The new first-pass test runs the real built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`), the ordinary `custodian` role with no one-shot flag (asserted in
  `role_every`), over a real redb store, and sends a real SIGINT through `kill -s INT`. The fence
  lines and pass tallies it counts are the reaper's own `emit_fenced` / `emit_pass` on the role's
  log sink. The in-process tests drive the real `CustodianService::run_reaper_until` /
  `run_reconstruction_until`, which are the same `run_until` the binary runs. `FlakyMeta` is a
  fault-injecting store under the real driver, not a stand-in for it.
- **(c) Fixture includes the fault?** Yes. The SIGINT lands inside the first pass: it is sent at
  that pass's first fence line, and the test asserts no pass tally had been seen by then
  (`passes_when_interrupted == Some(0)`). The seeded test asserts at least one fault was injected
  on every seed, and the mutant L run shows faults reach the driver as failed passes. G2c's
  endpoint is refused by the real gRPC dial; G2b hands the loop 3 servers against an operator
  count of 4.

## For the human

- **G2c and the brief disagree with each other** (in `deferred-findings.json`, C1): the brief
  asks for exactly one pass on the empty-fleet startup and leg A's full transition after one run.
  Teardown takes three passes, so both cannot hold. I kept one pass per start (checked on every
  start) and reach the transition across starts, as `Restart=on-failure` would.
- **G2b's red claim in the brief is stale** (above; in `deferred-findings.json`).
- **Behaviour changes to weigh:**
  - New this iteration: a Ctrl-C during the **fleet** role's first pass now lets that pass finish
    and exits 0, instead of killing the process by signal (the base behaviour). It also means a
    Ctrl-C during that first pass waits for it to end, as one during any later pass already did.
  - From iteration 3, unchanged: `wyrd custodian` with no `--endpoints` on a data dir with no redb
    store now exits 1 naming the path (base exited 0); with a store, the no-endpoints role holds
    redb's exclusive lock while it runs, as the fleet role does; `--ids`, `--failure-domains` or
    `--gc-expired-pending` without `--endpoints` are refused; `--interval-secs 0` is refused for
    every looping shape.
- **The diff-coverage gate measures against `origin/main`.** "patch.diff does not apply on
  origin/main" again: `origin/main` lacks 625.1 (`427e5ec`), so no patch for this slice can apply
  there; it applies on `6e44de9`. That looks like the driver giving C4-diff-cov a different base
  than C4-verify — a harness matter, not a patch defect.
- `review-rejected.md` holds the two decisions still standing (redb's synchronous calls; Tier-0
  DST), with this iteration's anchors.
- #633 must be re-pointed to this slice and to child-3 after acceptance (brief, Ordering note).
- **Scratch leftovers.** My runs left directories and logs under `$PDCA_SCRATCH`
  (`pdca-builder-1016-*`, including the verify worktree and its target dir) and the branches
  `pdca-verify-lb1016` / `pdca-cov-lb1016` in the shared repo. Cleanup belongs to the harness;
  once it reclaims the scratch root, `git worktree prune` and
  `git branch -D pdca-verify-lb1016 pdca-cov-lb1016` remove the rest.

## Commit-readiness and `cargo xtask ci`

- `cargo fmt --all` run over the worktree; `cargo fmt --all --check` clean.
- `cargo clippy -p wyrd-server --all-targets -- -D warnings`: clean. The same with
  `--all-features`: clean, so the TiKV and FoundationDB arms compile.
- `cargo test -p wyrd-server`: all 54 test targets `ok`, no failure (the driver's earlier stop
  poll changes when callers' stop timers start; nothing broke).
- `cargo xtask ci` through the project wrapper (`engine/xtask.sh ci`, the C4-ci gate) on the
  worktree holding exactly the final `patch.diff` (checked with `git diff | cmp - patch.diff`):
  **rc 0, "xtask ci: all checks passed"**. Every step ran: typos, lint_docs, render_site (101
  pages, link audit OK), gitlink/unsafe/blackbox guards, fmt, clippy, build, the workspace tests,
  cargo-machete, the three cargo deny runs, statics, deploy-guard, and the DST clippy and tests.
  235 `test result: ok` lines, 0 `FAILED`. Among them `custodian_gc` (16 passed) and
  `multipart_reaper_deployed` (9 passed, 4.15 s).
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed; both ran
  inside `cargo xtask ci`.

## Diff coverage, measured on the right base

`engine/scripts/run-diff-cov.sh` with `PDCA_BASE=6e44de9`, against a scratch copy of the bundle
(patch + brief only), so the gate's own `coverage/diff-cov.json` was not overwritten: **PASS,
80.8%** — 135 of 167 instrumentable changed lines ran (floor 80%); 167 of 361 changed lines were
instrumentable. This gate runs only the test file the patch adds. All 32 lines it did not see
are in `custodian.rs`, and each is run by a `custodian_gc.rs` test:

- `:648-667`, `:740-742`, `:840`: the fleet side of the driver (the `run_reconstruction_until`
  wrapper, the fleet destructuring, the fleet's reaper dispatch) — G2b and every fleet-loop test.
- `:731`: the return when the stop was already done at the first poll —
  `fleet_loop_polls_its_stop_before_its_first_pass` (and mutant N shows it is needed).
- `:754-760`: the no-fleet loop's `Fenced` and `Store` arms —
  `no_fleet_loop_stops_when_a_newer_term_holds_the_zone` and the seeded fault test (mutants M
  and L).
