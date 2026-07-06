# Build notes — issue 366 / obs-floor-observability (keystone, iteration 5)

Withheld from the reviewer. Rationale + what I ruled out.

## Scope of this iteration

This is the observability-floor **keystone** (proposal 0010 items 1–2): the durability
telemetry seam extracted to a shared `wyrd-telemetry` crate, and `wyrd custodian` wired as a
**runnable, deployable role** in `server`, proving the day-one signal — kill a D-server →
the under-replicated count **rises then returns to zero**, read back through the role's own
`gather_prometheus` surface.

I did **not** rebuild from scratch. Iterations 1–3's rejects (telemetry extraction, backend
routing via `resolve_backend`, connect-with-timeout, gauge-not-counter) were correctly
addressed by iteration 4 and I preserved that work as the base. Iteration 5 corrects the
**three defects the iteration-4 sign-off rejected**, each proven or verified below.

## The three iteration-4 rejects, each addressed

### 1. Gating C4-ci RED — the flaky metric read-back race (THE blocker)

**Root cause (confirmed in source, not assumed):** the durability metrics are emitted as
`tracing::info!(gauge.reconstruction_under_replicated = …)` callsites. `tracing` caches each
callsite's *interest* in a **process-global** table the first time it is hit
(`tracing-core-0.1.36`). Under the parallel gate, a sibling test that runs a reconcile pass
**without** installing a subscriber hits the callsite first → it registers against the no-op
default → latches `Interest::never`. The one test that reads the metric back
(`gather_prometheus`) then silently sees it missing. `rebalance.rs:884` has **11 of its 13
passes unsubscribed** (`reconcile_step(...)` with no `.with_subscriber`), so it latches the
callsite off and its two read-back tests flake — exactly what the adversary reproduced
(pass, pass, FAIL on the 3rd run).

**Why I verified this is NOT self-healing:** I read `tracing-core-0.1.36/src/dispatcher.rs`
— `set_global_default` does **not** call `rebuild_interest_cache()` (lines 299–332). So a
late guard cannot re-enable an already-latched callsite. The guard must therefore run
*before any callsite is hit*, in the process.

**Fix — the codebase's own proven remedy, applied where it was missing.** `scrub.rs:208`
already carries `enable_metric_callsites()`: a `Once` that installs a permissive global
`registry()` default, called at the **top of every metric-touching test**, so whichever test
the harness runs first sets the default before any callsite fires — every first-registration
then agrees `enabled`, and each test's own `.with_subscriber(...)` still routes its metrics
into its own provider (thread-local dispatch wins over the global default). I replicated this
exact pattern into the read-back binaries that lacked it:
`rebalance.rs` (9), `reconstruction.rs` (11), `gc_telemetry.rs` (1), `skeleton.rs` (2), and
the new `custodian_day_one.rs` (6). `gc.rs`'s only `gather_prometheus` hit is a doc comment
(no read-back), and `backfill_telemetry.rs`'s single pass is already subscribed — neither
races, so neither is touched.

**Proof this is now deterministic:** ran the *exact* failing gate command
`cargo test --workspace --exclude wyrd-dst` **3 times** → 0 failures each. Ran
`cargo test -p wyrd-custodian` 3× → clean.

*Alternative I rejected — re-architect the emission off `tracing` (direct OTel instrument
recording).* Cost: the durability plane emits through `tracing` from `reconstruction.rs`,
`rebalance.rs`, `gc`, `scrub`, `backfill` — ~5 call families across the M3 maintenance loops,
by ADR-0011/0012 design (the metrics bridge is the *decoupling* seam). Rewiring them to
thread a meter handle through every `reconcile_step` call would touch the whole plane and
violate the brief invariant "reuse, don't rebuild the durability telemetry seam." The
callsite guard is a **test-only, additive** fix on the exact seam the codebase already
established, so it is both smaller *and* the invariant-preserving choice.

### 2. Fabricated D-server topology (`cli.rs`)

iteration-4's fleet build fell back to `id = endpoint index` and
`failure_domain = endpoint URL` when `--ids` / `--failure-domains` were absent — and the
brief's canonical `wyrd custodian --otlp-endpoint …` command omits them, so it landed in that
branch. Two D-servers in one physical failure domain reached at different URLs would get
distinct *fabricated* domains → the reconstruction selector could re-place a rebuilt fragment
into the same real domain as a survivor, defeating cross-domain durability.

**Fix:** extracted `require_aligned_topology(n_endpoints, &ids, &domains)` (`cli.rs`), which
**rejects** any missing / short / long / mismatched list for static endpoints — no positional
fallback survives. The fleet is then keyed strictly by `ids[i]` / `domains[i]`. Deriving
topology automatically from each D-server's registration record needs the etcd `Coordination`
discovery seam, which is explicitly **out of scope** (0015's other prerequisite half), so the
operator supplies the real topology and the role never invents it. Proven headless by
`cli::tests::require_aligned_topology_rejects_missing_or_mismatched_lists` (unit test on the
pure validator).

### 3. The binding signal never drove the real binary's backend-open path

iteration-3's hard reject (custodian opening an empty local redb instead of the cluster's
store) could regress verbatim because **no test drove `cmd_custodian`'s backend preamble**
(resolve_backend → open_local_meta_redb → run loop). iteration-4 fixed the routing but tests
still hand-built `MemMeta` and called `run_reconstruction_until` directly.

**Fix:** factored the backend match + store open + loop into
`cli::run_reconstruction_over_backend(...)` — the *exact production path* `cmd_custodian`
runs — and added
`gauge_rises_then_returns_to_zero_through_the_redb_backend_open_path`: the "cluster" writes an
RS(2,1) object into a **real on-disk `RedbMetadataStore`** + the D-servers, drops the writer
(redb is single-writer), kills a D-server, then drives `run_reconstruction_over_backend(
MetadataBackend::Redb, data_dir, …)` — which calls the real `open_local_meta_redb(data_dir)`.
Two invocations capture the binding shape end to end through the real backend: the first
pass finds the loss and repairs it (gauge **1**), a fresh run reopens the redb, reassesses the
restored redundancy and drains the obligation (gauge **0**). The repair is confirmed persisted
by reopening the redb and asserting the queue drained. Headless: redb file + in-memory
D-servers + TCP-free; no GUI/display dependency.

## Red → green evidence

- **Behavioral red (deterministic), the gauge signal:** reverting *only* the `Unrepairable`
  arm's `under_replicated += 1` in `reconstruction.rs` (the iteration-2 Adv-2 fix) makes
  `gauge_counts_a_loss_beyond_tolerance` fail — `left: Some(0.0), right: Some(1.0)` at
  `custodian_day_one.rs:567`. Restoring it → green. (Captured live during the build.)
- **The named keystone test** `crates/server/tests/custodian_day_one.rs` — all 6 tests green
  post-fix, including the new backend-driven one. Pre-fix on the target branch the module
  (`wyrd_server::custodian`, `run_reconstruction_over_backend`) does not exist, so the file
  cannot compile — the "red" of a keystone that introduces new production surface; the
  deterministic behavioral red above is the meaningful within-patch flip.
- **Gate reproduction:** `cargo test --workspace --exclude wyrd-dst` (the iteration-4 failing
  gate, exit 101) now exits **0**, three consecutive runs.

## Commit-readiness

`cargo fmt --all --check` clean; `cargo clippy -p wyrd-server -p wyrd-custodian --all-targets`
exit 0 (workspace `-D warnings`). `cargo test --no-run -p wyrd-dst` compiles (the carried
metric-name follow in `crates/dst/tests/custodian.rs`).

## Carried-forward open items (human calls, recorded — not silently resolved)

- **Cross-process leader election.** `MemCoordination` is host-local single-active only; two
  custodian processes on one host both self-grant. Real fencing awaits the etcd `Coordination`
  backend (out of scope, 0015's other half). Docstring on `cmd_custodian` states this honestly.
- **Reachability probe vs registration-driven membership** (iteration-2 §6.3 / iteration-3
  §C5a): the live-fleet probe is a stand-in for lease-driven membership. Flagged, deferred.
- **Milestone decomposition + typed-errors × #255 sequencing + live-exporter fitness** remain
  the pre-agreed §6 sign-off items from prior iterations.
- **Residual latent flake:** `gc.rs` and `backfill_telemetry.rs` do not read metrics back so
  are unaffected; all binaries that DO read back are now guarded.
