"""SUMMARY §1 must carry a brief field's WHOLE value (issue #174, stdlib unittest).

§1 is what the sign-off leaf and the HUMAN read to decide accept or iterate, and what the C6
accept-guard gates on. It was assembled through `brief.parse_fields`, which is line-based, so
every field arrived cut at its first line: 88 of 94 committed criteria truncated, four rendered
as nothing at all, and the worst showed 69 characters of 13,357 — cut mid-clause.

The shapes below are the ones the corpus actually contains, not invented ones. The template
itself wraps its Success criterion placeholder over two lines (`templates/brief.md.tpl`), which
is why the failure rate was 93% rather than a handful.

Run from the project root:
    python -m unittest discover -s tests
"""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path

from pdca_harness import assemble, brief


class WholeField(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.bp = self.tmp / "brief.md"

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _brief(self, text: str) -> Path:
        self.bp.write_text(text, encoding="utf-8")
        return self.bp

    def test_a_wrapped_value_reassembles(self) -> None:
        # The shape the template itself writes.
        bp = self._brief("- **Success criterion:** the reaper commits its fence under a\n"
                         "  concurrent abort, demonstrable at C4-verify\n"
                         "- **Scope:** the fence path\n")
        self.assertEqual(brief.field(bp, "success criterion"),
                         "the reaper commits its fence under a")
        whole = brief.whole_field(bp, "success criterion")
        self.assertIn("concurrent abort", whole)
        self.assertNotIn("the fence path", whole, "must stop at the next field")

    def test_a_value_written_BENEATH_its_label_is_recovered(self) -> None:
        """The pointer-brief shape (issues 256/258/364/366). `field` returns "" for these, so
        four bundles rendered an empty criterion and the human was asked "did this work?"
        against a blank line."""
        bp = self._brief("- **Success criterion:**\n"
                         "  - **BINDING:** the count rises and returns to zero\n"
                         "- **Scope:** the seam\n")
        self.assertEqual(brief.field(bp, "success criterion"), "")
        self.assertIn("BINDING", brief.whole_field(bp, "success criterion"))

    def test_an_INDENTED_sub_bullet_does_not_end_the_value(self) -> None:
        # `_FIELD_RE` tolerates leading whitespace, so using it as the terminator truncated
        # every pointer brief to nothing. Only an UNINDENTED field ends a value.
        bp = self._brief("- **Success criterion:**\n"
                         "  - **(a)** first condition\n"
                         "  - **(b)** second condition\n"
                         "- **Scope:** x\n")
        whole = brief.whole_field(bp, "success criterion")
        self.assertIn("first condition", whole)
        self.assertIn("second condition", whole)

    def test_a_heading_ends_the_value(self) -> None:
        bp = self._brief("- **Success criterion:** it works\n\n## Notes\n\nnot the criterion\n")
        self.assertNotIn("not the criterion", brief.whole_field(bp, "success criterion"))

    def test_label_fallback_matches_field(self) -> None:
        bp = self._brief("- **Goal:** the stated goal\n")
        self.assertEqual(brief.whole_field(bp, "defect", "goal"), "the stated goal")

    def test_the_value_is_RAW_placeholders_included(self) -> None:
        """Unlike `field`, no placeholder filtering — §1 must keep rendering an unfilled field
        exactly as it does today, and the `/handoff` gate applies its own test instead."""
        bp = self._brief("- **Success criterion:** <the observable condition>\n")
        self.assertEqual(brief.field(bp, "success criterion"), "")
        self.assertTrue(brief.whole_field(bp, "success criterion").startswith("<"))

    def test_field_is_unchanged(self) -> None:
        # Several driver paths read a first line on purpose (`_resolve_target` partitions on
        # `@`, `depends_on` ignores trailing prose). Widening `field` would reach all of them.
        bp = self._brief("- **Repo + branch target:** `o/r` @ `main`\n  trailing prose\n")
        self.assertEqual(brief.field(bp, "repo + branch target"), "`o/r` @ `main`")


class SpecRendering(unittest.TestCase):
    """What §1 actually emits — the artifact the human reads."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.bp = self.tmp / "brief.md"

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _line(self, text: str, label: str, *fields: str, default: str = "") -> str:
        self.bp.write_text(text, encoding="utf-8")
        return assemble._spec_line(label, self.bp, *fields, default=default)

    def test_a_single_line_value_stays_inline(self) -> None:
        out = self._line("- **Success criterion:** it works\n", "Success criterion",
                         "success criterion")
        self.assertEqual(out, "- Success criterion: it works")

    def test_a_multiline_value_starts_on_its_own_line(self) -> None:
        out = self._line("- **Success criterion:** first\n  second\n", "Success criterion",
                         "success criterion")
        self.assertEqual(out.splitlines()[0], "- Success criterion:")
        self.assertIn("second", out)

    def test_every_continuation_stays_inside_the_list_item(self) -> None:
        # A line at column 0 would break out of the list and render as body text.
        out = self._line("- **Success criterion:**\n  - **(a)** one\n    wrapped\n  - **(b)** two\n",
                         "Success criterion", "success criterion")
        for line in out.splitlines()[1:]:
            if line.strip():
                self.assertTrue(line.startswith("  "), f"escapes the list item: {line!r}")

    def test_the_briefs_own_nesting_is_preserved(self) -> None:
        out = self._line("- **Success criterion:**\n  - **(a)** one\n    continued\n",
                         "Success criterion", "success criterion")
        body = out.splitlines()[1:]
        self.assertLess(len(body[0]) - len(body[0].lstrip()),
                        len(body[1]) - len(body[1].lstrip()),
                        "a sub-bullet's continuation must stay more indented than its bullet")

    def test_an_absent_field_falls_back_to_the_default(self) -> None:
        self.assertEqual(self._line("- **Slug:** s\n", "Outcome", "disposition hint",
                                    default="Fixed"), "- Outcome: Fixed")

    def test_an_unfilled_placeholder_renders_as_before(self) -> None:
        out = self._line("- **Success criterion:** <the observable condition>\n",
                         "Success criterion", "success criterion")
        self.assertIn("<the observable condition>", out)


if __name__ == "__main__":
    unittest.main()
