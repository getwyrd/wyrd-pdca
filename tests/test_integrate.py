"""Slice for integration-branch stacking (`integrate.fold`) — the default wave
sequencing that folds each wave's accepted patches onto a run-scoped branch the next
wave builds on, without merging (#wave-model).

Two halves: pure/dry-run cases (no git — naming, nothing-to-fold, dry-run shells
nothing, different-target exclusion) and real-git folds against a bare ``origin`` +
a primary checkout (a clean fold pushes the branch; an undeclared overlap raises
``IntegrationError``). Run from the project root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import io
import json
import shutil
import subprocess
import tempfile
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from pdca_harness import integrate
from pdca_harness.config import Config, LeafConfig


def _cfg(root: Path, repo_spec: str, primary: Path) -> Config:
    return Config(
        root=root, bundle_root=root / "results", process_dir=root / "process",
        templates_dir=root / "templates", default_branch="main", tracker_system="github",
        tracker_url="", issue_id_example="#1",
        builder=LeafConfig(mode="stub"), reviewer=LeafConfig(mode="stub"),
        base_remote="origin", repo_checkouts={repo_spec: str(primary)})


class FoldDryAndUnit(unittest.TestCase):
    """No git — naming, the nothing-to-fold short-circuits, dry-run shells nothing."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _cfg(self.tmp, "org/repo", self.tmp / "repo")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _bundle(self, iid: str, *, target: str = "org/repo @ main",
                patch: str | None = "diff --git a/f.py b/f.py\n@@ -1 +1 @@\n-x\n+y\n") -> Path:
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True)
        (d / "brief.md").write_text(
            f"- **Slug:** {iid.lower()}\n- **Repo + branch target:** {target}\n",
            encoding="utf-8")
        if patch is not None:
            (d / "patch.diff").write_text(patch, encoding="utf-8")
        return d

    def test_integration_branch_name_is_injective_per_base(self) -> None:
        self.assertEqual(integrate.integration_branch(self.cfg, "main"),
                         "pdca-integration/main")
        # Prefix-free escape (`-`→`-h`, `/`→`-s`): two bases that differ only by `/` vs `-`
        # never collide onto one branch / worktree (#187).
        self.assertEqual(integrate.integration_branch(self.cfg, "release/2.0"),
                         "pdca-integration/release-s2.0")
        self.assertEqual(integrate.integration_branch(self.cfg, "release-2.0"),
                         "pdca-integration/release-h2.0")
        self.assertNotEqual(integrate.integration_branch(self.cfg, "release/2.0"),
                            integrate.integration_branch(self.cfg, "release-2.0"))
        # Adjacent `-`/`/` stayed injective: the old `-`→`--` / `/`→`-` scheme collapsed both
        # `-/` and `/-` to a `---` run and aliased these onto one branch/worktree (#199).
        self.assertNotEqual(integrate.integration_branch(self.cfg, "release-/2"),
                            integrate.integration_branch(self.cfg, "release/-2"))
        self.assertNotEqual(integrate.integration_branch(self.cfg, "a-/b"),
                            integrate.integration_branch(self.cfg, "a/-b"))

    def test_run_key_is_stable_order_free_and_injective_with_the_base(self) -> None:
        # #591: a re-run of the same batch rebuilds the SAME branch.
        self.assertEqual(integrate.run_key_for(["issue_2", "issue_1"]),
                         integrate.run_key_for(["issue_1", "issue_2", "issue_1"]))
        self.assertEqual(integrate.run_key_for([]), "")
        self.assertEqual(integrate.integration_branch(self.cfg, "main", ""),
                         "pdca-integration/main")          # empty key = upstream name
        # The key is its own ref component, so the base component keeps its length, and
        # `r-<key>` can never equal a flattened base (its `-` would be escaped).
        keyed = integrate.integration_branch(self.cfg, "main", "abc")
        self.assertEqual(keyed, "pdca-integration/r-abc/main")
        self.assertNotEqual(integrate.integration_branch(self.cfg, "r-abc"),
                            "pdca-integration/r-abc")
        long_base = "b" * 245   # valid under the upstream name; must stay valid keyed
        self.assertEqual(integrate.integration_branch(self.cfg, long_base, "abc").split("/")[-1],
                         long_base)

    def test_run_key_does_not_collide_on_the_reviewed_pair(self) -> None:
        # PR #265 review: 32-bit keys collided on these two ordinary id sets.
        a = integrate.run_key_for(["issue_1000181", "issue_181"])
        b = integrate.run_key_for(["issue_1025538", "issue_25538"])
        self.assertNotEqual(a, b)
        self.assertEqual(len(a), 32)

    def test_nothing_to_fold(self) -> None:
        self.assertEqual(integrate.fold(self.cfg, []), {})
        no_patch = self._bundle("NP", patch=None)            # close/no-fix: nothing to ship
        self.assertEqual(integrate.fold(self.cfg, [no_patch]), {})

    def test_dry_run_shells_nothing(self) -> None:
        b = self._bundle("D1")
        with mock.patch("pdca_harness.integrate.subprocess.run") as m, \
                redirect_stdout(io.StringIO()) as out:
            folded = integrate.fold(self.cfg, [b], dry_run=True)
        self.assertEqual(folded, {("org/repo", "main"): ("pdca-integration/main", None)})
        m.assert_not_called()                                 # no git in a dry-run
        self.assertIn("pdca-integration/main", out.getvalue())

    def test_each_target_gets_its_own_integration_line(self) -> None:
        # A batch spanning two (repo, base) targets folds one line per target — not a single
        # global branch a sibling-target bundle would wrongly stack on (#187).
        a = self._bundle("S1", target="org/repo @ main")
        b = self._bundle("O1", target="other/repo @ develop")
        self.cfg.repo_checkouts["other/repo"] = str(self.tmp / "other")
        with redirect_stdout(io.StringIO()) as out:
            folded = integrate.fold(self.cfg, [a, b], dry_run=True)
        self.assertEqual(folded, {
            ("org/repo", "main"): ("pdca-integration/main", None),
            ("other/repo", "develop"): ("pdca-integration/develop", None)})
        self.assertNotIn("excluded", out.getvalue())          # nothing dropped anymore
        self.assertIn("fold 1 patch(es) onto pdca-integration/main", out.getvalue())
        self.assertIn("fold 1 patch(es) onto pdca-integration/develop", out.getvalue())


class FoldGit(unittest.TestCase):
    """Real git: fold patches onto the integration branch off a bare origin."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.origin = self.tmp / "origin.git"
        self.primary = self.tmp / "repo"
        subprocess.run(["git", "init", "--bare", "-q", str(self.origin)], check=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.primary)], check=True)
        self._cfg_git(self.primary)
        (self.primary / "base.txt").write_text("base\n", encoding="utf-8")
        self._git(self.primary, "add", "-A")
        self._git(self.primary, "commit", "-q", "-m", "base")
        self._git(self.primary, "remote", "add", "origin", str(self.origin))
        self._git(self.primary, "push", "-q", "origin", "main")
        self.cfg = _cfg(self.tmp, "org/repo", self.primary)

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _git(self, repo: Path, *args: str) -> None:
        subprocess.run(["git", "-C", str(repo), *args], check=True,
                       capture_output=True, text=True)

    def _cfg_git(self, repo: Path) -> None:
        self._git(repo, "config", "user.email", "t@example.com")
        self._git(repo, "config", "user.name", "Tester")
        self._git(repo, "config", "commit.gpgsign", "false")

    def _modify_patch(self, new_content: str) -> str:
        """A valid patch that rewrites base.txt to ``new_content`` (generated by git)."""
        (self.primary / "base.txt").write_text(new_content, encoding="utf-8")
        diff = subprocess.run(["git", "-C", str(self.primary), "diff"],
                              capture_output=True, text=True).stdout
        self._git(self.primary, "checkout", "--", "base.txt")
        return diff

    def _add_patch(self, name: str, content: str) -> str:
        """A valid patch that adds a new file (generated by git)."""
        (self.primary / name).write_text(content, encoding="utf-8")
        self._git(self.primary, "add", name)
        diff = subprocess.run(["git", "-C", str(self.primary), "diff", "--cached"],
                              capture_output=True, text=True).stdout
        self._git(self.primary, "reset", "-q", "HEAD", name)
        (self.primary / name).unlink()
        return diff

    def _bundle(self, iid: str, patch: str, *, publish: bool = True,
                remote: str = "origin", pr_url: str | None = None,
                pin: bool = True) -> Path:
        """A bundle as `publish` leaves it: its patch, and — the fold merges REAL PR
        branches since #593 — a signed commit on `fix/<iid>` pushed to ``remote``, recorded
        in publish.json with the exact commit pushed (``head_sha``; ``pin=False`` writes a
        record from before that field existed)."""
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True, exist_ok=True)
        (d / "brief.md").write_text(
            f"- **Slug:** {iid.lower()}\n- **Repo + branch target:** org/repo @ main\n",
            encoding="utf-8")
        (d / "patch.diff").write_text(patch, encoding="utf-8")
        if publish:
            branch = f"fix/{iid.lower()}"
            p = self.primary
            self._git(p, "fetch", "-q", "origin")
            self._git(p, "checkout", "-q", "-B", branch, "origin/main")
            self._git(p, "apply", str((d / "patch.diff").resolve()))
            self._git(p, "add", "-A")
            self._git(p, "commit", "-q", "-s", "-m", f"fix {iid}")
            self._git(p, "push", "-q", "-f", remote, branch)
            sha = subprocess.run(["git", "-C", str(p), "rev-parse", "HEAD"],
                                 capture_output=True, text=True).stdout.strip()
            self._git(p, "checkout", "-q", "main")
            rec = {"branch": branch, "remote": remote, "repo": "org/repo", "base": "main",
                   "pr_url": f"https://example/pr/{iid}" if pr_url is None else pr_url}
            if pin:
                rec["head_sha"] = sha
            (d / "publish.json").write_text(json.dumps(rec), encoding="utf-8")
        return d

    def _show(self, ref: str, path: str) -> subprocess.CompletedProcess:
        self._git(self.primary, "fetch", "-q", "origin")
        return subprocess.run(["git", "-C", str(self.primary), "show", f"origin/{ref}:{path}"],
                              capture_output=True, text=True)

    def _tip(self, branch: str) -> str:
        return subprocess.run(["git", "-C", str(self.primary), "ls-remote", "origin",
                               f"refs/heads/{branch}"], capture_output=True,
                              text=True).stdout.split()[0]

    def _pushed(self, branch: str) -> bool:
        out = subprocess.run(
            ["git", "-C", str(self.primary), "ls-remote", "--heads", "origin", branch],
            capture_output=True, text=True).stdout
        return branch in out

    def test_single_fold_pushes_branch(self) -> None:
        b = self._bundle("F1", self._modify_patch("one\n"))
        folded = integrate.fold(self.cfg, [b])
        branch, wt = folded[("org/repo", "main")]
        self.assertEqual(branch, "pdca-integration/main")
        self.assertIsNotNone(wt)
        self.assertEqual((wt / "base.txt").read_text(encoding="utf-8"), "one\n")
        self.assertTrue(self._pushed("pdca-integration/main"))

    def test_fold_commits_carry_a_dco_signoff(self) -> None:
        # #405: the integration branch is rebuilt each fold, so a stacked PR cut from an
        # earlier fold carries these commits outside the base's ancestry — where a
        # DCO-gated host inspects them. Sign them like publish does (#81).
        b = self._bundle("S1", self._modify_patch("one\n"))
        _, wt = integrate.fold(self.cfg, [b])[("org/repo", "main")]
        trailer = subprocess.run(
            ["git", "-C", str(wt), "log", "-1", "--format=%(trailers:key=Signed-off-by,valueonly)"],
            capture_output=True, text=True).stdout.strip()
        self.assertEqual(trailer, "Tester <t@example.com>")

    def test_two_batches_fold_onto_their_own_branches(self) -> None:
        """#591: two concurrent stack-mode runs on one base (two parallel tracks) must not
        share an integration branch — each fold rebuilds its branch with only ITS batch's
        patches and force-pushes it, so a shared branch hands one run the other's work."""
        a = self._bundle("A1", self._modify_patch("track a\n"))
        b = self._bundle("B1", self._add_patch("b.txt", "track b\n"))
        key_a, key_b = integrate.run_key_for(["issue_A1"]), integrate.run_key_for(["issue_B1"])
        self.assertNotEqual(key_a, key_b)
        branch_a, _ = integrate.fold(self.cfg, [a], run_key=key_a)[("org/repo", "main")]
        branch_b, wt = integrate.fold(self.cfg, [b], run_key=key_b)[("org/repo", "main")]
        self.assertEqual(branch_a, f"pdca-integration/r-{key_a}/main")
        self.assertNotEqual(branch_a, branch_b)
        self.assertTrue(self._pushed(branch_a) and self._pushed(branch_b))
        # B's fold left A's branch as A built it: A's change, not B's file.
        show = lambda ref, path: subprocess.run(
            ["git", "-C", str(self.primary), "show", f"origin/{ref}:{path}"],
            capture_output=True, text=True)
        self._git(self.primary, "fetch", "-q", "origin")
        self.assertEqual(show(branch_a, "base.txt").stdout, "track a\n")
        self.assertNotEqual(show(branch_a, "b.txt").returncode, 0)
        self.assertEqual(show(branch_b, "b.txt").stdout, "track b\n")
        self.assertEqual(show(branch_b, "base.txt").stdout, "base\n")

    def test_a_later_fold_appends_and_never_rewrites_an_earlier_one(self) -> None:
        """#593: the wave-1 fold must continue the wave-0 fold, not rebuild it — a PR
        stacked on the wave-0 fold's commits must still find them as ancestors."""
        b0 = self._bundle("W0", self._modify_patch("wave0\n"))
        branch, _ = integrate.fold(self.cfg, [b0], run_key="k")[("org/repo", "main")]
        tip0 = self._tip(branch)
        # A second apart, so a REBUILD (the pre-#593 fold) would mint a new commit even
        # for identical content — same-second rebuilds reproduce the same SHA and hide it.
        time.sleep(1.1)
        b1 = self._bundle("W1", self._add_patch("w1.txt", "wave1\n"))
        integrate.fold(self.cfg, [b0, b1], run_key="k")          # cumulative, as the flow passes
        tip1 = self._tip(branch)
        self.assertNotEqual(tip0, tip1)
        self._git(self.primary, "fetch", "-q", "origin")
        ancestry = subprocess.run(["git", "-C", str(self.primary), "merge-base", "--is-ancestor",
                              tip0, tip1])
        self.assertEqual(ancestry.returncode, 0, "the wave-0 fold was rewritten")

    def test_a_refold_of_the_same_batch_adds_nothing(self) -> None:
        b = self._bundle("R1", self._modify_patch("one\n"))
        branch, _ = integrate.fold(self.cfg, [b], run_key="k")[("org/repo", "main")]
        tip = self._tip(branch)
        time.sleep(1.1)   # a rebuild would now mint a new SHA (see the append test above)
        integrate.fold(self.cfg, [b], run_key="k")
        self.assertEqual(self._tip(branch), tip)

    def test_the_fold_carries_the_published_commits_themselves(self) -> None:
        """#593: the stacked branch inherits the predecessor's OWN commit (same SHA), so
        a dependent PR's diff shrinks to its own change once the predecessor merges."""
        b = self._bundle("S2", self._modify_patch("one\n"))
        branch, _ = integrate.fold(self.cfg, [b], run_key="k")[("org/repo", "main")]
        self._git(self.primary, "fetch", "-q", "origin")
        ancestry = subprocess.run(["git", "-C", str(self.primary), "merge-base", "--is-ancestor",
                              "origin/fix/s2", f"origin/{branch}"])
        self.assertEqual(ancestry.returncode, 0)

    def test_an_unpublished_bundle_cannot_be_stacked_on(self) -> None:
        b = self._bundle("U1", self._modify_patch("one\n"), publish=False)
        with self.assertRaises(integrate.IntegrationError) as caught:
            integrate.fold(self.cfg, [b])
        self.assertIn("no published branch", str(caught.exception))

    def test_a_pushed_branch_without_a_pr_cannot_be_stacked_on(self) -> None:
        # #266 review: publish writes publish.json with an empty pr_url when the push
        # worked but `gh pr create` failed; a predecessor with no PR can't merge bottom-up.
        b = self._bundle("N1", self._modify_patch("one\n"), pr_url="")
        with self.assertRaises(integrate.IntegrationError) as caught:
            integrate.fold(self.cfg, [b])
        self.assertIn("no PR", str(caught.exception))

    def test_the_fold_merges_the_pushed_commit_not_a_later_tip(self) -> None:
        # #266 review: a commit added to the PR branch after publish (a bot, another
        # actor) was never checked, so it must not ride into the next wave's base.
        b = self._bundle("P1", self._modify_patch("one\n"))
        p = self.primary
        self._git(p, "checkout", "-q", "fix/p1")
        (p / "extra.txt").write_text("unreviewed\n", encoding="utf-8")
        self._git(p, "add", "-A")
        self._git(p, "commit", "-q", "-s", "-m", "later")
        self._git(p, "push", "-q", "origin", "fix/p1")
        self._git(p, "checkout", "-q", "main")
        with self.assertRaises(integrate.IntegrationError) as caught:
            integrate.fold(self.cfg, [b], run_key="k")
        self.assertIn("moved since Check", str(caught.exception))
        self.assertFalse(self._pushed("pdca-integration/r-k/main"))

    def test_a_failed_fetch_stops_instead_of_folding_a_cached_copy(self) -> None:
        # #266 review: a cached remote-tracking ref must not stand in for a fetch that
        # failed.
        b = self._bundle("G1", self._modify_patch("one\n"))
        self._git(self.primary, "fetch", "-q", "origin")   # origin/fix/g1 is now cached
        self._git(self.primary, "config", "remote.origin.fetch",
                  "+refs/heads/does-not-exist:refs/remotes/origin/nothing")
        from unittest import mock
        real = integrate._git
        fail_fetch = lambda repo, *a: 1 if a[:1] == ("fetch",) and "fix/g1" in a else real(repo, *a)
        with mock.patch.object(integrate, "_git", fail_fetch):
            with self.assertRaises(integrate.IntegrationError) as caught:
                integrate.fold(self.cfg, [b], run_key="k")
        self.assertIn("could not fetch", str(caught.exception))

    def test_a_republished_bundle_replaces_its_earlier_fold_append_only(self) -> None:
        # #266 review: `signoff --iterate-do` rebuilds a bundle off the base and publish
        # force-updates its PR branch, so the new commit is not a descendant of the one
        # already folded. The fold reverts the earlier fold and merges the new commit —
        # the branch still only grows.
        b = self._bundle("I1", self._modify_patch("old\n"))
        branch, _ = integrate.fold(self.cfg, [b], run_key="k")[("org/repo", "main")]
        tip0 = self._tip(branch)
        b = self._bundle("I1", self._modify_patch("revised\n"))
        integrate.fold(self.cfg, [b], run_key="k")
        self.assertEqual(self._show(branch, "base.txt").stdout, "revised\n")
        ancestry = subprocess.run(["git", "-C", str(self.primary), "merge-base",
                                   "--is-ancestor", tip0, f"origin/{branch}"])
        self.assertEqual(ancestry.returncode, 0, "the earlier fold was rewritten")
        # Every commit the fold added carries a DCO sign-off, the revert included.
        log = subprocess.run(["git", "-C", str(self.primary), "log", "--first-parent",
                              "--format=%s|%(trailers:key=Signed-off-by,valueonly)",
                              f"{tip0}..origin/{branch}"],
                             capture_output=True, text=True).stdout.splitlines()
        log = [line for line in log if line]
        self.assertEqual(len(log), 2, log)
        self.assertTrue(log[1].startswith('Revert "pdca-integrate: issue_I1'), log)
        self.assertTrue(all(line.split("|")[1] for line in log), log)

    def test_a_branch_pushed_to_another_remote_is_fetched_from_it(self) -> None:
        # #266 review: an `Onto branch: <remote>/<branch>` publishes to that remote; the
        # fold must fetch it there, not a same-named branch on origin.
        fork = self.tmp / "fork.git"
        subprocess.run(["git", "init", "--bare", "-q", str(fork)], check=True)
        self._git(self.primary, "remote", "add", "fork", str(fork))
        decoy = self._bundle("R9", self._add_patch("decoy.txt", "origin\n"))  # fix/r9 on origin
        decoy.joinpath("publish.json").unlink()
        b = self._bundle("R9", self._add_patch("real.txt", "fork\n"), remote="fork")
        branch, _ = integrate.fold(self.cfg, [b], run_key="k")[("org/repo", "main")]
        self.assertEqual(self._show(branch, "real.txt").stdout, "fork\n")
        self.assertNotEqual(self._show(branch, "decoy.txt").returncode, 0)

    def test_a_record_without_head_sha_folds_the_current_tip(self) -> None:
        # Records written before head_sha existed still fold (the tip, with a warning).
        b = self._bundle("L1", self._modify_patch("one\n"), pin=False)
        with redirect_stderr(io.StringIO()) as err:
            branch, _ = integrate.fold(self.cfg, [b], run_key="k")[("org/repo", "main")]
        self.assertIn("predates head_sha", err.getvalue())
        self.assertEqual(self._show(branch, "base.txt").stdout, "one\n")

    def test_multi_disjoint_fold_carries_all(self) -> None:
        # A modify + an add (disjoint) both land on the branch — the multi-parent fold
        # the old _stack_base_branch parents[0] could not express.
        b1 = self._bundle("M1", self._modify_patch("one\n"))
        b2 = self._bundle("M2", self._add_patch("feature.txt", "hi\n"))
        branch, wt = integrate.fold(self.cfg, [b1, b2])[("org/repo", "main")]
        self.assertEqual((wt / "base.txt").read_text(encoding="utf-8"), "one\n")
        self.assertTrue((wt / "feature.txt").is_file())
        self.assertTrue(self._pushed(branch))

    def test_fold_fails_closed_when_the_integ_lock_is_unavailable(self) -> None:
        # #297 review rounds 6/7: the build runs under the worktree's lifecycle lock;
        # an unattainable lock ABORTS the fold — proceeding unserialized could
        # apply/push a mixed stack interleaved with another fold's commits.
        import contextlib as ctx
        from unittest import mock
        b = self._bundle("L1", self._modify_patch("one\n"))

        @ctx.contextmanager
        def unheld(wt, **kw):
            yield False

        with mock.patch.object(integrate, "integ_lock", unheld):
            with self.assertRaises(integrate.IntegrationError):
                integrate.fold(self.cfg, [b])
        self.assertFalse(self._pushed("pdca-integration/main"))  # nothing left origin

    def test_multi_target_locks_acquire_in_sorted_order(self) -> None:
        # #297 review round 11: with a caller-held locks stack, two concurrent
        # multi-target flows encountering their groups in opposite bundle order
        # would deadlock (each holding one lock, waiting on the other's). Groups are
        # processed in sorted (repo, base) order regardless of the accepted order,
        # so acquisition is globally consistent and concurrent folds serialize.
        import contextlib as ctx
        from unittest import mock
        self._git(self.primary, "push", "-q", "origin", "main:aa")
        b_main = self._bundle("O1", self._modify_patch("one\n"))
        b_aa = self.cfg.bundle("O2")
        b_aa.mkdir(parents=True)
        (b_aa / "brief.md").write_text(
            "- **Slug:** o2\n- **Repo + branch target:** org/repo @ aa\n",
            encoding="utf-8")
        (b_aa / "patch.diff").write_text(self._add_patch("f2.txt", "hi\n"),
                                         encoding="utf-8")
        # Published off `aa`, as publish would (the fold merges real branches, #593).
        self._git(self.primary, "fetch", "-q", "origin")
        self._git(self.primary, "checkout", "-q", "-B", "fix/o2", "origin/aa")
        self._git(self.primary, "apply", str((b_aa / "patch.diff").resolve()))
        self._git(self.primary, "add", "-A")
        self._git(self.primary, "commit", "-q", "-s", "-m", "fix O2")
        self._git(self.primary, "push", "-q", "-f", "origin", "fix/o2")
        self._git(self.primary, "checkout", "-q", "main")
        (b_aa / "publish.json").write_text(json.dumps(
            {"branch": "fix/o2", "repo": "org/repo", "base": "aa",
             "pr_url": "https://example/pr/O2"}), encoding="utf-8")
        order: list[str] = []
        real_lock = integrate.integ_lock

        @ctx.contextmanager
        def spy(wt, **kw):
            order.append(wt.name)
            with real_lock(wt, **kw) as held:
                yield held

        with mock.patch.object(integrate, "integ_lock", spy):
            integrate.fold(self.cfg, [b_main, b_aa])     # accepted order: main FIRST
        self.assertEqual(order, ["repo.pdca-integ-aa", "repo.pdca-integ-main"])

    def test_caller_stack_keeps_the_lock_held_after_fold(self) -> None:
        # #297 review round 10: with a caller-supplied ExitStack the integ lock
        # SURVIVES fold's return, covering the re-gate window — a concurrent sweep's
        # non-blocking probe finds the tree busy until the stack exits, so no gap
        # exists between fold and the re-gate attesting the tree.
        import contextlib as ctx
        b = self._bundle("K1", self._modify_patch("one\n"))
        with ctx.ExitStack() as locks:
            folded = integrate.fold(self.cfg, [b], locks=locks)
            _branch, wt = folded[("org/repo", "main")]
            with integrate.integ_lock(wt, wait=False) as held:
                self.assertFalse(held)               # still held by the stack
        with integrate.integ_lock(wt, wait=False) as held:
            self.assertTrue(held)                    # released with the stack

    def test_run_integration_fails_closed_when_the_integ_lock_is_unavailable(self) -> None:
        # Same contract for the between-waves re-gate: a result read from a tree a
        # concurrent fold could be rewriting would attest nothing.
        import contextlib as ctx
        from unittest import mock
        from pdca_harness import gates

        @ctx.contextmanager
        def unheld(wt, **kw):
            yield False

        with mock.patch.object(integrate, "integ_lock", unheld):
            with self.assertRaises(integrate.IntegrationError):
                gates.run_integration(self.cfg, self.primary)

    def test_overlap_raises_integration_error(self) -> None:
        # Two patches that each rewrite base.txt's only line — the second can't apply onto
        # the first, an undeclared cross-wave overlap → a loud STOP.
        b1 = self._bundle("C1", self._modify_patch("one\n"))
        b2 = self._bundle("C2", self._modify_patch("two\n"))
        with redirect_stderr(io.StringIO()):
            with self.assertRaises(integrate.IntegrationError):
                integrate.fold(self.cfg, [b1, b2])


class PointAtIntegration(unittest.TestCase):
    """`flow._point_at_integration` writes each later-wave bundle's stack base from the
    integration line matching *its own* (repo, base) — the second half of the #187 fix
    (fold tracks per target; the driver routes per target)."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.cfg = _cfg(self.tmp, "org/repo", self.tmp / "repo")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _bundle(self, iid: str, target: str) -> Path:
        d = self.cfg.bundle(iid)
        d.mkdir(parents=True)
        (d / "brief.md").write_text(
            f"- **Slug:** {iid.lower()}\n- **Repo + branch target:** {target}\n",
            encoding="utf-8")
        return d

    def test_routes_each_bundle_to_its_own_target_branch(self) -> None:
        from pdca_harness import flow, publish
        a = self._bundle("A", "org/repo @ main")
        b = self._bundle("B", "other/repo @ develop")
        c = self._bundle("C", "third/repo @ main")          # not integrated → off its own base
        integ = {("org/repo", "main"): "pdca-integration/main",
                 ("other/repo", "develop"): "pdca-integration/develop"}
        flow._point_at_integration(integ, [a, b, c])
        self.assertEqual(publish._read_stack_base(a), "pdca-integration/main")
        self.assertEqual(publish._read_stack_base(b), "pdca-integration/develop")  # not main!
        self.assertEqual(publish._read_stack_base(c), "")   # no integ line → builds off base

    def test_clears_a_stale_stack_base_for_an_un_integrated_target(self) -> None:
        # A bundle carrying a stack base from a prior/resumed run whose target isn't integrated
        # this run must have it CLEARED, else it builds against an old integration branch (#187).
        from pdca_harness import flow, publish
        d = self._bundle("D", "third/repo @ main")
        publish.write_stack_base(d, "pdca-integration/stale")     # left by a prior run
        flow._point_at_integration({("org/repo", "main"): "pdca-integration/main"}, [d])
        self.assertEqual(publish._read_stack_base(d), "")          # cleared → off its own base


if __name__ == "__main__":
    unittest.main()
