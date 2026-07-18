"""The single-id `pdca flow <id>` path must treat a RESOLVED tracker as SUCCESS.

`RESOLVED` is in `state.HALTED`, so `driver.run_issue` returns it immediately. The
single-id `cli._flow` path previously treated only COMPLETE as "already done" and returned
0 only for COMPLETE/AWAITING_SIGNOFF, so `pdca flow 262` on a resolved tracker would print
RESOLVED and exit 1 — a legitimate terminal state reported as failure (Codex on #150). This
pins the early-return-0 with no cycle driven (the multi-id sweep already skips RESOLVED).
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from pdca_harness import cli, flow, state
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
        issue_id_example="#1",
        builder=LeafConfig(mode="stub", family="claude"),
        reviewer=LeafConfig(mode="stub", family="codex"),
        planner=LeafConfig(mode="stub", family="claude", interactive=True),
        signoff=LeafConfig(mode="stub", family="claude", interactive=True),
        publisher=LeafConfig(mode="stub", family="claude", interactive=True),
        act=LeafConfig(mode="stub", family="claude", interactive=True),
    )


def _args(*ids: str) -> SimpleNamespace:
    return SimpleNamespace(
        issue_ids=list(ids), from_csv=None, from_briefs=None, no_publish=True,
        no_act=True, by="", lanes=1, max_passes=None, auto_iterate=False,
    )


class FlowResolvedSingleIdTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.cfg = _cfg(self.root)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_resolved_tracker_is_success_and_drives_nothing(self) -> None:
        d = self.cfg.bundle("262")
        d.mkdir(parents=True)
        (d / "notes.json").write_text(
            json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
        )
        self.assertEqual(state.state(d), state.RESOLVED)
        # `flow.flow` must NOT be called — a resolved tracker is terminal, nothing to run.
        with mock.patch.object(flow, "flow", side_effect=AssertionError("must not drive")):
            rc = cli._flow(self.cfg, _args("262"))
        self.assertEqual(rc, 0)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
