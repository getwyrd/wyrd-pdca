# Adversarial review — issue 366 (obs-floor / deployable custodian), iteration 7

Lens: refute the red→green evidence and the reviewer's verdict. Grounded on the target
source at `/home/eddie/wyrd/wyrd.pdca-wt-l0` (patch applied). Advisory only — I gate nothing.

The keystone change this iteration is the split: `Unrepairable` now routes to a new
high-severity `emit_data_loss` signal and is removed from `reconstruction_under_replicated`,
which becomes a **gauge**. I attacked that split and its surrounding claims.

## Findings

- **NEEDS-HUMAN — a transient multi-node outage is falsely reported as permanent DATA LOSS.**
  `crates/server/src/custodian.rs:204` (`live_reconstruction_view`) *drops* any D-server whose
  `health()` probe errs, so an unreachable server never enters the `stores` map. In
  `crates/custodian/src/reconstruction.rs:305,322` a placement entry for a dropped server hits
  the `None => None` arm and is counted **missing** — not as the transient fault `assess`
  otherwise carefully propagates (`reconstruction.rs:313-320`). When enough servers are
  *transiently* down that survivors `< k`, `assess` returns `Unrepairable`
  (`reconstruction.rs:346-348`) and the patch fires `emit_data_loss`
  (`reconstruction.rs:179` → `:618-625`) at `error` severity: `"…DATA IS LOST; NEEDS-HUMAN"`.
  Concrete failing case: RS(2,1), a rolling restart of two D-servers (or a partition isolating
  `m+1` nodes) → every chunk with a fragment on each downed node is flagged permanent data loss
  and increments `reconstruction_data_loss`, though all fragments are physically intact and
  fully recover when the nodes return. The new signal escalates a recoverable reachability gap
  into the system's most severe (false) alarm — and the `live_reconstruction_view` drop-path
  deliberately bypasses the very transient/permanent distinction `assess` implements.

- **NEEDS-HUMAN — the binding "returns to ZERO" does not hold for a `Repairable` chunk whose
  repair repeatedly aborts.** `reconstruction.rs:163-165` increments `under_replicated` for
  **every** `Repairable` chunk, and `:216` emits that as the gauge — *before* the repair loop
  (`:222-227`). A repair that yields `Aborted` (selector found no valid distinct domain — e.g.
  a minimal cluster at exactly `n` servers for RS(k,m), one killed, no free domain for the
  rebuilt fragment) leaves the obligation queued (`:225-226` offset it and it is re-assessed).
  Next pass it re-classifies `Repairable` and re-counts, so the gauge is pinned at ≥1 forever —
  the same "floored gauge" failure the iteration-5/6 rejections chased, now surviving in the
  `Repairable`-but-never-repaired case the patch did not close. Every day-one test provides a
  spare server (`custodian_day_one.rs` tests 1/2/3/5/6 all build servers 0–3 and kill one of
  0–2, leaving server 3 as a free domain), so the no-free-domain abort loop is uncovered. The
  binding signal is only demonstrated where repair is guaranteed to succeed.

- **NEEDS-HUMAN — `cmd_custodian`'s own backend-open + run-loop glue is still not exercised
  end to end.** The iteration-5/6 T5c ask ("a backend-driven process test THROUGH
  `cmd_custodian`") is only half met. `cmd_custodian` reaches `run_reconstruction_over_backend`
  (→ `open_local_meta_redb`) at `crates/server/src/cli.rs:643`, but the two tests that drive the
  real entry point never get there: `cmd_custodian_rejects_misaligned_topology…` errors at
  `require_aligned_topology`, and `cmd_custodian_starts_degraded_on_an_unreachable_fleet…`
  hits the empty-fleet early return at `cli.rs:628` *before* the backend is opened. The
  backend-open path is covered only via the factored helper (`custodian_day_one.rs` test 5,
  `run_reconstruction_over_backend` directly). So the exact iteration-3 reject surface — the
  custodian opening the wrong metadata plane inside `cmd_custodian`'s own `block_on` closure —
  can still regress behind green gates. Ratify or add the through-`cmd_custodian` backend test.

## Weaker / advisory

- **The `emit_data_loss` NEEDS-HUMAN audit line is dropped in the deployed role.**
  `reconstruction.rs:620-625` writes the human-readable, chunk-id-bearing audit line via
  `tracing::error!`, but `CustodianService` installs only a **metrics** bridge, not a log
  subscriber (admitted at `custodian.rs:34`). In production only the `reconstruction_data_loss`
  *counter* survives; the actionable "which chunk is lost" line goes to no sink. The docstring's
  claim of "a NEEDS-HUMAN audit line, at least the parity `emit_needs_human` gives"
  (`reconstruction.rs:611-612`) overstates production value — both audit lines vanish until the
  deferred log-subscriber slice (item 3) lands.

- **The metric-type change is not "purely additive."** `reconstruction.rs:568` changes the
  M3-published `reconstruction_under_replicated` from `monotonic_counter` (`…_total`) to
  `gauge`. That is a breaking change to an existing metric's contract (brief invariant,
  `brief.md:153` "Purely additive instrumentation"): any existing scrape/recording rule keyed on
  `reconstruction_under_replicated_total` now reads nothing. It is arguably necessary for the
  return-to-zero shape, but it is a contract change the reviewer should have called out, not an
  additive one.

- **The evidence: per-fix red→green was never mechanically run.** `check-gates.json:41-49`
  (`C4-verify`) is still `fail`: `pathspec 'crates/telemetry/src/lib.rs' did not match any
  file(s) known to git` — the new-crate rename artifact (`patch.diff:3026-3029`, a `git mv` of
  `custodian/src/telemetry.rs`). This has been red since iteration 3. The asserted red→green is
  therefore *reasoned in the tests*, not demonstrated by the harness; the only "red" the harness
  produced is a rename/compile error, not the defect. The confirmatory gate that actually passed
  (`C4-ci`) proves the suite is green *post*-fix but establishes nothing about the pre-fix red.

## Attempted but could not refute

- The single-server day-one drill (`custodian_day_one.rs` test 1) genuinely reads the gauge
  `1` then `0` on **one** continuous `DurabilityTelemetry` provider (not a fresh provider per
  pass), so the "gauge, not counter" return-to-zero claim holds on the same export surface for
  the in-scope drill.
- `under_replicated_gauge_excludes_malformed…` / `…_unrepairable…` (`reconstruction.rs` tests)
  and day-one test 2 do exercise a *populated* store and show the backlog gauge returns to 0
  while a malformed/lost chunk persists — the iteration-5/6 floor concern is addressed for those
  two classifications.
- `require_aligned_topology` (cli.rs) plus its unit test and day-one test 7 do close the
  iteration-4 fabricated-topology reject at the real entry point.
