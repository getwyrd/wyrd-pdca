# Result — issue 439 / fdb-dev-ci-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) exists and works —
- Success criterion: `cargo test -p xtask --test fdb_harness` passes on the plain
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Give the shipped-but-ungated `fdb` backend a standing automated signal and an

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

Issue 439 adds a standing FoundationDB conformance CI signal, an actionable FDB preflight doctor, and feature-gated FDB type-check coverage without making the normal gate require Docker or libfdb_c.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The patch targets the specified surfaces: FDB workflow paths include `crates/metadata-fdb/**` and `crates/server/**`, the doctor is exported from the xtask lib, and the audit note stays in `deny.toml` rather than `docs/design/` (`.github/workflows/fdb-conformance.yml:42`, `xtask/src/lib.rs:18`, `deny.toml:7`). |
| C2 Reproduction (red pre-fix) | PASS | A clean `HEAD` archive run of `cargo test -p xtask --test fdb_harness` is red with “no test target named `fdb_harness`”, while the patched harness contains planted-red cases for the doctor rows and workflow evasions (`xtask/tests/fdb_harness.rs:304`, `xtask/tests/fdb_harness.rs:827`). |
| C3 Change | PASS | The change supplies the missing automation and preflight path: the workflow actually runs `cargo xtask fdb-conformance`, `run_fdb_conformance` delegates to the gated preflight, and `feature_gated_checks` emits both FDB feature rows (`.github/workflows/fdb-conformance.yml:134`, `xtask/src/main.rs:309`, `xtask/src/lib.rs:84`). |
| C4 Verification (red→green) | PASS | Re-ran `cargo test -p xtask --test fdb_harness` green on the patch, re-ran `cargo xtask ci` green, and independently reproduced the pre-fix red from a clean `HEAD` archive; the exact wrapper scripts named in `check-gates.json` were not present, so I reran their substantive gates directly (`xtask/tests/fdb_harness.rs:872`, `xtask/src/main.rs:1385`). |
| C5 Causal adequacy | NEEDS-HUMAN | Capability-probe smell-test is triggered: the fix intentionally adds a `libfdb_c`/environment preflight, so the human must confirm this is the accepted root-cause treatment rather than removing eager system-library coupling (`xtask/src/fdb_doctor.rs:418`, `xtask/src/fdb_doctor.rs:454`). |
| T1 Structure | PASS | Pure decision logic lives in the lib target and the impure command body is a pass-through into the gate, preserving container-free testability (`xtask/src/fdb_doctor.rs:280`, `xtask/src/main.rs:312`). |
| T2 Shape | PASS | The workflow command-head contract is constrained and tested against the real dispatch table, including explicit evasion cases for comments, arguments, and no-op prefixes (`xtask/tests/fdb_harness.rs:790`, `xtask/tests/fdb_harness.rs:872`). |
| T3 Runtime | NEEDS-HUMAN | Docker access was denied in this sandbox, so I verified only the local skip/remediation path; the human must confirm the live single-node FDB stack actually passes with Docker access (`xtask/src/fdb_doctor.rs:101`, `xtask/src/main.rs:323`). |
| T4 Contribution | NEEDS-HUMAN | Local affected-path history was checked with `git log --all -- <paths>` and showed no prior doctor/workflow/harness files, but artifact-only review could not mechanically settle closed/rejected PR prior art, so the human must clear that corpus (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`). |
| T5 Judgment | PASS | No deterministic patch defect found: scope is confined to the requested files, no new Cargo dependency was added, and the remaining concerns are explicit human/environment clearances (`xtask/tests/fdb_harness.rs:645`, `deny.toml:15`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off is owed on whether the built signal is sufficient for production confidence, because the GitHub-hosted workflow and Docker-backed live conformance run were not observable from this sandbox (`.github/workflows/fdb-conformance.yml:125`, `.github/workflows/fdb-conformance.yml:144`). |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Capability-probe smell-test is triggered: the fix intentionally adds a `libfdb_c`/environment preflight, so the human must confirm this is the accepted root-cause treatment rather than removing eager system-library coupling (`xtask/src/fdb_doctor.rs:418`, `xtask/src/fdb_doctor.rs:454`).
- [x] T3 Runtime — Docker access was denied in this sandbox, so I verified only the local skip/remediation path; the human must confirm the live single-node FDB stack actually passes with Docker access (`xtask/src/fdb_doctor.rs:101`, `xtask/src/main.rs:323`).
- [x] T4 Contribution — Local affected-path history was checked with `git log --all -- <paths>` and showed no prior doctor/workflow/harness files, but artifact-only review could not mechanically settle closed/rejected PR prior art, so the human must clear that corpus (`.github/workflows/fdb-conformance.yml:1`, `xtask/src/fdb_doctor.rs:1`).
- [x] Validation — fitness-to-purpose — Human sign-off is owed on whether the built signal is sufficient for production confidence, because the GitHub-hosted workflow and Docker-backed live conformance run were not observable from this sandbox (`.github/workflows/fdb-conformance.yml:125`, `.github/workflows/fdb-conformance.yml:144`).
- [x] advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

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
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Leaf-infra bug: the codex adversary leaf cannot access Docker, so the adversarial review produced no artifact for this bundle (empty verdict was infra, not substance).
