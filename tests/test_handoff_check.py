"""Deterministic tests for the leaf exit gate (`scripts/handoff-check`).

The gate exists because `leaves._invoke` runs every `interactive = true` leaf as a bare
`subprocess.run(...)` with no `check=` and no capture: the driver blocks until the `claude`
process exits and discards the exit code, so "the human pressed Ctrl-D" and "the leaf
discharged its contract" are the same event. These pin the contract per leaf, and the two
rules that decide what counts as a failure:

- MISSING fails only for a bundle the caller NAMED — a planner session legitimately briefs
  some issues and not others, so an absent brief on an unnamed bundle is not a defect;
- MALFORMED always fails — whoever wrote the artifact wrote it wrong, and no scoping
  argument excuses that.

Plus the two false positives the corpus caught while this was being written: an empty
`Test file` and a missing `Slug` are both tolerated by the driver, so neither may fail the
gate (see `_LOAD_BEARING_FIELDS`).

Run from the project root:
    python -m unittest discover -s tests
"""

from __future__ import annotations

import importlib.machinery
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

from pdca_harness import leaves

_SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "handoff-check"


def _load():
    loader = importlib.machinery.SourceFileLoader("handoff_check", str(_SCRIPT))
    spec = importlib.util.spec_from_loader("handoff_check", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


hc = _load()

_BRIEF = """# Brief

- **Slug:** fix-the-thing
- **Repo + branch target:** `getwyrd/wyrd` @ `main`
- **Test file:** crates/x/tests/thing.rs
"""


class _Base(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.d = self.tmp / "issue_1"
        self.d.mkdir(parents=True)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _brief(self, text: str = _BRIEF) -> None:
        (self.d / "brief.md").write_text(text, encoding="utf-8")

    def _ok(self, results) -> bool:
        return not [r for r in results if not r.ok and not r.warn]


class Planner(_Base):
    def test_an_authored_brief_passes(self) -> None:
        self._brief()
        self.assertTrue(self._ok(hc.check_planner(self.d, named=True)))

    def test_an_absent_brief_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_planner(self.d, named=True)))
        # Unnamed: the planner briefs some issues and not others — silence, not a defect.
        self.assertEqual(hc.check_planner(self.d, named=False), [])

    def test_an_unfilled_template_fails_even_unnamed(self) -> None:
        # MALFORMED, not MISSING: the file exists, so someone wrote it and wrote it wrong.
        self._brief("# Brief\n\n- **Slug:** <short-kebab-slug>\n")
        self.assertFalse(self._ok(hc.check_planner(self.d, named=False)))

    def test_a_missing_branch_target_fails(self) -> None:
        # The one field with no fallback: `publish._resolve_target` partitions on `@` and
        # an empty value yields an empty repo spec AND an empty base.
        self._brief("# Brief\n\n- **Slug:** fix-the-thing\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("repo + branch target" in r.label for r in results), results)

    def test_an_empty_test_file_is_tolerated(self) -> None:
        """Seven committed bundles ship `Test file` empty on purpose, and `brief.test_files`
        returns [] for it — the driver only uses it to unlink a shipped test on iterate. A
        gate that failed them would false-positive on 7% of the corpus."""
        self._brief("# Brief\n\n- **Slug:** s\n- **Repo + branch target:** `o/r` @ `main`\n"
                    "- **Test file:**\n")
        self.assertTrue(self._ok(hc.check_planner(self.d, named=True)))

    def test_an_unfilled_optional_field_warns_but_does_not_fail(self) -> None:
        self._brief(_BRIEF + "- **Ordering note:** <optional free text>\n")
        results = hc.check_planner(self.d, named=True)
        self.assertTrue(self._ok(results))
        self.assertTrue(any(r.warn for r in results), results)


class Signoff(_Base):
    def _decision(self, text: str) -> None:
        (self.d / leaves.SIGNOFF_DECISION).write_text(text, encoding="utf-8")

    def test_a_bare_accept_passes(self) -> None:
        self._decision("accept\n")
        self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_every_valid_token_is_recognized(self) -> None:
        for token in sorted(leaves.VALID_DECISIONS):
            with self.subTest(token=token):
                self._decision(f"{token}\nbecause the cause was misread\n")
                self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_an_unrecognized_token_fails(self) -> None:
        self._decision("looks-good-to-me\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("not recognized" in r.label for r in results))

    def test_an_iterate_without_a_rationale_fails(self) -> None:
        # The driver folds the rationale into the brief's carry-forward; without it the next
        # beat rebuilds blind.
        self._decision("iterate-do\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("no rationale" in r.label for r in results), results)

    def test_a_discontinue_without_a_rationale_fails(self) -> None:
        self._decision("discontinue\n   \n")
        self.assertFalse(self._ok(hc.check_signoff(self.d, named=True)))

    def test_an_absent_decision_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_signoff(self.d, named=True)))
        self.assertEqual(hc.check_signoff(self.d, named=False), [])

    def test_a_model_authored_section9_fails(self) -> None:
        """§9 is the driver's to write, under the C6 accept-guard (`signoff.record`). This
        runs BEFORE the flow applies the decision, so a set §9 can only be the session's."""
        self._decision("accept\n")
        (self.d / "SUMMARY.md").write_text(
            "## 9. Check sign-off\n\n- Outcome: merged-wider\n", encoding="utf-8")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("§9" in r.label for r in results), results)


class Publisher(_Base):
    def test_both_artifacts_present_passes(self) -> None:
        (self.d / "commit-msg.txt").write_text("fix: the thing\n", encoding="utf-8")
        (self.d / "pr-description.md").write_text("## What\n\nthe thing\n", encoding="utf-8")
        self.assertTrue(self._ok(hc.check_publisher(self.d, named=True)))

    def test_an_empty_artifact_fails_even_unnamed(self) -> None:
        (self.d / "commit-msg.txt").write_text("   \n", encoding="utf-8")
        self.assertFalse(self._ok(hc.check_publisher(self.d, named=False)))

    def test_a_missing_artifact_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_publisher(self.d, named=True)))
        self.assertEqual(hc.check_publisher(self.d, named=False), [])


class Act(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        (self.tmp / "process").mkdir()
        self.cfg = mock.Mock(process_dir=self.tmp / "process")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _log(self, text: str) -> None:
        (self.tmp / "process" / "act-log.md").write_text(text, encoding="utf-8")

    def test_an_entry_dated_today_passes(self) -> None:
        self._log("# Act review — 2026-07-26 — the thing\n")
        self.assertTrue(all(r.ok for r in hc.check_act(self.cfg, "2026-07-26")))

    def test_only_older_entries_fails_and_names_the_newest(self) -> None:
        self._log("# Act review — 2026-07-21 — older\n")
        results = hc.check_act(self.cfg, "2026-07-26")
        self.assertFalse(all(r.ok for r in results))
        self.assertIn("2026-07-21", results[0].detail)

    def test_an_absent_log_fails(self) -> None:
        self.assertFalse(all(r.ok for r in hc.check_act(self.cfg, "2026-07-26")))

    def test_an_act_queue_heading_counts(self) -> None:
        # `process/act-log.md` carries both "Act review" and "Act queue" headings.
        self._log("# Act queue — 2026-07-26 — raised at Plan\n")
        self.assertTrue(all(r.ok for r in hc.check_act(self.cfg, "2026-07-26")))


class TheMarker(_Base):
    """The gate records its verdict only on a clean pass — a marker is a claim."""

    def _run(self, *argv: str) -> int:
        with redirect_stdout(io.StringIO()):
            return hc.main(list(argv))

    def test_a_pass_records_the_marker(self) -> None:
        self._brief()
        cfg = mock.Mock(bundle_root=self.tmp, process_dir=self.tmp / "process")
        cfg.find_bundle.return_value = self.d
        with mock.patch.object(hc.Config, "load", return_value=cfg):
            self.assertEqual(self._run("--leaf", "planner", "1"), 0)
        stamp = json.loads((self.d / hc.MARKER).read_text(encoding="utf-8"))
        self.assertEqual(stamp["leaf"], "planner")

    def test_a_failure_records_nothing(self) -> None:
        cfg = mock.Mock(bundle_root=self.tmp, process_dir=self.tmp / "process")
        cfg.find_bundle.return_value = self.d
        with mock.patch.object(hc.Config, "load", return_value=cfg):
            self.assertEqual(self._run("--leaf", "planner", "1"), 1)
        self.assertFalse((self.d / hc.MARKER).exists())

    def test_no_record_suppresses_the_marker_on_a_pass(self) -> None:
        self._brief()
        cfg = mock.Mock(bundle_root=self.tmp, process_dir=self.tmp / "process")
        cfg.find_bundle.return_value = self.d
        with mock.patch.object(hc.Config, "load", return_value=cfg):
            self.assertEqual(self._run("--leaf", "planner", "1", "--no-record"), 0)
        self.assertFalse((self.d / hc.MARKER).exists())


if __name__ == "__main__":
    unittest.main()
