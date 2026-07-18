# Result — issue 576 / tonic-health-readiness-probes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: a deployment supervisor (systemd, k8s, a load balancer) can ask a wyrd gRPC
- Success criterion: against a served d-server (the workspace's gRPC role), a
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: register the standard `tonic-health` service on the d-server's tonic server

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Review of issue 576: expose standard gRPC liveness/readiness for the d-server, backed by `ChunkStore::health()` and reachable under data-plane saturation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision boundary is explicit: standard gRPC health, fail-closed readiness with bounded refresh, and probe availability under admission saturation; no material requirement is ambiguous (`brief.md`, Design). |
| C2 Reproduction (red pre-fix) | PASS | Retaining `crates/server/tests/health_probe.rs` while reversing production reproduced RED as unresolved `tonic_health` and missing `with_health_refresh_interval` at `crates/server/tests/health_probe.rs:134`. |
| C3 Change | FAIL | A deployment supervisor needs a stable/configured or advertised probe address, but production binds an unpredictable second port and exposes it only via an in-process getter, so real systemd/k8s/load-balancer probes cannot discover what to dial (`crates/server/src/dserver.rs:531`, `crates/server/src/dserver.rs:625`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept incomplete broad-gate reproduction: focused RED→GREEN was reproduced and all three loopback tests pass, but `cargo xtask ci` reached `cargo deny check` and could not acquire the read-only advisory DB lock, so the deny result remains provisional (`crates/server/tests/health_probe.rs:337`). |
| C5 Causal adequacy | FAIL | The implementation proves health semantics only for callers already holding `DServer` and therefore does not remove the operational root cause—a deployment supervisor still lacks a usable probe endpoint (`crates/server/tests/health_probe.rs:130`, `crates/server/tests/health_probe.rs:136`). |
| T1 Structure | PASS | The human must only decide endpoint exposure; within the chosen two-server composition, the health service is isolated from admission layers and the refresher is cancelled with serving (`crates/server/src/dserver.rs:824`, `crates/server/src/dserver.rs:868`). |
| T2 Shape | PASS | The added ordinary integration test drives the real server over loopback without feature gating, matching the required test shape (`crates/server/tests/health_probe.rs:130`, `crates/server/tests/health_probe.rs:151`). |
| T3 Runtime | PASS | Independent execution observed 3/3 focused tests pass, including Healthy, Unhealthy, error fail-closed, recovery, and saturated-data-plane behavior (`crates/server/tests/health_probe.rs:334`, `crates/server/tests/health_probe.rs:371`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether prior art is clear: affected-path merged history was checked and showed no prior tonic-health implementation, but closed/rejected work could not be mechanically searched because repository search access was unavailable; duplication risk therefore remains unsettled (`crates/server/src/dserver.rs:690`). |
| T5 Judgment | NEEDS-HUMAN | Approve or reject the new `tonic-health` dependency under the project’s ADR-0003 three-test audit; this governs supply-chain/license acceptance beyond the mechanically exercised code (`Cargo.toml:168`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the operator-facing contract is fit only after providing a stable/configurable/advertised health address and exercising an actual supervisor against it; the current test bypasses that deployment boundary via `health_endpoint()` (`crates/server/tests/health_probe.rs:136`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept incomplete broad-gate reproduction: focused RED→GREEN was reproduced and all three loopback tests pass, but `cargo xtask ci` reached `cargo deny check` and could not acquire the read-only advisory DB lock, so the deny result remains provisional (`crates/server/tests/health_probe.rs:337`).
- [ ] T4 Contribution — Decide whether prior art is clear: affected-path merged history was checked and showed no prior tonic-health implementation, but closed/rejected work could not be mechanically searched because repository search access was unavailable; duplication risk therefore remains unsettled (`crates/server/src/dserver.rs:690`).
- [ ] T5 Judgment — Approve or reject the new `tonic-health` dependency under the project’s ADR-0003 three-test audit; this governs supply-chain/license acceptance beyond the mechanically exercised code (`Cargo.toml:168`).
- [ ] Validation — fitness-to-purpose — Decide whether the operator-facing contract is fit only after providing a stable/configurable/advertised health address and exercising an actual supervisor against it; the current test bypasses that deployment boundary via `health_endpoint()` (`crates/server/tests/health_probe.rs:136`).

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
- Iteration delta (if iterating): Rejected per advisory review C3/C5: the health service binds a second, ephemeral port exposed only via the in-process `health_endpoint()` getter, so a real supervisor (systemd/k8s/LB) has no stable or configurable address to dial — the operational root cause is not removed. Next attempt: keep the health semantics (mapping, fail-closed, admission bypass — all reviewed as sound) but give the health listener a stable/configurable bind address (cli.rs flag plumbing is already within the brief's scope) so the probe endpoint is discoverable by an operator, and exercise that configured address in the test instead of bypassing the deployment boundary via `health_endpoint()`.
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_576: C4 — reviewer's independent `cargo xtask ci` rerun stalled at `cargo deny check` (couldn't acquire the read-only advisory-DB lock in its sandbox); make the reviewer environment able to run deny, or make the gate outcome authoritative for the review.
