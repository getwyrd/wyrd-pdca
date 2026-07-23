"""Deterministic tests for the batched-review gate (`scripts/review-branch`).

The gating batched review went through three review rounds on getwyrd/wyrd-pdca
PR #159 because each fix was verified with throwaway snippets rather than
committed tests, so an interaction slipped every time. This suite pins the
behaviours those rounds established, so the drip is replaced by one pass:

- a pass is USABLE only with a parseable finding or the explicit NO-FINDINGS
  sentinel (malformed output is never read as clean);
- passes run with cwd = the target checkout, not the harness root;
- the gate requires all N passes (one retry) and refuses a thinner union;
- it blocks on EVERY surviving finding, not only BUG;
- the DCO/sign-off noise filter matches the false-positive SHAPE, not any
  rationale mentioning the words;
- a rejection binds to (loc, class, rationale-match), so an old rejection
  cannot auto-clear a new defect at the same line;
- every blocker is carried in ONE final evidence line (the harness keeps only
  the last output line).

Run from the project root:
    python -m unittest discover -s tests
"""

from __future__ import annotations

import importlib.machinery
import importlib.util
import io
import contextlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

_SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "review-branch"


def _load():
    loader = importlib.machinery.SourceFileLoader("review_branch", str(_SCRIPT))
    spec = importlib.util.spec_from_loader("review_branch", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


rb = _load()


def _f(loc, cls, why, votes=1):
    return {"loc": loc, "cls": cls, "why": why, "votes": votes}


class ParseFindings(unittest.TestCase):
    def test_parses_a_finding_line(self):
        out = rb.parse_findings("crates/x.rs:12 | BUG | off-by-one in the loop")
        self.assertEqual(out, [{"loc": "crates/x.rs:12", "cls": "BUG",
                                "why": "off-by-one in the loop"}])

    def test_ignores_prose_and_banners(self):
        self.assertEqual(rb.parse_findings("codex\n2026-... INFO some banner\nprose here"), [])

    def test_no_findings_sentinel(self):
        self.assertTrue(rb.NO_FINDINGS_RE.search("NO FINDINGS"))
        self.assertTrue(rb.NO_FINDINGS_RE.search("preamble\nNO FINDINGS\ntrailer"))
        self.assertFalse(rb.NO_FINDINGS_RE.search("no problems were found"))


class NoiseFilter(unittest.TestCase):
    """The DCO/sign-off exemption must catch the false-positive shape only."""

    def _noise(self, why, cls="BUG"):
        return rb.is_noise(_f("f:1", cls, why))

    def test_dco_false_positives_are_noise(self):
        for why in ("commit dc275e is missing a Signed-off-by trailer",
                    "Add the required DCO sign-off",
                    "The reviewed commit message has no Signed-off-by"):
            self.assertTrue(self._noise(why), why)

    def test_real_signoff_logic_bugs_survive(self):
        # incl. the DCO-*mechanism* bugs that the too-broad filter dropped
        for why in ("this guard blocks sign-off for valid input",
                    "the change allows a DCO failure to pass unnoticed",
                    "Missing DCO validation allows unsigned commits",
                    "No branch invokes the DCO check",
                    "the DCO guard accepts unsigned commits"):
            self.assertFalse(self._noise(why), why)

    def test_tracked_deferral_is_noise(self):
        self.assertTrue(self._noise("Deferred — tracked in #624"))

    def test_ordinary_bug_is_not_noise(self):
        self.assertFalse(self._noise("null deref when the map is empty"))


class Rejections(unittest.TestCase):
    """A rejection binds to (loc, class, MATCH-substring-of-rationale)."""

    def _rejected_file(self, body):
        d = tempfile.mkdtemp()
        (Path(d) / "review-rejected.md").write_text(body, encoding="utf-8")
        ns = mock.Mock(rejected=None, bundle=True)
        with mock.patch.dict(os.environ, {"PDCA_BUNDLE": d}):
            return rb.load_rejected(ns, None)

    def test_requires_class_match_and_reason(self):
        rej = self._rejected_file(
            "# decisions\n"
            "x.rs:10 | BUG | off-by-one in the range check | intentional, tracked #7\n"
            "y.rs:5 | BUG | | reasonless-so-ignored\n"        # empty MATCH
            "z.rs:9 | BUG | some phrase |   \n"               # empty reason
            "w.rs:1 | NOTACLASS | phrase | reason\n")         # bad class
        self.assertEqual(rej, [("x.rs:10", "BUG", "off-by-one in the range check")])

    def test_same_defect_is_suppressed(self):
        rej = [("x.rs:10", "BUG", "off-by-one in the range check")]
        self.assertTrue(rb.is_rejected(
            _f("x.rs:10", "BUG", "There is an off-by-one in the range check here"), rej))

    def test_new_defect_at_a_rejected_line_still_blocks(self):
        rej = [("x.rs:10", "BUG", "off-by-one in the range check")]
        self.assertFalse(rb.is_rejected(
            _f("x.rs:10", "BUG", "null deref when the map is empty"), rej))

    def test_different_class_is_not_cleared(self):
        rej = [("x.rs:10", "CONVENTION", "naming")]
        self.assertFalse(rb.is_rejected(_f("x.rs:10", "BUG", "naming"), rej))


class RunPassUsability(unittest.TestCase):
    """A pass is usable only with a parseable finding or the sentinel; it runs
    in the target checkout."""

    def _proc(self, stdout="", returncode=0, raises=None):
        def fake(*a, **k):
            self._kwargs = k
            if raises:
                raise raises
            return subprocess.CompletedProcess(a, returncode, stdout=stdout, stderr="err")
        return fake

    def test_findings_make_a_pass_usable(self):
        with mock.patch("subprocess.run", self._proc("f:1 | BUG | boom")):
            idx, ok, findings, note = rb.run_pass(1, "p", 60, Path("/tgt"))
        self.assertTrue(ok)
        self.assertEqual(findings[0]["cls"], "BUG")

    def test_no_findings_sentinel_is_usable_and_clean(self):
        with mock.patch("subprocess.run", self._proc("NO FINDINGS")):
            _, ok, findings, _ = rb.run_pass(1, "p", 60, Path("/tgt"))
        self.assertTrue(ok)
        self.assertEqual(findings, [])

    def test_malformed_nonempty_output_is_unusable(self):
        with mock.patch("subprocess.run", self._proc('{"json": true} explanatory prose')):
            _, ok, _, note = rb.run_pass(1, "p", 60, Path("/tgt"))
        self.assertFalse(ok)
        self.assertIn("unusable", note)

    def test_nonzero_exit_is_unusable(self):
        with mock.patch("subprocess.run", self._proc("", returncode=1)):
            _, ok, _, _ = rb.run_pass(1, "p", 60, Path("/tgt"))
        self.assertFalse(ok)

    def test_timeout_is_unusable(self):
        with mock.patch("subprocess.run",
                        self._proc(raises=subprocess.TimeoutExpired("codex", 60))):
            _, ok, _, note = rb.run_pass(1, "p", 60, Path("/tgt"))
        self.assertFalse(ok)
        self.assertIn("timed out", note)

    def test_pass_runs_in_the_target_checkout(self):
        with mock.patch("subprocess.run", self._proc("NO FINDINGS")):
            rb.run_pass(1, "p", 60, Path("/some/target"))
        self.assertEqual(self._kwargs.get("cwd"), "/some/target")


class GateIntegration(unittest.TestCase):
    """Drive main() with scripted passes and assert exit code, report, and the
    single-line gate evidence — the layer the round-2/round-3 findings hit."""

    def setUp(self):
        self.bundle = tempfile.mkdtemp()
        self.target = Path(tempfile.mkdtemp())
        self._env = mock.patch.dict(os.environ, {"PDCA_BUNDLE": self.bundle}, clear=False)
        self._env.start()
        self._argv = mock.patch.object(sys, "argv", ["review-branch", "--bundle"])
        self._argv.start()
        self.p = [
            mock.patch.object(rb, "resolve_target", lambda repo: self.target),
            mock.patch.object(rb, "load_rubric", lambda t: "RUBRIC"),
            mock.patch.object(rb, "collect_diff", lambda a, t: "diff --git a b\n+x"),
        ]
        for p in self.p:
            p.start()

    def tearDown(self):
        for p in self.p:
            p.stop()
        self._argv.stop()
        self._env.stop()

    def _run(self, scripted):
        """scripted(idx) -> (usable, findings, note). Returns (rc, stdout, report)."""
        def fake_run_pass(idx, prompt, timeout, target):
            ok, findings, note = scripted(idx)
            return idx, ok, findings, note
        buf = io.StringIO()
        with mock.patch.object(rb, "run_pass", fake_run_pass), \
                contextlib.redirect_stdout(buf), \
                contextlib.redirect_stderr(io.StringIO()):
            try:
                rc = rb.main()
            except SystemExit as e:  # thin-union refusal
                rc = e.code if isinstance(e.code, int) else 1
        report = Path(self.bundle) / "review-batch.md"
        return rc, buf.getvalue(), (report.read_text() if report.is_file() else "")

    def test_all_clean_passes_zero(self):
        rc, out, report = self._run(lambda i: (True, [], "clean"))
        self.assertEqual(rc, 0)
        self.assertIn("No untriaged findings", report)
        self.assertNotIn("BLOCKING:", out)

    def test_a_bug_blocks_and_appears_in_one_evidence_line(self):
        rc, out, report = self._run(lambda i: (True, [_f("x.rs:1", "BUG", "boom")], "1"))
        self.assertEqual(rc, 1)
        self.assertIn("x.rs:1", report)
        self.assertIn("BLOCKING:", out)
        self.assertIn("x.rs:1 [BUG] boom", out)

    def test_a_convention_only_finding_also_blocks(self):
        rc, _, _ = self._run(lambda i: (True, [_f("x.rs:1", "CONVENTION", "nit")], "1"))
        self.assertEqual(rc, 1)

    def test_two_distinct_defects_on_one_line_stay_separate(self):
        # Different rationales at the same (loc, class) must NOT collapse into a
        # single corroborating vote — both must remain independently blocking.
        pair = [_f("x.rs:1", "BUG", "off-by-one in the range check"),
                _f("x.rs:1", "BUG", "null deref when the map is empty")]
        rc, out, report = self._run(lambda i: (True, pair, "2"))
        self.assertEqual(rc, 1)
        self.assertIn("off-by-one in the range check", report)
        self.assertIn("null deref when the map is empty", report)
        # rejecting only ONE of them must still leave the other blocking
        (Path(self.bundle) / "review-rejected.md").write_text(
            "x.rs:1 | BUG | off-by-one in the range check | tracked #9\n", encoding="utf-8")
        rc2, _, report2 = self._run(lambda i: (True, pair, "2"))
        self.assertEqual(rc2, 1)
        self.assertIn("null deref when the map is empty", report2)

    def test_every_blocker_is_on_the_single_final_line(self):
        findings = [_f("a.rs:1", "BUG", "one"), _f("b.rs:2", "TEST-GAP", "two")]
        rc, out, _ = self._run(lambda i: (True, findings, "2"))
        self.assertEqual(rc, 1)
        last = [ln for ln in out.splitlines() if ln.strip()][-1]
        self.assertIn("a.rs:1 [BUG] one", last)
        self.assertIn("b.rs:2 [TEST-GAP] two", last)

    def test_thin_union_refuses_to_certify(self):
        # pass 2 fails on both the initial run and its retry.
        rc, out, _ = self._run(lambda i: (i != 2, [], "ok" if i != 2 else "boom"))
        self.assertNotEqual(rc, 0)

    def test_retry_recovers_a_transient_failure(self):
        calls = {}

        def scripted(i):
            calls[i] = calls.get(i, 0) + 1
            if i == 2 and calls[i] == 1:
                return (False, [], "transient")
            return (True, [], "ok")
        rc, _, report = self._run(scripted)
        self.assertEqual(rc, 0)
        self.assertIn("No untriaged findings", report)

    def test_a_recorded_rejection_clears_the_block(self):
        (Path(self.bundle) / "review-rejected.md").write_text(
            "x.rs:1 | BUG | boom | deliberate, tracked #9\n", encoding="utf-8")
        rc, out, report = self._run(lambda i: (True, [_f("x.rs:1", "BUG", "boom happens")], "1"))
        self.assertEqual(rc, 0)
        self.assertIn("Recorded rejections", report)
        self.assertNotIn("BLOCKING:", out)

    def test_a_dco_false_positive_is_dropped_not_blocking(self):
        rc, _, report = self._run(
            lambda i: (True, [_f("x.rs:1", "BUG", "Add the required DCO sign-off")], "1"))
        self.assertEqual(rc, 0)
        self.assertIn("Auto-dropped", report)


if __name__ == "__main__":
    unittest.main()
