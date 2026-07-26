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

## 2. Do not invent an artifact

You write only what your role names. `AGENTS.md` § Boundaries is explicit about this, and
`agents/signoff.md` states its list three times over ("write exactly three things, nothing
else … That is the complete list").

An earlier draft of this command asked `planner` and `signoff` for a `handoff-notes.md`
carrying decisions-and-why, considered-and-rejected, and open questions — the context that
dies with the session, since `driver._carry_forward_into_brief` runs after you are gone and
can only recover what was already an artifact. That is a real gap, but closing it here would
have meant widening a deliberately tight write boundary for a file nothing reads.

**So it is deferred to eduralph/pdca-harness#331 item 3**, where the *consuming* half is
built: the driver merges the session's contribution into the brief's carry-forward. Registering
the artifact and consuming it belong in the same change. Until then, put anything the next beat
must know into the channels your role already owns — the brief's fields for `planner`, the
`signoff-decision` rationale lines and §10 Act candidates for `signoff`.

## 3. Run the gate

```bash
scripts/handoff-check --leaf <your leaf> $ARGUMENTS   # ids required
```

For `act`, name the entry you appended so the gate can tell it from an earlier run's:

```bash
scripts/handoff-check --leaf act --entry "<the heading you wrote>"
```

**You must name the bundle ids you worked** — the bundle leaves refuse a bare invocation. A
scan cannot tell which bundles are this session's output, and it was wrong in both directions:
it exempted a brief you just wrote from the checks that apply only to your own work, and it
passed on somebody else's valid artifact even when you wrote nothing.

For a publisher handoff in pending-id mode (as `pdca publish --no-issue`), add `--no-issue`
so the lint does not demand a tracker reference the downstream path explicitly permits
omitting.

It is deterministic — file existence and set membership, reusing the driver's own predicates
so it cannot disagree with the harness about what a valid artifact is. With an id it requires
the artifacts to be present; with no id it scans and reports only *malformed* ones, because a
planner legitimately briefs some issues and not others.

**On FAIL:** fix what is genuinely yours to fix and re-run. Some failures are not yours —
`§9 Outcome is already set` means a decision the driver owns was authored by a leaf, and the
fix is to remove it, not to work around the check. If a failure needs the human, say so
plainly and stop; do not exit on a red gate and leave it to be found three beats later.

**On PASS:** nothing is written. The gate's verdict is its exit status and its report — it writes no artifact into any bundle, because only your role's named artifacts belong there.

## 4. Hand back

Report, in this order: the gate verdict, the artifacts you wrote, and anything the human must
pick up — including anything you reasoned about that no artifact carries, since that is what
the deferred carry-forward (step 2) would otherwise have caught. Then say the session is ready
to close.

**You cannot end the session yourself** — a slash command is a prompt expansion, not a
process signal. Ask the human to press Ctrl-D (or `/exit`), which is what releases the
driver. Do not attempt to kill the process to simulate it.
