"""Assemble ``SUMMARY.md`` from brief + gates + review (docs 02 §SUMMARY.md).

Pure code, no model: the driver assembles §1–8 from the brief, the gate JSON, and
the reviewer's findings, routes every reviewer ``NEEDS-HUMAN`` into §6, and leaves
§9 (sign-off) and §10 (Act candidates) empty for the human. The section shape
mirrors ``templates/SUMMARY.md.tpl`` — keep the two in step if you edit either.
"""

from __future__ import annotations

import functools
import json
import re
from pathlib import Path
from typing import NamedTuple

from . import brief, doctor, size_signal, state
from .config import Config
from .gates import canonical_elements

# The two kinds of §6 item (issue #264).
#   IMPL  — an implementation defect the BUILDER can fix by iterating Do.
#   HUMAN — an architectural / fitness-to-purpose / environmental call only the human makes.
IMPL = "impl"
HUMAN = "human"
# The reviewer's `Validation — fitness-to-purpose` row, which its prompt hard-codes to
# NEEDS-HUMAN on EVERY cycle regardless of content (agents/reviewer.md.jinja; the 5/5/1's
# validation oracle is literally "human at sign-off"). It is the human's to settle at sign-off
# — but because it is emitted unconditionally it carries NO signal, so it must not be read as
# evidence that a human has to look *right now*. Treating it as an ordinary HUMAN item made
# auto-iterate (#264) unreachable in production: every real review artifact carries this row,
# and `eligible()` demanded that EVERY item be IMPL, so the feature never once fired (#293).
#
# It still renders in §6 as a `- [ ]` the human must clear, and the C6 accept-guard still
# blocks on it. The ONLY thing it no longer does is veto a rebuild.
STANDING = "standing"


class NeedsHumanItem(NamedTuple):
    """One §6 row: the text the human reads, plus who can resolve it."""

    text: str
    kind: str


# Which classification survives when the same finding arrives twice. HUMAN wins over IMPL —
# it reaches the human either way and never triggers a rebuild on its own — and both win over
# STANDING, since a finding a leaf actually wrote about is not the signal-free constant however
# it is spelled (PR #168 review rounds 3-5).
_SAFEST = {HUMAN: 0, IMPL: 1, STANDING: 2}


def resolve_duplicates(items: list[NeedsHumanItem]) -> list[NeedsHumanItem]:
    """Collapse repeated findings to their SAFEST classification, keeping first position.

    Applied twice on purpose. Within one artifact it stops a leaf that spells one objection
    two ways from routing on output ORDER. Across the whole collected set it stops something
    worse: the primary reviewer emitting a plain HUMAN judgment while an advisory tags the
    same text `[impl]` left BOTH entries standing, and since `eligible()` now needs only one
    IMPL item anywhere, that advisory tag sent an explicitly human-only concern to the builder
    unattended (PR #168 review round 5).
    """
    out: list[NeedsHumanItem] = []
    at: dict[str, int] = {}
    for item in items:
        key = item.text.casefold()
        if key not in at:
            at[key] = len(out)
            out.append(item)
        elif _SAFEST[item.kind] < _SAFEST[out[at[key]].kind]:
            out[at[key]] = item     # keep the first POSITION, take the safer kind
    return out


# The implementation/architectural split is NOT a new taxonomy — it is the `kind` already
# carried by the canonical 5/5/1 (gates._FIVE_FIVE_ONE). `gate` cells (C2/C4/T1..T4) are
# mechanically checkable ⇒ builder-fixable. `judgment` cells (C5 causal adequacy, T5
# judgment, V validation) and `input` cells (C1 spec, C3 change) are the human's.
_GATE_ELEMENTS = frozenset(e for e, _label, kind, _oracle in canonical_elements()
                           if kind == "gate")

# Elements the REVIEWER may promote to builder-fixable with an `[impl]` tag (issue #332).
# The taxonomy says where a finding sits; only the reviewer knows what it actually IS, and a
# judgment cell routinely carries an ordinary build defect (a weak causal argument that is
# really a missing case). So a judgment row may be promoted — but nothing else:
#   * `input` cells (C1 spec, C3 change) are NOT promotable. A defective brief is a Plan miss,
#     and rebuilding against the same brief cannot fix it — that is an iterate-PLAN, which
#     this module deliberately cannot decide.
#   * `V` is NOT promotable. It is the STANDING row, emitted every cycle whatever the reviewer
#     found (#293); a constant carries no signal in either direction.
_PROMOTABLE_ELEMENTS = frozenset(e for e, _label, kind, _oracle in canonical_elements()
                                 if kind == "judgment") - {"V"}

# A §6 item's leading 5/5/1 element id, when the reviewer's table row carries one.
_ELEMENT_RE = re.compile(r"^(C[1-5]|T[1-5]|V)\b")

# An advisory leaf tags a builder-fixable finding `- NEEDS-HUMAN [impl] — …`. Unmarked
# findings stay HUMAN, so a legacy advisory file can never trigger an auto-iteration.
_IMPL_MARKER_RE = re.compile(r"^\[impl\]\s*[—:-]*\s*", re.IGNORECASE)

# `[human]` is the explicit counterpart #332 asks the advisory leaves for. It carries no
# classification (an untagged bullet is HUMAN anyway — that is the fail-safe), so its only job
# is to prove the leaf DECIDED rather than forgot. It must still be stripped from the stored
# text: left in, the same objection written `- NEEDS-HUMAN [human] — X` one round and
# `- NEEDS-HUMAN — X` the next dedups as two different findings, and §6 grows a duplicate
# blocking box the human must clear twice (PR #168 review).
_HUMAN_MARKER_RE = re.compile(r"^\[human\]\s*[—:-]*\s*", re.IGNORECASE)

# The one STANDING row (#293) — recognised by the canonical label, not a hardcoded string, so
# it cannot drift from the matrix the reviewer's table mirrors. `V` is the only element the
# reviewer's prompt hard-codes to NEEDS-HUMAN on every cycle; C5/T5 are judgment too, but the
# reviewer raises those only when it has an actual concern, so they stay situational HUMAN.
_V_LABEL = next(label for e, label, _kind, _oracle in canonical_elements() if e == "V")
# Every 5/5/1 Item cell, used to recognise the MANDATED verdict table itself — the row alone was
# never enough (PR #294 review, local pass): a "## Concerns" table can carry the exact same label.
_CANONICAL_LABELS = frozenset(label.strip().casefold()
                              for _e, label, _kind, _oracle in canonical_elements())

# An Item cell's optional leading element id (issue #332). `leaves._REVIEW_PROMPT` lists the
# matrix as `{elem} — {label}` and then asks for "the element label above" in the Item column,
# so `V — Validation — fitness-to-purpose` is the literal reading of the instruction — and 37
# rows of the wyrd corpus wrote exactly that, against 185 bare ones. The exact-match STANDING
# test then failed and the constant row became a HUMAN veto: #293 returning through a
# formatting variant. Normalize the prefix away before comparing.
_ITEM_ELEMENT_PREFIX_RE = re.compile(r"^(?:C[1-5]|T[1-5]|V)\s*[—–-]{1,2}\s*")


def _normalized_item_label(cell: str) -> str:
    """An Item cell reduced to its canonical label for an EXACT comparison.

    Strips a leading element id and folds an ASCII `--` to the em-dash the matrix uses (one
    corpus row wrote `Validation -- fitness-to-purpose`). Deliberately narrow: only the
    element prefix and the separator are normalized, never the label text. Matching the
    label by PREFIX is what PR #294 identified as letting a real objection wear the
    template's clothes, so the comparison stays exact — this only removes decoration the
    prompt itself invites.
    """
    text = _ITEM_ELEMENT_PREFIX_RE.sub("", cell.strip(), count=1)
    return re.sub(r"\s*--\s*", " — ", text).strip()

# Leaf-status marker (issue #278). When a reviewer / advisory leaf could not produce a
# verdict, `leaves` writes a placeholder carrying one of these as a machine-readable comment.
# An EMPTY advisory artifact is otherwise ambiguous: "the adversary ran and found nothing"
# reads identically to "the adversary never ran" — and an infra failure then presents as a
# clean adversarial pass. The status lets §6 say WHY the artifact is empty, and lets a
# consumer act on it (re-run vs adjudicate) instead of parsing prose.
# Both INFRA shapes mean "nothing reviewed the diff", but they call for different ACTIONS, so
# the §6 row must not conflate them: a transient blip is safe to re-run as-is, while a leaf
# whose command could never be launched will fail identically until that command is fixed —
# telling the operator "safe to re-run" there would be a false instruction (PR #285 review).
LEAF_STATUS_INFRA = "infra-empty"      # ran, died with no output — a transient blip
LEAF_STATUS_STARTUP = "startup-empty"  # never launched — binary absent / not executable
LEAF_STATUS_HUMAN = "human-empty"      # ran, but yielded no usable verdict
_LEAF_STATUS_RE = re.compile(r"<!--\s*pdca:leaf-status\s+(\S+)\s*-->")
_LEAF_STATUS_LABEL = {
    LEAF_STATUS_INFRA: "leaf did not run (transient infra — safe to re-run)",
    LEAF_STATUS_STARTUP: ("leaf did not run (its command could not be launched — fix the "
                          "leaf's config, then re-run)"),
    LEAF_STATUS_HUMAN: "leaf produced no usable verdict (needs a human)",
}


def leaf_status(artifact_text: str) -> str:
    """The leaf-status marker a reviewer/advisory placeholder carries, or "" for a real
    artifact (a leaf that actually produced findings) — issue #278."""
    m = _LEAF_STATUS_RE.search(artifact_text)
    return m.group(1) if m else ""


def _one_line(value: str) -> str:
    """A brief value flattened to one line, for a context that cannot hold a newline.

    Only the SUMMARY title uses this: it is a Markdown `#` heading, so the two-space
    continuation indent :func:`_item` applies would render as literal text rather than a
    wrapped list item (#336).
    """
    return " ".join(value.split())


def _item(value: str) -> str:
    """A brief value rendered as the tail of a SUMMARY `- Label: …` bullet.

    Continuations are indented two spaces so a multi-line value stays ONE Markdown list
    item instead of terminating the list and dumping the remainder as body prose (#336).
    """
    lines = value.splitlines() or [""]
    # A value whose FIRST line is itself a list item — ORDERED (`1.`, `2)`) as well as
    # unordered — is a nested list under an empty
    # label — the documented Scope/API shape. Rendering it inline gives
    # `- Scope: - **API:** …` with the remaining bullets nested beneath, which flattens the
    # first child into the label and changes the brief's meaning in SUMMARY. Put the whole
    # block on its own lines instead, so the hierarchy the brief authored survives.
    if re.match(r"^\s*(?:[-*+]|\d+[.)])\s", lines[0]):
        return "\n" + "\n".join(f"  {line}" if line else "" for line in lines)
    first, *rest = lines
    return "\n".join([first] + [f"  {line}" if line else "" for line in rest])


def _classify_finding(text: str, *, standing: bool = False,
                      tagged_impl: bool = False) -> NeedsHumanItem:
    """Classify one reviewer / advisory §6 item, stripping any `[impl]` marker.

    Three kinds. IMPL — a rebuild can address it. STANDING — the reviewer's `Validation` row,
    which its prompt emits NEEDS-HUMAN on every cycle whatever it finds, so its presence proves
    nothing (#293). HUMAN — everything else.

    ``standing`` is decided by the CALLER and defaults off. It is true only for the canonical
    5/5/1 verdict row of the PRIMARY review, identified by an exact match on its Item cell
    (:func:`_needs_human`). This function does not re-derive it from the text, deliberately: a
    prefix test on the text is what let a real objection wear the template's clothes, and two
    sources of truth for "is this the constant row" is what produced that bug (PR #294 review).

    Fail safe throughout: an item we cannot map to a gate element — an unmarked advisory bullet,
    a reviewer row whose Item cell doesn't start with a canonical id, the missing-review
    placeholder — is HUMAN. Auto-iterate only ever fires on findings we positively know a
    rebuild can address, and STANDING is never one of them: it does not *cause* a rebuild, it
    merely declines to veto one.
    """
    stripped = _IMPL_MARKER_RE.sub("", text, count=1)
    if stripped != text:
        return NeedsHumanItem(stripped.strip(), IMPL)
    # `[human]` normalizes away without changing the verdict — the tag only records that the
    # leaf made the call. Keeping it in the text would make the tagged and untagged spellings
    # of one objection dedup as two.
    dehumanized = _HUMAN_MARKER_RE.sub("", text, count=1)
    if dehumanized != text:
        return NeedsHumanItem(dehumanized.strip(), HUMAN)
    if standing:
        return NeedsHumanItem(text, STANDING)   # emitted every cycle ⇒ carries no signal (#293)
    m = _ELEMENT_RE.match(text)
    if m and m.group(1) in _GATE_ELEMENTS:
        return NeedsHumanItem(text, IMPL)
    # The reviewer tagged its own verdict cell `NEEDS-HUMAN [impl]` (#332). Honoured only on a
    # judgment cell — see :data:`_PROMOTABLE_ELEMENTS`. Ordered AFTER the standing check on
    # purpose: a tag on the V row must not turn the constant into a rebuild trigger. An
    # unmappable row is not promotable either, since there is no element to check the tag
    # against, and this function fails safe to HUMAN throughout.
    if tagged_impl and m and m.group(1) in _PROMOTABLE_ELEMENTS:
        return NeedsHumanItem(text, IMPL)
    return NeedsHumanItem(text, HUMAN)


def _items_from_artifact(text: str, *, allow_standing: bool = False) -> list[NeedsHumanItem]:
    """§6 items from one reviewer / advisory artifact, labelled by its leaf status (#278).

    ``allow_standing`` is passed only for the PRIMARY review (#294 review) — see
    :func:`_classify_finding`. An advisory leaf's free-form bullets never earn STANDING.

    A placeholder (the leaf could not produce a verdict) has its items prefixed with WHY the
    artifact is empty — infra vs substance — so the human doesn't have to hand-annotate it,
    and forced to HUMAN: there is no finding for a rebuild to fix, so an infra-empty must
    never be auto-iterated (#264). A real artifact is unaffected."""
    label = _LEAF_STATUS_LABEL.get(leaf_status(text), "")
    # Dedup on the CLASSIFIED text, not the raw line. `_needs_human` keys its own `seen` set
    # on the raw text, which is before the `[impl]` / `[human]` marker is stripped — so one
    # objection written both ways in a round survives as two identical §6 boxes the human has
    # to clear twice (PR #168 review round 2).
    items = resolve_duplicates([
        _classify_finding(f.text, standing=allow_standing and f.standing,
                          tagged_impl=f.tagged_impl)
        for f in _needs_human(text)])
    if not label:
        return items
    return [NeedsHumanItem(f"{label} — {it.text}", HUMAN) for it in items]


def collect_needs_human(d: Path, cfg: Config) -> list[NeedsHumanItem]:
    """Every §6 item for this bundle, tagged IMPL / HUMAN, in the order §6 renders them.

    Single source for both the rendered §6 and the auto-iterate decision (issue #264), so
    the classifier can never disagree with what the C6 accept-guard sees.
    """
    gates_json = json.loads((d / "check-gates.json").read_text(encoding="utf-8"))
    review_path = d / "check-review.md"
    review_text = (review_path.read_text(encoding="utf-8")
                   if review_path.exists() else _missing_review_text(d))
    advisory_texts = [p.read_text(encoding="utf-8")
                      for p in sorted(d.glob("check-advisory-*.md"))]

    # Only the PRIMARY review may carry a STANDING row: it is the one artifact whose prompt
    # mandates the Validation row unconditionally, which is the entire basis for treating it as
    # signal-free. An advisory leaf raising fitness-to-purpose means it FOUND something.
    items = _items_from_artifact(review_text, allow_standing=True)
    for atext in advisory_texts:
        items += _items_from_artifact(atext)
    # A gate that COULD NOT RUN is not builder-fixable — rebuilding would spin against the
    # same missing mechanic — so it is HUMAN regardless of its (gate-kind) element.
    items += [NeedsHumanItem(t, HUMAN) for t in _unverifiable_items(gates_json)]
    items += _failed_gating_items(gates_json)
    items += _flaky_gate_items(gates_json)
    build_notes = d / "build-notes.md"
    if build_notes.exists():
        items += [NeedsHumanItem(t, HUMAN)
                  for t in _declared_external_deps(build_notes.read_text(encoding="utf-8"))]
    # The Do-exit adjudication record (#341): a declaration the detect-cmd probe REFUTED
    # proceeded to full Check, and the refutation must reach the human — and `pdca act
    # index`, which reads §6 — rather than stay a bundle-local json only the driver saw.
    # HUMAN, never IMPL: a rebuild cannot fix a mis-declaration. Local import, because
    # dependency_halt delegates its marker parsing to `_declared_external_deps` above
    # (one parser for "did the builder declare a dependency").
    from . import dependency_halt
    items += [NeedsHumanItem(t, HUMAN) for t in dependency_halt.refuted_items(d)]
    items += [NeedsHumanItem(t, HUMAN)
              for t in _unregistered_dependency_items(d / "brief.md", cfg)]
    # Plan-advisory findings (#301 + review): folded into §6 individually, exactly like
    # the Check advisories — including the decorrelation note and any NOT-COMPLETED
    # placeholder, which no other summary path reads. Each finding stays visible until
    # the human dispositions it at sign-off: a bundle-wide "was the brief revised?" bit
    # cannot say WHICH findings the revision addressed, so it must never suppress them
    # (one cosmetic edit would have hidden every remaining objection from C6). All
    # HUMAN-kind by construction (the plan prompt emits no [impl] markers), so
    # auto-iterate correctly declines (#264).
    for ptext in [p.read_text(encoding="utf-8")
                  for p in sorted(d.glob("plan-advisory-*.md"))]:
        items += _items_from_artifact(ptext)
    # The empirical size backstop (#324). HUMAN, never IMPL — and that tag is the whole
    # mechanism: `autoiterate.eligible()` requires every item be IMPL or STANDING, so this
    # DISQUALIFIES auto-iterate, which is what should happen to a bundle behaving
    # oversized. Tagged IMPL it would instead count as a reason to rebuild, turning the
    # backstop into an accelerator for the very failure it exists to stop.
    # `current`, not `read`: the recorded file wins, but its ABSENCE must not read as
    # "measured and small". A failed write would otherwise delete the backstop.
    size_reasons = size_signal.oversize_reasons(size_signal.current(d, cfg), cfg)
    if size_reasons:
        items += [NeedsHumanItem(size_signal.needs_human_text(size_reasons), HUMAN)]
    # Across EVERY source, INCLUDING the ledger (PR #168 review round 6). Filtering deferred
    # entries against the current set first was the bug: a finding stored as HUMAN that a
    # later review re-raised as IMPL had its deferred copy suppressed by the text filter, so
    # the IMPL classification stood alone — `eligible()` allowed another unattended round and
    # `rationale()` handed the explicitly deferred judgment to the builder. Appending them as
    # ordinary members and letting `resolve_duplicates` decide keeps HUMAN precedence across
    # the ledger boundary too.
    return resolve_duplicates(items + _deferred_items(d))


def _deferred_items(d: Path) -> list[NeedsHumanItem]:
    """Findings earlier auto-iterate rounds passed over, re-entering §6 (issue #332).

    An ``iterate-do`` archives SUMMARY.md and check-review.md, and the rebuild assembles a
    fresh §6 from a fresh review — so a HUMAN finding raised in round 1 survives nowhere
    unless the next reviewer independently raises it again. Auto-iterate may now run several
    rounds past such a finding, so without this the driver could quietly iterate a real
    architectural objection out of existence and hand the human a §6 that never mentions it.

    Re-entering them here rather than only at render keeps :func:`collect_needs_human` the
    single source it claims to be: the C6 accept-guard, the rendered §6 and the auto-iterate
    classifier all see the same set. They are HUMAN, so they never make a bundle eligible on
    their own, and they no longer block a rebuild either — they simply must not be lost.
    """
    from . import autoiterate  # local import: autoiterate imports this module
    try:
        held = autoiterate.deferred(d)
    except autoiterate.DeferredLedgerUnreadable as exc:
        # Assembly must never crash on a bundle file (the defensive contract of this module),
        # but it must not silently drop the ledger either — that is the failure the ledger
        # exists to prevent. Surface it as a §6 item instead: the human sees that findings
        # were lost, and the C6 accept-guard holds until they clear it. `flow` separately
        # refuses to auto-iterate a bundle in this state, so no rebuild runs meanwhile.
        return [NeedsHumanItem(
            f"the deferred-findings ledger is unreadable — findings held over from earlier "
            f"auto-iterate rounds may be LOST; recover or reconstruct it before accepting "
            f"({exc})", HUMAN)]
    return [NeedsHumanItem(t, HUMAN) for t in held]


def ensure_section6_item(summary_path: Path, text: str) -> bool:
    """Append one unchecked §6 item to an ALREADY-ASSEMBLED summary; True if it was added.

    For a condition discovered after assembly that the C6 accept-guard must see (PR #168
    review round 7). Appending in place rather than re-assembling is deliberate: a
    re-assemble regenerates §6 from the artifacts and would discard any box the human has
    already ticked in this sign-off session.

    Idempotent, and best-effort like every other writer here — a summary that cannot be read
    or written is not worth crashing a flow over, and the caller still halts either way.
    """
    try:
        body = summary_path.read_text(encoding="utf-8")
    except OSError:
        return False
    if text.split("(")[0].strip() in body:
        return False
    lines, out, placed = body.splitlines(keepends=True), [], False
    for line in lines:
        if not placed and re.match(r"^#+\s*7\.", line):
            out.append(f"- [ ] {text}\n\n")
            placed = True
        out.append(line)
    if not placed:
        out.append(f"\n- [ ] {text}\n")
    try:
        summary_path.write_text("".join(out), encoding="utf-8")
    except OSError:
        return False
    return True

def _plan_advisory_benefit(d: Path) -> dict | None:
    """The bundle's plan-advisory benefit record (#301), or None if absent/unreadable —
    the same tolerant contract as every other bundle-file read (testbed #3)."""
    p = d / "plan-advisory-benefit.json"
    if not p.exists():
        return None
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except (ValueError, OSError):
        return None
    return data if isinstance(data, dict) else None


def assemble_summary(d: Path, cfg: Config) -> None:
    bp = d / "brief.md"
    fields = brief.parse_fields(bp)   # still used for the title's deliberate [:40] truncation
    gates = json.loads((d / "check-gates.json").read_text(encoding="utf-8"))
    review_path = d / "check-review.md"
    # The review is advisory; a missing one (e.g. the reviewer's model connection
    # dropped mid-run) must not crash this deterministic step. Fall back to a
    # placeholder that routes a blocking item into §6 — so the bundle still assembles
    # and reaches sign-off, but can't be accepted until a real review exists.
    review_text = (
        review_path.read_text(encoding="utf-8")
        if review_path.exists()
        else _missing_review_text(d)
    )
    # Optional advisory reviewers (issue #64): each check-advisory-<id>.md is folded into
    # §5 and its NEEDS-HUMAN findings into §6, exactly like the main reviewer.
    advisory_paths = sorted(d.glob("check-advisory-*.md"))
    advisory_texts = [p.read_text(encoding="utf-8") for p in advisory_paths]

    # §6 is fed by the reviewer's NEEDS-HUMAN verdicts, the advisory reviewers', any gate
    # that declared itself unverifiable (issue #46), any gating gate that hard-FAILED
    # (issue #166), a builder-declared external dependency Plan didn't list (#250), and a
    # declared dependency with no registered doctor row (#263) — all become `- [ ]` items
    # the C6 guard makes the human clear before accept. `collect_needs_human` is the single
    # source (it also tags each item IMPL/HUMAN for the auto-iterate decision, #264).
    needs_human = [it.text for it in collect_needs_human(d, cfg)]

    advisory_block = "\n".join(
        f"\n### Advisory — {p.stem.removeprefix('check-advisory-')}\n\n{t.strip()}"
        for p, t in zip(advisory_paths, advisory_texts)
    )

    issue = d.name.replace("issue_", "")
    # §1-8 render the brief's spec fields for a human (and the C6 accept-guard) to judge
    # against, so they must carry the WHOLE value — `parse_fields` is line-based and cut
    # every wrapped field at its first line (#336). `fields` stays for everything that
    # genuinely wants one line.
    spec = functools.partial(brief.whole_field, d / "brief.md")
    out = "\n".join(
        [
            f"# Result — issue {issue} / {_one_line(spec('slug') or fields.get('defect', '')[:40])}",
            "",
            "## 1. Spec (from brief.md)              ← Check verifies against THIS",
            f"- Defect / goal: {_item(spec('defect', 'goal'))}",
            f"- Success criterion: {_item(spec('success criterion'))}",
            f"- Repo + branch target: {_item(spec('repo + branch target', 'branch target'))}",
            f"- Scope (one logical fix) / out of scope: {_item(spec('scope'))}",
            "",
            "## 2. Disposition claimed               ← sign-off confirms or overrides",
            f"- Outcome: {_item(spec('disposition hint', default='Fixed'))}",
            "- Confidence: medium",
            "- Recommendation: (set by Do)",
            "",
            "## 3. Correctness (Check — chain)",
            _gate_lines(gates, prefix="C"),
            "",
            "## 4. Conformance (Check — stack)",
            _gate_lines(gates, prefix="T"),
            "- T5 judgment: → see §5.",
            "",
            "## 5. Advisory review (artifact-only, decorrelated)",
            "Reviewer ran without build-notes.md. Summary:",
            "",
            review_text.strip(),
            advisory_block,
            "",
            "## 6. NEEDS-HUMAN — items the human must clear before sign-off",
            _needs_human_block(needs_human),
            "",
            "## 7. Proven / not proven",
            f"- Proven by which oracle: gates overall = {gates['overall']} (stub oracles).",
            "- Unproven / needs manual run: anything flagged in §6.",
            "",
            "## 8. Ready-to-ship attachments",
            "- patch.diff",
            "- tracker-comment.md     (ALWAYS, every tracker item)",
            "- build-notes.md         (builder rationale — for the human, not the reviewer)",
            "",
            "## 9. Check sign-off                     ← human completes Check here",
            "- Disposition confirmed / overridden:",
            "- Outcome:",
            "- Iteration delta (if iterating):",
            "- By / date:",
            "",
            "## 10. Act candidates (hints for the next Act review)",
            *_plan_advisory_act_lines(d),
            "- (empty is the common case)",
            "",
        ]
    )
    (d / "SUMMARY.md").write_text(out, encoding="utf-8")


def _plan_advisory_act_lines(d: Path) -> list[str]:
    """§10 line for the plan-advisory benefit record (#301): benefit telemetry is process
    signal — exactly what Act reviews to judge whether plan reviews pay off over cycles."""
    benefit = _plan_advisory_benefit(d)
    if not benefit:
        return []
    return [f"- Plan advisory: {benefit.get('findings', 0)} finding(s); brief revised: "
            f"{'yes' if benefit.get('revised') else 'no'} (plan-advisory-*.md)"]


def _gate_lines(gates: dict, *, prefix: str) -> str:
    lines = []
    for r in gates["rows"]:
        if r["check"].startswith(prefix):
            ev = r["path_line"] or r["oracle"]
            lines.append(f"- {r['check']}: {r['result']} — {ev}")
    return "\n".join(lines)


def _unverifiable_items(gates: dict) -> list[str]:
    """Gate rows the mechanic couldn't run (``result == "unverifiable"``) → §6 items, so
    the C6 accept-guard forces the human to clear them before accept (issue #46).

    ``unverifiable`` ONLY — a ``deferred`` row (issue #401) is deliberately NOT lifted, the
    single difference between the two gate-declared, non-gating results. ``unverifiable``
    means "nobody has an answer, so a human must decide"; ``deferred`` means "this row's
    substantive audit runs later, at a gate that cannot be skipped"
    (``gates._deferrable`` → ``publish.publish_gates``) — there is nothing for the human to
    clear. Lifting it anyway is the defect this closes: the Check-time T4 contribution row is
    default-open by design (its artifacts are drafted at publish), and its vacuous green fired
    a §6 NEEDS-HUMAN on 9 of 9 frozen bundles, cleared unread every time — which trains the
    human to tick §6 boxes, the very guard C6 depends on. The row stays visible in §5
    evidence (:func:`_gate_lines`) with its reason, so what is owed at publish is still read.
    """
    return [
        f"{r['check']} unverifiable — {r['path_line'] or r['oracle'] or 'no reason given'}"
        for r in gates["rows"]
        if r.get("result") == "unverifiable"
    ]


def _failed_gating_items(gates: dict) -> list[NeedsHumanItem]:
    """A **gating** gate that returned a hard FAIL → a §6 NEEDS-HUMAN item (issue #166).

    Without this, only ``unverifiable`` rows reached §6; a gating ``fail`` set
    ``overall = fail`` and showed in §5 but added no §6 item — and the C6 accept-guard
    (:func:`signoff.open_needs_human`) only blocks on open §6 ``- [ ]`` items, so a red
    gating gate could be signed off to COMPLETE. Routing it here forces the human to clear
    it (accept with override, iterate, or discontinue) before sign-off.

    The kind comes from the row's structured ``element`` (issue #264), never from parsing
    its label — an instance names its own gates, so the label may not start with the id.
    A blank / unrecognised element is HUMAN (fail safe).
    """
    return [
        NeedsHumanItem(
            f"{r['check']} FAILED (gating) — {r['path_line'] or r['oracle'] or 'no reason given'}",
            IMPL if r.get("element") in _GATE_ELEMENTS else HUMAN,
        )
        for r in gates["rows"]
        if r.get("gating") and r.get("result") == "fail"
    ]


def _flaky_gate_items(gates: dict) -> list[NeedsHumanItem]:
    """A gating gate that failed, then PASSED its once-only confirm re-run (#371 upstream)
    → a §6 NEEDS-HUMAN item. The row records ``pass``, so nothing else would surface the
    flip — and a flake swallowed silently is indistinguishable from a clean green, which
    is how flaky substrate stays flaky. HUMAN, not IMPL: a rebuild cannot fix the
    substrate the gate ran on, so auto-iterate defers the item to the human instead of
    spending a round on it; the C6 accept-guard still makes the human acknowledge it."""
    items: list[NeedsHumanItem] = []
    for r in gates["rows"]:
        if not r.get("flaky"):
            continue
        where = f" (full output: {r['log']})" if r.get("log") else ""
        items.append(NeedsHumanItem(
            f"{r['check']} flaked at Check — failed, then passed its once-only confirm "
            f"re-run{where} — confirm the pass is trustworthy and note what interfered",
            HUMAN,
        ))
    return items


def _missing_review_text(d: Path) -> str:
    """Placeholder when ``check-review.md`` is absent — flags a §6 NEEDS-HUMAN so the
    bundle assembles and reaches sign-off but cannot be accepted without a review.

    Two wordings (#369), split on the error log — the engine's failed-leaf
    discriminator (#138: a reviewer that ran and FAILED wrote
    ``state.REVIEW_ERROR_LOG``; a successful run removed any stale one). Without the
    split, a reviewer that NEVER RAN (the beat died between the gate write and the
    leaf) read exactly like one that ran and failed, and the record could not
    distinguish "not yet run" from "ran and yielded nothing".
    """
    if (d / state.REVIEW_ERROR_LOG).exists():
        return (
            "# Advisory review MISSING — the reviewer RAN AND FAILED\n\n"
            "- NEEDS-HUMAN — no check-review.md was produced: the reviewer leaf ran "
            f"and FAILED (see `{state.REVIEW_ERROR_LOG}` in this bundle for the "
            "captured error). Fix the cause, then re-run the Check reviewer before "
            "accepting.\n"
        )
    return (
        "# Advisory review MISSING — the reviewer NEVER RAN\n\n"
        "- NEEDS-HUMAN — no check-review.md was produced and no "
        f"`{state.REVIEW_ERROR_LOG}` exists: the reviewer leaf NEVER RAN (the Check "
        "beat was interrupted before it), it did not run-and-fail. The driver "
        "recovers a never-ran reviewer on the next `advance` (#369); if this text "
        "persists, re-run the Check reviewer before accepting.\n"
    )


class _ReviewFinding(NamedTuple):
    """One parsed NEEDS-HUMAN row, with the two facts the classifier needs about it."""

    text: str
    standing: bool      # IS the canonical constant Validation row (#293)
    tagged_impl: bool   # the reviewer marked the verdict cell `[impl]` (#332)


# The reviewer's own builder-fixability tag, in the VERDICT cell: `NEEDS-HUMAN [impl]` (#332).
# In the verdict cell rather than a new column so the table schema is unchanged — `_needs_human`
# already finds that cell by content, and every existing parser keeps working.
_VERDICT_IMPL_RE = re.compile(r"\[impl\]", re.IGNORECASE)


def _needs_human(review_text: str) -> list[_ReviewFinding]:
    """Every reviewer NEEDS-HUMAN → ``(text, standing, tagged_impl)``, ordered and deduped.

    The reviewer always emits the 5/5/1 verdict table (see leaves._REVIEW_PROMPT);
    a table row whose verdict cell is NEEDS-HUMAN becomes a §6 item (Item — Basis).
    Legacy ``- NEEDS-HUMAN — …`` bullet lines are still honoured.

    The second element says whether the item IS the canonical standing row — the one the prompt
    hard-codes every cycle. It demands an **exact** match on the row's *Item cell* against the
    5/5/1's own label, which is the only thing that identifies the template row:

    * a **legacy bullet** never qualifies — it is free prose the reviewer chose to write, so
      "Validation — fitness-to-purpose: patches the wrong layer" is a real objection.
    * nor does a row in some **other table** the reviewer happened to add (a "concerns" table),
      for the same reason. Keying on "came from a table" was still too wide, and keying on the
      text's *prefix* let a real objection wear the template's clothes (PR #294 review).

    Everything else keeps its signal.
    """
    items: list[_ReviewFinding] = []
    seen: dict[str, int] = {}
    lines = review_text.splitlines()
    verdict_table = _verdict_table_lines(lines)

    def add(text: str, *, standing: bool, tagged_impl: bool = False) -> None:
        """Record a finding, MERGING a duplicate's metadata rather than discarding it.

        Dropping the later row outright made the HUMAN-over-IMPL resolution downstream
        unreachable for the one shape that needs it: the same table row repeated with
        conflicting verdicts (`NEEDS-HUMAN [impl]` then plain `NEEDS-HUMAN`) has identical
        Item and Basis cells, so the second was dropped here and the first's `[impl]` won on
        ORDER alone (PR #168 review round 4).

        Merging keeps the safer reading of each flag: untagged beats `[impl]` (it routes to
        the human and never triggers a rebuild by itself), and non-standing beats standing (a
        row the reviewer wrote about twice is not the signal-free constant).
        """
        text = text.strip()
        if not text:
            return
        key = text.lower()
        if key not in seen:
            seen[key] = len(items)
            items.append(_ReviewFinding(text, standing, tagged_impl))
            return
        prev = items[seen[key]]
        items[seen[key]] = _ReviewFinding(
            prev.text, prev.standing and standing, prev.tagged_impl and tagged_impl)

    for i, line in enumerate(lines):
        s = line.strip()
        if s.startswith("- NEEDS-HUMAN"):
            # A bullet's `[impl]` rides in the TEXT and is read by `_classify_finding`'s
            # marker strip — the advisory contract, unchanged.
            add(s[len("- NEEDS-HUMAN"):].lstrip(" —:-").strip(), standing=False)
        elif s.startswith("|") and "needs-human" in s.lower():
            cells = [c.strip() for c in s.strip("|").split("|")]
            vi = next((j for j, c in enumerate(cells) if "needs-human" in c.lower()), None)
            if vi is None:
                continue
            label = cells[0] if cells else ""
            basis = cells[vi + 1] if vi + 1 < len(cells) else ""
            add(f"{label} — {basis}" if basis else label,
                standing=(i in verdict_table
                          and _normalized_item_label(label).casefold() == _V_LABEL.casefold()),
                tagged_impl=bool(_VERDICT_IMPL_RE.search(cells[vi])))

    # FAIL CLOSED on ambiguity. The template row is a CONSTANT — it occurs exactly once. If two
    # survive (a second verdict-shaped table, a duplicated row), at least one of them is not the
    # constant, and we cannot tell which. Grant STANDING to neither, so the bundle halts for the
    # human rather than risk archiving a real objection.
    if sum(1 for it in items if it.standing) > 1:
        return [it._replace(standing=False) for it in items]
    return items


def _verdict_table_lines(lines: list[str]) -> set[int]:
    """Line indices belonging to the reviewer's MANDATED 5/5/1 verdict table.

    The whole basis for STANDING is that *that* table's Validation row is a constant the prompt
    emits every cycle. So the parser has to know which table a row came from — and it did not.
    Matching the Item cell alone let a "## Concerns" table carrying the **exact** canonical label
    earn the exemption, and an unattended rebuild would archive that real objection (PR #294
    review, local pass). Keying on the row was the fourth scoping of this same rule; the table is
    what the justification was always about.

    A contiguous run of ``|``-rows is the verdict table when **two or more** of its Item cells
    exactly match canonical 5/5/1 labels — the mandated table carries all eleven, while an
    ad-hoc concerns table carries its own prose. Two, not one, so a lone Validation row in a
    stray table cannot nominate itself.
    """
    out: set[int] = set()
    i = 0
    while i < len(lines):
        if not lines[i].strip().startswith("|"):
            i += 1
            continue
        j = i
        while j < len(lines) and lines[j].strip().startswith("|"):
            j += 1
        block = range(i, j)
        # Normalized like the STANDING test itself (#332): the prompt invites a `C4 — ` style
        # element prefix on every Item cell, so a table written that way carries ZERO cells
        # that match exactly and would not be recognised as the mandated table at all — which
        # then denies its Validation row the exemption for a second, independent reason.
        labels = {_normalized_item_label(lines[k].strip().strip("|").split("|")[0]).casefold()
                  for k in block}
        if len(labels & _CANONICAL_LABELS) >= 2:
            out.update(block)
        i = j
    return out


def _declared_external_deps(build_notes_text: str) -> list[str]:
    """Builder-declared external dependencies (#250) → §6 items.

    ``build-notes.md`` is withheld from the reviewer (the independence contract) and is not
    otherwise read into ``SUMMARY.md``, so an external dependency Do hit that Plan didn't
    list — and that no gate happens to cover (a stub or unrelated-gate config) — would never
    reach the human. The builder marks each with a line
    ``NEEDS-HUMAN external dependency: <dep> — <what it blocks>`` (see agents/builder.md);
    this lifts them into §6 deterministically, independent of the reviewer and the gate set.
    Match is bullet- and case-insensitive; the remainder after the marker becomes the item.
    """
    items: list[str] = []
    seen: set[str] = set()
    for line in build_notes_text.splitlines():
        s = line.strip().lstrip("-*").strip()
        low = s.lower()
        if low.startswith("needs-human") and "external dependency" in low:
            item = s[len("needs-human"):].lstrip(" —:-").strip()
            if item and item.lower() not in seen:
                seen.add(item.lower())
                items.append(item)
    return items


def _unregistered_dependency_items(brief_path: Path, cfg: Config) -> list[str]:
    """The Check-time BACKSTOP for #263, delegating to the one implementation (#333).

    #333 moved the primary check to Plan exit, before Do dispatches. This stays, and is
    not redundant: ``pdca.toml`` can gain or lose rows mid-cycle, which is exactly why the
    reconciliation reads the file as it stands now rather than from the run's opening
    snapshot (PR #269 review). A row deleted after Plan passed is still caught here.
    """
    return doctor.unregistered_dependencies(brief_path, cfg)


def _needs_human_block(items: list[str]) -> str:
    if not items:
        return "- (none — every model-attempted item came back PASS, no always-human item applied)"
    return "\n".join(f"- [ ] {it}" for it in items)
