# Build notes — issue 205 / d-server gRPC admission control (iteration 3)

Target: `getwyrd/wyrd @ main` (worktree base `d60ef6c`). Citations are `path:line` on
that base as edited in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`).

## What iteration 3 had to do (carry-forward)

Iteration 2's **admission-control fix was accepted as sound** by sign-off — the
server-wide `GlobalConcurrencyLimitLayer` (one shared `Arc<Semaphore>`) + `LoadShedLayer`
applied via `Server::layer`, plus the request timeout and per-connection caps. It was
rejected **only** because the gating gate `C4-ci` (`cargo xtask ci`, which runs the
50-seed madsim DST sweep) was red on
`crates/dst/tests/custodian.rs::durability_emission_rises_then_returns_to_zero`.

The carry-forward instruction was explicit: **preserve the admission code unchanged**,
and in the *same* iteration **de-flake the gating DST test** so `C4-ci` is green
deterministically — fix the determinism (ADR-0009: DST must be deterministic) rather than
re-run until a sweep happens to pass.

This iteration does exactly that: the admission-control patch is carried forward verbatim
from `iteration-v2/patch.diff` (it `git apply --check`s clean on the current base), and a
root-caused, source-verified de-flake of the DST test is added.

## Root cause of the DST flake (diagnosed, not guessed)

The carry-forward *hypothesised* "real-time/scheduling nondeterminism." That is **not**
the cause. I reproduced and isolated it:

- **Reproduced red:** with the unmodified test, running the compiled `custodian` madsim
  binary `MADSIM_TEST_NUM=50 ... --test-threads=8` failed **7 / 15** runs, always with:
  ```
  custodian.rs:956: assertion `left == right` failed: the under-replicated count rises to 1
    left: []
   right: [1]
  ```
  `left: []` is the tell — `MetricCapture` recorded **nothing**, i.e. the
  `tracing::info!` event never reached the capture layer (not a wrong *value*, an *absent*
  one).
- **Isolated:** the same binary `--test-threads=1` failed **0 / 5**. The flake is purely a
  function of *parallel execution*, not of the madsim seed — which is why a fixed
  `MADSIM_TEST_SEED` reproduces both pass and fail.

**Mechanism.** The durability metrics are emitted with `tracing::info!`
(`crates/custodian/src/reconstruction.rs:417,425,435-436`, e.g.
`reconstruction_under_replicated`). The telemetry property reads them back through a
**scoped** `MetricCapture` installed per-future via `.with_subscriber(..)`
(`custodian.rs:952,975`). But `tracing` caches an **interest per callsite the first time
that callsite is hit, in process-global state**. The thread that hits the callsite first
decides the cache. Every *other* property in this file (`reconstruct_to_full_redundancy`,
`commit_point_atomic_repair_under_crash`, `scrub_…`, `fenced_…`) runs `reconcile_step`
**without** a capture subscriber, i.e. under the process default `NoSubscriber`, whose
interest is `Interest::never()`. Under `cargo test`'s parallel threads (×50 seeds) one of
those non-capturing properties routinely registers the shared callsite *first*, caching it
`never` process-wide — after which the event is short-circuited before it can reach the
telemetry property's later `with_subscriber` layer. So the capture is empty and the
emission assertion flakes. This is a documented `tracing` test-isolation hazard, not a
DST/madsim-determinism bug, and it is entirely off the d-server admission path
(the DST suite uses the in-memory `MemDServer`, never `DServer::serve`).

## The DST fix (smallest change that restores determinism)

`crates/dst/tests/custodian.rs`:

- New helper `install_metric_dispatch()` (`custodian.rs:331-355` region) that installs a
  **permissive, process-global default subscriber** (`tracing_subscriber::registry()`)
  exactly once via `std::sync::Once`. With a permissive global default in place, every
  callsite registers as *enabled* no matter which property hits it first, so the
  interest cache can **never** be poisoned to `never`. A *scoped* `with_subscriber` still
  overrides the global default for routing, so the telemetry property's captured values
  are unchanged; non-capturing properties just route their events to the no-op registry.
- `install_metric_dispatch()` is called as the **first line** of every `#[madsim::test]`
  (`custodian.rs` test entry points: `reconstruct_to_full_redundancy_q1`,
  `commit_point_atomic_repair_under_crash`, `scrub_detects_bit_rot_then_reconstructs_q2`,
  `gc_reclaims_only_true_orphans_q3`, `fenced_stale_leader_lands_nothing`,
  `durability_emission_rises_then_returns_to_zero`, `committed_regression_seeds_stay_green`).
  Because `Once::call_once` is a barrier, the first test to start installs the global
  default before any test body reaches a metric callsite — closing the race for all of
  them.

**Verified green & deterministic:** with the fix, the same stress that reproduced the red
now passes **0 / 30** failures at `--test-threads=8` and **0 / 20** at `--test-threads=16`
(`MADSIM_TEST_NUM=50`). This is a root-cause fix that *removes* the nondeterministic state
read, not a re-run-until-green; the green is now invariant under parallel load.

### Why this fix, and rejected alternatives

- **Why not `#[ignore]`/quarantine the test?** The carry-forward allows pinning *with
  justification*, but quarantining drops the BINDING property-6 durability-plane coverage
  (`0005:400-403`) entirely — and the test is *correct*; only its harness interaction is
  racy. Fixing the 1-line-per-test root cause keeps the coverage. Cost of the rejected
  path: it would mute the only assertion that the under-replicated count rises→returns to
  zero, a real M3 graduation criterion.
- **Why not make `MetricCapture` the single global default (drop `with_subscriber`)?**
  Then *all* parallel properties would emit into one shared capture, cross-contaminating
  the per-property `values(..)` assertions — it trades one race for another. The scoped
  `with_subscriber` capture is the right isolation; the only missing piece is keeping the
  global interest cache permissive, which the no-op global default does.
- **Why not `tracing::callsite::rebuild_interest_cache()` before the capture?** It does
  not close the window: a parallel non-capturing thread can register/poison the callsite
  *after* the rebuild and before the poll, and registration itself triggers no rebuild.
  A permissive global default makes registration itself never produce `never`, which is
  the only race-free fix.
- The DST change touches **only the test file** (`crates/dst/tests/custodian.rs`, +37
  lines); no production code and no admission-control code is altered to fix the flake.

## The admission-control fix (carried forward from iteration 2, unchanged)

Reproduced here for the human; full source-verification is in
`iteration-v2/build-notes.md`. `crates/server/src/dserver.rs:269-307` (`DServer::serve`):

```
Server::builder()
    .layer(LoadShedLayer::new())                                   // outermost: shed on overload
    .layer(GlobalConcurrencyLimitLayer::new(admission.max_concurrent_requests)) // ONE shared Arc<Semaphore>
    .concurrency_limit_per_connection(admission.max_concurrent_requests_per_connection)
    .load_shed(true)
    .timeout(admission.request_timeout)                            // cut a hung handler
    .max_concurrent_streams(Some(admission.max_concurrent_streams))
    .tcp_nodelay(admission.tcp_nodelay)
    .http2_keepalive_interval(admission.http2_keepalive_interval)
    .add_service(ChunkStoreServer::new(service))
```

- **Server-wide bound (the binding fix for §8.9).** `Server::layer` applies the stack
  once and `MakeSvc` *clones* it per connection; `GlobalConcurrencyLimitLayer` holds one
  `Arc<Semaphore>` whose clones share the same permits — a true server-wide cap,
  independent of connection count. This is precisely the property iteration-1's
  `concurrency_limit_per_connection` lacked (per-connection semaphore ⇒ aggregate in-flight
  unbounded in connection count). Verified against tonic-0.14.6 `mod.rs:784,1228` and
  tower-0.5.3 `limit/concurrency/{layer,service}.rs`.
- **Shed → retryable status.** `LoadShedLayer` turns the limit's backpressure into an
  immediate `Overloaded`, which tonic maps to `RESOURCE_EXHAUSTED` (verified tonic-0.14.6
  `status.rs:365-368`).
- **Configurable, HDD/SSD-tunable.** `AdmissionControl` (`dserver.rs:166-202` region) with
  `with_admission_control`; CLI `--max-concurrent-requests` / `--request-timeout-secs`
  (`crates/server/src/cli.rs:266-281,301-304`). Default 64, documented as a moderate
  middle ground to tune to the device's useful queue depth — not a fixed constant.

### Named test (red→green) — `crates/server/tests/dserver.rs`

Two net-new tests (the criterion is absent pre-fix, per the brief's verification posture):

1. `overload_across_connections_sheds_excess_with_a_retryable_status` — global limit = 1,
   two **separate connections**; A holds the one slot, B (its own per-connection budget)
   must still be shed with `ResourceExhausted`/`Unavailable`. The cross-connection
   discriminator the iteration-1 carry-forward demanded: a per-connection-only limit would
   admit B.
2. `hung_handler_is_cut_by_the_request_timeout` — a never-returning handler is cut by a
   200 ms timeout with `Cancelled`/`DeadlineExceeded`.

Both import-light (tonic/tokio, no GUI/display dep), self-bounded with
`tokio::time::timeout(5s)` so they cannot hang the runner. Green on this base:
`cargo test -p wyrd-server --test dserver` → **4/4 pass**. Red pre-fix (bare
`Server::builder()`) was shown in iteration 2 (`iteration-v2/build-notes.md`): both new
tests fail on `Elapsed(())`.

## Gate status

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-dst --tests` (under `--cfg madsim`): clean.
- `cargo test -p wyrd-server --test dserver`: 4/4 pass.
- DST de-flake stress: 0/30 (`-j8`) and 0/20 (`-j16`) failures, `MADSIM_TEST_NUM=50`.
- `cargo xtask ci` (the gating `C4-ci`, via `engine/xtask.sh`): see SUMMARY / the run log.

## Dependency note (unchanged from v2)

`tower` added as a direct dep of `wyrd-server` with only `["limit","load-shed"]`; already
in-tree transitively via `tonic`, so `Cargo.lock` is a single new edge, no new crates.
