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

Review of issue #576: expose deployable d-server liveness/readiness through the standard gRPC health protocol, driven by backing-store health and isolated from data-plane admission pressure.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is fully bounded: store-health mapping, fail-closed errors, bounded refresh, stable operator bind, and overload bypass are explicit in `brief.md`; the production entry point supplies the configurable bind at `crates/server/src/cli.rs:632`. |
| C2 Reproduction (red pre-fix) | PASS | In an attributable scratch copy with production changes reversed but `crates/server/tests/health_probe.rs` retained, `cargo test -p wyrd-server --test health_probe` failed on missing `tonic_health` and `DServer::with_health_bind`, grounding the absent pre-fix capability at `crates/server/tests/health_probe.rs:235`. |
| C3 Change | PASS | The operational decision is satisfied because the deployable role defaults or parses a stable address and passes it into the server (`crates/server/src/cli.rs:632`, `crates/server/src/cli.rs:1412`), while the health listener is separately bound at `crates/server/src/dserver.rs:726`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the focused independent red→green plus the recorded CI gate is sufficient — all three focused tests passed, but `./engine/xtask.sh ci` could not be re-run because `engine/xtask.sh` is absent from `$PDCA_TARGET`, so the full fmt/clippy/build/deny/conformance result remains provisional; focused assertions are at `crates/server/tests/health_probe.rs:238`, `crates/server/tests/health_probe.rs:279`, and `crates/server/tests/health_probe.rs:353`. |
| C5 Causal adequacy | PASS | The root-cause decision is discharged: supervisors receive a known/configurable socket rather than an in-process-only endpoint (`crates/server/src/dserver.rs:81`), and readiness directly polls the served store instance with fail-closed mapping (`crates/server/src/dserver.rs:709`, `crates/server/src/dserver.rs:758`); no capability-probe/runtime-guard smell was added. |
| T1 Structure | PASS | The architecture decision preserves the existing data service while placing health on its own unlayered tonic server, which is the required isolation boundary (`crates/server/src/dserver.rs:820`, `crates/server/src/dserver.rs:863`). |
| T2 Shape | PASS | The public/configuration shape is coherent: a documented stable default, CLI override, builder setter, and inspection getter meet operator and library use cases (`crates/server/src/dserver.rs:89`, `crates/server/src/dserver.rs:624`, `crates/server/src/dserver.rs:633`). |
| T3 Runtime | PASS | Real loopback execution observed SERVING, both unhealthy and erroring transitions to NOT_SERVING, and a successful health RPC while one data admission slot was held; the saturation boundary is exercised at `crates/server/tests/health_probe.rs:395`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether prior-art clearance is complete — affected-path `git log --all` showed earlier server/admission work but no health-probe implementation, while closed/rejected remote work could not be mechanically inspected from the supplied artifacts; duplication/conflict risk therefore remains for sign-off. |
| T5 Judgment | NEEDS-HUMAN | Approve the new `tonic-health` dependency under the project-required ADR-0003 three-test audit and allowlist review — dependency provenance and maintenance policy affect long-term supply-chain acceptance (`Cargo.toml:161`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a dedicated default loopback probe port and named ChunkStore readiness status match the intended systemd/k8s/load-balancer deployment topology — the loopback suite proves protocol behavior, but operator fitness remains the required human judgment (`crates/server/src/dserver.rs:81`, `crates/server/src/dserver.rs:697`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether the focused independent red→green plus the recorded CI gate is sufficient — all three focused tests passed, but `./engine/xtask.sh ci` could not be re-run because `engine/xtask.sh` is absent from `$PDCA_TARGET`, so the full fmt/clippy/build/deny/conformance result remains provisional; focused assertions are at `crates/server/tests/health_probe.rs:238`, `crates/server/tests/health_probe.rs:279`, and `crates/server/tests/health_probe.rs:353`.
- [x] T4 Contribution — Decide whether prior-art clearance is complete — affected-path `git log --all` showed earlier server/admission work but no health-probe implementation, while closed/rejected remote work could not be mechanically inspected from the supplied artifacts; duplication/conflict risk therefore remains for sign-off.
- [x] T5 Judgment — Approve the new `tonic-health` dependency under the project-required ADR-0003 three-test audit and allowlist review — dependency provenance and maintenance policy affect long-term supply-chain acceptance (`Cargo.toml:161`).
- [x] Validation — fitness-to-purpose — Decide whether a dedicated default loopback probe port and named ChunkStore readiness status match the intended systemd/k8s/load-balancer deployment topology — the loopback suite proves protocol behavior, but operator fitness remains the required human judgment (`crates/server/src/dserver.rs:81`, `crates/server/src/dserver.rs:697`).

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
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
