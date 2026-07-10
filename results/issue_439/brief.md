# Brief — issue 439 / fdb-dev-ci-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> **RE-PLAN (iteration 4).** Iterations 1–3 rebuilt against the previous brief and the
> adversarial pass reopened the SAME two seams each time. This brief does not change the
> design — it pins those two seams by their *binding property* so Do cannot re-decide them
> a fourth time. See "Re-plan resolution" below; the rest carries forward verified.

- **Slug:** fdb-dev-ci-harness
- **Defect:** `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) exists and works —
  #438 landed it together with the throwaway cluster at
  `deploy/fdb-single-node/docker-compose.yml` — but **nothing invokes it.** Three concrete
  gaps, all re-verified against `origin/main` (`b1ccca3`):
  1. **No CI job runs it.** `.github/workflows/` has no FDB job at all (`git -C ../wyrd
     ls-tree --name-only origin/main .github/workflows/ | grep -i fdb` → empty). The
     FoundationDB backend — the *chosen production metadata backend* (ADR-0042) — has zero
     automated coverage on any PR or nightly. It can rot silently.
  2. **The gate never compiles the `fdb` feature.** `run_ci_steps` builds/tests
     `--workspace` with default features; the only feature-gated typecheck is
     `feature_gated_checks()` (`xtask/src/main.rs:1255`, re-read: a **zero-argument fn** with
     exactly one entry — `wyrd-metadata-tikv --features tikv` — gated on
     `WYRD_TIKV_TOOLCHAIN`). So the entire `#[cfg(feature = "fdb")]` `store` module of
     `crates/metadata-fdb/src/lib.rs` and every `#[cfg(feature = "fdb")]` arm in
     `crates/server/src/cli.rs` are **never type-checked by `cargo xtask ci`**.
  3. **No preflight.** A developer with no `libfdb_c`, no cluster file, or a dead
     container gets a raw linker error or a transaction timeout, not an actionable message.
     There is **no doctor anywhere in the tree** (`git -C ../wyrd grep -il doctor origin/main
     -- xtask/` → empty).
- **Success criterion:** `cargo test -p xtask --test fdb_harness` passes on the plain
  worktree (**no Docker, no `libfdb_c`, default toolchain**), asserting all five of:
  1. `xtask::fdb_doctor` exists as a **pure, non-privileged** library module (peer:
     `xtask::deploy_guard`, exported at `xtask/src/lib.rs:16`) mapping each probe outcome
     to a verdict + remediation string: a missing `libfdb_c` names the client package; an
     unreadable cluster file names `WYRD_FDB_CLUSTER_FILE`
     (`crates/metadata-fdb/src/lib.rs:386`) and its `/etc/foundationdb/fdb.cluster` default
     (`:390`); an unhealthy cluster names
     `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`.
  2. A **demonstrated red**: a planted failing probe outcome makes the doctor report
     not-ok, mirroring `scan_dir_is_red_when_an_orchestrator_import_is_planted`
     (`xtask/tests/deploy_no_orchestrator_coupling.rs:67`) — the row logic is load-bearing,
     not resting red on non-existence.
  3. **[RE-PLANNED — binds on the command HEAD, not on mention.]**
     `.github/workflows/fdb-conformance.yml` exists and is CONSISTENT with the real xtask
     dispatch table, where consistency is bound by the command-HEAD property:
     - Every `cargo xtask` invocation in the workflow is a **bare, single-command `run:`
       step** (`run: cargo xtask <sub>` — no `&&`, no wrapping command, no shell
       composition; artifact constraint, see Scope note (b)).
     - The test extracts the **command head** of each `run:` step (the first shell-word
       sequence after the `run:` / `- run:` key and leading whitespace) and (i) requires the
       required subcommands to appear as heads, and (ii) cross-checks every `cargo xtask
       <sub>` head against the **real dispatched-subcommand set** scraped from `Some("<sub>")
       =>` in `xtask/src/main.rs` — the compiled `readme_dev_section.rs:42` dispatch pattern
       — so a bogus/typo'd head fails.
     - A subcommand that appears only as a **non-head token** — an argument
       (`echo … cargo xtask fdb-conformance`), after a no-op builtin (`: cargo xtask …`), or
       inside a comment — MUST NOT be counted. **No `windows()` scan of the whole line.**
     - The **demonstrated-red enumerates every known evasion shape as an explicit case**,
       each of which turns the suite RED when it replaces the real invoking step:
       (i) full-line `#` comment; (ii) trailing inline `#` comment on a real command;
       (iii) mention-as-argument (`echo would run cargo xtask fdb-conformance`); (iv) no-op
       builtin prefix (`: cargo xtask fdb-conformance`). This is the exact class that reopened
       iterations 1–3; the enumerated red is how a fourth regression is caught at Check.
     - **PR path-filter ↔ command coverage** (iteration-1 item 4): the pull-request path
       filter includes `crates/metadata-fdb/**` **and** `crates/server/**`, and the PR leg
       actually runs a command that type-checks BOTH FDB feature arms (e.g.
       `cargo check -p wyrd-server --features fdb` on the PR leg). No filter may promise a
       check the leg does not perform — either the leg checks `crates/server`'s `fdb` arms or
       `crates/server/**` is dropped from the filter.
  4. `xtask::feature_gated_checks(tikv: bool, fdb: bool)` — **moved into `xtask/src/lib.rs`**
     (it is a private free fn in the *binary* target today, `xtask/src/main.rs:1255`, so an
     integration test under `xtask/tests/` — which links the **lib** — cannot call it; it
     must move as `deploy_guard` did) — yields the `wyrd-metadata-fdb --features fdb` and
     `wyrd-server --features fdb` rows when `fdb` is true, and *not* when it is false,
     independently of `tikv`. Two explicit parameters (not the old zero-arg fn appended to)
     and a separate `fdb_toolchain_available()`, so the FDB typecheck does not silently
     couple to `WYRD_TIKV_TOOLCHAIN`. Plus `cargo xtask ci` (the gating `C4-ci`) stays green
     on a machine with no FDB.
  5. **[RE-PLANNED — the impure-adapter guard; adjudicated at Plan, see Re-plan resolution
     §2.]** No effect-supplying decision lives UNGUARDED in the binary target. The main.rs
     adapters that hand real effects to the tested pure/injected lib functions —
     `probe_client_library()` (the `&|name| std::env::var(name).ok()` read, ~`main.rs:899`)
     and `run_fdb_conformance`'s preflight call site (the `run_gated_conformance(…)` args,
     ~`main.rs:309`) — are **total, logic-free pass-throughs**, and that totality is pinned at
     Check. **Binding property (falsifiable):** mutating the env closure
     `&|name| std::env::var(name).ok()` → `&|_| None`, OR hardcoding the preflight call's
     probe arguments to a passing `Outcome`, MUST turn a Check test RED. (Do MAY satisfy this
     with the established structural body-assertion idiom — `function_body` /
     `conformance_body_is_gated`, extended to assert the adapter body carries the real effect
     and no hardcoded outcome — OR a behavioral live-adapter test; either is acceptable, but
     both named mutations must demonstrably go red.)
- **Falsifiability:** Every assertion goes RED on the plain `$PDCA_WORKTREE` /
  `../wyrd-verify` checkout with the stock toolchain — assertions 1/3 because the module and
  the workflow file do not exist (compile/IO error), assertion 2 because the planted probe
  has nothing to catch it, assertion 4 because `feature_gated_checks()` returns a one-element
  vec today (`xtask/src/main.rs:1255-1264`), assertion 5 because there is no adapter or guard
  yet. No Docker and no `libfdb_c` are needed for the binding criterion — deliberate: the
  *point* of the slice is that the gate stays container-free.
  **The two re-planned assertions are specifically falsifiable by the mutations that reopened
  prior iterations** — assertion 3 by each of the four enumerated evasion shapes (each must go
  red), assertion 5 by the `real-effect → None` / hardcoded-outcome mutations (each must go
  red). If Do ships either seam such that its named mutation stays GREEN, the criterion is not
  met — that is the whole reason for the re-plan.
  **Supplementary live evidence, runnable here:** this host has Docker (`docker info` OK),
  `libfdb_c` **7.3.77** (`/lib/libfdb_c.so`) and `fdbcli` **7.3 (v7.3.77)** — byte-matching
  the compose image `foundationdb/foundationdb:7.3.77` and the crate pin. So Do MUST also run
  `cargo xtask fdb-conformance` end-to-end and record the result in `build-notes.md`.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`, and the branch
  is deleted — the issue's "targets `main` after M4 merges" condition is met, and INTEGRATION
  §2's M4 integration-branch exception no longer applies.)
- **Ordering note:** 439 has **no dependency and no conflict**: it lands in wave 1 alongside
  441 and 468, and its write-set (`xtask/src/main.rs`, `xtask/src/lib.rs`, the new
  `xtask/src/fdb_doctor.rs` and `xtask/tests/fdb_harness.rs`, the new
  `.github/workflows/fdb-conformance.yml`, and `deny.toml`'s header comment) is disjoint from
  every other bundle in the batch. An earlier draft declared `Conflicts with: 469` because 469
  would have edited `xtask/src/main.rs:658-660` when renaming `deploy/small-multi-node/`; the
  human deferred that rename, so 469 now touches no `xtask/src/` at all and the edge is gone.
  Two invariants hold that: 469 must not take the rename or add an `xtask` bring-up arm, and
  **439 must not write to `docs/design/`** (441 owns `07-deployment-view.md` in the same wave)
  — the audit-policy note goes in `deny.toml`'s header comment. 470 keeps its test helpers
  local to `xtask/tests/fdb_image.rs`, so it adds no `xtask/src/lib.rs` hunk. 439 is
  deliberately kept **independent of the `wyrd:fdb` image (#470)**: its workflow installs the
  FoundationDB *client* package on the runner and calls `cargo xtask fdb-conformance`, which
  brings up the existing `deploy/fdb-single-node/` compose service — no image build in the loop.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Give the shipped-but-ungated `fdb` backend a standing automated signal and an
  actionable local preflight. Three deliverables:
  (a) `xtask::fdb_doctor` — a pure library module (probe results in → verdict + remediation
  text out) plus an `xtask fdb-doctor` dispatch arm (a new `Some("fdb-doctor") =>` in the
  dispatch table; it is not there today, `main.rs:64-79`), invoked as `run_fdb_conformance`'s
  preflight (`xtask/src/main.rs:292-323`) so the job fails fast with guidance instead of a
  linker error. The three rows the issue names: `libfdb_c` present/loadable, cluster file
  readable, `fdbcli --exec "status minimal"` healthy.
  (b) `.github/workflows/fdb-conformance.yml` — installs the pinned FoundationDB client
  package, runs `cargo xtask fdb-conformance`, on pull requests touching
  `crates/metadata-fdb/**` (and `crates/server/**` iff the PR leg type-checks its `fdb` arms)
  **and** nightly. Follow the shape of `.github/workflows/integration-nightly.yml` (container
  job, `docker info` step, timeout, failure-artifact upload) and
  `.github/workflows/tier1-jepsen.yml` (nightly cron + `workflow_dispatch`, explicitly **not**
  a required merge gate — ADR-0016 keeps the `cargo xtask ci` merge gate unprivileged and
  container-free). **Author every xtask invocation as a bare, single-command `run:` step**
  (`run: cargo xtask <sub>`): this is the "stronger structural check" the re-plan asked me to
  weigh — rather than build an ever-cleverer parser for unconstrained shell (the rabbit hole
  three iterations fell into), constrain the artifact so the command head is unambiguous and
  cross-check it against the compiled dispatch table. Do NOT wrap xtask in `bash -c`, `&&`
  chains, or `run: |` multi-command blocks.
  (c) `feature_gated_checks()` gains the two `--features fdb` typechecks (metadata-fdb,
  server) behind an FDB-toolchain env gate mirroring `WYRD_TIKV_TOOLCHAIN`
  (`xtask/src/main.rs:1272`), plus the **audit-policy record**: a short note stating the split
  the issue calls out — `cargo deny`/`cargo audit` see the Rust graph
  (`foundationdb-sys -> bindgen -> clang-sys -> libloading`, already allowlisted at
  `deny.toml:38-45`) but **cannot** see `libfdb_c` itself, whose advisory surface is tracked
  by following upstream FoundationDB release notes.
  **The note lands in `deny.toml`'s header comment block (`:1-6`) — that file IS the audit
  policy.** This location is pinned, not left to Do, for a scheduling reason: **441** (same
  wave) owns `docs/design/architecture/07-deployment-view.md`, the other plausible home for a
  packaging note. Two wave-1 bundles editing one file are built blind on the same base and
  `integrate.fold` would apply both patches — a textual conflict there is an
  `IntegrationError` and a hard STOP. **Do MUST NOT write to any file under `docs/design/`.**
  ADR-0003 §2 is the normative source and is **Accepted / immutable**; do not touch it.

  **out of scope:**
  - **Item 1 of the issue body (the local dev cluster) — ALREADY LANDED.** `#438` shipped
    `deploy/fdb-single-node/docker-compose.yml` (commit `22d39b6`), the `configure new single
    memory` init (`xtask/src/main.rs:334`), the host-side cluster-file write, and the test
    driver. Do MUST NOT re-create it. The issue's "one `make`/script entry point" is already
    satisfied by `cargo xtask fdb-conformance` — **Wyrd has no `Makefile` and deliberately
    does not want one** (ADR-0016). Do not add one.
  - **`deny.toml` allowlist / `[advisories]` / `[bans]` edits.** Item 4's dependency-graph
    work is already done (the `fdb` optional dependency is in `Cargo.lock`, so `cargo deny
    check` already traverses it — precisely why `deny.toml:38-45` has the ISC entry). Only the
    **written audit-policy note** is missing, and it goes in that file's **header comment**
    (`:1-6`); no allowlist row, advisory ignore, or ban is added or changed.
  - The `wyrd:fdb` OCI image and its build job (#470); the FDB deploy profiles (#469); the
    version-skew startup guard (#441); the nightly **TiKV** conformance counterpart (#420,
    still open — this slice establishes the pattern #420 mirrors, it does not implement #420).
- **Repro instruction:** On `origin/main` (`b1ccca3`) in the target checkout:
  `git -C ../wyrd show origin/main:xtask/src/main.rs | sed -n '1255,1264p'` → the
  feature-gated typecheck list has one entry (tikv). `git -C ../wyrd ls-tree --name-only
  origin/main .github/workflows/ | grep -i fdb` → empty. `git -C ../wyrd grep -in doctor
  origin/main -- xtask/` → no output: there is no doctor anywhere in `xtask/`.
- **External dependencies:**
  - *Binding criterion:* **none** beyond the base Rust toolchain. Hard requirement: the new
    `xtask/tests/fdb_harness.rs` must run inside the ordinary `cargo test --workspace` that
    `run_ci` performs, on a machine with no FoundationDB. Keep the doctor's decision logic
    pure (probe results are *inputs*), as `xtask::deploy_guard::scan_line` is pure.
  - **No new Cargo dependency.** `xtask/Cargo.toml` depends only on `wyrd-chunk-format`,
    `serde`, `serde_json` — there is **no YAML parser**. Assert the workflow's content with
    plain-text/substring + command-head checks, exactly as `xtask/tests/readme_dev_section.rs`
    asserts the README. Adding a crate triggers the ADR-0003 §2 three-test audit + the
    `deny.toml` allowlist — a **human-only** decision (INTEGRATION §4) that would turn this
    bundle into a NEEDS-HUMAN at sign-off.
  - *Supplementary live leg (`cargo xtask fdb-conformance`), all present on this host:*
    Docker + the compose plugin; `libfdb_c.so` 7.3.77; `fdbcli` 7.3.77; the ability to pull
    `foundationdb/foundationdb:7.3.77`. If Do finds any missing it MUST say so in
    `build-notes.md` rather than substituting a code-read for the run.
- **Test file:** `xtask/tests/fdb_harness.rs` (new). Peer pattern to imitate:
  `xtask/tests/deploy_no_orchestrator_coupling.rs` (planted-red + green-on-real-tree) and
  `xtask/tests/readme_dev_section.rs` (documented/authored command ↔ real dispatch table).
- **Verification posture:** Mixed, declared here so it is not a surprise at sign-off.
  - **Flippable at Check (the binding criterion):** the `fdb_doctor` unit tests, the
    planted-red, the workflow command-head↔dispatch consistency assertion (assertion 3, with
    its four enumerated evasion reds), the `feature_gated_checks()` content assertion, and the
    impure-adapter guard (assertion 5, with its two mutation reds). All red pre-fix, green
    post-fix, no external dependency.
  - **Deferred (off-Check):** the workflow's own *green on a GitHub-hosted runner*. Nobody can
    observe that from `cargo xtask ci`. Confirmed by the maintainer on the first nightly run /
    on the PR that lands it.
  - **Deferred ≠ unbuilt (the #146 forcing function):** what is BUILT AND EXERCISED at Check
    is (i) the whole `fdb_doctor` module, unit-tested; (ii) the workflow file, parsed and
    asserted against the real `xtask` dispatch table by command head; (iii) the extended
    `feature_gated_checks()` list; and (iv) the impure adapters' totality. The deferred item is
    only the *hosted execution* of a workflow whose every referenced command is proven to
    exist and to be a real dispatched subcommand. Nothing here is inert scaffolding.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change.
  Composition slices — Do MAY open these cited peer callsites to mirror the pattern:
  - Pure-logic-in-lib + planted-red test: `xtask/src/lib.rs:1-19` (its module doc states the
    "born-at-tier flippable coverage seam" contract) and
    `xtask/tests/deploy_no_orchestrator_coupling.rs:67` / `:102`.
  - **Command-head ↔ real dispatch cross-check (assertion 3):** `xtask/tests/readme_dev_section.rs:42`
    (scrapes the real `Some("<sub>") =>` dispatch set from `main.rs` and asserts a documented
    command is backed by it) and the dispatch table itself, `xtask/src/main.rs:64-79`. Mirror
    this: bind the workflow's command HEADS to that same compiled set, never a text window.
  - **Total-pass-through body assertion (assertion 5):** the existing `function_body` +
    `conformance_body_is_gated` idiom (in the current `xtask/tests/fdb_harness.rs`, iteration-3
    at `:461`/`:479`) — extend it so the adapter body must carry the real effect
    (`std::env::var`) and must NOT carry a hardcoded `None`/`Outcome`.
  - Env-gated feature typecheck: `xtask/src/main.rs:1255` and `:1272`.
  - Preflight/skip convention (hard failure in CI, warn-and-skip locally):
    `xtask/src/main.rs:299-312` and `is_ci` at `:1508`.
  - Workflow shape: `.github/workflows/integration-nightly.yml` (whole file) and
    `.github/workflows/tier1-jepsen.yml:1-30` (nightly + opt-in + non-gating rationale).
- **Prior-art check (triage cycles):** Searched by affected file path across merged history,
  open PRs, and closed/rejected PRs (re-verified `b1ccca3`).
  - `git -C ../wyrd log --oneline --all -i --grep=doctor` → **no commits, ever.** The issue's
    "extend the environment doctor" describes something that does not exist; the only `docker
    info` occurrences are three CI workflow *steps*. This slice creates the doctor.
  - `.github/workflows/` — no FDB job has ever existed. `xtask/src/main.rs` —
    `fdb-conformance` added by #438, never wired to CI.
  - Rejected work: one non-merged closed PR in the last 60 (#400, docs/proposal scope) —
    unrelated to these paths. Merged prereqs: #438 (PR #492), #440 (PR #493). #420 (nightly
    TiKV conformance) is still **open** — so there is no TiKV counterpart to "mirror"; this
    slice is the first of the pair.
  - **This bundle's own iterations 1–3** are preserved under `iteration-v1..v3/`; the two
    re-planned seams above are the sole delta from iteration-3's brief.
- **Disposition hint:** new-feature

## Re-plan resolution — the two seams pinned at Plan (the whole reason for iteration 4)

The re-plan directive (SUMMARY §9, iteration 3) delegated two decisions to Plan. Both are
resolved here so Do does not re-decide them:

**1. Workflow↔execution — bind on the command HEAD + compiled dispatch, and constrain the
artifact.** Root cause of the three-iteration recurrence: `xtask_subcommands` scans a
`tokens.windows(3)` over the whole line, so a *mention* (`echo would run cargo xtask
fdb-conformance`, `: cargo xtask …`, a trailing `# …` comment) counts as an *execution*.
Resolution (Success criterion assertion 3): (a) the workflow authors every xtask call as a
**bare single-command `run:` step**, removing the shell degrees of freedom the scraper
exploited; (b) the test binds on the **command head** of each `run:` step and cross-checks it
against the **real `Some("<sub>") =>` dispatch set** (`readme_dev_section.rs:42`), so a
non-head mention never counts and a bogus head fails; (c) the **demonstrated-red enumerates
all four evasion shapes** as explicit red cases. No window scan. This is a Plan decision, not a
Do choice: iterations 1–3 each independently reached for a narrower comment-stripper; the brief
now names the binding property and the artifact constraint outright.

**2. The impure adapter DOES warrant a Check guard — adjudicated, not left open.** The second
recurring finding: the main.rs adapters that supply real effects to the tested pure/injected
lib fns are unguarded — mutating `probe_client_library()`'s `&|name| std::env::var(name).ok()`
→ `&|_| None`, or hardcoding `run_fdb_conformance`'s preflight args, leaves every Check test
green while reintroducing iteration-1's exact false-negative (a working `FDB_CLIENT_LIB_PATH`
build reported "missing" → local false-green skip / CI hard-fail). **Adjudication: guard it.**
A class that has already shipped a rejected defect is not "trusted glue." The guard is cheap
because the adapter is genuinely trivial — a total, logic-free pass-through of real effects to a
fully-tested pure core — so pinning its *totality* is sufficient (we are not asking Do to
unit-test `std::env::var`). Binding property in Success criterion assertion 5: the two named
mutations must turn a Check test RED; Do may use the structural body-assertion idiom or a
behavioral live test.

## Plan-exit gate

Not applicable as a *gate*: this is new-functionality work (`docs/principles.md` §1.3 —
"New-feature work is not governed by minimalism at all"), not a structural / lifecycle /
load-or-import-safety defect, so the category-gated Plan-exit checks do not apply and no
`Invariant to restore` is pulled. Mechanism is named on purpose here: for a feature the brief
is a specification, and the binding *properties* (command head, adapter totality) are pinned
precisely because leaving them to Do re-opened the bundle three times.

## Deliberate deviation from the tracker — flagged, not silent

Issue #470's body says *"the #439 conformance workflow and the #442 battery consume this image
rather than ad-hoc builds."* This brief's workflow does **not** consume `wyrd:fdb`: it installs
the FoundationDB *client package* on the runner and calls `cargo xtask fdb-conformance`, which
boots a `foundationdb/foundationdb` container and runs `cargo test -p wyrd-metadata-fdb`. That is
a test-harness run; the `wyrd:fdb` image carries the `wyrd` binary, not the test suite. This
bundle builds no ad-hoc image, so the harm the tracker names does not arise. Wiring the two would
force 439 (wave 1) to depend on 470 (wave 2), inverting the batch order for no gain. The full
argument, and where the requirement genuinely lands (#469, #442), is recorded in **470's** brief.
Do MUST NOT attempt to consume the image here.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration carry-forward (context for Do — do NOT re-attempt the rejected approaches)

- **Iterations 1–3 preserved** in `iteration-v1/`, `iteration-v2/`, `iteration-v3/`
  (patch.diff, build-notes.md, SUMMARY.md, check-*). The design was sound and passed the
  primary reviewer every time; the **adversarial pass** reopened the bundle on exactly the two
  seams now pinned above. **Trust the mutations, not the PASS row.**
- **What SURVIVED adversarial attack across iterations — keep as-is, do NOT churn:**
  the fdb/tikv toolchain-coupling independence (`feature_gated_checks(tikv, fdb)` +
  `run_ci_steps` per-gate env lookup); the doctor's pure logic, planted-red, and the three-file
  version-pin drift guard; `HEALTHY_STATUS_NEEDLE` (text-match, not exit status — real fdbcli
  7.3.77 exits 0 against a dead coordinator); `cluster_file_path` mirroring the driver's
  `config::cluster_file`; the preflight branch counting in `run_gated_conformance`.
- **Out of scope, recorded as an Act candidate — do NOT fix here:** `configure_fdb_database`'s
  readiness poll gates on `status.success()` with output nulled; given fdbcli's exit-0-when-
  unavailable behaviour it returns Ok on attempt 1. Pre-existing (#438), not introduced here;
  wiring the tested predicate there is a separate slice.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild against the same brief — do NOT re-plan. Gating C4-ci is green and the harness (doctor module, workflow, feature-gated checks, audit note) is real and worth keeping — do not churn it. What to fix — the adversary's landed refutation on the one seam this issue has reopened for four iterations: 1. The assertion-5 "impure adapter" guard is SUBSTRING-only, not data-flow. It pins that the adapter body *contains the text* `std::env::var` (fdb_harness.rs:541) and `Path::new(`/`.exists()` (:547) — character checks, not behaviour. Two one-line semantic-equivalent mutations keep all 28 tests green while reintroducing the exact iteration-1 false-negative: - main.rs:393 `&|name| std::env::var(name).ok()` -> `&|name| { let _ = std::env::var(name); None }` (env read discarded, closure always None -> working FDB_CLIENT_LIB_PATH build reported "missing" -> false-skip locally / hard-fail in CI). - main.rs:394 `&|candidate| Path::new(candidate).exists()` -> `&|candidate| { let _ = ...; true }` (library always reported present -> preflight proceeds into the container stack with no libfdb_c -> linker error minutes in, the very thing the preflight exists to prevent). FIX: close the DEFECT CLASS, not the two literal mutation spellings. Add the behavioural live-adapter test the brief already offered (drive the real adapter against a fake environment/filesystem where only the FDB_CLIENT_LIB_PATH dir has libfdb_c, and assert the probe resolves it; and a fake where nothing exists, asserting it reports missing) so a discarded env-read or a hard-coded `true` flips a Check test red. The brief's intent is "the false-negative cannot be reintroduced," not "the two named mutations go red." 2. Minor, fold in while there: fdb_doctor.rs:297 HEALTH_COMMAND is `fdbcli --exec "status minimal"` but the real probe runs `fdbcli -C <file> --exec "status minimal"`. Cosmetic (advisory text), align it. Carried-forward NEEDS-HUMAN that remain open for the rebuild's sign-off: T3 runtime (live FDB topology never exercised — Docker denied here; a Docker-enabled runner must confirm the real compose run), T4 prior-art (closed/rejected PR state not mechanically settled), and fitness-to-purpose (container-free evidence before the first hosted FDB workflow green).
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
