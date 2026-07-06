# Check review — issue 366 / obs-floor-observability (keystone slice)

**Task under review:** Deliver the observability-floor keystone (proposal 0010 items 1–2):
extract the backend-agnostic telemetry seam out of `custodian` into a shared `crates/telemetry`
crate, wire `wyrd custodian` as a *runnable, deployable* process role in `server`, and prove the
day-one durability signal — kill a D-server → the under-replicated count **rises then returns to
zero** — through the wired role's real Prometheus export surface (`gather_prometheus`). Items 3–7
(log-level, RED, capacity, typed errors, tonic-health) are deferred follow-on slices.

**Grounding note:** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l1` is readable and holds the
**post-patch** state (`crates/telemetry/src/lib.rs` present; `crates/custodian/src/telemetry.rs`
deleted; `crates/server/src/custodian.rs` present), so citations ground on the target. `cargo`
build/test re-run is gated in this sandbox (permission-blocked), so C4 leans on the green
deterministic gate (`check-gates.json`: C4-ci PASS gating, C4-verify PASS) plus source grounding,
not an independent re-run.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is a pointer to proposal 0010; keystone scope (items 1–2 + the day-one signal) and the binding success criterion are unambiguous. Patch targets exactly that keystone, defers 3–7 as the brief directs. |
| C2 Reproduction (red pre-fix) | PASS | No C2 gate configured, but the regression is real: pre-fix `emit_under_replicated` used `monotonic_counter.` (exported `_total`, pinned at 1, never returns to zero), so the un-suffixed gauge query in `custodian_day_one.rs:1113` reads `None`/1 → RED. C4-verify gate re-derived "red without the fix." Independent re-run env-gated. |
| C3 Change | PASS | Diff grounds on target: gauge switch at `crates/custodian/src/reconstruction.rs:515-516`; new `crates/telemetry/src/lib.rs`; runnable role at `crates/server/src/cli.rs:488` (`cmd_custodian`) + `crates/server/src/custodian.rs`. Additive, reuses the extracted seam. |
| C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci PASS (gating, `xtask ci` fmt/clippy/build/test/deny/conformance) and C4-verify PASS ("red…green"). Target source matches the patch. Independent `cargo test` re-run was permission-blocked in this env — a sandbox limit, **not** a patch defect; no stale-target caveat (target is post-patch). |
| C5 Causal adequacy | PASS | Genuine root-cause fix, not a symptom guard: the level metric is a *gauge* (returns to zero) and the library seam is *wired into a runnable process* — cause removed/transformed, not papered over. Symptom-guard smell-test does **not** fire (no capability probe / hasattr / try-import; `match otlp-endpoint` is config selection, `gather_prometheus`→`Option` is a legitimate "no-surface" case). |
| T1 Structure | PASS | Clean extraction: `wyrd-telemetry` owns the OTel/Prometheus deps, `custodian` re-exports `DurabilityTelemetry`/`ExporterConfig`/`TelemetryError` so M3 consumers compile unchanged (`crates/custodian/src/lib.rs:43-46`); role wiring isolated in `server/src/custodian.rs`. Minor: stale comment `crates/server/src/cli.rs:53` "The CLI runs no custodian sweep" is now inaccurate — non-blocking cleanup. |
| T2 Shape | PASS | Matches ADR-0010/0012: no backend hardcoded (`ExporterConfig` chosen at role entry), seam behind a leaf-crate-free boundary, per-pass scoped `Dispatch` install rather than a global default. Additive instrumentation only; `traits/lib.rs` untouched (`:59` BoxError, `:86` IntegrityFault, `:224` Health all unchanged). |
| T3 Runtime | PASS | Role runs the fenced `reconcile_step` (no parallel entry, anti-#141), tokio runtime for OTLP transport, Ctrl-C shutdown, wall-clock stamps. Reviewed statically; not runtime-exercised here (cargo gated) — the day-one test is the runtime evidence and the gate is green. |
| T4 Contribution | PASS | Delivers the load-bearing keystone (items 1–2 + the gauge fix that makes the day-one signal expressible) through the wired binary path `cmd_custodian → run_until → reconcile_pass`, asserted off the role's own export surface. Correctly leaves 3–7 to their own slices. |
| T5 Judgment | NEEDS-HUMAN | Two judgment calls the brief reserves for the human ride on this bundle: (a) **milestone decomposition** — confirm this keystone bundle is the intended slice #1 of 0010's 7-PR sequence; (b) **typed-errors × #255 (M4.4) sequencing** — 0010 requires this decision be *recorded* before item 6 lands (this patch doesn't touch the trait seam, so nothing forces it yet, but the record is owed). The extract-vs-keep call is resolved (maintainer chose extract, iteration-1 carry-forward) and the patch honors it. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Does the runnable `wyrd custodian` + rise-then-zero gauge satisfy #367's day-one runbook as the real operational-visibility floor? The **live-exporter evidence is off-Check by design** (a Prometheus-scrape / OTLP-collector run against the blueprint on a Tier-2 single node) — pre-agreed sign-off item, needs an operator/CI-eval host, not reproducible from artifacts here. Human decides fitness at sign-off. |
