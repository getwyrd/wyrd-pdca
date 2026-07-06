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

# Check review — issue 366 / obs-floor-observability (keystone slice)

**Task under review:** deliver the observability-floor keystone (proposal 0010 items 1–2) —
extract the durability-telemetry seam into a shared `wyrd-telemetry` crate, make `wyrd custodian`
a runnable/deployable role, and prove the day-one signal (kill a D-server → the under-replicated
count *rises then returns to zero*, read back via `gather_prometheus`), while correcting the two
iteration-2 rejections (Adv-1 crash-on-killed-D-server; Adv-2 gauge undercounts the worst losses).

Target grounded at `$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt-l1` (patch-applied worktree,
confirmed readable). Note: I could not run `cargo`/`git` in the reviewer sandbox (both require an
approval I cannot grant), so the C4 verdict is grounded on the deterministic gate result plus
static re-derivation on the target source, not a local test re-run.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Keystone is well-specified by 0010 (items 1–2 + the binding rise-then-zero signal); the diff targets exactly that. Milestone decomposition is a separate human call (see T5), not a C1 gap. |
| C2 Reproduction (red pre-fix) | PASS | Re-derived: pre-fix `emit_under_replicated(plans.len())` reads **0** for an `Unrepairable` chunk (survivors < k), so the added `under_replicated_gauge_counts_a_loss_beyond_tolerance` test (asserts 1) is genuinely red pre-fix. The harness auto-stash could not confirm mechanically — it errored `pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git`, a new-crate **rename artifact** in `run-verify` (non-gating C4-verify), not a patch defect. |
| C3 Change | PASS | Diff implements the intended change on target: seam extracted to `crates/telemetry/src/lib.rs` with API preserved via re-export (`crates/custodian/src/lib.rs:46`), deployable role wired (`crates/server/src/cli.rs:463` `cmd_custodian`, `crates/server/src/custodian.rs`), gauge correction (`crates/custodian/src/reconstruction.rs:159,165,192,541`). |
| C4 Verification (red→green) | **FAIL** | Gating gate red: `cargo xtask ci` → `cargo test --workspace --exclude wyrd-dst` exited **101** (check-gates.json C4-ci). This is a genuine **test-phase** red (build/fmt/clippy passed; test binaries ran/compiled and something panicked or failed to compile), **not** the stale-target/rename artifact — the patch applied and reached the test phase. I could not reproduce the exact failing test locally; the human/builder must pull the CI log. Accept is blocked deterministically regardless of this advisory row. **Decision owed:** confirm which test fails — if it is the binding `custodian_day_one.rs` signal, the day-one claim itself is unproven; if a pre-existing custodian telemetry test, the seam extraction/gauge change regressed it. |
| C5 Causal adequacy | **NEEDS-HUMAN** | Two contested root-cause calls. (a) The Adv-1 crash-on-killed-D-server is fixed by a **reachability probe that pre-filters the fleet** (`crates/server/src/custodian.rs:115` `health().await.is_ok()`) + catch-and-continue (`:285`), *not* by fixing the cause: an unreachable fetch is still classified **transient** and propagated in reconstruction (`crates/custodian/src/reconstruction.rs` ~:341 `is_permanent_read_fault`/`EIO`), so a server dying mid-pass still throws. This is the symptom-guard smell — a capability/reachability guard around a present cause. **Decision owed:** should the classification seam treat unreachable-during-reconstruction as missing (remove the cause) rather than probe around it? (b) Iteration-2 §6.3 demanded *real cross-process leader election*; the builder instead ships host-local file-lock single-active with a lone-process `MemCoordination` (`crates/server/src/cli.rs:524`) and re-scopes cross-host fencing as out-of-scope (`:534` advertisement). **Decision owed:** is deferring true single-active acceptable for this slice, reversing the prior must-fix? |
| T1 Structure | PASS | Clean crate extraction: new `crates/telemetry` owns the OTel/Prometheus deps, `custodian` re-exports the M3 API unchanged (`crates/custodian/src/lib.rs:46`), server gains `pub mod custodian`. No leaf crate anchors the backend (0010 invariant honored). |
| T2 Shape | PASS | Gauge-as-*level* is the correct instrument for a rise-then-zero signal (a monotonic counter pins at 1 through an accumulating registry); documented at `crates/custodian/src/reconstruction.rs:522-541`. `gauge.` prefix is supported by the pinned `tracing-opentelemetry 0.33` (verified in the vendored `metrics.rs:24,176`), so the shape is viable. |
| T3 Runtime | **FAIL** | Same gate as C4: the workspace test phase is red (exit 101), so runtime behavior is not green. Candidate loci are the new `crates/server/tests/custodian_day_one.rs` assertions or a pre-existing `crates/custodian/tests/reconstruction.rs` telemetry test — one or more panics/fails to build. |
| T4 Contribution | PASS | Delivers the keystone (deployable role + shared seam + gauge correctness) that the remaining floor items 3–7 build on; scoped as the recommended first bundle. Positive contribution, incomplete milestone by design. |
| T5 Judgment | **NEEDS-HUMAN** | Enumerated human calls carried by 0010/brief remain open and are exercised by this diff: (1) telemetry **extract-vs-keep** — builder chose extract (matches iteration-1 sign-off guidance); (2) **milestone decomposition** — which slice this bundle is; (3) **typed-errors × #255 (M4.4) sequencing** must be *recorded* before parallel work; (4) the leader-election scoping call (also in C5b). **Decision owed:** ratify these judgment calls (or send back). Prior-art check: iterations v1/v2 carry-forward are the prior art; the builder addressed the tracing-subscriber comment (now honestly documents the log subscriber as a follow-on gap) and the fleet-includes-dead-node test — confirm those closures satisfy the reviewers who raised them. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The live Prometheus-scrape / OTLP day-one run on a Tier-2 node is off-Check (pre-agreed sign-off item, 0010 DST/ADR-0012); and whether the runnable `wyrd custodian` role genuinely satisfies architecture §7.4 step-4 in a real deployment is a human fitness judgment. **Decision owed:** schedule/accept the off-Check exporter evidence and confirm the role meets the first-deployment gate (#367). |

### Advisory — adversary

# Adversarial review — issue 366 (obs-floor / deployable custodian)

Lens: refute the red→green evidence and the reviewer's verdict. Grounded on the target
source at `$PDCA_TARGET` (patch applied to the working tree). C4 is already a **gating
FAIL**; the findings below explain *why*, and attack the fix's fitness where the gates are
silent.

## The fix is broken for its own binding purpose

- **NEEDS-HUMAN — The deployable `wyrd custodian` cannot reach the M4 production metadata
  backend.** `cmd_custodian` unconditionally calls `open_local_meta_redb(data_dir)`
  (`crates/server/src/cli.rs:516`) — it never calls `resolve_backend` and exposes no
  `--metadata-backend` flag (contrast `cmd_put`/`cmd_get`, which do:
  `cli.rs:233`/`cli.rs:320`, with a real TiKV arm at `cli.rs:257`). The brief's **BINDING**
  success is "the day-one durability signal through the wired, runnable custodian role" as
  an **M4 deployment prerequisite**, and M4 production runs the **TiKV** metadata backend
  (redb is dev/eval only, ADR-0014). Against a real M4 cluster, `wyrd custodian` opens an
  empty local redb file, finds no chunks and no repair obligations, and the under-replicated
  gauge **never rises** — the signal is undemonstrable on the very deployment it gates. The
  at-Check test hides this by handing the role a hand-built in-memory `MemMeta`
  (`custodian_day_one.rs:352`) instead of driving `cmd_custodian`'s backend open.

- **Concrete hang: the "survive the killed D-server" path stalls on a silent peer.**
  `cmd_custodian` dials each endpoint with `GrpcChunkStore::connect(...)`
  (`cli.rs:558`), which builds a channel with **no request/connect timeout**.
  `live_reconstruction_view` then probes `store.health().await` each pass
  (`crates/server/src/custodian.rs:606`). The codebase's own
  `GrpcChunkStore::connect_with_timeout` (`crates/chunkstore-grpc/src/client.rs:79`) exists
  precisely because "an RPC to a server that has stopped responding mid-call — a
  `docker pause`d node or an injected network partition that leaves the connection
  established but the peer silent — would hang the future indefinitely" (client.rs:70-78).
  The day-one fault is *kill a D-server*; a process-kill yields connection-refused (→ `Err`
  → dropped, fine), but a **partition / pause** leaves `health()` hanging forever, stalling
  the whole reconcile loop — the role neither survives nor emits. The test's `DeadDServer`
  returns `Err` from `health()` **instantly** (`custodian_day_one.rs:329-…`), so the hang
  is never exercised.

- **NEEDS-HUMAN — Fabricated failure domains can defeat the durability invariant the
  custodian exists to uphold.** `cmd_custodian` keys each D-server positionally,
  `id: i as DServerId` with a synthetic `failure_domain: format!("domain-{i}")`
  (`cli.rs:573-574`), ignoring each D-server's real registered failure domain. Two servers
  in the **same real** rack/domain are therefore modelled as distinct domains, so a rebuilt
  fragment can be re-placed onto a server in the same real failure domain as a survivor —
  the exact domain-collapse the reconstruction placement guard is meant to prevent. This is
  the iteration-2 adversary's DServerId concern; the builder *documented* it (cli.rs:570-577
  comment) but did **not** fix it. The at-Check test never exercises this: it hand-builds
  `ConfiguredDServer` with correct distinct domains A/B/C/D (`custodian_day_one.rs:…`,
  `four_domains`), bypassing the binary's synthetic assignment.

- **The "single-active" leadership advertised by the binary is hollow in production.**
  `cmd_custodian` constructs `MemCoordination::new()` (`cli.rs:524`), which always grants
  leadership to the lone process; the eprintln advertises "single-active". The builder
  re-scoped the iteration-2 fix to "host-local single-active via the redb store lock" — but
  that lock only exists for the redb file, and (per finding 1) the role can't open a shared
  store at all. For any multi-host deployment two custodians are unfenced. The rebuild did
  **not** implement cross-process election as iteration-2 §6.3 required; it renamed the gap.

## The evidence is weaker than the brief claims

- **The at-Check test does not exercise the production loop it names.** The module/test
  comments claim the "exact production path `cli::cmd_custodian → run_reconstruction_until →
  live_reconstruction_view + reconcile_pass`" (`custodian_day_one.rs:12,367`), but the test
  body calls only `live_reconstruction_view` + `reconcile_pass` directly
  (`custodian_day_one.rs:395,417,433,500,521`); it never calls `run_reconstruction_until`,
  `cmd_custodian`, or `open_local_meta_redb`. The loop's advertised survival behaviour —
  `ReconcileError::Store` → log-and-continue, `ReconcileError::Fenced` → stop
  (`custodian.rs:766-781`) — is **untested**. "Survive the kill" is demonstrated only by
  `live_reconstruction_view` dropping a health-erroring stub plus one manual pass.

- **NEEDS-HUMAN — the per-fix red→green was never mechanically established, and the "red"
  is a compile error, not the defect.** `check-gates.json` C4-verify failed with
  `pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git`. Both
  at-Check tests import `wyrd_server::custodian` and `wyrd_telemetry`
  (`custodian_day_one.rs:51,50`) — entirely new module + crate. Reverting the patch to
  reach "red" deletes the imported symbols, so pre-fix the test **does not compile**; a
  compile-error "red" is not a red→green proof that the test catches the defect. Separately,
  the modified DST property (`crates/dst/tests/custodian.rs`, monotonic→gauge) runs **only**
  in `wyrd-dst`, which the C4 gate **excludes** (`--exclude wyrd-dst`), so the gauge-semantics
  change's principal property test is outside the gate.

- **NEEDS-HUMAN — the gating C4 failure is a flaky, tracing-callsite-race read-back test.**
  `cargo test --workspace --exclude wyrd-dst` fails at
  `crates/custodian/tests/rebalance.rs:884`
  (`emits_per_failure_domain_utilization_on_the_durability_seam`), reproduced
  non-deterministically (2 of 3 parallel standalone runs; passes in isolation). Cause:
  `tracing` caches per-callsite *interest* in process-global state, so under parallel
  scheduling a sibling no-subscriber test poisons the `capacity_domain_utilization` callsite
  before this test installs its metrics subscriber. The patch does not touch rebalance.rs
  (so likely **pre-existing** — human to confirm), **but** it is the reason C4 (gating) is
  red *on this patch*, and it is the **same** process-global read-back mechanism the patch's
  own evidence depends on — the builder even isolates `custodian_day_one.rs` into its own
  binary for exactly this reason (test header, lines 43-45). The keystone's whole test
  strategy rides on a mechanism the suite already demonstrates is racy.

## Attempted and could not refute

- The `reconstruction.rs` gauge change itself (`emit_under_replicated` counting
  `Repairable + Unrepairable + Malformed`, monotonic→gauge, `reconstruction.rs:160,179,187,
  202,231`) is sound and genuinely covered: `under_replicated_gauge_counts_a_loss_beyond_
  tolerance` asserts `1` where pre-fix `plans.len()` was `0` for the below-`k` case — a real
  behavioural distinction. I ran the at-Check binary 12× under parallelism; it was stable
  (0 failures). I could not make the gauge read a wrong value for the covered cases.

### Advisory — codex

- `crates/server/src/cli.rs:516` — `wyrd custodian` always opens local redb metadata and the usage line exposes no `--metadata-backend`, while `put`/`get` already route through `resolve_backend`; in a TiKV-backed M4 deployment the custodian will scan a different metadata plane, miss the real repair queue, and the day-one under-replicated signal will not rise for production writes.
- `crates/server/src/custodian.rs:115` — the live-fleet probe awaits `health()` without any request deadline, and the CLI constructs clients with the no-timeout `GrpcChunkStore::connect` path at `crates/server/src/cli.rs:558`; a paused/partitioned D-server can hang the custodian loop before it drops the node, so the role may still fail the “survive a killed/timed-out D-server” requirement.
- `crates/server/src/cli.rs:572` — the custodian fabricates `DServerId` and failure-domain labels from `--endpoints` order instead of the D-server registration’s stable `id` / `failure_domain`; if operators use `d-server --id` / `--failure-domain` or reorder endpoints, reconstruction can read the wrong placement IDs and choose replacement domains from invented topology.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Two contested root-cause calls. (a) The Adv-1 crash-on-killed-D-server is fixed by a **reachability probe that pre-filters the fleet** (`crates/server/src/custodian.rs:115` `health().await.is_ok()`) + catch-and-continue (`:285`), *not* by fixing the cause: an unreachable fetch is still classified **transient** and propagated in reconstruction (`crates/custodian/src/reconstruction.rs` ~:341 `is_permanent_read_fault`/`EIO`), so a server dying mid-pass still throws. This is the symptom-guard smell — a capability/reachability guard around a present cause. **Decision owed:** should the classification seam treat unreachable-during-reconstruction as missing (remove the cause) rather than probe around it? (b) Iteration-2 §6.3 demanded *real cross-process leader election*; the builder instead ships host-local file-lock single-active with a lone-process `MemCoordination` (`crates/server/src/cli.rs:524`) and re-scopes cross-host fencing as out-of-scope (`:534` advertisement). **Decision owed:** is deferring true single-active acceptable for this slice, reversing the prior must-fix?
- [ ] T5 Judgment — Enumerated human calls carried by 0010/brief remain open and are exercised by this diff: (1) telemetry **extract-vs-keep** — builder chose extract (matches iteration-1 sign-off guidance); (2) **milestone decomposition** — which slice this bundle is; (3) **typed-errors × #255 (M4.4) sequencing** must be *recorded* before parallel work; (4) the leader-election scoping call (also in C5b). **Decision owed:** ratify these judgment calls (or send back). Prior-art check: iterations v1/v2 carry-forward are the prior art; the builder addressed the tracing-subscriber comment (now honestly documents the log subscriber as a follow-on gap) and the fleet-includes-dead-node test — confirm those closures satisfy the reviewers who raised them.
- [ ] Validation — fitness-to-purpose — The live Prometheus-scrape / OTLP day-one run on a Tier-2 node is off-Check (pre-agreed sign-off item, 0010 DST/ADR-0012); and whether the runnable `wyrd custodian` role genuinely satisfies architecture §7.4 step-4 in a real deployment is a human fitness judgment. **Decision owed:** schedule/accept the off-Check exporter evidence and confirm the role meets the first-deployment gate (#367).
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
- Iteration delta (if iterating): Rejected: both gates red, and the keystone fails its own binding purpose — the deployable custodian is wired to the wrong metadata plane. Primary defect (must fix): cmd_custodian (cli.rs:516) hardcodes `open_local_meta_redb(data_dir)` — no resolve_backend call, no --metadata-backend flag, no TiKV arm. M4 production runs the TiKV metadata backend (redb is dev/eval only), so on a real cluster the custodian opens an empty local redb, sees zero chunks / zero repair obligations, and the day-one under-replicated gauge never rises — the signal is undemonstrable on the very deployment (#367) it gates. The solution MUST route the custodian through the backend: reuse the established seam (resolve_backend + --metadata-backend flag + `#[cfg(feature="tikv")] MetadataBackend::Tikv => open_tikv_meta().await?`), exactly as cmd_put/cmd_get and the helpers at cli.rs:865/910 already do. And the day-one signal must be demonstrated driving cmd_custodian against the backend, NOT a hand-built in-memory MemMeta (custodian_day_one.rs:352 masks the defect). Also fix (implementation, all flagged and unaddressed/renamed since iteration 2): - No-timeout hang: live-fleet probe awaits health() with no deadline via the no-timeout GrpcChunkStore::connect (cli.rs:558 / custodian.rs:606). A paused/partitioned peer hangs the reconcile loop forever; the day-one "survive a killed D-server" only holds for connection-refused. Use connect_with_timeout (it exists for exactly this) so partition/pause is survived. Test's DeadDServer returns Err instantly and never exercises the hang. - Fabricated failure domains: cmd_custodian keys D-servers positionally with synthetic domain-{i} (cli.rs:572-574), ignoring each server's registered failure_domain — a rebuilt fragment can be re-placed into the same real domain as a survivor, defeating the durability invariant. Use the D-server's stable id/failure_domain, don't invent topology. - Test must drive the real loop: custodian_day_one.rs calls live_reconstruction_view/reconcile_pass directly and never run_reconstruction_until/cmd_custodian; the advertised survival behaviour (Store->continue, Fenced->stop) is untested. Gate notes: C4-ci (gating) red at rebalance.rs:884 — a flaky tracing-callsite-race read-back test, pre-existing (patch doesn't touch rebalance.rs) but it's the SAME process-global read-back mechanism this keystone's own evidence rides on; re-run/stabilize. C4-verify red is a new-crate rename artifact in run-verify, so per-fix red->green was never mechanically established and the "red" is a compile error, not the defect. Open judgment for the next Check (not blocking the do): cross-process leader election (iteration-2 §6.3) was re-scoped to out-of-scope; the MemCoordination single-active is host-local only. Confirm the deferral is acceptable or require it. §6 items: none ticked — cannot accept while gates are red; open items are the reject basis.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
