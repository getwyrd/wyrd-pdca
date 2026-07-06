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

# Check review — issue 366 / obs-floor-observability (iteration 7)

**Task under review:** deliver the keystone slice of the operational-visibility floor (proposal 0010 items 1–2) — a *runnable, deployable* `wyrd custodian` process that installs the telemetry handle and runs the fenced reconstruction loop, so the binding day-one signal (kill a D-server → under-replicated count RISES then RETURNS TO ZERO, read back via `gather_prometheus`) is observable through a real process, on a *populated* store. This iteration must clear the iteration-6 rejection: an `Unrepairable` (below-k, un-reconstructable) loss was being folded into the under-replicated gauge, flooring it ≥1 forever and breaking "returns to zero".

**Grounding note:** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0` is the post-fix worktree (telemetry crate + `server/custodian.rs` present, patch applied). Citations ground there; behaviour that could not be re-run under the sandbox is grounded on `patch.diff` + the green gate. The gating **C4-ci gate is PASS** ("xtask ci: all checks passed" — full `cargo test --workspace`). The non-gating **C4-verify FAIL is the known new-crate-rename harness artifact** (`pathspec 'crates/telemetry/src/lib.rs'` — the `custodian/src/telemetry.rs → telemetry/src/lib.rs` move the harness can't stash), recurring since iter-3/5; it is a verification-hygiene artifact, **not** a patch defect, and is treated as advisory, not a blocking C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Keystone spec (day-one durability signal through the runnable custodian) is well-defined and delivered; the milestone *decomposition* itself is a standing human call (see T5), not a spec gap. `brief.md:50-59`. |
| C2 Reproduction (red pre-fix) | PASS | Mechanical red→green blocked only by the new-crate-rename harness artifact (C4-verify, advisory). The two new tests assert behaviour the fix *introduces* — pre-fix the gauge reads 2→1 and `reconstruction_data_loss` does not exist; post-fix 1→0 with the counter raised (`patch.diff:686-688`) — a credible, well-constructed red. Could not stash/run red myself (no git; target is post-fix). |
| C3 Change | PASS | Purely additive instrumentation + wiring: gauge now tallies only `Repairable`, `Unrepairable`→`emit_data_loss`, `Malformed`→`emit_needs_human` (`reconstruction.rs:160-216`), plus the deployable-role wiring in `server`. No commit-protocol/on-disk-format touch. |
| C4 Verification (red→green) | PASS | Gating `C4-ci` PASS on the target (full workspace test green, `check-gates.json:33-40`). `C4-verify` FAIL is the telemetry new-crate-rename harness artifact (non-gating) — not a defect; not presented as a blocking FAIL. |
| C5 Causal adequacy | NEEDS-HUMAN | The iteration-6 root cause is properly fixed (loss excluded from the level gauge, raised on its own `reconstruction_data_loss`). BUT fleet membership is a per-pass **reachability probe-and-drop** — `health().is_ok()` at `custodian.rs:195-204` reads *around* a down/Unhealthy peer with a TOCTOU window, standing in for a registration/lease seam; and tikv "single-active" is warning-only (unfenced, `cli.rs:569-597`). Decision owed: ratify deferring the probe-and-drop membership + real tikv fencing to the etcd `Coordination` backend (#365) for #367, or require it now — **these accepted deferrals must LAND WITH #365**. |
| T1 Structure | PASS | Telemetry seam cleanly extracted to `crates/telemetry`; connector abstracted behind an injectable `DServerConnector` trait with an owned `ConfiguredDServer` and a single `connect_fleet` assembly point (`custodian.rs:97,113,143`). |
| T2 Shape | PASS | Backend routed through the established `resolve_backend`/`--metadata-backend` seam (tikv `#[cfg]`-gated as put/get); topology operator-supplied and `require_aligned_topology`-checked — no positional fabrication (`cli.rs:706-749`). |
| T3 Runtime | PASS | Start-degraded around startup-down peers, survive-the-kill via probe-drop, and Fenced→stop are all exercised; workspace tests green under C4-ci. `custodian_day_one.rs` §1–7. |
| T4 Contribution | PASS | New coverage lands where iters 3–6 were rejected: populated-store return-to-zero for both `Malformed` and `Unrepairable` (`reconstruction.rs` tests), and `cmd_custodian` driven end-to-end through the real entry (`custodian_day_one.rs:2931,2966`). |
| T5 Judgment | NEEDS-HUMAN | Decision owed, per 0010 graduation criteria: (a) ratify this bundle as keystone slice items 1–2 of 0010's seven-PR sequence; (b) **record** the typed-errors (item 6) × #255 (M4.4) sequencing decision before the two run in parallel; (c) confirm the taken telemetry-extraction call (extract into `crates/telemetry`). None can be settled by the reviewer. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | In-process `gather_prometheus` read-back is the at-Check fitness evidence; the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 node is deferred (off-Check, #367). Decision owed: confirm in-process read-back suffices at Check and ratify the live-exporter run as the pre-agreed #367 sign-off item — not a surprise. `brief.md:67-70`. |

## Notes for the human (§6 carry)
- **C5 / deferrals must land with #365** (not silently dropped): probe-and-drop fleet membership (`custodian.rs:195-204`), and tikv single-active fencing that is honestly warning-only (`cli.rs:569-597`, no store lock on the shared backend). No corruption results meanwhile (repoint is a CAS commit), but the safety property is genuinely absent on the tikv arm.
- **Iteration-6 items all addressed & grounded:** (1) `Unrepairable`→dedicated `reconstruction_data_loss` counter + NEEDS-HUMAN audit line, removed from the backlog gauge, with a populated-store return-to-zero test (`reconstruction.rs:179,618`; test `under_replicated_gauge_excludes_unrepairable_data_loss_so_it_returns_to_zero`); (2) usage text now states `--ids`/`--failure-domains` REQUIRED when `--endpoints` present (`cli.rs:170,179`); (3) `cmd_custodian` driven end-to-end by two real `#[test]`s (`custodian_day_one.rs:2931,2966`).
- **Verification hygiene:** C4-verify cannot mechanically establish red→green while the telemetry crate is a rename (new-crate-rename artifact). Recommend the harness gain new-crate-rename handling, or the human explicitly accept the code-derived reds — this has recurred every iteration and is the last verification-mechanics gap.

### Advisory — adversary

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

### Advisory — codex

- NEEDS-HUMAN — `crates/custodian/src/reconstruction.rs:179` excludes `Unrepairable` chunks from `reconstruction_under_replicated` and moves them to `reconstruction_data_loss`; that makes the rise-then-zero gauge clean on populated stores, but it changes the prior requirement that the under-replicated/durability count cover lost-beyond-tolerance chunks. Human should confirm the separate data-loss metric is the accepted operator contract.
- `crates/server/src/custodian.rs:165` swallows startup dial failures and omits those endpoints from the owned fleet forever; later passes only probe the retained `ConfiguredDServer`s at `crates/server/src/custodian.rs:204`, so a D-server that is temporarily down when the custodian starts is never retried/rejoined and will be treated as permanently missing until the process restarts.
- NEEDS-HUMAN — `crates/server/src/cli.rs:628` returns `Ok` and exits when every configured D-server is unreachable at startup. That supports the new “start degraded” policy, but for a deployable long-running custodian it can turn a transient fleet-wide outage or bad endpoint config into a clean process exit with no subsequent retries or durability telemetry.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — The iteration-6 root cause is properly fixed (loss excluded from the level gauge, raised on its own `reconstruction_data_loss`). BUT fleet membership is a per-pass **reachability probe-and-drop** — `health().is_ok()` at `custodian.rs:195-204` reads *around* a down/Unhealthy peer with a TOCTOU window, standing in for a registration/lease seam; and tikv "single-active" is warning-only (unfenced, `cli.rs:569-597`). Decision owed: ratify deferring the probe-and-drop membership + real tikv fencing to the etcd `Coordination` backend (#365) for #367, or require it now — **these accepted deferrals must LAND WITH #365**.  [RATIFIED: proper membership/fencing is the #365 etcd Coordination work (M4-adjacent, 0015 prerequisite; NOT M5, which is the step-ca trust fabric); #365's own bundle has §6 cleared + gates green, so this is a real landing target. Probe-and-drop + warning-only tikv fencing deferred to land with #365. NB: the probe-and-drop's live false-data-loss-on-transient-outage behavior is a SEPARATE present-bundle concern, not covered by this deferral.]
- [ ] T5 Judgment — Decision owed, per 0010 graduation criteria: (a) ratify this bundle as keystone slice items 1–2 of 0010's seven-PR sequence; (b) **record** the typed-errors (item 6) × #255 (M4.4) sequencing decision before the two run in parallel; (c) confirm the taken telemetry-extraction call (extract into `crates/telemetry`). None can be settled by the reviewer.
- [x] Validation — fitness-to-purpose — In-process `gather_prometheus` read-back is the at-Check fitness evidence; the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 node is deferred (off-Check, #367). Decision owed: confirm in-process read-back suffices at Check and ratify the live-exporter run as the pre-agreed #367 sign-off item — not a surprise. `brief.md:67-70`.  [RATIFIED: the in-process read-back exercises the real OTel instrument→OTel-Prometheus exporter→text-exposition pipeline (custodian_day_one.rs:428,461,575), accepted as binding at-Check evidence; 0010's "live exporter, not read-back" graduation DoD is deferred to the #367 first-deployment gate as pre-agreed. Gap closed: the collector substrate #367's live run needs is now tracked as getwyrd/wyrd#446 (deploy Prometheus/OTLP collector or ratify operator-supplied) — previously unowned.]
- [x] `crates/custodian/src/reconstruction.rs:179` excludes `Unrepairable` chunks from `reconstruction_under_replicated` and moves them to `reconstruction_data_loss`; that makes the rise-then-zero gauge clean on populated stores, but it changes the prior requirement that the under-replicated/durability count cover lost-beyond-tolerance chunks. Human should confirm the separate data-loss metric is the accepted operator contract.  [ACCEPTED as the operator contract: the two-metric split (repairable-backlog gauge returns to zero; below-k loss on its own high-severity `reconstruction_data_loss`) is the intended shape. NB: the SEPARATE transient-vs-permanent defect that misfires this signal on a transient outage is NOT accepted — see iterate-do rationale.]
- [ ] `crates/server/src/cli.rs:628` returns `Ok` and exits when every configured D-server is unreachable at startup. That supports the new “start degraded” policy, but for a deployable long-running custodian it can turn a transient fleet-wide outage or bad endpoint config into a clean process exit with no subsequent retries or durability telemetry.  [NOT ACCEPTED (revised at sign-off): an all-unreachable startup fleet must NOT be a silent `return Ok(())` — the process must PANIC (fail loud, non-zero exit + diagnostic) so the supervisor restarts and an operator sees it, rather than exit 0 and vanish. Sent to iterate-do.]

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
- Iteration delta (if iterating): issue_366 — the keystone slice (telemetry seam + runnable custodian + the rise/return-to-zero durability signal) is close, and the governance/scope calls are settled (see below). But the BINDING day-one signal mis-fires on a routine transient fault, and that must be fixed before this ships. MUST FIX — the transient-vs-permanent distinction at the membership/classification seam: - `live_reconstruction_view` (crates/server/src/custodian.rs:195-213) DROPS any D-server whose health probe errs, so a transiently-down server (rolling restart / partition isolating m+1 nodes) has its fragments counted as MISSING in `assess`. When survivors fall below k, `assess` returns `Unrepairable` and the pass fires `emit_data_loss` at ERROR severity ("DATA IS LOST; NEEDS-HUMAN", reconstruction.rs:179 -> :618-625) — a FALSE permanent-data-loss alarm on physically-intact fragments that fully recover when the nodes return. The drop-path bypasses the very transient/permanent distinction `assess` otherwise implements. Fix: do not classify a below-k that is driven by REACHABILITY (server dropped from the view) as confirmed data loss / Unrepairable; distinguish "unreachable right now" from "fragments confirmed gone" before emitting the high-severity data-loss signal. - Add a test that actually exercises it: RS(2,1) (or equivalent), transiently down enough D-servers that survivors < k, assert NO `reconstruction_data_loss` is emitted and the signal recovers when the nodes return. The current suite never covers this — every day-one drill hands the custodian a spare server (server 3), so the false-alarm path is uncovered. ALSO FIX (revised at sign-off, §6.5) — an all-unreachable startup fleet must FAIL LOUD, not exit silently: - `crates/server/src/cli.rs:628` currently prints a notice and `return Ok(())` when `connect_fleet` yields an empty fleet (every D-server unreachable at startup). For a deployable long-running custodian that is exit 0 — the supervisor does not restart and the operator sees nothing. Change it to PANIC (non-zero exit + diagnostic) so a transient fleet-wide outage / bad `--endpoints` is a loud, restartable failure rather than a clean vanish. (The per-peer "start degraded around ONE down server" behaviour stays; only the ALL-unreachable case changes.) ALSO FIX (same binding-signal-integrity class, also uncovered) — "returns to ZERO" must hold for a Repairable chunk whose repair repeatedly aborts: - `RepairOutcome::Aborted` (reconstruction.rs, no free/distinct domain — e.g. minimal cluster at exactly n, one killed) offsets `reconstruction_aborted` but leaves the obligation queued; next pass it re-classifies `Repairable` and re-counts under_replicated, pinning the gauge at >=1 forever. Same floored-gauge failure the iter-5/6 rejections chased, surviving in the never-repaired-Repairable case. Add a no-free-domain drill and make the gauge return to zero (or route the un-repairable-for-now case off the repairable-backlog gauge). RATIFIED this iteration — do NOT re-litigate: - §6.1 membership/fencing (probe-and-drop + warning-only tikv fencing) deferred to the #365 etcd Coordination backend (M4-adjacent; NOT M5/step-ca). #365 is human-accepted with §6 cleared + gates green, so the deferral target is real. - §6.2 (a) keystone = 0010 PRs 1-2; (c) telemetry extraction into crates/telemetry. (b) typed-errors x #255 sequencing is DETERMINED: #255 is merged onto the M4 branch, so typed-errors (item 6 / PR-3) lands AFTER M4 and adapts the TiKV MetadataStore; recorded in SUMMARY §10 for Act to formalize in 0010/an ADR. - §6.3 in-process gather_prometheus read-back accepted as at-Check evidence; live-exporter run deferred to #367; the collector substrate #367 needs is now tracked as getwyrd/wyrd#446. - §6.4 the two-metric split (repairable-backlog gauge + dedicated reconstruction_data_loss) is the ACCEPTED operator contract. (§6.5 was NOT accepted — see the fail-loud/panic MUST FIX above.) Verification-hygiene note (not blocking, recurring): C4-verify is red on the new-crate-rename artifact (git mv custodian/src/telemetry.rs -> telemetry/src/lib.rs), so per-fix red->green is reasoned-in-tests, not mechanically run. Either teach run-verify new-crate-rename handling or keep accepting code-derived reds explicitly.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_366 §6.2(b) sequencing record — formalize in proposal 0010 / an ADR note: the typed-errors enum (0010 item 6 / PR-3) sequences AFTER M4/#255, which is now MERGED onto the M4 branch, so PR-3 must ADAPT M4's TiKV `MetadataStore` to emit the new variants. The "land the enum first" arm is foreclosed by the merge; keep the enum additive on the pre-1.0 `crates/traits` seam. This satisfies 0010's graduation criterion ("typed-errors/M4 sequencing recorded before the two run in parallel"), which is currently unrecorded anywhere in docs/ADRs/act-log.
