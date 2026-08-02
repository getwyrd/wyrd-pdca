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

* **Scoped by construction.** Each bundle gets ``<root>/issue_<id>``, handed to its leaves and
  gates as ``$PDCA_SCRATCH`` *and* ``$TMPDIR``. Scoping is structural on purpose: the models
  are told to name their trees ``pdca-<leaf>-<issue>-*`` and they partly don't, and cargo's
  own ``.tmp*`` dirs carry no bundle identity at all. A per-bundle root captures every one of
  them regardless of who made it or what they called it.
* **Reclaimed at the boundary.** :func:`reclaim` is called by ``sweep()``, which the flow
  already invokes per bundle at publish/freeze. Reuse *below* that line is untouched: the
  auto-iterate Do→Check rounds all happen first, so a re-review still finds its build tree
  warm (the property upstream eduralph/pdca-harness#422 is about).

Ownership is stamped, not timed. Each bundle dir gets a sibling ``issue_<id>.owner`` naming
the creating pid — ``worktree.py``'s convention, and a sibling for its reason: it survives
anything that empties the directory itself. A run SIGKILLed before the boundary leaves a
stamp whose pid is provably gone, and the next sweep reclaims it. Nothing here consults a
wall clock, so a long run is never at risk of having its live scratch aged out from under it.

Deliberately conservative, matching ``worktree.orphan_overflow_dirs``: only a pid that
*positively no longer exists* marks an orphan. A live pid, a pid we cannot signal, or a
missing/unparseable stamp all read as "someone else's, leave it" — never reclaim what cannot
be classified.
"""

from __future__ import annotations

import contextlib
import os
import shutil
from pathlib import Path

from .config import Config

_STAMP_SUFFIX = ".owner"


def root(cfg: Config, _env: dict | None = None) -> Path | None:
    """The effective scratch root for this process, or ``None`` when unset.

    ``cli._export_scratch`` resolves the configured root once at CLI entry and exports it as
    ``$PDCA_SCRATCH``; that is the authority, because it is also what *rejects* an unusable
    root (it pops the variable and the run falls back to the default temp location). Reading
    it back keeps this module agreeing with that decision instead of re-deriving it.

    Falls back to ``[driver].scratch_dir`` so the manual ``pdca sweep`` still works when the
    variable never got exported. ``_env`` is injected by tests only — never pass a *leaf's*
    env, which carries the per-bundle path rather than the root.
    """
    e = os.environ if _env is None else _env
    raw = e.get("PDCA_SCRATCH") or cfg.scratch_dir
    if not raw:
        return None
    p = Path(raw).expanduser()
    return p if p.is_absolute() else cfg.root / p


def _stamp(bundle_scratch: Path) -> Path:
    """The owner sidecar for ``bundle_scratch`` — a SIBLING, so it outlives anything that
    empties or removes the directory itself (``worktree._owner_file``'s reasoning)."""
    return bundle_scratch.with_name(bundle_scratch.name + _STAMP_SUFFIX)


def _owner_pid(bundle_scratch: Path) -> int | None:
    """The pid stamped into ``bundle_scratch``'s sidecar, or ``None`` when there is no
    readable, parseable stamp. ``None`` proves nothing and must never license a delete."""
    f = _stamp(bundle_scratch)
    try:
        raw = f.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    return int(raw) if raw.isdigit() else None


def _reclaimable(bundle_scratch: Path) -> tuple[bool, str]:
    """``(may_remove, why_not)`` for one bundle scratch dir.

    Ours or provably dead ⇒ remove. Another process that is still alive ⇒ leave: it may be
    mid-build in a concurrent flow, and pulling its ``$TMPDIR`` out from under it would fail
    that run's gates — the same reasoning that guards overflow trees in ``sweep()``.
    """
    from . import worktree  # lazy: keeps this module import-light for leaves/gates
    pid = _owner_pid(bundle_scratch)
    if pid is None or pid == os.getpid():
        return True, ""
    if worktree._pid_alive(pid):
        return False, f"owner process {pid} still alive"
    return True, ""


def for_bundle(cfg: Config, d: Path) -> Path | None:
    """Create and return this bundle's scratch dir, stamping it with our pid.

    ``None`` when no scratch root is configured — byte-for-byte today's behavior, the same
    contract ``cli._export_scratch`` keeps. Idempotent: re-stamping across a run's iterations
    is what keeps the stamp current for a resumed lane.
    """
    r = root(cfg)
    if r is None:
        return None
    p = r / d.name
    try:
        p.mkdir(parents=True, exist_ok=True)
        _stamp(p).write_text(f"{os.getpid()}\n", encoding="utf-8")
    except OSError:
        # A hygiene redirect must never abort a run (the #134 contract): fall back to the
        # process-wide root, which cli._export_scratch already proved writable.
        return r
    return p


def env_for(cfg: Config, d: Path) -> dict:
    """``{PDCA_SCRATCH, TMPDIR}`` for this bundle's leaves and gates, or ``{}`` when no
    scratch root is configured. Merge into the env dict at the call site — the lanes are
    THREADS in one process (``flow.py``), so ``os.environ`` cannot hold a per-bundle value.
    """
    p = for_bundle(cfg, d)
    return {} if p is None else {"PDCA_SCRATCH": str(p), "TMPDIR": str(p)}


def orphans(cfg: Config) -> list[Path]:
    """Bundle scratch dirs whose creating process is provably gone — crash leftovers.

    One ``glob`` at depth 1; never walks or sizes the tree (the #297 rule: a scratch root has
    been 96 GB, and ``du`` over it is not something a sweep or a doctor row may do).
    """
    r = root(cfg)
    if r is None or not r.is_dir():
        return []
    out: list[Path] = []
    for p in sorted(r.glob("issue_*")):
        if not p.is_dir():
            continue  # the .owner sidecars, and anything else that isn't a bundle dir
        pid = _owner_pid(p)
        if pid is not None and pid != os.getpid():
            from . import worktree  # lazy, as in _reclaimable
            if not worktree._pid_alive(pid):
                out.append(p)
    return out


def _remove(p: Path, *, dry_run: bool) -> None:
    """Best-effort removal of a bundle scratch dir and its stamp. Never raises: teardown
    must not fail a run (``sweep``'s contract)."""
    if dry_run:
        return
    shutil.rmtree(p, ignore_errors=True)
    with contextlib.suppress(OSError):
        _stamp(p).unlink(missing_ok=True)


def reclaim(cfg: Config, bundles: list[Path] | None = None, *,
            dry_run: bool = False) -> list[str]:
    """Reclaim leaf scratch; return human-readable report lines (``sweep``'s shape).

    Removes each named bundle's scratch, then any orphan left by a crashed run. ``bundles``
    is what the flow passes at the publish/freeze boundary; ``None`` (the manual ``pdca
    sweep``) reclaims orphans only — an unnamed bundle may be in flight in another process,
    and its liveness is exactly what the stamp is there to answer.
    """
    r = root(cfg)
    if r is None or not r.is_dir():
        return []
    lines: list[str] = []
    verb = "would " if dry_run else ""
    seen: set[Path] = set()
    for d in bundles or []:
        p = r / d.name
        if not p.exists() and not _stamp(p).exists():
            continue
        ok, why = _reclaimable(p)
        if not ok:
            lines.append(f"sweep: left leaf scratch {p.name} ({why})")
            continue
        _remove(p, dry_run=dry_run)
        seen.add(p)
        lines.append(f"sweep: {verb}reclaim leaf scratch {p.name}")
    stale = [p for p in orphans(cfg) if p not in seen]
    for p in stale:
        _remove(p, dry_run=dry_run)
    if stale:
        lines.append(f"sweep: {verb}reclaim {len(stale)} orphaned leaf scratch dir(s) "
                     f"(owner process gone)")
    return lines
