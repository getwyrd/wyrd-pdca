"""The networked reviewer keeps the STOP discipline (#135, PR #136 review).

With ``[leaves.sandbox] network_access = true`` an authenticated host ``gh`` is reachable
from inside the reviewer/advisory leaves, which previously ran with an UNGUARDED PATH —
so ``gh pr ready`` / ``merge`` / ``review --approve`` would have reached GitHub, bypassing
the mechanical STOP guard the builder and publisher already get.

The shim is UNCONDITIONAL in the sandboxed runners — ``native_guard`` cannot be trusted
from a temp cwd: the claude PreToolUse hook rides on the builder/publisher agent
frontmatter, and the reviewer/adversary/code-review agents declare none, so a sandboxed
claude Check leaf is exactly as unguarded as a codex one (PR #136 review, 2nd pass). The
builder/publisher paths keep their native_guard branch — they run in the project cwd,
where the claude hook genuinely applies (see test_publish_slice).

Offline: shim_env is mocked; no model CLIs. Run from root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from pdca_harness import leaves
from pdca_harness.config import Config, LeafConfig


def _cfg(root: Path) -> Config:
    return Config(
        root=root,
        bundle_root=root / "results",
        process_dir=root / "process",
        templates_dir=root / "templates",
        default_branch="main",
        tracker_system="github",
        tracker_url="",
        issue_id_example="1",
        builder=LeafConfig(mode="stub"),
        reviewer=LeafConfig(mode="stub"),
    )


class ReviewerGhShim(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.cfg = _cfg(self.tmp)
        self.d = self.cfg.bundle("X")
        self.d.mkdir(parents=True)

    def _env_passed(self, family: str):
        self.cfg.reviewer = LeafConfig(mode="command", family=family, agent="reviewer")
        captured: dict = {}

        def invoke(leaf, cwd, prompt, **kw):
            captured["env"] = kw.get("env")
            return None  # leaf "succeeded"; no artifact copy-back matters here

        with mock.patch.object(leaves, "_invoke_leaf_resilient", side_effect=invoke), \
             mock.patch.object(leaves.guard, "shim_env",
                               side_effect=lambda cfg, env: {**(env or {}), "PATH": "SHIMMED"}):
            leaves._run_review_sandboxed(self.d, self.cfg)
        return captured["env"]

    def test_codex_reviewer_gets_the_gh_shim(self) -> None:
        env = self._env_passed("codex")
        self.assertIsNotNone(env)
        self.assertEqual(env.get("PATH"), "SHIMMED")

    def test_claude_reviewer_is_shimmed_too(self) -> None:
        # native_guard is builder/publisher frontmatter; the sandboxed reviewer has no
        # hook of its own, so the vendor-neutral PATH shim must apply here as well.
        env = self._env_passed("claude")
        self.assertEqual((env or {}).get("PATH"), "SHIMMED")

    def test_advisory_leaf_gets_the_gh_shim_too(self) -> None:
        leaf = LeafConfig(mode="command", family="codex", agent="adversary")
        captured: dict = {}

        def invoke(l, cwd, prompt, **kw):
            captured["env"] = kw.get("env")
            return None

        with mock.patch.object(leaves, "_invoke_leaf_resilient", side_effect=invoke), \
             mock.patch.object(leaves.guard, "shim_env",
                               side_effect=lambda cfg, env: {**(env or {}), "PATH": "SHIMMED"}):
            leaves._run_advisory_sandboxed(self.d, self.cfg, leaf,
                                           {"id": "adversary", "role": "adversary"},
                                           "adversary")
        self.assertEqual((captured["env"] or {}).get("PATH"), "SHIMMED")


if __name__ == "__main__":
    unittest.main()
