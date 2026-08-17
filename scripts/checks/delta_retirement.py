#!/usr/bin/env python3
"""Delta-retirement check (issue #231): notice when a local delta's upstream fix lands.

This instance carries deliberate divergences from the vendored engine — each marked
``INSTANCE DELTA`` where it lives, each naming the upstream issue whose landing retires
it (``eduralph/pdca-harness#N``). Until now that retirement condition existed only as
prose in the file the delta lives in, read by whoever was already editing it — and the
failure is invisible in the direction it fails: a retired-but-not-removed delta keeps
working, it just duplicates upstream, diverges from it on the next upstream change, and
makes the next ``copier update``'s conflict resolution harder than it needed to be.

This script is the automated notice. It scans the instance's code and config for the
marker, associates each site with the upstream issue(s) named in full ``owner/repo#N``
form on the marker line or the two lines right after it (forward-only, stopping at the
next marker — a symmetric window would let a site borrow its neighbour's reference),
asks ``gh`` for each issue's state, and is LOUD when one is CLOSED — naming the delta
sites so a human can judge whether the local code can go. It judges nothing itself:
the fix landing upstream does not mean the local delta is safe to drop unread, which
is why a hit warns rather than blocks.

Run it directly for the site-by-site report, or through its ``[[doctor.checks]]`` row
in ``pdca.toml`` (group "upgrade", level WARN). It needs network and an authenticated
``gh``, which is why it lives in the doctor / upgrade routine and NOT in the offline
``make check`` suite (docs/INTEGRATION.md §2).

Exit codes: 0 — every referenced issue is still OPEN; 1 — retirement candidate(s)
found (an issue CLOSED/MERGED, or a marked site naming no issue at all — an
unattributable divergence is a discipline break to fix, not to ignore); 3 — the check
could not fully answer (``gh`` missing/offline/unauthenticated, or a scan root file
unreadable). Loud beats silent: 1 and 3 both turn the doctor row non-OK.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterator, NamedTuple

# \b on both ends keeps the plural out: prose ABOUT the deltas ("the instance
# deltas above") is not a divergence site, only the singular marker is.
_MARKER = re.compile(r"\binstance\s+delta\b", re.IGNORECASE)
# Only the fully-qualified form counts — `owner/repo#N` names its repo itself. A
# short-form "#531" is ambiguous between the instance tracker and upstream, so it
# reads as unattributed and warns (docs/INTEGRATION.md §2 states the convention).
_FULL_REF = re.compile(r"\b([A-Za-z0-9][\w.-]*/[\w.-]+)#(\d+)")

# What to scan, relative to the project root: everywhere a divergence can live —
# the vendored engine, instance scripts and role prompts, the runtime config.
# Deliberately NOT docs/ or process/: prose narrating a delta is not the delta,
# and results/ bundles quote anything. tests/ is out for the same reason (fixtures).
SCAN_PATHS = ("src", "scripts", "agents", "templates", "engine", ".claude",
              "pdca.toml", "Makefile")
_SKIP_DIRS = {"__pycache__", "node_modules", ".git"}
_SELF = Path(__file__).resolve()
# The association window is FORWARD-only — the marker line plus the two lines after
# it, truncated at any line carrying another marker. Every live site names its issue
# on the marker line itself or in the wrapped comment right below; a symmetric window
# would let a marker borrow a NEIGHBOUR's reference (an unattributed delta two lines
# under an attributed one would silently pass, and a closed issue would name the
# neighbour's sites), which is exactly the misattribution this check exists to end.
_WINDOW = 2


class Site(NamedTuple):
    """One marked divergence: where it is and which upstream issue(s) it names."""
    path: str          # project-root-relative, for the report
    line: int          # 1-based marker line
    refs: tuple[tuple[str, int], ...]  # (repo, issue) pairs; empty ⇒ unattributed


def _iter_files(root: Path) -> Iterator[Path]:
    for entry in SCAN_PATHS:
        p = root / entry
        if p.is_file():
            yield p
        elif p.is_dir():
            for f in sorted(p.rglob("*")):
                if f.is_file() and not _SKIP_DIRS.intersection(f.parts):
                    yield f


def scan_file(path: Path, rel: str) -> list[Site] | None:
    """The file's marked sites — ``None`` when it cannot be read, so the caller can
    say so: an unreadable file silently counted as marker-free would let the check
    claim success over a scan it did not finish."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    lines = text.splitlines()
    sites: list[Site] = []
    for i, line in enumerate(lines):
        if not _MARKER.search(line):
            continue
        window = [line]
        for nxt in lines[i + 1:i + 1 + _WINDOW]:
            if _MARKER.search(nxt):
                break  # the next marker's window, not this one's
            window.append(nxt)
        refs = [(repo, int(n)) for repo, n in _FULL_REF.findall("\n".join(window))]
        sites.append(Site(rel, i + 1, tuple(dict.fromkeys(refs))))
    return sites


def scan_tree(root: Path) -> tuple[list[Site], list[str]]:
    """(sites, unreadable-paths) across the scan roots, in walk order."""
    sites: list[Site] = []
    unreadable: list[str] = []
    for f in _iter_files(root):
        if f.resolve() == _SELF:
            continue  # this file names the marker in its own strings
        rel = f.relative_to(root).as_posix()
        got = scan_file(f, rel)
        if got is None:
            unreadable.append(rel)
        else:
            sites.extend(got)
    return sites, unreadable


def issue_state(repo: str, num: int, *, runner=subprocess.run) -> tuple[str, str] | None:
    """(state, title) of ``repo#num``, or ``None`` when ``gh`` cannot answer.

    Tries the issue endpoint first, then the PR one — a delta may name either kind,
    and ``gh issue view`` refuses a PR number rather than answering for it. A missing
    ``gh`` binary is the same answer as an unauthenticated one: ``None``, reported
    UNREACHABLE — never a traceback.
    """
    for sub in ("issue", "pr"):
        try:
            r = runner(["gh", sub, "view", str(num), "-R", repo,
                        "--json", "state,title"], capture_output=True, text=True)
        except OSError:  # gh not installed (FileNotFoundError) or not executable
            return None
        if r.returncode == 0:
            try:
                data = json.loads(r.stdout)
                return str(data.get("state", "?")).upper(), str(data.get("title", ""))
            except (json.JSONDecodeError, AttributeError):
                return None
    return None


def main(*, root: Path | None = None, runner=subprocess.run, out=None) -> int:
    out = out or sys.stdout
    root = (root or Path(__file__).resolve().parents[2]).resolve()
    sites, unreadable = scan_tree(root)
    by_issue: dict[tuple[str, int], list[Site]] = {}
    unattributed = [s for s in sites if not s.refs]
    for s in sites:
        for ref in s.refs:
            by_issue.setdefault(ref, []).append(s)

    print(f"delta-retirement: {len(sites)} marked site(s) → "
          f"{len(by_issue)} upstream issue(s)  (root: {root})", file=out)
    rc = 0
    for rel in unreadable:
        print(f"  UNREADABLE    {rel} — could not be scanned, so this run cannot "
              f"promise the marker inventory is complete", file=out)
        rc = max(rc, 3)
    for s in unattributed:
        print(f"  UNATTRIBUTED  {s.path}:{s.line} — the marker names no upstream "
              f"issue; add `owner/repo#N` beside it so retirement can be tracked",
              file=out)
        rc = max(rc, 1)
    for (repo, num), where in sorted(by_issue.items()):
        state = issue_state(repo, num, runner=runner)
        locs = ", ".join(f"{s.path}:{s.line}" for s in where)
        if state is None:
            print(f"  UNREACHABLE   {repo}#{num}  (gh could not answer — offline or "
                  f"unauthenticated?)\n                sites: {locs}", file=out)
            rc = max(rc, 3)
            continue
        verdict, title = state
        print(f"  {verdict:<13} {repo}#{num}  {title}\n                sites: {locs}",
              file=out)
        if verdict != "OPEN":
            print(f"                ^ RETIREMENT CANDIDATE — the upstream fix landed; "
                  f"read each site and decide whether the local delta can go "
                  f"(docs/INTEGRATION.md §2). Do not drop it unread.", file=out)
            rc = max(rc, 1)
    return rc


if __name__ == "__main__":
    sys.exit(main())
