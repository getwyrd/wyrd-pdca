# Build notes — issue 1016 (625.2): the multipart reaper runs on every deployed custodian startup shape

Iteration 2. Target: getwyrd/wyrd @ main, stacked on the integration branch
`origin/pdca-integration/main-r9dd123c9ed70` (the bundle's `stack-base`), tip `6e44de9`
("pdca-integrate: issue_1015"). 625.1 is on it: `427e5ec` ("custodian: reap multipart uploads whose
client went silent") is an ancestor of `6e44de9`. 625.1 is **not** on `origin/main`, so this patch
cannot apply there (see "For the human" and "Diff coverage, measured on the right base" below).

Line numbers are on the patched tree unless marked "base" (= `6e44de9`).

## What iteration 1 got wrong, and what changed

| Carry-forward finding | Fix in this iteration |
|---|---|
| The empty-fleet reaper pass also ran under `--reconcile-after-restore`, changing a just-restored store before the command failed. | The pass is skipped when `reconcile_after_restore` is set (`cli.rs:1273`, reasons at `:1267-1272`). New test `empty_fleet_restore_one_shot_fails_with_the_store_untouched` (`multipart_reaper_deployed.rs:721`) asserts the store is byte-identical, the exit is non-zero, the diagnostic is printed, and no pass ran. Mutant M3 (guard removed) fails it. |
| G2a restarted the role up to six times, so a role that reaped once and then idled passed. | G2a now runs the role **once**, never restarted (`multipart_reaper_deployed.rs:616`). It reads stderr line by line and, after the fifth `monotonic_counter.reaper_passes` line (`PASSES_IN_ONE_RUN`, `:82`), sends the role Ctrl-C (SIGINT, `interrupt`, `:249`). It asserts the teardown finished, the role was still up when interrupted, five passes came from that one process, and the Ctrl-C ended it with exit 0 and "shutting down" (`:648`). Mutant M1 (one pass, then wait on shutdown forever) and mutant M4 (the loop ignores Ctrl-C) each fail it. |
| G2c never checked "exactly one pass per start". | Every start must emit exactly one reaper-pass tally (`:684`), and the first start must have moved the session off its seeded `Open` record (`:690`). Mutant M2 (several passes before the panic) fails it. The restart loop stays, and the test doc says why (`:26-33`): one pass per start cannot finish a three-pass teardown, so the brief's "one run, then all three asserts" cannot hold for the store assert. That conflict is the human's call (see "For the human"). |
| The subprocess tests inherited `WYRD_METADATA_BACKEND`. | `role()` passes `--metadata-backend redb` and removes `WYRD_METADATA_BACKEND` from the child's environment (`:204`, `:208`). It also pins `--log-level info` (`:205`) so a caller's `RUST_LOG` cannot hide the pass tallies the tests count. |
| The 60 s bound does nothing on redb. | Documented, not "fixed": the bound is stated to cover the networked backends only, with the reason (`custodian.rs:146-150`; `cli.rs:1264-1266`; `custodian.rs:557-559`). Rejected as a code change in `review-rejected.md`, decision 1 (cost below). |
| The no-endpoints loop awaited each pass with no deadline and checked shutdown only after it (Ctrl-C blocked). | Each pass is now raced against `shutdown` (`biased`, shutdown first) and wrapped in `tokio::time::timeout(NO_FLEET_REAPER_PASS_BOUND, ..)` (`custodian.rs:583-605`). A timed-out pass is logged and the next interval runs a new one. |
| The no-endpoints TiKV open had no deadline. | `open_within_no_fleet_bound` (`cli.rs:1963`) bounds the TiKV/FDB open by the same constant and fails the role (non-zero, so a supervisor restarts it) if it elapses. The empty-fleet pass's open is already inside that pass's timeout (`cli.rs:1274`). |
| No seeded Tier-0 DST coverage. | Declined: the brief forbids edits under `crates/dst/` (child-3 owns it). Recorded in `review-rejected.md`, decision 2. |
| C4 diff coverage: "patch.diff does not apply on origin/main". | Not fixable in the patch (see below). |

Iteration 1's empty-fleet pass reused the interval loop with an already-ready shutdown future. That
no longer works, because the loop now checks shutdown *before* each pass. The empty-fleet path
therefore calls the single-pass entry `reap_abandoned_uploads_once` (`custodian.rs:482`) through a
new backend dispatch, `run_reaper_pass_over_backend` (`cli.rs:1923`), instead.

## What changed, file by file

### `crates/server/src/custodian.rs`

- `NO_FLEET_REAPER_PASS_BOUND` (`:151`, 60 s, `pub(crate)`) bounds a no-fleet pass. Its doc
  (`:135-150`) says what it does and does not bound.
- `reaper_pass` (`:514`, private) is **the one reaper interval step**, as the brief's Scope asks.
  It builds the `ReaperContext` (the caller's one `LoopClock`, `clock_source = wall`,
  `ReaperPolicy::default()`) and runs one fenced `reconcile_pass` with the reaper, plus the
  reconstruction context when there is a fleet. Every shape dispatches the reaper through it:
  - the fleet loop: `reaper_pass(.., Some(&ctx))` (`:791`). This replaces 625.1's inline
    `ReaperContext` and `reconcile_pass` (base `:634-639`, `:672-683`). Same call into
    `reconcile_step`, same error handling. It sits outside GC's gate (`:858`), which is unchanged;
  - the one-shot and the empty-fleet pass: `reap_abandoned_uploads_once` (`:482`) now calls
    `reaper_pass(.., None)`. Its behaviour is unchanged, and its doc now names the empty-fleet pass;
  - the no-fleet loop `run_reaper_until` (`:566`, new, pub): `reaper_pass(.., None)` every
    interval, raced against shutdown and bounded (`:583-605`); `Fenced` stops the loop, `Store` is
    logged and retried, a timeout is logged and retried; then a `biased` select of shutdown against
    `sleep(interval)` (`:609-613`).

### `crates/server/src/cli.rs`

- `usage()` (`:518-524`) and the `cmd_custodian` doc (`:961-967`) state the new behaviour (docs
  currency for a CLI behaviour change; no new flag).
- **No `--endpoints`** (`:1173-1187`): instead of printing "nothing to reconstruct" and returning
  success (base `:1158-1162`), the role says it runs the reaper alone and calls
  `run_reaper_over_backend(.., shutdown)` (Ctrl-C), then returns success. The
  `--reconcile-after-restore` refusal on that branch (`:1162`) is untouched and still comes first.
- **Empty fleet** (`:1256-1291`): after `connect_fleet` comes back empty (`:1249`) and before the
  unchanged `panic!` (`:1292`), and only when `!reconcile_after_restore` (`:1273`), there is one
  call to `run_reaper_pass_over_backend` wrapped in `tokio::time::timeout(NO_FLEET_REAPER_PASS_BOUND, ..)`
  (`:1274-1290`). Any error or timeout is logged, never returned, so the panic message and the exit
  status (101) do not change. The pass runs under the leadership from `Custodian::elect`
  (`:1072`), which runs before the endpoints check.
- `run_reaper_over_backend` (`:1878`, new, pub) opens the store per backend for the no-fleet loop,
  mirroring `run_reconstruction_over_backend`. On redb it uses the same `open_local_meta_redb`
  (`:2307`) the fleet role uses; on TiKV/FDB it uses a bounded open.
- `run_reaper_pass_over_backend` (`:1923`, new, pub) is the same dispatch for the single
  empty-fleet pass.
- `open_within_no_fleet_bound` (`:1963`, `cfg(any(tikv, fdb))`) is the bounded networked open.

### `docs/design/architecture/06-runtime-view.md`

Appended to 625.1's reaper paragraph (line 94): the three startup shapes, the per-pass bound, and
that the restore one-shot runs no such pass.

### Tests

- NEW `crates/server/tests/multipart_reaper_deployed.rs`, not `#![cfg]`-gated. Leg A's helpers
  are copied from `multipart_reaper.rs`, not shared. Four tests: G2a (`:616`), G2c (`:667`), the
  restore guard (`:721`), and `usage_says_what_the_role_does_with_no_fleet` (`:756`), which pins
  the three new usage lines (the CLI half of docs currency).
- APPENDED to `crates/server/tests/custodian_gc.rs`: `leg_a_records` (`:1677`) and G2b,
  `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`:1753`). It hands
  three servers to the real `run_reconstruction_until` (through the file's `drive_passes_at`,
  `:1373`, the same driver shape as `drive_deployed_loop_operator`, `:496`) with
  `operator_fleet_size = 4`, so GC is deferred on every pass. The file's module doc lists it.

## Call path from `cmd_custodian` to the reaper, per shape

1. **No `--endpoints`.** `cmd_custodian` (`cli.rs:975`) → `Custodian::elect` (`:1072`) →
   `let Some(endpoints) = endpoints else` (`:1161`) → `run_reaper_over_backend` (`:1184` →
   `:1878`) → `CustodianService::run_reaper_until` (`custodian.rs:566`) → `reaper_pass(.., None)`
   (`custodian.rs:514`) → `reconcile_pass` → `wyrd_custodian::reconcile_step` with
   `reconstruction = None, reaper = Some` → the reaper, then the record-only retirement leg. This
   repeats every interval until Ctrl-C.
2. **`--endpoints` given, none reachable.** `cmd_custodian` → `Custodian::elect` (`:1072`) →
   `connect_fleet` (`cli.rs:1240`; `custodian.rs:254`) returns empty → `if configured.is_empty()`
   (`cli.rs:1249`) → `if !reconcile_after_restore` (`:1273`) → `timeout(.., run_reaper_pass_over_backend(..))`
   (`:1274` → `:1923`) → `reap_abandoned_uploads_once` (`custodian.rs:482`) → one
   `reaper_pass(.., None)` → `reconcile_step` record-only → `panic!` (`cli.rs:1292`), unchanged.
3. **Fleet (whole or degraded).** `cmd_custodian` → `connect_fleet` (non-empty) →
   `run_reconstruction_over_backend` (`cli.rs:1367`) → `run_reconstruction_until`
   (`custodian.rs:688`) → each interval, after the scrub pass, `reaper_pass(.., Some(&ctx))`
   (`custodian.rs:791`) → `reconcile_step` with `reconstruction = Some, reaper = Some`. Then the GC
   pass, but only if `fleet.len() == operator_fleet_size` (`custodian.rs:858`).

## Red → green evidence

Runner: the project's verify gate `engine/scripts/run-verify.sh` with
`PDCA_BASE=6e44de924cc8a451a71ce7f7ed63aca76c67f509`, `WYRD_REPO=$PDCA_WORKTREE`, its worktree and
target dir under `$PDCA_SCRATCH`, and `PDCA_LANE=b1016`, so it could not touch another lane's
checkout. It ran on the final `patch.diff`.

- GREEN (fix applied): `running 4 tests` … `test result: ok. 4 passed; 0 failed` (4.15 s).
- RED (production reverted, tests kept): `running 4 tests` … `test result: FAILED. 1 passed;
  3 failed` (0.35 s). **Executed: 4, failed: 3.**
  - G2a fails on the store assertion (`multipart_reaper_deployed.rs:623`): "one run of the
    no-endpoints role did not finish leg A's teardown (it made 0 reaper pass(es) …)". Stderr shows
    "no --endpoints; nothing to reconstruct": the role exited at once with status 0, as the brief
    predicts.
  - G2c fails on the store assertion (`:705`): "leg A's teardown did not converge within 6 starts".
    Every start panicked (exit 101) with the diagnostic and made 0 passes. The status is non-zero on
    both trees, so the store assertion is what tells them apart, as the brief says.
  - The usage test fails (`:770`): the base usage text does not have the new lines.
  - The restore guard **passes on base by design**: on base the command also panics without
    touching the store. It guards against iteration 1's defect (mutant M3), so it is not a
    red→green test.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).`

An earlier run of the gate, before the Ctrl-C change to G2a and before the usage test was added,
gave GREEN 3/3 and RED 3 executed / 2 failed, on the same production code.

Teardown takes three passes, measured. G2c with a temporary print converged after exactly 3
starts, one pass each. So G2a waiting for 5 pass lines means at least 4 passes ran to the end, one
to spare. G2a took about 4 s.

### G2b does not go red on the base: the brief's claim for it is stale

With only `cli.rs` and `custodian.rs` reverted to base and the new G2b kept, G2b **passes**
(`1 passed`). 625.1 already dispatches the reaper on the reconstruction pass (base
`custodian.rs:672-683`, `Some(&reaper)` at `:680`), and that pass is not behind GC's gate (base
`custodian.rs:749`). So the brief's Defect bullet ("That call sits behind GC's whole-fleet gate…
`custodian.rs:680-713`") and its Falsifiability line for G2b do not hold on this base. G2b is a
**regression guard**, pinned by negation 11 below. It is `cargo xtask ci` evidence, not C4, as the
brief already says. The fleet-loop edit (`custodian.rs:791`) is the refactor onto the shared
interval step and changes no behaviour.

### Negations and mutants

Each was a temporary edit made by a script, run, then restored from a scratch copy. The line
numbers are from the runs against the final test file.

- **(11)** reaper dispatched only inside GC's whole-fleet branch (the fleet loop calls
  `reaper_pass` only when `fleet.len() == operator_fleet_size`, else a reconstruction pass with no
  reaper): G2b **FAILED** at `custodian_gc.rs:1809`; the other 12 tests in that file, the
  whole-fleet reaper test among them, stayed ok.
- **(12)** no-endpoints early exit left in place (the `run_reaper_over_backend` call removed): G2a
  **FAILED** (`multipart_reaper_deployed.rs:623`, 0 passes); the other three ok.
- **(13)** the empty-fleet reaper pass skipped: G2c **FAILED** (`:705`, never converged); the other
  three ok.
- **M1** (iteration 1's G2a gap: one pass at startup, then wait on shutdown forever): G2a
  **FAILED** (`:623`) after the 120 s run bound, with too few passes and the teardown unfinished.
- **M2** (iteration 1's G2c gap: the empty-fleet pass runs the interval loop with a 3 s shutdown,
  so several passes run before the panic): G2c **FAILED** (`:713`): "N reaper passes, where the
  empty fleet runs exactly one".
- **M3** (iteration 1's defect: the pass also runs under `--reconcile-after-restore`): the restore
  guard **FAILED** (`:731`): the store changed.
- **M4** (the no-endpoints loop never looks at shutdown, before a pass or during the sleep): G2a
  **FAILED** (`:647`) on the clean-exit check. With shutdown never polled, tokio never installs its
  SIGINT handler, so the Ctrl-C killed the role (wait status 2) instead of ending it with exit 0.
  The store check passed: the reaping itself was fine.

## Refuting my own test

- **(a) Genuine red?** Yes. The verify gate reverted production and kept the tests: 4 executed,
  3 failed; G2a and G2c on the store assertion, the usage test on the missing lines. That is not a
  compile error: the file names no symbol the patch adds and drives the binary by argv. Negations
  12/13 and mutants M1–M4 each fail exactly their own test. G2b's lack of a base red is recorded
  above and is pinned by negation 11.
- **(b) Production path?** Yes. G2a, G2c, the restore guard and the usage test run the real built
  `wyrd` binary (`env!("CARGO_BIN_EXE_wyrd")`) as the ordinary `custodian` role, with no one-shot flag (asserted
  in `role()`), over a real redb store, and read the store back after the process is gone. Pass
  counts come from the reaper's own `emit_pass` tally on the role's real log sink. G2b runs the real
  `CustodianService::run_reconstruction_until`. There is no stand-in and no test-only entry.
- **(c) Fixture includes the fault?** Yes. G2a has no `--endpoints`. G2c names
  `http://127.0.0.1:1`, which the real gRPC dial refuses (stderr: "could not connect to D server:
  transport error"); the real `connect_fleet` returns empty and the role really panics on every
  start (exit 101, diagnostic present). G2b hands the loop 3 servers against an operator count of 4,
  so the GC gate really defers (`assert!(servers.len() < OPERATOR_FLEET_SIZE)`). The session fixture
  is leg A's, with the same JSON spellings and stamps.

## Choices, and what I ruled out (with costs)

- **The redb bound (rejected as a code change).** To make the 60 s bound real on redb, the pass
  would have to run on a detached OS thread with its own runtime, while the main thread waits on a
  channel with a timeout. Tokio's `spawn_blocking` does not work for this: dropping the runtime
  during the panic's unwind waits for blocking-pool threads. The thread needs owned `'static`
  copies of `CustodianService` (which holds the OTLP exporter built on the main runtime),
  `FencedZone` and `Custodian`, plus a second runtime. My estimate is about 40 lines more than this
  patch, and a second exporter lifetime to reason about. Even then, a thread stuck in
  uninterruptible disk I/O holds up process exit, so the stall it targets would still hang the
  role. Every other redb pass the role runs (the fleet loop, both one-shots) is held only to the
  per-pass budgets. Documented instead (`custodian.rs:146-150`).
- **Not** running the no-fleet shapes through `run_reconstruction_until` with an empty fleet and
  `operator_fleet_size = 0`. Then `0 == 0` passes GC's gate, and **GC would run over an empty
  fleet**. Unsafe. A separate record-only loop avoids that.
- **Not** reusing `run_reaper_once_over_backend` for the empty-fleet pass. Its redb open
  (`open_existing_local_meta_redb`) refuses a missing store with text that names
  `--reap-abandoned-uploads`, which would confuse an operator reading the empty-fleet failure. Both
  no-fleet shapes open the store as the fleet role does. As a result, on redb the no-endpoints role
  creates `meta.redb` if it is missing, just as the fleet role always has (see "For the human").
- **Not** moving the fleet loop's reaper to a record-only pass. On a fleet, the retirement leg needs
  the fleet for by-chunk-id retirement; a record-only pass would hold those records for good on a
  fleet role.
- **Cancelling a pass** (on shutdown or at the bound) is safe for the same reason a crash is: every
  commit the reaper and the drain send is conditioned on what they read, and the next pass reads
  the store afresh. On redb nothing can cancel a pass mid-way, since it never yields.
- **G2a stops the role with Ctrl-C (SIGINT), sent through the system `kill` command**
  (`multipart_reaper_deployed.rs:249`). The other ways to send it were `libc::kill`, which needs
  `unsafe` (the file is `#![forbid(unsafe_code)]`), or a new dev-dependency such as `nix`
  (`Cargo.toml` is outside this slice's files). CI runs only on `ubuntu-latest`, and other tests in
  the tree already shell out (`docker kill`, `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:578`).
  My first version stopped the role with SIGKILL (`Child::kill`). Measuring diff coverage showed
  why that was worse: a killed process never writes its coverage data, so the whole no-endpoints
  loop showed as never run (38.4% diff coverage). It also left the Ctrl-C shutdown untested. With
  SIGINT the role exits through its normal path, its coverage is recorded, and G2a asserts the
  clean exit (mutant M4 shows that check binds).
- `--gc-expired-pending`, and `--ids`/`--failure-domains` without `--endpoints`, are still parsed
  and ignored on the no-endpoints shape, as before.

## For the human

- **G2c and the brief disagree with each other.** The brief asks for "exactly one reaper pass" on
  the empty-fleet startup *and* for leg A's full transition after one run. Teardown takes three
  passes, so both cannot hold. I kept "exactly one pass per start" (checked on every start) and
  reach the transition across starts, the way a supervisor's `Restart=on-failure` would. This is
  the C1 item held in `deferred-findings.json`.
- **G2b's red claim in the brief is stale** (above). This is held in `deferred-findings.json`.
- **The no-endpoints role creates a redb store if none is there**, then reaps it every interval and
  prints the "Running the multipart reaper alone" line. With a mistyped `--data-dir`, that is an
  empty store reaped forever. The fleet role has always opened its store this way. Refusing a
  missing store on the no-fleet shapes would be a one-line change (`open_existing_local_meta_redb`)
  plus a new refusal message, and it changes behaviour, so it is left for you (held in
  `deferred-findings.json`).
- **On redb, a no-endpoints role now holds the store's exclusive lock for as long as it runs.**
  Before, it exited at once. Any other process on the same `--data-dir` is locked out, just as with
  the fleet role. This is expected for redb (dev/eval), but it is a visible change.
- **The diff-coverage gate measured against `origin/main`.** Iteration 1's C4-diff-cov ran with
  `base_ref: origin/main` ("patch.diff does not apply on origin/main"), while C4-verify applied the
  patch on the stack base. `origin/main` does not carry 625.1 (`427e5ec`), so no patch for this slice
  can apply there. The patch applies cleanly on `origin/pdca-integration/main-r9dd123c9ed70`
  (`6e44de9`). That looks like the driver giving the two bundle-scoped gates different bases, which
  is a harness matter, not a patch defect.
- `review-rejected.md` records the two findings I declined (the redb bound, and DST coverage), with
  reasons.
- #633 must be re-pointed to this slice and to child-3 after acceptance (from the brief's Ordering
  note).
- **Leftovers from my own gate runs.** `run-verify.sh` and `run-diff-cov.sh` each added a git
  worktree under `$PDCA_SCRATCH` (`pdca-builder-1016-verify`, `pdca-builder-1016-cov`) and a branch
  in the shared Wyrd repo (`pdca-verify-lb1016`, `pdca-cov-lb1016`). I left them, since cleanup of
  the scratch root belongs to the harness. Once it reclaims that root, `git worktree prune` drops the
  two registrations, and `git branch -D pdca-verify-lb1016 pdca-cov-lb1016` removes the branches.

## Commit-readiness

- `cargo fmt --all -- --check`: clean (after `cargo fmt --all`).
- `typos` on every touched file: clean.
- `cargo clippy -p wyrd-server --all-targets --all-features -- -D warnings`: clean, so the tikv and
  fdb arms of both new dispatches and `open_within_no_fleet_bound` compile. The same with default
  features: clean (this is where `open_within_no_fleet_bound` is compiled out).

### `cargo xtask ci` on the final tree

Run through the project's wrapper (`engine/xtask.sh ci`, the C4-ci gate) on the worktree. First
run, on the tree before the last test-file change: **exit 0, "xtask ci: all checks passed"**. Every step ran: typos,
lint_docs, render_site (the link audit), the gitlink/unsafe/blackbox guards, fmt, clippy, build,
`cargo test --workspace --exclude wyrd-dst`, cargo-machete, all three cargo deny runs, statics,
deploy-guard, DST clippy and DST tests (madsim). 235 `test result: ok` lines, 0 FAILED. Among them
are `custodian_gc` (13 passed, G2b included) and `multipart_reaper_deployed` (3 passed, 4.11 s).
Iteration 1's one-off `custodian_gc` hang did not recur. The only "running for over 60 seconds"
note was a DST test (`drain_ceiling_refusal_is_confirmed_on_the_segment`), which passed.

Second run, on the worktree holding exactly the final `patch.diff`, after the last test-file change
(Ctrl-C in G2a, the usage test). Production code, docs and `custodian_gc.rs` were byte-identical
between the two runs. Result again: **exit 0, "xtask ci: all checks passed"**, 235 `test result:
ok`, 0 FAILED, `multipart_reaper_deployed` 4 passed. Its clippy step (`--workspace --all-targets`,
default features) covers the final test file; I also ran the all-features clippy on it: clean.

### Diff coverage, measured on the right base

I ran the project's `engine/scripts/run-diff-cov.sh` myself with `PDCA_BASE=6e44de9`, against a
scratch copy of the bundle (patch.diff + brief.md only), so the gate's own `coverage/diff-cov.json`
was not overwritten. Result: **PASS, 87.5%**: 98 of 112 instrumentable changed lines ran (floor
80%); 112 of 309 changed lines were instrumentable. The lines that did not run:

- `cli.rs:1283-1289`: the empty-fleet pass's error and timeout arms. No test makes that pass fail
  or hang.
- `custodian.rs:593-606`: the no-fleet loop's `Fenced`, `Store` and timeout arms, for the same
  reason.
- `custodian.rs:791`: the fleet loop's `reaper_pass` call. G2b in `custodian_gc.rs` runs it, but
  this gate runs only the test file the patch adds.

My first measurement was 38.4%. G2a then stopped the role with SIGKILL, so the child never wrote
its coverage data; that is what moved G2a to Ctrl-C (see "Choices").

## Self-review against the review rubric (`AGENTS.md`, "Review rubric & protocol")

- **One clock per correctness lifecycle:** every reaper judgment, drain mark and pass `now_millis`
  in all three shapes reads one `LoopClock` over the role's `wall_clock_millis`, the source 625.1
  already uses (`clock_source = wall`). The tokio timers this patch adds (the pass bound, the
  interval sleep) only schedule; the doc says so (`custodian.rs:563-565`). The test's fixture stamps
  use `SystemTime::now()` with the allow and reason copied from 625.1's file.
- **Seams and dependency direction:** no new crate dependency; `server` calls `wyrd_custodian`'s
  existing API only.
- **No DST-reachable global state:** none added; `xtask statics` passed.
- **`#![forbid(unsafe_code)]`:** no new crate root; the new test file carries it, which is why
  SIGINT goes through `kill` rather than `libc::kill`.
- **Docs currency:** a CLI behaviour change, no new flag: `usage()`, the `cmd_custodian` doc and
  `06-runtime-view.md` are updated in this patch, and the usage test pins the help text.
- **Absent or unsupported entries:** the empty-fleet pass logs its fault and then fails loud
  (panic, 101), as before; the no-endpoints loop logs and retries a store fault, as the fleet loop
  does; a held session is named by the reaper itself.
- **Await discipline:** the no-fleet pass is raced against shutdown and bounded; the networked open
  is bounded; the interval sleep is raced against shutdown. The one gap (redb's synchronous calls)
  is documented and recorded in `review-rejected.md`. No helper task is spawned.
- **Test fidelity:** the tests drive the real binary and the real run loop. Seeded DST coverage is
  declined because `crates/dst/` is off-limits to this slice (recorded).
- Protocol input, grammar strictness, serialization identity, transactions, probes and workflow
  edits: not touched by this diff.
