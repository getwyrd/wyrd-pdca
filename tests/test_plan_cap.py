"""scripts/plan-cap — the Plan intake cap (wyrd-pdca-P1, #238)."""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "plan-cap"

STATUS = """\
AWAITING_SIGNOFF  issue_selftest  [2 NEEDS-HUMAN]
BUILT             issue_682  [oversized]
PLANNED           issue_508  [blocked-by: 637, 693]  [oversized]
PLANNED           issue_625  [blocked-by: 637]
UNPLANNED         issue_265
ITERATE_DO        issue_771  [oversized]
COMPLETE          issue_115  [unpublished]
DISCONTINUED      issue_636  [oversized]
RESOLVED          issue_262
"""


class PlanCapTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.status = Path(self.tmp.name) / "status.txt"
        self.status.write_text(STATUS, encoding="utf-8")
        self.env = {k: v for k, v in os.environ.items() if k != "PDCA_PLANNED_CAP"}

    def tearDown(self):
        self.tmp.cleanup()

    def run_cap(self, *args, status=None, env=None):
        return subprocess.run([sys.executable, str(SCRIPT), "--status-file", str(status or self.status), *args],
                              capture_output=True, text=True, env=env or self.env)

    def test_counts_only_post_plan_pre_signoff_states(self):
        r = self.run_cap("--cap", "10")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.splitlines()[0],
                         "planned 5/10 (cap) — room for 5, need 1: Plan intake open")
        # UNPLANNED, COMPLETE, DISCONTINUED and RESOLVED are not intake.
        for absent in ("265", "115", "636", "262"):
            self.assertNotIn(absent, r.stdout)

    def test_over_cap_exits_one_and_names_the_rule(self):
        r = self.run_cap("--cap", "3")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.splitlines()[0],
                         "planned 5/3 (cap) — room for 0, need 1: Plan intake closed")
        self.assertIn("wyrd-pdca-P1", r.stdout)
        self.assertIn("PLANNED            2  508 625", r.stdout)

    def test_at_cap_has_no_room(self):
        # Codex review on PR #245: a count equal to the cap must not admit one more.
        r = self.run_cap("--cap", "5", "--quiet")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.strip(), "planned 5/5 (cap) — room for 0, need 1: Plan intake closed")

    def test_need_is_a_batch_budget(self):
        # Codex review on PR #245: a batch must fit in the remaining room as a whole.
        self.assertEqual(self.run_cap("--cap", "8", "--need", "3", "--quiet").returncode, 0)
        r = self.run_cap("--cap", "8", "--need", "4", "--quiet")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.strip(), "planned 5/8 (cap) — room for 3, need 4: Plan intake closed")

    def test_cap_from_environment(self):
        r = self.run_cap("--quiet", env={**self.env, "PDCA_PLANNED_CAP": "2"})
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.strip(), "planned 5/2 (cap) — room for 0, need 1: Plan intake closed")

    def test_default_cap_is_six(self):
        self.assertIn("5/6 (cap) — room for 1, need 1: Plan intake open", self.run_cap("--quiet").stdout)

    def test_unreadable_status_exits_two(self):
        # Codex review on PR #245: a measurement failure must not read as a closed gate.
        r = self.run_cap(status=Path(self.tmp.name) / "missing.txt")
        self.assertEqual(r.returncode, 2)
        self.assertEqual(r.stdout, "")
        self.assertIn("cannot read status file", r.stderr)
        self.assertNotIn("Traceback", r.stderr)

    def test_bad_need_is_a_usage_error(self):
        self.assertEqual(self.run_cap("--need", "0").returncode, 2)


if __name__ == "__main__":
    unittest.main()
