"""Bundle state derived from files present — no database (docs 03 §state machine).

The state of an issue *is* the set of files in its bundle directory. This module
is the single source of truth for "what state is issue N in"; the driver acts on
the answer. Keeping state in the filesystem is what makes the pipeline resumable
and inspectable (``ls`` answers the question).
"""

from __future__ import annotations

import json
from pathlib import Path

from . import brief, signoff

# The ordered states a bundle moves through. The terminal/halted states
# (UNPLANNED, AWAITING_SIGNOFF, COMPLETE) are where the driver stops and a human
# acts; the rest the driver advances through unattended.
UNPLANNED = "UNPLANNED"  # no brief — human authors it (Plan)
PLANNED = "PLANNED"  # brief present, ready for Do
BUILT = "BUILT"  # patch present, ready for Check (gates + reviewer)
CHECKED = "CHECKED"  # gates + review present, ready to assemble SUMMARY
AWAITING_SIGNOFF = "AWAITING_SIGNOFF"  # SUMMARY assembled, §9 empty — STOP, human
ITERATE_DO = "ITERATE_DO"  # sign-off chose iterate-to-Do
ITERATE_PLAN = "ITERATE_PLAN"  # sign-off chose iterate-to-Plan
COMPLETE = "COMPLETE"  # sign-off accepted — bundle frozen
DISCONTINUED = "DISCONTINUED"  # sign-off chose discontinue — deliberately abandoned, no transition
RESOLVED = "RESOLVED"  # a notes-only tracker whose issue was resolved OUTSIDE a cycle — terminal

# States where the driver does nothing (human work, or done).
HALTED = {UNPLANNED, AWAITING_SIGNOFF, COMPLETE, DISCONTINUED, RESOLVED}

# Close-disposition fast path (issue #60): a bundle whose Plan concluded a close /
# no-fix outcome never builds a patch. Its close marker is the Do artifact — the
# symmetric stand-in for patch.diff — so the state machine reads it as "past Do".
CLOSE_MARKER = "close-disposition"

# Everything Do and Check write — i.e. everything downstream of brief.md. The single
# source of truth (the driver archives exactly this set on iterate; re-exported as
# ``driver.DOWNSTREAM_OF_BRIEF``). It lives HERE because "which files mean a cycle ran"
# is a state question: `is_resolved` uses it to tell a real cycle from a notes-only
# tracker. Includes the close marker (issue #60) so an iterate archives it too.
DOWNSTREAM_OF_BRIEF = [
    "patch.diff",
    "build-notes.md",
    CLOSE_MARKER,
    "MANUAL-VERIFICATION.md",
    "check-gates.json",
    "check-gates.md",
    "check-review.md",
    "SUMMARY.md",
]

# The rest of a cycle's output, by pattern: the advisory artifacts (#64) and each leaf's
# captured error tail (#280). The iterate archive moves these alongside DOWNSTREAM_OF_BRIEF
# (``driver._archive_iteration``), so they are cycle evidence by exactly the same argument —
# and they live here for the same reason: enumerating the set twice is how the two lists drift
# apart, which is the defect this guard exists to close.
DOWNSTREAM_GLOBS = (
    "check-advisory-*.md",
    "*.error.log",
    # Each gate's full captured output (eduralph/pdca-harness#370) — the record behind the
    # row's 120-char evidence line, and the only way a non-reproducing red can be diagnosed.
    "gate-logs/*.log",
)

# Cycle evidence the archive deliberately does NOT move (issue #170) — the one place where
# "what `_archive_iteration` moves" and "what `is_resolved` counts as evidence" must DIFFER.
#
# Both files ACCUMULATE across rebuilds, which is why they are kept out of the two lists
# above, and both are unambiguous proof a cycle ran — a bundle cannot hold `auto-iterate.json`
# without having auto-iterated. Counting them was simply missed when the evidence guard was
# built (#150/#164): a bundle stripped to one of them plus a stray `resolved` notes.json read
# RESOLVED, left the resume set, and had Plan skip it, abandoning a real iteration history
# with nothing reported.
#
# Do NOT "tidy" this by folding these names into DOWNSTREAM_OF_BRIEF. The archive would then
# move them, and each has a distinct failure if it does:
#   auto-iterate.json       the round budget resets every iterate ⇒ auto-iterate never
#                           terminates (`autoiterate.BUDGET_FILE`)
#   deferred-findings.json  a deferred human finding vanishes into iteration-v<N>/ ⇒ exactly
#                           the loss it exists to prevent (`autoiterate.DEFERRED_FILE`)
#
# The names are literals rather than imports because `autoiterate` imports `assemble`, which
# would cycle back here. `test_state_resolved` pins them against those constants, so a rename
# breaks the test rather than silently reopening the misclassification.
CYCLE_EVIDENCE_ONLY = (
    "auto-iterate.json",
    "deferred-findings.json",
)

# §9 outcome token → bundle state. state owns the state names, so the mapping
# lives here; signoff knows only the tokens (no import cycle).
_OUTCOME_TO_STATE = {
    "merged-wider": COMPLETE,
    "accepted": COMPLETE,
    "iterated-to-Do": ITERATE_DO,
    "iterated-to-Plan": ITERATE_PLAN,
    "discontinued": DISCONTINUED,
}


def is_resolved(d: Path) -> bool:
    """True iff this is a **notes-only tracker** (an open-question / research issue
    logged as ``notes.json`` but never carried through a PDCA cycle) whose tracking issue
    was **resolved outside the cycle**, recorded by a top-level ``resolved`` object in
    ``notes.json`` (github state + close date + a note that the question was decided
    in-issue).

    Such a tracker has no result to sign off, so it can never reach COMPLETE/DISCONTINUED
    through the normal transitions and would otherwise sit in the pending UNPLANNED list
    forever. The ``resolved`` record makes it terminal ([`RESOLVED`]).

    A bundle carrying **any** evidence a cycle ran — ``brief.md``, any artifact in
    [`DOWNSTREAM_OF_BRIEF`] or matching [`DOWNSTREAM_GLOBS`], one of the accumulators in
    [`CYCLE_EVIDENCE_ONLY`], or an ``iteration-v*`` archive — is NOT a tracker, so a stray
    ``resolved`` key can never reclassify it (including a rejected cycle left briefless by
    ``iterate-to-Plan``, which archives ``brief.md`` + everything downstream and must stay
    UNPLANNED for its re-plan). A malformed / unreadable ``notes.json`` is "not resolved",
    never a crash (every bundle file is possibly-absent/garbled, the defensive contract of
    this module).
    """
    if (
        (d / "brief.md").exists()
        or any((d / f).exists() for f in DOWNSTREAM_OF_BRIEF)
        or any(q.is_file() for g in DOWNSTREAM_GLOBS for q in d.glob(g))
        # The accumulators the archive skips (#170) — evidence, but never moved.
        or any((d / f).exists() for f in CYCLE_EVIDENCE_ONLY)
        or any(d.glob("iteration-v*"))
    ):
        return False
    p = d / "notes.json"
    if not p.exists():
        return False
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError, ValueError):
        return False
    # A top-level array / string / number / null is valid JSON but not a notes object —
    # guard the type before `.get` so it can never raise (the "never a crash" contract).
    return isinstance(data, dict) and isinstance(data.get("resolved"), dict)


def state(d: Path) -> str:
    """Return the bundle's state from the files present (docs 03 §state)."""
    bp = d / "brief.md"
    if not bp.exists():
        # A briefless bundle is UNPLANNED — UNLESS it is a notes-only tracker whose issue
        # was resolved outside a cycle: that is terminal, not pending work waiting on a
        # Plan.
        return RESOLVED if is_resolved(d) else UNPLANNED
    # Do is done when there's a patch — OR, on the close-disposition fast path, the
    # close marker that stands in for it (a close bundle never builds a patch.diff).
    if not (d / "patch.diff").exists() and not (d / CLOSE_MARKER).exists():
        # Pre-Do only: a brief that's still an unfilled template (Slug missing / a `<…>`
        # placeholder) means the planner never authored it, so treat it as UNPLANNED and
        # let the Plan beat re-plan it instead of being skipped (issue #113). Scoped to
        # the pre-Do boundary so a real, progressed bundle is never reclassified.
        return UNPLANNED if brief.is_placeholder(bp) else PLANNED
    if not (d / "check-gates.json").exists():
        return BUILT
    if not (d / "SUMMARY.md").exists():
        return CHECKED
    if not signoff.is_set(d / "SUMMARY.md"):
        return AWAITING_SIGNOFF
    # is_set() guarantees the token is one of VALID_OUTCOMES, but stay defensive: a
    # token without a mapping (a future outcome added to signoff but not here) means
    # "not validly complete" → AWAITING_SIGNOFF, never a KeyError out of the one
    # primitive the whole driver depends on (testbed issue #3).
    return _OUTCOME_TO_STATE.get(signoff.outcome_token(d / "SUMMARY.md"), AWAITING_SIGNOFF)
