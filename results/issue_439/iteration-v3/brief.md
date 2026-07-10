# Brief — issue 439 / fdb-dev-ci-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** fdb-dev-ci-harness
- **Defect:** `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) exists and works —
  #438 landed it together with the throwaway cluster at
  `deploy/fdb-single-node/docker-compose.yml` — but **nothing invokes it.** Three concrete
  gaps, all verified against `origin/main` (`b1ccca3`):
  1. **No CI job runs it.** `.github/workflows/` has no FDB job at all. The FoundationDB
     backend — the *chosen production metadata backend* (ADR-0042) — has zero automated
     coverage on any PR or nightly. It can rot silently.
  2. **The gate never compiles the `fdb` feature.** `run_ci_steps` builds/tests
     `--workspace` with default features; the only feature-gated typecheck is
     `feature_gated_checks()` (`xtask/src/main.rs:1255`), which contains exactly one entry
     — `wyrd-metadata-tikv --features tikv` — behind `tikv_toolchain_available()`
     (`:1273`). So the entire `#[cfg(feature = "fdb")]` `store` module of
     `crates/metadata-fdb/src/lib.rs` and every `#[cfg(feature = "fdb")]` arm in
     `crates/server/src/cli.rs` (`:101`, `:120`, `:168`, `:373`, `:459`, `:714`, `:840`,
     `:1480`, `:1486`) are **never type-checked by `cargo xtask ci`**.
  3. **No preflight.** A developer with no `libfdb_c`, no cluster file, or a dead
     container gets a raw linker error or a transaction timeout, not an actionable message.
- **Success criterion:** `cargo test -p xtask --test fdb_harness` passes on the plain
  worktree (**no Docker, no `libfdb_c`, default toolchain**), asserting all four of:
  1. `xtask::fdb_doctor` exists as a **pure, non-privileged** library module (peer:
     `xtask::deploy_guard`, exported at `xtask/src/lib.rs:16`) mapping each probe outcome
     to a verdict + remediation string: a missing `libfdb_c` names the client package; an
     unreadable cluster file names `WYRD_FDB_CLUSTER_FILE`
     (`crates/metadata-fdb/src/lib.rs:386`) and its
     `/etc/foundationdb/fdb.cluster` default (`:390`); an unhealthy cluster names
     `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`.
  2. A **demonstrated red**: a planted failing probe outcome makes the doctor report
     not-ok, mirroring `scan_dir_is_red_when_an_orchestrator_import_is_planted`
     (`xtask/tests/deploy_no_orchestrator_coupling.rs:67`) — the row logic is load-bearing,
     not resting red on non-existence.
  3. `.github/workflows/fdb-conformance.yml` exists; every `cargo xtask <sub>` it names is
     a subcommand `xtask/src/main.rs` actually dispatches (the
     `xtask/tests/readme_dev_section.rs` doc↔dispatch pattern, applied to the workflow);
     and its pull-request path filter includes `crates/metadata-fdb/**`.
  4. `xtask::feature_gated_checks(tikv: bool, fdb: bool)` — **moved into `xtask/src/lib.rs`**
     — yields the `wyrd-metadata-fdb --features fdb` and `wyrd-server --features fdb` rows when
     `fdb` is true, and *not* when it is false, independently of `tikv`.

  **Two hazards in assertion 4; both are why it is worded this way.**
  (i) *Reachability.* `feature_gated_checks()` today is a **private free fn in the binary
  target** (`xtask/src/main.rs:1255`). An integration test under `xtask/tests/` links the
  **lib** target (`xtask/src/lib.rs`), so it **cannot call it** — verified: `xtask/src/lib.rs`
  exports only `deploy_guard`, `disk_faults`, `metadata_faults`. It must move to the lib, as
  `deploy_guard` did (`main.rs:1208` already calls `xtask::deploy_guard::scan_dir`, the exact
  precedent). Do not "solve" this by text-scraping `main.rs`.
  (ii) *The gate is currently wrong for a second backend.* `run_ci()` (`:1324`) calls
  `run_ci_steps(tikv_toolchain_available(), …)`, and `run_ci_steps` gates the **whole**
  `feature_gated_checks()` list on that one boolean. Appending fdb rows to the existing
  zero-argument fn would make the FDB typecheck fire only when `WYRD_TIKV_TOOLCHAIN` is set —
  a silent, wrong coupling. Hence the two explicit parameters and a separate
  `fdb_toolchain_available()`.

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green **on a machine with no FDB**.
- **Falsifiability:** Every assertion above goes RED on the plain `$PDCA_WORKTREE` /
  `../wyrd-verify` checkout with the stock toolchain — assertions 1/3 because the module
  and the workflow file do not exist (compile/IO error), assertion 2 because the planted
  probe has nothing to catch it, assertion 4 because `feature_gated_checks()` returns a
  one-element vec today (read at `xtask/src/main.rs:1255-1264`). No Docker and no
  `libfdb_c` are needed for the binding criterion — this is deliberate, because the
  *point* of the slice is that the gate stays container-free.
  **Supplementary live evidence, and it IS runnable here:** this host has Docker (`docker
  info` OK), `libfdb_c` **7.3.77** (`/lib/libfdb_c.so`) and `fdbcli` **7.3 (v7.3.77)** —
  byte-matching the compose image `foundationdb/foundationdb:7.3.77` and the crate pin
  `foundationdb … features = ["fdb-7_3"]` (`Cargo.toml:108`). So Do MUST also run
  `cargo xtask fdb-conformance` end-to-end and record the result in `build-notes.md`.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`, and the
  branch is deleted — the issue's "targets `main` after M4 merges" condition is met, and
  INTEGRATION §2's M4 integration-branch exception no longer applies.)
- **Ordering note:** 439 has **no dependency and no conflict**: it lands in wave 1 alongside
  441 and 468, and its write-set (`xtask/src/main.rs`, `xtask/src/lib.rs`, the new
  `xtask/src/fdb_doctor.rs` and `xtask/tests/fdb_harness.rs`, the new
  `.github/workflows/fdb-conformance.yml`, and `deny.toml`'s header comment) is disjoint from
  every other bundle in the batch. An earlier draft declared `Conflicts with: 469` because 469
  would have edited `xtask/src/main.rs:658-660` when renaming `deploy/small-multi-node/`; **the
  human deferred that rename**, so 469 now touches no `xtask/src/` at all and the edge is gone.
  Two invariants hold that: 469 must not take the rename or add an `xtask` bring-up arm
  (its Recorded decisions 1 and 2), and **439 must not write to `docs/design/`** (441 owns
  `07-deployment-view.md` in the same wave) — the audit-policy note goes in `deny.toml`'s
  header comment. 470 likewise keeps its test helpers local to `xtask/tests/fdb_image.rs`
  (the `readme_dev_section.rs` precedent), so it adds no `xtask/src/lib.rs` hunk. 439 is
  deliberately kept **independent
  of the `wyrd:fdb` image (#470)**: its workflow installs the FoundationDB *client*
  package on the runner and calls `cargo xtask fdb-conformance`, which brings up the
  existing `deploy/fdb-single-node/` compose service (`xtask/src/main.rs:292-323`) — no
  image build in the loop. That keeps the conformance signal fast and un-blocked.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Give the shipped-but-ungated `fdb` backend a standing automated signal and an
  actionable local preflight. Three deliverables:
  (a) `xtask::fdb_doctor` — a pure library module (probe results in → verdict + remediation
  text out) plus an `xtask fdb-doctor` dispatch arm, invoked as `run_fdb_conformance`'s
  preflight (`xtask/src/main.rs:292-323`) so the job fails fast with guidance instead of a
  linker error. The three rows the issue names: `libfdb_c` present/loadable, cluster file
  readable, `fdbcli --exec "status minimal"` healthy.
  (b) `.github/workflows/fdb-conformance.yml` — installs the pinned FoundationDB client
  package, runs `cargo xtask fdb-conformance`, on pull requests touching
  `crates/metadata-fdb/**` **and** nightly. Follow the shape of
  `.github/workflows/integration-nightly.yml` (container job, `docker info` step, timeout,
  failure-artifact upload) and `.github/workflows/tier1-jepsen.yml` (nightly cron +
  `workflow_dispatch`, explicitly **not** a required merge gate — ADR-0016 keeps the
  `cargo xtask ci` merge gate unprivileged and container-free).
  (c) `feature_gated_checks()` gains the two `--features fdb` typechecks (metadata-fdb,
  server) behind an FDB-toolchain env gate mirroring `WYRD_TIKV_TOOLCHAIN`
  (`xtask/src/main.rs:1273`), plus the **audit-policy record**: a short note stating the
  split the issue calls out — `cargo deny`/`cargo audit` see the Rust graph
  (`foundationdb-sys -> bindgen -> clang-sys -> libloading`, already allowlisted at
  `deny.toml:38-45`) but **cannot** see `libfdb_c` itself, whose advisory surface is
  tracked by following upstream FoundationDB release notes.
  **The note lands in `deny.toml`'s header comment block (`:1-6`) — that file IS the audit
  policy.** This location is pinned, not left to Do, for a scheduling reason: **441** (same
  wave) owns `docs/design/architecture/07-deployment-view.md`, the other plausible home for
  a packaging/dependency note. Two wave-1 bundles editing one file are built blind on the
  same base and `integrate.fold` would apply both patches in sequence — a textual conflict
  there is an `IntegrationError` and a hard STOP, not an advisory row. Do MUST NOT write to
  any file under `docs/design/`. ADR-0003 §2 is the normative source and is **Accepted /
  immutable**; do not touch it either.

  **out of scope:**
  - **Item 1 of the issue body (the local dev cluster) — ALREADY LANDED.** `#438` shipped
    `deploy/fdb-single-node/docker-compose.yml` (commit `22d39b6`), the `configure new
    single memory` init (`xtask/src/main.rs:334`), the host-side cluster-file write
    (`:382`), and the five-leg test driver (`:430`). Do MUST NOT re-create it. The issue's
    "one `make`/script entry point" is already satisfied by `cargo xtask fdb-conformance` —
    **Wyrd has no `Makefile` and deliberately does not want one** (`xtask/Cargo.toml`
    description: "so every check runs identically on a laptop and in CI without a working
    `make`"; ADR-0016). Do not add one.
  - **`deny.toml` allowlist / `[advisories]` / `[bans]` edits.** Item 4's dependency-graph
    work is already done: the `fdb` optional dependency is in `Cargo.lock`, so `cargo deny
    check` (run by `run_ci`, `xtask/src/main.rs:1423`) already traverses it — which is
    precisely *why* `deny.toml` needed the ISC entry at `:38-45`. Only the **written
    audit-policy note** is missing, and it goes in that file's **header comment** (`:1-6`);
    no allowlist row, advisory ignore, or ban is added or changed.
  - The `wyrd:fdb` OCI image and its build job (#470); the FDB deploy profiles (#469); the
    version-skew startup guard (#441); the nightly **TiKV** conformance counterpart (#420,
    still open — this slice establishes the pattern that #420 mirrors, it does not
    implement #420).
- **Repro instruction:** On `origin/main` in the target checkout:
  `git -C ../wyrd show origin/main:xtask/src/main.rs | sed -n '1255,1264p'` → the
  feature-gated typecheck list has one entry (tikv). `git -C ../wyrd ls-tree --name-only
  origin/main .github/workflows/` → no FDB workflow. `git -C ../wyrd grep -in doctor
  origin/main` → **no output at all**: there is no doctor anywhere in the tree.
- **External dependencies:**
  - *Binding criterion:* **none** beyond the base Rust toolchain. This is a hard
    requirement, not an accident: the new `xtask/tests/fdb_harness.rs` must run inside the
    ordinary `cargo test --workspace` that `run_ci` performs, on a machine with no
    FoundationDB. Keep the doctor's decision logic pure (probe results are *inputs*), the
    way `xtask::deploy_guard::scan_line` is pure.
  - **No new Cargo dependency.** `xtask/Cargo.toml` depends only on `wyrd-chunk-format`,
    `serde`, `serde_json` — there is **no YAML parser**. Assert the workflow's content with
    plain-text/substring checks, exactly as `xtask/tests/readme_dev_section.rs` asserts the
    README. Adding a crate triggers the ADR-0003 §2 three-test audit + the `deny.toml`
    allowlist, which INTEGRATION §4 makes a **human-only** decision and would turn this
    bundle into a NEEDS-HUMAN at sign-off.
  - *Supplementary live leg (`cargo xtask fdb-conformance`), all present on this host:*
    Docker + the compose plugin; `libfdb_c.so` 7.3.77; `fdbcli` 7.3.77; the ability to pull
    `foundationdb/foundationdb:7.3.77`. If Do finds any of these missing it MUST say so in
    `build-notes.md` rather than substituting a code-read for the run.
- **Test file:** `xtask/tests/fdb_harness.rs` (new). Peer pattern to imitate:
  `xtask/tests/deploy_no_orchestrator_coupling.rs` (planted-red + green-on-real-tree) and
  `xtask/tests/readme_dev_section.rs` (documented command ↔ real dispatch).
- **Verification posture:** Mixed, declared here so it is not a surprise at sign-off.
  - **Flippable at Check (the binding criterion):** the `fdb_doctor` unit tests, the
    planted-red, the workflow↔dispatch consistency assertion, and the
    `feature_gated_checks()` content assertion. All red pre-fix, green post-fix, no
    external dependency.
  - **Deferred (off-Check):** the workflow's own *green on a GitHub-hosted runner*. Nobody
    can observe that from `cargo xtask ci`. Confirmed by the maintainer on the first
    nightly run / on the PR that lands it.
  - **Deferred ≠ unbuilt (the #146 forcing function):** what is BUILT AND EXERCISED at
    Check is (i) the whole `fdb_doctor` module, unit-tested in this slice, (ii) the
    workflow file, parsed and asserted against the real `xtask` dispatch table, and
    (iii) the extended `feature_gated_checks()` list. The deferred item is only the
    *hosted execution* of a workflow whose every referenced command is proven to exist.
    Nothing here is inert dispatch scaffolding. Additionally, `cargo xtask fdb-conformance`
    IS runnable in this environment, so Do is expected to actually run it.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change.
  Composition slices — Do MAY open these cited peer callsites to mirror the pattern:
  - Pure-logic-in-lib + planted-red test: `xtask/src/lib.rs:1-19` (its module doc states
    exactly this "born-at-tier flippable coverage seam" contract) and
    `xtask/tests/deploy_no_orchestrator_coupling.rs:67` / `:102`.
  - Env-gated feature typecheck: `xtask/src/main.rs:1255` and `:1273`.
  - Preflight/skip convention (hard failure in CI, warn-and-skip locally):
    `xtask/src/main.rs:882` (`docker_available`), `:1508` (`is_ci`), and the same pattern
    inside `run_fdb_conformance` at `:299-312`.
  - Workflow shape: `.github/workflows/integration-nightly.yml` (whole file — container
    job, `docker info`, `timeout-minutes`, failure artifact) and
    `.github/workflows/tier1-jepsen.yml:1-30` (nightly + opt-in + non-gating rationale).
- **Prior-art check (triage cycles):** Searched by affected file path across merged
  history, open PRs, and **closed/rejected** PRs.
  - `git -C ../wyrd log --oneline --all -i --grep=doctor` → **no commits, ever.** The
    issue's phrase "extend the environment doctor … parallel to the existing `docker info`
    / TiKV rows pattern" describes something that **does not exist**; the only `docker
    info` occurrences are three CI workflow *steps*. This slice creates the doctor.
  - `.github/workflows/` history (`4b0c759`, `34bee68`, `91aa8ed`, `7780d70`) — no FDB job
    has ever existed. `xtask/src/main.rs` history (`576fc15`, `22d39b6`, `20fd2af`,
    `9374758`) — `fdb-conformance` added by #438, never wired to CI.
  - Rejected work: exactly one non-merged closed PR in the last 60 (#400, a docs/proposal
    scope PR) — unrelated to these paths. Merged prereqs: #438 (PR #492), #440 (PR #493).
    #420 (nightly TiKV conformance) is still **open** — so there is no TiKV counterpart to
    "mirror"; this slice is the first of the pair.
- **Disposition hint:** new-feature

## Plan-exit gate

Not applicable as a *gate*: this is new-functionality work (`docs/principles.md` §1.3 —
"New-feature work is not governed by minimalism at all"), not a structural /
lifecycle / load-or-import-safety defect, so the category-gated Plan-exit checks
(no-named-mechanism, no-single-module-satisfiable-invariant) do not apply and no
`Invariant to restore` is pulled. Mechanism is named on purpose here: for a feature the
brief is a specification, and `design-proposal.md.tpl`'s "enough that Do can implement
without re-deciding the design" is the governing standard.

## Deliberate deviation from the tracker — flagged, not silent

Issue #470's body says *"the #439 conformance workflow and the #442 battery consume this image
rather than ad-hoc builds."* This brief's workflow does **not** consume `wyrd:fdb`: it installs
the FoundationDB *client package* on the runner and calls `cargo xtask fdb-conformance`, which
boots a `foundationdb/foundationdb` container and runs `cargo test -p wyrd-metadata-fdb`. That is
a test-harness run; the `wyrd:fdb` image carries the `wyrd` binary, not the test suite. This
bundle also builds no ad-hoc image, so the harm the tracker names does not arise. Wiring the two
would additionally force 439 (wave 1) to depend on 470 (wave 2), inverting the batch order for no
gain. The full argument, and where the requirement genuinely lands (#469, #442), is recorded in
**470's** brief under the same heading. Do MUST NOT attempt to consume the image here.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The brief and the design stand; the workflow itself is largely right. What fails is that the tests meant to pin the three seams this slice exists to create are vacuous, and shipped comments assert coverage that does not exist. Rebuild against the same brief — do NOT re-plan. Note for the rebuild: the primary reviewer returned PASS on all of C1-C5/T1-T4. It was the adversarial pass, which mutated production code and watched the suite stay green, that found these. Trust the mutations, not the PASS row. WHAT TO FIX (four items, all local) 1. The workflow<->dispatch test passes on a workflow that runs no xtask command. `xtask_subcommands` (xtask/tests/fdb_harness.rs:281) scrapes `cargo xtask <sub>` from the WHOLE workflow file text, including the `#` header comment prose, which mentions fdb-conformance, ci and fdb-doctor before any `run:` key is reached. Deleting both steps that actually invoke xtask leaves all 16 tests green — the `subs.len() >= 2` non-vacuity guard at :311 is satisfied by comments alone. So success-criterion assertion 3 is met for the wrong reason. FIX: scope the scrape to `run:` lines (or assert the token appears after a `run:` key), so the test binds execution rather than mention. 2. False coverage claims in shipped comments. xtask/src/main.rs:1713-1714 states "Setting `fdb_toolchain` to `tikv_toolchain` in `run_ci` ... flips this red." It does not — that exact mutation leaves `cargo test -p xtask` entirely green. xtask/src/lib.rs:54-55 repeats the claim. Nothing invokes `run_ci()`; `recorded_invocations` passes booleans straight to `run_ci_steps`, never exercising the call site that chooses the arguments. Likewise "one doctor, two call sites" (xtask/src/fdb_doctor.rs:14) is pinned by nothing: deleting the whole preflight block (main.rs:327-340) leaves the suite green. FIX: cover the `run_ci` call site and the preflight call site, OR delete the comments. A comment claiming a red that does not exist is worse than no test — a future reviewer trusts a guard that is not there. 3. `probe_client_library` reports a working FDB build as a missing client library. CLIENT_LIBRARY_CANDIDATES (xtask/src/fdb_doctor.rs:50-56) checks five hard-coded paths plus `ldconfig -p`, but never reads FDB_CLIENT_LIB_PATH — the variable foundationdb-sys 0.10's own build.rs:61-64 uses to locate the library. A lib under /opt/foundationdb/lib with FDB_CLIENT_LIB_PATH set and no ld.so.conf entry links fine and probes Failed. Consequences, both NEW with this patch: locally `cargo xtask fdb-conformance` warns and `return Ok(())` (main.rs:339) — exit 0 with five test legs silently skipped, a FALSE GREEN where the suite previously ran and passed; in CI it hard-fails a job whose build would have succeeded. The doc comment at main.rs:393-395 is false in both directions. FIX: read FDB_CLIENT_LIB_PATH as a fourth candidate source. 4. The PR leg never type-checks the server's `fdb` arms — half of gap 2. `cargo xtask ci` carries `if: github.event_name != 'pull_request'` (.github/workflows/fdb-conformance.yml:129/135), so on a PR the only xtask step is `cargo xtask fdb-conformance`, whose legs build `-p wyrd-metadata-fdb` only. The server crate is never compiled with the feature on. Yet the path filter includes `crates/server/**` (:42) precisely because those `#[cfg(feature = "fdb")]` arms are never compiled. A type error in one is green on ci.yml and green on the PR leg, and surfaces only on the 06:00 UTC cron, up to 24h after merge. FIX: either run `cargo check -p wyrd-server --features fdb` on the PR leg, or drop `crates/server/**` from the path filter. Do not leave the filter promising a check it does not perform. WHAT SURVIVED ADVERSARIAL ATTACK — keep as-is, do not churn - HEALTHY_STATUS_NEEDLE (fdb_doctor.rs:84): matching on text rather than exit status is demonstrably the right predicate; real fdbcli 7.3.77 exits 0 against a dead coordinator. No false positive found. - The drift guards (fdb_harness.rs:449, :475) genuinely read driver, compose file and workflow; a partial version bump does go red. - cluster_file_path (fdb_doctor.rs:113) faithfully mirrors config::cluster_file. - probe_cluster_health has no timeout but fdbcli self-bounds at 5s. Not a hang. - actions/checkout@v7, actions/upload-artifact@v7 match every other workflow here. OUT OF SCOPE — recorded as a §10 Act candidate, do NOT fix in this rebuild `configure_fdb_database`'s readiness poll (main.rs:483-508) gates on `status.success()` with stdout/stderr nulled; given fdbcli's exit-0-when-unavailable behaviour it returns Ok on attempt 1 regardless. Pre-existing, not introduced by this diff. This patch adds the tested predicate that would fix it, but wiring it there is a separate slice.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversarial reviewer's mutation findings — the same class of issue that reopened iteration 1. Rebuild against the same brief; do NOT re-plan. Trust the mutations, not the primary PASS row. What to fix: 1. Carry-forward fix #1 is still incomplete: the workflow<->dispatch guard counts a *mention* as an execution when it is a TRAILING inline `#` comment on a `run:` line. `run_script_lines` (xtask/tests/fdb_harness.rs:467) strips only lines that START with `#`; `xtask_subcommands` (:510) then tokenises the surviving trailing comment as a real command. Demonstrated: replacing the conformance step with `run: echo DISABLED # cargo xtask fdb-conformance` leaves `the_fdb_conformance_workflow_executes_only_real_subcommands` GREEN while the job invokes no xtask command. The demonstrated-red (`run_script_scraping_ignores_prose…`) exercises only full-line comments, so the hole is uncovered. FIX: strip inline `#…` before tokenising (respecting quotes), or require the token be the command head, not any window — and extend the demonstrated-red to the trailing-comment shape. 2. Fix #3's production wiring is Check-unguarded. The pure search-path logic (`client_library_search_paths`) is covered, but the impure read of FDB_CLIENT_LIB_PATH at xtask/src/main.rs:386 is not. Mutating it to `let configured: Option<String> = None;` reintroduces the exact false-negative iteration 1 was rejected for (false-green skip locally / hard-fail in CI) and ALL tests stay green. Add a Check-level guard on that wiring so the rejected defect class cannot silently regress. What survived and must NOT be churned: fixes #2, #4, #5 bind under mutation (the fdb/tikv toolchain coupling, the PR-leg server fdb type-check, the doctor pure logic / planted-red / drift guards). Keep them as-is.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected to re-plan, not to rebuild against the same brief: three iterations have now reproduced the SAME defect class the adversarial pass keeps landing — the workflow↔dispatch guard binds a *mention* of a subcommand, not its *execution* (iter-3: `run: echo would run cargo xtask fdb-conformance` leaves `the_fdb_conformance_workflow_executes_only_real_subcommands` green while the job runs no xtask command). Iterations 1 and 2 named the correct fix ("require the token be the command head, not any window"); Do repeatedly chose a narrower comment-stripping variant instead. That the same hole survives three headless rebuilds is a signal the *brief* is steering Do toward a scrape-based approach that is structurally prone to this class, rather than pinning the binding property the reviewer keeps asking for. For the re-plan, decide at Plan time (not Do time): 1. Specify the workflow↔execution assertion by its binding property — the token must appear as the command HEAD of a real `run:` step — and require the demonstrated-red to cover the mention-as-argument shape, so the hole cannot pass. Consider whether "scrape the workflow text" is the right mechanism at all vs. a stronger structural check. 2. Adjudicate the second adversarial finding: the impure preflight/probe adapter (`main.rs:393` env read, `run_fdb_conformance` call site) is Check-unguarded — mutating it reintroduces the iter-1 false-negative with all tests green. Decide in the brief whether that thin wiring warrants a Check guard or is acceptable untested wiring over a tested pure core; do not leave it for Do to re-decide a third time. The primary reviewer PASSed all rows again; trust the mutations, not the PASS row. §6 (T3 live Docker run, T5 prior art, fitness-to-purpose) was not reached — left unresolved, superseded by the re-plan.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
