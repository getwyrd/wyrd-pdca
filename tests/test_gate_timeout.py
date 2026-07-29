"""Offline slice for the gate wall-clock bound (issue #187, stdlib unittest).

A gate had no timeout at all: `run_with_heartbeat` took no bound and `_run_one` passed
none, so one hung command held the whole Check beat open while the heartbeat printed
`… still working`. issue_635's ADVISORY C5-mutants row did exactly that for 19h16m.

Proves: the bound is resolved per check (own value, then the [gates] default, with an
explicit 0 as a deliberate opt-out), a gate that outruns it is KILLED — including the
grandchildren a `shell=True` gate spawns — and the row is recorded `unverifiable`
rather than `fail`, because a gate that never answered has no verdict to hold against
the fix. Real commands, no Claude / Docker. Run from the project root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from pdca_harness import gates, progress
from pdca_harness.config import Config, LeafConfig

_GATE = {"id": "C4", "tier": "C4", "label": "verify", "scope": "bundle", "gating": True}


def _stub_config(root: Path) -> Config:
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
    )


class TimeoutResolution(unittest.TestCase):
    """`_timeout_for` — which bound applies to one check."""

    def test_check_value_wins_over_the_default(self) -> None:
        self.assertEqual(gates._timeout_for({"timeout_secs": 30}, 7200), 30)

    def test_default_applies_when_the_check_is_silent(self) -> None:
        self.assertEqual(gates._timeout_for({}, 7200), 7200)

    def test_explicit_zero_on_the_check_opts_OUT_of_the_default(self) -> None:
        # Not a fall-through to the default: a project-wide bound has to be escapable
        # by the one gate that genuinely may run as long as it likes.
        self.assertIsNone(gates._timeout_for({"timeout_secs": 0}, 7200))

    def test_no_default_and_no_value_is_unbounded(self) -> None:
        self.assertIsNone(gates._timeout_for({}, 0))  # pre-#187 behaviour, unchanged

    def test_malformed_bound_falls_back_rather_than_crashing(self) -> None:
        # A typo in one gate's bound must not take down the whole gate run.
        self.assertEqual(gates._timeout_for({"timeout_secs": "soon"}, 7200), 7200)

    def test_a_negative_bound_falls_back_instead_of_unbounding_the_gate(self) -> None:
        # Only an explicit 0 is the documented opt-out; a negative is far likelier a typo
        # than an intent to run forever, and must not silently escape the project default.
        self.assertEqual(gates._timeout_for({"timeout_secs": -1}, 7200), 7200)


class HeartbeatBound(unittest.TestCase):
    """`run_with_heartbeat` — the bound is enforced, and the kill reaches the tree."""

    def test_bounded_hang_raises_instead_of_running_forever(self) -> None:
        start = time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            progress.run_with_heartbeat("sleep 60", shell=True, capture=True,
                                        interval=1, timeout=2)
        self.assertLess(time.monotonic() - start, 30, "the bound did not fire")

    def test_an_unbounded_command_is_untouched(self) -> None:
        rc, output, _ = progress.run_with_heartbeat("echo ok", shell=True, capture=True,
                                                    interval=1)
        self.assertEqual(rc, 0)
        self.assertIn("ok", output)

    def test_a_command_finishing_inside_its_bound_is_untouched(self) -> None:
        rc, output, _ = progress.run_with_heartbeat("echo quick", shell=True, capture=True,
                                                    interval=1, timeout=30)
        self.assertEqual(rc, 0)
        self.assertIn("quick", output)

    def test_the_bound_is_honoured_regardless_of_the_heartbeat_interval(self) -> None:
        """A hard bound must be hard: waiting a full tick before even checking the clock
        would let a 2s bound overrun to 60s under the default 15s interval."""
        start = time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            progress.run_with_heartbeat("sleep 120", shell=True, capture=True,
                                        interval=60, timeout=2)
        self.assertLess(time.monotonic() - start, 30,
                        "the bound waited for the heartbeat tick instead of the deadline")

    def test_a_sigterm_ignoring_grandchild_still_gets_killed(self) -> None:
        """The shell exits on SIGTERM while the real work — which may trap it — keeps
        running. Waiting on the leader answers 'did the shell die', not 'is it over', so
        SIGKILL has to go out regardless."""
        marker = Path(tempfile.mkdtemp()) / "stubborn.pid"
        try:
            cmd = f"(trap '' TERM; sleep 300) & echo $! > {marker}; wait"
            start = time.monotonic()
            with self.assertRaises(subprocess.TimeoutExpired):
                progress.run_with_heartbeat(cmd, shell=True, capture=True,
                                            interval=1, timeout=2)
            # Assert the CALL returns promptly, not merely that the process is eventually
            # gone: with the group id re-read after SIGTERM reaped the leader, the SIGKILL
            # missed and this only "passed" when `sleep 300` finally expired on its own.
            # Wall clock must not be able to stand in for the kill working.
            self.assertLess(time.monotonic() - start, 45,
                            "the timeout path waited out the child instead of killing it")
            stubborn = int(marker.read_text().strip())
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                try:
                    os.kill(stubborn, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.2)
            else:
                os.kill(stubborn, signal.SIGKILL)
                self.fail(f"SIGTERM-ignoring grandchild {stubborn} survived the timeout")
        finally:
            shutil.rmtree(marker.parent, ignore_errors=True)

    def test_the_kill_reaches_the_grandchild_a_shell_gate_spawned(self) -> None:
        """The regression that matters: under `shell=True` the direct child is only a
        shell, so killing it alone orphans the real work — a `cargo` tree that keeps
        burning CPU long after the gate 'timed out'. The whole process group must die."""
        marker = Path(tempfile.mkdtemp()) / "grandchild.pid"
        try:
            # The shell backgrounds a long sleep, records its pid, then blocks itself.
            cmd = f"sleep 300 & echo $! > {marker}; wait"
            with self.assertRaises(subprocess.TimeoutExpired):
                progress.run_with_heartbeat(cmd, shell=True, capture=True,
                                            interval=1, timeout=2)
            grandchild = int(marker.read_text().strip())
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    os.kill(grandchild, 0)
                except ProcessLookupError:
                    break  # reaped — the group kill reached it
                time.sleep(0.2)
            else:
                os.kill(grandchild, signal.SIGKILL)  # don't leak it out of the test
                self.fail(f"grandchild {grandchild} survived the gate timeout")
        finally:
            shutil.rmtree(marker.parent, ignore_errors=True)


class TimedOutGateRow(unittest.TestCase):
    """The recorded row: a timed-out gate is `unverifiable`, never `fail`."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _stub_config(self.tmp)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, gate: dict, default_secs: int) -> dict:
        d = self.cfg.bundle("TO")
        d.mkdir(parents=True, exist_ok=True)
        (d / "brief.md").write_text("- **Slug:** to\n", encoding="utf-8")
        (d / "patch.diff").write_text("--- a\n+++ b\n", encoding="utf-8")
        self.cfg.gates_checks = [gate]
        self.cfg.gates_default_timeout_secs = default_secs
        result = gates.run_gates(d, self.cfg)
        return next(r for r in result["rows"] if r["element"] == "C4"), result

    def test_a_hung_gate_is_unverifiable_and_does_not_fail_overall(self) -> None:
        row, result = self._run({**_GATE, "cmd": "sleep 60", "timeout_secs": 2}, 0)
        self.assertEqual(row["result"], "unverifiable")
        self.assertIn("timeout", row["path_line"])
        # A gating row that never answered must not be read as a failing verdict — that
        # would blame the fix for the gate's own hang and block a possibly-good patch.
        self.assertEqual(result["overall"], "pass")

    def test_the_project_default_bounds_a_check_that_sets_none(self) -> None:
        row, _ = self._run({**_GATE, "cmd": "sleep 60"}, 2)
        self.assertEqual(row["result"], "unverifiable")

    def test_a_gate_that_really_fails_still_fails(self) -> None:
        row, result = self._run({**_GATE, "cmd": "false"}, 30)
        self.assertEqual(row["result"], "fail")  # a bound changes nothing for a real verdict
        self.assertEqual(result["overall"], "fail")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
