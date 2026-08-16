# Repository integration — Wyrd PDCA

> What Wyrd PDCA provides to plug into the generic PDCA cycle (see
> [quality-cycle.md](../PCDA/quality-cycle.md)). This is the project's answer to the
> "which / where / how" questions the generic model deliberately leaves open.
> It does **not** restate the cycle. Conflict rule: generic wins on cycle
> *shape*; this integration wins on *instantiation*.
>
> **Host repo:** Wyrd, the Rust workspace monorepo at the sibling checkout `../wyrd`
> (`https://github.com/getwyrd/wyrd`). This PDCA project is **out-of-tree** — it wraps
> Wyrd's existing Plan/Do/Check machinery and adds the Act beat; it never modifies
> Wyrd's tree. Wyrd's process is recorded in its ADRs (`../wyrd/docs/design/adr/`),
> `GOVERNANCE.md`, and `specs/`; those are the normative sources cited below.
> Maintained by Act (append changes; don't silently rewrite).

## 1. Tracker integration
- **System / URL:** GitHub Issues — https://github.com/getwyrd/wyrd/issues. Already
  gated host-side: Wyrd's `require-issue` xtask/CI check rejects a PR with no linked issue,
  so PDCA's "init from brief" maps onto an issue that already satisfies that rule.
- **Issue-ID format:** `#123` (GitHub integer; bare `#123` in commit/PR text).
- **Cross-link form (commit/PR → tracker):** `Fixes #nnnn` (GitHub auto-close) — this is
  `[tracker].issue_trailer = "Fixes #{id}"` in `pdca.toml`. A ticketless work item uses
  `pdca publish --no-issue` (omits the trailer; records the bundle `id_pending`).
- **Status → disposition mapping:** GitHub `open` → fixable at Plan; `closed` set on merge
  at Check. Milestone (`M2.2`, …) and labels carry the rest; no custom state machine.
- **Per-release field updated on a fix:** the issue's **milestone** (Wyrd's `Mx.y`
  milestones); closed-by-PR link records the fix.
- **Comment voice / template:** `templates/tracker-comment.md.tpl`. No tracker scraper is
  needed — GitHub issues are read with `gh issue view <id>` when full thread context helps.

## 2. Branch-target rules
- **Target checkout:** the sibling **`../wyrd`** checkout — but a cycle's Do/Check never
  mutate it directly. With `[driver].worktree` on (the default; native since
  eduralph/pdca-harness#94, v0.30.0) the driver runs Do/Check in a **per-cycle git
  worktree** off the target's base (`origin/main`), reset clean before each Do and exposed
  to the builder and gate commands as **`$PDCA_WORKTREE`** — so the human's working tree is
  never touched and concurrent lanes get private worktrees. `engine/xtask.sh` runs
  `cargo xtask ci` in `$PDCA_WORKTREE` (falling back to `../wyrd` if isolation is off), so
  the gate tests the SAME tree the builder edited; the per-fix `C4-verify` gate uses its
  own dedicated `../wyrd-verify` worktree, cut off **the bundle's resolved base** — not a
  hardcoded `origin/main` (see "How `C4-verify` resolves the base" below). `$WYRD_VERIFY_BASE`
  overrides the *base*; `$WYRD_REPO` / `$WYRD_VERIFY` override the *repo* and *worktree*
  paths. Both
  the cycle worktree and the `C4-verify` worktree (and its `pdca-verify` branch) are
  **scoped per lane** under in-driver concurrency (`-l<slot>` suffix from `$PDCA_LANE`), so
  `--lanes N` runs without two lanes colliding on a checkout or a branch — the active gate
  set (`C4-ci`, `C4-verify`) is multi-lane-safe.
- **Per-area branch map:** single-slice fixes and standalone features target **`main`**
  directly. Wyrd has **no maintenance branches** (no `maintenance/*`, no
  master-vs-maintenance split) — don't invent one. **A multi-slice milestone stacks on a
  shared integration branch** instead (the generic rule is fork-discipline.md §3).
  The worked example — **Milestone 4** (proposal 0007's PR sequence — issues #252, #253, …)
  — targeted **`feat/m4-production-metadata-backend`** (cut off `main`); each M4 slice
  branched off *that* branch and PR'd *into* it, and the integration branch merged to `main`
  in one PR when M4 completed. So an M4 bundle's brief **"Repo + branch target"** read
  `getwyrd/wyrd @ feat/m4-production-metadata-backend`, not `@ main` — that is the base the
  publisher opens the slice PR against. **M4 is COMPLETE** (merged as getwyrd/wyrd PR #489;
  the branch is deleted from `origin`), so this stands as the **pattern**, not a live
  instruction — a brief naming that branch today names a ref that no longer exists, and
  `C4-verify` would warn and fall back to `origin/main`. Add another integration branch here
  when a future milestone needs the same.
- **Wave sequencing is `wave_mode = "merge"`** (changed from `"stack"`, 2026-08-02): for a
  dependent multi-issue batch, the driver `gh pr merge`s each **non-final** wave's PRs into
  the real base (`main`) before the next wave builds; the final wave's PRs stay the human's
  to merge. `"stack"` folded waves onto the run-scoped `pdca-integration/<base>` branch and
  opened stacked PRs against it — but that branch is force-rebuilt on every fold, so any
  stacked PR still open at the next fold went stale: its old `pdca-integrate:*` commits fell
  out of the base's ancestry (wyrd's `dco` gate red on harness commits) and a later
  overlapping bundle turned it CONFLICTING against its own already-folded content (wyrd
  #675/#676, 2026-08-01, batch 648–650). Under `"merge"` no rebuilt-each-run branch ever
  serves as a PR base. This is distinct from the M4 pattern above: a *milestone's* durable
  `feat/*` integration branch remains the right shape for a planned PR sequence; `"merge"`
  governs the driver's own wave stacking within one batch run.
- **But `auto_merge = false`** (2026-08-08): merge mode's *bases* are what this instance
  wants — real `main`, no rebuilt-each-run branch under an open PR — not its merging. With
  it off the driver readies and merges **nothing**: every accepted bundle stays the draft PR
  publish opened, and a multi-wave batch STOPs after wave 0 with a message telling you to
  merge that wave yourself and re-run (`compute_waves` levels by longest path, so wave *k*+1
  always has a prerequisite in wave *k* — continuing would build it against a base that
  prerequisite never reached). What forced this: on 2026-08-08 the driver merged wyrd #703
  six seconds after opening it, before the required `gate` context had reported, so `gh pr
  merge` refused it and the wave stopped with #703 readied and #704–706 untouched
  (getwyrd/pdca-harness#462). Turn back on once #462's `--auto`/poll fix lands upstream. A
  single-wave batch is unaffected either way — the final wave never merges.
  Three consequences of the stop, all from the #462 review:
  - **The re-run verifies your merge; it does not assume it.** With this off, `_runnable`
    gates *every* declared `Depends on` on `merged.is_merged`, not just the stricter
    `Depends on (merged)`. Nothing else can move a base here — the driver merges nothing,
    and the wave fold is the `stack` path — so a prerequisite that is COMPLETE but unmerged
    holds its dependent back, and the next run says `prerequisite(s) not ready` rather than
    quietly rebuilding the very mistake the stop prevented.
  - **A wave with nothing to merge does not stop.** A close / no-fix bundle carries no patch
    and gets no PR, so no base has to move; the run continues to the next wave in the same
    invocation instead of telling you to merge PRs that do not exist.
  - **Act is deferred past any stop.** Act reviews a *finished* batch, so a run that STOPs at
    a boundary defers it to the invocation that reaches the final wave. Otherwise a
    multi-wave batch would fire Act once per resume rather than once per batch.
- **How `C4-verify` resolves the base** (getwyrd/wyrd-pdca#91 is **closed** — the old
  "validates against a hardcoded `origin/main`" caveat no longer applies).
  `engine/scripts/run-verify.sh` (`_resolve_base_ref`) takes the first of:
  1. **`$WYRD_VERIFY_BASE`** — explicit override, used **verbatim**, so pass the full ref
     (`origin/<branch>`);
  2. the **brief's "Repo + branch target"** base, prefixed `origin/` — parsed exactly as
     `publish._clean_ref` does, so the gate validates against the SAME base the PR is opened
     against (a stacked slice therefore validates against its own integration branch);
  3. **`origin/main`**.

  A named base that does not exist on `origin` warns and falls back to `origin/main`.
  *Remaining gap:* a **wave fold** builds the next wave on a driver-generated
  `pdca-integration/<base>` branch that **no brief names**, and a bundle-scoped gate is never
  told about it — so a wave≥1 dependent is still verified against `origin/<brief base>`.
  That is a different problem from #91 and is tracked upstream as
  **eduralph/pdca-harness#273**; the `$WYRD_VERIFY_BASE` slot above is what a fix would
  feed. **Mooted here since `wave_mode = "merge"`** (2026-08-02, see "Wave sequencing"
  above): a wave≥1 bundle now builds on a genuinely merged `origin/<brief base>`, so the
  ref C4-verify resolves IS the base the PR opens against. The gap stays live upstream for
  `"stack"`-mode instances.
- **Override convention:** a maintainer's explicit base-branch request on the PR wins
  (per `GOVERNANCE.md` decision-making); otherwise `main`.
- **Cross-version cherry-pick rules:** none today (single line). If back-porting starts,
  cherry-pick is a **correctness** check — "applies cleanly" ≠ "remains correct"; verify
  against the target branch's related code, including files the patch doesn't touch.
- **Immutability rule (host-enforced):** Wyrd's `adr-immutability` gate forbids editing an
  Accepted ADR (`../wyrd/docs/design/adr/`, ADR-0001). A Plan that needs to change an
  accepted decision authors a **new** superseding ADR — never edits the old one.
- **Plan foundation — the design corpus, referenced in place (never copied):** when
  briefing, ground the plan not only on the tracker (the issue source) but on the target
  repo's existing **design corpus**, read in place under the `../wyrd` checkout with the
  safe `git -C ../wyrd …` / Read / Grep idiom — **never** copied into the bundle. That
  corpus is `../wyrd/docs/design/`: the ADRs (`adr/`), proposals (`proposals/`),
  architecture notes (`architecture/`), and normative specs (`specs/`), indexed by
  `docs/design/README.md`; brief against it alongside the code you cite for root cause. It
  anchors a brief in the accepted decisions and strengthens the prior-art / superseding
  check (the immutability rule above). These are **reference foundation, not bundle
  artifacts** — do not wire them as copy-based `[[plan.source]]` providers; the planner
  reads them live from `../wyrd`, so nothing is duplicated into `results/`.

## 3. Reproduction fixtures and runners
- **Canonical fixture path:** the on-disk-format **conformance vectors** at
  `../wyrd/docs/design/specs/conformance/vectors/v1/` (valid) and `.../invalid/v1/`
  (malformed), each a `.fragment` + its `.expected.json` / `.reason.txt` oracle (ADR-0002).
- **Verification runner (the whole gate):** **`cargo xtask ci`**, delegated via
  `./engine/xtask.sh` (§9). It runs the same checks on a laptop and in CI (ADR-0016): the
  prose gates (`typos`, docs lint/render — wyrd#599) first, then fmt (`--check`), clippy
  (`-D warnings`), build, test (incl. DST property tests), `cargo deny check`, and the
  conformance run. Exit 0 = pass. This is Wyrd's single source of gate truth. **One
  laptop/CI asymmetry to know:** the prose gates need external tools (`typos-cli`, the
  pinned doc renderer); when those are absent locally the gate **warns and skips** them,
  whereas CI has them and runs them — so a local green is not full CI parity unless
  those tools are installed.
- **Reproduction runner(s):** Wyrd's DST is the repro substrate (ADR-0009) — a failing
  **seed** under madsim is the deterministic reproduction; `cargo xtask dst` sweeps seeds,
  and a bug-finding seed becomes a permanent regression test. `cargo xtask conformance`
  re-checks the format vectors. Neither is containerized (containers break seed determinism).
- **Platform variants:** pure-Rust workspace; Linux CI is the matrix. No OS-specific runners.
- **What counts as a successful repro:** the regression test (DST seed or conformance
  vector) is **red before** the fix and **green after**; `cargo xtask ci` exits 0.

## 4. Conformance ruleset (answers the validation-tooling matrix for this repo)
Wyrd does **not** use a doc16-style T1/T2/T3/T4 ladder; its gates are single-sourced in
`cargo xtask` (ADR-0016) and rolled into `ci`. The table maps PDCA's tier slots onto what
Wyrd actually enforces — every command is a `cargo xtask` delegation, none re-declared here.

Gating policy: the whole-tree `cargo xtask ci` is the one **gating** check (`C4-ci`). The
finer per-tier rows below are the same checks `ci` already runs, listed for *auditability*;
ship them advisory (and commented in `pdca.toml`) so they don't double-run.

Not every registered check fits this table. The **per-fix** C4 rows — `C4-verify` (red→green)
and `C4-diff-cov` (diff coverage) — are engine-owned scripts under `engine/scripts/`, not
`cargo xtask` delegations, because their subject is one bundle's `patch.diff` rather than the
tree Wyrd's own gate audits. They are listed in §9 with the other repo-specific tooling, and
declared with the rest of the executable ruleset in `pdca.toml` `[gates] checks`.

| Tier | Written ruleset (normative source) | Home | Single-sourced command | Status |
|---|---|---|---|---|
| C4 correctness | the change + Wyrd's whole gate (wyrd#598 moved the **prose gates** — `typos` + docs lint/render, previously host-CI-only and the one gate class published PRs kept failing — into `cargo xtask ci`; **merged in wyrd PR#599 on 2026-07-18**, so `C4-ci` now inherits them — **conditionally closed**: the prose gates warn-and-skip when their tools (`typos-cli`, the pinned doc renderer) are absent on the PDCA host, so `C4-ci` closes the blind spot only where those tools are installed; otherwise it goes locally green and the host CI can still open the PR red on these (see §3)) | `cargo xtask` (`../wyrd/xtask/`) | `./engine/xtask.sh ci` (delegates `cargo xtask ci`) | [built — **gating**, scope=repo] |
| T1 format-conformance | chunk-format spec v1, RFC-2119 (`../wyrd/docs/design/specs/chunk-format/v1.md`); conformance spec `specs/conformance/v1.md` (ADR-0002) | `cargo xtask conformance` (vectors in `specs/conformance/`) | `./engine/xtask.sh conformance` | [built — runs inside `ci`; advisory row optional] |
| T2 shape | `rustfmt` + `clippy -D warnings` (no project style doc; the linters are the rule) | inside `cargo xtask ci` | (subsumed by `ci`) | [built — part of `ci`] |
| T3 runtime / DST | testing strategy, ADR-0009 (madsim DST is the spine; from M0) | `cargo xtask dst` + `test` | `./engine/xtask.sh dst` | [built — runs inside `ci`; advisory row optional] |
| T4 contribution | ADR-0003 §1 (DCO), `require-issue`, `adr-immutability`; commit/PR conventions (§8) | `cargo xtask` gates + GitHub CI | `./engine/xtask.sh ci` (re-gate) + host CI | [built — host-enforced] |
| T5 judgment | reviewer contract below | Check reviewer + sign-off | (model) | [planned] |

- **Reviewer family (cross-vendor, ≠ builder):** codex — canonical role body
  `agents/reviewer.md`, inlined for a codex reviewer / resolved via `--agent reviewer` for a
  claude one (`.claude/agents/reviewer.md` renders only when the reviewer family is claude).
  `AGENTS.md` now carries general codex **project context** (STOP discipline, boundaries),
  not the reviewer role.
- **Builder family:** claude — canonical role body `agents/builder.md`; the
  Claude wrapper `.claude/agents/builder.md` (with the ready-mark block enforced by the
  `.claude/hooks/builder_guard.py` PreToolUse hook) is materialized only when the builder
  family is claude. A codex builder runs `codex exec --sandbox workspace-write`, confined to
  the worktree cwd.
- **Interactive family:** claude — the human-in-the-loop leaves (Plan,
  Sign-off, Publish, Act) run a seeded `claude --agent <name>` REPL or a `codex` TUI, chosen
  by `interactive_family`. A codex publisher gets the same `gh` STOP-shim the builder does
  (it has no PreToolUse hook), so it can't `gh pr ready`/`merge`.
- **Role prompts (vendor-neutral source):** each leaf's instructions live once in
  `agents/<name>.md`. Claude leaves also get `.claude/agents/<name>.md` (frontmatter wrapper
  that includes that body, so `--agent` resolves); non-Claude (inline) leaves read the
  `agents/` body directly. Only Claude leaves carry a `.claude/agents/` file.
- **Vendor profiles:** every family-specific behavior (streaming, extra-dir grounding,
  role-prompt injection, STOP-guard mechanism) is data in `pdca_harness.families`,
  overridable via `pdca.toml [families.<name>]` — swapping or adding a vendor is a
  config edit, not a driver change. A non-claude builder gets the STOP discipline
  from the driver's `gh` PATH shim (same `builder_guard.py` rules as the claude hook).
- **Project-defined human-only items** (reviewer emits NEEDS-HUMAN by design): any **ADR /
  spec / proposal** change (architecture-board / founding-maintainer authority per
  GOVERNANCE, not a model's to accept); any change to the **normative on-disk format**
  (ADR-0002); any **new dependency or license** (the ADR-0003 three-test audit + `deny.toml`
  allowlist); fitness-to-purpose ("is this the right thing at all").

## 5. Upstream-isn't-ahead routine
- **What "upstream" is:** **N/A** — Wyrd is the canonical upstream, not a fork. There is no
  prior-art-in-upstream search step. (Keep this section as an explicit "none" so a future
  fork relationship is a deliberate addition, not a silent gap.)

## 6. Brief and design-proposal templates
- **Brief template:** `templates/brief.md.tpl`.
- **Plan reference (the Plan beat's artifact):** Wyrd's Plan is **a set of existing
  artifacts**, not a new document — the issue's linked **ADR** (`../wyrd/docs/design/adr/`),
  **proposal** (`../wyrd/docs/design/proposals/`), or **spec** (`../wyrd/docs/design/specs/`).
  PDCA's Plan step *points at* the relevant one (`templates/plan-pointer.md.tpl`); it does
  not impose its own format.
- **Design-proposal template:** `templates/design-proposal.md.tpl` — reserved for the
  exception (major architecture / format / API). Most work points at an existing ADR or
  proposal. An accepted ADR is immutable (§2); a change to one is a *new* superseding ADR.
- **Required project-specific frontmatter/sections:** the linked ADR/proposal/spec/issue.

## 7. Bundle and act-log paths
- **Bundle root + ID format:** `results/issue_<id>/`.
- **Act log path:** `process/act-log.md` — **this project owns the Act beat** (the one
  PDCA beat Wyrd lacked natively; no ADR/act-log is added to Wyrd's tree).
- **Iterate archive:** a rejected attempt is preserved in `iteration-v<N>/` in the bundle.

## 8. Committing and PR conventions
- **DCO sign-off:** `git commit -s` (Developer Certificate of Origin, ADR-0003 §1 — Wyrd
  uses DCO, not a CLA). Already aligned with the harness's `DCO` file.
- **Commit-message format:** concise subject; body explains the *why*. No model/tool
  attribution trailers of any kind — commits, PRs, comments, and reviews carry the
  human contributor's identity only (maintainer's standing rule).
- **PR description format:** Root cause / Fix / Verified against / Test
  (`templates/pr-description.md.tpl`); reference the issue with `Fixes #nnnn`.
- **Enforcement mechanism:** host-side — `dco`, `require-issue`, and `adr-immutability`
  xtask/CI gates + maintainer review (GOVERNANCE). The builder/publisher STOP hook
  (`builder_guard.py`) is a backstop, not the authority.
- **Definition of done for a PR (review protocol):** host gates green **plus one
  batched, fully triaged review pass** — the `T4-batch-review` gate row
  (`scripts/review-branch`, 3 parallel rubric-armed codex passes, unioned) with every
  finding either fixed or rejected with a recorded reason. Do **not** retrigger
  `@codex review` chasing reviewer silence: the external reviewer surfaces ~1 new
  pre-existing finding per re-poke (measured across 158 findings / ~79 PRs), so serial
  retriggering converges slowly by construction — batch the depth up front instead.
  A finding deferred with "Deferred — tracked in #N" is settled (per the target
  AGENTS.md § Review rubric & protocol). When external findings do arrive on a
  published PR, process them with **`pdca triage <pr>`** so every one lands in the Act
  ledger (bug → tracker/carry-forward; convention → gate or rubric delta; noise →
  rubric-exclusion candidate; test-gap → a candidate test note). It pulls the PR's
  reviews and review comments through `gh api`, following pagination to the last page,
  and registers every finding as a `codex-pr:<class>` signal — so `pdca act index`
  flags a class that **reappears after its process delta was applied**, which is the
  question the ledger exists to answer. It proposes only: it never edits `pdca.toml`,
  files a gate row, or touches the rubric.

  *(Until v0.57.0 this was `scripts/triage-pr-findings`, an instance-local script. The
  harness now ships the same job as `pdca triage`, so the script was retired with the
  upgrade — the dated entries in `process/act-log.md` that name it are the historical
  record of runs that really happened under that name, and are left as written.)*

  **One boundary in the ledger, worth knowing before you read a recurrence.** Both the
  retired script and `pdca triage` write `codex-pr:` signals, but they slug them
  differently: the 18 pre-0.57 entries in `process/act-ledger.json` are topic slugs
  (`codex-pr:docs-currency`, `codex-pr:vacuous-assertion`), while upstream's grammar puts
  the CLASS first and the deciding keyword second (`codex-pr:convention-docstring`). So a
  finding that reappears after the upgrade registers under a *new* name and will **not**
  match its pre-0.57 entry — the first recurrence of an old class reads as a new signal.
  The old entries were left as written rather than rewritten: their class is not
  recoverable from the topic slug, and guessing it would put invented data in the record
  the Act cadence trusts. Expect the recurrence signal to be blind across this one seam,
  and check the pre-0.57 entries by hand when an old delta looks like it came back.

## 9. Repo-specific scripts and tooling
| Role | Path | Invocation | Status |
|---|---|---|---|
| **Gate runner (delegated)** | `engine/xtask.sh` → `$PDCA_WORKTREE` `cargo xtask` | `./engine/xtask.sh <ci\|conformance\|dst>` (cd's to the per-cycle worktree, execs `cargo xtask`) | [built — wholesale delegation; **Wyrd owns the gate defs**, ADR-0016] |
| Per-fix verify | `engine/scripts/run-verify.sh` | `C4-verify` gate (red→green in a `../wyrd-verify` worktree) | [built — bundle-scoped, advisory] |
| Per-fix diff coverage | `engine/scripts/run-diff-cov.sh` | `C4-diff-cov` gate (`cargo llvm-cov` over the patch's changed lines in a `../wyrd-cov` worktree; floor `$WYRD_DIFFCOV_MIN`, default 80) | [built — bundle-scoped, **permanently advisory**: no `promote_after`, because a `#[cfg]`-gated line is indistinguishable from a comment in lcov, so the row can read 100% green over a fraction of a patch (#222)] |
| Gates (single-sourced) | `pdca.toml` `[gates] runner` + `checks` | `pdca gates [<id>] [--working-tree]` | [built — `C4-ci` gating; T1/T3 rows optional/advisory] |
| Tracker read | `gh` CLI | `gh issue view <id>` (ad hoc; no scraper needed) | [host tool] |
| Driver | `src/pdca_harness/` | `pdca run <id>` / `pdca flow <id>` | [built — stub leaves; wire `command` for real runs] |
| Act tooling (L4) | `src/pdca_harness/act.py` | `pdca act index`, `pdca act log --date <d>` | [built] |
| PR-review triage | `src/pdca_harness/triage.py` | `pdca triage <pr>` (gh-paginated ingest → 4-class routing → `codex-pr:` ledger signals) | [built — v0.57.0; replaced `scripts/triage-pr-findings`] |
| Bundle recording | `src/pdca_harness/record.py` | `pdca record [<ids>…]` (terminal bundles only; publish triggers it) | [built — v0.57.0; `[records] mode = "commit"`] |
| Leaf exit contract | `src/pdca_harness/handoff.py` + `.claude/hooks/handoff_guard.py` | `/handoff <issue_id>` in an interactive leaf; enforced by the `Stop` hook | [built — v0.57.0; replaced `scripts/handoff-check`] |
| Gates (single-sourced) | `pdca.toml` `[[gates.checks]]` | `pdca gates [<id>] [--working-tree]` | [built — stub fallback; fill checks] |
| Reviewer role prompt | `agents/reviewer.md` (canonical body; inlined for codex, `.claude/agents/reviewer.md` is the Claude packaging) | (model leaf) | [built — contract; wire command mode] |
| Builder role prompt | `agents/builder.md` (canonical body); `.claude/agents/builder.md` (Claude wrapper) + `.claude/hooks/builder_guard.py` | (model leaf) | [built — ready-mark blocked] |

## 10. Maintainer and governance
- **Who reviews:** Eduard Ralph (founding maintainer during bootstrap, per
  `../wyrd/docs/governance/GOVERNANCE.md`). ADR/spec/proposal acceptance is the
  architecture board's (provisional founding-maintainer authority until the board reaches
  three members).
- **Ready-mark gate:** PRs open as **draft**; the human re-reads and marks ready. The
  builder/publisher leaves never `gh pr ready` / `gh pr merge` (mechanically blocked by
  `builder_guard.py`). **Scope under `wave_mode = "merge"`** (2026-08-02, see §2 "Wave
  sequencing"): this gate holds unchanged for the model leaves and for every FINAL-wave PR;
  for a **non-final** wave of a dependent batch, the deterministic driver (not a leaf)
  readies and merges the wave's PRs at the wave boundary — the human's fresh-eyes read for
  those happens at per-bundle sign-off (before publish), not on the open PR. That trade is
  deliberate; its guardrail is host-side: keep `main`'s **required status checks** covering
  the real gates (`rust`, `gate`, `dco` — as of 2026-08-02 only `docs-check` /
  `require-issue` / `docs-immutability` are required, which would NOT stop a red auto-merge;
  tighten before the first merge-mode batch).
  **Superseded 2026-08-08 by `auto_merge = false`** (see §2): the trade above is off, so the
  ready-mark gate now holds for **every** PR the driver opens, non-final waves included —
  nothing is readied or merged without a human. `gate` did become a required context by
  2026-08-08 (`docs-check`, `require-issue`, `docs-immutability`, `gate`, `dco`), so the
  host-side guardrail is in place should auto-merge be turned back on.
- **External-contribution flow:** standard GitHub PR against `main`, gated by
  `require-issue` / `dco` / `cargo xtask ci`.
- **MAINTAINERS file:** `../wyrd/docs/governance/GOVERNANCE.md` is the authority (roles +
  ladder); no separate MAINTAINERS file.

### Composing with the host's CI / PR governance (issue #67)

PDCA **supplements** Wyrd's existing governance; it does not replace it:

| Host gate (Wyrd) | PDCA equivalent (supplement) | How they compose |
|---|---|---|
| `require-issue` on PRs | `[tracker].issue_trailer` (`Fixes #{id}`) | The trailer satisfies the linked-issue rule; init-from-brief maps onto a qualifying issue. |
| `dco` / `adr-immutability` | builder/publisher STOP hook (`builder_guard.py`) | The hook backstops `gh pr ready`/`merge` for the leaves; Wyrd's gates are the authority. |
| `cargo xtask ci` | `pdca gates --working-tree` re-gate | Both invoke the **same** `cargo xtask ci` via `engine/xtask.sh` — one definition, no drift. |
| `typos` / `docs-check` (always-on prose jobs) | covered by the same `cargo xtask ci` delegation as of wyrd#598 (**merged in wyrd PR#599, 2026-07-18**) | Before wyrd#598 these two host jobs were OUTSIDE `cargo xtask ci` and unmapped here — the blind spot behind wyrd PRs #595/#564/#569 opening red on `typos` (act-log 2026-07-18); **conditionally closed by the PR#599 merge** — `C4-ci` now re-gates them against the patched tree, but only where `typos-cli` / the doc renderer are installed on the PDCA host; absent those it warn-skips and the pipeline can still open a PR red on these (§3). `docs-immutability` remains host-only by decision (needs git-diff context; has never failed a pipeline PR). |

`ship_ci_workflow = false` at render (Wyrd runs its own CI); `ship_merge_guard = true`
keeps the builder STOP backstop.

## 11. Per-repo P-/D-/C-/A- extensions
None today. Add repo-prefixed rules (e.g. `wyrd-pdca-C7`) that *tighten
or add to* a generic rule — never weaken one — as running cycles surface them.

## Answering an interactive leaf from another device

The four `interactive = true` leaves — planner, sign-off, publisher, Act — hand the terminal
to a human+model REPL and block there. That means the human has to be at the terminal the
flow runs in, for the whole batch: a `pdca flow` over several bundles can park on one
sign-off adjudication for hours because nobody is at that machine.

Claude Code's `--remote-control` flag removes the constraint. Add it to an interactive
leaf's `argv` in `pdca.toml` — anywhere but last: the driver appends the seed prompt as a
positional after the argv, and `--remote-control` takes an optional `[name]` value, so as
the final token it would swallow the whole seed as the RC session name (issue #396; the
claude-family spawn also inserts `--` before the seed as a backstop, but no flag with an
optional value should ever sit last). With the flag in place that leaf becomes answerable
from another enrolled device; nothing else changes — `signoff-decision`, the §6 ticks and
the C6 accept-guard all run the same code path, and only the human's location differs.

Enabling Remote Control in your own shell does **not** reach the leaves: each is a separate
`claude` subprocess whose argv comes from `pdca.toml`. That is the whole reason this needs
documenting.

The headless builder and reviewer must not carry the flag — it starts an *interactive*
session, and they have no human to reach.
