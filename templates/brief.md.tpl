# Brief — issue <id> / <slug>

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** <short-kebab-slug>
- **Defect:** <what is wrong — the observable problem>
- **Success criterion:** <the observable condition that means it is fixed — must be
  demonstrable by C4-verify (the patch applied in isolation at Check). Do NOT scope this
  to a T3 whole-suite pass or a fork-CI green: those only clear after the fix is merged,
  not at Check. Use them as supplementary evidence only. State the BINDING observable
  condition; if you name a specific mechanism/component/API/file ("composing X over Y",
  "README only"), mark it BINDING or merely ILLUSTRATIVE — so Do diverging on mechanism,
  while the binding condition still holds, is a Do call and NOT a scope NEEDS-HUMAN.>
- **Invariant to restore:** <the property the fix must make true, stated over the
  defect CATEGORY, not the repro file. NOT a mechanism. Cite its source (language spec /
  framework docs / internal rule) per `docs/principles.md` §3–§6. SELF-TEST: could Do
  satisfy this by guarding a single module? If yes, it's the narrow symptom-sentence —
  widen it. Omit only for non-structural behavioural bug fixes (principles.md §1.1).>
- **Repo + branch target:** <owner/repo> @ <branch>   (resolve here at Plan — do not leave to Do)
- **Onto branch:** <remote>/<existing-pr-branch>   (optional — stack the fix as a commit onto an existing open PR's branch instead of opening a new PR; the fix is tested, committed, and pushed against THIS branch; docs 03)
- **Depends on:** <id>[, <id>…]   (optional — ids only on the value line, any trailing note is ignored; batch/lane scheduling waits until these bundles are COMPLETE before this one runs; docs 09)
- **Depends on (merged):** <id>[, <id>…]   (optional — ids only on the value line, any trailing note is ignored; stricter than Depends on: hold this bundle until each prereq's PR is MERGED, not merely COMPLETE. Use when this edits files a prereq also edits, so Do builds on the merged result instead of conflicting at merge; docs 09)
- **Conflicts with:** <id>[, <id>…]   (optional — ids only on the value line, any trailing note is ignored; never co-schedule these in the same concurrent wave, e.g. they edit a shared file; docs 09)
- **Stacks on:** <id>[, <id>…]   (optional — ids only on the value line, any trailing note is ignored; build THIS on top of the prereq's just-produced branch within the SAME flow run and publish a separate stacked PR (`gh --base <prereq-branch>`), so a file-overlapping chain completes in one run; the base is derived from the prereq, not written here; docs 09)
- **Ordering note:** <optional free text — WHY the scheduling fields above are set as they are (e.g. "depends-on-merged 12 because both edit cache.py"). Not machine-parsed; it documents the human's sequencing decision next to the bare-id fields.>
- **Surfaces:** <where the change is observable — `gui` (touches the frontend / an E2E
  through the app is needed), `data` (backend/logic only), or `both`. Drives which
  runtime gates apply (e.g. an E2E gate runs only when this is `gui`). Optional.>
- **Difficulty:** <`low` | `medium` | `high` — the fix's **blast-radius / cross-file
  reach**: how many files/call-sites it touches and how far its effects propagate (what a
  diff-reviewer must hold in view), NOT edge-case density (the deterministic gates own
  that). low = a localized one-site change; high = a wide, cross-cutting change. Routes
  the Do backend and review depth (issues #133/#134). Optional; absent/unknown is the safe
  default — no review or capability is skipped on a missing tag.>
- **Scope:** <the defect to remove — one logical fix. MUST NOT name a probe/guard/helper
  (a capability check, `hasattr`, `try/except import`): naming a mechanism seats the fix
  shape for Do. Leave mechanism to Do; Do prefers removing the cause over guarding it
  (principles.md §3.1, §3.3).> / out of scope: <what is explicitly excluded>
- **Repro instruction:** <fixture + exact steps on the target branch>
- **Test file:** <path where the regression test ships — must fail pre-fix, pass post-fix>
- **Verification posture:** <how Check can actually demonstrate this. DEFAULT (omit the
  field): a flippable regression test — red pre-fix, green post-fix at Check (the `Test
  file` above). Declare a posture HERE when that default does NOT hold, so C2/C4 land as a
  pre-declared sign-off item rather than a surprise NEEDS-HUMAN: (a) NET-NEW coverage /
  infrastructure where "red" is criterion-ABSENCE (a new file / born-at-tier — no prior
  failing assertion to flip); or (b) a test INERT at Check because its green is observable
  only off-Check (a Docker host, an env var like `WYRD_DSERVER_ENDPOINTS`, a live CI / fork
  PR run, real hardware). When you declare a deferred posture: NAME where/who confirms the
  deferred green, and ask Do to capture a *demonstrated* red where feasible (a temporary
  negation/stub proving the new seam is load-bearing) rather than resting red on
  non-existence. FORCING FUNCTION (deferred ≠ unbuilt — the #146 Tier-1/2 gap): a
  deferred/off-Check posture is ONLY for code that EXISTS but cannot be verified HERE; it
  MUST NOT wave through a deliverable that is not built. When you declare it, state (i) what
  IS built AND exercised at Check (the seam/harness code, unit-tested in this slice) vs.
  what is deferred, and (ii) that the deferred deliverable is itself BUILT and exercised by
  SOMETHING at Check (e.g. unit tests over the harness code) — never merely inert dispatch
  scaffolding. If a tier/job is not yet functionally implemented, it is a SEPARATE work item,
  not a deferred-verification line.>
- **Production reach:** <OPTIONAL — declare HERE when this slice builds a SEAM ahead of its
  production consumer, so the BINDING criterion is honoured only by a test double, a
  hand-authored fixture, or an in-process (Option-A) stand-in while the LIVE path still
  collapses to the old behaviour (e.g. routing through `index % n`, an identity/default
  record, a static-endpoints bypass). State: (a) WHAT honours the seam now (the double /
  fixture / in-process loop) vs. what production still does; (b) WHERE the production wiring
  lands — which later slice, and what must exist first (e.g. "a discovery-driven gateway write
  must exist first"); (c) that the double exercises the seam LOAD-BEARINGLY (not dead
  scaffolding). This converts the recurring "is a test-double-only seam causally sufficient?"
  C5/T5 question into a PRE-DECLARED sign-off item rather than a surprise NEEDS-HUMAN. Distinct
  from `Verification posture` (that is red/green observability at Check; this is whether the
  LIVE path reaches the seam at all). Omit when production traverses the seam at Check.>
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** <searched by file path — merged history / open PRs / closed PRs — result>
- **Disposition hint:** <one triage flag — drives the driver's Do path. FIX (full
  Do+Check band): `likely-fix`, `POSSIBLY-FIXED → verify first` (needs verification, so
  NOT close). CLOSE / no-fix (FAST-PATHED — builder + reviewer leaves skipped, routed
  straight to sign-off; docs 04 §close-disposition fast path): `likely-close`, `wontfix`,
  `by-design`, `duplicate`, `not-reproducible`, `manual-verification`, `upstream` (not this
  repo's defect), `external` (not a defect in scope). `NO-NOTES` is a low-triage-signal
  flag, not an outcome. The close set is configurable per instance in `pdca.toml`
  `[driver].close_dispositions` — keep this list in step with it.>

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
