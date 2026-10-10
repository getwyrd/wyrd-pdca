"""An unreadable out-of-batch prerequisite is a refusal, not a traceback (stdlib unittest).

INSTANCE DELTA (eduralph/pdca-harness#660, PR #259 review; re-raised on PR #272):
`waves.check_dep_graph` reads an out-of-batch prerequisite's state to decide whether it is
COMPLETE. A prerequisite whose brief.md is not UTF-8 made that read raise
`UnicodeDecodeError`, which `flow` does not turn into its one-line refusal (#589), so the
whole run aborted with a traceback. Unreadable is not COMPLETE: refuse with rc 2, name the
prerequisite and why, and build nothing.

Run from the project root:
    PYTHONPATH=src python -m unittest tests.test_unreadable_prereq
"""

from __future__ import annotations

import unittest

from test_flow_dependency_refusal import FlowDependencyRefusal


class UnreadablePrerequisite(FlowDependencyRefusal):
    def test_a_non_utf8_prerequisite_brief_refuses_rc2(self) -> None:
        self._bundle("A", "- **Depends on:** B")
        b = self.cfg.bundle("B")
        b.mkdir(parents=True)
        (b / "brief.md").write_bytes(b"# Brief\n\n- **Slug:** \xff\xfe not utf-8\n")
        rc = self._call(["A"])
        err = self._assert_refusal(rc)
        self.assertIn("issue_A: declared dependency 'B' cannot be read", err)
        self.assertIn("UnicodeDecodeError", err)
        self.assertIn("repair B's brief.md", err)
        self._assert_nothing_built("A")


# Only the new case runs here; the inherited ones run in their own module.
del FlowDependencyRefusal


if __name__ == "__main__":
    unittest.main()
