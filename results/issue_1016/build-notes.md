# Build notes — issue 1016 (625.2): the multipart reaper runs on every deployed custodian startup shape

Iteration 5. Target: getwyrd/wyrd @ main, stacked on the integration branch
`origin/pdca-integration/main-r9dd123c9ed70` (the bundle's `stack-base`), tip `6e44de9`
("pdca-integrate: issue_1015"), which is the worktree's `HEAD` and the bundle's `stack-base-tip`.
625.1 is on it (`427e5ec`, "custodian: reap multipart uploads whose client went silent").
625.1 is not on `origin/main`, so this patch cannot apply there (see "For the human", item 7).

The iteration-4 sign-off said: keep the design, fix five things, no new features. So I started
from iteration 4's `patch.diff` (it applied cleanly on `6e44de9`) and changed only what is listed
below. Line numbers are on the patched tree unless marked "base" (= `6e44de9`).

What I read besides `brief.md`: iteration 4's `patch.diff` and my own iteration-4
`build-notes.md` (to keep the design, as told), plus `review-rejected.md` and `review-batch.md`
in the bundle root. `review-batch.md` is addressed to the builder: its triage rule says every
finding in it must be fixed or recorded as declined. I did not read `SUMMARY.md`, the `check-*`
files or `deferred-findings.json`.

## The five sign-off items

| Sign-off item | What changed | Evidence |
|---|---|---|
| **Shutdown priority.** A ready stop must win over a ready timer. | The loop's wait is `biased`, the stop first: `custodian.rs:739-743` (one added keyword, plus the comment at `:735-738` and the doc at `:686-688`). | New driver test `a_stop_ready_with_the_interval_timer_starts_no_further_pass` (`custodian_gc.rs:2254`). Without `biased` it fails in round 0 (4 runs out of 4, both shapes seen): "left: 2, right: 1" passes. |
| **Fault model.** `FlakyMeta::commit` answers `CommitUnknownResult { may_still_commit: false, .. }`; assert both draws; seeds still converge. | `custodian_gc.rs:2024-2046`. A failed commit is sent on or not (drawn), counted as applied only when the store answered `Committed`, and answered with the seam's typed error either way. | `no_fleet_loop_rides_out_seeded_store_faults_to_leg_a_teardown` (`:2063`): all 32 seeds reach leg A's teardown and return `Ok`; over the seeds 11 failed commits were applied and 6 were not (`:2120`); the reaper raised its `reaper-fence-outcome-unknown` signal 10 times (`:2125`). Same three numbers in three runs. |
| **Scope.** Refuse `--interval-secs 0` only with no `--endpoints`. | `cli.rs:1054-1075`: the refusal sits inside the no-endpoints branch. With `--endpoints` the flag is taken as on base. | `a_zero_interval_is_refused_with_no_endpoints_and_only_there` (`multipart_reaper_deployed.rs:1210`, renamed): no `--endpoints` at 0 is refused with the store untouched; the empty fleet at 0 is not refused, makes its one pass, and exits 101 with the unchanged diagnostic; the one-shot takes 0. Mutants Z1 and Z2 below. |
| **Usage text.** Say that no-endpoints on redb holds the data-dir lock. | `cli.rs:517-526`. Same sentence in the `cmd_custodian` doc (`:971-973`) and in `06-runtime-view.md:94`. | `usage_says_what_the_role_does_with_no_fleet` (`:1287`) pins the text. G2a now also checks the fact: while the role is up, a second `wyrd` on the same data dir exits non-zero with redb's "Database already open" and runs no pass (`:835-857`). |
| **Records.** G2b is a regression guard only. | No code. See "G2b" below: it passes on the base, and I claim no fix for the degraded fleet. | — |

Also tightened, because the Scope item says "panic message and exit 101 unchanged": G2c now
checks that every start exits with status 101 exactly, where before it checked "non-zero"
(`multipart_reaper_deployed.rs:1026-1034`).

## Two changes beyond the sign-off list

Both are test-only. I made them because leaving them out would have cost a round or made the
suite less reliable. If you disagree, each is easy to drop.

### 1. The first-pass Ctrl-C test no longer depends on timing

`review-batch.md` holds three groups of blocking findings. The sign-off list covers two of them.
The third (3 findings, at `multipart_reaper_deployed.rs:801` and `:387`) is not in the list and not
among the "decided" items: the test read "the Ctrl-C landed inside the first pass" off how many
lines the *test* had consumed, so a role that had already finished the pass could pass the check.
The gate blocks on any finding that is neither fixed nor declined, and the finding is right (the
rubric names this class: "a count-based assertion that can pass while the property fails"). So I
fixed it.

How it works now (`interrupt_frozen`, `:388`; the test, `:934`):

1. When the first fence line is read, the test sends the role SIGSTOP.
2. It waits until every thread of the role shows state `T` in `/proc/<pid>/task/*/stat` (`:359`).
3. It writes a mark line into the role's own stderr pipe. The test now creates that pipe itself
   (`std::io::pipe`, `:430`) so it holds a write end. A stopped role writes nothing, so every role
   line before the mark was written before the stop, and every line after it after the continue.
4. It sends SIGINT (which stays pending on a stopped process), then SIGCONT.

The test then reads where the role stood from the lines before the mark: at least one fence and
no pass tally means the SIGINT was delivered inside the first pass. If the role had already
written its tally, that run proves nothing; it is set aside and the test runs again on a fresh
store, up to 5 times, and fails if no run stopped the role inside the pass. A run is set aside on
its stderr only, never on its outcome.

Measured: in 12 runs out of 12 the role was stopped after exactly one fence with no tally, on the
first attempt. With the fix removed (mutant K), 5 runs out of 5 fail with the role killed by
signal 2, each with "stopped after 1 fence(s)".

Cost: about 90 added lines in the test file. The test is Linux-only (it reads `/proc`); elsewhere
it is reported as ignored with that reason, not silently skipped. CI runs on Ubuntu only.

What I ruled out:

- **Declining the finding** (one row in `review-rejected.md`, 0 code lines). The only reason I
  could give was "the sign-off did not list it", which is not a reason the finding is wrong.
- **Holding the role with a full stderr pipe instead of SIGSTOP.** The first pass writes about
  48 lines × 300 bytes ≈ 15 KB; a Linux pipe holds 64 KB, and the reaper lists at most 64
  uploads a pass (`SESSIONS_PER_PASS`), so the pipe cannot be filled from one pass.
- **Shrinking the pipe** (`F_SETPIPE_SZ`): needs `libc` and `unsafe`, which the file forbids.

### 2. A guard against a `tracing` deadlock in the `custodian_gc` test binary

While running the whole server suite I hit a hang in `custodian_day_one`, a file this patch does
not touch. It is a deadlock in the libraries, and it exists on the base:

- `tracing-core` 0.1.36 holds its dispatcher-list lock, with a temporary reference to every live
  dispatch, while it registers a dispatch or a callsite (`callsite.rs`, `register_dispatch` and
  `Rebuilder::for_each`, line 572).
- If another test drops its role at that moment, the temporary reference is the last one, and the
  role's meter provider is dropped under the lock.
- `opentelemetry_sdk` logs through `tracing` in that drop (`meter_provider.rs:166`). The first
  time, the log line registers its callsite, which needs the same lock. The thread waits on a
  lock it holds, and every other test of the binary queues behind it.

Stack of the stuck thread (gdb, `custodian_day_one`): `cli::run` → `logging::init_global` →
`Dispatch::new` → `register_dispatch` → `rebuild_interest` → `Rebuilder::for_each` →
`drop_in_place<Dispatch>` → `SdkMeterProviderInner::drop` → `DefaultCallsite::register` →
`Dispatchers::rebuilder` → `RwLock::read_contended`. Fourteen other threads wait in
`register_dispatch` for the write lock. The dump is in `$PDCA_SCRATCH/pdca-builder-1016-i5/gdb-d1-run.txt`.

Hang rates I measured, running each test binary directly under `timeout 30`:

| Binary | Tree | Hangs |
|---|---|---|
| `custodian_day_one` | base `6e44de9` | 3 of 300 |
| `custodian_day_one` | patched (file untouched) | 4 of 150 |
| `custodian_gc` | base `6e44de9` | 1 of 300 |
| `custodian_gc` | patched, tests as first written this iteration | 12 of 300 |
| `custodian_gc` | patched, one role per test instead of one per round | 7 of 300 |
| `custodian_gc` | patched, with the guard | **0 of 500** |

So the hang is not new, but the driver tests this slice adds made it about ten times likelier in
`custodian_gc` (more tests building and dropping a role as the binary starts). That is this
slice's doing, so I fixed it there:

- The two looping tests build one role each, not one per seed or round (`:2066-2075`, `:2259-2267`).
- `register_meter_drop_callsites` (`custodian_gc.rs:1250`), called from the existing
  `enable_log_callsites` once (`:1231`): it drops one meter provider on the calling thread, outside
  any dispatch, before the test builds a role. That registers the SDK's drop-path callsites with
  no lock held. After that, a provider dropped under the lock takes no lock. Every test this slice
  adds calls `enable_log_callsites` first.

I did not touch `custodian_day_one.rs`: this slice does not own it. See "For the human", item 3.

## What changed, file by file (whole patch)

### `crates/server/src/custodian.rs`

- `FleetPlane` (`:324`, private, `Copy`): the fleet inputs of the loop.
- `reap_abandoned_uploads_once` (`:476`) calls `reaper_pass(.., None)` (`:488`).
- `reaper_pass` (`:508`, private): the reaper's one step, one fenced `reconcile_pass` with the
  reconstruction context when there is a fleet.
- `run_reaper_until` (`:545`, pub): `run_until(.., None, ..)`.
- `run_reconstruction_until` (`:636`): signature unchanged; body is `run_until(.., Some(plane), ..)`.
- `run_until` (`:694`, private): the one interval driver. The stop is polled once before the
  first pass (`:714-717`) and honoured at the first wait if it was already done (`:732-734`).
  **New this iteration:** the wait is `biased`, the stop first (`:739-743`). The no-fleet arm
  (`:746-770`): `reaper_pass(.., None)`, `Fenced` stops, `Store` is logged and retried. The fleet's
  reaper dispatch `reaper_pass(.., Some(&ctx))` (`:847`). GC's gate (`:914`) unchanged.

### `crates/server/src/cli.rs`

- `usage()` (`:515-526`) and the `cmd_custodian` doc (`:964-975`). **New:** the lock sentence, and
  the zero interval named as a no-endpoints refusal.
- Refusals before the runtime starts (`:1050-1075`). **New:** both live in the no-endpoints
  branch; the zero-interval one no longer applies with `--endpoints`.
- No `--endpoints` (`:1193-1223`): `open_no_fleet_meta` (`:1211`), banner, `run_reaper_until`
  (`:1220`) until Ctrl-C, then success.
- Empty fleet (`:1283-1330`): one pass when `!reconcile_after_restore` (`:1311`), errors logged,
  then the unchanged `panic!` (`:1325`).
- `NO_FLEET_EXCLUDES` / `no_fleet_conflict` (`:1816`, `:1833`), `open_no_fleet_meta` (`:1951`),
  `open_existing_local_meta_redb_for` (`:2332`). Unchanged since iteration 4.

### `docs/design/architecture/06-runtime-view.md`

Line 94, 625.1's reaper paragraph. **New:** 0 is refused only with no `--endpoints`; a stop that
is there when the interval runs out wins; on redb the no-endpoints role holds the store's lock.

### Tests

- `crates/server/tests/multipart_reaper_deployed.rs` (NEW, not `#![cfg]`-gated), nine tests:
  G2a (`:831`), the first pass (`:934`), G2c (`:1017`), the restore guard (`:1074`), the two
  no-store tests (`:1109`, `:1140`), the flag refusals (`:1170`), the zero interval (`:1210`), the
  usage (`:1287`). New helpers: `signal` (`:340`), `every_thread_stopped` (`:359`),
  `interrupt_frozen` (`:388`), `run_role_and` (`:427`), `empty_fleet_role_every` (`:299`).
- `crates/server/tests/custodian_gc.rs` (APPENDED, plus 2 lines in `enable_log_callsites`):
  `register_meter_drop_callsites` (`:1250`); `leg_a_records` (`:1709`); `LegA` (`:1785`); G2b
  (`:1866`); `FlakyMeta` (`:1940`); the seeded test (`:2063`); the fenced test (`:2136`); the
  stop-before-first-pass test (`:2185`); the both-ready test (`:2254`).

## Call path from `cmd_custodian` to the reaper, per shape

1. **No `--endpoints`.** `cmd_custodian` (`cli.rs:982`) → refusals (`:1054-1075`) →
   `Custodian::elect` → `let Some(endpoints) = endpoints else` (`:1193`) → `open_no_fleet_meta`
   (`:1211` → `:1951`) → `CustodianService::run_reaper_until` (`:1220` → `custodian.rs:545`) →
   `run_until(.., None, ..)` (`custodian.rs:694`) → stop polled, Ctrl-C handler installed
   (`:714`) → each interval `reaper_pass(.., None)` (`:756` → `:508`) → `reconcile_pass` →
   `wyrd_custodian::reconcile_step` with `reconstruction = None, reaper = Some` → the reaper, then
   the record-only retirement leg. Wait `interval`, the stop first (`:739-743`), repeat, until
   Ctrl-C.
2. **`--endpoints` given, none reachable.** `cmd_custodian` → `Custodian::elect` → `connect_fleet`
   (`cli.rs:1274`) returns empty → `if configured.is_empty()` (`:1283`) →
   `if !reconcile_after_restore` (`:1311`) → `open_no_fleet_meta` (`:1313`) →
   `reap_abandoned_uploads_once` (`:1315` → `custodian.rs:476`) → one `reaper_pass(.., None)` →
   `reconcile_step` record-only → `panic!` (`cli.rs:1325`), unchanged.
3. **Fleet (whole or degraded).** `cmd_custodian` → `connect_fleet` (non-empty) →
   `run_reconstruction_over_backend` (`cli.rs:1399`) → `run_reconstruction_until`
   (`custodian.rs:636`) → `run_until(.., Some(plane), ..)` → stop polled (`:714`) → each interval,
   after the scrub pass, `reaper_pass(.., Some(&ctx))` (`:847`) → `reconcile_step` with
   `reconstruction = Some, reaper = Some`. Then GC, only if `fleet.len() == operator_fleet_size`
   (`:914`).

## Red → green evidence

Runner: the project's verify gate `engine/scripts/run-verify.sh`, on the final `patch.diff`, with
`PDCA_BUNDLE` = a scratch copy of the bundle (patch + brief only),
`PDCA_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, `WYRD_REPO=$PDCA_WORKTREE`, its worktree
under `$PDCA_SCRATCH`, `PDCA_LANE=b1016`. Base commit: `6e44de9`; 625.1 is on it.

- GREEN (fix applied): `running 9 tests` … `test result: ok. 9 passed; 0 failed` (4.22 s).
- RED (production reverted, tests kept): `running 9 tests` … `test result: FAILED. 2 passed;
  7 failed` (0.37 s). **Executed: 9, failed: 7.**
  - G2a fails on the store assertion (`:865`): 0 passes, the role exited 0 by itself.
  - The first-pass test fails at `:944`: the base role fences nothing before it exits.
  - G2c fails on the store assertion (`:1058`): no convergence in 6 starts. Every start exits 101
    on both trees, so the store assertion is what tells them apart, as the brief says.
  - The data-dir refusal (`:1121`), the flag refusals (`:1182`), the zero interval (`:1217`), the
    usage (`:1306`).
  - **Pass on base by design:** `empty_fleet_restore_one_shot_fails_with_the_store_untouched` and
    `empty_fleet_role_creates_no_store` (on base the empty fleet panics before touching any
    store). Each guards a defect of an earlier iteration.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).`

### G2b is a regression guard, not a red→green

G2b (`custodian_gc.rs:1866`) passes on the base. 625.1 already dispatches the reaper on the
reconstruction pass (base `custodian.rs:672-683`), outside GC's gate (base `:749`). So this slice
does not fix a degraded-fleet defect, and the brief's line "on a partially degraded fleet it
reaps nothing while GC is deferred" does not hold on this base. What G2b does is keep that
behaviour from regressing; negation 11 shows it bites. It is `cargo xtask ci` evidence, as the
brief says.

### Negations and mutants (all run on the final tree)

Each was a scripted text edit to one production file, then the named test through
`cargo test -p wyrd-server --test <file> -- <name>` under `timeout 900`, then the file restored
from a copy and checked with `cmp`. All ten went red.

| # | Edit | Test that failed |
|---|---|---|
| **11** | Fleet loop dispatches the reaper only when `fleet.len() == operator_fleet_size` | G2b, at `custodian_gc.rs:1828` |
| **12** | No-endpoints early exit left in place (`run_reaper_until` → `drop((meta, shutdown))`) | G2a, at `:865` |
| **13** | Empty-fleet pass skipped (`if !reconcile_after_restore` → `if false`) | G2c, at `:1058` |
| P | `biased;` removed from the wait | both-ready test, at `custodian_gc.rs:2333` |
| K | Stop not polled before the first pass | first-pass test, at `:965`: killed by signal 2 |
| A | Wait is `sleep(interval.saturating_sub(interval))` | G2a, at `:894` (spacing) |
| L | No-fleet `Store` arm returns the error | seeded test, at `custodian_gc.rs:2103` |
| N | A stop already done at the first poll not honoured | stop-before-first-pass test, at `custodian_gc.rs:2201` |
| Z1 | Zero-interval refusal applied to every looping shape | zero-interval test, at `:1238` |
| Z2 | Zero-interval refusal removed | zero-interval test, at `:1217` (the role ran to the 120 s bound) |

## Refuting my own test

- **(a) Genuine red?** Yes. The verify gate reverted production and kept the tests: 9 executed, 7
  failed, all on assertions. The test file names no symbol the patch adds and drives the binary
  by argv, so it builds on the base. Negations 11–13 and the seven mutants each fail their own
  test.
- **(b) Production path?** Yes. The subprocess tests run the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`), the ordinary `custodian` role with no one-shot flag (asserted in
  `role_every`, `:263`), over a real redb store, with real signals sent through `kill`. The pass
  tallies and fence lines they count are the reaper's own log lines. The in-process tests drive
  the real `run_reaper_until` / `run_reconstruction_until`, which are the `run_until` the binary
  runs. `FlakyMeta` is a fault-injecting store under the real driver, not a stand-in for it, and
  it now answers the same typed error a real backend answers.
- **(c) Fixture includes the fault?** Yes, and for the two timing cases the fixture checks
  itself:
  - First pass: the SIGINT is sent while the role is stopped, and the test proves from the role's
    own stderr that it was stopped before the pass's tally.
  - Both-ready: the stop records the paused clock at each poll, and the test asserts the three
    polls it must show (`custodian_gc.rs:2325`): before the first pass, at the start of the wait
    with the clock unmoved, and at the shared deadline.
  - Seeded faults: at least one fault on every seed, both commit draws over the seeds, and at
    least one unknown-outcome fence taken by the reaper.
  - G2c's endpoint is refused by the real gRPC dial; G2b hands the loop 3 servers against an
    operator count of 4.

## For the human

1. **"store-creating open" — check my reading.** The carry-forward lists it under "Decided at
   sign-off (do not reopen)" with no detail. I read it as: the question is closed and the patch
   stays as it is. So the two shapes without a fleet still open the store with a non-creating
   open (`open_no_fleet_meta`, `cli.rs:1951`), the fleet role still uses the creating open it has
   on base, and the two no-store tests are unchanged. If you meant "switch the no-fleet shapes to
   a store-creating open", that is a change I did not make: it would turn
   `no_endpoints_role_refuses_a_data_dir_with_no_store` and `empty_fleet_role_creates_no_store`
   around, and the usage sentence "must already exist" with them.
2. **Two changes go beyond your five-item list** (both test-only, both explained above): the
   first-pass Ctrl-C test, and the `tracing` deadlock guard in `custodian_gc.rs`.
3. **The `custodian_day_one` test binary hangs about 1 run in 100 on the base** (3 of 300; 4 of
   150 on the patched tree, the file untouched). It is the same library deadlock described above.
   When it happens, `cargo xtask ci` sits until its timeout. I did not fix it here: the file is
   outside this slice. A follow-up issue could say: "`custodian_day_one` (and any test binary whose
   tests build and drop a `CustodianService` in parallel) can deadlock in `tracing-core` 0.1.36:
   `Rebuilder::for_each` drops the last reference to another test's dispatch under the
   dispatcher-list lock, and `SdkMeterProviderInner::drop` then registers a callsite, which needs
   that lock. Need: these binaries never hang." The guard in `custodian_gc.rs:1250` is one way.
4. **`--interval-secs 0` with a reachable fleet has no subprocess test.** That shape needs a live
   D server as a subprocess, which no test here has. What I can show: the refusal's condition is
   `endpoints.is_none()` (`cli.rs:1054`), the empty-fleet case at 0 is tested end to end, and
   mutant Z1 shows the test catches a refusal that reaches the `--endpoints` shapes.
5. **G2c and the brief disagree with each other**, and you settled it: one pass per start,
   convergence across starts. G2c checks every start for exactly one pass.
6. **Behaviour changes to weigh**, all unchanged since iteration 4 except the first:
   - New this iteration: `--interval-secs 0` is refused only with no `--endpoints`. With
     `--endpoints` it is taken, as on base.
   - `wyrd custodian` with no `--endpoints` on a data dir with no redb store exits 1 naming the
     path (base exited 0). With a store, it holds redb's exclusive lock while it runs; the usage
     text and the architecture doc now say so.
   - `--ids`, `--failure-domains` or `--gc-expired-pending` without `--endpoints` are refused.
   - A Ctrl-C during the fleet role's first pass lets that pass finish and exits 0 (base: killed
     by the signal).
7. **The diff-coverage gate measures against `origin/main`.** "patch.diff does not apply on
   origin/main" will show again: `origin/main` lacks 625.1 (`427e5ec`), so no patch for this slice
   can apply there; it applies on `6e44de9`. That looks like the driver giving C4-diff-cov a
   different base than C4-verify. It is a harness matter, not a patch defect.
8. `review-rejected.md` holds three declined items with this iteration's line numbers: redb's
   synchronous calls, Tier-0 DST (your "no DST rows here"), and restart-driven convergence.
9. #633 must be re-pointed to this slice and to child-3 after acceptance (brief, Ordering note).
10. **Scratch leftovers.** My runs left files under `$PDCA_SCRATCH/pdca-builder-1016-i5/` (logs,
    the gdb dump, the mutant script), a base build in `$PDCA_SCRATCH/pdca-builder-1016-verify/`,
    and many `.tmp*` test directories from test runs I killed on purpose (the hang measurements).
    Earlier iterations left `pdca-builder-1016-*` too. I removed nothing: cleanup belongs to the
    harness. Once it reclaims the scratch root, `git worktree prune` and
    `git branch -D pdca-verify-lb1016 pdca-cov-lb1016` remove the rest.

## Commit-readiness and `cargo xtask ci`

- `cargo fmt --all` run over the worktree; `cargo fmt --all --check` clean.
- `cargo clippy -p wyrd-server --all-targets -- -D warnings`: clean. The same with
  `--all-features`: clean, so the TiKV, FoundationDB and etcd arms compile.
- `cargo xtask ci` through the project wrapper (`engine/xtask.sh ci`, the C4-ci gate) on the
  worktree holding exactly the final `patch.diff` (checked with `git diff | cmp - patch.diff`
  before and after): **rc 0, "xtask ci: all checks passed"**. Every step ran: typos, lint_docs,
  render_site (101 pages, link audit OK), the gitlink, unsafe and blackbox guards, fmt, clippy,
  build, the workspace tests, cargo-machete, the three cargo deny runs, statics, deploy-guard,
  and the DST clippy and tests. 235 `test result: ok` lines, 0 `FAILED`. Among them
  `custodian_gc` (17 passed), `multipart_reaper_deployed` (9 passed, 4.22 s) and
  `custodian_day_one` (15 passed; this is the binary that can hang, item 3 above — it did not in
  this run).
- One earlier run of the plain `cargo test -p wyrd-server` did hang, in `custodian_day_one`.
  That is how I found the deadlock. I killed that test process and measured the rates above.
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed; both ran
  inside `cargo xtask ci`.

## Diff coverage, measured on the right base

`engine/scripts/run-diff-cov.sh` with `PDCA_BASE=6e44de9`, against a scratch copy of the bundle
(patch + brief only), so the gate's own `coverage/diff-cov.json` was not overwritten: **PASS,
81.1%** — 137 of 169 instrumentable changed lines ran (floor 80%); 169 of 376 changed lines were
instrumentable. This gate runs only the test file the patch adds. All 32 lines it did not see
are in `custodian.rs`, and each is run by a `custodian_gc.rs` test:

- `:648-667`, `:747-749`, `:847`: the fleet side of the driver (the `run_reconstruction_until`
  wrapper, the fleet destructuring, the fleet's reaper dispatch) — G2b and every fleet-loop test.
- `:733`: the return when the stop was already done at the first poll —
  `fleet_loop_polls_its_stop_before_its_first_pass` (mutant N).
- `:761-767`: the no-fleet loop's `Fenced` and `Store` arms —
  `no_fleet_loop_stops_when_a_newer_term_holds_the_zone` and the seeded fault test (mutant L).

The line this iteration adds to the driver, the `biased` wait (`:739-743`), is run by G2a in the
added test file and pinned by the both-ready test in `custodian_gc.rs` (mutant P).
