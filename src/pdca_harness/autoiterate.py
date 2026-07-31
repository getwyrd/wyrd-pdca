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

from . import signoff, size_signal
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

# Matching a ticked §6 row to a ledger entry when the human edited the MIDDLE of the row, so
# neither string contains the other. A fixed character threshold is brittle exactly where it
# matters — the two strings in the motivating case share 39 characters and diverge on the 40th
# — so the test is PROPORTIONAL: the shared opening must be most of the shorter string.
#
# That is what discriminates. Two different findings on the same 5/5/1 element share only the
# element and label (~21 characters of "C5 Causal adequacy — ") before diverging, which is a
# small fraction of a full §6 row; one finding and its annotated self share nearly all of it.
_MATCH_RATIO = 0.6      # of the shorter string
_MATCH_FLOOR = 20       # …and never fewer than this many characters, so short rows can't match


def _same_finding(a: str, b: str) -> bool:
    """Do these two §6 texts name the same finding, allowing for a human's edit?"""
    a, b = " ".join(a.split()).casefold(), " ".join(b.split()).casefold()
    if not a or not b:
        return False
    if a == b or a in b or b in a:
        return True
    shared = 0
    for x, y in zip(a, b):
        if x != y:
            break
        shared += 1
    shortest = min(len(a), len(b))
    return shared >= max(_MATCH_FLOOR, int(shortest * _MATCH_RATIO))


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

    The ONE HUMAN item that does disqualify: the empirical size backstop (#324). It is
    evidence that MORE REBUILDS ARE THE WRONG MOVE — findings on an oversized slice look
    implementation-shaped every round, so rebuilding past it turns the backstop into an
    accelerator for the exact failure it exists to stop. The KIND is the mechanism, not
    the text: the same wording tagged IMPL counts as ordinary rebuild work, which is what
    keeps this a deliberate override rather than a text-based veto (upstream pins both
    directions in test_size_signal).
    """
    if any(item.kind == HUMAN and size_signal.is_size_item(item.text) for item in items):
        return False
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


# Set by `defer` when it writes the ledger, and ONLY by it (PR #168 review round 10). The
# previous discriminator was "the budget carries impl_counts", which `observe` writes at every
# Check — so a pre-#332 bundle that merely reached sign-off acquired the key without ever
# having a ledger, and was then reported as having LOST one, blocking every accept forever.
# A marker the ledger's own writer sets cannot be acquired that way.
LEDGER_MARK = "ledger"


def _write_state(d: Path, **updates) -> None:
    """Merge into the budget file. Every writer goes through here, so none can drop a key
    another owns — `observe` and `bump` used to rewrite the whole object, which is exactly how
    a marker like `LEDGER_MARK` would be lost."""
    st = _state(d)
    st.update(updates)
    (d / BUDGET_FILE).write_text(json.dumps(st) + "\n", encoding="utf-8")


def count(d: Path) -> int:
    """How many automatic iterations this bundle has already spent."""
    try:
        return int(_state(d)["count"])
    except (KeyError, TypeError, ValueError):
        return 0


def generation(d: Path) -> int:
    """Which Check this is: the number of rebuilds archived so far (PR #168 review round 4).

    An observation has to be tied to the Check that produced it, not merely appended. Without
    that, re-evaluating an UNCHANGED bundle rewrote the baseline with its own count — so a
    growth halt at 2 -> 4 became 4 -> 4 on the next look, which reads as convergent, and the
    round that was just refused fired with no rebuild and no human in between. A batch
    sign-off pass that records no decision, or an operator re-running `pdca flow`, is enough
    to trigger it.

    `iteration-v*` is the right generation counter because the archive is what a rebuild
    creates: same Check re-read ⇒ same generation ⇒ the observation is REPLACED, not appended.
    """
    return len(list(d.glob("iteration-v*")))


def _observations(d: Path) -> list[list[int]]:
    """Raw ``[generation, impl_count]`` rows, oldest first. Tolerant of the pre-round-4 shape
    (a flat list of counts), which is read as one row per generation in order."""
    raw = _state(d).get("impl_counts")
    if not isinstance(raw, list):
        return []
    out: list[list[int]] = []
    for i, row in enumerate(raw):
        if isinstance(row, list) and len(row) == 2 and all(
                isinstance(v, int) and not isinstance(v, bool) for v in row):
            out.append([row[0], row[1]])
        elif isinstance(row, int) and not isinstance(row, bool):
            out.append([i, row])          # legacy flat entry
    return out


def _record_observation(d: Path, items: list[NeedsHumanItem]) -> list[list[int]]:
    """This Check's observation, REPLACING any earlier one for the same generation."""
    gen, now = generation(d), impl_count(items)
    rows = [r for r in _observations(d) if r[0] != gen]
    rows.append([gen, now])
    rows.sort()
    return rows


def impl_history(d: Path) -> list[int]:
    """The IMPL count observed at each Check that reached sign-off, oldest first.

    Appended by :func:`observe` on EVERY such Check — including ones that halted — so the
    baseline always represents the Check immediately preceding the next rebuild, whoever
    triggered it (PR #168 review round 3). It is deliberately not tied to the round counter.

    Empty for a pre-#332 budget file, which is the compatibility case that matters: a bundle
    already mid-iteration when this ships has a count but no history, and
    :func:`should_iterate` reads an absent baseline as "cannot test convergence, fire" —
    the pre-#332 behaviour, rather than halting a bundle on a comparison we cannot make.
    """
    return [n for _gen, n in _observations(d)]


def observe(d: Path, items: list[NeedsHumanItem]) -> None:
    """Record this Check's IMPL count as the convergence baseline (PR #168 review round 3).

    Called at EVERY Check that reaches sign-off, not only at the ones that auto-iterate — the
    history has to represent the Check immediately preceding the next rebuild, whoever
    triggered it. When only automatic rounds appended, a human `iterate-do` taken after a
    growth halt left the baseline pointing at the last AUTOMATIC round's input: a halt at
    2 -> 4 followed by a human-driven improvement to 3 was then read as 2 -> 3 and halted
    again, while a drop to 2 could restart automatic rounds that should not have resumed.
    """
    # `count(d)` rather than a direct int() on the raw field: `count` is tolerant of a
    # garbled or legacy value by design, and `observe` now runs at EVERY Check that reaches
    # sign-off, so raising here would abort the flow on a corrupted field that every other
    # reader in this module degrades past (PR #168 review round 4).
    _write_state(d, impl_counts=_record_observation(d, items))


def bump(d: Path) -> int:
    """Spend one automatic iteration. The observation is recorded separately by `observe`."""
    n = count(d) + 1
    _write_state(d, count=n)
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
    # Compare against the observation from a PREVIOUS generation. Anything recorded for the
    # current one is this same Check being re-read, and comparing a Check with itself always
    # looks convergent (PR #168 review round 4).
    # EXACTLY generation-1 (PR #168 review round 6). Taking the newest older observation let
    # a GAP — auto-iterate disabled for a run, or an unreadable ledger returning before the
    # observation — silently substitute a much older Check: observed gen 0 at 2, unobserved
    # gen 1 at 4, then gen 2 at 3 read as a 2 -> 3 regression and halted, though the rebuild
    # had improved 4 -> 3. A gap is not a baseline; it is the absence of one.
    prior = [n for gen, n in _observations(d) if gen == generation(d) - 1]
    if not prior:
        return True, ""  # no observation for the preceding Check — nothing to compare against
    now, before = impl_count(items), prior[-1]
    if now > before:
        return False, (f"soft budget spent ({spent}/{cfg.soft_auto_iters}) and the "
                       f"implementation findings did not converge ({before} → {now})")
    return True, ""


class DeferredLedgerUnreadable(Exception):
    """The deferred ledger exists but cannot be read (PR #168 review, P1).

    Distinguished from an ABSENT ledger on purpose. Absent means "nothing has been deferred
    yet" — the ordinary first-round state, and reading it as an empty list is correct. But a
    file that exists and will not parse is a ledger whose contents we have LOST, and treating
    that as empty is the one failure this whole mechanism exists to prevent: the next
    :func:`defer` would rewrite the file from the current §6 alone, and the following
    ``iterate-do`` would archive the current ``SUMMARY.md``, so every objection deferred in an
    earlier round would be gone from every artifact at once.

    Every other reader in this module is deliberately tolerant of a garbled file (a bundle
    must never crash the flow). This one is not, because the tolerant reading is silently
    destructive in the accepting direction — auto-iterate would carry on rebuilding while the
    human's findings evaporated.
    """


def _read_items(d: Path) -> list[str]:
    """The ledger's items, tolerating absence. For `defer`, which creates the file."""
    try:
        return deferred(d)
    except DeferredLedgerUnreadable:
        p = d / DEFERRED_FILE
        if not p.exists():
            return []
        raise


def deferred(d: Path) -> list[str]:
    """Every HUMAN finding this bundle has iterated past, oldest first, deduped.

    Raises :class:`DeferredLedgerUnreadable` if the file exists but does not parse — see
    that class for why this one reader refuses to fail soft. An ABSENT file is [].
    """
    p = d / DEFERRED_FILE
    if not p.exists():
        # Absence is only innocent BEFORE the first round (PR #168 review round 9). `defer`
        # runs on every `write_decision`, immediately after `bump`, and writes unconditionally
        # — an empty `items` list included. So on a bundle that has SPENT a round under this
        # code, a missing file means the write was interrupted between bump and defer, or the
        # file was deleted. Reading that as "nothing was deferred" would let `_apply_decision`
        # accept against a stale SUMMARY and discard earlier rounds' HUMAN findings for good,
        # which is the same fail-open the unreadable-content path already closes.
        #
        # A PRE-#332 bundle also has a count and no ledger, and must stay innocent — it ran
        # before the ledger existed.
        if _state(d).get(LEDGER_MARK):
            raise DeferredLedgerUnreadable(
                f"{p} is absent though this bundle has written one before — the ledger was "
                f"deleted or its write was interrupted")
        return []
    try:
        raw = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise DeferredLedgerUnreadable(f"{p} exists but cannot be read: {exc}") from exc
    rows = raw.get("items") if isinstance(raw, dict) else raw
    if not isinstance(rows, list):
        raise DeferredLedgerUnreadable(f"{p} holds no `items` list")
    # A non-string entry fails CLOSED too (PR #168 review). Filtering it out would report the
    # ledger as readable while dropping an entry we cannot interpret — and the next `defer`
    # rewrites the file without it, so a partial recovery or a schema change would erase a
    # held objection permanently, without ever raising the §6 warning. Syntactically valid
    # JSON we cannot read is still a ledger we have lost.
    bad = [r for r in rows if not isinstance(r, str)]
    if bad:
        raise DeferredLedgerUnreadable(
            f"{p} holds {len(bad)} non-string entry(s) — cannot tell what was deferred")
    return list(rows)


def _norm_line(line: str) -> str:
    """A §6 line reduced to its finding text, checkbox stripped."""
    body = line.strip()
    for box in ("- [ ]", "- [x]", "- [X]"):
        if body.startswith(box):
            body = body[len(box):]
            break
    return " ".join(body.split()).casefold()


def retire_cleared(d: Path, summary_path: Path) -> list[str]:
    """Drop ledger entries the human has TICKED in §6; return what remains (PR #168 review).

    A deferred finding re-enters §6 unchecked on every assembly. Without this, a human who
    adjudicates one and then chooses ``iterate-do`` for some *other* reason loses that
    adjudication: the transition archives the ticked ``SUMMARY.md``, the next assembly
    recreates the entry from the ledger unchecked, and the same objection blocks accept again
    — every round, permanently, with no way to clear it.

    The tick is the human's authority, so it is what retires the entry — and it must be read
    POSITIVELY, from the ticked rows themselves. An earlier version inferred clearance from
    absence ("no longer in `open_needs_human`"), which silently deleted any entry the human
    had *edited* rather than ticked: annotating `- [ ] needs an ADR` into
    `- [ ] needs an ADR (owner: architecture)` left it neither open under its old text nor
    checked, so the objection vanished on the next archive despite never being adjudicated
    (PR #168 review). Absence is not consent; only a tick is.

    Called at the iterate transition, while the ticked SUMMARY is still at the top level and
    before ``_archive_iteration`` moves it. Best-effort on an unreadable ledger: leave it
    alone rather than rewrite it from a partial read — `deferred` raises there, and the
    accept-guard already holds the bundle.
    """
    try:
        ledger = deferred(d)
    except DeferredLedgerUnreadable:
        return []
    if not ledger:
        return []
    ticked = [line[len("- [x]"):].strip().casefold()
              for line in signoff.cleared_needs_human(summary_path)]
    # Match by CONTAINMENT, not equality (PR #168 review round 3). A human who annotates a row
    # while ticking it — `- [x] needs an ADR (owner: architecture)` — has adjudicated it just
    # as definitely as one who ticked it untouched, but the text no longer equals the ledger
    # entry, so an exact test left it deferred and the next assembly recreated it UNCHECKED.
    # An explicitly cleared finding would then block sign-off again, forever.
    #
    # Containment in either direction covers both edits: text appended to the row (the common
    # annotation) and text trimmed from it. Both sides are long sentences lifted from §6, so
    # an accidental match between two distinct findings is not a realistic shape — and this
    # only ever fires on a row the human positively ticked, which is the guard that matters.
    # FAIL CLOSED on an ambiguous tick (PR #168 review round 4). The proportional match from
    # round 3 survives a human's edit, but two genuinely distinct findings can share a long
    # opening — "C5 Causal adequacy — guards the symptom in the parser" and the corresponding
    # renderer objection — and merging them would retire an UNADJUDICATED objection off the
    # back of a tick meant for its neighbour. That is unrecoverable; a finding that lingers is
    # merely visible.
    #
    # So a tick retires exactly one entry or none. This is the same shape `_needs_human` uses
    # for two STANDING candidates: when at least one of the matches must be wrong and we
    # cannot tell which, decide for neither.
    # A ledger entry that is STILL VISIBLY UNCHECKED in this §6 has not been adjudicated,
    # whatever else fuzzy-matched (PR #168 review round 6). Without this, a newly raised
    # finding resembling a deferred one — a renderer objection beside a deferred parser
    # objection sharing a long "C5 Causal adequacy — guards the symptom…" opening — could be
    # ticked and retire the OTHER entry, while the deferred row sat unticked in plain sight.
    still_open = [_norm_line(line) for line in signoff.open_needs_human(summary_path)]

    # The open row must be recognised by the SAME relation the tick is matched with
    # (issue #173). Round 6 compared open rows by exact text while the tick matched fuzzily,
    # so the guard was exactly as strong as its weaker half: a human who ANNOTATED an open
    # row — `- [ ] …parser… (owner: architecture)` — and ticked a similar new finding had
    # the annotated row slip the exclusion, and the never-adjudicated entry retired anyway.
    # Assignment is EXACT-first, mirroring the tick match below: an open row that is
    # verbatim some ledger entry protects THAT entry alone — so a still-open near-twin
    # cannot shield its exactly-ticked neighbour (the round-5 drain stays drainable) —
    # while an EDITED open row protects every entry it could name, failing closed on the
    # same doctrine as round 4: a lingering finding is visible, a lost one is unrecoverable.
    # (Upstream: eduralph/pdca-harness#335; this render carries the fix until that lands.)
    protected: set[int] = set()
    for o in still_open:
        owned = [i for i, t in enumerate(ledger) if _norm_line(t) == o]
        protected.update(owned if owned else
                         [i for i, t in enumerate(ledger) if _same_finding(t, o)])

    retire: set[int] = set()
    for row in ticked:
        # EXACT first (PR #168 review round 5). The round-4 fail-closed rule counted an exact
        # match and a fuzzy one as equally ambiguous, so a pair of near-identical findings
        # became permanently unclearable: ticking either row unchanged still matched both, so
        # neither retired, and no amount of ticking could ever drain them. An exact match is
        # not ambiguous — it is the row the ledger rendered.
        exact = [i for i, t in enumerate(ledger) if _norm_line(t) == _norm_line(row)]
        hits = exact if exact else [i for i, t in enumerate(ledger)
                                    if _same_finding(t, row)]
        if len(hits) == 1 and hits[0] not in protected:
            retire.add(hits[0])
    kept = [t for i, t in enumerate(ledger) if i not in retire]
    if len(kept) != len(ledger):
        (d / DEFERRED_FILE).write_text(
            json.dumps({"items": kept}, indent=1) + "\n", encoding="utf-8")
    return kept


def defer(d: Path, items: list[NeedsHumanItem], *, attempt: int) -> list[str]:
    """Record the HUMAN findings this round is iterating past; return the full ledger.

    Deduped on the item text, oldest first: a reviewer that raises the same objection every
    round must not grow the handover §6 by one copy per round. STANDING is not recorded — it
    is emitted every cycle whatever the reviewer found, so it is not something being deferred.
    """
    # Read tolerantly: this function CREATES the ledger, so it cannot go through the
    # absence guard in `deferred()` — on the first round the file legitimately does not
    # exist yet.
    ledger = _read_items(d)
    seen = {text.casefold() for text in ledger}
    for item in items:
        if item.kind != HUMAN or item.text.casefold() in seen:
            continue
        seen.add(item.text.casefold())
        ledger.append(item.text)
    (d / DEFERRED_FILE).write_text(
        json.dumps({"items": ledger, "through_round": attempt}, indent=1) + "\n",
        encoding="utf-8")
    # Marker AFTER the file, so an interruption between them leaves a ledger and no marker —
    # innocent — rather than a marker with no ledger, which reads as loss.
    _write_state(d, **{LEDGER_MARK: True})
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
    # Ledger FIRST, then the counter (PR #168 review round 9). The absence guard in
    # `deferred()` reads "count >= 1 with no ledger" as a lost file, so the two writes must
    # happen in the order that keeps that true under interruption: a crash between them then
    # leaves a ledger and no spent round, which is harmless, rather than a spent round whose
    # ledger never appeared, which now reads as data loss.
    defer(d, items, attempt=count(d) + 1)
    attempt = bump(d)
    (d / SIGNOFF_DECISION).write_text(
        f"{DECISION}\n{rationale(items, attempt=attempt)}\n", encoding="utf-8")
