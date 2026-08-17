"""Offline slice for the delta-retirement check (issue #231, stdlib unittest).

The instance's divergences from the vendored engine are marked ``INSTANCE DELTA`` and
name the upstream issue whose landing retires them; ``scripts/checks/delta_retirement.py``
is the automated notice that one has landed. The *state query* needs network and lives in
the doctor row — but the *scan* is pure file reading, so everything except the `gh` call
is proven here, offline, with the query stubbed.

Proves: a marker is associated with the issue(s) named in full ``owner/repo#N`` form on
the marker line or the two lines after it — forward-only and stopped at the next marker,
so a site can never borrow a neighbour's reference (the misattribution the check exists
to end); a short-form ``(#N)`` or a preceding-line reference reads as unattributed and
warns, as does prose about the deltas never registering at all; a CLOSED (or MERGED —
via the `gh pr view` fallback) issue is a retirement candidate (exit 1) while all-OPEN
is quiet (exit 0); a `gh` that fails OR is not installed is its own loud state (exit 3),
as is a file the scan could not read; and — against THIS repo's real tree — every live
marker stays attributable, so a delta added without its retirement condition in
scanner-readable form fails `make check` before it can rot.

Run from the project root:
    python -m unittest discover -s tests
"""
from __future__ import annotations

import importlib.util
import io
import json
import os
import stat
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
_SPEC = importlib.util.spec_from_file_location(
    "delta_retirement", ROOT / "scripts" / "checks" / "delta_retirement.py")
dr = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(dr)

UPSTREAM = "eduralph/pdca-harness"


def _tree(tmp: str, files: dict[str, str]) -> Path:
    root = Path(tmp)
    for rel, text in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
    return root


def _gh(issues: dict[tuple[str, int], tuple[str, str]] | None = None,
        prs: dict[tuple[str, int], tuple[str, str]] | None = None,
        raises: Exception | None = None):
    """A fake `gh` runner, mirroring the real call shape (argv[1] is issue|pr,
    argv[3] the number). Unlisted ⇒ that subcommand fails; ``raises`` ⇒ the binary
    itself is unavailable (FileNotFoundError et al.). The stub ASSERTS the
    load-bearing call shape — the binary name, the view subcommand, the --json
    request, captured text output and a finite timeout — so a regression in how
    the real `gh` is invoked cannot hide behind a permissive fake."""
    tables = {"issue": issues or {}, "pr": prs or {}}
    def runner(argv, capture_output=False, text=False, timeout=None):
        if raises is not None:
            raise raises
        assert argv[0] == "gh" and argv[2] == "view" and "--json" in argv
        assert capture_output is True and text is True
        assert timeout is not None and 0 < timeout <= 300
        got = tables[argv[1]].get((argv[argv.index("-R") + 1], int(argv[3])))
        if got is None:
            return SimpleNamespace(returncode=1, stdout="", stderr="not found")
        state, title = got
        return SimpleNamespace(
            returncode=0, stdout=json.dumps({"state": state, "title": title}), stderr="")
    return runner


class ScanAssociation(unittest.TestCase):
    def _sites(self, text: str):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "mod.py"
            p.write_text(text, encoding="utf-8")
            return dr.scan_file(p, "mod.py")

    def test_full_ref_on_marker_line(self):
        s = self._sites(f"# INSTANCE DELTA ({UPSTREAM}#462, OPEN): wait.\n")
        self.assertEqual(s, [dr.Site("mod.py", 1, ((UPSTREAM, 462),))])

    def test_ref_on_following_line_within_window(self):
        s = self._sites("# INSTANCE DELTA — sync before the gate\n"
                        f"# ({UPSTREAM}#531, OPEN). Ordering is the trick.\n")
        self.assertEqual(s[0].refs, ((UPSTREAM, 531),))

    def test_bare_shorthand_is_unattributed(self):
        # "#N" is ambiguous between the instance tracker and upstream — only the
        # full owner/repo#N form attributes (docs/INTEGRATION.md §2).
        s = self._sites('"""Bring a PR up to date. INSTANCE DELTA (#531).\n"""\n')
        self.assertEqual(s[0].refs, ())

    def test_preceding_line_ref_is_not_adopted(self):
        # Forward-only: a reference ABOVE the marker could as easily be a
        # neighbour's; the convention puts it on the marker line or right below.
        s = self._sites(f"# {UPSTREAM}#462 is still OPEN at v0.57.0.\n"
                        "# The wait is an INSTANCE DELTA in merge.py.\n")
        self.assertEqual(s[0].refs, ())

    def test_window_stops_at_the_next_marker(self):
        # An unattributed site two lines above an attributed one must NOT inherit
        # its neighbour's reference — that would silently pass the exact
        # discipline break the check reports.
        s = self._sites("# INSTANCE DELTA: an orphan divergence\n"
                        f"# INSTANCE DELTA ({UPSTREAM}#531): the neighbour\n")
        self.assertEqual(s[0].refs, ())
        self.assertEqual(s[1].refs, ((UPSTREAM, 531),))

    def test_marker_is_case_insensitive_and_singular_only(self):
        self.assertEqual(len(self._sites("# instance delta, owner/repo#7 — lowercase\n")), 1)
        self.assertEqual(self._sites("# the instance deltas above are all tracked\n"), [])

    def test_ref_without_marker_is_not_a_site(self):
        self.assertEqual(self._sites(f"# see {UPSTREAM}#370 for context\n"), [])

    def test_instance_refs_beside_the_upstream_one_are_kept_distinct(self):
        # "(#228, upstream owner/repo#531)" — the short-form instance issue is not
        # adopted; the full-form upstream one is.
        s = self._sites(f"# INSTANCE DELTA (#228, upstream {UPSTREAM}#531).\n")
        self.assertEqual(s[0].refs, ((UPSTREAM, 531),))

    def test_ref_three_lines_below_is_out_of_window(self):
        # The documented window is the marker line plus TWO lines — a ref on the
        # third reads as unattributed, exactly as docs/INTEGRATION.md §2 warns.
        s = self._sites("# INSTANCE DELTA: a wrapped comment\n"
                        "# that keeps wrapping\n"
                        "# and wrapping some more\n"
                        f"# ({UPSTREAM}#531).\n")
        self.assertEqual(s[0].refs, ())

    def test_non_utf8_bytes_do_not_crash_the_scan(self):
        # A stray binary byte in a scanned file must not take the checker down —
        # errors="replace" keeps the line-shape and the refs readable.
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "mod.py"
            p.write_bytes(b"# INSTANCE DELTA (" + UPSTREAM.encode() + b"#7)\n"
                          b"\xff\xfe not text \xff\n")
            s = dr.scan_file(p, "mod.py")
        self.assertEqual(s[0].refs, ((UPSTREAM, 7),))


class MainVerdicts(unittest.TestCase):
    FILES = {
        "src/pdca_harness/merge.py":
            f"# INSTANCE DELTA ({UPSTREAM}#462, OPEN): wait for the rollup.\n"
            "x = 1\n"
            f"# INSTANCE DELTA ({UPSTREAM}#531, OPEN): sync the base.\n",
        "pdca.toml":
            f"# INSTANCE DELTA ({UPSTREAM}#531): default ON here.\n",
        # Not scanned: prose/bundles/tests never register sites.
        "results/issue_9/SUMMARY.md": "INSTANCE DELTA (owner/repo#1) quoted in a bundle\n",
        "tests/test_x.py": "# INSTANCE DELTA (owner/repo#2) in a fixture\n",
    }

    def _run(self, runner, files=None):
        out = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            root = _tree(tmp, files or self.FILES)
            rc = dr.main(root=root, runner=runner, out=out)
        return rc, out.getvalue()

    def test_all_open_is_quiet(self):
        rc, text = self._run(_gh(issues={(UPSTREAM, 462): ("OPEN", "wait"),
                                         (UPSTREAM, 531): ("OPEN", "sync")}))
        self.assertEqual(rc, 0)
        self.assertNotIn("RETIREMENT CANDIDATE", text)
        self.assertIn("src/pdca_harness/merge.py:1", text)

    def test_closed_issue_is_a_retirement_candidate(self):
        rc, text = self._run(_gh(issues={(UPSTREAM, 462): ("CLOSED", "wait"),
                                         (UPSTREAM, 531): ("OPEN", "sync")}))
        self.assertEqual(rc, 1)
        self.assertIn("RETIREMENT CANDIDATE", text)
        # It names which delta and which issue — the definition of done.
        self.assertIn(f"{UPSTREAM}#462", text)
        self.assertIn("src/pdca_harness/merge.py:1", text)
        # The sibling delta's sites are grouped under ITS issue, not the closed one.
        self.assertIn("src/pdca_harness/merge.py:3, pdca.toml:1", text)

    def test_a_merged_pr_ref_is_a_candidate_via_the_pr_fallback(self):
        # `gh issue view` refuses a PR number; the check must fall through to
        # `gh pr view` rather than reporting the ref unreachable.
        rc, text = self._run(_gh(issues={(UPSTREAM, 531): ("OPEN", "sync")},
                                 prs={(UPSTREAM, 462): ("MERGED", "wait")}))
        self.assertEqual(rc, 1)
        self.assertIn("MERGED", text)
        self.assertIn("RETIREMENT CANDIDATE", text)

    def test_unanswerable_gh_is_loud_not_silent(self):
        rc, text = self._run(_gh())  # every subcommand fails (offline/unauth)
        self.assertEqual(rc, 3)
        self.assertIn("UNREACHABLE", text)

    def test_missing_gh_binary_is_unreachable_not_a_traceback(self):
        rc, text = self._run(_gh(raises=FileNotFoundError("gh")))
        self.assertEqual(rc, 3)
        self.assertIn("UNREACHABLE", text)

    def test_wedged_gh_is_unreachable_not_a_hang_or_traceback(self):
        import subprocess
        rc, text = self._run(_gh(raises=subprocess.TimeoutExpired("gh", 30)))
        self.assertEqual(rc, 3)
        self.assertIn("UNREACHABLE", text)

    def test_a_reply_without_a_state_is_unreachable_not_a_candidate(self):
        # `gh` exiting 0 with JSON that carries no `state` confirmed nothing — the
        # "retirement candidate" banner on such a reply would assert a fix landed
        # on evidence that never said so.
        def runner(argv, capture_output=False, text=False, timeout=None):
            return SimpleNamespace(returncode=0, stdout="{}", stderr="")
        rc, text = self._run(runner)
        self.assertEqual(rc, 3)
        self.assertIn("UNREACHABLE", text)
        self.assertNotIn("RETIREMENT CANDIDATE", text)

    def test_one_unreachable_issue_does_not_swallow_the_rest_of_the_report(self):
        # 462 unanswerable, 531 CLOSED: both must appear — an early UNREACHABLE
        # stopping the loop would hide a real retirement candidate behind it.
        rc, text = self._run(_gh(issues={(UPSTREAM, 531): ("CLOSED", "sync")}))
        self.assertEqual(rc, 3)  # unreachable outranks, but both are reported
        self.assertIn("UNREACHABLE", text)
        self.assertIn("RETIREMENT CANDIDATE", text)
        self.assertIn(f"{UPSTREAM}#531", text)

    def test_the_default_root_is_this_repo(self):
        # main() with no explicit root must scan THIS project tree, and say so in
        # the header — the doctor row relies on that default.
        out = io.StringIO()
        rc = dr.main(runner=_gh(), out=out)
        self.assertEqual(rc, 3)  # live sites found, every query stubbed to fail
        self.assertIn("delta-retirement:", out.getvalue())
        self.assertIn(f"(root: {ROOT})", out.getvalue())

    def test_unattributed_marker_warns(self):
        rc, text = self._run(_gh(), files={
            "src/x.py": "# INSTANCE DELTA with no issue named\n"})
        self.assertEqual(rc, 1)
        self.assertIn("UNATTRIBUTED", text)
        self.assertIn("src/x.py:1", text)

    @unittest.skipIf(os.geteuid() == 0, "root reads through mode 000")
    def test_unreadable_file_is_loud_not_a_clean_pass(self):
        out = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            root = _tree(tmp, {"src/hidden.py": "# INSTANCE DELTA (owner/repo#9)\n"})
            (root / "src/hidden.py").chmod(0)
            try:
                rc = dr.main(root=root, runner=_gh(), out=out)
            finally:
                (root / "src/hidden.py").chmod(stat.S_IRUSR | stat.S_IWUSR)
        self.assertEqual(rc, 3)
        self.assertIn("UNREADABLE", out.getvalue())
        self.assertIn("src/hidden.py", out.getvalue())


class LiveTreeDiscipline(unittest.TestCase):
    """The real repo's markers stay attributable — offline (scan only, no `gh`)."""

    def test_every_live_marker_names_its_upstream_issue(self):
        sites, unreadable = dr.scan_tree(ROOT)
        self.assertEqual(unreadable, [])
        self.assertTrue(sites, "the known deltas exist — an empty scan means the "
                               "scanner or the markers broke")
        unattributed = [f"{s.path}:{s.line}" for s in sites if not s.refs]
        self.assertEqual(unattributed, [],
                         "marked divergence without a retirement condition — name the "
                         "upstream issue in full `owner/repo#N` form on the marker line "
                         "or the two lines right after it")
        # The scan must span roots, not stop at its own exclusion: the engine AND
        # the config both carry live sites today.
        paths = {s.path for s in sites}
        self.assertIn("pdca.toml", paths)
        self.assertTrue(any(p.startswith("src/") for p in paths))

    def test_no_near_miss_marker_spelling_hides_from_the_scanner(self):
        # The guard above is vacuous for a marker the regex never finds — so pin
        # the realistic drift shapes (INSTANCE-DELTA, INSTANCE_DELTA) to the
        # canonical spelling: every near-miss in the scan roots must ALSO be a
        # real marker hit, or someone wrote a divergence the checker cannot see.
        import re
        near = re.compile(r"\binstance[\s_-]+delta\b", re.IGNORECASE)
        offenders = []
        for f in dr._iter_files(ROOT):
            if f.resolve() == Path(dr.__file__).resolve():
                continue
            text = f.read_text(encoding="utf-8", errors="replace")
            for i, line in enumerate(text.splitlines(), 1):
                if near.search(line) and not dr._MARKER.search(line):
                    offenders.append(f"{f.relative_to(ROOT)}:{i}: {line.strip()}")
        self.assertEqual(offenders, [],
                         "near-miss marker spelling — use `INSTANCE DELTA` exactly, "
                         "or the retirement check cannot track it")

    def test_the_documented_deltas_are_found(self):
        # The three deltas issue #231 tables, plus the #335 carry (marked with #231).
        sites, _ = dr.scan_tree(ROOT)
        found = {n for s in sites for repo, n in s.refs if repo == UPSTREAM}
        self.assertLessEqual({335, 371, 462, 531}, found)


if __name__ == "__main__":
    unittest.main()
