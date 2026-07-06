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

# Check review — issue 366 / obs-floor-observability (iteration 6)

**Task under review:** deliver the keystone of the M4 observability floor (proposal 0010 items 1–2) — extract the telemetry seam into a shared `crates/telemetry` crate and wire `wyrd custodian` as a runnable, leader-elected process so the day-one durability signal (kill a D-server → the under-replicated count **rises then returns to zero**, read back via `DurabilityTelemetry::gather_prometheus`) is observable through a real deployed role, not a library caller.

**Target-state caveat:** `$PDCA_TARGET` could not be resolved in this sandbox (env read is gated and I must not wander into other checkouts), so every citation below is grounded on `patch.diff`. The deterministic gates ran green against the base (`check-gates.json`: C4-ci `pass`, C4-verify `pass`), so red→green verification is mechanically established — not merely reasoned as in iterations 3–5.

This iteration directly answers all four iteration-5 BLOCKING/REQUIRED items, each with a dedicated RED→GREEN test:
- **#1 gauge poisoned by Malformed** → `Malformed` no longer increments the under-replicated gauge (`reconstruction.rs` reconcile tally; `emit_under_replicated` now a **gauge** not a monotonic counter); test `under_replicated_gauge_excludes_malformed_so_it_returns_to_zero` drives a populated store (repairable + malformed) and asserts 1→0.
- **#2 evidence never drove the real binary path** → owned `ConfiguredDServer`, injectable `DServerConnector`, factored `connect_fleet` + `run_reconstruction_over_backend`; tested through `connect_fleet` (fake connector) and against a real on-disk redb via `run_reconstruction_over_backend`.
- **#3 exit-on-startup-down-peer** → `connect_fleet` reads *around* an unreachable peer (start-degraded); test `connect_fleet_starts_degraded_around_a_startup_down_peer_and_repairs`.
- **#4 false single-active on tikv** → per-backend honest wording; the tikv arm logs a WARNING that fencing is NOT enforced pending #365.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's BINDING criterion (brief.md:50–59) — under-replicated rises→zero via `gather_prometheus` through a runnable role — is the delivered scope; keystone items 1–2 match 0010 §Scope. Milestone-slice decomposition is deferred to T5/human (brief.md:170–173). |
| C2 Reproduction (red pre-fix) | PASS | Tests document their pre-fix red concretely: `gauge_counts_a_loss_beyond_tolerance` reads 0 pre-fix (patch.diff:2285), malformed test reads 2→1 pre-fix (patch.diff:687); C4-verify gate confirms red-without / green-with mechanically (check-gates.json:46). |
| C3 Change | PASS | Purely additive: new `crates/telemetry` (extraction of the deleted `custodian/src/telemetry.rs`), new `server/src/custodian.rs`, `wyrd custodian` subcommand wiring (cli.rs:1003). No commit-protocol / on-disk-format touch — honors the brief invariants (brief.md:152–168). |
| C4 Verification (red→green) | PASS | Both gates green: C4-ci (fmt/clippy/build/test/deny/conformance) `pass` and per-fix C4-verify `pass` (check-gates.json:34–48) — the callsite-interest flake that held iterations 3–5 red is now pinned via `enable_metric_callsites()` (patch.diff:2009). Re-ran gates not available (no target); grounded on gate record + tests present. |
| C5 Causal adequacy | NEEDS-HUMAN | Root causes of the four iter-5 defects are fixed at root, BUT survival-of-the-kill rests on a per-pass reachability **probe-and-drop** of the fleet (`live_reconstruction_view`, patch.diff:1518) that the code itself declares a *stand-in* for registration/lease membership (etcd `Coordination`, #365). **Decision owed:** is the probe stand-in acceptable for the #367 first-deployment gate, or must unreachable-during-reconstruction be classified as missing at the trait seam / must #365 land first? Contested symptom-vs-root-cause carried from iter-3 §C5a. |
| T1 Structure | PASS | Telemetry seam lives in a backend-agnostic `crates/telemetry` (patch.diff:2795), not anchored in `custodian`; concretes (gRPC dial, backend open) confined to the `server` crate per ADR-0010 (custodian.rs module header, patch.diff:1338). Layering is clean. |
| T2 Shape | PASS | Injectable `DServerConnector` trait + owned `ConfiguredDServer` + single `connect_fleet` (require+dial+map) and `run_reconstruction_over_backend` factoring (patch.diff:1466,1147) make the fleet path coverable headlessly; `connect_with_timeout` closes the no-timeout hang (patch.diff:1236); topology is operator-supplied via `require_aligned_topology`, never fabricated (patch.diff:1196). |
| T3 Runtime | PASS | C4-ci green covers build/clippy/test; run-loop behaviour is exercised — survives dead peer (patch.diff:2310), logs-and-continues on Store fault (patch.diff:2386), stops when Fenced (patch.diff:2444), start-degraded (patch.diff:2655). |
| T4 Contribution | PASS | Coherent keystone bundle: items 1–2 + the gauge correctness fix; items 3–7 explicitly deferred as follow-on floor slices (brief.md:119–125). No scope creep beyond 0010's seven items. |
| T5 Judgment | NEEDS-HUMAN | Standing judgment calls to record, not silently resolve: (a) **single-active is NOT enforced on the tikv arm** — documented honestly (cli.rs:1062) but defers real fencing to #365; human may harden to must-fix. (b) **`--ids`/`--failure-domains` are now REQUIRED with `--endpoints`** (require_aligned_topology, patch.diff:1196) — a runbook contract change from 0010's canonical flag-less bring-up command. (c) `cmd_custodian` itself (top-level glue: arg parse + `GrpcDServerConnector` wiring) is covered only via its factored halves, not end-to-end — iter-5 asked for a test "THROUGH cmd_custodian"; confirm the factored coverage suffices. (d) gauge is a level over the repair *queue*, not the whole chunk population. **Decision owed:** accept these dispositions or upgrade any to must-fix. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The BINDING signal is demonstrated **in-process** (`gather_prometheus` read-back); the DEFERRED live Prometheus-scrape / OTLP-collector run on a Tier-2 host against the day-one blueprint is off-Check supplementary evidence and is ultimately #367's gate (brief.md:67–70). **Decision owed:** confirm in-process read-back is the accepted at-Check fitness evidence and that the live-exporter run is a pre-agreed sign-off item, not a blocker — plus confirm this bundle is the intended milestone slice-#1 (brief.md:170–173). |

### Advisory — adversary

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

### Advisory — codex

- NEEDS-HUMAN — `crates/server/src/cli.rs:170` still advertises `--ids` and `--failure-domains` as optional for `wyrd custodian`, but any invocation with `--endpoints` now fails unless both lists are present and aligned (`crates/server/src/cli.rs:709`). Decide whether the runbook/CLI contract should require explicit topology or derive it from registration; as written, the help text and canonical easy-start shape lead operators to a startup error.
- NEEDS-HUMAN — `crates/server/src/cli.rs:571` explicitly warns that `--metadata-backend tikv` has no single-active fencing because each process uses local `MemCoordination`; sign-off should decide whether warning-only is acceptable for this slice, since the role still “campaigns for single-active leadership” in the entrypoint docs while TiKV production deployments can run multiple active custodians.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Root causes of the four iter-5 defects are fixed at root, BUT survival-of-the-kill rests on a per-pass reachability **probe-and-drop** of the fleet (`live_reconstruction_view`, patch.diff:1518) that the code itself declares a *stand-in* for registration/lease membership (etcd `Coordination`, #365). **Decision owed:** is the probe stand-in acceptable for the #367 first-deployment gate, or must unreachable-during-reconstruction be classified as missing at the trait seam / must #365 land first? Contested symptom-vs-root-cause carried from iter-3 §C5a.
- [ ] T5 Judgment — Standing judgment calls to record, not silently resolve: (a) **single-active is NOT enforced on the tikv arm** — documented honestly (cli.rs:1062) but defers real fencing to #365; human may harden to must-fix. (b) **`--ids`/`--failure-domains` are now REQUIRED with `--endpoints`** (require_aligned_topology, patch.diff:1196) — a runbook contract change from 0010's canonical flag-less bring-up command. (c) `cmd_custodian` itself (top-level glue: arg parse + `GrpcDServerConnector` wiring) is covered only via its factored halves, not end-to-end — iter-5 asked for a test "THROUGH cmd_custodian"; confirm the factored coverage suffices. (d) gauge is a level over the repair *queue*, not the whole chunk population. **Decision owed:** accept these dispositions or upgrade any to must-fix.
- [ ] Validation — fitness-to-purpose — The BINDING signal is demonstrated **in-process** (`gather_prometheus` read-back); the DEFERRED live Prometheus-scrape / OTLP-collector run on a Tier-2 host against the day-one blueprint is off-Check supplementary evidence and is ultimately #367's gate (brief.md:67–70). **Decision owed:** confirm in-process read-back is the accepted at-Check fitness evidence and that the live-exporter run is a pre-agreed sign-off item, not a blocker — plus confirm this bundle is the intended milestone slice-#1 (brief.md:170–173).
- [ ] `crates/server/src/cli.rs:170` still advertises `--ids` and `--failure-domains` as optional for `wyrd custodian`, but any invocation with `--endpoints` now fails unless both lists are present and aligned (`crates/server/src/cli.rs:709`). Decide whether the runbook/CLI contract should require explicit topology or derive it from registration; as written, the help text and canonical easy-start shape lead operators to a startup error.
- [ ] `crates/server/src/cli.rs:571` explicitly warns that `--metadata-backend tikv` has no single-active fencing because each process uses local `MemCoordination`; sign-off should decide whether warning-only is acceptable for this slice, since the role still “campaigns for single-active leadership” in the entrypoint docs while TiKV production deployments can run multiple active custodians.

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
- Iteration delta (if iterating): Why rejected: the `Unrepairable` (below-k, un-reconstructable) case is mishandled. An Unrepairable chunk is the storage system failing its primary responsibility — data that was meant to be durable is actually lost. That is MORE severe than `Malformed` (a metadata/placement error). Yet the patch gives Malformed the dignified treatment (its own `reconstruction_malformed_placement` metric + a NEEDS-HUMAN audit line) while folding Unrepairable silently into the generic `reconstruction_under_replicated` gauge (reconstruction.rs:189). That both buries the emergency and floors that gauge at >=1 forever, breaking the binding day-one "rises then returns to zero" signal for any store carrying a permanent loss. What to change next: 1. Give `Unrepairable` its own dedicated high-severity metric + NEEDS-HUMAN / audit surface (>= parity with Malformed's emit_needs_human), and REMOVE it from `reconstruction_under_replicated` so that gauge stays a true repairable-backlog level that returns to zero (preserving the binding day-one signal). Add a test: a second reconcile pass over an Unrepairable chunk shows the under-replicated gauge returns to zero AND the distinct data-loss signal is raised. 2. CLI help-text contract (codex): cli.rs:170 still advertises --ids/--failure-domains as optional, but --endpoints now requires them (cli.rs:709). Fix help text / canonical bring-up so operators don't hit a startup error, or derive topology from registration. 3. cmd_custodian end-to-end coverage (T5c): iter-5 asked for a test THROUGH cmd_custodian; still only the factored halves are covered. Add it or ratify. Accepted deferrals — do NOT re-litigate, but they must LAND WITH #365 (in the active development sequence; see SUMMARY §10): - C5 probe-and-drop fleet membership (live_reconstruction_view) -> #365 registration/lease Coordination. - T5(a) / codex tikv single-active fencing (local MemCoordination, warning-only) -> real fencing from #365. - Validation: in-process gather_prometheus read-back is the accepted at-Check fitness evidence; live Prometheus/OTLP scrape deferred to #367.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_366 — #365-deferred items to land WITH #365's work: (a) probe-and-drop fleet membership stand-in (`live_reconstruction_view`) → replace with #365 registration/lease `Coordination`; (b) tikv single-active fencing (currently local `MemCoordination`, warning-only, `cli.rs:571,1062`) → real fencing from #365. Deferrals accepted here because #365 is in the active development sequence; must not be silently dropped when #365 is built.
