"""Resolution tests for `scripts/pdca` — the launcher the T4-contribution gate row runs.

The row used to name `.venv/bin/wyrd-pdca` directly, which is one install layout of
several: a source run (`PYTHONPATH=src python -m pdca_harness.cli`) or a pipx / system
install has no such file, so the gate exited 127 on a harness that was at that moment
running fine — on every patch-bearing Check, and again at publish (PR #184 review).

These run the real script against synthetic project roots (a copy of it in a tmp tree),
so each branch of the chain is exercised as the shell actually takes it. Every stub CLI
prints a marker plus its argv, which pins BOTH which one was chosen and that the
arguments reached it unmangled.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "pdca"


def _exe(path: Path, body: str) -> Path:
    """Write an executable /bin/sh stub."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!/bin/sh\n{body}\n", encoding="utf-8")
    path.chmod(0o755)
    return path


class PdcaLauncherTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.root = self.tmp / "project"
        (self.root / "scripts").mkdir(parents=True)
        self.script = self.root / "scripts" / "pdca"
        shutil.copyfile(_SCRIPT, self.script)
        self.script.chmod(0o755)
        # An empty PATH dir by default: nothing resolves unless a test puts it there.
        self.pathdir = self.tmp / "bin"
        self.pathdir.mkdir()

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, *args: str, env: dict[str, str] | None = None,
             cwd: Path | None = None) -> subprocess.CompletedProcess:
        # A deliberately minimal env: PDCA_CLI never leaks in from the developer's shell,
        # and PATH holds only this test's stubs plus the system dirs `env`/`sh` live in.
        full = {"PATH": f"{self.pathdir}:/usr/bin:/bin", "HOME": str(self.tmp)}
        full.update(env or {})
        return subprocess.run([str(self.script), *args], capture_output=True, text=True,
                              env=full, cwd=str(cwd or self.root))

    def _venv_cli(self, rel: str = ".venv/bin/wyrd-pdca") -> None:
        _exe(self.root / rel, 'echo "VENV $@"')

    def _path_cli(self) -> None:
        _exe(self.pathdir / "wyrd-pdca", 'echo "PATH $@"')

    def _source_tree(self) -> None:
        # A stand-in package: the launcher's job is to invoke `python -m pdca_harness.cli`
        # with src on PYTHONPATH — whether the real module works is other tests' business.
        pkg = self.root / "src" / "pdca_harness"
        pkg.mkdir(parents=True)
        (pkg / "__init__.py").write_text("", encoding="utf-8")
        (pkg / "cli.py").write_text(
            "import sys\nprint('SOURCE ' + ' '.join(sys.argv[1:]))\n", encoding="utf-8")

    # --- the chain, in order -------------------------------------------------------

    def test_pdca_cli_override_wins_over_everything(self) -> None:
        self._venv_cli()
        self._path_cli()
        override = _exe(self.tmp / "custom-cli", 'echo "OVERRIDE $@"')
        got = self._run("contribcheck", env={"PDCA_CLI": str(override)})
        self.assertEqual(got.stdout.strip(), "OVERRIDE contribcheck", got.stderr)

    def test_pdca_cli_override_may_be_a_command_line_not_just_a_path(self) -> None:
        """`PDCA_CLI="python -m pdca_harness.cli"` is the documented source invocation —
        it has to survive as argv, not be exec'd as one long filename."""
        self._source_tree()
        got = self._run("contribcheck", env={
            "PDCA_CLI": f"env PYTHONPATH={self.root}/src python3 -m pdca_harness.cli"})
        self.assertEqual(got.stdout.strip(), "SOURCE contribcheck", got.stderr)

    def test_pdca_cli_override_takes_an_unquoted_spaced_path_whole(self) -> None:
        """PR #184 review r2: whitespace-splitting an override reintroduces the 127 this
        launcher exists to remove — through the escape hatch, on exactly the platforms
        (macOS, Windows) where a spaced install path is ordinary. A value that names an
        executable is one word, whatever it contains."""
        spaced = _exe(self.tmp / "My Tools" / "wyrd-pdca", 'echo "SPACED $@"')
        got = self._run("contribcheck", env={"PDCA_CLI": str(spaced)})
        self.assertEqual(got.stdout.strip(), "SPACED contribcheck", got.stderr)

    def test_pdca_cli_override_honours_quoting(self) -> None:
        """A spaced path WITH arguments can only be expressed by quoting it, so the split
        has to read quotes the way the command line it claims to be would."""
        spaced = _exe(self.tmp / "My Tools" / "wyrd-pdca", 'echo "SPACED $@"')
        got = self._run("contribcheck", env={"PDCA_CLI": f'"{spaced}" --flag'})
        self.assertEqual(got.stdout.strip(), "SPACED --flag contribcheck", got.stderr)

    def test_pdca_cli_override_honours_backslash_escapes(self) -> None:
        """The other way to spell it."""
        spaced = _exe(self.tmp / "My Tools" / "wyrd-pdca", 'echo "SPACED $@"')
        got = self._run("contribcheck", env={"PDCA_CLI": str(spaced).replace(" ", r"\ ")})
        self.assertEqual(got.stdout.strip(), "SPACED contribcheck", got.stderr)

    def test_an_unparseable_override_fails_closed(self) -> None:
        """An override that doesn't parse must not fall through to `exec "$@"` — that
        would run `contribcheck` as a command and, on a gating row, whatever it exits
        with decides the gate. 127 and say which value broke."""
        got = self._run("contribcheck", env={"PDCA_CLI": '"/opt/unbalanced'})
        self.assertEqual(got.returncode, 127)
        self.assertIn("PDCA_CLI", got.stderr)
        self.assertIn("/opt/unbalanced", got.stderr)

    def test_an_override_that_names_no_command_fails_closed(self) -> None:
        """Set-but-whitespace parses cleanly to an empty argv — also 127, not a silent
        `exec` of the arguments."""
        got = self._run("contribcheck", env={"PDCA_CLI": "   "})
        self.assertEqual(got.returncode, 127)
        self.assertIn("names no command", got.stderr)

    def test_a_non_executable_override_fails_closed_rather_than_falling_back(self) -> None:
        """A directory (`-x` is true of any searchable one, which is why the whole-path
        branch also tests `-f`) or a file without `+x`: the launcher does NOT quietly fall
        through to the next candidate. An override that was meant to be used and cannot be
        is an operator error, and on a gating row a silent substitution is worse than a
        refusal — 126 is the shell's own 'found it, cannot run it', and it names the path."""
        self._venv_cli()  # present, and deliberately NOT used as a consolation prize
        (self.tmp / "not-a-cli").mkdir()
        unrunnable = self.tmp / "no-plus-x"
        unrunnable.write_text("#!/bin/sh\necho nope\n", encoding="utf-8")
        unrunnable.chmod(0o644)
        for target in (self.tmp / "not-a-cli", unrunnable):
            with self.subTest(target=target.name):
                got = self._run("contribcheck", env={"PDCA_CLI": str(target)})
                self.assertEqual(got.returncode, 126)
                self.assertIn(target.name, got.stderr)

    def test_project_venv_is_preferred_over_path(self) -> None:
        """The venv is THIS project's pinned install; PATH is whatever the machine has."""
        self._venv_cli()
        self._path_cli()
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "VENV contribcheck", got.stderr)

    def test_windows_venv_layout_resolves(self) -> None:
        """`scripts/install.ps1` produces `.venv/Scripts/wyrd-pdca.exe`, not `bin/`."""
        _exe(self.root / ".venv/Scripts/wyrd-pdca.exe", 'echo "WINVENV $@"')
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "WINVENV contribcheck", got.stderr)

    def test_this_checkouts_source_beats_a_console_script_on_path(self) -> None:
        """The order that matters in practice: a `git worktree` of this repo has no .venv
        of its own, and the `wyrd-pdca` it finds on PATH is the MAIN checkout's install —
        so PATH-first would lint this bundle with a different tree's harness (and, right
        after a fix like this one, with the pre-fix harness). `$here/src` is certainly
        this tree's code, and the package is dependency-free, so it goes first."""
        self._path_cli()
        self._source_tree()
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "SOURCE contribcheck", got.stderr)

    def test_falls_back_to_the_console_script_on_path(self) -> None:
        """A pipx / system install: no venv AND no src in the tree, `wyrd-pdca` on PATH."""
        self._path_cli()
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "PATH contribcheck", got.stderr)

    def test_bare_pdca_on_path_is_never_used(self) -> None:
        """pyproject.toml names the console script `wyrd-pdca` precisely because a bare
        `pdca` on this machine is a SIBLING project's install. Running the wrong harness
        against this bundle is worse than finding none, so the chain must skip it."""
        _exe(self.pathdir / "pdca", 'echo "SIBLING $@"')
        self._source_tree()
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "SOURCE contribcheck", got.stderr)

    def test_falls_back_to_the_source_tree_with_no_install_at_all(self) -> None:
        self._source_tree()
        got = self._run("contribcheck")
        self.assertEqual(got.stdout.strip(), "SOURCE contribcheck", got.stderr)

    def test_nothing_resolvable_exits_127_with_an_actionable_message(self) -> None:
        """The failure the hardcoded path produced silently, now named: 127 is 'no such
        command', and the message has to say which layouts were looked for."""
        got = self._run("contribcheck")
        self.assertEqual(got.returncode, 127)
        self.assertIn("PDCA_CLI", got.stderr)
        self.assertIn(".venv", got.stderr)

    # --- invocation contract -------------------------------------------------------

    def test_arguments_are_forwarded_verbatim(self) -> None:
        """Including one with a space: the gate runner may pass a bundle path, and word
        splitting there would silently lint the wrong (or no) bundle."""
        self._venv_cli()
        got = self._run("contribcheck", "--no-issue", "issue 266")
        self.assertEqual(got.stdout.strip(), "VENV contribcheck --no-issue issue 266",
                         got.stderr)

    def test_exit_code_is_the_cli_s_own(self) -> None:
        """It is a gate: a nonzero from the checker must arrive as this process's status,
        not be swallowed by the wrapper."""
        _exe(self.root / ".venv/bin/wyrd-pdca", "exit 3")
        self.assertEqual(self._run("contribcheck").returncode, 3)

    def test_resolution_is_relative_to_the_script_not_the_cwd(self) -> None:
        """Gates run with cwd at the project root today, but the launcher must not depend
        on that — `here` is derived from the script's own path."""
        self._venv_cli()
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        got = self._run("contribcheck", cwd=elsewhere)
        self.assertEqual(got.stdout.strip(), "VENV contribcheck", got.stderr)


class ShippedLauncherTest(unittest.TestCase):
    """The real file, as the gate row names it."""

    def test_the_gate_row_invokes_an_executable_launcher(self) -> None:
        self.assertTrue(_SCRIPT.is_file(), "pdca.toml's T4-contribution row names it")
        self.assertTrue(os.access(_SCRIPT, os.X_OK), "a gate `cmd` must be executable")

    def test_pdca_toml_routes_contribcheck_through_it(self) -> None:
        """Pins the fix itself: no hardcoded interpreter/venv path back in the row."""
        toml = (Path(__file__).resolve().parents[1] / "pdca.toml").read_text(encoding="utf-8")
        row = next(ln for ln in toml.splitlines() if '"T4-contribution"' in ln)
        self.assertIn('cmd = "scripts/pdca contribcheck"', row)


if __name__ == "__main__":
    unittest.main()
