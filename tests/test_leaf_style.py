"""Offline slice for the per-leaf prose style (issue #235; upstream eduralph/pdca-harness#535).

The prose-report leaves' output used to have no governed SHAPE: role prompts asked for
content and the reports narrated chronologically, burying the verdict at the bottom —
costliest at sign-off, the one gate where a human is deliberately in the loop. The
``style_file`` leaf key renders their prose through the Minto Pyramid style, family-mapped
at spawn (an INSTANCE DELTA until the upstream issue ships it template-side).

Proves the mapping, offline: the claude family gets the style body — frontmatter
stripped — appended to the SYSTEM prompt via argv (``--append-system-prompt`` with
inline text: the sizer/splitter spawn with cwd = the bundle dir, where a cwd-relative
path is a hard CLI error; and argv, not the prompt, keeps an interactive REPL's seed
clean), while an inline family (the codex reviewer) gets it prepended to the task
prompt the same way its role body rides; explicit argv already carrying an
``--append-system-prompt`` flag wins; an unreadable, undecodable, empty, or unset
style file degrades to a byte-identical spawn, never a crashed leaf, and a path that
escapes the project root (absolute, ``..`` traversal — the shapes the rubric loader
refuses) is refused rather than read into a prompt. The key also survives the two
places a named-table key historically got dropped: an escalation/variant spec inherits
its base leaf's style (the ``memory_max`` lesson) and the array-form advisory
constructor passes it through; and the sizer's cache key covers the style path and
body, so wiring or editing the style earns a fresh verdict instead of reusing the
differently-shaped cached one. And — against THIS repo's real ``pdca.toml`` — exactly
the prose-report leaves are wired: planner, signoff, act, sizer, splitter, reviewer;
never the builder or publisher (output consumed structurally) and never via a
dangling path.

Run from the project root:
    PYTHONPATH=src python -m unittest discover -s tests
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from pdca_harness import families, leaves
from pdca_harness.config import Config, LeafConfig

ROOT = Path(__file__).resolve().parents[1]

STYLE = ("---\nname: Test Style\ndescription: x\n---\n\n"
         "# Report shape\n\nConclusion first, then grouped support.\n")
BODY = "# Report shape\n\nConclusion first, then grouped support."


def _cfg(tmp: str, text: str | None = STYLE) -> SimpleNamespace:
    root = Path(tmp)
    if text is not None:
        p = root / ".claude" / "output-styles" / "s.md"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
    return SimpleNamespace(root=root)


def _leaf(**kw) -> LeafConfig:
    kw.setdefault("style_file", ".claude/output-styles/s.md")
    return LeafConfig(mode="command", **kw)


class StyleInjection(unittest.TestCase):
    CLAUDE = families.resolve("claude", None)
    CODEX = families.resolve("codex", None)

    def test_claude_gets_the_body_on_the_system_prompt_frontmatter_stripped(self):
        with tempfile.TemporaryDirectory() as tmp:
            argv, prefix = leaves._style_injection(
                _cfg(tmp), _leaf(family="claude", argv=["claude", "-p"]), self.CLAUDE)
        self.assertEqual(argv, ["--append-system-prompt", BODY])
        self.assertEqual(prefix, "")  # never the prompt: an interactive seed stays clean

    def test_inline_family_gets_a_prompt_prefix_not_argv(self):
        with tempfile.TemporaryDirectory() as tmp:
            argv, prefix = leaves._style_injection(
                _cfg(tmp), _leaf(family="codex", argv=["codex", "exec"]), self.CODEX)
        self.assertEqual(argv, [])
        self.assertEqual(prefix, BODY + "\n\n---\n\n")

    def test_explicit_argv_wins(self):
        # The escape-hatch idiom every mapped key follows: a leaf whose argv already
        # carries the flag (either variant) is left alone.
        for flag in ("--append-system-prompt", "--append-system-prompt-file"):
            with tempfile.TemporaryDirectory() as tmp:
                argv, prefix = leaves._style_injection(
                    _cfg(tmp), _leaf(family="claude", argv=["claude", flag, "x"]),
                    self.CLAUDE)
            self.assertEqual((argv, prefix), ([], ""))

    def test_unset_missing_or_empty_style_degrades_to_no_styling(self):
        with tempfile.TemporaryDirectory() as tmp:
            unset = leaves._style_injection(
                _cfg(tmp), _leaf(family="claude", style_file=""), self.CLAUDE)
            self.assertEqual(unset, ([], ""))
        with tempfile.TemporaryDirectory() as tmp:  # file absent — degrade, don't crash
            missing = leaves._style_injection(
                _cfg(tmp, text=None), _leaf(family="claude"), self.CLAUDE)
            self.assertEqual(missing, ([], ""))
        with tempfile.TemporaryDirectory() as tmp:  # frontmatter-only ⇒ empty body
            empty = leaves._style_injection(
                _cfg(tmp, text="---\nname: x\n---\n"), _leaf(family="claude"),
                self.CLAUDE)
            self.assertEqual(empty, ([], ""))

    def test_no_cfg_degrades_to_no_styling(self):
        # The legacy `_invoke(cfg=None)` shape: a styled leaf without a config to
        # resolve the root against must degrade, not raise on `cfg.root`.
        got = leaves._style_injection(None, _leaf(family="claude"), self.CLAUDE)
        self.assertEqual(got, ([], ""))

    def test_a_body_over_the_argv_bound_degrades_instead_of_crashing_exec(self):
        # One argv element carries the body; past MAX_ARG_STRLEN execve fails
        # E2BIG — a crash, not a degrade. Bounded, loudly, on the argv branch;
        # the inline branch has no per-argument limit and keeps the big body.
        with tempfile.TemporaryDirectory() as tmp:
            cfg = _cfg(tmp, text="x" * (leaves._STYLE_ARGV_CAP + 1))
            claude = leaves._style_injection(cfg, _leaf(family="claude"), self.CLAUDE)
            self.assertEqual(claude, ([], ""))
            inline_argv, inline_prefix = leaves._style_injection(
                cfg, _leaf(family="codex"), self.CODEX)
            self.assertEqual(inline_argv, [])
            self.assertTrue(inline_prefix)

    def test_undecodable_style_degrades_instead_of_crashing_the_leaf(self):
        with tempfile.TemporaryDirectory() as tmp:
            cfg = _cfg(tmp, text=None)
            p = Path(tmp) / ".claude" / "output-styles" / "s.md"
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(b"\xff\xfe not utf-8 \xff")
            got = leaves._style_injection(cfg, _leaf(family="claude"), self.CLAUDE)
        self.assertEqual(got, ([], ""))

    def test_a_root_escaping_style_path_is_refused_not_read(self):
        # The shapes rubric._resolve refuses: an absolute join silently discards the
        # root, and `..` walks out of it — neither may read a host file into a prompt.
        with tempfile.TemporaryDirectory() as tmp:
            outside = Path(tmp) / "outside.md"
            outside.write_text("host file\n", encoding="utf-8")
            root = Path(tmp) / "project"
            root.mkdir()
            for rel in (str(outside), "../outside.md"):
                got = leaves._style_injection(
                    SimpleNamespace(root=root), _leaf(family="claude", style_file=rel),
                    self.CLAUDE)
                self.assertEqual(got, ([], ""), f"style_file={rel!r} must be refused")

    def test_an_escalation_spec_inherits_its_base_leafs_style(self):
        # The memory_max lesson: a per-leaf key an escalation/variant spec omits must
        # inherit from the base leaf, not silently reset — and a spec's own value wins.
        base = _leaf(family="claude", argv=["claude"])
        inherited = leaves._leaf_from_spec({"argv": ["claude", "--model", "opus"]}, base)
        self.assertEqual(inherited.style_file, base.style_file)
        overridden = leaves._leaf_from_spec({"style_file": "other.md"}, base)
        self.assertEqual(overridden.style_file, "other.md")

    def test_the_array_form_advisory_constructor_passes_the_key_through(self):
        leaf = leaves._advisory_leaf(
            {"mode": "command", "family": "claude", "argv": ["claude"],
             "style_file": ".claude/output-styles/s.md"}, "advisory", "x")
        self.assertEqual(leaf.style_file, ".claude/output-styles/s.md")
        # …and a spec without the key defaults to unstyled, not to some phantom path.
        bare = leaves._advisory_leaf(
            {"mode": "command", "family": "claude", "argv": ["claude"]}, "advisory", "x")
        self.assertEqual(bare.style_file, "")

    def test_the_sizer_cache_key_covers_the_style_path_and_body(self):
        # "The CONFIGURATION is an input too": wiring or editing the sizer's style
        # must earn a fresh verdict, not reuse the differently-shaped cached one.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bp = root / "brief.md"
            bp.write_text("# brief\n", encoding="utf-8")
            style = root / ".claude" / "output-styles" / "s.md"
            style.parent.mkdir(parents=True)
            style.write_text("Conclusion first.\n", encoding="utf-8")
            def key(style_file):
                cfg = SimpleNamespace(
                    root=root, sizer_escalation=[],
                    sizer=LeafConfig(mode="command", family="claude",
                                     argv=["claude"], style_file=style_file))
                return leaves._sizer_key(root, cfg, bp)
            unstyled, styled = key(""), key(".claude/output-styles/s.md")
            self.assertNotEqual(unstyled, styled)
            style.write_text("Narrate chronologically.\n", encoding="utf-8")
            self.assertNotEqual(styled, key(".claude/output-styles/s.md"))

    def test_the_sizer_cache_key_covers_escalation_spec_style_bodies_too(self):
        # The spec item tuples carry only the PATH — the body must be hashed as
        # well, or editing a per-spec style reuses the verdict produced under the
        # old one (the exact staleness the key comment promises against).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "brief.md").write_text("# brief\n", encoding="utf-8")
            spec_style = root / "s2.md"
            spec_style.write_text("Conclusion first.\n", encoding="utf-8")
            def key():
                cfg = SimpleNamespace(
                    root=root,
                    sizer=LeafConfig(mode="command", family="claude", argv=["claude"]),
                    sizer_escalation=[{"on_band": ["watch"], "style_file": "s2.md"}])
                return leaves._sizer_key(root, cfg, root / "brief.md")
            before = key()
            spec_style.write_text("Narrate chronologically.\n", encoding="utf-8")
            self.assertNotEqual(before, key())


class LiveWiring(unittest.TestCase):
    """This repo's pdca.toml wires exactly the prose-report leaves, to a real file."""

    @classmethod
    def setUpClass(cls):
        cls.cfg = Config.load(ROOT)

    def test_the_prose_leaves_are_styled_and_the_structural_ones_are_not(self):
        styled = {"planner": self.cfg.planner, "signoff": self.cfg.signoff,
                  "act": self.cfg.act, "sizer": self.cfg.sizer,
                  "splitter": self.cfg.splitter, "reviewer": self.cfg.reviewer}
        for name, leaf in styled.items():
            self.assertEqual(leaf.style_file, ".claude/output-styles/minto-pyramid.md",
                             f"[leaves.{name}] should carry the style")
        for name in ("builder", "publisher"):
            self.assertEqual(getattr(self.cfg, name).style_file, "",
                             f"[leaves.{name}] output is consumed structurally")

    def test_the_wired_style_file_exists_and_has_a_body(self):
        path = ROOT / self.cfg.planner.style_file
        body = families.strip_frontmatter(path.read_text(encoding="utf-8")).strip()
        self.assertTrue(body, "a wired style file must not be a dangling path")


if __name__ == "__main__":
    unittest.main()
