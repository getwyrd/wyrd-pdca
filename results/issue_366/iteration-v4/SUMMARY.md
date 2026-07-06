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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

# Check review — issue 366 / obs-floor-observability

**Task under review:** deliver the keystone slice of the observability floor (proposal 0010 items 1–2) — extract the backend-agnostic telemetry seam into a shared `crates/telemetry`, make `wyrd custodian` a runnable/deployable role wired through that seam and the real metadata backend, and prove the day-one durability signal (kill a D-server → under-replicated count RISES then RETURNS TO ZERO, read back via `gather_prometheus`) through the running role — while the gauge now covers un-repairable/malformed losses, not just auto-repairable ones.

> Re-run scope note: `cargo` and `git` are gated in this review sandbox (every invocation requires interactive approval), so I could not mechanically re-run the workspace build/tests. I grounded every citation on the applied patch in the target worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1` (the patch IS applied there: `crates/telemetry/`, `crates/server/src/custodian.rs` present; `crates/custodian/src/telemetry.rs` deleted), read the deterministic gate results in `check-gates.json`, and reasoned statically. Where a verdict turns on a re-run I could not perform, I say so and hand the human the exact command.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding criterion is concrete and testable (brief L50–59): running role → injected loss → under-replicated gauge rises then returns to 0 via `gather_prometheus`. Patch targets exactly this; scope stays the keystone bundle (items 1–2 + gauge fix), consistent with the milestone decomposition. |
| C2 Reproduction (red pre-fix) | PASS | Harness `C4-verify` (advisory) records red-without-fix / green-with-fix (`check-gates.json` rows). The gauge-semantics regression is genuinely behavioral: `gauge_counts_a_loss_beyond_tolerance` reads 0 pre-fix (count = `plans.len()`) vs 1 post-fix. Caveat: I could not independently re-run (`cargo` gated); trusting the recorded red→green. |
| C3 Change | PASS | Change grounds on target: gauge tally `reconstruction.rs:144–192`, emit `reconstruction.rs:541`; backend routing `cli.rs:558` (`resolve_backend`) + `:571` (`connect_with_timeout`) + cfg-gated Tikv arm; role `server/src/custodian.rs`. Iteration-3 primary defect (hardcoded `open_local_meta_redb`) is fixed — custodian now routes the same seam as `cmd_put/get`, `MetadataBackend::Tikv` is cfg-gated so the match stays exhaustive both ways (`cli.rs:81–90`). |
| C4 Verification (red→green) | FAIL | **Gating `C4-ci` is RED** — `cargo test --workspace --exclude wyrd-dst` exit 101 (`check-gates.json`, gating:true) — so accept is blocked deterministically. This is NOT a stale-target artifact: target is readable and the patch is applied. I could not re-run to isolate it. Decision owed: is the red the known process-global `tracing` callsite-interest race across the 5 concurrent `#[tokio::test]`s in the new `custodian_day_one` binary (iteration-3 carried this forward; the code itself acknowledges it, `server/tests/custodian_day_one.rs:44–48` — the fix is a separate binary but the 5 in-binary tests still race each other unsserialized), or a genuine regression? Human/driver must re-run `cargo test -p wyrd-server --test custodian_day_one -- --test-threads=1` and the full `xtask ci` to isolate; if flaky, serialize the read-back tests before accept. |
| C5 Causal adequacy | NEEDS-HUMAN | Contested symptom-vs-root-cause. The role survives a killed D-server by a **runtime reachability guard** — `live_reconstruction_view` probes `store.health().await.is_ok()` and drops unreachable nodes (`server/src/custodian.rs:128`) — reading *around* the loss rather than removing the cause: `reconstruction::assess` classifies an unreachable-during-fetch as transient and propagates it, which is what would otherwise crash the process. The builder flags this as a "stand-in" for lease/registration-driven membership. Decision owed: is probe-and-drop (with a TOCTOU window a peer can die inside, caught only by the run-loop's Store→continue at `:252`) acceptable for the #367 gate, or must the classification seam treat unreachable-during-reconstruction as missing? Must be recorded, not resolved silently. |
| T1 Structure | PASS | Seam extracted to `crates/telemetry` (per iteration-1 sign-off: extract); `server` owns the concrete role (ADR-0010); custodian re-exports for M3 callers (`custodian/src/lib.rs`). Minor smell: the shared crate still hardcodes `SCOPE = "wyrd.custodian"` (`telemetry/src/lib.rs`) though it is meant to serve request/capacity planes too — non-blocking. |
| T2 Shape | PASS | Largely additive (new crate, new subcommand, new role file). One non-additive edit: the under-replicated metric changes type `monotonic_counter → gauge` (`reconstruction.rs:541`), which is deliberate and correct (a level must return to zero through an accumulating registry) and its ripple is contained — the DST expectation is updated in lockstep (`dst/tests/custodian.rs:1023,1046`). Invariants (no protocol/on-disk/consistency change) hold. |
| T3 Runtime | NEEDS-HUMAN | Per-fix test passes, but the deployable-binary runtime path is unexercised at Check: tests drive the library seams over in-memory stores; the real `GrpcChunkStore::connect_with_timeout` dial, the `cmd_custodian` end-to-end, and the actual OTLP/Prometheus scrape against a cluster are never run (they are the off-Check #367 gate). Compounded by the unreproduced gating `C4-ci` red. Decision owed: accept in-memory-seam coverage as sufficient at Check, deferring real-cluster runtime to #367. |
| T4 Contribution | PASS | Advances the milestone: delivers the keystone (runnable custodian + shared telemetry seam), corrects both prior-iteration substantive defects (backend routing; gauge covering worst-case losses), and adds survive-the-kill + fenced-stop behavior with tests. Meaningful, non-cosmetic. |
| T5 Judgment | NEEDS-HUMAN | Multiple recorded-decision items owed (brief §"Known NEEDS-HUMAN"): (a) milestone slice mapping — confirm this bundle carries items 1–2; (b) telemetry extraction vs keep-in-custodian — patch chose extract, confirm; (c) cross-host leader election deferred — `cmd_custodian` builds a process-local `MemCoordination` (`cli.rs:541`) so two deployed custodians do NOT fence each other; patch argues CAS commits prevent corruption and defers etcd `Coordination` as out-of-scope — human must accept or require it; (d) typed-errors × #255 (item 6) not attempted — sequencing decision still unrecorded. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Off-Check by design (brief L67–70): the live Prometheus-scrape / OTLP collector day-one run on a Tier-2 node against the blueprint checklist is supplementary evidence and ultimately the #367 first-deployment gate. Decision owed at sign-off: does the in-process `gather_prometheus` read-back through the running role adequately stand in for the real scrape until #367, and does the keystone slice satisfy the floor's operational intent? Concrete steps for the human: build `wyrd --features tikv`, run `wyrd custodian --endpoints <fleet> --otlp-endpoint <collector> --metadata-backend tikv`, kill a D-server, and confirm the under-replicated series rises then returns to zero on the scraped surface. |

### Advisory — adversary

# Adversarial review — issue_366 / obs-floor-observability (iteration 4)

Skeptic's pass. Attacked the red→green evidence, the fix's edges, and the reviewer's
verdict. Grounded on the target worktree at `$PDCA_TARGET` (patch applied). Findings below.

## Attack on the evidence

- **NEEDS-HUMAN — the gating C4-ci failure is a REAL, reproducible flake in the exact
  read-back seam the keystone's binding evidence rides on — not pre-existing noise to
  wave away.** `check-gates.json` reports C4-ci `fail` (exit 101) while C4-verify reports
  `pass`. I reproduced it: `cargo test --workspace --exclude wyrd-dst` passed twice then
  **failed exit 101 on the third run** at `crates/custodian/tests/rebalance.rs:884`
  (`emits_per_failure_domain_utilization_on_the_durability_seam`). The panic shows
  `gather_prometheus()` returned only `target_info` — the metric event was silently dropped
  because `tracing` caches per-callsite *interest* process-globally, and under the gate's
  parallel binary/test scheduling a sibling test evaluated the callsite first with no
  `MetricsLayer` active (`Interest::never`), so nobody records it. This is the **same
  `DurabilityTelemetry` → `tracing`→OTel → `gather_prometheus` read-back mechanism** the
  patch's own binding day-one signal asserts through (`crates/server/tests/custodian_day_one.rs:1372`
  `under_replicated()` → `gauge_value("reconstruction_under_replicated")`). The patch does
  **not** fix this seam — it changes the under-replicated callsite from `monotonic_counter.`
  to `gauge.` and adds *more* read-back tests on the same fragile mechanism. So the
  C4-verify "green" is non-deterministic: it passes in isolation (I ran `custodian_day_one`
  5× and `-p wyrd-custodian` 6× green) but the *gate's* full-workspace parallel run is
  precisely where it breaks. The deterministic gate is correctly RED; treating it as
  unrelated flake is the unwarranted rationalization to guard against.

- **The binding day-one signal is asserted through a parallel re-implementation, never the
  production entry `cmd_custodian`.** `crates/server/src/cli.rs:518` (`fn cmd_custodian`) is
  the real `wyrd custodian` binary path — arg parse, `resolve_backend`,
  `open_local_meta_redb`/`open_tikv_meta`, `GrpcChunkStore::connect_with_timeout`, and the
  `ConfiguredDServer` fleet build. **No test drives it** (confirmed: no `cli::run` /
  `cmd_custodian` / `"custodian"` reference anywhere under `crates/**/tests`). Tests 1–2 in
  `custodian_day_one.rs` call `service.reconcile_pass(...)` directly over a hand-built
  `MemMeta` + hand-built `configured([...])`; test 3 calls `run_reconstruction_until` but
  still with `MemMeta` and hand-built fleet. The header comment claims this is "the same
  production wiring the `wyrd custodian` binary runs (`cli::cmd_custodian` → …)"
  (`custodian_day_one.rs:1017-1020`) — but the binary's own backend routing, connect path,
  and fleet construction execute in **no** test. The iteration-3 primary rejection (custodian
  opening the wrong metadata backend) could regress verbatim and every gate here would stay
  green.

## Attack on the fix — a concrete input that breaks it

- **`crates/server/src/cli.rs:585,589` — the default branch fabricates topology, the exact
  defect iteration-3 rejected, and the brief's canonical bring-up command triggers it.**
  `--ids`/`--failure-domains` are optional (default empty: `cli.rs:531-532`). When absent,
  `id: ids.get(i).copied().unwrap_or(i as u64)` keys D-servers **positionally by endpoint
  index**, and `failure_domain: … unwrap_or_else(|| endpoints[i].clone())` uses the **endpoint
  URL** as the domain. The day-one command the brief cites — `wyrd custodian --otlp-endpoint …`
  (brief L30-31) — carries no `--failure-domains`, so it lands in this branch. Concrete break:
  two D-servers physically in the same rack/failure-domain but reached at different URLs get
  **distinct** fabricated domains → `Topology` believes them independent → a rebuilt fragment
  can be re-placed into the same real failure domain as a survivor, defeating the durability
  invariant. The `ConfiguredDServer` docstring asserts these are "NOT fabricated positionally
  from the `--endpoints` order" (`crates/server/src/custodian.rs:810-813`) — the default path
  does exactly that. NEEDS-HUMAN: is "operator must hand-align `--ids`/`--failure-domains`"
  acceptable when the documented day-one command omits them and the default silently invents
  topology?

- **NEEDS-HUMAN — `crates/server/src/cli.rs:541` per-process `MemCoordination::new()`
  "single-active" is unsound for the *production* backend the keystone gates.** The startup
  log advertises "leader for zone … host-local single-active" (`cli.rs:546-548`) and the
  docstring justifies it with "the redb store's exclusive file lock keeps a second custodian
  off the same `--data-dir`" (`cli.rs:496-498`). But the brief pins production to **TiKV**
  (redb is dev/eval only), and TiKV is a shared networked store with **no `--data-dir` file
  lock**. Two `wyrd custodian` processes on one host pointed at the same TiKV backend each
  build their own process-local `MemCoordination`, which always grants leadership
  (`coordination-mem` lone-process grant), so **both run reconstruction concurrently** — the
  file-lock safety argument does not hold on the very deployment (#367) this floor gates. The
  CAS-commit argument bounds corruption but not the false "single-active" advertisement or
  the duplicated work. iteration-3 re-scoped cross-process fencing out; the human should
  confirm the deferral knowing the host-local guarantee is redb-only, not TiKV.

## Attack on the verdict

- **`crates/custodian/src/reconstruction.rs:135-143,192` — the gauge measures "queued
  obligations that are degraded," not "every under-replicated chunk," despite the comment.**
  The tally iterates only `queue = repair::queued_repairs(ctx.meta)` (`:128`). A chunk that
  has genuinely lost redundancy but whose repair obligation was never enqueued (no scrub /
  read-repair has flagged it yet) contributes **0**. The docstring claim "EVERY chunk this
  pass found below its scheme's fragment count" (`:135-137`, `:210-213`) overstates: the
  day-one rise depends on something first calling `enqueue_repair` (the tests do so by hand
  at `custodian_day_one.rs:1416`). Any reviewer statement that the gauge is a true durability
  *level* is unwarranted — it is a level over the *repair queue*, not the chunk population.

## Attempted but could not refute

- The gauge-vs-counter rationale for "returns to zero" (`reconstruction.rs:519-540`) holds:
  I confirmed in isolation the gauge reads 1 then 0; a monotonic counter would pin at 1.
- Test 2's red→green is mechanically real: pre-fix `plans.len()` is 0 for an `Unrepairable`
  chunk and the counter exports as `..._total`, so `gauge_value("reconstruction_under_replicated")`
  returns `None ≠ Some(1.0)` (red); post-fix the gauge matches (green).
- Test 3 (`run_loop_survives_a_dead_dserver_and_keeps_running`) genuinely drives the real
  `run_reconstruction_until` spine over a fleet that includes the dead node — the survive-the-
  kill loop is exercised (though only over `MemMeta`, not through `cmd_custodian`).

### Advisory — codex

- `crates/server/src/cli.rs:585` — `wyrd custodian` still silently fabricates D-server identity/topology when `--ids` or `--failure-domains` are absent or too short (`i as u64`, then endpoint string as the domain). That reintroduces the iteration-3 risk: the custodian can repair against IDs/domains that do not match the D-servers' registered stable IDs / real failure domains, so placement can be mis-keyed and rebuilt fragments can be selected into the same real failure domain as a survivor. The role should reject missing/mismatched topology for static endpoints, or derive it from the real registration seam when available, rather than falling back silently.
- `crates/server/src/custodian.rs:128` — `live_reconstruction_view` keeps any server whose `health()` returns `Ok(_)`, including `Health::Unhealthy`. That makes a not-ready store eligible for survivor reads and repair placement, which conflicts with the floor's readiness semantics ("unhealthy store reads not-ready") and can turn a known-unhealthy peer into repeated `Store` failures or bad placement choices. The live reconstruction fleet should at least exclude `Unhealthy` responses, not only transport errors.
- NEEDS-HUMAN — `crates/server/tests/custodian_day_one.rs:394` — the day-one regression still demonstrates the signal through in-memory `MemMeta` and direct `CustodianService` calls, not by driving `wyrd custodian`/`cmd_custodian` against the selected metadata backend. The production code now has a backend arm, but the test would not catch a regression where the binary reopens the wrong backend or misparses the CLI flags; decide whether this seam-level test is acceptable or require the carry-forward's requested backend-driven process test.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Contested symptom-vs-root-cause. The role survives a killed D-server by a **runtime reachability guard** — `live_reconstruction_view` probes `store.health().await.is_ok()` and drops unreachable nodes (`server/src/custodian.rs:128`) — reading *around* the loss rather than removing the cause: `reconstruction::assess` classifies an unreachable-during-fetch as transient and propagates it, which is what would otherwise crash the process. The builder flags this as a "stand-in" for lease/registration-driven membership. Decision owed: is probe-and-drop (with a TOCTOU window a peer can die inside, caught only by the run-loop's Store→continue at `:252`) acceptable for the #367 gate, or must the classification seam treat unreachable-during-reconstruction as missing? Must be recorded, not resolved silently.
- [ ] T3 Runtime — Per-fix test passes, but the deployable-binary runtime path is unexercised at Check: tests drive the library seams over in-memory stores; the real `GrpcChunkStore::connect_with_timeout` dial, the `cmd_custodian` end-to-end, and the actual OTLP/Prometheus scrape against a cluster are never run (they are the off-Check #367 gate). Compounded by the unreproduced gating `C4-ci` red. Decision owed: accept in-memory-seam coverage as sufficient at Check, deferring real-cluster runtime to #367.
- [ ] T5 Judgment — Multiple recorded-decision items owed (brief §"Known NEEDS-HUMAN"): (a) milestone slice mapping — confirm this bundle carries items 1–2; (b) telemetry extraction vs keep-in-custodian — patch chose extract, confirm; (c) cross-host leader election deferred — `cmd_custodian` builds a process-local `MemCoordination` (`cli.rs:541`) so two deployed custodians do NOT fence each other; patch argues CAS commits prevent corruption and defers etcd `Coordination` as out-of-scope — human must accept or require it; (d) typed-errors × #255 (item 6) not attempted — sequencing decision still unrecorded.
- [ ] Validation — fitness-to-purpose — Off-Check by design (brief L67–70): the live Prometheus-scrape / OTLP collector day-one run on a Tier-2 node against the blueprint checklist is supplementary evidence and ultimately the #367 first-deployment gate. Decision owed at sign-off: does the in-process `gather_prometheus` read-back through the running role adequately stand in for the real scrape until #367, and does the keystone slice satisfy the floor's operational intent? Concrete steps for the human: build `wyrd --features tikv`, run `wyrd custodian --endpoints <fleet> --otlp-endpoint <collector> --metadata-backend tikv`, kill a D-server, and confirm the under-replicated series rises then returns to zero on the scraped surface.
- [ ] `crates/server/tests/custodian_day_one.rs:394` — the day-one regression still demonstrates the signal through in-memory `MemMeta` and direct `CustodianService` calls, not by driving `wyrd custodian`/`cmd_custodian` against the selected metadata backend. The production code now has a backend arm, but the test would not catch a regression where the binary reopens the wrong backend or misparses the CLI flags; decide whether this seam-level test is acceptable or require the carry-forward's requested backend-driven process test.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Gating C4-ci is RED (exit 101) and, unlike a wave-off flake, the red is in the exact seam the keystone's binding evidence rides on. The adversary reproduced it (pass, pass, FAIL on the 3rd run) at `crates/custodian/tests/rebalance.rs:884`: `tracing` caches per-callsite interest process-globally, so under the parallel gate a sibling test evaluates the metric callsite first with no MetricsLayer active (Interest::never) and `gather_prometheus()` returns only target_info. This is the SAME DurabilityTelemetry->tracing->OTel->gather_prometheus read-back mechanism the day-one signal asserts through (custodian_day_one.rs:1372), and the patch does not fix it — it adds MORE read-back tests on the fragile seam. The harness-level flake is captured as an Act candidate (§10); the deterministic gate is correctly RED. Substantive rebuild items (not just a re-run): - Iteration-3 primary defect REGRESSED: `crates/server/src/cli.rs:585,589` still fabricates D-server identity/topology when `--ids`/`--failure-domains` are absent (keys positionally by endpoint index; uses the endpoint URL as the failure domain) — and the brief's own canonical day-one command omits those flags, so it lands in this branch. Two D-servers in one physical failure domain reached at different URLs get distinct fabricated domains -> a rebuilt fragment can be re-placed into the same real domain as a survivor, defeating durability. Reject missing/mismatched topology for static endpoints or derive it from the real registration seam; the docstring even claims it is NOT fabricated positionally, but the default path does exactly that. - The binding day-one signal never drives the real binary: `cmd_custodian` (cli.rs:518 — arg parse, resolve_backend, backend open, connect_with_timeout, fleet build) is exercised by NO test; tests 1-3 hand-build MemMeta + fleet and call reconcile_pass/run_reconstruction_until directly. The iteration-3 rejection (custodian opening the wrong metadata backend) could regress verbatim with every gate green. Add a backend-driven process test through cmd_custodian. - Fix the read-back race so the binding evidence is deterministic (register the metric callsite / serialize the read-back tests) rather than adding more tests on the flaky seam. Human calls to settle at next Check (record, do not resolve silently): - MemCoordination "single-active" (cli.rs:541) is redb-file-lock-only and UNSOUND on the production TiKV backend (no --data-dir lock): two custodian processes on one host both self-grant leadership and run reconstruction concurrently while logging "single-active". Confirm the deferral or require real Coordination. - C5 probe-and-drop membership (custodian.rs:128) reads around the loss with a TOCTOU window and keeps Health::Unhealthy peers eligible for survivor reads/placement (codex) — decide acceptable-for-#367 or require the classification seam to treat unreachable-during-reconstruction as missing. - Gauge is a level over the repair QUEUE, not the chunk population (reconstruction.rs:135-143) — the "EVERY under-replicated chunk" docstring overstates; a lost-but-not-yet-enqueued chunk contributes 0.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_366: latent workspace-wide gate flake — a process-global `tracing` per-callsite interest cache silently drops metric events under the full-workspace parallel gate (`cargo test --workspace --exclude wyrd-dst`), so any of the ~8 `gather_prometheus` read-back tests can spuriously fail exit 101 (reproduced ~1-in-3 at `crates/custodian/tests/rebalance.rs:884`; a sibling test evaluates the callsite first with no MetricsLayer → Interest::never). Independent of the patch under test; likely also the true cause of issue_364's unlocalized C4-ci red. Fix belongs in the test harness (pre-register metric callsites / install a process-global MetricsLayer, or serialize the read-back tests), not per-bundle.
