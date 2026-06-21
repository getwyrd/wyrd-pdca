#!/usr/bin/env python3
"""Deterministic inspection for issue #153 — contributor docs + GitHub templates.

The Success criterion (brief.md) is verified here, not by a proxy: the files
exist AND accurately describe the rules CI actually enforces, cross-checked
against `.github/workflows/require-issue.yml` and `.github/workflows/dco.yml`
on the target repo.

Import-light by design: stdlib only (no GUI / network / heavy deps), so a
headless runner cannot crash on load. Run directly:

    python3 results/issue_153/test_contributing_docs.py

or under unittest discovery. The target checkout is resolved from $WYRD_REPO,
else the sibling `../wyrd` convention (docs/INTEGRATION.md §2).
"""

import os
import pathlib
import unittest


def _wyrd_repo() -> pathlib.Path:
    env = os.environ.get("WYRD_REPO")
    if env:
        return pathlib.Path(env).resolve()
    # This file: <pdca>/results/issue_153/test_contributing_docs.py
    # pdca root is parents[2]; sibling Wyrd checkout is ../wyrd from it.
    pdca_root = pathlib.Path(__file__).resolve().parents[2]
    return (pdca_root.parent / "wyrd").resolve()


REPO = _wyrd_repo()


def _read(rel: str) -> str:
    return (REPO / rel).read_text(encoding="utf-8")


class ContributingDocsTest(unittest.TestCase):
    def setUp(self) -> None:
        if not (REPO / ".github" / "workflows").is_dir():
            self.skipTest(f"Wyrd checkout not found at {REPO}")

    # ---- CONTRIBUTING.md -------------------------------------------------

    def test_contributing_exists(self) -> None:
        self.assertTrue(
            (REPO / "CONTRIBUTING.md").is_file(),
            "CONTRIBUTING.md must exist at the repo root",
        )

    def test_contributing_documents_dco_as_enforced(self) -> None:
        """The DCO sign-off described must match what dco.yml actually checks."""
        dco = _read(".github/workflows/dco.yml")
        # dco.yml greps every commit for a 'Signed-off-by' trailer.
        self.assertIn("Signed-off-by", dco, "precondition: dco.yml checks Signed-off-by")
        contributing = _read("CONTRIBUTING.md")
        self.assertIn("git commit -s", contributing, "must show the `git commit -s` flag")
        self.assertIn("Signed-off-by", contributing, "must name the Signed-off-by trailer dco.yml requires")
        self.assertIn("dco.yml", contributing, "must cite the enforcing workflow")
        self.assertRegex(contributing, r"ADR-0003", "must cite ADR-0003 for the DCO decision")

    def test_contributing_documents_require_issue_as_enforced(self) -> None:
        """The 'link an issue' rule must match require-issue.yml."""
        req = _read(".github/workflows/require-issue.yml")
        self.assertIn("must reference an issue", req, "precondition: require-issue.yml enforces a linked issue")
        contributing = _read("CONTRIBUTING.md")
        self.assertIn("require-issue.yml", contributing, "must cite the enforcing workflow")
        # require-issue.yml accepts a `#N` reference; CONTRIBUTING must show the closing form.
        self.assertIn("Closes #", contributing, "must show a `Closes #N` reference that satisfies require-issue")

    def test_contributing_documents_ci_gate(self) -> None:
        contributing = _read("CONTRIBUTING.md")
        self.assertIn("cargo xtask ci", contributing, "must tell contributors to run `cargo xtask ci` before pushing")

    def test_contributing_documents_tier2(self) -> None:
        contributing = _read("CONTRIBUTING.md")
        self.assertIn("cargo xtask integration", contributing, "must describe the optional Tier-2 command")
        self.assertIn("Docker", contributing, "must note the Tier-2 Docker requirement")

    # ---- .github/PULL_REQUEST_TEMPLATE.md -------------------------------

    def test_pr_template_exists(self) -> None:
        self.assertTrue(
            (REPO / ".github" / "PULL_REQUEST_TEMPLATE.md").is_file(),
            ".github/PULL_REQUEST_TEMPLATE.md must exist",
        )

    def test_pr_template_preseeds_issue_and_checklist(self) -> None:
        tpl = _read(".github/PULL_REQUEST_TEMPLATE.md")
        self.assertIn("Closes #", tpl, "must pre-seed a `Closes #N` line (require-issue)")
        low = tpl.lower()
        self.assertIn("signed off", low, "checklist must remind about DCO sign-off")
        self.assertIn("cargo xtask ci", tpl, "checklist must remind about the ci gate")

    # ---- .github/ISSUE_TEMPLATE/ ----------------------------------------

    def test_issue_templates_present(self) -> None:
        d = REPO / ".github" / "ISSUE_TEMPLATE"
        self.assertTrue(d.is_dir(), ".github/ISSUE_TEMPLATE/ must exist")
        self.assertTrue((d / "config.yml").is_file(), "ISSUE_TEMPLATE/config.yml must exist")
        names = {p.name for p in d.iterdir() if p.is_file()}
        has_bug = any("bug" in n for n in names)
        has_enh = any("enhancement" in n or "feature" in n for n in names)
        self.assertTrue(has_bug, f"a bug template must exist; found {sorted(names)}")
        self.assertTrue(has_enh, f"an enhancement template must exist; found {sorted(names)}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
