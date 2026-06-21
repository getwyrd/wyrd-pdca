# Act log — Wyrd PDCA

> Append-only, cross-cycle (docs 02 §ACT). Each entry records which frozen
> bundles an Act review considered, what their records exposed, the concrete
> process deltas applied (each located by a path / rule ID / template field), and
> how the next review will judge whether the delta worked. Act never re-decides a
> contribution's disposition. Newest entries on top.

<!-- Template for a new entry:

# Act review — <date> — cycles considered: <issue_ids>

## What the cycles' records exposed
- <pattern across one or more cycles, citing SUMMARY §6/§7/§10>

## Process deltas
- Spec template: <field added/clarified/removed>            (path)
- Ruleset: <rule added/retired/relaxed/tightened>           (path:line)
- Gates: <check added/promoted/moved>                       (path:line)
- Agent skills: <SKILL.md / AGENTS.md adjustment>           (path:line)

## Follow-ups routed (not process deltas — work handed to an owner)
- Another bug (project/component): filed <tracker> #NNNN    (link)
- Design issue: <name> → dedicated design phase, owner <who>
- Harness/driver issue: this repo's tracker | template feedback upstream  (link)
- Other open Act item: <item> → owner <who>, next step <…>

## How effectiveness will be judged
- The next Do phases should not recreate <specific issue>. Watch the next K cycles.
-->

# Act review — 2026-06-21 — cycles considered: issue_116, issue_117, issue_150, issue_151, issue_152, issue_154, issue_155

## What the cycles' records exposed

- **C2/C4 cannot be demonstrated at Check for net-new / environment-gated work
  (4 of 7: #116, #117, #150, #151).** `C2 Reproduction (red pre-fix)` is
  `result:"none"`, oracle `"(no gate configured)"` in *every* bundle's
  `check-gates.json` — never machine-checked, always a human call (by design). But these
  four recur for one *structural* reason the human keeps re-adjudicating: the work is
  **net-new coverage/infrastructure**, not a defect-to-remove, so there is no failing test
  to flip. "Red" rests on file non-existence / criterion-absence (#116 new `network.rs`;
  #117 born-at-M2 tier `tier2_integration.rs:236-245`; #151 net-new gate), and the green is
  observable only off-Check — Docker host / `WYRD_DSERVER_ENDPOINTS` / a live GitHub
  Actions PR / real hardware (#117, #150, #151) — so the shipped test is *inert* at Check
  (and #116's fault injection was flagged "not proven load-bearing", `network.rs:689`). The
  brief template assumed a defect with a flippable repro and had **no slot** to declare an
  inherently-deferred verification posture, so each cycle re-raised C2 as a *surprise*
  NEEDS-HUMAN.
- **Brief prose read as binding mechanism; builder reasonably diverges (3 of 7: #116,
  #152, #155).** The template already forbids naming a mechanism in *Scope*/*Invariant*,
  but the divergences came through *Success criterion* prose and Scope wording: #152
  "README additions only" (builder added a Rust test `readme_dev_section.rs`), #155
  "composing `Gateway` over `FanoutChunkStore<GrpcChunkStore>`" (builder deliberately
  bypassed `Gateway`, `cli.rs:448-487`), #116 named a three-property suite (one re-run,
  `network.rs:861`). Each forced a human "is this divergence acceptable" call.
- **Reviews skew implementation-heavy.** Across cycles the reviewer's per-item Basis tends
  to re-derive the diff rather than state the *context and impact* the human's sign-off
  decision turns on — making §6 NEEDS-HUMAN rows describe code instead of naming the
  decision owed. (Human observation at this review.)
- **Validation fitness-to-purpose is NEEDS-HUMAN in all 7 cycles — working as designed.**
  It is an explicit always-human item (INTEGRATION.md §4); **no delta warranted** there.

## Process deltas

- Spec template: **new `Verification posture` field** — Plan declares up-front when C2's
  red is criterion-absence vs a flippable assertion, and when the green is observable only
  off-Check; names where/who confirms the deferred green and asks Do to capture a
  *demonstrated* red where feasible.   (`templates/brief.md.tpl` — `Verification posture`,
  after `Test file`)
- Spec template: **`Success criterion` clarified** — state the BINDING observable
  condition; any named mechanism/component/API/file is marked BINDING or merely
  ILLUSTRATIVE, so Do diverging on mechanism (binding condition still holding) is a Do call,
  not a scope NEEDS-HUMAN.   (`templates/brief.md.tpl` — `Success criterion`)
- Agent skills: **reviewer Basis must state context + impact, not re-derive the
  implementation** — for NEEDS-HUMAN rows especially, name the decision owed and why it
  matters.   (`AGENTS.md` "What you do" bullet; `.claude/agents/reviewer.md` verdict-table
  note, line 62)

## Follow-ups routed (not process deltas — work handed to an owner)

- Harness/template feedback (upstream): the reviewer-Basis agent-skill delta above is
  *generic* (every rendered instance benefits, not just Wyrd). Propagate it upstream to the
  template the reviewer contract is rendered from, so it does not drift instance-only.
  → owner: Eduard; next step: open template-feedback issue when bumping the harness.
- Open Act item: **#117 §10 Q6** — throughput/scaling numbers deliberately deferred to a
  post-merge measurement on real hardware off the nightly lane. Not a system change; a
  tracked work item. → owner: Eduard; next step: file against the Wyrd tracker (per
  INTEGRATION.md §1) or carry forward; revisit next review.

## How effectiveness will be judged

- The next net-new / environment-gated cycles (Tier-2 container, DST, CI-gate work) should
  carry a `Verification posture` line so C2/C4 land as a *pre-declared* sign-off item — not
  a surprise §6 NEEDS-HUMAN. Watch the next ~5 cycles for recurrence of "inert test / red
  rests on non-existence".
- Mechanism/scope-divergence NEEDS-HUMAN (the #152/#155 shape) should drop once briefs mark
  named mechanisms BINDING vs ILLUSTRATIVE.
- §6 rows should read as decisions-owed (context + impact), not diff restatements.
