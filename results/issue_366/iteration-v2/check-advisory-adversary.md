# Adversarial review — issue 366 / obs-floor-observability (advisory, non-gating)

Attacked the red→green evidence (`crates/server/tests/custodian_day_one.rs`,
`crates/dst/tests/custodian.rs`), the fix (`reconstruction.rs` gauge, the new
`server::custodian::CustodianService` + `cli::cmd_custodian`), and the reviewer's
"green through the wired binary" claim. Grounded on the target source at
`/home/eddie/wyrd/wyrd.pdca-wt-l1`. Findings below; the two `NEEDS-HUMAN` items are the
load-bearing ones.

## Refutations

- **NEEDS-HUMAN — The at-Check test dodges the exact production failure the day-one
  runbook triggers: the deployable custodian *crashes* on a killed D-server.**
  `crates/custodian/src/reconstruction.rs:341` classifies an *unreachable / timed-out*
  fetch (what a killed gRPC D-server produces) as **transient**, so `assess` does
  `Err(e) => return Err(e)` (`reconstruction.rs:274`) and the error propagates
  `reconcile → reconcile_step → CustodianService::reconcile_pass →
  run_until` (`crates/server/src/custodian.rs:727`, `.await?`) → `cmd_custodian`
  (`crates/server/src/cli.rs:541`/`:551`, `service.run_until(...).await?`) → the process
  exits. In production `cmd_custodian` builds the fleet from **all** `--endpoints`
  including the server that will die (`cli.rs:519-523`), so the *first* fetch to the dead
  server after a §7.4-step-4 kill returns transient → the custodian terminates on the very
  fault it exists to repair around. The test never hits this: it hand-builds a
  `healthy_fleet` that **excludes** the killed server (`custodian_day_one.rs:1074-1079`),
  so `stores.get(&dead)` is `None` → treated as a missing shard → read-around succeeds.
  The "runnable custodian delivers the day-one signal" claim is validated only for a fleet
  that has already been curated to omit the dead node — i.e. the production path is not
  exercised end-to-end. A human must decide whether the runnable role is fit for #367's
  day-one runbook before sign-off.

- **NEEDS-HUMAN — The gauge repurposed as *the* binding durability signal undercounts the
  worst losses; it reads 0 for a chunk that has lost redundancy beyond tolerance.**
  `crates/custodian/src/reconstruction.rs:170` feeds `emit_under_replicated(plans.len())`,
  and `plans` holds only `Assessment::Repairable` chunks (survivors ≥ k). Chunks with
  survivors **< k** (`Assessment::Unrepairable`, `reconstruction.rs:145,300-303`) and
  malformed placements (`:149`) are excluded. The diff renames this to a **gauge** and its
  new doc (`reconstruction.rs:504-514`) asserts it is "the number of chunks currently
  under-replicated as of this pass" — a *level*. Concrete failing case: kill two fragments
  of an RS(2,1) chunk (survivors = 1 < k = 2) → `plans` empty → the gauge emits **0** while
  the chunk sits queued and un-reconstructable (`emit_queue_depth` shows 1, but the
  under-replicated gauge shows 0). The counter→gauge/"level" reinterpretation this diff
  introduces is precisely what turns the pre-existing repairable-only count into a
  *semantic* mismatch: an operator watching "under-replicated rises then returns to zero"
  sees zero for the most severe, most-attention-worthy durability event. The binding
  success criterion ("the under-replicated count RISES … observable via the emitted
  metric") holds only for auto-repairable loss.

## Weaker notes (not blocking, but the reviewer may have over-credited them)

- The **DST test change is not evidence for the fix's core claim.** `MetricCapture`
  (`crates/dst/tests/custodian.rs:1016-1049`) reads the **raw emitted event value** (1 then
  0), which a `monotonic_counter` emitted identically — it never touches the OTel/Prometheus
  export path. The whole "a gauge is needed because an accumulating counter stays pinned at
  1" argument (`reconstruction.rs:510-514`) is proven *only* by
  `custodian_day_one.rs`'s `gather_prometheus` read-back. The DST field rename
  (`custodian.rs:1023,1046`) corroborates nothing about the export/return-to-zero semantics.

- `crates/server/src/custodian.rs:606-608` claims "the process-global `set_global_default`
  install belongs to the binary entry point (`cli::cmd_custodian`)", but `cmd_custodian`
  (`cli.rs:461-557`) installs **no** global subscriber — only the scoped, metrics-only
  `MetricsLayer` per pass. So brief item 1 ("tracing subscriber wired at role entry") is not
  actually met by the runnable role, and every non-metric event the loops emit — the
  `reconstruction.audit` lines and the **NEEDS-HUMAN** malformed-placement warning
  (`reconstruction.rs:543-551`) — is dropped in production (no subscriber captures them).
  Blessed as deferred logging (item 3), but the code comment asserting it happens is
  unwarranted.

- `cmd_custodian` assigns `DServerId` by `--endpoints` index (`cli.rs:519-527`,
  `topology.register(i, "domain-{i}")`). If a chunk's committed `placement` vector was
  written with a different id ordering, the custodian's fleet keys won't match the placement
  ids → real survivors resolve to `None` and are mis-assessed as missing. Not exercised by
  the in-memory test (ids are chosen to align). Secondary to the crash finding above.

- `run_until` (`custodian.rs:700-733`) elects once and never renews the lease; a
  long-running custodian past its lease TTL is fenced (writes rejected, no corruption) but
  keeps looping. Deferred per the diff's own note; flagged for completeness.

## What I attempted and could NOT refute

- The red→green is **genuine**, not a tautology: pre-fix `monotonic_counter.` exports as
  `reconstruction_under_replicated_total`, so `gauge_value(…, "reconstruction_under_replicated")`
  (`custodian_day_one.rs:1029-1045`) is `None` → RED; post-fix the gauge exports the
  un-suffixed name → 1.0/0.0 → GREEN. The test reads back off the role's own Prometheus
  surface, and `reconcile_pass` is the same call `run_until`/`cmd_custodian` drive — it is
  the production wiring, not a parallel re-implementation.
- The return-to-zero is real for the tested scenario: `emit_under_replicated(plans.len())`
  fires unconditionally each pass (`reconstruction.rs:170`), and pass 2's drained queue
  yields `plans.len() == 0`.
- Pinned versions (`tracing-opentelemetry 0.33`, `opentelemetry 0.32`,
  `Cargo.toml:138-143`) support the synchronous `Gauge` and the `gauge.` field prefix, so
  the bridge mechanism is sound.
- Did not execute the suite here (read-only, no build attempted); the C4 gates assert
  `xtask ci` and run-verify pass. The findings above are source-grounded, not build-derived.
