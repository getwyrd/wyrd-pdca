"""Auto-merge mode for wave sequencing (#wave-model, opt-in) — merge each wave's PRs so
the next wave builds on the genuinely-merged base.

The **default** sequencing folds accepted work onto an integration branch without merging
(fork-safe, STOP discipline intact — see :mod:`integrate`). For an own-repo /
continuous-delivery target where "landed in the base" is the deliverable *and* the operator
has merge rights on ``base_remote``, ``[driver].wave_mode = "merge"`` instead merges each
non-final wave's PRs (``gh pr merge``) and fetches the base, so the next wave's Do worktree
(which resets to ``<base_remote>/<base>``) builds on the merged result.

Fail-closed: a PR that does not merge — a conflict, a failing required check, no merge
rights — returns non-zero so the caller STOPs; the next wave must never build on an
unmerged base. Idempotent (a resumed run skips an already-merged PR). Merging is
deterministic ``git``/``gh`` (no model); dry-run (stubbed publisher) prints the plan and
merges nothing. The harness's own ``gh pr merge`` runs in the orchestrator, outside the
``builder_guard`` hook that blocks the model leaves from merging — exactly as publish's
``gh pr create`` does.

``[driver].auto_merge = false`` turns the merging back off without leaving merge mode: the
flow never calls :func:`merge_wave` and STOPs at the wave boundary instead, so the PRs keep
the draft ``publish`` opened and the merge stays the human's (pdca-harness#462). This module
is the mechanics only — it merges whenever it is called.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from . import merged, publish, state
from .config import Config


def has_contribution(d: Path) -> bool:
    """True if this bundle has a patch that must reach its base.

    A close / no-fix disposition writes no ``patch.diff`` (or an empty one) and publish opens
    no PR for it, so nothing of it needs merging and no base has to move on its account.

    Shared rather than inlined (#462 review): ``_merge_one`` skips these bundles, and the
    ``[driver].auto_merge = false`` boundary stop in ``flow`` must skip exactly the same set
    when deciding whether a completed wave has anything for the human to merge. Two copies of
    the test would eventually disagree, and the disagreement is invisible — a wave that stops
    for nothing, or one that does not stop when it should.
    """
    patch = d / "patch.diff"
    try:
        return patch.is_file() and bool(patch.read_text(encoding="utf-8").strip())
    except OSError:
        # Unreadable is not "nothing to merge". For the merge path this means attempting the
        # merge (which reports its own failure); for the boundary stop it means stopping.
        # Both cost an invocation; the opposite reading risks building on a base that never
        # moved, so fail toward the loud outcome.
        return True


def merge_wave(cfg: Config, bundles: list[Path], *, dry_run: bool = False,
               method: str = "merge") -> int:
    """Merge each accepted bundle's PR into its base, then fetch the base. Return 0 iff
    every bundle merged (or had nothing to merge); non-zero (STOP) on the first failure."""
    fetched: set[str] = set()
    for d in bundles:
        rc = _merge_one(cfg, d, dry_run=dry_run, method=method, fetched=fetched)
        if rc:
            return rc
    return 0


def _merge_one(cfg: Config, d: Path, *, dry_run: bool, method: str,
               fetched: set[str]) -> int:
    """Merge one bundle's recorded PR (idempotent, fail-closed). ``fetched`` dedupes the
    post-merge base fetch across bundles that share a checkout."""
    if state.state(d) != state.COMPLETE:
        return 0  # not accepted — nothing of this bundle's to merge
    if not has_contribution(d):
        return 0  # close / no-fix disposition — no contribution to merge
    rec = publish._publish_record(d)
    pr_url = rec.get("pr_url") if rec else None
    repo_spec = rec.get("repo") if rec else None
    if not pr_url:
        print(f"merge: {d.name} is COMPLETE but has no recorded PR — cannot merge a wave "
              "whose member wasn't published. STOP.", file=sys.stderr)
        return 1

    cmd = ["gh", "pr", "merge", str(pr_url), f"--{method}"]
    if dry_run:
        print(f"merge --dry-run — {d.name}: {' '.join(cmd)}")
        return 0
    iid = d.name.removeprefix("issue_")
    if merged.is_merged(cfg, iid):
        return 0  # already merged (a resumed run) — idempotent

    # The publisher opens every PR as a draft (STOP discipline), but `gh pr merge` refuses a
    # draft — so in merge mode a non-final wave's PRs must be readied before they can advance
    # the base (issue #279). `merge_wave` is only called for non-final waves, so this readies
    # exactly the PRs about to be merged; the final wave never reaches here and keeps its
    # draft for the human's ready-mark. Idempotent: `gh pr ready` on an already-ready PR is a
    # no-op. Fail-closed like the merge itself — if it can't be readied, it can't be merged.
    print(f"→ gh pr ready {pr_url}")
    ready = subprocess.run(["gh", "pr", "ready", str(pr_url)], capture_output=True, text=True)
    if ready.returncode != 0:
        print((ready.stderr or ready.stdout).strip(), file=sys.stderr)
        print(f"\n!!! merge: {d.name} ({pr_url}) could not be marked ready to merge. "
              "STOP: later waves are NOT run; resolve at the PR, then re-run.\n",
              file=sys.stderr)
        return 1

    print(f"→ gh pr merge {pr_url} --{method}")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print((r.stderr or r.stdout).strip(), file=sys.stderr)
        print(f"\n!!! merge: {d.name} ({pr_url}) did not merge — a conflict, a failing "
              "required check, or no merge rights on the base. STOP: later waves are NOT "
              "run; resolve at the PR, then re-run.\n", file=sys.stderr)
        return 1
    # Refresh the base so the NEXT wave's worktree resets to the merged result.
    if repo_spec and repo_spec not in fetched:
        repo = publish._checkout_path(cfg, repo_spec)
        subprocess.run(["git", "-C", str(repo), "fetch", cfg.base_remote],
                       capture_output=True, text=True)
        fetched.add(repo_spec)
    return 0
