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

**INSTANCE DELTA — ``[driver].merge_wait_secs`` (eduralph/pdca-harness#462, OPEN at
v0.57.0).** Reading the rollup *immediately* after ``gh pr ready`` is honest but early: the
ready-mark can trigger ``ready_for_review`` CI, so the verdict at that instant is ``pending``
and the run STOPs — making the wave-boundary stop the routine outcome of every multi-wave
batch, which is what merge mode exists to avoid. :func:`_await_rollup` waits for the rollup
to SETTLE (polling while ``pending``/``empty``, up to the budget) and then hands it to the
same gate. The gate is unchanged: a red, an unreadable rollup, or an exhausted budget still
refuses and still STOPs. ``merge_wait_secs = 0`` reproduces upstream exactly.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

from . import merged, publish, state
from .config import Config

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


#: How often :func:`_await_rollup` re-reads the rollup while it is unsettled. Not configurable
#: — the budget is (``[driver].merge_wait_secs``); a knob for the interval would only tune how
#: hard we poll GitHub for the same answer.
_POLL_INTERVAL_SECS = 30


def _await_rollup(pr_url: str, budget_secs: int, *,
                  sleep=time.sleep, now=time.monotonic) -> tuple[str, str]:
    """:func:`_check_rollup`, but WAIT for an unsettled rollup to settle first.

    INSTANCE DELTA (eduralph/pdca-harness#462, still OPEN at v0.57.0). ``merge_requires``
    reads the rollup immediately after ``gh pr ready`` — and the ready-mark can itself
    trigger ``ready_for_review`` CI, so at that instant the honest verdict is ``pending``,
    the merge refuses, and the run STOPs. That is not wrong, it is just early: the checks
    the gate wants to see are about to run. Upstream's answer makes the boundary stop the
    ROUTINE outcome of every multi-wave batch, which is the one thing merge mode exists to
    avoid — the operator merges by hand and re-runs, per wave, exactly as if merge mode
    were off.

    So: poll while the rollup is ``pending`` **or** ``empty``, up to ``budget_secs``, and
    hand the SETTLED verdict to the same gate. Both transient states are worth waiting on —
    a rollup is empty in the seconds before CI registers its first check, and treating that
    as terminal is the same "too early" mistake one step further back.

    What this does NOT do is weaken the gate. ``failing`` and ``unreadable`` return at once
    (a red is settled; an unreadable rollup is an auth/`gh` problem that waiting cannot
    fix), and exhausting the budget returns the last unsettled verdict, which still
    refuses and still STOPs. A wait can only ever turn a refusal into a merge that a later,
    slower read would have permitted anyway.

    ``budget_secs <= 0`` reproduces upstream exactly: one read, no wait.
    """
    verdict, detail = _check_rollup(pr_url)
    if budget_secs <= 0 or verdict not in ("pending", "empty"):
        return verdict, detail
    deadline = now() + budget_secs
    print(f"   checks are {verdict} — waiting up to {budget_secs}s for them to settle "
          f"(#462; [driver].merge_wait_secs)")
    while now() < deadline:
        sleep(min(_POLL_INTERVAL_SECS, max(1, int(deadline - now()))))
        verdict, detail = _check_rollup(pr_url)
        if verdict not in ("pending", "empty"):
            return verdict, detail
    return verdict, detail


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

    # Full check-rollup gate (issue #413), read AFTER the ready-mark and immediately before
    # the merge: `gh pr ready` can itself trigger `ready_for_review` CI, so only a rollup
    # read here says anything about green AT MERGE TIME. `gh pr merge` below fails closed
    # only on the checks the host repo marks required in branch protection; this refuses on
    # ANY failing, pending or missing check, whatever that host's protection happens to be.
    # `!= "required"` rather than `== "all"` so an unexpected value gates rather than
    # merging (Config.load already coerces one, but this module is the one that must not
    # merge past a red rollup).
    if cfg.merge_requires != "required":
        print(f"→ gh pr checks {pr_url}")
        verdict, detail = _await_rollup(str(pr_url), cfg.merge_wait_secs)
        if verdict != "green":
            why = {
                "failing": f"a check is FAILING — {detail}",
                "pending": f"a check has not finished — {detail}",
                "empty": f"the check rollup is EMPTY — {detail}; absence of evidence is "
                         "not green",
                "unreadable": f"the check rollup could not be read — {detail}",
            }[verdict]
            print(f"\n!!! merge: {d.name} ({pr_url}) was NOT merged — {why}. The host's "
                  "required-checks config is not enough: this wave's base must be green "
                  "before the next wave builds on it. STOP: later waves are NOT run; "
                  "re-run once the checks are green (the run resumes idempotently — the "
                  "PR stays ready), or set [driver] merge_requires = \"required\" to "
                  "merge on the host's required checks alone.\n", file=sys.stderr)
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
        return 1
    # Refresh the base so the NEXT wave's worktree resets to the merged result.
    if repo_spec and repo_spec not in fetched:
        repo = publish._checkout_path(cfg, repo_spec)
        subprocess.run(["git", "-C", str(repo), "fetch", cfg.base_remote],
                       capture_output=True, text=True)
        fetched.add(repo_spec)
    return 0
