"""Offline slice for resuming a Check leaf that never ran (issue #187, stdlib unittest).

`check-gates.json` IS the CHECKED marker (state.state), but the BUILT arm writes it and
only THEN runs the reviewer + advisory leaves. A death in that window — Ctrl-C on a hung
gate, OOM, a killed session — used to be UNRECOVERABLE: the driver saw CHECKED, went
straight to `assemble`, and papered over the hole with the missing-review placeholder.
The reviewer could never run again for that round. issue_635 landed in exactly that hole.

Proves: the driver resumes a leaf that left NO artifact and NO error log, leaves a leaf
that genuinely FAILED alone (its recorded failure stands — no silent retry), and doesn't
re-spend an advisory leaf that already succeeded. Stub leaves, no Claude. Run from the
project root:  PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path

from pdca_harness import driver, state
from pdca_harness.config import Config, LeafConfig

_ADVISORY = [{"id": "adversary", "mode": "stub"}]


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


class ResumeUnrunCheckLeaves(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _stub_config(self.tmp)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _interrupted_bundle(self, iid: str) -> Path:
        """A bundle killed between the gate write and the reviewer leaf: gates recorded,
        no review artifact, no error log, no SUMMARY — i.e. state CHECKED."""
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True)
        (d / "brief.md").write_text("- **Slug:** resume\n", encoding="utf-8")
        (d / "patch.diff").write_text("--- a\n+++ b\n", encoding="utf-8")
        (d / "check-gates.json").write_text('{"issue_dir": "x", "overall": "pass", '
                                            '"rows": []}', encoding="utf-8")
        self.assertEqual(state.state(d), state.CHECKED)
        return d

    # --- the hole: a leaf that never started is run before SUMMARY freezes its absence ---

    def test_a_reviewer_that_never_ran_is_resumed(self) -> None:
        d = self._interrupted_bundle("NEVER")
        driver.advance(d, self.cfg)
        self.assertTrue((d / "check-review.md").exists(),
                        "the reviewer leaf was skipped — its absence is now frozen forever")
        self.assertTrue((d / "SUMMARY.md").exists())

    def test_the_bundle_still_reaches_sign_off(self) -> None:
        d = self._interrupted_bundle("FLOW")
        driver.advance(d, self.cfg)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)

    # --- a leaf that RAN and FAILED keeps its recorded failure; it is not retried ---

    def test_a_failed_reviewer_is_left_alone(self) -> None:
        d = self._interrupted_bundle("FAILED")
        (d / "check-review.error.log").write_text("----- attempt 1 — exit 1 -----\n",
                                                  encoding="utf-8")
        driver.advance(d, self.cfg)
        self.assertFalse((d / "check-review.md").exists(),
                         "a leaf that genuinely failed was silently retried behind the "
                         "human's back — the error log is the record that it ran")

    def test_a_reviewer_that_already_succeeded_is_not_re_run(self) -> None:
        d = self._interrupted_bundle("DONE")
        (d / "check-review.md").write_text("HUMAN-WRITTEN VERDICT\n", encoding="utf-8")
        driver.advance(d, self.cfg)
        self.assertEqual((d / "check-review.md").read_text(encoding="utf-8"),
                         "HUMAN-WRITTEN VERDICT\n", "an existing review was overwritten")

    # --- a close bundle has the same hole: its leaf-2 stand-in is written after the gates ---

    def test_a_close_bundles_confirmation_note_is_resumed(self) -> None:
        d = self._interrupted_bundle("CLOSE")
        driver._resume_unrun_check_leaves(d, self.cfg, close="duplicate")
        review = d / "check-review.md"
        self.assertTrue(review.exists(),
                        "a close bundle interrupted between run_close_gates and the "
                        "close-confirmation note loses it permanently")
        self.assertIn("NEEDS-HUMAN", review.read_text(encoding="utf-8"))

    def test_a_close_bundle_with_its_note_already_written_is_untouched(self) -> None:
        d = self._interrupted_bundle("CLOSEDONE")
        (d / "check-review.md").write_text("EXISTING CLOSE NOTE\n", encoding="utf-8")
        driver._resume_unrun_check_leaves(d, self.cfg, close="duplicate")
        self.assertEqual((d / "check-review.md").read_text(encoding="utf-8"),
                         "EXISTING CLOSE NOTE\n")

    # --- advisory leaves follow the same rule, but as a PHASE (selection is a policy) ---

    def test_an_advisory_phase_that_never_ran_is_resumed(self) -> None:
        self.cfg.advisory_leaves = _ADVISORY
        d = self._interrupted_bundle("ADV")
        driver.advance(d, self.cfg)
        self.assertTrue((d / "check-advisory-adversary.md").exists())

    def test_an_advisory_leaf_that_failed_is_not_retried(self) -> None:
        self.cfg.advisory_leaves = _ADVISORY
        d = self._interrupted_bundle("ADVFAIL")
        (d / "check-advisory-adversary.error.log").write_text("boom\n", encoding="utf-8")
        driver.advance(d, self.cfg)
        self.assertFalse((d / "check-advisory-adversary.md").exists())

    def test_an_advisory_leaf_that_succeeded_is_not_re_spent(self) -> None:
        # The reviewer is missing (so the resume path RUNS), but this advisory leaf already
        # produced its verdict: re-running it would overwrite a recorded advisory finding
        # and re-spend an xhigh model run to do it.
        self.cfg.advisory_leaves = _ADVISORY
        d = self._interrupted_bundle("ADVDONE")
        (d / "check-advisory-adversary.md").write_text("PRIOR ADVISORY\n", encoding="utf-8")
        driver.advance(d, self.cfg)
        self.assertEqual((d / "check-advisory-adversary.md").read_text(encoding="utf-8"),
                         "PRIOR ADVISORY\n")
        self.assertTrue((d / "check-review.md").exists())  # the missing one still resumed

    def test_an_unselected_vendor_pool_member_is_not_mistaken_for_interrupted(self) -> None:
        """Under `vendor-complement` the configured list is a POOL from which exactly ONE
        leaf runs, so a deliberately unselected member has no artifact and no error log
        either. Judging per leaf would read that correct selection as an interruption and
        run a second, same-vendor reviewer — inventing a decorrelation lapse."""
        self.cfg.advisory_leaves = [{"id": "adversary", "mode": "stub", "family": "codex"},
                                    {"id": "code-review", "mode": "stub", "family": "claude"}]
        self.cfg.advisory_selection = {"mode": "vendor-complement"}
        d = self._interrupted_bundle("POOL")
        (d / "check-advisory-adversary.md").write_text("SELECTED LEAF RAN\n", encoding="utf-8")
        driver.advance(d, self.cfg)
        self.assertFalse((d / "check-advisory-code-review.md").exists(),
                         "an unselected pool member was run as though it had been interrupted")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
