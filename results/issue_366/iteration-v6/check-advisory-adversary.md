# Adversarial review — issue 366 (obs-floor keystone), iteration 6

Skeptic's pass. Grounded on the target source at `feat/obs-floor.1-keystone`
(`$PDCA_TARGET`). Gates are green (C4-ci pass, C4-verify pass); I attack what the
green tests do **not** cover and where the fix's own stated invariant fails.

## Attacks that landed

- **NEEDS-HUMAN — the fix reintroduces its own rejected "floor" defect on the
  `Unrepairable` arm.** `crates/custodian/src/reconstruction.rs:169`
  (`Assessment::Unrepairable => under_replicated += 1`) counts a below-`k`
  (un-reconstructable) chunk on the `reconstruction_under_replicated` gauge, but its
  obligation is **never drained** (only `Repairable→plans` and `Drain→drain_only` are
  cleared at `:205`/`:219`; `Unrepairable` falls through). So it is re-assessed and
  re-counted every pass and the gauge is **floored at ≥1 forever** on any store carrying
  a lost-beyond-tolerance chunk. This is verbatim the defect the fix's own docstring
  (`reconstruction.rs:148-153`) says it fixed by excluding `Malformed`: "it would
  re-count every pass and FLOOR this gauge at ≥1 forever — making returns to zero
  unobservable." Concrete failing case: on the #367 deployment this gates, a correlated
  double-D-server loss makes some RS(k,1) chunks `Unrepairable`; the binding "rises then
  **returns to ZERO**" success criterion (brief.md:52-59) then never returns to zero
  again until an operator manually intervenes. The commit at `reconstruction.rs:200-203`
  claims the gauge "can return to zero" on a populated store — that claim is only true if
  the store never carries an unrepairable chunk. **Human must adjudicate:** is a permanent
  non-zero floor on genuine data loss the intended durability-gauge semantics, and does it
  satisfy the binding "returns to zero" criterion, or must `Unrepairable` move to a
  distinct metric exactly as `Malformed` was? Note the tests dodge this: test #1
  (`custodian_day_one.rs:...gauge_rises_then_returns_to_zero...`) uses a **repairable**
  single-fragment loss, and test #2 (`gauge_counts_a_loss_beyond_tolerance`) asserts only
  the **rise** to 1 and stops — no test runs a second pass on an unrepairable chunk to
  show it never returns to zero.

- **NEEDS-HUMAN — the binding evidence still never drives the binary entry
  `cmd_custodian`; iteration-5 BLOCKING #2's specific ask is unmet.** `cmd_custodian`
  (`crates/server/src/cli.rs:511`) and the real gRPC connector `GrpcDServerConnector`
  (`cli.rs:735`) are referenced by **no test** — only in doc comments
  (`custodian_day_one.rs:11,211,479,960`). The tests drive the layer below
  (`run_reconstruction_over_backend`, `connect_fleet`, `run_reconstruction_until`), which
  the module docstring labels "the exact production path `cmd_custodian` builds"
  (`custodian_day_one.rs:479`). Uncovered glue that only lives in `cmd_custodian`: the
  `--otlp-endpoint`→`ExporterConfig` selection, the election + per-backend fencing
  message, the `let Some(endpoints) = endpoints else { return Ok }` early-out
  (`cli.rs:590`), the `if configured.is_empty() { return Ok }` early-out (`cli.rs:614`),
  and the wiring of `connect_fleet(&GrpcDServerConnector,…)` → `run_reconstruction_over_backend`
  (`cli.rs:605,629`). Iteration-5 asked precisely for "a backend-driven process test
  **THROUGH cmd_custodian**"; the builder again tested one layer down and the iteration-3
  (wrong backend) / iteration-4 (fabricated topology) reject surfaces could regress inside
  this uncovered glue with every gate green.

- **NEEDS-HUMAN — "start degraded and repair around" is start-degraded-**permanently**;
  a startup-unreachable peer is orphaned for the process's whole life.** `connect_fleet`
  (`crates/server/src/custodian.rs:...`, called once at `cli.rs:605`) dials each endpoint
  **once** and drops any that `Err`, so it is never placed in the `configured` Vec.
  `live_reconstruction_view` (`custodian.rs:195-209`) only re-probes members **of
  `configured`** each pass, so a peer down at boot can never rejoin — while a peer that is
  up at boot, dies, then recovers **does** rejoin (re-probed per pass). Asymmetry with a
  concrete failing case: a custodian (re)started during a transient partition permanently
  excludes every peer that was briefly unreachable at that instant; their storage is never
  used again and reconstruction keeps re-placing "their" fragments elsewhere, silently
  eroding failure-domain diversity until the process is restarted. Related: if the boot
  partition is total, `cli.rs:614` returns `Ok` and the **process exits** (no retry),
  contradicting the "survive the incident" narrative — the run loop retries, but startup
  connect does not. Test #6 keeps `e1` down forever, so recovery/orphaning is never
  exercised.

## Attacks I attempted but could not sustain

- **No-timeout hang on a paused peer (iteration-5 concern).** Refuted: `connect_with_timeout`
  (`crates/chunkstore-grpc/src/client.rs:79-91`) applies `.timeout(timeout)` as a
  **per-request** deadline, so `health()`/`get_fragment` on an established-but-silent peer
  returns transient `DEADLINE_EXCEEDED` rather than hanging. `cmd_custodian` uses it
  (via `GrpcDServerConnector`), default 10s.
- **Malformed silently lost after moving it off the gauge.** Refuted: `emit_needs_human`
  (`reconstruction.rs:580-588`) does emit `monotonic_counter.reconstruction_malformed_placement`
  plus a NEEDS-HUMAN audit line, so the corruption is surfaced on its own metric.
- **Gauge cannot return to zero through an accumulating registry.** Refuted for the
  repairable case: `emit_under_replicated` now emits `gauge.` not `monotonic_counter.`
  (`reconstruction.rs:553`), and test #1 reads 1→0 back off `gather_prometheus`. (The
  `Unrepairable` floor above is the surviving break, not the counter/gauge mechanism.)

## Advisory notes (lower severity)

- **Unhealthy-but-reachable peers stay eligible as survivor/replacement targets.**
  `custodian.rs:188,204` keep a server in the fleet on any `Ok(Health)`, including
  `Health::Unhealthy`. This is a standing C5 human item (iteration-4/5 §C5) — reachable ≠
  serving-good-fragments; noting it is still open, not newly introduced.
- **Test #2's red→green is partly an artifact of the counter→gauge rename.** Pre-fix the
  metric exported as `reconstruction_under_replicated_total`; `gauge_value(…,
  "reconstruction_under_replicated")` returns `None` on the name mismatch alone, so the
  assertion `Some(1.0)` goes RED regardless of whether `Unrepairable` was counted. The
  test therefore does not cleanly isolate the "Unrepairable now counts" behavior it claims
  to prove.
- **`require_aligned_topology` checks list *lengths* only** (`cli.rs`), not id uniqueness;
  `--ids 0,0,…` yields two `ConfiguredDServer` with the same `DServerId` and an
  overwritten topology entry. Operator error, low risk, but unguarded.
