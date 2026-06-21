# Brief — issue 153 / docs-contributing-and-pr-issue-templates

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** docs-contributing-and-pr-issue-templates
- **Defect / goal:** the contribution workflow is enforced by CI but undocumented for
  newcomers: every PR must link a real issue (`.github/workflows/require-issue.yml`) and
  every commit must carry a DCO `Signed-off-by` (`.github/workflows/dco.yml`, ADR-0003 §1),
  learned only by tripping the check. There is no `CONTRIBUTING.md` (the README points to
  Code of Conduct + Governance only), no `.github/PULL_REQUEST_TEMPLATE.md` (to pre-seed
  `Closes #N` + a DCO reminder), and no `.github/ISSUE_TEMPLATE/`.
- **Success criterion:** `CONTRIBUTING.md` exists documenting DCO sign-off (`git commit
  -s`), the require-an-issue rule, running `cargo xtask ci` before pushing, and the optional
  Tier-2 tier (`cargo xtask integration`, Docker); `.github/PULL_REQUEST_TEMPLATE.md`
  pre-seeds a `Closes #N` line + a DCO/ci checklist; `.github/ISSUE_TEMPLATE/` provides bug
  + enhancement templates and a `config.yml`. Verified at Check by deterministic inspection:
  the files exist and accurately describe the enforced rules (cross-checked against
  `require-issue.yml` / `dco.yml`). (These files live outside `docs/`, so the repo's only
  markdown linter — `lint_docs.py` via `docs-check`, which scans `docs/` — does not cover
  them; `docs-check` and `cargo xtask ci` stay green because this touches no `docs/` file and
  no Rust, which is supplementary, not the criterion.)
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (docs / process only)
- **Scope:** `CONTRIBUTING.md` + `.github/PULL_REQUEST_TEMPLATE.md` +
  `.github/ISSUE_TEMPLATE/`. / **out of scope:** the README dev/testing section (paired
  #152); changing the require-issue / dco gates themselves; branch protection (#151).
- **Citations expected:** Do cites `.github/workflows/{require-issue,dco}.yml` and ADR-0003
  §1 on `main` for the rules it documents.
- **Prior-art check:** `.github/` contains only `workflows/` (no `PULL_REQUEST_TEMPLATE.md`,
  no `ISSUE_TEMPLATE/`); no `CONTRIBUTING.md` at repo root. README §"Contributing &
  governance" links CoC + Governance, not a contributor guide. Net-new.
- **Disposition hint:** likely-fix

## STOP discipline
Draft only until Check sign-off.
