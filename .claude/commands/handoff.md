---
description: Signal this leaf is done — verify its artifact contract, capture carry-forward, end the session
argument-hint: "[issue_id]  (omit to cover every bundle this session touched)"
---

You are finishing an **interactive leaf** of the PDCA cycle. The driver is blocked on your
process exiting (`leaves._invoke` runs you with `subprocess.run(...)` and no `check=`), so
your exit is the signal that lets the harness move forward — and it discards your exit code,
which means nothing downstream checks that you actually did your job. That check is here.

Do these in order. Do not skip the gate because the work "obviously" succeeded.

## 1. Name your leaf

You are exactly one of `planner`, `signoff`, `publisher`, `act` — read it off your own role
prompt. If you genuinely cannot tell, stop and ask; guessing runs the wrong contract.

## 2. Capture what this session established

For `planner` and `signoff`, write `results/issue_<id>/handoff-notes.md` (one per bundle you
worked). This is the half that dies with the session otherwise: the driver's
`_carry_forward_into_brief` folds the §9 rationale and the failing gates into the next
attempt, but it runs *after* you are gone and can only recover what was already recorded as an
artifact. What you reasoned about and never wrote down is lost.

```markdown
# Handoff notes — issue_<id> — <leaf> — <YYYY-MM-DD>

## Decisions taken, and why
<each decision with the reason it went that way, not just the outcome>

## Considered and rejected
<the options you ruled out and what ruled them out — this is what stops the next
session re-deriving them, or worse, re-adopting them>

## Open questions
<what remains genuinely undecided, and who owns it>
```

Skip this for `publisher` (its artifacts are self-describing) and `act` (the act-log entry
*is* the record). Write nothing rather than filler — an empty section is worse than an absent
one, because it reads as "considered, nothing found".

## 3. Run the gate

```bash
scripts/handoff-check --leaf <your leaf> $ARGUMENTS
```

It is deterministic — file existence and set membership, reusing the driver's own predicates
so it cannot disagree with the harness about what a valid artifact is. With an id it requires
the artifacts to be present; with no id it scans and reports only *malformed* ones, because a
planner legitimately briefs some issues and not others.

**On FAIL:** fix what is genuinely yours to fix and re-run. Some failures are not yours —
`§9 Outcome is already set` means a decision the driver owns was authored by a leaf, and the
fix is to remove it, not to work around the check. If a failure needs the human, say so
plainly and stop; do not exit on a red gate and leave it to be found three beats later.

**On PASS:** the gate records `handoff.json` in each bundle it checked.

## 4. Hand back

Report, in this order: the gate verdict, the artifacts you wrote, and anything the human must
pick up. Then say the session is ready to close.

**You cannot end the session yourself** — a slash command is a prompt expansion, not a
process signal. Ask the human to press Ctrl-D (or `/exit`), which is what releases the
driver. Do not attempt to kill the process to simulate it.
