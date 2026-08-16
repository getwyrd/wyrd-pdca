"""Offline slice for opt-in auto-merge mode (`merge.merge_wave`, #wave-model).

Proves the fail-closed contract: a published, COMPLETE bundle's PR is `gh pr merge`d and
the base re-fetched; a close/no-fix bundle and an already-merged PR are skipped; a
COMPLETE bundle with no recorded PR, or a `gh pr merge` failure, returns non-zero so the
caller STOPs. Dry-run shells nothing. `gh` and state are mocked — no network. Run from the
project root:
    PYTHONPATH=src python -m unittest discover -s tests

Issue #413 extends that fail-closed contract from "merged" to "merged GREEN": `gh pr merge`
only refuses on checks the HOST marks required in branch protection, so `_merge_one` reads
the PR's own FULL check rollup (`gh pr checks`) after the ready-mark and immediately before
the merge, and refuses on any failing, pending or missing check — whatever branch
protection is (or isn't) configured to require. `[driver].merge_requires = "required"` opts
back into the host-config-only behaviour.
"""

from __future__ import annotations

import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from pdca_harness import merge, state
from pdca_harness.config import Config, LeafConfig


def _cfg(root: Path, **overrides: object) -> Config:
    return Config(
        root=root, bundle_root=root / "results", process_dir=root / "process",
        templates_dir=root / "templates", default_branch="main", tracker_system="github",
        tracker_url="", issue_id_example="#1",
        builder=LeafConfig(mode="stub"), reviewer=LeafConfig(mode="stub"),
        base_remote="origin", repo_checkouts={"org/repo": str(root / "repo")},
        **overrides)


def _rollup(*checks: tuple[str, str], code: int = 0) -> SimpleNamespace:
    """A `gh pr checks --json name,bucket` result: `(name, bucket)` pairs plus the exit
    code gh would pair with them (0 all passed, 1 something failed, 8 something pending —
    gh prints the JSON either way, so the buckets are what decides)."""
    return SimpleNamespace(
        returncode=code, stderr="",
        stdout=json.dumps([{"name": n, "bucket": b} for n, b in checks]))


def _gh(**by_verb: SimpleNamespace):
    """Build a `subprocess.run` stub: every `gh`/`git` call succeeds, except the verbs
    named here (`checks=`, `ready=`, `merge=`) which return the given result. The default
    rollup is green, so tests that are not about #413 reach `gh pr merge` exactly as they
    did before it."""
    default_checks = _rollup(("ci", "pass"))
    # The #531 sync runs before the rollup gate, so the shared stub has to answer its two
    # reads or every caller hits its fail-closed arm. Default: a PR that is UP TO DATE, so
    # tests not about #531 reach `gh pr merge` exactly as they did before it.
    default_view = SimpleNamespace(returncode=0, stderr="", stdout=json.dumps(
        {"baseRefName": "main", "headRefName": "fix/x"}))

    def run(cmd, **kw):
        if cmd[:2] == ["gh", "api"]:
            return by_verb.get("api", SimpleNamespace(returncode=0, stdout="0\n", stderr=""))
        if cmd[:2] == ["gh", "pr"]:
            if cmd[2] == "view" and "view" not in by_verb:
                return default_view
            return by_verb.get(cmd[2], default_checks if cmd[2] == "checks"
                               else SimpleNamespace(returncode=0, stdout="", stderr=""))
        return SimpleNamespace(returncode=0, stdout="", stderr="")

    return run


class MergeWave(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _cfg(self.tmp)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _bundle(self, iid: str, *, pr_url: str | None = "https://gh/pr/1",
                patch: str | None = "diff\n", repo: str = "org/repo") -> Path:
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True)
        if patch is not None:
            (d / "patch.diff").write_text(patch, encoding="utf-8")
        if pr_url is not None:
            (d / "publish.json").write_text(
                json.dumps({"pr_url": pr_url, "repo": repo}), encoding="utf-8")
        return d

    def test_dry_run_shells_nothing(self) -> None:
        b = self._bundle("M1")
        with mock.patch("pdca_harness.merge.subprocess.run") as run, \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                redirect_stdout(io.StringIO()) as out:
            rc = merge.merge_wave(self.cfg, [b], dry_run=True, method="merge")
        self.assertEqual(rc, 0)
        run.assert_not_called()                       # no gh in a dry-run
        self.assertIn("gh pr merge", out.getvalue())

    def test_merges_then_fetches_base(self) -> None:
        b = self._bundle("M2")
        runs: list[list[str]] = []
        gh = _gh()

        def fake_run(cmd, **kw):
            runs.append(cmd)
            return gh(cmd, **kw)

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fake_run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()):
            rc = merge.merge_wave(self.cfg, [b], method="squash")
        self.assertEqual(rc, 0)
        self.assertIn(["gh", "pr", "merge", "https://gh/pr/1", "--squash"], runs)
        self.assertTrue(any("fetch" in c for c in runs))   # base refreshed after merge

    def test_close_no_fix_skipped(self) -> None:
        b = self._bundle("M3", patch=None)             # no patch — nothing to merge
        with mock.patch("pdca_harness.merge.subprocess.run") as run, \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE):
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_no_pr_url_fails_closed(self) -> None:
        b = self._bundle("M4", pr_url=None)            # COMPLETE + patch but never published
        with mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                redirect_stderr(io.StringIO()) as err:
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 1)
        self.assertIn("no recorded PR", err.getvalue())

    def test_merge_failure_stops(self) -> None:
        b = self._bundle("M5")
        # ready + the check rollup succeed; the merge itself fails (a conflict, no rights).
        fail_merge = _gh(merge=SimpleNamespace(returncode=1, stdout="",
                                               stderr="not mergeable"))

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fail_merge), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stderr(io.StringIO()) as err:
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 1)
        self.assertIn("did not merge", err.getvalue())

    def test_readies_before_merging(self) -> None:
        # #279: the publisher opens every PR --draft, but `gh pr merge` refuses a draft, so a
        # non-final wave's PR must be readied first. `gh pr ready` must precede `gh pr merge`.
        b = self._bundle("M7")
        runs: list[list[str]] = []
        gh_ok = _gh()

        def fake_run(cmd, **kw):
            runs.append(cmd)
            return gh_ok(cmd, **kw)

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fake_run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()):
            rc = merge.merge_wave(self.cfg, [b], method="merge")
        self.assertEqual(rc, 0)
        gh = [c for c in runs if c[:2] == ["gh", "pr"]]
        self.assertEqual(gh[0], ["gh", "pr", "ready", "https://gh/pr/1"])
        self.assertEqual(gh[-1], ["gh", "pr", "merge", "https://gh/pr/1", "--merge"])

    def test_ready_failure_stops_before_merge(self) -> None:
        # If a PR can't be readied it can't be merged — fail-closed, and never attempt merge.
        b = self._bundle("M8")
        runs: list[list[str]] = []

        def fail_ready(cmd, **kw):
            runs.append(cmd)
            rc = 1 if cmd[:3] == ["gh", "pr", "ready"] else 0
            return SimpleNamespace(returncode=rc, stdout="", stderr="cannot ready")

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fail_ready), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stderr(io.StringIO()) as err:
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 1)
        self.assertIn("could not be marked ready", err.getvalue())
        self.assertNotIn(["gh", "pr", "merge", "https://gh/pr/1", "--merge"], runs)

    def test_dry_run_readies_nothing(self) -> None:
        # A dry-run must shell nothing — not even the new ready step.
        b = self._bundle("M9")
        with mock.patch("pdca_harness.merge.subprocess.run") as run, \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                redirect_stdout(io.StringIO()):
            merge.merge_wave(self.cfg, [b], dry_run=True)
        run.assert_not_called()

    def test_already_merged_skipped(self) -> None:
        b = self._bundle("M6")
        with mock.patch("pdca_harness.merge.subprocess.run") as run, \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=True):
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 0)
        run.assert_not_called()                        # idempotent — no second merge

    def test_first_failure_stops_the_wave(self) -> None:
        # The second bundle has no PR → the wave STOPs there; order is name-sorted by caller.
        ok = self._bundle("MA")
        bad = self._bundle("MB", pr_url=None)

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=_gh()), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stderr(io.StringIO()):
            rc = merge.merge_wave(self.cfg, [ok, bad])
        self.assertEqual(rc, 1)

    # ---- issue #413: merged means merged GREEN, not merely merged --------------------

    def _drive(self, iid: str, *, cfg: Config | None = None,
               **by_verb: SimpleNamespace) -> tuple[int, list[list[str]], str]:
        """Run one bundle through `merge_wave` against a stubbed `gh`. Returns the exit
        code, every command shelled, and stderr — so a test can assert BOTH the refusal
        and that `gh pr merge` was never reached."""
        b = self._bundle(iid)
        calls: list[list[str]] = []
        gh = _gh(**by_verb)

        def fake_run(cmd, **kw):
            calls.append(cmd)
            return gh(cmd, **kw)

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fake_run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as err:
            rc = merge.merge_wave(cfg or self.cfg, [b])
        return rc, calls, err.getvalue()

    def _merged(self, calls: list[list[str]]) -> bool:
        return any(c[:3] == ["gh", "pr", "merge"] for c in calls)

    def test_failing_check_refuses_and_never_merges(self) -> None:
        # The defect: a red job the HOST does not mark required in branch protection. `gh
        # pr merge` would happily succeed (the stub returns 0 for it) — the rollup read is
        # the only thing that can stop this.
        rc, calls, err = self._drive(
            "MC", checks=_rollup(("build", "pass"), ("lint", "fail"), code=1))
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("FAILING", err)
        self.assertIn("lint (fail)", err)              # names the offending check

    def test_pending_check_refuses(self) -> None:
        # Never merge past an in-flight run: wait-or-STOP, not merge-and-hope.
        rc, calls, err = self._drive("MD", checks=_rollup(("ci", "pending"), code=8))
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("not finished", err)

    def test_all_green_readies_then_checks_then_merges(self) -> None:
        rc, calls, _ = self._drive("ME")
        self.assertEqual(rc, 0)
        gh = [c[:3] for c in calls if c[:2] == ["gh", "pr"]]
        # `view` is the #531 behind-check. Order is the contract: the rollup is read AFTER
        # the ready-mark, and AFTER the sync read — so it describes the tree being merged
        # into, not the one the branch was cut from.
        self.assertEqual(gh, [["gh", "pr", "ready"], ["gh", "pr", "view"],
                              ["gh", "pr", "checks"], ["gh", "pr", "merge"]])

    def test_empty_rollup_refuses_under_the_default(self) -> None:
        # Absence of evidence is not green: nothing reported ⇒ nothing verified.
        rc, calls, err = self._drive("MF", checks=_rollup())
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("EMPTY", err)

    def test_rollup_gh_could_not_read_refuses(self) -> None:
        # gh's own shape for "no checks reported" (and for auth/network/too-old-gh): a
        # non-zero exit with no JSON at all. Fail-closed — never merge on a rollup we
        # could not read.
        rc, calls, err = self._drive("MF2", checks=SimpleNamespace(
            returncode=1, stdout="", stderr="no checks reported on the 'fix/x' branch"))
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("no checks reported", err)

    def test_skipped_and_neutral_checks_do_not_block(self) -> None:
        # Completed non-failures: a skipped path filter must not deadlock the wave.
        rc, calls, _ = self._drive(
            "MG", checks=_rollup(("ci", "pass"), ("docs", "skipping")))
        self.assertEqual(rc, 0)
        self.assertTrue(self._merged(calls))

    def test_unknown_bucket_is_treated_as_failing(self) -> None:
        # A bucket this harness has never heard of is not evidence of green.
        rc, calls, err = self._drive("MG2", checks=_rollup(("ci", "quantum"), code=1))
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("FAILING", err)

    def test_check_triggered_by_the_ready_mark_is_caught(self) -> None:
        # `gh pr ready` can itself trigger `ready_for_review` CI: green BEFORE the ready
        # mark, pending after it. A rollup read only pre-ready would have merged this —
        # this is what pins the read to AFTER ready, immediately before the merge.
        b = self._bundle("MH")
        readied = False
        calls: list[list[str]] = []

        def fake_run(cmd, **kw):
            nonlocal readied
            calls.append(cmd)
            if cmd[:3] == ["gh", "pr", "ready"]:
                readied = True
            elif cmd[:3] == ["gh", "pr", "checks"]:
                return (_rollup(("e2e", "pending"), code=8) if readied
                        else _rollup(("e2e", "pass")))
            elif cmd[:3] == ["gh", "pr", "view"]:      # #531 behind-check: up to date
                return SimpleNamespace(returncode=0, stderr="", stdout=json.dumps(
                    {"baseRefName": "main", "headRefName": "fix/x"}))
            elif cmd[:2] == ["gh", "api"]:
                return SimpleNamespace(returncode=0, stdout="0\n", stderr="")
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fake_run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as err:
            rc = merge.merge_wave(self.cfg, [b])
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertIn("not finished", err.getvalue())

    def test_a_red_wave_member_stops_the_wave_before_later_bundles(self) -> None:
        # Wave-level consequence of the gate: the red PR is not merged AND the next
        # bundle's PR is never touched, so no later wave can build on the half-merged set.
        red = self._bundle("MJ1")
        nxt = self._bundle("MJ2", pr_url="https://gh/pr/2")
        calls: list[list[str]] = []
        gh = _gh(checks=_rollup(("ci", "fail"), code=1))

        def fake_run(cmd, **kw):
            calls.append(cmd)
            return gh(cmd, **kw)

        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=fake_run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            rc = merge.merge_wave(self.cfg, [red, nxt])
        self.assertEqual(rc, 1)
        self.assertFalse(self._merged(calls))
        self.assertFalse(any("https://gh/pr/2" in c for c in calls))

    def test_merge_requires_required_restores_host_config_semantics(self) -> None:
        # The opt-in escape hatch: branch protection alone decides again, so the rollup is
        # not even read — and a red non-required check merges, exactly as before #413.
        cfg = _cfg(self.tmp, merge_requires="required")
        rc, calls, _ = self._drive("MI", cfg=cfg,
                                   checks=_rollup(("ci", "fail"), code=1))
        self.assertEqual(rc, 0)
        self.assertFalse(any(c[:3] == ["gh", "pr", "checks"] for c in calls))
        self.assertTrue(self._merged(calls))

    def test_merge_requires_comes_from_the_driver_table(self) -> None:
        # Through the REAL config loader, not a hand-built Config: `[driver]
        # merge_requires` in a rendered pdca.toml has to actually reach `_merge_one`.
        root = self.tmp / "instance"
        root.mkdir()
        toml = root / "pdca.toml"
        base = '[paths]\nbundle_root = "results"\n'

        toml.write_text(base + '\n[driver]\nmerge_requires = "required"\n', encoding="utf-8")
        self.assertEqual(Config.load(root).merge_requires, "required")

        toml.write_text(base, encoding="utf-8")                    # unset ⇒ the default
        self.assertEqual(Config.load(root).merge_requires, "all")

        toml.write_text(base + '\n[driver]\nmerge_requires = "whatever"\n', encoding="utf-8")
        with redirect_stderr(io.StringIO()) as err:
            cfg = Config.load(root)
        self.assertEqual(cfg.merge_requires, "all")   # an unknown value fails CLOSED
        self.assertIn("merge_requires", err.getvalue())


class AwaitRollup(unittest.TestCase):
    """`_await_rollup` — the #462 instance delta: WAIT for an unsettled rollup rather than
    reading it the instant `gh pr ready` returns and STOPping the whole batch on "pending".

    The gate itself is not under test here (that is `merge_requires`, issue #413) — what is
    under test is that waiting never turns a refusal into a merge the settled rollup would
    not have permitted, and that a budget of 0 reproduces upstream exactly.
    """

    def _await(self, verdicts, budget):
        """Drive `_await_rollup` over a scripted sequence of rollup reads, returning
        `(result, reads, sleeps)`.

        The fake clock advances by EXACTLY what the code asked to sleep. An earlier version
        advanced by a fixed tick and discarded the requested duration, so no test could
        observe how long the code slept — and five mutations survived the whole suite,
        including replacing both sleeps with `sleep(0)`, which makes the confirm-once a
        no-op (PR #224 adversarial review).
        """
        seq, seen, sleeps = list(verdicts), [], []
        clock = {"t": 0.0}

        def fake_rollup(_url):
            v = seq.pop(0) if seq else seen[-1]
            seen.append(v)
            return (v, f"detail:{v}")

        def fake_sleep(s):
            sleeps.append(s)
            clock["t"] += s
            # Bound the drive, so a mutant that stops advancing the clock (sleep -> 0) FAILS
            # here instead of hanging the suite. The real loop is bounded by a monotonic
            # clock; this fake one is only bounded by what the code asks for.
            if len(sleeps) > 500:
                raise AssertionError(
                    f"_await_rollup did not terminate: {len(sleeps)} sleeps, "
                    f"last={s!r}, clock={clock['t']} — a zero-length sleep never advances "
                    f"the deadline")

        with mock.patch.object(merge, "_check_rollup", side_effect=fake_rollup):
            out = merge._await_rollup("https://example/pr/1", budget,
                                      sleep=fake_sleep, now=lambda: clock["t"])
        return out, seen, sleeps

    def test_zero_budget_is_a_single_read(self):
        """budget 0 == upstream: one read, no wait, whatever it says."""
        (verdict, _), seen, _sleeps = self._await(["pending"], 0)
        self.assertEqual(verdict, "pending")
        self.assertEqual(len(seen), 1)

    def test_a_settled_refusal_never_waits(self):
        """A red is settled, and an unreadable rollup is an auth/`gh` problem waiting cannot
        fix — both refuse on the first read, so the diagnostic stays prompt."""
        for v in ("failing", "unreadable"):
            with self.subTest(v=v):
                (verdict, _), seen, _sleeps = self._await([v], 600)
                self.assertEqual(verdict, v)
                self.assertEqual(len(seen), 1, "a settled refusal must not be polled again")

    def test_green_is_confirmed_once_before_it_is_believed(self):
        """`gh pr ready` can trigger CI that has not REGISTERED yet, so the first read can be
        a green belonging entirely to the draft's earlier pushes. Waiting for `pending` to
        clear cannot catch that — the rollup never said pending — so a green is re-read once
        (PR #224 review)."""
        (verdict, _), seen, _sleeps = self._await(["green", "green"], 600)
        self.assertEqual(verdict, "green")
        self.assertEqual(seen, ["green", "green"], "green must be confirmed, not trusted")

    def test_a_check_registering_after_the_ready_mark_is_caught(self):
        """The case the confirmation exists for: green, then a ready-triggered check appears
        and the rollup goes pending. It must fall into the ordinary wait, not merge."""
        (verdict, _), seen, _sleeps = self._await(["green", "pending", "green"], 600)
        self.assertEqual(verdict, "green")
        self.assertEqual(seen[:2], ["green", "pending"])
        self.assertGreater(len(seen), 2, "it must keep waiting once the rollup goes pending")

    def test_a_check_registering_after_the_ready_mark_can_turn_it_red(self):
        """And the same path must be able to refuse: green, then the real check registers
        and fails. Believing the first read would have merged a red."""
        (verdict, _), _seen, _sleeps = self._await(["green", "failing"], 600)
        self.assertEqual(verdict, "failing")

    def test_zero_budget_does_not_confirm_either(self):
        """budget 0 is upstream exactly — one read, including for green."""
        (verdict, _), seen, _sleeps = self._await(["green"], 0)
        self.assertEqual(verdict, "green")
        self.assertEqual(len(seen), 1)

    def test_pending_then_green_merges(self):
        """The case the delta exists for: CI was still starting, then went green. The green
        is CONFIRMED before it is believed, so the last read repeats."""
        (verdict, _), seen, _sleeps = self._await(
            ["pending", "pending", "green", "green"], 600)
        self.assertEqual(verdict, "green")
        self.assertEqual(seen, ["pending", "pending", "green", "green"])

    def test_green_reached_through_the_loop_is_confirmed_too(self):
        """The asymmetry that shipped in the first cut: only a FIRST-read green was
        confirmed, so `empty → green` merged what `budget = 0` refuses. During the seconds a
        new PR's checks are registering the rollup reports whatever has registered so far —
        one fast workflow that already passed reads as a clean green while the slow one that
        matters has not created its check run yet — so every green needs the same treatment,
        wherever in the sequence it appears (PR #224 adversarial review)."""
        (verdict, _), seen, _ = self._await(["empty", "green", "green"], 600)
        self.assertEqual(verdict, "green")
        self.assertEqual(seen, ["empty", "green", "green"], "the loop's green must confirm")

    def test_a_late_check_registering_red_after_a_loop_green_refuses(self):
        """And the confirmation must be able to REFUSE there, not just delay."""
        (verdict, _), _seen, _ = self._await(["empty", "green", "failing"], 600)
        self.assertEqual(verdict, "failing")

    def test_it_actually_sleeps(self):
        """Both sleeps are real. Replacing either with `sleep(0)` used to survive the whole
        suite — which made the confirm-once a no-op, since a re-read microseconds later
        catches nothing."""
        _, _, sleeps = self._await(["pending", "green", "green"], 600)
        self.assertTrue(sleeps, "the wait must sleep")
        self.assertTrue(all(x > 0 for x in sleeps), f"no zero-length sleeps: {sleeps}")
        self.assertGreaterEqual(max(sleeps), 30, "a poll interval, not a spin")

    def test_the_confirm_is_charged_to_the_budget_and_cannot_overrun_it(self):
        """`merge_wait_secs = 5` must not sleep 30s. The confirm used to sleep a full
        interval computed independently of the budget, so small budgets were strictly worse
        than 0 — they converted a merge into a STOP after over-sleeping."""
        for budget in (1, 5, 29):
            with self.subTest(budget=budget):
                (verdict, _), _seen, sleeps = self._await(["green", "green"], budget)
                self.assertLessEqual(sum(sleeps), budget,
                                     f"slept {sum(sleeps)}s of a {budget}s budget")
                self.assertEqual(verdict, "green")

    def test_empty_is_transient_too(self):
        """A rollup is empty in the seconds before CI registers its first check; treating
        that as terminal is the same 'too early' mistake one step further back."""
        (verdict, _), _seen, _sleeps = self._await(["empty", "green"], 600)
        self.assertEqual(verdict, "green")

    def test_pending_then_failing_still_refuses(self):
        """Waiting must never launder a red."""
        (verdict, _), _seen, _sleeps = self._await(["pending", "failing"], 600)
        self.assertEqual(verdict, "failing")

    def test_exhausted_budget_returns_the_unsettled_verdict(self):
        """A timeout is not a pass: the last unsettled verdict goes to the gate, which
        refuses and STOPs."""
        (verdict, _), seen, _sleeps = self._await(["pending"] * 50, 60)
        self.assertEqual(verdict, "pending")
        self.assertLessEqual(len(seen), 5, "a 60s budget must not poll 50 times")

    def test_unreadable_is_not_waited_out(self):
        """An unreadable rollup is an auth/`gh` problem; waiting cannot fix it, and failing
        fast keeps the diagnostic honest."""
        (verdict, _), seen, _sleeps = self._await(["unreadable", "green"], 600)
        self.assertEqual(verdict, "unreadable")
        self.assertEqual(len(seen), 1)



class MergeWaitIsWired(unittest.TestCase):
    """`merge_wait_secs` must actually reach `_await_rollup`, and `Config.load` must actually
    read it. Neither was tested: the key appeared nowhere under tests/, so hard-coding the
    budget to 0 in EITHER place — disabling the whole feature in production — kept the suite
    green (PR #224 adversarial review). These two are the mutation kills."""

    def test_merge_one_passes_the_configured_budget_through(self) -> None:
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, True)
        cfg = _cfg(tmp, merge_wait_secs=1234)
        d = cfg.bundle("W1")
        d.mkdir(parents=True)
        (d / "patch.diff").write_text("diff\n", encoding="utf-8")
        (d / "publish.json").write_text(
            json.dumps({"pr_url": "https://gh/pr/1", "repo": "org/repo"}), encoding="utf-8")
        seen: dict[str, object] = {}

        def spy(url, budget, **kw):
            seen["budget"] = budget
            return ("green", "1 check")

        with mock.patch.object(merge, "_await_rollup", side_effect=spy), \
                mock.patch("pdca_harness.merge.subprocess.run", side_effect=_gh()), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()):
            merge._merge_one(cfg, d, dry_run=False, method="merge", fetched=set())
        self.assertEqual(seen.get("budget"), 1234,
                         "_merge_one must pass cfg.merge_wait_secs, not a constant")

    def test_config_reads_the_key(self) -> None:
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, True)
        (tmp / "pdca.toml").write_text("[driver]\nmerge_wait_secs = 777\n", encoding="utf-8")
        with redirect_stderr(io.StringIO()):
            cfg = Config.load(tmp)
        self.assertEqual(cfg.merge_wait_secs, 777, "Config.load must read the key")


class SyncBaseBeforeGate(unittest.TestCase):
    """`merge_sync_base` — the #531 delta. A wave's second merge must not be gated on a
    rollup computed before its sibling landed, so a behind PR is brought up to date BEFORE
    the rollup gate reads anything."""

    def _drive(self, name, *, behind, cfg_kw=None, update_rc=0, view_rc=0, cmp_rc=0,
               stays_behind=False, owner="org", checks=None):
        """`behind` is the INITIAL distance; a successful sync clears it, as a real
        update-branch does. `stays_behind=True` models a base that keeps moving — the case
        the round cap exists for."""
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, True)
        cfg = _cfg(tmp, **(cfg_kw or {}))
        d = cfg.bundle(name)
        d.mkdir(parents=True)
        (d / "patch.diff").write_text("diff\n", encoding="utf-8")
        (d / "publish.json").write_text(
            json.dumps({"pr_url": "https://gh/pr/1", "repo": "org/repo"}), encoding="utf-8")
        calls = []
        state_ = {"behind": behind}

        def run(cmd, **kw):
            calls.append(list(cmd))
            if cmd[:3] == ["gh", "pr", "view"]:
                return SimpleNamespace(returncode=view_rc, stderr="", stdout=json.dumps(
                    {"baseRefName": "main", "headRefName": "fix/x",
                     "headRepositoryOwner": {"login": owner}}))
            if cmd[:2] == ["gh", "api"]:
                return SimpleNamespace(returncode=cmp_rc, stderr="",
                                       stdout=f"{state_['behind']}\n")
            if cmd[:3] == ["gh", "pr", "update-branch"]:
                if update_rc == 0 and not stays_behind:
                    state_["behind"] = 0
                return SimpleNamespace(returncode=update_rc, stderr="update failed",
                                       stdout="")
            if cmd[:3] == ["gh", "pr", "checks"]:
                return checks if checks is not None else _rollup(("ci", "pass"))
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        err = io.StringIO()
        with mock.patch("pdca_harness.merge.subprocess.run", side_effect=run), \
                mock.patch.object(merge.state, "state", return_value=state.COMPLETE), \
                mock.patch.object(merge.merged, "is_merged", return_value=False), \
                redirect_stdout(io.StringIO()), redirect_stderr(err):
            rc = merge._merge_one(cfg, d, dry_run=False, method="merge", fetched=set())
        return rc, calls, err.getvalue()

    @staticmethod
    def _verbs(calls, verb):
        return [c for c in calls if c[:3] == ["gh", "pr", verb]]

    def test_a_behind_pr_is_synced_before_the_rollup_is_read(self) -> None:
        # Ordering is the whole point: a sync AFTER the gate would gate the stale tree.
        rc, calls, _ = self._drive("S1", behind=2)
        self.assertEqual(rc, 0)
        self.assertTrue(self._verbs(calls, "update-branch"), "a behind PR must be synced")
        upd = next(i for i, c in enumerate(calls) if c[:3] == ["gh", "pr", "update-branch"])
        chk = next(i for i, c in enumerate(calls) if c[:3] == ["gh", "pr", "checks"])
        self.assertLess(upd, chk, "the sync must precede the rollup read, not follow it")

    def test_an_up_to_date_pr_is_not_synced(self) -> None:
        rc, calls, _ = self._drive("S2", behind=0)
        self.assertEqual(rc, 0)
        self.assertFalse(self._verbs(calls, "update-branch"),
                         "a current PR must not be pushed for nothing")
        self.assertTrue(self._verbs(calls, "merge"))

    def test_a_failed_sync_stops_without_merging(self) -> None:
        # A behind PR that cannot be updated conflicts with a sibling that already merged;
        # merging it would verify a tree it is not merging into.
        rc, calls, err = self._drive("S3", behind=1, update_rc=1)
        self.assertEqual(rc, 1)
        self.assertFalse(self._verbs(calls, "merge"))
        self.assertIn("could NOT be updated", err)

    def test_an_unreadable_behind_state_stops_without_merging(self) -> None:
        # Fail-closed, on the rollup gate's own principle: absence of evidence is not green.
        rc, calls, err = self._drive("S4", behind=0, cmp_rc=1)
        self.assertEqual(rc, 1)
        self.assertFalse(self._verbs(calls, "merge"))
        self.assertIn("could NOT determine", err)

    def test_the_knob_off_reproduces_upstream(self) -> None:
        # merge_sync_base = false must skip the check entirely — including its fail-closed
        # arm, so an instance opting out is not stopped by an API it never wanted called.
        rc, calls, _ = self._drive("S5", behind=3, cfg_kw={"merge_sync_base": False})
        self.assertEqual(rc, 0)
        self.assertFalse(self._verbs(calls, "update-branch"))
        self.assertTrue(self._verbs(calls, "merge"))

    def test_a_base_that_keeps_moving_stops_rather_than_looping(self) -> None:
        # PR #230 review: the check must re-run after the wait, or a sibling landing during
        # `merge_wait_secs` leaves the head behind again with a green rollup for the old
        # tree. Re-running needs a bound, and exhausting it STOPs rather than merging.
        rc, calls, err = self._drive("S6", behind=1, stays_behind=True)
        self.assertEqual(rc, 1)
        self.assertFalse(self._verbs(calls, "merge"))
        self.assertEqual(len(self._verbs(calls, "update-branch")), merge._MAX_SYNC_ROUNDS)
        self.assertIn("moved under this PR", err)

    def test_rechecks_the_base_after_the_post_sync_wait(self) -> None:
        # The positive half of the same finding: one sync, then a re-read that finds the
        # base still — so exactly two compares, and the merge proceeds.
        rc, calls, _ = self._drive("S7", behind=1)
        self.assertEqual(rc, 0)
        self.assertEqual(len([c for c in calls if c[:2] == ["gh", "api"]]), 2,
                         "the base must be re-read after the post-sync gate, not once")
        self.assertTrue(self._verbs(calls, "merge"))

    def test_required_mode_still_waits_after_a_sync(self) -> None:
        # PR #230 review: `merge_requires = "required"` skips the rollup GATE, but a sync
        # invalidates the checks — merging straight after would fail on pending required
        # checks and stop every wave at its second PR. The wait is not optional.
        rc, calls, _ = self._drive("S8", behind=1, cfg_kw={"merge_requires": "required"})
        self.assertEqual(rc, 0)
        self.assertTrue(self._verbs(calls, "checks"),
                        "a sync must be followed by a rollup wait even in required mode")
        self.assertTrue(self._verbs(calls, "merge"))

    def test_a_fork_head_is_qualified_in_the_compare(self) -> None:
        # PR #230 review: `headRefName` is a bare branch name; unqualified it resolves
        # against the BASE repo, where a fork's branch does not exist — 404 => None =>
        # every merge stops. publish.py:301 qualifies the same way for `gh pr create`.
        _rc, calls, _ = self._drive("S9", behind=0, owner="contributor")
        api = [c for c in calls if c[:2] == ["gh", "api"]]
        self.assertTrue(api)
        self.assertIn("main...contributor:fix/x", api[0][2])

    def test_a_same_owner_head_is_not_qualified(self) -> None:
        _rc, calls, _ = self._drive("S10", behind=0, owner="org")
        api = [c for c in calls if c[:2] == ["gh", "api"]]
        self.assertIn("main...fix/x", api[0][2])

    def test_config_reads_the_key(self) -> None:
        # The mutation kill: hard-coding the default in Config.load would leave every test
        # above green while the knob did nothing in production.
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, True)
        (tmp / "pdca.toml").write_text("[driver]\nmerge_sync_base = false\n",
                                       encoding="utf-8")
        with redirect_stderr(io.StringIO()):
            cfg = Config.load(tmp)
        self.assertFalse(cfg.merge_sync_base, "Config.load must read the key")

    def test_default_is_on(self) -> None:
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, True)
        (tmp / "pdca.toml").write_text("[driver]\n", encoding="utf-8")
        with redirect_stderr(io.StringIO()):
            cfg = Config.load(tmp)
        self.assertTrue(cfg.merge_sync_base)


if __name__ == "__main__":
    unittest.main()
