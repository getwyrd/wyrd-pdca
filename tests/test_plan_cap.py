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

    def tearDown(self):
        self.tmp.cleanup()

    def run_cap(self, *args, env=None):
        return subprocess.run([sys.executable, str(SCRIPT), "--status-file", str(self.status), *args],
                              capture_output=True, text=True, env=env)

    def test_counts_only_post_plan_pre_signoff_states(self):
        r = self.run_cap("--cap", "10")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.splitlines()[0], "planned 5/10 (cap) — under cap: Plan intake open")
        # UNPLANNED, COMPLETE, DISCONTINUED and RESOLVED are not intake.
        for absent in ("265", "115", "636", "262"):
            self.assertNotIn(absent, r.stdout)

    def test_over_cap_exits_one_and_names_the_rule(self):
        r = self.run_cap("--cap", "3")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.splitlines()[0], "planned 5/3 (cap) — OVER by 2: Plan intake closed")
        self.assertIn("wyrd-pdca-P1", r.stdout)
        self.assertIn("PLANNED            2  508 625", r.stdout)

    def test_at_cap_is_open(self):
        self.assertEqual(self.run_cap("--cap", "5").returncode, 0)

    def test_cap_from_environment_and_quiet(self):
        r = self.run_cap("--quiet", env={**os.environ, "PDCA_PLANNED_CAP": "2"})
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout.strip(), "planned 5/2 (cap) — OVER by 3: Plan intake closed")

    def test_default_cap_is_six(self):
        r = self.run_cap("--quiet", env={k: v for k, v in os.environ.items() if k != "PDCA_PLANNED_CAP"})
        self.assertIn("5/6 (cap)", r.stdout)


if __name__ == "__main__":
    unittest.main()
