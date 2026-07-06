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

# Check review — issue_366 / obs-floor-observability (iteration 8)

**Task under review:** the keystone slice of the M4 observability floor (proposal 0010 PRs 1–2) — extract the durability-telemetry seam into a shared `crates/telemetry`, wire `wyrd custodian` as a runnable/deployable process that installs the telemetry handle and runs the leader-elected reconstruction loop, and make the **binding day-one signal** demonstrable through that role: after a killed D-server the under-replicated durability count **rises then returns to ZERO**, read back via `gather_prometheus`. This iteration addresses iteration-7's three MUST-FIX items (false data-loss on a transient below-k outage; all-unreachable startup must fail loud; a no-free-domain repair must not floor the backlog gauge).

**Target state:** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0` is present and has the patch **applied** (verified: `Assessment::Unreachable`/`Blocked`, `emit_data_loss`, `cmd_custodian` panic, `require_aligned_topology` all present at the cited lines). Citations grounded there. The **gating** gate `C4-ci` (`cargo xtask ci`, full workspace) is GREEN; `C4-verify` (advisory) is red only on the recurring new-crate-rename harness artifact (`git mv custodian/src/telemetry.rs → telemetry/src/lib.rs`), a run-verify limitation, **not** a patch defect — not treated as a blocking C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Keystone correctly scoped to 0010 items 1–2 + the binding rise→zero day-one signal; telemetry extracted (`crates/telemetry/Cargo.toml`), custodian made deployable (`crates/server/src/cli.rs:534` `cmd_custodian`). Milestone decomposition (which slice this bundle carries) is a standing human call — see T5/V, not a spec defect. |
| C2 Reproduction (red pre-fix) | PASS | Each new test encodes a genuine pre-fix red: gauge floored at ≥1 pre-fix (`custodian_day_one.rs` `a_loss_beyond_tolerance…`, `…returns_to_zero`), `#[should_panic]` unsatisfied pre-fix (`cmd_custodian_fails_loud…`), false data-loss pre-fix (`a_transient_below_k_outage…`). Mechanical red→green via run-verify is blocked by the new-crate-rename artifact (harness, not patch) — reasoned-in-tests, decision owed only on accepting that hygiene caveat (recurring since iter-3). |
| C3 Change | PASS | Purely additive instrumentation + wiring per the brief invariants: no commit-protocol/consistency/on-disk-format change; new metrics are gauges/counters on the `tracing`→OTel seam; `ExporterConfig` keeps the backend un-hardcoded (`cli.rs:552`). |
| C4 Verification (red→green) | PASS | Gating `C4-ci` = pass (full workspace build/clippy/test/deny/conformance, `check-gates.json:33-40`). Advisory `C4-verify` red is the new-crate-rename pathspec artifact (`crates/telemetry/src/lib.rs` absent at base), a target-state/harness caveat — not a patch "cannot apply/compile". Cargo re-run of the day-one binary needs approval in this sandbox; the harness CI re-run is the canonical green. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause of the false page is fixed at the classification seam — a below-k shortfall explained only by unreachability is `Assessment::Unreachable`, not `Unrepairable` (`reconstruction.rs:412-422`). But the **probe-and-drop reachability membership** (`server/src/custodian.rs` `live_reconstruction_view`, `health()` stand-in) is a deferral to the etcd `Coordination` seam (#365), and `Unhealthy`-but-reachable peers stay eligible as survivor/placement targets. Decision owed: confirm the #365 membership deferral remains acceptable for the #367 gate, or require the classification seam to treat unreachable-during-reconstruction as missing rather than probe-and-drop. |
| T1 Structure | PASS | Telemetry seam no longer anchored in `custodian` (own crate); role wiring lives in `server/src/custodian.rs` (the one crate allowed concretes, ADR-0010); injectable `DServerConnector` + owned `ConfiguredDServer` fleet type isolate transport for headless coverage. |
| T2 Shape | PASS | `connect_fleet` + `require_aligned_topology` + `run_reconstruction_over_backend` factored as iter-5 asked; three durability levels split onto distinct signals (`under_replicated` gauge, `reconstruction_unreachable`, `reconstruction_repair_blocked`, `reconstruction_data_loss`) with a clear operator contract. |
| T3 Runtime | PASS | Level metrics are gauges so they return to zero (`reconstruction.rs:655`); `connect_with_timeout` prevents a hung fetch; loop logs-and-continues on `Store` fault, stops on `Fenced`; all-unreachable startup panics loud (`cli.rs:636`). Behaviour exercised by the day-one tests under green CI. |
| T4 Contribution | PASS | One coherent keystone slice; remaining floor items (3–7) are explicitly follow-on PRs, not smuggled in. No scope creep beyond 0010 items 1–2. |
| T5 Judgment | NEEDS-HUMAN | Recorded judgment calls owed at sign-off: (a) milestone decomposition / which 0010 slice this bundle is; (b) typed-errors×#255 sequencing (iter-7 determined "after M4 / adapt TiKV" — ratify + record in 0010/ADR); (c) `tikv` single-active is advertised-off honestly (warns it is NOT enforced, `cli.rs:586`) pending #365 — accept the warning-only posture or require real fencing now; (d) the `--ids`/`--failure-domains` runbook contract change (now REQUIRED with `--endpoints`, `cli.rs:179`) vs the canonical bring-up command. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The at-Check evidence is an **in-process** `gather_prometheus` read-back of the rise→zero signal; the **live Prometheus-scrape / OTLP-collector** day-one run on a Tier-2 node is deferred off-Check to #367 (collector substrate tracked as getwyrd/wyrd#446). Decision owed: confirm the in-process read-back is accepted fitness evidence for this gate and the live-exporter run is a pre-agreed sign-off/#367 item, not a surprise gap. |

### Advisory — adversary

# Adversarial review — issue_366 (iteration 8, keystone: telemetry seam + runnable custodian + durability signal)

Attacked the three iteration-7 MUST-FIXes the sign-off turns on (transient-vs-permanent
classification, all-unreachable fail-loud, no-free-domain "returns to zero") plus the
red→green evidence. Two findings I could not talk myself out of, one evidence weakness,
and several attempts that failed (the fix held).

## Findings

- **NEEDS-HUMAN — the new transient/permanent split makes `reconstruction_data_loss` blind to
  the most common real permanent loss: a node that dies and stays dead.**
  `crates/custodian/src/reconstruction.rs:396` classifies any below-`k` shortfall as
  `Assessment::Unreachable` (no alarm) whenever `survivors.len() + transient_missing >= k`,
  where `transient_missing` counts every missing fragment whose placed server is in
  `ctx.unreachable` (`reconstruction.rs:377`). `ctx.unreachable` is rebuilt **each pass** purely
  from the current health probe in `crates/server/src/custodian.rs:200-222` — there is **no
  aging / last-seen / grace** (grepped: none), so a *permanently dead* server is in
  `unreachable` on every pass, forever. Concrete failing case: RS(2,1), n=3, two of the three
  D-servers are killed and never come back (the ordinary "two nodes died" permanent loss).
  survivors=1, transient_missing=2, sum=3 ≥ k=2 → `Unreachable` on every pass →
  `emit_data_loss` (`reconstruction.rs:297,490`) **never fires**. The `data_loss` counter can
  only be reached when fragments are missing on a *reachable* server (fragment absent but
  `health()` = Ok). The test that "proves" data-loss
  (`crates/server/tests/custodian_day_one.rs:2735`, `a_loss_beyond_tolerance_…`) manufactures
  the below-`k` condition exactly that way — it `delete_fragment`s bytes off two *live*
  `MemDServer`s (`custodian_day_one.rs:2764-2775`) rather than killing the servers. So the
  signal the brief frames as "a loss beyond tolerance… DATA LOSS" is demonstrated only for
  on-disk deletion under a live server, and is silent for node death — the dominant durability
  incident and precisely the §7.4 day-one fault. This false-negative is *introduced by this
  diff* (pre-diff, below-`k` was unconditionally `Unrepairable`); real membership/lease that
  would tell "dead" from "blip" is deferred to #365. A human must decide whether shipping a
  data-loss alarm that cannot see a dead server is acceptable, or whether the deferral note
  must say so explicitly.

- **NEEDS-HUMAN — the binding "kill a D-server → under-replicated rises then returns to zero"
  does not hold on a bare, exactly-`n` deployment; it requires spare failure-domain capacity.**
  `crates/custodian/src/reconstruction.rs:409` diverts a `Repairable`-in-principle chunk to
  `Assessment::Blocked` (off the `reconstruction_under_replicated` gauge, onto
  `reconstruction_repair_blocked`, `reconstruction.rs:288,468`) whenever
  `select_distinct_domains_excluding` finds no free domain distinct from the survivors. Because
  `live_reconstruction_view` registers the topology only from the *reachable* subset
  (`crates/server/src/custodian.rs:210-212`), killing one node of a minimal RS(2,1) 3-node
  cluster leaves topology = {survivor A, survivor C} with no free domain → `Blocked`. The
  builder's own test `a_repair_with_no_free_domain_is_blocked_off_the_backlog_gauge`
  (`custodian_day_one.rs:3001`, a 3-server cluster) asserts exactly this:
  `under_replicated == 0.0`, `reconstruction_repair_blocked == 1.0` (`:3064-3074`), and it
  **stays** blocked across passes (`:3086-3096`) — it never returns to zero until an operator
  adds a domain. Every drill that *does* show the brief's under-replicated rise→zero
  (`custodian_day_one.rs:2631`, `:2735`, `:3305`) hands the role a **4th spare server (domain
  D)**. So the load-bearing at-Check signal, as literally stated in the brief
  (`brief.md:52-59`), only manifests on the `under_replicated` gauge when spare capacity
  exists; on a real minimum-width cluster the same kill surfaces on a *different* gauge that
  does not return to zero. The #367 operator runbook needs to know which metric to watch is a
  function of spare capacity — a human should ratify this as the intended contract rather than
  let "kill a D-server, watch under-replicated rise then settle to zero" stand unqualified.

- **NEEDS-HUMAN (evidence) — the per-fix red→green was never mechanically run; "RED pre-fix"
  is asserted in prose, and it would be a compile error, not a behavioral failure.**
  `check-gates.json:41-49` records C4-verify **fail**: `pathspec 'crates/telemetry/src/lib.rs'
  did not match any file(s) known to git` (the `git mv custodian/src/telemetry.rs → new
  crates/telemetry` rename artifact, recurring since iteration 3). The entire day-one suite
  (`crates/server/tests/custodian_day_one.rs`, new file) depends on symbols this diff
  introduces — `Assessment::{Unreachable,Blocked}`, `emit_data_loss`,
  `reconstruction_repair_blocked`, `run_reconstruction_over_backend`, `ConfiguredDServer`,
  `ReconstructionContext.unreachable` — so it cannot compile against the pre-fix tree. The
  asserted "pre-fix reads 2 / no data_loss metric / floors at 1" (e.g. `custodian_day_one.rs:2823`,
  `:2833`) is therefore reasoned, not observed. C4-verify being non-gating means nothing
  mechanically proved these tests fail for the *defect* rather than for missing API. A human
  should either accept the code-derived red explicitly or have run-verify taught the
  new-crate-rename so the proof is real.

## Attempts that failed (the fix held)

- Tried to show the iteration-7 `RepairOutcome::Aborted` gauge-floor survives: assess's
  `Blocked` pre-check (`reconstruction.rs:409`) calls the *same* `select_distinct_domains_excluding`
  that `repair_chunk` uses (`reconstruction.rs:530`), and topology/fleet/`stores` are built from
  one reachable set (`custodian.rs:210-212`), so the second abort path (`reconstruction.rs:542`,
  target outside the fleet) cannot fire in the deployable role — a no-domain chunk is diverted
  before it ever becomes a plan. Could not floor the backlog gauge this way.
- Tried to break the all-unreachable fail-loud: `crates/server/src/cli.rs:1497-1511` panics on an
  empty fleet *inside* `runtime.block_on` on the calling thread, so it unwinds to `main` →
  non-zero exit; the redb open happens only afterward in `run_reconstruction_over_backend`, so
  the panic is not gated behind I/O. The `#[should_panic]` test (`custodian_day_one.rs:3617`)
  and the misaligned-topology reject test (`:3597`) drive `cmd_custodian` end-to-end. Could not
  make it exit 0.
- Tried to find a false `Blocked` that under-counts real backlog (assess more conservative than
  repair): the two selector calls are identical in arity and inputs, so assess and repair agree.
  Could not produce a Repairable-that-assess-calls-Blocked divergence on the production path.

### Advisory — codex

- No advisory findings. I found no patch-introduced correctness bugs or reuse / simplification / efficiency issues that merit a human sign-off item.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Root cause of the false page is fixed at the classification seam — a below-k shortfall explained only by unreachability is `Assessment::Unreachable`, not `Unrepairable` (`reconstruction.rs:412-422`). But the **probe-and-drop reachability membership** (`server/src/custodian.rs` `live_reconstruction_view`, `health()` stand-in) is a deferral to the etcd `Coordination` seam (#365), and `Unhealthy`-but-reachable peers stay eligible as survivor/placement targets. Decision owed: confirm the #365 membership deferral remains acceptable for the #367 gate, or require the classification seam to treat unreachable-during-reconstruction as missing rather than probe-and-drop.
- [x] T5 Judgment — Recorded judgment calls owed at sign-off: (a) milestone decomposition / which 0010 slice this bundle is; (b) typed-errors×#255 sequencing (iter-7 determined "after M4 / adapt TiKV" — ratify + record in 0010/ADR); (c) `tikv` single-active is advertised-off honestly (warns it is NOT enforced, `cli.rs:586`) pending #365 — accept the warning-only posture or require real fencing now; (d) the `--ids`/`--failure-domains` runbook contract change (now REQUIRED with `--endpoints`, `cli.rs:179`) vs the canonical bring-up command.
- [x] Validation — fitness-to-purpose — The at-Check evidence is an **in-process** `gather_prometheus` read-back of the rise→zero signal; the **live Prometheus-scrape / OTLP-collector** day-one run on a Tier-2 node is deferred off-Check to #367 (collector substrate tracked as getwyrd/wyrd#446). Decision owed: confirm the in-process read-back is accepted fitness evidence for this gate and the live-exporter run is a pre-agreed sign-off/#367 item, not a surprise gap.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_366: #365 membership deferral accepted at sign-off on condition it is recorded loudly — #367 runbook / 0010 must state that `reconstruction_data_loss` does NOT fire on node death until #365 lands; watch `reconstruction_unreachable` staying non-zero as the dead-node proxy.
- issue_366: #367 runbook must qualify the day-one signal — which gauge to watch is capacity-conditional: with spare failure-domain capacity a kill shows as `under_replicated` rise→zero; on a bare exactly-n cluster it lands on `reconstruction_repair_blocked` and stays until capacity is added.
- issue_366: run-verify cannot handle new-crate renames (`git mv` into a new crate breaks the pathspec) — recurring since iter-3, forced accepting a reasoned rather than observed red; teach the harness the rename case.
