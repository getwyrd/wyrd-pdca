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

"Never on an unmerged base" is only half the rule: the next wave must never build on a
base whose verification was not GREEN either, and ``gh pr merge``'s own refusal cannot
carry that (issue #413). It fails closed only on the checks the HOST repo marks *required*
in branch protection, so on a thinly-protected host a red non-required job — or a run
still in flight — merges anyway. Correctness here must not hinge on per-instance host
config, so ``_merge_one`` reads the PR's FULL check rollup itself (``gh pr checks``) and
refuses on any failing, pending or missing check. The read happens AFTER ``gh pr ready``
and immediately before ``gh pr merge``: marking a draft ready can itself trigger
``ready_for_review`` CI, so a rollup observed only pre-ready cannot promise green at merge
time. Refusing after the ready-mark is safe — a re-run resumes idempotently. An EMPTY
rollup refuses too (absence of evidence is not green); skipped/neutral checks are
completed non-failures and do not block. ``[driver].merge_requires = "required"``
(default ``"all"``) opts back into host-config-only semantics, skipping the gate.

A wave boundary fires SECONDS after the PR was opened (issue #462: getwyrd/wyrd#703, six
seconds between create and ready_for_review), so the FIRST rollup read above is routinely
``pending`` or ``empty`` — not a verdict, just evidence that has not arrived yet.
``_wait_for_green`` re-reads the rollup until it resolves or ``[driver].merge_wait_secs``
(default 300; ``0`` disables the wait — the original immediate-refusal behaviour) of
wall-clock time elapses, through the patchable ``_sleep`` below so a test costs no real
time. Whichever way ``_merge_one`` declines to merge a PR it already readied — the rollup
never resolving green, a failing ``gh pr merge`` — ``_undo_ready`` marks it back to draft
(``gh pr ready --undo``) before returning, so a stopped wave never leaves a PR advertising
a readiness no human granted (``docs/INTEGRATION.md`` §10).

**INSTANCE DELTA — ``[driver].merge_sync_base`` (eduralph/pdca-harness#531, OPEN).** The
rollup gate above is honest about whichever tree the PR's checks last ran on — which, for
every wave member after the first, is the tree BEFORE its siblings merged. Upstream merges a
wave's PRs back to back and verifies no combination: A and B are each green against
``main@X``, A merges to ``main@Y``, and B then merges on a rollup describing ``X``. If A and
B conflict semantically, ``main`` is red and the next wave builds on it. ``flow.py``'s
``_audit_wave_overlap`` sees *file*-level overlap only and is explicitly advisory, and
semantic conflicts need no shared files.

Whether that happens is decided entirely by the host's ``required_status_checks.strict``,
which upstream neither reads nor documents as load-bearing; ``getwyrd/wyrd`` is
``strict: false``, so GitHub does not re-run a PR's checks after its base moves.
:func:`_behind_by` + :func:`_sync_base` close it inside the driver instead of depending on
host configuration: a PR found behind is brought up to date first, which empties its rollup
and lets the *existing* wait-and-gate decide on checks for the tree it really merges into.

Three things the PR #230 review corrected, each of which would have left the delta broken in
a way its own tests did not see. The check **loops**: ``_wait_for_green`` may wait up to
``merge_wait_secs``, and a sibling landing during that wait leaves the head behind again with
a green rollup for the pre-move tree — the original failure in a smaller window — so the base
is re-read after every gate, bounded by :data:`_MAX_SYNC_ROUNDS`. The post-sync wait runs
**even under** ``merge_requires = "required"``: that mode skips the rollup *gate*, but a sync
invalidates the checks and ``gh pr merge`` refuses while required ones are pending, so
skipping the wait would fail every wave at its second PR. And the compare **qualifies a fork
head** as ``OWNER:BRANCH`` (as ``publish.py:301`` already does), since a bare name resolves
against the base repo — where a fork's branch does not exist, so every merge would stop.

Note this is not the same fix as turning on host strictness. ``strict: true`` alone would
make ``gh pr merge`` refuse every wave member after the first, stopping the batch —
upstream #462's shape, and the stop that ``auto_merge = true`` was turned back on to remove
— because upstream has no ``update-branch`` path at all. ``merge_sync_base = false``
reproduces upstream exactly.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

from . import merged, publish, state
from .config import Config

# Patchable indirection so a test can drive the wait loop below with no real wall-clock
# cost (issue #462) — tests replace this, never `time.sleep` itself.
_sleep = time.sleep

# `gh pr checks --json name,bucket` classifies every check into one of five buckets:
# pass | fail | pending | skipping | cancel (`gh pr checks --help`). "pass" and "skipping"
# (skipped/neutral) are completed non-failures and do not block; "pending" (running or
# queued) always blocks; everything else — "fail", "cancel", or a bucket a later gh grows
# that this harness has never heard of — counts as failing, because the fail-safe direction
# is to refuse, never to guess green on a bucket we cannot interpret.
_ROLLUP_OK = frozenset({"pass", "skipping"})
_ROLLUP_PENDING = frozenset({"pending"})


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


def _check_rollup(pr_url: str) -> tuple[str, str]:
    """Classify PR ``pr_url``'s FULL check rollup (issue #413). Returns
    ``(verdict, detail)``; only ``"green"`` may merge.

    * ``"green"``      — every reported check completed without failing (pass, or
      skipped/neutral); ``detail`` counts what was verified, for the run log.
    * ``"pending"``    — at least one check is still running or queued.
    * ``"failing"``    — at least one check failed, was cancelled, or reports a bucket
      this harness does not recognise.
    * ``"empty"``      — no checks were reported at all; absence of evidence is not green.
    * ``"unreadable"`` — ``gh`` could not enumerate the checks (auth, network, a ``gh``
      too old for ``--json``). Fail-closed, same as a failing check.

    ``gh pr checks`` prints the JSON *and then* sets an exit code summarising the rollup
    — 0 all passed, 1 something failed, 8 something is pending (``gh help exit-codes``) —
    so the exit code is not evidence of an error and the buckets, not the code, are what
    is classified. This needs a ``gh`` whose ``pr checks`` supports ``--json`` with the
    documented ``bucket`` field; one too old for it exits non-zero printing no JSON, which
    lands in ``unreadable`` and refuses — no version floor to enforce, because the
    degradation is already fail-closed.
    """
    r = subprocess.run(["gh", "pr", "checks", str(pr_url), "--json", "name,bucket"],
                       capture_output=True, text=True)
    out = (r.stdout or "").strip()
    err = (r.stderr or "").strip()
    if not out:
        # No JSON at all. gh reports a rollup with nothing in it as an error ("no checks
        # reported on the '<branch>' branch") rather than an empty list, so recognise that
        # one shape as EMPTY for a truthful message; anything else is unreadable. Both
        # refuse under the default, so a gh that reworded the message costs a message, not
        # a wrong merge.
        if r.returncode == 0 or "no checks reported" in err.lower():
            return "empty", err or "no checks reported"
        return "unreadable", err or f"`gh pr checks` exited {r.returncode}"
    try:
        checks = json.loads(out)
    except ValueError:
        return "unreadable", f"unparsable `gh pr checks` output: {out[:200]}"
    if not isinstance(checks, list):
        return "unreadable", f"unexpected `gh pr checks` payload: {out[:200]}"
    if not checks:
        return "empty", "no checks reported"
    failing = [c for c in checks if _bucket(c) not in _ROLLUP_OK | _ROLLUP_PENDING]
    if failing:
        return "failing", _names(failing)
    waiting = [c for c in checks if _bucket(c) in _ROLLUP_PENDING]
    if waiting:
        return "pending", _names(waiting)
    return "green", f"{len(checks)} check{'' if len(checks) == 1 else 's'}"


def _bucket(check: object) -> str:
    return str(check.get("bucket") or "") if isinstance(check, dict) else ""


def _names(checks: list) -> str:
    return ", ".join(
        f"{(c.get('name') if isinstance(c, dict) else None) or '?'} ({_bucket(c) or '?'})"
        for c in checks)


def _wait_for_green(pr_url: str, wait_secs: int, *, poll_interval: int = 15) -> tuple[str, str]:
    """Re-read ``pr_url``'s check rollup (``_check_rollup``) until it clears ``pending``/
    ``empty`` or ``wait_secs`` of (patchable) wall-clock time is exhausted (issue #462).
    Returns the final ``(verdict, detail)`` unchanged — this never itself decides to merge.

    ``wait_secs <= 0`` performs exactly one read and returns immediately: the original
    behaviour, for a host whose checks are known to already be in by the time the wave
    boundary fires. Sleeps go through the module-level ``_sleep`` so a test can make the
    whole loop cost no real time.
    """
    verdict, detail = _check_rollup(pr_url)
    waited = 0
    while verdict in ("pending", "empty") and waited < wait_secs:
        step = min(poll_interval, wait_secs - waited)
        _sleep(step)
        waited += step
        verdict, detail = _check_rollup(pr_url)
    return verdict, detail


def _undo_ready(pr_url: str) -> None:
    """Return ``pr_url`` to draft (issue #462): the documented inverse of the ``gh pr
    ready`` call in ``_merge_one``, run on every path where that function declines to merge
    a PR it already readied, so a stopped wave never leaves a PR advertising a readiness no
    human granted. Its own failure is reported — never masking the real reason the wave
    stopped — but does not change the caller's already-decided non-zero return."""
    print(f"→ gh pr ready {pr_url} --undo")
    undo = subprocess.run(["gh", "pr", "ready", str(pr_url), "--undo"],
                          capture_output=True, text=True)
    if undo.returncode != 0:
        print((undo.stderr or undo.stdout).strip(), file=sys.stderr)
        print(f"!!! merge: could not return {pr_url} to draft after declining to merge it "
              "— it is left marked ready; a human must re-draft it.", file=sys.stderr)


#: How many times :func:`_merge_one` will sync a PR onto a base that keeps moving before it
#: gives up (PR #230 review). Not a retry budget — each round is a real sync plus a real gate,
#: and a base that overtakes three of those is busy enough that a human should place the
#: merge. Bounded because the loop is otherwise unbounded on a contended base.
_MAX_SYNC_ROUNDS = 3


def _behind_by(repo_spec: str, pr_url: str) -> int | None:
    """How many commits this PR's head is behind its base — ``None`` when that cannot be
    read, which the caller treats as fail-closed.

    INSTANCE DELTA (eduralph/pdca-harness#531, OPEN). Deliberately asks the compare API
    rather than reading ``mergeStateStatus``: GitHub only reports ``BEHIND`` where the base
    requires strictness, so on a ``strict: false`` base — which is exactly the case this
    delta exists for — a behind PR reads ``CLEAN`` and the staleness is invisible. The
    commit count is the same on either configuration.
    """
    view = subprocess.run(
        ["gh", "pr", "view", str(pr_url), "--json",
         "baseRefName,headRefName,headRepositoryOwner"],
        capture_output=True, text=True)
    if view.returncode != 0:
        return None
    try:
        refs = json.loads(view.stdout or "{}")
        base, head = refs["baseRefName"], refs["headRefName"]
    except (ValueError, KeyError, TypeError):
        return None
    # A fork-based PR's head must be qualified OWNER:BRANCH, exactly as publish.py:301 does
    # for `gh pr create --head`: a bare name resolves against the BASE repo, where the fork's
    # branch does not exist. Unqualified, this compare would 404 (=> None => every merge
    # stops) or, worse, silently compare a same-named base-repo branch — staleness measured
    # against the wrong commit.
    owner = (refs.get("headRepositoryOwner") or {})
    owner_login = owner.get("login") if isinstance(owner, dict) else None
    if owner_login and owner_login != str(repo_spec).split("/")[0]:
        head = f"{owner_login}:{head}"
    cmp_ = subprocess.run(
        ["gh", "api", f"repos/{repo_spec}/compare/{base}...{head}", "--jq", ".behind_by"],
        capture_output=True, text=True)
    if cmp_.returncode != 0:
        return None
    try:
        return int((cmp_.stdout or "").strip())
    except ValueError:
        return None


def _sync_base(pr_url: str) -> bool:
    """Bring a behind PR up to date with its base. INSTANCE DELTA
    (eduralph/pdca-harness#531, OPEN).

    The push re-triggers CI, so the rollup for the new head is EMPTY — which is precisely
    what :func:`_wait_for_green` already polls on. The sync therefore needs no waiting of its
    own; it hands the existing gate a rollup that describes the tree the PR merges into.
    """
    print(f"→ gh pr update-branch {pr_url}")
    r = subprocess.run(["gh", "pr", "update-branch", str(pr_url)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print((r.stderr or r.stdout).strip(), file=sys.stderr)
        return False
    return True


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

    # INSTANCE DELTA — sync a behind PR onto its base BEFORE the rollup gate below
    # (eduralph/pdca-harness#531, OPEN). Without this the gate is honest about the wrong
    # tree: a wave's second merge reads a rollup computed before its sibling landed, so the
    # combination is never verified. Ordering is the whole trick — the sync's push empties
    # the rollup for the new head, and `_wait_for_green` already polls on empty, so the gate
    # below decides on checks that describe the tree this PR actually merges into.
    # Fail-closed on an unreadable behind-state, on the same principle the rollup gate uses:
    # absence of evidence is not green.
    # LOOPS, deliberately. Checking once is not enough: `_wait_for_green` below may wait for
    # up to `merge_wait_secs` (1800 here; the wait itself is upstream since v0.58.0, #462),
    # and a sibling landing on the base during that wait
    # leaves the head behind again — with a green rollup for the pre-move tree, which
    # non-strict protection will happily merge. That is the very failure this exists to
    # prevent, in a smaller window (PR #230 review). So: re-read after every gate, and stop
    # only when the base has not moved.
    synced_and_gated = False
    if cfg.merge_sync_base and repo_spec:
        for _round in range(_MAX_SYNC_ROUNDS):
            behind = _behind_by(str(repo_spec), str(pr_url))
            if behind is None:
                print(f"\n!!! merge: {d.name} ({pr_url}) — could NOT determine whether the "
                      "PR is behind its base, so whether its checks describe the tree it "
                      "would merge into is unknown. STOP: later waves are NOT run; resolve "
                      "at the PR, then re-run (or set [driver] merge_sync_base = false to "
                      "merge without the check).\n", file=sys.stderr)
                _undo_ready(pr_url)
                return 1
            if not behind:
                break
            print(f"   {behind} commit(s) behind base — syncing before the rollup gate")
            if not _sync_base(str(pr_url)):
                print(f"\n!!! merge: {d.name} ({pr_url}) is {behind} commit(s) behind its "
                      "base and could NOT be updated — a conflict with a sibling that "
                      "already merged, or no write access. Merging now would verify a tree "
                      "this PR is not merging into. STOP: later waves are NOT run; resolve "
                      "at the PR, then re-run.\n", file=sys.stderr)
                _undo_ready(pr_url)
                return 1
            # The sync invalidated whatever checks existed, so wait for the new head's —
            # ALWAYS, including under `merge_requires = "required"`. That mode skips the
            # rollup GATE below, but it cannot skip the wait: `gh pr merge` refuses while
            # required checks are pending, so merging straight after a sync would fail every
            # wave at its second PR (PR #230 review).
            print(f"→ gh pr checks {pr_url}  (after sync)")
            verdict, detail = _wait_for_green(str(pr_url), cfg.merge_wait_secs)
            if cfg.merge_requires != "required" and verdict != "green":
                print(f"\n!!! merge: {d.name} ({pr_url}) was NOT merged — after syncing onto "
                      f"its base the checks are {verdict} ({detail}). This is the combination "
                      "check: each fix was green alone, and this is the first time they were "
                      "verified together. STOP: later waves are NOT run.\n", file=sys.stderr)
                _undo_ready(pr_url)
                return 1
            if verdict == "green":
                print(f"   post-sync rollup green ({detail})")
            synced_and_gated = True
        else:
            print(f"\n!!! merge: {d.name} ({pr_url}) — the base moved under this PR "
                  f"{_MAX_SYNC_ROUNDS} times running; each sync was overtaken before it could "
                  "merge. Merging now would verify a tree it is not merging into. STOP: "
                  "later waves are NOT run; merge by hand when the base is quiet, then "
                  "re-run.\n", file=sys.stderr)
            _undo_ready(pr_url)
            return 1

    # Full check-rollup gate (issue #413), read AFTER the ready-mark and immediately before
    # the merge: `gh pr ready` can itself trigger `ready_for_review` CI, so only a rollup
    # read here says anything about green AT MERGE TIME. `gh pr merge` below fails closed
    # only on the checks the host repo marks required in branch protection; this refuses on
    # ANY failing, pending or missing check, whatever that host's protection happens to be.
    # `!= "required"` rather than `== "all"` so an unexpected value gates rather than
    # merging (Config.load already coerces one, but this module is the one that must not
    # merge past a red rollup).
    if cfg.merge_requires != "required" and not synced_and_gated:
        print(f"→ gh pr checks {pr_url}")
        # A wave boundary fires seconds after the PR opened (issue #462), so the first read
        # is routinely pending/empty — not a verdict yet. Wait for it to resolve, bounded by
        # [driver].merge_wait_secs, before treating an unresolved rollup as a refusal.
        verdict, detail = _wait_for_green(str(pr_url), cfg.merge_wait_secs)
        if verdict != "green":
            why = {
                "failing": f"a check is FAILING — {detail}",
                "pending": f"a check has not finished within {cfg.merge_wait_secs}s — "
                           f"{detail}",
                "empty": f"the check rollup was still EMPTY after {cfg.merge_wait_secs}s "
                         f"— {detail}; absence of evidence is not green",
                "unreadable": f"the check rollup could not be read — {detail}",
            }[verdict]
            print(f"\n!!! merge: {d.name} ({pr_url}) was NOT merged — {why}. The host's "
                  "required-checks config is not enough: this wave's base must be green "
                  "before the next wave builds on it. STOP: later waves are NOT run; "
                  "re-run once the checks are green (the run resumes idempotently), or set "
                  "[driver] merge_requires = \"required\" to merge on the host's required "
                  "checks alone, or raise [driver] merge_wait_secs if the checks just take "
                  "longer than that to report.\n", file=sys.stderr)
            _undo_ready(pr_url)
            return 1
        # Positive evidence in the run log that this merge was gated, not merged blind.
        print(f"   check rollup green ({detail})")

    print(f"→ gh pr merge {pr_url} --{method}")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print((r.stderr or r.stdout).strip(), file=sys.stderr)
        print(f"\n!!! merge: {d.name} ({pr_url}) did not merge — a conflict, no merge "
              "rights on the base, or a host-required check that failed or started after "
              "the rollup gate above. STOP: later waves are NOT run; resolve at the PR, "
              "then re-run.\n", file=sys.stderr)
        _undo_ready(pr_url)
        return 1
    # Refresh the base so the NEXT wave's worktree resets to the merged result.
    if repo_spec and repo_spec not in fetched:
        repo = publish._checkout_path(cfg, repo_spec)
        subprocess.run(["git", "-C", str(repo), "fetch", cfg.base_remote],
                       capture_output=True, text=True)
        fetched.add(repo_spec)
    return 0
