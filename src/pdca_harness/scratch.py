"""Bundle-scoped leaf scratch, reclaimed at the publish/freeze boundary (issue #200).

#134 moved leaf scratch off the tmpfs ``/tmp`` onto a disk-backed ``[driver].scratch_dir``,
which fixed the RAM exhaustion of that era. What it did not do is give that root an *owner*:
every bundle's throwaway work landed in one flat directory that nothing ever emptied. It
reached 96 GB and 3,930 stale dirs, and on 2026-08-02 the page cache from two reviewers'
~69 GB of cold build trees pushed ``user@1000.service`` past systemd-oomd's pressure
threshold and got a 3d 19h ``pdca flow`` run SIGKILLed wholesale (#199).

The rule this module implements: **a bundle's temporary data is gone by the time that bundle
is ready to be published.** That is not a new policy — it is #297's, which asked to sweep the
harness's worktrees "on publish/freeze (not lazily at the next Do)" and shipped as
:mod:`~pdca_harness.sweep`. Leaf scratch simply arrived afterwards and was never registered
with the sweeper that already ran at the right moment.

Two halves:

* **Scoped by construction.** Each bundle gets ``<root>/<project>/issue_<id>``, handed to its
  leaves and gates as ``$PDCA_SCRATCH`` *and* ``$TMPDIR``. Scoping is structural on purpose:
  the models are told to name their trees ``pdca-<leaf>-<issue>-*`` and they partly don't, and
  cargo's own ``.tmp*`` dirs carry no bundle identity at all. A per-bundle root captures every
  one of them regardless of who made it or what they called it.
* **Reclaimed at the boundary.** :func:`reclaim` is called by ``sweep()`` (the flow's
  publish/freeze hook) and by ``publish()`` (the piecemeal ``run`` → ``signoff --accept`` →
  ``publish`` path, which never goes through sweep). Reuse *below* that line is untouched: the
  auto-iterate Do→Check rounds all happen first, so a re-review still finds its build tree warm
  (the property upstream eduralph/pdca-harness#422 is about).

The ``<project>`` segment matters because ``scratch_dir`` is a MACHINE-WIDE path — the shipped
value is ``/var/tmp/pdca`` — while ``issue_<id>`` is only unique within one rendered project.
Two projects would otherwise share a directory for their respective issue 651, letting one
inherit the other's build trees (contaminating verification with unrelated source state) and
later delete scratch that was never its own.

Ownership is stamped, not timed. Each bundle dir gets a sibling ``issue_<id>.owner`` listing
the pid of every process currently using it — a sibling for its reason: it survives anything
that empties the directory itself. A run SIGKILLed before the boundary leaves stamps whose pids
are provably gone, and the next sweep reclaims it. Nothing here consults a wall clock, so a
long run is never at risk of having its live scratch aged out from under it.

Deliberately conservative, matching ``worktree.orphan_overflow_dirs``: only a pid that
*positively no longer exists* licenses a reclaim. A live pid, a pid we cannot signal, and a
missing / unparseable / out-of-range stamp all read as "not provably ours to delete".
"""

from __future__ import annotations

import contextlib
import hashlib
import os
import re
import shutil
from pathlib import Path

from .config import Config

_STAMP_SUFFIX = ".owner"
# os.kill raises OverflowError — NOT an OSError, so worktree._pid_alive does not catch it —
# on an integer outside the platform pid_t range. A truncated or corrupted sidecar holding a
# large integer must read as unclassifiable, not crash `pdca sweep` mid-run.
_PID_CEILING = 2 ** 31 - 1


def root(cfg: Config, _env: dict | None = None) -> Path | None:
    """The effective scratch root for this process, or ``None`` when there is none.

    ``$PDCA_SCRATCH`` is the ONLY authority, and deliberately so. ``cli._export_scratch``
    runs once at CLI entry before any dispatch, resolves the configured root, probes it for
    writability, and on failure *pops the variable* so the run falls back to the default temp
    location. Consulting ``cfg.scratch_dir`` as a fallback here would undo exactly that
    decision — every leaf and gate would be handed back the directory the CLI just rejected
    and warned about, which is worse than the no-op the operator was promised (#207 review).

    ``_env`` is injected by tests only. Never pass a *leaf's* env: that carries the
    per-bundle path, not the root.
    """
    raw = (os.environ if _env is None else _env).get("PDCA_SCRATCH")
    if not raw:
        return None
    p = Path(raw).expanduser()
    return p if p.is_absolute() else cfg.root / p


def _project_key(cfg: Config) -> str:
    """A stable directory name identifying THIS rendered project inside a shared root.

    Readable prefix so an operator can tell whose scratch a directory is, plus a digest of
    the absolute project root so two checkouts sharing a basename never collide.
    """
    try:
        real = str(cfg.root.resolve())
    except OSError:
        real = str(cfg.root)
    slug = re.sub(r"[^A-Za-z0-9._-]+", "-", cfg.root.name).strip("-") or "project"
    return f"{slug}-{hashlib.sha256(real.encode('utf-8')).hexdigest()[:8]}"


def project_root(cfg: Config) -> Path | None:
    """This project's slice of the shared scratch root; ``None`` when unset."""
    r = root(cfg)
    return None if r is None else r / _project_key(cfg)


def _stamp(bundle_scratch: Path) -> Path:
    """The owner sidecar for ``bundle_scratch`` — a SIBLING, so it outlives anything that
    empties or removes the directory itself (``worktree._owner_file``'s reasoning)."""
    return bundle_scratch.with_name(bundle_scratch.name + _STAMP_SUFFIX)


def _alive(pid: int) -> bool:
    """``worktree._pid_alive`` with the unknowable cases pinned to "alive".

    That helper already treats a pid it cannot signal as live; this adds the value errors it
    does not catch, so a corrupt stamp can never take a sweep down with it.
    """
    from . import worktree  # lazy: keeps this module import-light for leaves/gates
    try:
        return worktree._pid_alive(pid)
    except (OverflowError, ValueError, OSError):
        return True


def _owners(bundle_scratch: Path) -> list[int] | None:
    """Pids currently claiming ``bundle_scratch``, or ``None`` when unclassifiable.

    ``None`` — no stamp, an unreadable one, or any entry that is not a plausible pid — proves
    nothing about who owns the tree and must never license a blind delete.
    """
    try:
        raw = _stamp(bundle_scratch).read_text(encoding="utf-8")
    except OSError:
        return None
    pids: list[int] = []
    for token in raw.split():
        if not token.isdigit():
            return None
        value = int(token)
        if not 0 < value <= _PID_CEILING:
            return None
        pids.append(value)
    return pids or None


def _claim(bundle_scratch: Path) -> None:
    """Add our pid to the owner stamp, dropping owners that have since died.

    A LIST, not a single pid (#207 review): two processes working the same bundle both use
    this directory, and a single overwritten value would let whichever stamped last delete it
    at its own boundary while the other still has a leaf or gate using it as ``$TMPDIR``.
    Pruning dead pids on every claim keeps the file from growing across a run's iterations.
    """
    mine = os.getpid()
    live = [p for p in (_owners(bundle_scratch) or []) if p != mine and _alive(p)]
    with contextlib.suppress(OSError):
        _stamp(bundle_scratch).write_text(
            "".join(f"{p}\n" for p in [*live, mine]), encoding="utf-8")


def _reclaimable(bundle_scratch: Path) -> tuple[bool, str]:
    """``(may_remove, why_not)`` for one bundle scratch dir.

    Every owner ours or provably dead ⇒ remove. Any other live owner ⇒ leave: it may be
    mid-build in a concurrent flow, and pulling its ``$TMPDIR`` out from under it would fail
    that run's gates — the reasoning that guards overflow trees in ``sweep()``.
    """
    owners = _owners(bundle_scratch)
    if owners is None:
        # Reached only on the EXPLICIT path (the caller named this bundle at its own
        # boundary), which is an instruction, not a guess. `orphans()` skips the same case.
        return True, ""
    others = [p for p in owners if p != os.getpid() and _alive(p)]
    return (False, f"owner process {others[0]} still alive") if others else (True, "")


def for_bundle(cfg: Config, d: Path) -> Path | None:
    """Create and return this bundle's scratch dir, claiming it for our pid.

    ``None`` when no scratch root is configured — byte-for-byte today's behavior, the same
    contract ``cli._export_scratch`` keeps. Idempotent: re-claiming across a run's iterations
    is what keeps the stamp current.
    """
    pr = project_root(cfg)
    if pr is None:
        return None
    p = pr / d.name
    try:
        p.mkdir(parents=True, exist_ok=True)
    except OSError:
        # A hygiene redirect must never abort a run (the #134 contract): fall back to the
        # process-wide root, which cli._export_scratch already proved writable.
        return root(cfg)
    _claim(p)
    return p


def env_for(cfg: Config, d: Path) -> dict:
    """``{PDCA_SCRATCH, TMPDIR}`` for this bundle's leaves and gates, or ``{}`` when no
    scratch root is configured. Merge into the env dict at the call site — the lanes are
    THREADS in one process (``flow.py``), so ``os.environ`` cannot hold a per-bundle value.
    """
    p = for_bundle(cfg, d)
    return {} if p is None else {"PDCA_SCRATCH": str(p), "TMPDIR": str(p)}


def orphans(cfg: Config) -> list[Path]:
    """Bundle scratch dirs whose every owner is provably gone — crash leftovers.

    Scoped to THIS project's slice, so a shared root never exposes another project's
    in-flight work to our sweep. One ``glob`` at depth 1; never walks or sizes the tree (the
    #297 rule: a scratch root has been 96 GB, and ``du`` over it is not something a sweep or
    a doctor row may do).
    """
    pr = project_root(cfg)
    if pr is None or not pr.is_dir():
        return []
    out: list[Path] = []
    for p in sorted(pr.glob("issue_*")):
        if not p.is_dir():
            continue  # the .owner sidecars, and anything else that isn't a bundle dir
        owners = _owners(p)
        if owners is None:
            continue  # unclassifiable — never reclaim what cannot be classified
        if all(pid != os.getpid() and not _alive(pid) for pid in owners):
            out.append(p)
    return out


def _remove(p: Path, *, dry_run: bool) -> bool:
    """Best-effort removal of a bundle scratch dir and its stamp; ``True`` if it is gone.

    The stamp is unlinked ONLY once the tree actually is (#207 review): ``ignore_errors``
    hides a transient permission / mount / filesystem failure, and dropping the sidecar anyway
    would leave a potentially huge directory that neither :func:`orphans` nor a manual ``pdca
    sweep`` could ever see again — recreating the permanent residue this module exists to
    stop. Never raises: teardown must not fail a run (``sweep``'s contract).
    """
    if dry_run:
        return True
    shutil.rmtree(p, ignore_errors=True)
    if p.exists():
        return False
    with contextlib.suppress(OSError):
        _stamp(p).unlink(missing_ok=True)
    return True


def reclaim(cfg: Config, bundles: list[Path] | None = None, *,
            dry_run: bool = False) -> list[str]:
    """Reclaim leaf scratch; return human-readable report lines (``sweep``'s shape).

    Removes each named bundle's scratch, then any orphan left by a crashed run. ``bundles``
    is what the flow and ``publish()`` pass at the boundary; ``None`` (the manual ``pdca
    sweep``) reclaims orphans only — an unnamed bundle may be in flight in another process,
    and its liveness is exactly what the stamp is there to answer.
    """
    pr = project_root(cfg)
    if pr is None or not pr.is_dir():
        return []
    lines: list[str] = []
    verb = "would " if dry_run else ""
    seen: set[Path] = set()
    for d in bundles or []:
        p = pr / d.name
        if not p.exists() and not _stamp(p).exists():
            continue
        seen.add(p)
        ok, why = _reclaimable(p)
        if not ok:
            lines.append(f"sweep: left leaf scratch {p.name} ({why})")
        elif _remove(p, dry_run=dry_run):
            lines.append(f"sweep: {verb}reclaim leaf scratch {p.name}")
        else:
            lines.append(f"sweep: could not remove leaf scratch {p} — left for the next "
                         f"sweep (its owner stamp is kept, so it stays discoverable)")
    stale = [p for p in orphans(cfg) if p not in seen]
    failed = [p for p in stale if not _remove(p, dry_run=dry_run)]
    if len(stale) > len(failed):
        lines.append(f"sweep: {verb}reclaim {len(stale) - len(failed)} orphaned leaf scratch "
                     f"dir(s) (owner process gone)")
    lines += [f"sweep: could not remove orphaned leaf scratch {p} — left for the next sweep"
              for p in failed]
    return lines
