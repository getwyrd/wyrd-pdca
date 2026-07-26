"""Auto-iterate: resolve implementation-only Check findings without stopping for a human.

Issue #264. A big Do lands with implementation defects the reviewer or the adversary
catches — a logic slip, a weak test, a failing gate. Today every one of those parks the
bundle at ``AWAITING_SIGNOFF`` and asks the human to press "iterate-do", which is exactly
the decision the driver could have made itself. The human's judgment is owed only to
findings that are *architecturally* relevant.

The split already exists in the codebase: ``gates._FIVE_FIVE_ONE`` tags each of the 11
check cells ``input | gate | judgment``. The ``gate`` cells (C2 reproduction, C4
verification, T1..T4) are mechanically checkable, so a rebuild can address them; the
``judgment`` cells (C5 causal adequacy, T5 judgment, V validation) and the ``input`` cells
(C1 spec, C3 change) are the human's. ``assemble.collect_needs_human`` tags every §6 item
IMPL or HUMAN from exactly that source.

So: when a bundle reaches ``AWAITING_SIGNOFF`` with at least one IMPL item and nothing the
human must see first, the driver writes an ``iterate-do`` decision and re-drives Do. Anything
else — an empty §6 (a clean bundle awaiting a human accept), a situational HUMAN item, an
exhausted budget — halts as before.

One item is deliberately NOT "something the human must see first": the reviewer's
``Validation — fitness-to-purpose`` row, which its prompt hard-codes to NEEDS-HUMAN on EVERY
cycle whatever it finds (:data:`assemble.STANDING`). It is a constant, so it carries no
signal. Counting it as an ordinary HUMAN item made the original ``all(IMPL)`` rule impossible
to satisfy on a real bundle, and this feature never fired once in production (#293).

Three properties hold by construction:

* **It only ever writes ``iterate-do``.** Never ``accept``, never ``discontinue``. The
  decision goes through the same C6-guarded ``flow._apply_decision`` a human sign-off uses,
  so §9 stays authored solely by ``signoff.record``.
* **It never clears a §6 box.** An ``iterate-do`` archives the whole SUMMARY, unticked, into
  ``iteration-v<N>/``; the rebuild produces a fresh §6.
* **It is bounded.** ``[driver].max_auto_iters`` automatic rounds per bundle, counted in
  ``auto-iterate.json`` (deliberately NOT in ``driver.DOWNSTREAM_OF_BRIEF``, so the archive
  step doesn't move it and the count accumulates across rebuilds). On exhaustion the bundle
  is left at ``AWAITING_SIGNOFF`` for the human — never dropped.

Opt-in: ``[driver].auto_iterate = false`` by default.
"""

from __future__ import annotations

import json
from pathlib import Path

from .assemble import HUMAN, IMPL, NeedsHumanItem
from .config import Config
from .leaves import SIGNOFF_DECISION

BUDGET_FILE = "auto-iterate.json"

# Findings the driver iterated PAST (issue #332). An `iterate-do` archives SUMMARY.md and
# check-review.md and the rebuild assembles a fresh §6, so a HUMAN finding raised in an early
# round exists nowhere afterwards unless the next reviewer happens to raise it again. Over a
# multi-round budget that is a live way to drop a real architectural objection, so each
# deferred item is recorded here and merged back into §6 at handover (assemble.py). Like
# BUDGET_FILE it is deliberately NOT in driver.DOWNSTREAM_OF_BRIEF, so the archive step leaves
# it in place and it accumulates across rebuilds.
DEFERRED_FILE = "deferred-findings.json"

# The only token this module is ever allowed to write.
DECISION = "iterate-do"


def eligible(items: list[NeedsHumanItem]) -> bool:
    """True iff there is implementation work a rebuild can do: at least one IMPL finding.

    An **empty** §6 is deliberately not eligible — that is a clean bundle awaiting a human
    *accept*, and auto-iterate must never accept. Neither is a §6 of HUMAN items with no IMPL
    item beside them: there is nothing for a rebuild to address, so the bundle goes straight
    to the human.

    A HUMAN item no longer disqualifies a bundle that DOES carry implementation work (#332).
    It used to, and the cost was measured: over a 230-attempt corpus only 31 attempts (13.5%)
    were eligible, and a single situational judgment row vetoed the rest however many build
    defects sat beside it. A finding needing a human is not a signal to stop rebuilding — it
    is a signal that Plan overlooked something, and it is the ROUND BUDGET, not the finding,
    that bounds the iteration. Such items are deferred: recorded in :data:`DEFERRED_FILE` and
    merged back into §6 at handover, so nothing raised in an early round is lost.

    The STANDING `Validation` row has never counted either way (#293): the reviewer's prompt
    emits it on every cycle whatever it found, so it is a constant and a constant is not
    evidence about anything.
    """
    return any(item.kind == IMPL for item in items)


def impl_count(items: list[NeedsHumanItem]) -> int:
    """How many findings in this §6 a rebuild can address — the convergence signal."""
    return sum(1 for item in items if item.kind == IMPL)


def _state(d: Path) -> dict:
    """The budget file as a dict. Tolerant of missing/garbled/legacy content, like
    ``loop-telemetry.json`` — a bundle mid-flight when this ships must not crash."""
    try:
        raw = json.loads((d / BUDGET_FILE).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return raw if isinstance(raw, dict) else {}


def count(d: Path) -> int:
    """How many automatic iterations this bundle has already spent."""
    try:
        return int(_state(d)["count"])
    except (KeyError, TypeError, ValueError):
        return 0


def impl_history(d: Path) -> list[int]:
    """The IMPL count observed at each Check that spent a round, oldest first.

    Empty for a pre-#332 budget file, which is the compatibility case that matters: a bundle
    already mid-iteration when this ships has a count but no history, and
    :func:`should_iterate` reads an absent baseline as "cannot test convergence, fire" —
    the pre-#332 behaviour, rather than halting a bundle on a comparison we cannot make.
    """
    raw = _state(d).get("impl_counts")
    if not isinstance(raw, list):
        return []
    return [int(n) for n in raw if isinstance(n, int) and not isinstance(n, bool)]


def bump(d: Path, observed_impl: int) -> int:
    """Spend one automatic iteration, recording the IMPL count that justified it."""
    n = count(d) + 1
    history = impl_history(d) + [int(observed_impl)]
    (d / BUDGET_FILE).write_text(
        json.dumps({"count": n, "impl_counts": history}) + "\n", encoding="utf-8")
    return n


def should_iterate(d: Path, items: list[NeedsHumanItem], cfg: Config) -> tuple[bool, str]:
    """Whether the round about to be spent may fire — ``(fire, why_not)`` (issue #332).

    Two budgets, because "keep trying" and "keep trying only while it is working" are
    different needs and one number cannot express both:

    * ``n <= soft_auto_iters`` — fires unconditionally. Early rounds are allowed to get
      worse: a builder that fixes one defect and uncovers three has still made progress the
      count cannot see, and stopping there would waste the cheap rounds.
    * ``soft_auto_iters < n <= max_auto_iters`` — fires only while the implementation
      findings are not INCREASING. Past the floor, a round that leaves more work than it
      found is not converging, and the escalation ladder has it on the top model tier by
      then, so spinning is expensive.
    * ``n > max_auto_iters`` — never. The hard ceiling is absolute.

    Equal counts continue: the bound is on getting *worse*, not on failing to improve, since
    a round can trade one finding for another of equal number and still be closing in.
    """
    spent = count(d)
    upcoming = spent + 1
    if upcoming > cfg.max_auto_iters:
        return False, f"hard budget spent ({spent}/{cfg.max_auto_iters})"
    if upcoming <= cfg.soft_auto_iters:
        return True, ""
    history = impl_history(d)
    if not history:
        return True, ""  # legacy ledger: no baseline to compare — keep the old behaviour
    now, before = impl_count(items), history[-1]
    if now > before:
        return False, (f"soft budget spent ({spent}/{cfg.soft_auto_iters}) and the "
                       f"implementation findings did not converge ({before} → {now})")
    return True, ""


def deferred(d: Path) -> list[str]:
    """Every HUMAN finding this bundle has iterated past, oldest first, deduped."""
    try:
        raw = json.loads((d / DEFERRED_FILE).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return []
    rows = raw.get("items") if isinstance(raw, dict) else raw
    if not isinstance(rows, list):
        return []
    return [str(r) for r in rows if isinstance(r, str)]


def defer(d: Path, items: list[NeedsHumanItem], *, attempt: int) -> list[str]:
    """Record the HUMAN findings this round is iterating past; return the full ledger.

    Deduped on the item text, oldest first: a reviewer that raises the same objection every
    round must not grow the handover §6 by one copy per round. STANDING is not recorded — it
    is emitted every cycle whatever the reviewer found, so it is not something being deferred.
    """
    ledger = deferred(d)
    seen = {text.casefold() for text in ledger}
    for item in items:
        if item.kind != HUMAN or item.text.casefold() in seen:
            continue
        seen.add(item.text.casefold())
        ledger.append(item.text)
    (d / DEFERRED_FILE).write_text(
        json.dumps({"items": ledger, "through_round": attempt}, indent=1) + "\n",
        encoding="utf-8")
    return ledger


def rationale(items: list[NeedsHumanItem], *, attempt: int) -> str:
    """The §9 "Iteration delta" line, which the driver folds into the brief's carry-forward
    so the next Do iteration isn't blind about why it was rejected.

    The *findings* named here are IMPL items ONLY, and that filter is load-bearing: the
    STANDING `Validation` row and any deferred HUMAN item ride along in ``items``, but no
    builder can act on either, and handing the next Do a human-only judgment call as though
    it were a defect to fix is exactly the failure PR #294's review caught.

    Deferred items are *counted* rather than quoted, for the same reason — the §9 record has
    to say the human's findings still exist and are waiting (they are, in
    :data:`DEFERRED_FILE`, and they return to §6 at handover), without dressing them up as
    build work. Before #332 this line asserted "implementation-level items only, no
    architectural judgment required", which a deferring round makes false.
    """
    findings = "; ".join(item.text for item in items if item.kind == IMPL)
    held = sum(1 for item in items if item.kind == HUMAN)
    tail = (f" {held} finding(s) needing human judgment were deferred to sign-off, not "
            f"addressed here." if held else "")
    return (f"Auto-iterate (round {attempt}): rebuilding for the implementation-level "
            f"findings — {findings}.{tail}")


def write_decision(d: Path, items: list[NeedsHumanItem]) -> None:
    """Write the ``iterate-do`` decision + rationale, and spend one round of the budget.

    Guarded: refuses to write anything for an item set with no implementation work in it, so
    no caller can turn this into an auto-accept.
    """
    if not eligible(items):
        raise ValueError("auto-iterate: refusing to decide on a non-implementation finding set")
    attempt = bump(d, impl_count(items))
    defer(d, items, attempt=attempt)
    (d / SIGNOFF_DECISION).write_text(
        f"{DECISION}\n{rationale(items, attempt=attempt)}\n", encoding="utf-8")
