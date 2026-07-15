"""[driver].scratch_dir — throwaway heavy leaf work is redirected off tmpfs /tmp (#134).

Three promises:
  * config: `[driver].scratch_dir` parses; `PDCA_SCRATCH` overrides for one run;
    unset ⇒ "" (the knob fails open to the status quo).
  * CLI entry: `_export_scratch` creates the dir and exports BOTH `PDCA_SCRATCH` and
    `TMPDIR` so every subprocess inherits the redirect; unset ⇒ no-op; an uncreatable
    dir warns and falls back rather than aborting the run.
  * agent definitions: the four heavy-checkout leaves (builder, reviewer, adversary,
    code-review) carry the scratch discipline — `$TMPDIR` only helps tools that respect
    it; a model writing a literal `/tmp/wyrd-adv` needs the standing instruction.

Offline: pure config/env, no leaves. Run from root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

from pdca_harness import cli
from pdca_harness.config import Config

ROOT = Path(__file__).resolve().parents[1]


class ScratchDirConfig(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        for var in ("PDCA_SCRATCH", "PDCA_LEAVES_MODE"):
            saved = os.environ.pop(var, None)
            if saved is not None:
                self.addCleanup(os.environ.__setitem__, var, saved)

    def _load(self, extra: str = "") -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n' + extra,
            encoding="utf-8",
        )
        return Config.load(self.tmp)

    def test_unset_is_empty_string(self) -> None:
        self.assertEqual(self._load().scratch_dir, "")

    def test_toml_value_parses(self) -> None:
        self.assertEqual(
            self._load('[driver]\nscratch_dir = "/var/tmp/pdca"\n').scratch_dir,
            "/var/tmp/pdca")

    def test_env_overrides_the_toml_for_one_run(self) -> None:
        os.environ["PDCA_SCRATCH"] = "/elsewhere"
        self.addCleanup(os.environ.pop, "PDCA_SCRATCH", None)
        self.assertEqual(
            self._load('[driver]\nscratch_dir = "/var/tmp/pdca"\n').scratch_dir,
            "/elsewhere")


class ScratchExport(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)

    def _cfg(self, scratch: str) -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n',
            encoding="utf-8",
        )
        cfg = Config.load(self.tmp)
        cfg.scratch_dir = scratch
        return cfg

    def test_unset_is_a_no_op(self) -> None:
        env: dict = {}
        self.assertIsNone(cli._export_scratch(self._cfg(""), env))
        self.assertEqual(env, {})

    def test_creates_the_dir_and_exports_both_variables(self) -> None:
        target = self.tmp / "scratch" / "pdca"     # two levels: mkdir must use parents
        env: dict = {}
        got = cli._export_scratch(self._cfg(str(target)), env)
        self.assertEqual(got, target)
        self.assertTrue(target.is_dir())
        self.assertEqual(env["PDCA_SCRATCH"], str(target))
        self.assertEqual(env["TMPDIR"], str(target))

    def test_existing_unwritable_dir_warns_and_falls_back(self) -> None:
        # mkdir(exist_ok=True) succeeds on a pre-existing read-only dir; only a real
        # write probe catches it. Root ignores mode bits, so skip there.
        if os.geteuid() == 0:
            self.skipTest("mode bits don't bind root")
        ro = self.tmp / "ro"
        ro.mkdir()
        ro.chmod(0o555)
        self.addCleanup(ro.chmod, 0o755)
        env: dict = {}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(ro)), env)
        self.assertIsNone(got)
        self.assertEqual(env, {})
        self.assertIn("not usable", err.getvalue())

    def test_uncreatable_dir_warns_and_falls_back(self) -> None:
        blocker = self.tmp / "file"
        blocker.write_text("not a dir", encoding="utf-8")
        env: dict = {}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(blocker / "sub")), env)
        self.assertIsNone(got)                      # fail open: the run proceeds on /tmp
        self.assertEqual(env, {})
        self.assertIn("not usable", err.getvalue())


class ScratchDiscipline(unittest.TestCase):
    def test_the_four_heavy_leaves_carry_the_instruction(self) -> None:
        # $TMPDIR only redirects tools that respect it; a model inventing a literal
        # /tmp/... path needs the standing instruction in its role prompt. The wrappers
        # are covered transitively by test_role_prompts (wrapper == canonical body).
        for leaf in ("builder", "reviewer", "adversary", "code-review"):
            with self.subTest(leaf=leaf):
                body = (ROOT / "agents" / f"{leaf}.md").read_text(encoding="utf-8")
                self.assertIn("$PDCA_SCRATCH", body)
                self.assertIn(f"pdca-{leaf}-<issue>-*", body)


if __name__ == "__main__":
    unittest.main()
