# Result — issue 150 / ci-harden-nightly-tier2-job

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: on a failing integration run, container diagnostics
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the nightly Tier-2 job's **operability** only — (a) capture container logs on

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

# Check review — issue 150 / ci-harden-nightly-tier2-job

Advisory, artifact-only, decorrelated from the builder. Inputs: `brief.md`,
`patch.diff`, `check-gates.json` (build-notes withheld). Citations re-derived
against the target source at `/home/eddie/wyrd/wyrd` (read-only; it carries the
patch and matches `patch.diff`), cross-checked against `patch.diff`.

## Verdict matrix (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Success criterion is concrete and falsifiable: logs captured before teardown, `timeout-minutes` present, workflow-YAML verified by file inspection at Check (`brief.md:17-28`); split of gated-vs-inspected oracle is explicit (`brief.md:21-27`). |
| C2 — C2 Reproduction (red pre-fix) | N/A | No gate configured (`check-gates.json:15-22`). The defect is a structural teardown→capture ordering in a Docker-gated path; the new tests drive a **new** fn `finish_integration` that did not exist pre-fix, so no executable red-against-old-source state exists. Per `brief.md:27-28` the live container-failure path is supplementary nightly evidence, not the Check oracle. |
| C3 — C3 Change | PASS | Patch implements all three sub-goals: capture-before-teardown via `finish_integration` (`xtask/src/main.rs:118,130-144`), `compose_logs` persists logs to `target/tier2-logs/` ahead of `compose_down` (`xtask/src/main.rs:181-224`), `timeout-minutes: 45` (`.github/workflows/integration-nightly.yml:34`), and `if: failure()` artifact upload of that path (`.github/workflows/integration-nightly.yml:57-64`). |
| C4 — C4 Verification (red→green) | PASS | Sole gating row: `C4-ci` = pass, "xtask ci: all checks passed" (`check-gates.json:33-40`) — fmt/clippy/build/test green, so the two new unit tests run and pass under `cargo test`. Scope is the Rust change only; the workflow YAML is not exercised by `xtask ci` (`brief.md:22-27`) and is covered by C3/C5 inspection here. |
| C5 — C5 Causal adequacy | PASS | Root cause (teardown destroys diagnostics before the error propagates; no timeout; no failure surfacing) is uncontested and each leg is addressed: on `Err` capture runs, then teardown, then `result` propagates unchanged (`xtask/src/main.rs:139-143`); persisted path feeds the artifact upload (`xtask/src/main.rs:212-223` ↔ `.github/workflows/integration-nightly.yml:61-62`); hang bounded by `timeout-minutes` (`.github/workflows/integration-nightly.yml:34`). Final causal sign-off rests with the human (oracle: reviewer + human). |
| T1 — T1 Structure | PASS | Idiomatic `#[cfg(test)] mod tests { use super::*; }` co-located in the unit under test (`xtask/src/main.rs:496-540`); no fixtures or external harness needed. |
| T2 — T2 Shape | PASS | Two tests assert the right things via order-recording closures: failure path asserts `["capture_logs","teardown"]` + `is_err` propagation (`xtask/src/main.rs:507-519`); success path asserts `["teardown"]` only + `is_ok` (`xtask/src/main.rs:526-538`). Both branches and the propagation invariant covered. |
| T3 — T3 Runtime | PASS | Tests need no container runtime (generic `FnOnce` closures), so they execute under the `cargo test` leg of the green `C4-ci` gate (`check-gates.json:33-40`). |
| T4 — T4 Contribution | PASS | Tests genuinely guard the invariant: reordering to teardown-before-capture fails `failure_captures_logs_before_teardown` (`xtask/src/main.rs:507-519`); an always-capture regression fails `success_tears_down_without_capturing_logs` (`xtask/src/main.rs:526-538`). Limitation (honest, not a fail): the real I/O wiring — `compose_logs` writing `target/tier2-logs/` and the `if: failure()` upload — is verified by inspection (C3), not by any test, consistent with `brief.md:25-28`. |
| T5 — T5 Judgment | PASS | Extracting `finish_integration` to make ordering unit-testable without Docker is a sound, non-gaming choice (`xtask/src/main.rs:123-144`); closures mock only the two side effects, which is exactly the extracted invariant. Advisory PASS; oracle reserves final judgment to the human. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (oracle: human at sign-off, `check-gates.json:96-102`). Whether captured logs actually make a real nightly Tier-2 failure diagnosable can only be confirmed on the live Docker path, which is not exercised at Check. |

## §6 — items the human must clear

1. **Validation fitness-to-purpose (from V).** Confirm the operational intent is
   met: on a genuinely failing nightly run, `target/tier2-logs/docker-compose.log`
   is produced *before* `compose_down` and surfaces as the `tier2-container-logs`
   artifact. This requires the Docker-gated live path, which Check cannot run.
2. **Scope / scheduling conflict with #154 (ambiguous scope).** `brief.md:35,50-53`
   flags that #154 reworks the *same* `run_integration` teardown
   (`xtask/src/main.rs:111-118`) this patch touches. The human must ensure the two
   are not co-scheduled in one concurrent wave and that whichever lands second is
   rebased onto the other's teardown shape.
3. **Build-failure sub-path (advisory note, not a blocker).** If the cold image
   build itself hangs/fails, `compose_up` returns `Err` before `finish_integration`
   is reached (`xtask/src/main.rs:112`), so no `compose_logs` runs — there are no
   containers to log, and the upload's `if-no-files-found: ignore` tolerates the
   empty case. `timeout-minutes: 45` covers the hang. Confirm this is the intended
   behaviour; no diagnostics are expected for pure-build failures.

## Notes

- No FAIL found. The single gating oracle (`C4-ci`) is green and the workflow-YAML
  legs that `cargo xtask ci` does not exercise (`brief.md:22-27`) are confirmed
  present by direct inspection: `timeout-minutes: 45`
  (`.github/workflows/integration-nightly.yml:34`) and the `if: failure()` upload
  of `target/tier2-logs/` (`.github/workflows/integration-nightly.yml:57-64`),
  whose path matches the writer (`xtask/src/main.rs:213,218`).
- STOP discipline (`brief.md:55-57`): remains a draft until human sign-off; this
  review is advisory and does not itself accept.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] V — Validation — fitness-to-purpose — Always-human (oracle: human at sign-off, `check-gates.json:96-102`). Whether captured logs actually make a real nightly Tier-2 failure diagnosable can only be confirmed on the live Docker path, which is not exercised at Check.

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
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
