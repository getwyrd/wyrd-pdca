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

    def test_an_issue_reference_is_not_a_heading(self) -> None:
        """PR #175 review. `line.lstrip().startswith("#")` matched an indented issue
        reference in prose — `  #442 rule` at results/issue_407/brief.md:24 — and stopped
        extraction mid-criterion, truncating two committed briefs to roughly half. The
        anti-truncation fix was still truncating, for a different reason."""
        bp = self._brief("- **Success criterion:** the oracle refuses a fault that did not\n"
                         "  bite (an un-materialized run FAILS as inconclusive — the\n"
                         "  #442 rule); and the logic is exercised red-green at Check\n"
                         "- **Scope:** x\n")
        whole = brief.whole_field(bp, "success criterion")
        self.assertIn("#442", whole)
        self.assertIn("red-green at Check", whole, "extraction must not stop at the reference")

    def test_ANY_unindented_line_ends_the_value(self) -> None:
        """PR #175 review round 3. Two special cases had accumulated — the next field, then
        ATX headings — and each was a guess at what else might sit at column 0. A
        `</content>` wrapper line in `results/issue_256/brief.md` slipped past both, so §2
        rendered `likely-fix </content>`. The general rule is that a continuation is blank or
        INDENTED; anything else at column 0 ends the value, whatever it happens to be."""
        for terminator in ("</content>", "## Notes", "- **Scope:** x", "plain prose"):
            with self.subTest(terminator=terminator):
                bp = self._brief(f"- **Success criterion:** it works\n{terminator}\n")
                self.assertEqual(brief.whole_field(bp, "success criterion"), "it works")

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
    """What §1 actually emits — the artifact the human reads.

    v0.56.0 merge: exercised through the LIVE composition `- label: _item(whole_field(…))`
    (upstream #336's rendering, which replaced the instance's `_spec_line`). The properties
    pinned here are renderer-independent — a value must stay one list item with its
    structure intact — and the instance-corpus SIBLING regression stays pinned. (The old
    renderer's every-multiline-value-on-its-own-line shape was cosmetic and is gone; the
    inline-first-line shape `_item` emits is equally valid Markdown.)"""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.bp = self.tmp / "brief.md"

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _line(self, text: str, label: str, *fields: str, default: str = "") -> str:
        self.bp.write_text(text, encoding="utf-8")
        value = brief.whole_field(self.bp, *fields, default=default)
        return f"- {label}: {assemble._item(value)}"

    def test_a_single_line_value_stays_inline(self) -> None:
        out = self._line("- **Success criterion:** it works\n", "Success criterion",
                         "success criterion")
        self.assertEqual(out, "- Success criterion: it works")

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

    def test_SIBLING_sub_bullets_stay_siblings(self) -> None:
        """PR #175 review round 2. `whole_field`'s `.strip()` dedented the inline remainder
        only, so a uniform re-indent pushed every continuation one level deeper — turning a
        value's sibling sub-bullets into children of the first and changing the structure the
        human reads at sign-off (`results/issue_364/brief.md:48-62`)."""
        out = self._line("- **Success criterion:**\n  - **BINDING:** one\n    wrapped\n"
                         "  - **DEFERRED:** two\n", "Success criterion", "success criterion")
        indents = {l.split("- **")[0].count(" "): l for l in out.splitlines()
                   if "**" in l and l.strip().startswith("- **")}
        binding = next(l for l in out.splitlines() if "BINDING" in l)
        deferred = next(l for l in out.splitlines() if "DEFERRED" in l)
        wrapped = next(l for l in out.splitlines() if "wrapped" in l)
        ind = lambda x: len(x) - len(x.lstrip())
        self.assertEqual(ind(binding), ind(deferred), "siblings must share an indent")
        self.assertGreater(ind(wrapped), ind(binding), "a continuation stays under its bullet")

    def test_an_absent_field_falls_back_to_the_default(self) -> None:
        self.assertEqual(self._line("- **Slug:** s\n", "Outcome", "disposition hint",
                                    default="Fixed"), "- Outcome: Fixed")

    def test_an_unfilled_placeholder_renders_as_before(self) -> None:
        out = self._line("- **Success criterion:** <the observable condition>\n",
                         "Success criterion", "success criterion")
        self.assertIn("<the observable condition>", out)


if __name__ == "__main__":
    unittest.main()
