"""Intake tracks — INSTANCE DELTA (eduralph/pdca-harness#594).

A track is a stream of work that touches its own code: a milestone run in parallel with
the others. The Plan intake cap (wyrd-pdca-P1, ``scripts/plan-cap``) is counted per track,
so the track a bundle belongs to has to be written down and has to be one the Act decision
opened. This module is the one place both are read:

* the open tracks and the default track come from ``[intake]`` in ``pdca.toml``;
* a bundle's track is its brief's ``- **Track:**`` field.

Enforcement is opt-in: with no ``[intake].tracks`` list the instance has no tracks, and
nothing here constrains a brief. With one, ``handoff.check_planner`` requires every brief
leaving Plan to name an open track, and ``split`` gives every child its parent's track.
"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path

from . import brief as _brief

FALLBACK_DEFAULT = "alpha"


@dataclass(frozen=True)
class Tracks:
    open: tuple[str, ...]   # empty: the instance declares no tracks (nothing is enforced)
    default: str            # what a brief without the field counts toward

    @property
    def enforced(self) -> bool:
        return bool(self.open)

    def problem(self, value: str) -> str:
        """Why ``value`` (a brief's Track, already normalized) cannot leave Plan, or ``""``."""
        if not self.enforced:
            return ""
        if not value:
            return ("brief.md field 'track' is missing or an unfilled placeholder — the "
                    "intake cap is per track (wyrd-pdca-P1); name one of: "
                    + ", ".join(self.open))
        if value not in self.open:
            return (f"brief.md field 'track' is {value!r}, which is not an open track — "
                    f"open: {', '.join(self.open)} ([intake].tracks in pdca.toml; opening "
                    f"a track is an Act decision)")
        return ""


def settings(root: Path) -> Tracks:
    """The ``[intake]`` table of ``root/pdca.toml``. Unreadable or absent → no tracks: a
    pdca.toml that cannot be read stops the driver's own config load, and the planner reap
    reports it once (``handoff.stop_problems``) rather than once per brief."""
    try:
        data = tomllib.loads((root / "pdca.toml").read_text(encoding="utf-8"))
    except (OSError, ValueError):   # ValueError: TOMLDecodeError, and bytes not UTF-8
        return Tracks((), FALLBACK_DEFAULT)
    intake = data.get("intake") or {}
    names = tuple(dict.fromkeys(normalize(str(t)) for t in intake.get("tracks") or ()
                                if normalize(str(t))))
    default = normalize(str(intake.get("default_track") or "")) or FALLBACK_DEFAULT
    return Tracks(names, default)


def normalize(value: str) -> str:
    """``` `M5` ``` → ``m5``: the first word, without backticks, lower-cased."""
    words = value.replace("`", " ").split()
    return words[0].lower() if words else ""


def of(brief_path: Path) -> str:
    """The track a brief names, normalized; ``""`` when absent, a placeholder, or unreadable."""
    try:
        return of_text(brief_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return ""


def of_text(text: str) -> str:
    """:func:`of` for brief TEXT (a split child's body, before it is a file). The first
    filled ``Track`` field wins, like ``brief.field``."""
    for line in text.splitlines():
        m = _brief._FIELD_RE.match(line)
        if m and m.group(1).strip().lower() == "track" and not _brief._is_placeholder(m.group(2)):
            found = normalize(m.group(2))
            if found:
                return found
    return ""


def with_track(text: str, track: str) -> str:
    """``text`` naming ``track``: an unfilled ``Track`` placeholder is filled in, and a
    brief with no ``Track`` field gets one after its ``Slug`` (or first) line. A brief
    that already names a track is returned unchanged — callers check it matches first."""
    if of_text(text):
        return text
    lines = text.splitlines(keepends=True)
    field = f"- **Track:** {track}\n"
    for i, line in enumerate(lines):
        m = _brief._FIELD_RE.match(line)
        if m and m.group(1).strip().lower() == "track":
            # A placeholder may run over several lines (`<…` … `>`): drop them all.
            end = i + 1
            if ">" not in line:
                while end < len(lines) and ">" not in lines[end - 1]:
                    end += 1
            return "".join(lines[:i] + [field] + lines[end:])
    for i, line in enumerate(lines):
        m = _brief._FIELD_RE.match(line)
        if m and m.group(1).strip().lower() == "slug":
            return "".join(lines[:i + 1] + [field] + lines[i + 1:])
    return field + text
