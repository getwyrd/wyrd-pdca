"""[driver].scratch_dir — throwaway heavy leaf work is redirected off tmpfs /tmp (#134),
and is bundle-scoped so it can be reclaimed at the publish/freeze boundary (#200).

Four promises:
  * config: `[driver].scratch_dir` parses; `PDCA_SCRATCH` overrides for one run;
    unset ⇒ "" (the knob fails open to the status quo).
  * CLI entry: `_export_scratch` creates the dir and exports BOTH `PDCA_SCRATCH` and
    `TMPDIR` so every subprocess inherits the redirect; unset ⇒ no-op; an uncreatable
    dir warns and falls back rather than aborting the run.
  * agent definitions: the four heavy-checkout leaves (builder, reviewer, adversary,
    code-review) carry the scratch discipline — `$TMPDIR` only helps tools that respect
    it; a model writing a literal `/tmp/wyrd-adv` needs the standing instruction.
  * lifetime (#200): each bundle gets its OWN subdir, and it is gone once that bundle is
    ready to publish. Reuse below that line (the auto-iterate rounds) is preserved; a
    crashed run's leftovers are reclaimed by pid stamp, never by a wall clock.

Offline: pure config/env, no leaves. Run from root:
    PYTHONPATH=src python -m unittest discover -s tests
"""

from __future__ import annotations

import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

from pdca_harness import cli, scratch, sweep
from pdca_harness.config import Config

ROOT = Path(__file__).resolve().parents[1]


class ScratchDirConfig(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        for var in ("PDCA_SCRATCH", "PDCA_LEAVES_MODE"):
            saved = os.environ.pop(var, None)
            if saved is not None:
                self.addCleanup(os.environ.__setitem__, var, saved)

    def _load(self, extra: str = "") -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n' + extra,
            encoding="utf-8",
        )
        return Config.load(self.tmp)

    def test_unset_is_empty_string(self) -> None:
        self.assertEqual(self._load().scratch_dir, "")

    def test_toml_value_parses(self) -> None:
        self.assertEqual(
            self._load('[driver]\nscratch_dir = "/var/tmp/pdca"\n').scratch_dir,
            "/var/tmp/pdca")

    def test_env_overrides_the_toml_for_one_run(self) -> None:
        os.environ["PDCA_SCRATCH"] = "/elsewhere"
        self.addCleanup(os.environ.pop, "PDCA_SCRATCH", None)
        self.assertEqual(
            self._load('[driver]\nscratch_dir = "/var/tmp/pdca"\n').scratch_dir,
            "/elsewhere")


class ScratchExport(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)

    def _cfg(self, scratch: str) -> Config:
        (self.tmp / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n',
            encoding="utf-8",
        )
        cfg = Config.load(self.tmp)
        cfg.scratch_dir = scratch
        return cfg

    def test_unset_is_a_no_op(self) -> None:
        env: dict = {}
        self.assertIsNone(cli._export_scratch(self._cfg(""), env))
        self.assertEqual(env, {})

    def test_creates_the_dir_and_exports_both_variables(self) -> None:
        target = self.tmp / "scratch" / "pdca"     # two levels: mkdir must use parents
        env: dict = {}
        got = cli._export_scratch(self._cfg(str(target)), env)
        self.assertEqual(got, target)
        self.assertTrue(target.is_dir())
        self.assertEqual(env["PDCA_SCRATCH"], str(target))
        self.assertEqual(env["TMPDIR"], str(target))

    def test_existing_unwritable_dir_warns_and_falls_back(self) -> None:
        # mkdir(exist_ok=True) succeeds on a pre-existing read-only dir; only a real
        # write probe catches it. Root ignores mode bits, so skip there.
        if os.geteuid() == 0:
            self.skipTest("mode bits don't bind root")
        ro = self.tmp / "ro"
        ro.mkdir()
        ro.chmod(0o555)
        self.addCleanup(ro.chmod, 0o755)
        env: dict = {}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(ro)), env)
        self.assertIsNone(got)
        self.assertEqual(env, {})
        self.assertIn("not usable", err.getvalue())

    def test_relative_scratch_dir_is_anchored_at_root_and_exported_absolute(self) -> None:
        # Children run with different cwds (worktree, temp sandbox); a relative value
        # exported verbatim would resolve differently — or not at all — in the leaf.
        env: dict = {}
        got = cli._export_scratch(self._cfg("scratch/rel"), env)
        expected = (self.tmp / "scratch" / "rel").resolve()
        self.assertEqual(got, expected)
        self.assertTrue(expected.is_dir())
        self.assertTrue(Path(env["PDCA_SCRATCH"]).is_absolute())
        self.assertEqual(env["PDCA_SCRATCH"], str(expected))
        self.assertEqual(env["TMPDIR"], str(expected))

    def test_symlink_loop_takes_the_fallback_not_a_crash(self) -> None:
        # resolve() raises on a self-referential symlink; that must take the documented
        # fail-open path (warn + None), never abort CLI startup.
        loop = self.tmp / "loop"
        loop.symlink_to(loop)
        env: dict = {"PDCA_SCRATCH": str(loop / "sub")}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(loop / "sub")), env)
        self.assertIsNone(got)
        self.assertNotIn("PDCA_SCRATCH", env)
        self.assertIn("not usable", err.getvalue())

    def test_rejected_env_override_is_cleared_on_fallback(self) -> None:
        # The bad root may have arrived via $PDCA_SCRATCH itself; falling back while the
        # variable survives hands every leaf the rejected path anyway (the role prompts
        # prefer $PDCA_SCRATCH over $TMPDIR). It must be cleared. A pre-set TMPDIR is the
        # operator's and stays.
        blocker = self.tmp / "blocking-file"
        blocker.write_text("not a dir", encoding="utf-8")
        env = {"PDCA_SCRATCH": str(blocker / "sub"), "TMPDIR": "/operator/tmp"}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(blocker / "sub")), env)
        self.assertIsNone(got)
        self.assertNotIn("PDCA_SCRATCH", env)
        self.assertEqual(env["TMPDIR"], "/operator/tmp")

    def test_uncreatable_dir_warns_and_falls_back(self) -> None:
        blocker = self.tmp / "file"
        blocker.write_text("not a dir", encoding="utf-8")
        env: dict = {}
        err = io.StringIO()
        with redirect_stderr(err):
            got = cli._export_scratch(self._cfg(str(blocker / "sub")), env)
        self.assertIsNone(got)                      # fail open: the run proceeds on /tmp
        self.assertEqual(env, {})
        self.assertIn("not usable", err.getvalue())


class ScratchDiscipline(unittest.TestCase):
    def test_the_four_heavy_leaves_carry_the_instruction(self) -> None:
        # $TMPDIR only redirects tools that respect it; a model inventing a literal
        # /tmp/... path needs the standing instruction in its role prompt. The wrappers
        # are covered transitively by test_role_prompts (wrapper == canonical body).
        for leaf in ("builder", "reviewer", "adversary", "code-review"):
            with self.subTest(leaf=leaf):
                body = (ROOT / "agents" / f"{leaf}.md").read_text(encoding="utf-8")
                self.assertIn("$PDCA_SCRATCH", body)
                self.assertIn(f"pdca-{leaf}-<issue>-*", body)
                # The worked example must use the shell-safe fallback chain: a bare
                # `"$PDCA_SCRATCH/..."` with the variable unset expands to a
                # filesystem-root `/pdca-...` path (PR #137 review).
                self.assertIn("${PDCA_SCRATCH:-${TMPDIR:-/tmp}}", body)


class BundleScratch(unittest.TestCase):
    """#200 — the scratch root gets an owner: one subdir per bundle, reclaimed at the
    publish/freeze boundary. Before this, every bundle's throwaway work landed in one flat
    directory that nothing emptied (96 GB / 3,930 stale dirs, and the page cache it
    generated OOM-killed a 3d 19h run)."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        saved = os.environ.pop("PDCA_SCRATCH", None)
        self.addCleanup(lambda: os.environ.__setitem__("PDCA_SCRATCH", saved)
                        if saved is not None else os.environ.pop("PDCA_SCRATCH", None))
        self.cfg = self._cfg(self.tmp)
        self.scratch_root = self.tmp / "scratch"
        self.scratch_root.mkdir()
        # What `cli._export_scratch` does at CLI entry — and the ONLY thing scratch.root()
        # reads, so that a root the CLI rejected stays rejected (see the P1 test below).
        os.environ["PDCA_SCRATCH"] = str(self.scratch_root)

    def _cfg(self, root: Path) -> Config:
        root.mkdir(parents=True, exist_ok=True)
        (root / "pdca.toml").write_text(
            '[project]\ndefault_branch = "main"\n'
            '[leaves.builder]\nmode = "stub"\n'
            '[leaves.reviewer]\nmode = "stub"\n',
            encoding="utf-8",
        )
        return Config.load(root)

    def _bundle(self, name: str = "issue_651") -> Path:
        d = self.cfg.bundle_root / name
        d.mkdir(parents=True, exist_ok=True)
        return d

    # --- scoping -----------------------------------------------------------------
    def test_each_bundle_gets_its_own_dir_and_env(self) -> None:
        a, b = self._bundle("issue_651"), self._bundle("issue_652")
        ea, eb = scratch.env_for(self.cfg, a), scratch.env_for(self.cfg, b)
        self.assertNotEqual(ea["PDCA_SCRATCH"], eb["PDCA_SCRATCH"])
        # BOTH variables move together: $PDCA_SCRATCH is what the role prompts name, and
        # $TMPDIR is what catches everyone who never heard of it (cargo's .tmp* dirs).
        for e in (ea, eb):
            self.assertEqual(e["PDCA_SCRATCH"], e["TMPDIR"])
            self.assertTrue(Path(e["PDCA_SCRATCH"]).is_dir())
        self.assertEqual(Path(ea["PDCA_SCRATCH"]).name, "issue_651")

    def test_unset_scratch_dir_is_byte_for_byte_the_old_behaviour(self) -> None:
        os.environ.pop("PDCA_SCRATCH", None)
        self.cfg.scratch_dir = ""
        self.assertIsNone(scratch.for_bundle(self.cfg, self._bundle()))
        self.assertEqual(scratch.env_for(self.cfg, self._bundle()), {})
        self.assertEqual(scratch.reclaim(self.cfg, [self._bundle()]), [])

    def test_a_rejected_root_is_not_resurrected_from_the_config(self) -> None:
        # `_export_scratch` POPS $PDCA_SCRATCH when the configured dir is unusable, so the run
        # falls back to the default temp location. Reading cfg.scratch_dir as a fallback would
        # hand every leaf and gate the directory the CLI just rejected and warned about — the
        # P1 from the #207 review.
        os.environ.pop("PDCA_SCRATCH", None)
        self.cfg.scratch_dir = "/definitely/not/usable"
        self.assertIsNone(scratch.root(self.cfg))
        self.assertEqual(scratch.env_for(self.cfg, self._bundle()), {})

    def test_two_projects_sharing_a_root_do_not_collide(self) -> None:
        # scratch_dir is machine-wide (/var/tmp/pdca ships as the value) while issue_<id> is
        # unique only within one project. Without the project segment, two projects' issue_651
        # share a directory: one inherits the other's build trees, then deletes them.
        other = self._cfg(self.tmp / "other-project")
        d = self._bundle("issue_651")
        mine = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        (other.bundle_root / "issue_651").mkdir(parents=True)
        theirs = Path(scratch.env_for(other, other.bundle_root / "issue_651")["PDCA_SCRATCH"])
        self.assertNotEqual(mine, theirs)
        self.assertEqual(mine.name, theirs.name)          # same bundle name…
        self.assertNotEqual(mine.parent, theirs.parent)   # …different project slice
        # And our sweep must not see, let alone reclaim, the other project's dir.
        scratch._stamp(theirs).write_text("999999\n", encoding="utf-8")
        self.assertNotIn(theirs, scratch.orphans(self.cfg))
        scratch.reclaim(self.cfg, [d])
        self.assertTrue(theirs.is_dir())

    # --- the deadline ------------------------------------------------------------
    def test_reclaimed_at_the_boundary(self) -> None:
        d = self._bundle()
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        (p / "cargo-target-ci").mkdir()          # the 19 GB tree, in miniature
        (p / ".tmpABC").mkdir()                  # cargo's own, carrying no bundle identity
        lines = scratch.reclaim(self.cfg, [d])
        self.assertFalse(p.exists())
        self.assertFalse(scratch._stamp(p).exists())
        self.assertTrue(any("issue_651" in ln for ln in lines), lines)

    def test_survives_iteration_so_a_rebuild_stays_warm(self) -> None:
        # The #422 property: reuse BELOW the publish line is untouched. Re-resolving the
        # bundle's scratch (as each Do→Check round does) must not wipe the build tree.
        d = self._bundle()
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        (p / "cargo-target-ci").mkdir()
        again = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        self.assertEqual(p, again)
        self.assertTrue((p / "cargo-target-ci").is_dir())

    def test_dry_run_reports_without_removing(self) -> None:
        d = self._bundle()
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        lines = scratch.reclaim(self.cfg, [d], dry_run=True)
        self.assertTrue(p.is_dir())
        self.assertTrue(any("would" in ln for ln in lines), lines)

    # --- the crash backstop ------------------------------------------------------
    def test_orphan_with_a_dead_owner_is_reclaimed(self) -> None:
        d = self._bundle("issue_999")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).write_text("999999999\n", encoding="utf-8")   # provably not a pid
        self.assertEqual(scratch.orphans(self.cfg), [p])
        scratch.reclaim(self.cfg, [])          # no bundles named — orphan pass alone
        self.assertFalse(p.exists())

    def test_live_owner_is_left_alone(self) -> None:
        # A dir stamped with another LIVE pid may be a concurrent flow's $TMPDIR; removing
        # it would fail that run's gates. Same rule as worktree.orphan_overflow_dirs.
        d = self._bundle("issue_998")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).write_text(f"{os.getppid()}\n", encoding="utf-8")
        self.assertEqual(scratch.orphans(self.cfg), [])
        lines = scratch.reclaim(self.cfg, [d])
        self.assertTrue(p.is_dir())
        self.assertTrue(any("still alive" in ln for ln in lines), lines)

    def test_a_second_live_owner_blocks_reclaim(self) -> None:
        # Two processes on one bundle share the directory. A single overwritten pid would let
        # whichever stamped last delete it at its own boundary while the other still has a
        # leaf or gate using it as $TMPDIR — so the stamp is a LIST (#207 review).
        d = self._bundle("issue_996")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).write_text(f"{os.getppid()}\n{os.getpid()}\n", encoding="utf-8")
        lines = scratch.reclaim(self.cfg, [d])
        self.assertTrue(p.is_dir(), "reclaimed while another live process still owned it")
        self.assertTrue(any("still alive" in ln for ln in lines), lines)
        self.assertEqual(scratch.orphans(self.cfg), [])

    def test_claiming_prunes_dead_owners_but_keeps_live_ones(self) -> None:
        d = self._bundle("issue_995")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).write_text(f"999999999\n{os.getppid()}\n", encoding="utf-8")
        scratch.for_bundle(self.cfg, d)                       # re-claim
        owners = scratch._owners(p)
        self.assertIn(os.getpid(), owners)
        self.assertIn(os.getppid(), owners)                   # live: kept
        self.assertNotIn(999999999, owners)                   # dead: pruned

    def test_an_out_of_range_pid_is_unclassifiable_not_a_crash(self) -> None:
        # os.kill raises OverflowError — not an OSError, so worktree._pid_alive does not catch
        # it — on an integer outside pid_t. A corrupt sidecar must not take `pdca sweep` down.
        d = self._bundle("issue_994")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).write_text("9" * 40 + "\n", encoding="utf-8")
        self.assertIsNone(scratch._owners(p))
        self.assertEqual(scratch.orphans(self.cfg), [])       # no crash, and left alone
        self.assertEqual(sweep.sweep(self.cfg, []), [])

    def test_a_failed_removal_keeps_the_stamp_discoverable(self) -> None:
        # ignore_errors hides a transient permission/mount failure. Dropping the sidecar anyway
        # would leave a potentially huge tree that neither orphans() nor `pdca sweep` can ever
        # see again — the permanent residue this module exists to stop.
        if os.geteuid() == 0:
            self.skipTest("mode bits don't bind root")
        d = self._bundle("issue_993")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        (p / "sub").mkdir()
        p.chmod(0o555)                                        # rmtree of `sub` will fail
        self.addCleanup(p.chmod, 0o755)
        lines = scratch.reclaim(self.cfg, [d])
        self.assertTrue(p.is_dir())
        self.assertTrue(scratch._stamp(p).exists(), "stamp dropped — the tree is now invisible")
        self.assertTrue(any("could not remove" in ln for ln in lines), lines)

    def test_missing_stamp_is_never_licence_to_delete(self) -> None:
        # An unstamped dir proves nothing (an older run, or a leaf that wiped the sidecar).
        # Never reclaim what cannot be classified.
        d = self._bundle("issue_997")
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        scratch._stamp(p).unlink()
        self.assertEqual(scratch.orphans(self.cfg), [])
        self.assertTrue(p.is_dir())

    # --- wiring ------------------------------------------------------------------
    def test_sweep_reclaims_scratch_even_when_worktree_sweeping_is_off(self) -> None:
        # sweep_worktrees expresses a preference about WORKTREES. "a bundle's temp data is
        # gone once it is ready to publish" is policy, and must not switch off with it.
        d = self._bundle()
        p = Path(scratch.env_for(self.cfg, d)["PDCA_SCRATCH"])
        self.cfg.sweep_worktrees = "off"
        lines = sweep.sweep(self.cfg, [d])
        self.assertFalse(p.exists())
        self.assertTrue(any("leaf scratch" in ln for ln in lines), lines)


if __name__ == "__main__":
    unittest.main()
