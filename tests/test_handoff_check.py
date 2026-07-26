"""Deterministic tests for the leaf exit gate (`scripts/handoff-check`).

The gate exists because `leaves._invoke` runs every `interactive = true` leaf as a bare
`subprocess.run(...)` with no `check=` and no capture: the driver blocks until the `claude`
process exits and discards the exit code, so "the human pressed Ctrl-D" and "the leaf
discharged its contract" are the same event. These pin the contract per leaf, and the two
rules that decide what counts as a failure:

- MISSING fails only for a bundle the caller NAMED — a planner session legitimately briefs
  some issues and not others, so an absent brief on an unnamed bundle is not a defect;
- MALFORMED always fails — whoever wrote the artifact wrote it wrong, and no scoping
  argument excuses that.

Plus the two false positives the corpus caught while this was being written: an empty
`Test file` and a missing `Slug` are both tolerated by the driver, so neither may fail the
gate (see `_LOAD_BEARING_FIELDS`).

Run from the project root:
    python -m unittest discover -s tests
"""

from __future__ import annotations

import importlib.machinery
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from pdca_harness import leaves

_SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "handoff-check"

# §9 must carry the `- Outcome:` line `signoff.record` writes into: it REPLACES that line, so
# without it the decision is silently dropped (PR #169 review round 2).
_SUMMARY = ("## 6. NEEDS-HUMAN\n\n- (none)\n\n## 9. Check sign-off\n\n"
            "- Outcome:\n- Iteration delta (if iterating):\n")


def _load():
    loader = importlib.machinery.SourceFileLoader("handoff_check", str(_SCRIPT))
    spec = importlib.util.spec_from_loader("handoff_check", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


hc = _load()

_BRIEF = """# Brief

- **Slug:** fix-the-thing
- **Scope:** the reaper fence commit path only
- **Success criterion:** the reaper commits its fence under a concurrent abort
- **Falsifiability:** revert the fence commit; the reaper conformance clause goes RED
- **Repo + branch target:** `getwyrd/wyrd` @ `main`
- **Test file:** crates/x/tests/thing.rs
"""


class _Base(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.d = self.tmp / "issue_1"
        self.d.mkdir(parents=True)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _brief(self, text: str = _BRIEF) -> None:
        (self.d / "brief.md").write_text(text, encoding="utf-8")

    def _ok(self, results) -> bool:
        return not [r for r in results if not r.ok and not r.warn]


class Planner(_Base):
    def test_an_authored_brief_passes(self) -> None:
        self._brief()
        self.assertTrue(self._ok(hc.check_planner(self.d, named=True)))

    def test_an_absent_brief_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_planner(self.d, named=True)))
        # Unnamed: the planner briefs some issues and not others — silence, not a defect.
        self.assertEqual(hc.check_planner(self.d, named=False), [])

    def test_an_unfilled_template_fails_even_unnamed(self) -> None:
        # MALFORMED, not MISSING: the file exists, so someone wrote it and wrote it wrong.
        self._brief("# Brief\n\n- **Slug:** <short-kebab-slug>\n")
        self.assertFalse(self._ok(hc.check_planner(self.d, named=False)))

    def test_a_missing_branch_target_fails(self) -> None:
        # The one field with no fallback: `publish._resolve_target` partitions on `@` and
        # an empty value yields an empty repo spec AND an empty base.
        self._brief("# Brief\n\n- **Slug:** fix-the-thing\n- **Scope:** one path\n"
                    "- **Success criterion:** it works\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("repo + branch target" in r.label for r in results), results)

    def test_an_empty_test_file_is_tolerated(self) -> None:
        """Seven committed bundles ship `Test file` empty on purpose, and `brief.test_files`
        returns [] for it — the driver only uses it to unlink a shipped test on iterate. A
        gate that failed them would false-positive on 7% of the corpus."""
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:** it works\n"
                    "- **Falsifiability:** revert it\n"
                    "- **Repo + branch target:** `o/r` @ `main`\n- **Test file:**\n")
        self.assertTrue(self._ok(hc.check_planner(self.d, named=True)))

    def test_a_missing_success_criterion_fails(self) -> None:
        """PR #169 review. `agents/planner.md:22` names this THE load-bearing field — the
        sentence Check later tests "did this work" against. Nothing in the driver reads it
        mechanically, which is why the first draft of this gate classified it optional; the
        contract requires it, and Do/Check otherwise proceed without the condition they exist
        to implement and verify."""
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    ""
                    "- **Repo + branch target:** `o/r` @ `main`\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("success criterion" in r.label for r in results), results)

    def test_a_multiline_success_criterion_is_authored(self) -> None:
        """PR #169 review, second pass. `brief.parse_fields` is line-based, so a value
        indented BENEATH its label parses as empty — and that is a real brief shape: pointer
        briefs (`plan-pointer.md.tpl`) and any long criterion write it that way. Four
        committed bundles do (256/258/364/366). Failing them would be a parser artifact, not
        a contract breach."""
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Repo + branch target:** `o/r` @ `main`\n"
                    "- **Falsifiability:** revert it\n"
                    "- **Success criterion:**\n"
                    "  - **BINDING:** the under-replicated count rises and returns to zero\n")
        self.assertTrue(self._ok(hc.check_planner(self.d, named=True)))

    def test_a_label_with_nothing_under_it_still_fails(self) -> None:
        # The other direction: tolerating multi-line must not tolerate an EMPTY field.
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:**\n"
                    "- **Repo + branch target:** `o/r` @ `main`\n")
        self.assertFalse(self._ok(hc.check_planner(self.d, named=True)))

    def test_a_multiline_placeholder_still_fails(self) -> None:
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Repo + branch target:** `o/r` @ `main`\n"
                    "- **Success criterion:**\n  <the observable condition that means it is "
                    "fixed>\n")
        self.assertFalse(self._ok(hc.check_planner(self.d, named=True)))

    def test_a_placeholder_success_criterion_fails(self) -> None:
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:** <the observable "
                    "condition>\n- **Repo + branch target:** `o/r` @ `main`\n")
        self.assertFalse(self._ok(hc.check_planner(self.d, named=True)))

    def test_a_target_without_a_branch_fails(self) -> None:
        """PR #169 review. `brief.field` is truthy for `owner/repo` with no `@ branch`, so a
        presence check passes while `_resolve_target` yields an empty base and publish aborts
        — after the interactive session is gone."""
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:** it works\n"
                    "- **Repo + branch target:** `getwyrd/wyrd`\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("@ <branch>" in r.label for r in results), results)

    def test_a_target_without_an_owner_fails(self) -> None:
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:** it works\n"
                    "- **Repo + branch target:** `wyrd` @ `main`\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("owner/repo" in r.label for r in results), results)

    def test_a_WRAPPED_placeholder_is_not_authored(self) -> None:
        """PR #169 review round 2. The stock placeholders WRAP, so the first line opened with
        `<` and the continuation did not — and the continuation was read as authored text, so
        an entirely untouched template field passed."""
        self._brief("# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Repo + branch target:** `o/r` @ `main`\n"
                    "- **Falsifiability:** revert the commit; the clause goes RED\n"
                    "- **Success criterion:** <the observable condition that means it is "
                    "fixed — must be\n  demonstrable by C4-verify on the target harness>\n")
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("success criterion" in r.label for r in results), results)

    def test_falsifiability_is_required_for_a_NAMED_bundle(self) -> None:
        """PR #169 review round 2. agents/planner.md:157-164 calls an unavailable RED
        environment a Plan-BLOCKING gap."""
        self._brief(_BRIEF.replace(
            "- **Falsifiability:** revert the fence commit; the reaper conformance clause "
            "goes RED\n", ""))
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("falsifiability" in r.label for r in results), results)

    def test_falsifiability_is_NOT_required_when_scanning(self) -> None:
        """It entered the template at v0.52.1, so 52 of the 85 committed briefs predate the
        field. Holding a historical bundle to a field its template never had is a false
        positive, not a finding — hence the named/scanned split."""
        self._brief(_BRIEF.replace(
            "- **Falsifiability:** revert the fence commit; the reaper conformance clause "
            "goes RED\n", ""))
        self.assertTrue(self._ok(hc.check_planner(self.d, named=False)))

    def test_a_target_with_an_empty_side_fails(self) -> None:
        # PR #169 review round 2: `owner/` and `/repo` both contain a slash but name nothing.
        # `owner/` is the dangerous one — `publish._checkout_path` resolves the empty last
        # segment to cfg.root.parent, aiming later git operations at the wrong directory.
        for target in ("`getwyrd/` @ `main`", "`/wyrd` @ `main`"):
            with self.subTest(target=target):
                self._brief(f"# Brief\n\n- **Slug:** s\n"
                    "- **Scope:** the one path\n"
                    "- **Success criterion:** it works\n"
                            f"- **Falsifiability:** revert it\n"
                            f"- **Repo + branch target:** {target}\n")
                results = hc.check_planner(self.d, named=True)
                self.assertFalse(self._ok(results), target)
                self.assertTrue(any("owner/repo" in r.label for r in results), results)

    def test_a_missing_scope_fails(self) -> None:
        """PR #169 review round 2. agents/planner.md:24 requires scope / out of scope so Do
        cannot sprawl; without it the gate hands implementation an unbounded slice. Authored
        in all 85 committed briefs, so unlike falsifiability this is not a legacy gap."""
        self._brief(_BRIEF.replace(
            "- **Scope:** the reaper fence commit path only\n", ""))
        results = hc.check_planner(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("`scope`" in r.label for r in results), results)

    def test_an_unfilled_optional_field_warns_but_does_not_fail(self) -> None:
        self._brief(_BRIEF + "- **Ordering note:** <optional free text>\n")
        results = hc.check_planner(self.d, named=True)
        self.assertTrue(self._ok(results))
        self.assertTrue(any(r.warn for r in results), results)


class Signoff(_Base):
    def setUp(self) -> None:
        super().setUp()
        # A summary must exist for a decision to be recorded against — see
        # test_a_missing_summary_fails_the_signoff for why the gate now insists.
        (self.d / "SUMMARY.md").write_text(_SUMMARY, encoding="utf-8")

    def _decision(self, text: str) -> None:
        (self.d / leaves.SIGNOFF_DECISION).write_text(text, encoding="utf-8")

    def test_a_summary_with_no_outcome_FIELD_fails(self) -> None:
        """PR #169 review round 2. `outcome_token()` returns "" for a MISSING field and an
        unset one alike, so an accept passed — and `signoff.record` only REPLACES an existing
        line, so it then silently recorded nothing, deleted the decision, and left the bundle
        awaiting sign-off with the session gone."""
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n- (none)\n\n## 9. Check sign-off\n\n(nothing)\n",
            encoding="utf-8")
        self._decision("accept\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("writable `outcome`" in r.label for r in results), results)

    def test_the_REAL_assembled_summary_shape_passes(self) -> None:
        """THE regression test (PR #169 review round 3). `outcome_token()` on an unset
        `- Outcome:` followed by `- Iteration delta (if iterating):` returns that NEXT LABEL,
        which is truthy — so the "§9 already set" check failed EVERY legitimate sign-off. The
        earlier test passed only because its fixture put nothing after the Outcome line, which
        is a shape `assemble_summary` never emits. This fixture is the real one."""
        real = ("## 6. NEEDS-HUMAN\n\n- (none)\n\n"
                "## 9. Check sign-off                     \u2190 human completes Check here\n"
                "- Disposition confirmed / overridden:\n"
                "- Outcome:\n"
                "- Iteration delta (if iterating):\n"
                "- By / date:\n")
        (self.d / "SUMMARY.md").write_text(real, encoding="utf-8")
        self._decision("accept\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertTrue(self._ok(results),
                        [r.label for r in results if not r.ok and not r.warn])

    def test_a_lookalike_outcome_field_is_not_writable(self) -> None:
        """`signoff.record` replaces only the exact `^- Outcome:` form, so a tolerant
        predicate reported `- outcome:` or an indented variant as writable and passed an
        accept the recorder would silently fail to write."""
        for variant in ("- outcome:", "  - Outcome:", "-  Outcome:"):
            with self.subTest(variant=variant):
                (self.d / "SUMMARY.md").write_text(
                    f"## 6. NEEDS-HUMAN\n\n- (none)\n\n## 9. Check sign-off\n\n{variant}\n",
                    encoding="utf-8")
                self._decision("accept\n")
                self.assertFalse(self._ok(hc.check_signoff(self.d, named=True)), variant)

    def test_an_iterate_needs_a_writable_delta_line(self) -> None:
        """Without it `signoff.record` cannot insert the rationale, the flow deletes the
        decision, and `_carry_forward_into_brief` reads an empty delta — so the next attempt
        loses the human's rejection reason and can repeat the same approach."""
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n- (none)\n\n## 9. Check sign-off\n\n- Outcome:\n",
            encoding="utf-8")
        self._decision("iterate-do\nthe cause was misread\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("iteration delta" in r.label for r in results), results)

    def test_an_accept_with_no_section6_at_all_fails(self) -> None:
        """An over-reaching leaf deleting the §6 heading and body leaves `open_needs_human`
        returning [], and the driver applies the same empty-list guard — so a malformed
        summary could reach COMPLETE without the human ever seeing the mandatory section."""
        (self.d / "SUMMARY.md").write_text(
            "## 9. Check sign-off\n\n- Outcome:\n- Iteration delta (if iterating):\n",
            encoding="utf-8")
        self._decision("accept\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("§6 NEEDS-HUMAN is absent" in r.label for r in results), results)

    def test_a_missing_summary_fails_the_signoff(self) -> None:
        """PR #169 review round 2. `open_needs_human` and `outcome_token` both return
        "nothing" for an absent SUMMARY by their own defensive contract, so an accept sailed
        through — and `flow._apply_decision` then discarded the decision and re-drove the
        bundle with the session gone. Green exactly where downstream rejects it."""
        (self.d / "SUMMARY.md").unlink()
        self._decision("accept\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("SUMMARY.md is absent" in r.label for r in results), results)

    def test_a_bare_accept_passes(self) -> None:
        self._decision("accept\n")
        self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_every_valid_token_is_recognized(self) -> None:
        for token in sorted(leaves.VALID_DECISIONS):
            with self.subTest(token=token):
                self._decision(f"{token}\nbecause the cause was misread\n")
                self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_an_unrecognized_token_fails(self) -> None:
        self._decision("looks-good-to-me\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("not recognized" in r.label for r in results))

    def test_an_iterate_without_a_rationale_fails(self) -> None:
        # The driver folds the rationale into the brief's carry-forward; without it the next
        # beat rebuilds blind.
        self._decision("iterate-do\n")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("no rationale" in r.label for r in results), results)

    def test_a_discontinue_without_a_rationale_fails(self) -> None:
        self._decision("discontinue\n   \n")
        self.assertFalse(self._ok(hc.check_signoff(self.d, named=True)))

    def test_an_absent_decision_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_signoff(self.d, named=True)))
        self.assertEqual(hc.check_signoff(self.d, named=False), [])

    def test_an_accept_with_open_section6_items_fails(self) -> None:
        """PR #169 review. `flow._apply_decision` refuses an accept while §6 has open items,
        so passing one here sends the human away and leaves the flow blocked with the
        interactive context gone — the exact contradiction this gate exists to catch. Runs
        the same `signoff.open_needs_human` predicate the driver runs."""
        self._decision("accept\n")
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n- [ ] C5 Causal adequacy — needs an ADR\n"
            "\n## 9. Check sign-off\n\n- Outcome:\n"
            "- Iteration delta (if iterating):\n", encoding="utf-8")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("§6 item(s) still open" in r.label for r in results), results)

    def test_an_iterate_with_open_section6_items_is_fine(self) -> None:
        # C6 guards ACCEPT only — an iterate is exactly what you record when §6 is not clear.
        self._decision("iterate-do\nthe cause was misread\n")
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n- [ ] C5 Causal adequacy — needs an ADR\n"
            "\n## 9. Check sign-off\n\n- Outcome:\n"
            "- Iteration delta (if iterating):\n", encoding="utf-8")
        self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_an_accept_with_every_item_cleared_passes(self) -> None:
        self._decision("accept\n")
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n- [x] C5 Causal adequacy — adjudicated\n"
            "\n## 9. Check sign-off\n\n- Outcome:\n"
            "- Iteration delta (if iterating):\n", encoding="utf-8")
        self.assertTrue(self._ok(hc.check_signoff(self.d, named=True)))

    def test_a_model_authored_section9_fails(self) -> None:
        """§9 is the driver's to write, under the C6 accept-guard (`signoff.record`). This
        runs BEFORE the flow applies the decision, so a set §9 can only be the session's."""
        self._decision("accept\n")
        (self.d / "SUMMARY.md").write_text(
            "## 6. NEEDS-HUMAN\n\n\n## 9. Check sign-off\n\n- Outcome: merged-wider\n",
            encoding="utf-8")
        results = hc.check_signoff(self.d, named=True)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("§9" in r.label for r in results), results)


class Publisher(_Base):
    def test_both_artifacts_present_passes(self) -> None:
        (self.d / "commit-msg.txt").write_text("fix: the thing\n", encoding="utf-8")
        (self.d / "pr-description.md").write_text("## What\n\nthe thing\n", encoding="utf-8")
        self.assertTrue(self._ok(hc.check_publisher(self.d, named=True)))

    def test_an_empty_artifact_fails_even_unnamed(self) -> None:
        (self.d / "commit-msg.txt").write_text("   \n", encoding="utf-8")
        self.assertFalse(self._ok(hc.check_publisher(self.d, named=False)))

    def test_a_missing_artifact_fails_only_when_named(self) -> None:
        self.assertFalse(self._ok(hc.check_publisher(self.d, named=True)))
        self.assertEqual(hc.check_publisher(self.d, named=False), [])


class Act(unittest.TestCase):
    """The leaf NAMES the entry it wrote; the gate verifies that entry (PR #169 review r2).

    Three things the earlier versions got wrong: date membership is not authorship (Act runs
    more than once a day), HEAD is not the session boundary (the leaf appends and does not
    commit, so a prior UNCOMMITTED entry already made the tree longer than HEAD), and a
    heading is not a review.
    """

    HEAD = "# Act review — 2026-07-26 — the morning run\n\nConsidered 505, 509. No delta.\n"

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        (self.tmp / "process").mkdir()
        self.cfg = mock.Mock(root=self.tmp, process_dir=self.tmp / "process")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _log(self, text: str) -> None:
        (self.tmp / "process" / "act-log.md").write_text(text, encoding="utf-8")

    def _check(self, entry: str, *, committed: str | None):
        with mock.patch.object(hc, "_committed_act_log", return_value=committed):
            return hc.check_act(self.cfg, "2026-07-26", entry)

    def _ok(self, results) -> bool:
        return not [r for r in results if not r.ok and not r.warn]

    def test_a_new_substantive_entry_passes(self) -> None:
        self._log(self.HEAD + "\n# Act review — 2026-07-26 — the afternoon run\n\n"
                  "Considered 634, 638. The prose gate is conditional, not closed.\n")
        self.assertTrue(self._ok(self._check("the afternoon run", committed=self.HEAD)))

    def test_a_stale_entry_no_longer_passes_automatically(self) -> None:
        """The round-2 finding was that a growth-vs-HEAD check passed for a session that
        wrote nothing, because the leaf appends without committing and a prior UNCOMMITTED
        entry already made the tree longer than HEAD.

        Naming the entry removes the automatic pass: the gate no longer infers authorship
        from the log being longer, so a session that writes nothing has nothing to name and
        gets `no --entry`. What it cannot do is catch a leaf that names someone ELSE'S
        uncommitted entry — that needs a session-START snapshot, which an end-of-session
        command cannot take. Documented as a residual limit rather than papered over.
        """
        self._log(self.HEAD)
        self.assertFalse(self._ok(self._check("", committed="")))       # nothing named
        # And the committed case, which IS detectable, fails:
        self.assertFalse(self._ok(self._check("the morning run", committed=self.HEAD)))

    def test_an_entry_already_in_HEAD_fails(self) -> None:
        self._log(self.HEAD)
        results = self._check("the morning run", committed=self.HEAD)
        self.assertFalse(self._ok(results))
        self.assertTrue(any("already in HEAD" in r.label for r in results), results)

    def test_a_heading_with_no_body_fails(self) -> None:
        """A heading is not a review: the cycles considered, what they exposed and the agreed
        delta are Act's only durable output. 'No delta warranted' is valid and still needs
        writing down."""
        self._log("# Act review — 2026-07-26 — the afternoon run\n")
        results = self._check("the afternoon run", committed="")
        self.assertFalse(self._ok(results))
        self.assertTrue(any("no body" in r.label for r in results), results)

    def test_a_missing_entry_argument_fails(self) -> None:
        self._log(self.HEAD)
        results = self._check("", committed="")
        self.assertFalse(self._ok(results))
        self.assertTrue(any("no --entry" in r.label for r in results), results)

    def test_an_unmatched_entry_fails(self) -> None:
        self._log(self.HEAD)
        results = self._check("a heading that was never written", committed="")
        self.assertFalse(self._ok(results))

    def test_an_ambiguous_entry_fails(self) -> None:
        self._log(self.HEAD + "\n# Act review — 2026-07-26 — the morning run\n\nagain\n")
        results = self._check("the morning run", committed="")
        self.assertFalse(self._ok(results))
        self.assertTrue(any("cannot tell which" in r.label for r in results), results)

    def test_the_date_is_not_recomputed(self) -> None:
        """The driver hands Act the flow's `today` and an interactive session can outlive it.
        Recomputing the date here failed a correctly authored entry across midnight; naming
        the entry removes the date from the question entirely."""
        self._log("# Act review — 2026-07-25 — yesterday by the flow's clock\n\nbody\n")
        self.assertTrue(self._ok(self._check("yesterday by the flow's clock", committed="")))

    def test_an_untouched_scaffold_is_not_a_review(self) -> None:
        """PR #169 review round 3. `act.scaffold_entry` pre-fills bundles and patterns and
        leaves the DELTAS as TODO — so the body is non-empty and a body-emptiness test passed
        an untouched scaffold as a completed review."""
        self._log("# Act review — 2026-07-26 — cycles considered: 505, 509\n\n"
                  "## What the cycles' records exposed\n- [C4] a recurring signal\n\n"
                  "## Process deltas  (TODO — the human decides these; each must be located)\n"
                  "- Spec template: <field added/clarified/removed>            (path)\n")
        results = self._check("cycles considered", committed="")
        self.assertFalse(self._ok(results))
        self.assertTrue(any("untouched scaffold" in r.label for r in results), results)

    def test_an_absent_log_fails(self) -> None:
        self.assertFalse(self._ok(self._check("anything", committed="")))

    def test_an_unreadable_baseline_degrades_to_a_warning(self) -> None:
        self._log(self.HEAD)
        results = self._check("the morning run", committed=None)
        self.assertTrue(self._ok(results))
        self.assertTrue(any(r.warn for r in results), results)


class ScanRequiresIds(_Base):
    """A bare scan cannot know which bundles this session touched (PR #169 review round 3).

    Both directions were wrong: a newly authored brief was exempted from the named-only
    checks, while any unrelated pre-existing valid brief supplied a PASS even when the session
    wrote nothing. Deriving the set needs a session-START baseline, which an end-of-session
    command cannot take.
    """

    def _run(self, *argv: str) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = hc.main(list(argv))
        return rc, out.getvalue() + err.getvalue()

    def test_each_bundle_leaf_refuses_a_bare_scan(self) -> None:
        cfg = mock.Mock(root=self.tmp, bundle_root=self.tmp, process_dir=self.tmp / "process")
        for leaf in ("planner", "signoff", "publisher"):
            with self.subTest(leaf=leaf):
                with mock.patch.object(hc.Config, "load", return_value=cfg):
                    rc, out = self._run("--leaf", leaf)
                self.assertEqual(rc, 2, out)
                self.assertIn("name the bundle ids", out)

    def test_act_still_needs_no_ids(self) -> None:
        (self.tmp / "process").mkdir(exist_ok=True)
        (self.tmp / "process" / "act-log.md").write_text(
            "# Act review — 2026-07-26 — a real one\n\nConsidered 505. No delta.\n",
            encoding="utf-8")
        cfg = mock.Mock(root=self.tmp, bundle_root=self.tmp, process_dir=self.tmp / "process")
        with mock.patch.object(hc, "_committed_act_log", return_value=""), \
             mock.patch.object(hc.Config, "load", return_value=cfg):
            rc, _out = self._run("--leaf", "act", "--entry", "a real one")
        self.assertEqual(rc, 0)


class NoBundleWrites(_Base):
    """The gate writes NOTHING into a bundle (PR #169 review round 2).

    It used to record `handoff.json`, which no role names — a fourth write for sign-off,
    whose contract is "exactly three things, nothing else". Invoking a deterministic helper
    from inside a leaf does not make its output a driver artifact.
    """

    def _run(self, *argv: str) -> int:
        with redirect_stdout(io.StringIO()):
            return hc.main(list(argv))

    def test_a_passing_run_leaves_the_bundle_untouched(self) -> None:
        self._brief()
        before = {p.name for p in self.d.iterdir()}
        cfg = mock.Mock(root=self.tmp, bundle_root=self.tmp, process_dir=self.tmp / "process")
        cfg.find_bundle.return_value = self.d
        with mock.patch.object(hc.Config, "load", return_value=cfg):
            self.assertEqual(self._run("--leaf", "planner", "1"), 0)
        self.assertEqual({p.name for p in self.d.iterdir()}, before)

    def test_no_handoff_json_constant_survives(self) -> None:
        self.assertFalse(hasattr(hc, "MARKER"), "the in-bundle marker must be gone entirely")


if __name__ == "__main__":
    unittest.main()
