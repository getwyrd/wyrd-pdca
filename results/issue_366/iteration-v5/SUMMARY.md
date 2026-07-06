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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — error: pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git
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

# Check review — issue 366 / obs-floor-observability (iteration 5)

**Task under review:** deliver the keystone slice of the operational-visibility floor (proposal 0010 items 1–2): extract the durability telemetry seam into a shared `crates/telemetry` crate, wire `wyrd custodian` as a runnable/deployable process role, and demonstrate the day-one durability signal — after a killed D-server the under-replicated count RISES then RETURNS TO ZERO, observed via `gather_prometheus` through the running role (architecture §7.4 step 4).

**Grounding note:** `$PDCA_TARGET` did not resolve to a readable checkout (the only sibling on the host, `/tmp/verify364`, is a *different* issue's worktree and out of bounds). Per protocol every citation below grounds on `patch.diff` alone; `path:line` refers to post-patch content in the diff.

**Gate state:** gating `C4-ci` = **pass** ("xtask ci: all checks passed"), overall pass. Non-gating `C4-verify` = fail with `pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git` — a recurring **new-crate rename artifact** in run-verify's stash/checkout step (the file is a rename-new from `custodian/src/telemetry.rs`, absent under that path in the base the script diffs against), identical to iteration-3's string. Not a patch defect; not treated as a blocking C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pins the keystone (0010 items 1–2) and its binding oracle (`gather_prometheus` rise→zero) precisely; the milestone *decomposition* — which slice this bundle carries — is a pre-declared human call (brief §"Known NEEDS-HUMAN"), surfaced in T5/§6, not a spec gap. |
| C2 Reproduction (red pre-fix) | PASS | Red is derivable from the diff: pre-fix `emit_under_replicated(plans.len())` excludes `Unrepairable`, so `gauge_counts_a_loss_beyond_tolerance` (custodian_day_one.rs) reads 0; pre-fix `monotonic_counter.reconstruction_under_replicated` cannot return to 0 through an accumulating registry, so the DST rename assertion (custodian.rs:1023,1046) fails. Mechanical per-fix red→green did **not** run (rename artifact below) — the human owes confidence that the encoded reds are real, not just asserted. |
| C3 Change | PASS | Purely additive: new `crates/telemetry` (extraction), new `server/src/custodian.rs` role + `cmd_custodian`, `plans.len()`→full-degraded-set tally (reconstruction.rs:160,185), `monotonic_counter`→`gauge` (reconstruction.rs:233). No commit-protocol / on-disk-format / consistency-contract touch — invariants held. |
| C4 Verification (red→green) | PASS | Gating `xtask ci` passes (all checks). The advisory `C4-verify` FAIL is the recurring rename artifact, not a defect — but it means red→green was never mechanically established by the harness; the residual decision owed is whether to accept code-derived pre/post assertions or require the run-verify script fixed to handle a new-crate rename (verification-hygiene, not correctness). |
| C5 Causal adequacy | NEEDS-HUMAN | Three unresolved root-cause calls. (a) **Symptom-guard**: `live_reconstruction_view` (custodian.rs) probes `health()` and drops unreachable peers to read *around* them, papering over the real cause — `reconstruction::assess` classifies an unreachable fetch as *transient* and unwinds the pass; decide guard-vs-fix-the-classification (treat unreachable-during-reconstruction as missing), and the TOCTOU window the probe leaves. (b) **MemCoordination "single-active"** (cli.rs) is host-local (redb file lock) only; on the TiKV backend two processes self-grant leadership — decide the deferral to the out-of-scope etcd `Coordination` is acceptable for #367. (c) The gauge is a level over the repair **queue**, not the chunk population — a lost-but-not-yet-enqueued chunk still contributes 0; confirm that scope. |
| T1 Structure | PASS | New crate registered in workspace members + `[workspace.dependencies]` (Cargo.toml:71,81); `pub mod custodian` (server/lib.rs); telemetry re-exported from custodian (lib.rs) so M3 `wyrd_custodian::DurabilityTelemetry` consumers keep compiling. Structurally clean; matches iteration-1's extract decision. |
| T2 Shape | PASS | API additions (`CustodianService`, `ConfiguredDServer`, `run_reconstruction_over_backend`, `require_aligned_topology`) are coherent; export seam stays behind `ExporterConfig` with no backend hardcoded (telemetry/lib.rs) — the 0010 invariant. |
| T3 Runtime | PASS | `connect_with_timeout` (cli.rs) closes the no-timeout hang; run loop logs-and-continues on `ReconcileError::Store`, stops on `Fenced` (custodian.rs) — the survive-the-kill behaviour is exercised by tests 1/3/3b/4. Topology-required ergonomics tension noted under T5. |
| T4 Contribution | PASS | Substantial tests: 5 day-one properties + a real-redb backend-open path (custodian_day_one.rs), `require_aligned_topology` unit test (cli.rs), callsite-race fix applied across 4 custodian test files. Caveat: the new test drives `run_reconstruction_over_backend`, one layer *below* `cmd_custodian` — the subcommand's own arg-parse / `resolve_backend` / `connect_with_timeout` / fleet-build wrapper (cli.rs) remains uncovered, so the iteration-3 "wrong backend plane" class could still regress in that wrapper. |
| T5 Judgment | NEEDS-HUMAN | `require_aligned_topology` now **rejects** missing `--ids`/`--failure-domains`, but the brief's own canonical `wyrd custodian --otlp-endpoint …` bring-up command omits them — with `--endpoints` present it would now error, deferring real topology to the out-of-scope etcd discovery seam. Decide: reject-and-require vs derive-from-registration, plus the two carried-forward recorded calls (shared-`crates/telemetry` extraction — done, confirm; typed-errors × #255 sequencing — deferred; milestone slice-#1 confirmation). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The binding signal is demonstrated **in-process** via `gather_prometheus` read-back; the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 host is off-Check supplementary evidence (brief §DEFERRED) and ultimately the #367 first-deployment gate. Human owes: does the in-process read-back suffice for sign-off, or is a live-exporter run required before this floor is called fit? (Ran what I could: the in-process rise→zero assertions and backend-open path are encoded in custodian_day_one.rs; a live scrape needs a Tier-2 node this environment cannot provide.) |

### Advisory — adversary

# Adversarial review — issue 366 / obs-floor keystone (iteration 5)

Scope: this diff only. Patch confirmed applied at `$PDCA_TARGET`
(`feat/obs-floor.1-keystone`); `cargo test -p wyrd-server --test custodian_day_one`
→ 6 passed. Attacks below ground on the target source, not the diff.

## Findings

- **NEEDS-HUMAN — `Malformed` chunks are counted on `reconstruction_under_replicated`,
  which both raises false durability alarms and defeats the binding "returns to ZERO"
  shape.** `crates/custodian/src/reconstruction.rs:164-167` adds `under_replicated += 1`
  for `Assessment::Malformed`. But `assess` returns `Malformed` from
  `reconstruction.rs:263` *before any fragment is fetched* — purely because
  `checked_fragments()` found a wrong-length placement vector. Such a chunk may have
  **all its fragments physically present** (full redundancy), yet it is reported on a
  gauge literally named *under-replicated*. Worse, a malformed placement is never
  auto-repaired (the code explicitly refuses to "rebuild over a fabricated identity
  vector"), so the obligation stays queued and is re-counted on **every** pass. Concrete
  failing case: a store containing one pre-existing malformed chunk plus a healthy fleet.
  An operator runs the §7.4 day-one step-4 drill (kill a D-server, watch the gauge "rise
  then return to zero"). The gauge floors at ≥1 forever, so it returns to **1, never 0** —
  the brief's BINDING success criterion ("rises and then returns to ZERO",
  `brief.md:52-59`) is unobservable. This is the direct side effect of the iteration-2
  Adv-2 request; whether "cover the worst losses" should override "returns to zero" is a
  human call, not silently resolvable in the counting line.

- **NEEDS-HUMAN — the process advertises a single-active safety property that is false on
  the production (TiKV) backend.** `crates/server/src/cli.rs:549` prints "host-local
  single-active via the store lock", and the docstring `cli.rs:497-503` justifies it by
  "the redb store's exclusive file lock keeps a second custodian off the same
  `--data-dir`". But the production path is `--metadata-backend tikv` →
  `open_tikv_meta()` (`cli.rs` tikv arm) — **no `--data-dir`, no local file lock**. Each
  process constructs its own process-local `MemCoordination` (`cli.rs:698` in diff /
  target `cmd_custodian`) which always self-grants leadership. Concrete case: two
  `wyrd custodian --metadata-backend tikv` on one host both elect, both log
  "single-active", and both run reconstruction concurrently against the same TiKV store.
  The CAS commit prevents corruption but not the wasted double-repair, and the printed
  claim is simply untrue for the deployment (#367) this gates. Carried from iteration-4
  §coordination and still unaddressed for the tikv arm.

- **The binding evidence never drives the real binary entry `cmd_custodian` — only the
  factored-out `run_reconstruction_over_backend`.** `crates/server/src/cli.rs:504`
  (`cmd_custodian`) is referenced by tests only in comments (`custodian_day_one.rs:11,438`);
  its body — arg parse, `resolve_backend`, `GrpcChunkStore::connect_with_timeout`
  (`cli.rs:577`), and the `id: ids[i]`/`failure_domain: domains[i]` fleet build
  (`cli.rs:587-593`) — is exercised by **no** test. This is the exact surface iteration-3
  (wrong backend) and iteration-4 (fabricated topology) were rejected on; a regression
  there could re-enter with every gate green. `require_aligned_topology` is unit-tested,
  but the `connect_with_timeout` dial and the client→`ConfiguredDServer` assembly are not.
  iteration-4 asked specifically for "a backend-driven process test **through
  cmd_custodian**"; the builder added one through the helper below it, leaving the
  binary's own glue uncovered.

- **NEEDS-HUMAN — `live_reconstruction_view` keeps `Health::Unhealthy` peers as survivor
  read / re-placement targets.** `crates/server/src/custodian.rs:128` drops a server only
  when `health().await` is `Err`; a reachable server returning `Health::Unhealthy`
  (e.g. a failing/degraded disk) stays in the live fleet and topology
  (`custodian.rs:129-130`). Concrete case: a rebuilt fragment is placed onto an
  Unhealthy-but-reachable node, or that node is trusted as a survivor read, so the repair
  "restores" redundancy onto a dying device. The docstring (`custodian.rs:112-118`)
  acknowledges this as deliberate; carried from iteration-4 §C5 — a human must decide it
  is acceptable-for-#367 or require unreachable-during-reconstruction to be treated as
  missing.

- **The gauge is a level over the repair *queue*, not the chunk population — the docstring
  overstates it.** The tally iterates only `queue = repair::queued_repairs(...)`
  (`reconstruction.rs:128,145`), yet `emit_under_replicated`'s docstring
  (`reconstruction.rs:540`, "EVERY chunk this pass found below its scheme's fragment
  count") and the inline comment (`reconstruction.rs:135-143`) claim total coverage.
  Concrete case: a chunk that has lost redundancy but has **not yet been enqueued** by a
  scrub/read-path finding contributes 0 — the "silent non-zero durability failure" the
  metric is sold as catching is still silent until something enqueues it. Flagged in
  iteration-4 and unaddressed; the docstring language is stronger than the code.

- **Evidence gap — the per-fix red→green was never mechanically demonstrated.**
  `check-gates.json` C4-verify = **fail** (`pathspec 'crates/telemetry/src/lib.rs' did
  not match any file(s) known to git`): the crate-extraction rename breaks the harness's
  checkout-of-pre-state, so red→green was reasoned, not run. It *is* logically sound for
  the gauge counting change (pre-fix `emit_under_replicated(plans.len())` excludes
  `Unrepairable`, so `gauge_counts_a_loss_beyond_tolerance` would read 0 → red), but the
  survival/run-loop tests depend on brand-new types (`CustodianService`,
  `live_reconstruction_view`) that do not exist pre-fix, so their "red" is a compile
  error, not a defect exhibition. A confirmatory reviewer accepting "red→green
  established" for those tests is on thin ice.

## Attempted but could not refute

- **The callsite-interest race (iteration-4's RED gate).** Ran `cargo test -p
  wyrd-custodian --tests` 5× — all green. Audited every test file: every reconcile-driving
  (metric-firing) test calls `enable_metric_callsites()` first; the only unguarded tests
  (`skeleton.rs:91` selector, `reconstruction.rs` `repair_priority_rises_as_redundancy_falls`)
  are pure functions that never hit a metric callsite. Could not reproduce the flake. The
  guard is per-test opt-in and thus fragile against a future metric-firing test that
  forgets the call, but no such gap exists in this diff.

- **The clean-store rise-then-zero signal.** `gauge_rises_then_returns_to_zero_*` drives
  the real `reconcile_step` seam (via `CustodianService::reconcile_pass`), not a parallel
  re-implementation, and the redb-backend variant drives the real `open_local_meta_redb`
  path. On a single-chunk store the 1→0 shape holds. Could not break it (the Malformed/
  Unrepairable floor above only bites on a *populated* store, which is why it is filed as
  a NEEDS-HUMAN tension rather than a refutation of the at-Check test).

### Advisory — codex

- `crates/server/src/cli.rs:577` — `cmd_custodian` eagerly `connect_with_timeout`s every configured endpoint and returns an error on the first unreachable D-server, before `live_reconstruction_view` can drop dead peers. That means a custodian started or restarted during the day-one killed-D-server incident exits instead of repairing around the failed node; the new tests bypass this path with in-memory `ConfiguredDServer`s.
- NEEDS-HUMAN — `crates/server/src/cli.rs:544` — the deployable role still uses a fresh process-local `MemCoordination`, so two `wyrd custodian --metadata-backend tikv` processes have no shared fencing/leadership despite the role being described as single-active. The comments scope this as host-local / deferred, but production TiKV has no redb file lock, so sign-off should explicitly accept or reject that deferral.
- NEEDS-HUMAN — `crates/server/src/custodian.rs:128` — `live_reconstruction_view` keeps any peer whose `health()` returns `Ok(_)`, including `Health::Unhealthy`, eligible for survivor reads and replacement topology. If the readiness contract means unhealthy stores are not serving, this can still route repair work through not-ready peers; accept the probe stand-in for #367 or require health-state filtering.
- NEEDS-HUMAN — `crates/server/src/cli.rs:170` — the usage presents `--ids` / `--failure-domains` as optional, but `require_aligned_topology` rejects any `--endpoints` run without them (`crates/server/src/cli.rs:683`). That is safer than fabricated topology, but it changes the previously cited day-one command shape, so the runbook/operator contract needs an explicit sign-off.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec
- [ ] C5 Causal adequacy — Three unresolved root-cause calls. (a) **Symptom-guard**: `live_reconstruction_view` (custodian.rs) probes `health()` and drops unreachable peers to read *around* them, papering over the real cause — `reconstruction::assess` classifies an unreachable fetch as *transient* and unwinds the pass; decide guard-vs-fix-the-classification (treat unreachable-during-reconstruction as missing), and the TOCTOU window the probe leaves. (b) **MemCoordination "single-active"** (cli.rs) is host-local (redb file lock) only; on the TiKV backend two processes self-grant leadership — decide the deferral to the out-of-scope etcd `Coordination` is acceptable for #367. (c) The gauge is a level over the repair **queue**, not the chunk population — a lost-but-not-yet-enqueued chunk still contributes 0; confirm that scope.
- [ ] T5 Judgment — `require_aligned_topology` now **rejects** missing `--ids`/`--failure-domains`, but the brief's own canonical `wyrd custodian --otlp-endpoint …` bring-up command omits them — with `--endpoints` present it would now error, deferring real topology to the out-of-scope etcd discovery seam. Decide: reject-and-require vs derive-from-registration, plus the two carried-forward recorded calls (shared-`crates/telemetry` extraction — done, confirm; typed-errors × #255 sequencing — deferred; milestone slice-#1 confirmation).
- [ ] Validation — fitness-to-purpose — The binding signal is demonstrated **in-process** via `gather_prometheus` read-back; the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 host is off-Check supplementary evidence (brief §DEFERRED) and ultimately the #367 first-deployment gate. Human owes: does the in-process read-back suffice for sign-off, or is a live-exporter run required before this floor is called fit? (Ran what I could: the in-process rise→zero assertions and backend-open path are encoded in custodian_day_one.rs; a live scrape needs a Tier-2 node this environment cannot provide.)
- [ ] `crates/server/src/cli.rs:544` — the deployable role still uses a fresh process-local `MemCoordination`, so two `wyrd custodian --metadata-backend tikv` processes have no shared fencing/leadership despite the role being described as single-active. The comments scope this as host-local / deferred, but production TiKV has no redb file lock, so sign-off should explicitly accept or reject that deferral.
- [ ] `crates/server/src/custodian.rs:128` — `live_reconstruction_view` keeps any peer whose `health()` returns `Ok(_)`, including `Health::Unhealthy`, eligible for survivor reads and replacement topology. If the readiness contract means unhealthy stores are not serving, this can still route repair work through not-ready peers; accept the probe stand-in for #367 or require health-state filtering.
- [ ] `crates/server/src/cli.rs:170` — the usage presents `--ids` / `--failure-domains` as optional, but `require_aligned_topology` rejects any `--endpoints` run without them (`crates/server/src/cli.rs:683`). That is safer than fabricated topology, but it changes the previously cited day-one command shape, so the runbook/operator contract needs an explicit sign-off.

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
- Iteration delta (if iterating): Rejected (iter-5): genuine progress (clean telemetry extraction, callsite-race not reproducible, clean-store rise→zero holds, backend-open now tested through a real on-disk redb), but the BINDING success criterion is undercut on a populated store and the exact iter-3/iter-4 reject surface is still uncovered — with a real day-one-breaking bug living in the gap. Required fixes: (1) BLOCKING — the binding "returns to ZERO" is unobservable on a populated store. `crates/custodian/src/reconstruction.rs:164-167` adds `under_replicated += 1` for `Assessment::Malformed`, but `assess` returns `Malformed` (reconstruction.rs:263) BEFORE any fragment is fetched — a chunk with all fragments physically present is counted on a gauge named *under-replicated*, and malformed placements are never auto-repaired, so the obligation is re-counted every pass. A store with one pre-existing malformed chunk floors the day-one drill gauge at ≥1 forever → returns to 1, never 0 (brief.md:52-59 BINDING criterion). Decide/fix: don't count non-fetchable-classification chunks (Malformed) on the under-replicated durability gauge, or split them onto a distinct metric so the rise→zero shape is observable on a populated store. (2) BLOCKING — the binding evidence still never drives the real binary entry `cmd_custodian`; it drives the factored-out `run_reconstruction_over_backend` one layer below. Uncovered glue between them: the `GrpcChunkStore::connect_with_timeout` dial loop (cli.rs:730-735) and the `id: ids[i]`/`failure_domain: domains[i]` fleet assembly (cli.rs:741-749) — the exact surfaces iter-3 (wrong backend) and iter-4 (fabricated topology) were rejected on. iteration-4 asked specifically for a backend-driven process test THROUGH cmd_custodian; the builder tested the helper below it and labelled it "the exact production path" (build-notes §3, cli.rs docstring 774-781) — narrowly false. Build the injectable seam so this is coverable headlessly: (a) abstract the concrete connect call behind an injected `DServerConnector` trait/closure (production wires gRPC by default; tests pass a fake that returns an in-memory ChunkStore and can return Err for one endpoint); (b) introduce an OWNED fleet type so a fake store can be injected — `ConfiguredDServer<'_>` currently borrows (`store: &dyn ChunkStore`), forcing the whole assembly to live inside cmd_custodian; (c) extract the dial+assemble into one testable `connect_fleet(...)` function (require_aligned_topology + connect loop + id/domain mapping in one place); (d) reuse the existing in-memory D-server fake the day-one tests already build, injected THROUGH the connector rather than below it. (3) BLOCKING (lives in the gap #2 leaves) — cmd_custodian exits during the very incident it must survive. The dial loop `connect_with_timeout(...).await ... ?` (cli.rs:731) returns Err on the FIRST unreachable endpoint at startup, so a custodian started/restarted during the day-one killed-D-server incident exits instead of repairing around the down node — contradicting the §7.4 day-one step-4 drill and the code's own comment (cli.rs:736-740). No test catches it because tests inject already-connected in-memory fleets, bypassing the loop. Make the connect-failure policy an explicit, tested decision (start-degraded / repair around a startup-down peer, per the drill), and add a test through the new connector seam where one endpoint errors, asserting the custodian proceeds on the reachable subset rather than exiting. (4) REQUIRED — single-active is advertised but FALSE on production TiKV. cli.rs:549/703-708 print "host-local single-active via the store lock", justified by redb's exclusive file lock — but `--metadata-backend tikv` has no --data-dir/file lock; each process builds its own process-local `MemCoordination` and self-grants, so two `wyrd custodian --metadata-backend tikv` on one host both elect and reconstruct concurrently. Disposition: real cross-host fencing legitimately defers to the out-of-scope etcd `Coordination` backend (#365) — but a process MUST NOT log/document a safety property it does not hold. At minimum correct the log line + docstring to state honestly that single-active fencing is NOT enforced on the tikv arm pending #365. (Human may harden to a must-fix at the next Check.) Standing human calls owed at next Check (not blocking): gauge-over-repair-queue-not-chunk-population scope (reconstruction.rs docstring overstates it); Unhealthy-but-reachable peers kept as survivor/replacement targets (custodian.rs:128) — accept probe stand-in for #367 or require health-state filtering; the `--ids`/`--failure-domains` runbook contract change (usage says optional, require_aligned_topology now rejects); Validation in-process read-back vs live-exporter run; milestone slice-#1 decomposition confirmation. Verification hygiene: C4-verify failed on the new-crate rename artifact (`pathspec 'crates/telemetry/src/lib.rs'`), so per-fix red→green was reasoned not mechanically run — fix run-verify's new-crate-rename handling so the harness can establish red→green, or accept the code-derived reds explicitly.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
