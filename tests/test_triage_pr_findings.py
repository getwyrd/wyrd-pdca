"""Deterministic tests for external-finding triage (`scripts/triage-pr-findings`).

Pins the behaviours review established on getwyrd/wyrd-pdca PR #159 and the
classifier fixes made while running it:

- issue filing is deduplicated by the comment URL (a re-run does not re-file);
- a failed `gh issue create` fails the run (the BUG is not silently lost);
- the keyword classifier maps the stable cases and does not regress the ones
  that were mis-filed (a bare "limit" is not `encoded-caps`);
- `seed_rubric` seeds rubric-addressed classes `applied` and delta-owed classes
  `open`, so recurrence detection is not falsely armed.

Run from the project root:
    python -m unittest discover -s tests
"""

from __future__ import annotations

import contextlib
import importlib.machinery
import importlib.util
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


@contextlib.contextmanager
def _quiet():
    with contextlib.redirect_stdout(io.StringIO()):
        yield

_SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "triage-pr-findings"


def _load():
    loader = importlib.machinery.SourceFileLoader("triage_pr_findings", str(_SCRIPT))
    spec = importlib.util.spec_from_loader("triage_pr_findings", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


tp = _load()


class Classify(unittest.TestCase):
    """The stable class mappings and the specific regressions review flagged."""

    def _cat(self, text):
        return tp.classify(text)

    def test_dco_false_positive(self):
        for why in ("Add the required DCO sign-off",
                    "commit dc275e is missing a Signed-off-by trailer",
                    "The reviewed commit message has no Signed-off-by"):
            self.assertEqual(self._cat(why)[0], "dco-false-positive", why)

    def test_dco_mechanism_bug_is_not_dco_noise(self):
        # A real bug about the DCO enforcement mechanism must be classified on
        # its own merits (so it can be filed), not dropped as a trailer FP.
        for why in ("the DCO guard accepts unsigned commits",
                    "No branch invokes the DCO check"):
            self.assertNotEqual(self._cat(why)[0], "dco-false-positive", why)

    def test_encoded_caps_on_a_real_sizing_finding(self):
        key, _ = self._cat("Size the body cap for XML-escaped maximum keys")
        self.assertEqual(key, "encoded-caps")

    def test_bare_limit_is_not_encoded_caps(self):
        # The regression: "limit" without a sizing word must not grab this class.
        key, _ = self._cat("Limit the number of retries to eight")
        self.assertNotEqual(key, "encoded-caps")

    def test_enforcement_reach(self):
        key, _ = self._cat("the guard reports success without checking any crate")
        self.assertEqual(key, "enforcement-reach")

    def test_false_red(self):
        key, _ = self._cat("this incorrectly rejects valid crate roots")
        self.assertEqual(key, "false-red")

    def test_docs_currency(self):
        key, _ = self._cat("Update the living architecture doc for bulk delete")
        self.assertEqual(key, "docs-currency")


class TitleOf(unittest.TestCase):
    def test_strips_badge_and_sub_tags(self):
        body = ("**<sub><sub>![P2 Badge](https://img.shields.io/badge/P2-yellow)"
                "</sub></sub>  Reject unsupported deletes**\n\nBody text.")
        self.assertEqual(tp.title_of(body), "Reject unsupported deletes")


class Ledger(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self._p = mock.patch.object(tp, "LEDGER", self.tmp / "act-ledger.json")
        self._p.start()

    def tearDown(self):
        self._p.stop()

    def test_round_trip(self):
        entries = [{"signal": "codex-pr:x", "status": "open"}]
        tp.save_ledger(entries)
        self.assertEqual(tp.load_ledger(), entries)

    def test_seed_marks_owed_classes_open_and_addressed_applied(self):
        with _quiet():
            tp.seed_rubric()
        by = {e["signal"]: e for e in tp.load_ledger()}
        for key in tp.NO_APPLIED_DELTA:
            self.assertEqual(by[f"codex-pr:{key}"]["status"], "open", key)
        # a rubric-addressed class is applied on the rubric date
        self.assertEqual(by["codex-pr:dco-false-positive"]["status"], "applied")

    def test_seed_is_idempotent(self):
        with _quiet():
            tp.seed_rubric()
            n = len(tp.load_ledger())
            tp.seed_rubric()
        self.assertEqual(len(tp.load_ledger()), n)


class IssueDedup(unittest.TestCase):
    def _proc(self, stdout):
        return lambda *a, **k: subprocess.CompletedProcess(a, 0, stdout=stdout, stderr="")

    def test_true_when_an_issue_references_the_url(self):
        with mock.patch("subprocess.run", self._proc('[{"number": 5}]')):
            self.assertTrue(tp.issue_exists_for("https://x/c1"))

    def test_false_when_none(self):
        with mock.patch("subprocess.run", self._proc("[]")):
            self.assertFalse(tp.issue_exists_for("https://x/c1"))

    def test_false_on_unparseable_search_output(self):
        with mock.patch("subprocess.run", self._proc("boom not json")):
            self.assertFalse(tp.issue_exists_for("https://x/c1"))

    def test_empty_url_never_searches(self):
        with mock.patch("subprocess.run") as run:
            self.assertFalse(tp.issue_exists_for(""))
        run.assert_not_called()


class FilingFailure(unittest.TestCase):
    """A failed `gh issue create` must fail the run, not report success."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.patches = [
            mock.patch.object(tp, "LEDGER", self.tmp / "act-ledger.json"),
            mock.patch.object(tp, "INBOX", self.tmp / "triage"),
            mock.patch.object(tp, "issue_exists_for", lambda url: False),
        ]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def _merged_bug_comment(self):
        return [{"user": {"login": tp.BOT}, "in_reply_to_id": None,
                 "path": "x.rs", "html_url": "https://x/c1",
                 "created_at": "2026-07-01T00:00:00Z",
                 "body": "**Reject unsupported version-specific deletes**\n\nA BUG."}]

    def _dispatch(self, create_rc, view_rc=0):
        def run(cmd, *a, **k):
            if "view" in cmd:            # gh pr view ... mergedAt
                out = "" if view_rc else "2026-07-02T00:00:00Z"
                return subprocess.CompletedProcess(cmd, view_rc, stdout=out, stderr="api down")
            if "create" in cmd:          # gh issue create
                return subprocess.CompletedProcess(cmd, create_rc, stdout="", stderr="auth expired")
            return subprocess.CompletedProcess(cmd, 0, stdout="", stderr="")
        return run

    def test_failed_creation_returns_failure_and_records_unfiled(self):
        with mock.patch.object(tp, "gh_json", lambda p: self._merged_bug_comment()), \
                mock.patch("subprocess.run", self._dispatch(create_rc=1)), _quiet():
            failed = tp.triage_pr(1, file_issues=True, today="2026-07-23")
        self.assertTrue(failed)
        report = (self.tmp / "triage" / "PR-1.md").read_text()
        self.assertIn("FAILED to file", report)

    def test_successful_creation_returns_ok(self):
        with mock.patch.object(tp, "gh_json", lambda p: self._merged_bug_comment()), \
                mock.patch("subprocess.run", self._dispatch(create_rc=0)), _quiet():
            failed = tp.triage_pr(1, file_issues=True, today="2026-07-23")
        self.assertFalse(failed)

    def test_failed_merge_lookup_fails_closed(self):
        # gh pr view errors -> merge state unknown must not read as "open" and
        # silently skip a merged PR's BUGs; the run fails and says so.
        with mock.patch.object(tp, "gh_json", lambda p: self._merged_bug_comment()), \
                mock.patch("subprocess.run", self._dispatch(create_rc=0, view_rc=1)), _quiet():
            failed = tp.triage_pr(1, file_issues=True, today="2026-07-23")
        self.assertTrue(failed)
        self.assertIn("merge state", (self.tmp / "triage" / "PR-1.md").read_text())


if __name__ == "__main__":
    unittest.main()
