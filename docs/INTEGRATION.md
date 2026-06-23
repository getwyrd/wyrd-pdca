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
  own dedicated `../wyrd-verify` worktree off `origin/main` (`$WYRD_REPO` overrides). Both
  the cycle worktree and the `C4-verify` worktree (and its `pdca-verify` branch) are
  **scoped per lane** under in-driver concurrency (`-l<slot>` suffix from `$PDCA_LANE`), so
  `--lanes N` runs without two lanes colliding on a checkout or a branch — the active gate
  set (`C4-ci`, `C4-verify`) is multi-lane-safe.
- **Per-area branch map:** everything targets **`main`**. Wyrd is early and has **no
  maintenance branches** today (no `maintenance/*`, no master-vs-maintenance split) — say
  so rather than invent one; add a map here if/when a release branch is cut.
- **Override convention:** a maintainer's explicit base-branch request on the PR wins
  (per `GOVERNANCE.md` decision-making); otherwise `main`.
- **Cross-version cherry-pick rules:** none today (single line). If back-porting starts,
  cherry-pick is a **correctness** check — "applies cleanly" ≠ "remains correct"; verify
  against the target branch's related code, including files the patch doesn't touch.
- **Immutability rule (host-enforced):** Wyrd's `adr-immutability` gate forbids editing an
  Accepted ADR (`../wyrd/docs/design/adr/`, ADR-0001). A Plan that needs to change an
  accepted decision authors a **new** superseding ADR — never edits the old one.

## 3. Reproduction fixtures and runners
- **Canonical fixture path:** the on-disk-format **conformance vectors** at
  `../wyrd/docs/design/specs/conformance/vectors/v1/` (valid) and `.../invalid/v1/`
  (malformed), each a `.fragment` + its `.expected.json` / `.reason.txt` oracle (ADR-0002).
- **Verification runner (the whole gate):** **`cargo xtask ci`**, delegated via
  `./engine/xtask.sh` (§9). It runs identically on a laptop and in CI (ADR-0016): fmt
  (`--check`), clippy (`-D warnings`), build, test (incl. DST property tests), `cargo deny
  check`, and the conformance run. Exit 0 = pass. This is Wyrd's single source of gate truth.
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

| Tier | Written ruleset (normative source) | Home | Single-sourced command | Status |
|---|---|---|---|---|
| C4 correctness | the change + Wyrd's whole gate | `cargo xtask` (`../wyrd/xtask/`) | `./engine/xtask.sh ci` (delegates `cargo xtask ci`) | [built — **gating**, scope=repo] |
| T1 format-conformance | chunk-format spec v1, RFC-2119 (`../wyrd/docs/design/specs/chunk-format/v1.md`); conformance spec `specs/conformance/v1.md` (ADR-0002) | `cargo xtask conformance` (vectors in `specs/conformance/`) | `./engine/xtask.sh conformance` | [built — runs inside `ci`; advisory row optional] |
| T2 shape | `rustfmt` + `clippy -D warnings` (no project style doc; the linters are the rule) | inside `cargo xtask ci` | (subsumed by `ci`) | [built — part of `ci`] |
| T3 runtime / DST | testing strategy, ADR-0009 (madsim DST is the spine; from M0) | `cargo xtask dst` + `test` | `./engine/xtask.sh dst` | [built — runs inside `ci`; advisory row optional] |
| T4 contribution | ADR-0003 §1 (DCO), `require-issue`, `adr-immutability`; commit/PR conventions (§8) | `cargo xtask` gates + GitHub CI | `./engine/xtask.sh ci` (re-gate) + host CI | [built — host-enforced] |
| T5 judgment | reviewer contract below | Check reviewer + sign-off | (model) | [planned] |

- **Reviewer family (cross-vendor, ≠ builder):** codex — config `AGENTS.md` (decorrelated
  path); `.claude/agents/reviewer.md` is the same-vendor fallback (execute-only scope).
- **Builder family:** claude — `.claude/agents/builder.md`, ready-mark blocked by the
  `.claude/hooks/builder_guard.py` PreToolUse hook.
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
- **Commit-message format:** concise subject; body explains the *why*; trailer
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **PR description format:** Root cause / Fix / Verified against / Test
  (`templates/pr-description.md.tpl`); reference the issue with `Fixes #nnnn`.
- **Enforcement mechanism:** host-side — `dco`, `require-issue`, and `adr-immutability`
  xtask/CI gates + maintainer review (GOVERNANCE). The builder/publisher STOP hook
  (`builder_guard.py`) is a backstop, not the authority.

## 9. Repo-specific scripts and tooling
| Role | Path | Invocation | Status |
|---|---|---|---|
| **Gate runner (delegated)** | `engine/xtask.sh` → `$PDCA_WORKTREE` `cargo xtask` | `./engine/xtask.sh <ci\|conformance\|dst>` (cd's to the per-cycle worktree, execs `cargo xtask`) | [built — wholesale delegation; **Wyrd owns the gate defs**, ADR-0016] |
| Per-fix verify | `engine/scripts/run-verify.sh` | `C4-verify` gate (red→green in a `../wyrd-verify` worktree) | [built — bundle-scoped, advisory] |
| Gates (single-sourced) | `pdca.toml` `[gates] runner` + `checks` | `pdca gates [<id>] [--working-tree]` | [built — `C4-ci` gating; T1/T3 rows optional/advisory] |
| Tracker read | `gh` CLI | `gh issue view <id>` (ad hoc; no scraper needed) | [host tool] |
| Driver | `src/pdca_harness/` | `pdca run <id>` / `pdca flow <id>` | [built — stub leaves; wire `command` for real runs] |
| Act tooling (L4) | `src/pdca_harness/act.py` | `pdca act index`, `pdca act log --date <d>` | [built] |
| Reviewer config | `AGENTS.md` + `.claude/agents/reviewer.md` | (model leaf) | [built — contract; wire command mode] |
| Builder subagent | `.claude/agents/builder.md` + `.claude/hooks/builder_guard.py` | (model leaf) | [built — ready-mark blocked] |

## 10. Maintainer and governance
- **Who reviews:** Eduard Ralph (founding maintainer during bootstrap, per
  `../wyrd/docs/governance/GOVERNANCE.md`). ADR/spec/proposal acceptance is the
  architecture board's (provisional founding-maintainer authority until the board reaches
  three members).
- **Ready-mark gate:** PRs open as **draft**; the human re-reads and marks ready. The
  builder/publisher leaves never `gh pr ready` / `gh pr merge` (mechanically blocked by
  `builder_guard.py`).
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

`ship_ci_workflow = false` at render (Wyrd runs its own CI); `ship_merge_guard = true`
keeps the builder STOP backstop.

## 11. Per-repo P-/D-/C-/A- extensions
None today. Add repo-prefixed rules (e.g. `wyrd-pdca-C7`) that *tighten or add to* a
generic rule — never weaken one — as running cycles surface them.
