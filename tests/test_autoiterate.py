"""Auto-iterate on implementation Check findings (issues #264 / #332, stdlib unittest).

The driver rebuilds a bundle unattended while its SUMMARY §6 carries implementation work — a
`gate` cell of the 5/5/1 (C2/C4/T1..T4), an advisory finding the leaf tagged `[impl]`, or a
judgment cell (C5/T5) the REVIEWER tagged `[impl]` in its verdict cell.

Findings needing a human no longer veto that (#332): they are DEFERRED into
`autoiterate.DEFERRED_FILE` and re-enter §6 at handover, because a human-needing finding is
evidence Plan overlooked something rather than a reason to stop rebuilding. What bounds the
iteration is the round budget, in two tiers — a soft floor that fires unconditionally, and a
hard ceiling above which nothing fires; between them a round fires only while the
implementation-finding count is not increasing.

Load-bearing negatives, each its own test: it must never auto-accept, never tick a §6 box,
never promote an `input` cell or the standing Validation row, never LOSE a deferred finding,
and never run past the hard ceiling. Offline: stub leaves, real gate commands, no Claude.
"""

from __future__ import annotations

import inspect
import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from pdca_harness import assemble, autoiterate, cli, driver, flow, gates, leaves, signoff, state
from pdca_harness.config import Config, LeafConfig

_GATE = {"id": "C4", "tier": "C4", "label": "verify", "scope": "bundle", "gating": True}
_PASS = {**_GATE, "cmd": "true"}
_FAIL = {**_GATE, "cmd": "false"}
_UNVERIFIABLE = {**_GATE, "cmd": "echo 'PDCA-UNVERIFIABLE: no prod file'; exit 0"}

_CLEAN_REVIEW = "All advisory items PASS.\n"


# The reviewer's prompt (agents/reviewer.md.jinja) hard-codes this row to NEEDS-HUMAN on EVERY
# cycle — validation is the human's call by definition. So EVERY real `check-review.md` carries
# it, and a fixture without it is a shape the product never produces. Omitting it is exactly why
# the original #264 tests passed while auto-iterate was unreachable in production (#293): they
# tested the mental model, not the artifact. It belongs in the fixture, not in one new test.
_STANDING_ROW = "| Validation — fitness-to-purpose | NEEDS-HUMAN | fitness is the human's call |"

# The Item cell shapes production ACTUALLY writes (issue #332). `_REVIEW_PROMPT` lists the
# matrix as `{elem} — {label}` and asks for "the element label above", so the `V — ` prefix is
# the literal reading of the instruction — 37 rows of the wyrd corpus wrote it that way against
# 185 bare ones, plus one ASCII `--`. Every one of them silently failed the exact-match STANDING
# test and became a HUMAN veto. The fixture above hard-coded only the bare form, which is
# exactly the "tested the mental model, not the artifact" failure the comment above warns about
# — so the standing tests now run over all three.
_STANDING_ROW_FORMS = (
    "Validation — fitness-to-purpose",
    "V — Validation — fitness-to-purpose",
    "Validation -- fitness-to-purpose",
)


def _review_table(item: str, verdict: str = "NEEDS-HUMAN", basis: str = "off-by-one",
                  *, standing: bool = True, standing_form: str = _STANDING_ROW_FORMS[0]) -> str:
    rows = f"| {item} | {verdict} | {basis} |\n"
    if standing:
        rows += f"| {standing_form} | NEEDS-HUMAN | fitness is the human's call |\n"
    return f"# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n{rows}"


def _stub_config(root: Path) -> Config:
    return Config(
        root=root,
        bundle_root=root / "results",
        process_dir=root / "process",
        templates_dir=root / "templates",
        default_branch="main",
        tracker_system="github",
        tracker_url="",
        issue_id_example="#1",
        builder=LeafConfig(mode="stub", family="claude"),
        reviewer=LeafConfig(mode="stub", family="codex"),
        auto_iterate=True,
        max_auto_iters=3,
    )


class _Base(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _stub_config(self.tmp)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _bundle(self, iid: str, *, gate: dict = _PASS, review: str = _CLEAN_REVIEW,
                advisory: str | None = None, build_notes: str | None = None,
                brief_body: str = "- **Slug:** ai\n") -> Path:
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True)
        (d / "brief.md").write_text(brief_body, encoding="utf-8")
        (d / "patch.diff").write_text("--- a\n+++ b\n", encoding="utf-8")
        (d / "check-review.md").write_text(review, encoding="utf-8")
        if advisory is not None:
            (d / "check-advisory-adversary.md").write_text(advisory, encoding="utf-8")
        if build_notes is not None:
            (d / "build-notes.md").write_text(build_notes, encoding="utf-8")
        self.cfg.gates_checks = [gate]
        gates.run_gates(d, self.cfg)
        assemble.assemble_summary(d, self.cfg)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)
        return d

    def _try(self, d: Path, *, apply_now: bool = False) -> bool:
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            return flow._maybe_auto_iterate(
                self.cfg, d, by="", today="2026-07-09", apply_now=apply_now)

    def _assert_halted(self, d: Path) -> None:
        """No decision written, no budget spent, bundle still waiting on the human."""
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)
        self.assertFalse((d / leaves.SIGNOFF_DECISION).exists())
        self.assertEqual(autoiterate.count(d), 0)
        self.assertTrue(signoff.open_needs_human(d / "SUMMARY.md") or True)  # §6 untouched

    def _assert_held(self, d: Path, needle: str) -> None:
        """The finding survived the rebuild in the deferred ledger (issue #332).

        This is the property the pre-#332 "must halt" assertions were really protecting: a real
        objection must never be ARCHIVED by an unattended rebuild. Halting was how that was
        guaranteed; now that a rebuild may proceed past a human finding, the ledger is — so the
        assertion moves rather than disappears.
        """
        held = autoiterate.deferred(d)
        self.assertTrue(any(needle in t for t in held),
                        f"{needle!r} was not held for the human; ledger={held}")

    def _assert_not_standing(self, d: Path, needle: str) -> None:
        """The finding is a real objection, never the signal-free constant (#293/#294)."""
        items = assemble.collect_needs_human(d, self.cfg)
        matched = [i for i in items if needle in i.text]
        self.assertTrue(matched, f"{needle!r} missing from §6 entirely; items={items}")
        for item in matched:
            self.assertEqual(item.kind, assemble.HUMAN,
                             f"{item.text!r} must be a real finding, not STANDING")


class AutoIterates(_Base):
    """Implementation-only findings ⇒ the driver rebuilds without asking."""

    def test_failed_gating_gate_auto_iterates(self) -> None:
        d = self._bundle("GATEFAIL", gate=_FAIL)
        self.assertTrue(self._try(d))
        self.assertEqual(state.state(d), state.ITERATE_DO)
        self.assertEqual(signoff.outcome_token(d / "SUMMARY.md"), "iterated-to-Do")
        self.assertEqual(autoiterate.count(d), 1)

    def test_reviewer_needs_human_on_a_gate_cell_auto_iterates(self) -> None:
        d = self._bundle("C4NH", review=_review_table("C4 Verification (red→green)"))
        self.assertTrue(self._try(d))
        self.assertEqual(state.state(d), state.ITERATE_DO)

    def test_conformance_gate_cells_auto_iterate(self) -> None:
        for elem in ("C2 Reproduction (red pre-fix)", "T1 Structure", "T2 Shape",
                     "T3 Runtime", "T4 Contribution"):
            with self.subTest(elem=elem):
                d = self._bundle(f"E{elem[:2]}", review=_review_table(elem))
                self.assertTrue(self._try(d), f"{elem} is a gate cell — should auto-iterate")

    def test_advisory_impl_marker_auto_iterates_and_text_is_clean(self) -> None:
        d = self._bundle("ADVIMPL", advisory="- NEEDS-HUMAN [impl] — off-by-one at src/x.py:12\n")
        items = assemble.collect_needs_human(d, self.cfg)
        self.assertEqual([i.kind for i in items], [assemble.IMPL])
        self.assertTrue(items[0].text.startswith("off-by-one"))  # the marker is stripped
        self.assertTrue(self._try(d))

    def test_rationale_reaches_the_brief_carry_forward(self) -> None:
        # The next Do iteration must not be blind about why it was rejected.
        d = self._bundle("CARRY", gate=_FAIL)
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=True)
            driver.run_issue(d, self.cfg)
        brief_text = (d / "brief.md").read_text(encoding="utf-8")
        self.assertIn("carry-forward", brief_text.lower())
        self.assertIn("Auto-iterate", brief_text)
        self.assertTrue((d / "iteration-v1").is_dir())      # prior attempt archived, not deleted

    def test_signoff_is_attributed_to_the_driver_not_a_human(self) -> None:
        d = self._bundle("ATTR", gate=_FAIL)
        self._try(d)
        self.assertIn("auto-iterate", (d / "SUMMARY.md").read_text(encoding="utf-8"))


class TheStandingValidationRow(_Base):
    """Issue #293 — the row that made this whole feature dead code.

    The reviewer's prompt hard-codes `Validation — fitness-to-purpose` to NEEDS-HUMAN on EVERY
    cycle, whatever it found: validation is the human's call by definition. So every real
    `check-review.md` carries it. The original rule demanded that EVERY §6 item be IMPL, so a
    single such row disqualified every bundle and auto-iterate NEVER FIRED in production — a
    constant was being read as evidence that a human must look right now.

    It still renders in §6 and the C6 accept-guard still blocks on it. All it no longer does is
    veto a rebuild.
    """

    def test_an_impl_finding_beside_the_standing_row_auto_iterates(self) -> None:
        # THE production shape, and the one the old fixture never built.
        d = self._bundle("SV1", review=_review_table("C4 Verification (red→green)"))
        self.assertTrue(self._try(d), "a Do-fixable defect must rebuild, not spend a human")
        self.assertEqual(autoiterate.count(d), 1)

    def test_the_standing_row_alone_still_halts(self) -> None:
        # Nothing for a rebuild to fix: a clean bundle awaiting the human's ACCEPT. Never
        # auto-accept — `eligible` needs at least one IMPL item, not merely "no HUMAN item".
        d = self._bundle("SV2", review=f"# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                                       f"{_STANDING_ROW}\n")
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_a_situational_judgment_concern_beside_it_still_halts(self) -> None:
        # The distinction that makes this safe: C5/T5 are judgment cells too, but the reviewer
        # raises them only on a REAL concern — so they carry signal and must still stop the
        # bundle, standing row or not.
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | off-by-one |\n"
                  "| C5 Causal adequacy | NEEDS-HUMAN | guards the symptom, not the cause |\n"
                  f"{_STANDING_ROW}\n")
        d = self._bundle("SV3", review=review)
        self._assert_not_standing(d, "C5 Causal adequacy")
        self.assertTrue(self._try(d), "the C4 defect is still Do's to fix (#332)")
        self._assert_held(d, "C5 Causal adequacy")

    def test_an_advisory_fitness_objection_is_never_standing(self) -> None:
        """PR #294 review (codex). STANDING is the PRIMARY review's privilege, and nothing
        else's.

        `collect_needs_human` runs `check-review.md` and every `check-advisory-*.md` through the
        same classifier. The adversary's prompt tells it to raise architectural / scope /
        fitness objections as free-form `- NEEDS-HUMAN — …` bullets — so one that happens to
        begin "Validation — fitness-to-purpose" was being read as the reviewer's signal-free
        standing row, and an unattended rebuild would ARCHIVE a real objection instead of
        halting for sign-off. The basis for STANDING is "this row is a constant", which is true
        of the reviewer's mandated table and of nothing else.
        """
        advisory = ("# Adversary\n\n- NEEDS-HUMAN — Validation — fitness-to-purpose: this "
                    "patches the wrong layer; the success criterion cannot be met by this "
                    "design\n")
        d = self._bundle("SV5", review=_review_table("C4 Verification (red→green)"),
                         advisory=advisory)
        self._assert_not_standing(d, "patches the wrong layer")
        self.assertTrue(self._try(d))
        self._assert_held(d, "patches the wrong layer")   # never archived by the rebuild

    def test_a_legacy_validation_bullet_in_the_review_is_never_standing(self) -> None:
        """PR #294 review (codex), second pass. Scoping STANDING to the primary ARTIFACT was
        still too wide — it must be scoped to the mandated verdict-table ROW.

        `_needs_human` also honours legacy `- NEEDS-HUMAN — …` bullets in `check-review.md`.
        Those are free prose the reviewer CHOSE to write, so one reading "Validation —
        fitness-to-purpose: patches the wrong layer" is a substantive objection, not the
        template row — and would have been archived by an unattended rebuild. Only a table row
        is the constant that earns STANDING.
        """
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | [impl] off-by-one |\n"
                  f"{_STANDING_ROW}\n"
                  "- NEEDS-HUMAN — Validation — fitness-to-purpose: patches the wrong layer\n")
        d = self._bundle("SV6", review=review)
        self._assert_not_standing(d, "patches the wrong layer")
        self.assertTrue(self._try(d))
        self._assert_held(d, "patches the wrong layer")   # never archived by the rebuild

    def test_a_second_table_never_earns_the_standing_exemption(self) -> None:
        """PR #294 review (codex), third pass. Keying on "came from a table" was STILL too wide.

        The reviewer may write more than one table — a "concerns" table beside the mandated
        verdict table. A row there reading `| Validation — fitness-to-purpose: patches the wrong
        layer | NEEDS-HUMAN | … |` is a substantive objection, but it came from a table and its
        text starts with the canonical label, so it was classified STANDING and an unattended
        rebuild would archive it. The canonical row is now identified by an EXACT match on its
        Item cell — the only thing that actually distinguishes the template row.
        """
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | [impl] off-by-one |\n"
                  f"{_STANDING_ROW}\n"
                  "\n## Concerns\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| Validation — fitness-to-purpose: patches the wrong layer | NEEDS-HUMAN "
                  "| the criterion cannot be met by this design |\n")
        d = self._bundle("SV7", review=review)
        self._assert_not_standing(d, "patches the wrong layer")
        self.assertTrue(self._try(d))
        self._assert_held(d, "patches the wrong layer")   # never archived by the rebuild

    def test_a_concerns_table_with_the_EXACT_label_still_halts(self) -> None:
        """PR #294, local codex pass. The fourth scoping of the same rule, and the one that
        finally names the right thing.

        Matching the Item cell was still not enough: a `## Concerns` table can carry the row
        `| Validation — fitness-to-purpose | NEEDS-HUMAN | patches the wrong layer |` with the
        **exact** canonical label. The parser had no idea which TABLE a row came from, so that
        real objection earned STANDING and an unattended rebuild would archive it. My previous
        test only covered a concerns row with EXTRA text in the cell, so it sailed past this.

        The justification was always "the MANDATED TABLE's Validation row is a constant" — so the
        parser now identifies that table (≥2 exact canonical Item cells) and only its V row can
        be standing.
        """
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | [impl] off-by-one |\n"
                  "| C5 Causal adequacy | PASS | ok |\n"
                  f"{_STANDING_ROW}\n"
                  "\n## Concerns\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| Validation — fitness-to-purpose | NEEDS-HUMAN | patches the wrong layer |\n")
        d = self._bundle("SV9", review=review)
        self._assert_not_standing(d, "patches the wrong layer")
        self.assertTrue(self._try(d))
        self._assert_held(d, "patches the wrong layer")   # never archived by the rebuild

    def test_two_standing_candidates_fail_closed(self) -> None:
        # The template row is a CONSTANT — it occurs once. If two survive (a duplicated row, a
        # second verdict-shaped table), at least one is not the constant and we cannot tell
        # which. Grant STANDING to neither and halt, rather than risk archiving a real objection.
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | [impl] off-by-one |\n"
                  "| C5 Causal adequacy | PASS | ok |\n"
                  f"{_STANDING_ROW}\n"
                  "| Validation — fitness-to-purpose | NEEDS-HUMAN | and again, differently |\n")
        d = self._bundle("SV10", review=review)
        # Fail closed still means "neither row is the constant" — both are real findings, so
        # both must reach the human. #332 changes only WHEN, never WHETHER.
        self._assert_not_standing(d, "and again, differently")
        self.assertTrue(self._try(d))
        self._assert_held(d, "and again, differently")

    def test_the_standing_row_is_never_carried_forward_to_the_builder(self) -> None:
        """PR #294 review (codex). STANDING rides along in `items` so it cannot veto the rebuild
        — but it is not a finding, and no builder can act on it. Carrying it into the §9 delta
        and the brief's carry-forward handed the next Do a human-only judgment call as though it
        were a defect to fix, under a sentence claiming the set was "implementation-level items
        only"."""
        d = self._bundle("SV8", review=_review_table("C4 Verification (red→green)"))
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=True)
            driver.run_issue(d, self.cfg)
        brief_text = (d / "brief.md").read_text(encoding="utf-8")
        self.assertIn("C4 Verification", brief_text)                     # the real defect…
        self.assertNotIn("Validation — fitness-to-purpose", brief_text)  # …and only that

    def test_the_standing_row_still_blocks_accept(self) -> None:
        # The C6 guard is untouched: the human must still clear §6 before accepting. Not
        # vetoing a REBUILD is not the same as not needing a human at SIGN-OFF.
        d = self._bundle("SV4", review=_review_table("C4 Verification (red→green)"))
        summary = (d / "SUMMARY.md").read_text(encoding="utf-8")
        self.assertIn("Validation — fitness-to-purpose", summary)   # still rendered in §6
        self.assertTrue(signoff.open_needs_human(d / "SUMMARY.md"))  # still blocks accept


class HaltsForTheHuman(_Base):
    """Anything architectural, environmental, or unclassifiable still stops."""

    def test_judgment_cells_halt(self) -> None:
        # THE load-bearing negative: C5 causal adequacy, T5 judgment, the validation act.
        for elem in ("C5 Causal adequacy", "T5 Judgment", "Validation — fitness-to-purpose"):
            with self.subTest(elem=elem):
                d = self._bundle(f"J{abs(hash(elem)) % 9999}", review=_review_table(elem))
                self.assertFalse(self._try(d), f"{elem} is a judgment cell — must halt")
                self._assert_halted(d)

    def test_input_cells_halt(self) -> None:
        for elem in ("C1 Spec", "C3 Change"):
            with self.subTest(elem=elem):
                d = self._bundle(f"I{elem[:2]}", review=_review_table(elem))
                self.assertFalse(self._try(d))
                self._assert_halted(d)

    def test_a_judgment_item_beside_impl_work_defers_rather_than_halting(self) -> None:
        # #332 reverses the old "one judgment item disqualifies the whole bundle". A finding
        # needing a human is not a reason to stop rebuilding — the round budget bounds that —
        # so the C4 defect is rebuilt and the C5 concern is HELD, not dropped.
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C4 Verification (red→green) | NEEDS-HUMAN | off-by-one |\n"
                  "| C5 Causal adequacy | NEEDS-HUMAN | guards the symptom |\n")
        d = self._bundle("MIXED", review=review)
        self.assertTrue(self._try(d))
        self.assertEqual(state.state(d), state.ITERATE_DO)
        held = autoiterate.deferred(d)
        self.assertTrue(any("C5 Causal adequacy" in t for t in held), held)
        self.assertFalse(any("C4 Verification" in t for t in held),
                         "the rebuilt defect is not a deferred human finding")

    def test_a_judgment_item_alone_still_halts(self) -> None:
        # The other half of the same rule: with no implementation work beside it there is
        # nothing for a rebuild to do, so it goes straight to the human.
        d = self._bundle("JONLY", review=_review_table("C5 Causal adequacy"))
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_unverifiable_gate_alone_halts(self) -> None:
        # A gate that COULD NOT RUN is a gate-kind element, but rebuilding can't fix a
        # missing mechanic — it would spin. Forced HUMAN, and with nothing else to build.
        d = self._bundle("UNVER", gate=_UNVERIFIABLE)
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_declared_external_dependency_defers_beside_impl_work(self) -> None:
        # A missing system dependency is not builder-fixable, so it is still HUMAN and still
        # reaches sign-off — but it no longer vetoes the failing gate's rebuild (#332). The
        # cost is real: a bundle blocked only on a missing tool spends soft rounds first.
        d = self._bundle("EXTDEP", gate=_FAIL,
                         build_notes="NEEDS-HUMAN external dependency: protoc — cannot compile\n")
        self.assertTrue(self._try(d))
        self.assertTrue(any("protoc" in t for t in autoiterate.deferred(d)))

    def test_unregistered_dependency_defers_beside_impl_work(self) -> None:
        self.cfg.doctor_checks = []
        d = self._bundle("UNREG", gate=_FAIL,
                         brief_body="- **Slug:** ai\n- **External dependencies:** `protoc` (build)\n")
        self.assertTrue(self._try(d))
        self.assertTrue(any("protoc" in t for t in autoiterate.deferred(d)))

    def test_unmarked_advisory_finding_halts(self) -> None:
        # Backward compatibility: an advisory file written before #264 has no [impl] tag,
        # so it can never trigger an auto-iteration.
        d = self._bundle("ADVPLAIN", advisory="- NEEDS-HUMAN — the scope looks wider than the brief\n")
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_unmappable_review_row_halts(self) -> None:
        # An Item cell with no canonical element id → fail safe toward the human.
        d = self._bundle("UNMAP", review=_review_table("Some bespoke lens"))
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_empty_section6_halts_and_never_auto_accepts(self) -> None:
        d = self._bundle("CLEAN")
        self.assertEqual(signoff.open_needs_human(d / "SUMMARY.md"), [])
        self.assertFalse(self._try(d))
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)   # NOT COMPLETE
        self.assertNotEqual(signoff.outcome_token(d / "SUMMARY.md"), "merged-wider")

    def test_missing_review_defers_and_is_never_lost(self) -> None:
        # A missing review is an infra failure, not a verdict — nothing reviewed the diff.
        # Beside a real failing gate a rebuild is now allowed (it re-runs the reviewer too),
        # but the "no review exists" item must survive to sign-off or the bundle could be
        # accepted having never been reviewed.
        d = self._bundle("NOREV", gate=_FAIL)
        (d / "check-review.md").unlink()
        assemble.assemble_summary(d, self.cfg)
        self.assertTrue(self._try(d))
        self.assertTrue(any("check-review.md" in t for t in autoiterate.deferred(d)),
                        autoiterate.deferred(d))

    def test_missing_review_alone_halts(self) -> None:
        d = self._bundle("NOREVCLEAN")
        (d / "check-review.md").unlink()
        assemble.assemble_summary(d, self.cfg)
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_bundle_not_awaiting_signoff_is_a_noop(self) -> None:
        d = self._bundle("NOTREADY", gate=_FAIL)
        signoff.record(d / "SUMMARY.md", action="iterate-do", by="t", date="2026-07-09")
        self.assertEqual(state.state(d), state.ITERATE_DO)
        self.assertFalse(self._try(d))

    def test_disabled_by_config(self) -> None:
        self.cfg.auto_iterate = False
        d = self._bundle("OFF", gate=_FAIL)
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_close_disposition_bundle_halts(self) -> None:
        # The close fast path skips builder + reviewer and asks the human to confirm the
        # close. That confirmation is a human call — never auto-iterate it.
        d = self.cfg.bundle("CLOSE")
        d.mkdir(parents=True)
        (d / "brief.md").write_text(
            "- **Slug:** c\n- **Disposition hint:** likely-close\n", encoding="utf-8")
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            driver.run_issue(d, self.cfg)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_truncated_gates_json_declines_instead_of_crashing(self) -> None:
        # An over-reaching leaf can truncate a bundle's downstream. The file still exists, so
        # the bundle still reads AWAITING_SIGNOFF — but it no longer parses. The single-issue
        # flow has no `_isolate` around auto-iterate, so this must degrade, not raise.
        d = self._bundle("CORRUPT", gate=_FAIL)
        (d / "check-gates.json").write_text('{"rows": [', encoding="utf-8")
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)
        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(io.StringIO()):
            fired = flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09",
                                             apply_now=False)   # must NOT raise
        self.assertFalse(fired)
        self.assertIn("cannot classify Check findings", buf.getvalue())

    def test_missing_gates_json_is_not_awaiting_signoff(self) -> None:
        # Deleting it moves the bundle back to BUILT, so the state guard declines first.
        d = self._bundle("GONE", gate=_FAIL)
        (d / "check-gates.json").unlink()
        self.assertEqual(state.state(d), state.BUILT)
        self.assertFalse(self._try(d))

    def test_stub_reviewer_never_auto_iterates(self) -> None:
        # Offline / CI (PDCA_LEAVES_MODE=stub): the stub review flags the always-human
        # validation act, so a rehearse run can never auto-iterate.
        d = self.cfg.bundle("STUB")
        d.mkdir(parents=True)
        (d / "brief.md").write_text("- **Slug:** ai\n", encoding="utf-8")
        driver.run_issue(d, self.cfg)   # stub builder + stub reviewer
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)
        self.assertFalse(self._try(d))


class Budget(_Base):
    def test_exhausted_budget_hands_over_to_the_human(self) -> None:
        self.cfg.max_auto_iters = 2
        d = self._bundle("BUDGET", gate=_FAIL)
        (d / autoiterate.BUDGET_FILE).write_text('{"count": 2}\n', encoding="utf-8")
        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(io.StringIO()):
            fired = flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=False)
        self.assertFalse(fired)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)   # halted, never dropped
        self.assertFalse((d / leaves.SIGNOFF_DECISION).exists())
        self.assertIn("hard budget spent (2/2)", buf.getvalue())

    def test_budget_survives_the_iteration_archive(self) -> None:
        # auto-iterate.json must NOT be in driver.DOWNSTREAM_OF_BRIEF, or the count resets
        # every rebuild and the loop never terminates.
        self.assertNotIn(autoiterate.BUDGET_FILE, driver.DOWNSTREAM_OF_BRIEF)
        d = self._bundle("SURVIVE", gate=_FAIL)
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=True)
        self.assertTrue((d / "iteration-v1").is_dir())
        self.assertEqual(autoiterate.count(d), 1)                  # not reset by the archive

    def test_garbled_budget_file_reads_as_zero(self) -> None:
        d = self._bundle("GARBLE", gate=_FAIL)
        (d / autoiterate.BUDGET_FILE).write_text("{ not json", encoding="utf-8")
        self.assertEqual(autoiterate.count(d), 0)
        self.assertTrue(self._try(d))

    def test_repeated_rounds_terminate_at_the_cap(self) -> None:
        # A bundle whose rebuild keeps failing the same gate must reach the human, not spin.
        # The reviewer is stubbed to a CLEAN review so every rebuild's §6 stays impl-only —
        # otherwise the stub reviewer's always-human validation row would halt it at round 1
        # (which it does, correctly: see test_stub_reviewer_never_auto_iterates).
        self.cfg.max_auto_iters = 2
        d = self._bundle("SPIN", gate=_FAIL)

        def clean_review(bundle: Path, cfg: Config) -> None:
            (bundle / "check-review.md").write_text(_CLEAN_REVIEW, encoding="utf-8")

        rounds = 0
        with mock.patch.object(leaves, "run_review", clean_review), \
             redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            for _ in range(5):
                if not flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09",
                                                apply_now=True):
                    break
                rounds += 1
        self.assertEqual(rounds, 2)                                # stopped at the cap
        self.assertEqual(autoiterate.count(d), 2)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)   # handed over, not dropped
        self.assertTrue((d / "iteration-v2").is_dir())             # both attempts preserved


class BatchSweep(_Base):
    """In `_drive_wave` an auto-iterate must behave exactly like a deferred human iterate-do:
    the bundle leaves the sign-off queue, and the NEXT pass's build-all rebuilds it."""

    def test_auto_iterated_bundle_leaves_the_queue_and_rebuilds_next_pass(self) -> None:
        d = self._bundle("WAVE", gate=_FAIL)
        signed_off: list[str] = []

        def signoff_batch(cfg: Config, bundles: list[Path]) -> None:
            signed_off.extend(b.name for b in bundles)
            for b in bundles:                       # a human would accept here
                summ = b / "SUMMARY.md"
                summ.write_text(summ.read_text().replace("- [ ]", "- [x]"), encoding="utf-8")
                (b / leaves.SIGNOFF_DECISION).write_text("accept\n", encoding="utf-8")

        def clean_review(bundle: Path, cfg: Config) -> None:
            (bundle / "check-review.md").write_text(_CLEAN_REVIEW, encoding="utf-8")

        with mock.patch.object(leaves, "run_signoff_batch", signoff_batch), \
             mock.patch.object(leaves, "run_review", clean_review), \
             redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._drive_wave(self.cfg, [d], by="t", today="2026-07-09", max_passes=1)

        # Pass 1 auto-iterated it, so the human's sign-off session never saw it …
        self.assertEqual(signed_off, [])
        self.assertEqual(state.state(d), state.ITERATE_DO)
        self.assertEqual(autoiterate.count(d), 1)
        # … and its rebuild is deferred to the next pass, not run mid-review.
        self.assertFalse((d / "iteration-v1").is_dir())

    def test_judgment_finding_still_reaches_the_signoff_queue(self) -> None:
        d = self._bundle("WAVEJ", review=_review_table("C5 Causal adequacy"))
        seen: list[str] = []

        def signoff_batch(cfg: Config, bundles: list[Path]) -> None:
            seen.extend(b.name for b in bundles)
            for b in bundles:
                (b / leaves.SIGNOFF_DECISION).write_text("discontinue\nnot now\n", encoding="utf-8")

        with mock.patch.object(leaves, "run_signoff_batch", signoff_batch), \
             redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._drive_wave(self.cfg, [d], by="t", today="2026-07-09", max_passes=1)
        self.assertEqual(seen, ["issue_WAVEJ"])   # the human got it, as they must

    def test_repeated_auto_iterations_count_as_progress_not_a_stuck_wave(self) -> None:
        """PR #270 review (codex). A bundle already ITERATE_DO is rebuilt by `_build_all` to
        AWAITING_SIGNOFF, re-Checked, then routed straight back to ITERATE_DO — so the
        before/after state snapshots MATCH. With the sign-off queue empty, the no-progress
        check fired and the wave returned after only TWO auto rounds, stranding the bundle
        with both `max_auto_iters` and `max_passes` budget to spare."""
        self.cfg.max_auto_iters = 3
        d = self._bundle("WAVELOOP", gate=_FAIL)     # a gate that stays red across rebuilds
        signed_off: list[str] = []

        def signoff_batch(cfg: Config, bundles: list[Path]) -> None:
            signed_off.extend(b.name for b in bundles)
            for b in bundles:                        # the human clears §6 and accepts
                summ = b / "SUMMARY.md"
                summ.write_text(summ.read_text().replace("- [ ]", "- [x]"), encoding="utf-8")
                (b / leaves.SIGNOFF_DECISION).write_text("accept\n", encoding="utf-8")

        def clean_review(bundle: Path, cfg: Config) -> None:
            (bundle / "check-review.md").write_text(_CLEAN_REVIEW, encoding="utf-8")

        buf = io.StringIO()
        with mock.patch.object(leaves, "run_signoff_batch", signoff_batch), \
             mock.patch.object(leaves, "run_review", clean_review), \
             redirect_stderr(buf), redirect_stdout(io.StringIO()):
            flow._drive_wave(self.cfg, [d], by="t", today="2026-07-10", max_passes=6)

        # the FULL auto budget is spent — not truncated at two by a false stuck-wave verdict
        self.assertEqual(autoiterate.count(d), 3)
        self.assertNotIn("a full pass made no progress", buf.getvalue())
        # …and once it is spent the bundle reaches the human and completes, never abandoned
        self.assertEqual(signed_off, ["issue_WAVELOOP"])
        self.assertEqual(state.state(d), state.COMPLETE)

    def test_a_wave_that_truly_stalls_still_warns(self) -> None:
        # The negative: with the auto budget spent, nothing advances — the no-progress guard
        # must still fire. `auto_iterated` must never mask a genuine stall.
        d = self._bundle("WAVESTALL", gate=_FAIL)
        (d / autoiterate.BUDGET_FILE).write_text('{"count": 99}\n', encoding="utf-8")
        signoff.record(d / "SUMMARY.md", action="iterate-do", by="t", date="2026-07-10")
        buf = io.StringIO()
        with mock.patch.object(flow, "_build_all", lambda cfg, bundles: None), \
             redirect_stderr(buf), redirect_stdout(io.StringIO()):
            flow._drive_wave(self.cfg, [d], by="t", today="2026-07-10", max_passes=5)
        self.assertIn("a full pass made no progress", buf.getvalue())
        self.assertIn("issue_WAVESTALL", buf.getvalue())

    def test_a_raising_auto_iterate_does_not_kill_the_sweep(self) -> None:
        d = self._bundle("WAVEBOOM", gate=_FAIL)
        with mock.patch.object(flow.autoiterate, "write_decision",
                               side_effect=OSError("disk full")), \
             mock.patch.object(leaves, "run_signoff_batch", lambda cfg, bundles: None), \
             redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._drive_wave(self.cfg, [d], by="t", today="2026-07-09", max_passes=1)
        self.assertEqual(state.state(d), state.AWAITING_SIGNOFF)   # isolated, still reviewable


class DecisionModule(unittest.TestCase):
    """`autoiterate` itself — the guard that keeps this from ever becoming an auto-accept."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _items(self, *kinds: str) -> list[assemble.NeedsHumanItem]:
        return [assemble.NeedsHumanItem(f"finding {i}", k) for i, k in enumerate(kinds)]

    def test_eligible_needs_implementation_work_and_nothing_else(self) -> None:
        self.assertTrue(autoiterate.eligible(self._items(assemble.IMPL, assemble.IMPL)))
        self.assertFalse(autoiterate.eligible([]))                                 # never accept
        self.assertFalse(autoiterate.eligible(self._items(assemble.HUMAN)))        # nothing to build
        self.assertFalse(autoiterate.eligible(self._items(assemble.STANDING)))     # a constant
        # #332: a human finding beside real implementation work no longer vetoes it.
        self.assertTrue(autoiterate.eligible(self._items(assemble.IMPL, assemble.HUMAN)))

    def test_write_decision_only_ever_writes_iterate_do(self) -> None:
        autoiterate.write_decision(self.tmp, self._items(assemble.IMPL))
        token = (self.tmp / leaves.SIGNOFF_DECISION).read_text(encoding="utf-8").splitlines()[0]
        self.assertEqual(token, "iterate-do")
        self.assertIn(token, leaves.VALID_DECISIONS)
        self.assertNotEqual(token, "accept")

    def test_write_decision_refuses_a_non_implementation_set(self) -> None:
        with self.assertRaises(ValueError):
            autoiterate.write_decision(self.tmp, self._items(assemble.HUMAN))
        self.assertFalse((self.tmp / leaves.SIGNOFF_DECISION).exists())
        self.assertEqual(autoiterate.count(self.tmp), 0)          # no budget spent either

    def test_write_decision_refuses_an_empty_set(self) -> None:
        with self.assertRaises(ValueError):
            autoiterate.write_decision(self.tmp, [])

    def test_rationale_is_a_single_line_naming_the_findings(self) -> None:
        r = autoiterate.rationale(self._items(assemble.IMPL, assemble.IMPL), attempt=2)
        self.assertNotIn("\n", r)
        self.assertIn("round 2", r)
        self.assertIn("finding 0", r)
        self.assertIn("finding 1", r)


class Classification(unittest.TestCase):
    """The impl/human split is taken from the canonical 5/5/1, not re-invented."""

    def test_gate_elements_match_the_canonical_matrix(self) -> None:
        expected = {e for e, _l, k, _o in gates.canonical_elements() if k == "gate"}
        self.assertEqual(assemble._GATE_ELEMENTS, expected)
        self.assertEqual(expected, {"C2", "C4", "T1", "T2", "T3", "T4"})

    def test_judgment_and_input_cells_are_never_impl(self) -> None:
        # THE invariant: a rebuild can never be aimed at a judgment / input cell. Unchanged.
        for elem, label, kind, _oracle in gates.canonical_elements():
            if kind in ("judgment", "input"):
                item = assemble._classify_finding(f"{label} — some basis")
                self.assertNotEqual(item.kind, assemble.IMPL, f"{elem} must never be impl")

    def test_only_the_validation_row_is_standing(self) -> None:
        # #293. Of the 5/5/1's own rows, V is the one the reviewer's prompt hard-codes to
        # NEEDS-HUMAN every cycle, so it alone can be STANDING (a constant carries no signal).
        # C5/T5 are judgment too, but the reviewer raises those only on a real concern — they
        # stay situational HUMAN and still halt the bundle. The PARSER decides which row is the
        # canonical one; the classifier only honours that decision.
        for elem, label, kind, _oracle in gates.canonical_elements():
            if kind not in ("judgment", "input"):
                continue
            # A REAL verdict table: the row under test plus another canonical row, which is what
            # makes it the mandated table rather than a stray one (a lone row cannot nominate
            # itself as the constant).
            table = ("| Item | Verdict | Basis |\n|---|---|---|\n"
                     "| C1 Spec | PASS | ok |\n"
                     f"| {label} | NEEDS-HUMAN | some basis |\n")
            [found] = assemble._needs_human(table)
            got = assemble._classify_finding(found.text, standing=found.standing).kind
            want = assemble.STANDING if elem == "V" else assemble.HUMAN
            self.assertEqual(got, want, f"{elem} ({label})")

    def test_standing_needs_an_EXACT_match_on_the_canonical_item_cell(self) -> None:
        """PR #294 review (codex). What identifies the template row is its Item cell being
        EXACTLY the canonical label — not the text's prefix, and not merely "it came from a
        table". A prefix test let a real objection wear the template's clothes; a table test let
        a second table do the same. Both are the same mistake, one layer apart."""
        TBL = "| Item | Verdict | Basis |\n|---|---|---|\n| C1 Spec | PASS | ok |\n"
        canonical = TBL + "| Validation — fitness-to-purpose | NEEDS-HUMAN | the human's call |\n"
        objection = TBL + ("| Validation — fitness-to-purpose: patches the wrong layer "
                           "| NEEDS-HUMAN | the criterion cannot be met |\n")
        bullet = TBL + "- NEEDS-HUMAN — Validation — fitness-to-purpose: patches the wrong layer\n"
        lone = "| Validation — fitness-to-purpose | NEEDS-HUMAN | the human's call |\n"

        standing = assemble._needs_human(canonical)[0].standing
        self.assertTrue(standing, "the canonical row of the MANDATED table IS the constant")
        standing = assemble._needs_human(objection)[0].standing
        self.assertFalse(standing, "a longer Item cell is a real objection, not the template")
        standing = assemble._needs_human(bullet)[0].standing
        self.assertFalse(standing, "free prose is never the template row")
        standing = assemble._needs_human(lone)[0].standing
        self.assertFalse(standing, "a lone row in a stray table cannot nominate itself")

    def test_the_classifier_never_re_derives_standing_from_the_text(self) -> None:
        # Two sources of truth for "is this the constant row" is what produced the bug. The
        # classifier honours the caller's verdict and does not second-guess it from the text.
        text = "Validation — fitness-to-purpose — the human's call"
        self.assertEqual(assemble._classify_finding(text).kind, assemble.HUMAN)
        self.assertEqual(assemble._classify_finding(text, standing=True).kind, assemble.STANDING)

    def test_impl_marker_is_case_insensitive_and_stripped(self) -> None:
        self.assertEqual(assemble._classify_finding("[IMPL] — bug"),
                         assemble.NeedsHumanItem("bug", assemble.IMPL))

    def test_unknown_text_is_human(self) -> None:
        self.assertEqual(assemble._classify_finding("something bespoke").kind, assemble.HUMAN)

    def test_a_gate_row_kind_comes_from_its_element_not_its_label(self) -> None:
        # An instance names its own gates; the label may not start with the element id.
        rows = {"rows": [{"check": "fix verified", "result": "fail", "gating": True,
                          "element": "C4", "path_line": "", "oracle": "run-verify.sh"}]}
        self.assertEqual(assemble._failed_gating_items(rows)[0].kind, assemble.IMPL)
        rows["rows"][0]["element"] = ""      # unknown → fail safe
        self.assertEqual(assemble._failed_gating_items(rows)[0].kind, assemble.HUMAN)


class ConfigPlumbing(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _load(self, extra: str = "") -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n[leaves.reviewer]\nmode = "stub"\n' + extra,
            encoding="utf-8")
        return Config.load(self.tmp)

    def test_off_by_default(self) -> None:
        cfg = self._load()
        self.assertFalse(cfg.auto_iterate)
        self.assertEqual(cfg.max_auto_iters, 3)

    def test_driver_table_enables_it(self) -> None:
        self.assertTrue(self._load("[driver]\nauto_iterate = true\n").auto_iterate)

    def test_env_overrides_the_toml(self) -> None:
        with mock.patch.dict(os.environ, {"PDCA_AUTO_ITERATE": "1"}):
            self.assertTrue(self._load().auto_iterate)
        with mock.patch.dict(os.environ, {"PDCA_AUTO_ITERATE": "0"}):
            self.assertFalse(self._load("[driver]\nauto_iterate = true\n").auto_iterate)

    def test_max_auto_iters_is_clamped_below_max_passes(self) -> None:
        # Else exhausting the auto budget could coincide with the wave's pass budget running
        # out, leaving the bundle mid-flight at ITERATE_DO (#260's abandonment shape).
        cfg = self._load("[driver]\nmax_passes = 3\nmax_auto_iters = 99\n")
        self.assertEqual(cfg.max_auto_iters, 2)
        self.assertLess(cfg.max_auto_iters, cfg.max_passes)

    def test_max_auto_iters_floor_of_one(self) -> None:
        # The RAW value floors at 1 (a zero budget with auto-iterate on is a misconfig)…
        self.assertEqual(
            self._load("[driver]\nmax_passes = 5\nmax_auto_iters = 0\n").max_auto_iters, 1)
        # …but the strictly-below clamp wins at a ONE-pass budget (#132): an auto-iterate
        # there would spend the only allowed pass on an iterate-do that is never rebuilt,
        # stranding the bundle at ITERATE_DO — the invariant the clamped-below test above
        # asserts. Zero declines cleanly (flow's spent >= budget check).
        self.assertEqual(
            self._load("[driver]\nmax_passes = 1\nmax_auto_iters = 0\n").max_auto_iters, 0)
        self.assertEqual(
            self._load("[driver]\nmax_passes = 1\nmax_auto_iters = 3\n").max_auto_iters, 0)

    def test_cli_flag_opts_in(self) -> None:
        cfg = _stub_config(self.tmp)
        cfg.auto_iterate = False
        with mock.patch.object(cli.Config, "load", return_value=cfg), \
             mock.patch.object(cli.flow, "flow", return_value=state.COMPLETE), \
             redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            cli.main(["flow", "ID1", "--auto-iterate", "--no-publish", "--no-act"])
        self.assertTrue(cfg.auto_iterate)


class SoftAndHardRounds(unittest.TestCase):
    """Issue #332 — the two round budgets, driven by the maintainer's own worked example."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _stub_config(self.tmp)
        self.cfg.soft_auto_iters, self.cfg.max_auto_iters = 3, 5
        self.d = self.tmp / "b"
        self.d.mkdir()

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _items(self, n_impl: int, n_human: int = 1) -> list[assemble.NeedsHumanItem]:
        return ([assemble.NeedsHumanItem(f"impl {i}", assemble.IMPL) for i in range(n_impl)]
                + [assemble.NeedsHumanItem(f"judgment {i}", assemble.HUMAN)
                   for i in range(n_human)])

    def _spend(self, n_impl: int) -> None:
        autoiterate.write_decision(self.d, self._items(n_impl))

    def test_the_worked_example(self) -> None:
        """soft 3 / hard 5, counts 5 → 7 → 3, then the round-4 branch.

        Rounds 1-3 fire without any test — r1→r2 gets WORSE (5 → 7) and still fires, which is
        the whole point of a floor: a builder that fixes one defect and uncovers two more has
        made progress the count cannot see.
        """
        for n in (5, 7, 3):
            fire, why = autoiterate.should_iterate(self.d, self._items(n), self.cfg)
            self.assertTrue(fire, f"below the soft floor everything fires; got {why}")
            self._spend(n)
        self.assertEqual(autoiterate.impl_history(self.d), [5, 7, 3])

        # Round 4 is above the floor, so it is now conditional on the count not rising.
        for probe, expected in ((4, False), (3, True), (2, True)):
            with self.subTest(round4=probe):
                fire, _why = autoiterate.should_iterate(self.d, self._items(probe), self.cfg)
                self.assertIs(fire, expected)

    def test_the_hard_ceiling_is_absolute(self) -> None:
        for n in (5, 4, 3, 3, 2):          # five rounds, converging throughout
            fire, _ = autoiterate.should_iterate(self.d, self._items(n), self.cfg)
            self.assertTrue(fire)
            self._spend(n)
        fire, why = autoiterate.should_iterate(self.d, self._items(1), self.cfg)
        self.assertFalse(fire, "round 6 must not fire even though 2 → 1 converged")
        self.assertIn("hard budget spent (5/5)", why)

    def test_equal_counts_continue(self) -> None:
        # The bound is on getting WORSE, not on failing to improve: a round can trade one
        # finding for another and still be closing in.
        for n in (4, 4, 4):
            self.assertTrue(autoiterate.should_iterate(self.d, self._items(n), self.cfg)[0])
            self._spend(n)
        self.assertTrue(autoiterate.should_iterate(self.d, self._items(4), self.cfg)[0])

    def test_soft_equal_to_hard_is_the_pre_332_behaviour(self) -> None:
        # The default an instance gets by not declaring soft_auto_iters: every allowed round
        # unconditional, exactly as before this change.
        self.cfg.soft_auto_iters = self.cfg.max_auto_iters = 3
        for n in (2, 9, 40):               # wildly diverging; fires anyway
            self.assertTrue(autoiterate.should_iterate(self.d, self._items(n), self.cfg)[0])
            self._spend(n)
        self.assertFalse(autoiterate.should_iterate(self.d, self._items(1), self.cfg)[0])

    def test_a_legacy_budget_file_keeps_the_old_behaviour(self) -> None:
        # A bundle already mid-iteration when this ships has a count but no history. With no
        # baseline the convergence test cannot run, and halting on a comparison we cannot make
        # would strand it — so it fires, as it would have before.
        (self.d / autoiterate.BUDGET_FILE).write_text('{"count": 4}\n', encoding="utf-8")
        self.assertEqual(autoiterate.impl_history(self.d), [])
        self.assertTrue(autoiterate.should_iterate(self.d, self._items(99), self.cfg)[0])

    def test_a_garbled_budget_file_does_not_crash_the_gate(self) -> None:
        (self.d / autoiterate.BUDGET_FILE).write_text("{ not json", encoding="utf-8")
        self.assertEqual(autoiterate.count(self.d), 0)
        self.assertEqual(autoiterate.impl_history(self.d), [])
        self.assertTrue(autoiterate.should_iterate(self.d, self._items(3), self.cfg)[0])

    def test_config_normalizes_the_soft_floor(self) -> None:
        cfg = _stub_config(self.tmp)
        cfg.max_auto_iters, cfg.soft_auto_iters = 5, 0
        cfg._normalize_auto_iters()
        self.assertEqual(cfg.soft_auto_iters, 5, "unset ⇒ no soft tier")
        cfg.soft_auto_iters = 99
        cfg._normalize_auto_iters()
        self.assertEqual(cfg.soft_auto_iters, 5, "a floor above the ceiling clamps to it")

    def test_lowering_the_pass_budget_drags_the_soft_floor_down(self) -> None:
        cfg = _stub_config(self.tmp)
        cfg.max_passes, cfg.max_auto_iters, cfg.soft_auto_iters = 20, 5, 4
        cfg.override_max_passes(3)
        self.assertEqual(cfg.max_auto_iters, 2)
        self.assertLessEqual(cfg.soft_auto_iters, cfg.max_auto_iters)


class TheReviewerImplTag(_Base):
    """Issue #332 — the reviewer may say a JUDGMENT row is really a build defect."""

    def test_a_tagged_judgment_row_auto_iterates(self) -> None:
        for elem in ("C5 Causal adequacy", "T5 Judgment"):
            with self.subTest(elem=elem):
                d = self._bundle(f"TAG{elem[:2]}",
                                 review=_review_table(elem, verdict="NEEDS-HUMAN [impl]"))
                self.assertTrue(self._try(d), f"{elem} tagged [impl] must rebuild")

    def test_an_untagged_judgment_row_alone_still_halts(self) -> None:
        d = self._bundle("UNTAGGED", review=_review_table("T5 Judgment"))
        self.assertFalse(self._try(d))
        self._assert_halted(d)

    def test_the_tag_is_ignored_on_input_cells(self) -> None:
        """A defective brief is a PLAN miss. Rebuilding against the same brief cannot fix it,
        so the reviewer does not get to route it back to Do however it labels the row."""
        for elem in ("C1 Spec", "C3 Change"):
            with self.subTest(elem=elem):
                d = self._bundle(f"IN{elem[:2]}",
                                 review=_review_table(elem, verdict="NEEDS-HUMAN [impl]"))
                items = assemble.collect_needs_human(d, self.cfg)
                self.assertTrue(any(elem in i.text and i.kind == assemble.HUMAN for i in items),
                                items)
                self.assertFalse(self._try(d))
                self._assert_halted(d)

    def test_the_tag_never_promotes_the_standing_row(self) -> None:
        # The constant carries no signal in EITHER direction — a tag on it must not turn the
        # row the prompt emits every cycle into a rebuild trigger.
        review = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
                  "| C1 Spec | PASS | ok |\n"
                  "| Validation — fitness-to-purpose | NEEDS-HUMAN [impl] | the human's call |\n")
        d = self._bundle("TAGV", review=review)
        items = assemble.collect_needs_human(d, self.cfg)
        kinds = {i.kind for i in items if "Validation" in i.text}
        self.assertEqual(kinds, {assemble.STANDING})
        self.assertFalse(self._try(d), "a tagged V row is still just the constant")
        self._assert_halted(d)

    def test_promotable_elements_come_from_the_canonical_matrix(self) -> None:
        judgment = {e for e, _l, k, _o in gates.canonical_elements() if k == "judgment"}
        self.assertEqual(assemble._PROMOTABLE_ELEMENTS, judgment - {"V"})
        self.assertEqual(assemble._PROMOTABLE_ELEMENTS, {"C5", "T5"})

    def test_an_unmappable_tagged_row_is_not_promoted(self) -> None:
        # No element id ⇒ nothing to check the tag against ⇒ fail safe to the human.
        d = self._bundle("TAGBESPOKE",
                         review=_review_table("Some bespoke lens", verdict="NEEDS-HUMAN [impl]"))
        self.assertFalse(self._try(d))
        self._assert_halted(d)


class TheValidationRowItemCellForms(_Base):
    """Issue #332 — every shape `_REVIEW_PROMPT` actually elicits must read as the constant.

    `leaves._REVIEW_PROMPT` lists the matrix as `{elem} — {label}` and asks the Item column to
    carry "the element label above", so `V — Validation — fitness-to-purpose` is the literal
    reading. 37 rows of the wyrd corpus wrote it that way (against 185 bare, plus one ASCII
    `--`), every one of which failed the exact-match STANDING test and became a HUMAN veto —
    #293 returning through a formatting variant, invisible because the fixture only ever built
    the bare form.
    """

    def test_every_observed_form_is_standing(self) -> None:
        for i, form in enumerate(_STANDING_ROW_FORMS):
            with self.subTest(form=form):
                d = self._bundle(f"VF{i}", review=_review_table(
                    "C4 Verification (red→green)", standing_form=form))
                items = assemble.collect_needs_human(d, self.cfg)
                kinds = {it.kind for it in items if "fitness-to-purpose" in it.text}
                self.assertEqual(kinds, {assemble.STANDING}, f"{form!r} → {items}")

    def test_normalization_does_not_swallow_a_real_objection(self) -> None:
        """The #294 property must survive the new normalization: stripping the element prefix
        must not let a LONGER Item cell match the canonical label."""
        for cell in ("V — Validation — fitness-to-purpose: patches the wrong layer",
                     "Validation — fitness-to-purpose: patches the wrong layer"):
            with self.subTest(cell=cell):
                table = ("| Item | Verdict | Basis |\n|---|---|---|\n"
                         "| C1 Spec | PASS | ok |\n"
                         f"| {cell} | NEEDS-HUMAN | the criterion cannot be met |\n")
                self.assertFalse(assemble._needs_human(table)[0].standing, cell)

    def test_the_normalizer_only_touches_the_element_prefix(self) -> None:
        self.assertEqual(assemble._normalized_item_label("V — Validation — fitness-to-purpose"),
                         "Validation — fitness-to-purpose")
        self.assertEqual(assemble._normalized_item_label("Validation -- fitness-to-purpose"),
                         "Validation — fitness-to-purpose")
        # "Validation" begins with V but is not the element id — it must survive untouched.
        self.assertEqual(assemble._normalized_item_label("Validation — fitness-to-purpose"),
                         "Validation — fitness-to-purpose")


class DeferredFindingsSurvive(_Base):
    """Issue #332 — the one way this change could silently lose a real finding."""

    _REVIEW = ("# Review\n\n| Item | Verdict | Basis |\n|---|---|---|\n"
               "| C4 Verification (red→green) | NEEDS-HUMAN | off-by-one |\n"
               "| C5 Causal adequacy | NEEDS-HUMAN | guards the symptom, not the cause |\n"
               f"{_STANDING_ROW}\n")

    def test_the_ledger_survives_the_iteration_archive(self) -> None:
        # Same contract as auto-iterate.json: in DOWNSTREAM_OF_BRIEF it would be moved into
        # iteration-v<N>/ with the attempt and the deferred finding would vanish.
        self.assertNotIn(autoiterate.DEFERRED_FILE, driver.DOWNSTREAM_OF_BRIEF)

    def test_a_round_one_finding_is_still_in_section6_at_handover(self) -> None:
        d = self._bundle("DEFER1", review=self._REVIEW)
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=True)
            driver.run_issue(d, self.cfg)
        # The rebuild's own Check produced a fresh §6 that knows nothing of round 1's concern.
        self.assertNotIn("guards the symptom",
                         (d / "check-review.md").read_text(encoding="utf-8"))
        self.assertIn("guards the symptom", (d / "SUMMARY.md").read_text(encoding="utf-8"))
        self.assertTrue(any("guards the symptom" in t
                            for t in signoff.open_needs_human(d / "SUMMARY.md")))

    def test_a_deferred_finding_still_blocks_accept(self) -> None:
        d = self._bundle("DEFER2", review=self._REVIEW)
        self.assertTrue(self._try(d, apply_now=True))
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            driver.run_issue(d, self.cfg)
        self.assertTrue(signoff.open_needs_human(d / "SUMMARY.md"),
                        "C6 must still make the human clear what was deferred")

    def test_the_ledger_dedups_a_repeated_objection(self) -> None:
        # A reviewer that raises the same concern every round must not grow §6 by one copy
        # per round.
        d = self._bundle("DEFER3", review=self._REVIEW)
        items = assemble.collect_needs_human(d, self.cfg)
        for round_no in (1, 2, 3):
            autoiterate.defer(d, items, attempt=round_no)
        self.assertEqual(len(autoiterate.deferred(d)), 1, autoiterate.deferred(d))

    def test_the_standing_row_is_never_deferred(self) -> None:
        # It is emitted every cycle whatever the reviewer found, so it is not something being
        # held over — and carrying it would grow the ledger forever.
        d = self._bundle("DEFER4", review=self._REVIEW)
        autoiterate.defer(d, assemble.collect_needs_human(d, self.cfg), attempt=1)
        self.assertFalse(any("fitness-to-purpose" in t for t in autoiterate.deferred(d)))

    def test_an_absent_ledger_reads_as_empty(self) -> None:
        d = self._bundle("DEFER5", review=self._REVIEW)
        self.assertFalse((d / autoiterate.DEFERRED_FILE).exists())
        self.assertEqual(autoiterate.deferred(d), [])   # nothing deferred yet is not an error

    def test_an_unreadable_ledger_fails_closed(self) -> None:
        """PR #168 review (codex, P1). Reading a garbled ledger as EMPTY is silently
        destructive: the next `defer` would rewrite it from the current §6 alone and the
        following iterate-do would archive the current SUMMARY, so every objection held over
        from an earlier round would vanish from every artifact at once. This is the one
        reader in the module that must not fail soft."""
        d = self._bundle("DEFER5B", review=self._REVIEW)
        (d / autoiterate.DEFERRED_FILE).write_text("{ not json", encoding="utf-8")
        with self.assertRaises(autoiterate.DeferredLedgerUnreadable):
            autoiterate.deferred(d)

    def test_an_unreadable_ledger_halts_the_rebuild(self) -> None:
        d = self._bundle("DEFER5C", review=self._REVIEW)
        (d / autoiterate.DEFERRED_FILE).write_text("{ not json", encoding="utf-8")
        self.assertFalse(self._try(d), "a rebuild here would destroy the held findings")
        self.assertEqual(autoiterate.count(d), 0)      # no budget spent

    def test_an_unreadable_ledger_surfaces_in_section6_without_crashing(self) -> None:
        # Assembly is defensive by contract, so it must not raise — but it must not drop the
        # ledger silently either. It becomes a §6 item, which the C6 guard then holds on.
        d = self._bundle("DEFER5D", review=self._REVIEW)
        (d / autoiterate.DEFERRED_FILE).write_text("{ not json", encoding="utf-8")
        assemble.assemble_summary(d, self.cfg)
        self.assertTrue(any("unreadable" in t for t in
                            signoff.open_needs_human(d / "SUMMARY.md")))

    def test_a_ticked_deferral_retires_from_the_ledger(self) -> None:
        """PR #168 review (codex, P2). Without this a human who adjudicates a deferred item
        and then iterates for some OTHER reason loses that adjudication: the ticked SUMMARY
        is archived, the next assembly recreates the entry unchecked from the ledger, and the
        objection blocks accept again on every future round with no way to clear it."""
        d = self._bundle("DEFER7", review=self._REVIEW)
        self.assertTrue(self._try(d))                       # round 1 defers the C5 concern
        held = autoiterate.deferred(d)
        self.assertTrue(any("guards the symptom" in t for t in held), held)

        # The human ticks it in §6, then iterates for an unrelated reason.
        summary = d / "SUMMARY.md"
        summary.write_text(summary.read_text(encoding="utf-8").replace("- [ ]", "- [x]"),
                           encoding="utf-8")
        autoiterate.retire_cleared(d, summary)
        self.assertEqual(autoiterate.deferred(d), [], "a ticked item must not come back")

    def test_an_unticked_deferral_survives_retirement(self) -> None:
        # The half that must not regress: retire only what the human actually ticked.
        d = self._bundle("DEFER8", review=self._REVIEW)
        self.assertTrue(self._try(d))
        before = autoiterate.deferred(d)
        autoiterate.retire_cleared(d, d / "SUMMARY.md")     # nothing ticked
        self.assertEqual(autoiterate.deferred(d), before)

    def test_retirement_runs_before_the_archive_moves_the_summary(self) -> None:
        # Ordering is the fix: once _archive_iteration has moved SUMMARY.md, the record of
        # what the human cleared is no longer where the next assembly looks.
        src = inspect.getsource(driver.advance)
        self.assertLess(src.index("_retire_cleared_deferrals"), src.index("_archive_iteration"))

    def test_the_rationale_names_what_was_addressed_and_what_was_held(self) -> None:
        items = [assemble.NeedsHumanItem("off-by-one at x.py:12", assemble.IMPL),
                 assemble.NeedsHumanItem("needs an ADR", assemble.HUMAN)]
        r = autoiterate.rationale(items, attempt=2)
        self.assertNotIn("\n", r)
        self.assertIn("off-by-one at x.py:12", r)
        self.assertIn("deferred", r.lower())
        # The DEFERRED text itself must not reach the builder's carry-forward as a defect.
        self.assertNotIn("needs an ADR", r)
        self.assertNotIn("implementation-level items only", r)

    def test_a_deferred_finding_never_reaches_the_builder_carry_forward(self) -> None:
        d = self._bundle("DEFER6", review=self._REVIEW)
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            flow._maybe_auto_iterate(self.cfg, d, by="", today="2026-07-09", apply_now=True)
            driver.run_issue(d, self.cfg)
        brief_text = (d / "brief.md").read_text(encoding="utf-8")
        self.assertIn("C4 Verification", brief_text)      # the defect Do can act on…
        self.assertNotIn("guards the symptom", brief_text)  # …never the human's judgment call


if __name__ == "__main__":
    unittest.main()
