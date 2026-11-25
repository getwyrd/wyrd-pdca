# Build notes — issue 1016 (625.2: the multipart reaper runs on every deployed custodian startup shape)

Target: getwyrd/wyrd @ main. Base commit in the worktree: `6e44de9` ("pdca-integrate: issue_1015"),
on top of `427e5ec` ("custodian: reap multipart uploads whose client went silent") — that is 625.1,
so 625.1 **is** on the base. Line numbers below are on the patched tree unless marked "base".

## What changed

### `crates/server/src/custodian.rs`

- **`CustodianService::reaper_pass`** (`custodian.rs:495`, private) — the one reaper interval step.
  It builds the `ReaperContext` (one `LoopClock`, `clock_source = wall`, `ReaperPolicy::default()`)
  and runs one fenced `reconcile_pass` carrying the reaper plus an optional
  `ReconstructionContext`. This is the "one reaper interval driver" the brief's Scope asks for:
  every shape calls this, and only this, to dispatch the reaper.
  - fleet loop: `self.reaper_pass(.., Some(&ctx))` inside `run_reconstruction_until`
    (`custodian.rs:749-751`), replacing the inline `ReaperContext` + `reconcile_pass` that 625.1
    wrote (base `custodian.rs:634-639`, `:672-683`). Same calls to `reconcile_step`, same error
    handling. It is **outside** GC's gate (`custodian.rs:822`), which is unchanged.
  - operator one-shot: `reap_abandoned_uploads_once` now calls `reaper_pass(.., None)`
    (`custodian.rs:475`); behaviour unchanged.
  - **`run_reaper_until`** (`custodian.rs:541`, new, pub) — the no-fleet loop: `reaper_pass(.., None)`
    each interval, record-only; `Fenced` stops it, `Store` is logged and retried. The `select!` is
    `biased;` with shutdown first (`custodian.rs:575`), so an already-ready shutdown means exactly
    one pass, whatever `--interval-secs` is (without `biased`, `--interval-secs 0` would race the
    ready sleep against the ready shutdown and could run more than one pass).

### `crates/server/src/cli.rs`

- **No `--endpoints`** (`cli.rs:1163-1191`): instead of printing "nothing to reconstruct" and
  returning success (base `cli.rs:1158-1162`), the role prints that it runs the reaper alone and
  calls `run_reaper_over_backend(.., shutdown)` (Ctrl-C), then returns success. The
  `--reconcile-after-restore` refusal on that branch is untouched.
- **Empty fleet** (`cli.rs:1251-1303`): after `connect_fleet` comes back empty and before the
  unchanged `panic!` (`cli.rs:1297`), one call to `run_reaper_over_backend(.., std::future::ready(()))`
  — exactly one pass — wrapped in `tokio::time::timeout(EMPTY_FLEET_REAPER_PASS_BOUND, ..)`
  (`cli.rs:1270`; constant `cli.rs:914`, 60 s). Any error or timeout is logged, never returned, so
  the panic message and exit status (101) do not change. It runs under the leadership from
  `Custodian::elect` (`cli.rs:1074`), which runs before the endpoints check.
- **`run_reaper_over_backend`** (`cli.rs:1882`, new, pub) — the per-backend store open (redb / tikv /
  fdb) for the no-fleet loop, mirroring `run_reconstruction_over_backend` and using the same
  `open_local_meta_redb` the fleet role uses.
- `cmd_custodian` doc comment and the `usage()` text (`cli.rs:515-520`) now state the two new
  behaviours (docs currency for a CLI behaviour change).

### `docs/design/architecture/06-runtime-view.md`

Appended to 625.1's reaper paragraph (line 94): the three startup shapes.

### Tests

- NEW `crates/server/tests/multipart_reaper_deployed.rs` — G2a (`:506`) and G2c (`:530`). Leg A's
  helpers copied from `multipart_reaper.rs` (not shared). Every argv is asserted to lack
  `--reap-abandoned-uploads`. G2a runs the role for a 3 s window (interval 1 s), then kills it;
  G2c waits (bounded at 120 s) for the role to exit. Both repeat up to 6 runs until the session is
  gone, like 625.1's `reap_until` (the terminal delete waits for the `retire:` obligation to drain,
  so it lands on the second or third pass); G2c's restart-per-pass is exactly how a supervisor
  would drive the failing role. Every G2c run is checked for non-zero exit and the diagnostic.
- APPENDED to `crates/server/tests/custodian_gc.rs`: `leg_a_records` (`:1672`) and G2b
  `deployed_loop_reaps_an_idle_upload_on_a_degraded_fleet_while_gc_is_deferred` (`:1748`): three
  servers handed to the real `run_reconstruction_until` (through the file's own `drive_passes_at`,
  `:1368`) with `operator_fleet_size = 4`, so GC is deferred on every pass.

## Call path from `cmd_custodian` to the reaper, per shape

1. **No `--endpoints`**: `cmd_custodian` (`cli.rs:977`) → `Custodian::elect` (`:1074`) →
   `let Some(endpoints) = endpoints else` (`:1163`) → `run_reaper_over_backend` (`:1186` → `:1882`)
   → `CustodianService::run_reaper_until` (`custodian.rs:541`) → `reaper_pass(.., None)`
   (`custodian.rs:495`) → `reconcile_pass` → `wyrd_custodian::reconcile_step` with
   `reconstruction = None, reaper = Some` → reaper, then `retirement::reconcile_record_only`
   (`crates/custodian/src/reconciliation.rs:196-209`). Every interval until Ctrl-C.
2. **`--endpoints`, none reachable**: `cmd_custodian` → `Custodian::elect` (`:1074`) →
   `connect_fleet` (`cli.rs:1242`; `custodian.rs:236`) returns empty → `if configured.is_empty()`
   (`cli.rs:1251`) → `tokio::time::timeout(.., run_reaper_over_backend(.., ready(())))` (`:1270`)
   → `run_reaper_until` → one `reaper_pass(.., None)` → `reconcile_step` record-only (as above)
   → shutdown polled first (`biased`) → return → `panic!` (`cli.rs:1297`), unchanged.
3. **Fleet (whole or degraded)**: `cmd_custodian` → `connect_fleet` (non-empty) →
   `run_reconstruction_over_backend` (`cli.rs:1371`) → `run_reconstruction_until`
   (`custodian.rs:653`) → per interval, after the scrub pass, `reaper_pass(.., Some(&ctx))`
   (`custodian.rs:749-751`) → `reconcile_step` with `reconstruction = Some, reaper = Some` →
   reconstruction, reaper, `retirement::reconcile` (`reconciliation.rs:168-195`). Then the GC
   pass, only if `fleet.len() == operator_fleet_size` (`custodian.rs:822`).

## Red → green evidence

Runner: the project's verify gate `engine/scripts/run-verify.sh` (bundle-scoped C4-verify), run with
`PDCA_BASE=6e44de9`, `WYRD_REPO=$PDCA_WORKTREE`, and its worktree/lane pinned into my scratch dir
(`WYRD_VERIFY=$PDCA_SCRATCH/pdca-builder-1016-verify`, `PDCA_LANE=b1016`) so it could not touch
another lane's `../wyrd-verify`. The figures below are from the first run, before the timeout edit
to `cli.rs`; the re-run on the final patch matches (see Commit-readiness).

- GREEN (fix applied): `running 2 tests` … `test result: ok. 2 passed; 0 failed` (3.11 s).
- RED (production reverted, test kept): `running 2 tests` … `test result: FAILED. 0 passed; 2 failed`
  (0.33 s). **Executed: 2, failed: 2.** Both fail on the store assertion ("the store did not converge
  within 6 runs of the ordinary role"): G2a's runs each "exited by itself, exit status: 0" after
  "no --endpoints; nothing to reconstruct"; G2c's runs each exited 101 with the diagnostic — the
  status is non-zero on both trees, so the store assertion is what discriminates, as the brief says.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (2 test(s) ran red).`

G2b (`cargo test -p wyrd-server --test custodian_gc`, wrapped in `timeout`): 13/13 pass with the fix.

### G2b does NOT red on the base — the brief's falsifiability claim for it is stale

With only `crates/server/src/{cli,custodian}.rs` reverted to base and the new G2b kept, G2b
**passes** (`1 passed`). Reason: 625.1 already dispatches the reaper on the reconstruction pass
(base `custodian.rs:672-683`), and `reconcile_step` runs the reaper and the retirement leg on the
reconstruction input "whatever the fleet's state" (`crates/custodian/src/reconciliation.rs:172-195`).
That pass is not behind GC's `fleet.len() == operator_fleet_size` gate (base `custodian.rs:749`).
So on this base the brief's "the reaper sits behind the whole-fleet gate" is not true; the
brief's own Defect bullet (`custodian.rs:680-713` as the gate around the reaper) appears to describe
an earlier draft of 625.1. G2b is therefore a **regression guard** (pinned by negation 11 below),
not a red→green discriminator. It is not C4 evidence anyway (the brief puts it under `cargo xtask ci`).

### Negations (each a temporary mutation, run, then restored from a scratch copy)

- **(11)** reaper dispatched only inside the whole-fleet branch (fleet loop passes
  `Some(&reaper)` only when `fleet.len() == operator_fleet_size`): G2b **FAILED** at
  `custodian_gc.rs:1807` ("the idle upload is fenced and its teardown finished on the degraded fleet");
  the whole-fleet test `deployed_loop_reaps_an_idle_open_upload_and_tears_it_down` stayed **ok**.
- **(12)** no-endpoints early exit left in place (the `run_reaper_over_backend` call removed):
  G2a **FAILED**, G2c ok.
- **(13)** the empty-fleet reaper pass skipped: G2c **FAILED**, G2a ok.

## Refuting my own test

- **(a) Genuine red?** Yes. The verify gate reverted production and kept the test: 2 executed,
  2 failed, both on the store assertion (not a compile error — the test names no symbol the patch
  adds; it drives the binary by argv). Negations 12 and 13 each red exactly their own leg.
- **(b) Production path?** Yes. G2a and G2c run the real built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) as the ordinary `custodian` role, no one-shot flag (asserted in
  `role()`), over a real redb store, and read the store back after the process is gone. G2b runs the
  real `CustodianService::run_reconstruction_until`. No stand-in, no test-only entry.
- **(c) Fixture includes the fault?** Yes. G2a has no `--endpoints` at all. G2c names
  `http://127.0.0.1:1`, which the real gRPC dial refuses (stderr: "could not connect to D server:
  transport error"), and the real `connect_fleet` returns empty — the role really panics on every
  run (exit 101, diagnostic present). G2b hands the loop 3 servers against an operator count of 4,
  so the GC gate really defers (`assert!(servers.len() < OPERATOR_FLEET_SIZE)`); the session fixture
  is leg A's, byte-for-byte the same JSON spellings and stamps.

## Choices and what I ruled out

- **Not** running the no-fleet shapes through `run_reconstruction_until` with an empty
  `configured` and `operator_fleet_size = 0`: then `fleet.len() == operator_fleet_size` holds
  (0 == 0) and **GC would run over an empty fleet**, and reconstruction would assess every
  placed chunk over no servers. Both unsafe. A separate record-only loop has neither.
- **Not** reusing `run_reaper_once_over_backend` (the one-shot's open) for the empty-fleet pass:
  it uses `open_existing_local_meta_redb`, whose refusal text names `--reap-abandoned-uploads`,
  which would confuse an operator reading the empty-fleet failure. Both no-fleet shapes use the
  role's ordinary open (`open_local_meta_redb`), as the fleet role does. Consequence: on redb a
  no-endpoints role (or the empty-fleet pass) now creates `meta.redb` if it is missing, as the
  fleet role always has. The one-shot's "creates none" rule is the one-shot's, not the role's.
- **Not** moving the fleet loop's reaper to a record-only pass: on the fleet the retirement leg
  needs the fleet for by-chunk-id retirement (`retirement.rs:132-152`); a record-only pass would
  hold those records for good on a fleet role.
- **Timeout on the empty-fleet pass** (`cli.rs:914`, `:1270`): added in self-review against the
  rubric's await-discipline rule. Without it a hung metadata backend (e.g. TiKV PD down) would keep
  the role from ever reaching the panic — a regression of the loud-fail behaviour the brief says
  must stay. 60 s is a judgment call; I found nothing in the tree to derive it from. The
  no-endpoints loop's store calls are not individually bounded, exactly like the existing fleet
  loop's; I did not add bounds there (out of scope, would change the fleet loop's policy too).
- `--gc-expired-pending` and `--ids`/`--failure-domains` without `--endpoints` are still parsed and
  ignored on the no-endpoints shape, as before (the role used to exit 0 with them). Not changed.

## Things the human may want to look at

- The brief's G2b red claim does not hold on this base (above). G2b ships as a regression guard
  with negation 11 as its evidence.
- Rubric "Test fidelity: a new destructive or concurrent path lands with seeded Tier-0 DST
  coverage": the no-fleet loop calls the same `reconcile_step` record-only path the 625.1 one-shot
  already calls; it adds no new reaper or drain logic. Any DST coverage would live in `crates/dst/`,
  which this slice MUST NOT edit (child-3 owns it). If the reviewer asks for DST coverage of the
  no-fleet loop, that is a §6 item for child-3 / a follow-up, not something this slice can add.
- #633 must be re-pointed to this slice and child-3 after acceptance (from the brief's ordering note).

## Commit-readiness

`cargo fmt --all -- --check` clean; `typos` clean on every touched file;
`cargo clippy -p wyrd-server --all-targets --all-features -- -D warnings` clean (so the tikv and fdb
arms of `run_reaper_over_backend` compile).

Verify gate re-run on the **final** `patch.diff` (after the timeout edit): GREEN 2 passed (3.08 s),
RED 2 executed / 2 failed (0.42 s), `run-verify.sh: PASS`.

### `cargo xtask ci` on the final tree

Every step passed — typos, lint_docs, render_site (link audit OK), the guards, fmt, clippy, build,
cargo-machete, cargo deny (all three), statics, deploy-guard, DST clippy and DST tests (madsim) —
**except** `cargo test --workspace --exclude wyrd-dst`, and only because of one event:

- In that run the `custodian_gc` test binary **hung**: 4 tests finished, the other 9 (every one
  still running at that moment, including `deployed_run_loop_refuses_duplicate_endpoints`, which
  never starts the run loop) sat in `futex` waits for minutes with the host idle. I killed it
  (SIGTERM), which is the reported failure. I could not take stacks (ptrace is restricted on this
  host).
- I could **not reproduce** it: the same test binary as CI built it (`custodian_gc-6d8f770a…`) ran
  80 times, 8 at a time, with a 20 s bound each: 0 hangs. Another build of it: 60 sequential +
  80 parallel runs, 0 hangs. `cargo test --workspace --exclude wyrd-dst --test custodian_gc`: 13/13
  in 0.17 s.
- Then `cargo test --workspace --exclude wyrd-dst --no-fail-fast` on the final tree: **exit 0**,
  every test binary ok (222 `test result: ok` lines, none FAILED, no "running for over 60 seconds"),
  including `custodian_gc`, `multipart_reaper` and the new `multipart_reaper_deployed`.
- My guess, not proven: a lock-order hang in process-global `tracing` state. Every test in the
  binary hung at once, which points at a shared global rather than one test's loop, and the file
  sets a global `tracing` default lazily from inside tests (`enable_log_callsites`,
  `custodian_gc.rs:1209`) while other tests register callsites. My G2b calls that helper too, as the
  two 625.1 tests beside it do, so it may slightly raise the odds of a pre-existing race; it adds no
  new kind of global use. If Check's C4-ci run hangs the same way, this is where to look first.
