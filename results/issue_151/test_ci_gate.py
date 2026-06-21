#!/usr/bin/env python3
"""Structural verification for issue #151 — the CI gate-aggregation job + DCO
gating-readiness.

The deliverable is pure workflow YAML; `cargo xtask ci` never reads
`.github/workflows/` and the repo has no actionlint, so the gating Check
criterion is deterministic inspection of `ci.yml` / `dco.yml` (brief
"Success criterion"). This test encodes exactly that inspection so it can be
demonstrated red (before the patch) → green (after):

  * ci.yml's `pull_request` trigger no longer carries a `paths-ignore`, so the
    workflow — and therefore the gate job — always runs on every PR (a
    path-filtered workflow's checks stay "pending", #125).
  * an always-runs `gate` job exists, `needs:` the `rust` job, runs with
    `if: always()`, and passes on rust == success OR skipped (docs-only).
  * the docs-only path-skip is preserved at the JOB level: a `changes` job
    classifies the change and `rust` is gated on `needs.changes.outputs.code`.
  * dco.yml stays gating-ready: a `dco` job, no paths filter, always reports.

Import-light: stdlib + PyYAML (already a CI dependency, docs-check.yml). It
reads files only; it pulls in no GUI/IO-heavy module, so a headless runner is
fine. Run standalone: `python3 test_ci_gate.py` (exit 0 = green).
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import yaml


def _wyrd_repo() -> Path:
    env = os.environ.get("WYRD_REPO")
    if env:
        return Path(env)
    # Bundle lives at <root>/results/issue_151/; the Wyrd checkout is the sibling
    # <root>/../wyrd (INTEGRATION.md §2).
    return Path(__file__).resolve().parents[3] / "wyrd"


WF = _wyrd_repo() / ".github" / "workflows"


def _as_list(value) -> list:
    if value is None:
        return []
    if isinstance(value, list):
        return value
    return [value]


def _load(name: str) -> dict:
    # PyYAML maps the bare `on:` key to Python True; load and normalise.
    doc = yaml.safe_load((WF / name).read_text())
    if True in doc and "on" not in doc:
        doc["on"] = doc.pop(True)
    return doc


def _check(cond: bool, msg: str, failures: list) -> None:
    if not cond:
        failures.append(msg)


def verify_ci(failures: list) -> None:
    ci = _load("ci.yml")
    jobs = ci.get("jobs", {})
    on = ci.get("on", {})
    pr = on.get("pull_request") or {}

    # 1. Workflow always runs on PRs (no paths filter), so the gate can report.
    _check(
        "paths-ignore" not in pr and "paths" not in pr,
        "ci.yml: pull_request trigger must NOT carry a paths filter "
        "(a path-filtered workflow leaves docs-only PRs stuck 'pending')",
        failures,
    )

    # 2. The always-runs gate-aggregation job.
    gate = jobs.get("gate")
    _check(gate is not None, "ci.yml: missing the 'gate' aggregation job", failures)
    if gate is not None:
        _check(
            "rust" in _as_list(gate.get("needs")),
            "ci.yml: gate job must `needs:` the 'rust' job",
            failures,
        )
        gate_if = str(gate.get("if", ""))
        _check(
            "always()" in gate_if,
            "ci.yml: gate job must run with `if: always()` so it reports even "
            "when rust is skipped",
            failures,
        )
        run_text = " ".join(
            str(step.get("run", "")) for step in _as_list(gate.get("steps"))
        )
        _check(
            "success" in run_text and "skipped" in run_text,
            "ci.yml: gate step must pass on rust == success OR skipped "
            "(the skip-or-pass logic)",
            failures,
        )

    # 3. Docs-only path-skip preserved at the job level (#125 stands).
    changes = jobs.get("changes")
    _check(changes is not None, "ci.yml: missing the 'changes' classifier job", failures)
    rust = jobs.get("rust")
    _check(rust is not None, "ci.yml: missing the 'rust' job", failures)
    if rust is not None:
        _check(
            "changes" in _as_list(rust.get("needs")),
            "ci.yml: rust job must `needs:` the 'changes' job",
            failures,
        )
        _check(
            "needs.changes.outputs.code" in str(rust.get("if", "")),
            "ci.yml: rust job must be gated on needs.changes.outputs.code "
            "(docs-only skip relocated to the job level)",
            failures,
        )


def verify_dco(failures: list) -> None:
    dco = _load("dco.yml")
    on = dco.get("on", {})
    _check("dco" in dco.get("jobs", {}), "dco.yml: missing the 'dco' job", failures)
    _check(
        "pull_request" in on,
        "dco.yml: must trigger on pull_request",
        failures,
    )
    pr = on.get("pull_request") or {}
    _check(
        "paths-ignore" not in pr and "paths" not in pr,
        "dco.yml: must carry no paths filter so it always reports "
        "(gating-ready, DCO covers every commit — ADR-0003 §1)",
        failures,
    )


def main() -> int:
    failures: list = []
    verify_ci(failures)
    verify_dco(failures)
    if failures:
        print("FAIL — CI gate / DCO gating-readiness not satisfied:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("PASS — ci.yml gate job wired and dco.yml gating-ready")
    return 0


if __name__ == "__main__":
    sys.exit(main())
