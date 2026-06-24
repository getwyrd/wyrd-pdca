# Build notes — issue 197 / reconstruction-repaired-success-identity

## Root cause (two sentences)

`emit_repaired` fires once per plan **up front** (`reconstruction.rs:163-165`, before the
rebuild/commit loop), so `reconstruction_repaired` counts every plan the pass *attempts*.
In the outcome loop the `Conflict` arm is offset by `emit_conflict` (`:171`) but the
`Aborted` arm (`:172`) was offset by nothing — so the documented identity
`successes = reconstruction_repaired − conflict` (`:160-161`, `:433` on the target's base)
over-counts true successes by the number of Aborted plans.

## Fix — add a distinct `reconstruction_aborted` offset (the brief's second ILLUSTRATIVE option)

Smallest change that restores the **invariant** (durability-plane success identity =
count of `Committed` plans), keeping the load-bearing constraint:

- `reconstruction.rs` `RepairOutcome::Aborted => emit_aborted(plan.chunk_id)` (was `{}`).
- New `emit_aborted` mirrors `emit_conflict`: `monotonic_counter.reconstruction_aborted = 1`
  + an audit event. Aborted is now offset on its own counter, exactly like Conflict.
- Documented identity updated in the two places it lived in-code (the `reconcile` comment
  and the `emit_repaired` doc) to `repaired − conflict − aborted`, plus the
  `RepairOutcome::Aborted` doc.

Post-fix: `repaired(2) − conflict(0) − aborted(1) = 1 == committed_count(1)`.

## Why this approach, and why NOT the other ILLUSTRATIVE option

The brief offered two routes and named the deciding constraint: the up-front emission is
deliberate because the `tracing`→OTel bridge can drop events emitted *after* the heavy
erasure-decode/commit section under load, **so the fix must not reintroduce unreliable
late emission of the metrics that matter** (the brief, and `reconstruction.rs:152-161`).

- **Rejected: gate `emit_repaired` on `Committed`.** This is the option the constraint
  rules out. To know an outcome is `Committed` you must run `repair_chunk` first, so the
  `emit_repaired` call (the metric that *matters* — the durability-failure signal) would
  move from `:163-165` (assessment frame, up front) into the loop's `Committed` arm —
  i.e. *after* the heavy decode/commit section. That is precisely the unreliable late
  emission the brief forbids: under load the dropped event would now *under*-count true
  successes (a silent durability blind spot), which is worse than the current over-count.

- **Chosen: a distinct aborted offset.** `reconstruction_repaired` — the metric that
  matters — stays emitted up front, untouched. The new `reconstruction_aborted` is a pure
  *offset*, emitted late in the `Aborted` arm. This is **symmetric with the existing
  `reconstruction_conflict`**, which is already emitted late in the loop (`:171`) and
  which the brief explicitly accepts as the model ("recorded on the separate
  `reconstruction_conflict` counter"). A late offset that is dropped degrades identically
  to a dropped conflict offset — the established, already-accepted failure mode — and the
  metric that matters is never late. So this respects the constraint where gating does not.

This is a behavioural accounting fix (principles.md §1.1), not a structural one — no
lifecycle/structure change, no Plan-exit structural gate.

## Scope held to one logical change

The brief's out-of-scope `time_to_repair` elapsed-window defect (`:436` records the pass's
absolute `now_millis`, not an elapsed repair window) is **not** touched here — it needs a
per-obligation enqueue stamp threaded through the shared repair queue's value encoding (a
data-model change), self-flagged at `:427-431`. File/track separately; not bundled.

The identity lives **only** in-code (grep across `docs/` + `crates/` found it at
`reconstruction.rs:160` and `:433` only). Proposal 0005 §319-344 lists the five named
durability metrics but does not state this `repaired − conflict` identity, and
`reconstruction_repaired/conflict/aborted` are implementation sub-counters of the
"time-to-repair / dispatched-repair" metric, not new proposal-level metrics — so no
proposal/ADR edit is in scope. Updating the in-code documented identity is the binding
"update the documented identity" the brief asks for.

## Test — `crates/custodian/tests/reconstruction.rs::an_aborted_repair_is_not_counted_as_a_successful_repair`

Constructs a single reconstruction pass with a **mix of outcomes** (one `Committed`, one
`Aborted`) over the telemetry-capture harness (`tracing_subscriber` + `DurabilityTelemetry`
Prometheus surface — the same stack as the existing `emits_the_three_repair_metrics_…`
test), then asserts the BINDING identity: `repaired − conflict − aborted == committed_count`.

Engineering the mixed pass (both chunks RS(2,1), both placed on servers 0,1,2):
- **Committed chunk** loses its domain-C fragment (server 2) → survivors on A,B → the
  selector's free domains are {C, G}, which tie on utilization and resolve to C by label →
  rebuilt fragment re-places on server 2 (in the fleet) → `Committed`.
- **Aborted chunk** loses its domain-B fragment (server 1) → survivors on A,C → free
  domains {B, G}; domain B (server 1) is loaded (`set_utilization(1, 100)`) so the
  least-utilized free domain is the **ghost** G (server 7), which the topology knows but
  the fleet does NOT hold → `repair_chunk` hits `stores.get(&target) == None` →
  `RepairOutcome::Aborted` (`reconstruction.rs:330-334`). This is the *only* path that
  yields `Aborted`; `InsufficientDomains` would be a propagated `Err`, not an `Aborted`.

`committed_count` is observed independently of the metric (drained obligation + inode
version bump: 2 enqueued − 1 still queued = 1), so the assertion is not a magic constant.

The test is **fix-approach-agnostic in the green direction**: it asserts the invariant, so
it would also pass had Do gated on `Committed` (repaired=1) — it is RED *only* for the
buggy state. `counter_total` accepts the metric with or without the Prometheus `_total`
suffix and sums all sample lines, so it does not depend on exporter naming conventions.

## Red→green proof (project's cargo, headless, load-light)

- Post-fix: `cargo test -p wyrd-custodian --test reconstruction an_aborted_…` → `ok. 1 passed`.
- Pre-fix (stashed just `reconstruction.rs`, `Aborted => {}`): `FAILED … left: 2 right: 1`
  — derived successes 2 vs committed 1, over-counted by the one Aborted plan. Fix restored.
- `cargo fmt --check -p wyrd-custodian` clean; `cargo clippy -p wyrd-custodian --tests
  --all-targets -- -D warnings` clean (commit-ready for the target's fmt+clippy hooks).

The Prometheus read-back during the run confirmed the live exposition format
(`reconstruction_repaired_total{otel_scope_name="tracing/tracing-opentelemetry"} 2`),
which `counter_total` parses correctly.

## Citations (target branch `getwyrd/wyrd @ main`, worktree base `0371177`)

- Defect: `crates/custodian/src/reconstruction.rs:163-165` (up-front emit), `:172` (the
  unoffset `Aborted` arm), `:160-161` / `:433` (the documented identity), `:330-334` (the
  abort condition).
- Fix: `reconstruction.rs:175` (`Aborted => emit_aborted(…)`), new `emit_aborted`
  (`:459-475`), identity comments updated at the same sites.
- Invariant source: proposal 0005 §326-332 / §319-344
  (`docs/design/proposals/accepted/0005-milestone-3-custodians.md`), ADR-0011
  (`docs/design/adr/0011-durability-telemetry-and-declarative-management.md`).
