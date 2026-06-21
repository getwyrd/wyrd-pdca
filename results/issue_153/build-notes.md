# Build notes — issue 153 / docs-contributing-and-pr-issue-templates

## What the brief asks for (Success criterion)

Net-new contributor onboarding files for `getwyrd/wyrd` @ `main`, accurately
describing the rules CI already enforces:

1. `CONTRIBUTING.md` — DCO sign-off (`git commit -s`), the require-an-issue rule,
   running `cargo xtask ci` before pushing, and the optional Tier-2 tier
   (`cargo xtask integration`, Docker).
2. `.github/PULL_REQUEST_TEMPLATE.md` — pre-seeds `Closes #N` + a DCO/ci checklist.
3. `.github/ISSUE_TEMPLATE/` — bug + enhancement templates and a `config.yml`.

Verified by deterministic inspection (the criterion is not a proxy: files exist
**and** describe the enforced rules, cross-checked against the workflows).

## Files added (all net-new; confirmed absent on `main` — brief prior-art check)

- `CONTRIBUTING.md`
- `.github/PULL_REQUEST_TEMPLATE.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/enhancement.yml`
- `.github/ISSUE_TEMPLATE/config.yml`

## Citations on `main` for the rules documented

Every rule the docs describe is verified against the authoritative workflow /
ADR on the target branch, not from memory:

- **DCO**: `.github/workflows/dco.yml:34` greps each commit for
  `^Signed-off-by: .+ <.+@.+>`; `:40` names `git commit -s` and points to
  ADR-0003. CONTRIBUTING documents exactly `git commit -s` + the `Signed-off-by`
  trailer and cites `docs/design/adr/0003-apache-2-license-and-dco.md` §1
  (the DCO-not-CLA decision).
- **Require-issue**: `.github/workflows/require-issue.yml:35` accepts a `#N` /
  `issues/N` reference in the PR title or body; `:37`/`:50` use `Closes #12` /
  `Closes #N` as the example. CONTRIBUTING + the PR template use `Closes #N`,
  which matches the `(#|issues/)[0-9]+` grep.
- **CI gate**: `.github/workflows/ci.yml:63` runs `cargo xtask ci`; the inline
  comment (`:3`) enumerates fmt/clippy/build/test/deny/conformance. CONTRIBUTING
  lists the same sub-steps and tells contributors to run it before pushing.
- **Tier-2**: `.github/workflows/integration-nightly.yml:46-47` run
  `cargo xtask integration`; `:43-44` require a Docker runtime; the header
  (`:6`) marks it nightly / not a required PR check. CONTRIBUTING describes it as
  optional, Docker-needing, and non-gating.

## Why this scope and no more

- **README left untouched.** The brief lists README as *out of scope* (paired
  #152). The existing "Contributing & governance" section (README.md:90–94 on
  `main`) already links CoC + Governance; adding a CONTRIBUTING pointer there
  would overlap #152, so it is deliberately deferred.
- **Gates/branch-protection untouched** (#151 / out of scope): this change adds
  documentation only; it does not modify `require-issue.yml` / `dco.yml`.

## Test — deterministic inspection, red→green

`results/issue_153/test_contributing_docs.py` (stdlib `unittest` only — no GUI /
network / heavy imports, so a headless runner can load it). It asserts the files
exist **and** that their content matches the enforced rules, cross-checking
against `dco.yml` / `require-issue.yml` in the target checkout (resolved from
`$WYRD_REPO`, else the sibling `../wyrd` convention, INTEGRATION §2).

- **Pre-fix** (new files moved aside): `Ran 8 tests … FAILED (failures=3,
  errors=5)`.
- **Post-fix** (files present): `Ran 8 tests … OK`.

Run: `python3 results/issue_153/test_contributing_docs.py`. The cross-check tests
also assert their precondition (e.g. that `dco.yml` does check `Signed-off-by`),
so the test stays honest if a workflow later changes.

### Why a standalone inspection test rather than a host-runner test

Wyrd's gate runner is `cargo xtask ci` (`./engine/xtask.sh ci`), which by design
**does not cover** these files: it is fmt/clippy/build/test/deny/conformance over
the Rust workspace, and the only markdown linter (`lint_docs.py` via `docs-check`)
scans `docs/` only — these files live at the repo root / `.github/`. The brief
states this explicitly and confirms `cargo xtask ci` and `docs-check` stay green
(the change touches no `docs/` file and no Rust). So the criterion's "deterministic
inspection" has no existing host harness; the inspection script *is* that gate.
It is pure file I/O — no loop over an external resource — so it cannot hang.

## Alternatives ruled out

- **Markdown issue templates (`.md`) instead of YAML issue forms.** Forms give
  structured, validated fields (better for newcomers, the brief's stated
  audience) and are the current GitHub standard. `config.yml` must be YAML
  regardless. All three YAML files were verified to parse with `yaml.safe_load`.
- **Editing the README contributing section to link CONTRIBUTING.md.** Rejected:
  out of scope (#152 owns README). Cost of including it would be a second logical
  change in one bundle — exactly what the one-change-per-PR rule forbids.

## Commit-readiness

No formatter/commit hook in the target repo covers these paths: no
`.pre-commit-config`, `.prettierrc`, `.markdownlint`, or `.editorconfig`; no
active `.git/hooks`. rustfmt/clippy apply to Rust only; `lint_docs.py` to `docs/`
only. All files end with a trailing newline. `patch.diff` was verified to
`git apply --check` cleanly against a fresh `main` worktree.
