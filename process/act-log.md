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

# Act review — 2026-06-23 — follow-up filing (no new cycles)

> Not a cycle review — this entry closes the audit loop the three prior reviews
> (2026-06-21, 2026-06-22, 2026-06-22 cont.) left open: every routed follow-up was to
> "appear as a tracker id (or an explicit 'still deferred') at the next review." All seven
> were unfiled. Each was re-reviewed with the human and the code-bug claims re-verified
> against `getwyrd/wyrd` `origin/main` @ `82be6ae` before filing (verify-don't-recall).
> No contribution disposition is re-decided.

## Follow-ups resolved (now filed)

- **Stale/unreadable `$PDCA_TARGET` drifts reviewer grounding** (from 2026-06-22 cont.) —
  filed **eduralph/pdca-harness#120** (upstream; the deterministic pin/fetch-to-`origin/main`
  fix that the downstream agent-skill caveat only backstops).
- **Tier-1 / Tier-2 fault testing must be implemented** (from 2026-06-22 cont.) — re-verified:
  not built on `main` (no `WYRD_TIER1`/`WYRD_TIER2` runner; #146/M3.8 landed only Tier-0 DST),
  so this is net-new *implementation*, not "remove inert scaffolding." Filed as two issues,
  each with an off-Check harness-code leg + a privileged-CI leg: Tier-1 **getwyrd/wyrd#195**
  (M3.9, dm-flakey/dm-error + Jepsen), Tier-2 **getwyrd/wyrd#196** (M3.10, kill-and-reconstruct).
- **#144 reconstruction telemetry** (from 2026-06-22) — filed **getwyrd/wyrd#197**. Re-phrased
  from the log's wording after verification: `emit_repaired` fires upfront for every plan
  (`reconstruction.rs:163-172`), and the `Aborted` arm is offset by nothing, so the module's own
  `successes = repaired − conflict` identity (`:432-433`) over-counts by the Aborted count;
  `time_to_repair` is an absolute instant (`:436`), not elapsed.
- **#143 read-path `chunk_id` recheck asymmetry** (from 2026-06-22, scope was "contested") —
  filed **getwyrd/wyrd#198** as a bug. Verification removed the contest: `repair.rs:50-51`
  documents the verify as *shared with the read path*, but `read.rs:138`/`:176` check only the
  checksum — a misplaced-but-intact fragment is silently decoded. The module's own stated
  invariant is violated.
- **#144 crash-safety coverage gap** (from 2026-06-22, was an open Act item) — filed
  **getwyrd/wyrd#199** as a tracked M3.6 test-debt item (crash between fragment writes and the
  CAS commit; reader concurrent with the commit window).
- **Reviewer Basis must state context + impact, not re-derive the diff** (from 2026-06-21) —
  filed **eduralph/pdca-harness#121** (upstream; generic to every rendered reviewer instance).
- **#117 post-merge throughput/scaling measurement** (from 2026-06-21) — filed
  **getwyrd/wyrd#200** so the deferred real-hardware measurement stays auditable.

## Still open (not in this session's scope)

- Two frozen bundles have not yet had an Act review: **issue_115** (ACCEPTED, 2026-06-20) and
  **issue_153** (discontinued / handed off, 2026-06-21). → next Act review.

## How effectiveness will be judged

- The prior reviews' watch-items stand unchanged; the seven follow-ups are now auditable as
  tracker ids — the next review should see them progressed/closed, not silently dropped.

# Act review — 2026-06-22 (cont.) — cycles considered: issue_145, issue_146

> These two froze after the earlier 2026-06-22 review (which covered #139–#144). This
> entry considers only the newly-frozen pair; it does not re-open the earlier entry's
> deltas or follow-ups.

## What the cycles' records exposed

- **The review/verify TARGET drifts from the base the gates actually ran against — a stale
  or unreadable `$PDCA_TARGET` made the reviewer fabricate / lose grounding (2×: #145, #146).**
  In #145 the reviewer grounded citations on a **stale local `../wyrd`** checkout (`aaee133`,
  pre-#144) and produced a *false-blocking* C4 headline — "patch cannot apply/compile against
  the target" (`SUMMARY.md:40-66`) — yet the patch applies cleanly to `origin/main` (`41c8165`,
  which carries the declared `Depends on: 144`), and the gates (C4-ci, C4-verify) ran green off
  that base in `$PDCA_WORKTREE`. The reviewer's own §10 self-nominates the fix: *"Pin the review
  target to `origin/main` (or fetch first) so a stale checkout can't fabricate ordering-gate
  blockers."* In #146 the same surface failed the other way: `$PDCA_TARGET` "was not readable
  in this environment (`env`/`printenv` were denied)", so the reviewer correctly grounded on
  `patch.diff` alone (`SUMMARY.md:42-43`) — no fabrication, but no target grounding either.
  Root cause is deterministic-harness setup: `$PDCA_TARGET` is resolved to the human's sibling
  `../wyrd` checkout (INTEGRATION.md §2), which can lag `origin/main` (the base the worktree +
  `../wyrd-verify` actually use) or be sandbox-unreadable. Two failure modes, one cause.
- **A "deferred (off-Check) posture" label let an ABSENT deliverable pass (#146).** The prior
  review's `Verification posture` field (added 2026-06-21) converts a surprise C2/C4 into a
  pre-declared sign-off item — working as intended for #146's Tier-0 born-at-tier suite. But
  #146's §10 exposes the flip side: Tier-1 (dm-flakey/dm-error + Jepsen) and Tier-2 (single-node
  kill-reconstruct) landed as **inert dispatch scaffolding only** (`xtask/src/faults.rs` runners
  exit unless `WYRD_TIER1`/`WYRD_TIER2` is set, `patch.diff:1289-1292`, `:1429-1440`), yet the
  deferred-posture label carried them through Check as if merely unverifiable-here. As the
  bundle's own §10 puts it: *"no forcing function distinguishes 'can't be verified here' from
  'isn't built'."* That is a gap in the field the prior review added — `deferred` was readable as
  *unbuilt*, not only *built-but-off-Check*.
- **C5 / T5 / V remain NEEDS-HUMAN by design (both cycles).** Always-human (INTEGRATION.md §4);
  **no delta warranted** — consistent with both prior reviews.

## Process deltas

- Spec template: **`Verification posture` tightened with a deferred-≠-unbuilt forcing function**
  — a deferred/off-Check posture is ONLY for code that EXISTS but can't be verified here; it must
  not wave through an unbuilt deliverable. Plan must state what IS built and exercised at Check vs.
  what is deferred, and confirm the deferred deliverable is itself built and exercised by something
  at Check (e.g. unit tests over the harness code), never inert dispatch scaffolding; a
  not-yet-implemented tier/job is a SEPARATE work item, not a deferred-verification line. Directly
  actions #146's §10 "no forcing function" gap.   (`templates/brief.md.tpl` — `Verification posture`)
- Agent skills: **stale-/unreadable-target grounding caveat** — a SET-but-stale `$PDCA_TARGET`
  (its base lacks a declared `Depends on` the worktree/gates already ran against, off `origin/main`)
  is a target-state caveat, not a patch defect; the reviewer notes the staleness and grounds the
  affected citations on `patch.diff`, and must NOT present a stale-/unreadable-target "patch cannot
  apply/compile" as a blocking C4 FAIL. Backstops the harness fix below; actions #145's §10.
  (`AGENTS.md` "What you do"; `.claude/agents/reviewer.md` "What you do")

## Follow-ups routed (not process deltas — work handed to an owner)

- Harness/driver issue (root cause of #145/#146, upstream): the deterministic fix is to pin/fetch
  `$PDCA_TARGET` to the SAME base the gates run against (`origin/main`) before the reviewer leaf
  runs — so a stale or unreadable sibling checkout cannot drift the reviewer's grounding from the
  worktree. This is harness machinery (`src/pdca_harness/**` worktree/target resolution), so it
  routes **upstream to the template** the harness is rendered from (the agent-skill delta above is
  only the backstop). → owner: Eduard; next step: open a harness/template-feedback issue when
  bumping the harness; record the id next review.
- Work/design item (#146 Tier-1/Tier-2 split): the two higher tiers are not functionally
  implemented — only inert scaffolding landed. Each must be split into (a) an **off-Check
  harness-code** item (Tier-1 needs root + device-mapper / Jepsen harness; Tier-2 needs a real
  node — NVMe/fsync, docker — and ADR-0016 keeps `xtask ci` unprivileged) and (b) a **privileged
  CI job** that runs it green; do not fold them into a deterministic-worktree slice with a single
  C4 DoD. Not a process delta — scheduling/scoping work. → owner: Eduard; next step: file as new
  M3 implementation issues on getwyrd/wyrd; record ids next review.

## How effectiveness will be judged

- The next cross-cycle / dependency-ordered slices (a fix that `Depends on` an unmerged sibling)
  should NOT produce a "patch cannot apply/compile against target" C4 NEEDS-HUMAN that turns out
  to be a stale-checkout artifact. Watch the next ~5 dependency-chained cycles; if it recurs after
  the agent-skill delta, the harness pin (routed above) is overdue.
- The next deferred-posture brief should name what is built-and-exercised-here vs. deferred, so an
  unbuilt tier surfaces as its own work item — not a green-by-deferral §6 row. Watch the next
  off-Check / multi-tier cycles.
- The routed harness issue + the #146 tier-split issues should appear as tracker ids (or an
  explicit "still deferred") at the next review — follow-ups stay auditable.

# Act review — 2026-06-22 — cycles considered: issue_139, issue_140, issue_141, issue_142, issue_143, issue_144

## What the cycles' records exposed

- **Seam built ahead of its production consumer — the "production-wired" claim is honoured
  only by a test double / hand-authored fixture / in-process stand-in (3+ of 6: #139, #141,
  #142, #144).** These M3.x (proposal-0005) foundation slices build a seam, but the live
  path still collapses to the old behaviour while the BINDING criterion is met only off the
  production path: #139 — production record is inert (`WritePlan::chunk_refs` hardcodes the
  identity vector, read routes `index % n`, `get_fragment_at` defaults to ignore `dserver`);
  the *only* honouring consumer is the `Fleet` test double, and Property 2 reads a
  *hand-authored* record bypassing the write path (§6 C5/T5). #141 — its own §10 candidate:
  criterion (3) "production write wired" accepted at the **library/test level only**;
  live-CLI placement deferred because "a discovery-driven gateway write must exist first" —
  and it explicitly asks Act to *"re-scope future 'production wiring' criteria to a reachable
  write path."* #142/#144 — in-process Option-A green; #144's repair-vs-serve seat
  "referenced but not wired" (priority only orders the drain, `patch.diff:420`). Each recurs
  as a *surprise* C5/T5 NEEDS-HUMAN ("is a seam whose sole honouring consumer is a test
  double causally sufficient?"). This is a **different axis** from the existing `Verification
  posture` field: there the worry is red/green *observability* at Check; here the test is
  green — the open question is whether *production* reaches the seam at all. The brief had no
  slot for Plan to pre-declare consumer reach, so it kept surfacing as adjudication.
- **The prior review's `Verification posture` delta is taking effect — no new delta, record
  as evidence.** #142's brief pre-declared its net-new born-at-tier red + in-process green as
  "a pre-agreed sign-off item, not a NEEDS-HUMAN surprise" (`brief.md:143-145`) — exactly the
  intended conversion of a surprise C2 into a declared sign-off item. (#141, also 06-21, still
  raised C2 as a net-new surprise — its brief predates the field.) Keep watching.
- **Validation fitness-to-purpose is NEEDS-HUMAN in all 6 cycles — working as designed.**
  Always-human (INTEGRATION.md §4); **no delta warranted**, consistent with 2026-06-21.

## Process deltas

- Spec template: **new `Production reach` field** — Plan declares up-front when a slice builds
  a SEAM ahead of its production consumer, so the BINDING criterion is honoured only by a test
  double / hand-authored fixture / in-process (Option-A) stand-in while the live path still
  collapses to the old behaviour. It names (a) what honours the seam now vs. what production
  still does, (b) where the production wiring lands and what must exist first, and (c) that the
  double exercises the seam load-bearingly (not dead scaffolding) — converting the recurring
  "test-double-only seam causally sufficient?" C5/T5 question into a *pre-declared* sign-off
  item. Directly actions #141's §10 self-nomination.
  (`templates/brief.md.tpl` — new `Production reach`, after `Verification posture`)

## Follow-ups routed (not process deltas — work handed to an owner)

- Another bug (Wyrd): **#144 telemetry/accounting inaccuracies** — `time_to_repair` emits the
  absolute logical instant `now_millis`, not an elapsed window (self-declared placeholder,
  `patch.diff:704-706`); `reconstruction_repaired` over-counts the Aborted path (counted in
  repaired, subtracted by neither conflict nor anything). Real code defects, out of scope to
  fix here. → owner: Eduard; next step: file against getwyrd/wyrd Issues (`Fixes #` cross-link
  per INTEGRATION.md §1); record id next review.
- Another bug (Wyrd, contested in-scope): **#143 read-path header-recheck asymmetry** — scrub
  verifies checksum AND `header.chunk_id == chunk` (`patch.diff:203-205`) but the read path's
  inline decode re-checks only checksum, so a misplaced-but-intact fragment is silently fed to
  the decoder on read. Whether in-scope is contested (binding leg 4 names only checksum). →
  owner: Eduard; next step: confirm scope at the next M3 read/repair slice; file against
  getwyrd/wyrd if confirmed.
- Open Act item: **#144 crash-safety coverage gap** — code is structure-correct (rebuilt
  fragments written before the single CAS commit; displaced orphaned in the same commit) but no
  test exercises a crash between the fragment writes and the commit, nor a reader concurrent
  with the commit window. Test debt, not a system change. → owner: Eduard; next step: carry as
  a coverage item for the M3.6 follow-on; revisit next review.

## How effectiveness will be judged

- The remaining proposal-0005 slices that wire seams into the live path (relocatable fan-out,
  custodian-aware routing, the deferred discovery-driven gateway write) should carry a
  `Production reach` line, so the "sole honouring consumer is a test double" question lands as a
  *pre-declared* sign-off item — not a surprise §6 C5/T5. Watch the next ~5 seam/foundation
  cycles for recurrence.
- The routed #144/#143 items should appear as tracker ids (or an explicit "still deferred")
  at the next review — the follow-ups must stay auditable, not silently drop.

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
