"""Offline unit for the RESOLVED terminal state — a notes-only tracker whose issue was
resolved OUTSIDE a PDCA cycle (`state.is_resolved` / `state.state`).

An open-question / research issue is logged as `notes.json` and never carried through a
cycle (no `brief.md`); when its tracking issue is resolved by decision in-issue, a
top-level `resolved` object in `notes.json` records that. Such a tracker has no result to
sign off, so it can never reach COMPLETE/DISCONTINUED and would otherwise sit in the
pending UNPLANNED list forever. The `resolved` record makes it terminal.

Pins: a briefless bundle with a `resolved` object reads RESOLVED; without it, UNPLANNED; a
malformed `notes.json` is UNPLANNED (never a crash); the reclassification is scoped to
briefless bundles so a real cycle bundle with a stray key is untouched; RESOLVED is HALTED.
"""

from __future__ import annotations

import fnmatch
import inspect
import json
import tempfile
import unittest
from pathlib import Path

from pdca_harness import state


def _bundle(root: Path, name: str) -> Path:
    d = root / name
    d.mkdir(parents=True)
    return d


class ResolvedStateTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_briefless_tracker_with_resolved_object_is_terminal(self) -> None:
        d = _bundle(self.root, "issue_262")
        (d / "notes.json").write_text(
            json.dumps({"body": "open question", "resolved": {"github_state": "closed"}}),
            encoding="utf-8",
        )
        self.assertTrue(state.is_resolved(d))
        self.assertEqual(state.state(d), state.RESOLVED)
        self.assertIn(state.RESOLVED, state.HALTED)

    def test_briefless_tracker_without_resolved_is_unplanned(self) -> None:
        d = _bundle(self.root, "issue_265")
        (d / "notes.json").write_text(json.dumps({"body": "still open"}), encoding="utf-8")
        self.assertFalse(state.is_resolved(d))
        self.assertEqual(state.state(d), state.UNPLANNED)

    def test_no_notes_json_is_unplanned(self) -> None:
        d = _bundle(self.root, "issue_999")
        self.assertFalse(state.is_resolved(d))
        self.assertEqual(state.state(d), state.UNPLANNED)

    def test_malformed_notes_json_is_not_a_crash(self) -> None:
        d = _bundle(self.root, "issue_998")
        (d / "notes.json").write_text("{not valid json", encoding="utf-8")
        self.assertFalse(state.is_resolved(d))
        self.assertEqual(state.state(d), state.UNPLANNED)

    def test_resolved_must_be_an_object_not_a_bare_value(self) -> None:
        # A truthy-but-non-object `resolved` (a bool/string) is not the structured record.
        d = _bundle(self.root, "issue_997")
        (d / "notes.json").write_text(json.dumps({"resolved": True}), encoding="utf-8")
        self.assertFalse(state.is_resolved(d))
        self.assertEqual(state.state(d), state.UNPLANNED)

    def test_non_object_toplevel_notes_json_is_not_a_crash(self) -> None:
        # Valid JSON whose top level is an array/string/number is not a notes object —
        # must be "not resolved", never an AttributeError from `.get`.
        for payload in ("[1, 2, 3]", '"a string"', "42", "null"):
            d = _bundle(self.root, f"issue_arr_{abs(hash(payload))}")
            (d / "notes.json").write_text(payload, encoding="utf-8")
            self.assertFalse(state.is_resolved(d))
            self.assertEqual(state.state(d), state.UNPLANNED)

    def test_iterated_cycle_left_briefless_is_not_resolved(self) -> None:
        # `iterate-to-Plan` archives brief.md (+ downstream) into iteration-vN/, leaving a
        # REAL rejected cycle briefless and awaiting a re-plan. Even with a stray `resolved`
        # key it must stay UNPLANNED (in the resume set), never RESOLVED (Codex P2 on #150).
        d = _bundle(self.root, "issue_iter")
        (d / "iteration-v1").mkdir()
        (d / "iteration-v1" / "brief.md").write_text("# archived brief\n", encoding="utf-8")
        (d / "notes.json").write_text(
            json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
        )
        self.assertFalse(state.is_resolved(d))
        self.assertEqual(state.state(d), state.UNPLANNED)

    def test_briefless_with_any_downstream_artifact_is_not_resolved(self) -> None:
        # ANY artifact in DOWNSTREAM_OF_BRIEF — not just patch.diff — means a cycle touched
        # this bundle, so a resolved key cannot short-circuit it to terminal. Covers the
        # ones a hand-picked subset would miss (build-notes.md, check-review.md, …).
        for artifact in state.DOWNSTREAM_OF_BRIEF:
            d = _bundle(self.root, f"issue_stray_{artifact.replace('.', '_')}")
            (d / artifact).write_text("x\n", encoding="utf-8")
            (d / "notes.json").write_text(
                json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
            )
            self.assertFalse(
                state.is_resolved(d), f"{artifact} present must block RESOLVED"
            )
            self.assertEqual(state.state(d), state.UNPLANNED)

    def test_briefless_with_a_glob_matched_artifact_is_not_resolved(self) -> None:
        # The iterate archive moves `check-advisory-*.md` and `*.error.log` with the attempt
        # that produced them (driver._archive_iteration), so they are cycle evidence by the
        # same argument as DOWNSTREAM_OF_BRIEF: a damaged bundle retaining only one of them
        # is a cycle awaiting a re-plan, not a notes-only tracker.
        for artifact in ("check-advisory-adversary.md", "build.error.log"):
            d = _bundle(self.root, f"issue_glob_{artifact.replace('.', '_')}")
            (d / artifact).write_text("x\n", encoding="utf-8")
            (d / "notes.json").write_text(
                json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
            )
            self.assertFalse(
                state.is_resolved(d), f"{artifact} present must block RESOLVED"
            )
            self.assertEqual(state.state(d), state.UNPLANNED)

    def test_archive_globs_are_the_shared_source_of_truth(self) -> None:
        # Same rule as the list above, for the patterns: the archive reads
        # state.DOWNSTREAM_GLOBS rather than repeating the globs, so the set the archive
        # moves and the set is_resolved counts as evidence cannot drift apart.
        from pdca_harness import driver

        self.assertIs(driver.state.DOWNSTREAM_GLOBS, state.DOWNSTREAM_GLOBS)
        src = inspect.getsource(driver._archive_iteration)
        self.assertIn("state.DOWNSTREAM_GLOBS", src)
        self.assertNotIn('d.glob("check-advisory-*.md")', src)

    def test_briefless_with_only_an_accumulator_is_not_resolved(self) -> None:
        # Issue #170. `auto-iterate.json` and `deferred-findings.json` are kept OUT of the
        # two sets above so they survive the archive and accumulate across rebuilds — and
        # that exclusion also dropped them from the evidence guard. A bundle cannot hold
        # either without having run a cycle, so neither may read as a notes-only tracker.
        for artifact in state.CYCLE_EVIDENCE_ONLY:
            d = _bundle(self.root, f"issue_acc_{artifact.replace('.', '_')}")
            (d / artifact).write_text("{}\n", encoding="utf-8")
            (d / "notes.json").write_text(
                json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
            )
            self.assertFalse(
                state.is_resolved(d), f"{artifact} present must block RESOLVED"
            )
            self.assertEqual(state.state(d), state.UNPLANNED)

    def test_the_accumulators_are_evidence_but_never_archived(self) -> None:
        """The asymmetry IS the fix (#170), so pin it in both directions.

        Folding these names into DOWNSTREAM_OF_BRIEF would fix the misclassification and
        break something worse: the archive would move them, resetting auto-iterate's round
        budget every iterate (so it never terminates) and dropping the deferred findings
        into iteration-v<N>/ (the exact loss the ledger exists to prevent).
        """
        for artifact in state.CYCLE_EVIDENCE_ONLY:
            self.assertNotIn(artifact, state.DOWNSTREAM_OF_BRIEF, artifact)
            self.assertFalse(
                any(fnmatch.fnmatch(artifact, g) for g in state.DOWNSTREAM_GLOBS),
                f"{artifact} must not be swept up by an archive glob either",
            )

    def test_the_accumulator_names_match_their_owning_constants(self) -> None:
        # state.py cannot import autoiterate (it imports assemble, which cycles back), so the
        # names are literals. This is what makes a rename break loudly instead of silently
        # reopening the misclassification.
        from pdca_harness import autoiterate

        owned = {autoiterate.BUDGET_FILE}
        # DEFERRED_FILE arrives with #332; tolerate its absence so this fix does not depend
        # on that branch having merged.
        if hasattr(autoiterate, "DEFERRED_FILE"):
            owned.add(autoiterate.DEFERRED_FILE)
        self.assertTrue(
            owned <= set(state.CYCLE_EVIDENCE_ONLY),
            f"{owned - set(state.CYCLE_EVIDENCE_ONLY)} is an accumulator the guard misses",
        )

    def test_downstream_of_brief_is_the_shared_source_of_truth(self) -> None:
        # driver re-exports state's list — one source, so is_resolved and the iterate
        # archive can never diverge on "which files mean a cycle ran".
        from pdca_harness import driver

        self.assertIs(driver.DOWNSTREAM_OF_BRIEF, state.DOWNSTREAM_OF_BRIEF)

    def test_resolved_is_a_flow_terminal_state(self) -> None:
        # RESOLVED must count as terminal to the flow driver, else the batch sweep would
        # pick a resolved notes-tracker back up as "work in flight".
        from pdca_harness import flow

        self.assertIn(state.RESOLVED, flow._TERMINAL)

    def test_a_real_cycle_bundle_is_never_reclassified(self) -> None:
        # A briefed bundle with a stray `resolved` key falls through to the normal
        # transitions — the RESOLVED shortcut is scoped to briefless (notes-only) trackers.
        d = _bundle(self.root, "issue_996")
        (d / "brief.md").write_text(
            "# Brief — issue 996\n- Defect / goal: a real cycle\n", encoding="utf-8"
        )
        (d / "notes.json").write_text(
            json.dumps({"resolved": {"github_state": "closed"}}), encoding="utf-8"
        )
        self.assertNotEqual(state.state(d), state.RESOLVED)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
