# Build notes — issue #151 / ci-enforce-rust-gate-and-dco-required

> Withheld from the reviewer. Rationale, alternatives, and the cost behind each
> rejection.

## Target & citations (getwyrd/wyrd @ main, HEAD a829993)

- `.github/workflows/ci.yml` on main: workflow-level `paths-ignore` at lines
  19–30; `rust` job at lines 40–63; non-gating `bench` step at lines 69–71.
- `.github/workflows/dco.yml` on main: always-on provenance workflow, header at
  lines 2–7, `dco` job at lines 17–43.
- #125 (closed): the deliberate decision to keep `rust` out of the required set
  because a path-filtered workflow used as a required check wedges docs-only PRs
  in "pending".
- ADR-0003 §1: DCO must cover every commit — the stated intent that `dco` enforce
  on merge, which the current branch-protection config does not honour.

## Root cause

The whole `ci` workflow is path-filtered (`paths-ignore: docs/**, **/*.md,
LICENSE, NOTICE`). GitHub leaves a *skipped* workflow's checks in "pending"
forever, and a required check that is forever-pending blocks every docs-only PR.
That is precisely why #125 left `rust` non-required. So the Rust gate runs but
cannot gate: a code PR that fails `cargo xtask ci` is still mergeable. `dco` has
the inverse problem — it is correctly always-on (no paths filter) but was never
added to the required-checks set, so it reports without gating (ADR-0003 §1
intent vs. config mismatch).

## Fix — restore the invariant with the smallest YAML that makes the gate
## *reportable*

The brief names an **Invariant to restore** (every code-affecting PR passes the
Rust gate; every commit carries DCO — enforced, not merely reported), so the
deciding axis is "smallest change that restores the invariant", not smallest
diff (`docs/principles.md` §1.2, §2). The invariant has two halves:

1. **A status that is safe to require.** The canonical GitHub pattern for
   "required check that must tolerate a path-skipped heavy job" is: stop skipping
   the *workflow*; skip only the heavy *job*; add an always-runs aggregation job
   that reports the combined result. So:
   - Removed the `pull_request` (and `push`) `paths-ignore` — the workflow now
     always triggers, so its jobs always report.
   - Added a `changes` job that classifies the change as code vs. docs-only using
     the same `gh api` idiom as `dco.yml`/`require-issue.yml` (no new action
     dependency), mirroring the exact #125 docs paths.
   - Gated `rust` on `needs.changes.outputs.code == 'true'` — the docs-only
     path-skip is **preserved** (brief scope: "keep the docs-only path-skip on
     rust"), just relocated from the workflow trigger to the job `if:`. On a
     docs-only PR `rust` is now *skipped* (the run completes) rather than the
     whole workflow being *skipped* (forever pending).
   - Added the `gate` job: `needs: [changes, rust]`, `if: always()`, passing iff
     `changes` succeeded AND `rust` was `success` or `skipped`. This is the job a
     maintainer adds to the required-checks set; it always reports, so docs-only
     PRs never wedge.

2. **DCO discoverably gating-ready.** `dco.yml` is already always-on, so the only
   in-repo gap is that nothing records it as a *designated* required check. Added
   a header comment stating that the `dco` context belongs in the
   branch-protection required-checks set (ADR-0003 §1) and that no paths filter
   means it always reports. No behavioural change — changing its triggers was
   unnecessary and would be out of scope.

The actual enforcement flip — adding the `gate` and `dco` contexts to
`branches/main/protection` (and any `enforce_admins` / review posture) — is a
GitHub admin action, not an in-repo artifact, and is correctly **NEEDS-HUMAN**
(brief lines 28–30, 50–52). This Do beat ships the in-repo precondition that
makes that flip safe and non-wedging.

## Fail-safe choice

The `changes` classifier defaults to `code=true` whenever the changed-file list
can't be determined (API hiccup, first push with a zero `before` SHA). Skipping
the gate on uncertainty would silently let a code PR through — the opposite of
the invariant — so the safe default runs `rust`.

## Alternatives considered and rejected

- **Keep the workflow-level `paths-ignore` and add the gate job inside it.**
  Rejected: it cannot work. A job only runs when its workflow triggers; with the
  workflow path-filtered, a docs-only PR skips the workflow, so the gate job
  never runs and stays pending — the exact #125 failure. The gate *must* live in
  an always-triggering workflow, which forces removing the trigger filter.

- **`dorny/paths-filter@v3` for the `changes` job** instead of the ~22-line
  `gh api` shell block. Rejected on two concrete counts. (a) It adds a *new*
  third-party action; the repo currently pins only `actions/checkout`,
  `Swatinem/rust-cache`, and `taiki-e/install-action`, and a new marketplace
  action is supply-chain surface a maintainer must vet. (b) The "any file outside
  docs" semantics need `predicate-quantifier: 'every'` with four negated
  patterns; getting that wrong silently *skips* `rust` on a code PR — a direct
  invariant violation — and I cannot verify dorny's negation semantics against
  its source from here. The shell `case` block is house-style, dependency-free,
  and fully reasoned/tested locally, so the small extra line count buys
  correctness I can stand behind.

- **Add a Rust test under `xtask/` that parses the workflow YAML.** Rejected:
  the brief scopes this to "CI config only" and "out of scope: any change to what
  `cargo xtask ci` runs". A new Rust test changes what `cargo test` (hence
  `cargo xtask ci`) runs, and parsing YAML would pull in a `serde_yaml`-class
  dependency that triggers the ADR-0003 three-test dependency audit (NEEDS-HUMAN,
  INTEGRATION §4). Out of scope and disproportionate.

## Test — why a bundle-local structural test, not a gate test

`cargo xtask ci` provably never reads `.github/workflows/` (fmt/clippy/build/
test/deny/conformance on Rust only — brief lines 22–24), and the repo has no
actionlint, so the deliverable has no automated gate surface. Per the global
"a change ships with the means to verify it (or a stated why)" rule, I encoded
the brief's deterministic Check inspection as an executable test:
`test_ci_gate.py` parses `ci.yml`/`dco.yml` and asserts the gate job's wiring
(`needs: [rust]`, `if: always()`, success-or-skipped logic), the relocated
docs-only skip (`changes` job + `rust` `if:`), the removed PR paths filter, and
dco gating-readiness.

- Import-light: stdlib + PyYAML, already a CI dependency (`docs-check.yml`
  installs PyYAML; the repo's own docs tooling imports it). No GUI/IO-heavy
  module, so a headless runner is safe.
- **Red → green proven.** Against `HEAD:.github/workflows/*` it FAILS (5
  assertions: PR paths filter present, no `gate`/`changes` job, `rust` not gated
  on `changes`). Against the patched tree it PASSES. The dco assertions are green
  on both sides — dco was already structurally gating-ready; the substantive
  red→green is the ci `gate` job.

It is **not** run by `cargo xtask ci` (it lives in the PDCA bundle, not Wyrd's
tree — consistent with "Surfaces: data, CI config only"). It is a tiny, no-hang,
no-display script; I ran it directly. I did **not** run the full `cargo xtask ci`
as a regression check for this change because (a) the diff touches only YAML
under `.github/workflows/`, which that gate provably does not read, so no Rust
regression is reachable, and (b) the shared `../wyrd` working tree carries
unrelated uncommitted edits (`xtask/src/main.rs`, `integration-nightly.yml`) from
another bundle that would contaminate the result. The C4-ci gate re-runs at Check
on a clean tree.

## Behavioural demonstration — NEEDS-HUMAN

The gate reporting correctly on a real docs-only vs. code PR is observable only by
running GitHub Actions (a live-PR or fork-CI run) and is supplementary per the
brief — not the gating criterion.

## Commit-readiness

No pre-commit / editorconfig / yamllint config in the target repo; `rustfmt`
does not touch YAML. Verified both files have no trailing whitespace and a final
newline. `patch.diff` is scoped to exactly the two workflow files and applies
cleanly to HEAD (verified `git apply --check`).
