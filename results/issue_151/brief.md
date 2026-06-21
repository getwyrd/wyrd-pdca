# Brief — issue 151 / ci-enforce-rust-gate-and-dco-required

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** ci-enforce-rust-gate-and-dco-required
- **Defect / goal:** the required status checks on `main` are only `adr-immutability`,
  `docs-check`, `require-issue` (verified via `gh api
  repos/getwyrd/wyrd/branches/main/protection`). The Rust gate (`ci` → job `rust`:
  fmt / clippy `-D warnings` / build / test / cargo-deny / conformance / DST) and `dco`
  are **not** required, `required_approving_review_count` is 0, and `enforce_admins` is
  false — so a PR that fails `cargo xtask ci` or lacks a DCO sign-off can be merged. The
  `rust` non-requirement is deliberate (#125: path-filtered so docs-only PRs skip it, and
  a path-filtered workflow used as a required check leaves docs-only PRs stuck "pending").
  But the same change moved `dco` to its own always-on workflow precisely because "DCO
  must cover every commit" (ADR-0003 §1) — yet `dco` was never added to the required list,
  so it runs without gating: a stated-intent/config mismatch.
- **Success criterion:** `ci.yml` gains a small always-runs "gate" job (no paths filter)
  that `needs:` the `rust` job and reports success when `rust` was skipped (docs-only) or
  passed, failure otherwise; AND `dco` is wired to be gating-ready. Because the deliverable
  is pure workflow YAML and the C4 gate `cargo xtask ci` runs only Rust checks (it never
  reads `.github/workflows/`, and the repo has no actionlint), the **gating Check criterion
  is deterministic inspection at Check**: the reviewer confirms `ci.yml` contains the gate
  job with the correct `needs: [rust]` and the skip-or-pass `if:` logic, and that
  `cargo xtask ci` stays green (no Rust regression). The **behavioral** demonstration — the
  gate job reporting correctly on a real docs-only vs. a code PR — is observable only by
  running GitHub Actions and is therefore **supplementary / NEEDS-HUMAN** (a live-PR or
  fork-CI observation), not the gating criterion. NOTE: flipping the branch-protection
  **required-checks** set (add the gate job + `dco`) is a GitHub admin action recorded and
  performed by the human at sign-off — not an in-repo artifact, and **NEEDS-HUMAN**.
- **Invariant to restore:** every code-affecting PR must pass the Rust gate
  (build / clippy / test / cargo-deny) and every commit must carry a DCO sign-off
  (ADR-0003 §1) **before merge** — enforced, not merely reported. (Provenance + correctness
  gate, stated over the PR category, not a single workflow.)
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (CI config only)
- **Scope:** add the always-runs gate-aggregation job to `.github/workflows/ci.yml` (keep
  the docs-only path-skip on `rust`); make `dco` gating-ready. / **out of scope:** removing
  the docs-only path-skip (the #125 decision stands); requiring code review or
  `enforce_admins` (posture choices — note for the human, do not implement unasked); any
  change to what `cargo xtask ci` runs.
- **Citations expected:** Do cites `.github/workflows/{ci,dco}.yml` path:line on `main` and
  references #125 + ADR-0003 §1.
- **Prior-art check:** #125 (closed) is the deliberate non-required decision; the live
  required-check contexts are confirmed via `gh api`. No open PR re-enables the gate; the
  gate-job pattern is net-new here.
- **Disposition hint:** likely-fix

## Sign-off note (expected NEEDS-HUMAN)
The branch-protection required-checks change (and any review / `enforce_admins` posture) is
the maintainer's admin action at §9, distinct from the in-repo gate-job YAML the cycle
produces.

## STOP discipline
Draft only until Check sign-off. A draft PR MAY be opened for CI; it MUST NOT be marked
ready before sign-off accepts.
