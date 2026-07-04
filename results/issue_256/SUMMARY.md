# Result — issue 256 / m4.5-deploy-tikv-pd-etcd

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: author the `deploy/` bring-up for the single-zone "Small multi-node Production"

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

# Check review — issue 256 / m4.5-deploy-tikv-pd-etcd

**Task under review:** ship the single-zone "Small multi-node Production" bring-up under `deploy/` (a docker-compose stack composing TiKV-small + a 3-node PD ensemble + a 3-node etcd ensemble for L5 Coordination + local-disk D servers, outside the Cargo workspace), plus an `xtask` runner and a structural guard enforcing ADR-0010's "no workspace crate couples to an orchestrator API." Net-new infrastructure; verification posture MIXED (live bring-up + L5 discovery pre-declared off-Check).

**Grounding:** target `$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt`; patch is applied there. Cargo/docker execution is blocked in this sandbox, so red→green/compose-config live runs are taken from the deterministic gates (`check-gates.json`: C4-ci PASS, C4-verify PASS red→green); everything else was re-derived by reading the applied target source and grep.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Both binding criteria are addressed and specific: (a) `deploy/small-multi-node/docker-compose.yml` composes the four roles (etcd0-2, pd0-2, tikv, dserver0-2); (b) the no-orchestrator-coupling guard is red-when-planted. Matches brief §Success criterion (a)/(b). |
| C2 Reproduction (red pre-fix) | PASS | Net-new (no bug to repro); brief pre-declares the "red = criterion-absence" demonstrated via the flippable guard. `deploy_no_orchestrator_coupling.rs:654` plants `use kube::Client;` in a temp fixture and asserts the shared `scan_dir` catches it exactly once — a genuine demonstrated red, not a guard resting on non-existence. |
| C3 Change | PASS | Change is confined to `deploy/` + `xtask/` as scoped; no `crates/`/traits/format touch. `xtask/src/deploy_guard.rs` (new), exported at `xtask/src/lib.rs:16`, wired into CI at `xtask/src/main.rs:822`, runner at `main.rs:485`. |
| C4 Verification (red→green) | PASS | Gates: C4-ci (fmt/clippy/build/test/deny/conformance+guard) PASS and C4-verify red→green PASS. Independently confirmed the guard's green resting-state — `rg 'kube::\|k8s_openapi\|kube_runtime::'` over `crates/` returns no matches, so `scan_dir_is_green_over_the_real_workspace_crates` holds; `scan_line` (`deploy_guard.rs:355`) matches `kube::` on a real import line. Could not re-execute cargo/docker (sandbox blocks it); relied on the deterministic gate for the compiled/live legs. |
| C5 Causal adequacy | PASS | Addresses the actual goal (a bring-up recipe now exists; coupling is mechanically forbidden), not a symptom. Symptom-guard smell-test does NOT fire: D servers omit `--coordination` and register against in-process `MemCoordination` (grounded `crates/server/src/cli.rs:22,294`) as a *documented deferral* of a not-yet-existent backend (#365), not a capability probe/runtime guard papering over a load-time side effect; `docker_available` warn-skip is a test-env gate, not a production capability probe. |
| T1 Structure | PASS | New stack mirrors existing `deploy/tikv-single-node/`; runner mirrors `tikv-conformance` (not in `ci`, docker-gated); guard follows the `run_statics` single-source style. Files land in the expected places. |
| T2 Shape | PASS | Compose declares all four roles with non-colliding host ports (etcd 12379/22379/32379, pd 23791-3, tikv 20160/20180, dserver 50061-3), per-service volumes, bridge DNS; guard API is small and pure (`scan_line`/`scan_dir`). Referenced images/Dockerfile exist (`crates/chunkstore-grpc/tests/dserver/Dockerfile`, `wyrd-dserver:local` reused from root `docker-compose.yml:28`). |
| T3 Runtime | PASS | Compiled/tested leg carried by C4-ci gate; d-server CLI flags used by the compose commands (`--bind`/`--data-dir`/`--group`/`--failure-domain`) all exist (`crates/server/src/cli.rs:84,262`). Live bring-up + `docker compose config` are pre-declared off-Check (need a Docker host). |
| T4 Contribution | PASS | Load-bearing, not scaffolding: the guard is flippably red on a planted import and runs on every `cargo xtask ci` (`main.rs:822`); the stack is a real, parseable multi-node topology exercised by the compose-config test. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: is a 6-needle substring grep (`kube::`/`k8s_openapi`/`kube_runtime::`, `deploy_guard.rs:342`) a strong-enough enforcement of ADR-0010? It would not catch other orchestrator clients (nomad/containerd/docker API, or a k8s client under a different crate name) nor a Cargo.toml-declared-but-unimported dep. Brief says the mechanism is ILLUSTRATIVE/Do's choice, so this is acceptable-by-design — but a human owns whether the needle set is broad enough for the invariant's weight. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: the binding at-Check criteria are met, but real fitness — the stack actually standing up and "peers discovered through L5" — is DEFERRED off-Check, gated on #365 (etcd-backed Coordination) **plus a gateway/custodian process role the brief flags as likely UNTRACKED**. Human must (a) confirm the deferral posture is acceptable for merge into the M4 integration branch, (b) confirm/open the missing gateway/custodian tracking issue (brief §Success criterion DEFERRED, "flag at sign-off"), and (c) confirm the live `docker compose config` validity + bring-up on a Docker host / CI-eval run (#367) — not runnable in this sandbox. Runnable steps for the operator: `docker compose -f deploy/small-multi-node/docker-compose.yml config` (expect clean parse + all ten services), then `cargo xtask deploy-small-multi-node` on a Docker host (expect all seven endpoints to accept connections, then clean teardown). |

## Notes
- Prior-art check by affected path could not be settled mechanically here (git history/PR queries are blocked in this sandbox); raised under Validation for the human to confirm no closed/rejected `deploy/small-multi-node/` or orchestrator-guard work conflicts.
- No blocking findings. Two NEEDS-HUMAN rows (T5 guard-breadth judgment; Validation deferral + untracked gateway/custodian tracking + live bring-up) carry to SUMMARY §6.

### Advisory — codex

- `xtask/src/main.rs:262` — `cargo xtask deploy-small-multi-node` does not actually wait for every component it claims to smoke-check: `SMALL_MULTI_NODE_ENDPOINTS` covers etcd, PD, and TiKV only, while the compose stack publishes the D-server ports at `deploy/small-multi-node/docker-compose.yml:192`, `deploy/small-multi-node/docker-compose.yml:214`, and `deploy/small-multi-node/docker-compose.yml:236`. A broken or crash-looping `dserver*` service can therefore still produce the success message at `xtask/src/main.rs:310`.
- NEEDS-HUMAN — `xtask/src/main.rs:779` scans only `$workspace/crates` for orchestrator imports, but `Cargo.toml:24` makes `xtask` itself a workspace member. If the sign-off criterion is literally "no workspace crate imports a k8s/orchestrator API", the guard/test should include `xtask` or the docs should narrow the claim to product crates under `crates/`.
- NEEDS-HUMAN — `deploy/small-multi-node/docker-compose.yml:174` starts a three-D-server stack, but the first-deployment sizing guide calls out 6 D servers as the minimum and 9 for full RS(6,3) at `docs/design/architecture/m4-first-deployment-blueprint.md:399`. The brief says "local-disk D servers" without restating a count, so this may be an intentional CI/eval shrink, but it is not the documented small-production sizing.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Decision owed: is a 6-needle substring grep (`kube::`/`k8s_openapi`/`kube_runtime::`, `deploy_guard.rs:342`) a strong-enough enforcement of ADR-0010? It would not catch other orchestrator clients (nomad/containerd/docker API, or a k8s client under a different crate name) nor a Cargo.toml-declared-but-unimported dep. Brief says the mechanism is ILLUSTRATIVE/Do's choice, so this is acceptable-by-design — but a human owns whether the needle set is broad enough for the invariant's weight.
- [x] Validation — fitness-to-purpose — Decision owed: the binding at-Check criteria are met, but real fitness — the stack actually standing up and "peers discovered through L5" — is DEFERRED off-Check, gated on #365 (etcd-backed Coordination) **plus a gateway/custodian process role the brief flags as likely UNTRACKED**. Human must (a) confirm the deferral posture is acceptable for merge into the M4 integration branch, (b) confirm/open the missing gateway/custodian tracking issue (brief §Success criterion DEFERRED, "flag at sign-off"), and (c) confirm the live `docker compose config` validity + bring-up on a Docker host / CI-eval run (#367) — not runnable in this sandbox. Runnable steps for the operator: `docker compose -f deploy/small-multi-node/docker-compose.yml config` (expect clean parse + all ten services), then `cargo xtask deploy-small-multi-node` on a Docker host (expect all seven endpoints to accept connections, then clean teardown).
- [x] `xtask/src/main.rs:779` scans only `$workspace/crates` for orchestrator imports, but `Cargo.toml:24` makes `xtask` itself a workspace member. If the sign-off criterion is literally "no workspace crate imports a k8s/orchestrator API", the guard/test should include `xtask` or the docs should narrow the claim to product crates under `crates/`.
- [x] `deploy/small-multi-node/docker-compose.yml:174` starts a three-D-server stack, but the first-deployment sizing guide calls out 6 D servers as the minimum and 9 for full RS(6,3) at `docs/design/architecture/m4-first-deployment-blueprint.md:399`. The brief says "local-disk D servers" without restating a count, so this may be an intentional CI/eval shrink, but it is not the documented small-production sizing.

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
- issue_256: validate that the gateway/custodian process-role tracking issue exists (brief §Success criterion "flag at sign-off"); believed to exist but unconfirmed — file one if not.
- issue_256: smoke-check gap (codex) — `SMALL_MULTI_NODE_ENDPOINTS` (xtask/src/main.rs) waits on etcd/PD/TiKV only, not the D-server ports; a crash-looping `dserver*` still prints success. Add the D-server ports to the readiness wait.
