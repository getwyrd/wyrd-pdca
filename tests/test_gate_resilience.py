"""Offline slice for gate resilience (eduralph/pdca-harness#370/#371/#372, stdlib unittest).

Three defects, one incident (issue_648, 2026-07-31): a gating C4-ci recorded `cargo test`
exit 101 in a run that was green in all six prior rounds and green on every re-run — and
(a) the full output was discarded, so the failing test could not even be named; (b) the
one transient sample stood as the verdict and parked the bundle; (c) the likely cause
class — a straggler from a finished child still holding the substrate — is never swept
(a leaked test process from a prior cycle was found burning a core for 21 hours).

Proves: every bundle-scoped gate run persists its full output to gate-logs/<rule_id>.log
and the row records where and how long; a failed GATING row is confirmed exactly once,
recording BOTH verdicts (fail→pass passes flagged flaky and raises a §6 HUMAN item;
fail→fail stays red; advisory rows and opted-out configs are single-sample); and a
captured child's process group is swept after a NORMAL exit, not only on timeout.
Real commands, no Claude / Docker. Run from the project root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import tempfile
import time
import unittest
from pathlib import Path

from pdca_harness import assemble, gates, progress
from pdca_harness.config import Config, LeafConfig

_GATE = {"id": "C4-ci", "tier": "C4", "label": "verify", "scope": "bundle", "gating": True}


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


class GateRun(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _stub_config(self.tmp)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, gate: dict, **cfg_overrides) -> tuple[dict, dict, Path]:
        d = self.cfg.bundle("GR")
        d.mkdir(parents=True, exist_ok=True)
        (d / "brief.md").write_text("- **Slug:** gr\n", encoding="utf-8")
        (d / "patch.diff").write_text("--- a\n+++ b\n", encoding="utf-8")
        self.cfg.gates_checks = [gate]
        for k, v in cfg_overrides.items():
            setattr(self.cfg, k, v)
        result = gates.run_gates(d, self.cfg)
        row = next(r for r in result["rows"] if r["rule_id"] == gate["id"])
        return row, result, d


class OutputPersistence(GateRun):
    """#370 — the full output survives in the bundle; the row says where and how long."""

    def test_a_gate_run_writes_its_full_output_to_the_bundle(self) -> None:
        row, _, d = self._run({**_GATE, "cmd": "echo one; echo two; echo three"})
        self.assertEqual(row["log"], "gate-logs/C4-ci.log")
        text = (d / row["log"]).read_text(encoding="utf-8")
        # The row's evidence keeps only the LAST line — the log must hold all of them.
        for line in ("one", "two", "three"):
            self.assertIn(line, text)
        self.assertIn("# cmd: echo one; echo two; echo three", text)

    def test_the_row_records_its_duration(self) -> None:
        row, _, _ = self._run({**_GATE, "cmd": "true"})
        self.assertIsInstance(row["duration_secs"], int)

    def test_a_failed_gate_keeps_the_output_that_named_the_failure(self) -> None:
        row, _, d = self._run(
            {**_GATE, "cmd": "echo the_failing_test_name; false", "gating": False})
        self.assertEqual(row["result"], "fail")
        # The 120-char evidence line is the summary; the log is the record behind it.
        self.assertIn("the_failing_test_name",
                      (d / row["log"]).read_text(encoding="utf-8"))

    def test_a_timed_out_gate_logs_the_partial_output_it_produced(self) -> None:
        row, _, d = self._run(
            {**_GATE, "cmd": "echo got_this_far; sleep 60", "timeout_secs": 2,
             "gating": False})
        self.assertEqual(row["result"], "unverifiable")
        # Without the partial capture riding the TimeoutExpired, a hung gate's log
        # would say nothing about WHERE it hung.
        self.assertIn("got_this_far", (d / row["log"]).read_text(encoding="utf-8"))

    def test_the_dry_regate_never_touches_the_frozen_evidence(self) -> None:
        row, _, d = self._run({**_GATE, "cmd": "echo first-run"})
        frozen = (d / row["log"]).read_text(encoding="utf-8")
        gates.run_gates_dry(d, self.cfg)  # revalidate's runner
        self.assertEqual((d / row["log"]).read_text(encoding="utf-8"), frozen,
                         "revalidate overwrote the frozen Check's gate log")

    def test_log_filenames_stay_distinct_when_sanitization_collides(self) -> None:
        # "foo bar" and "foo_bar" both sanitize to "foo_bar" — two rows must never end
        # up pointing at each other's evidence through one shared file.
        a = gates._write_gate_log(
            self.tmp, {"id": "foo bar"}, cmd="true", cwd=self.tmp, worktree_path=None,
            attempts=[{"result": "pass", "note": "exit 0", "output": "A",
                       "duration_secs": 0, "started": "t"}])
        b = gates._write_gate_log(
            self.tmp, {"id": "foo_bar"}, cmd="true", cwd=self.tmp, worktree_path=None,
            attempts=[{"result": "pass", "note": "exit 0", "output": "B",
                       "duration_secs": 0, "started": "t"}])
        self.assertNotEqual(a, b)
        self.assertIn("A", (self.tmp / a).read_text(encoding="utf-8"))
        self.assertIn("B", (self.tmp / b).read_text(encoding="utf-8"))

    def test_the_iterate_archive_keeps_the_logs_where_the_frozen_rows_say(self) -> None:
        # The archived check-gates.json still says "gate-logs/<id>.log" — the archive
        # must preserve that relative layout, not flatten to the basename.
        from pdca_harness import driver
        row, _, d = self._run({**_GATE, "cmd": "echo evidence"})
        driver._archive_iteration(d, 1, include_brief=False)
        self.assertTrue((d / "iteration-v1" / row["log"]).is_file(),
                        "the archived log is not where the archived row points")


class ConfirmOnce(GateRun):
    """#371 — a failed gating row is confirmed exactly once; both verdicts recorded."""

    def _flip_cmd(self) -> str:
        # Fails on the first run, passes on the second — a transient in two lines.
        flag = self.tmp / "already-failed"
        return f"test -f {flag} || {{ touch {flag}; echo transient; exit 1; }}; echo fine"

    def test_a_transient_fail_becomes_a_flagged_pass_not_a_parked_bundle(self) -> None:
        row, result, _ = self._run({**_GATE, "cmd": self._flip_cmd()})
        self.assertEqual(row["result"], "pass")
        self.assertEqual(row["attempts"], ["fail", "pass"])
        self.assertTrue(row["flaky"])
        self.assertIn("transiently", row["path_line"])  # the flip is visible, not silent
        self.assertEqual(result["overall"], "pass")

    def test_both_attempts_land_in_the_gate_log(self) -> None:
        row, _, d = self._run({**_GATE, "cmd": self._flip_cmd()})
        text = (d / row["log"]).read_text(encoding="utf-8")
        self.assertIn("attempt 1/2: fail", text)
        self.assertIn("attempt 2/2: pass", text)
        self.assertIn("transient", text)
        self.assertIn("fine", text)

    def test_a_reproducible_fail_stays_red(self) -> None:
        row, result, _ = self._run({**_GATE, "cmd": "echo still-red; false"})
        self.assertEqual(row["result"], "fail")
        self.assertEqual(row["attempts"], ["fail", "fail"])
        self.assertFalse(row["flaky"])
        self.assertEqual(result["overall"], "fail")

    def test_an_advisory_row_is_a_single_sample(self) -> None:
        # The confirm exists so one transient cannot PARK the bundle; an advisory row
        # cannot park anything, so it keeps the cheaper single run.
        row, _, _ = self._run({**_GATE, "cmd": self._flip_cmd(), "gating": False})
        self.assertEqual(row["result"], "fail")
        self.assertNotIn("attempts", row)

    def test_the_config_switch_restores_single_sample_verdicts(self) -> None:
        row, result, _ = self._run({**_GATE, "cmd": self._flip_cmd()},
                                   gates_confirm_fail=False)
        self.assertEqual(row["result"], "fail")
        self.assertNotIn("attempts", row)
        self.assertEqual(result["overall"], "fail")

    def test_a_check_can_opt_out_so_a_model_backed_gate_is_never_resampled(self) -> None:
        # A gating row whose command IS a model (the batched-review row) must stay a
        # single sample: re-running it re-samples a nondeterministic judge, and a
        # second, luckier sample could overwrite real first-run blockers as "flaky".
        row, result, _ = self._run(
            {**_GATE, "cmd": self._flip_cmd(), "confirm_fail": False})
        self.assertEqual(row["result"], "fail")
        self.assertNotIn("attempts", row)
        self.assertEqual(result["overall"], "fail")

    def test_a_pass_is_never_re_run(self) -> None:
        counter = self.tmp / "runs"
        row, _, _ = self._run({**_GATE, "cmd": f"echo x >> {counter}; true"})
        self.assertEqual(row["result"], "pass")
        self.assertEqual(len(counter.read_text().splitlines()), 1)

    def test_a_flaky_pass_raises_a_section6_item_the_human_must_clear(self) -> None:
        row, result, _ = self._run({**_GATE, "cmd": self._flip_cmd()})
        items = assemble._flaky_gate_items(result)
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].kind, assemble.HUMAN)  # substrate, not builder-fixable
        self.assertIn("flaked at Check", items[0].text)
        self.assertIn(row["log"], items[0].text)

    def test_a_clean_matrix_raises_no_flake_items(self) -> None:
        _, result, _ = self._run({**_GATE, "cmd": "true"})
        self.assertEqual(assemble._flaky_gate_items(result), [])


class StragglerSweep(unittest.TestCase):
    """#372 — what a finished child left behind is swept, not inherited by the next run."""

    def _straggler_survives(self, **kwargs) -> tuple[int, bool]:
        marker = Path(tempfile.mkdtemp()) / "straggler.pid"
        try:
            cmd = f"sleep 300 & echo $! > {marker}; echo done"
            rc, _, _ = progress.run_with_heartbeat(cmd, shell=True, interval=1, **kwargs)
            straggler = int(marker.read_text().strip())
            deadline = time.monotonic() + 10
            alive = True
            while time.monotonic() < deadline:
                try:
                    os.kill(straggler, 0)
                except ProcessLookupError:
                    alive = False
                    break
                time.sleep(0.1)
            if alive:
                os.kill(straggler, signal.SIGKILL)  # don't leak it out of the test
            return rc, alive
        finally:
            shutil.rmtree(marker.parent, ignore_errors=True)

    def test_a_captured_childs_straggler_is_swept_after_a_NORMAL_exit(self) -> None:
        # The incident case: the command SUCCEEDS and returns — pre-#372 the backgrounded
        # process simply survived, burning CPU and holding ports into the next cycle.
        rc, alive = self._straggler_survives(capture=True)
        self.assertEqual(rc, 0)
        self.assertFalse(alive, "a straggler outlived its finished captured command")

    def test_a_bounded_childs_straggler_is_swept_on_normal_exit_too(self) -> None:
        rc, alive = self._straggler_survives(capture=True, timeout=60)
        self.assertEqual(rc, 0)
        self.assertFalse(alive, "a straggler outlived its finished bounded command")

    def test_an_interactive_style_child_is_left_alone(self) -> None:
        # Uncaptured + unbounded = the interactive-leaf shape: it shares the driver's
        # session (sessionizing it would cost the tty), so there is no group to sweep —
        # the straggler must survive, exactly as before.
        rc, alive = self._straggler_survives()
        self.assertEqual(rc, 0)
        self.assertTrue(alive, "an interactive-shape child's group was swept — that "
                               "group is the DRIVER'S own")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
