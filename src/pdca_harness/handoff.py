"""The interactive leaves' checked exit contract (issue #331).

Today the driver's entire completion signal for an interactive leaf is process exit —
``leaves._invoke`` runs ``subprocess.run(argv + [seed], ...)`` with no ``check=`` and
captures nothing — so "the human pressed Ctrl-D" and "the leaf discharged its contract"
are the same event, and a malformed/absent artifact is discovered later, far from the
cause. This module gives each interactive leaf a *checked* boundary:

* :func:`run_check` — the ``/handoff <issue_id>`` verdict: verify the CURRENT leaf's
  contract for ONE named id and report PASS/FAIL. Ids are REQUIRED — there is no scan
  mode (prototype finding, getwyrd/wyrd-pdca#166: a scan judges old bundles against a
  contract that postdates them; a named id only ever judges what this session worked).
* :func:`stop_problems` — the artifact verdict that makes the check non-optional: which
  contract artifacts are missing or malformed, unless the session deliberately abandons
  (:func:`record_abandon`). Read on disk, so it needs nothing from the live session.
* :func:`_report_reap` — WHERE that verdict is enforced (issue #534): the driver, after
  the leaf exits. The ``Stop`` hook (``.claude/hooks/handoff_guard.py``) also calls
  :func:`stop_problems`, but only as a one-turn reminder — ``Stop`` fires at the end of
  every assistant turn, so on an interactive leaf it cannot tell a question to the human
  from a session ending, and its feedback goes to the model, not the human.
* :func:`session` — the driver-side registration: env for the spawned leaf naming its
  role and a session-state scratch file (the act-log baseline where authorship must be
  distinguished, the abandon channel, the record of passed ``/handoff`` runs). The
  scratch file lives OUTSIDE the bundle: the gate's verdict is exit status + report,
  never a bundle artifact (prototype finding — no ``handoff.json``). Its exit is the
  reap above.

Which contract applies is derived from the RENDER — the ``interactive = true`` leaves
and their ``agent`` names in ``pdca.toml`` (:func:`contracts`) — not from a hardcoded
leaf list. Per-field brief checks go through :func:`brief.whole_field`, because the
measured corpus (85 bundles) writes multi-line values the line-based
``brief.parse_fields`` reads as empty; and only the fields every template mandates AND
the corpus actually satisfies are required (``Test file`` is legitimately empty in 7
bundles, ``Falsifiability`` absent in 52/85 — neither is required here).
"""

from __future__ import annotations

import contextlib
import dataclasses
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path

from .config import Config, LeafConfig

#: Env var naming the role of the currently running interactive leaf (driver-set).
ENV_ROLE = "PDCA_HANDOFF_ROLE"
#: Env var pointing at the session-state scratch file (driver-created, driver-removed).
ENV_STATE = "PDCA_HANDOFF_STATE"
#: Scratch-file prefix — dot-prefixed and gitignored, like the seed spill (#313).
STATE_PREFIX = ".pdca-handoff-"


# ----------------------------------------------------------------------------
# Which contract applies — derived from the render, not hardcoded.
# ----------------------------------------------------------------------------
def interactive_roles(cfg: Config) -> dict[str, str]:
    """``{role: agent name}`` for every leaf the RENDER marks ``interactive = true``.

    Introspected from the ``Config`` dataclass fields whose value is a
    :class:`LeafConfig`, so an instance that renders a leaf non-interactive sheds its
    contract with no code change (issue #331 criterion f).
    """
    out: dict[str, str] = {}
    for f in dataclasses.fields(cfg):
        v = getattr(cfg, f.name, None)
        if isinstance(v, LeafConfig) and v.interactive:
            out[f.name] = v.agent or f.name
    return out


def contracts(cfg: Config) -> dict[str, str]:
    """The subset of :func:`interactive_roles` that has a defined exit contract.

    The contract *checks* are per-role code (below); whether one is ACTIVE derives from
    the render. An interactive leaf without a defined contract (e.g. the splitter) is
    simply unchecked — never blocked on a contract it does not have.
    """
    checkable = set(_BUNDLE_CONTRACTS) | {"act"}
    return {r: a for r, a in interactive_roles(cfg).items() if r in checkable}


# ----------------------------------------------------------------------------
# The per-role contract checks. Each returns a list of problems; empty ⇒ PASS.
# ----------------------------------------------------------------------------
def _unfilled(value: str) -> bool:
    """A field value that is absent or still the template's ``<…>`` placeholder.

    ``whole_field`` returns values raw, so a multi-line placeholder arrives whole here —
    a leading ``<`` is the template's, never an authored value's."""
    v = value.strip()
    return not v or v.startswith("<")


def check_planner(d: Path, cfg: Config, *, allow_absent: bool = False) -> list[str]:
    """The Plan exit contract: an AUTHORED ``brief.md`` whose declared external
    dependencies are registered AND present (#333/#340 — the same probe the
    pre-dispatch guard runs, so the two verdicts cannot drift apart).

    ``allow_absent`` is the id-seeded batch wrinkle: the batch prompt documents "leave
    it UNPLANNED (write no brief.md) and say why" as a legitimate outcome, so at the
    Stop boundary a wholly-absent brief passes there — a brief that EXISTS malformed
    never does.
    """
    from . import brief as _brief  # local: keep this module import-light for the hook
    from . import doctor as _doctor
    bp = d / "brief.md"
    if not bp.exists():
        if allow_absent:
            return []
        return ["brief.md is missing — the Plan contract is an authored brief"]
    if _brief.is_placeholder(bp):
        return ["brief.md is still an unfilled template copy (Slug is a placeholder) — "
                "author it or remove it"]
    problems: list[str] = []
    # The fields EVERY brief template mandates and the measured corpus satisfies —
    # read via whole_field (multi-line values, #336), never line-based parse_fields.
    # `defect` and `scope` joined the set with #214: they were only ever missing
    # because the splitter/pointer templates spelled their labels differently and
    # the exact lookup read "" — under the shared synonym tuples the whole corpus
    # (112/112 briefs, measured 2026-08-15) resolves both, and the gate now covers
    # the very fields whose silent absence Check adjudicated against.
    for labels in (("slug",), _brief.DEFECT_LABELS, ("success criterion",),
                   ("repo + branch target",), _brief.SCOPE_LABELS):
        if _unfilled(_brief.whole_field(bp, *labels)):
            problems.append(f"brief.md field '{labels[0]}' is empty or an unfilled "
                            "placeholder — it is required by every brief template")
    # The dependency clause (#331 layer over #333/#340): every backticked token must
    # name a registered [[doctor.checks]] row whose detect cmd exits 0; an annotated
    # `(no-check: …)` token yields no token at all and is exempt by construction.
    problems += _doctor.unregistered_dependencies(bp, cfg)
    problems += _doctor.failing_dependencies(bp, cfg)
    return problems


def check_signoff(d: Path, cfg: Config) -> list[str]:
    """The sign-off exit contract: ``signoff-decision`` carries one valid token, plus a
    rationale below it for every non-accept decision (that rationale IS the session
    carry-forward the driver captures — see :data:`state.SESSION_CARRY`)."""
    from . import leaves as _leaves  # local: leaves imports this module at top level
    p = d / _leaves.SIGNOFF_DECISION
    tokens = ", ".join(sorted(_leaves.VALID_DECISIONS))
    if not p.exists():
        return [f"{_leaves.SIGNOFF_DECISION} is missing — write the agreed decision "
                f"(one of: {tokens}) as its first line"]
    token = _leaves.signoff_decision(d)
    if not token:
        first = (p.read_text(encoding="utf-8").splitlines() or [""])[0].strip()
        return [f"{_leaves.SIGNOFF_DECISION} first line {first!r} is not a valid "
                f"decision token (one of: {tokens})"]
    if token != "accept" and not _leaves.signoff_rationale(d):
        return [f"decision '{token}' has no rationale below the token — write WHY "
                "(why rejected / what to change, or why discontinued); the driver "
                "carries it into the next attempt's brief"]
    return []


def check_publisher(d: Path, cfg: Config) -> list[str]:
    """The publish exit contract: both contribution artifacts exist, non-empty, and
    pass the instance's own deterministic lint (``cli.contribution_problems`` — the
    same rules as the T4 ``contribcheck`` gate, reused rather than re-declared)."""
    from . import cli as _cli
    from . import publish as _publish
    problems: list[str] = []
    for name in (_publish.COMMIT_MSG, _publish.PR_BODY):
        p = d / name
        if not p.is_file() or not p.read_text(encoding="utf-8").strip():
            problems.append(f"{name} is missing or empty — the publish contract is "
                            "exactly these two artifacts")
    if not problems:
        problems += _cli.contribution_problems(d)
    return problems


def check_act(cfg: Config, entry: str, baseline: dict) -> list[str]:
    """The Act exit contract: the session NAMES the act-log entry it wrote.

    ``entry`` is the id the session hands ``/handoff`` (the entry's date). ``baseline``
    is the driver's session-start snapshot of ``process/act-log.md`` — supplied by the
    driver because an end-of-session command structurally cannot take one — and is what
    distinguishes an entry THIS session wrote from one that predates it.
    """
    if not entry.strip():
        return ["an entry id is required — run `/handoff <entry-date>` naming the "
                "act-log entry this session wrote (there is no scan mode)"]
    log = cfg.process_dir / "act-log.md"
    try:
        text = log.read_text(encoding="utf-8") if log.exists() else ""
    except OSError:
        text = ""
    if entry not in text:
        return [f"process/act-log.md has no entry containing '{entry}' — append the "
                "dated entry (even a 'no delta warranted' one) and name it here"]
    if baseline:
        if _sha(text) == baseline.get("act_log_sha"):
            return ["process/act-log.md is unchanged since this session started — the "
                    f"entry '{entry}' predates the session; append THIS session's entry"]
        prev_len = baseline.get("act_log_len")
        if isinstance(prev_len, int) and 0 <= prev_len <= len(text) \
                and entry not in text[prev_len:]:
            return [f"'{entry}' appears only in act-log text that predates this session "
                    "— name the entry THIS session appended"]
    return []


_BUNDLE_CONTRACTS = {
    "planner": check_planner,
    "signoff": check_signoff,
    "publisher": check_publisher,
}


def check_bundle(role: str, d: Path, cfg: Config, **kw) -> list[str]:
    """Dispatch the bundle-scoped contract for ``role``; unknown role ⇒ a problem
    naming it (never a silent pass for a contract that was asked for by name)."""
    fn = _BUNDLE_CONTRACTS.get(role)
    if fn is None:
        return [f"no exit contract is defined for role '{role}'"]
    return fn(d, cfg, **kw) if role == "planner" else fn(d, cfg)


# ----------------------------------------------------------------------------
# Session state — the driver-owned channel (env + a scratch file OUTSIDE the bundle).
# ----------------------------------------------------------------------------
def _sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _read_json(path: Path | None) -> dict:
    if path is None:
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def load_state(environ: dict | None = None) -> dict:
    """The current session's state dict, from :data:`ENV_STATE` — ``{}`` if none."""
    env = environ if environ is not None else os.environ
    raw = env.get(ENV_STATE, "")
    return _read_json(Path(raw)) if raw else {}


def _update_state(path: Path, **fields) -> None:
    data = _read_json(path)
    data.update(fields)
    try:
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    except OSError:
        pass  # best-effort — the artifact checks stand on their own


def record_pass(path: Path, ident: str) -> None:
    """Record a passed ``/handoff <ident>`` — how the session NAMES its work where the
    driver could not register a bundle set at spawn (CSV-batch Plan, Act)."""
    data = _read_json(path)
    passed = list(data.get("passed") or [])
    if ident not in passed:
        passed.append(ident)
    _update_state(path, passed=passed)


def record_abandon(path: Path, reason: str) -> None:
    """The deliberate-abandon escape hatch: a TYPED reason, recorded in the driver's
    session channel (never the bundle). Both the Stop hook and the driver's reap then
    treat the contract as settled, and the driver reports the reason."""
    _update_state(path, abandoned=reason.strip() or "(no reason given)")


def record_reminded(path: Path) -> None:
    """Cap the Stop hook's reminder at one per SESSION, not one per block (#534 review).

    ``stop_hook_active`` bounds only the immediate model continuation of a block. After
    the human replies, the next assistant turn arrives with a fresh envelope where the
    flag is false again — so a multi-turn Plan or sign-off, held before its contract
    artifact exists, would be blocked and re-reminded on EVERY turn. That is the same
    closed loop the cap exists to prevent, and it would make the hook's own promise
    ("this reminder will not repeat") false.

    This marker lives in the driver's session channel, which outlives the turn, so the
    reminder survives exactly once per spawned leaf. Best-effort, like every write here:
    if it cannot be persisted the hook declines to block at all rather than risk the
    loop — the driver's reap is the enforcement either way.
    """
    _update_state(path, reminded=True)


@contextlib.contextmanager
def session(cfg: Config, role: str, bundles: list[Path] | None = None, *,
            require_artifact: bool = True, outcome: dict | None = None):
    """Driver-side registration for one interactive leaf session (issue #331 e).

    Yields the env to merge into the spawn: the role and a session-state scratch file
    (created in ``cfg.root`` with the gitignored :data:`STATE_PREFIX`, removed on exit).
    Captures the session-start act-log baseline for the act role. Yields ``{}`` — no
    contract — when the render does not mark the leaf interactive (criterion f), and on
    ANY setup failure (a checked exit contract must never break the leaf it checks).

    ``outcome``, when a dict is passed, receives ``{"discharged": bool}`` at reap — for
    the caller whose OWN side effect must not fire on an undischarged session (#534
    review, P1). Most callers need nothing: their side effect is the artifact itself, so
    a missing one already stops the state machine. Act is the exception —
    ``mark_reviewed`` advances a global frontier that no artifact gates. It is ``True``
    on every no-contract path (non-interactive leaf, setup failure, a broken check): a
    contract that was never established must not withhold a caller's work.
    """
    if outcome is not None:
        outcome["discharged"] = True
    leaf = getattr(cfg, role, None)
    if not isinstance(leaf, LeafConfig) or not leaf.interactive:
        yield {}
        return
    path: Path | None = None
    try:
        baseline: dict = {}
        if role == "act":
            log = cfg.process_dir / "act-log.md"
            text = log.read_text(encoding="utf-8") if log.exists() else ""
            baseline = {"act_log_len": len(text), "act_log_sha": _sha(text)}
        fh = tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=cfg.root,
            prefix=STATE_PREFIX, suffix=".json", delete=False)
        with fh:
            json.dump({
                "role": role,
                "bundles": [str(b) for b in (bundles or [])],
                "require_artifact": bool(require_artifact),
                "baseline": baseline,
                "passed": [],
            }, fh, indent=2)
        path = Path(fh.name)
        env = {ENV_ROLE: role, ENV_STATE: str(path)}
    except OSError as exc:
        print(f"handoff: could not register the {role} session state ({exc}) — the "
              "exit contract is unenforced for this session", file=sys.stderr)
        yield {}
        return
    try:
        yield env
    finally:
        try:
            discharged = _report_reap(cfg, role, _read_json(path))
        except Exception as exc:  # noqa: BLE001 — a broken check must not mask the leaf
            print(f"handoff: contract check unavailable at reap ({exc}) — the {role} "
                  "session's exit contract is unverified", file=sys.stderr)
            discharged = True  # unverified is not "failed": never withhold on a crash
        if outcome is not None:
            outcome["discharged"] = discharged
        path.unlink(missing_ok=True)


def _report_reap(cfg: Config, role: str, state: dict) -> bool:
    """Report one interactive leaf's exit contract when the DRIVER reaps it (#534).

    This is where enforcement lives. :func:`stop_problems` re-verifies artifacts on
    disk and needs nothing from the live session, so the driver can check them here,
    after the leaf exits — which is the only place the two events the Stop hook cannot
    tell apart ("the human is being asked a question" / "the leaf is finishing") are
    already distinct. The hook stays, capped, as an in-session reminder only.

    Mostly reporting: for the artifact-backed roles an undischarged contract means the
    artifact the state machine reads is missing or malformed, so the driver does not
    advance on it anyway, and what this adds is that the human is TOLD, naming each item,
    at the moment the session ends rather than at the next state read.

    Returns True when the contract is SETTLED — discharged, or deliberately abandoned —
    and False when the session ended undischarged. :func:`session` relays that through
    its ``outcome`` argument, because one caller's side effect is NOT artifact-backed and
    so is not self-gating: Act's ``mark_reviewed`` advances the review frontier and would
    otherwise retire cycles a session never reviewed (#534 review, P1).

    It deliberately does NOT claim "nothing was advanced" — that is the caller's fact to
    report, not this function's to assert; the Act path says it for itself.
    """
    reason = str(state.get("abandoned") or "").strip()
    if reason:
        print(f"handoff: the {role} session was deliberately abandoned — {reason}",
              file=sys.stderr)
        return True
    problems = stop_problems(cfg, role, state)
    if not problems:
        return True
    print(f"handoff: the {role} session ended with its exit contract UNDISCHARGED:",
          file=sys.stderr)
    for p in problems:
        print(f"  - {p}", file=sys.stderr)
    print(f"handoff: re-run the {role} leaf, or record a deliberate abandon next time "
          '(`python3 .claude/hooks/handoff_guard.py --abandon "<why>"`).',
          file=sys.stderr)
    return False


# ----------------------------------------------------------------------------
# The two verdicts: /handoff (ergonomics) and the artifact check (enforcement at the
# driver's reap, a capped reminder in the Stop hook — #534).
# ----------------------------------------------------------------------------
def resolve_bundle(cfg: Config, ident: str) -> Path:
    """``issue_331`` / ``331`` → the bundle dir, matching ``cfg.bundle`` keying."""
    return cfg.bundle(str(ident).strip().removeprefix("issue_"))


def run_check(cfg: Config, ident: str, *, role: str | None = None,
              environ: dict | None = None) -> int:
    """The ``/handoff <id>`` verdict: verify the current leaf's contract for ONE id.

    PASS ⇒ 0, FAIL ⇒ 1, no active contract ⇒ 2. The verdict is exit status + report —
    nothing is written into the bundle. A pass is recorded in the session state file
    (when one is registered), which is how a session whose work set the driver could
    not know at spawn names what it did.
    """
    env = environ if environ is not None else dict(os.environ)
    role = (role or env.get(ENV_ROLE) or "").strip()
    if not role:
        print("handoff: no leaf contract is active in this session "
              f"({ENV_ROLE} unset) — nothing to verify", file=sys.stderr)
        return 2
    active = contracts(cfg)
    if role not in active:
        print(f"handoff: the render defines no exit contract for role '{role}' "
              "(not interactive, or no contract exists) — nothing to verify",
              file=sys.stderr)
        return 2
    state = load_state(env)
    if role == "act":
        problems = check_act(cfg, ident, state.get("baseline") or {})
        label = f"handoff({role}) {ident}"
    else:
        d = resolve_bundle(cfg, ident)
        problems = check_bundle(role, d, cfg)
        label = f"handoff({role}) {d.name}"
    if problems:
        print(f"{label}: FAIL — the {role} exit contract is not discharged:")
        for p in problems:
            print(f"  - {p}")
        print("Fix the items above and run /handoff again. To deliberately abandon "
              "instead: python3 .claude/hooks/handoff_guard.py --abandon \"<why>\"")
        return 1
    print(f"{label}: PASS — the {role} exit contract is discharged")
    raw = env.get(ENV_STATE, "")
    if raw:
        record_pass(Path(raw), ident)
    return 0


def stop_problems(cfg: Config, role: str, state: dict) -> list[str]:
    """The artifact verdict: why this session's contract is not discharged (empty ⇒ it is).

    Re-verifies the ARTIFACTS for every bundle the driver registered at spawn — the
    contract is the artifacts, so a session that discharged them without ever typing
    ``/handoff`` still ends cleanly. Where the driver could not register the work set
    (the CSV-batch planner, Act), the session must have named its work through a
    passing ``/handoff``. A recorded abandonment always allows the stop.
    """
    if state.get("abandoned"):
        return []
    if role not in contracts(cfg):
        return []
    if role == "act":
        if state.get("passed"):
            return []
        return ["the act session has not verified its exit contract — append the dated "
                "act-log entry (even a 'no delta warranted' one) and run "
                "`/handoff <entry-date>` naming it"]
    bundles = [Path(b) for b in (state.get("bundles") or [])]
    if bundles:
        allow_absent = role == "planner" and not state.get("require_artifact", True)
        out: list[str] = []
        for d in bundles:
            kw = {"allow_absent": allow_absent} if role == "planner" else {}
            out += [f"{d.name}: {p}" for p in check_bundle(role, d, cfg, **kw)]
        return out
    if state.get("passed"):
        return []
    return [f"the {role} session registered no bundle set at spawn and verified none — "
            "run `/handoff issue_<id>` for each bundle this session worked"]
