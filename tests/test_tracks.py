"""Intake tracks — INSTANCE DELTA (eduralph/pdca-harness#594), PR #264 review.

The per-track intake cap only holds if every bundle is charged to the right track: a brief
leaving Plan names an OPEN track, and a split's children carry their parent's.
"""
import shutil
import tempfile
import unittest
from pathlib import Path

from pdca_harness import handoff, split, tracks
from pdca_harness.config import Config, LeafConfig

TEMPLATES = Path(__file__).resolve().parents[1] / "templates"
INTAKE = '[intake]\ntracks = ["alpha", "blackbox", "m5"]\ndefault_track = "alpha"\n'
BRIEF = ("- **Slug:** s\n{track}- **Defect:** d\n- **Success criterion:** c\n"
         "- **Repo + branch target:** org/repo @ main\n- **Scope:** one thing\n")


def _proposal(*children: str) -> str:
    body = "<!-- pdca:split-proposal v1 -->\n# Split proposal\n\n"
    for i, child in enumerate(children, 1):
        body += f"<!-- pdca:child child-{i} -->\n{child}\n<!-- pdca:end child-{i} -->\n\n"
    return body


class Base(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = Config(
            root=self.tmp, bundle_root=self.tmp / "results",
            process_dir=self.tmp / "process", templates_dir=TEMPLATES,
            default_branch="main", tracker_system="github",
            tracker_url="https://github.com/acme/widgets", issue_id_example="#1",
            builder=LeafConfig(mode="stub"), reviewer=LeafConfig(mode="stub"))

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def intake(self, text: str = INTAKE) -> None:
        (self.tmp / "pdca.toml").write_text(text, encoding="utf-8")


class Settings(Base):
    def test_no_intake_table_enforces_nothing(self) -> None:
        tr = tracks.settings(self.tmp)
        self.assertFalse(tr.enforced)
        self.assertEqual(tr.problem(""), "")
        self.assertEqual(tr.default, "alpha")

    def test_the_declared_tracks_are_the_open_set(self) -> None:
        self.intake('[intake]\ntracks = ["Alpha", "`M5`"]\ndefault_track = "m5"\n')
        tr = tracks.settings(self.tmp)
        self.assertEqual(tr.open, ("alpha", "m5"))
        self.assertEqual(tr.default, "m5")
        self.assertIn("not an open track", tr.problem("m7"))
        self.assertIn("missing", tr.problem(""))
        self.assertEqual(tr.problem("m5"), "")


class Field(unittest.TestCase):
    def test_reads_the_filled_field_in_any_spelling(self) -> None:
        self.assertEqual(tracks.of_text("- **Track:** `M5`\n"), "m5")
        self.assertEqual(tracks.of_text("- **Track**: blackbox (the tester)\n"), "blackbox")
        self.assertEqual(tracks.of_text("- **Track:** <required — one OPEN track\n  more>\n"), "")
        self.assertEqual(tracks.of_text("- **Slug:** x\n"), "")

    def test_with_track_fills_a_placeholder_or_adds_the_field(self) -> None:
        placeholder = "- **Slug:** x\n- **Track:** <required —\n  more>\n- **Defect:** d\n"
        self.assertEqual(tracks.with_track(placeholder, "m5"),
                         "- **Slug:** x\n- **Track:** m5\n- **Defect:** d\n")
        self.assertEqual(tracks.with_track("- **Slug:** x\n- **Defect:** d\n", "m5"),
                         "- **Slug:** x\n- **Track:** m5\n- **Defect:** d\n")
        named = "- **Slug:** x\n- **Track:** alpha\n"
        self.assertEqual(tracks.with_track(named, "m5"), named)


class PlanExit(Base):
    """#264 review: `/handoff` must refuse a brief whose Track would charge the wrong cap."""

    def brief(self, track_line: str) -> Path:
        d = self.cfg.bundle("7")
        d.mkdir(parents=True, exist_ok=True)
        (d / "brief.md").write_text(BRIEF.format(track=track_line), encoding="utf-8")
        return d

    def problems(self, track_line: str) -> list[str]:
        return handoff.check_planner(self.brief(track_line), self.cfg, dependencies=False)

    def test_an_open_track_passes(self) -> None:
        self.intake()
        self.assertEqual(self.problems("- **Track:** m5\n"), [])

    def test_missing_placeholder_and_unopened_tracks_are_refused(self) -> None:
        self.intake()
        for line in ("", "- **Track:** <required — one OPEN track>\n", "- **Track:** m7\n"):
            with self.subTest(line=line):
                found = self.problems(line)
                self.assertEqual(len(found), 1, found)
                self.assertIn("'track'", found[0])

    def test_nothing_is_required_without_declared_tracks(self) -> None:
        self.assertEqual(self.problems(""), [])

    def test_the_split_archive_check_skips_the_track(self) -> None:
        self.intake()
        d = self.brief("")
        self.assertEqual(handoff.check_planner(d, self.cfg, dependencies=False, track=False), [])


class SplitChildren(Base):
    """#264 review: children written from the split template carry the parent's track."""

    def setUp(self) -> None:
        super().setUp()
        self.intake()
        self.parent = self.cfg.bundle("500")
        self.parent.mkdir(parents=True)
        (self.parent / "brief.md").write_text("- **Slug:** parent\n- **Track:** blackbox\n",
                                              encoding="utf-8")

    def propose(self, *children: str) -> list[split.Child]:
        text = _proposal(*children)
        (self.parent / split.PROPOSAL).write_text(text, encoding="utf-8")
        return split.parse(text)

    def test_children_inherit_the_parent_track(self) -> None:
        tpl_child = "- **Slug:** a\n- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>\n"
        children = self.propose(tpl_child, "- **Slug:** b\n")
        split.preflight(self.parent, children, self.cfg)
        staged = split.materialise(children, ["601", "602"], self.cfg, self.tmp / "stage",
                                   parent=self.parent)
        self.assertEqual([tracks.of(d / "brief.md") for d in staged], ["blackbox", "blackbox"])

    def test_a_child_in_another_track_is_refused_before_filing(self) -> None:
        children = self.propose("- **Slug:** a\n- **Track:** m5\n")
        with self.assertRaises(split.SplitError) as caught:
            split.preflight(self.parent, children, self.cfg)
        self.assertIn("inherits the parent's track", str(caught.exception))

    def test_an_untagged_parent_passes_the_default_track_on(self) -> None:
        (self.parent / "brief.md").write_text("- **Slug:** parent\n", encoding="utf-8")
        children = self.propose("- **Slug:** a\n")
        split.preflight(self.parent, children, self.cfg)
        (d,) = split.materialise(children, ["601"], self.cfg, self.tmp / "stage",
                                 parent=self.parent)
        self.assertEqual(tracks.of(d / "brief.md"), "alpha")


if __name__ == "__main__":
    unittest.main()
