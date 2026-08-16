#!/usr/bin/env python3
"""Stop hook for the interactive PDCA leaves — the checked exit contract (issue #331).

The interactive leaves (Plan, sign-off, publish, Act) each have a checkable exit
contract, and until now nothing checked it at the boundary: the driver discards the
session's exit code, so "the human pressed Ctrl-D" and "the leaf discharged its
contract" are the same event. This hook makes the ``/handoff`` check non-optional —
a slash command cannot terminate its own session or be relied on to be typed — by
naming exactly what is missing when a session's turn ends undischarged, with a
deliberate-abandon escape hatch.

It is a REMINDER, not the enforcement (issue #534). ``Stop`` fires at the end of every
assistant turn, so on an interactive leaf it cannot distinguish "the human is being
asked a question" from "the leaf is finishing" — and because a block's stderr goes to
the model rather than the human, repeating the block is a closed loop that only the
human's answer could break. Enforcement therefore lives where the two events are
already distinct: ``pdca_harness.handoff._report_reap``, run by the driver after the
leaf exits, off the same on-disk artifacts. This hook blocks at most ONE turn per
SESSION — bounded by a ``reminded`` marker persisted in the driver's session channel,
not by the envelope's ``stop_hook_active`` alone, which resets on every human reply.

Protocols (mirroring ``builder_guard.py``, the mechanical-discipline peer):

* **Stop hook** (Claude Code): the event arrives as JSON on stdin; exit 0 allows the
  stop, exit 2 blocks it (stderr is fed back to the model). Inert — exit 0 — outside a
  driver-spawned leaf session (no ``PDCA_HANDOFF_ROLE`` in the environment), so an
  ad-hoc human session in the instance is never blocked; inert again once the envelope's
  ``stop_hook_active`` marks this stop as a continuation of a prior block, and inert for
  the rest of the session once the ``reminded`` marker is recorded.
* **``--check <id>``** (vendor-neutral CLI, used by the rendered ``/handoff`` command):
  verify the current leaf's contract for ONE required id; PASS ⇒ 0, FAIL ⇒ 1. There is
  no scan mode. The verdict is exit status + report — nothing is written to the bundle.
* **``--abandon "<why>"``**: the escape hatch — record a TYPED reason in the driver's
  session channel; the next stop is allowed and the driver reports the reason.

All contract logic lives in ``pdca_harness.handoff`` (plain Python, unit-tested
offline); this file only bootstraps the import and speaks the hook protocol.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def _bootstrap():
    """Import the harness from the instance this hook is rendered into."""
    root = Path(os.environ.get("CLAUDE_PROJECT_DIR") or
                Path(__file__).resolve().parents[2])
    src = root / "src"
    if str(src) not in sys.path:
        sys.path.insert(0, str(src))
    from pdca_harness import handoff  # noqa: PLC0415 — deliberate late import
    from pdca_harness.config import Config
    return handoff, Config.load(root)


def _stop_verdict() -> int:
    role = (os.environ.get("PDCA_HANDOFF_ROLE") or "").strip()
    if not role:
        return 0  # not a driver-spawned leaf session — nothing to enforce
    event: dict = {}
    try:
        loaded = json.load(sys.stdin)  # the Stop event envelope
        if isinstance(loaded, dict):
            event = loaded
    except Exception:  # noqa: BLE001 — an unparseable event must not block a human
        pass
    # ONE reminder, never a second (issue #534). `Stop` fires at the end of every
    # assistant turn, not at session end, so for an interactive leaf "the human is being
    # asked a question" and "the leaf is finishing" arrive as the same event — and the
    # block's feedback reaches the MODEL, not the human, so the question never lands.
    # Repeating it is a closed loop with no exit from the model's side, and the pressure
    # it leaves is toward writing the human's own decision artifact to escape.
    #
    # `stop_hook_active` is the bound the runtime itself prescribes, verbatim from the
    # warning it emitted during the observed deadlock: "For Stop/SubagentStop hooks,
    # check stop_hook_active in the input and return success while it's true." (Claude
    # Code also caps consecutive blocks — CLAUDE_CODE_STOP_HOOK_BLOCK_CAP — but that is
    # its backstop, not ours.) Same shape as the official security-guidance plugin's
    # Stop hook. Enforcement proper is the driver's reap (`handoff._report_reap`); this
    # is a nudge that costs at most one turn.
    if event.get("stop_hook_active"):
        return 0
    try:
        handoff, cfg = _bootstrap()
        state = handoff.load_state()
        # `stop_hook_active` caps only the block's IMMEDIATE continuation: once the human
        # replies, the next assistant turn brings a fresh envelope with the flag false, so
        # a multi-turn Plan or sign-off held before its artifact exists would be blocked
        # again on every turn — the loop this cap exists to prevent, and it would make the
        # "will not repeat" promise below a lie. The marker persists in the driver's
        # session channel, which outlives the turn (#534 review, P2).
        if state.get("reminded"):
            return 0
        problems = handoff.stop_problems(cfg, role, state)
    except Exception as exc:  # noqa: BLE001 — a broken check must not trap the session
        print(f"handoff_guard: contract check unavailable ({exc}) — allowing the stop",
              file=sys.stderr)
        return 0
    if not problems:
        return 0
    # Persist BEFORE blocking, and only block if it stuck: an unrecordable marker means
    # the next turn cannot know this fired, and blocking on that would be the unbounded
    # loop again. The driver's reap enforces regardless, so declining costs nothing.
    raw = (os.environ.get(handoff.ENV_STATE) or "").strip()
    if not raw:
        return 0
    handoff.record_reminded(Path(raw))
    if not handoff.load_state().get("reminded"):
        return 0
    print(
        f"Reminder (once — the next stop is allowed either way): this {role} leaf's "
        "exit contract is not discharged yet:\n"
        + "\n".join(f"  - {p}" for p in problems) + "\n"
        "IF YOU WERE ASKING THE HUMAN SOMETHING: this fired on your turn ending, not on "
        "the session ending, and your question did not reach them. Ask it again — this "
        "reminder will not repeat. Never write a contract artifact that records the "
        "human's decision (a sign-off token, a brief) to satisfy this text; that "
        "decision is theirs alone.\n"
        "OTHERWISE: write the missing/malformed artifact(s), verify with `/handoff <id>` "
        "(the required id: the bundle's issue id, or the act-log entry date), then end "
        "the session again. To deliberately abandon instead, run:\n"
        '  python3 .claude/hooks/handoff_guard.py --abandon "<why>"',
        file=sys.stderr)
    return 2


def main() -> int:
    argv = sys.argv[1:]
    if argv and argv[0] == "--check":
        if len(argv) < 2 or not argv[1].strip():
            print("handoff_guard: --check requires an id (issue id or act-log entry "
                  "date) — there is no scan mode", file=sys.stderr)
            return 2
        handoff, cfg = _bootstrap()
        return handoff.run_check(cfg, argv[1])
    if argv and argv[0] == "--abandon":
        handoff, _cfg = _bootstrap()
        raw = os.environ.get(handoff.ENV_STATE, "")
        if not raw:
            print("handoff_guard: no leaf session is registered "
                  f"({handoff.ENV_STATE} unset) — nothing to abandon", file=sys.stderr)
            return 2
        reason = argv[1] if len(argv) > 1 else ""
        if not reason.strip():
            print("handoff_guard: --abandon requires a typed reason — the driver "
                  "reports it when the session ends", file=sys.stderr)
            return 2
        handoff.record_abandon(Path(raw), reason)
        print("handoff_guard: abandonment recorded — the session may now end; the "
              "driver will report the reason")
        return 0
    return _stop_verdict()


if __name__ == "__main__":
    raise SystemExit(main())
