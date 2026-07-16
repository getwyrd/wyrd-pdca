# Brief — issue 409 / m4-elle-ci-job-dst-regression-loop

> The Plan artifact for getwyrd/wyrd#409 — "M329.6 — privileged off-Check CI job +
> bug→DST-regression loop". Slice 6 of #329 (DoD items 3 and 4): turn the #408 checked
> consistency run into a **standing automated job**, and make every anomaly it finds
> become a tracked bug and a **permanent deterministic regression** (ADR-0009; the
> FoundationDB/TigerBeetle pattern). Milestone note: the 2026-07-07 tracker comment pins
> the run to the **FoundationDB** backend (ADR-0042); the backend-agnostic harness pieces
> (#406 models, #407 nemesis) are reused, not reworked. #407 is merged
> (getwyrd/wyrd PR #569, e0e39c1); #408 is the in-flight prereq bundle this stacks on.

- **Slug:** m4-elle-ci-job-dst-regression-loop
- **Defect:** data-safety verification is currently a one-shot achievement, not a standing
  guard. The landed verdict dispatch routes the Elle verdict to a privileged off-Check job
  named by `ELLE_OFF_CHECK_VERDICT_JOB = "elle-register-verdict"`
  (`crates/server/src/consistency_workload.rs:851`), but **no such workflow exists** in
  `.github/workflows/` — the seat is empty, so nothing runs the #408 checked consistency
  run on a schedule, a future regression that reintroduces a way to lose or corrupt data
  would go unnoticed until the next manual run. And the learning loop is missing both
  ends: `report-nightly-failure.yml` does not watch the (nonexistent) job, so an anomaly
  would produce no tracked bug; and the metadata-store DST campaign
  (`crates/dst/tests/commit_ambiguity.rs`, over `SimFdbMetadataStore`) has **no
  committed-regression-seed anchor** (the `custodian.rs:1343` pattern), so there is no
  promotion target for a minimized anomaly.
- **Success criterion:** the standing job and the loop exist and are pinned at Check. The
  added `xtask/tests/consistency_ci_job.rs` proves, red→green under C4-verify, that:
  (a) `.github/workflows/elle-register-verdict.yml` exists and its workflow name equals
  the landed seat constant `ELLE_OFF_CHECK_VERDICT_JOB` — mechanically coupled by
  READING `crates/server/src/consistency_workload.rs` as a file (the `fdb_image.rs`
  version-coupling pattern), never by importing `wyrd-server` into xtask;
  (b) the workflow triggers on `schedule` (a distinct cron hour — 02:00–06:00 UTC are
  taken; e.g. 07:00) + `workflow_dispatch` and on NOTHING else, is timeout-bounded, and
  is thin per the repo's CI rule ("CI logic lives in `cargo xtask`" — ADR-0009 §CI
  paragraph; ADR-0016 single-sources automation in the xtask crate): it opts in via
  `WYRD_TIER1=1` and invokes `cargo xtask consistency-run` (the #408 runner) rather
  than carrying CI logic in YAML — so the unprivileged `cargo xtask ci` merge gate
  stays container-free and JVM-free. The proof standard for the YAML properties is
  **textual pinning**, the `fdb_harness.rs`/`fdb_image.rs` precedent (file-read +
  substring/parse helpers local to the test; no YAML dependency added to xtask): pin
  the triggers by asserting the `schedule:`/`workflow_dispatch:` lines present AND
  `pull_request`/`push` triggers absent from the file — state it as pinning, not
  semantic YAML proof. One coupling MUST cross the YAML boundary: the xtask subcmd the
  workflow invokes must appear as an arm of the subcmd dispatch match in
  `xtask/src/main.rs` (`:72-80` on today's main; #408's fold adds the
  `consistency-run` arm) — asserted by file-read, so a workflow invoking a
  nonexistent or later-renamed subcmd fails at Check instead of failing silently on
  the first cron;
  (c) the workflow uploads the anomaly raw material as run artifacts — the EDN
  histories, the machine-readable run summary, the checker output, and the rendered
  report. The names/paths listed in #408's design (`run-summary.json`,
  `target/consistency-run/`) are **provisional until 408's fold**: bind the workflow
  and the pinning test to what #408's folded result ACTUALLY emits (read it from the
  wave base), never reshape #408 and never assert guessed paths;
  (d) `report-nightly-failure.yml`'s watched `workflows:` list includes
  `elle-register-verdict`, so a scheduled failure becomes exactly one tracked bug (the
  existing idempotent one-open-issue rule). Note the reporter fires only on
  `conclusion == 'failure'` for `schedule` events (`report-nightly-failure.yml:49-53`);
  a `timeout-minutes` overrun surfaces as `failure` on GitHub, so the timeout bound is
  covered — do NOT widen the reporter's conclusion filter;
  (e) the DST promotion anchor exists: a `REGRESSION_SEEDS`-style committed-seed replay
  block appended to `crates/dst/tests/commit_ambiguity.rs`. The `custodian.rs:1343-1375`
  precedent supplies the ANCHOR shape only (the committed const + the documented
  one-line-append rule); the replay MECHANICS are this file's own idiom — a plain
  `#[test]` fn calling the seed-parameterized bodies directly per seed, exactly as
  `the_settling_re_read_covers_both_halves_of_the_ambiguity_space` does
  (`commit_ambiguity.rs:354-358`) — do not port custodian's `dst_campaign_test!` /
  `ChaCha8Rng` machinery here. Replay each committed seed through the existing
  metadata-ambiguity property bodies (`run_cas_ambiguity`,
  `run_blind_ambiguity`, `run_timeout_ambiguity`, `run_contended_1031`) — in each
  body's FAITHFUL configuration (the faithful `FdbFidelity` / honest observer the
  campaign tests use, never a permissive twin) and asserting the SAME per-seed
  anti-vacuity observations those tests assert (nonzero ambiguity/deferral counts —
  `commit_ambiguity.rs:332-333`, `:686-699`, `:865-896`), so a seed that never
  activates the fault path cannot replay green silently. Seed provenance, stated
  honestly (ADR-0009 commits *bug-finding* seeds): the initial fixed set is a
  **deterministic replay set that keeps the anchor live and proves the append path**,
  documented in the block comment as NOT yet bug-finding; the first checker-found
  anomaly's minimized seed is appended per the documented procedure — never fabricate
  bug provenance for the initial seeds.
  The first live scheduled/dispatched execution of the job is deferred off-Check (see
  Verification posture) — do NOT scope the binding criterion to it. Structure the
  pinning test as SEPARATE `#[test]` functions per deliverable (a)–(e) so each red is
  independently attributable in the C4-verify log, not one assertion chain that
  short-circuits on the first miss.
- **Falsifiability:** RED is produced in the C4-verify worktree: the gate reverts the
  production change (the two workflow YAMLs and the appended DST block) and keeps the
  added `xtask/tests/consistency_ci_job.rs`, whose file-read assertions then fail against
  the absent workflow / unwired reporter / missing anchor. The added-test classification
  was dry-run confirmed at Plan via `run-verify.sh --classify` on a synthetic patch with
  this exact file set: it emits `ADDED_TEST xtask/tests/consistency_ci_job.rs` (crate
  `xtask` pre-exists, so the red leg compiles and genuinely fails), and the appended
  `crates/dst/tests/commit_ambiguity.rs` block is NOT classified as a discriminator (a
  modified file), so its self-contained green cannot vacuously satisfy the red leg. The
  *live* forbidden failure (the standing job catching a real anomaly) is exhibitable only
  on a GitHub-hosted runner post-merge — deferred by design, not a gap in the binding
  criterion.
- **Invariant to restore:** the standing-verification loop invariant, sourced precisely:
  (1) the privileged tier boundary — the checker (JVM/Clojure) and any container leg
  **MUST run only in a privileged off-Check job and MUST NOT enter `cargo xtask ci`**
  (ADR-0041 §Decision, lines 87-89), with CI logic living in `cargo xtask`, not YAML
  (ADR-0009 §CI paragraph, line 25; ADR-0016 is the single-sourced-xtask-automation
  decision itself); (2) the learning loop — a scheduled quality-job failure becomes
  exactly one tracked bug (`report-nightly-failure.yml` header rule), an anomaly the
  checker finds is minimized and promoted into DST (ADR-0041 lines 122-123), and **a
  bug-finding seed is committed as a permanent regression test** (ADR-0009 line 25; the
  committed-seeds precedent `crates/dst/tests/custodian.rs:1343`). Do must not satisfy
  this by guarding one module — the loop spans workflow, reporter, and DST anchor.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 408
- **Ordering note:** depends on 408 because the workflow's one load-bearing line invokes
  `cargo xtask consistency-run` — the #408 runner, which exists only on 408's folded wave
  result — and the pinning test asserts that invocation against a tree that has it. File
  sets checked at Plan: 409 touches `.github/workflows/*` (one added, one modified),
  `xtask/tests/consistency_ci_job.rs` (added), `crates/dst/tests/commit_ambiguity.rs`
  (appended); 408 touches `xtask/src/*`, `xtask/tests/consistency_run_orchestration.rs`,
  `crates/server/tests/*` — no shared file, so the dependency is the invocation seam, not
  a textual conflict. Harness-side, `engine/scripts/run-verify.sh:186-192` honours
  `PDCA_BASE` → `PDCA_VERIFY_BASE` before the brief base, so this wave≥1 bundle's
  C4-verify runs against the wave's folded base, not a stale `origin/main`. #407 is
  already merged to main (e0e39c1) — no scheduling field needed for it.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** the standing job + the loop, four deliverables: (1) the new
  `.github/workflows/elle-register-verdict.yml` — scheduled + dispatchable, privileged
  (Docker/FDB-client/JVM/elle-cli provisioned inside the runner, mirroring
  `fdb-conformance.yml`'s provisioning steps and `tier1-jepsen.yml`'s gating-policy
  header/concurrency/timeout shape), thin per the CI rule (ADR-0009 §CI: CI logic
  lives in `cargo xtask`, not YAML), opting in via `WYRD_TIER1=1
  cargo xtask consistency-run`, uploading the run artifacts; (2) appending
  `elle-register-verdict` to `report-nightly-failure.yml`'s watched list; (3) the DST
  promotion anchor appended to `crates/dst/tests/commit_ambiguity.rs` (committed seed
  set replayed through the existing property bodies) plus the documented promotion
  procedure (anomaly artifacts → minimize → deterministic DST reproduction → append the
  seed), stated in the workflow header the way `tier1-jepsen.yml` documents its policy;
  (4) the pinning test `xtask/tests/consistency_ci_job.rs`. / out of scope: any change to
  #408's runner semantics (its inconclusive gate, fixtures self-check, checker contract
  — consume `cargo xtask consistency-run` as-is), #407's nemesis, the #406
  workload/serializers; making the job a required status check (it is post-merge
  regression visibility, per the ADR-0016 gating policy all sibling nightlies state); a
  TiKV leg; any new ADR/spec; automated anomaly-minimization tooling beyond retaining
  the artifacts (minimization to a seed is the documented human step, as everywhere else
  in the repo).
- **Repro instruction:** on getwyrd/wyrd main (719a225): `ls .github/workflows/ | grep
  elle` → nothing (the seat named at `crates/server/src/consistency_workload.rs:851` is
  unfilled); `grep -n "elle-register-verdict" .github/workflows/report-nightly-failure.yml`
  → nothing (the loop's entry is unwired); `grep -n "REGRESSION_SEEDS"
  crates/dst/tests/commit_ambiguity.rs` → nothing (no promotion anchor; contrast
  `crates/dst/tests/custodian.rs:1350`).
- **External dependencies:** none for the Check-testable core (the pinning test is pure
  file reads on the base toolchain; the DST anchor runs in the default `dst` tier inside
  `cargo xtask ci`). The live standing job's environment — Docker, the FoundationDB
  client, the JVM, the elle-cli jar, the `deploy/fdb-multi-replica` topology — is
  provisioned inside the GitHub-hosted runner by the workflow itself (no-check: a CI
  runner environment shape, not an operator-installable tool here); an operator
  reproducing that leg locally is preflighted by the already-registered rows `docker`,
  `java`, `elle-cli`, `libfdb_c loadable`, `fdb headers (bindgen)` (registered in the
  #407/#408 cycles).
- **Test file:** xtask/tests/consistency_ci_job.rs
- **Verification posture:** net-new coverage + deferred live green (postures (a)+(b),
  pre-declared). Built AND exercised at Check, by TWO DISTINCT gates — keep them
  separate in evidence: (i) **C4-verify** runs ONLY the named pinning test
  (`cargo test -p xtask --test consistency_ci_job`) red→green — its file-read
  assertions cover both workflow YAMLs, the seat-name coupling to the landed constant,
  the triggers/timeout/thinness pinning, the artifact-upload steps, and the DST
  anchor's presence; C4-verify never executes the modified DST file. (ii) **C4-ci**
  (`cargo xtask ci` → `run_dst`, which supplies `--cfg madsim`) is what actually
  EXECUTES the appended DST anchor block.
  **Test-graph constraint, pinned:** everything `consistency_ci_job.rs` imports must be
  default-compiled — no feature/cfg gate, no `wyrd-server` dependency in `xtask`
  (`xtask/Cargo.toml:11-14` has none today; keep it so), unconditional `#[test]`
  functions, helpers local to the test file per the `fdb_image.rs` /
  `readme_dev_section.rs` precedent — otherwise the C4-verify red degrades to a vacuum.
  The appended DST block is `#![cfg(madsim)]` like its file; that is fine because it is
  NOT the C4-verify discriminator (classification dry-run confirmed) and is executed by
  the `dst` tier. Deferred: the workflow's first actual execution — a workflow can only
  run once it exists on GitHub, so the maintainer confirms it via `workflow_dispatch`
  after merge (post-merge, named confirmer: Eduard Ralph; record the run link on the
  issue). Deferred ≠ unbuilt: the YAML's entire logic is one invocation of the #408
  runner, which is itself unit-tested at its own Check and witnessed per its brief — this
  slice adds NO new host-dependent logic that could hide untested (DoD item 3's
  "host-independent logic is itself unit-tested" is met by the pinning test + the thin
  logic-in-xtask split (ADR-0009 §CI) + 408's tested runner).
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites to mirror (composition slice — Do MAY open exactly these):
  - the seat this workflow fills: `ELLE_OFF_CHECK_VERDICT_JOB`
    (`crates/server/src/consistency_workload.rs:851`) and the dispatch that routes to it
    (`consistency_verdict_dispatch`, `:887`) — the workflow `name:` must equal the
    constant, coupled by a file-read of that source, never an import;
  - the workflow shape to mirror: `.github/workflows/tier1-jepsen.yml` (the gating-policy
    header, cron/concurrency/timeout, `WYRD_TIER1=1` opt-in, thin xtask delegation) and
    `.github/workflows/fdb-conformance.yml` (the in-runner FDB provisioning steps; its
    cron is 06:00 — occupied slots are 02:00/03:00/04:00/05:00/06:00, pick a distinct
    hour, e.g. 07:00 UTC);
  - the loop's entry: `.github/workflows/report-nightly-failure.yml:28-32` (the watched
    `workflows:` list to append to; the header states the one-open-issue idempotency
    rule this inherits);
  - the pinning-test pattern: `xtask/tests/fdb_image.rs:20-28` (`workspace_root` +
    local file-read helpers; the mechanical version/name coupling discipline) and
    `xtask/tests/fdb_harness.rs:63` (the `WORKFLOW` const pattern);
  - the promotion-anchor shape to append: `crates/dst/tests/custodian.rs:1343-1375`
    (`REGRESSION_SEEDS` + `committed_regression_seeds_stay_green`), replaying the
    seed-parameterized bodies `run_cas_ambiguity`
    (`crates/dst/tests/commit_ambiguity.rs:318`), `run_blind_ambiguity` (`:523`),
    `run_timeout_ambiguity` (`:676`), `run_contended_1031` (`:854`) — in the faithful
    configuration and with the same anti-vacuity counter assertions the campaign's
    sweep tests carry (`:332-333`, `:686-699`, `:865-896`), which Do MAY open;
  - the runner the workflow invokes, consumed as-is from 408's folded result: the
    `consistency-run` xtask subcmd (its opt-in/hard-error/teardown discipline is #408's;
    never reopen it here). Do MAY open the folded runner's source in `xtask/src/` on
    the wave base to READ the artifact names/paths it actually emits (criterion (c)
    binds to those) and the subcmd dispatch match in `xtask/src/main.rs` (criterion
    (b)'s coupling target) — read-to-bind only, never modify.
- **Prior-art check (by affected file path):** merged history —
  `.github/workflows/elle-register-verdict.yml` has never existed (no commit touches it;
  no `elle*` branch on origin); `report-nightly-failure.yml` landed as the generic
  reporter (925d66d, hardened 142e671) with a watched list that does not include this
  job; `crates/dst/tests/commit_ambiguity.rs` landed via #468 (last touched b3ae23f) with
  no regression-seed anchor; the committed-seeds precedent lives only in
  `custodian.rs:1343`. No open or closed PR titled for #409 (the one `409` commit hit,
  26d6013, is HTTP 409 Conflict — unrelated). Nothing supersedes this; genuinely
  additive.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
