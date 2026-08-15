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


class InputCeiling(unittest.TestCase):
    """#209 unit layer: the per-file split, the chunk packing, and the shared
    unverifiable convention. A diff past the reviewer's input ceiling must never
    read as a review VERDICT."""

    def test_split_per_file_keeps_headers_and_preamble(self):
        diff = ("preamble\n"
                "diff --git a/a.rs b/a.rs\n+one\n"
                "diff --git a/b.rs b/b.rs\n+two\n")
        segs = rb.split_per_file(diff)
        self.assertEqual(len(segs), 2)
        self.assertTrue(segs[0].startswith("preamble\ndiff --git a/a.rs"))
        self.assertTrue(segs[1].startswith("diff --git a/b.rs"))
        self.assertEqual("".join(segs), diff)  # byte-preserving: nothing lost

    def test_handmade_patch_without_headers_is_one_segment(self):
        self.assertEqual(rb.split_per_file("--- a\n+++ b\n+x\n"),
                         ["--- a\n+++ b\n+x\n"])

    def test_pack_chunks_packs_in_order_and_isolates_oversize(self):
        a, b, c = "a" * 40, "b" * 40, "c" * 120
        chunks, oversize = rb.pack_chunks([a, c, b], budget=100)
        self.assertEqual(chunks, [a + b])       # neighbours pack; order preserved
        self.assertEqual(oversize, [c])         # too large even ALONE — never dropped

    def test_pack_chunks_measures_encoded_bytes_not_code_points(self):
        # The ceiling is a BYTE bound on the encoded stdin payload (#220 review):
        # 60 'é' are 60 code points but 120 UTF-8 bytes, so a code-point budget
        # would pack this segment (85 ≤ 100) and the pass would still die on
        # input_too_large — the exact error the planning exists to prevent.
        seg = "diff --git a/u.rs b/u.rs\n" + "é" * 60
        self.assertLessEqual(len(seg), 100)              # fits by code points…
        self.assertGreater(rb.prompt_bytes(seg), 100)    # …not by bytes
        chunks, oversize = rb.pack_chunks([seg], budget=100)
        self.assertEqual(chunks, [])
        self.assertEqual(oversize, [seg])

    def test_segment_file_reads_the_b_path(self):
        self.assertEqual(rb.segment_file("diff --git a/x/y.rs b/x/y.rs\n+1\n"),
                         "x/y.rs")

    def test_unverifiable_convention_matches_the_harness(self):
        # The script deliberately does not import the harness; these mirrored
        # values are what make the gate row read `unverifiable` — pin them.
        from pdca_harness import gates
        self.assertEqual(rb.UNVERIFIABLE_RC, gates.UNVERIFIABLE_RC)
        self.assertEqual(rb.UNVERIFIABLE_MARKER, gates.UNVERIFIABLE_MARKER)


class CeilingIntegration(unittest.TestCase):
    """#209 gate layer: main() under a small patched ceiling. A fitting diff is
    reviewed whole (every other test in this file); an oversize one is chunked
    per file and DECLARED degraded; a file too large even alone makes the run
    `unverifiable` — the review never ran — instead of a red that reads like a
    verdict (the issue_635 failure that cost a sign-off session)."""

    SEG_A = "diff --git a/a.rs b/a.rs\n" + "+a\n" * 30
    SEG_B = "diff --git a/b.rs b/b.rs\n" + "+b\n" * 30
    BIG = "diff --git a/big.rs b/big.rs\n" + "+x\n" * 200

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
        ]
        for p in self.p:
            p.start()

    def tearDown(self):
        for p in self.p:
            p.stop()
        self._argv.stop()
        self._env.stop()

    def _run(self, scripted, diff, budget=160):
        """Patch the ceiling to (prompt overhead + budget) so `budget` is the
        exact diff budget, script the passes, capture prompts per pass."""
        overhead = rb.prompt_bytes(rb.PROMPT_TEMPLATE.format(n=3, rubric="RUBRIC", diff=""))
        self.prompts_seen = []

        def fake_run_pass(idx, prompt, timeout, target):
            self.prompts_seen.append(prompt)
            ok, findings, note = scripted(idx)
            return idx, ok, findings, note
        buf = io.StringIO()
        with mock.patch.object(rb, "CODEX_INPUT_CEILING", overhead + budget), \
                mock.patch.object(rb, "collect_diff", lambda a, t: diff), \
                mock.patch.object(rb, "run_pass", fake_run_pass), \
                contextlib.redirect_stdout(buf), \
                contextlib.redirect_stderr(io.StringIO()):
            try:
                rc = rb.main()
            except SystemExit as e:
                rc = e.code if isinstance(e.code, int) else 1
        report = Path(self.bundle) / "review-batch.md"
        return rc, buf.getvalue(), (report.read_text() if report.is_file() else "")

    def test_oversize_diff_runs_chunked_and_says_degraded(self):
        rc, out, report = self._run(lambda i: (True, [], "clean"),
                                    diff=self.SEG_A + self.SEG_B)
        self.assertEqual(rc, 0)                      # a clean chunked run stays green…
        self.assertEqual(len(self.prompts_seen), 6)  # 2 chunks × 3 passes
        self.assertIn("Degraded run", report)        # …but says so where triage happens
        self.assertIn("[chunked ×2]", out)
        # Each chunk prompt carries the rubric and exactly one file's diff.
        self.assertTrue(all("RUBRIC" in p for p in self.prompts_seen))
        self.assertTrue(any("b/a.rs" in p and "b/b.rs" not in p
                            for p in self.prompts_seen))
        self.assertTrue(any("b/b.rs" in p and "b/a.rs" not in p
                            for p in self.prompts_seen))

    def test_chunked_findings_still_block(self):
        rc, out, _ = self._run(lambda i: (True, [_f("a.rs:1", "BUG", "boom")], "1"),
                               diff=self.SEG_A + self.SEG_B)
        self.assertEqual(rc, 1)
        self.assertIn("BLOCKING:", out)

    def test_oversize_single_file_declares_unverifiable_not_fail(self):
        rc, out, report = self._run(lambda i: (True, [_f("a.rs:1", "BUG", "boom")], "1"),
                                    diff=self.SEG_A + self.BIG)
        self.assertEqual(rc, rb.UNVERIFIABLE_RC)     # not 1: this is no verdict
        last = [ln for ln in out.splitlines() if ln.strip()][-1]
        self.assertTrue(last.startswith(rb.UNVERIFIABLE_MARKER),
                        f"the marker must be the LAST line (gate evidence): {last!r}")
        self.assertIn("big.rs", last)                # the unreviewed file is NAMED
        self.assertIn("Unreviewable", report)
        self.assertIn("boom", report)                # partial findings still surfaced
        self.assertEqual(len(self.prompts_seen), 3)  # only the fitting chunk ran

    def test_unverifiable_survives_a_failed_fitting_pass(self):
        # #220 review: oversize file + fitting chunk, and pass 2 over the fitting
        # chunk fails BOTH rounds (a codex timeout). The thin-union refusal must
        # not reach sys.exit first and record a hard failure — the oversize file
        # can never be reviewed by a re-run, so the unverifiable verdict owns the
        # row and the marker still names the file.
        rc, out, _ = self._run(lambda i: (i != 2, [], "ok" if i != 2 else "boom"),
                               diff=self.SEG_A + self.BIG)
        self.assertEqual(rc, rb.UNVERIFIABLE_RC)
        last = [ln for ln in out.splitlines() if ln.strip()][-1]
        self.assertTrue(last.startswith(rb.UNVERIFIABLE_MARKER), last)
        self.assertIn("big.rs", last)
        self.assertIn("2/3 passes", last)            # the thin union is named too

    def test_nothing_fits_no_pass_runs_unverifiable(self):
        rc, out, report = self._run(lambda i: (True, [], "clean"), diff=self.BIG)
        self.assertEqual(rc, rb.UNVERIFIABLE_RC)
        self.assertEqual(self.prompts_seen, [])      # the review never ran at all
        self.assertTrue(any(ln.startswith(rb.UNVERIFIABLE_MARKER)
                            for ln in out.splitlines()))
        self.assertEqual(report, "")                 # no report pretending otherwise


if __name__ == "__main__":
    unittest.main()
