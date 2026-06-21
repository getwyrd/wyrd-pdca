# ci: make the Rust gate a requireable check; mark dco gating-ready

## Root cause
The entire `ci` workflow was path-filtered (`paths-ignore: docs/**, **/*.md,
LICENSE, NOTICE`), and GitHub leaves a path-skipped workflow's checks "pending"
forever — so requiring the Rust gate would block every docs-only PR, which is
why #125 deliberately kept `rust` out of the required-checks set and left a PR
failing `cargo xtask ci` mergeable. `dco` had the mirror gap: it is correctly
always-on but was never designated a required check, so it reported without
gating despite ADR-0003 §1 requiring a sign-off on every commit.

## Fix
Stop skipping the *workflow*; skip only the heavy *job*, and add an always-runs
aggregator that is safe to require.
- Removed the `push` and `pull_request` `paths-ignore` so the workflow always
  triggers and its jobs always report.
- Added a `changes` job that classifies the diff as code vs. docs-only via the
  same `gh api` idiom as `dco.yml` / `require-issue.yml` (no new action
  dependency), mirroring the exact #125 docs paths. It fails safe: an
  indeterminate changed-file list is treated as code so `rust` runs rather than
  being silently skipped.
- Gated `rust` on `needs.changes.outputs.code == 'true'` — the docs-only skip is
  preserved, just relocated from the workflow trigger to the job `if:`, so a
  docs-only PR now *skips the job* (run completes) instead of *skipping the
  workflow* (forever pending).
- Added a `gate` job (`needs: [changes, rust]`, `if: always()`) that passes when
  `changes` succeeded and `rust` was `success` or `skipped`, fails otherwise. It
  carries no path filter, so it always reports — this is the context a maintainer
  adds to the required-checks set.
- Added a header note to `dco.yml` designating its `dco` context as a required
  check (no behavioural change; it was already always-on).

The branch-protection flip — adding `gate` and `dco` to
`branches/main/protection`, plus any review / `enforce_admins` posture — is a
maintainer admin action, not an in-repo change, and is recorded as NEEDS-HUMAN.
This PR lands the precondition that makes that flip non-wedging.

## Verified against
- `.github/workflows/ci.yml:8-14` (main) — the banner stating the gate is NOT a
  required check because a path-skipped workflow stays pending; replaced by the
  job-level skip + always-runs `gate` rationale.
- `.github/workflows/ci.yml:16-30` (main) — the `push`/`pull_request`
  `paths-ignore` blocks removed so the workflow always triggers.
- `.github/workflows/ci.yml:39-40` (main) — the `rust` job, now preceded by
  `changes` and gated on `needs.changes.outputs.code`; `gate` appended after the
  non-gating `bench` step.
- `.github/workflows/dco.yml:3-7` (main) — the always-on provenance header,
  extended to designate `dco` as a required check per ADR-0003 §1.
- #125 (closed) — the deliberate non-required decision the relocated skip honours.
- ADR-0003 §1 — DCO must cover every commit; the config gap this closes.

## Test
`cargo xtask ci` provably never reads `.github/workflows/` (Rust-only:
fmt/clippy/build/test/cargo-deny/conformance/DST) and the repo has no actionlint,
so the deliverable has no automated gate surface in-tree. The brief's gating
Check criterion is therefore deterministic inspection of the workflow YAML, which
is encoded as an executable structural test (`test_ci_gate.py`, in the PDCA
bundle — not Wyrd's tree, consistent with "Surfaces: data, CI config only"). It
parses `ci.yml` / `dco.yml` and asserts the `gate` wiring (`needs: [rust]`,
`if: always()`, success-or-skipped logic), the relocated docs-only skip
(`changes` job + `rust` `if:`), the removed PR paths filter, and dco
gating-readiness. Proven red→green: it FAILS against `main` (PR paths filter
present; no `gate`/`changes` job; `rust` not gated on `changes`) and PASSES
against the patched tree.

Behavioural demonstration — the gate reporting correctly on a real docs-only vs.
code PR (live GitHub Actions / fork CI) — is supplementary and NEEDS-HUMAN, along
with the branch-protection required-checks flip.

Fixes #151
