"""The auto-iterate opt-in fails closed, and a CLI pass override re-clamps its budget (#132).

Two hardening regressions from the PR #127 review:
  * ``auto_iterate`` is an autonomy opt-in (the driver records iterate-do and rebuilds
    with no human prompt), so a false-looking value — the TOML string ``"false"``, the
    env value ``False`` — must parse as OFF, never silently enable it.
  * ``Config.load()`` clamps ``max_auto_iters`` strictly below the pass budget; a CLI
    ``--max-passes`` that lowers the budget after load must re-clamp, or the final pass
    can be spent on an auto-iterate and strand the bundle at ITERATE_DO.

Offline: pure config parsing, no leaves. Run from root:
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

from pdca_harness.config import Config


class AutoIterateOptIn(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        for var in ("PDCA_LEAVES_MODE", "PDCA_AUTO_ITERATE", "PDCA_MAX_PASSES"):
            saved = os.environ.pop(var, None)
            if saved is not None:
                self.addCleanup(os.environ.__setitem__, var, saved)

    def _load(self, extra: str = "", env: dict[str, str] | None = None) -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n' + extra,
            encoding="utf-8",
        )
        for k, v in (env or {}).items():
            os.environ[k] = v
            self.addCleanup(os.environ.pop, k, None)
        with redirect_stderr(io.StringIO()) as err:
            cfg = Config.load(self.tmp)
        self.stderr = err.getvalue()
        return cfg

    # --- the opt-in fails closed ------------------------------------------------

    def test_real_toml_booleans_parse(self) -> None:
        self.assertTrue(self._load('[driver]\nauto_iterate = true\n').auto_iterate)
        self.assertFalse(self._load('[driver]\nauto_iterate = false\n').auto_iterate)

    def test_string_false_is_off(self) -> None:
        # bool('false') is True — the opt-in must not be enabled by a quoted "false".
        self.assertFalse(self._load('[driver]\nauto_iterate = "false"\n').auto_iterate)

    def test_string_true_is_on(self) -> None:
        self.assertTrue(self._load('[driver]\nauto_iterate = "true"\n').auto_iterate)

    def test_garbage_fails_closed_and_warns(self) -> None:
        cfg = self._load('[driver]\nauto_iterate = "treu"\n')
        self.assertFalse(cfg.auto_iterate)
        self.assertIn("fails closed", self.stderr)

    def test_env_false_is_off_case_insensitively(self) -> None:
        # The old check was case-sensitive: PDCA_AUTO_ITERATE=False counted as ON.
        for v in ("False", "false", "0", "no", "OFF"):
            with self.subTest(value=v):
                cfg = self._load('[driver]\nauto_iterate = true\n',
                                 env={"PDCA_AUTO_ITERATE": v})
                self.assertFalse(cfg.auto_iterate)

    def test_env_true_opts_in(self) -> None:
        cfg = self._load(env={"PDCA_AUTO_ITERATE": "1"})
        self.assertTrue(cfg.auto_iterate)

    def test_env_empty_string_disables_a_toml_enabled_opt_in(self) -> None:
        # A PRESENT env var always overrides: `PDCA_AUTO_ITERATE=` must turn a
        # toml-enabled opt-in OFF for the run, not be skipped as falsy.
        cfg = self._load('[driver]\nauto_iterate = true\n',
                         env={"PDCA_AUTO_ITERATE": ""})
        self.assertFalse(cfg.auto_iterate)

    # --- a CLI --max-passes override re-clamps the auto budget -------------------

    def test_override_max_passes_reclamps_auto_budget(self) -> None:
        # Loaded with headroom: max_auto_iters = 3 clamps below max_passes = 20.
        cfg = self._load('[driver]\nauto_iterate = true\nmax_passes = 20\n')
        self.assertEqual(cfg.max_auto_iters, 3)
        # The CLI lowers the budget after load: the clamp must follow (2 passes → 1 auto),
        # or the final pass is spent on an auto-iterate and the bundle strands at ITERATE_DO.
        cfg.override_max_passes(2)
        self.assertEqual(cfg.max_passes, 2)
        self.assertEqual(cfg.max_auto_iters, 1)

    def test_override_max_passes_never_raises_the_budget(self) -> None:
        cfg = self._load('[driver]\nauto_iterate = true\nmax_auto_iters = 2\nmax_passes = 5\n')
        self.assertEqual(cfg.max_auto_iters, 2)
        cfg.override_max_passes(50)  # raising the pass budget keeps the configured cap
        self.assertEqual(cfg.max_auto_iters, 2)

    def test_one_pass_budget_means_zero_auto_iterations(self) -> None:
        # With a single allowed pass there is no next pass to rebuild in — an auto budget
        # of 1 would spend the only pass on an iterate-do and strand the bundle at
        # ITERATE_DO. The clamp must reach 0 so flow's spent >= budget check declines.
        cfg = self._load('[driver]\nauto_iterate = true\nmax_passes = 20\n')
        cfg.override_max_passes(1)
        self.assertEqual(cfg.max_auto_iters, 0)
        # And the same via config alone, without a CLI override.
        cfg = self._load('[driver]\nauto_iterate = true\nmax_passes = 1\n')
        self.assertEqual(cfg.max_auto_iters, 0)


if __name__ == "__main__":
    unittest.main()
