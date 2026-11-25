# Build notes — issue 1016 (625.2): the multipart reaper runs on every deployed custodian startup shape

Iteration 3. Target: getwyrd/wyrd @ main, stacked on the integration branch
`origin/pdca-integration/main-r9dd123c9ed70` (the bundle's `stack-base`), tip `6e44de9`
("pdca-integrate: issue_1015"). 625.1 is on it: `git merge-base --is-ancestor 427e5ec HEAD`
succeeds in the worktree (`427e5ec` = "custodian: reap multipart uploads whose client went
silent"). 625.1 is not on `origin/main`, so this patch cannot apply there (see "For the human").

Line numbers are on the patched tree unless marked "base" (= `6e44de9`). The brief's own line
numbers were taken on an older tree; on this base the cited spots are: `cmd_custodian` base
`cli.rs:959`, the no-endpoints exit base `cli.rs:1145-1163`, the empty-fleet panic base
`cli.rs:1223-1237`, the fleet dispatch base `cli.rs:1305-1322`; `run_reconstruction_until` base
`custodian.rs:565`, the whole-fleet GC gate base `custodian.rs:749`, `connect_fleet` base
`custodian.rs:236-265`; `drive_deployed_loop_operator` base `custodian_gc.rs:491`.

## What iteration 2 got wrong, and what changed

| Carry-forward finding | Fix in this iteration |
|---|---|
| C5/BUG: the whole-pass timeout threw away a pass's progress, so on a slow store later uploads and the drain could never be reached (`custodian.rs:583/584` in v2). | **Removed.** `NO_FLEET_REAPER_PASS_BOUND`, both `tokio::time::timeout` calls and `open_within_no_fleet_bound` are gone. A pass is bounded by what it may do (625.1's per-pass budgets) and, on TiKV and FoundationDB, by each store call's own adapter deadline — the reviewer's own suggested fix ("use the existing backend operation deadlines and work budgets"). Doc: `custodian.rs:679-684`. |
| T5/TEST-GAP: the timeout and shutdown race could drop a pass mid-commit, with no test of that (`custodian.rs:582/587/591` in v2). | **The path is gone.** The role reads its shutdown only between two passes (`custodian.rs:711-721`), exactly as the fleet loop always has (base `custodian.rs:795-798`). A pass is never dropped by the role. |
| G2a counted passes but not their spacing; `sleep(interval.saturating_sub(interval))` passed. | Each stderr line is timed by the reader thread as it is read (`multipart_reaper_deployed.rs:321-330`); G2a asserts the first and fifth tally are at least `4 × 1 s − 500 ms` apart (`:688-698`). The reviewer's exact mutant now fails at `:693`. |
| The doc claimed every no-fleet pass is time-bounded (`06-runtime-view.md:94`). | Reworded: no deadline on a pass as a whole; bounded by budgets and, on TiKV/FDB, per-call deadlines. |
| `--gc-expired-pending` silently ignored with no `--endpoints` (`cli.rs:1184` in v2). | Refused, together with `--ids` and `--failure-domains`, before anything is opened: `NO_FLEET_EXCLUDES` (`cli.rs:1805`), `no_fleet_conflict` (`cli.rs:1822`), check at `cli.rs:1046-1055`. Same shape as the one-shot's `REAPER_ONE_SHOT_EXCLUDES` (base `cli.rs:1679`). |
| A mistyped `--data-dir` made an empty redb store and reaped it forever (`cli.rs:1894`); the empty-fleet pass also created one (`cli.rs:1936`). | Both no-fleet shapes open through `open_no_fleet_meta` (`cli.rs:1940`), whose redb arm is the store's non-creating open, `open_existing_local_meta_redb_for` (`cli.rs:2321`). The one-shot's `open_existing_local_meta_redb` (`cli.rs:2309`) now delegates to it with its exact old message. |
| `--interval-secs 0` turned the no-endpoints role into back-to-back scans (`custodian.rs:612`). | Refused for every looping shape (`cli.rs:1059-1066`); the one-shots never wait and still take 0. |
| C5 missed mutant: `open_within_no_fleet_bound -> Ok(Default::default())`. | The function is removed (see row 1). |
| Deferred: "two loops instead of one driver". | **One driver now**: `CustodianService::run_until` (`custodian.rs:687`). `run_reconstruction_until` (`:636`) and `run_reaper_until` (`:545`) are thin wrappers around it. |
| C4 diff coverage: "patch.diff does not apply on origin/main". | Not fixable in the patch (see "For the human"). |

## What changed, file by file

### `crates/server/src/custodian.rs`

- `FleetPlane` (`:324`, private, `Copy`): the fleet inputs of the loop — `configured`,
  `operator_fleet_size`, `expired_pending`.
- `reap_abandoned_uploads_once` (`:476`) now calls `reaper_pass(.., None)` (`:488`); behaviour
  unchanged; its doc names the empty-fleet pass too.
- `reaper_pass` (`:508`, private): the reaper's one step — `ReaperContext` over the caller's
  `LoopClock`, `clock_source = wall`, `ReaperPolicy::default()`, one fenced `reconcile_pass`, with
  the reconstruction context when there is a fleet. As in iteration 2.
- `run_reaper_until` (`:545`, pub): `run_until(.., None, ..)` (`:558`).
- `run_reconstruction_until` (`:636`): **signature unchanged**; its body is now
  `run_until(.., Some(FleetPlane { .. }), ..)` (`:651-667`).
- `run_until` (`:687`, private): the one interval driver. The base loop body moved into it
  **unchanged** — same indentation, same lines — except for three edits:
  - one wait at the loop top, skipped on the first interval (`:711-721`), replacing the base wait
    at the loop bottom (base `:795-798`). Pass, wait, pass, … — the same order as before;
  - `let Some(FleetPlane { configured, operator_fleet_size, expired_pending }) = plane else { … }`
    (`:722-746`): with no fleet, one `reaper_pass(.., None)` (`:732`), `Fenced` stops the loop,
    `Store` is logged and retried, then `continue` to the wait. The destructuring rebinds the
    base parameter names, so the fleet body below is textually the base one;
  - the fleet's reaper dispatch is `reaper_pass(.., Some(&ctx))` (`:823`) instead of the inline
    `ReaperContext` + `reconcile_pass` (base `:634-639`, `:672-683`). Same call into
    `reconcile_step`, same error arms. GC's gate (`:890`) is unchanged.

### `crates/server/src/cli.rs`

- `usage()` (`:515-524`) and the `cmd_custodian` doc (`:962-971`): the new behaviour, the store
  rule, the refusals.
- Before the runtime starts (`cli.rs:1046-1066`): `one_shot`; the no-fleet refusals; the
  `--interval-secs 0` refusal.
- **No `--endpoints`** (`:1198-1214`): instead of printing "nothing to reconstruct" and returning
  success (base `:1158-1162`), open the store with `open_no_fleet_meta` (`:1203`), print a banner
  naming the store (`reaper_store_name`, `:1846`, doc widened), then
  `service.run_reaper_until(.., shutdown)` (`:1211-1213`) until Ctrl-C, then success. The
  `--reconcile-after-restore` refusal on that branch is untouched and still comes first.
- **Empty fleet** (`:1284-1313`): after `connect_fleet` comes back empty (`:1275`) and before the
  unchanged `panic!` (`:1314`), and only when `!reconcile_after_restore` (`:1300`): one pass —
  `open_no_fleet_meta` then `reap_abandoned_uploads_once` (`:1301-1307`). Any error is logged and
  never returned, so the panic message and the exit status (101) do not change. The pass runs
  under the leadership from `Custodian::elect`, which runs before the endpoints check.
- `NO_FLEET_EXCLUDES` / `no_fleet_conflict` (`:1798-1833`).
- `open_no_fleet_meta` (`:1930-1957`): one `Box<dyn MetadataStore>` per backend. It replaces
  iteration 2's `run_reaper_over_backend`, `run_reaper_pass_over_backend` and
  `open_within_no_fleet_bound` (106 added lines in v2) with 29.
- `open_existing_local_meta_redb_for` (`:2318-2339`), and `open_existing_local_meta_redb`
  (`:2309-2316`) delegating to it with the old message text.

### `docs/design/architecture/06-runtime-view.md`

Line 94, appended to 625.1's reaper paragraph: the three startup shapes, one loop for both
looping shapes, the pacing and stop rule, what bounds a pass, the store rule and refusals, and
that the restore one-shot runs no pass.

### Tests

- NEW `crates/server/tests/multipart_reaper_deployed.rs`, not `#![cfg]`-gated, leg A's helpers
  copied from `multipart_reaper.rs`. Eight tests:
  - G2a `no_endpoints_role_reaps_every_interval_in_one_lifetime` (`:657`): one process, five
    passes, spaced an interval apart, then SIGINT, clean exit, leg A's transition;
  - G2c `empty_fleet_role_runs_one_reaper_pass_then_fails_as_before` (`:719`): every start exits
    non-zero with the diagnostic and exactly one pass (`:736`), the first start moves the session
    (`:742`), leg A's transition after convergence;
  - `empty_fleet_restore_one_shot_fails_with_the_store_untouched` (`:773`);
  - NEW `no_endpoints_role_refuses_a_data_dir_with_no_store` (`:808`);
  - NEW `empty_fleet_role_creates_no_store` (`:839`);
  - NEW `no_endpoints_role_refuses_a_flag_only_a_fleet_uses` (`:869`);
  - NEW `a_zero_interval_is_refused_wherever_the_role_loops` (`:907`): both looping shapes
    refused, store untouched; the reaper one-shot still takes 0 and runs one pass;
  - `usage_says_what_the_role_does_with_no_fleet` (`:968`).
  - Command builders: `role` / `role_every` / `custodian` (`:216-249`) pin
    `--metadata-backend redb`, drop `WYRD_METADATA_BACKEND`, pin `--log-level info`.
- APPENDED to `crates/server/tests/custodian_gc.rs` (unchanged from iteration 2): `leg_a_records`
  (`:1677`) and G2b `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred`
  (`:1753`), three servers against an operator count of four through `drive_passes_at` (`:1373`,
  the same driver shape as `drive_deployed_loop_operator`, `:496`).

## Call path from `cmd_custodian` to the reaper, per shape

1. **No `--endpoints`.** `cmd_custodian` (`cli.rs:978`) → `no_fleet_conflict` and the interval
   check (`:1046-1066`) → `Custodian::elect` → `let Some(endpoints) = endpoints else`
   (`:1185`) → `open_no_fleet_meta` (`:1203` → `:1940`) → `CustodianService::run_reaper_until`
   (`:1212` → `custodian.rs:545`) → `run_until(.., None, ..)` (`custodian.rs:687`) → each interval
   `reaper_pass(.., None)` (`:732` → `:508`) → `reconcile_pass` → `wyrd_custodian::reconcile_step`
   with `reconstruction = None, reaper = Some` → the reaper, then the record-only retirement leg.
   Wait `interval` (`:715-720`), repeat, until Ctrl-C.
2. **`--endpoints` given, none reachable.** `cmd_custodian` → `Custodian::elect` → `connect_fleet`
   (`cli.rs:1266`; `custodian.rs:236`) returns empty → `if configured.is_empty()` (`cli.rs:1275`)
   → `if !reconcile_after_restore` (`:1300`) → `open_no_fleet_meta` (`:1302`) →
   `reap_abandoned_uploads_once` (`:1305` → `custodian.rs:476`) → one `reaper_pass(.., None)` →
   `reconcile_step` record-only → `panic!` (`cli.rs:1314`), unchanged.
3. **Fleet (whole or degraded).** `cmd_custodian` → `connect_fleet` (non-empty) →
   `run_reconstruction_over_backend` (`cli.rs:1388`) → `run_reconstruction_until`
   (`custodian.rs:636`) → `run_until(.., Some(plane), ..)` → each interval, after the scrub pass,
   `reaper_pass(.., Some(&ctx))` (`custodian.rs:823`) → `reconcile_step` with
   `reconstruction = Some, reaper = Some`. Then GC, only if `fleet.len() == operator_fleet_size`
   (`custodian.rs:890`).

## Red → green evidence

Runner: the project's verify gate `engine/scripts/run-verify.sh`, run on the final `patch.diff`
with `PDCA_BUNDLE` = a scratch copy of the bundle (patch + brief only),
`PDCA_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, `WYRD_REPO=$PDCA_WORKTREE`, its worktree
and target dir under `$PDCA_SCRATCH`, `PDCA_LANE=b1016`.

- GREEN (fix applied): `running 8 tests` … `test result: ok. 8 passed; 0 failed` (4.16 s).
- RED (production reverted, tests kept): `running 8 tests` … `test result: FAILED. 2 passed;
  6 failed` (0.37 s). **Executed: 8, failed: 6.**
  - G2a fails on the store assertion (`multipart_reaper_deployed.rs:664`): "one run of the
    no-endpoints role did not finish leg A's teardown (it made 0 reaper pass(es); exited by
    itself: Some(ExitStatus(unix_wait_status(0))))" — stderr "no --endpoints; nothing to
    reconstruct". The role exited at once, as the brief predicts.
  - G2c fails on the store assertion (`:757`): "leg A's teardown did not converge within 6
    starts". Every start panicked (exit 101) with the diagnostic and made 0 passes. The status is
    non-zero on both trees, so the store assertion is what tells them apart, as the brief says.
  - `no_endpoints_role_refuses_a_data_dir_with_no_store` (`:820`),
    `no_endpoints_role_refuses_a_flag_only_a_fleet_uses` (`:881`),
    `a_zero_interval_is_refused_wherever_the_role_loops` (`:931`): base exits 0 at once.
  - The usage test (`:985`): the base usage text lacks the new lines.
  - **Pass on base by design:** `empty_fleet_restore_one_shot_fails_with_the_store_untouched` and
    `empty_fleet_role_creates_no_store`. On base the empty fleet panics before touching any store.
    Both guard against defects of earlier iterations (the pass under the restore one-shot; the
    creating open), proven by mutants F and G below. They are not red→green tests.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).`

### G2b does not go red on the base: the brief's claim for it is stale

Unchanged from iteration 2. 625.1 already dispatches the reaper on the reconstruction pass (base
`custodian.rs:672-683`, `Some(&reaper)` at `:680`), outside GC's gate (base `:749`). So G2b passes
on the base. It is a regression guard, pinned by negation 11 below, and `cargo xtask ci` evidence
as the brief says. The fleet-loop edit is a refactor onto the shared step and driver.

### Negations and mutants (all run this iteration, on the final tree)

Each was a scripted text edit to one production file, then the named test through
`cargo test -p wyrd-server --test <file>` under `timeout 400`, then the file restored from a
scratch copy. After the batch I grepped both files for every mutant's text: none left.

- **(11)** the fleet loop dispatches the reaper only when `fleet.len() == operator_fleet_size`
  (else a reconstruction pass with no reaper): G2b **FAILED** at `custodian_gc.rs:1809`; the other
  12 tests in the file passed.
- **(12)** the no-endpoints early exit left in place (`run_reaper_until` call replaced by dropping
  the store and the shutdown): G2a **FAILED** at `:664`, 0 passes, 0.07 s.
- **(13)** the empty-fleet pass skipped (`if !reconcile_after_restore` → `if false`): G2c
  **FAILED** at `:757`, never converged.
- **A** (the iteration-2 reviewer's mutant): the wait is `sleep(interval.saturating_sub(interval))`:
  G2a **FAILED** at `:693`, the spacing assertion, in 0.13 s.
- **E** (iteration 1's gap): with no fleet, one pass and then wait on shutdown forever: G2a
  **FAILED** at `:664` after the 120 s run bound.
- **F** (iteration 1's defect): the empty-fleet pass also under `--reconcile-after-restore`
  (`if true`): the restore guard **FAILED** at `:783`, the store changed.
- **G** (iteration 2's defect): the no-fleet redb open is the creating `open_local_meta_redb`:
  `no_endpoints_role_refuses_a_data_dir_with_no_store` **FAILED** at `:814` and
  `empty_fleet_role_creates_no_store` **FAILED** at `:845` (a `meta.redb` appeared).
- **H** `no_fleet_conflict` always `None`: the refusal test **FAILED** at `:881`.
- **J** the interval refusal without `&& !one_shot`: the zero-interval test **FAILED** at `:947`
  (the one-shot was refused).

## Refuting my own test

- **(a) Genuine red?** Yes. The verify gate reverted production and kept the tests: 8 executed,
  6 failed, G2a and G2c on the store assertion. That is not a compile error: the file names no
  symbol the patch adds and drives the binary by argv. Negations 11–13 and mutants A, E, F, G, H, J
  each fail their own test. The two tests that pass on base, and G2b, are recorded above as guards,
  each pinned by a mutant or a negation.
- **(b) Production path?** Yes. Every test in the new file runs the real built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) over a real redb store and reads the store back after the
  process is gone. G2a, G2c and the refusal/store tests run the ordinary `custodian` role with no
  one-shot flag (asserted in `role_every`, `:221-232`); only the one-shot leg of the zero-interval
  test passes `--reap-abandoned-uploads`, on purpose, through the separate `custodian` builder.
  Pass counts and their times come from the reaper's own `emit_pass` tally on the role's real log
  sink. G2b runs the real `CustodianService::run_reconstruction_until`, now a wrapper over the
  same `run_until` the no-endpoints role runs. No stand-in, no test-only entry.
- **(c) Fixture includes the fault?** Yes. G2a has no `--endpoints`. G2c names
  `http://127.0.0.1:1`, which the real gRPC dial refuses; the real `connect_fleet` returns empty
  and the role really panics on every start (exit 101, diagnostic present). G2b hands the loop 3
  servers against an operator count of 4 (`assert!(servers.len() < OPERATOR_FLEET_SIZE)`). The
  missing-store tests point at a real empty temp dir. The session fixture is leg A's.

## Choices, and what I ruled out (with costs)

- **One driver, without re-indenting the fleet body.** Moving the fleet loop's body into its own
  helper would re-indent it all: base `custodian.rs:592-794`, 203 lines removed and re-added. The
  let-else at the loop top keeps every one of those lines where it was. The cost is a
  `first_interval` flag so the one wait can sit at the top (`:711-721`).
- **No pass deadline at all, rather than "preserve progress between bounded slices".** Keeping a
  deadline would need the reaper to save its listing position page by page, which is
  `crates/custodian/src/reaper.rs` (saves only after the walk, `reaper.rs:776` per the review) —
  off-limits to this slice (child-3 owns `crates/custodian/`). Removing the deadline matches every
  other pass the role runs, and the adapters already bound each call
  (`crates/metadata-tikv/src/lib.rs:137-266`, `crates/metadata-fdb/src/lib.rs:78-89`).
- **Ctrl-C is read between passes, never during one.** That is the base fleet loop's rule, and it
  is what removes the cancellation path T5 asked DST to cover. A stop waits for the current pass
  to end, which the budgets and per-call deadlines bound.
- **Refuse, not warn, for `--gc-expired-pending` / `--ids` / `--failure-domains` without
  `--endpoints`.** Same rule as the one-shot (refused rather than ignored). A warning would be one
  line, but would leave a role running for good that silently skips what its command line asked.
- **`--interval-secs 0` refused for the fleet role too.** One driver, one rule. This is a small
  behaviour change on the fleet role (see "For the human").
- **`Box<dyn MetadataStore>` for the no-fleet open.** One open function (29 lines) instead of two
  generic per-backend dispatch functions plus a bounded-open helper (106 lines in v2). The pass
  and the loop already take `&dyn MetadataStore`. It also lets the banner print only after the
  store opened.
- Not running the no-fleet shapes through the fleet loop with an empty fleet and
  `operator_fleet_size = 0`: `0 == 0` would pass GC's gate and run GC, scrub and reconstruction
  over nothing. The `None` arm runs none of them.

## For the human

- **G2c and the brief disagree with each other** (held in `deferred-findings.json`, C1). The brief
  asks for exactly one pass on the empty-fleet startup and leg A's full transition after one run.
  Teardown takes three passes, so both cannot hold. I kept one pass per start (checked on every
  start) and reach the transition across starts, as `Restart=on-failure` would.
- **G2b's red claim in the brief is stale** (above; held in `deferred-findings.json`).
- **Behaviour changes to weigh:**
  - `wyrd custodian` with no `--endpoints` on a data dir with no redb store: base exited 0
    ("nothing to reconstruct"); it now exits 1 naming the path. With the shipped systemd unit
    (`deploy/dist/systemd/wyrd-custodian.service`, `Restart=on-failure`, `RestartSec=2`) on a redb
    host with no store, the unit now restarts until systemd's start limit, where it used to exit 0
    and stay down.
  - With a store present, the no-endpoints role on redb holds the store's exclusive lock while it
    runs, as the fleet role does (`wyrd put` on the same `--data-dir` fails until it stops). Held
    in `deferred-findings.json`.
  - `--ids`, `--failure-domains` or `--gc-expired-pending` without `--endpoints` (no one-shot): base
    ignored them and exited 0; now refused.
  - `--interval-secs 0` on the fleet role: base ran passes back to back; now refused.
- **The diff-coverage gate measures against `origin/main`.** Iterations 1 and 2 got "patch.diff
  does not apply on origin/main". `origin/main` lacks 625.1 (`427e5ec`), so no patch for this slice
  can apply there; it applies on `6e44de9`. That looks like the driver giving C4-diff-cov a
  different base than C4-verify — a harness matter, not a patch defect.
- `review-rejected.md` holds the two decisions still standing (redb's synchronous calls; DST
  coverage), with this iteration's anchors.
- #633 must be re-pointed to this slice and to child-3 after acceptance (brief, Ordering note).
- **Scratch leftovers.** My runs left directories under `$PDCA_SCRATCH`
  (`pdca-builder-1016-*`, including the verify and coverage worktrees from iterations 2 and 3)
  and branches `pdca-verify-lb1016` / `pdca-cov-lb1016` in the shared repo. Cleanup belongs to the
  harness; once it reclaims the scratch root, `git worktree prune` and
  `git branch -D pdca-verify-lb1016 pdca-cov-lb1016` remove the rest.

## Commit-readiness and `cargo xtask ci`

- `cargo fmt --all` run over the worktree; `cargo xtask ci`'s fmt check then passed.
- `cargo clippy -p wyrd-server --all-targets --all-features -- -D warnings`: clean (exit 0), so the
  TiKV and FoundationDB arms of `open_no_fleet_meta` compile. The same with default features:
  clean.
- `cargo xtask ci` through the project wrapper (`engine/xtask.sh ci`, the C4-ci gate) on the
  worktree holding exactly the final `patch.diff`: **exit 0, "xtask ci: all checks passed"**.
  Every step ran: typos, lint_docs, render_site (link audit OK, 101 pages), fmt, clippy, build,
  the workspace tests, cargo-machete, the three cargo deny runs, statics, deploy-guard, and the
  DST clippy and tests. 235 `test result: ok` lines, 0 `FAILED`. Among them
  `multipart_reaper_deployed` (8 passed, 4.14 s) and `custodian_gc` (13 passed, G2b included).

## Diff coverage, measured on the right base

I ran the project's `engine/scripts/run-diff-cov.sh` with `PDCA_BASE=6e44de9`, against the same
scratch copy of the bundle (patch + brief only), so the gate's own `coverage/diff-cov.json` was
not overwritten. Result: **PASS, 80.6%**: 129 of 160 instrumentable changed lines ran (floor
80%); 160 of 341 changed lines were instrumentable. All 31 lines that did not run are in
`custodian.rs`:

- `:648-667` and `:723-725`, `:823`: the fleet side of the driver (the `run_reconstruction_until`
  wrapper, the fleet destructuring, the fleet's reaper dispatch). G2b in `custodian_gc.rs` and
  the existing fleet-loop tests run them, but this gate runs only the test file the patch adds.
- `:737-743`: the no-fleet loop's `Store` error arm (log and retry). No test makes a no-fleet
  pass fail on the store.

Iteration 2 measured 87.5%; the drop comes from the fleet wrapper's lines, which this
iteration moved into `run_until` and which only the fleet tests reach.
