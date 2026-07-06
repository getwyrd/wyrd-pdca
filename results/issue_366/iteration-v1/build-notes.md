# Build notes — issue 366 / obs-floor-observability (keystone slice: items 1–2)

**Target branch:** `getwyrd/wyrd @ feat/m4-production-metadata-backend`
(worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`, detached at
`origin/feat/m4-production-metadata-backend`). All `path:line` citations below are
against that tree, post-edit.

## What the brief asked for (and the scope I built)

The brief is a **milestone-scoped pointer** to proposal 0010 (observability floor,
seven independently-PR-able slices). It names exactly ONE at-Check demonstrable, the
**BINDING** success criterion:

> through the **wired, runnable custodian role** (not the library alone), after an
> injected fragment loss the **under-replicated / durability count RISES and then
> returns to ZERO**, observed via `DurabilityTelemetry::gather_prometheus`.

and says explicitly it *does not fit one `patch.diff`* — the load-bearing keystone is
**items 1–2** (telemetry handle wired at role entry + a runnable custodian role that
runs the loop through it). I built exactly that keystone. Items 3–7 (EnvFilter logging,
request-plane RED, capacity-plane signals, typed errors, `tonic-health` readiness) and
the `wyrd custodian` **binary** in the `server` crate are the follow-on slices, left for
their own bundles (see NEEDS-HUMAN below). I invented no scope beyond items 1–2.

## Root cause — the concrete gap behind the binding criterion

On the target branch the durability plane already exists as a **library**:
`DurabilityTelemetry` / `ExporterConfig` / `gather_prometheus`
(`crates/custodian/src/telemetry.rs:31,67,76,135`), and the reconstruction loop already
emits the under-replicated count (`crates/custodian/src/reconstruction.rs`). Two things,
together, meant the binding day-one signal was **not actually observable through the
production export surface**:

1. **No role tied the telemetry handle to the running loop.** The loops are driven by
   the fenced `reconcile_step` control point (`crates/custodian/src/reconciliation.rs:65`),
   but nothing *owned* a telemetry handle and ran the loop *through* it. Every test that
   wanted the emission had to hand-assemble a subscriber inline
   (`crates/custodian/tests/gc_telemetry.rs:185-189`). "not the library alone" was
   literally unmet: there was no role object.

2. **`emit_under_replicated` was a MONOTONIC COUNTER, which can never return to zero.**
   `tracing::info!(monotonic_counter.reconstruction_under_replicated = count)`
   (old `reconstruction.rs`). Through an accumulating Prometheus registry a counter does
   `add(1)` then `add(0)` → stays pinned at **1** and is exported as
   `reconstruction_under_replicated_total`. So via `gather_prometheus` a repaired zone
   still reads *permanently degraded*. The "rises then returns to zero" shape was only
   ever provable through the DST campaign's **bespoke `MetricCapture`** layer
   (`crates/dst/tests/custodian.rs:344-388`), which records each emitted *value* (1, then
   0) — NOT the accumulated counter. That is precisely the "sim-only gap" 0010 names.

The under-replicated count is a **level** — "how many chunks are under-replicated right
now" — which is a gauge, not a running total. The monotonic counter was the latent defect
that made the day-one signal unobservable through the real seam.

## The fix (smallest change that restores the invariant)

The brief frames items 1–2 as an **invariant to restore** ("the durability signal must be
observable as rise-then-zero through the running process"), so the target is the smallest
change that restores it, not the smallest diff.

1. **Gauge, not counter** — `crates/custodian/src/reconstruction.rs:515-516`:
   `tracing::info!(gauge.reconstruction_under_replicated = count as u64)`. `gauge.` maps
   to an OTel `Gauge<u64>` (verified in the pinned `tracing-opentelemetry-0.33`
   `metrics.rs:24,138-140,176`), which exports the **last recorded value** — 1 after the
   loss, 0 after repair. Purely a change of instrument *type* on an existing emission
   point; no loop logic changed (Invariant: "emission points, not new logic"). Semantics
   are now correct: a level that returns to zero. `gauge.` is already the codebase's
   idiom for a level (`rebalance.rs:321`).

2. **The runnable custodian role** — new `crates/custodian/src/role.rs`,
   `pub struct CustodianRole` (`:54`), re-exported at
   `crates/custodian/src/lib.rs:30,44`. It **owns** a `DurabilityTelemetry` handle
   (item 1, installed at `CustodianRole::new`, `role.rs:68-69`) and runs the fenced loop
   through it: `reconcile_pass` (`role.rs:96`) delegates to the *real* `reconcile_step`
   (never a parallel entry — the anti-#141 guard) and installs the handle's
   `tracing`→OpenTelemetry metrics bridge **scoped for the pass** via
   `WithSubscriber::with_subscriber` (`role.rs:115`). The durability metrics the loops
   emit therefore land in *this role's* provider and are observable through
   `role.telemetry().gather_prometheus()`.

   **Why scoped `with_subscriber`, not a global `set_global_default`:** ADR-0035's
   host-side statics gate (`xtask/src/main.rs:658-663`, scanning
   `STATICS_SCAN_CRATES` incl. `crates/custodian`) forbids DST-reachable global mutable
   state; `set_global_default` in a library `src/` file would fail that gate. The
   process-global subscriber install belongs to the **binary entry point** (the `server`
   slice), not this library seam. The role builds its `Dispatch` **once** at construction
   (`role.rs:69`) and clones it per pass (`Dispatch` is `Arc`-backed), so both passes
   record into the same instruments and one callsite-interest registration covers the
   role's lifetime.

No commit protocol, consistency contract, or on-disk format touched — purely additive
instrumentation + wiring (Invariant held). No concrete backend leaked: the export surface
stays behind `ExporterConfig`, chosen by the caller (Invariant held, ADR-0012). The
telemetry seam is reused, not rebuilt (Invariant held).

## Test — red → green, driving PRODUCTION code

`crates/custodian/tests/reconstruction_telemetry.rs`
(`under_replicated_gauge_rises_then_returns_to_zero_via_gather_prometheus`, `:298`).
It builds a `CustodianRole` over a Prometheus telemetry handle (`:342`), kills D-server 1
(architecture §7.4 day-one step 4), and runs two fenced passes through
`role.reconcile_pass` (`:346,:369`), reading `reconstruction_under_replicated` back off
`role.telemetry().gather_prometheus()` (`:357,:384`):
pass 1 → **1**, pass 2 → **0**. It drives the real production path
(`CustodianRole → reconcile_step → reconstruction::reconcile → emit_under_replicated →
DurabilityTelemetry → gather_prometheus`); the `MemMeta`/`MemDServer`/`Fleet` are the
standard in-memory *trait doubles* the whole custodian suite uses, not a re-implementation
of production.

**Placement (path is ILLUSTRATIVE per the brief).** I put it in the **custodian crate**
as a plain `#[tokio::test]`, not `crates/dst/tests/`. Rationale:
- `crates/dst` compiles **only** under `--cfg madsim` (`crates/dst/Cargo.toml:47`), and the
  M3 authors deliberately kept the OTel runtime **out** of the simulator, using a bespoke
  `MetricCapture` instead (`crates/dst/tests/custodian.rs:342-343`). Driving the real OTel
  Prometheus provider through `gather_prometheus` — which is the whole point of the binding
  criterion — is exactly what they avoided under madsim.
- The custodian crate already has the **proven** template for real-`gather_prometheus`
  telemetry tests as plain tokio tests, in their own binaries:
  `gc_telemetry.rs`, `backfill_telemetry.rs`. My file mirrors that convention (own binary,
  documented callsite-interest reason at `reconstruction_telemetry.rs:30-33`). It runs in
  `cargo xtask ci`'s `cargo test --workspace` step (`xtask/src/main.rs:817`), deterministic
  and headless.

**Red evidence (two independent reds):**
- *Compile red (no patch):* the test references `CustodianRole`, which does not exist
  pre-patch → the test binary fails to compile.
- *Runtime red proving the gauge is load-bearing (recorded, reproduced):* with the role
  present but `emit_under_replicated` reverted to `monotonic_counter`, the metric exports as
  `reconstruction_under_replicated_total` (a counter pinned at 1) — `gauge_value(...)` finds
  no matching gauge family and the *first* assertion already fails
  (`left: None, right: Some(1.0)`), and the counter never returns to zero. I reproduced this
  by flipping the one line and re-running, then restored the gauge.

**Green evidence:**
- `cargo test -p wyrd-custodian --test reconstruction_telemetry` → `1 passed`.
- Whole custodian suite green (`backfill*`, `gc*`, `rebalance`, `reconstruction` (12),
  `reconstruction_telemetry` (1), `scrub`, `skeleton`, `tier1_disk_faults`).
- DST property 6 under madsim (`RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=3 cargo test -p
  wyrd-dst --test custodian durability_emission_rises_then_returns_to_zero`) → `ok` with the
  updated `gauge.reconstruction_under_replicated` field name
  (`crates/dst/tests/custodian.rs:1023,1046`).

The existing `reconstruction.rs::emits_the_three_repair_metrics_on_the_durability_seam`
asserts `exposed.contains("reconstruction_under_replicated")`, which still matches the gauge
family name (the gauge drops the `_total` suffix a counter carried) — verified still green.

## Alternatives considered and rejected

- **Keep the monotonic counter; assert rise→zero only via the DST `MetricCapture`
  (per-event values).** Rejected: that is the status quo the brief calls the "sim-only gap."
  It cannot satisfy the binding criterion, which requires the signal read back off
  `gather_prometheus` — where a counter is provably stuck at 1 (reproduced above). Cost of
  keeping it: the day-one operator signal is a lie (repaired zone reads degraded forever).
- **Guard the symptom by post-processing the counter in the scrape endpoint (compute a
  "current" value by differencing).** Rejected: it guards a symptom and pushes durability
  semantics into a not-yet-built endpoint; the *cause* is that the instrument type is wrong.
  A one-line gauge change removes the cause.
- **Wire the full `wyrd custodian` binary in the `server` crate now (item 2's deployment
  half).** Rejected as out-of-keystone: the brief states the issue "does not fit one
  `patch.diff`" and names items 1–2's *signal* as the sole at-Check demonstrable. The binary
  wiring pulls in the continuous run loop, leadership lifecycle, EnvFilter logging (item 3),
  and `tonic-health` (item 7) — each its own 0010 slice. `CustodianRole` is the reusable
  library seam those slices compose onto.
- **`set_global_default` inside the role for a simpler API.** Rejected: fails the ADR-0035
  statics gate for a DST-reachable production crate (`xtask/src/main.rs:640-663`); scoped
  `with_subscriber` is both gate-clean and the correct home for the global install (the
  binary, not the library).

## Commit-readiness

- `cargo fmt -p wyrd-custodian -- --check` and `cargo fmt -p wyrd-dst -- --check` → clean.
- `cargo clippy -p wyrd-custodian --tests -- -D warnings` → clean (fixed one
  `clippy::double_ended_iterator_last`: `.last()` → `.next_back()` in the gauge parser).
- The full host gate (`cargo xtask ci`, incl. fmt/clippy/deny/conformance/DST sweep) is the
  Check gate and was not run end-to-end here (it sweeps 50 DST seeds); the targeted
  compile/clippy/fmt/test checks above cover the touched surface.

## Known NEEDS-HUMAN / carry-forward for later slices

- **Milestone decomposition.** This bundle carries the recommended keystone (items 1–2). The
  human/driver decides how the remaining 0010 slices (3–7) map onto follow-on bundles.
- **Typed-errors × M4.4 (#255) sequencing** (0010 item 6) — must be *recorded* before the two
  run in parallel; not the builder's call. Untouched here.
- **Shared `crates/telemetry` extraction vs keep-in-`custodian`** — deferred by 0010 to
  implementation; not forced by this keystone (the role lives in `custodian`).
- **Live-exporter evidence is off-Check** — a real Prometheus scrape / OTLP collector run on
  a Tier-2 node is the pre-agreed sign-off item, not a surprise; this bundle proves the
  in-process `gather_prometheus` read-back only.
- **`wyrd custodian` binary + continuous loop / leadership lifecycle** — item 2's deployment
  half, deliberately deferred; `CustodianRole::reconcile_pass` is the seam it will drive.
