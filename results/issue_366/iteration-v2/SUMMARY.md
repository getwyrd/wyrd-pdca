# Result — issue 366 / obs-floor-observability

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: the minimum operational-visibility floor (0010 §"Scope boundary" items 1–7):

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (for the keystone slice — items 1–2 delivering the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

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

### Advisory — adversary

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

### Advisory — codex

- `crates/server/src/cli.rs:551` — The runnable custodian rebuilds its reconstruction `Topology` from the positional `--endpoints` list and assigns synthetic one-endpoint-per-domain labels (`domain-{i}`), instead of using the D-server registration's stable id and `failure_domain` (`crates/server/src/dserver.rs:138`). That can make repair place a rebuilt shard into a domain that is not actually distinct from the survivors, undermining the failure-domain guarantee enforced by `select_distinct_domains_excluding`.
- NEEDS-HUMAN — `crates/server/src/cli.rs:518` — `wyrd custodian` advertises "single-active" leadership, but constructs a fresh process-local `MemCoordination`; that backend documents that leadership is always granted to the lone process (`crates/coordination-mem/src/lib.rs:184`). Two deployed custodian processes therefore do not fence each other and can both run reconstruction. If this slice is allowed to defer cross-process coordination, record that explicitly at sign-off; otherwise the deployable role is not actually leader-elected across processes.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Two judgment calls the brief reserves for the human ride on this bundle: (a) **milestone decomposition** — confirm this keystone bundle is the intended slice #1 of 0010's 7-PR sequence; (b) **typed-errors × #255 (M4.4) sequencing** — 0010 requires this decision be *recorded* before item 6 lands (this patch doesn't touch the trait seam, so nothing forces it yet, but the record is owed). The extract-vs-keep call is resolved (maintainer chose extract, iteration-1 carry-forward) and the patch honors it.
- [ ] Validation — fitness-to-purpose — Does the runnable `wyrd custodian` + rise-then-zero gauge satisfy #367's day-one runbook as the real operational-visibility floor? The **live-exporter evidence is off-Check by design** (a Prometheus-scrape / OTLP-collector run against the blueprint on a Tier-2 single node) — pre-agreed sign-off item, needs an operator/CI-eval host, not reproducible from artifacts here. Human decides fitness at sign-off.
- [ ] `crates/server/src/cli.rs:518` — `wyrd custodian` advertises "single-active" leadership, but constructs a fresh process-local `MemCoordination`; that backend documents that leadership is always granted to the lone process (`crates/coordination-mem/src/lib.rs:184`). Two deployed custodian processes therefore do not fence each other and can both run reconstruction. If this slice is allowed to defer cross-process coordination, record that explicitly at sign-off; otherwise the deployable role is not actually leader-elected across processes.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: the day-one durability signal is only demonstrated on a curated happy path, and the metric that IS the signal blind-spots the most severe event. The rebuild must CORRECT the following. Must be corrected: - Adv-1: the deployable custodian crashes on a killed D-server. An unreachable/ timed-out fetch is classified transient (reconstruction.rs:341), so the error propagates reconcile -> reconcile_step -> reconcile_pass -> run_until (.await?) -> cmd_custodian and the process exits on the very fault it exists to repair. The at-Check test dodges this by hand-building a healthy_fleet that EXCLUDES the killed server (custodian_day_one.rs:1074-1079). Fix: exercise the real production path — fleet built from all --endpoints including the node that dies — and make the role survive the kill rather than exit. - Adv-2: the under-replicated gauge undercounts the worst losses. emit_under_ replicated(plans.len()) counts only Repairable chunks (survivors >= k); chunks with survivors < k (Unrepairable) and malformed placements emit 0. Concrete case: kill two fragments of an RS(2,1) chunk -> gauge reads 0 while the chunk is un- reconstructable. Fix the gauge so the binding "rises then returns to zero" signal covers un-repairable / lost-beyond-tolerance chunks, not just auto-repairable loss. - §6.3 coordination: `wyrd custodian` advertises "single-active" leadership but constructs a process-local MemCoordination that always grants leadership to the lone process (coordination-mem/src/lib.rs:184). Two deployed custodians do not fence each other and both run reconstruction. Implement cross-process leader election correctly so the deployable role is genuinely single-active. Related to fix while in there (adversary/codex weaker notes): - The role installs no global tracing subscriber, yet a comment claims it does (custodian.rs:606-608); non-metric events (audit lines, malformed-placement warn) are dropped in production. Either wire the subscriber or correct the comment. - DServerId assigned by --endpoints index (cli.rs:519-527) can mis-key placements and undermine failure-domain distinctness; use the D-server registration's stable id / failure_domain. Not raised for this iteration: §6.1 T5 record (milestone-slice-#1 confirmation, typed-errors x #255 sequencing) and §6.2 live-exporter fitness remain open judgment/ validation items to be revisited at the next sign-off.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
