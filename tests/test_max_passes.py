"""The per-wave iteration cap (`[driver].max_passes`) and its abandonment guard.

Regression for the bug where a bundle signed off `iterate-do` on the last allowed
pass was left at ITERATE_DO and the run silently published the accepted siblings
around it (no next iteration, no signal). The cap must now be configurable and, when
hit with a bundle still iterating, reported loudly — never silently dropped.

Offline: stub leaves + stub gates (no Claude, no TTY, no Docker). Run from root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from pdca_harness import cli, flow, leaves, state
from pdca_harness.config import Config, LeafConfig


def _stub_config(root: Path) -> Config:
    """All six leaves stubbed, gates empty (all-PASS stub rows); default max_passes."""
    return Config(
        root=root,
        bundle_root=root / "results",
        process_dir=root / "process",
        templates_dir=root / "templates",  # empty → planner stub uses its fallback brief
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
        act_cadence=1,
    )


def _load(tmp: Path, extra: str = "") -> Config:
    (tmp / "pdca.toml").write_text(
        '[project]\ndefault_branch = "main"\n'
        '[leaves.builder]\nmode = "stub"\n'
        '[leaves.reviewer]\nmode = "stub"\n' + extra,
        encoding="utf-8",
    )
    saved = os.environ.pop("PDCA_LEAVES_MODE", None)
    try:
        return Config.load(tmp)
    finally:
        if saved is not None:
            os.environ["PDCA_LEAVES_MODE"] = saved


class ConfigMaxPasses(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.addCleanup(lambda: os.environ.pop("PDCA_MAX_PASSES", None))

    def test_default_is_twenty(self) -> None:
        self.assertEqual(_load(self.tmp).max_passes, 20)

    def test_driver_table_overrides_default(self) -> None:
        self.assertEqual(_load(self.tmp, "[driver]\nmax_passes = 7\n").max_passes, 7)

    def test_env_overrides_table(self) -> None:
        os.environ["PDCA_MAX_PASSES"] = "3"
        self.assertEqual(_load(self.tmp, "[driver]\nmax_passes = 7\n").max_passes, 3)

    def test_floor_of_one(self) -> None:
        self.assertEqual(_load(self.tmp, "[driver]\nmax_passes = 0\n").max_passes, 1)


class CliMaxPasses(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.cfg = _stub_config(self.tmp)

    def test_flag_overrides_cfg_max_passes(self) -> None:
        # `pdca flow <id> --max-passes N` sets cfg.max_passes before driving (mirrors --lanes).
        seen = {}

        def capture(cfg, ids, **kw):
            seen["max_passes"] = cfg.max_passes
            return {}

        orig = flow.flow_ids
        flow.flow_ids = capture
        args = SimpleNamespace(issue_ids=["X", "Y"], from_csv=None, from_briefs=None,
                               no_publish=True, no_act=True, by="", lanes=None, max_passes=5)
        try:
            cli._flow(self.cfg, args)
        finally:
            flow.flow_ids = orig
        self.assertEqual(self.cfg.max_passes, 5)
        self.assertEqual(seen["max_passes"], 5)


class FlowMaxPassesCap(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.cfg = _stub_config(self.tmp)

    def _accept(self, d: Path) -> None:
        summ = d / "SUMMARY.md"
        summ.write_text(summ.read_text().replace("- [ ]", "- [x]"), encoding="utf-8")
        (d / leaves.SIGNOFF_DECISION).write_text("accept\n", encoding="utf-8")

    def test_cap_warns_leaves_iterating_and_publishes_sibling(self) -> None:
        # BATCH1 always iterates-do (never converges); BATCH2 accepts. With max_passes=2 the
        # wave hits the cap: BATCH1 is reported (not silently dropped) and NOT published,
        # while the accepted sibling BATCH2 still publishes. This is the reported bug.
        def signoff_batch(cfg: Config, bundles: list[Path]) -> None:
            for d in bundles:
                if d.name == "issue_BATCH1":
                    (d / leaves.SIGNOFF_DECISION).write_text("iterate-do\n", encoding="utf-8")
                else:
                    self._accept(d)

        self.cfg.max_passes = 2  # drive via cfg (no explicit max_passes arg)
        orig = leaves.run_signoff_batch
        leaves.run_signoff_batch = signoff_batch
        err = io.StringIO()
        try:
            with redirect_stderr(err), redirect_stdout(io.StringIO()):
                results = flow.flow_batch(self.cfg, today="2026-06-04")
        finally:
            leaves.run_signoff_batch = orig

        msg = err.getvalue()
        # Loud, actionable, and names ONLY the capped bundle with a resume hint.
        self.assertIn("hit the 2-pass cap with issue_BATCH1 still iterating", msg)
        self.assertIn("pdca flow BATCH1", msg)
        # The capped bundle is left iterating and unpublished; the sibling is done + published.
        self.assertEqual(results["BATCH1"], state.ITERATE_DO)
        self.assertEqual(results["BATCH2"], state.COMPLETE)
        self.assertFalse((self.cfg.bundle("BATCH1") / "commit-msg.txt").exists())
        self.assertTrue((self.cfg.bundle("BATCH2") / "commit-msg.txt").exists())

    def test_higher_cap_converges_without_warning(self) -> None:
        # BATCH1 iterates-do twice then accepts; with headroom (max_passes=6) both reach
        # COMPLETE and NO cap warning is printed — proving the cap is the gate and tunable.
        iters = {"BATCH1": 0}

        def signoff_batch(cfg: Config, bundles: list[Path]) -> None:
            for d in bundles:
                if d.name == "issue_BATCH1" and iters["BATCH1"] < 2:
                    iters["BATCH1"] += 1
                    (d / leaves.SIGNOFF_DECISION).write_text("iterate-do\n", encoding="utf-8")
                else:
                    self._accept(d)

        orig = leaves.run_signoff_batch
        leaves.run_signoff_batch = signoff_batch
        err = io.StringIO()
        try:
            with redirect_stderr(err), redirect_stdout(io.StringIO()):
                results = flow.flow_batch(self.cfg, today="2026-06-04", max_passes=6)
        finally:
            leaves.run_signoff_batch = orig

        self.assertNotIn("-pass cap", err.getvalue())
        self.assertEqual(iters["BATCH1"], 2)
        self.assertTrue(all(s == state.COMPLETE for s in results.values()))


if __name__ == "__main__":
    unittest.main()
