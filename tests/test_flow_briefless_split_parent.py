"""A split parent with no brief.md must not crash `flow` (INSTANCE DELTA,
eduralph/pdca-harness#597).

A split accepted before #481 left its parent with a `split` close marker and no brief:
the brief had been archived by an iterate-to-Plan. The marker reads as past Do (BUILT), so
`flow` drove it, and the first brief read (`_point_at_integration` → `_resolve_target`)
raised FileNotFoundError and killed the whole run (issue_654, 2026-10-01).
"""
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from pdca_harness import flow, handoff, publish, split, state
from pdca_harness.config import Config, LeafConfig

TEMPLATES = Path(__file__).resolve().parents[1] / "templates"
ARCHIVED = ("- **Slug:** big-slice\n- **Defect:** d\n- **Success criterion:** c\n"
            "- **Repo + branch target:** getwyrd/wyrd @ main\n- **Scope:** s\n")


class BrieflessSplitParent(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = Config(
            root=self.tmp, bundle_root=self.tmp / "results",
            process_dir=self.tmp / "process", templates_dir=TEMPLATES,
            default_branch="main", tracker_system="github",
            tracker_url="https://github.com/acme/widgets", issue_id_example="#1",
            builder=LeafConfig(mode="stub"), reviewer=LeafConfig(mode="stub"))
        self.parent = self.cfg.bundle("654")
        (self.parent / "iteration-v2").mkdir(parents=True)
        (self.parent / "iteration-v2" / "brief.md").write_text(ARCHIVED, encoding="utf-8")
        (self.parent / state.CLOSE_MARKER).write_text("split\n", encoding="utf-8")
        (self.parent / split.PROPOSAL).write_text("proposal", encoding="utf-8")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_the_parent_reads_built_so_flow_would_drive_it(self) -> None:
        self.assertEqual(state.state(self.parent), state.BUILT)

    def test_restore_writes_the_brief_accept_writes_since_481(self) -> None:
        (self.parent / split.LINEAGE).write_text(
            json.dumps({"version": 1, "id": "654", "children": ["691", "692"]}), encoding="utf-8")
        bp = split.restore_parent_brief(self.parent, self.cfg)
        text = bp.read_text(encoding="utf-8")
        self.assertIn("issue_691, issue_692", text)
        self.assertIn("iteration-v2/brief.md", text)
        self.assertEqual(handoff.check_planner(self.parent, self.cfg, dependencies=False,
                                               track=False), [])
        self.assertEqual(publish._resolve_target(self.parent)[:2], ("getwyrd/wyrd", "main"))

    def test_restore_refuses_a_bundle_that_is_not_a_split_parent(self) -> None:
        (self.parent / state.CLOSE_MARKER).write_text("wontfix\n", encoding="utf-8")
        with self.assertRaises(split.SplitError):
            split.restore_parent_brief(self.parent, self.cfg)
        self.assertFalse((self.parent / "brief.md").exists())

    def test_restore_refuses_without_an_archive(self) -> None:
        shutil.rmtree(self.parent / "iteration-v2")
        with self.assertRaises(split.SplitError):
            split.restore_parent_brief(self.parent, self.cfg)
        self.assertFalse((self.parent / "brief.md").exists())

    def test_flow_restores_the_brief_and_drives_the_parent(self) -> None:
        with mock.patch.object(flow, "_drive_and_act", return_value={}) as drive:
            flow.flow_ids(self.cfg, ["654"])
        self.assertTrue((self.parent / "brief.md").exists())
        self.assertEqual(drive.call_args.args[1], [self.parent])

    def test_flow_skips_an_unrestorable_briefless_bundle_instead_of_crashing(self) -> None:
        shutil.rmtree(self.parent / "iteration-v2")
        with mock.patch.object(flow, "_drive_and_act", return_value={}) as drive:
            got = flow.flow_ids(self.cfg, ["654"])
        drive.assert_not_called()
        self.assertEqual(got, {"654": state.BUILT})


    def test_a_malformed_lineage_children_value_falls_back_to_the_proposal(self) -> None:
        # #269 review: a valid-version record whose `children` is not a list of ids must not
        # raise TypeError out of the restore (flow_ids only catches SplitError).
        for value in (7, [1, None], "691"):
            with self.subTest(children=value):
                (self.parent / "brief.md").unlink(missing_ok=True)
                (self.parent / split.LINEAGE).write_text(
                    json.dumps({"version": 1, "id": "654", "children": value}), encoding="utf-8")
                text = split.restore_parent_brief(self.parent, self.cfg).read_text(encoding="utf-8")
                self.assertIn(f"named in `{split.PROPOSAL}`", text)

    def test_a_failed_write_leaves_no_brief_behind(self) -> None:
        # #269 review: a torn brief.md would read as present next run and skip the restore.
        with mock.patch.object(split.os, "replace", side_effect=OSError("disk full")):
            with self.assertRaises(split.SplitError) as caught:
                split.restore_parent_brief(self.parent, self.cfg)
        self.assertIn("disk full", str(caught.exception))
        self.assertFalse((self.parent / "brief.md").exists())
        self.assertEqual([p.name for p in self.parent.iterdir() if p.name.startswith(".brief")], [])

    def test_the_csv_sweep_restores_the_brief_too(self) -> None:
        # #269 review: `flow --from-csv` (flow_batch) sweeps every in-flight bundle into
        # _drive_and_act without going through flow_ids.
        with mock.patch.object(flow.leaves, "do_plan_batch"), \
                mock.patch.object(flow, "_drive_and_act", return_value={}) as drive:
            flow.flow_batch(self.cfg, csv="x.csv")
        self.assertTrue((self.parent / "brief.md").exists())
        self.assertEqual(drive.call_args.args[1], [self.parent])

    def test_the_csv_sweep_leaves_out_an_unrestorable_bundle(self) -> None:
        shutil.rmtree(self.parent / "iteration-v2")
        with mock.patch.object(flow.leaves, "do_plan_batch"), \
                mock.patch.object(flow, "_drive_and_act", return_value={}) as drive:
            self.assertEqual(flow.flow_batch(self.cfg, csv="x.csv"), {})
        drive.assert_not_called()


if __name__ == "__main__":
    unittest.main()
